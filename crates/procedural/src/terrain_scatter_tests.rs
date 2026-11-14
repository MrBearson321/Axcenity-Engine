//! Procedural worlds PW1: what a Terrain's ScatterRules place.

use std::sync::Arc;

use axcenity_core::{
    ClassName, FoliageInstance, FoliagePlant, FoliageSource, InstanceId, TerrainData, Value,
    Vector3, World,
};
use axcenity_terrain::Placement;

use crate::terrain_scatter::{TerrainScatter, rules_of, source};

fn set(world: &mut World, id: InstanceId, name: &str, value: Value) {
    world.set_property(id, name, value).unwrap();
}

/// A 256-stud terrain (129 samples, 2 studs apart) with these heights, 0
/// to 1 of its 100-stud height range, by sample.
fn terrain(world: &mut World, height: impl Fn(usize, usize) -> f32) -> InstanceId {
    let workspace = world.workspace();
    let terrain = world.create(ClassName::Terrain).unwrap();
    world.set_parent(terrain, Some(workspace)).unwrap();
    set(
        world,
        terrain,
        "Position",
        Value::Vector3(Vector3::new(0.0, 0.0, 0.0)),
    );
    set(
        world,
        terrain,
        "Size",
        Value::Vector3(Vector3::new(256.0, 100.0, 256.0)),
    );
    let mut data = TerrainData::flat(129, 129, 0.0).unwrap();
    let all = data.bounds();
    let heights = data.heights_mut_in(all);
    for z in 0..129 {
        for x in 0..129 {
            heights[z * 129 + x] = height(x, z);
        }
    }
    world.set_terrain(terrain, Arc::new(data));
    terrain
}

fn rule(world: &mut World, terrain: InstanceId, models: &str, spacing: f64) -> InstanceId {
    let rule = world.create(ClassName::ScatterRule).unwrap();
    world.set_parent(rule, Some(terrain)).unwrap();
    set(world, rule, "Models", Value::String(models.into()));
    set(world, rule, "Spacing", Value::Number(spacing));
    // As many as the spacing allows.
    set(world, rule, "Density", Value::Number(100.0));
    rule
}

fn generated(world: &World, terrain: InstanceId) -> Vec<FoliageInstance> {
    world
        .terrain(terrain)
        .unwrap()
        .generated_foliage()
        .cloned()
        .collect()
}

fn placement(world: &World, terrain: InstanceId) -> Placement {
    Placement::of(world.get(terrain).unwrap()).unwrap()
}

/// World X and Z of a generated instance.
fn xz(world: &World, terrain: InstanceId, instance: &FoliageInstance) -> glam::Vec2 {
    let data = world.terrain(terrain).unwrap();
    placement(world, terrain).to_world_xz(data, glam::Vec2::new(instance.x, instance.z))
}

#[test]
fn the_same_rules_and_ground_give_the_same_layout_and_another_seed_another() {
    let mut world = World::new();
    let land = terrain(&mut world, |x, z| ((x + z) as f32 * 0.002).min(1.0));
    let trees = rule(&mut world, land, "plant:Tree; plant:Bush", 6.0);
    set(&mut world, trees, "Density", Value::Number(1.5));
    let mut scatter = TerrainScatter::default();
    assert!(scatter.update(&mut world) > 0);
    let first = generated(&world, land);
    assert!(first.len() > 50, "{} placed", first.len());
    assert!(
        first
            .iter()
            .any(|i| i.source == FoliageSource::Plant(FoliagePlant::Tree))
    );
    assert!(
        first
            .iter()
            .any(|i| i.source == FoliageSource::Plant(FoliagePlant::Bush))
    );

    // Another device (a fresh world and scatter) makes the same.
    let mut other = World::new();
    let other_land = terrain(&mut other, |x, z| ((x + z) as f32 * 0.002).min(1.0));
    let other_trees = rule(&mut other, other_land, "plant:Tree; plant:Bush", 6.0);
    set(&mut other, other_trees, "Density", Value::Number(1.5));
    TerrainScatter::default().update(&mut other);
    assert_eq!(generated(&other, other_land), first);

    // Nothing changed: nothing is made again.
    assert_eq!(scatter.update(&mut world), 0);

    set(&mut world, trees, "Seed", Value::Number(2.0));
    scatter.update(&mut world);
    assert_ne!(generated(&world, land), first);
}

#[test]
fn spots_keep_about_their_spacing_and_density_thins_them() {
    let mut world = World::new();
    let land = terrain(&mut world, |_, _| 0.2);
    let grass = rule(&mut world, land, "plant:Grass", 4.0);
    let mut scatter = TerrainScatter::default();
    scatter.update(&mut world);
    let full = generated(&world, land);
    // One per 4-stud cell over 256 × 256 studs.
    assert!((3900..=4100).contains(&full.len()), "{}", full.len());
    let spots: Vec<glam::Vec2> = full.iter().map(|i| xz(&world, land, i)).collect();
    let closest = spots
        .iter()
        .enumerate()
        .flat_map(|(a, p)| spots[a + 1..].iter().map(move |q| p.distance(*q)))
        .fold(f32::MAX, f32::min);
    assert!(
        closest >= 4.0 * 0.2 - 1e-3,
        "two spots {closest} studs apart"
    );

    // 1 per 100 square studs is 0.16 per cell: about 655 over the terrain.
    set(&mut world, grass, "Density", Value::Number(1.0));
    scatter.update(&mut world);
    let thin = generated(&world, land).len();
    assert!((520..=800).contains(&thin), "{thin}");
}

#[test]
fn height_slope_and_water_keep_a_rule_where_it_belongs() {
    let mut world = World::new();
    // Flat at 10 studs on the left half; a ramp (about 31 degrees) on the
    // right, rising from 10 to 87 studs.
    let land = terrain(&mut world, |x, _| {
        if x < 64 {
            0.1
        } else {
            0.1 + (x - 64) as f32 * 0.012
        }
    });
    let flat = rule(&mut world, land, "plant:Flower", 4.0);
    set(&mut world, flat, "MaxSlope", Value::Number(20.0));
    let mut scatter = TerrainScatter::default();
    scatter.update(&mut world);
    let placed = generated(&world, land);
    assert!(!placed.is_empty());
    // Only on the flat half (and the first sample or two of the ramp's
    // foot). The very edge reads half the slope (its outer neighbour is
    // itself), so it's left out of the check.
    assert!(
        placed.iter().all(|i| i.x <= 65.0 || i.x >= 127.5),
        "one at x {}",
        placed.iter().map(|i| i.x).fold(0.0, f32::max)
    );

    // Only above 50 studs: on the ramp, never on the flat.
    set(&mut world, flat, "MaxSlope", Value::Number(90.0));
    set(&mut world, flat, "MinHeight", Value::Number(50.0));
    set(&mut world, flat, "HeightFade", Value::Number(0.0));
    scatter.update(&mut world);
    let data = world.terrain(land).unwrap().clone();
    let high = generated(&world, land);
    assert!(!high.is_empty());
    assert!(
        high.iter()
            .all(|i| data.height_at(i.x, i.z) * 100.0 >= 50.0)
    );

    // Water at 20 studs: nothing on the flat (10 studs, under water) with
    // the height window open again, unless Underwater is on.
    set(&mut world, flat, "MinHeight", Value::Number(0.0));
    set(&mut world, land, "Water", Value::Bool(true));
    set(&mut world, land, "WaterLevel", Value::Number(20.0));
    scatter.update(&mut world);
    let dry = generated(&world, land);
    assert!(!dry.is_empty());
    assert!(dry.iter().all(|i| data.height_at(i.x, i.z) * 100.0 >= 20.0));
    set(&mut world, flat, "Underwater", Value::Bool(true));
    scatter.update(&mut world);
    assert!(generated(&world, land).iter().any(|i| i.x < 60.0));
}

#[test]
fn a_later_rule_keeps_its_avoid_radius_from_the_rules_before_it() {
    let mut world = World::new();
    let land = terrain(&mut world, |_, _| 0.2);
    let trees = rule(&mut world, land, "plant:Tree", 10.0);
    set(&mut world, trees, "Density", Value::Number(0.6));
    let bushes = rule(&mut world, land, "plant:Bush", 3.0);
    set(&mut world, bushes, "AvoidRadius", Value::Number(5.0));
    let mut scatter = TerrainScatter::default();
    scatter.update(&mut world);
    let placed = generated(&world, land);
    let tree = FoliageSource::Plant(FoliagePlant::Tree);
    let tree_spots: Vec<_> = placed
        .iter()
        .filter(|i| i.source == tree)
        .map(|i| xz(&world, land, i))
        .collect();
    let bush_spots: Vec<_> = placed
        .iter()
        .filter(|i| i.source != tree)
        .map(|i| xz(&world, land, i))
        .collect();
    assert!(tree_spots.len() > 100 && bush_spots.len() > 1000);
    for bush in &bush_spots {
        for tree in &tree_spots {
            assert!(
                bush.distance(*tree) >= 5.0,
                "a bush {} studs from a tree",
                bush.distance(*tree)
            );
        }
    }
}

#[test]
fn chunks_agree_at_their_edges_and_sculpting_remakes_only_nearby_ones() {
    let mut world = World::new();
    let land = terrain(&mut world, |x, z| ((x * 7 + z * 3) % 50) as f32 * 0.002);
    rule(&mut world, land, "plant:Tree", 9.0);
    let bushes = rule(&mut world, land, "plant:Bush", 3.0);
    set(&mut world, bushes, "AvoidRadius", Value::Number(6.0));
    let mut scatter = TerrainScatter::default();
    // 129 samples: 4 × 4 chunks.
    assert_eq!(scatter.update(&mut world), 16);
    let before = generated(&world, land);

    // Worked out over the whole terrain as one, the avoid rule holds across
    // chunk edges as well as inside them.
    let tree = FoliageSource::Plant(FoliagePlant::Tree);
    let spots: Vec<_> = before
        .iter()
        .map(|i| (i.source == tree, xz(&world, land, i)))
        .collect();
    for (is_tree, at) in &spots {
        if !is_tree {
            assert!(
                spots
                    .iter()
                    .filter(|(t, _)| *t)
                    .all(|(_, t)| t.distance(*at) >= 6.0)
            );
        }
    }

    // Raise a bump in one corner chunk.
    let data = world.terrain_mut(land).unwrap();
    let rect = axcenity_core::SampleRect::new(2, 2, 6, 6);
    let heights = data.heights_mut_in(rect);
    for z in 2..6 {
        for x in 2..6 {
            heights[z * 129 + x] = 0.5;
        }
    }
    let made = scatter.update(&mut world);
    assert!((1..=4).contains(&made), "{made} chunks made again");
    let after = generated(&world, land);
    // Far from the bump, every chunk is as it was.
    let far = |list: &[FoliageInstance]| -> Vec<FoliageInstance> {
        list.iter()
            .filter(|i| i.x >= 64.0 || i.z >= 64.0)
            .cloned()
            .collect()
    };
    assert_eq!(far(&after), far(&before));
}

#[test]
fn a_rule_on_a_layer_places_only_where_that_layer_covers() {
    let mut world = World::new();
    let land = terrain(&mut world, |_, _| 0.2);
    for name in ["Grass", "Sand"] {
        let layer = world.create(ClassName::TerrainLayer).unwrap();
        world.set_parent(layer, Some(land)).unwrap();
        world.set_name(layer, name).unwrap();
    }
    // Paint the sand fully over the left half.
    let data = world.terrain_mut(land).unwrap();
    let rect = axcenity_core::SampleRect::new(0, 0, 64, 129);
    let paint = data.paint_mut_in(rect);
    for z in 0..129 {
        for x in 0..64 {
            paint[z * 129 + x] = [0, 255, 0, 0, 0, 0, 0, 0];
        }
    }
    let palms = rule(&mut world, land, "plant:Tree", 6.0);
    set(&mut world, palms, "Layer", Value::String("Sand".into()));
    let mut scatter = TerrainScatter::default();
    scatter.update(&mut world);
    let placed = generated(&world, land);
    assert!(!placed.is_empty());
    assert!(placed.iter().all(|i| i.x < 64.0));

    // A layer the terrain doesn't have places nothing.
    set(&mut world, palms, "Layer", Value::String("Snow".into()));
    scatter.update(&mut world);
    assert!(generated(&world, land).is_empty());
}

#[test]
fn generated_foliage_is_never_saved_and_goes_with_its_rules() {
    let mut world = World::new();
    let land = terrain(&mut world, |_, _| 0.2);
    let trees = rule(&mut world, land, "plant:Tree; assets/models/Pine.glb", 8.0);
    set(&mut world, trees, "CanCollide", Value::Bool(true));
    let mut scatter = TerrainScatter::default();
    let revision = world.terrain(land).unwrap().revision();
    scatter.update(&mut world);
    let data = world.terrain(land).unwrap();
    assert!(data.generated_count() > 100);
    assert!(data.generated_foliage().all(|i| i.collide));
    assert!(
        data.generated_foliage()
            .any(|i| matches!(&i.source, FoliageSource::Model(p) if p == "assets/models/Pine.glb"))
    );
    // Not an edit, not painted, and not in the files.
    assert_eq!(data.revision(), revision);
    assert!(data.foliage().is_empty());
    assert_eq!(data.all_foliage().count(), data.generated_count());
    let files = axcenity_terrain::files::encode(data);
    assert!(
        files
            .iter()
            .all(|(name, _)| *name != axcenity_terrain::files::FOLIAGE_FILE)
    );

    // Switched off, its plants go.
    set(&mut world, trees, "Enabled", Value::Bool(false));
    scatter.update(&mut world);
    assert_eq!(world.terrain(land).unwrap().generated_count(), 0);
}

#[test]
fn models_name_built_in_plants_props_and_files() {
    use axcenity_core::FoliageProp;
    assert_eq!(
        source(" plant:Grass "),
        Some(FoliageSource::Plant(FoliagePlant::Grass))
    );
    assert_eq!(
        source("prop:Crate"),
        Some(FoliageSource::Prop(FoliageProp::Crate))
    );
    assert_eq!(source("plant:Cactus"), None);
    assert_eq!(source(""), None);
    assert_eq!(
        source("assets/models/Rock.glb"),
        Some(FoliageSource::Model("assets/models/Rock.glb".into()))
    );
    let mut world = World::new();
    let land = terrain(&mut world, |_, _| 0.2);
    rule(&mut world, land, "plant:Cactus;", 8.0);
    assert!(rules_of(&world, land).is_empty(), "nothing it can place");
}

/// How long a forest (trees, undergrowth, grass) takes to place over a
/// 2,048-stud terrain of 1,025 samples, then to remake after one stroke.
/// `cargo test -p axcenity-procedural --release bench_forest -- --ignored --nocapture`
#[test]
#[ignore = "benchmark"]
fn bench_forest_on_a_2048_stud_terrain() {
    let mut world = World::new();
    let workspace = world.workspace();
    let land = world.create(ClassName::Terrain).unwrap();
    world.set_parent(land, Some(workspace)).unwrap();
    set(
        &mut world,
        land,
        "Size",
        Value::Vector3(Vector3::new(2048.0, 200.0, 2048.0)),
    );
    let mut data = TerrainData::flat(1025, 1025, 0.0).unwrap();
    let all = data.bounds();
    let heights = data.heights_mut_in(all);
    for z in 0..1025 {
        for x in 0..1025 {
            heights[z * 1025 + x] =
                0.2 + 0.1 * ((x as f32 * 0.02).sin() * (z as f32 * 0.015).cos());
        }
    }
    world.set_terrain(land, Arc::new(data));
    for (models, spacing, density, avoid) in [
        ("plant:Tree", 9.0, 0.8, 0.0),
        ("plant:Bush", 4.0, 1.5, 4.0),
        ("plant:Grass", 2.0, 10.0, 1.5),
    ] {
        let r = rule(&mut world, land, models, spacing);
        set(&mut world, r, "Density", Value::Number(density));
        set(&mut world, r, "AvoidRadius", Value::Number(avoid));
        set(&mut world, r, "Clumping", Value::Number(0.4));
    }
    let mut scatter = TerrainScatter::default();
    let start = std::time::Instant::now();
    let chunks = scatter.update(&mut world);
    let full = start.elapsed();
    let count = world.terrain(land).unwrap().generated_count();
    let start = std::time::Instant::now();
    assert_eq!(scatter.update(&mut world), 0);
    let idle = start.elapsed();
    let data = world.terrain_mut(land).unwrap();
    let rect = axcenity_core::SampleRect::new(500, 500, 520, 520);
    for h in data
        .heights_mut_in(rect)
        .iter_mut()
        .skip(500 * 1025)
        .take(10)
    {
        *h += 0.01;
    }
    let start = std::time::Instant::now();
    let remade = scatter.update(&mut world);
    let stroke = start.elapsed();
    println!(
        "{count} placed in {chunks} chunks: {full:?} at first, {idle:?} unchanged, {stroke:?} for {remade} chunks after a stroke"
    );
}
