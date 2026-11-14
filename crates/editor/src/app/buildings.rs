//! BuildingGenerators in the editor (procedural worlds PW6): Insert puts
//! one in front of the camera, standing on what the view looks at, or in
//! the selected Spline. What it makes is made each frame like a
//! SplineMesh's pieces (`spline_content.rs`), and Bake keeps it.

use axcenity_core::{ClassName, InstanceId, Value, Vector3};

use super::EditorApp;
use crate::tree::Insertable;

impl EditorApp {
    /// Insert > Building: into the Spline `near` is (or is a point of),
    /// whose points are then its corners, or standing free in front of the
    /// camera. Selected, so its handles move, turn and size its footprint.
    pub(super) fn insert_building(&mut self, near: Option<InstanceId>) {
        let what = Insertable::BuildingGenerator;
        let spline = near.and_then(|id| {
            let instance = self.world.get(id)?;
            match instance.class() {
                ClassName::Spline => Some(id),
                ClassName::SplinePoint => instance.parent(),
                _ => None,
            }
        });
        if let Some(spline) = spline {
            self.insert_named(what, Some(spline), "Building", Vec::new());
            self.generate_frame();
            return;
        }
        let defaults = what.class().default_properties();
        let size = defaults
            .iter()
            .find(|(name, _)| *name == "Size")
            .and_then(|(_, value)| value.as_vector3())
            .unwrap_or(Vector3::new(24.0, 1.0, 16.0));
        let spot = self.viewport.spawn_spot(&self.world);
        let tidy = |value: f32| (self.snap.studs(value) * 1000.0).round() / 1000.0 + 0.0;
        let size = axcenity_render::glam::Vec3::new(size.x, size.y, size.z);
        // Upright on whatever it stands on, however that slopes.
        let spot = crate::spawning::Spot {
            normal: axcenity_render::glam::Vec3::Y,
            ..spot
        };
        let at = crate::spawning::center(spot, size, tidy);
        self.insert_named(
            what,
            None,
            "Building",
            vec![("Position", Value::Vector3(Vector3::new(at.x, at.y, at.z)))],
        );
        self.generate_frame();
    }
}
