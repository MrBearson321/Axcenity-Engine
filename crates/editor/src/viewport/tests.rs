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
