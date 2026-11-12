//! Attempts to escape the sandbox, which must all fail, and `require`.

use axcenity_core::{ClassName, InstanceId, World};

use super::*;

/// Makes a Script, LocalScript or ModuleScript Instance like the project
/// loader does, named after the file, and starts `source` as it. The path's
/// ending says which (`.client.lua`, `.module.lua`, otherwise a Script).
fn start(runtime: &mut ScriptRuntime, world: &mut World, path: &str, source: &str) -> InstanceId {
    let (class, service, suffix) = if path.ends_with(".client.lua") {
        let service = world.starter_player_scripts();
        (ClassName::LocalScript, service, ".client.lua")
    } else if path.ends_with(".module.lua") {
        let service = world.replicated_storage();
        (ClassName::ModuleScript, service, ".module.lua")
    } else {
        let service = world.server_script_service();
        (ClassName::Script, service, ".server.lua")
    };
    let script = world.create(class).unwrap();
    let name = path.rsplit('/').next().unwrap().trim_end_matches(suffix);
    world.set_name(script, name).unwrap();
    world.set_parent(script, Some(service)).unwrap();
    runtime.start_script(world, script, path, source);
    script
}

fn lines(runtime: &mut ScriptRuntime) -> Vec<String> {
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

/// Runs one Script in a fresh runtime and returns its output.
fn output_of(source: &str) -> Vec<String> {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(&mut runtime, &mut world, "scripts/test.server.lua", source);
    lines(&mut runtime)
}

// --- Escape attempts ---------------------------------------------------------

#[test]
fn every_way_out_of_the_standard_library_is_missing() {
    let names = [
        "io",
        "debug",
        "load",
        "loadstring",
        "dofile",
        "loadfile",
        "package",
        "module",
        "getfenv",
        "setfenv",
        "newproxy",
        "os.execute",
        "os.exit",
        "os.getenv",
        "os.remove",
        "os.rename",
        "os.tmpname",
        "os.setlocale",
        "string.dump",
        "coroutine.dump",
        "table.dump",
    ];
    let checks: Vec<String> = names.iter().map(|name| format!("type({name})")).collect();
    let output = output_of(&format!("print({})", checks.join(", ")));
    assert_eq!(output, [vec!["nil"; names.len()].join("\t")]);
}

#[test]
fn the_string_metatable_is_locked() {
    let output = output_of(
        r#"
        print(getmetatable(""))
        print(("x").dump, ("x").upper == string.upper)
        print(pcall(setmetatable, "", {}))
        "#,
    );
    assert_eq!(output[0], "The metatable is locked");
    // Methods still work, but `dump` isn't one of them.
    assert_eq!(output[1], "nil\ttrue");
    assert!(output[2].starts_with("false\t"), "{output:?}");
}

#[test]
fn the_libraries_are_read_only_as_in_luau() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/a.server.lua",
        r#"
        print(pcall(function() string.upper = function() return "hacked" end end))
        print(pcall(function() math.sqrt = nil end))
        print(pcall(function() os.exit = function() end end))
        print(("a"):upper(), string.upper("a"), math.sqrt(16), table.isfrozen(math))
        "#,
    );
    let output = lines(&mut runtime);
    for line in &output[..3] {
        assert!(
            line.starts_with("false\t") && line.contains("attempt to modify a readonly table"),
            "{output:?}"
        );
    }
    assert_eq!(output[3], "A\tA\t4\ttrue");
}

#[test]
fn a_script_can_still_replace_a_library_with_its_own_table() {
    // `_G` and plain globals are the script's own, so a script may swap
    // in its own `math`, and calls through the name use it.
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/a.server.lua",
        r#"
        local real = math
        math = setmetatable({ sqrt = function() return "mine" end }, { __index = real })
        typeof = function() return "also mine" end
        print(math.sqrt(16), math.floor(2.5), typeof(1))
        "#,
    );
    start(
        &mut runtime,
        &mut world,
        "scripts/b.server.lua",
        "print(math.sqrt(16), typeof(Vector3.new()))",
    );
    assert_eq!(lines(&mut runtime), ["mine\t2\talso mine", "4\tVector3"]);
}

#[test]
fn engine_object_metatables_cannot_be_changed() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/a.server.lua",
        r#"
        for _, value in ipairs({ workspace, Vector3.new(), Color3.new(), Enum, Enum.Material,
                                 Enum.Material.Neon, Instance.new("Part").Touched }) do
            local mt = getmetatable(value)
            if type(mt) == "table" then
                pcall(function() mt.__index = function() return "hacked" end end)
                pcall(rawset, mt, "__index", function() return "hacked" end)
                pcall(setmetatable, mt, { __index = function() return "hacked" end })
            end
        end
        "#,
    );
    start(
        &mut runtime,
        &mut world,
        "scripts/b.server.lua",
        r#"print(workspace.Name, Vector3.new(1, 2, 3).X, Color3.new(1, 0, 0).R, Enum.Material.Neon.Name)"#,
    );
    let output = lines(&mut runtime);
    assert_eq!(
        output.last().unwrap(),
        "Workspace\t1\t1\tNeon",
        "{output:?}"
    );
}

#[test]
fn bytecode_cannot_be_made_or_run() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    // Compiled Luau chunks start with their bytecode version, a low byte.
    start(&mut runtime, &mut world, "scripts/m.module.lua", "\x06\x03");
    start(
        &mut runtime,
        &mut world,
        "scripts/a.server.lua",
        r#"
        print(string.dump, ("x").dump)
        print(pcall(require, game.ReplicatedStorage.m))
        "#,
    );
    let output = lines(&mut runtime);
    assert_eq!(output[0], "nil\tnil");
    assert!(
        output[1].starts_with("false\t") && output[1].contains("binary chunk"),
        "{output:?}"
    );
}

#[test]
fn the_real_global_table_is_out_of_reach() {
    let output = output_of(
        r#"
        print(_G.print == print, getmetatable(_G), rawget(_G, "io"))
        -- Errors from the engine reach scripts as plain strings, not objects
        -- with methods that could lead somewhere.
        local ok, err = pcall(Instance.new, 5)
        print(ok, type(err))
        -- Every engine function is a Rust function or engine Lua with no way
        -- in: without `debug`, nothing can read their upvalues.
        print(type(task.wait), type(print), type(require))
        "#,
    );
    assert_eq!(
        output,
        [
            "true\tnil\tnil",
            "false\tstring",
            "function\tfunction\tfunction"
        ]
    );
}

#[test]
fn finalizers_cannot_be_made() {
    // Lua runs `__gc` with hooks off, so the watchdog couldn't stop one.
    let output = output_of(
        r#"
        local ok, err = pcall(setmetatable, {}, { __gc = function() while true do end end })
        print(ok, err)
        print(setmetatable({}, { __index = { x = 1 } }).x)
        setmetatable({}, { __gc = print })
        "#,
    );
    assert_eq!(output.len(), 3, "{output:?}");
    assert!(
        output[0].starts_with("false\t") && output[0].contains("__gc isn't supported in scripts"),
        "{output:?}"
    );
    assert_eq!(output[1], "1");
    assert!(
        output[2].starts_with("error: scripts/test.server.lua:5: __gc isn't supported"),
        "{output:?}"
    );
}

#[test]
fn coroutine_resume_gives_clean_messages() {
    let output = output_of(
        r#"
        local co = coroutine.create(function() workspace.Nope.Name = "x" end)
        print(coroutine.resume(co))
        "#,
    );
    assert_eq!(
        output,
        ["false\tscripts/test.server.lua:2: Nope is not a valid member of Workspace \"Workspace\""]
    );
}

#[test]
fn collectgarbage_only_counts() {
    let output = output_of(
        r#"
        print(type(collectgarbage("count")), collectgarbage("count") > 0)
        for _, option in ipairs({ "collect", "stop", "restart", "step", "incremental",
                                  "generational", "isrunning" }) do
            local ok = pcall(collectgarbage, option)
            print(option, ok)
        end
        print(pcall(collectgarbage))
        "#,
    );
    assert_eq!(output[0], "number\ttrue");
    for line in &output[1..8] {
        assert!(line.ends_with("\tfalse"), "{output:?}");
    }
    assert!(
        output[8]
            .starts_with("false\tscripts/test.server.lua:8: collectgarbage only takes \"count\""),
        "{output:?}"
    );
}

#[test]
fn os_keeps_only_the_clock_and_rejects_bad_date_formats() {
    let output = output_of(
        r#"
        local names = {}
        for name in pairs(os) do table.insert(names, name) end
        table.sort(names)
        print(table.concat(names, ","))
        print((pcall(os.date, "%Q")))
        "#,
    );
    assert_eq!(output, ["clock,date,difftime,time", "false"]);
}

#[test]
fn bit32_and_buffer_work_and_buffers_stay_in_bounds() {
    let output = output_of(
        r#"
        print(bit32.band(12, 10), bit32.bor(12, 10), bit32.bxor(12, 10), bit32.lshift(1, 4))
        local b = buffer.create(4)
        buffer.writeu32(b, 0, 0xDEADBEEF)
        print(buffer.len(b), buffer.readu8(b, 0), buffer.readu32(b, 0) == 0xDEADBEEF)
        print(buffer.tostring(buffer.fromstring("hi")))
        print((pcall(buffer.readu32, b, 2)))
        print((pcall(buffer.writeu8, b, -1, 0)))
        "#,
    );
    assert_eq!(
        output,
        ["8\t14\t6\t16", "4\t239\ttrue", "hi", "false", "false"]
    );
}

#[test]
fn typeof_names_engine_values() {
    let output = output_of(
        r#"
        local part = Instance.new("Part")
        print(typeof(part), typeof(workspace), typeof(game))
        print(typeof(Vector3.new()), typeof(Color3.new()), typeof(UDim2.new()), typeof(UDim2.new().X))
        print(typeof(Enum.Material.Neon), typeof(Enum.Material), typeof(Enum))
        print(typeof(part.Touched), typeof(part.Touched:Connect(function() end)))
        print(typeof(game:GetService("RunService")))
        print(typeof(1), typeof("a"), typeof(nil), typeof({}), typeof(print), typeof(buffer.create(1)))
        "#,
    );
    assert_eq!(
        output,
        [
            "Instance\tInstance\tInstance",
            "Vector3\tColor3\tUDim2\tUDim",
            "EnumItem\tEnum\tEnums",
            "Signal\tConnection",
            "RunService",
            "number\tstring\tnil\ttable\tfunction\tbuffer",
        ]
    );
}

#[test]
fn a_local_script_cannot_reach_the_servers_globals() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/a.server.lua",
        "secret = 1\n_G.secret = 1\nmath = { secret = 1 }",
    );
    start(
        &mut runtime,
        &mut world,
        "scripts/b.client.lua",
        "print(secret, _G.secret, math.secret)",
    );
    assert_eq!(lines(&mut runtime), ["nil\tnil\tnil"]);
}

// --- require -----------------------------------------------------------------

#[test]
fn require_loads_a_module_once_per_side_and_shares_its_value() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/shared/Counter.module.lua",
        r#"
        print("loading Counter in", script.Name)
        local Counter = { count = 0 }
        function Counter.add()
            Counter.count = Counter.count + 1
            return Counter.count
        end
        return Counter
        "#,
    );
    let use_it = "local Counter = require(game.ReplicatedStorage.Counter)\nprint(Counter.add())";
    start(&mut runtime, &mut world, "scripts/a.server.lua", use_it);
    start(&mut runtime, &mut world, "scripts/b.server.lua", use_it);
    start(&mut runtime, &mut world, "scripts/c.client.lua", use_it);
    // One table on the server, shared by `a` and `b`; the client has its own.
    assert_eq!(
        lines(&mut runtime),
        [
            "loading Counter in\tCounter",
            "1",
            "2",
            "loading Counter in\tCounter",
            "1"
        ]
    );
}

#[test]
fn a_module_has_its_own_globals() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/m.module.lua",
        "hidden = 1\nreturn { seen = secret }",
    );
    start(
        &mut runtime,
        &mut world,
        "scripts/a.server.lua",
        "secret = 2\nlocal m = require(game.ReplicatedStorage.m)\nprint(m.seen, hidden)",
    );
    assert_eq!(lines(&mut runtime), ["nil\tnil"]);
}

#[test]
fn require_takes_only_module_scripts_never_paths_or_libraries() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(&mut runtime, &mut world, "scripts/m.module.lua", "return 1");
    start(
        &mut runtime,
        &mut world,
        "scripts/a.server.lua",
        r#"
        local attempts = {
            "os", "io", "debug", "m", "scripts/m.module.lua", "../../Cargo.toml",
            "C:/Windows/win.ini", "/etc/passwd", workspace, game.ServerScriptService.a,
            script, nil, 42, {}, print, Instance.new("ModuleScript"),
        }
        for i = 1, 16 do
            local ok, err = pcall(require, attempts[i])
            print(ok, err)
        end
        print(require(game.ReplicatedStorage.m))
        "#,
    );
    let output = lines(&mut runtime);
    assert_eq!(output.len(), 17, "{output:?}");
    for line in &output[..15] {
        assert!(
            line.starts_with("false\tscripts/a.server.lua:8: require takes a ModuleScript"),
            "{line}"
        );
    }
    assert!(output[0].contains("not a string (\"os\")"), "{}", output[0]);
    assert!(output[8].contains("not a Workspace"), "{}", output[8]);
    assert!(output[11].contains("not a nil"), "{}", output[11]);
    // A ModuleScript made by a script has no file, so no code to run.
    assert!(
        output[15].contains("this ModuleScript has no code"),
        "{}",
        output[15]
    );
    assert_eq!(output[16], "1");
}

#[test]
fn an_error_in_a_module_reaches_the_script_and_it_loads_again_next_time() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/m.module.lua",
        r#"
        tries = (tries or 0) + 1
        error("not ready")
        "#,
    );
    start(
        &mut runtime,
        &mut world,
        "scripts/a.server.lua",
        r#"
        print(pcall(require, game.ReplicatedStorage.m))
        require(game.ReplicatedStorage.m)
        "#,
    );
    assert_eq!(
        lines(&mut runtime),
        [
            "false\tscripts/m.module.lua:3: not ready",
            "error: scripts/m.module.lua:3: not ready",
        ]
    );
}

#[test]
fn a_syntax_error_in_a_module_names_both_files() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(&mut runtime, &mut world, "scripts/m.module.lua", "return {");
    start(
        &mut runtime,
        &mut world,
        "scripts/a.server.lua",
        "require(game.ReplicatedStorage.m)",
    );
    let output = lines(&mut runtime);
    assert!(
        output[0].starts_with("error: scripts/a.server.lua:1: scripts/m.module.lua:1:"),
        "{output:?}"
    );
}

#[test]
fn a_module_must_return_exactly_one_value() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/none.module.lua",
        "local x = 1",
    );
    start(
        &mut runtime,
        &mut world,
        "scripts/two.module.lua",
        "return 1, 2",
    );
    start(
        &mut runtime,
        &mut world,
        "scripts/a.server.lua",
        r#"
        print(pcall(require, game.ReplicatedStorage.none))
        print(pcall(require, game.ReplicatedStorage.two))
        "#,
    );
    let output = lines(&mut runtime);
    assert!(
        output[0].contains("`scripts/none.module.lua` must return exactly one value")
            && output[0].ends_with("but it returned 0"),
        "{output:?}"
    );
    assert!(output[1].ends_with("but it returned 2"), "{output:?}");
}

#[test]
fn modules_that_require_each_other_get_a_clear_error() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/a.module.lua",
        // Not `return require(...)`: Lua forgets the caller of a tail call,
        // so the error would name `main.server.lua` instead.
        "local b = require(game.ReplicatedStorage.b)\nreturn b",
    );
    start(
        &mut runtime,
        &mut world,
        "scripts/b.module.lua",
        "local a = require(game.ReplicatedStorage.a)\nreturn a",
    );
    start(
        &mut runtime,
        &mut world,
        "scripts/main.server.lua",
        "require(game.ReplicatedStorage.a)",
    );
    let output = lines(&mut runtime);
    assert_eq!(output.len(), 1, "{output:?}");
    assert!(
        output[0].starts_with("error: scripts/b.module.lua:1: `scripts/a.module.lua` is required again while it's still loading"),
        "{output:?}"
    );
}

#[test]
fn a_module_can_wait_while_loading_and_other_scripts_wait_for_it() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/slow.module.lua",
        "print('loading')\ntask.wait(1)\nreturn 'ready'",
    );
    let use_it = "print(script.Name, require(game.ReplicatedStorage.slow))";
    start(&mut runtime, &mut world, "scripts/a.server.lua", use_it);
    start(&mut runtime, &mut world, "scripts/b.server.lua", use_it);
    assert_eq!(lines(&mut runtime), ["loading"]);
    for _ in 0..70 {
        runtime.step(&mut world, 1.0 / 60.0);
    }
    assert_eq!(lines(&mut runtime), ["a\tready", "b\tready"]);
}

#[test]
fn luau_builtins_the_sandbox_replaces_or_leaves_out_stay_that_way() {
    // Script environments are `safeenv`, so Luau runs its builtins without
    // looking them up; `sandbox::compiler` turns that off for these.
    let output = output_of(
        r#"
        print(typeof(workspace), typeof(Vector3.new()))
        print(pcall(function() return unpack({ 1, 2 }) end))
        print(pcall(function() return vector.create(1, 2, 3) end))
        print(pcall(function() return integer.create(1) end))
        print(pcall(function() return setmetatable({}, { __gc = function() end }) end))
        "#,
    );
    assert_eq!(output[0], "Instance\tVector3");
    for line in &output[1..] {
        assert!(line.starts_with("false\t"), "{output:?}");
    }
    assert!(output[4].contains("__gc isn't supported"), "{output:?}");
}

#[test]
fn syntax_errors_are_found_without_running_anything() {
    assert_eq!(
        crate::find_syntax_error("print('fine')\nlocal x = 1\n"),
        None
    );
    let error = crate::find_syntax_error("local function f()\n  print(1)\n").unwrap();
    assert_eq!(error.line, 3, "{error:?}");
    assert!(error.message.contains("Expected 'end'"), "{error:?}");
    let error = crate::find_syntax_error("local x = \nprint(x +)\n").unwrap();
    assert_eq!(error.line, 2, "{error:?}");
}
