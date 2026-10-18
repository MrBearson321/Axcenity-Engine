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
//! With the Move, Scale or Rotate tool, it also gets handles to drag
//! (see `gizmo.rs`). With any tool, left-dragging a part by its body moves
//! it onto the surface under the mouse (see `placing.rs`).
//!
//! UI is visible as an in-place preview both while editing and during Play.
//! It's drawn over the 3D view, so a click selects the UI element under the
//! mouse before any part. A selected UI element gets an outline and handles
//! to move and resize it (see `ui_handles.rs`).
//!
//! During Play there's no tool: the camera and selecting still work, but
//! nothing can be dragged.

use axcenity_assets::Assets;
use axcenity_core::{InstanceId, Value, World};
use axcenity_render::TextureRenderer;
use axcenity_render::glam::Vec3;
use axcenity_render::wgpu;
use axcenity_scene::{FlyCamera, MoveKeys};
use axcenity_ui::{Stylesheet, UiLayout};
use eframe::egui;
use eframe::egui_wgpu::RenderState;

use crate::gizmo::{self, Drag, Handle, Pose, Snap, Tool};
use crate::placing::PlaceDrag;
use crate::screen::Screen;
use crate::ui_handles::{Grip, Handles, UiDrag, UiDragEdit};

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

/// A left click in the viewport, with the UI element or part under the
/// mouse (`None` for empty space, which clears the selection). Starting to
/// drag something by its body counts as clicking it, so it gets selected.
pub struct Clicked {
    pub part: Option<InstanceId>,
}

/// What a drag in the viewport does to a part, which names its undo step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragKind {
    Move,
    Resize,
    Rotate,
}

impl DragKind {
    pub fn of(handle: Handle) -> DragKind {
        match handle {
            Handle::Move(_) => DragKind::Move,
            Handle::Scale(..) => DragKind::Resize,
            Handle::Rotate(_) => DragKind::Rotate,
        }
    }
}

/// New property values for a part from dragging it or one of its handles.
#[derive(Clone, Debug, PartialEq)]
pub struct DragEdit {
    pub part: InstanceId,
    pub kind: DragKind,
    pub values: Vec<(&'static str, Value)>,
}

/// What happened in the viewport this frame.
#[derive(Default)]
pub struct ViewportOutput {
    pub clicked: Option<Clicked>,
    pub drag: Option<DragEdit>,
    /// A UI element being moved or resized.
    pub ui_drag: Option<UiDragEdit>,
    /// True on the frame a drag is let go, which ends that drag's undo step.
    pub drag_released: bool,
}

pub struct ViewportScene<'a> {
    pub world: &'a World,
    pub assets: &'a Assets,
    pub selected: Option<InstanceId>,
    pub tool: Option<Tool>,
    pub snap: Snap,
    pub stylesheets: &'a [Stylesheet],
}

/// What the left button holds, from going down on it until it's let go.
#[derive(Clone, Debug)]
enum Dragging {
    /// A handle of the selected part.
    Handle(Drag),
    /// A part, by its body.
    Body(PlaceDrag),
    /// A UI element, by its body or a handle.
    Ui(Box<UiDrag>),
}

/// The UI as laid out this frame, and the selected element's handles.
struct UiScene<'a> {
    layout: &'a UiLayout,
    handles: Option<&'a Handles>,
    /// Where the viewport's top left is, in points, and the points-to-pixels
    /// scale, to turn the mouse position into the layout's pixels.
    origin: egui::Pos2,
    pixels_per_point: f32,
}

impl UiScene<'_> {
    /// The topmost UI element at `pointer` (in points).
    fn hit(&self, pointer: egui::Pos2) -> Option<InstanceId> {
        let pixels = (pointer - self.origin) * self.pixels_per_point;
        self.layout.hit(pixels.x, pixels.y)
    }
}

/// What dragging did this frame, before anything is drawn.
#[derive(Default)]
struct DragInput {
    /// The handles to draw, and which one to highlight.
    shapes: Vec<gizmo::Shape>,
    highlighted: Option<Handle>,
    /// The selected UI element's handle under the mouse or being dragged.
    ui_highlighted: Option<Grip>,
    /// True if the left button is (or just was) dragging something, so the
    /// click that ends it doesn't also select something.
    in_use: bool,
    /// A part grabbed by its body this frame, to select.
    grabbed: Option<InstanceId>,
    edit: Option<DragEdit>,
    ui_edit: Option<UiDragEdit>,
    released: bool,
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
    /// What's being dragged, if anything.
    drag: Option<Dragging>,
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
            drag: None,
        }
    }

    /// Puts the camera back where it starts, for a newly opened project.
    /// The flying speed is a preference, not part of the project, so it stays.
    pub fn reset_camera(&mut self) {
        let speed = self.camera.speed();
        self.camera = start_camera();
        self.camera.set_speed(speed);
        self.drag = None;
    }

    /// The camera's focus point, where new parts are inserted.
    pub fn focus(&self) -> Vec3 {
        self.camera.focus()
    }

    /// Where the camera is and which way it looks (degrees, like a Part's
    /// Orientation), for `workspace.CurrentCamera` during Play.
    pub fn camera_view(&self) -> (axcenity_core::Vector3, axcenity_core::Vector3) {
        self.camera.view()
    }

    /// Moves the camera where a LocalScript put it during Play.
    pub fn set_camera_view(
        &mut self,
        position: axcenity_core::Vector3,
        orientation: axcenity_core::Vector3,
    ) {
        self.camera.set_view(position, orientation);
    }

    /// Points the camera at `target` from `position`.
    #[cfg(test)]
    pub fn look_at(&mut self, position: Vec3, target: Vec3) {
        self.camera = FlyCamera::looking_at(position, target);
    }

    /// True while a part or a handle is being dragged.
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Fills `ui` with the viewport: moves the camera from this frame's
    /// input, then draws `world` from there with a box around `selected`,
    /// and `tool`'s handles on it. Returns what a click picked and what a
    /// drag changed; the app applies those to the world. With no `tool`
    /// (during Play), nothing can be dragged.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        render_state: &RenderState,
        scene: ViewportScene<'_>,
    ) -> ViewportOutput {
        let ViewportScene {
            world,
            assets,
            selected,
            tool,
            snap,
            stylesheets,
        } = scene;
        let size = ui.available_size();
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        self.handle_input(ui, &response, rect.height());
        let screen = self.screen(rect);

        // The texture is sized in physical pixels, so it stays sharp on
        // high-DPI screens where one point is several pixels.
        let pixels_per_point = ui.ctx().pixels_per_point();
        let pixels = size * pixels_per_point;
        let (width, height) = (pixels.x.round() as u32, pixels.y.round() as u32);
        let ui_layout = axcenity_ui::layout_ui(world, width, height, stylesheets);
        let handles = selected
            .and_then(|id| ui_layout.element(id))
            .map(|&element| Handles::new(element, world, stylesheets, rect.min, pixels_per_point));
        let ui_scene = UiScene {
            layout: &ui_layout,
            handles: handles.as_ref(),
            origin: rect.min,
            pixels_per_point,
        };

        let drag = self.drag_input(
            ui,
            &response,
            &screen,
            world,
            selected,
            tool,
            snap,
            &ui_scene,
            stylesheets,
        );
        let clicked = match drag.grabbed {
            Some(part) => Some(Clicked { part: Some(part) }),
            None if drag.in_use => None,
            None => self.clicked_part(ui, &response, &screen, world, &ui_scene),
        };
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

        let mut draw_list =
            axcenity_scene::build_draw_list(world, assets, self.camera.to_render_camera());
        draw_list.ui = ui_layout.draw_items;
        if let Err(error) = target.renderer.render(&draw_list) {
            log::error!("could not render the viewport: {error:#}");
        }
        let whole_texture = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        ui.painter()
            .image(target.texture_id, rect, whole_texture, egui::Color32::WHITE);
        if let Some(selected) = selected {
            paint_selection_box(ui, rect, &screen, world, selected);
        }
        gizmo::paint(&ui.painter_at(rect), &drag.shapes, drag.highlighted);
        // Handles only show with a tool, so not during Play, but the
        // outline always does.
        if let Some(handles) = &handles {
            let painter = ui.painter_at(rect);
            match tool {
                Some(_) => handles.paint(&painter, SELECTION_COLOR, drag.ui_highlighted),
                None => {
                    painter.rect_stroke(
                        handles.rect,
                        0.0,
                        egui::Stroke::new(SELECTION_WIDTH, SELECTION_COLOR),
                        egui::StrokeKind::Middle,
                    );
                }
            }
        }
        self.paint_speed_popup(ui, rect);
        ViewportOutput {
            clicked,
            drag: drag.edit,
            ui_drag: drag.ui_edit,
            drag_released: drag.released,
        }
    }

    /// The camera as it draws into `rect`, for turning between screen and
    /// world positions.
    fn screen(&self, rect: egui::Rect) -> Screen {
        Screen::new(self.camera.to_render_camera(), rect)
    }

    /// Starts, continues or ends dragging a handle of the selected part or
    /// UI element, or any part or UI element by its body.
    #[allow(clippy::too_many_arguments)] // Each is a different thing dragging needs.
    fn drag_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        screen: &Screen,
        world: &World,
        selected: Option<InstanceId>,
        tool: Option<Tool>,
        snap: Snap,
        ui_scene: &UiScene,
        stylesheets: &[Stylesheet],
    ) -> DragInput {
        let Some(tool) = tool else {
            // Nothing to drag. A drag can't be under way either: Play
            // doesn't start during one.
            self.drag = None;
            return DragInput::default();
        };
        let selected = selected.and_then(|id| Some((id, Pose::of(world.get(id)?)?)));
        let shapes = match selected {
            Some((_, pose)) => gizmo::shapes(tool, pose, screen),
            None => Vec::new(),
        };
        let (pressed, down, pointer, press_origin, alt) = ui.input(|input| {
            (
                input.pointer.primary_pressed(),
                input.pointer.primary_down(),
                input.pointer.latest_pos(),
                input.pointer.press_origin(),
                input.modifiers.alt,
            )
        });
        let over = response.contains_pointer() && self.drag.is_none();
        let hovered = match pointer {
            Some(pointer) if over => gizmo::hovered(&shapes, pointer),
            _ => None,
        };
        // The UI element's handle under the mouse (not its body: that's
        // only grabbed by dragging, so a click can select what's inside it).
        let ui_hovered = match (pointer, ui_scene.handles) {
            (Some(pointer), Some(handles)) if over => handles.grip_at(pointer),
            _ => None,
        };
        let ui_handle = ui_hovered.filter(|&grip| grip != Grip::BODY);
        let mut input = DragInput::default();

        // Alt+left-drag orbits the camera, so it never grabs anything.
        // A handle is grabbed as soon as the button goes down on it.
        if pressed
            && !alt
            && let (Some(grip), Some(pointer), Some(handles)) =
                (ui_handle, pointer, ui_scene.handles)
        {
            self.drag = UiDrag::start(handles, grip, pointer, world)
                .map(|drag| Dragging::Ui(Box::new(drag)));
        } else if pressed
            && !alt
            && let (Some(handle), Some(pointer), Some((part, pose))) = (hovered, pointer, selected)
        {
            self.drag = Drag::start(handle, part, pose, screen.ray(pointer)).map(Dragging::Handle);
        }
        // A UI element or part's body only once the mouse has moved far
        // enough to be a drag, so a plain click still just selects. It's
        // grabbed where the button went down. UI is on top, so it goes first.
        let body_drag =
            self.drag.is_none() && !alt && response.drag_started_by(egui::PointerButton::Primary);
        if body_drag
            && let Some(origin) = press_origin
            && let Some(id) = ui_scene.hit(origin)
        {
            // Selected even when it can't move (in a flex container).
            input.grabbed = Some(id);
            if let Some(&element) = ui_scene.layout.element(id) {
                let handles = Handles::new(
                    element,
                    world,
                    stylesheets,
                    ui_scene.origin,
                    ui_scene.pixels_per_point,
                );
                self.drag = UiDrag::start(&handles, Grip::BODY, origin, world)
                    .map(|drag| Dragging::Ui(Box::new(drag)));
            }
        } else if body_drag
            && let Some(origin) = press_origin
            && let Some(hit) = axcenity_scene::pick_hit(world, screen.ray(origin), None)
            && let Some(place) =
                PlaceDrag::start(world, hit.part, screen.ray(origin).at(hit.distance))
        {
            self.drag = Some(Dragging::Body(place));
            input.grabbed = Some(hit.part);
        }

        input.highlighted = match &self.drag {
            Some(Dragging::Handle(drag)) => Some(drag.handle),
            _ => hovered,
        };
        input.ui_highlighted = match &self.drag {
            Some(Dragging::Ui(drag)) => Some(drag.grip),
            _ => ui_handle,
        };
        input.in_use = self.drag.is_some();
        match (&self.drag, ui_scene.handles) {
            (Some(Dragging::Ui(drag)), Some(handles)) => {
                ui.ctx().set_cursor_icon(handles.cursor(drag.grip));
            }
            (Some(_), _) => ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing),
            (None, Some(handles)) if ui_hovered.is_some() => {
                ui.ctx()
                    .set_cursor_icon(handles.cursor(ui_hovered.expect("checked")));
            }
            (None, _) if hovered.is_some() => ui.ctx().set_cursor_icon(egui::CursorIcon::Grab),
            (None, _) => {}
        }
        if let Some(dragging) = &mut self.drag {
            if !down {
                self.drag = None;
                input.released = true;
            } else if let Some(pointer) = pointer {
                let ray = screen.ray(pointer);
                input.edit = match dragging {
                    Dragging::Handle(drag) => drag.update(ray, &snap).map(|values| DragEdit {
                        part: drag.part,
                        kind: DragKind::of(drag.handle),
                        values,
                    }),
                    Dragging::Body(place) => {
                        place.update(world, ray, &snap).map(|position| DragEdit {
                            part: place.part,
                            kind: DragKind::Move,
                            values: vec![("Position", position)],
                        })
                    }
                    Dragging::Ui(drag) => {
                        input.ui_edit = Some(drag.update(pointer, ui_scene.pixels_per_point));
                        None
                    }
                };
            }
        }
        input.shapes = shapes;
        input
    }

    /// If this frame has a plain left click (not the end of a drag, and not
    /// Alt for orbiting), the UI element or part under the mouse.
    fn clicked_part(
        &self,
        ui: &egui::Ui,
        response: &egui::Response,
        screen: &Screen,
        world: &World,
        ui_scene: &UiScene,
    ) -> Option<Clicked> {
        if !response.clicked() || ui.input(|input| input.modifiers.alt) {
            return None;
        }
        let pointer = response.interact_pointer_pos()?;
        Some(Clicked {
            part: ui_scene
                .hit(pointer)
                .or_else(|| axcenity_scene::pick(world, screen.ray(pointer))),
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
    screen: &Screen,
    world: &World,
    selected: InstanceId,
) {
    let Some(edges) = axcenity_scene::part_box_edges(world, selected) else {
        return;
    };
    let painter = ui.painter_at(rect);
    let stroke = egui::Stroke::new(SELECTION_WIDTH, SELECTION_COLOR);
    for (a, b) in edges {
        if let Some(line) = screen.line(a, b) {
            painter.line_segment(line, stroke);
        }
    }
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
    fn clicking_the_middle_of_the_view_picks_the_part_the_camera_faces() {
        let mut world = World::new();
        let part = world.create(axcenity_core::ClassName::Part).unwrap();
        world.set_parent(part, Some(world.workspace())).unwrap();
        // The start camera looks at the origin, where a new part sits.
        let viewport = Viewport::new();
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0));
        let screen = viewport.screen(rect);
        assert_eq!(
            axcenity_scene::pick(&world, screen.ray(rect.center())),
            Some(part)
        );
        let corner = screen.ray(rect.left_top());
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
