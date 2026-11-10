//! The Model Studio's modeling tools (Phase 56): the operations' keys and
//! the Mesh menu, the knife and loop cut, following the mouse with an
//! operation's main value, the adjust panel, and the pivot and
//! proportional editing settings.
//!
//! Keys (Edit mode, mouse over the view): E extrude, I inset, Ctrl+B bevel
//! edges, Ctrl+Shift+B bevel vertices, Ctrl+R loop cut, G G edge slide, K
//! knife, M merge, X or Delete delete and dissolve, F fill, Shift+N and
//! Ctrl+Shift+N recalculate normals, Alt+N flip. In either mode: O
//! proportional editing, Shift+C the 3D cursor back to the origin.
//!
//! E, I, Ctrl+B, G G and a loop cut's click run the operation, then follow
//! the mouse with its main value (the extrude's distance along the
//! normal, the inset's thickness, the bevel's width, the slide), each
//! frame running it again as the adjust panel does. Typing a number sets
//! the value; a click or Enter keeps it; Esc or a right-click takes the
//! operation back. The wheel changes a bevel's segments, or a loop cut's
//! cuts while choosing the edge.

use axcenity_mesh_edit::{
    DeleteKind, DissolveKind, ExtrudeMode, KnifePoint, MergeAt, MeshOp, apply,
};
use axcenity_render::glam::Vec3;
use eframe::egui;

use super::view::{self, Event, Extras};
use super::{Falloff, Mode, ModelStudio, Pivot};
use crate::screen::Screen;
use crate::theme::{self, icons};
use crate::widgets;

/// What the tools are doing, if anything.
#[derive(Default)]
pub(super) struct Tools {
    /// The knife's points so far, and where each is.
    pub knife: Option<Vec<(KnifePoint, Vec3)>>,
    /// Choosing a loop cut's edge.
    pub loop_cut: Option<LoopCutChoice>,
    /// A value following the mouse.
    pub drag: Option<ValueDrag>,
    /// A menu a key opened, and where.
    pub menu: Option<(KeyMenu, egui::Pos2)>,
    /// The last operation's error, shown until the next one runs.
    pub status: Option<String>,
}

impl Tools {
    /// True while a tool has the mouse.
    pub fn busy(&self) -> bool {
        self.knife.is_some() || self.loop_cut.is_some() || self.drag.is_some()
    }
}

/// A loop cut waiting for its edge: the edge under the mouse, the loops
/// it would cut, and how many.
pub(super) struct LoopCutChoice {
    edge: Option<[u32; 2]>,
    lines: Vec<[Vec3; 2]>,
    cuts: u32,
}

impl Default for LoopCutChoice {
    fn default() -> Self {
        Self {
            edge: None,
            lines: Vec::new(),
            cuts: 1,
        }
    }
}

/// Which value of the last operation follows the mouse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Value {
    /// An extrude's distance, along the normal.
    Distance,
    /// An inset's thickness.
    Thickness,
    /// A bevel's width.
    Width,
    /// An edge slide's factor.
    Factor,
    /// A loop cut's slide.
    Slide,
}

/// A value following the mouse from where it was when the operation ran.
pub(super) struct ValueDrag {
    value: Value,
    start: egui::Pos2,
    /// The middle of what's picked, and its normal.
    center: Vec3,
    normal: Vec3,
    /// A number typed in place of the mouse's value.
    pub typed: String,
}

impl ValueDrag {
    /// The value with the mouse at `mouse`.
    fn amount(&self, screen: &Screen, mouse: egui::Pos2) -> f32 {
        if let Ok(typed) = self.typed.parse::<f32>() {
            return typed;
        }
        let delta = mouse - self.start;
        match self.value {
            Value::Distance => {
                // How far along the normal the mouse went: where its ray
                // passes nearest the normal's line, from where it started.
                let along = |at: egui::Pos2| {
                    let ray = screen.ray(at);
                    nearest_along(self.center, self.normal, ray.origin, ray.direction)
                };
                match (along(mouse), along(self.start)) {
                    (Some(now), Some(then)) => now - then,
                    _ => -delta.y * screen.studs_per_point(self.center),
                }
            }
            Value::Thickness | Value::Width => {
                let middle = screen.point(self.center).unwrap_or(self.start);
                let farther = (mouse - middle).length() - (self.start - middle).length();
                farther.max(0.0) * screen.studs_per_point(self.center)
            }
            Value::Factor | Value::Slide => (delta.x / 200.0).clamp(-1.0, 1.0),
        }
    }
}

/// How far along the line through `origin` in `direction` it passes
/// nearest the ray from `from` in `toward`, in lengths of `direction`;
/// `None` if they run the same way.
fn nearest_along(origin: Vec3, direction: Vec3, from: Vec3, toward: Vec3) -> Option<f32> {
    let gap = origin - from;
    let (a, b, c) = (
        direction.dot(direction),
        direction.dot(toward),
        toward.dot(toward),
    );
    let (d, e) = (direction.dot(gap), toward.dot(gap));
    let denominator = a * c - b * b;
    (denominator.abs() > 1e-9 * a * c).then(|| (b * e - c * d) / denominator)
}

/// `op` with `value` set to `amount`.
fn with_value(op: &MeshOp, value: Value, amount: f32) -> MeshOp {
    let mut op = op.clone();
    match (&mut op, value) {
        (MeshOp::Extrude { distance, .. }, Value::Distance) => *distance = amount,
        (MeshOp::Inset { thickness, .. }, Value::Thickness) => *thickness = amount,
        (MeshOp::Bevel { width, .. }, Value::Width) => *width = amount,
        (MeshOp::EdgeSlide { factor }, Value::Factor) => *factor = amount,
        (MeshOp::LoopCut { slide, .. }, Value::Slide) => *slide = amount,
        _ => {}
    }
    op
}

/// The menus keys open at the mouse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum KeyMenu {
    /// M.
    Merge,
    /// X or Delete.
    Delete,
}

/// Why the Mesh menu's operations and Mark Seam are greyed out.
pub(super) const EDIT_ONLY: &str =
    "These work on what's picked in Edit mode: press Tab or click Edit.";

impl ModelStudio {
    /// Runs `op` on what's picked as one undo step. False (with the reason
    /// in the Console and the view) if it couldn't run.
    pub fn run_op(&mut self, op: MeshOp) -> bool {
        self.refresh_snap();
        let Some(doc) = &mut self.doc else {
            return false;
        };
        match doc.run(op) {
            Ok(()) => {
                self.tools.status = None;
                true
            }
            Err(error) => {
                self.notices.push(format!("Model Studio: {error}"));
                self.tools.status = Some(error);
                false
            }
        }
    }

    /// Runs the last operation again with `op`'s settings (the adjust
    /// panel), with no new undo step.
    pub fn adjust_op(&mut self, op: MeshOp) -> bool {
        self.refresh_snap();
        let Some(doc) = &mut self.doc else {
            return false;
        };
        match doc.adjust(op) {
            Ok(()) => {
                self.tools.status = None;
                true
            }
            Err(error) => {
                self.tools.status = Some(error);
                false
            }
        }
    }

    /// Runs `op`, then has `value` follow the mouse from `at`.
    pub(super) fn begin_value(&mut self, op: MeshOp, value: Value, at: egui::Pos2) {
        if !self.run_op(op) {
            return;
        }
        let Some(doc) = &self.doc else {
            return;
        };
        let Some(mesh) = doc.mesh() else {
            return;
        };
        self.tools.drag = Some(ValueDrag {
            value,
            start: at,
            center: doc.selection.center(mesh).unwrap_or_default(),
            normal: doc.selection.normal(mesh),
            typed: String::new(),
        });
    }

    /// G G: the picked edges slide, following the mouse.
    pub(super) fn begin_edge_slide(&mut self, at: egui::Pos2) {
        self.begin_value(MeshOp::EdgeSlide { factor: 0.0 }, Value::Factor, at);
    }

    /// The size of the active mesh, for operations' first settings.
    fn mesh_size(&self) -> f32 {
        let Some(mesh) = self.doc.as_ref().and_then(|doc| doc.mesh()) else {
            return 1.0;
        };
        let (min, max) = mesh
            .positions()
            .iter()
            .fold((Vec3::INFINITY, Vec3::NEG_INFINITY), |(min, max), &p| {
                (min.min(p), max.max(p))
            });
        (max - min).max_element().max(0.01)
    }

    /// The operations' keys, outside of a move or a tool. True if one was
    /// pressed.
    pub(super) fn op_keys(&mut self, ui: &mut egui::Ui, pointer: egui::Pos2) -> bool {
        let none = egui::Modifiers::NONE;
        let command = egui::Modifiers::COMMAND;
        let shift = egui::Modifiers::SHIFT;
        let alt = egui::Modifiers::ALT;
        let edit = self.doc.as_ref().is_some_and(|doc| doc.mode == Mode::Edit);
        // More modifiers first: a plain key's shortcut takes Shift too.
        let (proportional, cursor_home) = ui.input_mut(|input| {
            (
                input.consume_key(none, egui::Key::O),
                input.consume_key(shift, egui::Key::C),
            )
        });
        if proportional {
            self.moves.proportional.on = !self.moves.proportional.on;
        }
        if cursor_home {
            self.moves.cursor = Vec3::ZERO;
        }
        if !edit {
            return proportional || cursor_home;
        }
        #[derive(PartialEq)]
        enum Key {
            None,
            BevelVertices,
            Bevel,
            LoopCut,
            Inside,
            Flip,
            Outside,
            Extrude,
            Inset,
            Knife,
            Merge,
            Delete,
            Fill,
        }
        let key = ui.input_mut(|input| {
            let both = command | shift;
            [
                (both, egui::Key::B, Key::BevelVertices),
                (command, egui::Key::B, Key::Bevel),
                (command, egui::Key::R, Key::LoopCut),
                (both, egui::Key::N, Key::Inside),
                (alt, egui::Key::N, Key::Flip),
                (shift, egui::Key::N, Key::Outside),
                (none, egui::Key::E, Key::Extrude),
                (none, egui::Key::I, Key::Inset),
                (none, egui::Key::K, Key::Knife),
                (none, egui::Key::M, Key::Merge),
                (none, egui::Key::X, Key::Delete),
                (none, egui::Key::Delete, Key::Delete),
                (none, egui::Key::F, Key::Fill),
            ]
            .into_iter()
            .find_map(|(modifiers, key, name)| input.consume_key(modifiers, key).then_some(name))
            .unwrap_or(Key::None)
        });
        match key {
            Key::None => return proportional || cursor_home,
            Key::Extrude => self.begin_value(
                MeshOp::Extrude {
                    mode: ExtrudeMode::Region,
                    distance: 0.0,
                },
                Value::Distance,
                pointer,
            ),
            Key::Inset => self.begin_value(
                MeshOp::Inset {
                    thickness: 0.0,
                    depth: 0.0,
                    individual: false,
                },
                Value::Thickness,
                pointer,
            ),
            Key::Bevel | Key::BevelVertices => self.begin_value(
                MeshOp::Bevel {
                    width: 0.0,
                    segments: 1,
                    profile: 0.5,
                    vertices: key == Key::BevelVertices,
                },
                Value::Width,
                pointer,
            ),
            Key::LoopCut => {
                self.tools.loop_cut = Some(LoopCutChoice::default());
            }
            Key::Knife => self.tools.knife = Some(Vec::new()),
            Key::Merge => self.tools.menu = Some((KeyMenu::Merge, pointer)),
            Key::Delete => self.tools.menu = Some((KeyMenu::Delete, pointer)),
            Key::Fill => {
                self.run_op(MeshOp::Fill);
            }
            Key::Outside | Key::Inside => {
                self.run_op(MeshOp::RecalculateNormals {
                    inside: key == Key::Inside,
                });
            }
            Key::Flip => {
                self.run_op(MeshOp::FlipNormals);
            }
        }
        true
    }

    /// Keys while a tool has the mouse. True if one does (the other keys
    /// wait).
    pub(super) fn tool_keys(&mut self, ui: &mut egui::Ui) -> bool {
        if !self.tools.busy() && self.tools.menu.is_none() {
            return false;
        }
        let none = egui::Modifiers::NONE;
        let (typed, back, enter, escape, wheel) = ui.input_mut(|input| {
            let typed: Vec<char> = input
                .events
                .iter()
                .filter_map(|event| match event {
                    egui::Event::Text(text) => Some(text.chars().collect::<Vec<_>>()),
                    _ => None,
                })
                .flatten()
                .collect();
            let wheel = input.raw_scroll_delta.y;
            input.raw_scroll_delta = egui::Vec2::ZERO;
            input.smooth_scroll_delta = egui::Vec2::ZERO;
            (
                typed,
                input.consume_key(none, egui::Key::Backspace),
                input.consume_key(none, egui::Key::Enter)
                    || input.consume_key(none, egui::Key::Space),
                input.consume_key(none, egui::Key::Escape),
                wheel,
            )
        });
        let steps = wheel.signum() as i32 * (wheel != 0.0) as i32;
        if self.tools.menu.is_some() {
            if escape {
                self.tools.menu = None;
            }
            return true;
        }
        if let Some(drag) = &mut self.tools.drag {
            for c in typed {
                if c.is_ascii_digit() || c == '.' || c == '-' {
                    drag.typed.push(c);
                }
            }
            if back {
                drag.typed.pop();
            }
            let last = self.doc.as_ref().and_then(|doc| doc.last_op.as_ref());
            if steps != 0
                && let Some(MeshOp::Bevel {
                    width,
                    segments,
                    profile,
                    vertices,
                }) = last.map(|last| last.op.clone())
            {
                self.adjust_op(MeshOp::Bevel {
                    width,
                    segments: (segments as i32 + steps).clamp(1, 64) as u32,
                    profile,
                    vertices,
                });
            }
            if enter {
                self.tools.drag = None;
            } else if escape {
                self.cancel_value();
            }
            return true;
        }
        if let Some(choice) = &mut self.tools.loop_cut {
            if steps != 0 {
                choice.cuts = (choice.cuts as i32 + steps).clamp(1, 64) as u32;
                // Shown again for the new count.
                choice.edge = None;
            }
            if escape {
                self.tools.loop_cut = None;
            }
            return true;
        }
        if let Some(points) = &mut self.tools.knife {
            if back {
                points.pop();
            }
            if escape {
                self.tools.knife = None;
            } else if enter {
                let points: Vec<KnifePoint> = self
                    .tools
                    .knife
                    .take()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(point, _)| point)
                    .collect();
                if points.len() >= 2 {
                    self.run_op(MeshOp::Knife { points });
                }
            }
            return true;
        }
        false
    }

    /// Takes back the operation a drag followed.
    fn cancel_value(&mut self) {
        self.tools.drag = None;
        if let Some(doc) = &mut self.doc {
            doc.cancel_last();
        }
    }

    /// The mouse in the view while a tool has it. True if one does.
    pub(super) fn handle_tools(&mut self, event: &Event) -> bool {
        if self.tools.drag.is_some() {
            if event.secondary_clicked {
                self.cancel_value();
                return true;
            }
            if let (Some(drag), Some(mouse)) = (&self.tools.drag, event.pointer) {
                let amount = drag.amount(&event.screen, mouse);
                let value = drag.value;
                let op = self
                    .doc
                    .as_ref()
                    .and_then(|doc| doc.last_op.as_ref())
                    .map(|last| with_value(&last.op, value, amount));
                if let Some(op) = op
                    && Some(&op)
                        != self
                            .doc
                            .as_ref()
                            .and_then(|d| d.last_op.as_ref())
                            .map(|l| &l.op)
                {
                    self.adjust_op(op);
                }
            }
            if event.clicked {
                self.tools.drag = None;
            }
            return true;
        }
        if let Some(choice) = &mut self.tools.loop_cut {
            if event.secondary_clicked {
                self.tools.loop_cut = None;
                return true;
            }
            let Some(mesh) = self.doc.as_ref().and_then(|doc| doc.mesh()) else {
                return true;
            };
            let hovered = event
                .pointer
                .and_then(|at| view::nearest_edge(mesh, &event.screen, at, 24.0))
                .map(|edge| {
                    let (a, b) = mesh.ends(edge);
                    [a, b]
                });
            if hovered != choice.edge {
                choice.edge = hovered;
                choice.lines = hovered
                    .and_then(|edge| loop_cut_lines(mesh, edge, choice.cuts))
                    .unwrap_or_default();
            }
            if event.clicked
                && let Some(edge) = choice.edge
            {
                let cuts = choice.cuts;
                self.tools.loop_cut = None;
                let at = event.click_at.unwrap_or_default();
                self.begin_value(
                    MeshOp::LoopCut {
                        edge,
                        cuts,
                        slide: 0.0,
                    },
                    Value::Slide,
                    at,
                );
            }
            return true;
        }
        if self.tools.knife.is_some() {
            if event.secondary_clicked {
                self.tools.knife = None;
                return true;
            }
            if let (Some(at), Some(mesh)) =
                (event.click_at, self.doc.as_ref().and_then(|doc| doc.mesh()))
                && let Some(point) = view::knife_point(mesh, &event.screen, at)
                && let Some(points) = &mut self.tools.knife
            {
                points.push(point);
            }
            return true;
        }
        false
    }

    /// What the tools draw over the view.
    pub(super) fn extras(&self) -> Extras {
        let mut extras = Extras {
            cursor: self.moves.cursor,
            hint: self.tools.status.clone(),
            ..Extras::default()
        };
        if let Some((modal, _)) = &self.modal
            && self.moves.proportional.on
        {
            extras.circle = Some((modal.center, self.moves.proportional.radius));
            extras.hint = Some(format!(
                "{} · proportional {} (wheel, Page Up/Down)",
                modal.describe(),
                self.moves.proportional.falloff.label()
            ));
        }
        if let Some(choice) = &self.tools.loop_cut {
            extras.lines = choice.lines.clone();
            extras.hint = Some(format!(
                "Loop Cut: {} cut{} · click an edge · wheel: more or fewer · Esc: cancel",
                choice.cuts,
                if choice.cuts == 1 { "" } else { "s" }
            ));
        }
        if let Some(points) = &self.tools.knife {
            extras.points = points.iter().map(|(_, p)| *p).collect();
            extras.lines = points
                .windows(2)
                .map(|pair| [pair[0].1, pair[1].1])
                .collect();
            extras.hint = Some(
                "Knife: click on edges and corners · Enter or Space: cut · Backspace: undo a point · Esc: cancel"
                    .to_owned(),
            );
        }
        if let Some(drag) = &self.tools.drag {
            let name = match drag.value {
                Value::Distance => "distance",
                Value::Thickness => "thickness",
                Value::Width => "width (wheel: segments)",
                Value::Factor | Value::Slide => "slide",
            };
            let typed = if drag.typed.is_empty() {
                String::new()
            } else {
                format!(" {}", drag.typed)
            };
            extras.hint = Some(format!(
                "Move the mouse to set the {name}{typed} · click or Enter to keep, right-click or Esc to cancel"
            ));
        }
        extras
    }

    /// The adjust panel and any key's menu, over the view at `rect`.
    pub(super) fn overlays(&mut self, ui: &mut egui::Ui, rect: egui::Rect) {
        self.key_menu(ui);
        if self.modal.is_some() || self.tools.busy() {
            return;
        }
        let Some(last) = self.doc.as_ref().and_then(|doc| doc.last_op.as_ref()) else {
            return;
        };
        let mut op = last.op.clone();
        let mut changed = false;
        egui::Area::new(ui.id().with("studio adjust"))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.left_bottom() + egui::vec2(8.0, -28.0))
            .pivot(egui::Align2::LEFT_BOTTOM)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_max_width(240.0);
                    egui::CollapsingHeader::new(op.label())
                        .id_salt("studio adjust header")
                        .default_open(true)
                        .show(ui, |ui| {
                            changed = settings_ui(ui, &mut op);
                        });
                });
            });
        if changed {
            self.adjust_op(op);
        }
    }

    /// The menu M, X or Delete opened.
    fn key_menu(&mut self, ui: &mut egui::Ui) {
        let Some((menu, at)) = self.tools.menu else {
            return;
        };
        let mut chosen = None;
        let response = egui::Area::new(ui.id().with("studio key menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(at)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    let cursor = self.moves.cursor;
                    let items: Vec<(&str, MeshOp)> = match menu {
                        KeyMenu::Merge => vec![
                            (
                                "At Center",
                                MeshOp::Merge {
                                    at: MergeAt::Center,
                                    cursor,
                                    distance: 0.0,
                                },
                            ),
                            (
                                "At Cursor",
                                MeshOp::Merge {
                                    at: MergeAt::Cursor,
                                    cursor,
                                    distance: 0.0,
                                },
                            ),
                            (
                                "By Distance",
                                MeshOp::Merge {
                                    at: MergeAt::Distance,
                                    cursor,
                                    distance: 0.0001,
                                },
                            ),
                        ],
                        KeyMenu::Delete => vec![
                            ("Delete Vertices", MeshOp::Delete(DeleteKind::Vertices)),
                            ("Delete Edges", MeshOp::Delete(DeleteKind::Edges)),
                            ("Delete Faces", MeshOp::Delete(DeleteKind::Faces)),
                            ("Delete Only Faces", MeshOp::Delete(DeleteKind::OnlyFaces)),
                            (
                                "Dissolve Vertices",
                                MeshOp::Dissolve(DissolveKind::Vertices),
                            ),
                            ("Dissolve Edges", MeshOp::Dissolve(DissolveKind::Edges)),
                            ("Dissolve Faces", MeshOp::Dissolve(DissolveKind::Faces)),
                        ],
                    };
                    widgets::section_label(
                        ui,
                        match menu {
                            KeyMenu::Merge => "Merge",
                            KeyMenu::Delete => "Delete",
                        },
                    );
                    for (name, op) in items {
                        if ui.button(name).clicked() {
                            chosen = Some(op);
                        }
                    }
                });
            })
            .response;
        if let Some(op) = chosen {
            self.tools.menu = None;
            self.run_op(op);
        } else if response.clicked_elsewhere() {
            self.tools.menu = None;
        }
    }

    /// The toolbar's Mesh menu: every operation, with first settings sized
    /// to the mesh, each explained on hover.
    pub(super) fn mesh_menu(&mut self, ui: &mut egui::Ui) {
        let edit = self.doc.as_ref().is_some_and(|doc| doc.mode == Mode::Edit);
        let size = self.mesh_size();
        let cursor = self.moves.cursor;
        let mut chosen: Option<MeshOp> = None;
        let mut start = None;
        // Shows `tip` on hover, enabled or not.
        let explain = |response: egui::Response, tip: &str| {
            response.on_hover_text(tip).on_disabled_hover_text(tip)
        };
        ui.menu_button(theme::with_icon(icons::WRENCH, "Mesh"), |ui| {
            if !edit {
                ui.weak(EDIT_ONLY);
            }
            ui.add_enabled_ui(edit, |ui| {
                let mut item = |ui: &mut egui::Ui, name: &str, tip: &str, op: MeshOp| {
                    if explain(ui.button(name), tip).clicked() {
                        chosen = Some(op);
                        ui.close();
                    }
                };
                for (name, tip, mode) in [
                    (
                        "Extrude Region (E)",
                        "Pull the picked faces out, with walls joining them to the rest. Picked open edges pull out into new faces.",
                        ExtrudeMode::Region,
                    ),
                    (
                        "Extrude Individual Faces",
                        "Pull each picked face out on its own, with walls of its own.",
                        ExtrudeMode::Individual,
                    ),
                    (
                        "Extrude Along Normals",
                        "Pull the picked faces out along their own normals, keeping an even thickness.",
                        ExtrudeMode::AlongNormals,
                    ),
                ] {
                    item(
                        ui,
                        name,
                        tip,
                        MeshOp::Extrude {
                            mode,
                            distance: size * 0.25,
                        },
                    );
                }
                item(
                    ui,
                    "Inset Faces (I)",
                    "A smaller copy inside the picked faces, joined to them by a ring of faces: panels, windows, rims.",
                    MeshOp::Inset {
                        thickness: size * 0.05,
                        depth: 0.0,
                        individual: false,
                    },
                );
                for (name, tip, vertices) in [
                    (
                        "Bevel Edges (Ctrl+B)",
                        "Round or cut off the picked edges with a band of new faces.",
                        false,
                    ),
                    (
                        "Bevel Vertices (Ctrl+Shift+B)",
                        "Cut the picked corners off.",
                        true,
                    ),
                ] {
                    item(
                        ui,
                        name,
                        tip,
                        MeshOp::Bevel {
                            width: size * 0.05,
                            segments: 1,
                            profile: 0.5,
                            vertices,
                        },
                    );
                }
                let tool = |ui: &mut egui::Ui, name: &str, tip: &str| explain(ui.button(name), tip).clicked();
                if tool(
                    ui,
                    "Loop Cut and Slide (Ctrl+R)",
                    "Hover an edge to see a loop of edges round the shape, click to cut it there, then slide it.",
                ) {
                    start = Some("loop cut");
                    ui.close();
                }
                if tool(
                    ui,
                    "Knife (K)",
                    "Click points on edges to cut new edges across the faces between them; Enter cuts, Esc stops.",
                ) {
                    start = Some("knife");
                    ui.close();
                }
                item(
                    ui,
                    "Edge Slide (G G)",
                    "Slide the picked edges along the faces beside them, keeping the shape.",
                    MeshOp::EdgeSlide { factor: 0.5 },
                );
                ui.separator();
                item(
                    ui,
                    "Bridge Edge Loops",
                    "Join two picked holes (loops of open edges) with a tube of faces.",
                    MeshOp::Bridge,
                );
                item(
                    ui,
                    "Fill (F)",
                    "Close each picked hole (a loop of open edges) with one face.",
                    MeshOp::Fill,
                );
                item(
                    ui,
                    "Grid Fill",
                    "Close one picked hole with a grid of four-sided faces (the loop needs an even number of edges).",
                    MeshOp::GridFill { span: 0, offset: 0 },
                );
                item(
                    ui,
                    "Subdivide",
                    "Split each picked face into smaller ones.",
                    MeshOp::Subdivide { cuts: 1 },
                );
                item(
                    ui,
                    "Spin",
                    "Sweep the picked faces or edges round the 3D cursor, making faces as they turn: bowls, vases, rings.",
                    MeshOp::Spin {
                        degrees: 90.0,
                        steps: 9,
                        axis: Vec3::Y,
                        center: cursor,
                    },
                );
                ui.separator();
                explain(
                    ui.menu_button("Merge (M)", |ui| {
                        for (name, tip, at) in [
                            ("At Center", "Join the picked vertices into one, at their middle.", MergeAt::Center),
                            ("At Cursor", "Join the picked vertices into one, at the 3D cursor.", MergeAt::Cursor),
                            (
                                "By Distance",
                                "Join picked vertices that sit on top of each other (cleans up doubles).",
                                MergeAt::Distance,
                            ),
                        ] {
                            item(
                                ui,
                                name,
                                tip,
                                MeshOp::Merge {
                                    at,
                                    cursor,
                                    distance: 0.0001,
                                },
                            );
                        }
                    })
                    .response,
                    "Join picked vertices into one.",
                );
                explain(
                    ui.menu_button("Delete (X)", |ui| {
                        for (name, tip, kind) in [
                            ("Vertices", "Remove the picked vertices and every edge and face using them.", DeleteKind::Vertices),
                            ("Edges", "Remove the picked edges and the faces beside them.", DeleteKind::Edges),
                            ("Faces", "Remove the picked faces, leaving a hole.", DeleteKind::Faces),
                            (
                                "Only Faces",
                                "Remove the picked faces but keep their edges and vertices.",
                                DeleteKind::OnlyFaces,
                            ),
                        ] {
                            item(ui, name, tip, MeshOp::Delete(kind));
                        }
                    })
                    .response,
                    "Remove what's picked, leaving a hole.",
                );
                explain(
                    ui.menu_button("Dissolve", |ui| {
                        for (name, tip, kind) in [
                            (
                                "Vertices",
                                "Remove the picked vertices; the faces round each become one.",
                                DissolveKind::Vertices,
                            ),
                            (
                                "Edges",
                                "Remove the picked edges; the faces either side become one.",
                                DissolveKind::Edges,
                            ),
                            ("Faces", "Join the picked faces into one.", DissolveKind::Faces),
                        ] {
                            item(ui, name, tip, MeshOp::Dissolve(kind));
                        }
                    })
                    .response,
                    "Remove what's picked without leaving a hole: the faces round it join up.",
                );
                ui.separator();
                item(
                    ui,
                    "Flip Normals (Alt+N)",
                    "Turn the picked faces inside out.",
                    MeshOp::FlipNormals,
                );
                item(
                    ui,
                    "Recalculate Outside (Shift+N)",
                    "Make the picked faces agree and face outwards (fixes faces that look dark or missing).",
                    MeshOp::RecalculateNormals { inside: false },
                );
                item(
                    ui,
                    "Recalculate Inside (Ctrl+Shift+N)",
                    "Make the picked faces agree and face inwards (for the inside of a room).",
                    MeshOp::RecalculateNormals { inside: true },
                );
            });
            ui.separator();
            // Shading works on whole objects in Object mode too.
            for (name, tip, op) in [
                (
                    "Shade Smooth",
                    "Light the faces as one smooth surface.",
                    MeshOp::ShadeSmooth,
                ),
                (
                    "Shade Flat",
                    "Light each face on its own, so the facets show.",
                    MeshOp::ShadeFlat,
                ),
                (
                    "Shade Auto Smooth",
                    "Smooth, except edges sharper than 30°, which stay crisp.",
                    MeshOp::AutoSmooth { degrees: 30.0 },
                ),
            ] {
                if explain(ui.button(name), tip).clicked() {
                    chosen = Some(op);
                    ui.close();
                }
            }
        });
        if let Some(op) = chosen {
            self.run_op(op);
        }
        match start {
            Some("loop cut") => self.tools.loop_cut = Some(LoopCutChoice::default()),
            Some(_) => self.tools.knife = Some(Vec::new()),
            None => {}
        }
    }

    /// The toolbar's pivot and proportional editing settings, and the 3D
    /// cursor's commands.
    pub(super) fn pivot_menu(&mut self, ui: &mut egui::Ui) {
        // Rig mode uses only the 3D cursor (Add Bone puts a bone there).
        let modeling = self
            .doc
            .as_ref()
            .is_some_and(|doc| matches!(doc.mode, Mode::Object | Mode::Edit));
        let proportional = &mut self.moves.proportional;
        if modeling
            && widgets::icon_button(
                ui,
                icons::CIRCLE_DASHED,
                "Proportional editing (O): what's near moves too",
                proportional.on,
            )
            .clicked()
        {
            proportional.on = !proportional.on;
        }
        let mut cursor_to_picked = false;
        let tip = if modeling {
            "Pivot, proportional editing and the 3D cursor"
        } else {
            "The 3D cursor, where Add Bone puts a bone"
        };
        ui.menu_button(icons::CROSSHAIR, |ui| {
            if modeling {
                widgets::section_label(ui, "Pivot");
                for pivot in Pivot::ALL {
                    ui.radio_value(&mut self.moves.pivot, pivot, pivot.label());
                }
                ui.separator();
                widgets::section_label(ui, "Proportional editing");
                let proportional = &mut self.moves.proportional;
                ui.checkbox(&mut proportional.on, "On (O)");
                egui::ComboBox::from_id_salt("studio falloff")
                    .selected_text(proportional.falloff.label())
                    .show_ui(ui, |ui| {
                        for falloff in Falloff::ALL {
                            ui.selectable_value(
                                &mut proportional.falloff,
                                falloff,
                                falloff.label(),
                            );
                        }
                    });
                ui.horizontal(|ui| {
                    ui.label("Radius");
                    ui.add(
                        egui::DragValue::new(&mut proportional.radius)
                            .speed(0.02)
                            .range(0.001..=10_000.0),
                    );
                });
                ui.separator();
            }
            widgets::section_label(ui, "3D cursor (Shift+right-click)");
            if ui.button("Cursor to Selected").clicked() {
                cursor_to_picked = true;
                ui.close();
            }
            if ui.button("Cursor to World Origin (Shift+C)").clicked() {
                self.moves.cursor = Vec3::ZERO;
                ui.close();
            }
        })
        .response
        .on_hover_text(tip);
        if cursor_to_picked && let Some((center, _)) = self.doc.as_ref().and_then(|doc| doc.pivot())
        {
            self.moves.cursor = center;
        }
    }
}

/// The loops a loop cut of `cuts` through `edge` would make, as lines.
fn loop_cut_lines(
    mesh: &axcenity_mesh_edit::EditMesh,
    edge: [u32; 2],
    cuts: u32,
) -> Option<Vec<[Vec3; 2]>> {
    let selection = axcenity_mesh_edit::Selection {
        mode: axcenity_mesh_edit::SelectMode::Edge,
        ..Default::default()
    };
    let (cut, picked) = apply(
        mesh,
        &selection,
        &MeshOp::LoopCut {
            edge,
            cuts,
            slide: 0.0,
        },
    )
    .ok()?;
    Some(
        picked
            .edges
            .iter()
            .map(|&e| {
                let (a, b) = cut.ends(e);
                [cut.position(a), cut.position(b)]
            })
            .collect(),
    )
}

/// A length setting, `min` or more. True if it changed.
fn length(ui: &mut egui::Ui, name: &str, value: &mut f32, min: f32) -> bool {
    ui.horizontal(|ui| {
        ui.label(name);
        ui.add(
            egui::DragValue::new(value)
                .speed(0.01)
                .range(min..=10_000.0),
        )
        .changed()
    })
    .inner
}

/// The adjust panel's settings for `op`. True if one changed.
pub(super) fn settings_ui(ui: &mut egui::Ui, op: &mut MeshOp) -> bool {
    let mut changed = false;
    match op {
        MeshOp::Extrude { mode, distance } => {
            changed |= length(ui, "Distance", distance, -10_000.0);
            for (option, name) in [
                (ExtrudeMode::Region, "Region"),
                (ExtrudeMode::Individual, "Individual faces"),
                (ExtrudeMode::AlongNormals, "Along normals"),
            ] {
                changed |= ui.radio_value(mode, option, name).changed();
            }
        }
        MeshOp::Inset {
            thickness,
            depth,
            individual,
        } => {
            changed |= length(ui, "Thickness", thickness, 0.0);
            changed |= length(ui, "Depth", depth, -10_000.0);
            changed |= ui.checkbox(individual, "Individual").changed();
        }
        MeshOp::Bevel {
            width,
            segments,
            profile,
            vertices,
        } => {
            changed |= length(ui, "Width", width, 0.0);
            ui.horizontal(|ui| {
                ui.label("Segments");
                changed |= ui
                    .add(egui::DragValue::new(segments).range(1..=64))
                    .changed();
            });
            ui.horizontal(|ui| {
                ui.label("Profile");
                changed |= ui.add(egui::Slider::new(profile, 0.0..=1.0)).changed();
            });
            changed |= ui.checkbox(vertices, "Vertices").changed();
        }
        MeshOp::LoopCut { cuts, slide, .. } => {
            ui.horizontal(|ui| {
                ui.label("Cuts");
                changed |= ui.add(egui::DragValue::new(cuts).range(1..=64)).changed();
            });
            ui.horizontal(|ui| {
                ui.label("Slide");
                changed |= ui.add(egui::Slider::new(slide, -0.99..=0.99)).changed();
            });
        }
        MeshOp::EdgeSlide { factor } => {
            ui.horizontal(|ui| {
                ui.label("Factor");
                changed |= ui.add(egui::Slider::new(factor, -1.0..=1.0)).changed();
            });
        }
        MeshOp::Merge { at, distance, .. } => {
            for (option, name) in [
                (MergeAt::Center, "At center"),
                (MergeAt::Cursor, "At cursor"),
                (MergeAt::Distance, "By distance"),
            ] {
                changed |= ui.radio_value(at, option, name).changed();
            }
            if *at == MergeAt::Distance {
                changed |= length(ui, "Distance", distance, 0.0);
            }
        }
        MeshOp::Delete(kind) => {
            for (option, name) in [
                (DeleteKind::Vertices, "Vertices"),
                (DeleteKind::Edges, "Edges"),
                (DeleteKind::Faces, "Faces"),
                (DeleteKind::OnlyFaces, "Only faces"),
            ] {
                changed |= ui.radio_value(kind, option, name).changed();
            }
        }
        MeshOp::Dissolve(kind) => {
            for (option, name) in [
                (DissolveKind::Vertices, "Vertices"),
                (DissolveKind::Edges, "Edges"),
                (DissolveKind::Faces, "Faces"),
            ] {
                changed |= ui.radio_value(kind, option, name).changed();
            }
        }
        MeshOp::GridFill { span, offset } => {
            ui.horizontal(|ui| {
                ui.label("Span (0: squarest)");
                changed |= ui.add(egui::DragValue::new(span).range(0..=256)).changed();
            });
            ui.horizontal(|ui| {
                ui.label("Offset");
                changed |= ui
                    .add(egui::DragValue::new(offset).range(0..=256))
                    .changed();
            });
        }
        MeshOp::Subdivide { cuts } => {
            ui.horizontal(|ui| {
                ui.label("Cuts");
                changed |= ui.add(egui::DragValue::new(cuts).range(1..=64)).changed();
            });
        }
        MeshOp::Spin {
            degrees,
            steps,
            axis,
            center,
        } => {
            ui.horizontal(|ui| {
                ui.label("Angle");
                changed |= ui
                    .add(
                        egui::DragValue::new(degrees)
                            .range(-360.0..=360.0)
                            .suffix("°"),
                    )
                    .changed();
            });
            ui.horizontal(|ui| {
                ui.label("Steps");
                changed |= ui
                    .add(egui::DragValue::new(steps).range(1..=1024))
                    .changed();
            });
            ui.horizontal(|ui| {
                ui.label("Axis");
                for (name, along) in [("X", Vec3::X), ("Y", Vec3::Y), ("Z", Vec3::Z)] {
                    if ui.selectable_label(*axis == along, name).clicked() {
                        *axis = along;
                        changed = true;
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label("Center");
                for value in [&mut center.x, &mut center.y, &mut center.z] {
                    changed |= ui.add(egui::DragValue::new(value).speed(0.02)).changed();
                }
            });
        }
        MeshOp::AutoSmooth { degrees } => {
            ui.horizontal(|ui| {
                ui.label("Angle");
                changed |= ui
                    .add(egui::Slider::new(degrees, 0.0..=180.0).suffix("°"))
                    .changed();
            });
        }
        MeshOp::Knife { points } => {
            ui.weak(format!("Cut through {} points", points.len()));
        }
        MeshOp::Bridge
        | MeshOp::Fill
        | MeshOp::FlipNormals
        | MeshOp::RecalculateNormals { .. }
        | MeshOp::ShadeSmooth
        | MeshOp::ShadeFlat => {
            ui.weak("No settings");
        }
    }
    changed
}
