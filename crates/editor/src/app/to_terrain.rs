//! Solid Modeling ▸ To Terrain: the selected parts become ground.
//!
//! The Terrain is a heightfield, so each part becomes the ground under its
//! top: every sample under it rises to the top of the part there (found by
//! a ray straight down onto it, so a ball makes a dome and a wedge a
//! ramp), never lower, painted with the TerrainLayer that matches its look.
//! The parts are then deleted. All of it is one undo step.

use std::sync::Arc;

use axcenity_core::{ClassName, InstanceId, TerrainData, Value, World};
use axcenity_render::glam::{Vec2, Vec3};
use axcenity_terrain::Placement;
use axcenity_terrain::brush::EditRecorder;

use super::EditorApp;
use crate::history;

impl EditorApp {
    /// Turns the selected parts into ground on the Terrain under them.
    pub(in crate::app) fn convert_to_terrain(&mut self) {
        let parts = self.solid_parts();
        let Some(terrain) = terrain_of(&self.world) else {
            self.output
                .error("Insert a Terrain first: parts become ground on the Terrain under them.");
            return;
        };
        let Some(placement) = self.world.get(terrain).and_then(Placement::of) else {
            return;
        };
        let before = self.world.clone();
        let mut converted = Vec::new();
        let mut too_high = false;
        for &part in &parts {
            let Some(corners) = axcenity_scene::part_box_edges(&self.world, part) else {
                continue;
            };
            let points = corners.iter().flat_map(|&(a, b)| [a, b]);
            let (min, max) = points.fold((Vec3::INFINITY, Vec3::NEG_INFINITY), |(min, max), p| {
                (min.min(p), max.max(p))
            });
            // Straight down from above the part onto it.
            let world = &self.world;
            let top = |at: Vec2| {
                let ray = axcenity_scene::Ray {
                    origin: Vec3::new(at.x, max.y + 1.0, at.y),
                    direction: Vec3::NEG_Y,
                };
                axcenity_scene::part_hit(world, part, ray).map(|hit| ray.at(hit.distance).y)
            };
            too_high |= max.y > placement.position.y + placement.size.y;
            let layer = matching_layer(world, terrain, part);
            // Changed on a copy while the part is still there to look at,
            // then put back.
            let Some(mut data) = world.terrain(terrain).map(|data| TerrainData::clone(data)) else {
                continue;
            };
            let covered = axcenity_terrain::convert::raise_to(
                &mut data,
                &placement,
                (Vec2::new(min.x, min.z), Vec2::new(max.x, max.z)),
                top,
                layer,
                &mut EditRecorder::new(),
            );
            self.world.set_terrain(terrain, Arc::new(data));
            if covered > 0 {
                converted.push(part);
            }
        }
        if converted.is_empty() {
            self.output
                .error("None of the selected parts is over the Terrain, so nothing was changed.");
            return;
        }
        for &part in &converted {
            let _ = history::put(&mut self.world, part, None);
        }
        let label = match converted.len() {
            1 => "Convert 1 part to terrain".to_owned(),
            n => format!("Convert {n} parts to terrain"),
        };
        self.history.record_world(&mut self.world, label, before);
        self.forget_selection_outside_tree();
        self.select_from_viewport(Some(terrain));
        if too_high {
            self.output.warning(
                "Part of it was higher than the Terrain reaches, so it was cut off there. \
                 Make the Terrain's Size taller to fit it.",
            );
        }
    }
}

/// The first Terrain in the Workspace.
fn terrain_of(world: &World) -> Option<InstanceId> {
    world
        .descendants(world.workspace())
        .into_iter()
        .find(|&id| {
            world
                .get(id)
                .is_some_and(|instance| instance.class() == ClassName::Terrain)
        })
}

/// The terrain's layer that looks most like `part`: one named after its
/// Material (a "Grass" layer for Grass), or else the one nearest its Color.
/// `None` when the terrain has no layers.
fn matching_layer(world: &World, terrain: InstanceId, part: InstanceId) -> Option<usize> {
    let layers = axcenity_terrain::materials::layers(world, terrain);
    let instance = world.get(part)?;
    let material = instance
        .property("Material")
        .and_then(Value::as_material)
        .map(|material| material.as_str().to_lowercase());
    if let Some(material) = &material
        && let Some(index) = layers.iter().position(|&layer| {
            world
                .get(layer)
                .is_some_and(|layer| layer.name().to_lowercase().contains(material.as_str()))
        })
    {
        return Some(index);
    }
    let color = instance.property("Color").and_then(Value::as_color3)?;
    layers
        .iter()
        .enumerate()
        .filter_map(|(index, &layer)| {
            let theirs = world.get(layer)?.property("Color")?.as_color3()?;
            let distance = (theirs.r - color.r).powi(2)
                + (theirs.g - color.g).powi(2)
                + (theirs.b - color.b).powi(2);
            Some((index, distance))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(index, _)| index)
}
