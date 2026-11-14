//! Debug › Animator Inspector… (AP6): during Play, what the selected
//! animated thing is doing, live: its state machine's state and how far
//! through it is, the state it's blending from, its layers, the
//! AnimationTracks and Animations playing over it, its parameters, the
//! events it fired lately and its root motion. The selected MeshPart, its
//! Animator or a Model holding one all show it; with nothing animated
//! selected, it lists what is animated to pick from. Looking changes
//! nothing.

use axcenity_anim::AnimatorView;
use axcenity_core::Parameter;

use super::*;

/// Whether the inspector is open, and the thing picked from its list.
#[derive(Default)]
pub(super) struct AnimatorInspector {
    pub open: bool,
    /// Picked from the list while nothing animated is selected.
    picked: Option<InstanceId>,
}

/// What the window shows this frame.
pub(super) enum Showing {
    /// Not playing: nothing animates.
    Stopped,
    /// Playing, nothing animated selected or picked: these are.
    Choose(Vec<(InstanceId, String)>),
    View(Box<AnimatorView>, String),
}

impl EditorApp {
    pub(super) fn open_animator_inspector(&mut self) {
        self.animator_inspector.open = true;
    }

    /// What the inspector shows: the first selected thing that's animated
    /// (or holds or is in something animated), else the one picked.
    pub(super) fn animator_showing(&self) -> Showing {
        let Some(play) = &self.play else {
            return Showing::Stopped;
        };
        let world = play.world();
        let view = self
            .selection
            .iter()
            .chain(&self.animator_inspector.picked)
            .find_map(|&id| play.inspect_animator(id));
        match view {
            Some(view) => {
                let name = world.full_name(view.owner).unwrap_or_default();
                Showing::View(Box::new(view), name)
            }
            None => Showing::Choose(
                play.animated()
                    .into_iter()
                    .map(|id| (id, world.full_name(id).unwrap_or_default()))
                    .collect(),
            ),
        }
    }

    /// Each frame: the window, while open.
    pub(super) fn animator_inspector_window(&mut self, ctx: &egui::Context) {
        if !self.animator_inspector.open {
            return;
        }
        let showing = self.animator_showing();
        if matches!(showing, Showing::View(..)) {
            // It's live: keep drawing while the game runs.
            ctx.request_repaint();
        }
        let mut open = true;
        let mut picked = None;
        crate::widgets::tool_window(icons::BONE, "Animator Inspector")
            .size(egui::vec2(380.0, 460.0))
            .show(ctx, &mut open, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| match &showing {
                    Showing::Stopped => {
                        ui.label(
                            "Press Play to watch Animators at work. Then select an animated \
                             MeshPart, its Animator or a Model holding one.",
                        );
                    }
                    Showing::Choose(animated) if animated.is_empty() => {
                        ui.label("Nothing is animated in this game.");
                    }
                    Showing::Choose(animated) => {
                        ui.label("Select an animated part, or pick one:");
                        for (id, name) in animated {
                            if ui.selectable_label(false, name).clicked() {
                                picked = Some(*id);
                            }
                        }
                    }
                    Showing::View(view, name) => animator_view(ui, view, name),
                });
            });
        if let Some(id) = picked {
            self.animator_inspector.picked = Some(id);
        }
        self.animator_inspector.open &= open;
    }
}

/// One animated thing's snapshot.
fn animator_view(ui: &mut egui::Ui, view: &AnimatorView, name: &str) {
    ui.strong(name);
    if !view.posed {
        ui.weak("Its skeleton isn't posed (no rigged model, or nothing moves bones).");
    }
    if let Some(machine) = &view.machine {
        ui.add_space(4.0);
        ui.label(egui::RichText::new("State machine").strong());
        egui::Grid::new("animator inspector machine")
            .num_columns(2)
            .striped(true)
            .show(ui, |ui| {
                ui.label("File");
                ui.label(&machine.path);
                ui.end_row();
                ui.label("State");
                ui.add(
                    egui::ProgressBar::new(machine.progress as f32).text(format!(
                        "{}  {:.2} / {:.2} s",
                        machine.state,
                        machine.progress * machine.duration,
                        machine.duration
                    )),
                );
                ui.end_row();
                if let Some(fade) = &machine.fade {
                    ui.label("Blending from");
                    ui.add(
                        egui::ProgressBar::new(fade.weight as f32)
                            .text(format!("{} over {:.2} s", fade.from, fade.seconds)),
                    );
                    ui.end_row();
                }
                ui.label("Root motion");
                ui.label(format!(
                    "{:.3}, {:.3}, {:.3}  turn {:.1}°",
                    machine.root_travel.x,
                    machine.root_travel.y,
                    machine.root_travel.z,
                    machine.root_yaw.to_degrees()
                ));
                ui.end_row();
            });
        if !machine.layers.is_empty() {
            ui.add_space(4.0);
            ui.label(egui::RichText::new("Layers").strong());
            egui::Grid::new("animator inspector layers")
                .num_columns(2)
                .striped(true)
                .show(ui, |ui| {
                    for layer in &machine.layers {
                        ui.label(&layer.name);
                        ui.add(
                            egui::ProgressBar::new(layer.weight as f32)
                                .text(format!("{}  weight {:.2}", layer.state, layer.weight)),
                        );
                        ui.end_row();
                    }
                });
        }
    } else if view.animator.is_none() {
        ui.weak("It has no Animator.");
    } else {
        ui.weak("Its Animator has no state machine.");
    }
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Playing over it").strong());
    if view.playing.is_empty() {
        ui.weak("No AnimationTracks or Animations.");
    }
    egui::Grid::new("animator inspector playing")
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            for playing in &view.playing {
                let clip = playing
                    .clip
                    .rsplit('/')
                    .next()
                    .unwrap_or(&playing.clip)
                    .to_owned();
                ui.label(clip).on_hover_text(&playing.clip);
                ui.add(
                    egui::ProgressBar::new(playing.progress as f32)
                        .text(format!("weight {:.2}", playing.weight)),
                );
                ui.end_row();
            }
        });
    if !view.parameters.is_empty() {
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Parameters").strong());
        egui::Grid::new("animator inspector parameters")
            .num_columns(2)
            .striped(true)
            .show(ui, |ui| {
                for (name, value) in &view.parameters {
                    ui.label(name);
                    ui.monospace(parameter_text(value));
                    ui.end_row();
                }
            });
    }
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Fired lately").strong());
    if view.fired.is_empty() {
        ui.weak("Nothing yet.");
    }
    for fired in &view.fired {
        ui.horizontal(|ui| {
            ui.weak(format!("{:.1} s ago", fired.seconds_ago));
            ui.label(&fired.what);
        });
    }
}

/// A parameter's value as the inspector shows it.
fn parameter_text(value: &Parameter) -> String {
    match value {
        Parameter::Bool(on) => on.to_string(),
        Parameter::Number(number) => format!("{number:.3}"),
        Parameter::Trigger(true) => "set".to_owned(),
        Parameter::Trigger(false) => "-".to_owned(),
    }
}
