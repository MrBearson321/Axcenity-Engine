//! Axcenity terrain: the algorithms behind Terrain Instances.
//!
//! A Terrain's heightfield and paint are a [`TerrainData`] in the world (see
//! `core`). This crate works on them:
//!
//! - [`Placement`] maps between world studs and the sample grid.
//! - [`brush`] sculpts and paints: raise, lower, smooth, flatten, sculpt,
//!   noise, stamps, material paint and foliage, with undo records.
//! - [`generate`] makes landscapes from settings and a seed, with presets.
//! - [`convert`] turns a part into ground under its top.
//! - [`erosion`] wears them down with water and with gravity.
//! - [`materials`] lays layers down by height and slope, blended with paint.
//! - [`mesh`] builds chunk meshes at several levels of detail.
//! - [`raycast`] finds where a ray meets the terrain.
//! - [`files`] reads and writes the saved PNG and JSON files.
//! - [`job`] runs slow work on another thread, with progress and cancel.

pub mod biomes;
pub mod brush;
pub mod convert;
pub mod erosion;
pub mod files;
pub mod generate;
pub mod job;
pub mod materials;
pub mod mesh;
pub mod noise;
pub mod raycast;
pub mod volume;

use axcenity_core::{ClassName, Instance, TerrainData, Value};
use glam::{Vec2, Vec3};

pub use axcenity_core::{
    CHUNK_CELLS, FoliageInstance, FoliagePlant, FoliageProp, FoliageSource, MAX_FOLIAGE_INSTANCES,
    MAX_TERRAIN_LAYERS, MAX_TERRAIN_SAMPLES, MIN_TERRAIN_SAMPLES, PaintWeights, SampleRect,
    TerrainRegion,
};

/// Where a Terrain is: `position` is the middle of its footprint at the
/// bottom of its height range, and `size` its width (X), height range (Y)
/// and length (Z), in studs. Terrains don't turn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub position: Vec3,
    pub size: Vec3,
}

impl Placement {
    /// A Terrain Instance's placement, or `None` for another class or a
    /// size that isn't a positive, finite number on every axis.
    pub fn of(instance: &Instance) -> Option<Placement> {
        if instance.class() != ClassName::Terrain {
            return None;
        }
        let vector = |name| {
            let v = instance.property(name).and_then(Value::as_vector3)?;
            Some(Vec3::new(v.x, v.y, v.z))
        };
        let placement = Placement {
            position: vector("Position")?,
            size: vector("Size")?,
        };
        let usable = placement.position.is_finite()
            && placement.size.is_finite()
            && placement.size.min_element() > 0.0;
        usable.then_some(placement)
    }

    /// Studs between samples along X and along Z.
    pub fn cell_size(&self, data: &TerrainData) -> Vec2 {
        let (size_x, size_z) = data.size();
        Vec2::new(
            self.size.x / (size_x - 1) as f32,
            self.size.z / (size_z - 1) as f32,
        )
    }

    /// The world's X and Z as sample coordinates (fractions between samples).
    pub fn to_grid(&self, data: &TerrainData, x: f32, z: f32) -> Vec2 {
        let cell = self.cell_size(data);
        Vec2::new(
            (x - self.position.x + self.size.x / 2.0) / cell.x,
            (z - self.position.z + self.size.z / 2.0) / cell.y,
        )
    }

    /// Sample coordinates as the world's X and Z.
    pub fn to_world_xz(&self, data: &TerrainData, grid: Vec2) -> Vec2 {
        let cell = self.cell_size(data);
        Vec2::new(
            self.position.x - self.size.x / 2.0 + grid.x * cell.x,
            self.position.z - self.size.z / 2.0 + grid.y * cell.y,
        )
    }

    /// A height from 0 to 1 as the world's Y.
    pub fn world_y(&self, height: f32) -> f32 {
        self.position.y + height * self.size.y
    }

    /// The world's Y as a height from 0 to 1 (not clamped).
    pub fn height_of(&self, y: f32) -> f32 {
        (y - self.position.y) / self.size.y
    }

    /// The point on the surface at sample (`x`, `z`).
    pub fn sample_point(&self, data: &TerrainData, x: usize, z: usize) -> Vec3 {
        let xz = self.to_world_xz(data, Vec2::new(x as f32, z as f32));
        let height = data.height(x as isize, z as isize);
        Vec3::new(xz.x, self.world_y(height), xz.y)
    }

    /// The ground's height (world Y) under world (`x`, `z`), blended
    /// between samples, or `None` off the terrain or over a hole.
    pub fn ground_y(&self, data: &TerrainData, x: f32, z: f32) -> Option<f32> {
        let grid = self.to_grid(data, x, z);
        let (size_x, size_z) = data.size();
        let inside = grid.x >= 0.0
            && grid.y >= 0.0
            && grid.x <= (size_x - 1) as f32
            && grid.y <= (size_z - 1) as f32;
        (inside && !data.hole_at(grid.x, grid.y))
            .then(|| self.world_y(data.height_at(grid.x, grid.y)))
    }

    /// The surface's direction at sample (`x`, `z`), from its neighbours'
    /// heights, one unit long and pointing up.
    pub fn normal(&self, data: &TerrainData, x: usize, z: usize) -> Vec3 {
        let (x, z) = (x as isize, z as isize);
        let cell = self.cell_size(data);
        let dx = (data.height(x + 1, z) - data.height(x - 1, z)) * self.size.y / (2.0 * cell.x);
        let dz = (data.height(x, z + 1) - data.height(x, z - 1)) * self.size.y / (2.0 * cell.y);
        Vec3::new(-dx, 1.0, -dz).normalize()
    }

    /// The slope at sample (`x`, `z`) in degrees: 0 is flat, 90 a wall.
    pub fn slope_degrees(&self, data: &TerrainData, x: usize, z: usize) -> f32 {
        self.normal(data, x, z)
            .y
            .clamp(-1.0, 1.0)
            .acos()
            .to_degrees()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placement() -> Placement {
        Placement {
            position: Vec3::new(100.0, -10.0, 50.0),
            size: Vec3::new(64.0, 20.0, 32.0),
        }
    }

    #[test]
    fn world_and_grid_coordinates_map_both_ways() {
        let data = TerrainData::flat(17, 9, 0.5).unwrap();
        let placement = placement();
        assert_eq!(placement.cell_size(&data), Vec2::new(4.0, 4.0));
        // The corner of the footprint is sample (0, 0); the middle is the middle.
        assert_eq!(placement.to_grid(&data, 68.0, 34.0), Vec2::ZERO);
        assert_eq!(placement.to_grid(&data, 100.0, 50.0), Vec2::new(8.0, 4.0));
        let back = placement.to_world_xz(&data, Vec2::new(8.0, 4.0));
        assert_eq!(back, Vec2::new(100.0, 50.0));
        assert_eq!(placement.world_y(0.5), 0.0);
        assert_eq!(placement.height_of(0.0), 0.5);
        assert_eq!(placement.ground_y(&data, 100.0, 50.0), Some(0.0));
        assert_eq!(placement.ground_y(&data, 0.0, 50.0), None);
        assert_eq!(
            placement.sample_point(&data, 16, 8),
            Vec3::new(132.0, 0.0, 66.0)
        );
    }

    #[test]
    fn a_ramp_has_its_slope() {
        // Rises 4 studs per 4-stud cell along X: 45 degrees.
        let mut data = TerrainData::flat(17, 9, 0.0).unwrap();
        let all = data.bounds();
        let heights = data.heights_mut_in(all);
        for z in 0..9 {
            for x in 0..17 {
                heights[z * 17 + x] = x as f32 * 4.0 / 80.0;
            }
        }
        let placement = Placement {
            size: Vec3::new(64.0, 80.0, 32.0),
            ..placement()
        };
        assert!((placement.slope_degrees(&data, 5, 4) - 45.0).abs() < 0.01);
        let normal = placement.normal(&data, 5, 4);
        assert!(normal.x < 0.0 && normal.y > 0.0 && normal.z.abs() < 1e-6);
    }

    #[test]
    fn only_terrains_with_a_real_size_have_a_placement() {
        let mut world = axcenity_core::World::new();
        let terrain = world.create(ClassName::Terrain).unwrap();
        assert!(Placement::of(world.get(terrain).unwrap()).is_some());
        world
            .set_property(
                terrain,
                "Size",
                Value::Vector3(axcenity_core::Vector3::new(0.0, 1.0, 1.0)),
            )
            .unwrap();
        assert!(Placement::of(world.get(terrain).unwrap()).is_none());
        let part = world.create(ClassName::Part).unwrap();
        assert!(Placement::of(world.get(part).unwrap()).is_none());
    }
}
