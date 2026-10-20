//! The Animation panel's preview: one model, posed, on a checkered floor,
//! seen through its own camera and drawn by its own renderer, so it never
//! touches the world being edited or Play.

use std::sync::Arc;

use axcenity_assets::{Assets, Model, Posed};
use axcenity_core::{ClassName, Color3, InstanceId, Value, Vector3, World};
use axcenity_render::glam::Vec3;
use axcenity_render::{Camera, TextureRenderer, wgpu};
use axcenity_scene::Poses;
use eframe::egui;
use eframe::egui_wgpu::RenderState;

/// Floor tiles along each side.
const TILES: i32 = 12;

/// The preview's texture, registered with egui.
struct Target {
    renderer: TextureRenderer,
    texture_id: egui::TextureId,
}

/// A model on a floor, with an orbiting camera.
pub struct Preview {
    target: Option<Target>,
    /// Turned around the model (degrees), looking down (degrees), and how
    /// far away, as a multiple of the model's height.
    yaw: f32,
    pitch: f32,
    zoom: f32,
    world: World,
    part: Option<InstanceId>,
    /// The model it shows (a `MeshId`), and that model's key.
    shown: Option<(String, u64)>,
    /// Half the model's height, the point the camera looks at.
    middle: Vec3,
    height: f32,
}

impl Default for Preview {
    fn default() -> Self {
        Self {
            target: None,
            yaw: 35.0,
            pitch: 15.0,
            zoom: 2.2,
            world: World::new(),
            part: None,
            shown: None,
            middle: Vec3::ZERO,
            height: 1.0,
        }
    }
}

impl Preview {
    /// Shows `mesh_id`'s model, standing on the floor, if it isn't already.
    pub fn set_model(&mut self, mesh_id: &str, model: &Model) {
        if self
            .shown
            .as_ref()
            .is_some_and(|(id, key)| id == mesh_id && *key == model.key())
        {
            return;
        }
        let size = Vec3::from(model.size).max(Vec3::splat(0.01));
        let height = size.y.max(size.x).max(size.z);
        let mut world = World::new();
        let workspace = world.workspace();
        let tile = (height / 3.0).max(0.25);
        for x in -TILES / 2..TILES / 2 {
            for z in -TILES / 2..TILES / 2 {
                let light = (x + z).rem_euclid(2) == 0;
                let color = if light {
                    Color3::new(0.42, 0.44, 0.48)
                } else {
                    Color3::new(0.3, 0.32, 0.36)
                };
                let id = world.create(ClassName::Part).expect("not a service");
                let properties = [
                    ("Anchored", Value::Bool(true)),
                    ("Color", Value::Color3(color)),
                    (
                        "Size",
                        Value::Vector3(Vector3::new(tile, tile * 0.05, tile)),
                    ),
                    (
                        "Position",
                        Value::Vector3(Vector3::new(
                            (x as f32 + 0.5) * tile,
                            -tile * 0.025,
                            (z as f32 + 0.5) * tile,
                        )),
                    ),
                ];
                for (name, value) in properties {
                    world
                        .set_property(id, name, value)
                        .expect("Parts have these");
                }
                world
                    .set_parent(id, Some(workspace))
                    .expect("Parts go in the Workspace");
            }
        }
        let part = world.create(ClassName::MeshPart).expect("not a service");
        for (name, value) in [
            ("Anchored", Value::Bool(true)),
            ("MeshId", Value::String(mesh_id.to_owned())),
            ("Size", Value::Vector3(Vector3::new(size.x, size.y, size.z))),
            (
                "Position",
                Value::Vector3(Vector3::new(0.0, size.y / 2.0, 0.0)),
            ),
        ] {
            world
                .set_property(part, name, value)
                .expect("MeshParts have these");
        }
        world
            .set_parent(part, Some(workspace))
            .expect("MeshParts go in the Workspace");
        self.world = world;
        self.part = Some(part);
        self.shown = Some((mesh_id.to_owned(), model.key()));
        self.middle = Vec3::new(0.0, size.y / 2.0, 0.0);
        self.height = height;
    }

    /// Shows nothing (a message is drawn instead).
    pub fn clear(&mut self) {
        self.shown = None;
        self.part = None;
        self.world = World::new();
    }

    pub fn has_model(&self) -> bool {
        self.part.is_some()
    }

    fn camera(&self) -> Camera {
        let (yaw, pitch) = (self.yaw.to_radians(), self.pitch.to_radians());
        let direction = Vec3::new(
            yaw.sin() * pitch.cos(),
            pitch.sin(),
            yaw.cos() * pitch.cos(),
        );
        Camera {
            position: self.middle + direction * self.height * self.zoom,
            look_at: self.middle,
            fov_y_degrees: 50.0,
        }
    }

    /// Draws the preview into `ui`'s space, `height` points tall, with
    /// `posed` as the model's pose (or still). Dragging turns the camera
    /// around the model, and the wheel zooms.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&RenderState>,
        assets: &Assets,
        posed: Option<Arc<Posed>>,
        height: f32,
    ) {
        let size = egui::vec2(ui.available_width(), height);
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::drag());
        if response.dragged() {
            let delta = response.drag_delta();
            self.yaw -= delta.x * 0.4;
            self.pitch = (self.pitch + delta.y * 0.3).clamp(-10.0, 85.0);
        }
        if response.hovered() {
            let scroll = ui.input(|input| input.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.zoom = (self.zoom * (1.0 - scroll * 0.002)).clamp(0.6, 10.0);
            }
        }
        let painter = ui.painter_at(rect);
        let Some(render_state) = render_state else {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "The preview needs the wgpu renderer.",
                egui::FontId::proportional(13.0),
                ui.visuals().weak_text_color(),
            );
            return;
        };
        if !self.has_model() {
            painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Nothing to preview: pick a model with this skeleton below.",
                egui::FontId::proportional(13.0),
                ui.visuals().weak_text_color(),
            );
            return;
        }
        let camera = self.camera();
        let pixels_per_point = ui.ctx().pixels_per_point();
        let pixels = rect.size() * pixels_per_point;
        let (width, height) = (
            (pixels.x.round() as u32).max(1),
            (pixels.y.round() as u32).max(1),
        );
        let target = self.target.get_or_insert_with(|| {
            let renderer =
                TextureRenderer::new(&render_state.device, &render_state.queue, width, height);
            let texture_id = render_state.renderer.write().register_native_texture(
                &render_state.device,
                renderer.view(),
                wgpu::FilterMode::Linear,
            );
            Target {
                renderer,
                texture_id,
            }
        });
        if target.renderer.size() != (width, height) {
            target.renderer.resize(width, height);
            render_state
                .renderer
                .write()
                .update_egui_texture_from_wgpu_texture(
                    &render_state.device,
                    target.renderer.view(),
                    wgpu::FilterMode::Linear,
                    target.texture_id,
                );
        }
        let mut poses = Poses::new();
        if let (Some(part), Some(posed)) = (self.part, posed) {
            poses.insert(part, posed);
        }
        let list = axcenity_scene::build_draw_list_posed(&self.world, assets, camera, &poses);
        if let Err(error) = target.renderer.render(&list) {
            log::error!("could not render the animation preview: {error:#}");
        }
        let whole = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        painter.image(target.texture_id, rect, whole, egui::Color32::WHITE);
        if response.hovered() {
            painter.text(
                rect.left_bottom() + egui::vec2(8.0, -6.0),
                egui::Align2::LEFT_BOTTOM,
                "Drag to turn · scroll to zoom",
                egui::FontId::proportional(11.0),
                egui::Color32::from_white_alpha(160),
            );
        }
    }
}
