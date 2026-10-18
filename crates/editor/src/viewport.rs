//! The 3D viewport: the world drawn by the same renderer as the player, into
//! a texture that egui shows, with a Studio-style camera.
//!
//! Controls: right-drag to look around, WASD to fly, E/Q for up and down,
//! Shift to go slowly, the mouse wheel to zoom toward the focus point,
//! middle-drag to pan, and Alt+left-drag to orbit around the focus point.
//! Shift+wheel, or the wheel while right-dragging, changes the flying speed,
//! and a pop-up in the middle of the viewport shows the new speed.
//!
//! Left-clicking a part selects it, and the selected part gets a blue box.

use axcenity_core::{InstanceId, World};
use axcenity_render::TextureRenderer;
use axcenity_render::glam::{Vec2, Vec3};
use axcenity_render::wgpu;
use axcenity_scene::{CameraView, FlyCamera, MoveKeys};
use eframe::egui;
use eframe::egui_wgpu::RenderState;

/// Where the camera starts, looking at the origin (the same as the player).
const START_CAMERA_POSITION: Vec3 = Vec3::new(12.0, 8.0, 16.0);
/// After a long pause, don't jump the camera by the whole pause; treat the
/// frame as at most this long.
const MAX_FRAME_SECONDS: f32 = 0.1;
/// Zoom per point of scrolling. One wheel notch is 40 points, which moves
/// about a fifth of the way to the focus point.
const ZOOM_PER_POINT: f32 = 0.005;
/// One mouse wheel notch, in egui's scroll points. Each notch is one step of
/// `FlyCamera::change_speed` (20% faster or slower).
const POINTS_PER_NOTCH: f32 = 40.0;
/// How long the speed pop-up stays fully visible after the last change,
/// then how long it takes to fade out, in seconds.
const POPUP_SECONDS: f64 = 1.0;
const POPUP_FADE_SECONDS: f64 = 0.4;
/// The selection box: a bright blue, like Studio's, that stands out against
/// the sky and most part colors.
const SELECTION_COLOR: egui::Color32 = egui::Color32::from_rgb(25, 160, 255);
const SELECTION_WIDTH: f32 = 2.0;

/// A left click in the viewport, with the part under the mouse (`None` for
/// empty space, which clears the selection).
pub struct Clicked {
    pub part: Option<InstanceId>,
}

pub struct Viewport {
    camera: FlyCamera,
    keys: MoveKeys,
    /// True while right-dragging, when the cursor is hidden and held.
    looking: bool,
    /// Made on the first frame, once eframe has set up the GPU.
    target: Option<Target>,
    /// When the flying speed last changed (egui's clock, in seconds), to
    /// show and fade the speed pop-up.
    speed_changed_at: Option<f64>,
}

/// The texture the scene is drawn into, and egui's name for it.
struct Target {
    renderer: TextureRenderer,
    texture_id: egui::TextureId,
}

impl Viewport {
    pub fn new() -> Self {
        Self {
            camera: start_camera(),
            keys: MoveKeys::default(),
            looking: false,
            target: None,
            speed_changed_at: None,
        }
    }

    /// Puts the camera back where it starts, for a newly opened project.
    /// The flying speed is a preference, not part of the project, so it stays.
    pub fn reset_camera(&mut self) {
        let speed = self.camera.speed();
        self.camera = start_camera();
        self.camera.set_speed(speed);
    }

    /// Fills `ui` with the viewport: moves the camera from this frame's
    /// input, then draws `world` from there with a box around `selected`.
    /// Returns what a left click picked, if there was one.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        render_state: &RenderState,
        world: &World,
        selected: Option<InstanceId>,
    ) -> Option<Clicked> {
        let size = ui.available_size();
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        self.handle_input(ui, &response, rect.height());
        let view = self.camera_view(rect);
        let clicked = self.clicked_part(ui, &response, rect, &view, world);

        // The texture is sized in physical pixels, so it stays sharp on
        // high-DPI screens where one point is several pixels.
        let pixels = size * ui.ctx().pixels_per_point();
        let (width, height) = (pixels.x.round() as u32, pixels.y.round() as u32);
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
        if target.renderer.size() != (width.max(1), height.max(1)) {
            target.renderer.resize(width, height);
            // The resize made a new texture, so point egui's name at it.
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

        let draw_list = axcenity_scene::build_draw_list(world, self.camera.to_render_camera());
        target.renderer.render(&draw_list);
        let whole_texture = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        ui.painter()
            .image(target.texture_id, rect, whole_texture, egui::Color32::WHITE);
        if let Some(selected) = selected {
            paint_selection_box(ui, rect, &view, world, selected);
        }
        self.paint_speed_popup(ui, rect);
        clicked
    }

    /// The camera as it draws into `rect`, for turning between screen and
    /// world positions.
    fn camera_view(&self, rect: egui::Rect) -> CameraView {
        // `max` keeps a zero-height (minimized) viewport from dividing by zero.
        let aspect = rect.width() / rect.height().max(1.0);
        CameraView::new(self.camera.to_render_camera(), aspect)
    }

    /// If this frame has a plain left click (not the end of a drag, and not
    /// Alt for orbiting), the part under the mouse.
    fn clicked_part(
        &self,
        ui: &egui::Ui,
        response: &egui::Response,
        rect: egui::Rect,
        view: &CameraView,
        world: &World,
    ) -> Option<Clicked> {
        if !response.clicked() || ui.input(|input| input.modifiers.alt) {
            return None;
        }
        let pointer = response.interact_pointer_pos()?;
        let ray = view.ray(to_screen_point(rect, pointer));
        Some(Clicked {
            part: axcenity_scene::pick(world, ray),
        })
    }

    /// Shows the flying speed in the middle of the viewport for a moment
    /// after it changes, then fades it out.
    fn paint_speed_popup(&self, ui: &egui::Ui, rect: egui::Rect) {
        let Some(changed_at) = self.speed_changed_at else {
            return;
        };
        let opacity = popup_opacity(ui.input(|input| input.time) - changed_at);
        if opacity <= 0.0 {
            return;
        }
        // egui only redraws when something happens, so ask for the frames
        // that keep the pop-up up and fade it.
        ui.ctx().request_repaint();

        let painter = ui.painter_at(rect);
        let galley = painter.layout_no_wrap(
            speed_label(self.camera.speed()),
            egui::FontId::proportional(18.0),
            egui::Color32::WHITE.gamma_multiply(opacity),
        );
        let background =
            egui::Rect::from_center_size(rect.center(), galley.size() + egui::vec2(28.0, 16.0));
        let shade = egui::Color32::from_black_alpha((170.0 * opacity) as u8);
        painter.rect_filled(background, 6.0, shade);
        painter.galley(
            rect.center() - galley.size() / 2.0,
            galley,
            egui::Color32::WHITE,
        );
    }

    fn handle_input(&mut self, ui: &egui::Ui, response: &egui::Response, view_height: f32) {
        let ctx = ui.ctx();
        // Read this *before* `ui.input`: egui locks its context while the
        // `ui.input` closure runs, and `drag_delta` takes the same lock, so
        // calling it inside the closure waits for itself forever (a deadlock).
        let drag = response.drag_delta();
        let (motion, scroll, wheel, alt, shift, seconds, focused, time) = ui.input(|input| {
            // Key presses and releases, by key position rather than by
            // letter, so WASD sits in the same place on AZERTY keyboards.
            for event in &input.events {
                if let egui::Event::Key {
                    key,
                    physical_key,
                    pressed,
                    ..
                } = event
                {
                    set_move_key(&mut self.keys, physical_key.unwrap_or(*key), *pressed);
                }
            }
            (
                // Raw mouse movement keeps coming while the cursor is held
                // still; it's missing on some platforms, so fall back to the drag.
                input.pointer.motion().unwrap_or(drag),
                input.smooth_scroll_delta.y,
                // The wheel without egui's smoothing, so each notch is
                // exactly one speed step. egui turns Shift+wheel into
                // sideways scrolling (and macOS already does), so add both.
                input.raw_scroll_delta.x + input.raw_scroll_delta.y,
                input.modifiers.alt,
                input.modifiers.shift,
                input.stable_dt.min(MAX_FRAME_SECONDS),
                input.focused,
                input.time,
            )
        });
        // Key releases aren't reported while another window has focus, so
        // let go of everything to avoid the camera drifting on its own.
        if !focused {
            self.keys = MoveKeys::default();
        }
        self.keys.slow = shift;

        let looking = response.dragged_by(egui::PointerButton::Secondary);
        if looking != self.looking {
            self.looking = looking;
            capture_mouse(ctx, looking);
        }
        if looking {
            self.camera.look(motion.x, motion.y);
        } else if alt && response.dragged_by(egui::PointerButton::Primary) {
            self.camera.orbit(motion.x, motion.y);
        } else if response.dragged_by(egui::PointerButton::Middle) {
            self.camera.pan(drag.x, drag.y, view_height);
        }
        // While flying (right button held) or holding Shift, the wheel sets
        // the flying speed, like Unity and Godot. Otherwise it zooms.
        let over_viewport = response.contains_pointer() || looking;
        if over_viewport && (looking || shift) {
            if wheel != 0.0 {
                self.camera.change_speed(wheel / POINTS_PER_NOTCH);
                self.speed_changed_at = Some(time);
            }
        } else if response.hovered() && scroll != 0.0 {
            self.camera.zoom(scroll * ZOOM_PER_POINT);
        }

        // Keys fly the camera while the mouse is over the viewport, unless
        // some text box is taking the typing.
        let flying = (response.contains_pointer() || looking) && !ctx.wants_keyboard_input();
        let keys = &self.keys;
        let moving = keys.forward || keys.back || keys.left || keys.right || keys.up || keys.down;
        if flying && moving {
            self.camera.update(keys, seconds);
            // egui only redraws when something happens, and holding a key
            // down isn't an event, so ask for the next frame.
            ctx.request_repaint();
        }
    }
}

/// Draws the blue box around the selected part, on top of everything so it
/// shows even behind other parts. Draws nothing if `selected` isn't a part.
fn paint_selection_box(
    ui: &egui::Ui,
    rect: egui::Rect,
    view: &CameraView,
    world: &World,
    selected: InstanceId,
) {
    let Some(edges) = axcenity_scene::part_box_edges(world, selected) else {
        return;
    };
    let painter = ui.painter_at(rect);
    let stroke = egui::Stroke::new(SELECTION_WIDTH, SELECTION_COLOR);
    for (a, b) in edges {
        if let Some((a, b)) = view.project_line(a, b) {
            painter.line_segment([to_ui_point(rect, a), to_ui_point(rect, b)], stroke);
        }
    }
}

/// Turns a position in the viewport (egui points, Y down) into the
/// renderer's screen coordinates (-1 to 1, Y up).
fn to_screen_point(rect: egui::Rect, point: egui::Pos2) -> Vec2 {
    let across = (point - rect.min) / rect.size();
    Vec2::new(across.x * 2.0 - 1.0, 1.0 - across.y * 2.0)
}

/// The reverse of [`to_screen_point`].
fn to_ui_point(rect: egui::Rect, screen: Vec2) -> egui::Pos2 {
    rect.min + egui::vec2((screen.x + 1.0) / 2.0, (1.0 - screen.y) / 2.0) * rect.size()
}

fn start_camera() -> FlyCamera {
    FlyCamera::looking_at(START_CAMERA_POSITION, Vec3::ZERO)
}

/// How visible the speed pop-up is, from 1 (fully) to 0 (gone), `age`
/// seconds after the speed last changed.
fn popup_opacity(age: f64) -> f32 {
    let fading = (age - POPUP_SECONDS) / POPUP_FADE_SECONDS;
    (1.0 - fading).clamp(0.0, 1.0) as f32
}

/// The pop-up text, with a decimal only for slow speeds where it matters.
fn speed_label(speed: f32) -> String {
    if speed < 10.0 {
        format!("Fly speed: {speed:.1} studs/s")
    } else {
        format!("Fly speed: {speed:.0} studs/s")
    }
}

/// Records a movement key press or release.
fn set_move_key(keys: &mut MoveKeys, key: egui::Key, pressed: bool) {
    let held = match key {
        egui::Key::W => &mut keys.forward,
        egui::Key::S => &mut keys.back,
        egui::Key::A => &mut keys.left,
        egui::Key::D => &mut keys.right,
        egui::Key::E => &mut keys.up,
        egui::Key::Q => &mut keys.down,
        _ => return,
    };
    *held = pressed;
}

/// While looking around, hides the cursor and keeps it in the window.
fn capture_mouse(ctx: &egui::Context, captured: bool) {
    // macOS can only hold the cursor still (`Locked`); Windows and X11 can
    // only keep it inside the window (`Confined`). Wayland can do both.
    let grab = match (captured, cfg!(target_os = "macos")) {
        (false, _) => egui::CursorGrab::None,
        (true, true) => egui::CursorGrab::Locked,
        (true, false) => egui::CursorGrab::Confined,
    };
    ctx.send_viewport_cmd(egui::ViewportCommand::CursorGrab(grab));
    ctx.send_viewport_cmd(egui::ViewportCommand::CursorVisible(!captured));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wasd_e_and_q_fly_the_camera_and_other_keys_do_nothing() {
        let mut keys = MoveKeys::default();
        set_move_key(&mut keys, egui::Key::W, true);
        set_move_key(&mut keys, egui::Key::E, true);
        set_move_key(&mut keys, egui::Key::Z, true);
        assert!(keys.forward && keys.up);
        assert!(!(keys.back || keys.left || keys.right || keys.down));
        set_move_key(&mut keys, egui::Key::W, false);
        assert!(!keys.forward);
    }

    #[test]
    fn speed_popup_stays_for_a_second_then_fades_out() {
        assert_eq!(popup_opacity(0.0), 1.0);
        assert_eq!(popup_opacity(POPUP_SECONDS), 1.0);
        let halfway = popup_opacity(POPUP_SECONDS + POPUP_FADE_SECONDS / 2.0);
        assert!((halfway - 0.5).abs() < 1e-6);
        // (Not exactly 0 at the end: 1.4 - 1.0 isn't exactly 0.4 in floating point.)
        assert!(popup_opacity(POPUP_SECONDS + POPUP_FADE_SECONDS) < 1e-6);
        assert_eq!(popup_opacity(100.0), 0.0);
    }

    #[test]
    fn speed_label_shows_a_decimal_only_when_slow() {
        assert_eq!(speed_label(20.0), "Fly speed: 20 studs/s");
        assert_eq!(speed_label(57.6), "Fly speed: 58 studs/s");
        assert_eq!(speed_label(2.4), "Fly speed: 2.4 studs/s");
    }

    #[test]
    fn viewport_corners_map_to_screen_corners_and_back() {
        let rect = egui::Rect::from_min_size(egui::pos2(100.0, 50.0), egui::vec2(800.0, 600.0));
        assert_eq!(to_screen_point(rect, rect.left_top()), Vec2::new(-1.0, 1.0));
        assert_eq!(
            to_screen_point(rect, rect.right_bottom()),
            Vec2::new(1.0, -1.0)
        );
        assert_eq!(to_screen_point(rect, rect.center()), Vec2::ZERO);
        let point = egui::pos2(250.0, 470.0);
        assert_eq!(to_ui_point(rect, to_screen_point(rect, point)), point);
    }

    #[test]
    fn clicking_the_middle_of_the_view_picks_the_part_the_camera_faces() {
        let mut world = World::new();
        let part = world.create(axcenity_core::ClassName::Part).unwrap();
        world.set_parent(part, Some(world.workspace())).unwrap();
        // The start camera looks at the origin, where a new part sits.
        let viewport = Viewport::new();
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0));
        let view = viewport.camera_view(rect);
        let ray = view.ray(to_screen_point(rect, rect.center()));
        assert_eq!(axcenity_scene::pick(&world, ray), Some(part));
        let corner = view.ray(to_screen_point(rect, rect.left_top()));
        assert_eq!(axcenity_scene::pick(&world, corner), None);
    }

    #[test]
    fn opening_another_project_keeps_the_flying_speed() {
        let mut viewport = Viewport::new();
        viewport.camera.change_speed(3.0);
        let speed = viewport.camera.speed();
        viewport.camera.position = Vec3::splat(100.0);
        viewport.reset_camera();
        assert_eq!(viewport.camera.position, START_CAMERA_POSITION);
        assert_eq!(viewport.camera.speed(), speed);
    }
}
