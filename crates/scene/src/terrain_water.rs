//! Terrain water (LE4, `docs/terrain.md`): a Terrain with `Water` on is
//! flooded up to its `WaterLevel` (world Y), drawn as the renderer's water
//! (`render::Water`, `docs/rendering/water.md`) over every chunk whose
//! lowest ground is below that level, so seas, lakes and rivers fill the
//! hollows the ground leaves under it, with foam where they meet it. Each
//! chunk's water is the top of a box as wide as the chunk, cut into the
//! water grid, and the waves follow world positions, so chunks meet
//! seamlessly.

use std::collections::HashMap;

use axcenity_core::{Instance, TerrainData, Value};
use axcenity_render::glam::{Mat4, Quat, Vec3};
use axcenity_render::{DrawItem, Material, Shape, Water};
use axcenity_terrain::Placement;

/// The water a Terrain asks for, or `None` while its `Water` is off.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TerrainWater {
    /// World Y of the water's surface at rest.
    pub(crate) level: f32,
    pub(crate) water: Water,
}

impl TerrainWater {
    pub(crate) fn of(terrain: &Instance) -> Option<Self> {
        if terrain.property("Water").and_then(Value::as_bool) != Some(true) {
            return None;
        }
        let number = |name: &str, otherwise: f32| {
            terrain
                .property(name)
                .and_then(Value::as_number)
                .map(|value| value as f32)
                .filter(|value| value.is_finite())
                .unwrap_or(otherwise)
        };
        let defaults = Water::default();
        let deep_color = terrain
            .property("WaterColor")
            .and_then(Value::as_color3)
            .map_or(defaults.deep_color, |color| [color.r, color.g, color.b]);
        Some(Self {
            level: number("WaterLevel", 0.0),
            water: Water {
                deep_color,
                clarity: number("WaterClarity", defaults.clarity).max(0.01),
                wave_height: number("WaveHeight", defaults.wave_height).max(0.0),
                wave_length: number("WaveLength", defaults.wave_length).max(0.5),
                wave_speed: number("WaveSpeed", defaults.wave_speed).max(0.0),
                wave_direction: number("WaveDirection", defaults.wave_direction),
                ..defaults
            },
        })
    }
}

/// Each chunk's lowest ground (world Y), with the heights' version it was
/// worked out from, so it's found again only after an edit.
#[derive(Default)]
pub(crate) struct Lowest {
    chunks: HashMap<(usize, usize), (u64, f32)>,
}

impl Lowest {
    fn of(&mut self, data: &TerrainData, placement: &Placement, (cx, cz): (usize, usize)) -> f32 {
        let version = data.chunk_height_version(cx, cz);
        if let Some(&(built, lowest)) = self.chunks.get(&(cx, cz))
            && built == version
        {
            return lowest;
        }
        let rect = data.chunk_rect(cx, cz);
        let mut lowest = f32::INFINITY;
        for z in rect.z0..rect.z1 {
            for x in rect.x0..rect.x1 {
                lowest = lowest.min(data.height(x as isize, z as isize));
            }
        }
        let lowest = placement.world_y(lowest);
        self.chunks.insert((cx, cz), (version, lowest));
        lowest
    }
}

/// Adds the water over each chunk of `data` whose ground dips below
/// `water`'s level to `items`.
pub(crate) fn draw(
    water: &TerrainWater,
    data: &TerrainData,
    placement: &Placement,
    lowest: &mut Lowest,
    items: &mut Vec<DrawItem>,
) {
    let (chunks_x, chunks_z) = data.chunk_counts();
    let cell = placement.cell_size(data);
    for cz in 0..chunks_z {
        for cx in 0..chunks_x {
            if lowest.of(data, placement, (cx, cz)) >= water.level {
                continue;
            }
            let rect = data.chunk_rect(cx, cz);
            // The chunk's footprint: its rect ends one past its last sample,
            // which it shares with the next chunk.
            let x1 = (rect.x1 - 1).max(rect.x0 + 1);
            let z1 = (rect.z1 - 1).max(rect.z0 + 1);
            let first = placement.sample_point(data, rect.x0, rect.z0);
            let width = (x1 - rect.x0) as f32 * cell.x;
            let depth = (z1 - rect.z0) as f32 * cell.y;
            let middle = Vec3::new(
                first.x + width * 0.5,
                water.level - 0.5,
                first.z + depth * 0.5,
            );
            items.push(water_item(
                water,
                Mat4::from_scale_rotation_translation(
                    Vec3::new(width, 1.0, depth),
                    Quat::IDENTITY,
                    middle,
                ),
            ));
        }
    }
}

/// One chunk's water: the top of the box `transform` makes of the unit box.
fn water_item(water: &TerrainWater, transform: Mat4) -> DrawItem {
    DrawItem {
        shape: Shape::Mesh(axcenity_render::water_surface()),
        transform,
        material: Material {
            water: Some(water.water),
            ..Material::default()
        },
        uv_tile: 0.0,
        casts_shadow: false,
        id: 0,
        cull_distance: 0.0,
        skin: None,
        sway: None,
        decal_role: Default::default(),
    }
}
