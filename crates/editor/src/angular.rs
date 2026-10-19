//! The Angular tool's geometry, modelled on Archimedes: copies of a part,
//! each placed against the end of the one before and turned by an angle,
//! so a run of copies bends into an arc (an arch, a loop, a curved road or
//! a round tower).
//!
//! Each copy is the part slid forward by its own length along the extend
//! direction, then turned around the bend axis through the edge of the end
//! face on the inside of the bend. So neighbours share that inner edge and
//! a wedge-shaped gap opens on the outside, as in Archimedes. Because every
//! step is the same move relative to the part, `360° / angle` blocks close
//! a ring exactly.

use axcenity_render::glam::{Quat, Vec3};

use crate::gizmo::{Axis, Pose, Side};

/// The Angular tool's settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AngularSettings {
    /// Which of the part's own faces each copy goes against.
    pub extend: (Axis, Side),
    /// The part's own axis the copies turn around. It can't be the extend
    /// axis; if it is, the next axis round is used.
    pub bend: Axis,
    /// How far each copy turns from the one before, in degrees. The sign
    /// picks which way the run bends.
    pub degrees: f32,
    /// How many copies one press makes.
    pub count: u32,
}

impl Default for AngularSettings {
    /// Forward along the part's length (X), bending up (around Z) in 15°
    /// steps, one copy at a time.
    fn default() -> Self {
        Self {
            extend: (Axis::X, Side::Plus),
            bend: Axis::Z,
            degrees: 15.0,
            count: 1,
        }
    }
}

impl AngularSettings {
    /// The most copies one press makes, so a typo can't freeze the editor.
    pub const MAX_COUNT: u32 = 360;

    /// The bend axis actually used: never the extend axis.
    pub fn bend_axis(&self) -> Axis {
        if self.bend != self.extend.0 {
            return self.bend;
        }
        match self.extend.0 {
            Axis::X => Axis::Z,
            Axis::Y => Axis::X,
            Axis::Z => Axis::Y,
        }
    }

    /// How many copies, with the part itself, make a closed ring: `None`
    /// if the angle doesn't divide a whole turn (to within a hundredth).
    pub fn copies_to_close(&self) -> Option<u32> {
        let angle = self.degrees.abs();
        if angle < 0.5 {
            return None;
        }
        let blocks = 360.0 / angle;
        ((blocks - blocks.round()).abs() < 0.01 && blocks.round() >= 2.0)
            .then(|| blocks.round() as u32 - 1)
    }
}

/// Where the next copy after a part at `pose` goes.
pub fn next_pose(pose: Pose, settings: &AngularSettings) -> Pose {
    let (axis, side) = settings.extend;
    let bend = settings.bend_axis();
    let local_forward = unit(axis) * sign(side);
    let local_bend = unit(bend);
    let forward = pose.rotation * local_forward;
    let length = pose.size[index(axis)];
    let slid = pose.position + forward * length;
    let radians = settings.degrees.to_radians();
    if radians == 0.0 {
        return Pose {
            position: slid,
            ..pose
        };
    }
    // Turning by a positive angle swings the far end toward bend × forward,
    // which is the inside of the bend.
    let local_inside = local_bend.cross(local_forward) * radians.signum();
    let thickness = pose.size.dot(local_inside.abs());
    let hinge =
        pose.position + forward * (length / 2.0) + pose.rotation * local_inside * (thickness / 2.0);
    let turn = Quat::from_axis_angle(pose.rotation * local_bend, radians);
    Pose {
        position: hinge + turn * (slid - hinge),
        rotation: (turn * pose.rotation).normalize(),
        size: pose.size,
    }
}

/// The poses of `settings.count` copies after a part at `pose`, each from
/// the one before.
pub fn chain(pose: Pose, settings: &AngularSettings) -> Vec<Pose> {
    let count = settings.count.min(AngularSettings::MAX_COUNT);
    let mut poses = Vec::with_capacity(count as usize);
    let mut last = pose;
    for _ in 0..count {
        last = next_pose(last, settings);
        poses.push(last);
    }
    poses
}

/// The 12 edges of the box at `pose`, for drawing a preview outline.
pub fn box_edges(pose: Pose) -> [(Vec3, Vec3); 12] {
    let half = pose.size / 2.0;
    let corner = |x: f32, y: f32, z: f32| {
        pose.position + pose.rotation * Vec3::new(x * half.x, y * half.y, z * half.z)
    };
    let c = [
        corner(-1.0, -1.0, -1.0),
        corner(1.0, -1.0, -1.0),
        corner(1.0, 1.0, -1.0),
        corner(-1.0, 1.0, -1.0),
        corner(-1.0, -1.0, 1.0),
        corner(1.0, -1.0, 1.0),
        corner(1.0, 1.0, 1.0),
        corner(-1.0, 1.0, 1.0),
    ];
    [
        (c[0], c[1]),
        (c[1], c[2]),
        (c[2], c[3]),
        (c[3], c[0]),
        (c[4], c[5]),
        (c[5], c[6]),
        (c[6], c[7]),
        (c[7], c[4]),
        (c[0], c[4]),
        (c[1], c[5]),
        (c[2], c[6]),
        (c[3], c[7]),
    ]
}

fn unit(axis: Axis) -> Vec3 {
    match axis {
        Axis::X => Vec3::X,
        Axis::Y => Vec3::Y,
        Axis::Z => Vec3::Z,
    }
}

fn index(axis: Axis) -> usize {
    match axis {
        Axis::X => 0,
        Axis::Y => 1,
        Axis::Z => 2,
    }
}

fn sign(side: Side) -> f32 {
    match side {
        Side::Plus => 1.0,
        Side::Minus => -1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(rotation: Quat) -> Pose {
        Pose {
            position: Vec3::new(3.0, 2.0, -1.0),
            rotation,
            size: Vec3::new(4.0, 1.0, 2.0),
        }
    }

    fn assert_near(a: Vec3, b: Vec3) {
        assert!(a.abs_diff_eq(b, 1e-3), "{a} is not close to {b}");
    }

    fn same_rotation(a: Quat, b: Quat) -> bool {
        a.dot(b).abs() > 1.0 - 1e-5
    }

    #[test]
    fn twenty_four_15_degree_blocks_close_a_ring() {
        let settings = AngularSettings {
            count: 24,
            ..AngularSettings::default()
        };
        assert_eq!(settings.copies_to_close(), Some(23));
        let start = block(Quat::IDENTITY);
        let poses = chain(start, &settings);
        assert_eq!(poses.len(), 24);
        // The 24th copy lands back on the part itself.
        assert_near(poses[23].position, start.position);
        assert!(same_rotation(poses[23].rotation, start.rotation));
    }

    #[test]
    fn neighbours_share_the_inner_edge() {
        let settings = AngularSettings::default();
        let start = block(Quat::IDENTITY);
        let next = next_pose(start, &settings);
        // Bending up around Z: the inner edge is the top edge of the +X face.
        let corner = |pose: Pose, x: f32, y: f32| {
            pose.position + pose.rotation * (pose.size / 2.0 * Vec3::new(x, y, 1.0))
        };
        assert_near(corner(start, 1.0, 1.0), corner(next, -1.0, 1.0));
        // The outer (bottom) edges open a gap.
        assert!(corner(start, 1.0, -1.0).distance(corner(next, -1.0, -1.0)) > 0.1);
    }

    #[test]
    fn every_extend_direction_and_bend_axis_works_on_a_turned_part() {
        let turned = Quat::from_euler(axcenity_render::glam::EulerRot::YXZ, 0.4, 0.7, -0.3);
        for axis in [Axis::X, Axis::Y, Axis::Z] {
            for side in [Side::Plus, Side::Minus] {
                for bend in [Axis::X, Axis::Y, Axis::Z] {
                    let settings = AngularSettings {
                        extend: (axis, side),
                        bend,
                        degrees: -30.0,
                        count: 12,
                    };
                    let start = block(turned);
                    let poses = chain(start, &settings);
                    assert_near(poses[11].position, start.position);
                    assert!(same_rotation(poses[11].rotation, start.rotation));
                    // The copies really go somewhere, and keep the size.
                    assert!(poses[0].position.distance(start.position) > 0.5);
                    assert_eq!(poses[5].size, start.size);
                }
            }
        }
    }

    #[test]
    fn no_angle_makes_a_straight_run() {
        let settings = AngularSettings {
            degrees: 0.0,
            count: 3,
            ..AngularSettings::default()
        };
        let poses = chain(block(Quat::IDENTITY), &settings);
        assert_near(poses[2].position, Vec3::new(15.0, 2.0, -1.0));
        assert_eq!(settings.copies_to_close(), None);
    }
}
