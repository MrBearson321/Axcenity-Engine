//! Phase 74: the hello-roads example in the player's frame order. Its road
//! is one straight 8-stud segment bent along a curving spline by a
//! SplineMesh, made when the game starts, and the car its script drives
//! goes the whole way along it on the bent pieces.

use axcenity_core::spline::SplineCurve;
use axcenity_core::{ClassName, InstanceId, Value, Vector3};

use super::tests::{run_until, start_example_in};
use super::*;

fn position(world: &World, id: InstanceId) -> Vector3 {
    world
        .get(id)
        .unwrap()
        .property("Position")
        .and_then(Value::as_vector3)
        .unwrap()
}

/// The curve's point `distance` along it, with its right and up.
fn frame(curve: &SplineCurve, distance: f32) -> (Vec3, Vec3, Vec3, Vec3) {
    let v = |v: Vector3| Vec3::new(v.x, v.y, v.z);
    let point = v(curve.position_at_distance(distance));
    let forward = v(curve.direction_at_distance(distance));
    let right = forward.cross(Vec3::Y).normalize();
    (point, right, right.cross(forward), forward)
}

fn find(world: &World, path: &[&str]) -> InstanceId {
    path.iter().fold(world.workspace(), |at, name| {
        world.find_first_child(at, name).unwrap()
    })
}

#[test]
fn the_car_drives_the_whole_curving_road_on_its_bent_pieces() {
    let (mut world, mut scripts, folder) = start_example_in("hello-roads");
    let assets = Assets::new(&folder);
    scripts.set_assets(assets.clone());
    let mut physics = Physics::with_assets(assets.clone());
    let mut clock = 0;
    run_until(&mut world, &mut scripts, &mut physics, &mut clock, 1.0);

    // The road is there: a piece every 8 studs or so, anchored.
    let road = find(&world, &["Road"]);
    let curve = SplineCurve::of(&world, road).unwrap();
    let pieces = world
        .get(find(&world, &["Road", "Asphalt"]))
        .unwrap()
        .children()
        .to_vec();
    assert_eq!(pieces.len(), (curve.length() / 8.0).round() as usize);
    let output: Vec<String> = scripts.take_output().into_iter().map(|l| l.text).collect();
    assert!(
        !output.iter().any(|line| line.contains("can't load")),
        "{output:?}"
    );

    // The car, driven by its script, goes the whole way on the road: its
    // chassis rides a road's thickness above the grass (on the grass it
    // would be about a stud lower), never far from the curve.
    let chassis = find(&world, &["AutoCar", "Chassis"]);
    let mut reached = false;
    let mut furthest: f32 = 0.0;
    while clock < 60 * 40 && !reached {
        let until = f64::from(clock + 6) / 60.0;
        run_until(&mut world, &mut scripts, &mut physics, &mut clock, until);
        let at = position(&world, chassis);
        let along = curve.closest_distance(at);
        let on_curve = curve.position_at_distance(along);
        let off = (at.x - on_curve.x).hypot(at.z - on_curve.z);
        assert!(
            off < 5.0,
            "{:.1}s: {off:.1} studs off the road at {at:?}",
            clock as f32 / 60.0
        );
        assert!(
            at.y > 2.2,
            "{:.1}s: fell off the road: {at:?}",
            clock as f32 / 60.0
        );
        furthest = furthest.max(along);
        reached = scripts
            .take_output()
            .iter()
            .any(|line| line.text == "The car reached the end of the road");
    }
    assert!(reached, "got {furthest:.0} of {:.0} studs", curve.length());
    assert!(furthest > curve.length() - 12.0, "{furthest}");
}

#[test]
fn the_examples_road_and_scatters_save_only_their_settings() {
    let (mut world, mut scripts, folder) = start_example_in("hello-roads");
    let assets = Assets::new(&folder);
    scripts.set_assets(assets.clone());
    let mut physics = Physics::with_assets(assets);
    let mut clock = 0;
    run_until(&mut world, &mut scripts, &mut physics, &mut clock, 0.1);
    let made = |path: &[&str]| world.get(find(&world, path)).unwrap().children().len();
    assert!(made(&["Road", "Asphalt"]) > 20);
    assert!(made(&["Treeline", "Trees"]) > 5);
    assert!(made(&["Grove", "Woods"]) > 20);
    let saved = axcenity_project::write_world(&world);
    let original = std::fs::read_to_string(folder.join("world.json")).unwrap();
    assert!(!saved.contains("MeshPart"), "nothing it made is saved");
    assert_eq!(
        axcenity_project::parse_world(&saved).unwrap().len(),
        axcenity_project::parse_world(&original).unwrap().len()
    );
    assert!(
        world
            .descendants(world.workspace())
            .iter()
            .any(|&id| world.get(id).unwrap().class() == ClassName::MeshPart)
    );
}

/// Seen from above, the road is drawn all along its curve: every point a
/// little off its middle (clear of the yellow line) is asphalt, never the
/// grass under it. With `AXCENITY_SNAPSHOTS` naming a folder, the frame is
/// saved there as `hello-roads-road.png`. Skipped without a GPU.
#[test]
fn the_road_is_drawn_along_its_whole_curve_with_no_gaps() {
    use axcenity_render::{TextureRenderer, wgpu};
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else {
        return;
    };
    let Ok((device, queue)) = pollster::block_on(adapter.request_device(&Default::default()))
    else {
        return;
    };
    let (mut world, mut scripts, folder) = start_example_in("hello-roads");
    let assets = Assets::new(&folder);
    scripts.set_assets(assets.clone());
    let mut physics = Physics::with_assets(assets.clone());
    let mut clock = 0;
    run_until(&mut world, &mut scripts, &mut physics, &mut clock, 0.1);

    let camera = FlyCamera::looking_at(Vec3::new(55.0, 170.0, 45.0), Vec3::new(55.0, 0.0, -15.0))
        .to_render_camera();
    let (width, height) = (640, 480);
    let mut renderer = TextureRenderer::new(&device, &queue, width, height);
    let list = axcenity_scene::build_draw_list(&world, &assets, camera);
    // Models load the first time they're drawn, so draw twice.
    renderer.render(&list).unwrap();
    renderer.render(&list).unwrap();
    let pixels = renderer.read_pixels();
    if let Some(snapshots) = std::env::var_os("AXCENITY_SNAPSHOTS") {
        let file =
            std::fs::File::create(std::path::Path::new(&snapshots).join("hello-roads-road.png"))
                .unwrap();
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&pixels)
            .unwrap();
    }
    let view = axcenity_scene::CameraView::new(camera, width as f32 / height as f32);
    let curve = SplineCurve::of(&world, find(&world, &["Road"])).unwrap();
    let pixel = |point: Vec3| -> [u8; 3] {
        let at = view.project_point(point).unwrap();
        let x = ((at.x + 1.0) / 2.0 * width as f32) as usize;
        let y = ((1.0 - at.y) / 2.0 * height as f32) as usize;
        let i = (y.min(height as usize - 1) * width as usize + x.min(width as usize - 1)) * 4;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    // The grass is an olive texture, and the road a blue-grey: green well
    // above blue tells them apart whatever the light makes of the grass.
    let grassy = |[_, g, b]: [u8; 3]| i32::from(g) > i32::from(b) + 20;
    // Grass beside the road looks like grass.
    let (start, right, _, _) = frame(&curve, 40.0);
    assert!(
        grassy(pixel(start + right * 14.0)),
        "{:?}",
        pixel(start + right * 14.0)
    );
    // Past the car at its start, the road is drawn every 2 studs.
    let mut distance = 20.0;
    while distance < curve.length() - 2.0 {
        let (point, right, up, _) = frame(&curve, distance);
        for side in [-3.0, 3.0] {
            let on_top = point + right * side + up * 1.05;
            let color = pixel(on_top);
            assert!(
                !grassy(color),
                "grass shows through at {distance} studs: {color:?}"
            );
        }
        distance += 2.0;
    }
}
