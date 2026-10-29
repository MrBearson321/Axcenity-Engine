//! The Model Studio's 3D view (Phase 55): the open model drawn by its own
//! renderer, an orbiting camera, picking objects and mesh elements, box
//! select, the editor's Move, Rotate and Scale handles, and the overlay of
//! vertices, edges, face dots and normals.
//!
//! The model is drawn as a MeshPart in a world of its own, with the model
//! put straight into its own `Assets`, so nothing here touches the world
//! being edited. The handles drag that MeshPart's pose (`gizmo.rs` works on
//! parts), and the change of pose is applied to what's picked.
//!
//! Mouse: left clicks pick (Shift adds, Alt picks a loop), a left drag in
//! Edit mode draws a box, a right or middle drag turns the camera, a
//! Shift+middle drag slides it, and the wheel zooms.

use axcenity_assets::Assets;
use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};
use axcenity_mesh_edit::{EditMesh, SelectMode};
use axcenity_render::Camera;
use axcenity_render::glam::{Mat4, Quat, Vec3};
use axcenity_scene::{Ray, rotation_from_orientation};
use eframe::egui;
use eframe::egui_wgpu::RenderState;

use super::{Modal, Mode, Moving, StudioDoc};
use crate::animation::preview::Canvas;
use crate::gizmo::{self, Drag, Modes, MoveAxes, Pointer, Pose, Snap, Tool};
use crate::screen::Screen;
use crate::theme;

/// The `MeshId` the open model is put under in the view's own `Assets`.
const MESH_ID: &str = "studio/model.glb";
/// How close (in points) the mouse must be to pick a vertex, or an edge.
const VERTEX_PICK: f32 = 10.0;
const EDGE_PICK: f32 = 8.0;

/// What the view is shown with this frame.
pub struct Frame<'a> {
    pub rect: egui::Rect,
    pub response: &'a egui::Response,
    pub snap: Snap,
    pub tool: Tool,
    pub axes: MoveAxes,
    pub show_normals: bool,
}

/// What the mouse did in the view.
pub enum ViewAction {
    None,
    /// Picked an object, or nothing.
    PickObject {
        object: Option<usize>,
        extend: bool,
    },
    /// Picked a vertex, edge (key) or face by the selection's mode.
    Pick {
        element: Option<u32>,
        extend: bool,
    },
    /// Alt-clicked near an edge (a half-edge).
    Loop {
        edge: u32,
        extend: bool,
    },
    /// These elements were inside a box.
    Box {
        elements: Vec<u32>,
        extend: bool,
    },
    /// A handle drag started, moved what's picked by this, or ended.
    BeginHandle,
    Handle(Mat4),
    EndHandle(&'static str),
}

pub struct Event {
    pub screen: Screen,
    pub clicked: bool,
    pub secondary_clicked: bool,
    pub action: ViewAction,
}

pub struct View {
    canvas: Canvas,
    world: World,
    /// The MeshPart that draws the model, whose pose the handles drag.
    part: InstanceId,
    assets: Assets,
    /// The revision of the meshes drawn, 0 for none.
    shown: u64,
    yaw: f32,
    pitch: f32,
    distance: f32,
    target: Vec3,
    /// Where a box drag started.
    box_start: Option<egui::Pos2>,
    /// A handle being dragged, and where its frame started.
    drag: Option<(Drag, Pose)>,
    /// What a handle drag moves.
    pub(super) moving: Option<Moving>,
}

impl Default for View {
    fn default() -> Self {
        let mut world = World::new();
        let part = world.create(ClassName::MeshPart).expect("not a service");
        let workspace = world.workspace();
        world
            .set_property(part, "Anchored", Value::Bool(true))
            .expect("MeshParts have this");
        world
            .set_parent(part, Some(workspace))
            .expect("MeshParts go in the Workspace");
        Self {
            canvas: Canvas::default(),
            world,
            part,
            assets: Assets::default(),
            shown: 0,
            yaw: 35.0,
            pitch: 25.0,
            distance: 8.0,
            target: Vec3::ZERO,
            box_start: None,
            drag: None,
            moving: None,
        }
    }
}

impl View {
    /// True while a handle or a box is being dragged.
    pub fn dragging(&self) -> bool {
        self.drag.is_some() || self.box_start.is_some()
    }

    /// Points the camera at `doc`'s model, and draws it afresh.
    pub fn frame_doc(&mut self, doc: &StudioDoc) {
        self.shown = 0;
        self.drag = None;
        self.moving = None;
        self.box_start = None;
        let (min, max) = bounds(doc);
        self.target = (min + max) / 2.0;
        self.distance = ((max - min).length() * 1.6).max(2.0);
    }

    fn camera(&self) -> Camera {
        let (yaw, pitch) = (self.yaw.to_radians(), self.pitch.to_radians());
        let direction = Vec3::new(
            yaw.sin() * pitch.cos(),
            pitch.sin(),
            yaw.cos() * pitch.cos(),
        );
        Camera {
            position: self.target + direction * self.distance,
            look_at: self.target,
            fov_y_degrees: 50.0,
        }
    }

    /// Puts the model's current meshes in the view's world.
    fn refresh(&mut self, doc: &StudioDoc) {
        if self.shown == doc.revision {
            return;
        }
        self.shown = doc.revision;
        let Some(model) = doc.model.to_model() else {
            let _ = self
                .world
                .set_property(self.part, "MeshId", Value::String(String::new()));
            return;
        };
        let size = Vec3::from(model.size).max(Vec3::splat(0.001));
        let center = Vec3::from(model.center);
        self.assets.insert(MESH_ID, model);
        for (name, value) in [
            ("MeshId", Value::String(MESH_ID.to_owned())),
            ("Size", Value::Vector3(Vector3::new(size.x, size.y, size.z))),
            (
                "Position",
                Value::Vector3(Vector3::new(center.x, center.y, center.z)),
            ),
        ] {
            let _ = self.world.set_property(self.part, name, value);
        }
        // The same MeshId with a new model: forget the old one's draw data.
        let _ = self
            .world
            .set_property(self.part, "MeshId", Value::String(MESH_ID.to_owned()));
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        frame: &Frame,
        render_state: Option<&RenderState>,
        doc: &mut StudioDoc,
        modal: Option<&Modal>,
    ) -> Event {
        let rect = frame.rect;
        let response = frame.response;
        self.refresh(doc);
        self.orbit(ui, response);
        let screen = Screen::new(self.camera(), rect);
        let mut event = Event {
            screen,
            clicked: response.clicked(),
            secondary_clicked: response.secondary_clicked(),
            action: ViewAction::None,
        };
        let (pointer, modifiers, pressed, down) = ui.input(|input| {
            (
                input.pointer.latest_pos(),
                input.modifiers,
                input.pointer.primary_pressed(),
                input.pointer.primary_down(),
            )
        });

        // The handles, on what's picked.
        let pose = (modal.is_none() && frame.tool.has_handles())
            .then(|| handle_pose(doc, frame.axes))
            .flatten();
        let shapes = pose.map_or_else(Vec::new, |pose| gizmo::shapes(frame.tool, pose, &screen));
        let hovered = match pointer {
            Some(at)
                if response.contains_pointer()
                    && self.drag.is_none()
                    && self.box_start.is_none() =>
            {
                gizmo::hovered(&shapes, at)
            }
            _ => None,
        };
        if modal.is_none() {
            event.action = self.interact(
                &screen, frame, doc, pose, hovered, pointer, modifiers, pressed, down,
            );
        }

        if let Some(render_state) = render_state {
            let list = axcenity_scene::build_draw_list(&self.world, &self.assets, self.camera());
            self.canvas.paint(ui, rect, render_state, &list);
        } else {
            ui.painter()
                .rect_filled(rect, 0.0, theme::palette(ui.ctx()).inset);
        }
        let painter = ui.painter_at(rect);
        paint_overlay(
            &painter,
            &screen,
            doc,
            frame.show_normals,
            theme::palette(ui.ctx()).accent,
        );
        let highlighted = self.drag.as_ref().map(|(drag, _)| drag.handle).or(hovered);
        gizmo::paint(&painter, &shapes, highlighted);
        if let (Some(start), Some(at)) = (self.box_start, pointer) {
            let accent = theme::palette(ui.ctx()).accent;
            let area = egui::Rect::from_two_pos(start, at);
            painter.rect_filled(area, 0.0, accent.gamma_multiply(0.12));
            painter.rect_stroke(
                area,
                0.0,
                egui::Stroke::new(1.0_f32, accent),
                egui::StrokeKind::Inside,
            );
        }
        let hint = match (modal, doc.mode) {
            (Some(modal), _) => modal.describe(),
            (None, Mode::Object) => "Click an object · Tab: Edit mode · G R S: move, turn, scale · right-drag: turn view".to_owned(),
            (None, Mode::Edit) => "1 2 3: vertex, edge, face · Alt-click: loop · L: linked · Ctrl +/−: grow, shrink · drag: box".to_owned(),
        };
        let palette = theme::palette(ui.ctx());
        painter.text(
            rect.left_bottom() + egui::vec2(8.0, -8.0),
            egui::Align2::LEFT_BOTTOM,
            hint,
            egui::FontId::proportional(12.0),
            palette.text_secondary,
        );
        event
    }

    /// Turns, slides and zooms the camera.
    fn orbit(&mut self, ui: &egui::Ui, response: &egui::Response) {
        let shift = ui.input(|input| input.modifiers.shift);
        if response.dragged_by(egui::PointerButton::Middle) && shift {
            let delta = response.drag_delta();
            let camera = self.camera();
            let forward = (camera.look_at - camera.position).normalize_or(Vec3::NEG_Z);
            let right = forward.cross(Vec3::Y).normalize_or(Vec3::X);
            let up = right.cross(forward);
            let per_point = self.distance / response.rect.height().max(1.0);
            self.target += (-right * delta.x + up * delta.y) * per_point;
        } else if response.dragged_by(egui::PointerButton::Secondary)
            || response.dragged_by(egui::PointerButton::Middle)
        {
            let delta = response.drag_delta();
            self.yaw -= delta.x * 0.4;
            self.pitch = (self.pitch + delta.y * 0.3).clamp(-89.0, 89.0);
        }
        if response.hovered() {
            let scroll = ui.input(|input| input.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.distance = (self.distance * (1.0 - scroll * 0.002)).clamp(0.05, 10_000.0);
            }
        }
    }

    /// Handles, clicks and boxes.
    #[allow(clippy::too_many_arguments)]
    fn interact(
        &mut self,
        screen: &Screen,
        frame: &Frame,
        doc: &StudioDoc,
        pose: Option<Pose>,
        hovered: Option<gizmo::Handle>,
        pointer: Option<egui::Pos2>,
        modifiers: egui::Modifiers,
        pressed: bool,
        down: bool,
    ) -> ViewAction {
        let response = frame.response;
        if pressed && let (Some(handle), Some(at), Some(pose)) = (hovered, pointer, pose) {
            let modes = Modes {
                axes: frame.axes,
                ..Modes::default()
            };
            if let Some(drag) =
                Drag::start_many(handle, pose, vec![(self.part, pose)], modes, screen, at)
            {
                self.drag = Some((drag, pose));
                return ViewAction::BeginHandle;
            }
        }
        if let Some((drag, start)) = &mut self.drag {
            if !down {
                self.drag = None;
                return ViewAction::EndHandle(match frame.tool {
                    Tool::Rotate => "Rotate",
                    Tool::Scale => "Scale",
                    _ => "Move",
                });
            }
            let Some(at) = pointer else {
                return ViewAction::None;
            };
            let snap = if modifiers.command {
                frame.snap.flipped()
            } else {
                frame.snap
            };
            let Some(values) = drag.update_all(Pointer::on(screen, at, modifiers.shift), &snap)
            else {
                return ViewAction::None;
            };
            let Some((_, values)) = values.into_iter().find(|(id, _)| *id == self.part) else {
                return ViewAction::None;
            };
            return ViewAction::Handle(pose_change(*start, &values));
        }

        // A box, in Edit mode.
        if doc.mode == Mode::Edit && response.drag_started_by(egui::PointerButton::Primary) {
            self.box_start = response
                .interact_pointer_pos()
                .map(|at| at - response.drag_delta());
        }
        if let Some(start) = self.box_start
            && (response.drag_stopped_by(egui::PointerButton::Primary) || !down)
        {
            self.box_start = None;
            let Some(end) = pointer else {
                return ViewAction::None;
            };
            let area = egui::Rect::from_two_pos(start, end);
            let elements = doc
                .mesh()
                .map(|mesh| boxed(mesh, doc.selection.mode, screen, area));
            return ViewAction::Box {
                elements: elements.unwrap_or_default(),
                extend: modifiers.shift,
            };
        }

        if !response.clicked() || hovered.is_some() {
            return ViewAction::None;
        }
        let Some(at) = response.interact_pointer_pos() else {
            return ViewAction::None;
        };
        let ray = screen.ray(at);
        let extend = modifiers.shift;
        match doc.mode {
            Mode::Object => ViewAction::PickObject {
                object: pick_object(doc, ray),
                extend,
            },
            Mode::Edit => {
                let Some(mesh) = doc.mesh() else {
                    return ViewAction::None;
                };
                if modifiers.alt {
                    return match nearest_edge(mesh, screen, at, EDGE_PICK * 1.5) {
                        Some(edge) => ViewAction::Loop { edge, extend },
                        None => ViewAction::None,
                    };
                }
                let element = match doc.selection.mode {
                    SelectMode::Vertex => nearest_vertex(mesh, screen, at),
                    SelectMode::Edge => {
                        nearest_edge(mesh, screen, at, EDGE_PICK).map(|e| mesh.edge_key(e))
                    }
                    SelectMode::Face => pick_face(mesh, ray).map(|(face, _)| face),
                };
                ViewAction::Pick { element, extend }
            }
        }
    }
}

/// The box around every object's vertices, or a unit box with none.
fn bounds(doc: &StudioDoc) -> (Vec3, Vec3) {
    let points = doc
        .model
        .objects
        .iter()
        .filter_map(|object| object.mesh.as_ref())
        .flat_map(|mesh| mesh.positions().iter().copied());
    let (min, max) = points.fold((Vec3::INFINITY, Vec3::NEG_INFINITY), |(min, max), p| {
        (min.min(p), max.max(p))
    });
    if min.cmple(max).all() {
        (min, max)
    } else {
        (Vec3::splat(-0.5), Vec3::splat(0.5))
    }
}

/// Where the handles go on what's picked: the middle of its box along
/// the axes (the selection's own with Local), sized to it.
pub(super) fn handle_pose(doc: &StudioDoc, axes: MoveAxes) -> Option<Pose> {
    let (_, normal) = doc.pivot()?;
    let rotation = match axes {
        MoveAxes::Local => super::modal::frame_of(normal),
        _ => Quat::IDENTITY,
    };
    let inverse = rotation.inverse();
    let (min, max) = doc
        .targets()
        .iter()
        .flat_map(|(object, vertices)| {
            let mesh = doc.model.objects[*object].mesh.as_ref();
            vertices
                .iter()
                .filter_map(move |&v| Some(inverse * mesh?.position(v)))
        })
        .fold((Vec3::INFINITY, Vec3::NEG_INFINITY), |(min, max), p| {
            (min.min(p), max.max(p))
        });
    if !min.cmple(max).all() {
        return None;
    }
    Some(Pose {
        position: rotation * ((min + max) / 2.0),
        rotation,
        size: max - min,
    })
}

/// The move from `start` to the pose a handle drag's `values` (Position,
/// Size, Orientation) give, as a matrix for the picked vertices. A size
/// that was flat stays as it was.
pub(super) fn pose_change(start: Pose, values: &[(&'static str, Value)]) -> Mat4 {
    let mut end = start;
    for (name, value) in values {
        let Value::Vector3(v) = value else {
            continue;
        };
        let v = Vec3::new(v.x, v.y, v.z);
        match *name {
            "Position" => end.position = v,
            "Size" => end.size = v,
            "Orientation" => end.rotation = rotation_from_orientation(Vector3::new(v.x, v.y, v.z)),
            _ => {}
        }
    }
    let ratio = Vec3::select(
        start.size.cmpgt(Vec3::splat(1e-4)),
        end.size / start.size.max(Vec3::splat(1e-4)),
        Vec3::ONE,
    );
    Mat4::from_translation(end.position)
        * Mat4::from_quat(end.rotation)
        * Mat4::from_scale(ratio)
        * Mat4::from_quat(start.rotation.inverse())
        * Mat4::from_translation(-start.position)
}

/// How far along `ray` it hits the triangle `a b c`, from either side.
fn hit_triangle(ray: Ray, [a, b, c]: [Vec3; 3]) -> Option<f32> {
    // Möller–Trumbore: solve origin + t·direction = a + u·(b−a) + v·(c−a).
    let (ab, ac) = (b - a, c - a);
    let p = ray.direction.cross(ac);
    let det = ab.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let to_origin = ray.origin - a;
    let u = to_origin.dot(p) / det;
    let q = to_origin.cross(ab);
    let v = ray.direction.dot(q) / det;
    let t = ac.dot(q) / det;
    (u >= 0.0 && v >= 0.0 && u + v <= 1.0 && t > 0.0).then_some(t)
}

/// The face of `mesh` nearest along `ray`, and how far.
pub(super) fn pick_face(mesh: &EditMesh, ray: Ray) -> Option<(u32, f32)> {
    (0..mesh.face_count() as u32)
        .flat_map(|face| {
            mesh.triangles(face)
                .into_iter()
                .filter_map(move |triangle| {
                    let corners = triangle.map(|e| mesh.position(mesh.half_edge(e).vertex));
                    Some((face, hit_triangle(ray, corners)?))
                })
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
}

/// The object nearest along `ray`.
fn pick_object(doc: &StudioDoc, ray: Ray) -> Option<usize> {
    doc.model
        .objects
        .iter()
        .enumerate()
        .filter_map(|(index, object)| Some((index, pick_face(object.mesh.as_ref()?, ray)?.1)))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(index, _)| index)
}

/// The vertex shown nearest `at`, within [`VERTEX_PICK`] points.
pub(super) fn nearest_vertex(mesh: &EditMesh, screen: &Screen, at: egui::Pos2) -> Option<u32> {
    mesh.positions()
        .iter()
        .enumerate()
        .filter_map(|(index, &p)| Some((index as u32, screen.point(p)?.distance(at))))
        .filter(|(_, distance)| *distance <= VERTEX_PICK)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(index, _)| index)
}

/// The edge (a half-edge) shown nearest `at`, within `reach` points.
pub(super) fn nearest_edge(
    mesh: &EditMesh,
    screen: &Screen,
    at: egui::Pos2,
    reach: f32,
) -> Option<u32> {
    mesh.edges()
        .into_iter()
        .filter_map(|edge| {
            let (a, b) = mesh.ends(edge);
            let [a, b] = screen.line(mesh.position(a), mesh.position(b))?;
            Some((edge, distance_to_segment(at, a, b)))
        })
        .filter(|(_, distance)| *distance <= reach)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(edge, _)| edge)
}

fn distance_to_segment(p: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// The elements of `mesh` (by `mode`) shown inside `area`: vertices, edges
/// with both ends inside, or faces whose middle is.
pub(super) fn boxed(
    mesh: &EditMesh,
    mode: SelectMode,
    screen: &Screen,
    area: egui::Rect,
) -> Vec<u32> {
    let inside = |p: Vec3| screen.point(p).is_some_and(|at| area.contains(at));
    match mode {
        SelectMode::Vertex => (0..mesh.vertex_count() as u32)
            .filter(|&v| inside(mesh.position(v)))
            .collect(),
        SelectMode::Edge => mesh
            .edges()
            .into_iter()
            .filter(|&edge| {
                let (a, b) = mesh.ends(edge);
                inside(mesh.position(a)) && inside(mesh.position(b))
            })
            .collect(),
        SelectMode::Face => (0..mesh.face_count() as u32)
            .filter(|&face| inside(mesh.face_center(face)))
            .collect(),
    }
}

/// Edit mode's vertices, edges, face dots and picked faces on the active
/// object; Object mode's outline of the picked objects; and normals.
fn paint_overlay(
    painter: &egui::Painter,
    screen: &Screen,
    doc: &StudioDoc,
    normals: bool,
    accent: egui::Color32,
) {
    let wire = egui::Color32::from_black_alpha(150);
    let normal_color = egui::Color32::from_rgb(90, 200, 255);
    let line = |a: Vec3, b: Vec3, stroke: egui::Stroke| {
        if let Some(segment) = screen.line(a, b) {
            painter.line_segment(segment, stroke);
        }
    };
    for (index, object) in doc.model.objects.iter().enumerate() {
        let Some(mesh) = &object.mesh else {
            continue;
        };
        let editing = doc.mode == Mode::Edit && index == doc.active;
        if normals {
            let length = mesh_size(mesh) * 0.2;
            for face in 0..mesh.face_count() as u32 {
                let center = mesh.face_center(face);
                line(
                    center,
                    center + mesh.face_normal(face) * length,
                    egui::Stroke::new(1.0_f32, normal_color),
                );
            }
        }
        if !editing {
            if doc.mode == Mode::Object && doc.objects.contains(&index) {
                let stroke = egui::Stroke::new(1.0_f32, accent.gamma_multiply(0.7));
                for edge in mesh.edges() {
                    let (a, b) = mesh.ends(edge);
                    line(mesh.position(a), mesh.position(b), stroke);
                }
            }
            continue;
        }
        let selection = &doc.selection;
        let picked_vertices: std::collections::BTreeSet<u32> =
            selection.vertices_of(mesh).into_iter().collect();
        // Picked faces, tinted.
        if selection.mode == SelectMode::Face {
            for &face in &selection.faces {
                for triangle in mesh.triangles(face) {
                    let points: Option<Vec<egui::Pos2>> = triangle
                        .iter()
                        .map(|&e| screen.point(mesh.position(mesh.half_edge(e).vertex)))
                        .collect();
                    if let Some(points) = points {
                        painter.add(egui::Shape::convex_polygon(
                            points,
                            accent.gamma_multiply(0.25),
                            egui::Stroke::NONE,
                        ));
                    }
                }
            }
        }
        for edge in mesh.edges() {
            let (a, b) = mesh.ends(edge);
            let picked = match selection.mode {
                SelectMode::Edge => selection.edges.contains(&edge),
                _ => picked_vertices.contains(&a) && picked_vertices.contains(&b),
            };
            let stroke = if picked {
                egui::Stroke::new(2.0_f32, accent)
            } else {
                egui::Stroke::new(1.0_f32, wire)
            };
            line(mesh.position(a), mesh.position(b), stroke);
        }
        match selection.mode {
            SelectMode::Vertex => {
                for (vertex, &p) in mesh.positions().iter().enumerate() {
                    if let Some(at) = screen.point(p) {
                        let color = if selection.vertices.contains(&(vertex as u32)) {
                            accent
                        } else {
                            egui::Color32::BLACK
                        };
                        painter.circle_filled(at, 3.0, color);
                    }
                }
            }
            SelectMode::Face => {
                for face in 0..mesh.face_count() as u32 {
                    if let Some(at) = screen.point(mesh.face_center(face)) {
                        let color = if selection.faces.contains(&face) {
                            accent
                        } else {
                            egui::Color32::BLACK
                        };
                        painter.rect_filled(
                            egui::Rect::from_center_size(at, egui::vec2(4.0, 4.0)),
                            0.0,
                            color,
                        );
                    }
                }
            }
            SelectMode::Edge => {}
        }
    }
}

/// The longest side of the box around `mesh`.
fn mesh_size(mesh: &EditMesh) -> f32 {
    let (min, max) = mesh
        .positions()
        .iter()
        .fold((Vec3::INFINITY, Vec3::NEG_INFINITY), |(min, max), &p| {
            (min.min(p), max.max(p))
        });
    (max - min).max_element().max(0.1)
}
