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

impl EditorApp {
    /// Shows `id` alone in the Object Viewer tab (cohesion pass CW5.2),
    /// a copy of it as it is now, and brings the tab forward.
    pub(in crate::app) fn view_instance(&mut self, id: InstanceId) {
        let world = shown(&self.world, &self.play);
        let name = world
            .get(id)
            .map_or_else(String::new, |instance| instance.name().to_owned());
        if self.object_viewer.show_instance(world, id) {
            self.layout.open(Pane::ObjectViewer);
        } else {
            self.output.info(format!(
                "{name} has no part to look at in the Object Viewer."
            ));
        }
    }

    /// Shows the model, prefab or material file at `path` in the Object Viewer tab.
    pub(in crate::app) fn view_asset(&mut self, path: &str) {
        let name = path.rsplit('/').next().unwrap_or(path).to_owned();
        let shown = if path.ends_with(crate::asset_browser::PREFAB_SUFFIX) {
            let text = match self.assets.read_file(path) {
                Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
                Err(err) => {
                    self.output.error(format!("Can't view {name}: {err}"));
                    return;
                }
            };
            // Its Instances in a Model of their own, to show as one.
            let mut world = World::new();
            let model = world.create(ClassName::Model).expect("not a service");
            let _ = world.set_name(
                model,
                name.trim_end_matches(crate::asset_browser::PREFAB_SUFFIX),
            );
            let workspace = world.workspace();
            let _ = world.set_parent(model, Some(workspace));
            if let Err(err) = axcenity_project::parse_prefab(&text, &mut world, model) {
                self.output.error(format!("Can't view {name}: {err}"));
                return;
            }
            self.object_viewer.show_instance(&world, model)
        } else if path.ends_with(axcenity_assets::MATERIAL_SUFFIX) {
            // A material is shown on a ball, where its shading reads best.
            let mut world = World::new();
            let ball = world.create(ClassName::Part).expect("not a service");
            let _ = world.set_name(
                ball,
                name.trim_end_matches(axcenity_assets::MATERIAL_SUFFIX),
            );
            let _ = world.set_property(ball, "Shape", Value::PartType(PartType::Ball));
            let _ = world.set_property(ball, "Size", Value::Vector3(Vector3::new(4.0, 4.0, 4.0)));
            let workspace = world.workspace();
            let _ = world.set_parent(ball, Some(workspace));
            super::material_files::take_material(&mut world, ball, path);
            self.object_viewer.show_instance(&world, ball)
        } else {
            match self.assets.model_blocking(path) {
                Some(model) => self.object_viewer.show_model(path, &model),
                None => {
                    self.output
                        .error(format!("Can't view {name}: it didn't load."));
                    return;
                }
            }
        };
        if shown {
            self.layout.open(Pane::ObjectViewer);
        } else {
            self.output.info(format!(
                "{name} has no part to look at in the Object Viewer."
            ));
        }
    }
}
