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

/// A part at the origin in the Workspace, and one above it.
fn two_stacked_parts(world: &mut World) -> (InstanceId, InstanceId) {
    let make = |world: &mut World, y: f32| {
        let part = world.create(axcenity_core::ClassName::Part).unwrap();
        world.set_parent(part, Some(world.workspace())).unwrap();
        let at = axcenity_core::Vector3::new(0.0, y, 0.0);
        world
            .set_property(part, "Position", axcenity_core::Value::Vector3(at))
            .unwrap();
        part
    };
    (make(world, 0.0), make(world, 5.0))
}

#[test]
fn a_click_goes_through_a_locked_part_to_what_is_behind_it() {
    let mut world = World::new();
    let (below, above) = two_stacked_parts(&mut world);
    let down = axcenity_scene::Ray {
        origin: Vec3::new(0.0, 20.0, 0.0),
        direction: Vec3::NEG_Y,
    };
    let viewport = Viewport::new();
    assert_eq!(viewport.pick_unlocked(&world, down).unwrap().part, above);
    world
        .set_property(above, "Locked", axcenity_core::Value::Bool(true))
        .unwrap();
    assert_eq!(viewport.pick_unlocked(&world, down).unwrap().part, below);
    world
        .set_property(below, "Locked", axcenity_core::Value::Bool(true))
        .unwrap();
    assert!(viewport.pick_unlocked(&world, down).is_none());
}

/// R16.2: a hidden data layer isn't drawn and a click goes through it, as
/// through a locked one.
#[test]
fn a_click_goes_through_hidden_and_locked_layers() {
    let mut world = World::new();
    let (below, above) = two_stacked_parts(&mut world);
    let down = axcenity_scene::Ray {
        origin: Vec3::new(0.0, 20.0, 0.0),
        direction: Vec3::NEG_Y,
    };
    let mut viewport = Viewport::new();
    viewport.set_layer_sets([above].into(), [above].into());
    assert!(viewport.terrain.streaming.is_hidden(above));
    assert!(!viewport.terrain.streaming.is_hidden(below));
    assert_eq!(viewport.pick_unlocked(&world, down).unwrap().part, below);
    viewport.set_layer_sets(Default::default(), [above, below].into());
    assert!(!viewport.terrain.streaming.is_hidden(above));
    assert!(viewport.pick_unlocked(&world, down).is_none());
}

#[test]
fn the_tools_leave_locked_parts_and_what_is_in_a_locked_model_alone() {
    let mut world = World::new();
    let (free, locked) = two_stacked_parts(&mut world);
    world
        .set_property(locked, "Locked", axcenity_core::Value::Bool(true))
        .unwrap();
    let ids = |world: &World, selection: &[InstanceId]| -> Vec<InstanceId> {
        selected_parts(world, selection, None)
            .into_iter()
            .map(|(id, _)| id)
            .collect()
    };
    assert_eq!(ids(&world, &[free, locked]), [free]);

    let model = world.create(axcenity_core::ClassName::Model).unwrap();
    world.set_parent(model, Some(world.workspace())).unwrap();
    world.set_parent(free, Some(model)).unwrap();
    assert_eq!(ids(&world, &[model]), [free]);
    world
        .set_property(model, "Locked", axcenity_core::Value::Bool(true))
        .unwrap();
    assert!(ids(&world, &[model]).is_empty(), "inside a locked Model");
    assert!(world.is_locked(free));
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
        report: None,
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
        events: std::iter::once(egui::Event::ModifiersChanged(modifiers))
            .chain(events)
            .collect(),
        ..Default::default()
    };
    let mut result = (None, false, false);
    let _ = crate::theme::run(ctx, input, |ui| {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
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

#[test]
fn streaming_while_editing_hides_what_is_far_but_never_the_selection() {
    let mut world = World::new();
    let workspace = world.workspace();
    world
        .set_property(workspace, "StreamingEnabled", Value::Bool(true))
        .unwrap();
    world
        .set_property(workspace, "StreamingTargetRadius", Value::Number(200.0))
        .unwrap();
    let mut part_at = |x: f32| {
        let part = world.create(ClassName::Part).unwrap();
        world
            .set_property(
                part,
                "Position",
                Value::Vector3(axcenity_core::Vector3::new(x, 0.0, 0.0)),
            )
            .unwrap();
        world.set_parent(part, Some(workspace)).unwrap();
        part
    };
    let near = part_at(0.0);
    let far = part_at(900.0);
    let selected = part_at(1500.0);
    let mut streaming = axcenity_scene::Streaming::default();

    stream_editing(&mut streaming, &world, true, Vec3::ZERO, 1.0, &[selected]);
    assert!(!streaming.is_hidden(near));
    assert!(streaming.is_hidden(far), "out of range isn't drawn");
    assert!(!streaming.is_hidden(selected), "the selection always shows");

    // A click goes through what isn't drawn (R16.3b).
    let mut viewport = Viewport::new();
    stream_editing(
        &mut viewport.terrain.streaming,
        &world,
        true,
        Vec3::ZERO,
        1.0,
        &[],
    );
    let toward = |x: f32| axcenity_scene::Ray {
        origin: Vec3::new(x, 50.0, 0.0),
        direction: Vec3::NEG_Y,
    };
    assert_eq!(
        viewport.pick_unlocked(&world, toward(0.0)).unwrap().part,
        near
    );
    assert!(viewport.pick_unlocked(&world, toward(900.0)).is_none());

    // Off, editing shows everything again.
    stream_editing(&mut streaming, &world, false, Vec3::ZERO, 1.0, &[]);
    assert!(!streaming.is_hidden(far));
    assert!(streaming.settings().is_none());
}
