//! A Part's `Orientation` (degrees around X, Y and Z) and the rotation it
//! stands for, both ways.
//!
//! Like Roblox, the part turns around Z first, then X, then Y. glam calls
//! that order `EulerRot::YXZ` and takes the angles as (y, x, z).

use axcenity_core::Vector3;
use axcenity_render::glam::{EulerRot, Quat};

/// The rotation an `Orientation` in degrees stands for.
pub fn rotation_from_orientation(orientation: Vector3) -> Quat {
    let [x, y, z] = [orientation.x, orientation.y, orientation.z].map(f32::to_radians);
    Quat::from_euler(EulerRot::YXZ, y, x, z)
}

/// The `Orientation` in degrees for a rotation: X from -90 to 90, Y and Z
/// from -180 to 180, as Roblox gives them. Each angle is rounded to 3
/// decimals, so turning by 15° gives 15 and not 14.999999.
pub fn orientation_from_rotation(rotation: Quat) -> Vector3 {
    let (y, x, z) = rotation.to_euler(EulerRot::YXZ);
    // `+ 0.0` turns -0.0 into 0.0, which reads better in world.json.
    let tidy = |radians: f32| (radians.to_degrees() * 1000.0).round() / 1000.0 + 0.0;
    Vector3::new(tidy(x), tidy(y), tidy(z))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_render::glam::Vec3;

    #[test]
    fn orientation_round_trips_through_a_rotation() {
        for (x, y, z) in [(0.0, 0.0, 0.0), (30.0, 45.0, -60.0), (-80.0, 170.0, 10.5)] {
            let orientation = Vector3::new(x, y, z);
            let back = orientation_from_rotation(rotation_from_orientation(orientation));
            assert_eq!(back, orientation);
        }
    }

    #[test]
    fn turning_a_quarter_around_y_points_x_at_minus_z() {
        let rotation = rotation_from_orientation(Vector3::new(0.0, 90.0, 0.0));
        assert!((rotation * Vec3::X).abs_diff_eq(Vec3::NEG_Z, 1e-6));
        let turned = Quat::from_rotation_y(15f32.to_radians());
        assert_eq!(
            orientation_from_rotation(turned),
            Vector3::new(0.0, 15.0, 0.0)
        );
    }
}
