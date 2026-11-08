//! Texture paint mode (Phase 59): painting the active object's material
//! maps in layers, in the 3D view or in the UV editor.
//!
//! The first stroke on a material slot starts a [`PaintSet`] for it at the
//! chosen resolution, with its material's own values and textures under
//! the paint; a stroke on a map not painted yet adds it. Each stroke, and
//! each layer change, is one undo step.
//!
//! In the 3D view a dab covers the texels near it on the surface, through
//! the slot's [`TexelMap`] (built once per mesh and kept), so strokes go
//! across uv seams without a line; X, Y and Z symmetry paint mirrored
//! copies. In the UV editor a dab covers the texels near it in uv space.
//! Ctrl-click sets the Clone brush's source; Alt-drag moves a stencil;
//! `[` and `]` change the radius. A pen's pressure (where the platform
//! reports it) changes the radius or strength.

use std::collections::BTreeSet;
use std::sync::Arc;

use axcenity_assets::{Image, ModelMaterial};
use axcenity_mesh_edit::paint::{
    Brush, BrushFalloff, BrushImage, Dab, ImageMode, PaintTool, SIZES, StampView, Target, Touch,
    stamp, uv_touches,
};
use axcenity_mesh_edit::{EditMesh, MapKind, PaintSet, TexelMap, uv};
use axcenity_render::glam::{Vec2, Vec3};
use eframe::egui;

use super::uv_editor::UvScreen;
use super::{Mode, ModelStudio, Snapshot, StudioAction, StudioDoc, view};
use crate::screen::Screen;
use crate::theme::{self, icons};
use crate::{responsive, widgets};

/// Texture paint mode's state.
pub struct PaintState {
    pub brush: Brush,
    /// The map strokes paint.
    pub map: MapKind,
    /// The active layer, or its mask.
    pub target: Target,
    /// The resolution a slot's first stroke paints at.
    pub size: u32,
    /// The active object's material slot painted.
    pub slot: u32,
    /// The Clone brush's source, as a uv on the slot's maps.
    pub clone_source: Option<Vec2>,
    texels: Option<TexelCache>,
    stroke: Option<Stroke>,
    /// The pen's pressure while it's down, if the platform reports it.
    pressure: Option<f32>,
    /// A panel setting being dragged, which joins one undo step.
    editing: bool,
    /// The map the UV editor shows, and what it was made from.
    texture: Option<(egui::TextureHandle, MapKey)>,
    /// The stencil drawn over the view, and its image's key.
    stencil: Option<(egui::TextureHandle, u64)>,
    /// Why the last stroke didn't paint, for the hint.
    pub status: Option<String>,
}

impl Default for PaintState {
    fn default() -> Self {
        Self {
            brush: Brush::default(),
            map: MapKind::BaseColor,
            target: Target::Layer,
            size: 1024,
            slot: 0,
            clone_source: None,
            texels: None,
            stroke: None,
            pressure: None,
            editing: false,
            texture: None,
            stencil: None,
            status: None,
        }
    }
}

/// What a map shown in the UV editor was made from: the revisions, the
/// object, the slot and the map.
type MapKey = (u64, u64, usize, u32, MapKind);

/// A change to a paint set from the Layers panel, and its undo step's name.
type LayerChange = (&'static str, Box<dyn FnOnce(&mut PaintSet)>);

/// The texel map of one slot of one object's mesh, at one size.
struct TexelCache {
    object: usize,
    slot: u32,
    size: u32,
    revision: u64,
    mesh: EditMesh,
    map: TexelMap,
}

/// A stroke under way.
struct Stroke {
    before: Snapshot,
    /// Where the last dab was on the view, and on the surface (3D) or in
    /// uvs (the UV editor).
    last: Option<(egui::Pos2, Vec3, Vec2)>,
    /// How far the mouse has gone since the last dab, in points.
    travelled: f32,
    /// The Clone brush's source map as the stroke began, and the uv shift
    /// to it.
    clone: Option<(Arc<Vec<u8>>, Vec2)>,
    /// Painted since the material was last shown.
    dirty: bool,
    painted: bool,
}

impl PaintState {
    /// True while a stroke is under way.
    pub fn busy(&self) -> bool {
        self.stroke.is_some()
    }

    /// Forgets anything under way, for another model.
    pub(super) fn forget(&mut self) {
        self.texels = None;
        self.stroke = None;
        self.clone_source = None;
        self.texture = None;
        self.status = None;
        self.slot = 0;
    }

    /// The texel map for `doc`'s active object and the slot, built if the
    /// mesh changed since.
    fn texels(&mut self, doc: &StudioDoc, size: u32) -> Option<&TexelMap> {
        let mesh = doc.mesh()?;
        let fresh = self.texels.as_ref().is_some_and(|cache| {
            cache.object == doc.active
                && cache.slot == self.slot
                && cache.size == size
                && (cache.revision == doc.revision || cache.mesh == *mesh)
        });
        if !fresh {
            self.texels = Some(TexelCache {
                object: doc.active,
                slot: self.slot,
                size,
                revision: doc.revision,
                mesh: mesh.clone(),
                map: TexelMap::build(mesh, self.slot, size),
            });
        }
        let cache = self.texels.as_mut()?;
        cache.revision = doc.revision;
        Some(&cache.map)
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
}

impl StudioDoc {
    /// The active object's paint set for `slot`, if it's being painted.
    pub fn paint_set(&self, slot: u32) -> Option<&PaintSet> {
        let object = self.model.objects.get(self.active)?;
        object.paint.iter().find(|set| set.slot == slot)
    }

    /// The active object's set for `slot` and its material, made ready to
    /// paint `kind` on: the set started at `size` (if it isn't yet) and the
    /// map added, both put on the material. `None` without a mesh or slot.
    fn paint_target(
        &mut self,
        slot: u32,
        size: u32,
        kind: MapKind,
    ) -> Option<(&mut PaintSet, &mut ModelMaterial)> {
        let object = self.model.objects.get_mut(self.active)?;
        object.mesh.as_ref()?;
        if object.materials.is_empty() {
            object.materials.push(ModelMaterial::default());
        }
        let slot = slot.min(object.materials.len() as u32 - 1);
        let material = &mut object.materials[slot as usize];
        let index = match object.paint.iter().position(|set| set.slot == slot) {
            Some(index) => index,
            None => {
                let set = PaintSet::new(slot, size, material);
                set.apply_to(material);
                object.paint.push(set);
                object.paint.len() - 1
            }
        };
        let set = &mut object.paint[index];
        if set.map(kind).is_none() {
            let before = material.clone();
            set.ensure_map(kind, &before);
            set.apply_to(material);
        }
        Some((set, material))
    }

    /// Changes the active object's set for `slot` (a layer added, hidden,
    /// moved…) and puts it on the material again, as one undo step named
    /// `label`; with `join`, part of the step the last joined change made.
    pub(crate) fn change_paint(
        &mut self,
        slot: u32,
        label: &str,
        join: Option<&mut bool>,
        change: impl FnOnce(&mut PaintSet),
    ) {
        let joined = join.as_deref().copied().unwrap_or(false);
        let before = (!joined).then(|| self.snapshot());
        let Some(object) = self.model.objects.get_mut(self.active) else {
            return;
        };
        let Some(set) = object.paint.iter_mut().find(|set| set.slot == slot) else {
            return;
        };
        change(set);
        if let Some(material) = object.materials.get_mut(slot as usize) {
            set.apply_to(material);
        }
        match before {
            Some(before) => self.record(label, before),
            None => self.revision += 1,
        }
        if let Some(join) = join {
            *join = true;
        }
    }
}

impl ModelStudio {
    /// Starts a stroke, if the active object has a mesh: the set and map
    /// are made ready first, inside the stroke's undo step.
    fn begin_stroke(&mut self) -> bool {
        let Some(doc) = &mut self.doc else {
            return false;
        };
        if doc.mode != Mode::Paint || doc.mesh().is_none() {
            return false;
        }
        if self.paint.brush.tool == PaintTool::Clone && self.paint.clone_source.is_none() {
            self.paint.status = Some("Ctrl-click to set where Clone copies from".to_owned());
            return false;
        }
        let before = doc.snapshot();
        let (slot, size, kind) = (self.paint.slot, self.paint.size, self.paint.map);
        if doc.paint_target(slot, size, kind).is_none() {
            return false;
        }
        // The slot as it's painted (a slot past the last one is the last).
        self.paint.slot = doc.paint_set(slot).map_or(slot, |set| set.slot).min(slot);
        doc.paint_revision += 1;
        self.paint.status = None;
        self.paint.stroke = Some(Stroke {
            before,
            last: None,
            travelled: 0.0,
            clone: None,
            dirty: false,
            painted: false,
        });
        true
    }

    /// Ends the stroke as one undo step (the height map's normal map is
    /// made now).
    fn end_stroke(&mut self) {
        let Some(stroke) = self.paint.stroke.take() else {
            return;
        };
        self.paint.pressure = None;
        let Some(doc) = &mut self.doc else {
            return;
        };
        if let Some(object) = doc.model.objects.get_mut(doc.active)
            && let Some(set) = object.paint.iter().find(|set| set.slot == self.paint.slot)
            && let Some(material) = object.materials.get_mut(set.slot as usize)
        {
            set.apply_map(self.paint.map, material);
        }
        let label = match self.paint.brush.tool {
            PaintTool::Fill => "Fill",
            _ if stroke.painted => "Paint Stroke",
            _ => "Start Painting",
        };
        doc.record(label, stroke.before);
    }

    /// Shows what the stroke has painted so far on the material.
    fn show_paint(&mut self) {
        let Some(stroke) = &mut self.paint.stroke else {
            return;
        };
        if !std::mem::take(&mut stroke.dirty) {
            return;
        }
        let Some(doc) = &mut self.doc else {
            return;
        };
        // The height map's normals are worked out when the stroke ends.
        if self.paint.map != MapKind::Height
            && let Some(object) = doc.model.objects.get_mut(doc.active)
            && let Some(set) = object.paint.iter().find(|set| set.slot == self.paint.slot)
            && let Some(material) = object.materials.get_mut(set.slot as usize)
        {
            set.apply_map(self.paint.map, material);
        }
        doc.paint_revision += 1;
    }

    /// Paints `touches` onto the stroke's map, with the dab's middle at
    /// `center` on the view and `radius` points wide.
    fn stamp_touches(&mut self, touches: &[Touch], center: Vec2, radius: f32, plain: bool) {
        let (Some(doc), Some(stroke)) = (&mut self.doc, &mut self.paint.stroke) else {
            return;
        };
        let Some(object) = doc.model.objects.get_mut(doc.active) else {
            return;
        };
        let Some(set) = object
            .paint
            .iter_mut()
            .find(|set| set.slot == self.paint.slot)
        else {
            return;
        };
        let Some(map) = set.map_mut(self.paint.map) else {
            return;
        };
        let view = StampView {
            center,
            radius,
            clone: stroke
                .clone
                .as_ref()
                .map(|(source, shift)| (source.as_slice(), *shift)),
        };
        let brush = if plain {
            Brush {
                image: None,
                ..self.paint.brush.clone()
            }
        } else {
            self.paint.brush.clone()
        };
        if stamp(map, self.paint.target, touches, &brush, &view) {
            stroke.dirty = true;
            stroke.painted = true;
        }
    }

    /// The set's size for the stroke's slot (or the size a new one gets).
    fn paint_size(&self) -> u32 {
        self.doc
            .as_ref()
            .and_then(|doc| doc.paint_set(self.paint.slot))
            .map_or(self.paint.size, |set| set.size)
    }

    /// A dab at `at` in the 3D view (and its mirror images).
    fn dab_in_view(&mut self, screen: &Screen, at: egui::Pos2) {
        let Some(doc) = &self.doc else {
            return;
        };
        let Some(mesh) = doc.mesh() else {
            return;
        };
        let ray = screen.ray(at);
        let Some((face, along)) = view::pick_face(mesh, ray) else {
            return;
        };
        let point = ray.origin + ray.direction * along;
        let mut normal = mesh.face_normal(face);
        if normal.dot(ray.direction) > 0.0 {
            normal = -normal;
        }
        let linked = (self.paint.brush.tool == PaintTool::Fill).then(|| linked_faces(mesh, face));
        let size = self.paint_size();
        let (radius_points, strength) = self.paint.radius_and_strength();
        let symmetry = self.paint.brush.symmetry;
        let falloff = self.paint.brush.falloff;
        let last = self.paint.stroke.as_ref().and_then(|stroke| stroke.last);
        let Some(doc) = &self.doc else {
            return;
        };
        let Some(texels) = self.paint.texels(doc, size) else {
            return;
        };
        let hit_uv = texels.uv_at(face, point);
        let project = |p: Vec3| screen.point(p).map(|q| Vec2::new(q.x, q.y));
        let plain = linked.is_some();
        let touches: Vec<Touch> = match linked {
            Some(faces) => texels
                .face_texels(&faces)
                .into_iter()
                .map(|texel| Touch {
                    texel,
                    weight: strength,
                    at: Vec2::NAN,
                    smear: Vec2::ZERO,
                })
                .collect(),
            None => {
                let dab = Dab {
                    center: point,
                    normal,
                    radius: radius_points * screen.studs_per_point(point),
                    strength,
                    motion: last.map_or(Vec3::ZERO, |(_, from, _)| point - from),
                    mirror: [false; 3],
                };
                dab.with_symmetry(symmetry)
                    .iter()
                    .flat_map(|copy| texels.touches(copy, falloff, &project))
                    .collect()
            }
        };
        let uv = hit_uv.unwrap_or(Vec2::NAN);
        if !self.clone_ready(uv) {
            return;
        }
        let center = Vec2::new(at.x, at.y);
        self.stamp_touches(&touches, center, radius_points, plain);
        if let Some(stroke) = &mut self.paint.stroke {
            stroke.last = Some((at, point, uv));
        }
    }

    /// Before a stroke's first Clone dab, at `uv`: the source map and the
    /// shift from here to the source. False if Clone can't paint.
    fn clone_ready(&mut self, uv: Vec2) -> bool {
        if self.paint.brush.tool != PaintTool::Clone {
            return true;
        }
        let (Some(source), Some(doc)) = (self.paint.clone_source, &self.doc) else {
            return false;
        };
        let Some(stroke) = &mut self.paint.stroke else {
            return false;
        };
        if stroke.clone.is_some() {
            return true;
        }
        if !uv.is_finite() {
            return false;
        }
        let Some(map) = doc
            .paint_set(self.paint.slot)
            .and_then(|set| set.map(self.paint.map))
        else {
            return false;
        };
        stroke.clone = Some((map.flat_shared(), source - uv));
        true
    }

    /// A dab at `at` in the UV editor.
    fn dab_in_uvs(&mut self, screen: &UvScreen, at: egui::Pos2) {
        let size = self.paint_size();
        let uv = screen.uv(at);
        let (radius_points, strength) = self.paint.radius_and_strength();
        let last = self.paint.stroke.as_ref().and_then(|stroke| stroke.last);
        let touches = if self.paint.brush.tool == PaintTool::Fill {
            // The uv island under the click.
            let Some(doc) = &self.doc else {
                return;
            };
            let Some(mesh) = doc.mesh() else {
                return;
            };
            let island = island_at(mesh, self.paint.slot, uv);
            let Some(texels) = self.paint.texels(doc, size) else {
                return;
            };
            texels
                .face_texels(&island)
                .into_iter()
                .map(|texel| Touch {
                    texel,
                    weight: strength,
                    at: Vec2::NAN,
                    smear: Vec2::ZERO,
                })
                .collect()
        } else {
            let to_view = |uv: Vec2| {
                let p = screen.point(uv);
                Vec2::new(p.x, p.y)
            };
            uv_touches(
                size,
                uv,
                radius_points / screen.scale(),
                self.paint.brush.falloff,
                strength,
                last.map_or(Vec2::ZERO, |(_, _, from)| uv - from),
                &to_view,
            )
        };
        if !self.clone_ready(uv) {
            return;
        }
        let plain = self.paint.brush.tool == PaintTool::Fill;
        self.stamp_touches(&touches, Vec2::new(at.x, at.y), radius_points, plain);
        if let Some(stroke) = &mut self.paint.stroke {
            stroke.last = Some((at, Vec3::ZERO, uv));
        }
    }

    /// Dabs from the last one toward `at`, one each spacing along the way.
    fn drag_to(&mut self, at: egui::Pos2, dab: &mut dyn FnMut(&mut Self, egui::Pos2)) {
        let Some(stroke) = &self.paint.stroke else {
            return;
        };
        let Some((from, ..)) = stroke.last else {
            dab(self, at);
            return;
        };
        if self.paint.brush.tool == PaintTool::Fill {
            return;
        }
        let step = (self.paint.brush.spacing * self.paint.radius_and_strength().0).max(1.0);
        let distance = from.distance(at);
        let mut travelled = stroke.travelled;
        let mut along = step - travelled;
        while along <= distance {
            dab(self, from + (at - from) * (along / distance));
            along += step;
        }
        travelled = (travelled + distance) % step;
        if let Some(stroke) = &mut self.paint.stroke {
            stroke.travelled = travelled;
            // Later dabs measure from where the mouse is.
            if let Some(last) = &mut stroke.last {
                last.0 = at;
            }
        }
    }

    /// A whole stroke through `points` in the 3D view, as dragging the
    /// mouse along them paints it: one undo step.
    #[cfg(test)]
    pub(crate) fn paint_stroke(&mut self, screen: &Screen, points: &[egui::Pos2]) -> bool {
        if !self.begin_stroke() {
            return false;
        }
        for &at in points {
            self.drag_to(at, &mut |studio, at| studio.dab_in_view(screen, at));
        }
        self.show_paint();
        self.end_stroke();
        true
    }

    /// A whole stroke through `points` in the UV editor.
    #[cfg(test)]
    pub(super) fn paint_uv_stroke(&mut self, screen: &UvScreen, points: &[egui::Pos2]) -> bool {
        if !self.begin_stroke() {
            return false;
        }
        for &at in points {
            self.drag_to(at, &mut |studio, at| studio.dab_in_uvs(screen, at));
        }
        self.show_paint();
        self.end_stroke();
        true
    }

    /// What the mouse does in the 3D view in Paint mode: strokes, Ctrl-click
    /// for Clone's source, Alt-drag to move a stencil. Draws the brush.
    pub(super) fn paint_in_view(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        screen: &Screen,
    ) {
        self.paint.read_pressure(ui);
        let (pointer, modifiers, pressed, down) = ui.input(|input| {
            (
                input.pointer.latest_pos(),
                input.modifiers,
                input.pointer.primary_pressed(),
                input.pointer.primary_down(),
            )
        });
        if let Some(image) = &mut self.paint.brush.image
            && image.mode == ImageMode::Stencil
        {
            if !image.center.is_finite() {
                let middle = response.rect.center();
                image.center = Vec2::new(middle.x, middle.y);
            }
            if modifiers.alt && response.dragged_by(egui::PointerButton::Primary) {
                let delta = response.drag_delta();
                image.center += Vec2::new(delta.x, delta.y);
            }
        }
        let over =
            pointer.is_some_and(|at| response.rect.contains(at)) && response.contains_pointer();
        if pressed && over && modifiers.command {
            if let Some(at) = pointer {
                self.set_clone_source(screen, at);
            }
        } else if pressed && over && !modifiers.alt && self.paint.stroke.is_none() {
            if self.begin_stroke()
                && let Some(at) = pointer
            {
                self.dab_in_view(screen, at);
            }
        } else if self.paint.stroke.is_some() {
            if let Some(at) = pointer.filter(|_| down) {
                self.drag_to(at, &mut |studio, at| studio.dab_in_view(screen, at));
            }
            if !down {
                self.end_stroke();
            }
        }
        self.show_paint();
        self.draw_brush(ui, response.rect, pointer.filter(|_| over));
    }

    /// Ctrl-click in the 3D view: Clone copies from the uv clicked.
    fn set_clone_source(&mut self, screen: &Screen, at: egui::Pos2) {
        let size = self.paint_size();
        let Some(doc) = &self.doc else {
            return;
        };
        let Some(mesh) = doc.mesh() else {
            return;
        };
        let ray = screen.ray(at);
        let Some((face, along)) = view::pick_face(mesh, ray) else {
            return;
        };
        let point = ray.origin + ray.direction * along;
        let Some(texels) = self.paint.texels(doc, size) else {
            return;
        };
        self.paint.clone_source = texels.uv_at(face, point);
    }

    /// The brush's circle at the mouse, and a stencil over the view.
    fn draw_brush(&mut self, ui: &egui::Ui, rect: egui::Rect, pointer: Option<egui::Pos2>) {
        let painter = ui.painter_at(rect);
        let accent = theme::palette(ui.ctx()).accent;
        if let Some(image) = &self.paint.brush.image
            && image.mode == ImageMode::Stencil
        {
            let key = image.image.key();
            if self
                .paint
                .stencil
                .as_ref()
                .is_none_or(|(_, shown)| *shown != key)
            {
                let pixels = egui::ColorImage::from_rgba_unmultiplied(
                    [image.image.width as usize, image.image.height as usize],
                    &image.image.rgba,
                );
                let handle =
                    ui.ctx()
                        .load_texture("studio stencil", pixels, egui::TextureOptions::LINEAR);
                self.paint.stencil = Some((handle, key));
            }
            if let Some((handle, _)) = &self.paint.stencil {
                let across = image.width.max(1.0);
                let down = across * image.image.height as f32 / (image.image.width.max(1) as f32);
                let area = egui::Rect::from_center_size(
                    egui::pos2(image.center.x, image.center.y),
                    egui::vec2(across, down),
                );
                painter.image(
                    handle.id(),
                    area,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::from_white_alpha(70),
                );
                painter.rect_stroke(
                    area,
                    0.0,
                    egui::Stroke::new(1.0_f32, accent.gamma_multiply(0.6)),
                    egui::StrokeKind::Middle,
                );
            }
        }
        if let Some(at) = pointer {
            let (radius, _) = self.paint.radius_and_strength();
            painter.circle_stroke(at, radius, egui::Stroke::new(1.5_f32, egui::Color32::WHITE));
            painter.circle_stroke(at, radius, egui::Stroke::new(1.0_f32, accent));
        }
    }

    /// What the mouse does in the UV editor in Paint mode.
    pub(super) fn paint_in_uvs(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        screen: &UvScreen,
    ) {
        self.paint.read_pressure(ui);
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
        if pressed && over && modifiers.command {
            if let Some(at) = pointer {
                self.paint.clone_source = Some(screen.uv(at));
            }
        } else if pressed && over && self.paint.stroke.is_none() {
            if self.begin_stroke()
                && let Some(at) = pointer
            {
                self.dab_in_uvs(screen, at);
            }
        } else if self.paint.stroke.is_some() {
            if let Some(at) = pointer.filter(|_| down) {
                self.drag_to(at, &mut |studio, at| studio.dab_in_uvs(screen, at));
            }
            if !down {
                self.end_stroke();
            }
        }
        self.show_paint();
        if let Some(at) = pointer.filter(|_| over) {
            let (radius, _) = self.paint.radius_and_strength();
            let accent = theme::palette(ui.ctx()).accent;
            let painter = ui.painter_at(response.rect);
            painter.circle_stroke(at, radius, egui::Stroke::new(1.0_f32, accent));
        }
        if let Some(source) = self.paint.clone_source
            && self.paint.brush.tool == PaintTool::Clone
        {
            let painter = ui.painter_at(response.rect);
            painter.circle_stroke(
                screen.point(source),
                5.0,
                egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(230, 60, 60)),
            );
        }
    }

    /// The map being painted, as a texture for the UV editor to draw under
    /// the uvs, if the active object's slot has it.
    pub(super) fn paint_texture(&mut self, ctx: &egui::Context) -> Option<egui::TextureId> {
        let doc = self.doc.as_ref()?;
        if doc.mode != Mode::Paint {
            return None;
        }
        let set = doc.paint_set(self.paint.slot)?;
        let map = set.map(self.paint.map)?;
        let key = (
            doc.revision,
            doc.paint_revision,
            doc.active,
            self.paint.slot,
            self.paint.map,
        );
        if self
            .paint
            .texture
            .as_ref()
            .is_none_or(|(_, shown)| *shown != key)
        {
            let size = [set.size as usize, set.size as usize];
            let mut pixels = map.flat().to_vec();
            for pixel in pixels.chunks_mut(4) {
                pixel[3] = 255;
            }
            let image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
            match &mut self.paint.texture {
                Some((handle, shown)) => {
                    handle.set(image, egui::TextureOptions::LINEAR);
                    *shown = key;
                }
                None => {
                    let handle =
                        ctx.load_texture("studio paint map", image, egui::TextureOptions::LINEAR);
                    self.paint.texture = Some((handle, key));
                }
            }
        }
        self.paint.texture.as_ref().map(|(handle, _)| handle.id())
    }

    /// `[` and `]` change the brush's radius; X swaps Draw and Erase.
    pub(super) fn paint_keys(&mut self, ui: &mut egui::Ui) {
        let none = egui::Modifiers::NONE;
        let (smaller, bigger, swap) = ui.input_mut(|input| {
            (
                input.consume_key(none, egui::Key::OpenBracket),
                input.consume_key(none, egui::Key::CloseBracket),
                input.consume_key(none, egui::Key::X),
            )
        });
        let brush = &mut self.paint.brush;
        if smaller {
            brush.radius = (brush.radius / 1.15).max(1.0);
        }
        if bigger {
            brush.radius = (brush.radius * 1.15).min(1000.0);
        }
        if swap {
            brush.tool = match brush.tool {
                PaintTool::Erase => PaintTool::Draw,
                _ => PaintTool::Erase,
            };
        }
    }

    /// The Paint panel: the slot, its maps and layers, and the brush.
    /// `actions` gets a brush image to load, when one is picked.
    pub(super) fn paint_panel(
        &mut self,
        ui: &mut egui::Ui,
        editable: bool,
        actions: &mut Vec<StudioAction>,
    ) {
        if !ui.input(|input| input.pointer.any_down()) {
            self.paint.editing = false;
        }
        let Some(doc) = &mut self.doc else {
            return;
        };
        let state = &mut self.paint;
        widgets::section_label(ui, "Paint");
        let Some(object) = doc.model.objects.get(doc.active) else {
            return;
        };
        if object.mesh.is_none() {
            ui.weak("Pick an object with a mesh to paint.");
            return;
        }
        let slots: Vec<String> = object
            .materials
            .iter()
            .enumerate()
            .map(|(k, m)| match m.name.as_str() {
                "" => format!("Slot {}", k + 1),
                name => name.to_owned(),
            })
            .collect();
        state.slot = state.slot.min(slots.len().saturating_sub(1) as u32);
        let painted: Vec<MapKind> = doc
            .paint_set(state.slot)
            .map(|set| set.maps.iter().map(|map| map.kind).collect())
            .unwrap_or_default();
        let set_size = doc.paint_set(state.slot).map(|set| set.size);
        ui.add_enabled_ui(editable, |ui| {
            responsive::form(ui, "studio paint target", |ui| {
                ui.label("Material");
                let mut slot = state.slot;
                egui::ComboBox::from_id_salt("studio paint slot")
                    .selected_text(slots.get(slot as usize).cloned().unwrap_or_default())
                    .show_ui(ui, |ui| {
                        for (k, name) in slots.iter().enumerate() {
                            ui.selectable_value(&mut slot, k as u32, name);
                        }
                    });
                if slot != state.slot {
                    state.slot = slot;
                    state.texels = None;
                }
                ui.end_row();
                ui.label("Resolution");
                match set_size {
                    Some(size) => {
                        ui.label(format!("{size} × {size}"));
                    }
                    None => {
                        egui::ComboBox::from_id_salt("studio paint size")
                            .selected_text(format!("{0} × {0}", state.size))
                            .show_ui(ui, |ui| {
                                for size in SIZES {
                                    ui.selectable_value(
                                        &mut state.size,
                                        size,
                                        format!("{size} × {size}"),
                                    );
                                }
                            });
                    }
                }
                ui.end_row();
                ui.label("Map");
                egui::ComboBox::from_id_salt("studio paint map")
                    .selected_text(state.map.label())
                    .show_ui(ui, |ui| {
                        for kind in MapKind::ALL {
                            let mark = if painted.contains(&kind) { "  ●" } else { "" };
                            ui.selectable_value(
                                &mut state.map,
                                kind,
                                format!("{}{mark}", kind.label()),
                            );
                        }
                    });
                ui.end_row();
            });
            if set_size.is_none() {
                ui.weak("The first stroke starts painting this material.");
            }
            layers_ui(ui, doc, state);
            ui.add_space(8.0);
            brush_ui(ui, state, &self.images, actions);
        });
    }
}

/// The map's layers, top first, and the active one's settings.
fn layers_ui(ui: &mut egui::Ui, doc: &mut StudioDoc, state: &mut PaintState) {
    let slot = state.slot;
    let kind = state.map;
    let Some(map) = doc.paint_set(slot).and_then(|set| set.map(kind)) else {
        if doc.paint_set(slot).is_some() {
            ui.weak(format!(
                "No {} painted yet: a stroke adds it.",
                kind.label()
            ));
        }
        return;
    };
    widgets::section_label(ui, "Layers");
    let active = map.active;
    let count = map.layers.len();
    let rows: Vec<(String, bool, bool)> = map
        .layers
        .iter()
        .map(|layer| (layer.name.clone(), layer.visible, layer.mask.is_some()))
        .collect();
    let mut toggle = None;
    let mut pick = None;
    for (index, (name, visible, masked)) in rows.iter().enumerate().rev() {
        ui.horizontal(|ui| {
            let eye = if *visible {
                icons::EYE
            } else {
                icons::EYE_SLASH
            };
            if widgets::icon_button(ui, eye, "Show or hide the layer", !*visible).clicked() {
                toggle = Some(index);
            }
            let mut text = name.clone();
            if *masked {
                text.push_str("  ◐");
            }
            if ui.selectable_label(index == active, text).clicked() {
                pick = Some(index);
            }
        });
    }
    if let Some(index) = toggle {
        let label = if rows[index].1 {
            "Hide Layer"
        } else {
            "Show Layer"
        };
        doc.change_paint(slot, label, None, |set| {
            if let Some(map) = set.map_mut(kind) {
                map.layers[index].visible = !map.layers[index].visible;
                map.refresh_all();
            }
        });
    }
    if let Some(index) = pick {
        // Picking the layer to paint on isn't an undo step.
        if let Some(object) = doc.model.objects.get_mut(doc.active)
            && let Some(set) = object.paint.iter_mut().find(|set| set.slot == slot)
            && let Some(map) = set.map_mut(kind)
        {
            map.active = index;
        }
    }
    let mut action: Option<LayerChange> = None;
    responsive::toolbar(ui, |ui| {
        if widgets::icon_button(ui, icons::PLUS, "Add a layer above this one", false).clicked() {
            action = Some((
                "Add Layer",
                Box::new(move |set| {
                    if let Some(map) = set.map_mut(kind) {
                        map.add_layer();
                    }
                }),
            ));
        }
        if ui
            .add_enabled(count > 1, egui::Button::new(icons::TRASH))
            .on_hover_text("Delete the layer")
            .clicked()
        {
            action = Some((
                "Delete Layer",
                Box::new(move |set| {
                    if let Some(map) = set.map_mut(kind) {
                        map.remove_layer(active);
                    }
                }),
            ));
        }
        if ui
            .add_enabled(active + 1 < count, egui::Button::new(icons::ARROW_UP))
            .on_hover_text("Move the layer up")
            .clicked()
        {
            action = Some((
                "Move Layer",
                Box::new(move |set| {
                    if let Some(map) = set.map_mut(kind) {
                        map.move_layer(active, true);
                    }
                }),
            ));
        }
        if ui
            .add_enabled(active > 0, egui::Button::new(icons::ARROW_DOWN))
            .on_hover_text("Move the layer down")
            .clicked()
        {
            action = Some((
                "Move Layer",
                Box::new(move |set| {
                    if let Some(map) = set.map_mut(kind) {
                        map.move_layer(active, false);
                    }
                }),
            ));
        }
        let masked = rows[active].2;
        let (mask_label, mask_tip) = if masked {
            ("Remove Mask", "Take the layer's mask off")
        } else {
            (
                "Add Mask",
                "A mask hides the layer where it's painted black",
            )
        };
        if ui.button(mask_label).on_hover_text(mask_tip).clicked() {
            action = Some((
                mask_label,
                Box::new(move |set| {
                    if let Some(map) = set.map_mut(kind) {
                        let size = map.size as usize;
                        let layer = map.active_layer_mut();
                        layer.mask = match layer.mask {
                            Some(_) => None,
                            None => Some(Arc::new(vec![255; size * size])),
                        };
                        map.refresh_all();
                    }
                }),
            ));
            state.target = if masked { Target::Layer } else { Target::Mask };
        }
    });
    if rows[active].2 {
        ui.horizontal(|ui| {
            ui.label("Paint on");
            widgets::segmented(
                ui,
                &mut state.target,
                &[(Target::Layer, None, "Layer"), (Target::Mask, None, "Mask")],
            );
        });
    } else {
        state.target = Target::Layer;
    }
    if let Some((label, change)) = action {
        doc.change_paint(slot, label, None, change);
    }

    // The active layer's name, blend mode and opacity (and the height
    // map's bump).
    let Some(set) = doc.paint_set(slot) else {
        return;
    };
    let Some(map) = set.map(kind) else {
        return;
    };
    let layer = &map.layers[map.active.min(map.layers.len() - 1)];
    let (mut name, mut blend, mut opacity, mut bump) =
        (layer.name.clone(), layer.blend, layer.opacity, set.bump);
    responsive::form(ui, "studio paint layer", |ui| {
        ui.label("Name");
        ui.text_edit_singleline(&mut name);
        ui.end_row();
        ui.label("Blend");
        egui::ComboBox::from_id_salt("studio paint blend")
            .selected_text(blend.label())
            .show_ui(ui, |ui| {
                for mode in axcenity_mesh_edit::paint::BlendMode::ALL {
                    ui.selectable_value(&mut blend, mode, mode.label());
                }
            });
        ui.end_row();
        ui.label("Opacity");
        ui.add(egui::Slider::new(&mut opacity, 0.0..=1.0));
        ui.end_row();
        if kind == MapKind::Height {
            ui.label("Bump");
            ui.add(egui::Slider::new(&mut bump, 0.0..=8.0));
            ui.end_row();
        }
    });
    let editing = &mut state.editing;
    if name != layer.name {
        doc.change_paint(slot, "Rename Layer", Some(editing), |set| {
            if let Some(map) = set.map_mut(kind) {
                map.active_layer_mut().name = name;
            }
        });
    } else if blend != layer.blend || opacity != layer.opacity {
        doc.change_paint(slot, "Layer Blend", Some(editing), |set| {
            if let Some(map) = set.map_mut(kind) {
                let layer = map.active_layer_mut();
                layer.blend = blend;
                layer.opacity = opacity;
                map.refresh_all();
            }
        });
    } else if bump != set.bump {
        doc.change_paint(slot, "Bump", Some(editing), |set| set.bump = bump);
    }
}

/// The brush's settings.
fn brush_ui(
    ui: &mut egui::Ui,
    state: &mut PaintState,
    images: &[String],
    actions: &mut Vec<StudioAction>,
) {
    widgets::section_label(ui, "Brush");
    let brush = &mut state.brush;
    ui.horizontal_wrapped(|ui| {
        for tool in PaintTool::ALL {
            if ui
                .selectable_label(brush.tool == tool, tool.label())
                .clicked()
            {
                brush.tool = tool;
            }
        }
    });
    responsive::form(ui, "studio paint brush", |ui| {
        if state.target == Target::Layer && state.map.is_color() {
            ui.label("Color");
            crate::color_palette::edit_srgb(ui, &mut brush.color);
        } else if state.target == Target::Layer {
            ui.label("Value");
            ui.add(egui::Slider::new(&mut brush.value, 0.0..=1.0));
        }
        ui.end_row();
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
        egui::ComboBox::from_id_salt("studio paint falloff")
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
        ui.label("Image");
        let shown = brush
            .image
            .as_ref()
            .map_or("None", |image| image.path.as_str());
        egui::ComboBox::from_id_salt("studio paint image")
            .selected_text(shown.rsplit('/').next().unwrap_or(shown))
            .show_ui(ui, |ui| {
                if ui.selectable_label(brush.image.is_none(), "None").clicked() {
                    brush.image = None;
                }
                for path in images {
                    let name = path.rsplit('/').next().unwrap_or(path);
                    let picked = brush.image.as_ref().is_some_and(|i| &i.path == path);
                    if ui.selectable_label(picked, name).clicked() {
                        actions.push(StudioAction::BrushImage(path.clone()));
                    }
                }
            });
        ui.end_row();
    });
    if let Some(image) = &mut brush.image {
        ui.horizontal(|ui| {
            widgets::segmented(
                ui,
                &mut image.mode,
                &[
                    (ImageMode::Stencil, None, "Stencil"),
                    (ImageMode::Texture, None, "Texture"),
                ],
            );
            if image.mode == ImageMode::Stencil {
                ui.add(
                    egui::DragValue::new(&mut image.width)
                        .range(16.0..=4000.0)
                        .suffix(" pt wide"),
                );
            }
        });
        if image.mode == ImageMode::Stencil {
            ui.weak("Alt-drag in the view moves the stencil.");
        }
    }
    if brush.tool == PaintTool::Clone {
        ui.weak(match state.clone_source {
            Some(_) => "Ctrl-click sets where Clone copies from (set).",
            None => "Ctrl-click sets where Clone copies from.",
        });
    }
}

/// The faces joined to `face` across edges.
fn linked_faces(mesh: &EditMesh, face: u32) -> BTreeSet<u32> {
    let mut found = BTreeSet::from([face]);
    let mut open = vec![face];
    while let Some(face) = open.pop() {
        for edge in mesh.face_half_edges(face) {
            if let Some(twin) = mesh.half_edge(edge).twin {
                let other = mesh.half_edge(twin).face;
                if found.insert(other) {
                    open.push(other);
                }
            }
        }
    }
    found
}

/// The faces of `slot` in the uv island under `uv`, if any.
fn island_at(mesh: &EditMesh, slot: u32, uv: Vec2) -> BTreeSet<u32> {
    let faces: Vec<u32> = (0..mesh.face_count() as u32)
        .filter(|&face| mesh.face(face).material == slot)
        .collect();
    let under = faces.iter().copied().find(|&face| {
        mesh.triangles(face).into_iter().any(|triangle| {
            let [a, b, c] = triangle.map(|e| mesh.half_edge(e).uv);
            let side = |p: Vec2, q: Vec2| (q - p).perp_dot(uv - p);
            let (x, y, z) = (side(a, b), side(b, c), side(c, a));
            (x >= 0.0 && y >= 0.0 && z >= 0.0) || (x <= 0.0 && y <= 0.0 && z <= 0.0)
        })
    });
    let Some(under) = under else {
        return BTreeSet::new();
    };
    uv::uv_islands(mesh, &faces, 0)
        .into_iter()
        .find(|island| island.contains(&under))
        .map(|island| island.into_iter().collect())
        .unwrap_or_default()
}

/// A brush image read from a PNG, JPEG, TGA or BMP file's `bytes`.
pub fn brush_image(path: &str, bytes: &[u8]) -> Result<BrushImage, String> {
    let image = image::load_from_memory(bytes).map_err(|err| err.to_string())?;
    let image = image.into_rgba8();
    let (width, height) = image.dimensions();
    Ok(BrushImage {
        path: path.to_owned(),
        image: Arc::new(Image::new(width, height, image.into_raw())),
        mode: ImageMode::Stencil,
        center: Vec2::NAN,
        width: 256.0,
    })
}
