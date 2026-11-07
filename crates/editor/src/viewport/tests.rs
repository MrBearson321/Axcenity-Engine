use super::*;

#[test]
fn wasd_e_and_q_fly_the_camera_and_other_keys_do_nothing() {
    let mut keys = MoveKeys::default();
    set_move_key(&mut keys, egui::Key::W, true);
    set_move_key(&mut keys, egui::Key::E, true);
    set_move_key(&mut keys, egui::Key::Z, true);
    assert!(keys.forward && keys.up);
    assert!(!(keys.back || keys.left || keys.right || keys.down));
    set_move_key(&mut keys, egui::Key::W, false);
    assert!(!keys.forward);
}

#[test]
fn speed_popup_stays_for_a_second_then_fades_out() {
    assert_eq!(popup_opacity(0.0), 1.0);
    assert_eq!(popup_opacity(POPUP_SECONDS), 1.0);
    let halfway = popup_opacity(POPUP_SECONDS + POPUP_FADE_SECONDS / 2.0);
    assert!((halfway - 0.5).abs() < 1e-6);
    // (Not exactly 0 at the end: 1.4 - 1.0 isn't exactly 0.4 in floating point.)
    assert!(popup_opacity(POPUP_SECONDS + POPUP_FADE_SECONDS) < 1e-6);
    assert_eq!(popup_opacity(100.0), 0.0);
}

#[test]
fn speed_label_shows_a_decimal_only_when_slow() {
    assert_eq!(speed_label(20.0), "Fly speed: 20 studs/s");
    assert_eq!(speed_label(57.6), "Fly speed: 58 studs/s");
    assert_eq!(speed_label(2.4), "Fly speed: 2.4 studs/s");
}

#[test]
fn clicking_the_middle_of_the_view_picks_the_part_the_camera_faces() {
    let mut world = World::new();
    let part = world.create(axcenity_core::ClassName::Part).unwrap();
    world.set_parent(part, Some(world.workspace())).unwrap();
    // The start camera looks at the origin, where a new part sits.
    let viewport = Viewport::new();
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0));
    let screen = viewport.screen(viewport.camera.to_render_camera(), rect);
    assert_eq!(
        axcenity_scene::pick(&world, screen.ray(rect.center())),
        Some(part)
    );
    let corner = screen.ray(rect.left_top());
    assert_eq!(axcenity_scene::pick(&world, corner), None);
}

#[test]
fn opening_another_project_keeps_the_flying_speed() {
    let mut viewport = Viewport::new();
    viewport.camera.change_speed(3.0);
    let speed = viewport.camera.speed();
    viewport.camera.position = Vec3::splat(100.0);
    viewport.reset_camera();
    assert_eq!(viewport.camera.position, START_CAMERA_POSITION);
    assert_eq!(viewport.camera.speed(), speed);
}

#[test]
fn a_light_dragged_by_its_icon_follows_the_mouse_in_the_plane_it_started_in() {
    let mut world = World::new();
    let id = world.create(ClassName::PointLight).unwrap();
    let drag = IconDrag {
        id,
        start: Vec3::new(0.0, 2.0, -10.0),
        grab_offset: Vec3::ZERO,
        normal: Vec3::NEG_Z,
    };
    let ray = |toward: Vec3| axcenity_scene::Ray {
        origin: Vec3::ZERO,
        direction: toward.normalize(),
    };
    let snap = Snap {
        move_on: false,
        ..Snap::default()
    };
    let at = drag.update(ray(Vec3::new(3.0, 1.0, -10.0)), &snap).unwrap();
    assert!(at.abs_diff_eq(Vec3::new(3.0, 1.0, -10.0), 1e-4), "{at}");
    // On the move grid, each axis moves in whole steps.
    let snap = Snap {
        move_on: true,
        move_step: 1.0,
        ..Snap::default()
    };
    let at = drag.update(ray(Vec3::new(3.4, 1.0, -10.0)), &snap).unwrap();
    assert!(at.abs_diff_eq(Vec3::new(3.0, 1.0, -10.0), 1e-4), "{at}");
    // Looking away from the plane, it stays where it is.
    assert!(drag.update(ray(Vec3::Z), &snap).is_none());
}

/// A wide floor with its top at y = 0, and a Spline on it from (-6, 0, 0)
/// to (6, 0, 0). Returns the world, the Spline and its points.
fn spline_on_a_floor() -> (World, InstanceId, Vec<InstanceId>) {
    let mut world = World::new();
    let floor = world.create(ClassName::Part).unwrap();
    world.set_parent(floor, Some(world.workspace())).unwrap();
    for (name, value) in [
        (
            "Size",
            Value::Vector3(axcenity_core::Vector3::new(200.0, 1.0, 200.0)),
        ),
        (
            "Position",
            Value::Vector3(axcenity_core::Vector3::new(0.0, -0.5, 0.0)),
        ),
    ] {
        world.set_property(floor, name, value).unwrap();
    }
    let spline = world.create(ClassName::Spline).unwrap();
    world.set_parent(spline, Some(world.workspace())).unwrap();
    let points = [-6.0, 6.0]
        .map(|x| {
            let point = world.create(ClassName::SplinePoint).unwrap();
            world.set_parent(point, Some(spline)).unwrap();
            world
                .set_property(
                    point,
                    "Position",
                    Value::Vector3(axcenity_core::Vector3::new(x, 0.0, 0.0)),
                )
                .unwrap();
            point
        })
        .to_vec();
    (world, spline, points)
}

/// One frame of the viewport's spline input over a 1280×720 view, with
/// `events` and `modifiers`.
fn spline_frame(
    ctx: &egui::Context,
    viewport: &mut Viewport,
    world: &World,
    editing: Option<InstanceId>,
    events: Vec<egui::Event>,
    modifiers: egui::Modifiers,
) -> (Option<SplineAction>, bool, bool) {
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0));
    let input = egui::RawInput {
        screen_rect: Some(rect),
        events,
        modifiers,
        ..Default::default()
    };
    let mut result = (None, false, false);
    let _ = ctx.run(input, |ctx| {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
                let screen = viewport.screen(viewport.camera.to_render_camera(), rect);
                result = viewport.spline_input(ui, &response, &screen, world, editing);
            });
    });
    result
}

#[test]
fn a_spline_point_drags_onto_the_surface_under_the_mouse_and_alt_drag_adds_one() {
    let (world, spline, points) = spline_on_a_floor();
    let ctx = egui::Context::default();
    let mut viewport = Viewport::new();
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0));
    let screen = viewport.screen(viewport.camera.to_render_camera(), rect);
    let on_screen = |x: f32, z: f32| screen.point(Vec3::new(x, 0.0, z)).unwrap();
    let none = egui::Modifiers::NONE;
    let press = |pos, pressed, modifiers| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers,
    };

    // A click on the curve between the points edits the Spline.
    let middle = on_screen(0.0, 0.0);
    spline_frame(
        &ctx,
        &mut viewport,
        &world,
        None,
        vec![egui::Event::PointerMoved(middle)],
        none,
    );
    let (action, taken, _) = spline_frame(
        &ctx,
        &mut viewport,
        &world,
        None,
        vec![press(middle, true, none)],
        none,
    );
    assert_eq!(action, Some(SplineAction::Edit(spline)));
    assert!(taken);
    spline_frame(
        &ctx,
        &mut viewport,
        &world,
        Some(spline),
        vec![press(middle, false, none)],
        none,
    );

    // Dragging the second point moves it onto the floor under the mouse.
    let grab = on_screen(6.0, 0.0);
    spline_frame(
        &ctx,
        &mut viewport,
        &world,
        Some(spline),
        vec![egui::Event::PointerMoved(grab)],
        none,
    );
    let (action, _, _) = spline_frame(
        &ctx,
        &mut viewport,
        &world,
        Some(spline),
        vec![press(grab, true, none)],
        none,
    );
    assert_eq!(action, Some(SplineAction::Pick(points[1])));
    let to = on_screen(6.0, 5.0);
    let (action, taken, _) = spline_frame(
        &ctx,
        &mut viewport,
        &world,
        Some(spline),
        vec![egui::Event::PointerMoved(to)],
        none,
    );
    match action {
        Some(SplineAction::Move { point, to }) => {
            assert_eq!(point, points[1]);
            assert!(to.abs_diff_eq(Vec3::new(6.0, 0.0, 5.0), 0.05), "{to}");
        }
        other => panic!("not moved: {other:?}"),
    }
    assert!(taken);
    let (_, _, released) = spline_frame(
        &ctx,
        &mut viewport,
        &world,
        Some(spline),
        vec![press(to, false, none)],
        none,
    );
    assert!(released, "letting go ends the drag's undo step");

    // Alt-dragging the first point adds one after it where it's let go.
    let alt = egui::Modifiers::ALT;
    let first = on_screen(-6.0, 0.0);
    spline_frame(
        &ctx,
        &mut viewport,
        &world,
        Some(spline),
        vec![egui::Event::PointerMoved(first)],
        alt,
    );
    spline_frame(
        &ctx,
        &mut viewport,
        &world,
        Some(spline),
        vec![press(first, true, alt)],
        alt,
    );
    let to = on_screen(-3.0, -4.0);
    let (action, _, _) = spline_frame(
        &ctx,
        &mut viewport,
        &world,
        Some(spline),
        vec![egui::Event::PointerMoved(to)],
        alt,
    );
    assert_eq!(action, None, "nothing moves until it's let go");
    let (action, _, released) = spline_frame(
        &ctx,
        &mut viewport,
        &world,
        Some(spline),
        vec![press(to, false, alt)],
        alt,
    );
    match action {
        Some(SplineAction::Add { after, at }) => {
            assert_eq!(after, points[0]);
            assert!(at.abs_diff_eq(Vec3::new(-3.0, 0.0, -4.0), 0.05), "{at}");
        }
        other => panic!("nothing added: {other:?}"),
    }
    assert!(released);
}
