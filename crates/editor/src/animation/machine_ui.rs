use super::*;

impl AnimationStudio {
    /// The open state machine: its toolbar, graph, inspector and live
    /// preview. Returns where double-clicking a state went (its clip).
    #[allow(clippy::too_many_arguments)] // Each is a different input to the studio.
    pub(super) fn machine_body(
        &mut self,
        ui: &mut egui::Ui,
        project: &Path,
        assets: &Assets,
        render_state: Option<&RenderState>,
        world: &mut World,
        output: &mut Output,
        actions: &mut Vec<StudioAction>,
    ) -> Option<Go> {
        let stacked = ui.available_width() < STACK_BELOW;
        let pick = egui::TopBottomPanel::top("state machine toolbar")
            .show_inside(ui, |ui| self.controller_toolbar(ui, stacked))
            .inner;
        if let Some(pick) = pick {
            self.act(pick, project, assets, world, output, actions);
        }

        let dt = f64::from(ui.input(|input| input.unstable_dt)).min(MAX_FRAME_SECONDS);
        let mut go = None;
        if stacked {
            egui::CentralPanel::default().show_inside(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("state machine stacked")
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        let size = egui::vec2(ui.available_width(), STACKED_GRAPH);
                        ui.allocate_ui(size, |ui| go = self.controller_graph(ui));
                        ui.add_space(6.0);
                        self.controller_live(ui, assets, render_state, dt);
                        ui.separator();
                        egui::CollapsingHeader::new(format!("{} Settings", icons::SLIDERS))
                            .id_salt("state machine inspector stacked")
                            .default_open(true)
                            .icon(widgets::chevron)
                            .show(ui, |ui| self.controller_inspector(ui));
                    });
            });
        } else {
            // Across the whole body, so the preview has room beside its
            // parameters; the graph keeps at least half the height, and
            // starts with two thirds of a short tab.
            let room = ui.available_height();
            egui::TopBottomPanel::bottom("state machine preview")
                .resizable(true)
                .default_height(CONTROLLER_PREVIEW.min(room / 3.0).max(100.0))
                .min_height(100.0)
                .max_height((room * 0.5).max(100.0))
                .show_inside(ui, |ui| self.controller_live(ui, assets, render_state, dt));
            if !self.machine.hide_inspector {
                let room = ui.available_width();
                egui::SidePanel::right("state machine inspector")
                    .resizable(true)
                    .default_width(320.0)
                    .max_width(room * 0.5)
                    .show_inside(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("state machine inspector scroll")
                            .show(ui, |ui| self.controller_inspector(ui))
                    });
            }
            egui::CentralPanel::default().show_inside(ui, |ui| go = self.controller_graph(ui));
        }
        if self.machine.doc.as_ref().is_some_and(|doc| doc.playing) {
            ui.ctx().request_repaint();
        }
        go
    }

    /// What's open's preview model and playback, the button that shows or
    /// hides the inspector, and the ⋯ menu. Returns what the ⋯ menu asked for.
    pub(super) fn controller_toolbar(&mut self, ui: &mut egui::Ui, stacked: bool) -> Option<Pick> {
        let open = self.controller_path();
        responsive::toolbar(ui, |ui| {
            let mut pick = None;
            let Some(doc) = &mut self.machine.doc else {
                return pick;
            };
            let shown = doc
                .preview_model
                .as_deref()
                .map_or("(pick a model)", file_name);
            let mut picked = doc.preview_model.clone();
            responsive::room_for(ui, MODEL_PICKER_ROOM);
            egui::ComboBox::from_id_salt("state machine preview model")
                .selected_text(format!("{} Preview on {shown}", icons::CUBE))
                .width(160.0)
                .show_ui(ui, |ui| {
                    for model in &self.files.models {
                        ui.selectable_value(&mut picked, Some(model.clone()), file_name(model));
                    }
                })
                .response
                .on_hover_text("The model the live preview shows");
            if picked != doc.preview_model {
                doc.preview_model = picked;
                doc.restart();
            }
            let (icon, hover) = if doc.playing {
                (icons::PAUSE, "Pause the preview")
            } else {
                (icons::PLAY, "Play the preview")
            };
            if ui.button(icon).on_hover_text(hover).clicked() {
                doc.playing = !doc.playing;
            }
            if ui
                .button(icons::ARROW_COUNTER_CLOCKWISE)
                .on_hover_text("Start again from the default state")
                .clicked()
            {
                doc.restart();
            }
            ui.separator();
            if !stacked
                && widgets::icon_button(
                    ui,
                    icons::SLIDERS,
                    "Show or hide the inspector",
                    !self.machine.hide_inspector,
                )
                .clicked()
            {
                self.machine.hide_inspector = !self.machine.hide_inspector;
            }
            if let Some(path) = &open {
                ui.menu_button(icons::DOTS_THREE, |ui| asset_menu(ui, path, &mut pick));
            }
            pick
        })
    }

    /// The open state machine's graph, filling `ui`. Returns where
    /// double-clicking a state went: the clip it plays (a blend tree's
    /// first).
    pub(super) fn controller_graph(&mut self, ui: &mut egui::Ui) -> Option<Go> {
        let controller = self.machine.doc.as_mut()?;
        let size = ui.available_size();
        let open = ui
            .allocate_ui(size, |ui| controller.graph_ui(ui, &self.files.clips))
            .inner?;
        let state = controller.file.states.get(open)?;
        let clip = motion_clips(&state.motion).into_iter().next()?;
        Some(Go::State(state.name.clone(), clip))
    }

    /// The live preview beside its parameters, or above them when narrow.
    pub(super) fn controller_live(
        &mut self,
        ui: &mut egui::Ui,
        assets: &Assets,
        render_state: Option<&RenderState>,
        dt: f64,
    ) {
        let Some(controller) = &mut self.machine.doc else {
            return;
        };
        let preview = &mut self.machine.preview;
        let height = ui.available_height().min(CONTROLLER_PREVIEW);
        let size = egui::vec2(ui.available_width(), height);
        ui.allocate_ui(size, |ui| {
            responsive::split(ui, SPLIT_PREVIEW_BELOW, 0.55, |ui, pane| {
                if pane == 0 {
                    show_model(preview, controller.preview_model.as_deref(), assets);
                    let posed = match controller.tick(assets, dt) {
                        Ok(posed) => Some(posed),
                        Err(problem) => {
                            ui.colored_label(ui.visuals().warn_fg_color, problem);
                            None
                        }
                    };
                    let height = ui.available_height().clamp(40.0, CONTROLLER_PREVIEW);
                    preview.show(ui, render_state, assets, posed, height);
                    return;
                }
                // Not the "Parameters" beside the graph, which defines them:
                // these only try values in the preview.
                ui.strong("Try parameters").on_hover_text(
                    "Set these to watch the preview follow the wires. They only change the \
                     preview: in the game, scripts and blocks set them.",
                );
                egui::ScrollArea::vertical()
                    .id_salt("live parameters")
                    .show(ui, |ui| controller.live_controls(ui));
            });
        });
    }

    pub(super) fn controller_inspector(&mut self, ui: &mut egui::Ui) {
        if let Some(controller) = &mut self.machine.doc {
            controller.inspector(ui, &self.files.clips);
        }
    }
}
