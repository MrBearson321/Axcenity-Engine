//! The Object Viewer (cohesion pass CW5.1): one object, or one model file,
//! alone in a world of its own, to look at from every side. It orbits,
//! pans and zooms, frames and resets, and has lighting presets, a floor and
//! a sky to turn off. It draws on the shared preview [`Canvas`], and only
//! when what it shows changes: a still view costs nothing.

use axcenity_assets::{Assets, Model};
use axcenity_core::{ClassName, Color3, InstanceId, Value, Vector3, World};
use axcenity_render::Camera;
use axcenity_render::glam::Vec3;
use axcenity_scene::part_box_edges;
use eframe::egui;
use eframe::egui_wgpu::RenderState;

use crate::preview::{Canvas, add_floor};

/// The camera's field of view, degrees.
const FOV: f32 = 45.0;
/// How it starts looking: turned round (degrees) and looking down (degrees).
const START_YAW: f32 = 35.0;
const START_PITCH: f32 = 20.0;

/// How the viewer lights what it shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lighting {
    /// A soft, even grey: shapes and materials, nothing else.
    #[default]
    Studio,
    /// Midday sun and a blue sky.
    Day,
    /// Low, warm sun.
    Sunset,
    /// Moonlight.
    Night,
}

impl Lighting {
    pub const ALL: [Lighting; 4] = [
        Lighting::Studio,
        Lighting::Day,
        Lighting::Sunset,
        Lighting::Night,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Lighting::Studio => "Studio",
            Lighting::Day => "Day",
            Lighting::Sunset => "Sunset",
            Lighting::Night => "Night",
        }
    }

    /// Lighting's properties for this preset: the time of day and the sky.
    fn properties(self) -> [(&'static str, Value); 4] {
        let color = |r, g, b| Value::Color3(Color3::new(r, g, b));
        match self {
            Lighting::Studio => [
                ("ClockTime", Value::Number(13.0)),
                ("SkyTopColor", color(0.42, 0.44, 0.48)),
                ("SkyHorizonColor", color(0.62, 0.63, 0.66)),
                ("EnvironmentIntensity", Value::Number(0.9)),
            ],
            Lighting::Day => [
                ("ClockTime", Value::Number(13.0)),
                ("SkyTopColor", color(0.25, 0.5, 0.9)),
                ("SkyHorizonColor", color(0.63, 0.8, 0.95)),
                ("EnvironmentIntensity", Value::Number(0.6)),
            ],
            Lighting::Sunset => [
                ("ClockTime", Value::Number(18.2)),
                ("SkyTopColor", color(0.3, 0.32, 0.6)),
                ("SkyHorizonColor", color(0.95, 0.55, 0.3)),
                ("EnvironmentIntensity", Value::Number(0.5)),
            ],
            Lighting::Night => [
                ("ClockTime", Value::Number(0.0)),
                ("SkyTopColor", color(0.02, 0.03, 0.08)),
                ("SkyHorizonColor", color(0.08, 0.1, 0.18)),
                ("EnvironmentIntensity", Value::Number(0.35)),
            ],
        }
    }
}

/// What the camera does, kept to tell when the view needs drawing again.
#[derive(Clone, Copy, Debug, PartialEq)]
struct View {
    yaw: f32,
    pitch: f32,
    /// How far from the middle, as a multiple of the object's reach.
    zoom: f32,
    /// How far the middle was moved from the object's.
    pan: Vec3,
}

impl Default for View {
    fn default() -> Self {
        Self {
            yaw: START_YAW,
            pitch: START_PITCH,
            zoom: framing_zoom(),
            pan: Vec3::ZERO,
        }
    }
}

/// The zoom at which an object's bounding sphere fills the view, with a
/// little room round it.
fn framing_zoom() -> f32 {
    1.15 / (FOV.to_radians() / 2.0).sin()
}

/// What was drawn last: if nothing in it changed, the last picture stands.
#[derive(Clone, Debug, PartialEq)]
struct Drawn {
    view: View,
    size: (u32, u32),
    version: u64,
}

/// One object on show (see the module docs).
#[derive(Default)]
pub struct ObjectViewer {
    canvas: Canvas,
    /// What it shows, as the user picked it: the object alone.
    subject: World,
    /// `subject` with the floor and lighting: what's drawn.
    shown: World,
    /// Its name, for the toolbar.
    title: String,
    /// The middle of the object's box, and how far its corners reach.
    middle: Vec3,
    reach: f32,
    bottom: f32,
    view: View,
    pub lighting: Lighting,
    /// Off: no floor under the object.
    pub hide_floor: bool,
    /// Off: a plain backdrop instead of the sky.
    pub hide_sky: bool,
    /// Counts changes to what's shown (object, lighting, floor, sky).
    version: u64,
    drawn: Option<Drawn>,
    /// How many times it has drawn, for tests.
    pub draws: usize,
}

impl ObjectViewer {
    /// Whether it shows anything.
    pub fn is_empty(&self) -> bool {
        self.title.is_empty()
    }

    /// The name of what it shows.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// What it shows, as copied in (its Workspace holds it).
    pub fn subject(&self) -> &World {
        &self.subject
    }

    /// Shows a copy of `id` from `world`, and everything inside it.
    /// Returns false (and shows nothing new) if it has no part to see.
    pub fn show_instance(&mut self, world: &World, id: InstanceId) -> bool {
        let mut subject = World::new();
        let Ok(copy) = subject.copy_from(world, id) else {
            return false;
        };
        let workspace = subject.workspace();
        if subject.set_parent(copy, Some(workspace)).is_err() {
            return false;
        }
        let title = world
            .get(id)
            .map_or_else(String::new, |instance| instance.name().to_owned());
        self.take(subject, title)
    }

    /// Shows the model file `mesh_id` as one MeshPart.
    pub fn show_model(&mut self, mesh_id: &str, model: &Model) -> bool {
        let mut subject = World::new();
        let size = Vec3::from(model.size).max(Vec3::splat(0.01));
        let part = subject.create(ClassName::MeshPart).expect("not a service");
        for (name, value) in [
            ("Anchored", Value::Bool(true)),
            ("MeshId", Value::String(mesh_id.to_owned())),
            ("Size", Value::Vector3(Vector3::new(size.x, size.y, size.z))),
            (
                "Position",
                Value::Vector3(Vector3::new(0.0, size.y / 2.0, 0.0)),
            ),
        ] {
            subject
                .set_property(part, name, value)
                .expect("MeshParts have these");
        }
        let workspace = subject.workspace();
        subject
            .set_parent(part, Some(workspace))
            .expect("MeshParts go in the Workspace");
        let title = mesh_id.rsplit('/').next().unwrap_or(mesh_id).to_owned();
        self.take(subject, title)
    }

    /// Shows `subject` (whose Workspace holds the object), framed.
    fn take(&mut self, subject: World, title: String) -> bool {
        let Some((low, high)) = parts_box(&subject) else {
            return false;
        };
        self.middle = (low + high) / 2.0;
        self.reach = ((high - low).length() / 2.0).max(0.25);
        self.bottom = low.y;
        self.subject = subject;
        self.title = title;
        self.reset();
        self.rebuild();
        true
    }

    /// Puts the whole object in view, keeping the way the camera looks.
    pub fn frame(&mut self) {
        self.view.zoom = framing_zoom();
        self.view.pan = Vec3::ZERO;
    }

    /// Back to the first view: turned a little, looking down, framed.
    pub fn reset(&mut self) {
        self.view = View::default();
    }

    pub fn set_lighting(&mut self, lighting: Lighting) {
        if self.lighting != lighting {
            self.lighting = lighting;
            self.rebuild();
        }
    }

    pub fn set_floor(&mut self, on: bool) {
        if self.hide_floor == on {
            self.hide_floor = !on;
            self.rebuild();
        }
    }

    pub fn set_sky(&mut self, on: bool) {
        if self.hide_sky == on {
            self.hide_sky = !on;
            self.rebuild();
        }
    }

    /// Makes the drawn world from the subject and the settings.
    fn rebuild(&mut self) {
        let mut shown = self.subject.clone();
        if !self.hide_floor {
            let tile = (self.reach / 3.0).max(0.25);
            add_floor(
                &mut shown,
                Vec3::new(self.middle.x, 0.0, self.middle.z),
                self.bottom,
                tile,
            );
        }
        let lighting = shown.lighting();
        let mut properties = self.lighting.properties().to_vec();
        if self.hide_sky {
            let backdrop = Value::Color3(Color3::new(0.16, 0.17, 0.19));
            properties.push(("SkyTopColor", backdrop.clone()));
            properties.push(("SkyHorizonColor", backdrop));
        }
        for (name, value) in properties {
            let _ = shown.set_property(lighting, name, value);
        }
        self.shown = shown;
        self.version += 1;
    }

    /// The camera, from the view.
    pub fn camera(&self) -> Camera {
        let (yaw, pitch) = (self.view.yaw.to_radians(), self.view.pitch.to_radians());
        let direction = Vec3::new(
            yaw.sin() * pitch.cos(),
            pitch.sin(),
            yaw.cos() * pitch.cos(),
        );
        let look_at = self.middle + self.view.pan;
        Camera {
            position: look_at + direction * self.reach * self.view.zoom,
            look_at,
            fov_y_degrees: FOV,
        }
    }

    /// Turns the camera round the object by a drag of `delta` points.
    pub fn orbit(&mut self, delta: egui::Vec2) {
        self.view.yaw -= delta.x * 0.4;
        self.view.pitch = (self.view.pitch + delta.y * 0.3).clamp(-85.0, 85.0);
    }

    /// Slides the camera sideways and up by a drag of `delta` points in a
    /// view `height` points tall, so the object follows the mouse.
    pub fn pan(&mut self, delta: egui::Vec2, height: f32) {
        let camera = self.camera();
        let forward = (camera.look_at - camera.position).normalize_or_zero();
        let right = forward.cross(Vec3::Y).normalize_or(Vec3::X);
        let up = right.cross(forward);
        let distance = camera.position.distance(camera.look_at);
        let per_point = 2.0 * distance * (FOV.to_radians() / 2.0).tan() / height.max(1.0);
        self.view.pan += (-right * delta.x + up * delta.y) * per_point;
    }

    /// Zooms by a wheel turn of `scroll` points (up comes closer).
    pub fn zoom(&mut self, scroll: f32) {
        self.view.zoom = (self.view.zoom * (1.0 - scroll * 0.002)).clamp(0.3, 40.0);
    }

    /// Draws the toolbar and the view into `ui`, filling it.
    pub fn ui(&mut self, ui: &mut egui::Ui, render_state: Option<&RenderState>, assets: &Assets) {
        self.toolbar(ui);
        let size = ui.available_size().max(egui::vec2(1.0, 1.0));
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        if response.dragged_by(egui::PointerButton::Primary) {
            self.orbit(response.drag_delta());
        }
        if response.dragged_by(egui::PointerButton::Secondary)
            || response.dragged_by(egui::PointerButton::Middle)
        {
            self.pan(response.drag_delta(), rect.height());
        }
        if response.double_clicked() {
            self.frame();
        }
        if response.hovered() {
            let scroll = ui.input(|input| input.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.zoom(scroll);
            }
        }
        let painter = ui.painter_at(rect);
        if self.is_empty() {
            painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Right-click an object in the Explorer, or a model, prefab or material in Assets, \
                 and pick View to look at it here.",
                egui::FontId::proportional(13.0),
                ui.visuals().weak_text_color(),
            );
            return;
        }
        let Some(render_state) = render_state else {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "The Object Viewer needs the wgpu renderer.",
                egui::FontId::proportional(13.0),
                ui.visuals().weak_text_color(),
            );
            return;
        };
        let drawn = Drawn {
            view: self.view,
            size: Canvas::pixels(ui, rect),
            version: self.version,
        };
        // Textures still loading change the picture by themselves.
        let loading = assets.texture_progress().is_some();
        if loading || self.drawn.as_ref() != Some(&drawn) || !self.canvas.show_last(ui, rect) {
            let list = axcenity_scene::build_draw_list(&self.shown, assets, self.camera());
            self.canvas.paint(ui, rect, render_state, &list);
            self.draws += 1;
            self.drawn = Some(drawn);
            if loading {
                ui.ctx().request_repaint();
            }
        }
        if response.hovered() {
            painter.text(
                rect.left_bottom() + egui::vec2(8.0, -6.0),
                egui::Align2::LEFT_BOTTOM,
                "Drag to turn · right-drag to pan · scroll to zoom · double-click to frame",
                egui::FontId::proportional(11.0),
                egui::Color32::from_white_alpha(170),
            );
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        use crate::theme::icons;
        use crate::widgets;
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(&self.title).strong());
            if widgets::icon_button(
                ui,
                icons::CORNERS_OUT,
                "Frame: the whole object in view",
                false,
            )
            .clicked()
            {
                self.frame();
            }
            if widgets::icon_button(ui, icons::ARROW_COUNTER_CLOCKWISE, "Reset the view", false)
                .clicked()
            {
                self.reset();
            }
            let mut lighting = self.lighting;
            let name = lighting.name();
            widgets::menu_button(ui, Some(icons::SUN), name, |ui| {
                for preset in Lighting::ALL {
                    if ui
                        .selectable_label(lighting == preset, preset.name())
                        .clicked()
                    {
                        lighting = preset;
                        ui.close();
                    }
                }
            });
            self.set_lighting(lighting);
            let mut floor = !self.hide_floor;
            if ui.checkbox(&mut floor, "Floor").changed() {
                self.set_floor(floor);
            }
            let mut sky = !self.hide_sky;
            if ui.checkbox(&mut sky, "Sky").changed() {
                self.set_sky(sky);
            }
        });
    }
}

/// The box round every part in `world`'s Workspace, as its lowest and
/// highest corners; `None` with no part.
fn parts_box(world: &World) -> Option<(Vec3, Vec3)> {
    let mut low = Vec3::splat(f32::INFINITY);
    let mut high = Vec3::splat(f32::NEG_INFINITY);
    for id in world.descendants(world.workspace()) {
        for (a, b) in part_box_edges(world, id).into_iter().flatten() {
            low = low.min(a).min(b);
            high = high.max(a).max(b);
        }
    }
    (low.x <= high.x).then_some((low, high))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world_with_a_tower() -> (World, InstanceId) {
        let mut world = World::new();
        let model = world.create(ClassName::Model).unwrap();
        world.set_name(model, "Tower").unwrap();
        let workspace = world.workspace();
        world.set_parent(model, Some(workspace)).unwrap();
        for (y, height) in [(1.0, 2.0), (4.0, 4.0)] {
            let part = world.create(ClassName::Part).unwrap();
            world
                .set_property(part, "Size", Value::Vector3(Vector3::new(2.0, height, 2.0)))
                .unwrap();
            world
                .set_property(part, "Position", Value::Vector3(Vector3::new(10.0, y, 0.0)))
                .unwrap();
            world.set_parent(part, Some(model)).unwrap();
        }
        (world, model)
    }

    #[test]
    fn it_shows_a_copy_framed_and_never_touches_the_world() {
        let (world, model) = world_with_a_tower();
        let before = world.descendants(world.root()).len();
        let mut viewer = ObjectViewer::default();
        assert!(viewer.show_instance(&world, model));
        assert_eq!(world.descendants(world.root()).len(), before);
        assert_eq!(viewer.title(), "Tower");
        let camera = viewer.camera();
        // The box runs from y 0 to 6 at x 10: the camera looks at its middle.
        assert!(camera.look_at.distance(Vec3::new(10.0, 3.0, 0.0)) < 1e-4);
        // Something with no part has nothing to see.
        let folder = World::new().workspace();
        assert!(!viewer.show_instance(&World::new(), folder));
        assert_eq!(viewer.title(), "Tower", "the last object stays");
    }

    #[test]
    fn orbit_pan_zoom_then_frame_and_reset_bring_it_back() {
        let (world, model) = world_with_a_tower();
        let mut viewer = ObjectViewer::default();
        viewer.show_instance(&world, model);
        let start = viewer.camera();
        viewer.orbit(egui::vec2(40.0, 10.0));
        viewer.pan(egui::vec2(30.0, 0.0), 400.0);
        viewer.zoom(120.0);
        let moved = viewer.camera();
        assert!(moved.look_at.distance(start.look_at) > 0.1, "panned");
        viewer.frame();
        let framed = viewer.camera();
        assert!(framed.look_at.distance(start.look_at) < 1e-4);
        assert!(
            (framed.position.distance(framed.look_at) - start.position.distance(start.look_at))
                .abs()
                < 1e-3
        );
        viewer.reset();
        assert!(viewer.camera().position.distance(start.position) < 1e-3);
    }

    #[test]
    fn floor_sky_and_lighting_change_only_the_shown_world() {
        let (world, model) = world_with_a_tower();
        let mut viewer = ObjectViewer::default();
        viewer.show_instance(&world, model);
        let parts =
            |viewer: &ObjectViewer| viewer.shown.descendants(viewer.shown.workspace()).len();
        let with_floor = parts(&viewer);
        viewer.set_floor(false);
        assert!(parts(&viewer) < with_floor, "the floor's tiles go");
        let version = viewer.version;
        viewer.set_floor(false);
        assert_eq!(viewer.version, version, "no change, nothing to draw");
        viewer.set_lighting(Lighting::Night);
        let lighting = viewer.shown.lighting();
        assert_eq!(
            viewer.shown.get(lighting).unwrap().property("ClockTime"),
            Some(&Value::Number(0.0))
        );
        viewer.set_sky(false);
        assert_eq!(
            viewer.shown.get(lighting).unwrap().property("SkyTopColor"),
            Some(&Value::Color3(Color3::new(0.16, 0.17, 0.19)))
        );
        assert_eq!(
            viewer.subject.descendants(viewer.subject.workspace()).len(),
            3
        );
    }
}
