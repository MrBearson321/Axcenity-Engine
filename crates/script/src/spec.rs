//! The game test runner's way into a game (Phase 53; the runner is the
//! `testing` crate). A spec file runs on the server side as a coroutine of
//! its own, like a Script: `task.wait` pauses it while the host runs frames.
//!
//! The runner's Lua (`describe`, `it`, `expect`, `Test`) is the runner's
//! own chunk, run in the spec's environment with a `host` table of the
//! functions here. What it asks of the host (pressing keys, clicking,
//! typing) waits as [`TestRequest`]s for the host's next frame; what it
//! finds and how a test ended wait as [`TestReport`]s.

use axcenity_core::{InstanceId, World};
use mlua::chunk::ChunkMode;
use mlua::{Function, Lua, Table, Thread};

use crate::instance::LuaInstance;
use crate::sides::Side;
use crate::{ScriptRuntime, errors, push_error, sandbox, scheduler, with_world};

/// Something a running spec asked the host to do, at its next frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TestRequest {
    /// Press (`down`) or let go of a key, mouse button or input action, by
    /// its name (`W`, `MouseButton1`, `Jump`).
    Press { input: String, down: bool },
    /// Click a button, as the player would.
    Click(InstanceId),
    /// Type `text` into a TextBox and finish with it.
    Type { text_box: InstanceId, text: String },
}

/// A test a spec file declares with `it`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoundTest {
    /// The `describe` names around it, outermost first, then its own.
    pub name: Vec<String>,
    /// The line of its `it`.
    pub line: u32,
    /// The level it runs in (`Test.level`), or `None` for the start level.
    pub level: Option<String>,
}

/// What a spec told the host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TestReport {
    Found(FoundTest),
    /// The spec's run ended: `None` when it passed (or, when only
    /// collecting, when the file ran), else the failure, `path:line:` first.
    Finished(Option<String>),
}

/// The spec running on this side, and what it asked and told, in Lua's
/// app data.
#[derive(Default)]
struct TestHost {
    thread: Option<Thread>,
    requests: Vec<TestRequest>,
    reports: Vec<TestReport>,
}

fn with_host(lua: &Lua, f: impl FnOnce(&mut TestHost)) {
    if let Some(mut host) = lua.app_data_mut::<TestHost>() {
        f(&mut host);
    }
}

impl ScriptRuntime {
    /// Starts the spec file `path` (its code `source`) as `script`, a
    /// server Script the host made for it. `runner` is the test runner's
    /// chunk: it's run first, in the same environment, with that
    /// environment and the host's functions, and returns the function the
    /// spec's coroutine runs, given the spec's code and `selected` (the
    /// test to run, from 1; `None` only collects them).
    ///
    /// A spec that doesn't compile is reported finished, with its error.
    pub fn start_spec(
        &mut self,
        world: &mut World,
        script: InstanceId,
        runner: &str,
        path: &str,
        source: &str,
        selected: Option<usize>,
    ) {
        self.fire_changes(world);
        let lua = &self.server.lua;
        lua.set_app_data(TestHost::default());
        let ids = sandbox::ScriptIds {
            game: world.root(),
            workspace: world.workspace(),
            script,
        };
        with_world(lua, world, || {
            match load_spec(lua, ids, runner, path, source) {
                Ok((thread, spec)) => {
                    with_host(lua, |host| host.thread = Some(thread.clone()));
                    scheduler::resume_as(lua, &thread, (spec, selected), Some(script));
                }
                Err(err) => {
                    push_error(lua, &err);
                    let message = errors::error_message(&err);
                    with_host(lua, |host| {
                        host.reports.push(TestReport::Finished(Some(message)));
                    });
                }
            }
        });
        self.collect(Side::Server);
        self.fire_changes(world);
    }

    /// What the spec asked of the host since the last call, in order.
    pub fn take_test_requests(&mut self) -> Vec<TestRequest> {
        let mut requests = Vec::new();
        with_host(&self.server.lua, |host| {
            requests = std::mem::take(&mut host.requests);
        });
        requests
    }

    /// What the spec told the host since the last call, in order.
    pub fn take_test_reports(&mut self) -> Vec<TestReport> {
        let mut reports = Vec::new();
        with_host(&self.server.lua, |host| {
            reports = std::mem::take(&mut host.reports);
        });
        reports
    }

    /// False once the spec's coroutine has ended, finished or not (an
    /// error the runner couldn't catch, such as the watchdog's, ends it).
    pub fn spec_running(&self) -> bool {
        self.server
            .lua
            .app_data_ref::<TestHost>()
            .and_then(|host| host.thread.as_ref().map(Thread::is_resumable))
            .unwrap_or(false)
    }
}

/// The spec's coroutine (the runner's function) and the spec's code.
fn load_spec(
    lua: &Lua,
    ids: sandbox::ScriptIds,
    runner: &str,
    path: &str,
    source: &str,
) -> mlua::Result<(Thread, Function)> {
    let env = sandbox::environment(lua, ids)?;
    // `=[test]` starts with `[`, so error traces leave the runner out.
    let run: Function = lua
        .load(runner)
        .set_name("=[test]")
        .set_mode(ChunkMode::Text)
        .set_environment(env.clone())
        .call((env.clone(), host_functions(lua)?))?;
    let spec = crate::chunks::load(lua, path, source, env)?;
    Ok((lua.create_thread(run)?, spec))
}

/// The `host` table the runner's chunk gets.
fn host_functions(lua: &Lua) -> mlua::Result<Table> {
    let host = lua.create_table()?;
    host.set(
        "press",
        lua.create_function(|lua, (input, down): (String, bool)| {
            with_host(lua, |host| {
                host.requests.push(TestRequest::Press { input, down });
            });
            Ok(())
        })?,
    )?;
    host.set(
        "click",
        lua.create_function(|lua, button: LuaInstance| {
            with_host(lua, |host| host.requests.push(TestRequest::Click(button.0)));
            Ok(())
        })?,
    )?;
    host.set(
        "type",
        lua.create_function(|lua, (text_box, text): (LuaInstance, String)| {
            with_host(lua, |host| {
                host.requests.push(TestRequest::Type {
                    text_box: text_box.0,
                    text,
                });
            });
            Ok(())
        })?,
    )?;
    host.set(
        "found",
        lua.create_function(
            |lua, (name, line, level): (Vec<String>, u32, Option<String>)| {
                with_host(lua, |host| {
                    host.reports
                        .push(TestReport::Found(FoundTest { name, line, level }));
                });
                Ok(())
            },
        )?,
    )?;
    host.set(
        "finished",
        lua.create_function(|lua, failure: Option<String>| {
            with_host(lua, |host| host.reports.push(TestReport::Finished(failure)));
            Ok(())
        })?,
    )?;
    // `path, line` of the nearest script code on the call stack: the spec
    // line (or the module's) that called the runner's function.
    host.set(
        "location",
        lua.create_function(|lua, ()| {
            let found = (0..)
                .map_while(|level| {
                    lua.inspect_stack(level, |debug| {
                        let path = debug.source().source?.strip_prefix('@')?.to_owned();
                        Some((path, debug.current_line()?))
                    })
                })
                .flatten()
                .next();
            Ok(found.unzip())
        })?,
    )?;
    Ok(host)
}
