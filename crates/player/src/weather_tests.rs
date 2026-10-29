//! Weather in a world: rain wets the ground over time in the frames a host
//! draws and it dries after, a script sets the weather, and it saves with
//! the world.

use axcenity_core::{ClassName, EnumItem, InstanceId, Value, Vector3};
use axcenity_render::TextureRenderer;
use axcenity_scene::Particles;

use super::tests::{run_until, start_example_in, test_gpu};
use super::*;

/// A wide grey floor under a Weather that doesn't rain yet, eases over
/// `transition` seconds and rains as hard as it can when it does.
fn floor_under_weather(transition: f64) -> (World, InstanceId) {
    let mut world = World::new();
    let floor = world.create(ClassName::Part).unwrap();
    world.set_parent(floor, Some(world.workspace())).unwrap();
    for (name, value) in [
        ("Size", Value::Vector3(Vector3::new(400.0, 1.0, 400.0))),
        ("Position", Value::Vector3(Vector3::new(0.0, -0.5, 0.0))),
        ("Anchored", Value::Bool(true)),
    ] {
        world.set_property(floor, name, value).unwrap();
    }
    let weather = world.create(ClassName::Weather).unwrap();
    world.set_parent(weather, Some(world.lighting())).unwrap();
    for (name, value) in [
        ("Precipitation", falling("None")),
        ("Intensity", Value::Number(1.0)),
        ("TransitionTime", Value::Number(transition)),
    ] {
        world.set_property(weather, name, value).unwrap();
    }
    (world, weather)
}

fn falling(item: &str) -> Value {
    Value::EnumItem(EnumItem {
        enum_name: "Precipitation".to_owned(),
        item: item.to_owned(),
    })
}

/// The middle brightness of the frame (the floor's, whatever raindrops
/// cross it), as a host draws it after moving its particles on by
/// `seconds`.
fn floor_brightness(
    renderer: &mut TextureRenderer,
    particles: &mut Particles,
    world: &World,
    seconds: f32,
) -> u32 {
    let folder = std::env::temp_dir().join(format!("axcenity-weather-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let assets = Assets::new(&folder);
    // Particles move at most a quarter of a second a frame.
    let mut left = seconds;
    while left > 0.0 {
        particles.update(world, left.min(0.25));
        left -= 0.25;
    }
    let camera = FlyCamera::looking_at(Vec3::new(0.0, 20.0, 12.0), Vec3::ZERO);
    let mut list = axcenity_scene::build_draw_list(world, &assets, camera.to_render_camera());
    list.sprites = particles.sprites(world, &assets);
    particles.add_weather(&mut list);
    renderer.render(&list).unwrap();
    let mut brightness: Vec<u32> = renderer
        .read_pixels()
        .chunks(4)
        .map(|pixel| pixel[..3].iter().map(|&c| u32::from(c)).sum())
        .collect();
    brightness.sort_unstable();
    brightness[brightness.len() / 2]
}

#[test]
fn rain_wets_the_ground_over_time_and_it_dries_after() {
    let Some((device, queue)) = test_gpu() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, 160, 90);
    let (mut world, weather) = floor_under_weather(2.0);
    let mut particles = Particles::new();
    let dry = floor_brightness(&mut renderer, &mut particles, &world, 0.25);
    assert!(
        !particles.is_active(&world),
        "dry and settled: no frames needed"
    );

    world
        .set_property(weather, "Precipitation", falling("Rain"))
        .unwrap();
    let mut wetting = Vec::new();
    for _ in 0..3 {
        wetting.push(floor_brightness(
            &mut renderer,
            &mut particles,
            &world,
            0.75,
        ));
    }
    assert!(particles.is_active(&world), "it's raining");
    assert!(
        dry > wetting[0] && wetting[0] > wetting[1] && wetting[1] >= wetting[2],
        "darker as it wets: {dry} dry, then {wetting:?}"
    );

    world
        .set_property(weather, "Precipitation", falling("None"))
        .unwrap();
    let drying = floor_brightness(&mut renderer, &mut particles, &world, 1.0);
    let dried = floor_brightness(&mut renderer, &mut particles, &world, 1.5);
    assert!(
        wetting[2] < drying && drying < dried,
        "lighter as it dries: {} soaked, {drying}, {dried}",
        wetting[2]
    );
    assert_eq!(dried, dry, "dry again");
    assert!(!particles.is_active(&world));
}

#[test]
fn a_script_sets_the_weather_and_it_saves_with_the_world() {
    let (mut world, mut scripts, folder) = start_example_in("hello-part");
    let (mut physics, mut clock) = (Physics::new(), 0);
    assert!(
        axcenity_scene::build_draw_list(
            &world,
            &Assets::new(&folder),
            FlyCamera::looking_at(Vec3::new(0.0, 10.0, 10.0), Vec3::ZERO).to_render_camera()
        )
        .weather
        .is_clear(),
        "no Weather, no weather"
    );

    let script = world.create(ClassName::Script).unwrap();
    world
        .set_parent(script, Some(world.server_script_service()))
        .unwrap();
    scripts.start_script(
        &mut world,
        script,
        "scripts/snow.server.lua",
        "local weather = Instance.new(\"Weather\")\n\
         weather.Precipitation = Enum.Precipitation.Snow\n\
         weather.Intensity = 0.25\n\
         weather.SnowCover = 0.75\n\
         weather.Parent = game.Lighting\n\
         print(weather.Precipitation == Enum.Precipitation.Snow)",
    );
    run_until(&mut world, &mut scripts, &mut physics, &mut clock, 0.1);
    let printed: Vec<String> = scripts
        .take_output()
        .into_iter()
        .map(|line| line.text)
        .collect();
    assert!(printed.contains(&"true".to_owned()), "{printed:?}");

    let drawn = |world: &World| {
        axcenity_scene::build_draw_list(
            world,
            &Assets::new(&folder),
            FlyCamera::looking_at(Vec3::new(0.0, 10.0, 10.0), Vec3::ZERO).to_render_camera(),
        )
        .weather
    };
    let snowy = axcenity_render::Weather {
        wetness: 0.0,
        snow: 0.75,
    };
    assert_eq!(drawn(&world), snowy);

    axcenity_project::save_world(&folder, &world).unwrap();
    let reloaded = axcenity_project::load(&folder).unwrap().world;
    let weather = reloaded
        .find_first_child(reloaded.lighting(), "Weather")
        .and_then(|id| reloaded.get(id))
        .expect("the Weather saved");
    assert_eq!(
        weather.property("Precipitation"),
        Some(&falling("Snow")),
        "its Precipitation saved"
    );
    assert_eq!(drawn(&reloaded), snowy, "and draws the same");
}
