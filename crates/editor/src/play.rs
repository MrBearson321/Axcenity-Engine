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
use std::collections::BTreeSet;
use std::panic::{self, AssertUnwindSafe};
use std::path::Path;

use axcenity_anim::{Animators, PropertyPlayers};
use axcenity_assets::Assets;
use axcenity_audio::GameAudio;
use axcenity_core::{InstanceId, KeyCode, World};
use axcenity_physics::Physics;
use axcenity_project::ScriptFile;
use axcenity_script::{CameraState, KeyEvent, OutputKind, ScriptRuntime};
use eframe::egui;

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
    /// Plays the Animators and poses their MeshParts.
    animators: Animators,
    /// Plays property Animation Instances on Parts.
    property_players: PropertyPlayers,
    assets: Assets,
    /// Dropped at Stop, which silences every sound.
    audio: GameAudio,
    /// Set by Debug > Panic during Play, to check that a crash is caught.
    panic_next_frame: bool,
}

impl Session {
    /// Copies `edit_world` and starts every script in it, in the order given
    /// (Scripts, then LocalScripts, as the project lists them), the same way
    /// the player does. Script output and errors go to `output`. MeshParts
    /// collide as their models from `assets`.
    pub fn start(
        edit_world: &World,
        scripts: &[ScriptFile],
        output: &mut Output,
        assets: Assets,
        camera: CameraState,
    ) -> Result<Session, Crash> {
        let mut session = Session {
            world: edit_world.clone(),
            scripts: ScriptRuntime::new(),
            physics: Physics::with_assets(assets.clone()),
            animators: Animators::new(),
            property_players: PropertyPlayers::new(),
            assets,
            audio: GameAudio::default(),
            panic_next_frame: false,
        };
        guard(|| {
            // Animators start in their default states before any script
            // runs, as in the player, so scripts can read them.
            session
                .animators
                .update(&mut session.world, &session.assets, 0.0);
            session
                .property_players
                .update(&mut session.world, &session.assets, 0.0);
            session.forward_animator_errors(output);
            session.forward_property_animation_errors(output);
            for script in scripts {
                session.scripts.start_script(
                    &mut session.world,
                    script.instance,
                    &script.path,
                    &script.source,
                );
                session.forward_output(output);
            }
            session.apply_audio(camera, output);
        })?;
        Ok(session)
    }

    /// The world as the game has it now, for the panels and the viewport.
    pub fn world(&self) -> &World {
        &self.world
    }

    /// The game's world, for the Properties panel's edits during Play. The
    /// game's scripts see those changes at the next frame.
    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    /// Makes the next frame panic, as an engine bug would.
    pub fn panic_next_frame(&mut self) {
        self.panic_next_frame = true;
    }

    /// Fires a UI button event inside the guarded Play session.
    pub fn activate_ui_button(
        &mut self,
        button: axcenity_core::InstanceId,
        output: &mut Output,
    ) -> Result<(), Crash> {
        guard(|| {
            self.scripts.activate_ui_button(&mut self.world, button);
            self.forward_output(output);
        })
    }

    /// Types into a TextBox inside the guarded Play session.
    pub fn edit_text(
        &mut self,
        text_box: axcenity_core::InstanceId,
        command: axcenity_ui::TextCommand,
        output: &mut Output,
    ) -> Result<axcenity_ui::TextOutcome, Crash> {
        let mut outcome = axcenity_ui::TextOutcome::Ignored;
        guard(|| {
            outcome = axcenity_ui::edit_text_box(&mut self.world, text_box, command);
            self.forward_output(output);
        })?;
        Ok(outcome)
    }

    /// Fires `FocusLost` for a TextBox the player finished typing in.
    pub fn release_text_box(
        &mut self,
        text_box: axcenity_core::InstanceId,
        output: &mut Output,
    ) -> Result<(), Crash> {
        guard(|| {
            self.scripts.release_text_box(&mut self.world, text_box);
            self.forward_output(output);
        })
    }

    /// Runs steps 1 to 4 of one frame, `seconds` long, with the keys pressed
    /// and released since the last frame and the viewport's camera. The
    /// viewport lays out UI at step 5 and draws the result at step 6.
    /// Returns where a LocalScript moved the camera, if one did.
    pub fn frame(
        &mut self,
        seconds: f64,
        keys: &[KeyEvent],
        camera: CameraState,
        output: &mut Output,
    ) -> Result<Option<CameraState>, Crash> {
        let mut moved = None;
        guard(|| {
            if self.panic_next_frame {
                panic!("a deliberate engine panic, from Debug > Panic during Play");
            }
            // 1. Input, for LocalScripts: the camera and the keys.
            self.scripts.set_camera(camera);
            self.scripts.input(&mut self.world, keys);
            self.forward_output(output);
            // 2. Resume scripts whose wait is over (and due `task.delay`s),
            //    after delivering RemoteEvent messages.
            self.scripts.step(&mut self.world, seconds);
            self.forward_output(output);
            // 3. Step physics at a fixed 60 Hz (0 to N steps this frame).
            self.physics.step(&mut self.world, seconds);
            // 4. Fire `Touched` for parts that started touching (and
            //    `Changed` for parts that moved), then `RunService.Heartbeat`.
            let touches = self.physics.take_touches();
            self.scripts.fire_touched(&mut self.world, &touches);
            self.scripts.heartbeat(&mut self.world, seconds);
            self.forward_output(output);
            moved = self.scripts.take_camera_move();
            self.apply_audio(moved.unwrap_or(camera), output);
            // Animators move on and pose their models, as in the player.
            let events = self
                .animators
                .update(&mut self.world, &self.assets, seconds);
            for event in events {
                let (animator, name, args) = event.script_event();
                self.scripts
                    .fire_instance_event(&mut self.world, animator, name, &args);
            }
            self.forward_output(output);
            self.forward_animator_errors(output);
            let events = self
                .property_players
                .update(&mut self.world, &self.assets, seconds);
            for event in events {
                let (animation, name, args) = event.script_event();
                self.scripts
                    .fire_instance_event(&mut self.world, animation, name, &args);
            }
            self.forward_output(output);
            self.forward_property_animation_errors(output);
        })?;
        Ok(moved)
    }

    /// The posed models of the animated MeshParts, for the viewport.
    pub fn poses(&self) -> &axcenity_scene::Poses {
        self.animators.poses()
    }

    fn forward_animator_errors(&mut self, output: &mut Output) {
        for error in self.animators.take_errors() {
            output.error(error);
        }
    }

    fn forward_property_animation_errors(&mut self, output: &mut Output) {
        for error in self.property_players.take_errors() {
            output.error(error);
        }
    }

    /// The `emitter:Emit(count)` calls scripts made since the last call.
    pub fn take_particle_emits(&mut self) -> Vec<(InstanceId, u32)> {
        self.scripts.take_particle_emits()
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

    fn apply_audio(&mut self, camera: CameraState, output: &mut Output) {
        let actions = self.scripts.take_sound_actions();
        let listener = axcenity_audio::Listener::from_view(camera.position, camera.orientation);
        let update = self
            .audio
            .update(actions, &self.world, &self.assets, listener);
        for err in update.errors {
            output.error(format!("Audio: {err}"));
        }
        if !update.ended.is_empty() {
            self.scripts
                .fire_sounds_ended(&mut self.world, &update.ended);
            self.forward_output(output);
        }
    }
}

/// Turns the editor's keyboard events into key presses for LocalScripts
/// during Play.
#[derive(Default)]
pub struct GameKeys {
    held: BTreeSet<KeyCode>,
    /// Shift, Ctrl and Alt as last seen. egui reports them as modifiers, not
    /// keys, so a change in them is a press or release of the left one.
    modifiers: egui::Modifiers,
}

impl GameKeys {
    /// The presses and releases in this frame's `events`. While `typing`
    /// (a text box has the keyboard), or when the window lost focus, every
    /// held key is let go and new presses are ignored.
    pub fn read(
        &mut self,
        events: &[egui::Event],
        modifiers: egui::Modifiers,
        focused: bool,
        typing: bool,
    ) -> Vec<KeyEvent> {
        let mut keys = Vec::new();
        if typing || !focused {
            for key in std::mem::take(&mut self.held) {
                keys.push(KeyEvent {
                    key,
                    pressed: false,
                });
            }
            self.modifiers = egui::Modifiers::NONE;
            return keys;
        }
        let mut change = |key: KeyCode, pressed: bool| {
            let changed = if pressed {
                self.held.insert(key)
            } else {
                self.held.remove(&key)
            };
            if changed {
                keys.push(KeyEvent { key, pressed });
            }
        };
        let was = self.modifiers;
        for (key, before, now) in [
            (KeyCode::LeftShift, was.shift, modifiers.shift),
            (KeyCode::LeftControl, was.ctrl, modifiers.ctrl),
            (KeyCode::LeftAlt, was.alt, modifiers.alt),
        ] {
            if before != now {
                change(key, now);
            }
        }
        self.modifiers = modifiers;
        for event in events {
            if let egui::Event::Key {
                key,
                physical_key,
                pressed,
                ..
            } = event
                // The key's place on the keyboard, like the player, when known.
                && let Some(key) = script_key(physical_key.unwrap_or(*key))
            {
                change(key, *pressed);
            }
        }
        keys
    }
}

/// The key scripts know as `Enum.KeyCode.<name>` for an egui key, if it's
/// one they can see. Shift, Ctrl and Alt come from the modifiers instead.
fn script_key(key: egui::Key) -> Option<KeyCode> {
    use egui::Key as E;
    let key = match key {
        E::A => KeyCode::A,
        E::B => KeyCode::B,
        E::C => KeyCode::C,
        E::D => KeyCode::D,
        E::E => KeyCode::E,
        E::F => KeyCode::F,
        E::G => KeyCode::G,
        E::H => KeyCode::H,
        E::I => KeyCode::I,
        E::J => KeyCode::J,
        E::K => KeyCode::K,
        E::L => KeyCode::L,
        E::M => KeyCode::M,
        E::N => KeyCode::N,
        E::O => KeyCode::O,
        E::P => KeyCode::P,
        E::Q => KeyCode::Q,
        E::R => KeyCode::R,
        E::S => KeyCode::S,
        E::T => KeyCode::T,
        E::U => KeyCode::U,
        E::V => KeyCode::V,
        E::W => KeyCode::W,
        E::X => KeyCode::X,
        E::Y => KeyCode::Y,
        E::Z => KeyCode::Z,
        E::Num0 => KeyCode::Zero,
        E::Num1 => KeyCode::One,
        E::Num2 => KeyCode::Two,
        E::Num3 => KeyCode::Three,
        E::Num4 => KeyCode::Four,
        E::Num5 => KeyCode::Five,
        E::Num6 => KeyCode::Six,
        E::Num7 => KeyCode::Seven,
        E::Num8 => KeyCode::Eight,
        E::Num9 => KeyCode::Nine,
        E::Space => KeyCode::Space,
        E::Enter => KeyCode::Return,
        E::Escape => KeyCode::Escape,
        E::Tab => KeyCode::Tab,
        E::Backspace => KeyCode::Backspace,
        E::ArrowUp => KeyCode::Up,
        E::ArrowDown => KeyCode::Down,
        E::ArrowLeft => KeyCode::Left,
        E::ArrowRight => KeyCode::Right,
        _ => return None,
    };
    Some(key)
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
        let session = Session::start(
            &world,
            &scripts,
            &mut output,
            Assets::default(),
            CameraState::default(),
        )
        .unwrap();

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
        let mut session = Session::start(
            &world,
            &scripts,
            &mut output,
            Assets::default(),
            CameraState::default(),
        )
        .unwrap();
        for _ in 0..90 {
            session
                .frame(1.0 / 60.0, &[], CameraState::default(), &mut output)
                .unwrap();
        }
        let line = output.lines().last().unwrap();
        assert_eq!(line.kind, crate::output::OutputKind::Error);
        assert_eq!(line.text, "scripts/test.server.lua:2: boom");
    }

    #[test]
    fn a_panic_in_a_frame_is_caught_with_its_message() {
        let mut output = Output::default();
        let mut session = Session::start(
            &World::new(),
            &[],
            &mut output,
            Assets::default(),
            CameraState::default(),
        )
        .unwrap();
        let camera = CameraState::default();
        session.frame(1.0 / 60.0, &[], camera, &mut output).unwrap();
        session.panic_next_frame();
        let crash = session
            .frame(1.0 / 60.0, &[], camera, &mut output)
            .unwrap_err();
        assert!(crash.0.contains("deliberate engine panic"), "{crash:?}");
        // Formatted messages carry a `String`, which is read too.
        let crash = guard(|| panic!("{} went wrong", "this")).unwrap_err();
        assert_eq!(crash.0, "this went wrong");
    }

    fn key(key: egui::Key, pressed: bool) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn game_keys_report_presses_releases_and_modifiers_once() {
        let mut keys = GameKeys::default();
        let none = egui::Modifiers::NONE;
        let pressed = keys.read(
            &[
                key(egui::Key::ArrowRight, true),
                // Held keys repeat; that isn't a new press.
                key(egui::Key::ArrowRight, true),
                key(egui::Key::F1, true),
            ],
            egui::Modifiers::SHIFT,
            true,
            false,
        );
        let event = |key, pressed| KeyEvent { key, pressed };
        assert_eq!(
            pressed,
            [event(KeyCode::LeftShift, true), event(KeyCode::Right, true)]
        );
        // Typing in a script tab lets go of everything.
        let typing = keys.read(&[key(egui::Key::A, true)], none, true, true);
        assert_eq!(
            typing,
            [
                event(KeyCode::Right, false),
                event(KeyCode::LeftShift, false)
            ]
        );
        assert_eq!(keys.read(&[], none, true, false), []);
    }

    #[test]
    fn a_local_script_in_play_reads_keys_and_moves_the_camera() {
        let mut world = World::new();
        let instance = world.create(ClassName::LocalScript).unwrap();
        world
            .set_parent(instance, Some(world.starter_player_scripts()))
            .unwrap();
        let scripts = [ScriptFile {
            instance,
            path: "scripts/keys.client.lua".to_string(),
            source: r#"
                game:GetService("UserInputService").InputBegan:Connect(function(input)
                    print(input.KeyCode.Name)
                    workspace.CurrentCamera.Position = Vector3.new(0, 50, 0)
                end)
            "#
            .to_string(),
        }];
        let mut output = Output::default();
        let mut session = Session::start(
            &world,
            &scripts,
            &mut output,
            Assets::default(),
            CameraState::default(),
        )
        .unwrap();
        let space = KeyEvent {
            key: KeyCode::Space,
            pressed: true,
        };
        let moved = session
            .frame(1.0 / 60.0, &[space], CameraState::default(), &mut output)
            .unwrap();
        assert_eq!(output.lines().last().unwrap().text, "Space");
        assert_eq!(
            moved.map(|camera| camera.position),
            Some(axcenity_core::Vector3::new(0.0, 50.0, 0.0))
        );
    }
}
