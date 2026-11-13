//! Light probe volumes (graphics Phase 19): where a `LightProbeVolume`'s
//! probes stand, the volume as the renderer draws it, and a fingerprint of
//! the world so the editor can tell when a bake is out of date.

use std::fmt::Write as _;

use axcenity_assets::{Assets, MAX_LIGHT_PROBES_PER_SIDE};
use axcenity_core::{ClassName, Instance, InstanceId, Value, World};
use axcenity_render::glam::{Mat4, Vec3};
use axcenity_render::{LightProbeGrid, LightProbeVolume};

use crate::draw_list::part_placement;

/// Studs between probes when a volume's `Spacing` isn't a usable number.
const DEFAULT_SPACING: f32 = 4.0;

/// Where a volume's probes stand, to bake them.
#[derive(Clone, Debug, PartialEq)]
pub struct LightProbePoints {
    /// How many stand along the box's own X, Y and Z.
    pub counts: [u32; 3],
    /// Each one's place in the world, X first, then Y, then Z.
    pub points: Vec<Vec3>,
}

/// Where the probes of the LightProbeVolume `volume` stand: a grid filling
/// its parent part's box, about `Spacing` studs apart (at least one and at
/// most [`MAX_LIGHT_PROBES_PER_SIDE`] along each side), each in the middle
/// of its cell. `None` if it isn't a LightProbeVolume inside a part.
pub fn light_probe_points(world: &World, volume: InstanceId) -> Option<LightProbePoints> {
    let instance = world.get(volume)?;
    if instance.class() != ClassName::LightProbeVolume {
        return None;
    }
    let (_, transform) = part_placement(world.get(instance.parent()?)?)?;
    let spacing = instance
        .property("Spacing")
        .and_then(Value::as_number)
        .map(|spacing| spacing as f32)
        .filter(|spacing| spacing.is_finite() && *spacing > 0.0)
        .unwrap_or(DEFAULT_SPACING);
    let size = transform.to_scale_rotation_translation().0.abs();
    let counts = size
        .to_array()
        .map(|side| ((side / spacing).ceil() as u32).clamp(1, MAX_LIGHT_PROBES_PER_SIDE));
    Some(LightProbePoints {
        counts,
        points: grid_points(transform, counts),
    })
}

/// The middle of each cell of a `counts` grid filling the unit box that
/// `transform` places, X first, then Y, then Z.
fn grid_points(transform: Mat4, counts: [u32; 3]) -> Vec<Vec3> {
    let along = Vec3::from_array(counts.map(|count| count as f32));
    let mut points = Vec::new();
    for z in 0..counts[2] {
        for y in 0..counts[1] {
            for x in 0..counts[0] {
                let cell = (Vec3::new(x as f32, y as f32, z as f32) + 0.5) / along - 0.5;
                points.push(transform.transform_point3(cell));
            }
        }
    }
    points
}

/// The box a LightProbeVolume lights: its parent part's, with the bake its
/// `Bake` names. `None` if it isn't inside a part, or has no bake that can
/// be read (a missing file is reported once by `assets`), so the sky
/// lights it instead.
pub(crate) fn light_probe_volume_of(
    world: &World,
    instance: &Instance,
    assets: &Assets,
) -> Option<LightProbeVolume> {
    let (_, transform) = part_placement(world.get(instance.parent()?)?)?;
    let Some(Value::String(path)) = instance.property("Bake") else {
        return None;
    };
    let probes = assets.light_probes(path)?;
    let (size, rotation, center) = transform.to_scale_rotation_translation();
    let blend_distance = instance
        .property("BlendDistance")
        .and_then(Value::as_number)
        .unwrap_or(0.0) as f32;
    Some(LightProbeVolume {
        center,
        rotation,
        half_size: size.abs() * 0.5,
        blend_distance: blend_distance.max(0.0),
        probes: LightProbeGrid {
            key: probes.key(),
            counts: probes.counts,
            probes: probes.probes.clone(),
            invalid: probes.invalid.clone(),
        },
    })
}

/// A number standing for what a light probe bake sees of `world`: every
/// Instance in the Workspace and Lighting (its class, name, place in the
/// tree and properties, and a terrain's shape, paint and foliage), but not
/// scripts or any probe's `Bake`, which baking sets. The same world always
/// gives the same number, in any build, so it can be saved in a bake and
/// compared after the world is opened again.
pub fn light_bake_fingerprint(world: &World) -> u64 {
    let mut hash = Fnv::default();
    for root in [world.workspace(), world.lighting()] {
        hash_tree(world, root, &mut hash);
    }
    hash.0
}

fn hash_tree(world: &World, id: InstanceId, hash: &mut Fnv) {
    let Some(instance) = world.get(id) else {
        return;
    };
    let class = instance.class();
    if matches!(
        class,
        ClassName::Script | ClassName::LocalScript | ClassName::ModuleScript
    ) {
        return;
    }
    let _ = write!(hash, "<{}:{}", class.as_str(), instance.name());
    for (name, value) in instance.properties() {
        if name != "Bake" {
            let _ = write!(hash, "|{name}={value:?}");
        }
    }
    if let Some(terrain) = world.terrain(id) {
        for height in terrain.heights() {
            hash.bytes(&height.to_bits().to_le_bytes());
        }
        hash.bytes(terrain.paint().as_flattened());
        let _ = write!(hash, "{:?}", terrain.foliage());
    }
    for &child in instance.children() {
        hash_tree(world, child, hash);
    }
    hash.bytes(b">");
}

/// The 64-bit FNV-1a hash, which (unlike the standard library's hasher)
/// never changes between builds.
struct Fnv(u64);

impl Default for Fnv {
    fn default() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
}

impl Fnv {
    fn bytes(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 = (self.0 ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
    }
}

impl std::fmt::Write for Fnv {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        self.bytes(text.as_bytes());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use axcenity_core::Vector3;

    use super::*;
    use crate::build_draw_list;

    fn vector(x: f32, y: f32, z: f32) -> Value {
        Value::Vector3(Vector3::new(x, y, z))
    }

    fn room(world: &mut World) -> (InstanceId, InstanceId) {
        let part = world.create(ClassName::Part).unwrap();
        world.set_parent(part, Some(world.workspace())).unwrap();
        for (name, value) in [
            ("Size", vector(12.0, 6.0, 8.0)),
            ("Position", vector(1.0, 3.0, -2.0)),
            ("Orientation", vector(0.0, 90.0, 0.0)),
        ] {
            world.set_property(part, name, value).unwrap();
        }
        let volume = world.create(ClassName::LightProbeVolume).unwrap();
        world.set_parent(volume, Some(part)).unwrap();
        (part, volume)
    }

    fn camera() -> axcenity_render::Camera {
        axcenity_render::Camera {
            position: Vec3::new(0.0, 5.0, 10.0),
            look_at: Vec3::ZERO,
            fov_y_degrees: 70.0,
        }
    }

    #[test]
    fn probes_fill_the_parts_box_about_spacing_apart() {
        let mut world = World::new();
        let (_, volume) = room(&mut world);
        let grid = light_probe_points(&world, volume).unwrap();
        // 12, 6 and 8 studs at 4 apart.
        assert_eq!(grid.counts, [3, 2, 2]);
        assert_eq!(grid.points.len(), 12);
        // The first is in the corner cell, 2 studs in from each end: 4 from
        // the middle toward the box's own -X end (which the 90° turn puts
        // toward +Z), 1.5 below it, and 2 toward its -Z end (toward -X).
        let first = grid.points[0];
        assert!(
            first.abs_diff_eq(Vec3::new(1.0 - 2.0, 1.5, -2.0 + 4.0), 1e-4),
            "{first}"
        );
        // Their average is the box's middle.
        let middle = grid.points.iter().sum::<Vec3>() / 12.0;
        assert!(
            middle.abs_diff_eq(Vec3::new(1.0, 3.0, -2.0), 1e-4),
            "{middle}"
        );

        world
            .set_property(volume, "Spacing", Value::Number(0.01))
            .unwrap();
        let fine = light_probe_points(&world, volume).unwrap();
        assert_eq!(
            fine.counts, [MAX_LIGHT_PROBES_PER_SIDE; 3],
            "at most 16 a side"
        );
        world
            .set_property(volume, "Spacing", Value::Number(-1.0))
            .unwrap();
        assert_eq!(
            light_probe_points(&world, volume).unwrap().counts,
            [3, 2, 2]
        );
    }

    #[test]
    fn a_volume_lights_its_parts_box_and_a_missing_bake_falls_back_to_the_sky() {
        let folder = std::env::temp_dir().join(format!(
            "axcenity-scene-light-probes-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&folder);
        let path = axcenity_assets::light_probes_path("Hall");
        let baked =
            axcenity_assets::LightProbes::new([3, 2, 2], 7, vec![[[0.5; 3]; 4]; 12]).unwrap();
        axcenity_assets::write_light_probes(&folder, &path, &baked).unwrap();
        let assets = Assets::new(&folder);
        let mut world = World::new();
        let (_, volume) = room(&mut world);
        let volumes = |world: &World| build_draw_list(world, &assets, camera()).light_probe_volumes;
        assert!(volumes(&world).is_empty(), "not baked yet: the sky");

        world
            .set_property(volume, "Bake", Value::String(path.clone()))
            .unwrap();
        let drawn = volumes(&world);
        assert_eq!(drawn.len(), 1);
        assert_eq!(drawn[0].center, Vec3::new(1.0, 3.0, -2.0));
        assert!(
            drawn[0]
                .half_size
                .abs_diff_eq(Vec3::new(6.0, 3.0, 4.0), 1e-5)
        );
        assert!((drawn[0].rotation * Vec3::X).abs_diff_eq(Vec3::NEG_Z, 1e-5));
        assert_eq!(drawn[0].blend_distance, 2.0);
        assert_eq!(drawn[0].probes.counts, [3, 2, 2]);
        assert_eq!(
            volumes(&world)[0].probes.key,
            drawn[0].probes.key,
            "read once"
        );
        assert!(assets.take_errors().is_empty());

        world
            .set_property(
                volume,
                "Bake",
                Value::String("assets/probes/gone.lightprobes.json".into()),
            )
            .unwrap();
        assert!(volumes(&world).is_empty(), "a missing bake: the sky");
        assert!(volumes(&world).is_empty());
        let errors = assets.take_errors();
        assert_eq!(errors.len(), 1, "reported once: {errors:?}");
        assert!(errors[0].contains("gone.lightprobes.json"), "{errors:?}");
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn the_fingerprint_changes_with_the_world_but_not_with_bakes_or_scripts() {
        let mut world = World::new();
        let (part, volume) = room(&mut world);
        let before = light_bake_fingerprint(&world);
        assert_eq!(light_bake_fingerprint(&world), before, "the same world");

        world
            .set_property(
                volume,
                "Bake",
                Value::String("assets/probes/a.lightprobes.json".into()),
            )
            .unwrap();
        let script = world.create(ClassName::Script).unwrap();
        world.set_parent(script, Some(world.workspace())).unwrap();
        assert_eq!(
            light_bake_fingerprint(&world),
            before,
            "baking and scripts don't count"
        );

        world
            .set_property(part, "Position", vector(1.0, 3.5, -2.0))
            .unwrap();
        let moved = light_bake_fingerprint(&world);
        assert_ne!(moved, before, "a part moved");
        world
            .set_property(part, "Position", vector(1.0, 3.0, -2.0))
            .unwrap();
        assert_eq!(light_bake_fingerprint(&world), before, "and moved back");

        let lighting = world.lighting();
        world
            .set_property(lighting, "EnvironmentIntensity", Value::Number(0.2))
            .unwrap();
        assert_ne!(
            light_bake_fingerprint(&world),
            before,
            "the lighting changed"
        );
    }
}
