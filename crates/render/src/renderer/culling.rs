//! Frustum culling: what each view (the camera, a shadow cascade, a spot
//! light) can see.
//!
//! A view is a view-projection matrix. The box it maps to clip space is six
//! planes in world space, and a thing is culled when its bounding sphere is
//! wholly outside one of them.

use glam::{Mat4, Vec3, Vec4};

/// A sphere around something drawn: its center and radius, in world space.
pub(super) type Sphere = (Vec3, f32);

/// The space a view-projection matrix sees, as six planes facing inward.
#[derive(Clone, Copy, Debug)]
pub(super) struct Frustum {
    /// Each plane's normal (`xyz`, one unit long) and offset (`w`): a point
    /// `p` is inside when `normal · p + offset >= 0`.
    planes: [Vec4; 6],
}

impl Frustum {
    /// What `view_proj` maps into wgpu's clip-space box: x and y from -1 to
    /// 1, depth from 0 to 1. Each plane is a sum of the matrix's rows (the
    /// Gribb and Hartmann method), so a perspective or an orthographic view
    /// works the same way.
    pub(super) fn of(view_proj: Mat4) -> Self {
        let [x, y, z, w] = [0, 1, 2, 3].map(|row| view_proj.row(row));
        let planes = [w + x, w - x, w + y, w - y, z, w - z].map(|plane| {
            let length = plane.truncate().length();
            if length > 0.0 { plane / length } else { plane }
        });
        Self { planes }
    }

    /// False only when `sphere` is wholly outside. A sphere that isn't a
    /// number (NaN) counts as touching, so nothing is culled by mistake.
    pub(super) fn touches(&self, (center, radius): Sphere) -> bool {
        !self
            .planes
            .iter()
            .any(|plane| plane.truncate().dot(center) + plane.w < -radius)
    }
}

/// Every view a frame draws into: the camera, the sun's cascades, and the
/// lamps' shadow views, each limited to what its lamp can reach. What
/// touches none of them doesn't change the frame.
#[derive(Default)]
pub(super) struct Reach {
    pub(super) views: Vec<Frustum>,
    pub(super) lamps: Vec<(Sphere, Frustum)>,
}

impl Reach {
    pub(super) fn touches(&self, sphere: Sphere) -> bool {
        self.views.iter().any(|view| view.touches(sphere))
            || self
                .lamps
                .iter()
                .any(|(lamp, view)| spheres_touch(*lamp, sphere) && view.touches(sphere))
    }
}

/// What a shadow view's casters must touch.
#[derive(Clone, Copy)]
pub(super) enum ShadowView {
    /// One of the sun's cascades.
    Sun(Frustum),
    /// A lamp's view, limited to what the lamp can reach.
    Lamp(Sphere, Frustum),
    /// Not drawn this frame: a far cascade keeping what it drew last.
    Skipped,
}

impl ShadowView {
    pub(super) fn touches(&self, sphere: Sphere) -> bool {
        match self {
            ShadowView::Sun(view) => view.touches(sphere),
            ShadowView::Lamp(lamp, view) => spheres_touch(*lamp, sphere) && view.touches(sphere),
            ShadowView::Skipped => false,
        }
    }
}

/// Whether two spheres overlap.
pub(super) fn spheres_touch((a, a_radius): Sphere, (b, b_radius): Sphere) -> bool {
    a.distance(b) <= a_radius + b_radius
}

/// The box a mesh's corners span, in its own space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Bounds {
    pub(super) min: Vec3,
    pub(super) max: Vec3,
}

impl Bounds {
    /// The 1×1×1 cube centered on the origin, which every built-in shape
    /// fits.
    pub(super) const UNIT: Self = Self {
        min: Vec3::splat(-0.5),
        max: Vec3::splat(0.5),
    };

    /// The box around `points`, or the unit cube if there are none.
    pub(super) fn around(points: impl IntoIterator<Item = Vec3>) -> Self {
        let mut points = points.into_iter();
        let Some(first) = points.next() else {
            return Self::UNIT;
        };
        let (min, max) = points.fold((first, first), |(min, max), point| {
            (min.min(point), max.max(point))
        });
        Self { min, max }
    }

    /// A sphere around the box once `transform` places it: centered on the
    /// box's center, out to its farthest corner. A stretched or sheared box
    /// is a slanted one, whose corners are its center plus or minus each of
    /// its three edge directions, so the farthest is one of four sums.
    pub(super) fn sphere(&self, transform: Mat4) -> Sphere {
        let center = transform.transform_point3((self.min + self.max) * 0.5);
        let half = (self.max - self.min) * 0.5;
        let a = transform.x_axis.truncate() * half.x;
        let b = transform.y_axis.truncate() * half.y;
        let c = transform.z_axis.truncate() * half.z;
        let radius = [a + b + c, a + b - c, a - b + c, a - b - c]
            .map(Vec3::length)
            .into_iter()
            .fold(0.0, f32::max);
        (center, radius)
    }
}

/// An item's bounds and whether the camera drew it, kept for the editor's
/// culling overlay (see [`crate::TextureRenderer::freeze_culling`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CullingBounds {
    pub center: Vec3,
    pub radius: f32,
    /// False when it was outside the view, so it was culled.
    pub drawn: bool,
}
