use super::*;

impl ControllerDoc {
    /// Runs the preview's state machine on by `dt` seconds, rebuilding it
    /// when the controller or model changed. Returns the posed model.
    pub fn tick(&mut self, assets: &Assets, dt: f64) -> Result<Arc<Posed>, String> {
        let mesh_id = self
            .preview_model
            .clone()
            .ok_or("Pick a model to preview on below.")?;
        let model = assets
            .model(&mesh_id)
            .ok_or_else(|| format!("`{mesh_id}` can't be loaded"))?;
        let rig = model
            .rig
            .clone()
            .ok_or_else(|| format!("`{mesh_id}` has no skeleton"))?;
        if self.bones.len() != rig.bones.len() {
            self.bones = axcenity_anim::bone_paths(&rig)
                .into_iter()
                .filter(|path| !path.is_empty())
                .collect();
        }
        if self.file.states.is_empty() {
            return Err("Add a state to preview it.".to_owned());
        }
        let made_from = (model.key(), behaviour(&self.file));
        if self
            .preview
            .as_ref()
            .is_none_or(|run| run.made_from != made_from)
        {
            let (compiled, problems) = Compiled::new(self.file.clone(), Some(&rig), |path| {
                axcenity_anim::load_clip(assets, path)
            });
            let compiled = Arc::new(compiled);
            // Keep playing where it was if that state is still there; keep
            // the parameters that still mean the same.
            let old = self.preview.take();
            let mut machine = Machine::new(&compiled);
            let mut parameters: BTreeMap<String, Parameter> = self
                .file
                .parameters
                .iter()
                .map(|def| (def.name().to_owned(), def.initial()))
                .collect();
            if let Some(old) = old {
                let name = old
                    .compiled
                    .state_name(old.machine.current.state)
                    .to_owned();
                if let Some(state) = compiled.state_index(&name) {
                    machine.current = Playing {
                        state,
                        phase: old.machine.current.phase,
                    };
                }
                for (name, value) in old.parameters {
                    if let Some(slot) = parameters.get_mut(&name)
                        && slot.kind() == value.kind()
                    {
                        *slot = value;
                    }
                }
            }
            self.preview = Some(PreviewRun {
                made_from,
                compiled,
                machine,
                parameters,
                problems,
            });
        }
        let run = self.preview.as_mut().expect("made above");
        if self.playing {
            run.machine
                .advance(&run.compiled, Some(&rig), &run.parameters, dt);
            run.machine.evaluate(&run.compiled, &mut run.parameters);
        }
        // Scrubbing a selected transition shows its two poses mixed.
        let pose = match self.selected {
            Selected::Transition(index) if self.scrub < 1.0 => {
                let transition = &self.file.transitions[index];
                let to = run.compiled.state_index(&transition.to);
                let from = match from_node(&self.file, &transition.from) {
                    Some(Node::State(state)) => Some(state),
                    _ => Some(run.machine.current.state),
                };
                match (from, to) {
                    (Some(from), Some(to)) => {
                        let phase = run.machine.current.phase;
                        let mixed = Machine {
                            current: Playing { state: to, phase },
                            fade: Some(Fade {
                                from: Playing { state: from, phase },
                                elapsed: 0.0,
                                duration: 1.0,
                                interruptible: true,
                                transition: Some(index),
                            }),
                        };
                        mixed.pose_at(&run.compiled, &rig, &run.parameters, self.scrub)
                    }
                    _ => run.machine.pose(&run.compiled, &rig, &run.parameters),
                }
            }
            _ => run.machine.pose(&run.compiled, &rig, &run.parameters),
        };
        // A layer shows what it moves: the bones its mask picks.
        let mut pose = pose;
        if let Some(layer) = self.open_layer()
            && !layer.mask.is_empty()
        {
            let (mask, _) = axcenity_anim::bone_mask(&rig, &layer.mask);
            let rest = rig.rest_pose();
            for (bone, picked) in mask.iter().enumerate() {
                if !picked {
                    pose[bone] = rest[bone];
                }
            }
        }
        model
            .pose(&pose)
            .map(Arc::new)
            .ok_or_else(|| "the model can't be posed".to_owned())
    }

    /// What the graph shows of the preview: the state playing and the blend.
    pub fn live(&self) -> Live {
        let Some(run) = &self.preview else {
            return Live::default();
        };
        let current = run.machine.current;
        let through = if run.compiled.looping(current.state, &run.parameters) {
            current.phase.rem_euclid(1.0)
        } else {
            current.phase.min(1.0)
        };
        let state = self
            .file
            .states
            .iter()
            .position(|s| s.name == run.compiled.state_name(current.state));
        Live {
            state: state.map(|state| (state, through as f32)),
            fade: run.machine.fade.and_then(|fade| {
                let from = self
                    .file
                    .states
                    .iter()
                    .position(|s| s.name == run.compiled.state_name(fade.from.state))?;
                Some((from, fade.transition, fade.weight() as f32))
            }),
        }
    }

    /// Sets a parameter of the running preview, as its control would.
    #[cfg(test)]
    pub fn set_live_parameter(&mut self, name: &str, value: Parameter) {
        if let Some(run) = &mut self.preview {
            run.parameters.insert(name.to_owned(), value);
        }
    }

    /// Starts the preview again from the default state with the default
    /// parameters.
    pub fn restart(&mut self) {
        self.preview = None;
        self.playing = true;
    }

    /// The preview's parameter controls, the state playing and its blend.
    pub fn live_controls(&mut self, ui: &mut egui::Ui) {
        let Some(run) = &mut self.preview else {
            return;
        };
        let state = run
            .compiled
            .state_name(run.machine.current.state)
            .to_owned();
        ui.horizontal_wrapped(|ui| {
            ui.strong(format!("{} {state}", icons::PLAY_CIRCLE));
            if let Some(fade) = run.machine.fade {
                ui.weak(format!(
                    "blending from {}: {:.0}% new",
                    run.compiled.state_name(fade.from.state),
                    fade.weight() * 100.0
                ));
            }
        });
        for problem in &run.problems {
            ui.colored_label(ui.visuals().warn_fg_color, problem);
        }
        if self.file.parameters.is_empty() {
            ui.weak("Add parameters (in the state machine's settings) to drive transitions here.");
        }
        let ranges = number_ranges(&self.file);
        crate::responsive::form(ui, "live parameters", |ui| {
            for def in &self.file.parameters {
                let name = def.name().to_owned();
                ui.label(&name);
                let slot = run
                    .parameters
                    .entry(name.clone())
                    .or_insert_with(|| def.initial());
                match slot {
                    Parameter::Bool(value) => {
                        ui.checkbox(value, "");
                    }
                    Parameter::Number(value) => {
                        let (low, high) = ranges.get(&name).copied().unwrap_or((0.0, 10.0));
                        ui.add(egui::Slider::new(value, low..=high));
                    }
                    Parameter::Trigger(set) => {
                        ui.horizontal_wrapped(|ui| {
                            if ui.button("Fire").clicked() {
                                *set = true;
                            }
                            if *set {
                                ui.weak("waiting for a transition");
                            }
                        });
                    }
                }
                ui.end_row();
            }
        });
    }
}
