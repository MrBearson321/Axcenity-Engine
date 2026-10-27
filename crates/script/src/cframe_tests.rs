use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};

use super::*;
use crate::cframe::{from_vec3, rotation_from_orientation};

fn part(world: &mut World, name: &str, parent: InstanceId, position: Vector3) -> InstanceId {
    let part = world.create(ClassName::Part).unwrap();
    world.set_name(part, name).unwrap();
    world
        .set_property(part, "Position", Value::Vector3(position))
        .unwrap();
    world.set_parent(part, Some(parent)).unwrap();
    part
}

fn vector(world: &World, id: InstanceId, name: &str) -> Vector3 {
    match world.get(id).unwrap().property(name) {
        Some(Value::Vector3(v)) => *v,
        other => panic!("{name} is {other:?}"),
    }
}

fn near(a: Vector3, b: Vector3) -> bool {
    (a.x - b.x).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4 && (a.z - b.z).abs() < 1e-4
}

/// Runs `source` as a server Script and returns what it printed, with
/// errors marked.
fn run(world: &mut World, runtime: &mut ScriptRuntime, source: &str) -> Vec<String> {
    let script = world.create(ClassName::Script).unwrap();
    world
        .set_parent(script, Some(world.server_script_service()))
        .unwrap();
    runtime.start_script(world, script, "scripts/Math.server.lua", source);
    runtime
        .take_output()
        .into_iter()
        .map(|line| match line.kind {
            OutputKind::Error => format!("error: {}", line.text),
            _ => line.text,
        })
        .collect()
}

#[test]
fn look_at_points_the_look_vector_at_the_target_and_orientation_reads_back_as_set() {
    let mut world = World::new();
    let workspace = world.workspace();
    let from = Vector3::new(1.0, 2.0, 3.0);
    let block = part(&mut world, "Block", workspace, from);
    // Ten studs along the way a part turned to (30, 60, 0) faces.
    let look = rotation_from_orientation(Vector3::new(30.0, 60.0, 0.0)) * glam::Vec3::NEG_Z;
    let target_at = from_vec3(glam::Vec3::new(from.x, from.y, from.z) + look * 10.0);
    part(&mut world, "Target", workspace, target_at);
    let mut runtime = ScriptRuntime::new();
    let output = run(
        &mut world,
        &mut runtime,
        r#"
        local block, target = workspace.Block, workspace.Target
        block.CFrame = CFrame.lookAt(block.Position, target.Position)
        local toward = (target.Position - block.Position).Unit
        assert(block.CFrame.LookVector:FuzzyEq(toward, 1e-5), tostring(block.CFrame.LookVector))

        local other = Instance.new("Part")
        other.Orientation = Vector3.new(30, 60, 0)
        assert(other.Orientation == block.Orientation, tostring(block.Orientation))
        assert(other.CFrame.LookVector:FuzzyEq(block.CFrame.LookVector, 1e-5))
        print(block.Orientation)
        "#,
    );
    assert_eq!(output, ["30, 60, 0"]);
    assert_eq!(
        vector(&world, block, "Orientation"),
        Vector3::new(30.0, 60.0, 0.0)
    );
    assert_eq!(vector(&world, block, "Position"), from);
}

#[test]
fn pivot_to_turns_a_model_around_its_pivot_and_keeps_its_parts_offsets() {
    let mut world = World::new();
    let model = world.create(ClassName::Model).unwrap();
    world.set_name(model, "Cart").unwrap();
    world.set_parent(model, Some(world.workspace())).unwrap();
    let a = part(&mut world, "A", model, Vector3::new(2.0, 0.0, 0.0));
    let b = part(&mut world, "B", model, Vector3::new(-2.0, 0.0, 4.0));
    for id in [a, b] {
        world
            .set_property(id, "Size", Value::Vector3(Vector3::new(2.0, 2.0, 2.0)))
            .unwrap();
    }
    let mut runtime = ScriptRuntime::new();
    let output = run(
        &mut world,
        &mut runtime,
        r#"
        local model = workspace.Cart
        local pivot = model:GetPivot()
        print(pivot.Position)
        local before = {}
        for _, part in model:GetChildren() do
            before[part.Name] = pivot:ToObjectSpace(part.CFrame)
        end
        model:PivotTo(model:GetPivot() * CFrame.Angles(0, math.rad(90), 0))
        local turned = pivot * CFrame.Angles(0, math.rad(90), 0)
        for _, part in model:GetChildren() do
            local offset = turned:ToObjectSpace(part.CFrame)
            assert(offset.Position:FuzzyEq(before[part.Name].Position, 1e-4), part.Name)
            assert(offset.LookVector:FuzzyEq(before[part.Name].LookVector, 1e-4), part.Name)
        end
        assert(model:GetPivot().Position:FuzzyEq(pivot.Position, 1e-5))
        "#,
    );
    // The middle of the box from (-3, -1, -1) to (3, 1, 5).
    assert_eq!(output, ["0, 0, 2"]);
    // A quarter turn left around (0, 0, 2) turns +X into -Z and +Z into +X:
    // A's offset (2, 0, -2) becomes (-2, 0, -2), and B's (-2, 0, 2) becomes (2, 0, 2).
    assert!(near(
        vector(&world, a, "Position"),
        Vector3::new(-2.0, 0.0, 0.0)
    ));
    assert!(near(
        vector(&world, b, "Position"),
        Vector3::new(2.0, 0.0, 4.0)
    ));
    for id in [a, b] {
        assert_eq!(
            vector(&world, id, "Orientation"),
            Vector3::new(0.0, 90.0, 0.0)
        );
    }
}

#[test]
fn a_cframe_tween_to_350_degrees_turns_the_short_way() {
    let mut world = World::new();
    let workspace = world.workspace();
    let block = part(&mut world, "Block", workspace, Vector3::new(0.0, 5.0, 0.0));
    let mut runtime = ScriptRuntime::new();
    let output = run(
        &mut world,
        &mut runtime,
        r#"
        local block = workspace.Block
        local goal = CFrame.new(block.Position) * CFrame.Angles(0, math.rad(350), 0)
        local info = TweenInfo.new(1, Enum.EasingStyle.Linear)
        game:GetService("TweenService"):Create(block, info, {CFrame = goal}):Play()
        "#,
    );
    assert!(output.is_empty(), "{output:?}");
    let mut turns = Vec::new();
    for _ in 0..4 {
        runtime.step(&mut world, 0.25);
        turns.push(vector(&world, block, "Orientation").y);
    }
    // 350° is -10°: a quarter of the way is -2.5°, never past 180°.
    assert_eq!(turns, [-2.5, -5.0, -7.5, -10.0]);
    assert_eq!(
        vector(&world, block, "Position"),
        Vector3::new(0.0, 5.0, 0.0)
    );
}

/// The first numbers `Random.new(42)` gives. They're the same on every
/// platform: the generator is whole-number arithmetic.
const SEEDED: &str = r#"
    local random = Random.new(42)
    local numbers = {}
    for _ = 1, 3 do
        table.insert(numbers, string.format("%.17g", random:NextNumber()))
    end
    for _ = 1, 3 do
        table.insert(numbers, tostring(random:NextInteger(1, 100)))
    end
    local list = {1, 2, 3, 4, 5, 6}
    random:Shuffle(list)
    table.insert(numbers, table.concat(list, " "))
    local v = random:NextUnitVector()
    table.insert(numbers, string.format("%.6f %.6f %.6f", v.X, v.Y, v.Z))
    print(table.concat(numbers, ", "))
"#;

#[test]
fn random_with_a_seed_gives_the_same_sequence_every_run() {
    let first = run(&mut World::new(), &mut ScriptRuntime::new(), SEEDED);
    let second = run(&mut World::new(), &mut ScriptRuntime::new(), SEEDED);
    assert_eq!(first, second);
    assert_eq!(first, [GOLDEN]);
}

const GOLDEN: &str = "0.76155828461472108, 0.44811550417592128, 0.95970718129751265, \
    72, 78, 6, 3 2 6 1 4 5, 0.471409 -0.065983 -0.879443";

#[test]
fn random_numbers_stay_in_their_ranges_and_unseeded_generators_differ() {
    let output = run(
        &mut World::new(),
        &mut ScriptRuntime::new(),
        r#"
        local random = Random.new()
        for _ = 1, 200 do
            local n = random:NextNumber(5, 10)
            assert(n >= 5 and n < 10, n)
            local i = random:NextInteger(-2, 2)
            assert(i >= -2 and i <= 2 and i == math.floor(i), i)
            assert(math.abs(random:NextUnitVector().Magnitude - 1) < 1e-5)
        end
        local copy = random:Clone()
        assert(copy:NextNumber() == random:NextNumber())
        assert(Random.new():NextNumber() ~= Random.new():NextNumber())
        print(typeof(random), pcall(random.NextInteger, random, 3, 1))
        "#,
    );
    assert_eq!(
        output,
        [
            "Random\tfalse\tscripts/Math.server.lua:13: NextInteger needs the smaller number first, such as random:NextInteger(1, 3)"
        ]
    );
}

#[test]
fn vectors_have_their_math_methods_and_constants() {
    let output = run(
        &mut World::new(),
        &mut ScriptRuntime::new(),
        r#"
        local a, b = Vector3.new(1, 2, 3), Vector3.new(4, -5, 6)
        assert(a:Dot(b) == 12)
        assert(Vector3.xAxis:Cross(Vector3.yAxis) == Vector3.zAxis)
        assert(a:Lerp(b, 0.5) == Vector3.new(2.5, -1.5, 4.5))
        assert(math.abs(Vector3.xAxis:Angle(Vector3.zAxis) - math.pi / 2) < 1e-6)
        assert(Vector3.xAxis:Angle(Vector3.zAxis, Vector3.yAxis) < 0)
        assert(a:FuzzyEq(Vector3.new(1, 2, 3.000001)))
        assert(not a:FuzzyEq(Vector3.new(1, 2, 3.1)))
        assert(a:Min(b) == Vector3.new(1, -5, 3))
        assert(a:Max(b, Vector3.new(9, 0, 0)) == Vector3.new(9, 2, 6))
        assert(Vector3.new(-1.5, 2.5, -0.5):Abs() == Vector3.new(1.5, 2.5, 0.5))
        assert(Vector3.new(-1.5, 2.5, 0):Floor() == Vector3.new(-2, 2, 0))
        assert(Vector3.new(-1.5, 2.5, 0):Ceil() == Vector3.new(-1, 3, 0))
        assert(b / a == Vector3.new(4, -2.5, 2))
        assert(6 / a == Vector3.new(6, 3, 2))
        assert(Vector3.zero == Vector3.new() and Vector3.one == Vector3.new(1, 1, 1))

        local p, q = Vector2.new(3, 4), Vector2.new(1, 2)
        assert(p:Dot(q) == 11 and p:Lerp(q, 0.5) == Vector2.new(2, 3))
        assert(p / q == Vector2.new(3, 2) and p / 2 == Vector2.new(1.5, 2))
        assert(math.abs(Vector2.xAxis:Angle(Vector2.yAxis) - math.pi / 2) < 1e-6)
        assert(Vector2.yAxis:Angle(Vector2.xAxis, true) < 0)
        assert(p:Min(q) == q and p:Max(q) == p and p:FuzzyEq(Vector2.new(3, 4)))
        assert(Vector2.new(-1.5, 0.5):Abs() == Vector2.new(1.5, 0.5))
        assert(Vector2.new(-1.5, 0.5):Floor() == Vector2.new(-2, 0))
        assert(Vector2.new(-1.5, 0.5):Ceil() == Vector2.new(-1, 1))
        assert(Vector2.zero == Vector2.new() and Vector2.one == Vector2.new(1, 1))
        print("ok")
        "#,
    );
    assert_eq!(output, ["ok"]);
}

#[test]
fn colors_lerp_and_convert_to_and_from_hsv_and_hex() {
    let output = run(
        &mut World::new(),
        &mut ScriptRuntime::new(),
        r##"
        local red, blue = Color3.new(1, 0, 0), Color3.new(0, 0, 1)
        assert(red:Lerp(blue, 0.5) == Color3.new(0.5, 0, 0.5))
        assert(Color3.fromHSV(0, 1, 1) == red)
        assert(Color3.fromHSV(2 / 3, 1, 1):ToHex() == "0000ff")
        local h, s, v = Color3.fromRGB(255, 128, 0):ToHSV()
        assert(math.abs(h - 30 / 360) < 1e-3 and s == 1 and v == 1, h)
        assert(Color3.fromHex("#FF8000"):ToHex() == "ff8000")
        assert(Color3.fromHex("f80") == Color3.fromHex("ff8800"))
        print(select(2, pcall(Color3.fromHex, "orange")))
        "##,
    );
    assert_eq!(
        output,
        [
            "scripts/Math.server.lua:10: Color3.fromHex takes six hex digits such as \"#ff8000\" (or three, such as \"#f80\"), not \"orange\""
        ]
    );
}

#[test]
fn cframes_compose_invert_and_convert_between_spaces() {
    let output = run(
        &mut World::new(),
        &mut ScriptRuntime::new(),
        r#"
        local cf = CFrame.new(1, 2, 3) * CFrame.Angles(0, math.rad(90), 0)
        assert(CFrame.new(Vector3.new(1, 2, 3)) == CFrame.new(1, 2, 3))
        assert(CFrame.new().Position == Vector3.zero and CFrame.identity == CFrame.new())
        assert(cf.LookVector:FuzzyEq(-Vector3.xAxis), tostring(cf.LookVector))
        assert(cf.RightVector:FuzzyEq(-Vector3.zAxis), tostring(cf.RightVector))
        assert(cf.UpVector:FuzzyEq(Vector3.yAxis))
        local point = Vector3.new(0, 0, -5)
        assert((cf * point):FuzzyEq(Vector3.new(-4, 2, 3)), tostring(cf * point))
        assert(cf:PointToWorldSpace(point) == cf * point)
        assert(cf:PointToObjectSpace(cf * point):FuzzyEq(point))
        assert(cf:VectorToWorldSpace(point):FuzzyEq(Vector3.new(-5, 0, 0)))
        assert(cf:VectorToObjectSpace(Vector3.new(-5, 0, 0)):FuzzyEq(point))
        local back = cf * cf:Inverse()
        assert(back.Position:FuzzyEq(Vector3.zero) and back.LookVector:FuzzyEq(-Vector3.zAxis))
        local other = CFrame.new(0, 5, 0)
        assert(cf:ToObjectSpace(cf:ToWorldSpace(other)).Position:FuzzyEq(other.Position))
        assert((cf + Vector3.yAxis).Position == Vector3.new(1, 3, 3))
        assert((cf - Vector3.yAxis).Position == Vector3.new(1, 1, 3))
        local half = CFrame.new():Lerp(cf, 0.5)
        assert(half.Position:FuzzyEq(Vector3.new(0.5, 1, 1.5)))
        assert(half.LookVector:FuzzyEq(CFrame.Angles(0, math.rad(45), 0).LookVector))
        local rx, ry, rz = CFrame.fromEulerAnglesYXZ(0.1, 0.2, 0.3):ToEulerAnglesYXZ()
        assert(math.abs(rx - 0.1) < 1e-6 and math.abs(ry - 0.2) < 1e-6 and math.abs(rz - 0.3) < 1e-6)
        local turn = CFrame.fromAxisAngle(Vector3.yAxis, math.rad(90))
        assert(turn.LookVector:FuzzyEq(cf.LookVector))
        assert(typeof(cf) == "CFrame")
        print(CFrame.new(1, 2, 3))
        print(select(2, pcall(function() return cf * 2 end)))
        "#,
    );
    assert_eq!(
        output,
        [
            "1, 2, 3, 1, 0, 0, 0, 1, 0, 0, 0, 1",
            "scripts/Math.server.lua:29: a CFrame multiplies by a CFrame or a Vector3, not a number"
        ]
    );
}

#[test]
fn setting_cframe_to_itself_changes_no_stored_value() {
    let mut world = World::new();
    let workspace = world.workspace();
    let block = part(&mut world, "Block", workspace, Vector3::new(0.1, 2.0, -7.3));
    // Not how Orientation reads back from a rotation: 270 would read as -90,
    // and 12.34567 has more decimals than a rotation gives.
    let stored = [
        Vector3::new(0.0, 270.0, 0.0),
        Vector3::new(12.34567, -45.5, 190.0),
        Vector3::new(120.0, 0.0, 0.0),
    ];
    let mut runtime = ScriptRuntime::new();
    for orientation in stored {
        world
            .set_property(block, "Orientation", Value::Vector3(orientation))
            .unwrap();
        let before = world.get(block).unwrap().clone();
        let output = run(
            &mut world,
            &mut runtime,
            "workspace.Block.CFrame = workspace.Block.CFrame
            workspace.Block:PivotTo(workspace.Block:GetPivot())",
        );
        assert!(output.is_empty(), "{output:?}");
        let after = world.get(block).unwrap();
        assert_eq!(
            before.properties().collect::<Vec<_>>(),
            after.properties().collect::<Vec<_>>()
        );
    }
}

#[test]
fn cframe_is_only_on_parts_and_must_be_a_cframe() {
    let output = run(
        &mut World::new(),
        &mut ScriptRuntime::new(),
        r#"
        local part = Instance.new("Part")
        print(pcall(function() part.CFrame = Vector3.new() end))
        print(pcall(function() return Instance.new("Folder").CFrame end))
        print(pcall(function() Instance.new("Model"):PivotTo(5) end))
        "#,
    );
    assert_eq!(output.len(), 3);
    assert!(
        output[0].ends_with("Part.CFrame must be a CFrame, not a Vector3"),
        "{output:?}"
    );
    assert!(output[1].starts_with("false"), "{output:?}");
    assert!(
        output[2].ends_with(
            "PivotTo needs a CFrame, such as model:PivotTo(CFrame.new(0, 5, 0)), not a number"
        ),
        "{output:?}"
    );
}
