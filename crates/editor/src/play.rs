//! Play mode: the game running inside the editor.
//!
//! A [`Session`] runs on its own deep copy of the edit world, with its own
//! scripts and physics, so nothing that happens during Play touches the
//! world being edited. Stop simply drops the session.
//!
//! Every call into the game is guarded: a Rust panic (an engine bug) is
//! caught here and returned as a [`Crash`], so the editor can report it and
//! go back to editing instead of closing.

use std::any::Any;
use std::panic::{self, AssertUnwindSafe};
use std::path::Path;

use axcenity_core::World;
use axcenity_physics::Physics;
use axcenity_project::ScriptFile;
use axcenity_script::{OutputKind, ScriptRuntime};

use crate::output::Output;

/// The message of a Rust panic caught during Play.
#[derive(Debug)]
pub struct Crash(pub String);

/// The game while it plays: the copied world, the scripts running in it and
/// the physics moving its parts.
pub struct Session {
    world: World,
    scripts: ScriptRuntime,
    physics: Physics,
    /// Set by Debug > Panic during Play, to check that a crash is caught.
    panic_next_frame: bool,
}

impl Session {
    /// Copies `edit_world` and starts every script in it, in path order, the
    /// same way the player does. Script output and errors go to `output`.
    pub fn start(
        edit_world: &World,
        scripts: &[ScriptFile],
        output: &mut Output,
    ) -> Result<Session, Crash> {
        let mut session = Session {
            world: edit_world.clone(),
            scripts: ScriptRuntime::new(),
            physics: Physics::new(),
            panic_next_frame: false,
        };
        guard(|| {
            for script in scripts {
                session.scripts.start_script(
                    &mut session.world,
                    script.instance,
                    &script.path,
                    &script.source,
                );
                session.forward_output(output);
            }
        })?;
        Ok(session)
    }

    /// The world as the game has it now, for the panels and the viewport.
    pub fn world(&self) -> &World {
        &self.world
    }

    /// Makes the next frame panic, as an engine bug would.
    pub fn panic_next_frame(&mut self) {
        self.panic_next_frame = true;
    }

    /// Runs one frame of the game, `seconds` long: steps 2 to 4 of the fixed
    /// frame order in `docs/architecture.md`. The editor's viewport draws the
    /// result (step 6) afterward.
    pub fn frame(&mut self, seconds: f64, output: &mut Output) -> Result<(), Crash> {
        guard(|| {
            if self.panic_next_frame {
                panic!("a deliberate engine panic, from Debug > Panic during Play");
            }
            // 2. Resume scripts whose wait is over (and due `task.delay`s).
            self.scripts.step(&mut self.world, seconds);
            self.forward_output(output);
            // 3. Step physics at a fixed 60 Hz (0 to N steps this frame).
            self.physics.step(&mut self.world, seconds);
            // 4. Fire `Touched` for parts that started touching.
            let touches = self.physics.take_touches();
            self.scripts.fire_touched(&mut self.world, &touches);
            self.forward_output(output);
        })
    }

    /// Moves what scripts printed, and their errors, into the Output panel.
    fn forward_output(&mut self, output: &mut Output) {
        for line in self.scripts.take_output() {
            match line.kind {
                OutputKind::Print => output.info(line.text),
                OutputKind::Error => output.error(line.text),
            }
        }
    }
}

/// Runs `f`, catching a panic inside it and returning its message instead.
///
/// `catch_unwind` wants a closure that's "unwind safe": one that can't leave
/// half-changed state behind that someone uses after the panic. The session
/// a panic interrupts is thrown away, never used again, so
/// `AssertUnwindSafe` tells the compiler that's fine.
fn guard(f: impl FnOnce()) -> Result<(), Crash> {
    panic::catch_unwind(AssertUnwindSafe(f)).map_err(|payload| Crash(panic_message(&*payload)))
}

/// The text a panic was started with. `panic!("...")` carries a `&str`, and
/// `panic!("{x}")` a `String`; anything else has no readable message.
fn panic_message(payload: &(dyn Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        text.to_string()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "no message".to_string()
    }
}

/// The newest text of each script in the project in `folder`, read again
/// from disk so Play runs what was last saved in the code editor. A script
/// that can't be read is left out, with the reason in `output`.
pub fn read_scripts(folder: &Path, scripts: &[ScriptFile], output: &mut Output) -> Vec<ScriptFile> {
    let mut fresh = Vec::new();
    for script in scripts {
        match axcenity_project::read_script(folder, &script.path) {
            Ok(source) => fresh.push(ScriptFile {
                instance: script.instance,
                path: script.path.clone(),
                source,
            }),
            Err(err) => {
                let err = anyhow::Error::new(err);
                output.error(format!("{} won't run: {err:#}", script.path));
            }
        }
    }
    fresh
}

#[cfg(test)]
mod tests {
    use axcenity_core::ClassName;

    use super::*;

    fn script(world: &mut World, source: &str) -> ScriptFile {
        let instance = world.create(ClassName::Script).unwrap();
        world
            .set_parent(instance, Some(world.server_script_service()))
            .unwrap();
        ScriptFile {
            instance,
            path: "scripts/test.server.lua".to_string(),
            source: source.to_string(),
        }
    }

    #[test]
    fn scripts_change_only_the_copy() {
        let mut world = World::new();
        let scripts = [script(
            &mut world,
            "local p = Instance.new('Part') p.Name = 'Made' p.Parent = workspace print('hi')",
        )];
        let mut output = Output::default();
        let session = Session::start(&world, &scripts, &mut output).unwrap();

        let made = session.world().find_first_child(world.workspace(), "Made");
        assert!(made.is_some());
        assert!(world.find_first_child(world.workspace(), "Made").is_none());
        assert_eq!(output.lines().last().unwrap().text, "hi");
    }

    #[test]
    fn script_errors_go_to_output_and_the_game_keeps_running() {
        let mut world = World::new();
        let scripts = [script(&mut world, "task.wait(1)\nerror('boom')")];
        let mut output = Output::default();
        let mut session = Session::start(&world, &scripts, &mut output).unwrap();
        for _ in 0..90 {
            session.frame(1.0 / 60.0, &mut output).unwrap();
        }
        let line = output.lines().last().unwrap();
        assert_eq!(line.kind, crate::output::OutputKind::Error);
        assert_eq!(line.text, "scripts/test.server.lua:2: boom");
    }

    #[test]
    fn a_panic_in_a_frame_is_caught_with_its_message() {
        let mut output = Output::default();
        let mut session = Session::start(&World::new(), &[], &mut output).unwrap();
        session.frame(1.0 / 60.0, &mut output).unwrap();
        session.panic_next_frame();
        let crash = session.frame(1.0 / 60.0, &mut output).unwrap_err();
        assert!(crash.0.contains("deliberate engine panic"), "{crash:?}");
        // Formatted messages carry a `String`, which is read too.
        let crash = guard(|| panic!("{} went wrong", "this")).unwrap_err();
        assert_eq!(crash.0, "this went wrong");
    }
}
