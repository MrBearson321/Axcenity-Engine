use axcenity_core::{ClassName, Color3, InstanceId, Material, PartType, Value, Vector3, World};

use super::*;

/// A world with one Part named `Block` in the Workspace, at the origin.
fn world_with_block() -> (World, InstanceId) {
    let mut world = World::new();
    let block = world.create(ClassName::Part).unwrap();
    world.set_name(block, "Block").unwrap();
    world.set_parent(block, Some(world.workspace())).unwrap();
    (world, block)
}

/// Adds a Script Instance like the project loader does, then runs `source` as it.
fn run(runtime: &mut ScriptRuntime, world: &mut World, path: &str, source: &str) {
    let script = world.create(ClassName::Script).unwrap();
    let name = path
        .rsplit('/')
        .next()
        .unwrap()
        .trim_end_matches(".server.lua");
    world.set_name(script, name).unwrap();
    world
        .set_parent(script, Some(world.server_script_service()))
        .unwrap();
    runtime.run_script(world, script, path, source);
}

/// Runs one script in a fresh runtime and returns everything it printed or
/// raised, one string per line, errors starting with `error: `.
fn output_of(world: &mut World, source: &str) -> Vec<String> {
    let mut runtime = ScriptRuntime::new();
    run(&mut runtime, world, "scripts/test.server.lua", source);
    lines(&mut runtime)
}

fn lines(runtime: &mut ScriptRuntime) -> Vec<String> {
    runtime
        .take_output()
        .into_iter()
        .map(|line| match line.kind {
            OutputKind::Print => line.text,
            OutputKind::Error => format!("error: {}", line.text),
        })
        .collect()
}

fn property(world: &World, id: InstanceId, name: &str) -> Value {
    world.get(id).unwrap().property(name).unwrap().clone()
}

#[test]
fn a_script_creates_a_new_part_and_moves_an_existing_one() {
    let (mut world, block) = world_with_block();
    let output = output_of(
        &mut world,
        r#"
        local ball = Instance.new("Part")
        ball.Name = "Ball"
        ball.Shape = Enum.PartType.Ball
        ball.Position = Vector3.new(0, 10, 0)
        ball.Parent = workspace

        workspace.Block.Position = workspace.Block.Position + Vector3.new(5, 0, 0)
        "#,
    );
    assert!(output.is_empty(), "{output:?}");

    assert_eq!(
        property(&world, block, "Position"),
        Value::Vector3(Vector3::new(5.0, 0.0, 0.0))
    );
    let ball = world.find_first_child(world.workspace(), "Ball").unwrap();
    assert_eq!(world.get(ball).unwrap().class(), ClassName::Part);
    assert_eq!(
        property(&world, ball, "Shape"),
        Value::PartType(PartType::Ball)
    );
    assert_eq!(
        property(&world, ball, "Position"),
        Value::Vector3(Vector3::new(0.0, 10.0, 0.0))
    );
}

#[test]
fn dangerous_functions_are_unavailable() {
    let mut world = World::new();
    let output = output_of(
        &mut world,
        r#"
        print(io, os.execute, os.exit, os.remove, os.getenv, debug, load, dofile,
              loadfile, require, package, collectgarbage, string.dump)
        "#,
    );
    assert_eq!(output, [["nil"; 13].join("\t")]);
}

#[test]
fn safe_standard_functions_are_available() {
    let mut world = World::new();
    let output = output_of(
        &mut world,
        r#"
        print(type(os.time), type(os.clock), math.max(1, 3), string.upper("hi"),
              table.concat({ "a", "b" }, ","), select('#', 1, 2), tostring(nil))
        "#,
    );
    assert_eq!(output, ["function\tfunction\t3\tHI\ta,b\t2\tnil"]);
}

#[test]
fn compiled_bytecode_is_rejected() {
    let mut world = World::new();
    // "\27Lua" is how compiled Lua chunks start.
    let output = output_of(&mut world, "\x1bLua");
    assert_eq!(output.len(), 1);
    assert!(
        output[0].contains("attempt to load a binary chunk"),
        "{output:?}"
    );
}

#[test]
fn a_runtime_error_reports_the_path_and_line_and_other_scripts_keep_running() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        "scripts/broken.server.lua",
        "print('before')\nlocal missing = nil\nmissing.Position = 1\nprint('never')",
    );
    run(
        &mut runtime,
        &mut world,
        "scripts/fine.server.lua",
        "print('still running')",
    );
    let output = lines(&mut runtime);
    assert_eq!(output.len(), 3, "{output:?}");
    assert_eq!(output[0], "before");
    assert!(
        output[1].starts_with("error: scripts/broken.server.lua:3: attempt to index a nil value"),
        "{output:?}"
    );
    assert_eq!(output[2], "still running");
}

#[test]
fn engine_errors_report_the_path_and_line() {
    let (mut world, _) = world_with_block();
    let output = output_of(&mut world, "\n\nworkspace.Block.Size = 5");
    assert_eq!(
        output,
        ["error: scripts/test.server.lua:3: Part.Size must be a Vector3, not a number"]
    );
}

#[test]
fn a_syntax_error_reports_the_path_and_line() {
    let mut world = World::new();
    let output = output_of(&mut world, "print('ok')\nlocal = 5");
    assert_eq!(output.len(), 1);
    assert!(
        output[0].starts_with("error: scripts/test.server.lua:2:"),
        "{output:?}"
    );
}

#[test]
fn error_raised_by_a_script_keeps_its_location() {
    let mut world = World::new();
    let output = output_of(&mut world, "error('custom problem')");
    assert_eq!(output, ["error: scripts/test.server.lua:1: custom problem"]);
}

#[test]
fn scripts_cannot_see_each_others_globals() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        "scripts/a.server.lua",
        "secret = 42\n_G.shared = 1\nmath.extra = 1\nprint = nil\nVector3.new = nil",
    );
    run(
        &mut runtime,
        &mut world,
        "scripts/b.server.lua",
        "print(secret, _G.shared, math.extra, Vector3.new(1, 2, 3))",
    );
    assert_eq!(lines(&mut runtime), ["nil\tnil\tnil\t1, 2, 3"]);
}

#[test]
fn print_shows_engine_values_readably() {
    let (mut world, _) = world_with_block();
    let output = output_of(
        &mut world,
        r#"print("a", 1, 2.5, true, Vector3.new(1, 2, 3), workspace.Block,
                 Enum.Material.Neon, Color3.new(1, 0, 0))"#,
    );
    assert_eq!(
        output,
        ["a\t1\t2.5\ttrue\t1, 2, 3\tBlock\tEnum.Material.Neon\t1, 0, 0"]
    );
}

#[test]
fn vector3_supports_arithmetic_and_components() {
    let mut world = World::new();
    let output = output_of(
        &mut world,
        r#"
        local a = Vector3.new(1, 2, 3)
        print(a + Vector3.new(1, 1, 1), a - a, a * 2, 2 * a, a * a, a / 2, -a)
        print(a.X, a.Y, a.Z, Vector3.new(3, 4, 0).Magnitude, Vector3.new(0, 5, 0).Unit)
        print(a == Vector3.new(1, 2, 3), a == Vector3.new(), Vector3.new(0.1).X)
        "#,
    );
    assert_eq!(
        output,
        [
            "2, 3, 4\t0, 0, 0\t2, 4, 6\t2, 4, 6\t1, 4, 9\t0.5, 1, 1.5\t-1, -2, -3",
            "1.0\t2.0\t3.0\t5.0\t0, 1, 0",
            "true\tfalse\t0.1",
        ]
    );
}

#[test]
fn color3_takes_0_to_1_or_from_rgb_0_to_255() {
    let (mut world, block) = world_with_block();
    let output = output_of(
        &mut world,
        r#"
        workspace.Block.Color = Color3.fromRGB(255, 0, 51)
        print(Color3.new(0.5, 1, 0).G)
        Color3.new(255, 0, 0)
        "#,
    );
    assert_eq!(
        property(&world, block, "Color"),
        Value::Color3(Color3::new(1.0, 0.0, 0.2))
    );
    assert_eq!(output.len(), 2);
    assert_eq!(output[0], "1.0");
    assert!(output[1].contains("use Color3.fromRGB"), "{output:?}");
}

#[test]
fn enum_properties_accept_enum_items_and_names() {
    let (mut world, block) = world_with_block();
    let output = output_of(
        &mut world,
        r#"
        local block = workspace.Block
        block.Material = Enum.Material.Metal
        print(block.Material, block.Material == Enum.Material.Metal, block.Metalness)
        block.Material = "Neon"
        block.Shape = "Wedge"
        print(block.Material.Name, block.Shape)
        "#,
    );
    assert_eq!(
        output,
        [
            "Enum.Material.Metal\ttrue\t1.0",
            "Neon\tEnum.PartType.Wedge"
        ]
    );
    assert_eq!(
        property(&world, block, "Material"),
        Value::Material(Material::Neon)
    );
    assert_eq!(
        property(&world, block, "Shape"),
        Value::PartType(PartType::Wedge)
    );
}

#[test]
fn a_misspelled_enum_item_lists_the_choices() {
    let (mut world, _) = world_with_block();
    let output = output_of(&mut world, r#"workspace.Block.Material = "Wood""#);
    assert_eq!(output.len(), 1);
    assert!(
        output[0].ends_with("choose from Plastic, Metal, Glass, Neon"),
        "{output:?}"
    );

    let output = output_of(&mut world, "workspace.Block.Shape = Enum.Material.Neon");
    assert!(
        output[0].ends_with("Part.Shape must be a PartType, not a Material"),
        "{output:?}"
    );
}

#[test]
fn children_are_found_by_name() {
    let (mut world, _) = world_with_block();
    let output = output_of(
        &mut world,
        r#"
        print(workspace:FindFirstChild("Block"), workspace:FindFirstChild("Missing"))
        print(game.Workspace == workspace, workspace.Block.Parent == workspace)
        print(game:FindFirstChild("Block", true), workspace.Block.ClassName)
        local missing = workspace.Missing
        "#,
    );
    assert_eq!(output.len(), 4, "{output:?}");
    assert_eq!(output[0..3], ["Block\tnil", "true\ttrue", "Block\tPart"]);
    assert_eq!(
        output[3],
        "error: scripts/test.server.lua:5: Missing is not a valid member of Workspace \"Workspace\""
    );
}

#[test]
fn the_script_global_is_the_scripts_own_instance() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        "scripts/build.server.lua",
        "print(script.Name, script.ClassName, script.Parent.Name)",
    );
    assert_eq!(lines(&mut runtime), ["build\tScript\tServerScriptService"]);
}

#[test]
fn bad_assignments_explain_what_was_expected() {
    let (mut world, _) = world_with_block();
    let output = output_of(
        &mut world,
        r#"
        local ok, message = pcall(function() workspace.Block.Parent = "Workspace" end)
        print(message)
        ok, message = pcall(function() workspace.Parent = nil end)
        print(message)
        ok, message = pcall(function() Instance.new("Prat") end)
        print(message)
        ok, message = pcall(function() workspace.Block.Shininess = 1 end)
        print(message)
        "#,
    );
    let expected = [
        "Parent must be an Instance or nil, not a string",
        "can't set Parent: Workspace is a service, so it can't be created, moved or destroyed",
        "there is no class named \"Prat\"",
        "Shininess is not a valid member of Part \"Workspace.Block\"",
    ];
    assert_eq!(output.len(), expected.len(), "{output:?}");
    for (line, expected) in output.iter().zip(expected) {
        assert!(
            line.ends_with(expected),
            "{line:?} should end with {expected:?}"
        );
    }
}

#[test]
fn pcall_and_xpcall_give_scripts_clean_messages() {
    let (mut world, _) = world_with_block();
    let output = output_of(
        &mut world,
        r#"print(pcall(function() workspace.Block.Size = 1 end))
print(xpcall(function() error("boom") end, function(m) return "handled: " .. m end))
local ok, value = pcall(error, { code = 7 })
print(ok, value.code, pcall(function() return 1, 2 end))
print(pcall(function() return game.Nope end))"#,
    );
    assert_eq!(
        output,
        [
            "false	scripts/test.server.lua:1: Part.Size must be a Vector3, not a number",
            "false	handled: scripts/test.server.lua:2: boom",
            "false	7	true	1	2",
            "false	scripts/test.server.lua:5: Nope is not a valid member of DataModel \"Game\"",
        ]
    );
}

#[test]
fn the_world_is_returned_even_after_an_error() {
    let (mut world, block) = world_with_block();
    output_of(&mut world, "error('stop')");
    assert!(world.get(block).is_some());
}
