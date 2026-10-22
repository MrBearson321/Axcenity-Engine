//! Lua typed into the editor's Console, run with the same sandbox as scripts.
//!
//! A command is first tried as an expression (`return <command>`), so
//! `workspace.Baseplate.Size` prints its value; if that isn't valid Lua, it
//! runs as statements (`workspace.Baseplate.Transparency = 0.5`). Errors
//! read `console:1: ...`, so they never look like a link to a script file.
//!
//! During Play, commands run on the server side and share one environment,
//! so a global set by one command (`x = 5`) is there for the next. Outside
//! Play, each command runs in a runtime of its own that's thrown away after
//! it ([`run_edit_command`](crate::run_edit_command)).

use axcenity_core::World;
use mlua::chunk::ChunkMode;
use mlua::{Function, Lua, Table, Value};

use crate::errors::error_message;
use crate::sandbox::{self, ScriptIds};

/// The name commands have in error messages: `console:1: ...`.
const CHUNK_NAME: &str = "=console";

/// Wraps an expression command so it prints what it returns. Written in
/// Lua, so the command can still pause (`task.wait`) during Play.
const ECHO: &str = r#"
local pack, unpack, print = table.pack, table.unpack, ...

return function(command)
    return function()
        local results = pack(command())
        if results.n > 0 then
            print(unpack(results, 1, results.n))
        end
    end
end
"#;

/// The environment console commands share on one side during Play.
struct ConsoleEnv(Table);

/// The environment for the next command: the one commands share, made
/// the first time. `script` is nil, as nothing is running a script.
pub fn environment(lua: &Lua, world: &World) -> mlua::Result<Table> {
    if let Some(env) = lua.app_data_ref::<ConsoleEnv>() {
        return Ok(env.0.clone());
    }
    let env = sandbox::environment(
        lua,
        ScriptIds {
            game: world.root(),
            workspace: world.workspace(),
            script: world.root(),
        },
    )?;
    env.raw_set("script", Value::Nil)?;
    lua.set_app_data(ConsoleEnv(env.clone()));
    Ok(env)
}

/// Compiles `source` in `env`, as an expression that prints its results if
/// it is one. `Err` is the message for code that isn't valid Lua.
pub fn compile(lua: &Lua, env: &Table, source: &str) -> Result<Function, String> {
    let load = |code: &str| {
        lua.load(code)
            .set_name(CHUNK_NAME)
            // Only source text, as for scripts: bytecode can crash the interpreter.
            .set_mode(ChunkMode::Text)
            .set_environment(env.clone())
            .into_function()
    };
    if let Ok(expression) = load(&format!("return {source}")) {
        return echo(lua, env, expression).map_err(|err| error_message(&err));
    }
    load(source).map_err(|err| error_message(&err))
}

fn echo(lua: &Lua, env: &Table, expression: Function) -> mlua::Result<Function> {
    let wrap: Function = lua
        .load(ECHO)
        .set_name("=[engine]")
        .call(env.raw_get::<Value>("print")?)?;
    wrap.call(expression)
}

#[cfg(test)]
mod tests {
    use axcenity_core::{ClassName, InstanceId};

    use crate::{OutputKind, ScriptRuntime, run_edit_command};

    use super::*;

    fn world_with_baseplate() -> (World, InstanceId) {
        let mut world = World::new();
        let part = world.create(ClassName::Part).unwrap();
        world.set_name(part, "Baseplate").unwrap();
        world.set_parent(part, Some(world.workspace())).unwrap();
        (world, part)
    }

    fn texts(lines: &[crate::OutputLine]) -> Vec<String> {
        lines
            .iter()
            .map(|line| match line.kind {
                OutputKind::Print => line.text.clone(),
                OutputKind::Error => format!("error: {}", line.text),
            })
            .collect()
    }

    #[test]
    fn an_expression_prints_its_value_and_a_statement_changes_the_world() {
        let (mut world, part) = world_with_baseplate();
        let size = run_edit_command(&mut world, "workspace.Baseplate.Name").unwrap();
        assert_eq!(texts(&size.output), ["Baseplate"]);
        assert!(!size.changed);

        let set = run_edit_command(&mut world, "workspace.Baseplate.Transparency = 0.5").unwrap();
        assert!(set.output.is_empty());
        assert!(set.changed);
        assert_eq!(
            world.get(part).unwrap().property("Transparency"),
            Some(&axcenity_core::Value::Number(0.5))
        );
        // The editor's world isn't left recording changes.
        assert!(!world.is_recording_changes());
    }

    #[test]
    fn print_prints_once_and_errors_name_the_console() {
        let (mut world, _) = world_with_baseplate();
        let printed = run_edit_command(&mut world, "print(1 + 1)").unwrap();
        assert_eq!(texts(&printed.output), ["2"]);

        let failed = run_edit_command(&mut world, "workspace.Nope.Size = 1").unwrap();
        assert_eq!(failed.output.len(), 1);
        assert_eq!(failed.output[0].kind, OutputKind::Error);
        assert!(
            failed.output[0].text.starts_with("console:1:"),
            "{failed:?}"
        );

        let bad = run_edit_command(&mut world, "teleport Baseplate").unwrap_err();
        assert!(bad.starts_with("console:1:"), "{bad}");
    }

    #[test]
    fn edit_commands_use_the_sandbox() {
        let (mut world, _) = world_with_baseplate();
        let result = run_edit_command(&mut world, "print(io, debug, load, script)").unwrap();
        assert_eq!(texts(&result.output), ["nil\tnil\tnil\tnil"]);
    }

    #[test]
    fn play_commands_share_globals_and_can_wait() {
        let (mut world, _) = world_with_baseplate();
        let mut runtime = ScriptRuntime::new();
        runtime.run_command(&mut world, "x = 5").unwrap();
        runtime
            .run_command(&mut world, "task.wait(0.1) print(x * 2)")
            .unwrap();
        assert!(runtime.take_output().is_empty());
        runtime.step(&mut world, 0.2);
        assert_eq!(texts(&runtime.take_output()), ["10"]);
    }

    #[test]
    fn global_names_match_a_script_environment() {
        let context = crate::context::Context::new(crate::Side::Server);
        let world = World::new();
        let env = sandbox::environment(
            &context.lua,
            ScriptIds {
                game: world.root(),
                workspace: world.workspace(),
                script: world.root(),
            },
        )
        .unwrap();
        let mut names: Vec<String> = env
            .pairs::<String, Value>()
            .map(|pair| pair.unwrap().0)
            .collect();
        names.sort();
        assert_eq!(names, crate::global_names());
    }
}
