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
    runtime.start_script(world, script, path, source);
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
fn sound_play_and_stop_queue_host_actions() {
    let mut world = World::new();
    let sound = world.create(ClassName::Sound).unwrap();
    world.set_name(sound, "Chime").unwrap();
    world
        .set_property(
            sound,
            "SoundId",
            Value::String("assets/audio/chime.wav".to_owned()),
        )
        .unwrap();
    world
        .set_property(sound, "Volume", Value::Number(0.5))
        .unwrap();
    world
        .set_property(sound, "Looped", Value::Bool(true))
        .unwrap();
    world.set_parent(sound, Some(world.workspace())).unwrap();

    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        "scripts/audio.server.lua",
        "workspace.Chime:Play(); workspace.Chime:Stop()",
    );
    assert!(lines(&mut runtime).is_empty());
    assert_eq!(
        runtime.take_sound_actions(),
        [
            axcenity_audio::SoundAction::Play {
                instance: sound,
                sound_id: "assets/audio/chime.wav".to_owned(),
                volume: 0.5,
                looped: true,
            },
            axcenity_audio::SoundAction::Stop(sound),
        ]
    );
}

#[test]
fn sound_methods_keep_playing_in_step_and_ended_fires() {
    let mut world = World::new();
    let sound = world.create(ClassName::Sound).unwrap();
    world.set_name(sound, "Music").unwrap();
    world
        .set_property(sound, "SoundId", Value::String("music.ogg".to_owned()))
        .unwrap();
    world.set_parent(sound, Some(world.workspace())).unwrap();

    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        "scripts/audio.server.lua",
        "local music = workspace.Music
         music.Ended:Connect(function() print('ended', music.Playing) end)
         music:Play()
         print(music.Playing)
         music:Pause()
         print(music.Playing)
         music:Resume()
         print(music.Playing)",
    );
    assert_eq!(lines(&mut runtime), ["true", "false", "true"]);
    assert_eq!(property(&world, sound, "Playing"), Value::Bool(true));

    runtime.fire_sounds_ended(&mut world, &[sound]);
    assert_eq!(lines(&mut runtime), ["ended\tfalse"]);
}

#[test]
fn sound_play_rejects_invalid_volume_without_queueing_audio() {
    let mut world = World::new();
    let sound = world.create(ClassName::Sound).unwrap();
    world.set_name(sound, "Chime").unwrap();
    world
        .set_property(
            sound,
            "SoundId",
            Value::String("assets/audio/chime.wav".to_owned()),
        )
        .unwrap();
    world
        .set_property(sound, "Volume", Value::Number(-1.0))
        .unwrap();
    world.set_parent(sound, Some(world.workspace())).unwrap();

    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        "scripts/audio.server.lua",
        "workspace.Chime:Play()",
    );
    assert_eq!(
        lines(&mut runtime),
        [
            "error: scripts/audio.server.lua:1: Sound.Volume must be a finite number greater than or equal to 0"
        ]
    );
    assert!(runtime.take_sound_actions().is_empty());
}

#[test]
fn touched_events_play_effects_while_startup_music_loops() {
    let (mut world, pad, ball) = world_with_pad_and_ball();
    let chime = world.create(ClassName::Sound).unwrap();
    world.set_name(chime, "Chime").unwrap();
    world
        .set_property(
            chime,
            "SoundId",
            Value::String("assets/audio/chime.wav".to_owned()),
        )
        .unwrap();
    world
        .set_property(chime, "Looped", Value::Bool(false))
        .unwrap();
    world.set_parent(chime, Some(world.workspace())).unwrap();

    let music = world.create(ClassName::Sound).unwrap();
    world.set_name(music, "Music").unwrap();
    world
        .set_property(
            music,
            "SoundId",
            Value::String("assets/audio/ambient-loop.wav".to_owned()),
        )
        .unwrap();
    world
        .set_property(music, "Looped", Value::Bool(true))
        .unwrap();
    world.set_parent(music, Some(world.workspace())).unwrap();

    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        "scripts/audio.server.lua",
        r#"
        workspace.Music:Play()
        workspace.Pad.Touched:Connect(function()
            workspace.Chime:Play()
        end)
        "#,
    );
    assert_eq!(
        runtime.take_sound_actions(),
        [axcenity_audio::SoundAction::Play {
            instance: music,
            sound_id: "assets/audio/ambient-loop.wav".to_owned(),
            volume: 1.0,
            looped: true,
        }]
    );

    runtime.fire_touched(&mut world, &[(pad, ball)]);
    assert_eq!(
        runtime.take_sound_actions(),
        [axcenity_audio::SoundAction::Play {
            instance: chime,
            sound_id: "assets/audio/chime.wav".to_owned(),
            volume: 1.0,
            looped: false,
        }]
    );
}

#[test]
fn a_script_creates_and_updates_ui_instances_with_udim2() {
    let mut world = World::new();
    let output = output_of(
        &mut world,
        r#"
        local gui = Instance.new("ScreenGui")
        gui.Name = "Hud"
        gui.Parent = workspace

        local frame = Instance.new("Frame")
        frame.Position = UDim2.new(0.5, 12, 0.25, -4)
        frame.Size = UDim2.fromScale(0.5, 0.75)
        frame.Parent = gui

        local label = Instance.new("TextLabel")
        label.Text = "Ready"
        label.Parent = frame
        print(frame.Position.X.Scale, frame.Position.X.Offset, frame.Size.Y.Scale)
        "#,
    );
    assert_eq!(output, ["0.5\t12\t0.75"]);

    let gui = world.find_first_child(world.workspace(), "Hud").unwrap();
    let frame = world.find_first_child(gui, "Frame").unwrap();
    assert_eq!(
        property(&world, frame, "Position"),
        Value::UDim2(axcenity_core::UDim2::new(0.5, 12.0, 0.25, -4.0))
    );
    let label = world.find_first_child(frame, "TextLabel").unwrap();
    assert_eq!(
        property(&world, label, "Text"),
        Value::String("Ready".to_owned())
    );
}

#[test]
fn dangerous_functions_are_unavailable() {
    let mut world = World::new();
    let output = output_of(
        &mut world,
        r#"
        print(io, os.execute, os.exit, os.remove, os.getenv, debug, load, dofile,
              loadfile, package, string.dump)
        "#,
    );
    assert_eq!(output, [["nil"; 11].join("\t")]);
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
    // Compiled Luau chunks start with their bytecode version, a low byte.
    let output = output_of(&mut world, "\x06\x03");
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
        output[1].starts_with("error: scripts/broken.server.lua:3: attempt to index nil"),
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

/// The one error in `runtime`'s output: its text, and the `path:line` of
/// each call in its stack trace.
fn only_error(runtime: &mut ScriptRuntime) -> (String, Vec<String>) {
    let output = runtime.take_output();
    let [line] = output.as_slice() else {
        panic!("expected one line: {output:?}");
    };
    assert_eq!(line.kind, OutputKind::Error);
    let locations = line
        .trace
        .iter()
        .map(|frame| frame.split_once(": in ").expect(frame).0.to_owned())
        .collect();
    (line.text.clone(), locations)
}

#[test]
fn a_runtime_error_has_the_stack_of_calls_that_led_there() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        "scripts/test.server.lua",
        "local function explode()\n  error('boom')\nend\nlocal function light()\n  explode()\nend\nlight()",
    );
    let (text, trace) = only_error(&mut runtime);
    assert_eq!(text, "scripts/test.server.lua:2: boom");
    assert_eq!(
        trace,
        [
            "scripts/test.server.lua:2",
            "scripts/test.server.lua:5",
            "scripts/test.server.lua:7"
        ]
    );
}

#[test]
fn an_engine_error_has_a_stack_trace_without_engine_code() {
    let (mut world, _) = world_with_block();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        "scripts/test.server.lua",
        "local function grow()\n  workspace.Block.Size = 5\nend\ngrow()",
    );
    let (text, trace) = only_error(&mut runtime);
    assert_eq!(
        text,
        "scripts/test.server.lua:2: Part.Size must be a Vector3, not a number"
    );
    assert_eq!(
        trace,
        ["scripts/test.server.lua:2", "scripts/test.server.lua:4"]
    );
}

#[test]
fn an_error_in_another_script_names_the_failing_line_and_the_caller() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    let module = world.create(ClassName::ModuleScript).unwrap();
    world.set_name(module, "Fuse").unwrap();
    world
        .set_parent(module, Some(world.replicated_storage()))
        .unwrap();
    runtime.start_script(
        &mut world,
        module,
        "scripts/Fuse.module.lua",
        "local Fuse = {}\nfunction Fuse.light()\n  error('boom')\nend\nreturn Fuse",
    );
    run(
        &mut runtime,
        &mut world,
        "scripts/test.server.lua",
        "local Fuse = require(game.ReplicatedStorage.Fuse)\nFuse.light()",
    );
    let (text, trace) = only_error(&mut runtime);
    assert_eq!(text, "scripts/Fuse.module.lua:3: boom");
    assert_eq!(
        trace,
        ["scripts/Fuse.module.lua:3", "scripts/test.server.lua:2"]
    );
}

#[test]
fn a_syntax_error_and_print_have_no_stack_trace() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        "scripts/test.server.lua",
        "print('ok')",
    );
    run(
        &mut runtime,
        &mut world,
        "scripts/bad.server.lua",
        "local = 5",
    );
    let output = runtime.take_output();
    assert_eq!(output.len(), 2, "{output:?}");
    assert!(
        output.iter().all(|line| line.trace.is_empty()),
        "{output:?}"
    );
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
            "1\t2\t3\t5\t0, 1, 0",
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
    assert_eq!(output[0], "1");
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
        ["Enum.Material.Metal\ttrue\t1", "Neon\tEnum.PartType.Wedge"]
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

/// Steps the scheduler `count` times by `seconds`, like a game running at a
/// steady frame rate.
fn step_frames(runtime: &mut ScriptRuntime, world: &mut World, seconds: f64, count: usize) {
    for _ in 0..count {
        runtime.step(world, seconds);
    }
}

#[test]
fn a_forever_loop_with_task_wait_toggles_a_color_once_per_second() {
    let (mut world, block) = world_with_block();
    let mut runtime = ScriptRuntime::new();
    // `start_script` returning at all shows the loop doesn't block the caller.
    run(
        &mut runtime,
        &mut world,
        "scripts/blink.server.lua",
        r#"
        local block = workspace.Block
        local red, blue = Color3.new(1, 0, 0), Color3.new(0, 0, 1)
        block.Color = red
        while true do
            task.wait(1)
            if block.Color == red then
                block.Color = blue
            else
                block.Color = red
            end
        end
        "#,
    );

    // 3.5 seconds at 60 frames per second, noting when the color changes.
    let frame = 1.0 / 60.0;
    let mut color = property(&world, block, "Color");
    let mut changes = Vec::new();
    for i in 1..=210 {
        runtime.step(&mut world, frame);
        let now = property(&world, block, "Color");
        if now != color {
            changes.push(f64::from(i) * frame);
            color = now;
        }
    }
    assert_eq!(lines(&mut runtime), Vec::<String>::new());
    assert_eq!(changes.len(), 3, "{changes:?}");
    for (second, time) in [1.0, 2.0, 3.0].iter().zip(&changes) {
        // Each change happens on the first frame at or after the whole second.
        assert!(
            *time >= second - 1e-9 && *time < second + frame + 1e-9,
            "{changes:?}"
        );
    }
    assert_eq!(color, Value::Color3(Color3::new(0.0, 0.0, 1.0)));
}

#[test]
fn task_wait_returns_the_time_that_passed_on_a_simulated_clock() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        "scripts/test.server.lua",
        "print(task.wait(1))\nprint(task.wait(0.5))\nprint(task.wait())",
    );

    // Quarter seconds add up exactly in floating point.
    step_frames(&mut runtime, &mut world, 0.25, 3);
    assert_eq!(lines(&mut runtime), Vec::<String>::new());
    runtime.step(&mut world, 0.25);
    assert_eq!(lines(&mut runtime), ["1"]);
    step_frames(&mut runtime, &mut world, 0.25, 2);
    assert_eq!(lines(&mut runtime), ["0.5"]);
    // `task.wait()` with no time waits for the next step.
    runtime.step(&mut world, 0.25);
    assert_eq!(lines(&mut runtime), ["0.25"]);
}

#[test]
fn task_wait_is_accurate_to_a_frame_at_60_fps() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        "scripts/test.server.lua",
        "for _ = 1, 10 do print(task.wait(0.1)) end",
    );
    let frame = 1.0 / 60.0;
    step_frames(&mut runtime, &mut world, frame, 120);
    let waited: Vec<f64> = lines(&mut runtime)
        .iter()
        .map(|line| line.parse().unwrap())
        .collect();
    assert_eq!(waited.len(), 10);
    for seconds in waited {
        assert!(
            (0.1 - 1e-9..0.1 + frame + 1e-9).contains(&seconds),
            "{seconds}"
        );
    }
}

#[test]
fn task_wait_with_no_time_resumes_once_per_step() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        "scripts/test.server.lua",
        "local frames = 0\nwhile true do task.wait() frames = frames + 1 print(frames) end",
    );
    step_frames(&mut runtime, &mut world, 1.0 / 60.0, 3);
    assert_eq!(lines(&mut runtime), ["1", "2", "3"]);
}

#[test]
fn task_spawn_and_task_delay_run_in_a_predictable_order() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        "scripts/test.server.lua",
        r#"
        print("start")
        task.delay(0.5, print, "delay 0.5, scheduled first")
        task.delay(0, print, "delay 0")
        task.spawn(function(how)
            print("spawn " .. how)
            task.wait()
            print("spawn after task.wait()")
        end, "runs right away")
        task.delay(0.5, print, "delay 0.5, scheduled second")
        print("end of script")
        "#,
    );
    // `task.spawn` runs straight away; delays wait for a step, however short.
    assert_eq!(
        lines(&mut runtime),
        ["start", "spawn runs right away", "end of script"]
    );
    runtime.step(&mut world, 0.25);
    // Due at the same time, so they resume in the order they were scheduled.
    assert_eq!(lines(&mut runtime), ["delay 0", "spawn after task.wait()"]);
    runtime.step(&mut world, 0.25);
    assert_eq!(
        lines(&mut runtime),
        ["delay 0.5, scheduled first", "delay 0.5, scheduled second"]
    );
}

#[test]
fn an_error_in_a_spawned_task_is_reported_and_the_parent_keeps_running() {
    let (mut world, _) = world_with_block();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        "scripts/test.server.lua",
        r#"task.spawn(function()
    error("spawned problem")
end)
task.spawn(function() workspace.Block.Size = 5 end)
print("parent continues")
task.delay(0.1, function()
    local missing = nil
    missing.Position = 1
end)
task.wait(0.2)
print("parent still running")"#,
    );
    assert_eq!(
        lines(&mut runtime),
        [
            "error: scripts/test.server.lua:2: spawned problem",
            "error: scripts/test.server.lua:4: Part.Size must be a Vector3, not a number",
            "parent continues",
        ]
    );
    runtime.step(&mut world, 0.1);
    let output = lines(&mut runtime);
    assert_eq!(output.len(), 1, "{output:?}");
    assert!(
        output[0].starts_with("error: scripts/test.server.lua:8: attempt to index nil"),
        "{output:?}"
    );
    runtime.step(&mut world, 0.1);
    assert_eq!(lines(&mut runtime), ["parent still running"]);
}

#[test]
fn one_waiting_script_does_not_hold_up_the_others() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        "scripts/slow.server.lua",
        "task.wait(1)\nprint('slow done')",
    );
    run(
        &mut runtime,
        &mut world,
        "scripts/fast.server.lua",
        "print('fast done')",
    );
    assert_eq!(lines(&mut runtime), ["fast done"]);
    runtime.step(&mut world, 1.0);
    assert_eq!(lines(&mut runtime), ["slow done"]);
}

#[test]
fn task_wait_works_inside_pcall_and_coroutines() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        "scripts/test.server.lua",
        r#"local wrapped = coroutine.wrap(function()
    task.wait(0.1)
    print("coroutine resumed")
end)
wrapped()
print("after the coroutine paused")
print(pcall(function()
    task.wait(0.1)
    error("after waiting")
end))"#,
    );
    assert_eq!(lines(&mut runtime), ["after the coroutine paused"]);
    runtime.step(&mut world, 0.1);
    assert_eq!(
        lines(&mut runtime),
        [
            "coroutine resumed",
            "false\tscripts/test.server.lua:9: after waiting"
        ]
    );
}

#[test]
fn task_functions_explain_bad_arguments() {
    let mut world = World::new();
    let output = output_of(
        &mut world,
        r#"
        print(pcall(task.wait, "soon"))
        print(pcall(task.spawn, 5))
        print(pcall(task.delay, 1))
        print(pcall(task.delay, "1", print))
        "#,
    );
    let expected = [
        "task.wait takes a number of seconds, not a string",
        "task.spawn takes a function, not a number",
        "task.delay takes a function, not a nil",
        "task.delay takes a number of seconds, not a string",
    ];
    assert_eq!(output.len(), expected.len(), "{output:?}");
    for (line, expected) in output.iter().zip(expected) {
        assert!(
            line.starts_with("false\t") && line.ends_with(expected),
            "{line:?} should end with {expected:?}"
        );
    }
}

/// A world with two Parts in the Workspace, named `Pad` and `Ball`.
fn world_with_pad_and_ball() -> (World, InstanceId, InstanceId) {
    let (mut world, pad) = world_with_block();
    world.set_name(pad, "Pad").unwrap();
    let ball = world.create(ClassName::Part).unwrap();
    world.set_name(ball, "Ball").unwrap();
    world.set_parent(ball, Some(world.workspace())).unwrap();
    (world, pad, ball)
}

#[test]
fn touched_fires_on_both_parts_with_the_other_part() {
    let (mut world, pad, ball) = world_with_pad_and_ball();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        "scripts/touch.server.lua",
        r#"
        workspace.Pad.Touched:Connect(function(other)
            print("Pad touched by " .. other.Name)
            workspace.Pad.Color = Color3.new(0, 1, 0)
        end)
        workspace.Ball.Touched:Connect(function(other)
            print("Ball touched by " .. other.Name)
        end)
        "#,
    );
    assert_eq!(lines(&mut runtime), Vec::<String>::new());

    runtime.fire_touched(&mut world, &[(pad, ball)]);
    assert_eq!(
        lines(&mut runtime),
        ["Pad touched by Ball", "Ball touched by Pad"]
    );
    assert_eq!(
        property(&world, pad, "Color"),
        Value::Color3(Color3::new(0.0, 1.0, 0.0))
    );
}

#[test]
fn touched_fires_once_per_reported_touch() {
    let (mut world, pad, ball) = world_with_pad_and_ball();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        "scripts/touch.server.lua",
        r#"
        local count = 0
        workspace.Pad.Touched:Connect(function()
            count = count + 1
            print(count)
        end)
        "#,
    );
    // Frames with no new touch fire nothing.
    for _ in 0..5 {
        runtime.step(&mut world, 1.0 / 60.0);
        runtime.fire_touched(&mut world, &[]);
    }
    assert_eq!(lines(&mut runtime), Vec::<String>::new());
    runtime.fire_touched(&mut world, &[(pad, ball)]);
    runtime.fire_touched(&mut world, &[(ball, pad)]);
    assert_eq!(lines(&mut runtime), ["1", "2"]);
}

#[test]
fn disconnect_stops_further_calls_to_the_handler() {
    let (mut world, pad, ball) = world_with_pad_and_ball();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        "scripts/touch.server.lua",
        r#"
        local connection
        connection = workspace.Pad.Touched:Connect(function(other)
            print("touched, connected: " .. tostring(connection.Connected))
            connection:Disconnect()
            connection:Disconnect() -- twice is harmless
            print("now connected: " .. tostring(connection.Connected))
        end)
        "#,
    );
    runtime.fire_touched(&mut world, &[(pad, ball)]);
    assert_eq!(
        lines(&mut runtime),
        ["touched, connected: true", "now connected: false"]
    );
    runtime.fire_touched(&mut world, &[(pad, ball)]);
    assert_eq!(lines(&mut runtime), Vec::<String>::new());
}

#[test]
fn a_handler_can_disconnect_others_that_have_not_run_yet() {
    let (mut world, pad, ball) = world_with_pad_and_ball();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        "scripts/touch.server.lua",
        r#"
        local second
        workspace.Pad.Touched:Connect(function()
            print("first")
            second:Disconnect()
        end)
        second = workspace.Pad.Touched:Connect(function() print("second") end)
        workspace.Pad.Touched:Connect(function() print("third") end)
        "#,
    );
    runtime.fire_touched(&mut world, &[(pad, ball)]);
    assert_eq!(lines(&mut runtime), ["first", "third"]);
}

#[test]
fn a_handler_that_waits_does_not_block_other_handlers_or_the_frame() {
    let (mut world, pad, ball) = world_with_pad_and_ball();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        "scripts/touch.server.lua",
        r#"
        workspace.Pad.Touched:Connect(function(other)
            print("slow handler starts")
            task.wait(1)
            print("slow handler done, touched by " .. other.Name)
        end)
        workspace.Pad.Touched:Connect(function()
            print("fast handler")
        end)
        "#,
    );
    // `fire_touched` returning at all shows the frame isn't held up.
    runtime.fire_touched(&mut world, &[(pad, ball)]);
    assert_eq!(lines(&mut runtime), ["slow handler starts", "fast handler"]);
    // A second touch while the first call still waits runs it again, separately.
    runtime.fire_touched(&mut world, &[(pad, ball)]);
    assert_eq!(lines(&mut runtime), ["slow handler starts", "fast handler"]);

    runtime.step(&mut world, 0.5);
    assert_eq!(lines(&mut runtime), Vec::<String>::new());
    runtime.step(&mut world, 0.5);
    assert_eq!(
        lines(&mut runtime),
        [
            "slow handler done, touched by Ball",
            "slow handler done, touched by Ball"
        ]
    );
}

#[test]
fn an_error_in_a_handler_is_reported_and_other_handlers_still_run() {
    let (mut world, pad, ball) = world_with_pad_and_ball();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        "scripts/touch.server.lua",
        r#"workspace.Pad.Touched:Connect(function()
    error("oops")
end)
workspace.Pad.Touched:Connect(function() print("still runs") end)"#,
    );
    let expected = ["error: scripts/touch.server.lua:2: oops", "still runs"];
    runtime.fire_touched(&mut world, &[(pad, ball)]);
    assert_eq!(lines(&mut runtime), expected);
    // The handler stays connected, as in Roblox.
    runtime.fire_touched(&mut world, &[(pad, ball)]);
    assert_eq!(lines(&mut runtime), expected);
}

#[test]
fn touches_with_a_destroyed_part_are_skipped_and_its_handlers_dropped() {
    let (mut world, pad, ball) = world_with_pad_and_ball();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        "scripts/touch.server.lua",
        r#"
        local connection = workspace.Ball.Touched:Connect(function() print("ball touched") end)
        workspace.Pad.Touched:Connect(function() print("pad touched") end)
        task.delay(0.1, function() print(connection.Connected) end)
        "#,
    );
    world.destroy(ball).unwrap();
    runtime.fire_touched(&mut world, &[(pad, ball)]);
    runtime.step(&mut world, 0.1);
    assert_eq!(lines(&mut runtime), ["false"]);
}

#[test]
fn events_explain_mistakes() {
    let (mut world, _) = world_with_block();
    let output = output_of(
        &mut world,
        r#"
        print(workspace.Block.Touched)
        print(workspace.Block.Touched == workspace.Block.Touched)
        print(workspace.Block.Touched:Connect(print))
        print(pcall(function() workspace.Block.Touched = 5 end))
        print(pcall(function() workspace.Block.Touched:Connect("print") end))
        print(pcall(function() return workspace.Touched end))
        "#,
    );
    let expected = [
        "Signal Touched",
        "true",
        "Connection",
        "Part.Touched is an event and can't be set; use Touched:Connect(function) to react to it",
        "Connect takes a function, not a string",
        "Touched is not a valid member of Workspace \"Workspace\"",
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
fn a_script_can_retexture_a_mesh_part_with_a_surface_appearance() {
    let mut world = World::new();
    let output = output_of(
        &mut world,
        r#"
        local part = Instance.new("MeshPart")
        part.Parent = workspace
        local look = Instance.new("SurfaceAppearance")
        look.MaterialName = "Wood"
        look.RoughnessMap = "assets/textures/wood_orm.png"
        look.RoughnessChannel = Enum.TextureChannel.Green
        look.MetalnessChannel = "Blue"
        look.Parent = part
        print(look.RoughnessChannel, look.MetalnessChannel, look.InvertRoughness)
        "#,
    );
    assert_eq!(
        output,
        ["Enum.TextureChannel.Green\tEnum.TextureChannel.Blue\tfalse"]
    );
}

#[test]
fn emitters_emit_bursts_and_take_particle_types() {
    let mut world = World::new();
    let emitter = world.create(ClassName::ParticleEmitter).unwrap();
    world.set_name(emitter, "Sparks").unwrap();
    world.set_parent(emitter, Some(world.workspace())).unwrap();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        "scripts/burst.server.lua",
        "local sparks = workspace.Sparks
         sparks:Emit(12)
         sparks:Emit(5000)
         sparks.ParticleType = Enum.ParticleType.Fire
         print(sparks.ParticleType == Enum.ParticleType.Fire)
         sparks.ParticleType = \"Smoke\"
         print(pcall(sparks.Emit, sparks, -1))",
    );
    let output = lines(&mut runtime);
    assert_eq!(output[0], "true");
    assert!(output[1].starts_with("false"), "{}", output[1]);
    assert_eq!(
        runtime.take_particle_emits(),
        [(emitter, 12), (emitter, 1000)]
    );
    assert_eq!(
        property(&world, emitter, "ParticleType"),
        Value::ParticleType(axcenity_core::ParticleType::Smoke)
    );
}

#[test]
fn animator_parameters_are_set_and_read_by_kind() {
    let mut world = World::new();
    let animator = world.create(ClassName::Animator).unwrap();
    world.set_parent(animator, Some(world.workspace())).unwrap();
    world
        .animator_mut(animator)
        .unwrap()
        .parameters
        .insert("Speed".into(), axcenity_core::Parameter::Number(0.0));
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        "scripts/params.server.lua",
        "local animator = workspace.Animator
         animator:SetNumber('Speed', 4.5)
         print(animator:GetNumber('Speed'))
         animator:SetBool('IsGrounded', true)
         print(animator:GetBool('IsGrounded'))
         animator:SetTrigger('Jump')
         print(pcall(animator.SetBool, animator, 'Speed', true))
         print(pcall(animator.GetBool, animator, 'Nothing'))
         print(pcall(animator.SetNumber, animator, 'Speed', 'fast'))",
    );
    let output = lines(&mut runtime);
    assert_eq!(output[0], "4.5");
    assert_eq!(output[1], "true");
    assert!(output[2].contains("is a Number parameter"), "{}", output[2]);
    assert!(
        output[3].contains("has no parameter named `Nothing`"),
        "{}",
        output[3]
    );
    assert!(output[4].contains("needs a number"), "{}", output[4]);
    let live = world.animator(animator).unwrap();
    assert_eq!(
        live.parameters["Jump"],
        axcenity_core::Parameter::Trigger(true)
    );
}

#[test]
fn animators_take_play_requests_and_report_their_state() {
    let mut world = World::new();
    let animator = world.create(ClassName::Animator).unwrap();
    world.set_parent(animator, Some(world.workspace())).unwrap();
    world.animator_mut(animator).unwrap().current_state = "Idle".into();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        "scripts/anim.server.lua",
        "local animator = workspace.Animator
         print(animator.CurrentState)
         animator.StateChanged:Connect(function(state, previous)
             print(previous .. ' -> ' .. state)
         end)
         animator:Play('Walk', 0.5)
         print(pcall(function() animator.CurrentState = 'Run' end))
         print(pcall(animator.Play, animator, 3))",
    );
    runtime.fire_instance_event(
        &mut world,
        animator,
        "StateChanged",
        &["Walk".into(), "Idle".into()],
    );
    let output = lines(&mut runtime);
    assert_eq!(output[0], "Idle");
    assert!(
        output[1].starts_with("false") && output[1].contains("Play"),
        "{}",
        output[1]
    );
    assert!(output[2].starts_with("false"), "{}", output[2]);
    assert_eq!(output[3], "Idle -> Walk");
    let request = world.animator(animator).unwrap().play.clone().unwrap();
    assert_eq!((request.state.as_str(), request.blend), ("Walk", Some(0.5)));
}

#[test]
fn load_animation_gives_a_track_to_play_stop_and_hear() {
    let mut world = World::new();
    let door = world.create(ClassName::Part).unwrap();
    world.set_name(door, "Door").unwrap();
    world.set_parent(door, Some(world.workspace())).unwrap();
    let animator = world.create(ClassName::Animator).unwrap();
    world.set_parent(animator, Some(door)).unwrap();
    let animation = world.create(ClassName::Animation).unwrap();
    world.set_name(animation, "Open").unwrap();
    world.set_parent(animation, Some(door)).unwrap();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        "scripts/door.server.lua",
        "local door = workspace.Door
         local track = door.Animator:LoadAnimation(door.Open)
         print(track.ClassName, track.Animation == door.Open, track.IsPlaying)
         track:GetMarkerReachedSignal('Opened'):Connect(function(name) print('marker', name) end)
         track.Stopped:Connect(function() print('stopped') end)
         track:Play(0.2, 0.5, 2)
         print(track.IsPlaying, track.Speed, #door.Animator:GetPlayingAnimationTracks())
         track.Looped = true
         track.TimePosition = 0.25
         print(track.Looped, track.TimePosition)
         print(pcall(function() track.Speed = 3 end))
         print(pcall(door.Animator.LoadAnimation, door.Animator, door))
         print(pcall(Instance.new, 'AnimationTrack'))
         track:Stop(0)
         print(track.IsPlaying, track.WeightCurrent)",
    );
    let track = world.animator(animator).unwrap().tracks[0];
    runtime.fire_instance_event(&mut world, track, "KeyframeReached", &["Opened".into()]);
    runtime.fire_instance_event(&mut world, track, "KeyframeReached", &["Other".into()]);
    runtime.fire_instance_event(&mut world, track, "Stopped", &[]);
    let output = lines(&mut runtime);
    assert_eq!(output[0], "AnimationTrack\ttrue\tfalse");
    assert_eq!(output[1], "true\t2\t1");
    assert_eq!(output[2], "true\t0.25");
    assert!(output[3].contains("AdjustSpeed"), "{}", output[3]);
    assert!(output[4].contains("needs an Animation"), "{}", output[4]);
    assert!(output[5].contains("LoadAnimation"), "{}", output[5]);
    assert_eq!(output[6], "false\t0");
    assert_eq!(&output[7..], ["marker\tOpened", "stopped"]);
    let state = world.animation_track(track).unwrap();
    assert!(state.stopping && state.order == 1);
    assert_eq!(state.animation, animation);
}

#[test]
fn animation_play_still_works_and_says_once_what_the_new_way_is() {
    let mut world = World::new();
    let animation = world.create(ClassName::Animation).unwrap();
    world
        .set_parent(animation, Some(world.workspace()))
        .unwrap();
    let output = output_of(
        &mut world,
        "local a = workspace.Animation
         a:Play()
         a:Stop()
         a:Play()",
    );
    assert_eq!(output.len(), 1, "{output:?}");
    assert!(output[0].contains("LoadAnimation"), "{}", output[0]);
    assert!(world.property_animation(animation).unwrap().play);
}

#[test]
fn workspace_raycast_hits_a_part_including_a_sensor_and_does_not_move_it() {
    let (mut world, block) = world_with_block();
    let before = property(&world, block, "Position");
    let output = output_of(
        &mut world,
        r#"
        local hit = workspace:Raycast(Vector3.new(0, 10, 0), Vector3.new(0, -20, 0))
        print(hit.Instance.Name, hit.Distance, hit.Position.Y, hit.Normal.Y)
        local miss = workspace:Raycast(Vector3.new(100, 10, 0), Vector3.new(0, -20, 0))
        print(miss)
        workspace.Block.CanCollide = false
        local sensor = workspace:Raycast(Vector3.new(0, 10, 0), Vector3.new(0, -20, 0))
        print(sensor.Instance.Name)
        "#,
    );
    assert_eq!(output.len(), 3, "{output:?}");
    assert!(output[0].starts_with("Block\t"), "{}", output[0]);
    assert_eq!(output[1], "nil");
    assert_eq!(output[2], "Block");
    assert_eq!(property(&world, block, "Position"), before);
}

#[test]
fn workspace_raycast_keeps_the_last_casts_for_the_editor_overlay() {
    let (mut world, _) = world_with_block();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        "scripts/test.server.lua",
        "workspace:Raycast(Vector3.new(0, 10, 0), Vector3.new(0, -20, 0))
         workspace:Raycast(Vector3.new(100, 10, 0), Vector3.new(0, -20, 0))",
    );
    let rays = runtime.debug_rays();
    assert_eq!(rays.len(), 2);
    assert!(rays[0].hit.is_some());
    assert!(rays[1].hit.is_none());
}
