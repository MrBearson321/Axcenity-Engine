//! The Model Studio's Collision fold (roadmap R15.2b): what the active
//! object collides as in a game. Generate Box, Generate Hull and Generate
//! Convex Pieces (with a piece count) put `_COL` objects in the model, in
//! place of ones it had; Remove takes them out. Each is one undo step.
//! Pieces aren't drawn as solids: Show Collision outlines them in green.
//! They're ordinary objects otherwise, so Edit mode edits them. A MeshPart
//! whose `CollisionFidelity` is Default collides as them (decision D2 in
//! `docs/research/model-studio-r15.md`).

use axcenity_mesh_edit::{PieceMesh, box_piece};
use axcenity_render::glam::Vec3;
use eframe::egui;

use super::{Mode, StudioDoc};
use crate::theme::{self, icons};
use crate::widgets::{self, Role};

/// The fold's header.
pub const COLLISION_HEADER: &str = "Collision";

/// How many pieces Generate Convex Pieces asks for at first.
pub const DEFAULT_PIECES: u32 = 8;

/// What Generate makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollisionShape {
    /// The box around the object.
    Box,
    /// One convex hull of its corners.
    Hull,
    /// At most this many convex pieces following its hollows.
    Pieces(u32),
}

impl CollisionShape {
    fn label(self) -> &'static str {
        match self {
            CollisionShape::Box => "Generate Box Collision",
            CollisionShape::Hull => "Generate Hull Collision",
            CollisionShape::Pieces(_) => "Generate Convex Pieces",
        }
    }
}

impl StudioDoc {
    /// Gives the active object `shape` as its collision, in place of
    /// pieces it had, as one undo step.
    pub fn generate_collision(&mut self, shape: CollisionShape) -> Result<(), String> {
        let refused =
            "Collision is made for a full object with a mesh, not a group, a LOD copy or a piece";
        if self.model.is_lod_copy(self.active) || self.model.is_collision_piece(self.active) {
            return Err(refused.to_owned());
        }
        let (corners, triangles) = self.model.collision_source(self.active).ok_or(refused)?;
        let array: Vec<[f32; 3]> = corners.iter().map(|p| p.to_array()).collect();
        let to_piece = |(points, triangles): axcenity_physics::ConvexMesh| -> PieceMesh {
            (points.into_iter().map(Vec3::from).collect(), triangles)
        };
        let pieces: Vec<PieceMesh> = match shape {
            CollisionShape::Box => box_piece(&corners).into_iter().collect(),
            CollisionShape::Hull => axcenity_physics::convex_hull_mesh(&array)
                .map(to_piece)
                .into_iter()
                .collect(),
            CollisionShape::Pieces(count) => {
                axcenity_physics::convex_pieces(&array, &triangles, count)
                    .into_iter()
                    .map(to_piece)
                    .collect()
            }
        };
        if pieces.is_empty() {
            return Err(
                "It's flat, so it has no volume to collide as: try Generate Box".to_owned(),
            );
        }
        let before = self.snapshot();
        let had = !self.model.collision_pieces(self.active).is_empty();
        let index = self
            .model
            .set_collision(self.active, pieces)
            .ok_or(refused)?;
        self.active = index;
        self.objects = [index].into();
        self.mode = Mode::Object;
        self.selection.clear();
        self.show_collision = true;
        self.record(
            if had {
                "Regenerate Collision"
            } else {
                shape.label()
            },
            before,
        );
        Ok(())
    }

    /// Takes the active object's pieces out, as one undo step; it collides
    /// as its hull again. Nothing happens without pieces.
    pub fn remove_collision(&mut self) -> Result<(), String> {
        if self.model.collision_pieces(self.active).is_empty() {
            return Err("It has no collision pieces to remove".to_owned());
        }
        let before = self.snapshot();
        let index = self
            .model
            .remove_collision(self.active)
            .ok_or("Remove Collision needs a full object with a mesh")?;
        self.active = index;
        self.objects = [index].into();
        self.selection.clear();
        self.record("Remove Collision", before);
        Ok(())
    }
}

/// The fold, under Levels of detail.
pub(super) fn panel(
    ui: &mut egui::Ui,
    doc: &mut StudioDoc,
    editable: bool,
    notices: &mut Vec<String>,
) {
    let Some(object) = doc.model.objects.get(doc.active) else {
        return;
    };
    if object.mesh.is_none() || doc.model.is_lod_copy(doc.active) {
        return;
    }
    ui.add_space(8.0);
    if doc.model.is_collision_piece(doc.active) {
        ui.weak("A collision piece: games collide with it and don't draw it. Edit mode edits it.");
        return;
    }
    egui::CollapsingHeader::new(COLLISION_HEADER)
        .id_salt("studio collision")
        .default_open(false)
        .show(ui, |ui| sections(ui, doc, editable, notices))
        .header_response
        .on_hover_text("What it collides as in a game: a box, a hull or convex pieces");
}

fn sections(ui: &mut egui::Ui, doc: &mut StudioDoc, editable: bool, notices: &mut Vec<String>) {
    let palette = theme::palette(ui.ctx());
    let pieces = doc.model.collision_pieces(doc.active).len();
    if pieces == 0 {
        ui.weak("No pieces: a MeshPart collides as its convex hull, with hollows filled in.");
    } else {
        ui.label(format!(
            "{pieces} convex piece{}: a MeshPart collides as {}",
            if pieces == 1 { "" } else { "s" },
            if pieces == 1 { "it" } else { "them" },
        ));
    }
    ui.add_space(4.0);
    ui.add_enabled_ui(editable, |ui| {
        let mut generate = None;
        ui.horizontal_wrapped(|ui| {
            if ui
                .add(widgets::button(
                    Role::Secondary,
                    Some(icons::CUBE),
                    "Box",
                    &palette,
                ))
                .on_hover_text("One box around it: the cheapest to collide with")
                .clicked()
            {
                generate = Some(CollisionShape::Box);
            }
            if ui
                .add(widgets::button(
                    Role::Secondary,
                    Some(icons::POLYGON),
                    "Hull",
                    &palette,
                ))
                .on_hover_text("One convex hull of its corners, hollows filled in")
                .clicked()
            {
                generate = Some(CollisionShape::Hull);
            }
        });
        ui.horizontal(|ui| {
            if ui
                .add(widgets::button(
                    Role::Secondary,
                    Some(icons::SQUARES_FOUR),
                    "Convex Pieces",
                    &palette,
                ))
                .on_hover_text(
                    "Convex pieces following its hollows, so a doorway or the space under a \
                     table stays open. More pieces follow it closer and cost more",
                )
                .clicked()
            {
                generate = Some(CollisionShape::Pieces(doc.collision_pieces));
            }
            ui.add(
                egui::DragValue::new(&mut doc.collision_pieces)
                    .range(2..=axcenity_physics::MAX_PIECES)
                    .suffix(" at most"),
            )
            .on_hover_text("How many pieces Convex Pieces may make");
        });
        if let Some(shape) = generate
            && let Err(why) = doc.generate_collision(shape)
        {
            notices.push(why);
        }
        if ui
            .add_enabled(
                pieces > 0,
                widgets::button(Role::Secondary, Some(icons::TRASH), "Remove", &palette),
            )
            .on_hover_text("Take the pieces out; it collides as its hull again")
            .clicked()
            && let Err(why) = doc.remove_collision()
        {
            notices.push(why);
        }
    });
    ui.checkbox(&mut doc.show_collision, "Show Collision")
        .on_hover_text("Outline every collision piece in green");
}
