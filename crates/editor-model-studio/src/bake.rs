//! Baking (Phase 61, back in R15) in the Model Studio: the Bake fold of
//! the side panel (Object and Edit mode), Make Low Poly and the Bake
//! button (which the app runs, since it writes files: `app/model_studio.rs`).
//!
//! - **Reference**: another object of the model (a sculpt), kept as a
//!   [`Reference`] (rebuilt when its mesh changes).
//! - **Make Low Poly**: the reference decimated to a number of triangles,
//!   evened out on it (relaxed), and unwrapped by Smart UV Project (at
//!   45°), as a new object after it, `_Low`.
//! - **Bake**: the reference onto the active object's uvs (normal, ambient
//!   occlusion, curvature and base color maps); the warnings stay under
//!   the button, and **Pick Overlapping** picks islands that overlap.
//!
//! Retopology by hand (Poly Build, snapping to the reference) stays out
//! (Phase 90): creators retopologise in Blender.

use std::sync::Arc;

use axcenity_mesh_edit::bake::{BakeReport, BakeSettings};
use axcenity_mesh_edit::paint::SIZES;
use axcenity_mesh_edit::{
    DocObject, EditMesh, Purpose, Reference, SelectMode, decimate_to, relax, uv,
};
use eframe::egui;

use super::{Mode, ModelStudio, StudioAction, StudioDoc};
use crate::responsive;
use crate::theme::{self, icons};
use crate::widgets::{self, Role};

/// Make Low Poly's Smart UV Project angle: tighter than the UV menu's 66°,
/// so fewer faces are squashed across a chart's edge, where a bake would
/// show a line.
const LOW_POLY_ANGLE: f32 = 45.0;
/// How many times Make Low Poly evens out the decimated mesh on the
/// reference ([`relax`]).
const RELAX_TIMES: usize = 3;

/// The Bake fold's state.
pub struct BakeState {
    /// Make Low Poly's triangles.
    pub target: usize,
    pub settings: BakeSettings,
    /// The reference's surface, and the key of the mesh it was made from.
    cache: Option<(u64, Arc<Reference>)>,
    /// The last bake's warnings, and the faces of the islands that overlap.
    pub report: Option<(Vec<String>, Vec<u32>)>,
    /// What the last action did or why it didn't.
    pub status: Option<String>,
}

impl Default for BakeState {
    fn default() -> Self {
        Self {
            target: 4000,
            settings: BakeSettings::default(),
            cache: None,
            report: None,
            status: None,
        }
    }
}

impl BakeState {
    /// Forgets the reference's surface and the last bake, for another
    /// model.
    pub(super) fn forget(&mut self) {
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
            skin: None,
            mask: Vec::new(),
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

/// The fold Make Low Poly and Bake sit in.
pub(super) const BAKE_HEADER: &str = "Bake";

impl ModelStudio {
    /// The reference's surface, made again if its mesh changed.
    pub fn reference_surface(&mut self) -> Option<Arc<Reference>> {
        let doc = self.doc.as_ref()?;
        let index = doc.reference_object()?;
        let mesh = doc.model.objects[index].evaluated(Purpose::Viewport)?;
        let key = mesh_key(&mesh) ^ index as u64;
        match &self.bake.cache {
            Some((cached, surface)) if *cached == key => Some(surface.clone()),
            _ => {
                let surface = Arc::new(Reference::new(&mesh));
                self.bake.cache = Some((key, surface.clone()));
                Some(surface)
            }
        }
    }

    /// Make Low Poly, with the uv settings' margin.
    pub fn make_low_poly(&mut self) {
        let (angle, margin) = (LOW_POLY_ANGLE, self.uv.margin);
        let target = self.bake.target;
        let Some(doc) = &mut self.doc else {
            return;
        };
        self.bake.status = Some(match doc.make_low_poly(target, angle, margin) {
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
    pub fn bake_done(&mut self, report: &BakeReport) {
        let faces = report
            .overlaps
            .iter()
            .flat_map(|overlap| overlap.faces.iter().flatten().copied())
            .collect::<std::collections::BTreeSet<u32>>()
            .into_iter()
            .collect();
        self.bake.status = Some(format!("Baked {} maps", report.files.len()));
        self.bake.report = Some((report.warnings(), faces));
    }

    /// Picks the faces of the islands the last bake found overlapping, in
    /// Edit mode.
    fn pick_overlapping(&mut self) {
        let (Some(doc), Some((_, faces))) = (&mut self.doc, &self.bake.report) else {
            return;
        };
        doc.set_mode(Mode::Edit);
        doc.set_select_mode(SelectMode::Face);
        let count = doc.mesh().map_or(0, EditMesh::face_count) as u32;
        doc.selection.faces = faces.iter().copied().filter(|&f| f < count).collect();
    }

    /// The Bake fold of the side panel, closed at first: a sculpt → low
    /// poly → bake pipeline few models need. It opens by itself while it's
    /// in use (a reference set, or a bake's report showing).
    pub(super) fn bake_panel(
        &mut self,
        ui: &mut egui::Ui,
        editable: bool,
        actions: &mut Vec<StudioAction>,
    ) {
        let Some(doc) = &self.doc else {
            return;
        };
        let in_use = doc.reference_object().is_some() || self.bake.report.is_some();
        ui.add_space(8.0);
        let mut header = egui::CollapsingHeader::new(BAKE_HEADER)
            .id_salt("studio bake")
            .default_open(false);
        if in_use {
            header = header.open(Some(true));
        }
        header
            .show(ui, |ui| self.bake_sections(ui, editable, actions))
            .header_response
            .on_hover_text(
                "Make a light copy of a detailed sculpt and bake the sculpt's detail into its textures",
            );
    }

    fn bake_sections(
        &mut self,
        ui: &mut egui::Ui,
        editable: bool,
        actions: &mut Vec<StudioAction>,
    ) {
        let Some(doc) = &mut self.doc else {
            return;
        };
        let palette = theme::palette(ui.ctx());
        widgets::section_label(ui, "Low poly");
        let names: Vec<String> = doc.model.objects.iter().map(|o| o.name.clone()).collect();
        let name = |index: Option<usize>| {
            index
                .and_then(|i| names.get(i))
                .map_or("None".to_owned(), Clone::clone)
        };
        let mut reference = doc.reference_object();
        let mut make_low = false;
        ui.add_enabled_ui(editable, |ui| {
            responsive::form(ui, "studio low poly", |ui| {
                ui.label("Reference").on_hover_text(
                    "The high-poly object (a sculpt) to make a low-poly copy of and bake from",
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
                    egui::DragValue::new(&mut self.bake.target)
                        .speed(20.0)
                        .range(12..=1_000_000),
                )
                .on_hover_text("How many triangles Make Low Poly cuts the reference to");
                ui.end_row();
            });
            responsive::toolbar(ui, |ui| {
                if ui
                    .add_enabled(reference.is_some(), egui::Button::new(theme::with_icon(icons::ARROWS_IN, "Make Low Poly")))
                    .on_hover_text("A copy of the reference with that many triangles, unwrapped (Smart UV Project)")
                    .clicked()
                {
                    make_low = true;
                }
            });
        });
        if reference != doc.reference_object() {
            doc.reference = reference;
            self.bake.cache = None;
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
        let settings = &mut self.bake.settings;
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
        if let Some(status) = &self.bake.status {
            ui.weak(status);
        }
        if let Some((warnings, faces)) = &self.bake.report {
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
