//! The UV editor (Phase 58): a pane beside the Studio's 3D view showing
//! the active object's uvs, and the UV menu (U in the 3D view, and the
//! toolbar's UV menu) that marks seams, unwraps, projects and packs.
//!
//! The pane shows the faces picked in Edit mode (all of them when none
//! are), in the texture's square: (0, 0) at the top left, as glTF has it.
//! Its own picks are corners (half-edges): a uv vertex (every corner of a
//! vertex with the same uv), a uv edge, a face or a whole island. G, R and
//! S move them with the mouse (X or Y holds the move to that axis; click
//! or Enter keeps it, right-click or Esc puts it back), as one undo step.
//! P pins, Alt+P unpins, V stitches, A picks everything; a left drag draws
//! a box, a middle drag slides the view, and the wheel zooms.
//!
//! The pane colours faces by stretch (angle or area, blue none to red
//! much) on request, and the 3D view draws a checker on the active uv
//! channel with the checker on. Channel 2 is the second uvs, for
//! lightmaps.

use std::collections::BTreeSet;

use axcenity_mesh_edit::uv::{self, AlignAxis, Projection};
use axcenity_mesh_edit::{EditMesh, SelectMode};
use axcenity_render::glam::{Affine2, Vec2};
use eframe::egui;

use super::{ModalKind, Mode, ModelStudio, Snapshot, StudioDoc};
use crate::shortcuts::{self, Action};
use crate::theme::{self, icons};
use crate::widgets;

/// How close (in points) the mouse must be to pick a uv vertex or edge.
const PICK: f32 = 10.0;

/// What a click picks in the UV editor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UvSelect {
    #[default]
    Vertex,
    Edge,
    Face,
    Island,
}

/// How the UV editor colours faces.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StretchShow {
    #[default]
    Off,
    /// By how far corners' angles moved.
    Angle,
    /// By how far faces' share of the texture differs from their share
    /// of the surface.
    Area,
}

/// Something the UV menu or the UV editor does, as one undo step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UvAction {
    /// The picked edges become seams (or stop being).
    MarkSeam,
    ClearSeam,
    Unwrap,
    SmartProject,
    CubeProject,
    CylinderProject,
    SphereProject,
    ProjectFromView,
    Pack,
    AverageScale,
    /// The UV editor's picked uv vertices (or Edit mode's vertices).
    Pin,
    Unpin,
    Stitch,
    Align(AlignAxis),
    /// Drops the second uvs.
    ClearSecond,
}

impl UvAction {
    pub fn label(self) -> &'static str {
        match self {
            UvAction::MarkSeam => "Mark Seam",
            UvAction::ClearSeam => "Clear Seam",
            UvAction::Unwrap => "Unwrap",
            UvAction::SmartProject => "Smart UV Project",
            UvAction::CubeProject => "Cube Projection",
            UvAction::CylinderProject => "Cylinder Projection",
            UvAction::SphereProject => "Sphere Projection",
            UvAction::ProjectFromView => "Project From View",
            UvAction::Pack => "Pack Islands",
            UvAction::AverageScale => "Average Island Scale",
            UvAction::Pin => "Pin",
            UvAction::Unpin => "Unpin",
            UvAction::Stitch => "Stitch",
            UvAction::Align(AlignAxis::U) => "Align X",
            UvAction::Align(AlignAxis::V) => "Align Y",
            UvAction::ClearSecond => "Clear UV 2",
        }
    }

    /// What it does, for its menu item's hover.
    pub fn tip(self) -> &'static str {
        match self {
            UvAction::MarkSeam => {
                "Mark the picked edges as cuts, where Unwrap splits the surface to lay it flat"
            }
            UvAction::ClearSeam => "Take the cut marks off the picked edges",
            UvAction::Unwrap => {
                "Lay the surface flat on the texture, cut at the seams, with as little stretching as it can"
            }
            UvAction::SmartProject => {
                "Unwrap without seams: split the surface where it bends sharply and lay each part flat. The quick choice for most models"
            }
            UvAction::CubeProject => "Project the texture from six sides, like wrapping a box",
            UvAction::CylinderProject => "Wrap the texture round the model like a label on a can",
            UvAction::SphereProject => "Wrap the texture round the model like a map on a globe",
            UvAction::ProjectFromView => "Lay the texture on as seen from the view's camera",
            UvAction::Pack => {
                "Arrange the flattened parts (islands) to fill the texture without overlapping"
            }
            UvAction::AverageScale => {
                "Size the islands so the texture's detail is even across the model"
            }
            UvAction::Pin => "Hold the picked uv points in place while unwrapping",
            UvAction::Unpin => "Let the picked uv points move again",
            UvAction::Stitch => "Join the picked edges' islands where they meet on the model",
            UvAction::Align(AlignAxis::U) => "Line the picked uv points up across",
            UvAction::Align(AlignAxis::V) => "Line the picked uv points up down",
            UvAction::ClearSecond => "Remove the second uv set (the one lightmaps use)",
        }
    }
}

/// The UV editor's state.
pub struct UvEditor {
    /// The pane is shown beside the 3D view.
    pub open: bool,
    pub select: UvSelect,
    /// 0 for the first uvs, 1 for the second.
    pub channel: usize,
    pub stretch: StretchShow,
    /// The 3D view draws a checker on the uvs.
    pub checker: bool,
    /// Pack's and the unwraps' gap between islands, a share of the texture.
    pub margin: f32,
    /// Pack turns islands to fit.
    pub rotate: bool,
    /// Smart UV Project's angle limit, in degrees.
    pub angle: f32,
    /// The picked corners (half-edges) of the active mesh.
    pub selected: BTreeSet<u32>,
    /// The object and half-edge count the picks are of, so they're
    /// dropped when the mesh is rebuilt.
    selected_of: (usize, usize),
    /// The view: how many texture widths the shorter side shows the
    /// texture at, and the uv at its middle.
    zoom: f32,
    pan: Vec2,
    modal: Option<UvModal>,
    box_start: Option<egui::Pos2>,
    /// Where the U menu was opened in the 3D view.
    pub(super) menu: Option<egui::Pos2>,
    /// What went wrong last, for the pane's hint.
    status: Option<String>,
    /// How many islands overlap, worked out for a revision and channel.
    overlaps: (u64, usize, usize),
}

impl Default for UvEditor {
    fn default() -> Self {
        Self {
            open: false,
            select: UvSelect::default(),
            channel: 0,
            stretch: StretchShow::default(),
            checker: false,
            margin: 0.02,
            rotate: true,
            angle: 66.0,
            selected: BTreeSet::new(),
            selected_of: (usize::MAX, 0),
            zoom: 0.9,
            pan: Vec2::splat(0.5),
            modal: None,
            box_start: None,
            menu: None,
            status: None,
            overlaps: (0, usize::MAX, 0),
        }
    }
}

/// A G, R or S under way in the UV editor.
struct UvModal {
    kind: ModalKind,
    start: egui::Pos2,
    /// The middle of what moves, in uvs.
    center: Vec2,
    corners: Vec<u32>,
    start_uvs: Vec<Vec2>,
    before: Snapshot,
    /// 0 holds the move to u, 1 to v.
    lock: Option<usize>,
}

impl UvEditor {
    /// True while a move or a box is being dragged in the pane.
    pub fn busy(&self) -> bool {
        self.modal.is_some() || self.box_start.is_some()
    }

    /// Forgets the picks and anything under way, for another model.
    pub(super) fn forget(&mut self) {
        self.selected.clear();
        self.selected_of = (usize::MAX, 0);
        self.modal = None;
        self.box_start = None;
        self.menu = None;
        self.status = None;
    }

    /// Drops the picks if they're of another mesh than `doc`'s active one.
    fn sync(&mut self, doc: &StudioDoc) {
        let key = (doc.active, doc.mesh().map_or(0, |m| m.half_edges().len()));
        if self.selected_of != key {
            self.selected.clear();
            self.selected_of = key;
        }
    }
}

/// Between uvs and points in the pane.
pub(super) struct UvScreen {
    pub rect: egui::Rect,
    pub zoom: f32,
    pub pan: Vec2,
}

impl UvScreen {
    /// Points per texture width.
    pub fn scale(&self) -> f32 {
        self.rect.width().min(self.rect.height()) * self.zoom
    }

    pub fn point(&self, uv: Vec2) -> egui::Pos2 {
        let at = (uv - self.pan) * self.scale();
        self.rect.center() + egui::vec2(at.x, at.y)
    }

    pub fn uv(&self, point: egui::Pos2) -> Vec2 {
        let at = point - self.rect.center();
        Vec2::new(at.x, at.y) / self.scale() + self.pan
    }
}

impl StudioDoc {
    /// The faces the uv tools work on: those picked in Edit mode, or all
    /// of the active object's when none are.
    pub fn uv_faces(&self) -> Vec<u32> {
        let Some(mesh) = self.mesh() else {
            return Vec::new();
        };
        let all = || (0..mesh.face_count() as u32).collect();
        if self.mode != Mode::Edit || self.selection.is_empty() {
            return all();
        }
        let faces: Vec<u32> = match self.selection.mode {
            SelectMode::Face => self.selection.faces.iter().copied().collect(),
            _ => {
                let picked: BTreeSet<u32> = self.selection.vertices_of(mesh).into_iter().collect();
                (0..mesh.face_count() as u32)
                    .filter(|&f| mesh.face_vertices(f).iter().all(|v| picked.contains(v)))
                    .collect()
            }
        };
        if faces.is_empty() { all() } else { faces }
    }

    /// The edges Mark Seam marks: the picked ones (in Vertex mode, those
    /// with both ends picked; in Face mode, the picked faces' edges).
    fn picked_edges(&self) -> Vec<[u32; 2]> {
        let Some(mesh) = self.mesh() else {
            return Vec::new();
        };
        let ends = |e: u32| {
            let (a, b) = mesh.ends(e);
            [a, b]
        };
        match self.selection.mode {
            SelectMode::Edge => self.selection.edges.iter().map(|&e| ends(e)).collect(),
            SelectMode::Face => self
                .selection
                .faces
                .iter()
                .flat_map(|&f| mesh.face_half_edges(f))
                .map(ends)
                .collect(),
            SelectMode::Vertex => mesh
                .edges()
                .into_iter()
                .map(ends)
                .filter(|[a, b]| {
                    self.selection.vertices.contains(a) && self.selection.vertices.contains(b)
                })
                .collect(),
        }
    }

    /// Changes the active object's uvs (or seams and pins) with `edit`, as
    /// one undo step named `label`. Nothing changes if it fails.
    pub fn edit_uvs(
        &mut self,
        label: &str,
        edit: impl FnOnce(&mut EditMesh) -> Result<(), String>,
    ) -> Result<(), String> {
        let Some(mesh) = self.mesh() else {
            return Err("Open a model with a mesh first".to_owned());
        };
        let mut mesh = mesh.clone();
        edit(&mut mesh)?;
        let before = self.snapshot();
        let object = &mut self.model.objects[self.active];
        object.mesh = Some(mesh);
        // Its uvs are saved from now on, though the file had none.
        object.uvs = true;
        self.record(label, before);
        Ok(())
    }
}

impl ModelStudio {
    /// Runs `action` on the active object as one undo step. False (with
    /// the reason in the Console and the pane) if it couldn't run.
    pub fn run_uv(&mut self, action: UvAction) -> bool {
        let view = self.view.view_projection();
        let Some(doc) = &mut self.doc else {
            return false;
        };
        let state = &mut self.uv;
        state.sync(doc);
        let faces = doc.uv_faces();
        let edges = doc.picked_edges();
        let channel = state.channel;
        let (margin, rotate, angle) = (state.margin, state.rotate, state.angle);
        // The corners Pin, Stitch and Align work on: the pane's picks, or
        // (for Pin) the corners of Edit mode's picked vertices.
        let picked: Vec<u32> = state.selected.iter().copied().collect();
        let vertices: Vec<u32> = match doc.mesh() {
            Some(mesh) if !picked.is_empty() => {
                picked.iter().map(|&c| mesh.half_edge(c).vertex).collect()
            }
            Some(mesh) if doc.mode == Mode::Edit => doc.selection.vertices_of(mesh),
            _ => Vec::new(),
        };
        let result = doc.edit_uvs(action.label(), |mesh| match action {
            UvAction::MarkSeam | UvAction::ClearSeam => {
                if edges.is_empty() {
                    return Err("Pick edges in Edit mode to mark".to_owned());
                }
                mesh.set_seams(edges, action == UvAction::MarkSeam);
                Ok(())
            }
            UvAction::Unwrap => uv::unwrap(mesh, &faces, margin, channel),
            UvAction::SmartProject => uv::smart_project(mesh, &faces, angle, margin, channel),
            UvAction::CubeProject => uv::project(mesh, &faces, Projection::Cube, channel),
            UvAction::CylinderProject => uv::project(mesh, &faces, Projection::Cylinder, channel),
            UvAction::SphereProject => uv::project(mesh, &faces, Projection::Sphere, channel),
            UvAction::ProjectFromView => uv::project(mesh, &faces, Projection::View(view), channel),
            UvAction::Pack => uv::pack_islands(mesh, &faces, margin, rotate, channel),
            UvAction::AverageScale => uv::average_island_scale(mesh, &faces, channel),
            UvAction::Pin | UvAction::Unpin => {
                if vertices.is_empty() {
                    return Err("Pick uvs to pin".to_owned());
                }
                mesh.set_pins(vertices, action == UvAction::Pin);
                Ok(())
            }
            UvAction::Stitch => match uv::stitch(mesh, &picked, channel) {
                0 => Err("Pick both sides of a cut to stitch".to_owned()),
                _ => Ok(()),
            },
            UvAction::Align(axis) => {
                if picked.len() < 2 {
                    return Err("Pick uvs to line up".to_owned());
                }
                uv::align(mesh, &picked, axis, channel);
                Ok(())
            }
            UvAction::ClearSecond => {
                mesh.clear_uv2();
                Ok(())
            }
        });
        match result {
            Ok(()) => {
                state.status = None;
                true
            }
            Err(error) => {
                self.notices.push(format!("Model Studio: {error}"));
                state.status = Some(error);
                false
            }
        }
    }

    /// The items of the UV menu. Returns the one clicked.
    fn uv_items(&mut self, ui: &mut egui::Ui) -> Option<UvAction> {
        let edit = self.doc.as_ref().is_some_and(|doc| doc.mode == Mode::Edit);
        let mut chosen = None;
        let mut item = |ui: &mut egui::Ui, action: UvAction, keys: &str, enabled: bool| {
            let text = if keys.is_empty() {
                action.label().to_owned()
            } else {
                format!("{}    {keys}", action.label())
            };
            let tip = action.tip();
            if ui
                .add_enabled(enabled, egui::Button::new(text))
                .on_hover_text(tip)
                .on_disabled_hover_text(format!("{tip}. {}", super::tools::EDIT_ONLY))
                .clicked()
            {
                chosen = Some(action);
            }
        };
        widgets::section_label(ui, "Seams");
        item(ui, UvAction::MarkSeam, "", edit);
        item(ui, UvAction::ClearSeam, "", edit);
        widgets::section_label(ui, "Unwrap");
        for action in [
            UvAction::Unwrap,
            UvAction::SmartProject,
            UvAction::CubeProject,
            UvAction::CylinderProject,
            UvAction::SphereProject,
            UvAction::ProjectFromView,
        ] {
            item(ui, action, "", true);
        }
        widgets::section_label(ui, "Islands");
        item(ui, UvAction::Pack, "", true);
        item(ui, UvAction::AverageScale, "", true);
        let state = &mut self.uv;
        egui::Grid::new("studio uv settings")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("Margin");
                ui.add(
                    egui::DragValue::new(&mut state.margin)
                        .speed(0.002)
                        .range(0.0..=0.25),
                );
                ui.end_row();
                ui.label("Angle limit");
                ui.add(
                    egui::DragValue::new(&mut state.angle)
                        .speed(0.5)
                        .range(1.0..=89.0)
                        .suffix("°"),
                );
                ui.end_row();
            });
        ui.checkbox(&mut state.rotate, "Pack turns islands");
        if self
            .doc
            .as_ref()
            .and_then(StudioDoc::mesh)
            .is_some_and(EditMesh::has_uv2)
        {
            ui.separator();
            if ui
                .button(UvAction::ClearSecond.label())
                .on_hover_text(UvAction::ClearSecond.tip())
                .clicked()
            {
                chosen = Some(UvAction::ClearSecond);
            }
        }
        chosen
    }

    /// The toolbar's UV menu.
    pub(super) fn uv_menu(&mut self, ui: &mut egui::Ui) {
        let mut chosen = None;
        axcenity_editor_kit::widgets::menu_button(ui, Some(icons::SQUARES_FOUR), "UV", |ui| {
            chosen = self.uv_items(ui);
            if chosen.is_some() {
                ui.close();
            }
        });
        if let Some(action) = chosen {
            self.run_uv(action);
        }
    }

    /// U in the 3D view, in Edit mode: opens the UV menu at the mouse.
    pub(super) fn uv_keys(&mut self, ui: &mut egui::Ui, pointer: egui::Pos2) -> bool {
        let edit = self.doc.as_ref().is_some_and(|doc| doc.mode == Mode::Edit);
        if edit && ui.input_mut(|i| shortcuts::consume(i, Action::UvMenu)) {
            self.uv.menu = Some(pointer);
            return true;
        }
        false
    }

    /// The menu U opened, over the 3D view.
    pub(super) fn uv_key_menu(&mut self, ui: &mut egui::Ui) {
        let Some(at) = self.uv.menu else {
            return;
        };
        let mut chosen = None;
        let response = egui::Area::new(ui.id().with("studio uv menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(at)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    chosen = self.uv_items(ui);
                });
            })
            .response;
        if let Some(action) = chosen {
            self.uv.menu = None;
            self.run_uv(action);
        } else if response.clicked_elsewhere() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.uv.menu = None;
        }
    }

    /// The UV editor pane: its toolbar and the uvs.
    pub(super) fn uv_pane(&mut self, ui: &mut egui::Ui, editable: bool) {
        let Some(doc) = &self.doc else {
            return;
        };
        self.uv.sync(doc);
        let mut action = None;
        crate::responsive::toolbar(ui, |ui| {
            let state = &mut self.uv;
            widgets::segmented(
                ui,
                &mut state.select,
                &[
                    (UvSelect::Vertex, Some(icons::DOT_OUTLINE), "Vertex"),
                    (UvSelect::Edge, Some(icons::LINE_SEGMENT), "Edge"),
                    (UvSelect::Face, Some(icons::SELECTION), "Face"),
                    (UvSelect::Island, Some(icons::SQUARES_FOUR), "Island"),
                ],
            );
            let mut channel = state.channel;
            if widgets::segmented(ui, &mut channel, &[(0, None, "UV 1"), (1, None, "UV 2")])
                && channel != state.channel
            {
                state.channel = channel;
                state.selected.clear();
            }
            axcenity_editor_kit::widgets::menu_button(ui, None, "Stretch", |ui| {
                ui.selectable_value(&mut state.stretch, StretchShow::Off, "Off");
                ui.selectable_value(&mut state.stretch, StretchShow::Angle, "By angle");
                ui.selectable_value(&mut state.stretch, StretchShow::Area, "By area");
            });
            if widgets::icon_button(
                ui,
                icons::CHECKERBOARD,
                "Checker: draw a checker texture on these uvs in the 3D view",
                state.checker,
            )
            .clicked()
            {
                state.checker = !state.checker;
            }
            if editable {
                axcenity_editor_kit::widgets::menu_button(ui, None, "Edit", |ui| {
                    for (choice, keys) in [
                        (UvAction::Pin, "P"),
                        (UvAction::Unpin, "Alt+P"),
                        (UvAction::Stitch, "V"),
                        (UvAction::Align(AlignAxis::U), ""),
                        (UvAction::Align(AlignAxis::V), ""),
                        (UvAction::Pack, ""),
                        (UvAction::AverageScale, ""),
                    ] {
                        let text = format!("{}    {keys}", choice.label());
                        if ui.button(text.trim_end()).clicked() {
                            action = Some(choice);
                            ui.close();
                        }
                    }
                });
            }
        });
        if let Some(action) = action {
            self.run_uv(action);
        }
        let size = ui.available_size().max(egui::vec2(64.0, 64.0));
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        let screen = UvScreen {
            rect,
            zoom: self.uv.zoom,
            pan: self.uv.pan,
        };
        let painting = self.doc.as_ref().is_some_and(|doc| doc.mode == Mode::Paint);
        if editable && response.contains_pointer() && !painting {
            self.uv_pane_keys(ui, &screen);
        }
        self.uv_pane_mouse(ui, &response, &screen, editable);
        self.paint_uvs(ui, &screen);
    }

    /// The pane's keys while the mouse is over it.
    fn uv_pane_keys(&mut self, ui: &mut egui::Ui, screen: &UvScreen) {
        let pointer = ui
            .input(|i| i.pointer.latest_pos())
            .unwrap_or(screen.rect.center());
        if let Some(modal) = &mut self.uv.modal {
            let (lock, enter, escape) = ui.input_mut(|i| {
                // U, then V.
                let lock = shortcuts::consume_each(i, Action::LockUvAxis)
                    .into_iter()
                    .position(|pressed| pressed);
                (
                    lock,
                    shortcuts::consume(i, Action::FinishUvMove),
                    shortcuts::consume(i, Action::CancelUvMove),
                )
            });
            if let Some(axis) = lock {
                modal.lock = if modal.lock == Some(axis) {
                    None
                } else {
                    Some(axis)
                };
            }
            if enter {
                self.finish_uv_modal();
            } else if escape {
                self.cancel_uv_modal();
            }
            return;
        }
        let keys = ui.input_mut(|i| {
            // Alt+P before P, whose pattern takes Alt too.
            [
                Action::Unpin,
                Action::Pin,
                Action::Stitch,
                Action::SelectAllUvs,
                Action::GrabUvs,
                Action::TurnUvs,
                Action::ResizeUvs,
            ]
            .map(|action| shortcuts::consume(i, action))
        });
        let [unpin, pin, stitch, all, grab, rotate, scale] = keys;
        if unpin {
            self.run_uv(UvAction::Unpin);
        } else if pin {
            self.run_uv(UvAction::Pin);
        } else if stitch {
            self.run_uv(UvAction::Stitch);
        } else if all {
            self.select_all_uvs();
        }
        let kind = [
            (grab, ModalKind::Grab),
            (rotate, ModalKind::Rotate),
            (scale, ModalKind::Scale),
        ]
        .into_iter()
        .find_map(|(pressed, kind)| pressed.then_some(kind));
        if let Some(kind) = kind {
            self.begin_uv_modal(kind, pointer, screen.uv(pointer));
        }
    }

    /// A: every corner shown in the pane.
    pub fn select_all_uvs(&mut self) {
        let Some(doc) = &self.doc else {
            return;
        };
        self.uv.sync(doc);
        let Some(mesh) = doc.mesh() else {
            return;
        };
        self.uv.selected = doc
            .uv_faces()
            .into_iter()
            .flat_map(|f| mesh.face_half_edges(f))
            .collect();
    }

    /// Starts moving the picked uvs from the mouse at `at` (`uv` there).
    pub(super) fn begin_uv_modal(&mut self, kind: ModalKind, at: egui::Pos2, _uv: Vec2) -> bool {
        let Some(doc) = &self.doc else {
            return false;
        };
        let Some(mesh) = doc.mesh() else {
            return false;
        };
        let channel = self.uv.channel;
        let corners: Vec<u32> = self.uv.selected.iter().copied().collect();
        if corners.is_empty() {
            return false;
        }
        let start_uvs: Vec<Vec2> = corners.iter().map(|&c| mesh.uv(c, channel)).collect();
        let (min, max) = start_uvs
            .iter()
            .fold((Vec2::INFINITY, Vec2::NEG_INFINITY), |(min, max), &uv| {
                (min.min(uv), max.max(uv))
            });
        self.uv.modal = Some(UvModal {
            kind,
            start: at,
            center: (min + max) / 2.0,
            corners,
            start_uvs,
            before: doc.snapshot(),
            lock: None,
        });
        true
    }

    /// Moves the picked uvs for the mouse at `mouse`.
    pub(super) fn update_uv_modal(&mut self, screen: &UvScreen, mouse: egui::Pos2) {
        let (Some(modal), Some(doc)) = (&self.uv.modal, &mut self.doc) else {
            return;
        };
        let start = screen.uv(modal.start);
        let now = screen.uv(mouse);
        let matrix = match modal.kind {
            ModalKind::Grab => {
                let mut by = now - start;
                match modal.lock {
                    Some(0) => by.y = 0.0,
                    Some(_) => by.x = 0.0,
                    None => {}
                }
                Affine2::from_translation(by)
            }
            ModalKind::Rotate => {
                let angle = (start - modal.center).angle_to(now - modal.center);
                let angle = if angle.is_finite() { angle } else { 0.0 };
                around(modal.center, Affine2::from_angle(angle))
            }
            ModalKind::Scale => {
                let from = (start - modal.center).length().max(1e-6);
                let factor = (now - modal.center).length() / from;
                let scale = match modal.lock {
                    Some(0) => Vec2::new(factor, 1.0),
                    Some(_) => Vec2::new(1.0, factor),
                    None => Vec2::splat(factor),
                };
                around(modal.center, Affine2::from_scale(scale))
            }
        };
        let channel = self.uv.channel;
        let object = doc.active;
        if let Some(mesh) = doc.model.objects[object].mesh.as_mut() {
            for (&corner, &uv) in modal.corners.iter().zip(&modal.start_uvs) {
                mesh.set_uv(corner, channel, matrix.transform_point2(uv));
            }
        }
        doc.revision += 1;
    }

    /// Keeps the uv move as one undo step.
    pub(super) fn finish_uv_modal(&mut self) {
        if let (Some(modal), Some(doc)) = (self.uv.modal.take(), &mut self.doc) {
            doc.model.objects[doc.active].uvs = true;
            doc.record(&format!("{} UVs", modal.kind.label()), modal.before);
        }
    }

    /// Puts back what the uv move moved.
    pub(super) fn cancel_uv_modal(&mut self) {
        if let (Some(modal), Some(doc)) = (self.uv.modal.take(), &mut self.doc) {
            doc.restore(modal.before);
        }
    }

    /// Clicks, boxes, sliding and zooming in the pane.
    fn uv_pane_mouse(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        screen: &UvScreen,
        editable: bool,
    ) {
        let pointer = ui.input(|i| i.pointer.latest_pos());
        if self.uv.modal.is_some() {
            if let Some(mouse) = pointer {
                self.update_uv_modal(screen, mouse);
            }
            if response.clicked() {
                self.finish_uv_modal();
            } else if response.secondary_clicked() {
                self.cancel_uv_modal();
            }
            return;
        }
        if response.dragged_by(egui::PointerButton::Middle)
            || response.dragged_by(egui::PointerButton::Secondary)
        {
            let delta = response.drag_delta();
            self.uv.pan -= Vec2::new(delta.x, delta.y) / screen.scale();
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.uv.zoom = (self.uv.zoom * (1.0 + scroll * 0.002)).clamp(0.05, 50.0);
            }
        }
        if !editable {
            return;
        }
        if self.doc.as_ref().is_some_and(|doc| doc.mode == Mode::Paint) {
            self.paint_in_uvs(ui, response, screen);
            return;
        }
        let extend = ui.input(|i| i.modifiers.shift);
        if response.drag_started_by(egui::PointerButton::Primary) {
            self.uv.box_start = response.interact_pointer_pos();
        }
        if let Some(start) = self.uv.box_start
            && !ui.input(|i| i.pointer.primary_down())
        {
            self.uv.box_start = None;
            if let Some(end) = pointer {
                let area = egui::Rect::from_two_pos(start, end);
                self.box_uvs(screen, area, extend);
            }
            return;
        }
        if response.clicked()
            && let Some(at) = response.interact_pointer_pos()
        {
            self.click_uvs(screen, at, extend);
        }
    }

    /// The corners a click at `at` picks, by the pick mode.
    pub(super) fn click_uvs(&mut self, screen: &UvScreen, at: egui::Pos2, extend: bool) {
        let Some(doc) = &self.doc else {
            return;
        };
        let Some(mesh) = doc.mesh() else {
            return;
        };
        let channel = self.uv.channel;
        let faces = doc.uv_faces();
        let shown: BTreeSet<u32> = faces
            .iter()
            .flat_map(|&f| mesh.face_half_edges(f))
            .collect();
        let point = |c: u32| screen.point(mesh.uv(c, channel));
        let same = |c: u32| -> Vec<u32> {
            uv::uv_vertex(mesh, c, channel)
                .into_iter()
                .filter(|c| shown.contains(c))
                .collect()
        };
        let face_at = || {
            faces.iter().copied().find(|&f| {
                mesh.triangles(f).into_iter().any(|t| {
                    let [a, b, c] = t.map(&point);
                    inside(at, a, b, c)
                })
            })
        };
        let picked: Vec<u32> = match self.uv.select {
            UvSelect::Vertex => shown
                .iter()
                .copied()
                .map(|c| (c, point(c).distance(at)))
                .filter(|&(_, d)| d < PICK)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(c, _)| same(c))
                .unwrap_or_default(),
            UvSelect::Edge => shown
                .iter()
                .copied()
                .map(|c| {
                    let next = mesh.half_edge(c).next;
                    (c, to_segment(at, point(c), point(next)))
                })
                .filter(|&(_, d)| d < PICK)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(c, _)| {
                    let mut corners = same(c);
                    corners.extend(same(mesh.half_edge(c).next));
                    corners
                })
                .unwrap_or_default(),
            UvSelect::Face => face_at()
                .map(|f| mesh.face_half_edges(f))
                .unwrap_or_default(),
            UvSelect::Island => face_at()
                .and_then(|f| {
                    uv::uv_islands(mesh, &faces, channel)
                        .into_iter()
                        .find(|island| island.contains(&f))
                })
                .map(|island| {
                    island
                        .into_iter()
                        .flat_map(|f| mesh.face_half_edges(f))
                        .collect()
                })
                .unwrap_or_default(),
        };
        let selected = &mut self.uv.selected;
        if !extend {
            selected.clear();
            selected.extend(picked);
        } else if !picked.is_empty() && picked.iter().all(|c| selected.contains(c)) {
            for corner in picked {
                selected.remove(&corner);
            }
        } else {
            selected.extend(picked);
        }
    }

    /// The corners inside `area` (whole faces or islands in those modes).
    fn box_uvs(&mut self, screen: &UvScreen, area: egui::Rect, extend: bool) {
        let Some(doc) = &self.doc else {
            return;
        };
        let Some(mesh) = doc.mesh() else {
            return;
        };
        let channel = self.uv.channel;
        let faces = doc.uv_faces();
        let within = |c: u32| area.contains(screen.point(mesh.uv(c, channel)));
        let picked: Vec<u32> = match self.uv.select {
            UvSelect::Vertex | UvSelect::Edge => faces
                .iter()
                .flat_map(|&f| mesh.face_half_edges(f))
                .filter(|&c| within(c))
                .collect(),
            UvSelect::Face => faces
                .iter()
                .filter(|&&f| mesh.face_half_edges(f).into_iter().all(within))
                .flat_map(|&f| mesh.face_half_edges(f))
                .collect(),
            UvSelect::Island => uv::uv_islands(mesh, &faces, channel)
                .into_iter()
                .filter(|island| {
                    island
                        .iter()
                        .any(|&f| mesh.face_half_edges(f).into_iter().any(within))
                })
                .flatten()
                .flat_map(|f| mesh.face_half_edges(f))
                .collect(),
        };
        if !extend {
            self.uv.selected.clear();
        }
        self.uv.selected.extend(picked);
    }

    /// The texture's square, the faces (coloured by stretch if asked),
    /// their edges and uv vertices, and the pins.
    fn paint_uvs(&mut self, ui: &egui::Ui, screen: &UvScreen) {
        let palette = theme::palette(ui.ctx());
        let painter = ui.painter_at(screen.rect);
        painter.rect_filled(screen.rect, 0.0, palette.inset);
        // The texture, as a light checker of 8 by 8.
        let cells = 8;
        for row in 0..cells {
            for column in 0..cells {
                let min = Vec2::new(column as f32, row as f32) / cells as f32;
                let max = min + Vec2::splat(1.0 / cells as f32);
                let shade = if (row + column) % 2 == 0 { 60 } else { 40 };
                painter.rect_filled(
                    egui::Rect::from_two_pos(screen.point(min), screen.point(max)),
                    0.0,
                    egui::Color32::from_white_alpha(shade),
                );
            }
        }
        // In Paint mode, the map being painted.
        if let Some(texture) = self.paint_texture(ui.ctx()) {
            painter.image(
                texture,
                egui::Rect::from_two_pos(screen.point(Vec2::ZERO), screen.point(Vec2::ONE)),
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        painter.rect_stroke(
            egui::Rect::from_two_pos(screen.point(Vec2::ZERO), screen.point(Vec2::ONE)),
            0.0,
            egui::Stroke::new(1.0_f32, palette.text_secondary),
            egui::StrokeKind::Middle,
        );
        let Some(doc) = &self.doc else {
            return;
        };
        let Some(mesh) = doc.mesh() else {
            return;
        };
        let channel = self.uv.channel;
        let faces = doc.uv_faces();
        let accent = palette.accent;
        let stretch = match self.uv.stretch {
            StretchShow::Off => Vec::new(),
            _ => uv::stretch(mesh, &faces, channel),
        };
        let selected = &self.uv.selected;
        for (k, &face) in faces.iter().enumerate() {
            let corners = mesh.face_half_edges(face);
            let fill = match (self.uv.stretch, stretch.get(k)) {
                (StretchShow::Angle, Some(s)) => Some(stretch_color(s.angle)),
                (StretchShow::Area, Some(s)) => Some(stretch_color(s.area)),
                _ if corners.iter().all(|c| selected.contains(c)) => {
                    Some(accent.gamma_multiply(0.3))
                }
                _ => None,
            };
            if let Some(fill) = fill {
                for triangle in mesh.triangles(face) {
                    let points = triangle.map(|e| screen.point(mesh.uv(e, channel)));
                    painter.add(egui::Shape::convex_polygon(
                        points.to_vec(),
                        fill,
                        egui::Stroke::NONE,
                    ));
                }
            }
        }
        let wire = egui::Color32::from_gray(200);
        for &face in &faces {
            for corner in mesh.face_half_edges(face) {
                let next = mesh.half_edge(corner).next;
                let picked = selected.contains(&corner) && selected.contains(&next);
                let stroke = if picked {
                    egui::Stroke::new(2.0_f32, accent)
                } else {
                    egui::Stroke::new(1.0_f32, wire)
                };
                painter.line_segment(
                    [
                        screen.point(mesh.uv(corner, channel)),
                        screen.point(mesh.uv(next, channel)),
                    ],
                    stroke,
                );
            }
        }
        for &face in &faces {
            for corner in mesh.face_half_edges(face) {
                let at = screen.point(mesh.uv(corner, channel));
                let pinned = mesh.pins().contains(&mesh.half_edge(corner).vertex);
                if pinned {
                    painter.circle_filled(at, 3.5, palette.error);
                } else if selected.contains(&corner) {
                    painter.circle_filled(at, 3.0, accent);
                } else if self.uv.select == UvSelect::Vertex {
                    painter.circle_filled(at, 2.0, wire);
                }
            }
        }
        if let (Some(start), Some(at)) = (self.uv.box_start, ui.input(|i| i.pointer.latest_pos())) {
            let area = egui::Rect::from_two_pos(start, at);
            painter.rect_filled(area, 0.0, accent.gamma_multiply(0.12));
            painter.rect_stroke(
                area,
                0.0,
                egui::Stroke::new(1.0_f32, accent),
                egui::StrokeKind::Inside,
            );
        }
        // A word about overlapping islands, worked out once per change.
        let key = (doc.revision, channel);
        if (self.uv.overlaps.0, self.uv.overlaps.1) != key && self.uv.modal.is_none() {
            let count = uv::overlapping_islands(mesh, &faces, channel).len();
            self.uv.overlaps = (key.0, key.1, count);
        }
        let hint = match (&self.uv.modal, &self.uv.status) {
            (Some(modal), _) => format!(
                "{} UVs · X Y: hold to an axis · click or Enter to keep, right-click or Esc to cancel",
                modal.kind.label()
            ),
            (None, Some(status)) => status.clone(),
            _ if doc.mode == Mode::Paint => match &self.paint.status {
                Some(status) => status.clone(),
                None => "Drag: paint the map in uv space · Ctrl-click: Clone's source · Fill: an island".to_owned(),
            },
            (None, None) if self.uv.overlaps.2 > 0 => {
                format!("{} pairs of islands overlap · Pack Islands lays them out", self.uv.overlaps.2)
            }
            (None, None) => {
                "Click, Shift-click, drag: pick · G R S: move · P pin · V stitch · U in the 3D view: unwrap"
                    .to_owned()
            }
        };
        painter.text(
            screen.rect.left_bottom() + egui::vec2(8.0, -8.0),
            egui::Align2::LEFT_BOTTOM,
            hint,
            egui::FontId::proportional(12.0),
            palette.text_secondary,
        );
    }
}

/// `matrix` applied round `center`.
fn around(center: Vec2, matrix: Affine2) -> Affine2 {
    Affine2::from_translation(center) * matrix * Affine2::from_translation(-center)
}

/// Blue for none, through green and yellow, to red for 1 or more.
fn stretch_color(amount: f32) -> egui::Color32 {
    let t = amount.clamp(0.0, 1.0);
    let hue = (1.0 - t) * 0.66;
    let color: egui::Color32 = egui::ecolor::Hsva::new(hue, 0.8, 0.9, 0.6).into();
    color
}

/// True if `p` is inside the triangle `a`, `b`, `c` (either way round).
fn inside(p: egui::Pos2, a: egui::Pos2, b: egui::Pos2, c: egui::Pos2) -> bool {
    let side = |a: egui::Pos2, b: egui::Pos2| (b - a).x * (p - a).y - (b - a).y * (p - a).x;
    let (d1, d2, d3) = (side(a, b), side(b, c), side(c, a));
    let negative = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let positive = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(negative && positive)
}

/// How far `p` is from the segment from `a` to `b`.
fn to_segment(p: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
    (a + ab * t).distance(p)
}
