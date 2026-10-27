//! `workspace.CurrentCamera` (unified Phase 31): camera types, the field
//! of view, following a subject, shakes, screen points, and tweening it.

use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};

use super::*;

const FRAME: f64 = 1.0 / 60.0;

fn start(runtime: &mut ScriptRuntime, world: &mut World, source: &str) {
    let script = world.create(ClassName::LocalScript).unwrap();
    world
        .set_parent(script, Some(world.starter_player_scripts()))
        .unwrap();
    runtime.start_script(world, script, "scripts/camera.client.lua", source);
}

fn lines(runtime: &mut ScriptRuntime) -> Vec<String> {
    runtime
        .take_output()
        .into_iter()
        .map(|line| match line.kind {
            OutputKind::Error => format!("error: {}", line.text),
            _ => line.text,
        })
        .collect()
}

fn add(world: &mut World, class: ClassName, name: &str, position: Vector3) -> InstanceId {
    let id = world.create(class).unwrap();
    world.set_name(id, name).unwrap();
    world
        .set_property(id, "Position", Value::Vector3(position))
        .unwrap();
    world.set_parent(id, Some(world.workspace())).unwrap();
    id
}

fn near(a: Vector3, b: Vector3) -> bool {
    (a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3 && (a.z - b.z).abs() < 1e-3
}

const HOST: CameraState = CameraState {
    position: Vector3::new(1.0, 2.0, 3.0),
    orientation: Vector3::new(0.0, 90.0, 0.0),
};

#[test]
fn a_scriptable_camera_ignores_the_host_until_it_is_custom_again() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    runtime.set_camera(HOST);
    start(
        &mut runtime,
        &mut world,
        r#"
        local camera = workspace.CurrentCamera
        print(camera.CameraType, camera.FieldOfView)
        camera.CameraType = Enum.CameraType.Scriptable
        camera.CFrame = CFrame.new(0, 10, 0)
        "#,
    );
    assert_eq!(lines(&mut runtime), ["Enum.CameraType.Custom\t50"]);
    // The host's camera moves on, and the script's stays.
    runtime.set_camera(CameraState::default());
    assert_eq!(
        runtime.take_camera_move(),
        None,
        "only a Custom camera's moves are taken"
    );
    let game = runtime.step_camera(&world, FRAME);
    let scripted = game.scripted.expect("Scriptable holds the view");
    assert_eq!(scripted.position, Vector3::new(0.0, 10.0, 0.0));
    assert_eq!(game.view(HOST).position, Vector3::new(0.0, 10.0, 0.0));
    assert!(runtime.camera_scripted());

    // Back to Custom: scripts see the host's camera at once.
    runtime.set_camera(HOST);
    start(
        &mut runtime,
        &mut world,
        r#"
        local camera = workspace.CurrentCamera
        camera.CameraType = Enum.CameraType.Custom
        print(camera.Position)
        "#,
    );
    assert_eq!(lines(&mut runtime), ["1, 2, 3"]);
    assert_eq!(runtime.step_camera(&world, FRAME).scripted, None);
    assert!(!runtime.camera_scripted());
}

#[test]
fn a_fixed_camera_stays_and_cutting_to_a_viewpoint_copies_its_cframe() {
    let mut world = World::new();
    let overlook = add(
        &mut world,
        ClassName::Camera,
        "Overlook",
        Vector3::new(0.0, 30.0, 10.0),
    );
    world
        .set_property(
            overlook,
            "Orientation",
            Value::Vector3(Vector3::new(-50.0, 0.0, 0.0)),
        )
        .unwrap();
    let mut runtime = ScriptRuntime::new();
    runtime.set_camera(HOST);
    start(
        &mut runtime,
        &mut world,
        r#"
        local camera = workspace.CurrentCamera
        camera.CameraType = Enum.CameraType.Fixed
        camera.CFrame = workspace.Overlook.CFrame
        print(camera.Orientation, workspace.Overlook.FieldOfView)
        "#,
    );
    assert_eq!(lines(&mut runtime), ["-50, 0, 0\t50"]);
    for _ in 0..3 {
        runtime.set_camera(HOST);
        let game = runtime.step_camera(&world, FRAME);
        assert_eq!(
            game.scripted,
            Some(CameraState {
                position: Vector3::new(0.0, 30.0, 10.0),
                orientation: Vector3::new(-50.0, 0.0, 0.0),
            })
        );
    }
}

#[test]
fn track_follows_a_subject_without_turning_and_follow_turns_with_it() {
    let mut world = World::new();
    let target = add(
        &mut world,
        ClassName::Part,
        "Target",
        Vector3::new(0.0, 0.0, 0.0),
    );
    let mut runtime = ScriptRuntime::new();
    runtime.set_camera(HOST);
    start(
        &mut runtime,
        &mut world,
        r#"
        local camera = workspace.CurrentCamera
        camera.CameraSubject = workspace.Target
        camera.CameraType = Enum.CameraType.Track
        camera.CFrame = CFrame.lookAt(Vector3.new(0, 5, 10), Vector3.new(0, 0, 0))
        "#,
    );
    let start_view = runtime.step_camera(&world, FRAME).scripted.unwrap();
    world
        .set_property(
            target,
            "Position",
            Value::Vector3(Vector3::new(4.0, 0.0, -2.0)),
        )
        .unwrap();
    let tracked = runtime.step_camera(&world, FRAME).scripted.unwrap();
    assert!(
        near(tracked.position, Vector3::new(4.0, 5.0, 8.0)),
        "{tracked:?}"
    );
    assert!(near(tracked.orientation, start_view.orientation));

    // Follow: a quarter turn of the subject swings the camera round it.
    world
        .set_property(
            target,
            "Position",
            Value::Vector3(Vector3::new(0.0, 0.0, 0.0)),
        )
        .unwrap();
    start(
        &mut runtime,
        &mut world,
        r#"
        local camera = workspace.CurrentCamera
        camera.CameraType = Enum.CameraType.Follow
        camera.CFrame = CFrame.lookAt(Vector3.new(0, 5, 10), Vector3.new(0, 0, 0))
        print(camera.Focus.Position)
        "#,
    );
    assert_eq!(lines(&mut runtime), ["0, 0, 0"]);
    runtime.step_camera(&world, FRAME);
    world
        .set_property(
            target,
            "Orientation",
            Value::Vector3(Vector3::new(0.0, 90.0, 0.0)),
        )
        .unwrap();
    let followed = runtime.step_camera(&world, FRAME).scripted.unwrap();
    // Behind it (+Z) turns to its new behind (+X).
    assert!(
        near(followed.position, Vector3::new(10.0, 5.0, 0.0)),
        "{followed:?}"
    );
    assert!((followed.orientation.y - (start_view.orientation.y + 90.0)).abs() < 1e-2);
}

#[test]
fn field_of_view_and_cframe_tween_and_the_field_of_view_is_drawn() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    runtime.set_camera(CameraState::default());
    start(
        &mut runtime,
        &mut world,
        r#"
        local camera = workspace.CurrentCamera
        camera.CameraType = Enum.CameraType.Scriptable
        camera.CFrame = CFrame.new(0, 0, 0)
        local tween = game:GetService("TweenService"):Create(camera, TweenInfo.new(1, Enum.EasingStyle.Linear), {
            FieldOfView = 70,
            CFrame = CFrame.new(10, 0, 0),
        })
        print(tween.Instance == nil, typeof(tween.Instance))
        tween:Play()
        camera.FieldOfView = 500
        print(camera.FieldOfView)
        camera.FieldOfView = 50
        print(pcall(function()
            game:GetService("TweenService"):Create(camera, TweenInfo.new(), {LookVector = Vector3.new()})
        end))
        "#,
    );
    let output = lines(&mut runtime);
    assert_eq!(output[..2], ["false\tCamera", "120"]);
    assert!(
        output[2].contains("LookVector can't be tweened on the Camera"),
        "{}",
        output[2]
    );
    for _ in 0..30 {
        runtime.step(&mut world, FRAME);
    }
    let game = runtime.step_camera(&world, FRAME);
    assert!((game.field_of_view - 60.0).abs() < 1e-3, "{game:?}");
    assert!(near(
        game.scripted.unwrap().position,
        Vector3::new(5.0, 0.0, 0.0)
    ));
    assert!((game.view(HOST).render_camera().fov_y_degrees - 60.0).abs() < 1e-3);
}

#[test]
fn a_shake_moves_any_camera_type_and_fades_out_by_its_end() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    runtime.set_camera(HOST);
    start(
        &mut runtime,
        &mut world,
        "workspace.CurrentCamera:Shake(2, 0.5)",
    );
    let first = runtime.step_camera(&world, FRAME);
    assert_eq!(first.scripted, None, "a Custom camera stays the host's");
    let shaken = first.view(HOST);
    assert_ne!(shaken.position, HOST.position);
    assert!((shaken.position.x - HOST.position.x).abs() < 0.5 * 2.0);
    let mut last = first;
    for _ in 0..30 {
        last = runtime.step_camera(&world, FRAME);
    }
    assert_eq!(last.view(HOST).position, HOST.position);
    assert_eq!(last.view(HOST).orientation, HOST.orientation);
}

#[test]
fn screen_points_and_rays_meet_in_the_middle_and_points_behind_are_off_screen() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    runtime.set_viewport_size(800.0, 600.0);
    runtime.set_camera(CameraState {
        position: Vector3::new(0.0, 5.0, 10.0),
        orientation: Vector3::default(),
    });
    start(
        &mut runtime,
        &mut world,
        r#"
        local camera = workspace.CurrentCamera
        print(camera.ViewportSize)
        local ray = camera:ScreenPointToRay(400, 300)
        print(typeof(ray), ray.Origin, ray.Direction)
        local at, visible = camera:WorldToScreenPoint(Vector3.new(0, 5, 0))
        print(at, visible)
        local _, behind = camera:WorldToScreenPoint(Vector3.new(0, 5, 20))
        local _, aside = camera:WorldToViewportPoint(Vector3.new(100, 5, 0))
        print(behind, aside)
        print(camera:ScreenPointToRay(400, 300, 2).Origin)
        "#,
    );
    assert_eq!(
        lines(&mut runtime),
        [
            "800, 600",
            "Ray\t0, 5, 10\t0, 0, -1",
            "400, 300, 10\ttrue",
            "false\tfalse",
            "0, 5, 8",
        ]
    );
}

#[test]
fn the_mouse_is_only_on_the_players_side() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    // A LocalScript starting makes the player join.
    start(
        &mut runtime,
        &mut world,
        "print(typeof(game.Players.LocalPlayer:GetMouse()))",
    );
    let script = world.create(ClassName::Script).unwrap();
    world
        .set_parent(script, Some(world.server_script_service()))
        .unwrap();
    runtime.start_script(
        &mut world,
        script,
        "scripts/mouse.server.lua",
        "local player = game.Players:GetChildren()[1]\nprint(pcall(function() return player:GetMouse() end))",
    );
    let output = lines(&mut runtime);
    assert_eq!(output[0], "Mouse");
    assert!(
        output[1].starts_with("false")
            && output[1].contains("GetMouse only works in a LocalScript"),
        "{output:?}"
    );
}
