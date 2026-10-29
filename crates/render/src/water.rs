//! Water surfaces (graphics Phase 29): how a water material looks and
//! moves ([`Water`], a [`crate::Material`]'s `water`), and the grid its
//! waves are drawn on ([`water_surface`]). Drawing: `renderer/water.rs`
//! and `water.wgsl`; `docs/rendering/water.md`.

use std::sync::{Arc, OnceLock};

use crate::MeshSource;

/// How a water surface looks and moves. Colors are sRGB, 0 to 1. Shallow
/// water shows what's under it, tinted by the material's `base_color`;
/// the deeper it is, the more of `deep_color` takes its place.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Water {
    /// The color the water itself scatters, which deep water shows.
    pub deep_color: [f32; 3],
    /// How many studs of water light crosses before only about a third of
    /// what's under it still shows (e⁻¹). Bigger is clearer.
    pub clarity: f32,
    /// How high the biggest wave rises above the surface at rest, in studs.
    pub wave_height: f32,
    /// How long the biggest wave is from crest to crest, in studs. Smaller
    /// waves of shorter lengths ride on it.
    pub wave_length: f32,
    /// How fast the waves roll, as a share of how fast real waves of their
    /// length would (1). 0 stills them.
    pub wave_speed: f32,
    /// Which way the waves roll, in degrees: 0 toward +X, 90 toward +Z.
    pub wave_direction: f32,
    /// How far from where the water meets something the foam reaches, in
    /// studs of depth. 0 is none.
    pub foam: f32,
    /// How far the waves bend what's seen under them, in studs.
    pub refraction: f32,
}

impl Default for Water {
    fn default() -> Self {
        Self {
            deep_color: [0.02, 0.16, 0.22],
            clarity: 4.0,
            wave_height: 0.25,
            wave_length: 12.0,
            wave_speed: 1.0,
            wave_direction: 0.0,
            foam: 0.5,
            refraction: 0.3,
        }
    }
}

/// How many cells across the water grid is, each way.
pub const WATER_GRID: u32 = 64;

/// The mesh a water surface is drawn with: the top face of the 1×1×1 cube
/// (at y = 0.5, facing up), cut into [`WATER_GRID`]² squares so the vertex
/// shader can raise it into waves. A part's transform sizes it like any
/// built-in shape, so the water is the top of the part's box.
pub fn water_surface() -> Arc<dyn MeshSource> {
    static GRID: OnceLock<Arc<WaterGrid>> = OnceLock::new();
    GRID.get_or_init(|| Arc::new(WaterGrid::new())).clone()
}

struct WaterGrid {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    tangents: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

impl WaterGrid {
    fn new() -> Self {
        let cells = WATER_GRID;
        let corners = cells + 1;
        let mut grid = WaterGrid {
            positions: Vec::new(),
            normals: Vec::new(),
            uvs: Vec::new(),
            tangents: Vec::new(),
            indices: Vec::new(),
        };
        for j in 0..corners {
            for i in 0..corners {
                let (u, v) = (i as f32 / cells as f32, j as f32 / cells as f32);
                grid.positions.push([u - 0.5, 0.5, v - 0.5]);
                grid.normals.push([0.0, 1.0, 0.0]);
                grid.uvs.push([u, v]);
                // U runs along +X and V along +Z, which is cross(up, +X) * -1.
                grid.tangents.push([1.0, 0.0, 0.0, -1.0]);
            }
        }
        for j in 0..cells {
            for i in 0..cells {
                let at = |i: u32, j: u32| j * corners + i;
                let (a, b, c, d) = (at(i, j), at(i, j + 1), at(i + 1, j), at(i + 1, j + 1));
                // Counter-clockwise seen from above.
                grid.indices.extend([a, b, c, c, b, d]);
            }
        }
        grid
    }
}

impl MeshSource for WaterGrid {
    fn key(&self) -> u64 {
        // "WATERGRD", and the grid's size, so a different grid is a new mesh.
        0x5741_5445_5247_5244 ^ u64::from(WATER_GRID)
    }
    fn positions(&self) -> &[[f32; 3]] {
        &self.positions
    }
    fn normals(&self) -> &[[f32; 3]] {
        &self.normals
    }
    fn uvs(&self) -> &[[f32; 2]] {
        &self.uvs
    }
    fn tangents(&self) -> &[[f32; 4]] {
        &self.tangents
    }
    fn indices(&self) -> &[u32] {
        &self.indices
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_water_grid_is_the_top_of_the_unit_box_facing_up() {
        let grid = water_surface();
        let corners = (WATER_GRID + 1) as usize;
        assert_eq!(grid.positions().len(), corners * corners);
        assert_eq!(grid.indices().len(), (WATER_GRID * WATER_GRID * 6) as usize);
        assert!(grid.positions().iter().all(|p| p[1] == 0.5));
        let (min, max) = grid
            .positions()
            .iter()
            .fold((1.0f32, -1.0f32), |(lo, hi), p| {
                (lo.min(p[0]).min(p[2]), hi.max(p[0]).max(p[2]))
            });
        assert_eq!((min, max), (-0.5, 0.5));
        // Every triangle faces up (counter-clockwise from above).
        let p = |index: u32| glam::Vec3::from(grid.positions()[index as usize]);
        for triangle in grid.indices().chunks(3) {
            let normal = (p(triangle[1]) - p(triangle[0])).cross(p(triangle[2]) - p(triangle[0]));
            assert!(normal.y > 0.0, "{triangle:?}");
        }
        assert_eq!(grid.key(), water_surface().key(), "one grid, one key");
    }
}
