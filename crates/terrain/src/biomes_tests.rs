use super::*;
use crate::Placement;
use crate::materials::{MaterialRules, biome_weights, sample_weights};
use axcenity_core::{TerrainData, Vector3};

/// A 256-stud terrain rising from 0 studs (west) to 100 (east), with
/// layers Grass, Forest Floor, Snow and Mud (Mud covers below 10 studs by
/// its own rule), a Lowland biome below 40 studs on Forest Floor and an
/// Alpine biome above 60 on Snow, both with straight edges.
fn island() -> (World, InstanceId, TerrainData, [InstanceId; 2]) {
    let mut world = World::new();
    let terrain = world.create(ClassName::Terrain).unwrap();
    world
        .set_property(
            terrain,
            "Size",
            Value::Vector3(Vector3::new(256.0, 100.0, 256.0)),
        )
        .unwrap();
    for name in ["Grass", "Forest Floor", "Snow", "Mud"] {
        let layer = world.create(ClassName::TerrainLayer).unwrap();
        world.set_parent(layer, Some(terrain)).unwrap();
        world.set_name(layer, name).unwrap();
    }
    let mud = crate::materials::layers(&world, terrain)[3];
    for (name, value) in [
        ("AutoEnabled", Value::Bool(true)),
        ("MaxHeight", Value::Number(10.0)),
        ("RuleNoise", Value::Number(0.0)),
    ] {
        world.set_property(mud, name, value).unwrap();
    }
    let biome = |world: &mut World, ground: &str, low: f64, high: f64| {
        let biome = world.create(ClassName::Biome).unwrap();
        world.set_parent(biome, Some(terrain)).unwrap();
        for (name, value) in [
            ("GroundLayer", Value::String(ground.to_owned())),
            ("MinHeight", Value::Number(low)),
            ("MaxHeight", Value::Number(high)),
            ("EdgeNoise", Value::Number(0.0)),
        ] {
            world.set_property(biome, name, value).unwrap();
        }
        biome
    };
    let lowland = biome(&mut world, "Forest Floor", 0.0, 40.0);
    let alpine = biome(&mut world, "Snow", 60.0, 10000.0);
    let mut data = TerrainData::flat(65, 65, 0.0).unwrap();
    let all = data.bounds();
    for (index, height) in data.heights_mut_in(all).iter_mut().enumerate() {
        *height = (index % 65) as f32 / 64.0;
    }
    (world, terrain, data, [lowland, alpine])
}

fn weights_at(
    world: &World,
    terrain: InstanceId,
    data: &TerrainData,
    x: usize,
) -> [f32; MAX_BIOMES] {
    let placement = Placement::of(world.get(terrain).unwrap()).unwrap();
    let rules = MaterialRules::of(world, terrain);
    biome_weights(data, &placement, &rules, x, 32)
}

#[test]
fn biomes_cover_where_their_conditions_hold_and_add_up() {
    let (world, terrain, data, _) = island();
    let low = weights_at(&world, terrain, &data, 0);
    assert!((low[0] - 1.0).abs() < 1e-4 && low[1] == 0.0, "{low:?}");
    let middle = weights_at(&world, terrain, &data, 32);
    assert!(
        middle[0] < 0.01 && middle[1] < 0.01,
        "no biome at 50 studs: {middle:?}"
    );
    let high = weights_at(&world, terrain, &data, 64);
    assert!(high[0] == 0.0 && (high[1] - 1.0).abs() < 1e-4, "{high:?}");
    for x in 0..65 {
        let total: f32 = weights_at(&world, terrain, &data, x).iter().sum();
        assert!(total <= 1.0 + 1e-4, "{x}: {total}");
    }
}

#[test]
fn biome_edges_are_soft() {
    let (world, terrain, data, _) = island();
    let between = (0..65)
        .map(|x| weights_at(&world, terrain, &data, x)[0])
        .filter(|&w| w > 0.05 && w < 0.95)
        .count();
    assert!(between >= 2, "the Lowland fades out over its HeightFade");
}

#[test]
fn painting_a_biomes_ground_paints_the_biome_and_conditions_never_lose_it() {
    let (mut world, terrain, mut data, [_, alpine]) = island();
    // Forest Floor painted high up, inside the Alpine biome.
    let at = data.index(64, 32);
    data.paint_mut_in(data.bounds())[at] = [0, 255, 0, 0, 0, 0, 0, 0];
    let painted = weights_at(&world, terrain, &data, 64);
    assert!(
        (painted[0] - 1.0).abs() < 1e-4 && painted[1] == 0.0,
        "{painted:?}"
    );
    // Moving the Alpine's line keeps the paint.
    world
        .set_property(alpine, "MinHeight", Value::Number(20.0))
        .unwrap();
    let after = weights_at(&world, terrain, &data, 64);
    assert!((after[0] - 1.0).abs() < 1e-4, "{after:?}");
    // Half-painted, the conditions fill the other half.
    data.paint_mut_in(data.bounds())[at] = [0, 128, 0, 0, 0, 0, 0, 0];
    let half = weights_at(&world, terrain, &data, 64);
    assert!(
        (half[0] - 0.5).abs() < 0.01 && (half[1] - 0.5).abs() < 0.01,
        "{half:?}"
    );
}

#[test]
fn a_biomes_ground_lies_under_the_layers_own_rules() {
    let (world, terrain, data, _) = island();
    let placement = Placement::of(world.get(terrain).unwrap()).unwrap();
    let rules = MaterialRules::of(&world, terrain);
    let top = |x| {
        let weights = sample_weights(&data, &placement, &rules, x, 32);
        (0..4)
            .max_by(|&a, &b| weights[a].total_cmp(&weights[b]))
            .unwrap()
    };
    assert_eq!(top(0), 3, "Mud's own rule covers the lowest ground");
    assert_eq!(top(16), 1, "Forest Floor in the Lowland");
    assert_eq!(top(32), 0, "Grass where no biome is");
    assert_eq!(top(64), 2, "Snow in the Alpine");
}

#[test]
fn a_biome_without_auto_is_only_painted_and_one_switched_off_is_nowhere() {
    let (mut world, terrain, mut data, [lowland, _]) = island();
    let key = MaterialRules::of(&world, terrain).key();
    world
        .set_property(lowland, "AutoEnabled", Value::Bool(false))
        .unwrap();
    assert_ne!(MaterialRules::of(&world, terrain).key(), key);
    assert_eq!(weights_at(&world, terrain, &data, 0)[0], 0.0);
    let at = data.index(0, 32);
    data.paint_mut_in(data.bounds())[at] = [0, 255, 0, 0, 0, 0, 0, 0];
    assert!((weights_at(&world, terrain, &data, 0)[0] - 1.0).abs() < 1e-4);
    world
        .set_property(lowland, "Enabled", Value::Bool(false))
        .unwrap();
    assert_eq!(weights_at(&world, terrain, &data, 0)[0], 0.0);
    // The Alpine keeps its number.
    assert!((weights_at(&world, terrain, &data, 64)[1] - 1.0).abs() < 1e-4);
}

#[test]
fn moisture_is_the_same_every_time_and_spreads_from_dry_to_wet() {
    let samples: Vec<f32> = (0..400)
        .map(|i| {
            moisture(
                Vec2::new((i % 20) as f32 * 97.0, (i / 20) as f32 * 89.0),
                400.0,
            )
        })
        .collect();
    assert_eq!(moisture(Vec2::new(97.0, 89.0), 400.0), samples[21]);
    let low = samples.iter().copied().fold(1.0, f32::min);
    let high = samples.iter().copied().fold(0.0, f32::max);
    assert!(low >= 0.0 && high <= 1.0);
    assert!(low < 0.3 && high > 0.7, "{low}..{high}");
}
