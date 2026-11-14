//! Conform to Terrain and Blend into Terrain (procedural worlds PW5, in
//! the ribbon's Solid Modeling group): the selected parts and models set
//! on the ground, or the ground levelled under them. One undo step each.

use std::sync::Arc;

use axcenity_core::{TerrainData, Value, Vector3};
use axcenity_procedural::obstacles::Obstacle;
use axcenity_render::glam::Vec2;

use super::*;

/// Studs around a footprint over which Blend into Terrain fades out.
pub(super) const BLEND_FALLOFF: f32 = 6.0;

impl EditorApp {
    /// What the selection holds, one group per selected part or model
    /// (not one inside another selected one): what moves with it, and its
    /// parts' boxes.
    fn terrain_groups(&self) -> Vec<(Vec<InstanceId>, Vec<Obstacle>)> {
        let mut ids: Vec<InstanceId> = self.selection.clone();
        if let Some(selected) = self.selected
            && !ids.contains(&selected)
        {
            ids.push(selected);
        }
        let inside_another = |id: InstanceId| {
            ids.iter()
                .any(|&other| other != id && self.world.descendants(other).contains(&id))
        };
        ids.iter()
            .filter(|&&id| !inside_another(id))
            .filter_map(|&id| {
                let moving: Vec<InstanceId> = viewport::movable_parts(&self.world, &[id], None)
                    .into_iter()
                    .map(|(part, _)| part)
                    .filter(|&part| !axcenity_core::csg::in_union(&self.world, part))
                    .collect();
                let boxes: Vec<Obstacle> = moving
                    .iter()
                    .filter_map(|&part| Obstacle::part_box(self.world.get(part)?))
                    .collect();
                (!boxes.is_empty()).then_some((moving, boxes))
            })
            .collect()
    }

    /// The ground's height (world Y) under world `at` (X, Z) on the first
    /// Terrain there.
    fn ground_under(&self, at: Vec2) -> Option<f32> {
        self.terrains()
            .into_iter()
            .find_map(|(terrain, placement)| {
                placement.ground_y(self.world.terrain(terrain)?, at.x, at.y)
            })
    }

    /// Conform to Terrain: each selected part or model moved straight up or
    /// down so its bottom sits on the ground under its middle.
    pub(super) fn conform_to_terrain(&mut self) {
        let groups = self.terrain_groups();
        let before = self.world.clone();
        let mut moved = 0;
        for (moving, boxes) in groups {
            let bottom = boxes
                .iter()
                .map(Obstacle::bottom)
                .fold(f32::INFINITY, f32::min);
            let (min, max) = boxes
                .iter()
                .fold((Vec2::INFINITY, Vec2::NEG_INFINITY), |(min, max), part| {
                    (min.min(part.min), max.max(part.max))
                });
            let Some(ground) = self.ground_under((min + max) / 2.0) else {
                continue;
            };
            let rise = ground - bottom;
            if rise.abs() < 1e-4 {
                continue;
            }
            for id in moving {
                let Some(position) = self
                    .world
                    .get(id)
                    .and_then(|i| i.property("Position"))
                    .and_then(Value::as_vector3)
                else {
                    continue;
                };
                let raised = Vector3::new(position.x, position.y + rise, position.z);
                let _ = self
                    .world
                    .set_property(id, "Position", Value::Vector3(raised));
            }
            moved += 1;
        }
        if moved == 0 {
            self.world = before;
            self.output
                .warning("Nothing selected stands over a Terrain away from its ground.");
            return;
        }
        self.history
            .record_world(&mut self.world, "Conform to Terrain".to_owned(), before);
        self.output.info(format!("Set {moved} on the ground."));
    }

    /// Blend into Terrain: the ground under each selected part or model
    /// levelled to its bottom and painted with the Terrain Studio's Paint
    /// layer, fading out over [`BLEND_FALLOFF`] studs around.
    pub(super) fn blend_into_terrain(&mut self) {
        let groups = self.terrain_groups();
        let terrains = self.terrains();
        let before = self.world.clone();
        let mut changed = 0;
        for (_, boxes) in groups {
            let bottom = boxes
                .iter()
                .map(Obstacle::bottom)
                .fold(f32::INFINITY, f32::min);
            for &(terrain, placement) in &terrains {
                let count = axcenity_terrain::materials::layers(&self.world, terrain).len();
                let layer = (self.terrain.paint_layer < count).then_some(self.terrain.paint_layer);
                let Some(mut data) = self.world.terrain(terrain).map(|d| TerrainData::clone(d))
                else {
                    continue;
                };
                let set = axcenity_procedural::blend_footprint(
                    &mut data,
                    &placement,
                    &boxes,
                    bottom,
                    BLEND_FALLOFF,
                    layer,
                );
                if set > 0 {
                    changed += set;
                    self.world.set_terrain(terrain, Arc::new(data));
                }
            }
        }
        if changed == 0 {
            self.world = before;
            self.output
                .warning("Nothing selected stands over a Terrain, so nothing was changed.");
            return;
        }
        self.history
            .record_world(&mut self.world, "Blend into Terrain".to_owned(), before);
        self.output.info(format!(
            "Levelled {changed} terrain samples under the selection."
        ));
    }
}
