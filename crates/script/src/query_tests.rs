//! Tests for Phase 38: RaycastParams, OverlapParams, the Workspace's
//! queries, PhysicsService and TouchEnded, from Lua.

use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};
use axcenity_physics::Physics;

use super::*;

fn part(world: &mut World, name: &str, position: Vector3, size: Vector3) -> InstanceId {
    let id = world.create(ClassName::Part).unwrap();
    world.set_name(id, name).unwrap();
    world.set_parent(id, Some(world.workspace())).unwrap();
    for (property, value) in [
        ("Position", Value::Vector3(position)),
        ("Size", Value::Vector3(size)),
        ("Anchored", Value::Bool(true)),
    ] {
        world.set_property(id, property, value).unwrap();
    }
    id
}

fn v(x: f32, y: f32, z: f32) -> Vector3 {
    Vector3::new(x, y, z)
}

fn start(world: &mut World, runtime: &mut ScriptRuntime, source: &str) {
    let script = world.create(ClassName::Script).unwrap();
    world
        .set_parent(script, Some(world.server_script_service()))
        .unwrap();
    runtime.start_script(world, script, "scripts/test.server.lua", source);
}

fn output(runtime: &mut ScriptRuntime) -> Vec<String> {
    runtime
        .take_output()
        .into_iter()
        .map(|line| match line.kind {
            OutputKind::Error => format!("error: {}", line.text),
            _ => line.text.replace('\t', " "),
        })
        .collect()
}

fn run(world: &mut World, source: &str) -> Vec<String> {
    let mut runtime = ScriptRuntime::new();
    start(world, &mut runtime, source);
    output(&mut runtime)
}

/// A shooter's character (a Model with a body part), a wall behind it and
/// a floor.
fn shooting_range() -> World {
    let mut world = World::new();
    part(&mut world, "Floor", v(0.0, -0.5, 0.0), v(40.0, 1.0, 40.0));
    part(&mut world, "Wall", v(0.0, 3.0, -10.0), v(10.0, 6.0, 1.0));
    let character = world.create(ClassName::Model).unwrap();
    world.set_name(character, "Shooter").unwrap();
    world
        .set_parent(character, Some(world.workspace()))
        .unwrap();
    let torso = part(&mut world, "Torso", v(0.0, 3.0, 0.0), v(2.0, 2.0, 1.0));
    world.set_parent(torso, Some(character)).unwrap();
    world
}

#[test]
fn a_raycast_that_excludes_the_shooters_character_hits_the_wall_behind_it() {
    let mut world = shooting_range();
    let lines = run(
        &mut world,
        r#"
        local shooter = workspace.Shooter
        local from, along = Vector3.new(0, 3, 2), Vector3.new(0, 0, -50)
        print(workspace:Raycast(from, along).Instance.Name)
        local params = RaycastParams.new()
        params.FilterDescendantsInstances = {shooter}
        params.FilterType = Enum.RaycastFilterType.Exclude
        local hit = workspace:Raycast(from, along, params)
        print(hit.Instance.Name, hit.Position.Z, hit.Normal.Z, hit.Distance)
        print(params.FilterType, #params.FilterDescendantsInstances, typeof(params))
        local only = RaycastParams.new({
            FilterDescendantsInstances = {workspace.Floor},
            FilterType = Enum.RaycastFilterType.Include,
        })
        print(workspace:Raycast(from, along, only))
        print(pcall(function() params.FilterType = "Exclude" end))
        print(pcall(function() return params.MaxParts end))
        "#,
    );
    assert_eq!(lines[0], "Torso");
    assert_eq!(lines[1], "Wall -9.5 1 11.5");
    assert_eq!(lines[2], "Enum.RaycastFilterType.Exclude 1 RaycastParams");
    assert_eq!(lines[3], "nil");
    assert!(lines[4].starts_with("false") && lines[4].contains("Enum.RaycastFilterType"));
    assert!(lines[5].contains("MaxParts is not a valid member of RaycastParams"));
}

#[test]
fn get_parts_in_part_lists_the_parts_in_a_trigger_zone_except_unqueryable_ones() {
    let mut world = World::new();
    let zone = part(&mut world, "Zone", v(0.0, 5.0, 0.0), v(10.0, 10.0, 10.0));
    world
        .set_property(zone, "CanCollide", Value::Bool(false))
        .unwrap();
    part(&mut world, "Coin", v(2.0, 2.0, 2.0), v(1.0, 1.0, 1.0));
    part(&mut world, "Crate", v(5.0, 5.0, 0.0), v(2.0, 2.0, 2.0));
    let hidden = part(&mut world, "Hidden", v(-2.0, 5.0, 0.0), v(1.0, 1.0, 1.0));
    world
        .set_property(hidden, "CanQuery", Value::Bool(false))
        .unwrap();
    part(&mut world, "Far", v(20.0, 5.0, 0.0), v(1.0, 1.0, 1.0));
    let lines = run(
        &mut world,
        r#"
        local function names(parts)
            local list = {}
            for _, p in parts do table.insert(list, p.Name) end
            return table.concat(list, ",")
        end
        print(names(workspace:GetPartsInPart(workspace.Zone)))
        local params = OverlapParams.new()
        params:AddToFilter(workspace.Crate)
        print(names(workspace:GetPartsInPart(workspace.Zone, params)))
        params.MaxParts = 1
        params.FilterDescendantsInstances = {}
        print(names(workspace:GetPartBoundsInBox(CFrame.new(0, 5, 0), Vector3.new(10, 10, 10), params)))
        print(names(workspace:GetPartBoundsInRadius(Vector3.new(20, 5, 0), 2)))
        workspace.Hidden.CanQuery = true
        print(names(workspace:GetPartsInPart(workspace.Zone)))
        "#,
    );
    assert_eq!(
        lines,
        vec!["Coin,Crate", "Coin", "Zone", "Far", "Coin,Crate,Hidden",]
    );
}

#[test]
fn shape_casts_sweep_a_ball_a_block_or_a_part() {
    let mut world = World::new();
    part(&mut world, "Wall", v(0.0, 0.0, -10.0), v(10.0, 10.0, 1.0));
    part(&mut world, "Post", v(2.5, 0.0, -5.0), v(1.0, 10.0, 1.0));
    part(&mut world, "Bullet", v(2.5, 0.0, 0.0), v(1.0, 1.0, 1.0));
    let lines = run(
        &mut world,
        r#"
        local along = Vector3.new(0, 0, -20)
        print(workspace:Raycast(Vector3.new(), along).Instance.Name)
        print(workspace:Spherecast(Vector3.new(), 2.5, along).Instance.Name)
        local hit = workspace:Spherecast(Vector3.new(), 1, along)
        print(hit.Instance.Name, hit.Distance)
        print(workspace:Blockcast(CFrame.new(), Vector3.new(6, 1, 1), along).Instance.Name)
        print(workspace:Shapecast(workspace.Bullet, along).Instance.Name)
        local params = RaycastParams.new({FilterDescendantsInstances = {workspace.Post}})
        print(workspace:Shapecast(workspace.Bullet, along, params).Instance.Name)
        print(pcall(function() return workspace:Shapecast(workspace.Bullet, along, OverlapParams.new()) end))
        "#,
    );
    assert_eq!(
        lines[..6],
        ["Wall", "Post", "Wall 8.5", "Post", "Post", "Wall",]
    );
    assert!(lines[6].contains("must be a RaycastParams"), "{}", lines[6]);
}

#[test]
fn physics_service_registers_groups_that_queries_and_physics_respect() {
    let mut world = World::new();
    part(&mut world, "Floor", v(0.0, -0.5, 0.0), v(40.0, 1.0, 40.0));
    let ledge = part(&mut world, "Ledge", v(0.0, 2.0, 0.0), v(4.0, 1.0, 4.0));
    let ball = part(&mut world, "Ball", v(0.0, 6.0, 0.0), v(2.0, 1.0, 2.0));
    world
        .set_property(ball, "Anchored", Value::Bool(false))
        .unwrap();
    let lines = run(
        &mut world,
        r#"
        local PhysicsService = game:GetService("PhysicsService")
        PhysicsService:RegisterCollisionGroup("Balls")
        PhysicsService:RegisterCollisionGroup("Ledges")
        print(PhysicsService:CollisionGroupsAreCollidable("Balls", "Ledges"))
        PhysicsService:CollisionGroupSetCollidable("Balls", "Ledges", false)
        print(PhysicsService:CollisionGroupsAreCollidable("Ledges", "Balls"))
        print(PhysicsService:CollisionGroupsAreCollidable("Balls", "Default"))
        print(pcall(function() PhysicsService:CollisionGroupSetCollidable("Balls", "Ghosts", false) end))
        workspace.Ball.CollisionGroup = "Balls"
        workspace.Ledge.CollisionGroup = "Ledges"
        local down = Vector3.new(0, -10, 0)
        print(workspace:Raycast(Vector3.new(0, 4, 0), down).Instance.Name)
        local params = RaycastParams.new({CollisionGroup = "Balls"})
        print(workspace:Raycast(Vector3.new(0, 4, 0), down, params).Instance.Name)
        "#,
    );
    assert_eq!(lines[..3], ["true", "false", "true"]);
    assert!(
        lines[3].contains("no collision group named \"Ghosts\""),
        "{}",
        lines[3]
    );
    assert_eq!(lines[4..], ["Ledge", "Floor"]);

    let mut physics = Physics::new();
    for _ in 0..120 {
        physics.step(&mut world, 1.0 / 60.0);
    }
    let y = world
        .get(ball)
        .unwrap()
        .property("Position")
        .and_then(Value::as_vector3)
        .unwrap()
        .y;
    assert!((y - 0.5).abs() < 0.05, "it fell through the ledge: {y}");
    assert!(world.collision_groups().contains("Balls"));
    let _ = ledge;
}

#[test]
fn touch_ended_fires_once_on_both_parts_when_a_part_leaves_another() {
    let mut world = World::new();
    part(&mut world, "Floor", v(0.0, -0.5, 0.0), v(40.0, 1.0, 40.0));
    let block = part(&mut world, "Block", v(0.0, 3.0, 0.0), v(2.0, 1.0, 2.0));
    world
        .set_property(block, "Anchored", Value::Bool(false))
        .unwrap();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut world,
        &mut runtime,
        r#"
        workspace.Block.Touched:Connect(function(other) print("touched", other.Name) end)
        workspace.Block.TouchEnded:Connect(function(other) print("ended", other.Name) end)
        workspace.Floor.TouchEnded:Connect(function(other) print("floor ended", other.Name) end)
        "#,
    );
    let mut physics = Physics::new();
    let mut frame = |world: &mut World, runtime: &mut ScriptRuntime| {
        physics.step(world, 1.0 / 60.0);
        let touches = physics.take_touches();
        runtime.fire_touched(world, &touches);
        let ends = physics.take_touch_ends();
        runtime.fire_touch_ended(world, &ends);
    };
    for _ in 0..60 {
        frame(&mut world, &mut runtime);
    }
    assert_eq!(output(&mut runtime), vec!["touched Floor"]);

    // Lifted off the floor and held there: it ends once.
    world
        .set_property(block, "Anchored", Value::Bool(true))
        .unwrap();
    world
        .set_property(block, "Position", Value::Vector3(v(0.0, 10.0, 0.0)))
        .unwrap();
    for _ in 0..60 {
        frame(&mut world, &mut runtime);
    }
    assert_eq!(
        output(&mut runtime),
        vec!["floor ended Block", "ended Floor"]
    );
}
