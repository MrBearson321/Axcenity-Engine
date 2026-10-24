//! The 3D viewport: the world drawn by the same renderer as the player, into
//! a texture that egui shows, with a Studio-style camera.
//!
//! Controls: right-drag to look around, WASD to fly, E/Q for up and down,
//! Shift to go slowly, the mouse wheel to zoom toward the focus point,
//! middle-drag to pan, and Alt+left-drag to orbit around the focus point.
//! Shift+wheel, or the wheel while right-dragging, changes the flying speed,
//! and a pop-up in the middle of the viewport shows the new speed.
//!
//! Left-clicking a part selects it, and the selected part gets a blue box:
//! the edges of its sides that face the camera, where nothing hides them.
//! PointLights and SpotLights have no shape, so while editing each shows as
//! an icon where it shines from, as in Unreal; click one to select it, and
//! a selected light shows how far it reaches (a SpotLight, its cone).
//! With the Move, Scale or Rotate tool, it also gets handles to drag
//! (see `gizmo.rs`). With any tool, left-dragging a part by its body moves
//! it onto the surface under the mouse (see `placing.rs`).
//!
//! UI is visible as an in-place preview both while editing and during Play.
//! It's drawn over the 3D view, so a click selects the UI element under the
//! mouse before any part. A selected UI element gets an outline and handles
//! to move and resize it, and a drag snaps to other UI edges and centers
//! (see `ui_handles.rs`).
//!
//! With a Terrain Studio brush (see `terrain_studio.rs`), the left button
//! sculpts or paints the terrain instead: the brush shows on the ground
//! under the mouse, and nothing is selected or dragged. The camera works
//! as always (right-drag, Alt+left-drag, the wheel, WASD).
//!
//! During Play there's no tool: the camera and selecting still work, but
//! nothing can be dragged.

use std::time::Instant;

use axcenity_assets::Assets;
use axcenity_core::{InstanceId, Value, World};
use axcenity_render::TextureRenderer;
use axcenity_render::glam::Vec3;
use axcenity_render::wgpu;
use axcenity_scene::{FlyCamera, LightMarker, MoveKeys, Particles, TerrainCache};
use axcenity_ui::{Stylesheet, UiLayout, UiRect};
use eframe::egui;
use eframe::egui_wgpu::RenderState;

use crate::gizmo::{self, Drag, Handle, Modes, PartValues, Pose, Snap, Tool};
use crate::placing::PlaceDrag;
use crate::profiler::RenderCost;
use crate::screen::Screen;
use crate::theme::{self, icons};
use crate::ui_handles::{self, Grip, Handles, UiDrag, UiDragEdit};

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
/// The outline of parts about to be placed.
const PREVIEW_COLOR: egui::Color32 = egui::Color32::from_rgb(255, 196, 64);
/// A light's icon, in points, the same size however far away the light is.
const LIGHT_ICON_RADIUS: f32 = 13.0;

/// A left click in the viewport, with the UI element or part under the
/// mouse (`None` for empty space, which clears the selection). Starting to
/// drag something by its body counts as clicking it, so it gets selected.
pub struct Clicked {
    pub part: Option<InstanceId>,
    /// Ctrl, Shift and the other modifier keys held for this click.
    pub modifiers: egui::Modifiers,
    /// True when a selected part was grabbed to drag the whole selection,
    /// which keeps the rest of the selection.
    pub keep_group: bool,
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

/// New property values for the parts being dragged (by a body or a handle).
#[derive(Clone, Debug, PartialEq)]
pub struct DragEdit {
    pub kind: DragKind,
    /// Each part changed, with its new values. The last selected part is last.
    pub parts: PartValues,
}

/// What happened in the viewport this frame.
#[derive(Default)]
pub struct ViewportOutput {
    pub clicked: Option<Clicked>,
    /// A UI button clicked during Play, delivered to LocalScripts next frame.
    pub activated: Option<InstanceId>,
    /// A TextBox clicked during Play. The app gives it keyboard focus.
    pub text_box: Option<InstanceId>,
    /// The viewport was clicked during Play, including a click on empty space.
    pub viewport_clicked: bool,
    /// Page and billboard problems from this frame's layout.
    pub ui_warnings: Vec<String>,
    pub drag: Option<DragEdit>,
    /// A UI element being moved or resized.
    pub ui_drag: Option<UiDragEdit>,
    /// True on the frame a drag is let go, which ends that drag's undo step.
    pub drag_released: bool,
    /// Where something dragged from another panel (an asset) would land,
    /// while it's over the viewport.
    pub drop: Option<DropTarget>,
    /// The mouse over the terrain being brushed, while a Terrain Studio
    /// brush is on.
    pub terrain: Option<TerrainPointer>,
    /// What drawing the scene took, for the Profiler.
    pub render: Option<RenderCost>,
}

/// A Terrain Studio brush, as the viewport shows it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainBrush {
    pub terrain: InstanceId,
    /// Studs from the middle to the rim.
    pub radius: f32,
    /// 0 to 1: how much of the radius fades.
    pub falloff: f32,
    pub square: bool,
    pub color: egui::Color32,
}

/// The mouse and left button over the terrain being brushed this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TerrainPointer {
    /// The ground under the mouse, if it's over the terrain.
    pub point: Option<Vec3>,
    /// The left button went down this frame (starting a stroke).
    pub pressed: bool,
    /// A stroke is under way: the button went down over the viewport and
    /// hasn't been let go.
    pub down: bool,
    /// The stroke ended this frame.
    pub released: bool,
    pub modifiers: egui::Modifiers,
    /// Seconds since the last frame.
    pub seconds: f32,
}

/// What's under the mouse while an asset is dragged over the viewport.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DropTarget {
    /// The point on the surface under the mouse: a part, or the ground
    /// (the plane at Y = 0), or the camera's focus point if neither.
    pub point: Vec3,
    /// The way that surface faces, one stud long.
    pub normal: Vec3,
    /// The part under the mouse.
    pub part: Option<InstanceId>,
    /// The UI element under the mouse, which is in front of any part.
    pub ui: Option<InstanceId>,
    /// True on the frame the mouse button is let go: drop it there.
    pub released: bool,
}

pub struct ViewportScene<'a> {
    pub world: &'a World,
    pub assets: &'a Assets,
    pub selected: Option<InstanceId>,
    /// Every selected Instance. `selected` is the last of these, and the one
    /// that gets handles. The others are outlined too.
    pub selection: &'a [InstanceId],
    pub tool: Option<Tool>,
    pub snap: Snap,
    /// The Move, Resize and Rotate tools' options.
    pub modes: Modes,
    /// Outlines of parts about to be placed (the Angular tool's copies).
    pub preview: &'a [Pose],
    /// Particle bursts asked for this frame (`Emit` from scripts, or the
    /// Emission tool's Burst): each emitter and how many.
    pub emits: &'a [(InstanceId, u32)],
    pub stylesheets: &'a [Stylesheet],
    /// The TextBox being typed in during Play, so it gets a focus ring.
    pub focused_text: Option<InstanceId>,
    /// Show every light's reach, not just the selected ones' (View > Light Cones).
    pub light_cones: bool,
    /// Animated MeshParts' poses during Play; `None` draws every model still.
    pub poses: Option<&'a axcenity_scene::Poses>,
    /// A Terrain Studio brush, which takes the left button while it's on.
    pub terrain_brush: Option<TerrainBrush>,
}

/// What the left button holds, from going down on it until it's let go.
#[derive(Clone, Debug)]
enum Dragging {
    /// A handle of the selected part.
    Handle(Drag),
    /// A part, by its body, and the other selected parts that move with
    /// it, each with where it was at the start.
    Body(PlaceDrag, Vec<(InstanceId, Vec3)>),
    /// A UI element, by its body or a handle.
    Ui(Box<UiDrag>),
}

/// What's selected, for dragging.
#[derive(Clone, Copy)]
struct DragSelection<'a> {
    selected: Option<InstanceId>,
    selection: &'a [InstanceId],
    modes: Modes,
}

/// The parts the transform tools act on: each selected part, and the parts
/// inside each selected Model or Folder, once each, with the last selected
/// one (or the last part inside it) last.
pub fn selected_parts(
    world: &World,
    selection: &[InstanceId],
    selected: Option<InstanceId>,
) -> Vec<(InstanceId, Pose)> {
    let mut ids: Vec<InstanceId> = selection.to_vec();
    if let Some(selected) = selected {
        ids.retain(|id| *id != selected);
        ids.push(selected);
    }
    let mut parts: Vec<(InstanceId, Pose)> = Vec::new();
    for id in ids {
        let inside = std::iter::once(id).chain(world.descendants(id));
        for part in inside {
            let Some(pose) = world.get(part).and_then(Pose::of) else {
                continue;
            };
            // Seen again: it goes to the end, so the last selected stays last.
            parts.retain(|(seen, _)| *seen != part);
            parts.push((part, pose));
        }
    }
    parts
}

/// The UI as laid out this frame, and the selected element's handles.
struct UiScene<'a> {
    layout: &'a UiLayout,
    handles: Option<&'a Handles>,
    /// Where the viewport's top left is, in points, and the points-to-pixels
    /// scale, to turn the mouse position into the layout's pixels.
    origin: egui::Pos2,
    pixels_per_point: f32,
    /// The screen the UI was laid out into, in physical pixels. Drags snap
    /// to its edges and center.
    viewport: UiRect,
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
    /// The ParticleEmitters' particles, shown while editing and during Play.
    particles: Particles,
    /// True from the left button going down with a terrain brush until
    /// it's let go.
    terrain_stroke: bool,
    /// Terrains' chunk meshes and weights, kept between frames, and the
    /// Terrain Studio's previews and views.
    pub terrain: TerrainCache,
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
            particles: Particles::new(),
            terrain_stroke: false,
            terrain: TerrainCache::new(),
        }
    }

    /// Takes every particle away, for a new world (Play, Stop, another project).
    pub fn clear_particles(&mut self) {
        self.particles.clear();
    }

    /// Puts the camera back where it starts, for a newly opened project.
    /// The flying speed is a preference, not part of the project, so it stays.
    pub fn reset_camera(&mut self) {
        let speed = self.camera.speed();
        self.camera = start_camera();
        self.camera.set_speed(speed);
        self.drag = None;
        self.particles.clear();
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

    /// True while a part or a handle is being dragged, or a terrain stroke
    /// is under way.
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some() || self.terrain_stroke
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
            selection,
            tool,
            snap,
            modes,
            preview,
            emits,
            stylesheets,
            focused_text,
            light_cones,
            poses,
            terrain_brush,
        } = scene;
        // A brush takes the left button: no handles, dragging or picking.
        let brush = terrain_brush.filter(|_| tool.is_some());
        let drag_tool = if brush.is_some() { None } else { tool };
        let size = ui.available_size();
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        self.handle_input(ui, &response, rect.height());
        let screen = self.screen(rect);

        // The texture is sized in physical pixels, so it stays sharp on
        // high-DPI screens where one point is several pixels.
        let pixels_per_point = ui.ctx().pixels_per_point();
        let pixels = size * pixels_per_point;
        let (width, height) = (pixels.x.round() as u32, pixels.y.round() as u32);
        let pointer = ui.input(|input| input.pointer.latest_pos());
        let pointer_pixels = pointer
            .filter(|_| response.contains_pointer())
            .map(|pointer| (pointer - rect.min) * pixels_per_point)
            .map(|position| [position.x, position.y]);
        let camera = self.camera.to_render_camera();
        let base_layout = axcenity_ui::layout_ui_viewed(
            world,
            width,
            height,
            stylesheets,
            focused_text,
            Some(&camera),
        );
        let held_button = ui
            .input(|input| input.pointer.primary_down())
            .then(|| pointer_pixels.and_then(|[x, y]| base_layout.hit_button(x, y)))
            .flatten();
        let ui_layout = axcenity_ui::layout_ui_interactive(
            world,
            width,
            height,
            stylesheets,
            pointer_pixels,
            held_button,
            focused_text,
            Some(&camera),
        );
        let handles = selected
            .and_then(|id| ui_layout.element(id))
            .map(|&element| Handles::new(element, world, stylesheets, rect.min, pixels_per_point));
        let ui_scene = UiScene {
            layout: &ui_layout,
            handles: handles.as_ref(),
            origin: rect.min,
            pixels_per_point,
            viewport: UiRect {
                x: 0.0,
                y: 0.0,
                width: width as f32,
                height: height as f32,
            },
        };

        // Lights only show as icons while editing (there's a tool), as in
        // Unreal's editor; the game itself doesn't show them.
        let markers = if tool.is_some() {
            axcenity_scene::light_markers(world)
        } else {
            Vec::new()
        };
        // Only lights whose icon is on screen can be clicked, but a light
        // just off screen can still shine into view.
        let lights: Vec<(LightMarker, egui::Pos2)> = markers
            .iter()
            .filter_map(|marker| {
                let at = screen.point(marker.position)?;
                rect.contains(at).then(|| (marker.clone(), at))
            })
            .collect();
        let drag = self.drag_input(
            ui,
            &response,
            &screen,
            world,
            DragSelection {
                selected,
                selection,
                modes,
            },
            drag_tool,
            snap,
            &ui_scene,
            stylesheets,
        );
        let modifiers = ui.input(|input| input.modifiers);
        let terrain = brush.map(|brush| self.terrain_input(ui, &response, &screen, world, brush));
        let clicked = match drag.grabbed {
            Some(part) => Some(Clicked {
                part: Some(part),
                modifiers,
                keep_group: selection.contains(&part),
            }),
            None if drag.in_use || brush.is_some() => None,
            None => self.clicked_part(ui, &response, &screen, world, &ui_scene, &lights),
        };
        let playing = tool.is_none() && response.clicked_by(egui::PointerButton::Primary);
        let clicked_ui = |hit: fn(&axcenity_ui::UiLayout, f32, f32) -> Option<InstanceId>| {
            playing.then(|| {
                response.interact_pointer_pos().and_then(|pointer| {
                    let position = (pointer - rect.min) * pixels_per_point;
                    hit(&ui_layout, position.x, position.y)
                })
            })
        };
        let activated = clicked_ui(axcenity_ui::UiLayout::hit_button).flatten();
        let text_box = clicked_ui(axcenity_ui::UiLayout::hit_text_box).flatten();
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

        for &(emitter, count) in emits {
            self.particles.emit(world, emitter, count);
        }
        let seconds = ui.input(|input| input.unstable_dt).min(MAX_FRAME_SECONDS);
        self.particles.update(world, seconds);
        // Particles keep moving without input, so keep drawing frames.
        if self.particles.is_active(world) {
            ui.ctx().request_repaint();
        }
        let still = axcenity_scene::Poses::new();
        let poses = poses.unwrap_or(&still);
        let render_started = Instant::now();
        let mut draw_list =
            axcenity_scene::build_draw_list_posed(world, assets, camera, poses, &mut self.terrain);
        draw_list.sprites = self.particles.sprites(world, assets);
        draw_list.ui = ui_layout.draw_items.clone();
        axcenity_scene::attach_ui_textures(&mut draw_list.ui, assets);
        if let Err(error) = target.renderer.render(&draw_list) {
            log::error!("could not render the viewport: {error:#}");
        }
        let render = RenderCost {
            cpu: render_started.elapsed(),
            items: draw_list.items.len(),
            lights: draw_list.lights.len(),
            stats: target.renderer.stats(),
        };
        let whole_texture = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        ui.painter()
            .image(target.texture_id, rect, whole_texture, egui::Color32::WHITE);
        let eye = self.camera.to_render_camera().position;
        for &id in selection {
            if Some(id) != selected {
                if let Some(element) = ui_layout.element(id) {
                    let outline = egui::Rect::from_min_size(
                        rect.min + egui::vec2(element.rect.x, element.rect.y) / pixels_per_point,
                        egui::vec2(element.rect.width, element.rect.height) / pixels_per_point,
                    );
                    ui.painter_at(rect).rect_stroke(
                        outline,
                        0.0,
                        egui::Stroke::new(SELECTION_WIDTH, SELECTION_COLOR),
                        egui::StrokeKind::Middle,
                    );
                }
                paint_selection_box(ui, rect, &screen, world, id, eye);
            }
        }
        if let Some(selected) = selected {
            paint_selection_box(ui, rect, &screen, world, selected, eye);
        }
        let painter = ui.painter_at(rect);
        for marker in &markers {
            let chosen = selection.contains(&marker.id) || selection.contains(&marker.part);
            if chosen || light_cones {
                paint_light_reach(&painter, &screen, world, marker, chosen);
            }
        }
        paint_lights(ui, rect, &lights, selected, pointer);
        let painter = ui.painter_at(rect);
        for &pose in preview {
            for (a, b) in crate::angular::box_edges(pose) {
                if let Some(line) = screen.line(a, b) {
                    painter.line_segment(line, egui::Stroke::new(1.5_f32, PREVIEW_COLOR));
                }
            }
        }
        gizmo::paint(&ui.painter_at(rect), &drag.shapes, drag.highlighted);
        if let (Some(brush), Some(point)) = (brush, terrain.and_then(|t| t.point)) {
            paint_terrain_brush(&ui.painter_at(rect), &screen, world, brush, point);
        }
        // Handles only show with a tool, so not during Play, but the
        // outline always does.
        if let Some(handles) = &handles {
            let painter = ui.painter_at(rect);
            match tool {
                Some(_) => {
                    handles.paint(&painter, SELECTION_COLOR, drag.ui_highlighted);
                    let parent = world
                        .get(handles.element.id)
                        .and_then(|instance| instance.parent())
                        .and_then(|id| ui_layout.element(id))
                        .map(|element| {
                            egui::Rect::from_min_size(
                                rect.min
                                    + egui::vec2(element.rect.x, element.rect.y) / pixels_per_point,
                                egui::vec2(element.rect.width, element.rect.height)
                                    / pixels_per_point,
                            )
                        });
                    handles.paint_measures(&painter, rect, parent, pixels_per_point);
                }
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
        if let Some(edit) = &drag.ui_edit {
            ui_handles::paint_guides(
                &edit.guides,
                &ui.painter_at(rect),
                rect.min,
                pixels_per_point,
            );
        }
        self.paint_speed_popup(ui, rect);
        let drop = self.drop_target(ui, rect, &screen, world, &ui_scene);
        if let Some(target) = &drop
            && let Some(at) = screen.point(target.point)
        {
            let painter = ui.painter_at(rect);
            painter.circle_stroke(at, 9.0, egui::Stroke::new(SELECTION_WIDTH, SELECTION_COLOR));
            painter.circle_filled(at, 3.0, SELECTION_COLOR);
        }
        ViewportOutput {
            clicked,
            activated,
            text_box,
            viewport_clicked: playing,
            ui_warnings: ui_layout.warnings,
            drag: drag.edit,
            ui_drag: drag.ui_edit,
            drag_released: drag.released,
            drop,
            terrain,
            render: Some(render),
        }
    }

    /// The mouse and left button over the terrain `brush` works on.
    fn terrain_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        screen: &Screen,
        world: &World,
        brush: TerrainBrush,
    ) -> TerrainPointer {
        let (pressed, down, pointer, modifiers, seconds) = ui.input(|input| {
            (
                input.pointer.primary_pressed(),
                input.pointer.primary_down(),
                input.pointer.latest_pos(),
                input.modifiers,
                input.stable_dt.min(MAX_FRAME_SECONDS),
            )
        });
        let over = pointer.filter(|_| response.contains_pointer());
        let point =
            over.and_then(|pointer| ground_point(world, brush.terrain, screen.ray(pointer)));
        // Alt+left-drag orbits the camera instead.
        let pressed = pressed && over.is_some() && !modifiers.alt;
        if pressed {
            self.terrain_stroke = true;
        }
        let released = self.terrain_stroke && !down;
        if released {
            self.terrain_stroke = false;
        }
        if over.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        // Held still, a stroke keeps working (raising, smoothing), so keep
        // drawing frames.
        if self.terrain_stroke {
            ui.ctx().request_repaint();
        }
        TerrainPointer {
            point,
            pressed,
            down: self.terrain_stroke,
            released,
            modifiers,
            seconds,
        }
    }

    /// Where an asset dragged from another panel would land, if one is
    /// being dragged and the mouse is over the viewport.
    fn drop_target(
        &self,
        ui: &egui::Ui,
        rect: egui::Rect,
        screen: &Screen,
        world: &World,
        ui_scene: &UiScene,
    ) -> Option<DropTarget> {
        if !egui::DragAndDrop::has_any_payload(ui.ctx()) {
            return None;
        }
        let (pointer, released) =
            ui.input(|input| (input.pointer.latest_pos(), input.pointer.any_released()));
        let pointer = pointer.filter(|pointer| rect.contains(*pointer))?;
        let ray = screen.ray(pointer);
        let hit = axcenity_scene::pick_hit(world, ray, None);
        let (point, normal) = match &hit {
            Some(hit) => (ray.at(hit.distance), hit.normal),
            // The ground, if the mouse points down at it, near enough.
            None if ray.direction.y < -0.01 => {
                let distance = (-ray.origin.y / ray.direction.y).min(1000.0);
                (ray.at(distance), Vec3::Y)
            }
            None => (self.camera.focus(), Vec3::Y),
        };
        Some(DropTarget {
            point,
            normal,
            part: hit.map(|hit| hit.part),
            ui: ui_scene.hit(pointer),
            released,
        })
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
        chosen: DragSelection,
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
        let DragSelection {
            selected,
            selection,
            modes,
        } = chosen;
        // The handles act on every selected part (and the parts inside
        // selected Models), with the last selected one last.
        let parts = selected_parts(world, selection, selected);
        let poses: Vec<Pose> = parts.iter().map(|(_, pose)| *pose).collect();
        let frame = if tool.has_handles() {
            gizmo::handle_frame(tool, modes, &poses)
        } else {
            None
        };
        let shapes = match frame {
            Some(frame) => gizmo::shapes(tool, frame, screen),
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
            && let (Some(handle), Some(pointer), Some(frame)) = (hovered, pointer, frame)
        {
            self.drag = Drag::start_many(handle, frame, parts.clone(), modes, screen.ray(pointer))
                .map(Dragging::Handle);
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
            // Terrain is ground, not something to carry around.
            && world.get(hit.part).map(|i| i.class()) != Some(axcenity_core::ClassName::Terrain)
            && let Some(mut place) =
                PlaceDrag::start(world, hit.part, screen.ray(origin).at(hit.distance))
        {
            // Grabbing a selected part drags the whole selection with it.
            let others: Vec<(InstanceId, Vec3)> = if selection.contains(&hit.part) {
                parts
                    .iter()
                    .filter(|(id, _)| *id != hit.part)
                    .map(|(id, pose)| (*id, pose.position))
                    .collect()
            } else {
                Vec::new()
            };
            place.skip = others.iter().map(|(id, _)| *id).collect();
            self.drag = Some(Dragging::Body(place, others));
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
                    Dragging::Handle(drag) => drag.update_all(ray, &snap).map(|parts| DragEdit {
                        kind: DragKind::of(drag.handle),
                        parts,
                    }),
                    Dragging::Body(place, others) => {
                        place.update(world, ray, &snap).map(|position| {
                            let start = place.start;
                            let moved = position
                                .as_vector3()
                                .map_or(Vec3::ZERO, |p| Vec3::new(p.x, p.y, p.z) - start);
                            let mut parts: PartValues = others
                                .iter()
                                .map(|&(id, at)| {
                                    let at = at + moved;
                                    let value = axcenity_core::Vector3::new(at.x, at.y, at.z);
                                    (id, vec![("Position", Value::Vector3(value))])
                                })
                                .collect();
                            parts.push((place.part, vec![("Position", position)]));
                            DragEdit {
                                kind: DragKind::Move,
                                parts,
                            }
                        })
                    }
                    Dragging::Ui(drag) => {
                        input.ui_edit = Some(drag.update(
                            pointer,
                            ui_scene.pixels_per_point,
                            world,
                            &ui_scene.layout.elements,
                            ui_scene.viewport,
                        ));
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
        lights: &[(LightMarker, egui::Pos2)],
    ) -> Option<Clicked> {
        if !response.clicked() || ui.input(|input| input.modifiers.alt) {
            return None;
        }
        let pointer = response.interact_pointer_pos()?;
        Some(Clicked {
            part: ui_scene
                .hit(pointer)
                .or_else(|| light_at(lights, pointer))
                .or_else(|| axcenity_scene::pick(world, screen.ray(pointer))),
            modifiers: ui.input(|input| input.modifiers),
            keep_group: false,
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
        // A dark scrim, not the theme's colours: it has to read over
        // whatever the scene shows.
        let galley = painter.layout_no_wrap(
            speed_label(self.camera.speed()),
            egui::FontId::new(15.0, crate::theme::medium_family()),
            egui::Color32::WHITE.gamma_multiply(opacity),
        );
        let background =
            egui::Rect::from_center_size(rect.center(), galley.size() + egui::vec2(28.0, 14.0));
        let shade = egui::Color32::from_black_alpha((160.0 * opacity) as u8);
        painter.rect_filled(background, 8.0, shade);
        painter.rect_stroke(
            background,
            8.0,
            egui::Stroke::new(
                1.0_f32,
                egui::Color32::from_white_alpha((28.0 * opacity) as u8),
            ),
            egui::StrokeKind::Inside,
        );
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

/// Draws the blue box around the selected part, Studio-style: the edges of
/// the sides facing the camera, where no other part is in front of them.
/// Draws nothing if `selected` isn't a part.
fn paint_selection_box(
    ui: &egui::Ui,
    rect: egui::Rect,
    screen: &Screen,
    world: &World,
    selected: InstanceId,
    eye: Vec3,
) {
    let Some(lines) = axcenity_scene::visible_box_edges(world, selected, eye) else {
        return;
    };
    let painter = ui.painter_at(rect);
    // A soft glow under the line keeps it readable on any background.
    let glow = egui::Stroke::new(SELECTION_WIDTH + 3.0, SELECTION_COLOR.gamma_multiply(0.25));
    let stroke = egui::Stroke::new(SELECTION_WIDTH, SELECTION_COLOR);
    let lines: Vec<_> = lines
        .into_iter()
        .filter_map(|(a, b)| screen.line(a, b))
        .collect();
    for line in &lines {
        painter.line_segment(*line, glow);
    }
    for line in lines {
        painter.line_segment(line, stroke);
    }
}

/// The light whose icon is under `pointer`, the nearest if several are.
fn light_at(lights: &[(LightMarker, egui::Pos2)], pointer: egui::Pos2) -> Option<InstanceId> {
    lights
        .iter()
        .filter(|(_, at)| at.distance(pointer) <= LIGHT_ICON_RADIUS + 2.0)
        .min_by(|(_, a), (_, b)| a.distance(pointer).total_cmp(&b.distance(pointer)))
        .map(|(marker, _)| marker.id)
}

fn light_color(marker: &LightMarker) -> egui::Color32 {
    let [r, g, b] = marker
        .color
        .map(|channel| (channel.clamp(0.0, 1.0) * 255.0) as u8);
    egui::Color32::from_rgb(r, g, b)
}

/// Draws each light's icon (a bulb for a PointLight, a flashlight for a
/// SpotLight) in the light's color.
fn paint_lights(
    ui: &egui::Ui,
    rect: egui::Rect,
    lights: &[(LightMarker, egui::Pos2)],
    selected: Option<InstanceId>,
    pointer: Option<egui::Pos2>,
) {
    let painter = ui.painter_at(rect);
    let hovered = pointer.and_then(|pointer| light_at(lights, pointer));
    for (marker, at) in lights {
        // Grows a little under the mouse.
        let grow = ui.ctx().animate_bool_with_time(
            egui::Id::new(("light icon", marker.id)),
            hovered == Some(marker.id),
            0.12,
        );
        let radius = LIGHT_ICON_RADIUS * (1.0 + 0.15 * grow);
        let (fill, tint) = if marker.enabled {
            (egui::Color32::from_black_alpha(170), light_color(marker))
        } else {
            // Switched off: grey.
            (
                egui::Color32::from_black_alpha(120),
                egui::Color32::from_gray(150),
            )
        };
        painter.circle_filled(*at, radius, fill);
        let ring = if Some(marker.id) == selected {
            egui::Stroke::new(2.0_f32, SELECTION_COLOR)
        } else {
            egui::Stroke::new(1.0 + grow, tint.gamma_multiply(0.8))
        };
        painter.circle_stroke(*at, radius, ring);
        let icon = if marker.spot {
            icons::FLASHLIGHT
        } else {
            icons::LIGHTBULB
        };
        painter.text(
            *at,
            egui::Align2::CENTER_CENTER,
            icon,
            egui::FontId::new(radius * 1.15, theme::filled()),
            tint,
        );
    }
}

/// A light's reach in its color: three rings for a PointLight's range,
/// or for a SpotLight the cone it lights (see-through sides, edges and the
/// far rim), the beam's middle as an arrow, and where that beam lands on a
/// part, as a ring the size of the spot. `chosen` (the light or its part is
/// selected) draws it stronger; View > Light Cones draws the others faintly.
fn paint_light_reach(
    painter: &egui::Painter,
    screen: &Screen,
    world: &World,
    marker: &LightMarker,
    chosen: bool,
) {
    const SEGMENTS: usize = 48;
    let strength = if chosen { 0.85 } else { 0.4 };
    let color = light_color(marker);
    let stroke = egui::Stroke::new(1.5_f32, color.gamma_multiply(strength));
    let faint = egui::Stroke::new(1.0_f32, color.gamma_multiply(strength * 0.5));
    let range = marker.range.max(0.1);
    let circle = |center: Vec3, a: Vec3, b: Vec3, radius: f32| -> Vec<Vec3> {
        (0..=SEGMENTS)
            .map(|i| {
                let turn = i as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
                center + (a * turn.cos() + b * turn.sin()) * radius
            })
            .collect()
    };
    let outline = |points: &[Vec3], stroke: egui::Stroke| {
        for pair in points.windows(2) {
            if let Some(line) = screen.line(pair[0], pair[1]) {
                painter.line_segment(line, stroke);
            }
        }
    };
    let Some((direction, angle)) = marker.cone else {
        outline(&circle(marker.position, Vec3::X, Vec3::Y, range), stroke);
        outline(&circle(marker.position, Vec3::X, Vec3::Z, range), stroke);
        outline(&circle(marker.position, Vec3::Y, Vec3::Z, range), stroke);
        return;
    };

    let half = (angle.clamp(1.0, 179.0) * 0.5).to_radians();
    let (side, up) = direction.any_orthonormal_pair();
    let end = marker.position + direction * range * half.cos();
    let radius = range * half.sin();
    let rim = circle(end, side, up, radius);
    // The sides, as thin triangles from the light to the rim. Each one is
    // convex, which is all egui fills.
    let fill = color.gamma_multiply(if chosen { 0.12 } else { 0.05 });
    if let Some(apex) = screen.point(marker.position) {
        for pair in rim.windows(2) {
            if let (Some(a), Some(b)) = (screen.point(pair[0]), screen.point(pair[1])) {
                painter.add(egui::Shape::convex_polygon(
                    vec![apex, a, b],
                    fill,
                    egui::Stroke::NONE,
                ));
            }
        }
    }
    outline(&rim, stroke);
    // Halfway out, where the light is noticeably dimmer.
    let middle = marker.position + direction * range * half.cos() * 0.5;
    outline(&circle(middle, side, up, radius * 0.5), faint);
    for turn in 0..8 {
        let around = turn as f32 / 8.0 * std::f32::consts::TAU;
        let edge = end + (side * around.cos() + up * around.sin()) * radius;
        if let Some(line) = screen.line(marker.position, edge) {
            painter.line_segment(line, if turn % 2 == 0 { stroke } else { faint });
        }
    }

    // The beam's middle, out to where it first meets a part (not its own).
    let ray = axcenity_scene::Ray {
        origin: marker.position,
        direction,
    };
    let hit = axcenity_scene::pick_hit_skipping(world, ray, &[marker.part])
        .filter(|hit| hit.distance <= range);
    let beam_end = ray.at(hit.map_or(range, |hit| hit.distance));
    if let (Some(from), Some(to)) = (screen.point(marker.position), screen.point(beam_end)) {
        let width = if chosen { 2.0_f32 } else { 1.0 };
        painter.arrow(
            from,
            to - from,
            egui::Stroke::new(width, color.gamma_multiply(strength)),
        );
    }
    if let Some(hit) = hit {
        // The spot on the surface: the cone's width at that distance, laid
        // flat on the surface, lifted a hair so it isn't hidden in it.
        let spot = hit.distance * half.tan();
        let (a, b) = hit.normal.any_orthonormal_pair();
        let lifted = beam_end + hit.normal * 0.02;
        outline(&circle(lifted, a, b, spot), stroke);
        if let Some(at) = screen.point(lifted) {
            painter.circle_filled(at, 3.0, color.gamma_multiply(strength));
        }
    }
}

/// Where `ray` meets the ground of the Terrain `terrain`.
pub fn ground_point(world: &World, terrain: InstanceId, ray: axcenity_scene::Ray) -> Option<Vec3> {
    let placement = axcenity_terrain::Placement::of(world.get(terrain)?)?;
    let data = world.terrain(terrain)?;
    let distance = axcenity_terrain::raycast::raycast(data, &placement, ray.origin, ray.direction)?;
    Some(ray.at(distance))
}

/// The brush's rim (and the inner edge of its fade, fainter) laid on the
/// ground around `center`, and a dot in the middle.
fn paint_terrain_brush(
    painter: &egui::Painter,
    screen: &Screen,
    world: &World,
    brush: TerrainBrush,
    center: Vec3,
) {
    let ground = world.get(brush.terrain).and_then(|instance| {
        Some((
            axcenity_terrain::Placement::of(instance)?,
            world.terrain(brush.terrain)?.clone(),
        ))
    });
    let on_ground = |x: f32, z: f32| {
        let y = ground
            .as_ref()
            .and_then(|(placement, data)| placement.ground_y(data, x, z))
            .unwrap_or(center.y);
        // A little above, so the line isn't hidden in the ground.
        Vec3::new(x, y + 0.05, z)
    };
    let outline = |radius: f32| -> Vec<Vec3> {
        const SEGMENTS: usize = 72;
        (0..=SEGMENTS)
            .map(|i| {
                let t = i as f32 / SEGMENTS as f32;
                let (x, z) = if brush.square {
                    // Round the square's edge, a quarter per side.
                    let side = (t * 4.0).floor().min(3.0);
                    let along = t * 4.0 - side;
                    let s = along * 2.0 - 1.0;
                    match side as u8 {
                        0 => (s, -1.0),
                        1 => (1.0, s),
                        2 => (-s, 1.0),
                        _ => (-1.0, -s),
                    }
                } else {
                    let angle = t * std::f32::consts::TAU;
                    (angle.cos(), angle.sin())
                };
                on_ground(center.x + x * radius, center.z + z * radius)
            })
            .collect()
    };
    let draw = |points: Vec<Vec3>, stroke: egui::Stroke| {
        for pair in points.windows(2) {
            if let Some(line) = screen.line(pair[0], pair[1]) {
                painter.line_segment(line, stroke);
            }
        }
    };
    // A dark line under a bright one, so it reads on any ground.
    let shadow = egui::Stroke::new(3.5_f32, egui::Color32::from_black_alpha(110));
    draw(outline(brush.radius), shadow);
    draw(
        outline(brush.radius),
        egui::Stroke::new(1.6_f32, brush.color),
    );
    let inner = brush.radius * (1.0 - brush.falloff.clamp(0.0, 1.0));
    if inner > 0.05 && brush.falloff > 0.02 {
        draw(
            outline(inner),
            egui::Stroke::new(1.0_f32, brush.color.gamma_multiply(0.55)),
        );
    }
    if let Some(at) = screen.point(on_ground(center.x, center.z)) {
        painter.circle_filled(at, 3.0, brush.color);
    }
}

fn start_camera() -> FlyCamera {
    FlyCamera::looking_at(START_CAMERA_POSITION, Vec3::ZERO)
}

/// Where a newly opened project's camera starts, as the renderer takes it.
pub fn start_view() -> axcenity_render::Camera {
    start_camera().to_render_camera()
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
