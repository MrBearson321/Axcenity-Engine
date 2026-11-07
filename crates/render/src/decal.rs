//! Decals in the draw list: pictures laid onto surfaces.
//!
//! A [`Decal`] is a box that projects its texture onto the solid surfaces
//! inside it (a `ProjectedDecal`), drawn on screen from the depth and
//! normal prepass, so it follows any shape with no gaps and never fights
//! the surface for depth. A Part's `Decal` on one face is an ordinary
//! [`crate::DrawItem`] whose [`DecalRole`] is [`DecalRole::Decal`], laid on
//! [`decal_quad`]. See `docs/rendering/decals.md`.

use std::sync::{Arc, OnceLock};

use glam::{Mat4, Vec3};

use crate::{MeshSource, TextureSource};

/// A box that projects its texture onto the solid surfaces inside it.
#[derive(Clone, Debug)]
pub struct Decal {
    /// Places the 1×1×1 box centered on the origin, as a draw item's
    /// transform does: its scale is the box's size. The picture lies on the
    /// box's XY plane and faces its -Z side (a CFrame's `LookVector`), and
    /// is projected along Z through the whole box. Its top is toward +Y.
    pub transform: Mat4,
    /// The picture, whose alpha is its shape. `None` is a plain square.
    pub texture: Option<Arc<dyn TextureSource>>,
    /// An sRGB color that tints the texture.
    pub color: [f32; 3],
    /// 1 is solid, 0 invisible. Multiplies the texture's alpha.
    pub opacity: f32,
    /// Degrees a surface may turn away from the picture's facing before
    /// the decal is gone from it. It fades out over the second half of
    /// that turn, so the sides of what's inside the box aren't smeared.
    pub fade_angle: f32,
    /// Leaves characters' items ([`DecalRole::Character`]) bare.
    pub ignore_characters: bool,
}

impl Decal {
    /// The world-space middle of the box and the radius of a sphere around
    /// it, for culling.
    pub fn sphere(&self) -> (Vec3, f32) {
        let center = self.transform.w_axis.truncate();
        let half = 0.5
            * (self.transform.x_axis.truncate().length()
                + self.transform.y_axis.truncate().length()
                + self.transform.z_axis.truncate().length());
        (center, half)
    }
}

/// How a draw item meets decals.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DecalRole {
    /// Projected decals land on it, if it's solid.
    #[default]
    Surface,
    /// A character's: projected decals that ignore characters leave it bare.
    Character,
    /// A decal itself (a Part's `Decal`): drawn with the see-through items,
    /// blended by its texture's alpha even when solid, and drawn a hair
    /// toward the camera so it never fights the surface beneath for depth.
    /// It casts no shadow, and projected decals don't land on it.
    Decal,
}

/// The mesh a Part's `Decal` is drawn on: a 1×1 square in the XY plane at
/// z = 0, facing +Z, with the texture's top toward +Y and unmirrored seen
/// from the front.
pub fn decal_quad() -> Arc<dyn MeshSource> {
    static QUAD: OnceLock<Arc<DecalQuad>> = OnceLock::new();
    QUAD.get_or_init(|| Arc::new(DecalQuad)).clone()
}

struct DecalQuad;

impl MeshSource for DecalQuad {
    fn key(&self) -> u64 {
        // "DECALQUD".
        0x4445_4341_4c51_5544
    }
    fn positions(&self) -> &[[f32; 3]] {
        &[
            [-0.5, 0.5, 0.0],
            [0.5, 0.5, 0.0],
            [0.5, -0.5, 0.0],
            [-0.5, -0.5, 0.0],
        ]
    }
    fn normals(&self) -> &[[f32; 3]] {
        &[[0.0, 0.0, 1.0]; 4]
    }
    fn uvs(&self) -> &[[f32; 2]] {
        &[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]
    }
    fn tangents(&self) -> &[[f32; 4]] {
        // U runs along +X and V down (-Y): cross(+Z, +X) is +Y, so w is -1.
        &[[1.0, 0.0, 0.0, -1.0]; 4]
    }
    fn indices(&self) -> &[u32] {
        // Counter-clockwise seen from +Z.
        &[0, 3, 2, 0, 2, 1]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_decal_quad_faces_plus_z_counter_clockwise_with_its_top_up() {
        let quad = decal_quad();
        let p = quad.positions();
        for triangle in quad.indices().chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(p[triangle[i] as usize]));
            assert!((b - a).cross(c - a).z > 0.0, "{triangle:?} faces +Z");
        }
        // The texture's top (v = 0) is at the top (+Y).
        for (position, uv) in p.iter().zip(quad.uvs()) {
            assert_eq!(uv[1] == 0.0, position[1] > 0.0);
            assert_eq!(uv[0] == 0.0, position[0] < 0.0);
        }
    }

    #[test]
    fn a_decals_sphere_holds_its_whole_box() {
        let decal = Decal {
            transform: Mat4::from_scale_rotation_translation(
                Vec3::new(2.0, 4.0, 1.0),
                glam::Quat::from_rotation_y(0.7),
                Vec3::new(5.0, 0.0, -3.0),
            ),
            texture: None,
            color: [1.0; 3],
            opacity: 1.0,
            fade_angle: 60.0,
            ignore_characters: true,
        };
        let (center, radius) = decal.sphere();
        assert_eq!(center, Vec3::new(5.0, 0.0, -3.0));
        for corner in 0..8 {
            let unit = Vec3::new(
                if corner & 1 == 0 { -0.5 } else { 0.5 },
                if corner & 2 == 0 { -0.5 } else { 0.5 },
                if corner & 4 == 0 { -0.5 } else { 0.5 },
            );
            let world = decal.transform.transform_point3(unit);
            assert!(world.distance(center) <= radius + 1e-4);
        }
    }
}
