//! Tests for Phase 73: Splines from Lua, and SplineFollowers.

use axcenity_core::{ClassName, Value, World};

use super::*;

/// Runs `source` as a new server script with `runtime` and returns what it
/// printed, with errors marked.
fn run(runtime: &mut ScriptRuntime, world: &mut World, source: &str) -> Vec<String> {
    let script = world.create(ClassName::Script).unwrap();
    world
        .set_parent(script, Some(world.server_script_service()))
        .unwrap();
    runtime.start_script(world, script, "scripts/test.server.lua", source);
    runtime
        .take_output()
        .into_iter()
        .map(|line| match line.kind {
            OutputKind::Error => format!("error: {}", line.text),
            _ => line.text.replace('\t', " "),
        })
        .collect()
}

/// Lua making a Spline named Rail in the Workspace through `points`.
fn rail(points: &str, closed: bool) -> String {
    format!(
        r#"
        local rail = Instance.new("Spline")
        rail.Name = "Rail"
        rail.Closed = {closed}
        for _, p in {{ {points} }} do
            local point = Instance.new("SplinePoint")
            point.Position = p
            point.Parent = rail
        end
        rail.Parent = workspace
        "#
    )
}

#[test]
fn get_cframe_at_half_the_length_lands_on_a_known_curves_midpoint() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    // A straight 2-point line, and an arch that's the same either side of
    // its middle point: by distance, half way is (5, 0, 0) and (5, 5, 0).
    let source = format!(
        r#"
        {}
        local line = workspace.Rail
        line.Name = "Line"
        {}
        local function show(v)
            return string.format("%.2f %.2f %.2f", v.X, v.Y, v.Z)
        end
        local middle = line:GetCFrameAt(line.Length / 2)
        print(string.format("%.2f", line.Length), show(middle.Position), show(middle.LookVector))
        local arch = workspace.Rail
        local top = arch:GetCFrameAt(arch.Length / 2)
        print(show(top.Position), show(top.LookVector))
        print(show(arch:GetPositionAt(0.5)), show(arch:GetPositionAt(1)))
        print(string.format("%.2f", arch:GetClosestDistance(Vector3.new(5, 9, 0)) / arch.Length))
        print(pcall(function() return Instance.new("Spline"):GetCFrameAt(1) end))
        print(pcall(function() arch.Length = 3 end))
        "#,
        rail("Vector3.new(0, 0, 0), Vector3.new(10, 0, 0)", false),
        rail(
            "Vector3.new(0, 0, 0), Vector3.new(5, 5, 0), Vector3.new(10, 0, 0)",
            false
        ),
    );
    let output = run(&mut runtime, &mut world, &source);
    assert_eq!(output[0], "10.00 5.00 0.00 0.00 1.00 0.00 0.00");
    assert_eq!(output[1], "5.00 5.00 0.00 1.00 0.00 0.00");
    assert_eq!(output[2], "5.00 5.00 0.00 10.00 0.00 0.00");
    assert_eq!(output[3], "0.50");
    assert!(output[4].starts_with("false") && output[4].contains("SplinePoint"));
    assert!(output[5].starts_with("false"), "{output:?}");
}

/// A 4×1×4 anchored platform at x = 0, a Rail along +X to x = 10, and a
/// SplineFollower on the platform in `mode` at 10 studs a second. Prints
/// each Reached.
fn platform(mode: &str) -> String {
    format!(
        r#"
        {}
        local platform = Instance.new("Part")
        platform.Name = "Platform"
        platform.Anchored = true
        platform.Size = Vector3.new(4, 1, 4)
        platform.Position = Vector3.new(0, 0, 0)
        platform.Parent = workspace
        local follower = Instance.new("SplineFollower")
        follower.Spline = workspace.Rail
        follower.Speed = 10
        follower.Mode = Enum.SplineFollowMode.{mode}
        follower.Parent = platform
        follower.Reached:Connect(function(which) print("reached", which) end)
        "#,
        rail("Vector3.new(0, 0, 0), Vector3.new(10, 0, 0)", false)
    )
}

fn x_of(world: &World, name: &str) -> f32 {
    let part = world
        .get(world.workspace())
        .unwrap()
        .children()
        .iter()
        .copied()
        .find(|&id| world.get(id).unwrap().name() == name)
        .unwrap();
    world
        .get(part)
        .unwrap()
        .property("Position")
        .and_then(Value::as_vector3)
        .unwrap()
        .x
}

/// Steps `runtime` for `seconds` at 60 a second; returns what it printed.
fn play(runtime: &mut ScriptRuntime, world: &mut World, seconds: f64) -> Vec<String> {
    for _ in 0..(seconds * 60.0).round() as usize {
        runtime.step(world, 1.0 / 60.0);
    }
    runtime
        .take_output()
        .into_iter()
        .map(|line| line.text.replace('\t', " "))
        .collect()
}

#[test]
fn a_ping_pong_follower_goes_to_the_end_and_back_firing_reached_at_each() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    assert!(run(&mut runtime, &mut world, &platform("PingPong")).is_empty());
    assert!(play(&mut runtime, &mut world, 0.5).is_empty());
    assert!((x_of(&world, "Platform") - 5.0).abs() < 0.01);
    assert_eq!(play(&mut runtime, &mut world, 1.0), ["reached End"]);
    assert!((x_of(&world, "Platform") - 5.0).abs() < 0.01);
    assert_eq!(play(&mut runtime, &mut world, 1.0), ["reached Start"]);
    assert!((x_of(&world, "Platform") - 5.0).abs() < 0.01);
}

#[test]
fn loop_starts_again_and_once_stops_at_the_end() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    run(&mut runtime, &mut world, &platform("Loop"));
    assert_eq!(play(&mut runtime, &mut world, 1.25), ["reached End"]);
    assert!((x_of(&world, "Platform") - 2.5).abs() < 0.01);

    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    run(&mut runtime, &mut world, &platform("Once"));
    assert_eq!(play(&mut runtime, &mut world, 3.0), ["reached End"]);
    assert!((x_of(&world, "Platform") - 10.0).abs() < 1e-3);
}

#[test]
fn a_follower_moves_a_models_parts_together_and_face_along_turns_them() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    let source = format!(
        r#"
        {}
        local cart = Instance.new("Model")
        cart.Name = "Cart"
        for i, x in {{ -1, 1 }} do
            local part = Instance.new("Part")
            part.Name = "Part" .. i
            part.Anchored = true
            part.Size = Vector3.new(1, 1, 1)
            part.Position = Vector3.new(x, 0, 0)
            part.Parent = cart
        end
        cart.Parent = workspace
        local follower = Instance.new("SplineFollower")
        follower.Spline = workspace.Rail
        follower.FaceAlong = true
        follower.Parent = cart
        "#,
        rail("Vector3.new(0, 0, 0), Vector3.new(0, 0, -20)", false)
    );
    run(&mut runtime, &mut world, &source);
    play(&mut runtime, &mut world, 1.0);
    // 8 studs along -Z, facing -Z: unturned, so the parts keep x = ±1.
    let output = run(
        &mut runtime,
        &mut world,
        r#"
        local a, b = workspace.Cart.Part1.Position, workspace.Cart.Part2.Position
        print(string.format("%.2f %.2f %.2f %.2f", a.X, a.Z, b.X, b.Z))
        "#,
    );
    assert_eq!(output, ["-1.00 -8.00 1.00 -8.00"]);
    // The way back faces +Z, so the cart has turned round.
    play(&mut runtime, &mut world, 2.0);
    let output = run(
        &mut runtime,
        &mut world,
        r#"
        local a = workspace.Cart.Part1.Position
        print(string.format("%.2f %.0f", a.X, math.abs(workspace.Cart.Part1.Orientation.Y)))
        "#,
    );
    assert_eq!(output, ["1.00 180"]);
}

#[test]
fn patrol_sets_patrol_spline_and_checks_it_gets_a_spline() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    let source = format!(
        r#"
        {}
        local agent = Instance.new("NavigationAgent")
        agent:Patrol(workspace.Rail)
        print(agent.PatrolSpline == workspace.Rail)
        print(pcall(function() agent:Patrol(workspace) end))
        agent:Patrol(nil)
        print(agent.PatrolSpline)
        "#,
        rail("Vector3.new(0, 0, 0)", true)
    );
    let output = run(&mut runtime, &mut world, &source);
    assert_eq!(output[0], "true");
    assert!(output[1].starts_with("false") && output[1].contains("Spline"));
    assert_eq!(output[2], "nil");
}
