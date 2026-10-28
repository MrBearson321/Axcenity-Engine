//! Clouds in a world, drawn: Lighting's Clouds shade the ground, the
//! Workspace's GlobalWind carries their shadows, and taking them out
//! leaves the frame as it was.

use axcenity_core::{ClassName, Value, Vector3};
use axcenity_render::TextureRenderer;

use super::tests::test_gpu;
use super::*;

const SIZE: (u32, u32) = (160, 90);

/// Where the clouds' shadows under the default sun are half cloud, half
/// clear.
const CLOUDY: Vec3 = Vec3::new(0.0, 0.0, 750.0);

/// A wide grey floor around [`CLOUDY`].
fn floor_world() -> World {
    let mut world = World::new();
    let floor = world.create(ClassName::Part).unwrap();
    world.set_parent(floor, Some(world.workspace())).unwrap();
    for (name, value) in [
        ("Size", Value::Vector3(Vector3::new(2000.0, 1.0, 2000.0))),
        (
            "Position",
            Value::Vector3(Vector3::new(CLOUDY.x, -0.5, CLOUDY.z)),
        ),
        ("Anchored", Value::Bool(true)),
    ] {
        world.set_property(floor, name, value).unwrap();
    }
    world
}

/// `world` from high above the floor, `time` seconds in.
fn frame(renderer: &mut TextureRenderer, world: &World, time: f32) -> Vec<u8> {
    // An empty project: these worlds read no files.
    let folder = std::env::temp_dir().join(format!("axcenity-clouds-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let assets = Assets::new(&folder);
    let camera = FlyCamera::looking_at(CLOUDY + Vec3::new(0.0, 600.0, 0.5), CLOUDY);
    let mut list = axcenity_scene::build_draw_list(world, &assets, camera.to_render_camera());
    list.time = time;
    renderer.render(&list).unwrap();
    renderer.read_pixels()
}

fn set_wind(world: &mut World, x: f32) {
    let workspace = world.workspace();
    world
        .set_property(
            workspace,
            "GlobalWind",
            Value::Vector3(Vector3::new(x, 0.0, 0.0)),
        )
        .unwrap();
}

#[test]
fn global_wind_carries_cloud_shadows_and_removing_clouds_changes_nothing() {
    let Some((device, queue)) = test_gpu() else {
        return;
    };
    let (width, height) = SIZE;
    let mut renderer = TextureRenderer::new(&device, &queue, width, height);
    let mut world = floor_world();
    let clear = frame(&mut renderer, &world, 0.0);

    let clouds = world.create(ClassName::Clouds).unwrap();
    world.set_parent(clouds, Some(world.lighting())).unwrap();
    let shaded = frame(&mut renderer, &world, 0.0);
    let darker = shaded
        .chunks(4)
        .zip(clear.chunks(4))
        .filter(|(cloudy, clear)| u32::from(cloudy[0]) + 40 < u32::from(clear[0]))
        .count();
    let pixels = (width * height) as usize;
    assert!(
        darker > pixels / 10 && darker < pixels * 9 / 10,
        "clouds shade some of the floor: {darker} of {pixels}"
    );

    // Five seconds of wind moves the shadows on.
    set_wind(&mut world, 30.0);
    let blown = frame(&mut renderer, &world, 5.0);
    tests::write_png(
        &std::env::temp_dir().join("axcenity-cloud-shadows.png"),
        &shaded,
        SIZE,
    );
    tests::write_png(
        &std::env::temp_dir().join("axcenity-cloud-shadows-blown.png"),
        &blown,
        SIZE,
    );
    assert!(blown != shaded, "the shadows move with the wind");
    // Still air leaves them where they are.
    set_wind(&mut world, 0.0);
    let still = frame(&mut renderer, &world, 10.0);
    assert!(frame(&mut renderer, &world, 15.0) == still);

    world.destroy(clouds).unwrap();
    assert!(
        frame(&mut renderer, &world, 20.0) == clear,
        "without Clouds the frame is as it was"
    );
}
