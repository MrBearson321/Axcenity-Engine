use axcenity_core::{ClassName, World};
use axcenity_render::Camera;

use super::*;

fn pose(position: Vec3, size: Vec3) -> Pose {
    Pose {
        position,
        rotation: Quat::IDENTITY,
        size,
    }
}

fn some_part() -> InstanceId {
    let mut world = World::new();
    world.create(ClassName::Part).unwrap()
}

/// A ray from far in front of the scene, straight down -Z, through (x, y).
fn ray_through(x: f32, y: f32) -> Ray {
    Ray {
        origin: Vec3::new(x, y, 50.0),
        direction: Vec3::NEG_Z,
    }
}

fn vector(values: &[(&'static str, Value)], name: &str) -> Vec3 {
    let (_, value) = values.iter().find(|(n, _)| *n == name).unwrap();
    to_vec3(value.as_vector3().unwrap())
}

const NO_SNAP: Snap = Snap {
    move_on: false,
    move_step: 1.0,
    rotate_on: false,
    rotate_step: 15.0,
    surfaces: false,
    collisions: false,
    join: false,
    points: false,
};

fn assert_near(a: Vec3, b: Vec3) {
    assert!(a.abs_diff_eq(b, 1e-4), "{a} is not close to {b}");
}

#[test]
fn move_handle_slides_the_part_along_its_axis_only() {
    let start = pose(Vec3::new(1.0, 2.0, 0.0), Vec3::ONE);
    let mut drag = Drag::start(
        Handle::Move(Axis::X),
        some_part(),
        start,
        ray_through(1.5, 2.0),
    )
    .unwrap();
    // The mouse goes 3.25 studs right and 4 up; only the X part counts.
    let values = drag.update(ray_through(4.75, 6.0), &NO_SNAP).unwrap();
    assert_near(vector(&values, "Position"), Vec3::new(4.25, 2.0, 0.0));

    let mut drag = Drag::start(
        Handle::Move(Axis::Y),
        some_part(),
        start,
        ray_through(1.0, 2.5),
    )
    .unwrap();
    let values = drag.update(ray_through(9.0, 1.0), &NO_SNAP).unwrap();
    assert_near(vector(&values, "Position"), Vec3::new(1.0, 0.5, 0.0));
}

#[test]
fn scale_handle_moves_one_face_and_keeps_the_opposite_one() {
    // A 4-stud-wide part from x = -2 to x = 2; grab its right face.
    let start = pose(Vec3::ZERO, Vec3::new(4.0, 1.0, 2.0));
    let handle = Handle::Scale(Axis::X, Side::Plus);
    let mut drag = Drag::start(handle, some_part(), start, ray_through(2.5, 0.0)).unwrap();
    let values = drag.update(ray_through(5.5, 0.0), &NO_SNAP).unwrap();
    assert_near(vector(&values, "Size"), Vec3::new(7.0, 1.0, 2.0));
    // Now from x = -2 to x = 5: the left face hasn't moved.
    assert_near(vector(&values, "Position"), Vec3::new(1.5, 0.0, 0.0));

    // The left face, pulled outward to the left.
    let handle = Handle::Scale(Axis::X, Side::Minus);
    let mut drag = Drag::start(handle, some_part(), start, ray_through(-2.5, 0.0)).unwrap();
    let values = drag.update(ray_through(-3.5, 0.0), &NO_SNAP).unwrap();
    assert_near(vector(&values, "Size"), Vec3::new(5.0, 1.0, 2.0));
    assert_near(vector(&values, "Position"), Vec3::new(-0.5, 0.0, 0.0));
}

#[test]
fn scale_handles_follow_a_turned_part_and_never_shrink_it_to_nothing() {
    // Turned 90° around Z, so the part's X runs along the world's Y.
    let start = Pose {
        position: Vec3::ZERO,
        rotation: Quat::from_rotation_z(90f32.to_radians()),
        size: Vec3::new(4.0, 1.0, 2.0),
    };
    let handle = Handle::Scale(Axis::X, Side::Plus);
    let mut drag = Drag::start(handle, some_part(), start, ray_through(0.0, 2.0)).unwrap();
    let values = drag.update(ray_through(0.0, 3.0), &NO_SNAP).unwrap();
    assert_near(vector(&values, "Size"), Vec3::new(5.0, 1.0, 2.0));
    assert_near(vector(&values, "Position"), Vec3::new(0.0, 0.5, 0.0));

    let values = drag.update(ray_through(0.0, -30.0), &NO_SNAP).unwrap();
    assert_eq!(vector(&values, "Size").x, MIN_SIZE);
}

/// Looking straight down -Z at `at` from 20 studs away, in a 1280×720
/// viewport (nudged `off` along X, to see a ring nearly edge-on).
fn screen_along_z(at: Vec3, off: f32) -> Screen {
    let camera = Camera {
        position: at + Vec3::new(off, 0.0, 20.0),
        look_at: at,
        fov_y_degrees: 50.0,
    };
    Screen::new(
        camera,
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0)),
    )
}

/// How many points along a ring turn it `degrees`.
fn points_for(degrees: f32) -> f32 {
    degrees.to_radians() / TURN_PER_POINT
}

/// Moves the mouse of a ring `drag` from `from` by `points` the way the
/// ring turns forward there, in `steps`, and returns the last values.
fn turn_along(
    drag: &mut Drag,
    screen: &Screen,
    from: egui::Pos2,
    points: f32,
    snap: &Snap,
) -> Vec<(&'static str, Value)> {
    let spin = drag.spin;
    let mut values = None;
    for step in 1..=8 {
        let at = from + spin * points * step as f32 / 8.0;
        values = drag.update(Pointer::on(screen, at, false), snap);
    }
    values.unwrap()
}

#[test]
fn rotate_ring_turns_the_part_around_its_axis() {
    let start = pose(Vec3::ZERO, Vec3::ONE);
    let screen = screen_along_z(Vec3::ZERO, 0.0);
    // Looking straight down the Z axis at the Z ring: grab it on the
    // right (+X); its forward way there is up the screen.
    let handle = Handle::Rotate(Axis::Z);
    let grab = ring_point(&screen, start, Axis::Z, 0.0);
    let mut drag = Drag::turn(handle, some_part(), start, &screen, grab).unwrap();
    assert!(drag.spin.y < -0.99, "{:?}", drag.spin);
    let values = turn_along(&mut drag, &screen, grab, points_for(90.0), &NO_SNAP);
    assert_near(vector(&values, "Orientation"), Vec3::new(0.0, 0.0, 90.0));

    // Carrying on past half a turn keeps counting.
    let values = turn_along(&mut drag, &screen, grab, points_for(181.91), &NO_SNAP);
    let turned = drag.turned.to_degrees();
    assert!((turned - 181.91).abs() < 0.01, "{turned}");
    // 181.91° is the same as -178.09°, which is how Orientation says it.
    let z = vector(&values, "Orientation").z;
    assert!((z + 178.09).abs() < 0.01, "{z}");
}

#[test]
fn the_grabbed_point_of_a_ring_follows_the_mouse() {
    let start = pose(Vec3::new(1.0, 0.5, 0.0), Vec3::ONE);
    let screen = screen_along_z(start.position, 0.0);
    let grab = ring_point(&screen, start, Axis::Z, 0.7);
    let handle = Handle::Rotate(Axis::Z);
    let mut drag = Drag::turn(handle, some_part(), start, &screen, grab).unwrap();
    // A few points along the ring: the ring's point turned that far is
    // under the mouse.
    let values = turn_along(&mut drag, &screen, grab, 6.0, &NO_SNAP);
    let turned = vector(&values, "Orientation").z.to_radians();
    let under = ring_point(&screen, start, Axis::Z, 0.7 + turned);
    let mouse = grab + drag.spin * 6.0;
    assert!(under.distance(mouse) < 0.5, "{under:?} vs {mouse:?}");
}

#[test]
fn a_drag_as_long_turns_as_far_whether_a_ring_faces_the_camera_or_is_edge_on() {
    let start = pose(Vec3::ZERO, Vec3::ONE);
    let turned_by = |screen: &Screen, axis: Axis| {
        // Grab the ring where it's drawn, and drag 50 points along the
        // line it's drawn with there.
        let shape = shapes(Tool::Rotate, start, screen)
            .into_iter()
            .find(|shape| shape.handle == Handle::Rotate(axis))
            .unwrap();
        let [a, b] = shape
            .lines
            .iter()
            .copied()
            .max_by(|x, y| (x[1] - x[0]).length().total_cmp(&(y[1] - y[0]).length()))
            .unwrap();
        let grab = a.lerp(b, 0.5);
        let along = (b - a).normalized();
        let mut drag = Drag::turn(Handle::Rotate(axis), some_part(), start, screen, grab).unwrap();
        for step in 1..=10 {
            let at = grab + along * 5.0 * step as f32;
            drag.update(Pointer::on(screen, at, false), &NO_SNAP)
                .unwrap();
        }
        drag.turned.abs().to_degrees()
    };
    let facing = turned_by(&screen_along_z(Vec3::ZERO, 0.0), Axis::Z);
    let expected = (50.0 * TURN_PER_POINT).to_degrees();
    assert!((facing - expected).abs() < 0.05, "{facing} vs {expected}");
    // The X ring seen from along Z is a line; nudged, a thin ellipse.
    for off in [0.0, 0.3, 2.0] {
        let edge_on = turned_by(&screen_along_z(Vec3::ZERO, off), Axis::X);
        assert!(
            (edge_on - facing).abs() < 0.05,
            "nudged {off}: {edge_on} vs {facing}"
        );
    }
}

#[test]
fn shift_turns_a_quarter_as_fast_and_letting_go_does_not_jump() {
    let start = pose(Vec3::ZERO, Vec3::ONE);
    let screen = screen_along_z(Vec3::ZERO, 0.0);
    let grab = ring_point(&screen, start, Axis::Z, 0.0);
    let handle = Handle::Rotate(Axis::Z);
    let mut drag = Drag::turn(handle, some_part(), start, &screen, grab).unwrap();
    let up = drag.spin;
    let z_at = |drag: &mut Drag, points: f32, fine: bool| {
        let values = drag
            .update(Pointer::on(&screen, grab + up * points, fine), &NO_SNAP)
            .unwrap();
        vector(&values, "Orientation").z
    };
    let step = points_for(20.0);
    let normal = z_at(&mut drag, step, false);
    assert!((normal - 20.0).abs() < 1e-3, "{normal}");
    let fine = z_at(&mut drag, step * 2.0, true);
    assert!((fine - 25.0).abs() < 1e-3, "a quarter as far: {fine}");
    let let_go = z_at(&mut drag, step * 2.0, false);
    assert!((let_go - fine).abs() < 1e-4, "no jump: {let_go}");
    let after = z_at(&mut drag, step * 3.0, false);
    assert!((after - 45.0).abs() < 1e-3, "full speed again: {after}");
}

#[test]
fn rotating_a_turned_part_turns_it_further_around_the_world_axis() {
    // Already turned 30° around Y; another 15° around Y gives 45°.
    let start = Pose {
        position: Vec3::ZERO,
        rotation: Quat::from_rotation_y(30f32.to_radians()),
        size: Vec3::ONE,
    };
    let camera = Camera {
        position: Vec3::new(0.0, 20.0, 6.0),
        look_at: Vec3::ZERO,
        fov_y_degrees: 50.0,
    };
    let screen = Screen::new(
        camera,
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0)),
    );
    let frame = Pose {
        rotation: Quat::IDENTITY,
        ..start
    };
    let handle = Handle::Rotate(Axis::Y);
    let grab = ring_point(&screen, frame, Axis::Y, 0.0);
    let mut drag = Drag::turn(handle, some_part(), start, &screen, grab).unwrap();
    let values = turn_along(&mut drag, &screen, grab, points_for(15.0), &Snap::default());
    assert_eq!(vector(&values, "Orientation"), Vec3::new(0.0, 45.0, 0.0));
}

#[test]
fn snapping_moves_and_turns_in_steps_and_can_be_turned_off() {
    let start = pose(Vec3::new(0.3, 0.0, 0.0), Vec3::ONE);
    let handle = Handle::Move(Axis::X);
    let mut drag = Drag::start(handle, some_part(), start, ray_through(0.3, 0.0)).unwrap();
    let snapped = drag
        .update(ray_through(2.0, 0.0), &Snap::default())
        .unwrap();
    // 1.7 studs rounds to 2 steps of 1 stud, counted from 0.3.
    assert_near(vector(&snapped, "Position"), Vec3::new(2.3, 0.0, 0.0));
    let free = drag.update(ray_through(2.0, 0.0), &NO_SNAP).unwrap();
    assert_near(vector(&free, "Position"), Vec3::new(2.0, 0.0, 0.0));

    let handle = Handle::Rotate(Axis::Z);
    let screen = screen_along_z(start.position, 0.0);
    let grab = ring_point(&screen, start, Axis::Z, 0.0);
    let mut drag = Drag::turn(handle, some_part(), start, &screen, grab).unwrap();
    // About 20° around: snaps to 15°.
    let values = turn_along(&mut drag, &screen, grab, points_for(20.0), &Snap::default());
    assert_eq!(vector(&values, "Orientation"), Vec3::new(0.0, 0.0, 15.0));
    let free = turn_along(&mut drag, &screen, grab, points_for(20.0), &NO_SNAP);
    assert_near(vector(&free, "Orientation"), Vec3::new(0.0, 0.0, 20.0));
    // Flipped (Ctrl), on is off and off is on, with the same steps.
    let flipped = turn_along(
        &mut drag,
        &screen,
        grab,
        points_for(20.0),
        &Snap::default().flipped(),
    );
    assert_near(vector(&flipped, "Orientation"), Vec3::new(0.0, 0.0, 20.0));
    let flipped = turn_along(
        &mut drag,
        &screen,
        grab,
        points_for(20.0),
        &NO_SNAP.flipped(),
    );
    assert_eq!(vector(&flipped, "Orientation"), Vec3::new(0.0, 0.0, 15.0));
}

#[test]
fn changing_the_step_size_changes_the_steps() {
    let start = pose(Vec3::ZERO, Vec3::new(4.0, 1.0, 2.0));
    let quarter = Snap {
        move_step: 0.25,
        rotate_step: 45.0,
        ..Snap::default()
    };
    let handle = Handle::Move(Axis::X);
    let mut drag = Drag::start(handle, some_part(), start, ray_through(0.0, 0.0)).unwrap();
    let values = drag.update(ray_through(1.1, 0.0), &quarter).unwrap();
    assert_near(vector(&values, "Position"), Vec3::new(1.0, 0.0, 0.0));
    // Scale uses the move step too.
    let handle = Handle::Scale(Axis::X, Side::Plus);
    let mut drag = Drag::start(handle, some_part(), start, ray_through(2.0, 0.0)).unwrap();
    let values = drag.update(ray_through(2.4, 0.0), &quarter).unwrap();
    assert_near(vector(&values, "Size"), Vec3::new(4.5, 1.0, 2.0));

    let handle = Handle::Rotate(Axis::Z);
    let screen = screen_along_z(start.position, 0.0);
    let grab = ring_point(&screen, start, Axis::Z, 0.0);
    let mut drag = Drag::turn(handle, some_part(), start, &screen, grab).unwrap();
    let values = turn_along(&mut drag, &screen, grab, points_for(30.0), &quarter);
    assert_eq!(vector(&values, "Orientation"), Vec3::new(0.0, 0.0, 45.0));
}

#[test]
fn a_handle_seen_end_on_cannot_be_grabbed() {
    let start = pose(Vec3::ZERO, Vec3::ONE);
    // Looking straight along Z, the Z arrow is a dot.
    let ray = ray_through(0.0, 0.0);
    assert!(Drag::start(Handle::Move(Axis::Z), some_part(), start, ray).is_none());
}

/// The editor's start camera, looking at the origin, in a 1280×720 viewport.
fn screen() -> Screen {
    let camera = Camera {
        position: Vec3::new(12.0, 8.0, 16.0),
        look_at: Vec3::ZERO,
        fov_y_degrees: 50.0,
    };
    Screen::new(
        camera,
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0)),
    )
}

#[test]
fn each_tool_shows_its_own_handles() {
    let start = pose(Vec3::ZERO, Vec3::new(4.0, 1.0, 2.0));
    let handles = |tool| -> Vec<Handle> {
        shapes(tool, start, &screen())
            .iter()
            .map(|shape| shape.handle)
            .collect()
    };
    assert!(handles(Tool::Select).is_empty());
    assert_eq!(handles(Tool::Move), Axis::ALL.map(Handle::Move).to_vec());
    assert_eq!(handles(Tool::Scale).len(), 6);
    assert_eq!(
        handles(Tool::Rotate),
        Axis::ALL.map(Handle::Rotate).to_vec()
    );
}

#[test]
fn the_mouse_over_a_handle_finds_that_handle() {
    let screen = screen();
    let start = pose(Vec3::ZERO, Vec3::new(4.0, 1.0, 2.0));
    let move_shapes = shapes(Tool::Move, start, &screen);
    for axis in Axis::ALL {
        // Halfway along each arrow, and a few points off to the side.
        let shape = move_shapes
            .iter()
            .find(|s| s.handle == Handle::Move(axis))
            .unwrap();
        let [a, b] = shape.lines[0];
        let near = a + (b - a) / 2.0 + egui::vec2(0.0, 3.0);
        assert_eq!(hovered(&move_shapes, near), Some(Handle::Move(axis)));
    }
    // Far from the part, nothing.
    assert_eq!(hovered(&move_shapes, egui::pos2(5.0, 5.0)), None);

    // The ball on the top face of the part.
    let scale_shapes = shapes(Tool::Scale, start, &screen);
    let top = screen.point(Vec3::new(0.0, 0.5, 0.0)).unwrap();
    let above_top = scale_shapes
        .iter()
        .find(|s| s.handle == Handle::Scale(Axis::Y, Side::Plus))
        .unwrap();
    let End::Ball(ball) = above_top.end else {
        panic!("scale handles end in a ball");
    };
    assert!(ball.y < top.y, "the ball sits above the top face");
    assert_eq!(
        hovered(&scale_shapes, ball),
        Some(Handle::Scale(Axis::Y, Side::Plus))
    );
}

#[test]
fn handles_stay_the_same_size_on_screen_at_any_distance() {
    let screen = screen();
    let near = shapes(Tool::Move, pose(Vec3::ZERO, Vec3::ONE), &screen);
    // Twice as far from the camera along the same line of sight.
    let far_point = Vec3::new(-12.0, -8.0, -16.0);
    let far = shapes(Tool::Move, pose(far_point, Vec3::ONE), &screen);
    let length = |shapes: &[Shape]| {
        let [a, b] = shapes[1].lines[0];
        (b - a).length()
    };
    assert!((length(&near) - length(&far)).abs() < 1.0);
}

#[test]
fn the_pose_comes_from_the_parts_properties() {
    let mut world = World::new();
    let part = world.create(ClassName::Part).unwrap();
    world
        .set_property(
            part,
            "Orientation",
            Value::Vector3(Vector3::new(0.0, 90.0, 0.0)),
        )
        .unwrap();
    let pose = Pose::of(world.get(part).unwrap()).unwrap();
    assert_eq!(pose.size, Vec3::new(4.0, 1.0, 2.0));
    assert_near(pose.rotation * Vec3::X, Vec3::NEG_Z);
    assert!(Pose::of(world.get(world.workspace()).unwrap()).is_none());
}

fn two_parts() -> Vec<(InstanceId, Pose)> {
    let mut world = World::new();
    let a = world.create(ClassName::Part).unwrap();
    let b = world.create(ClassName::Part).unwrap();
    vec![
        (a, pose(Vec3::new(-2.0, 0.0, 0.0), Vec3::ONE)),
        (b, pose(Vec3::new(2.0, 0.0, 0.0), Vec3::ONE)),
    ]
}

fn value_of(values: &PartValues, id: InstanceId, name: &str) -> Vec3 {
    let (_, values) = values.iter().find(|(part, _)| *part == id).unwrap();
    vector(values, name)
}

#[test]
fn global_move_handles_sit_in_the_middle_and_move_every_part() {
    let parts = two_parts();
    let poses: Vec<Pose> = parts.iter().map(|(_, pose)| *pose).collect();
    let frame = handle_frame(Tool::Move, Modes::default(), &poses).unwrap();
    assert_near(frame.position, Vec3::ZERO);
    let mut drag = Drag::begin(
        Handle::Move(Axis::Y),
        frame,
        parts.clone(),
        Modes::default(),
        ray_through(0.5, 0.0),
        None,
    )
    .unwrap();
    let values = drag
        .update_all(ray_through(0.5, 3.0).into(), &NO_SNAP)
        .unwrap();
    assert_near(
        value_of(&values, parts[0].0, "Position"),
        Vec3::new(-2.0, 3.0, 0.0),
    );
    assert_near(
        value_of(&values, parts[1].0, "Position"),
        Vec3::new(2.0, 3.0, 0.0),
    );
}

#[test]
fn local_axes_move_each_part_along_its_own_axis() {
    let mut parts = two_parts();
    // The second part is turned 90° around Z: its X runs up the world's Y.
    parts[1].1.rotation = Quat::from_rotation_z(90f32.to_radians());
    let modes = Modes {
        axes: MoveAxes::Local,
        ..Modes::default()
    };
    let poses: Vec<Pose> = parts.iter().map(|(_, pose)| *pose).collect();
    let frame = handle_frame(Tool::Move, modes, &poses).unwrap();
    // The handles are the last part's: its X arrow points up.
    let mut drag = Drag::begin(
        Handle::Move(Axis::X),
        frame,
        parts.clone(),
        modes,
        ray_through(2.0, 0.5),
        None,
    )
    .unwrap();
    let values = drag
        .update_all(ray_through(2.0, 2.5).into(), &NO_SNAP)
        .unwrap();
    assert_near(
        value_of(&values, parts[0].0, "Position"),
        Vec3::new(0.0, 0.0, 0.0),
    );
    assert_near(
        value_of(&values, parts[1].0, "Position"),
        Vec3::new(2.0, 2.0, 0.0),
    );

    // With Last, both go the last part's way.
    let modes = Modes {
        axes: MoveAxes::Last,
        ..Modes::default()
    };
    let mut drag = Drag::begin(
        Handle::Move(Axis::X),
        frame,
        parts.clone(),
        modes,
        ray_through(2.0, 0.5),
        None,
    )
    .unwrap();
    let values = drag
        .update_all(ray_through(2.0, 2.5).into(), &NO_SNAP)
        .unwrap();
    assert_near(
        value_of(&values, parts[0].0, "Position"),
        Vec3::new(-2.0, 2.0, 0.0),
    );
}

#[test]
fn resizing_both_directions_keeps_the_center() {
    let parts = two_parts();
    let modes = Modes {
        directions: ResizeDirections::Both,
        ..Modes::default()
    };
    let frame = parts[1].1;
    let mut drag = Drag::begin(
        Handle::Scale(Axis::Y, Side::Plus),
        frame,
        parts.clone(),
        modes,
        ray_through(2.0, 1.0),
        None,
    )
    .unwrap();
    let values = drag
        .update_all(ray_through(2.0, 2.0).into(), &NO_SNAP)
        .unwrap();
    for (id, start) in &parts {
        assert_near(value_of(&values, *id, "Size"), Vec3::new(1.0, 3.0, 1.0));
        assert_near(value_of(&values, *id, "Position"), start.position);
    }
}

#[test]
fn center_pivot_turns_the_selection_around_its_middle_and_local_turns_each_in_place() {
    let parts = two_parts();
    let poses: Vec<Pose> = parts.iter().map(|(_, pose)| *pose).collect();
    let frame = handle_frame(Tool::Rotate, Modes::default(), &poses).unwrap();
    let turn = |modes: Modes, frame: Pose| {
        let screen = screen_along_z(frame.position, 0.0);
        let grab = ring_point(&screen, frame, Axis::Z, 0.0);
        let mut drag = Drag::start_many(
            Handle::Rotate(Axis::Z),
            frame,
            parts.clone(),
            modes,
            &screen,
            grab,
        )
        .unwrap();
        let at = grab + drag.spin * points_for(90.0);
        drag.update_all(Pointer::on(&screen, at, false), &NO_SNAP)
            .unwrap()
    };
    // A quarter turn around the middle: the parts swing round to ±Y.
    let values = turn(Modes::default(), frame);
    assert_near(
        value_of(&values, parts[0].0, "Position"),
        Vec3::new(0.0, -2.0, 0.0),
    );
    assert_near(
        value_of(&values, parts[1].0, "Position"),
        Vec3::new(0.0, 2.0, 0.0),
    );
    assert_near(
        value_of(&values, parts[0].0, "Orientation"),
        Vec3::new(0.0, 0.0, 90.0),
    );

    // Local: each turns where it is.
    let local = Modes {
        pivot: RotatePivot::Local,
        ..Modes::default()
    };
    let values = turn(local, parts[1].1);
    let (_, first) = values.iter().find(|(id, _)| *id == parts[0].0).unwrap();
    assert!(first.iter().all(|(name, _)| *name != "Position"));
    assert_near(
        value_of(&values, parts[0].0, "Orientation"),
        Vec3::new(0.0, 0.0, 90.0),
    );
}

#[test]
fn transform_shows_every_kind_of_handle_and_each_drags_from_its_own_frame() {
    let start = pose(Vec3::ZERO, Vec3::new(4.0, 1.0, 2.0));
    let screen = screen_along_z(Vec3::ZERO, 3.0);
    let shapes = shapes_for(Tool::Transform, Modes::default(), &[start], &screen);
    let count = |kind: fn(&Handle) -> bool| shapes.iter().filter(|s| kind(&s.handle)).count();
    assert_eq!(count(|h| matches!(h, Handle::Rotate(_))), 3);
    assert!(
        count(|h| matches!(h, Handle::Scale(..))) >= 4,
        "faces seen side on are hidden"
    );
    assert!(count(|h| matches!(h, Handle::Move(_))) >= 2);
    // Its arrows start out at the rings, unlike Move's from the middle.
    let arrow = |tool| {
        shapes_for(tool, Modes::default(), &[start], &screen)
            .into_iter()
            .find(|s| s.handle == Handle::Move(Axis::X))
            .unwrap()
    };
    let middle = screen.point(Vec3::ZERO).unwrap();
    let from = |shape: &Shape| shape.lines()[0][0].distance(middle);
    assert!(from(&arrow(Tool::Move)) < 1.0);
    assert!((from(&arrow(Tool::Transform)) - HANDLE_POINTS).abs() < 5.0);
    assert_eq!(
        frame_of(&shapes, Handle::Rotate(Axis::Z)),
        Some(Pose {
            rotation: Quat::IDENTITY,
            ..start
        })
    );
}

/// A visible, solid 2×2×2 part at `x` on the X axis, in the way.
fn block_at(x: f32) -> snapping::Obstacle {
    snapping::wall(snapping::obb(
        Vec3::new(x, 0.0, 0.0),
        Vec3::splat(2.0),
        Quat::IDENTITY,
    ))
}

#[test]
fn with_surfaces_a_move_lands_flush_against_the_part_in_its_way() {
    let start = pose(Vec3::ZERO, Vec3::splat(2.0));
    let snap = Snap {
        surfaces: true,
        ..NO_SNAP
    };
    let mut drag = Drag::start(
        Handle::Move(Axis::X),
        some_part(),
        start,
        ray_through(0.0, 0.0),
    )
    .unwrap()
    .among(vec![block_at(6.0)], 0.5);
    // 3.7 studs is within half a stud of flush (4).
    let values = drag.update(ray_through(3.7, 0.0), &snap).unwrap();
    assert_near(vector(&values, "Position"), Vec3::new(4.0, 0.0, 0.0));
    // Off, it goes where the mouse says.
    let values = drag.update(ray_through(3.7, 0.0), &NO_SNAP).unwrap();
    assert_near(vector(&values, "Position"), Vec3::new(3.7, 0.0, 0.0));
}

#[test]
fn with_collisions_a_move_or_a_resize_stops_at_a_solid_part() {
    let start = pose(Vec3::ZERO, Vec3::splat(2.0));
    let snap = Snap {
        collisions: true,
        ..NO_SNAP
    };
    let mut drag = Drag::start(
        Handle::Move(Axis::X),
        some_part(),
        start,
        ray_through(0.0, 0.0),
    )
    .unwrap()
    .among(vec![block_at(6.0)], 0.5);
    let values = drag.update(ray_through(9.0, 0.0), &snap).unwrap();
    assert_near(vector(&values, "Position"), Vec3::new(4.0, 0.0, 0.0));

    // The +X face grows to the other part's face (5) and no further.
    let handle = Handle::Scale(Axis::X, Side::Plus);
    let mut drag = Drag::start(handle, some_part(), start, ray_through(1.5, 0.0))
        .unwrap()
        .among(vec![block_at(6.0)], 0.5);
    let values = drag.update(ray_through(10.0, 0.0), &snap).unwrap();
    let size = vector(&values, "Size");
    assert!((size.x - 6.0).abs() < 0.03, "{size}");
}

#[test]
fn with_collisions_a_turn_stops_where_it_meets_a_solid_part() {
    // A long bar along X, with a block just above its right end.
    let start = pose(Vec3::ZERO, Vec3::new(8.0, 0.5, 0.5));
    let above = snapping::wall(snapping::obb(
        Vec3::new(3.5, 1.5, 0.0),
        Vec3::splat(1.0),
        Quat::IDENTITY,
    ));
    let screen = screen_along_z(Vec3::ZERO, 0.0);
    let snap = Snap {
        collisions: true,
        ..NO_SNAP
    };
    let frame = Pose {
        rotation: Quat::IDENTITY,
        ..start
    };
    let grab = ring_point(&screen, frame, Axis::Z, 0.0);
    let mut drag = Drag::start_many(
        Handle::Rotate(Axis::Z),
        frame,
        vec![(some_part(), start)],
        Modes::default(),
        &screen,
        grab,
    )
    .unwrap()
    .among(vec![above], 0.5);
    let z = |values: PartValues| vector(&values[0].1, "Orientation").z;
    // A small turn the other way is free.
    let down = drag.update_all(Pointer::on(&screen, grab - drag.spin() * 5.0, false), &snap);
    assert!(z(down.unwrap()) < -3.0);
    // Back and well past (38°): the bar's top corner meets the block at
    // about 10.8°, so it stops just short of that instead of swinging
    // through it.
    let up = drag.update_all(
        Pointer::on(&screen, grab + drag.spin() * 60.0, false),
        &snap,
    );
    let stopped = z(up.unwrap());
    assert!((8.0..10.8).contains(&stopped), "{stopped}");
    // Pushing further doesn't get it past.
    let more = drag.update_all(
        Pointer::on(&screen, grab + drag.spin() * 120.0, false),
        &snap,
    );
    assert_eq!(z(more.unwrap()), stopped);
}

#[test]
fn with_snap_points_a_move_clicks_to_a_corner_near_it_and_reports_how_far_it_came() {
    let mut world = World::new();
    let part = world.create(axcenity_core::ClassName::Part).unwrap();
    let start = Pose {
        position: Vec3::ZERO,
        rotation: Quat::IDENTITY,
        size: Vec3::splat(2.0),
    };
    let other = Obb {
        center: Vec3::new(10.0, 0.0, 0.0),
        axes: [Vec3::X, Vec3::Y, Vec3::Z],
        half: Vec3::ONE,
    };
    let obstacles = vec![Obstacle {
        id: world.create(axcenity_core::ClassName::Part).unwrap(),
        obb: other,
        solid: true,
        visible: true,
    }];
    let ray = |x: f32| Ray {
        origin: Vec3::new(x, 0.0, 20.0),
        direction: Vec3::NEG_Z,
    };
    let mut drag = Drag::start(Handle::Move(Axis::X), part, start, ray(0.0))
        .unwrap()
        .among(obstacles, 0.5);
    let snap = Snap {
        points: true,
        ..NO_SNAP
    };

    // The part's right face is 1 stud out, so 7.8 leaves it 0.2 from the
    // other's left face middle at x = 9.
    let parts = drag.update_all(ray(7.8).into(), &snap).unwrap();
    let Value::Vector3(at) = &parts[0].1[0].1 else {
        panic!("a position");
    };
    assert!(
        (at.x - 8.0).abs() < 0.001,
        "it clicked to x = 8, was {}",
        at.x
    );
    let report = drag.report().unwrap();
    assert!(report.link.is_some());
    assert!((report.distance() - 8.0).abs() < 0.001);
    assert_eq!(report.start, Vec3::ZERO);
    assert!(!report.targets.is_empty());

    // Too far from any point to click, it goes where the mouse is.
    drag.update_all(ray(3.3).into(), &snap).unwrap();
    let report = drag.report().unwrap();
    assert!(report.link.is_none());
    assert!((report.distance() - 3.3).abs() < 0.001);
}

#[test]
fn with_snap_points_off_a_move_still_reports_its_distance_but_shows_no_markers() {
    let mut world = World::new();
    let part = world.create(axcenity_core::ClassName::Part).unwrap();
    let start = Pose {
        position: Vec3::ZERO,
        rotation: Quat::IDENTITY,
        size: Vec3::splat(2.0),
    };
    let ray = |x: f32| Ray {
        origin: Vec3::new(x, 0.0, 20.0),
        direction: Vec3::NEG_Z,
    };
    let mut drag = Drag::start(Handle::Move(Axis::X), part, start, ray(0.0)).unwrap();
    drag.update_all(ray(4.0).into(), &NO_SNAP).unwrap();
    let report = drag.report().unwrap();
    assert!((report.distance() - 4.0).abs() < 0.001);
    assert!(report.targets.is_empty());
}
