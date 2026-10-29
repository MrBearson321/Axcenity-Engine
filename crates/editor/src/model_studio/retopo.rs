//! Retopology and baking (Phase 61) in the Model Studio: the Retopology
//! and Bake sections of the side panel (Object and Edit mode), snapping to
//! the reference, the Poly Build tool, Make Low Poly, and the Bake button
//! (which the app runs, since it writes files: `app/model_studio.rs`).
//!
//! - **Reference**: another object of the model (a sculpt), kept as a
//!   [`Reference`] (rebuilt when its mesh changes). With **Snap to
//!   surface** on, Edit mode's moves (keys and handles) put what they move
//!   on it, and operations put the vertices they make or move on it.
//! - **Poly Build** (Edit mode): a click on the reference adds a point,
//!   and every fourth point makes a quad facing the camera; a click with
//!   an open edge of the active object under the mouse makes a quad out
//!   from that edge to the click. New corners weld to vertices near them.
//!   Each quad is an undo step; Backspace drops a point, Esc or a
//!   right-click ends the tool.
//! - **Make Low Poly**: the reference decimated to a number of triangles,
//!   evened out on it (relaxed), and unwrapped by Smart UV Project (at
//!   45°), as a new object after it, `_Low`.
//! - **Bake**: the reference onto the active object's uvs (normal, ambient
//!   occlusion, curvature and base color maps); the warnings stay under
//!   the button, and **Pick Overlapping** picks islands that overlap.

use std::sync::Arc;

use axcenity_mesh_edit::bake::{BakeReport, BakeSettings};
use axcenity_mesh_edit::paint::SIZES;
use axcenity_mesh_edit::{
    DocObject, EditMesh, PolyStep, Purpose, Reference, SelectMode, decimate_to, poly_build, relax,
    uv,
};
use axcenity_render::glam::Vec3;
use eframe::egui;

use super::view::{self, Event, Extras};
use super::{Mode, ModelStudio, StudioAction, StudioDoc};
use crate::responsive;
use crate::theme::{self, icons};
use crate::widgets::{self, Role};

/// How close (in points) the mouse must be to an open edge for Poly Build
/// to build out from it, and to a vertex to weld to it.
const EDGE_REACH: f32 = 24.0;
const WELD_REACH: f32 = 12.0;
/// Make Low Poly's Smart UV Project angle: tighter than the UV menu's 66°,
/// so fewer faces are squashed across a chart's edge, where a bake would
/// show a line.
const LOW_POLY_ANGLE: f32 = 45.0;
/// How many times Make Low Poly evens out the decimated mesh on the
/// reference ([`relax`]).
const RELAX_TIMES: usize = 3;

/// The Retopology and Bake sections' state.
pub struct RetopoState {
    /// Edit mode's moves and new vertices stick to the reference.
    pub snap: bool,
    /// Make Low Poly's triangles.
    pub target: usize,
    pub bake: BakeSettings,
    /// Poly Build is on, with the points clicked so far for a quad.
    pub poly_build: Option<Vec<Vec3>>,
    /// Under the mouse while Poly Build is on: an open edge, and the point
    /// on the reference.
    hover: (Option<u32>, Option<Vec3>),
    /// The reference's surface, and the key of the mesh it was made from.
    cache: Option<(u64, Arc<Reference>)>,
    /// The last bake's warnings, and the faces of the islands that overlap.
    pub report: Option<(Vec<String>, Vec<u32>)>,
    /// What the last action did or why it didn't.
    pub status: Option<String>,
}

impl Default for RetopoState {
    fn default() -> Self {
        Self {
            snap: true,
            target: 4000,
            bake: BakeSettings::default(),
            poly_build: None,
            hover: (None, None),
            cache: None,
            report: None,
            status: None,
        }
    }
}

impl RetopoState {
    /// Forgets the reference and the tool, for another model.
    pub(super) fn forget(&mut self) {
        self.poly_build = None;
        self.hover = (None, None);
        self.cache = None;
        self.report = None;
        self.status = None;
    }
}

/// A key for `mesh`'s shape: the same while its vertices and faces are.
fn mesh_key(mesh: &EditMesh) -> u64 {
    // FNV-1a over the positions' bits and the counts.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut add = |word: u32| {
        hash ^= u64::from(word);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    };
    add(mesh.face_count() as u32);
    add(mesh.half_edges().len() as u32);
    for p in mesh.positions() {
        for value in p.to_array() {
            add(value.to_bits());
        }
    }
    hash
}

impl StudioDoc {
    /// The reference object, if it's set and has a mesh.
    pub fn reference_object(&self) -> Option<usize> {
        let index = self.reference?;
        self.model.objects.get(index)?.mesh.as_ref()?;
        Some(index)
    }

    /// Adds a quad by Poly Build to the active object, as one undo step,
    /// and picks it.
    fn poly_build(&mut self, step: &PolyStep, weld: f32) -> Result<(), String> {
        let mesh = self.mesh().ok_or("Pick an object with a mesh")?;
        let surface = self.snap.clone();
        let snap = |p: Vec3| surface.as_ref().map_or(p, |s| s.snap(p));
        let (mesh, face) = poly_build(mesh, step, weld, &snap)?;
        let before = self.snapshot();
        self.model.objects[self.active].mesh = Some(mesh);
        self.selection.clear();
        self.selection.mode = SelectMode::Face;
        self.selection.faces.insert(face);
        self.record("Poly Build", before);
        Ok(())
    }

    /// The reference decimated to `triangles` and unwrapped, as a new
    /// object after it (`<name>_Low`) that becomes the active one: one undo
    /// step.
    pub fn make_low_poly(
        &mut self,
        triangles: usize,
        angle: f32,
        margin: f32,
    ) -> Result<(), String> {
        let index = self
            .reference_object()
            .ok_or("Set a reference (the high-poly object) first")?;
        let reference = &self.model.objects[index];
        let high = reference.evaluated(Purpose::Viewport).expect("a mesh");
        let mut low = decimate_to(&high, triangles);
        relax(&mut low, &Reference::new(&high), RELAX_TIMES);
        let faces: Vec<u32> = (0..low.face_count() as u32).collect();
        uv::smart_project(&mut low, &faces, angle, margin, 0)?;
        let object = DocObject {
            name: format!("{}_Low", reference.name),
            depth: reference.depth,
            mesh: Some(low),
            materials: reference.materials.clone(),
            uvs: true,
            modifiers: Vec::new(),
            paint: Vec::new(),
        };
        // After the reference and what's inside it.
        let at = (index + 1..self.model.objects.len())
            .find(|&k| self.model.objects[k].depth <= reference.depth)
            .unwrap_or(self.model.objects.len());
        let before = self.snapshot();
        self.model.objects.insert(at, object);
        self.active = at;
        self.objects = [at].into();
        self.selection.clear();
        if self.mode != Mode::Edit {
            self.mode = Mode::Object;
        }
        self.record("Make Low Poly", before);
        Ok(())
    }

    /// Bakes the reference onto the active object (see
    /// [`ModelDoc::bake`](axcenity_mesh_edit::ModelDoc::bake)) as one undo
    /// step, for the model saved at `model`.
    pub fn bake(&mut self, model: &str, settings: &BakeSettings) -> Result<BakeReport, String> {
        let high = self
            .reference_object()
            .ok_or("Set a reference (the high-poly object) to bake from")?;
        let before = self.snapshot();
        let report = self.model.bake(model, self.active, high, settings)?;
        let name = self.model.objects[self.active].name.clone();
        self.record(&format!("Bake {name}"), before);
        Ok(report)
    }
}

impl ModelStudio {
    /// The reference's surface, made again if its mesh changed.
    pub(crate) fn reference_surface(&mut self) -> Option<Arc<Reference>> {
        let doc = self.doc.as_ref()?;
        let index = doc.reference_object()?;
        let mesh = doc.model.objects[index].evaluated(Purpose::Viewport)?;
        let key = mesh_key(&mesh) ^ index as u64;
        match &self.retopo.cache {
            Some((cached, surface)) if *cached == key => Some(surface.clone()),
            _ => {
                let surface = Arc::new(Reference::new(&mesh));
                self.retopo.cache = Some((key, surface.clone()));
                Some(surface)
            }
        }
    }

    /// Sets what Edit mode's moves and operations snap to: the reference,
    /// while Snap to surface is on and another object is edited.
    pub(super) fn refresh_snap(&mut self) {
        let wanted = self.doc.as_ref().is_some_and(|doc| {
            self.retopo.snap
                && doc.mode == Mode::Edit
                && doc.reference_object().is_some_and(|r| r != doc.active)
        });
        let surface = if wanted {
            self.reference_surface()
        } else {
            None
        };
        if let Some(doc) = &mut self.doc {
            doc.snap = surface;
        }
    }

    /// A Poly Build click: at `point` on the reference, out from open edge
    /// `edge` if there's one, welding within `weld`, facing `facing` (the
    /// way to the camera). Every fourth free point makes a quad.
    pub(crate) fn poly_build_click(
        &mut self,
        point: Vec3,
        edge: Option<u32>,
        weld: f32,
        facing: Vec3,
    ) {
        self.refresh_snap();
        let Some(points) = &mut self.retopo.poly_build else {
            return;
        };
        let step = match edge {
            Some(edge) => {
                points.clear();
                PolyStep::FromEdge { edge, to: point }
            }
            None => {
                points.push(point);
                if points.len() < 4 {
                    return;
                }
                let points: [Vec3; 4] = std::mem::take(points).try_into().expect("four points");
                PolyStep::Quad { points, facing }
            }
        };
        let Some(doc) = &mut self.doc else {
            return;
        };
        self.retopo.status = doc.poly_build(&step, weld).err();
    }

    /// Poly Build's keys: Backspace drops a point, Esc ends the tool. True
    /// while the tool is on.
    pub(super) fn poly_build_keys(&mut self, ui: &mut egui::Ui) -> bool {
        let Some(points) = &mut self.retopo.poly_build else {
            return false;
        };
        let none = egui::Modifiers::NONE;
        let (back, escape) = ui.input_mut(|input| {
            (
                input.consume_key(none, egui::Key::Backspace),
                input.consume_key(none, egui::Key::Escape),
            )
        });
        if back {
            points.pop();
        }
        if escape {
            self.retopo.poly_build = None;
        }
        true
    }

    /// The mouse in the view while Poly Build is on. True if it is.
    pub(super) fn handle_poly_build(&mut self, event: &Event) -> bool {
        if self.retopo.poly_build.is_none() {
            return false;
        }
        if self.doc.as_ref().is_none_or(|doc| doc.mode != Mode::Edit) {
            self.retopo.poly_build = None;
            return false;
        }
        if event.secondary_clicked {
            self.retopo.poly_build = None;
            return true;
        }
        let surface = self.reference_surface();
        let Some(doc) = &self.doc else {
            return true;
        };
        let screen = &event.screen;
        let open_edge = |at: egui::Pos2| {
            let mesh = doc.mesh()?;
            let edge = view::nearest_edge(mesh, screen, at, EDGE_REACH)?;
            // The side without a twin, if either is.
            let half = mesh.half_edge(edge);
            match half.twin {
                None => Some(edge),
                Some(_) => None,
            }
        };
        let on_surface = |at: egui::Pos2| {
            let ray = screen.ray(at);
            let (hit, _) = surface.as_ref()?.ray(ray.origin, ray.direction)?;
            Some(hit.position)
        };
        self.retopo.hover = match event.pointer {
            Some(at) => (open_edge(at), on_surface(at)),
            None => (None, None),
        };
        if let Some(at) = event.click_at {
            match on_surface(at) {
                Some(point) => {
                    let weld = screen.studs_per_point(point) * WELD_REACH;
                    let facing = -screen.ray(at).direction;
                    let edge = open_edge(at);
                    self.poly_build_click(point, edge, weld, facing);
                }
                None if surface.is_none() => {
                    self.retopo.status = Some("Set a reference to build on".to_owned());
                }
                None => {
                    self.retopo.status = Some("Click on the reference".to_owned());
                }
            }
        }
        true
    }

    /// What Poly Build draws over the view: its points, the open edge
    /// under the mouse and the quad a click would make from it.
    pub(super) fn retopo_extras(&self, extras: &mut Extras) {
        let Some(points) = &self.retopo.poly_build else {
            return;
        };
        extras.points = points.clone();
        extras.lines = points.windows(2).map(|pair| [pair[0], pair[1]]).collect();
        let (edge, at) = self.retopo.hover;
        if let (Some(edge), Some(at), Some(mesh)) =
            (edge, at, self.doc.as_ref().and_then(|doc| doc.mesh()))
        {
            let (a, b) = mesh.ends(edge);
            let (a, b) = (mesh.position(a), mesh.position(b));
            let half = (b - a) / 2.0;
            let (near_a, near_b) = (at - half, at + half);
            extras
                .lines
                .extend([[a, b], [a, near_a], [near_a, near_b], [near_b, b]]);
        } else if let (Some(at), Some(last)) = (at, points.last()) {
            extras.lines.push([*last, at]);
        }
        extras.hint = Some(self.retopo.status.clone().unwrap_or_else(|| {
            format!(
                "Poly Build: click on the reference ({} of 4 points) · near an open edge: a quad out from it · Backspace: drop a point · Esc: done",
                points.len()
            )
        }));
    }

    /// Make Low Poly, with the uv settings' margin.
    pub(crate) fn make_low_poly(&mut self) {
        let (angle, margin) = (LOW_POLY_ANGLE, self.uv.margin);
        let target = self.retopo.target;
        let Some(doc) = &mut self.doc else {
            return;
        };
        self.retopo.status = Some(match doc.make_low_poly(target, angle, margin) {
            Ok(()) => {
                let object = &doc.model.objects[doc.active];
                let triangles: usize = object.mesh.as_ref().map_or(0, |mesh| {
                    (0..mesh.face_count() as u32)
                        .map(|f| mesh.triangles(f).len())
                        .sum()
                });
                format!("Made {} ({triangles} triangles)", object.name)
            }
            Err(why) => why,
        });
    }

    /// Keeps a bake's report to show under the button.
    pub(crate) fn bake_done(&mut self, report: &BakeReport) {
        let faces = report
            .overlaps
            .iter()
            .flat_map(|overlap| overlap.faces.iter().flatten().copied())
            .collect::<std::collections::BTreeSet<u32>>()
            .into_iter()
            .collect();
        self.retopo.status = Some(format!("Baked {} maps", report.files.len()));
        self.retopo.report = Some((report.warnings(), faces));
    }

    /// Picks the faces of the islands the last bake found overlapping, in
    /// Edit mode.
    fn pick_overlapping(&mut self) {
        let (Some(doc), Some((_, faces))) = (&mut self.doc, &self.retopo.report) else {
            return;
        };
        doc.set_mode(Mode::Edit);
        doc.set_select_mode(SelectMode::Face);
        let count = doc.mesh().map_or(0, EditMesh::face_count) as u32;
        doc.selection.faces = faces.iter().copied().filter(|&f| f < count).collect();
    }

    /// The Retopology and Bake sections of the side panel.
    pub(super) fn retopo_panel(
        &mut self,
        ui: &mut egui::Ui,
        editable: bool,
        actions: &mut Vec<StudioAction>,
    ) {
        let Some(doc) = &mut self.doc else {
            return;
        };
        let palette = theme::palette(ui.ctx());
        ui.add_space(8.0);
        widgets::section_label(ui, "Retopology");
        let names: Vec<String> = doc.model.objects.iter().map(|o| o.name.clone()).collect();
        let name = |index: Option<usize>| {
            index
                .and_then(|i| names.get(i))
                .map_or("None".to_owned(), Clone::clone)
        };
        let mut reference = doc.reference_object();
        let mut make_low = false;
        let mut enter_edit = false;
        ui.add_enabled_ui(editable, |ui| {
            responsive::form(ui, "studio retopo", |ui| {
                ui.label("Reference").on_hover_text(
                    "The high-poly object (a sculpt) to build on, snap to and bake from",
                );
                egui::ComboBox::from_id_salt("studio reference")
                    .selected_text(name(reference))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut reference, None, "None");
                        for (index, object) in doc.model.objects.iter().enumerate() {
                            if object.mesh.is_some() {
                                ui.selectable_value(&mut reference, Some(index), &object.name);
                            }
                        }
                    });
                ui.end_row();
                ui.label("Triangles");
                ui.add(
                    egui::DragValue::new(&mut self.retopo.target)
                        .speed(20.0)
                        .range(12..=1_000_000),
                )
                .on_hover_text("How many triangles Make Low Poly cuts the reference to");
                ui.end_row();
            });
            responsive::toolbar(ui, |ui| {
                ui.toggle_value(&mut self.retopo.snap, theme::with_icon(icons::MAGNET, "Snap to surface"))
                    .on_hover_text(
                        "In Edit mode, what moves and what operations make sticks to the reference",
                    );
                let mut building = self.retopo.poly_build.is_some();
                if ui
                    .add_enabled(
                        reference.is_some(),
                        egui::Button::selectable(
                            building,
                            theme::with_icon(icons::POLYGON, "Poly Build"),
                        ),
                    )
                    .on_hover_text(
                        "Click four points on the reference for a quad, or near an open edge for a quad out from it",
                    )
                    .clicked()
                {
                    building = !building;
                    self.retopo.poly_build = building.then(Vec::new);
                    enter_edit = building;
                }
                if ui
                    .add_enabled(reference.is_some(), egui::Button::new(theme::with_icon(icons::ARROWS_IN, "Make Low Poly")))
                    .on_hover_text("A copy of the reference with that many triangles, unwrapped (Smart UV Project)")
                    .clicked()
                {
                    make_low = true;
                }
            });
        });
        if enter_edit {
            doc.set_mode(Mode::Edit);
        }
        if reference != doc.reference_object() {
            doc.reference = reference;
            self.retopo.cache = None;
        }

        ui.add_space(8.0);
        widgets::section_label(ui, "Bake");
        let target = &doc.model.objects[doc.active];
        let can_bake = reference.is_some_and(|r| r != doc.active) && target.mesh.is_some();
        if can_bake {
            ui.weak(format!("{} onto {}'s uvs", name(reference), target.name));
        } else {
            ui.weak("Set a reference, and pick the object to bake onto.");
        }
        let settings = &mut self.retopo.bake;
        let mut pick = false;
        ui.add_enabled_ui(editable, |ui| {
            responsive::form(ui, "studio bake", |ui| {
                ui.label("Size");
                egui::ComboBox::from_id_salt("studio bake size")
                    .selected_text(settings.size.to_string())
                    .show_ui(ui, |ui| {
                        for size in SIZES {
                            ui.selectable_value(&mut settings.size, size, size.to_string());
                        }
                    });
                ui.end_row();
                ui.label("Cage").on_hover_text(
                    "How far out from the surface rays start, back in to the reference",
                );
                optional_length(ui, &mut settings.cage, "studio bake cage");
                ui.end_row();
                ui.label("Occlusion");
                ui.horizontal(|ui| {
                    ui.add(
                        egui::DragValue::new(&mut settings.occlusion_rays)
                            .range(1..=256)
                            .suffix(" rays"),
                    );
                    optional_length(ui, &mut settings.occlusion_distance, "studio bake reach");
                });
                ui.end_row();
                ui.label("Maps");
                ui.horizontal_wrapped(|ui| {
                    ui.toggle_value(&mut settings.normal, "Normal");
                    ui.toggle_value(&mut settings.occlusion, "Occlusion");
                    ui.toggle_value(&mut settings.curvature, "Curvature");
                    ui.toggle_value(&mut settings.base_color, "Base color");
                });
                ui.end_row();
            });
            ui.checkbox(&mut settings.flip_green, "DirectX normal map (green down)")
                .on_hover_text("Saved as _NormalDX.png, which the engine reads as one");
            if ui
                .add_enabled(
                    can_bake,
                    widgets::button(Role::Primary, Some(icons::LIGHTNING), "Bake", &palette),
                )
                .on_hover_text("Writes the maps as PNGs in textures/ beside the model, and points the material at them")
                .clicked()
            {
                actions.push(StudioAction::Bake);
            }
        });
        if let Some(status) = &self.retopo.status {
            ui.weak(status);
        }
        if let Some((warnings, faces)) = &self.retopo.report {
            for warning in warnings {
                ui.colored_label(palette.warning, warning);
            }
            if !faces.is_empty() && ui.button("Pick Overlapping").clicked() {
                pick = true;
            }
        }
        if make_low {
            self.make_low_poly();
        }
        if pick {
            self.pick_overlapping();
        }
    }
}

/// A length that's worked out by itself (Auto) or set.
fn optional_length(ui: &mut egui::Ui, value: &mut Option<f32>, id: &str) {
    ui.push_id(id, |ui| {
        let mut auto = value.is_none();
        if ui.toggle_value(&mut auto, "Auto").changed() {
            *value = if auto { None } else { Some(0.05) };
        }
        if let Some(length) = value {
            ui.add(
                egui::DragValue::new(length)
                    .speed(0.005)
                    .range(0.0001..=1000.0)
                    .suffix(" studs"),
            );
        }
    });
}
