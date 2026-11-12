//! The frame-order steps hosts call each frame (input, `step`, Stepped,
//! render step, Heartbeat; see `docs/architecture/frame-loop.md`), and the
//! events they fire into scripts: touches, prompts, UI, sounds, Instance
//! events and change rounds.

use crate::*;

impl ScriptRuntime {
    /// Frame-order step 1: the keys pressed and released since the last
    /// frame, in order. LocalScripts see them through `UserInputService`.
    pub fn input(&mut self, world: &mut World, keys: &[KeyEvent]) {
        let events: Vec<InputEvent> = keys.iter().map(|&key| key.into()).collect();
        self.input_events(world, &events);
    }

    /// Frame-order step 1: everything the keyboard, mouse, touch screen
    /// and controllers did since the last frame, in order. LocalScripts see
    /// it through `UserInputService` and `ContextActionService`.
    pub fn input_events(&mut self, world: &mut World, events: &[InputEvent]) {
        // Changes made since the last frame, such as by the editor.
        self.fire_changes(world);
        // The Menu action skips a skippable Sequence (and goes no further);
        // a Sequence that locks input lets only releases through.
        let menu = self.input_map();
        let skips = events.iter().any(|event| presses(&menu, "Menu", event))
            && self.sequences.0.borrow().skippable(world);
        if skips {
            let stepped = {
                let mut camera = self.client_camera();
                self.sequences.0.borrow_mut().skip(world, &mut camera)
            };
            self.after_sequences(world, stepped);
        }
        let kept: Vec<InputEvent>;
        let events = if skips || self.input_locked(world) {
            let locked = self.input_locked(world);
            kept = events
                .iter()
                .filter(|event| !(skips && presses(&menu, "Menu", event)))
                .filter(|event| !locked || releases(event))
                .cloned()
                .collect();
            &kept[..]
        } else {
            events
        };
        // Even with no events, so `GetMouseDelta` is this frame's.
        self.client.fire_input(world, events);
        let tab = self.client.prompt_input().game.is_down(KeyCode::Tab);
        self.players.follow_tab(tab);
        if events.is_empty() {
            return;
        }
        self.join(world);
        self.collect(Side::Client);
        self.fire_changes(world);
    }

    /// The project's actions and bindings (`input.json`), for
    /// `UserInputService`'s actions. Call it before starting scripts.
    pub fn set_input_map(&mut self, map: InputMap) {
        self.client.set_input_map(map);
    }

    /// The input map with the player's rebinds (`RebindAction`, kept in
    /// SaveService's settings), for the built-in controllers and UI
    /// navigation.
    pub fn input_map(&self) -> InputMap {
        self.client.input_map()
    }

    /// `UserInputService.MouseBehavior` and `MouseIconEnabled`, which the
    /// host applies to the cursor.
    pub fn mouse(&self) -> (MouseBehavior, bool) {
        self.client.mouse()
    }

    /// Frame-order step 2, first on the server, then on the client: fires
    /// the RemoteEvent messages that arrived for that side, then moves its
    /// clock forward by `seconds` and resumes every paused script whose
    /// wait is over, and every `task.delay` whose time has come.
    ///
    /// So a message sent before this step (during step 1, or by the server
    /// during this step) arrives in this step; anything later, next frame.
    pub fn step(&mut self, world: &mut World, seconds: f64) {
        profiling::scope!("ScriptRuntime::step");
        self.fire_changes(world);
        self.fire_chat(world);
        // Players' data on a server is written once a second at most.
        self.save_clock += seconds.max(0.0);
        if self.save_clock >= 1.0 {
            self.save_clock = 0.0;
            if let Err(err) = self.saves.0.borrow_mut().flush_players() {
                self.output.push(OutputLine {
                    kind: OutputKind::Warning,
                    text: format!("SaveService: {err}"),
                    trace: Vec::new(),
                    side: None,
                });
            }
        }
        // A game with no LocalScripts still has its player.
        self.join(world);
        let assets = self
            .client
            .lua
            .app_data_ref::<Assets>()
            .map(|assets| assets.clone())
            .unwrap_or_default();
        // SplineMeshes and SplineScatters make their pieces again where a
        // spline changed (all of them the first frame), before scripts run.
        for problem in self.generators.update(world, &assets) {
            self.output.push(OutputLine {
                kind: OutputKind::Warning,
                text: problem,
                trace: Vec::new(),
                side: None,
            });
        }
        self.fire_changes(world);
        let destroyed = world.take_destroyed();
        for side in [Side::Server, Side::Client] {
            let messages = self.transport.receive(side);
            self.context(side)
                .step(world, messages, seconds, &destroyed);
            self.collect(side);
            self.fire_changes(world);
        }
        // SplineFollowers move on, after scripts that change them; a
        // device following a server sees the server's.
        if !self.client_only {
            for (id, event, args) in self.followers.step(world, seconds) {
                self.fire_instance_event(world, id, event, &args);
            }
        }
        // Then Sequences move on, after scripts that play them.
        let stepped = {
            let mut camera = self.client_camera();
            self.sequences
                .0
                .borrow_mut()
                .step(world, &assets, &mut camera, seconds)
        };
        self.after_sequences(world, stepped);
        // Volume sliders follow the volumes scripts set, and set the ones
        // the player dragged.
        self.volume_sliders.step(world);
        self.fire_changes(world);
    }

    /// Whether a Sequence with `LockPlayerInput` is playing (or paused):
    /// hosts then give the character controllers no movement, and scripts
    /// see only releases.
    pub fn input_locked(&self, world: &World) -> bool {
        self.sequences.0.borrow().input_locked(world)
    }

    /// The client's `workspace.CurrentCamera`.
    pub(crate) fn client_camera(&self) -> mlua::AppDataRefMut<'_, camera::Camera> {
        self.client
            .lua
            .app_data_mut::<camera::Camera>()
            .expect("the client side has a camera")
    }

    /// Does what stepping the Sequences left: Sounds, levels, warnings and
    /// their events.
    pub(crate) fn after_sequences(&mut self, world: &mut World, stepped: sequence::Stepped) {
        let actions = sequence::sound_actions(world, stepped.sounds);
        self.sound_actions.extend(actions);
        {
            let mut levels = self.levels.0.borrow_mut();
            for name in &stepped.remove_levels {
                let _ = levels::remove(&mut levels, world, name);
            }
            for name in &stepped.add_levels {
                if let Err(why) = levels::add(&mut levels, world, name) {
                    self.output.push(OutputLine {
                        kind: OutputKind::Warning,
                        text: format!("a sequence couldn't add a level: {why}"),
                        trace: Vec::new(),
                        side: None,
                    });
                }
            }
            if !(stepped.add_levels.is_empty() && stepped.remove_levels.is_empty()) {
                self.saves.0.borrow_mut().levels = levels.saved();
            }
        }
        for warning in stepped.warnings {
            self.output.push(OutputLine {
                kind: OutputKind::Warning,
                text: warning,
                trace: Vec::new(),
                side: None,
            });
        }
        for (id, event, args) in stepped.events {
            self.fire_instance_event(world, id, event, &args);
        }
    }

    /// Frame-order step 4: fires `Touched` on both parts of each pair that
    /// started touching, in the order given, in Scripts and then in
    /// LocalScripts. Each handler gets the other part and runs as its own
    /// coroutine, so one that waits doesn't hold up the rest. Pairs with a
    /// part that a handler destroyed are skipped.
    ///
    /// Changes physics made (parts moving) fire their change events first.
    pub fn fire_touched(&mut self, world: &mut World, touches: &[(InstanceId, InstanceId)]) {
        self.fire_touch_event(world, "Touched", touches);
    }

    /// Frame step 1, after the character controllers step: fires
    /// `Seated(active, seat)` on each controller that sat down (`true`) or
    /// stood up (`false`), in Scripts and then in LocalScripts.
    pub fn fire_seated(&mut self, world: &mut World, changes: &[(InstanceId, bool, InstanceId)]) {
        self.fire_changes(world);
        for side in [Side::Server, Side::Client] {
            self.context(side).fire_seated(world, changes);
            self.collect(side);
            self.fire_changes(world);
        }
    }

    /// Frame-order step 4, after [`fire_touched`](Self::fire_touched):
    /// fires `TouchEnded` on both parts of each pair that stopped
    /// touching, the same way.
    pub fn fire_touch_ended(&mut self, world: &mut World, ends: &[(InstanceId, InstanceId)]) {
        self.fire_touch_event(world, "TouchEnded", ends);
    }

    /// Frame-order step 3, before physics steps: hands the scripts'
    /// `fracture:Shatter(point, force)` calls to `physics`, which replaces
    /// the parts with their pieces now (Phase 42).
    pub fn apply_shatters(&mut self, world: &mut World, physics: &mut axcenity_physics::Physics) {
        for shatter in std::mem::take(&mut self.shatters) {
            physics.shatter(world, shatter.fracture, shatter.point, shatter.force);
        }
    }

    /// Frame-order step 4, after [`fire_touch_ended`](Self::fire_touch_ended):
    /// fires `Broken` on each joint or weld that broke in physics' steps,
    /// then `Shattered` on each Fracture whose part shattered (Phase 42).
    pub fn fire_breaks(&mut self, world: &mut World, physics: &mut axcenity_physics::Physics) {
        for constraint in physics.take_broken() {
            self.fire_instance_event(world, constraint, "Broken", &[]);
        }
        for fracture in physics.take_shattered() {
            self.fire_instance_event(world, fracture, "Shattered", &[]);
        }
    }

    /// Frame step 1, after [`input_events`](Self::input_events) (Phase
    /// 44): ProximityPrompts in reach of the player character show and
    /// follow their actions, and ClickDetectors follow the mouse, through
    /// the camera; their events fire with the Player, in Scripts and then
    /// in LocalScripts. Then [`shown_prompts`](Self::shown_prompts) and
    /// [`click_cursor`](Self::click_cursor) say what to draw.
    pub fn step_prompts(&mut self, world: &mut World, seconds: f64) {
        self.fire_changes(world);
        let input = self.client.prompt_input();
        // While a game system holds the keys (a dialogue runs, Phase 77),
        // prompts neither show nor trigger.
        if self.input_held() {
            self.step_systems_input(world, &input);
            return;
        }
        let ray = self.client.mouse_ray(input.mouse);
        let camera = ray.map(|ray| cframe::to_vec3(ray.origin));
        let assets = self.server.assets();
        let mut fired = self.prompts.step(world, &assets, &input, camera, seconds);
        fired.extend(self.clicks.step(world, &assets, &input, ray));
        self.step_systems_input(world, &input);
        if fired.is_empty() {
            return;
        }
        self.join(world);
        let player = self.local_player();
        for side in [Side::Server, Side::Client] {
            self.context(side).fire_prompt_events(world, &fired, player);
            self.collect(side);
            self.fire_changes(world);
        }
        for fired in fired.iter().filter(|fired| fired.event == "Triggered") {
            for system in self.systems.clone() {
                system.prompt_triggered(self, world, fired.instance, &input);
            }
        }
    }

    /// Each game system's [`GameSystem::step_input`], then the changes.
    pub(crate) fn step_systems_input(&mut self, world: &mut World, input: &input::PromptInput) {
        for system in self.systems.clone() {
            system.step_input(self, world, input);
            self.fire_changes(world);
        }
    }

    /// A player's request to activate `message.remote`, for this device's
    /// server: each game system's [`GameSystem::activation_requested`].
    pub(crate) fn activation_requested(&self, message: &Message) {
        for system in &self.systems {
            system.activation_requested(message.remote, message.player);
        }
    }

    /// For game systems: the player's keys, mouse and controller this
    /// frame, as ProximityPrompts and Tools read them.
    pub fn prompt_input(&self) -> PromptInput {
        self.client.prompt_input()
    }

    /// For game systems: the Humanoid `humanoid` died. If it was a
    /// player's character, they respawn after their `RespawnTime`.
    pub fn humanoid_died(&mut self, world: &World, humanoid: InstanceId) {
        self.players.died(world, humanoid);
    }

    /// For game systems: the side that runs what the player starts without
    /// a script, such as a prompt's dialogue: the server's, unless this
    /// device only follows one.
    pub fn acting_side(&self) -> Side {
        if self.client_only {
            Side::Client
        } else {
            Side::Server
        }
    }

    /// Whether a game system has the player's keys (a dialogue runs, Phase
    /// 77): prompts, ClickDetectors and Tools wait, and the character
    /// stands still.
    pub fn input_held(&self) -> bool {
        self.systems.iter().any(|system| system.holds_input())
    }

    /// Whether the player character should stand still: a Sequence locks
    /// input, or a game system holds it (a dialogue runs).
    pub fn movement_locked(&self, world: &World) -> bool {
        self.input_locked(world) || self.input_held()
    }

    /// Fires the waiting events ([`GameSystem::fire_events`]) of each game
    /// system that [fires first](GameSystem::fires_first) or, with `first`
    /// false, of the others, until none fire, up to the change-round limit.
    pub(crate) fn fire_system_events(&mut self, world: &mut World, first: bool) {
        let systems: Vec<_> = self
            .systems
            .iter()
            .filter(|system| system.fires_first() == first)
            .cloned()
            .collect();
        if systems.is_empty() {
            return;
        }
        for _ in 0..MAX_CHANGE_ROUNDS {
            let mut fired = false;
            for system in &systems {
                fired |= system.fire_events(self, world);
            }
            if !fired {
                return;
            }
        }
    }

    /// The ProximityPrompts showing, as of [`step_prompts`](Self::step_prompts).
    pub fn shown_prompts(&self) -> &[ShownPrompt] {
        if self.input_held() {
            return &[];
        }
        self.prompts.shown()
    }

    /// The cursor over the ClickDetector the mouse points at (its
    /// `CursorIcon`, a CSS cursor name, `pointer` by default), or `None`
    /// when it points at none.
    pub fn click_cursor(&self, world: &World) -> Option<String> {
        self.clicks.cursor(world)
    }

    pub(crate) fn fire_touch_event(
        &mut self,
        world: &mut World,
        event: &'static str,
        pairs: &[(InstanceId, InstanceId)],
    ) {
        self.fire_changes(world);
        for side in [Side::Server, Side::Client] {
            self.context(side).fire_touch_event(world, event, pairs);
            self.collect(side);
            self.fire_changes(world);
        }
    }

    /// Between frame steps 2 and 3, before physics: fires
    /// `RunService.Stepped` with the game time and the frame's length in
    /// seconds, in Scripts and then in LocalScripts.
    pub fn stepped(&mut self, world: &mut World, seconds: f64) {
        profiling::scope!("ScriptRuntime::stepped");
        self.fire_changes(world);
        for side in [Side::Server, Side::Client] {
            self.context(side).fire_stepped(world, seconds);
            self.collect(side);
            self.fire_changes(world);
        }
    }

    /// Frame step 6, just before the frame is built and drawn (and before
    /// [`Self::step_camera`]): runs the LocalScripts' functions bound with
    /// `RunService:BindToRenderStep` in priority order, then fires
    /// `RunService.RenderStepped`, each with the frame's length in seconds.
    pub fn render_step(&mut self, world: &mut World, seconds: f64) {
        self.fire_changes(world);
        self.client.render_step(world, seconds);
        self.collect(Side::Client);
        self.fire_changes(world);
    }

    /// Says the game runs in the editor's Play, for
    /// `RunService:IsStudio()`. The player never calls it.
    pub fn set_studio(&mut self, studio: bool) {
        let flags = run_service::RunFlags {
            studio,
            running: true,
        };
        self.server.set_run_flags(flags);
        self.client.set_run_flags(flags);
    }

    /// The end of frame step 4, after `Touched`: fires `RunService.Heartbeat`
    /// with the frame's length in seconds, in Scripts and then in LocalScripts.
    pub fn heartbeat(&mut self, world: &mut World, seconds: f64) {
        profiling::scope!("ScriptRuntime::heartbeat");
        self.fire_changes(world);
        for side in [Side::Server, Side::Client] {
            self.context(side).fire_heartbeat(world, seconds);
            self.collect(side);
            self.fire_changes(world);
        }
    }

    /// Delivers a button activation only to LocalScripts. A button whose
    /// `Interactable` is off doesn't fire.
    pub fn activate_ui_button(&mut self, world: &mut World, button: InstanceId) {
        let fires = world.get(button).is_some_and(|instance| {
            instance.class() == ClassName::TextButton
                && instance.property("Interactable").and_then(Value::as_bool) != Some(false)
        });
        if fires {
            self.fire_ui_event(world, button, "Activated");
        }
    }

    /// Fires `event` of a UI element (such as `MouseEnter`) only in
    /// LocalScripts. An element that's gone is skipped.
    pub fn fire_ui_event(&mut self, world: &mut World, element: InstanceId, event: &'static str) {
        self.fire_changes(world);
        if world.contains(element) {
            self.client.fire_instance_event(world, element, event);
            self.collect(Side::Client);
            self.fire_changes(world);
        }
    }

    /// Delivers `FocusLost` only to LocalScripts, when the player finishes a TextBox.
    pub fn release_text_box(&mut self, world: &mut World, text_box: InstanceId) {
        self.fire_changes(world);
        if world
            .get(text_box)
            .is_some_and(|instance| instance.class() == ClassName::TextBox)
        {
            self.client
                .fire_instance_event(world, text_box, "FocusLost");
            self.collect(Side::Client);
            self.fire_changes(world);
        }
    }

    /// Sounds that played to their end (see `GameAudio::update`): sets each
    /// one's `Playing` to false, then fires its `Ended` in Scripts and then
    /// in LocalScripts. Sounds a handler destroyed are skipped.
    pub fn fire_sounds_ended(&mut self, world: &mut World, sounds: &[InstanceId]) {
        self.fire_changes(world);
        for &sound in sounds {
            if world
                .set_property(sound, "Playing", Value::Bool(false))
                .is_err()
            {
                continue;
            }
            self.fire_changes(world);
            for side in [Side::Server, Side::Client] {
                self.context(side)
                    .fire_instance_event(world, sound, "Ended");
                self.collect(side);
                self.fire_changes(world);
            }
        }
    }

    /// Fires `event` of `instance` with `args`, in Scripts and then in
    /// LocalScripts, for events the host finds, such as an Animator's
    /// `StateChanged` (new state, old state). An Instance a handler
    /// destroyed is skipped.
    pub fn fire_instance_event(
        &mut self,
        world: &mut World,
        instance: InstanceId,
        event: &'static str,
        args: &[String],
    ) {
        self.fire_changes(world);
        for side in [Side::Server, Side::Client] {
            self.context(side)
                .fire_instance_event_with(world, instance, event, args);
            self.collect(side);
            self.fire_changes(world);
        }
    }

    /// Fires `Changed`, `GetPropertyChangedSignal`, `ChildAdded` and
    /// `ChildRemoved` for everything that changed in `world` since the last
    /// call, in the order it happened, in Scripts and then in LocalScripts.
    ///
    /// Every other method here calls this before and after running scripts,
    /// so the host never has to. Changes made while handlers run fire too,
    /// up to [`MAX_CHANGE_ROUNDS`] rounds; the rest wait for the next call.
    /// Only changes made after the first call are seen: it turns on the
    /// World's change log.
    ///
    /// Then scripts whose Instance was destroyed stop, and cloned scripts
    /// that are now where scripts run start.
    pub fn fire_changes(&mut self, world: &mut World) {
        self.fire_change_rounds(world);
        // Abilities' and Health's first, so Health events Effects cause
        // fire here, and before Players see a character die.
        self.fire_system_events(world, true);
        self.fire_player_events(world);
        self.fire_level_events(world);
        self.fire_system_events(world, false);
        self.stop_destroyed_scripts(world);
        self.start_clones(world);
    }

    /// Fires `Changed`, `ChildAdded` and the other change events for what
    /// changed, in rounds, without the engine events
    /// [`fire_changes`](Self::fire_changes) adds; for a game system's
    /// [`GameSystem::fire_events`], which `fire_changes` calls.
    pub fn fire_change_rounds(&mut self, world: &mut World) {
        world.record_changes();
        for _ in 0..MAX_CHANGE_ROUNDS {
            let changes = world.take_changes();
            if changes.is_empty() {
                return;
            }
            self.note_arrived_scripts(world, &changes);
            for side in [Side::Server, Side::Client] {
                self.context(side).fire_changes(world, &changes);
                self.collect(side);
            }
        }
        if world.has_changes() && !self.reported_change_loop {
            self.reported_change_loop = true;
            self.output.push(OutputLine {
                kind: OutputKind::Error,
                text: format!(
                    "Changed, ChildAdded or ChildRemoved handlers kept changing things \
                     that fire more of them ({MAX_CHANGE_ROUNDS} rounds in a row), so the \
                     rest will fire next frame. Look for a handler that changes the \
                     property it listens to."
                ),
                trace: Vec::new(),
                side: None,
            });
        }
    }

    /// Stops every started script whose Instance is gone (`Destroy`). A
    /// script that destroyed itself has finished its current run by now,
    /// so this is "after the current resume".
    pub(crate) fn stop_destroyed_scripts(&mut self, world: &World) {
        let (server, client) = (&self.server, &self.client);
        self.started.retain(|started| {
            let alive = world.contains(started.script);
            if !alive {
                server.stop_script(started.script);
                client.stop_script(started.script);
            }
            alive
        });
    }

    /// Starts each waiting script (a clone, or a template inside an object)
    /// that's now where scripts of its kind run (`World::runs_scripts_at`). The
    /// rest wait for the next call; those destroyed before they ran are
    /// forgotten.
    pub(crate) fn start_clones(&mut self, world: &mut World) {
        if self.starting_clones || self.clones.is_empty() {
            return;
        }
        self.starting_clones = true;
        for clone in std::mem::take(&mut self.clones) {
            if !world.contains(clone.script) {
                continue;
            }
            if world.runs_scripts_at(clone.script) {
                self.start_script(world, clone.script, &clone.path, &clone.source);
            } else {
                self.clones.push(clone);
            }
        }
        self.starting_clones = false;
    }
}
