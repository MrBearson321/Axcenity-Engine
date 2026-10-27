use axcenity_anim::{EasingDirection, EasingStyle, ease};
use axcenity_core::{ClassName, Color3, InstanceId, UDim2, Value, Vector3, World};

use super::*;
use crate::tween::interpolate;

/// A world with a Part named `Block` at the origin and a Frame named `Menu`
/// in a ScreenGui named `Hud`, all in the Workspace.
fn world_with_block_and_menu() -> (World, InstanceId, InstanceId) {
    let mut world = World::new();
    let block = world.create(ClassName::Part).unwrap();
    world.set_name(block, "Block").unwrap();
    world.set_parent(block, Some(world.workspace())).unwrap();
    let hud = world.create(ClassName::ScreenGui).unwrap();
    world.set_name(hud, "Hud").unwrap();
    world.set_parent(hud, Some(world.workspace())).unwrap();
    let menu = world.create(ClassName::Frame).unwrap();
    world.set_name(menu, "Menu").unwrap();
    world.set_parent(menu, Some(hud)).unwrap();
    (world, block, menu)
}

fn start(runtime: &mut ScriptRuntime, world: &mut World, source: &str) {
    let script = world.create(ClassName::Script).unwrap();
    world.set_name(script, "Tweens").unwrap();
    world
        .set_parent(script, Some(world.server_script_service()))
        .unwrap();
    runtime.start_script(world, script, "scripts/Tweens.server.lua", source);
}

fn output(runtime: &mut ScriptRuntime) -> Vec<String> {
    runtime
        .take_output()
        .into_iter()
        .map(|line| match line.kind {
            OutputKind::Print => line.text,
            OutputKind::Error => format!("error: {}", line.text),
            OutputKind::Warning => format!("warning: {}", line.text),
        })
        .collect()
}

/// Steps the game `frames` times, `seconds` each.
fn run_for(runtime: &mut ScriptRuntime, world: &mut World, frames: usize, seconds: f64) {
    for _ in 0..frames {
        runtime.step(world, seconds);
    }
}

fn x_of(world: &World, part: InstanceId) -> f32 {
    match world.get(part).unwrap().property("Position") {
        Some(Value::Vector3(position)) => position.x,
        other => panic!("not a position: {other:?}"),
    }
}

fn number_of(world: &World, id: InstanceId, name: &str) -> f64 {
    world
        .get(id)
        .unwrap()
        .property(name)
        .and_then(Value::as_number)
        .unwrap()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-4
}

const PART_AND_FRAME: &str = r#"
    local TweenService = game:GetService("TweenService")
    local info = TweenInfo.new(1, Enum.EasingStyle.Quad, Enum.EasingDirection.Out)
    local move = TweenService:Create(workspace.Block, info, {Position = Vector3.new(10, 0, 0)})
    local fade = TweenService:Create(workspace.Hud.Menu, info, {BackgroundTransparency = 1})
    move.Completed:Connect(function(state)
        print("moved", state, move.PlaybackState)
    end)
    fade.Completed:Connect(function(state)
        print("faded", state)
    end)
    move:Play()
    fade:Play()
    print(move.PlaybackState)
"#;

#[test]
fn a_part_and_a_frame_tween_over_one_second_with_quad_out_and_completed_fires_once() {
    let (mut world, block, menu) = world_with_block_and_menu();
    let mut runtime = ScriptRuntime::new();
    start(&mut runtime, &mut world, PART_AND_FRAME);
    assert_eq!(output(&mut runtime), ["Enum.PlaybackState.Delayed"]);

    // Halfway: Quad Out is three quarters of the way there.
    run_for(&mut runtime, &mut world, 2, 0.25);
    let expected = f64::from(ease(EasingStyle::Quad, EasingDirection::Out, 0.5));
    assert!(close(expected, 0.75));
    assert!(close(f64::from(x_of(&world, block)), 10.0 * expected));
    assert!(close(
        number_of(&world, menu, "BackgroundTransparency"),
        expected
    ));
    assert!(output(&mut runtime).is_empty());

    run_for(&mut runtime, &mut world, 2, 0.25);
    assert_eq!(x_of(&world, block), 10.0);
    assert_eq!(number_of(&world, menu, "BackgroundTransparency"), 1.0);
    assert_eq!(
        output(&mut runtime),
        [
            "moved\tEnum.PlaybackState.Completed\tEnum.PlaybackState.Completed",
            "faded\tEnum.PlaybackState.Completed",
        ]
    );

    // Completed fired once: later frames change nothing and print nothing.
    run_for(&mut runtime, &mut world, 10, 0.25);
    assert!(output(&mut runtime).is_empty());
    assert_eq!(x_of(&world, block), 10.0);
}

#[test]
fn a_tween_at_sixty_frames_a_second_ends_on_its_goal() {
    let (mut world, block, _) = world_with_block_and_menu();
    let mut runtime = ScriptRuntime::new();
    start(&mut runtime, &mut world, PART_AND_FRAME);
    output(&mut runtime);
    let mut frames = 0;
    while output(&mut runtime).is_empty() {
        runtime.step(&mut world, 1.0 / 60.0);
        frames += 1;
        assert!(frames <= 61, "still playing after {frames} frames");
    }
    assert!(frames >= 60);
    assert_eq!(x_of(&world, block), 10.0);
}

#[test]
fn cancel_stops_a_tween_where_it_is() {
    let (mut world, block, _) = world_with_block_and_menu();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        r#"
        local TweenService = game:GetService("TweenService")
        local info = TweenInfo.new(1, Enum.EasingStyle.Linear)
        tween = TweenService:Create(workspace.Block, info, {Position = Vector3.new(10, 0, 0)})
        tween.Completed:Connect(function(state) print("ended", state) end)
        tween:Play()
        task.wait(0.5)
        tween:Cancel()
        print(tween.PlaybackState)
        "#,
    );
    run_for(&mut runtime, &mut world, 2, 0.25);
    assert_eq!(output(&mut runtime), ["Enum.PlaybackState.Cancelled"]);
    assert_eq!(x_of(&world, block), 5.0);

    run_for(&mut runtime, &mut world, 4, 0.25);
    assert_eq!(
        output(&mut runtime),
        ["ended\tEnum.PlaybackState.Cancelled"]
    );
    assert_eq!(x_of(&world, block), 5.0);
}

#[test]
fn a_second_tween_on_the_same_property_replaces_the_first() {
    let (mut world, block, menu) = world_with_block_and_menu();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        r#"
        local TweenService = game:GetService("TweenService")
        local info = TweenInfo.new(1, Enum.EasingStyle.Linear)
        local right = TweenService:Create(workspace.Block, info, {Position = Vector3.new(10, 0, 0)})
        local fade = TweenService:Create(workspace.Hud.Menu, info, {BackgroundTransparency = 1})
        right.Completed:Connect(function(state) print("right", state) end)
        right:Play()
        fade:Play()
        task.wait(0.5)
        local left = TweenService:Create(workspace.Block, info, {Position = Vector3.new(-10, 0, 0)})
        left.Completed:Connect(function(state) print("left", state) end)
        left:Play()
        print(right.PlaybackState, fade.PlaybackState)
        "#,
    );
    run_for(&mut runtime, &mut world, 2, 0.25);
    // A tween on another Instance's property carries on.
    assert_eq!(
        output(&mut runtime),
        ["Enum.PlaybackState.Cancelled\tEnum.PlaybackState.Playing"]
    );
    assert_eq!(x_of(&world, block), 5.0);

    runtime.step(&mut world, 0.5);
    assert_eq!(
        output(&mut runtime),
        ["right\tEnum.PlaybackState.Cancelled"]
    );
    // Halfway from where the first left it (5) to -10.
    assert_eq!(x_of(&world, block), -2.5);
    assert_eq!(number_of(&world, menu, "BackgroundTransparency"), 1.0);

    runtime.step(&mut world, 0.5);
    assert_eq!(output(&mut runtime), ["left\tEnum.PlaybackState.Completed"]);
    assert_eq!(x_of(&world, block), -10.0);
}

#[test]
fn tweens_repeat_reverse_wait_and_pause() {
    let (mut world, block, _) = world_with_block_and_menu();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        r#"
        local TweenService = game:GetService("TweenService")
        -- Out and back, twice, after half a second.
        local info = TweenInfo.new(1, Enum.EasingStyle.Linear, Enum.EasingDirection.In, 1, true, 0.5)
        print(info.Time, info.EasingStyle, info.EasingDirection, info.RepeatCount, info.Reverses, info.DelayTime)
        tween = TweenService:Create(workspace.Block, info, {Position = Vector3.new(4, 0, 0)})
        tween.Completed:Connect(function(state) print(state) end)
        tween:Play()
        "#,
    );
    assert_eq!(
        output(&mut runtime),
        ["1\tEnum.EasingStyle.Linear\tEnum.EasingDirection.In\t1\ttrue\t0.5"]
    );
    let mut xs = Vec::new();
    for _ in 0..11 {
        runtime.step(&mut world, 0.5);
        xs.push(x_of(&world, block));
    }
    assert_eq!(xs, [0.0, 2.0, 4.0, 2.0, 0.0, 2.0, 4.0, 2.0, 0.0, 0.0, 0.0]);
    assert_eq!(output(&mut runtime), ["Enum.PlaybackState.Completed"]);

    // Pause holds it; Play carries on from there.
    runtime
        .run_command(
            &mut world,
            r#"
            workspace.Block.Position = Vector3.new()
            local t = game:GetService("TweenService"):Create(workspace.Block, TweenInfo.new(1, Enum.EasingStyle.Linear), {Position = Vector3.new(4, 0, 0)})
            t:Play()
            task.wait(0.5)
            t:Pause()
            print(t.PlaybackState)
            task.wait(1)
            t:Play()
            "#,
        )
        .unwrap();
    run_for(&mut runtime, &mut world, 2, 0.5);
    assert_eq!(output(&mut runtime), ["Enum.PlaybackState.Paused"]);
    assert_eq!(x_of(&world, block), 2.0);
    // The wait ends after this frame's tweens moved, so it moves next frame.
    runtime.step(&mut world, 0.5);
    assert_eq!(x_of(&world, block), 2.0);
    runtime.step(&mut world, 0.5);
    assert_eq!(x_of(&world, block), 4.0);
}

#[test]
fn colours_and_ui_positions_tween_and_strings_do_not() {
    let (mut world, block, menu) = world_with_block_and_menu();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        r#"
        local TweenService = game:GetService("TweenService")
        local info = TweenInfo.new(1, Enum.EasingStyle.Linear)
        workspace.Block.Color = Color3.new(0, 0, 0)
        TweenService:Create(workspace.Block, info, {Color = Color3.new(1, 0.5, 0)}):Play()
        TweenService:Create(workspace.Hud.Menu, info, {Position = UDim2.new(0.5, 100, 1, -20)}):Play()
        print(pcall(function() TweenService:Create(workspace.Block, info, {Name = "x"}) end))
        print(pcall(function() TweenService:Create(workspace.Hud.Menu, info, {Image = "x"}) end))
        print(pcall(function() TweenService:Create(workspace.Hud.Menu, info, {Position = 3}) end))
        print(pcall(function() TweenInfo.new(1, Enum.Material.Neon) end))
        print(typeof(TweenService), typeof(info), typeof(TweenService:Create(workspace.Block, info, {})))
        "#,
    );
    let printed = output(&mut runtime);
    assert_eq!(printed.len(), 5, "{printed:?}");
    assert!(
        printed[0].contains("Name is not a property of Part"),
        "{printed:?}"
    );
    assert!(
        printed[1].contains("Image is not a property of Frame"),
        "{printed:?}"
    );
    assert!(
        printed[2].contains("Frame.Position must be a UDim2, not a number"),
        "{printed:?}"
    );
    assert!(
        printed[3]
            .contains("Enum.EasingStyle item here, such as Enum.EasingStyle.Sine, not Material"),
        "{printed:?}"
    );
    assert_eq!(printed[4], "TweenService\tTweenInfo\tTween");

    run_for(&mut runtime, &mut world, 2, 0.25);
    assert_eq!(
        world.get(block).unwrap().property("Color"),
        Some(&Value::Color3(Color3::new(0.5, 0.25, 0.0)))
    );
    assert_eq!(
        world.get(menu).unwrap().property("Position"),
        Some(&Value::UDim2(UDim2::new(0.25, 50.0, 0.5, -10.0)))
    );
}

#[test]
fn a_tween_on_a_destroyed_instance_just_stops() {
    let (mut world, block, _) = world_with_block_and_menu();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        r#"
        local tween = game:GetService("TweenService"):Create(workspace.Block, TweenInfo.new(1), {Position = Vector3.new(4, 0, 0)})
        tween:Play()
        task.wait(1.5)
        print(tween.PlaybackState)
        "#,
    );
    runtime.step(&mut world, 0.25);
    world.destroy(block).unwrap();
    run_for(&mut runtime, &mut world, 8, 0.25);
    assert_eq!(output(&mut runtime), ["Enum.PlaybackState.Cancelled"]);
}

#[test]
fn whole_number_properties_step_and_easings_overshoot_but_colours_do_not() {
    let half = |name, from: f64, to: f64, alpha| {
        interpolate(name, &Value::Number(from), &Value::Number(to), alpha)
            .and_then(|value| value.as_number())
            .unwrap()
    };
    assert_eq!(half("ZIndex", 1.0, 4.0, 0.4), 2.0);
    assert!((half("BackgroundTransparency", 1.0, 4.0, 0.4) - 2.2).abs() < 1e-6);
    // Back goes past the goal; a colour stops at its end.
    let past = interpolate(
        "Color",
        &Value::Color3(Color3::new(0.0, 0.0, 0.0)),
        &Value::Color3(Color3::new(1.0, 1.0, 1.0)),
        1.2,
    );
    assert_eq!(past, Some(Value::Color3(Color3::new(1.0, 1.0, 1.0))));
    let past = interpolate(
        "Position",
        &Value::Vector3(Vector3::new(0.0, 0.0, 0.0)),
        &Value::Vector3(Vector3::new(10.0, 0.0, 0.0)),
        1.2,
    );
    assert_eq!(past, Some(Value::Vector3(Vector3::new(12.0, 0.0, 0.0))));
    // A switch flips at the end.
    let flip = |alpha| interpolate("Visible", &Value::Bool(true), &Value::Bool(false), alpha);
    assert_eq!(flip(0.99), Some(Value::Bool(true)));
    assert_eq!(flip(1.0), Some(Value::Bool(false)));
}

#[test]
fn the_api_reference_lists_tween_service() {
    let reference = api_reference();
    let has = |owner: &str, name: &str| {
        reference
            .iter()
            .any(|entry| entry.owner == owner && entry.name == name)
    };
    assert!(has("", "TweenService"));
    assert!(has("TweenService", "Create"));
    for name in ["Play", "Pause", "Cancel", "PlaybackState", "Completed"] {
        assert!(has("Tween", name), "Tween.{name}");
    }
    assert!(has("TweenInfo", "new"));
    assert!(has("Enum.EasingStyle", "Linear"));
    assert!(has("Enum.PlaybackState", "Cancelled"));
    assert!(lua_definitions().contains("declare TweenInfo: TweenInfoLibrary"));
}

#[test]
fn udim2_values_add_and_subtract() {
    let (mut world, _, _) = world_with_block_and_menu();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "print(UDim2.new(0.5, 10, 1, -4) - UDim2.fromScale(1, 0), UDim2.fromOffset(1, 2) + UDim2.fromScale(0, 1))",
    );
    assert_eq!(output(&mut runtime), ["-0.5, 10, 1, -4\t0, 1, 1, 2"]);
}
