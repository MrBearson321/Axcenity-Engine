//! Graphics volumes (graphics Phase 33): a `GraphicsVolume` (an invisible
//! part) changes the look while the camera is in its box, fading in over
//! its `BlendDistance` outside it. The effects inside it (a
//! `ColorCorrectionEffect`, `ExposureEffect`, `FogEffect`, `Atmosphere` or
//! `BloomEffect`) each replace that part of the world's look; what it
//! doesn't hold stays as it is.
//!
//! Volumes are blended in from the lowest `Priority` to the highest (equal
//! ones in Explorer order), each over the look so far, so where they
//! overlap the higher one wins. Numbers and colors blend smoothly. What
//! can't (a lookup table, posterizing, a palette, an Atmosphere's sky in
//! place of the plain one) switches halfway through the blend.
//!
//! All of it happens here: the draw list carries one look, so `render`
//! doesn't know about volumes.

use axcenity_assets::Assets;
use axcenity_core::{ClassName, Instance, Value, World};
use axcenity_render::glam::Vec3;
use axcenity_render::{Atmosphere, ColorGrading, Fog, PostProcessing, Sky, Sun};

use crate::draw_list::part_placement;

/// What graphics volumes change: the look the Lighting service and its
/// effects give, which goes into the draw list.
#[derive(Clone, Debug)]
pub(crate) struct Look {
    pub sun: Sun,
    pub sky: Sky,
    pub fog: Fog,
    pub post: PostProcessing,
    pub grading: Option<ColorGrading>,
}

/// `look` with every enabled GraphicsVolume in the Workspace around
/// `camera` blended in.
pub(crate) fn blend(
    world: &World,
    assets: &Assets,
    lighting: &Instance,
    camera: Vec3,
    mut look: Look,
) -> Look {
    let mut volumes: Vec<(f64, f32, &Instance)> = world
        .all_below(world.workspace(), |class| {
            class == ClassName::GraphicsVolume
        })
        .into_iter()
        .filter(|&id| !axcenity_core::csg::out_of_world(world, id))
        .filter_map(|id| world.get(id))
        .filter(|volume| volume.property("Enabled") != Some(&Value::Bool(false)))
        .filter_map(|volume| {
            let weight = weight(volume, camera)?;
            let priority = number(volume, "Priority").unwrap_or(0.0);
            (weight > 0.0).then_some((priority, weight, volume))
        })
        .collect();
    // A stable sort, so equal priorities keep Explorer order.
    volumes.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (_, weight, volume) in volumes {
        let target = applied(world, assets, lighting, volume, &look);
        look = mix(look, target, weight);
    }
    look
}

/// How much of `volume`'s look the camera at `camera` gets: 1 inside its
/// box, falling evenly to 0 at `BlendDistance` studs outside it.
fn weight(volume: &Instance, camera: Vec3) -> Option<f32> {
    let (_, transform) = part_placement(volume)?;
    let (size, rotation, center) = transform.to_scale_rotation_translation();
    let local = rotation.inverse() * (camera - center);
    let outside = (local.abs() - size.abs() * 0.5).max(Vec3::ZERO).length();
    if outside <= 0.0 {
        return Some(1.0);
    }
    let blend = number(volume, "BlendDistance").unwrap_or(0.0).max(0.0) as f32;
    if blend <= 0.0 {
        return Some(0.0);
    }
    Some((1.0 - outside / blend).max(0.0))
}

/// `look` as `volume`'s effects make it, whole.
fn applied(
    world: &World,
    assets: &Assets,
    lighting: &Instance,
    volume: &Instance,
    look: &Look,
) -> Look {
    let mut look = look.clone();
    let children: Vec<&Instance> = volume
        .children()
        .iter()
        .filter_map(|&id| world.get(id))
        .filter(|child| child.property("Enabled") != Some(&Value::Bool(false)))
        .collect();
    let first = |class| {
        children
            .iter()
            .copied()
            .find(|child| child.class() == class)
    };
    if let Some(atmosphere) = first(ClassName::Atmosphere) {
        look.sun =
            crate::time_of_day::sun_and_sky_through(lighting, Some(atmosphere), &mut look.sky);
        look.fog = crate::fog::atmosphere_fog(atmosphere);
    }
    if let Some(fog) = first(ClassName::FogEffect) {
        let start = number(fog, "Start").unwrap_or(0.0).max(0.0);
        let end = number(fog, "End").unwrap_or(200.0).max(start);
        let color = fog.property("Color").and_then(Value::as_color3);
        look.fog = Fog {
            color: color.map(|color| [color.r, color.g, color.b]),
            start: start as f32,
            end: end as f32,
            ..Fog::default()
        };
    }
    if let Some(exposure) = first(ClassName::ExposureEffect) {
        look.post.exposure = number(exposure, "Exposure").unwrap_or(1.0) as f32;
    }
    if let Some(bloom) = first(ClassName::BloomEffect) {
        look.post.bloom_enabled = true;
        look.post.bloom_intensity = number(bloom, "Intensity").unwrap_or(0.0).max(0.0) as f32;
        look.post.bloom_threshold = number(bloom, "Threshold").unwrap_or(1.0) as f32;
    }
    if first(ClassName::ColorCorrectionEffect).is_some() {
        look.grading = crate::color_grading::grading_in(world, volume, assets);
    }
    look
}

/// `from` blended `weight` of the way (0 to 1) toward `to`.
fn mix(from: Look, to: Look, weight: f32) -> Look {
    if weight >= 1.0 {
        return to;
    }
    let w = weight;
    let atmosphere = match (from.sky.atmosphere, to.sky.atmosphere) {
        (Some(a), Some(b)) => Some(mix_air(a, b, w)),
        (a, b) => halfway(a, b, w),
    };
    // Bloom that's off blends as bloom of no intensity.
    let bloom = |post: &PostProcessing| {
        if post.bloom_enabled {
            post.bloom_intensity
        } else {
            0.0
        }
    };
    let post = PostProcessing {
        exposure: lerp(from.post.exposure, to.post.exposure, w),
        bloom_enabled: from.post.bloom_enabled || to.post.bloom_enabled,
        bloom_intensity: lerp(bloom(&from.post), bloom(&to.post), w),
        bloom_threshold: lerp(from.post.bloom_threshold, to.post.bloom_threshold, w),
        ..from.post
    };
    Look {
        sun: Sun {
            color: lerp3(from.sun.color, to.sun.color, w),
            intensity: lerp(from.sun.intensity, to.sun.intensity, w),
            ..from.sun
        },
        sky: Sky {
            atmosphere,
            ..from.sky
        },
        fog: mix_fog(&from.fog, &to.fog, w),
        post,
        grading: mix_grading(from.grading, to.grading, w),
    }
}

fn mix_air(a: Atmosphere, b: Atmosphere, w: f32) -> Atmosphere {
    Atmosphere {
        density: lerp(a.density, b.density, w),
        altitude: lerp(a.altitude, b.altitude, w),
        tint: lerp3(a.tint, b.tint, w),
        decay: lerp3(a.decay, b.decay, w),
        glare: lerp(a.glare, b.glare, w),
        haze: lerp(a.haze, b.haze, w),
        ..a
    }
}

/// Fog blended `w` of the way from `a` to `b`. Distance fog that only one
/// side has comes in from far away (it starts and ends further off the
/// less of it there is), and height fog thickens from none.
fn mix_fog(a: &Fog, b: &Fog, w: f32) -> Fog {
    let mut fog = Fog::default();
    match (a.has_distance_fog(), b.has_distance_fog()) {
        (true, true) => {
            fog.start = lerp(a.start, b.start, w);
            fog.end = lerp(a.end, b.end, w);
        }
        (false, true) => (fog.start, fog.end) = (b.start / w, b.end / w),
        (true, false) => (fog.start, fog.end) = (a.start / (1.0 - w), a.end / (1.0 - w)),
        (false, false) => {}
    }
    let (in_a, in_b) = (a.has_height_fog(), b.has_height_fog());
    let density = |fog: &Fog, has: bool| if has { fog.density } else { 0.0 };
    fog.density = lerp(density(a, in_a), density(b, in_b), w);
    let shape = match (in_a, in_b) {
        (true, true) => Some(Fog {
            height: lerp(a.height, b.height, w),
            falloff: lerp(a.falloff, b.falloff, w),
            sun_scattering: lerp(a.sun_scattering, b.sun_scattering, w),
            forward_scattering: lerp(a.forward_scattering, b.forward_scattering, w),
            ..*a
        }),
        (true, false) => Some(*a),
        (false, true) => Some(*b),
        (false, false) => None,
    };
    if let Some(shape) = shape {
        fog.height = shape.height;
        fog.falloff = shape.falloff;
        fog.sun_scattering = shape.sun_scattering;
        fog.forward_scattering = shape.forward_scattering;
    }
    // A side with no fog has no color to blend from.
    fog.color = match (a.is_drawn(), b.is_drawn(), a.color, b.color) {
        (true, false, ..) => a.color,
        (false, true, ..) => b.color,
        (_, _, Some(a), Some(b)) => Some(lerp3(a, b, w)),
        (.., a, b) => halfway(a, b, w),
    };
    fog
}

fn mix_grading(a: Option<ColorGrading>, b: Option<ColorGrading>, w: f32) -> Option<ColorGrading> {
    if a.is_none() && b.is_none() {
        return None;
    }
    let (a, b) = (a.unwrap_or_default(), b.unwrap_or_default());
    let switched = halfway(a.clone(), b.clone(), w);
    Some(ColorGrading {
        brightness: lerp(a.brightness, b.brightness, w),
        contrast: lerp(a.contrast, b.contrast, w),
        saturation: lerp(a.saturation, b.saturation, w),
        tint: lerp3(a.tint, b.tint, w),
        ..switched
    })
}

/// `b` from halfway through the blend on, `a` before.
fn halfway<T>(a: T, b: T, w: f32) -> T {
    if w >= 0.5 { b } else { a }
}

fn lerp(a: f32, b: f32, w: f32) -> f32 {
    a + (b - a) * w
}

fn lerp3(a: [f32; 3], b: [f32; 3], w: f32) -> [f32; 3] {
    [0, 1, 2].map(|i| lerp(a[i], b[i], w))
}

fn number(instance: &Instance, name: &str) -> Option<f64> {
    instance
        .property(name)
        .and_then(Value::as_number)
        .filter(|value| value.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_core::{Color3, InstanceId, Vector3};
    use axcenity_render::{Camera, DrawList};

    fn set(world: &mut World, id: InstanceId, name: &str, value: Value) {
        world.set_property(id, name, value).unwrap();
    }

    fn child(world: &mut World, parent: InstanceId, class: ClassName) -> InstanceId {
        let id = world.create(class).unwrap();
        world.set_parent(id, Some(parent)).unwrap();
        id
    }

    /// A `size` GraphicsVolume at `at`, blending over `blend` studs.
    fn volume(world: &mut World, at: [f32; 3], size: [f32; 3], blend: f64) -> InstanceId {
        let workspace = world.workspace();
        let volume = child(world, workspace, ClassName::GraphicsVolume);
        let vector = |[x, y, z]: [f32; 3]| Value::Vector3(Vector3::new(x, y, z));
        set(world, volume, "Position", vector(at));
        set(world, volume, "Size", vector(size));
        set(world, volume, "BlendDistance", Value::Number(blend));
        volume
    }

    fn exposure_effect(world: &mut World, volume: InstanceId, exposure: f64) -> InstanceId {
        let effect = child(world, volume, ClassName::ExposureEffect);
        set(world, effect, "Exposure", Value::Number(exposure));
        effect
    }

    fn seen_from(world: &World, x: f32) -> DrawList {
        let camera = Camera {
            position: Vec3::new(x, 2.0, 0.0),
            look_at: Vec3::new(x, 2.0, -10.0),
            fov_y_degrees: 70.0,
        };
        crate::build_draw_list(world, &Assets::default(), camera)
    }

    fn exposure_at(world: &World, x: f32) -> f32 {
        seen_from(world, x).post.exposure
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn walking_into_a_volume_blends_its_look_in_over_its_blend_distance_and_out_again() {
        let mut world = World::new();
        // A box 10 studs wide around the origin, blending over 4 more.
        let volume = volume(&mut world, [0.0, 2.0, 0.0], [10.0, 10.0, 10.0], 4.0);
        exposure_effect(&mut world, volume, 3.0);
        let walk_in = [20.0, 9.0, 8.0, 7.0, 6.0, 5.0, 0.0];
        let expected = [1.0, 1.0, 1.5, 2.0, 2.5, 3.0, 3.0];
        for (x, expected) in walk_in.into_iter().zip(expected) {
            let seen = exposure_at(&world, x);
            assert!(close(seen, expected), "at x = {x}: {seen}, not {expected}");
        }
        // And out the other way.
        assert!(close(exposure_at(&world, -7.0), 2.0));
        assert!(close(exposure_at(&world, -9.0), 1.0));

        // Off, or with no blend distance, it doesn't reach outside.
        set(&mut world, volume, "Enabled", Value::Bool(false));
        assert!(close(exposure_at(&world, 0.0), 1.0));
        set(&mut world, volume, "Enabled", Value::Bool(true));
        set(&mut world, volume, "BlendDistance", Value::Number(0.0));
        assert!(close(exposure_at(&world, 5.5), 1.0));
        assert!(close(exposure_at(&world, 4.5), 3.0));
    }

    #[test]
    fn overlapping_volumes_follow_priority() {
        let mut world = World::new();
        let big = volume(&mut world, [0.0, 2.0, 0.0], [40.0, 10.0, 40.0], 0.0);
        exposure_effect(&mut world, big, 5.0);
        let small = volume(&mut world, [0.0, 2.0, 0.0], [10.0, 10.0, 10.0], 4.0);
        exposure_effect(&mut world, small, 2.0);
        set(&mut world, small, "Priority", Value::Number(1.0));
        assert!(close(exposure_at(&world, 0.0), 2.0), "the higher one wins");
        assert!(
            close(exposure_at(&world, 15.0), 5.0),
            "outside the small one"
        );
        assert!(
            close(exposure_at(&world, 7.0), 3.5),
            "halfway into the small one, over the big one"
        );

        set(&mut world, small, "Priority", Value::Number(0.0));
        set(&mut world, big, "Priority", Value::Number(2.0));
        assert!(close(exposure_at(&world, 0.0), 5.0), "now the big one wins");

        // Equal priorities: the later in Explorer order wins.
        set(&mut world, big, "Priority", Value::Number(0.0));
        assert!(close(exposure_at(&world, 0.0), 2.0));

        // A volume only replaces what it holds: one with just bloom leaves
        // the exposure under it.
        let bloom_only = volume(&mut world, [0.0, 2.0, 0.0], [10.0, 10.0, 10.0], 0.0);
        set(&mut world, bloom_only, "Priority", Value::Number(9.0));
        let bloom = child(&mut world, bloom_only, ClassName::BloomEffect);
        set(&mut world, bloom, "Intensity", Value::Number(1.5));
        let list = seen_from(&world, 0.0);
        assert!(close(list.post.exposure, 2.0));
        assert!(close(list.post.bloom_intensity, 1.5));
    }

    #[test]
    fn a_volume_is_its_box_turned_with_it() {
        let mut world = World::new();
        // Thin along X until it's turned a quarter round Y.
        let volume = volume(&mut world, [0.0, 2.0, 0.0], [2.0, 10.0, 20.0], 0.0);
        exposure_effect(&mut world, volume, 3.0);
        assert!(close(exposure_at(&world, 8.0), 1.0));
        set(
            &mut world,
            volume,
            "Orientation",
            Value::Vector3(Vector3::new(0.0, 90.0, 0.0)),
        );
        assert!(close(exposure_at(&world, 8.0), 3.0));
    }

    #[test]
    fn fog_bloom_and_color_correction_blend_too() {
        let mut world = World::new();
        let lighting = world.lighting();
        set(&mut world, lighting, "BloomEnabled", Value::Bool(false));
        let volume = volume(&mut world, [0.0, 2.0, 0.0], [10.0, 10.0, 10.0], 4.0);
        let fog = child(&mut world, volume, ClassName::FogEffect);
        set(&mut world, fog, "Start", Value::Number(10.0));
        set(&mut world, fog, "End", Value::Number(50.0));
        set(
            &mut world,
            fog,
            "Color",
            Value::Color3(Color3::new(0.2, 0.3, 0.4)),
        );
        let bloom = child(&mut world, volume, ClassName::BloomEffect);
        set(&mut world, bloom, "Intensity", Value::Number(1.0));
        let grading = child(&mut world, volume, ClassName::ColorCorrectionEffect);
        set(&mut world, grading, "Brightness", Value::Number(0.2));
        set(&mut world, grading, "Posterize", Value::Number(4.0));

        let outside = seen_from(&world, 20.0);
        assert!(!outside.fog.is_drawn());
        assert!(!outside.post.bloom_enabled);
        assert!(outside.grading.is_none(), "nothing changes out here");

        let inside = seen_from(&world, 0.0);
        assert_eq!((inside.fog.start, inside.fog.end), (10.0, 50.0));
        assert_eq!(inside.fog.color, Some([0.2, 0.3, 0.4]));
        assert!(inside.post.bloom_enabled && close(inside.post.bloom_intensity, 1.0));
        let graded = inside.grading.unwrap();
        assert!(close(graded.brightness, 0.2));
        assert_eq!(graded.posterize, 4);

        // A quarter of the way in, the fog comes from four times as far.
        let edge = seen_from(&world, 8.0);
        assert_eq!((edge.fog.start, edge.fog.end), (40.0, 200.0));
        assert_eq!(edge.fog.color, Some([0.2, 0.3, 0.4]));
        assert!(close(edge.post.bloom_intensity, 0.25), "bloom from none");
        let graded = edge.grading.unwrap();
        assert!(close(graded.brightness, 0.05));
        assert_eq!(graded.posterize, 0, "levels switch halfway");
        assert_eq!(seen_from(&world, 7.0).grading.unwrap().posterize, 4);
    }

    #[test]
    fn a_volumes_atmosphere_changes_the_sky_the_sun_and_the_fog() {
        let mut world = World::new();
        let volume = volume(&mut world, [0.0, 2.0, 0.0], [10.0, 10.0, 10.0], 4.0);
        let atmosphere = child(&mut world, volume, ClassName::Atmosphere);
        set(&mut world, atmosphere, "Haze", Value::Number(4.0));
        let outside = seen_from(&world, 20.0);
        assert!(outside.sky.atmosphere.is_none() && !outside.fog.is_drawn());
        let inside = seen_from(&world, 0.0);
        let air = inside.sky.atmosphere.expect("the volume's air");
        assert_eq!(air.haze, 4.0);
        assert!(inside.fog.has_height_fog());
        assert_eq!(inside.fog.color, None, "fog in the sky's color");
        assert_ne!(
            inside.sun.color, outside.sun.color,
            "hazy air tints the sun"
        );
        // The plain sky turns into the air halfway; the fog thickens.
        assert!(seen_from(&world, 8.0).sky.atmosphere.is_none());
        let halfway = seen_from(&world, 7.0);
        assert!(halfway.sky.atmosphere.is_some());
        assert!(close(halfway.fog.density, inside.fog.density * 0.5));

        // Over the world's own Atmosphere, the air blends.
        let lighting = world.lighting();
        let own = child(&mut world, lighting, ClassName::Atmosphere);
        set(&mut world, own, "Haze", Value::Number(0.0));
        let air = seen_from(&world, 7.0).sky.atmosphere.unwrap();
        assert!(close(air.haze, 2.0));
    }

    #[test]
    fn a_world_without_volumes_looks_as_it_did() {
        let mut world = World::new();
        let lighting = world.lighting();
        set(&mut world, lighting, "Exposure", Value::Number(1.5));
        set(&mut world, lighting, "FogEnd", Value::Number(80.0));
        let list = seen_from(&world, 0.0);
        assert!(close(list.post.exposure, 1.5));
        assert_eq!(list.fog.end, 80.0);
        assert!(list.grading.is_none());
        assert!(list.items.is_empty(), "nothing to see");
        // A volume out of the Workspace does nothing, and one is never seen.
        let stored = volume(&mut world, [0.0, 2.0, 0.0], [10.0, 10.0, 10.0], 0.0);
        exposure_effect(&mut world, stored, 3.0);
        assert!(seen_from(&world, 0.0).items.is_empty(), "invisible");
        let storage = world.replicated_storage();
        world.set_parent(stored, Some(storage)).unwrap();
        assert!(close(exposure_at(&world, 0.0), 1.5));
    }
}
