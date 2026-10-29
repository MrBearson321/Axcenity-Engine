//! Sculpt mode (Phase 60): brushes that push the active object's surface
//! about in the 3D view, its mask, and more detail to sculpt.
//!
//! The active object's mesh is copied into a [`Sculpt`] (kept until the
//! mesh changes some other way), which the brushes move. The view draws
//! the sculpt's chunks as meshes of their own, in place of the object: a
//! dab marks the chunks it touched, and only those are made again, with
//! the same key and a new version, so the renderer writes their corners
//! over the old ones on the GPU. Masked triangles are drawn darker.
//!
//! Each stroke is one undo step ("Sculpt Stroke"); the moved vertices go
//! back into the mesh when it ends. Ctrl does the brush's other way and
//! Shift smooths; `[` and `]` change the radius; a pen's pressure (where
//! the platform reports it) changes the radius or strength. Grab and Snake
//! Hook drag on the plane facing the camera through where they took hold.
//! The mask isn't an undo step and isn't saved; Hide Masked hides the
//! masked triangles until Show All. Voxel Remesh and Subdivide replace the
//! mesh, as an undo step each.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use axcenity_mesh_edit::paint::BrushFalloff;
use axcenity_mesh_edit::sculpt::{Sculpt, SculptBrush, SculptDab, SculptTool};
use axcenity_mesh_edit::{subdivide, voxel_remesh};
use axcenity_render::glam::{Mat4, Vec3};
use axcenity_render::{DrawItem, Material, MeshSource, Shape};
use eframe::egui;

use super::{Mode, ModelStudio, Snapshot, StudioDoc};
use crate::screen::Screen;
use crate::theme::{self, icons};
use crate::{responsive, widgets};

/// Sculpt chunks' mesh keys, apart from the models' own.
static NEXT_KEY: AtomicU64 = AtomicU64::new(1 << 62);
/// Versions start at 1: a mesh at version 0 is never rewritten.
static NEXT_VERSION: AtomicU64 = AtomicU64::new(1);

/// Clay, and the darker grey masked parts are drawn in.
const CLAY: [f32; 3] = [0.72, 0.68, 0.62];
const MASKED: [f32; 3] = [0.22, 0.22, 0.25];

/// Sculpt mode's state.
pub struct SculptState {
    pub brush: SculptBrush,
    /// Voxel Remesh's voxel size, in studs.
    pub voxel_size: f32,
    cache: Option<SculptCache>,
    stroke: Option<Stroke>,
    /// The pen's pressure while it's down, if the platform reports it.
    pressure: Option<f32>,
    /// What the last action did or why it didn't, for the hint.
    pub status: Option<String>,
}

impl Default for SculptState {
    fn default() -> Self {
        Self {
            brush: SculptBrush::default(),
            voxel_size: 0.05,
            cache: None,
            stroke: None,
            pressure: None,
            status: None,
        }
    }
}

/// The active object's mesh being sculpted, and the meshes drawn for it.
struct SculptCache {
    object: usize,
    /// The document's revision the sculpt matches.
    revision: u64,
    sculpt: Sculpt,
    /// Each chunk's meshes: its triangles not masked, and masked.
    meshes: Vec<[Option<Arc<ChunkMesh>>; 2]>,
}

/// A stroke under way.
struct Stroke {
    /// The undo step's state (none for the Mask brush, which isn't one).
    before: Option<Snapshot>,
    tool: SculptTool,
    invert: bool,
    /// Where the last dab was on the view, and in the object.
    last: Option<(egui::Pos2, Vec3)>,
    /// How far the mouse has gone since the last dab, in points.
    travelled: f32,
    /// Grab and Snake Hook: the plane they drag on (a point and its
    /// normal), and the radius they took hold with.
    plane: Option<(Vec3, Vec3, f32)>,
}

/// One chunk's triangles, drawn as a mesh.
struct ChunkMesh {
    key: u64,
    version: u64,
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    indices: Vec<u32>,
}

impl MeshSource for ChunkMesh {
    fn key(&self) -> u64 {
        self.key
    }
    fn positions(&self) -> &[[f32; 3]] {
        &self.positions
    }
    fn normals(&self) -> &[[f32; 3]] {
        &self.normals
    }
    fn uvs(&self) -> &[[f32; 2]] {
        &[]
    }
    fn indices(&self) -> &[u32] {
        &self.indices
    }
    fn version(&self) -> u64 {
        self.version
    }
}

impl SculptState {
    /// True while a stroke is under way.
    pub fn busy(&self) -> bool {
        self.stroke.is_some()
    }

    /// Forgets the sculpt, for another model.
    pub(super) fn forget(&mut self) {
        self.cache = None;
        self.stroke = None;
        self.status = None;
    }

    /// The radius and strength a dab has now, with the pen's pressure.
    fn radius_and_strength(&self) -> (f32, f32) {
        let pressure = self.pressure.unwrap_or(1.0);
        let brush = &self.brush;
        let radius = brush.radius * if brush.pressure_radius { pressure } else { 1.0 };
        let strength = brush.strength
            * if brush.pressure_strength {
                pressure
            } else {
                1.0
            };
        (radius.max(0.5), strength)
    }

    /// The pen's pressure from this frame's touches (kept while it's down).
    fn read_pressure(&mut self, ui: &egui::Ui) {
        ui.input(|input| {
            for event in &input.events {
                if let egui::Event::Touch { phase, force, .. } = event {
                    self.pressure = match phase {
                        egui::TouchPhase::End | egui::TouchPhase::Cancel => None,
                        _ => force.map(|force| force.clamp(0.05, 1.0)),
                    };
                }
            }
        });
    }

    /// The meshes of the chunks a dab (or the mask) changed, made again.
    fn refresh_meshes(cache: &mut SculptCache) {
        for chunk in cache.sculpt.take_dirty_chunks() {
            let (positions, normals) = cache.sculpt.chunk_corners(chunk);
            let lists = {
                let chunk = &cache.sculpt.chunks()[chunk];
                [&chunk.clay, &chunk.masked]
            };
            for (kind, indices) in lists.into_iter().enumerate() {
                let old = &cache.meshes[chunk][kind];
                cache.meshes[chunk][kind] = (!indices.is_empty()).then(|| {
                    // The GPU's copy keeps its triangles and takes new
                    // corners; different triangles need a new key.
                    let key = match old {
                        Some(old) if old.indices == *indices => old.key,
                        _ => NEXT_KEY.fetch_add(1, Ordering::Relaxed),
                    };
                    Arc::new(ChunkMesh {
                        key,
                        version: NEXT_VERSION.fetch_add(1, Ordering::Relaxed),
                        positions: positions.clone(),
                        normals: normals.clone(),
                        indices: indices.clone(),
                    })
                });
            }
        }
    }
}

/// The sculpt of `doc`'s active object in `cache`, made again if the mesh
/// changed some other way since (an undo, an operation). The mask stays if
/// the vertices are the same ones.
fn sculpt_of<'a>(
    cache: &'a mut Option<SculptCache>,
    doc: &StudioDoc,
) -> Option<&'a mut SculptCache> {
    let mesh = doc.mesh()?;
    let fresh = cache
        .as_ref()
        .is_some_and(|cache| cache.object == doc.active && cache.revision == doc.revision);
    if !fresh {
        let mut sculpt = Sculpt::new(mesh);
        if let Some(old) = cache.as_ref()
            && old.object == doc.active
        {
            sculpt.set_mask(old.sculpt.mask());
        }
        *cache = Some(SculptCache {
            object: doc.active,
            revision: doc.revision,
            meshes: vec![[None, None]; sculpt.chunks().len()],
            sculpt,
        });
    }
    cache.as_mut()
}

impl StudioDoc {
    /// Puts `mesh` in place of the active object's, as one undo step.
    fn replace_mesh(&mut self, label: &str, mesh: axcenity_mesh_edit::EditMesh) {
        let before = self.snapshot();
        let Some(object) = self.model.objects.get_mut(self.active) else {
            return;
        };
        object.mesh = Some(mesh);
        self.selection.clear();
        self.record(label, before);
    }
}

impl ModelStudio {
    /// What the view draws in place of the active object while sculpting:
    /// its chunks. `None` outside Sculpt mode.
    pub(super) fn sculpt_items(&mut self) -> Option<Vec<DrawItem>> {
        let doc = self.doc.as_ref()?;
        if doc.mode != Mode::Sculpt {
            return None;
        }
        let cache = sculpt_of(&mut self.sculpt.cache, doc)?;
        SculptState::refresh_meshes(cache);
        let material = |color| Material {
            base_color: color,
            roughness: 0.55,
            ..Material::default()
        };
        let items = cache
            .meshes
            .iter()
            .flatten()
            .zip([CLAY, MASKED].into_iter().cycle())
            .filter_map(|(mesh, color)| {
                let mesh: Arc<dyn MeshSource> = mesh.clone()?;
                Some(DrawItem {
                    shape: Shape::Mesh(mesh),
                    // The corners are in the object's own space already.
                    transform: Mat4::IDENTITY,
                    material: material(color),
                    uv_tile: 0.0,
                    casts_shadow: true,
                    id: 0,
                    cull_distance: 0.0,
                    skin: None,
                })
            })
            .collect();
        Some(items)
    }

    /// Starts a stroke, if the active object has a mesh. `invert` is the
    /// brush's other way; `smooth` smooths whatever the brush is.
    fn begin_sculpt(&mut self, invert: bool, smooth: bool) -> bool {
        let Some(doc) = &self.doc else {
            return false;
        };
        if doc.mode != Mode::Sculpt {
            return false;
        }
        let tool = if smooth {
            SculptTool::Smooth
        } else {
            self.sculpt.brush.tool
        };
        let before = (tool != SculptTool::Mask).then(|| doc.snapshot());
        let Some(cache) = sculpt_of(&mut self.sculpt.cache, doc) else {
            return false;
        };
        cache.sculpt.begin_stroke();
        self.sculpt.status = None;
        self.sculpt.stroke = Some(Stroke {
            before,
            tool,
            invert,
            last: None,
            travelled: 0.0,
            plane: None,
        });
        true
    }

    /// A dab at `at` on the view, and its mirrored copies.
    fn sculpt_dab(&mut self, screen: &Screen, at: egui::Pos2) {
        let Some(doc) = &self.doc else {
            return;
        };
        let (radius_points, strength) = self.sculpt.radius_and_strength();
        let (falloff, symmetry): (BrushFalloff, [bool; 3]) =
            (self.sculpt.brush.falloff, self.sculpt.brush.symmetry);
        let Some(cache) = sculpt_of(&mut self.sculpt.cache, doc) else {
            return;
        };
        let Some(stroke) = &mut self.sculpt.stroke else {
            return;
        };
        let ray = screen.ray(at);
        let drags = stroke.tool.drags();
        let (center, normal, radius) = match stroke.plane {
            Some((point, normal, radius)) if drags => {
                let across = ray.direction.dot(normal);
                if across.abs() < 1e-6 {
                    return;
                }
                let t = (point - ray.origin).dot(normal) / across;
                (ray.origin + ray.direction * t, normal, radius)
            }
            _ => {
                let Some((triangle, t)) = cache.sculpt.ray(ray.origin, ray.direction) else {
                    return;
                };
                let center = ray.origin + ray.direction * t;
                let mut normal = cache.sculpt.triangle_normal(triangle);
                if normal.dot(ray.direction) > 0.0 {
                    normal = -normal;
                }
                let radius = radius_points * screen.studs_per_point(center);
                if drags {
                    stroke.plane = Some((center, -ray.direction, radius));
                }
                (center, normal, radius)
            }
        };
        let dab = SculptDab {
            center,
            normal,
            radius,
            strength,
            motion: stroke.last.map_or(Vec3::ZERO, |(_, last)| center - last),
            invert: stroke.invert,
        };
        cache.sculpt.dab(stroke.tool, falloff, symmetry, &dab);
        stroke.last = Some((at, center));
    }

    /// Dabs from the last one toward `at`, one each spacing along the way
    /// (Grab and Snake Hook follow the mouse).
    fn sculpt_drag_to(&mut self, screen: &Screen, at: egui::Pos2) {
        let Some(stroke) = &self.sculpt.stroke else {
            return;
        };
        let Some((from, _)) = stroke.last else {
            self.sculpt_dab(screen, at);
            return;
        };
        if stroke.tool.drags() {
            if from != at {
                self.sculpt_dab(screen, at);
            }
            return;
        }
        let step = (self.sculpt.brush.spacing * self.sculpt.radius_and_strength().0).max(1.0);
        let distance = from.distance(at);
        let travelled = stroke.travelled;
        let mut along = step - travelled;
        while along <= distance {
            self.sculpt_dab(screen, from + (at - from) * (along / distance));
            along += step;
        }
        if let Some(stroke) = &mut self.sculpt.stroke {
            stroke.travelled = (travelled + distance) % step;
            // Later dabs measure from where the mouse is.
            if let Some(last) = &mut stroke.last {
                last.0 = at;
            }
        }
    }

    /// Ends the stroke: the moved vertices go into the mesh, as one undo
    /// step.
    fn end_sculpt(&mut self) {
        let Some(stroke) = self.sculpt.stroke.take() else {
            return;
        };
        self.sculpt.pressure = None;
        let (Some(doc), Some(cache)) = (&mut self.doc, &mut self.sculpt.cache) else {
            return;
        };
        let Some(before) = stroke.before else {
            return;
        };
        let Some(mesh) = doc
            .model
            .objects
            .get_mut(cache.object)
            .and_then(|object| object.mesh.as_mut())
        else {
            return;
        };
        if cache.sculpt.write_back(mesh) {
            doc.record("Sculpt Stroke", before);
            // The sculpt is the mesh as it is now.
            cache.revision = doc.revision;
        }
    }

    /// A whole stroke through `points` in the 3D view, as dragging the
    /// mouse along them sculpts it.
    #[cfg(test)]
    pub(crate) fn sculpt_stroke(
        &mut self,
        screen: &Screen,
        points: &[egui::Pos2],
        invert: bool,
    ) -> bool {
        if !self.begin_sculpt(invert, false) {
            return false;
        }
        for &at in points {
            self.sculpt_drag_to(screen, at);
        }
        self.end_sculpt();
        true
    }

    /// The active object's sculpt, for tests.
    #[cfg(test)]
    pub(crate) fn sculpt_of(&mut self) -> Option<&Sculpt> {
        let doc = self.doc.as_ref()?;
        Some(&sculpt_of(&mut self.sculpt.cache, doc)?.sculpt)
    }

    /// What the mouse does in the 3D view in Sculpt mode: strokes (Ctrl
    /// the other way, Shift smooths). Draws the brush.
    pub(super) fn sculpt_in_view(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        screen: &Screen,
    ) {
        self.sculpt.read_pressure(ui);
        let (pointer, modifiers, pressed, down) = ui.input(|input| {
            (
                input.pointer.latest_pos(),
                input.modifiers,
                input.pointer.primary_pressed(),
                input.pointer.primary_down(),
            )
        });
        let over =
            pointer.is_some_and(|at| response.rect.contains(at)) && response.contains_pointer();
        if pressed && over && self.sculpt.stroke.is_none() {
            if self.begin_sculpt(modifiers.command, modifiers.shift)
                && let Some(at) = pointer
            {
                self.sculpt_dab(screen, at);
            }
        } else if self.sculpt.stroke.is_some() {
            if let Some(at) = pointer.filter(|_| down) {
                self.sculpt_drag_to(screen, at);
            }
            if !down {
                self.end_sculpt();
            }
        }
        if let Some(at) = pointer.filter(|_| over) {
            let (radius, _) = self.sculpt.radius_and_strength();
            let accent = theme::palette(ui.ctx()).accent;
            let painter = ui.painter_at(response.rect);
            painter.circle_stroke(at, radius, egui::Stroke::new(1.5_f32, egui::Color32::WHITE));
            painter.circle_stroke(at, radius, egui::Stroke::new(1.0_f32, accent));
        }
    }

    /// `[` and `]` change the brush's radius.
    pub(super) fn sculpt_keys(&mut self, ui: &mut egui::Ui) {
        let none = egui::Modifiers::NONE;
        let (smaller, bigger) = ui.input_mut(|input| {
            (
                input.consume_key(none, egui::Key::OpenBracket),
                input.consume_key(none, egui::Key::CloseBracket),
            )
        });
        let brush = &mut self.sculpt.brush;
        if smaller {
            brush.radius = (brush.radius / 1.15).max(1.0);
        }
        if bigger {
            brush.radius = (brush.radius * 1.15).min(1000.0);
        }
    }

    /// Voxel Remesh (`remesh`) or Subdivide on the active object, as one
    /// undo step.
    pub(crate) fn sculpt_detail(&mut self, remesh: bool) {
        let Some(doc) = &mut self.doc else {
            return;
        };
        let Some(mesh) = doc.mesh() else {
            return;
        };
        let (label, result) = if remesh {
            ("Voxel Remesh", voxel_remesh(mesh, self.sculpt.voxel_size))
        } else {
            ("Subdivide", subdivide(mesh))
        };
        self.sculpt.status = Some(match result {
            Ok(mesh) => {
                let faces = mesh.face_count();
                doc.replace_mesh(label, mesh);
                format!("{label}: {faces} faces")
            }
            Err(why) => why,
        });
    }

    /// A mask action (not an undo step): `change` on the sculpt.
    pub(crate) fn change_mask(&mut self, change: impl FnOnce(&mut Sculpt)) {
        let Some(doc) = &self.doc else {
            return;
        };
        if let Some(cache) = sculpt_of(&mut self.sculpt.cache, doc) {
            change(&mut cache.sculpt);
        }
    }

    /// The Sculpt panel: the brush, the mask and the detail.
    pub(super) fn sculpt_panel(&mut self, ui: &mut egui::Ui, editable: bool) {
        widgets::section_label(ui, "Sculpt");
        let Some(doc) = &self.doc else {
            return;
        };
        let Some(mesh) = doc.mesh() else {
            ui.weak("Pick an object with a mesh to sculpt.");
            return;
        };
        let (vertices, faces) = (mesh.vertex_count(), mesh.face_count());
        let hidden = self
            .sculpt
            .cache
            .as_ref()
            .map_or(0, |cache| cache.sculpt.hidden_count());
        ui.weak(format!("{vertices} vertices, {faces} faces"));
        ui.add_enabled_ui(editable, |ui| {
            brush_ui(ui, &mut self.sculpt.brush);
            ui.add_space(8.0);
            widgets::section_label(ui, "Mask");
            let mut mask_action: Option<fn(&mut Sculpt)> = None;
            responsive::toolbar(ui, |ui| {
                if ui
                    .button(theme::with_icon(icons::SWAP, "Invert"))
                    .on_hover_text("Masked parts are free, and free parts masked")
                    .clicked()
                {
                    mask_action = Some(Sculpt::invert_mask);
                }
                if ui
                    .button(theme::with_icon(icons::ERASER, "Clear"))
                    .on_hover_text("Nothing is masked")
                    .clicked()
                {
                    mask_action = Some(Sculpt::clear_mask);
                }
                if ui
                    .button(theme::with_icon(icons::EYE_SLASH, "Hide Masked"))
                    .on_hover_text("Hide the masked parts: brushes and the view pass them over")
                    .clicked()
                {
                    mask_action = Some(|sculpt| {
                        sculpt.hide_masked();
                    });
                }
                if ui
                    .add_enabled(
                        hidden > 0,
                        egui::Button::new(theme::with_icon(icons::EYE, "Show All")),
                    )
                    .clicked()
                {
                    mask_action = Some(Sculpt::reveal);
                }
            });
            if let Some(action) = mask_action {
                self.change_mask(action);
            }
            ui.add_space(8.0);
            widgets::section_label(ui, "Detail");
            responsive::form(ui, "studio sculpt detail", |ui| {
                ui.label("Voxel size");
                ui.add(
                    egui::DragValue::new(&mut self.sculpt.voxel_size)
                        .speed(0.002)
                        .range(0.001..=100.0)
                        .suffix(" studs"),
                );
                ui.end_row();
            });
            let mut detail = None;
            responsive::toolbar(ui, |ui| {
                if ui
                    .button(theme::with_icon(icons::GRID_NINE, "Voxel Remesh"))
                    .on_hover_text(
                        "Rebuild the surface as even quads a voxel apart (closed; uvs go)",
                    )
                    .clicked()
                {
                    detail = Some(true);
                }
                if ui
                    .button(theme::with_icon(icons::SQUARES_FOUR, "Subdivide"))
                    .on_hover_text("Split every face into four, smoothing the surface")
                    .clicked()
                {
                    detail = Some(false);
                }
            });
            if let Some(remesh) = detail {
                self.sculpt_detail(remesh);
            }
        });
        if let Some(status) = &self.sculpt.status {
            ui.weak(status);
        }
    }
}

/// The brush, its settings and its keys.
fn brush_ui(ui: &mut egui::Ui, brush: &mut SculptBrush) {
    widgets::section_label(ui, "Brush");
    ui.horizontal_wrapped(|ui| {
        for tool in SculptTool::ALL {
            if ui
                .selectable_label(brush.tool == tool, tool.label())
                .clicked()
            {
                brush.tool = tool;
            }
        }
    });
    responsive::form(ui, "studio sculpt brush", |ui| {
        ui.label("Radius");
        ui.add(
            egui::Slider::new(&mut brush.radius, 1.0..=400.0)
                .logarithmic(true)
                .suffix(" pt"),
        );
        ui.end_row();
        ui.label("Strength");
        ui.add(egui::Slider::new(&mut brush.strength, 0.0..=1.0));
        ui.end_row();
        ui.label("Falloff");
        egui::ComboBox::from_id_salt("studio sculpt falloff")
            .selected_text(brush.falloff.label())
            .show_ui(ui, |ui| {
                for falloff in BrushFalloff::ALL {
                    ui.selectable_value(&mut brush.falloff, falloff, falloff.label());
                }
            });
        ui.end_row();
        ui.label("Spacing");
        ui.add(egui::Slider::new(&mut brush.spacing, 0.02..=2.0).logarithmic(true));
        ui.end_row();
        ui.label("Symmetry");
        ui.horizontal(|ui| {
            for (axis, name) in ["X", "Y", "Z"].into_iter().enumerate() {
                ui.toggle_value(&mut brush.symmetry[axis], name);
            }
        });
        ui.end_row();
        ui.label("Pressure");
        ui.horizontal(|ui| {
            ui.toggle_value(&mut brush.pressure_radius, "Radius")
                .on_hover_text(
                    "A pen's pressure changes the radius (where the platform reports it)",
                );
            ui.toggle_value(&mut brush.pressure_strength, "Strength")
                .on_hover_text("A pen's pressure changes the strength");
        });
        ui.end_row();
    });
    if brush.tool.drags() {
        ui.weak("Drags what's under the brush with the mouse; strength doesn't change it.");
    }
    ui.weak("Ctrl: the other way · Shift: Smooth · [ ]: radius");
}
