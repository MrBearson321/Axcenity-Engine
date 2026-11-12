//! Opening an Instance from the Explorer (cohesion pass CW3.2): the one
//! place that picks its editor (`explorer::opens_with`) and shows a tab
//! that's already open rather than another.

use super::*;
use crate::explorer::{Opens, opens_with, scene_bounds};

impl EditorApp {
    /// Opens `id` as a double-click on its Explorer row does: a script in
    /// its tab, UI in the UI Designer, an Animation's clip or an Animator's
    /// state machine in the Animation tab, and anything else in the scene
    /// selected and framed in the viewport.
    pub(in crate::app) fn open_instance(&mut self, ctx: &egui::Context, id: InstanceId) {
        let world = shown(&self.world, &self.play);
        let opens = opens_with(world, id);
        let bounds = scene_bounds(world, id);
        match opens {
            Opens::Script => self.open_script_instance(id),
            Opens::UiDesigner => {
                self.select_from_viewport(Some(id));
                self.layout.open(Pane::UiDesigner);
            }
            Opens::Clip(path) => {
                self.select_from_viewport(Some(id));
                self.open_animation_clip(&path, false);
            }
            Opens::StateMachine => {
                self.select_from_viewport(Some(id));
                self.open_animator_controller(id);
            }
            Opens::View => {
                let Some((center, radius)) = bounds else {
                    return;
                };
                self.select_from_viewport(Some(id));
                let seconds = if ctx.global_style().animation_time > 0.0 {
                    FRAME_SECONDS
                } else {
                    0.0
                };
                let now = ctx.input(|input| input.time);
                self.viewport.frame(center, radius, now, seconds);
                self.layout.open(Pane::Scene);
                ctx.request_repaint();
            }
            Opens::Nothing => {}
        }
    }
}

/// How long the camera takes to swing round to a double-clicked object.
const FRAME_SECONDS: f32 = 0.45;
