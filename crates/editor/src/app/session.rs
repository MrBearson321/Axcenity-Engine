//! Opening and closing projects, Play and Stop, and the "Save changes?" prompt.
//!
//! Play copies the world into a session. Edits made while playing go to that
//! copy and are dropped on Stop. These are methods of [`super::EditorApp`].

use super::*;

impl EditorApp {
    /// Opens the project in `folder` in place of the current one, before
    /// returning. If it can't be loaded, the error goes to Output and the
    /// current project stays open. [`Self::begin_open`] reads it in the
    /// background instead.
    pub fn open(&mut self, folder: &Path) {
        match axcenity_project::load(folder) {
            Ok(project) => {
                let assets = Assets::new(&project.folder);
                self.open_loaded(project, assets);
            }
            // `{:#}` on an `anyhow::Error` shows the whole chain of causes
            // on one line, such as which line of which file is wrong.
            Err(err) => self.open_failed(folder, &format!("{:#}", anyhow::Error::new(err))),
        }
    }
    /// Starts opening the project in `folder` in the background, with the
    /// loading screen over the window until it's read (see `loading.rs`).
    /// It opens in [`Self::follow_loading`].
    pub fn begin_open(&mut self, ctx: &egui::Context, folder: &Path) {
        if self.loading.is_some() {
            return;
        }
        let mut terrain = TerrainCache::new();
        terrain.view = self.viewport.terrain.view;
        terrain.detail_distance = self.viewport.terrain.detail_distance;
        self.loading = Some(Loading::start(ctx, folder, terrain, viewport::start_view()));
    }
    /// Opens the project being loaded once its thread is done. Returns true
    /// while it's still loading, when only the loading screen is drawn.
    pub(super) fn follow_loading(&mut self, ctx: &egui::Context) -> bool {
        let Some(result) = self.loading.as_ref().map(Loading::poll) else {
            return false;
        };
        let Some(result) = result else {
            return true;
        };
        let loading = self.loading.take().expect("checked above");
        match result {
            Ok(loaded) => {
                self.open_loaded(loaded.project, loaded.assets);
                // After opening, which forgets the old project's terrains.
                let mut terrain = loaded.terrain;
                terrain.view = self.viewport.terrain.view;
                terrain.detail_distance = self.viewport.terrain.detail_distance;
                self.viewport.terrain = terrain;
                self.cover_until = Some(ctx.cumulative_frame_nr() + FINISHING_FRAMES);
                self.follow_pending_handoff(true);
            }
            Err(err) => {
                self.open_failed(&loading.folder, &err);
                self.follow_pending_handoff(false);
                // Output isn't shown without a project, so say it on the start screen.
                if self.project.is_none()
                    && let Some(line) = self.output.lines().last()
                {
                    self.start.set_notice(line.text.clone());
                }
            }
        }
        ctx.request_repaint();
        false
    }
    /// Only the header and the loading screen, while a project loads.
    pub(super) fn loading_frame(&mut self, ctx: &egui::Context) {
        let palette = theme::palette(ctx);
        egui::TopBottomPanel::top("menu bar")
            .exact_height(window_frame::HEIGHT)
            .frame(egui::Frame::new().fill(palette.app_bg))
            .show(ctx, |ui| self.title_bar(ui));
        let cancel = egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                let loading = self.loading.as_ref().expect("only drawn while loading");
                loading.show(ui, ui.max_rect())
            })
            .inner;
        if cancel {
            // Dropping it tells the thread to stop.
            self.loading = None;
            ctx.request_repaint();
        }
        self.unsaved_changes_prompt(ctx);
        self.settings_window(ctx);
        self.toasts.show(ctx);
        theme::paint_crossfade(ctx);
    }
    /// For a frame or two after a project opens, the loading screen stays
    /// over the window while its scene is sent to the graphics card.
    pub(super) fn cover_while_finishing(&mut self, ctx: &egui::Context) {
        let Some(until) = self.cover_until else {
            return;
        };
        if ctx.cumulative_frame_nr() >= until {
            self.cover_until = None;
            return;
        }
        let mut rect = ctx.screen_rect();
        rect.min.y += window_frame::HEIGHT;
        let name = self
            .project
            .as_ref()
            .map_or("", |project| project.name.as_str());
        egui::Area::new(egui::Id::new("finishing load"))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .constrain(false)
            .show(ctx, |ui| loading::show_finishing(ui, rect, name));
        ctx.request_repaint();
    }
    fn open_failed(&mut self, folder: &Path, err: &str) {
        self.output.error(format!(
            "Could not open the project in `{}`: {err}",
            folder.display()
        ));
    }
    /// Opens `project`, read with its models and textures into `assets`.
    fn open_loaded(&mut self, project: axcenity_project::Project, assets: Assets) {
        if let Some(from) = project.migrated_from {
            let kept = project
                .migration_backup
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| axcenity_project::PREVIOUS_FORMAT_DIR.to_owned());
            self.output.info(format!(
                "Upgraded \"{}\" from project format {from} to {}. The previous files are kept in `{kept}`.",
                project.manifest.name,
                axcenity_project::FORMAT_VERSION,
            ));
        }

        let scripts = project.scripts.len();
        let show_splash = project.manifest.show_splash;
        let loading_screen = project.manifest.loading_screen.clone();
        self.output.info(format!(
            "Opened \"{}\" from `{}`: {} objects, {scripts} {}. Press Play (F5) \
             to run it.",
            project.manifest.name,
            project.folder.display(),
            // `game` and its services are always there, so don't count them.
            project
                .world
                .descendants(project.world.root())
                .into_iter()
                .filter(|&id| project
                    .world
                    .get(id)
                    .is_some_and(|instance| !instance.class().is_service()))
                .count(),
            if scripts == 1 { "script" } else { "scripts" },
        ));
        files::report_problems(&mut self.output, &project);
        let mut stylesheets = Vec::new();
        let stylesheet_texts = project
            .stylesheets
            .iter()
            .map(|file| (file.path.clone(), file.source.clone()))
            .collect();
        for file in &project.stylesheets {
            let (stylesheet, warnings) = axcenity_ui::parse_stylesheet(&file.path, &file.source);
            stylesheets.push(stylesheet);
            for warning in warnings {
                self.output
                    .warning(format!("{}: {}", warning.path, warning.message));
            }
        }
        // A project set up for autocomplete by an older engine gets this
        // engine's API.
        match autocomplete::refresh(&project.folder) {
            Ok(autocomplete::Refresh::Updated) => self.output.info(format!(
                "Updated `{}` to this version of the engine's script API.",
                autocomplete::DEFINITIONS_FILE
            )),
            Ok(autocomplete::Refresh::OldSetUp) => self.output.info(
                "This project's VS Code autocomplete uses the old Lua 5.4 set-up, which \
                 flags Luau such as `+=`. Choose File > Set Up VS Code Autocomplete to move \
                 it to luau-lsp."
                    .to_owned(),
            ),
            Ok(autocomplete::Refresh::Unchanged) => {}
            Err(err) => self.output.error(format!(
                "Could not update `{}`: {err}",
                autocomplete::DEFINITIONS_FILE
            )),
        }
        // The game being played belongs to the old project, and so do the models.
        self.play = None;
        self.thumbnail = None;
        self.thumbnail_wanted = None;
        self.pending_ui_activation = None;
        self.pending_ui_events.clear();
        self.focused_text = None;
        self.pending_focus_lost = None;
        self.assets = assets;
        self.asset_browser.open(Some(&project.folder));
        self.tabs.close_all();
        self.viewport.reset_camera();
        let world_text = files::read_world_text(&project.folder);
        // Saved as a full path, so it opens from the start screen wherever
        // the editor is started.
        let folder = std::path::absolute(&project.folder).unwrap_or(project.folder.clone());
        start::remember(
            &mut self.recent,
            ProjectLink {
                name: project.manifest.name.clone(),
                description: project.manifest.description.clone(),
                folder,
            },
        );
        let block_scripts = project
            .block_scripts
            .into_iter()
            .map(|file| BlockDocument {
                instance: file.instance,
                path: file.path,
                script: file.script.clone(),
                saved: file.script,
                highlighted: None,
            })
            .collect();
        let ui_graphs = ui_graphs::documents(&project.world, project.ui_graphs);
        self.project = Some(OpenProject {
            name: project.manifest.name,
            show_splash,
            loading_screen,
            input_map: project.input_map,
            watcher: Watcher::start(project.folder.clone()),
            after_play: Vec::new(),
            folder: project.folder,
            stylesheets,
            stylesheet_texts,
            scripts: project.scripts,
            block_scripts,
            ui_graphs,
            remembered_blocks: HashMap::new(),
            world_text,
            keep_world_file: !project.world_loaded,
            damaged_terrains: project.damaged_terrains,
            saved_terrains: HashMap::new(),
        });
        self.animation = AnimationStudio::default();
        self.behavior = BehaviorTreeStudio::default();
        self.asset_check.clear();
        self.replace_world(project.world);
    }
    /// Puts `world` in place of the world being edited, with nothing
    /// selected and a new undo history (its steps belong to the old world).
    pub(super) fn replace_world(&mut self, world: World) {
        self.world = world;
        self.selected = None;
        self.selection.clear();
        self.history = History::new();
        self.explorer = Explorer::new(&self.world);
        self.forget_terrain_work();
        if let Some(project) = &mut self.project {
            project.saved_terrains = terrain::terrain_revisions(&self.world);
        }
    }
    /// File > Set Up VS Code Autocomplete: writes the files luau-lsp needs
    /// into the project, and says what to do next.
    pub(super) fn set_up_autocomplete(&mut self) {
        let Some(project) = &self.project else {
            return;
        };
        match autocomplete::set_up(&project.folder) {
            Ok(result) => {
                let mut message = format!(
                    "Set up autocomplete: wrote {}.",
                    quoted_list(&result.written)
                );
                if !result.updated.is_empty() {
                    message.push_str(&format!(
                        " Added the engine's settings to your own {}.",
                        quoted_list(&result.updated)
                    ));
                }
                if !result.kept.is_empty() {
                    message.push_str(&format!(" Kept your own {}.", quoted_list(&result.kept)));
                }
                if !result.removed.is_empty() {
                    message.push_str(&format!(
                        " Removed the old Lua Language Server {}.",
                        quoted_list(&result.removed)
                    ));
                }
                message.push_str(
                    " Open the project folder in VS Code and install the Luau extension \
                     (JohnnyMorganz.luau-lsp) when it asks; scripts then autocomplete the \
                     engine's API.",
                );
                if !result.not_json.is_empty() {
                    message.push_str(&format!(
                        " Couldn't add the engine's settings to {}: not plain JSON (it may \
                         have comments). Remove the comments and set up again.",
                        quoted_list(&result.not_json)
                    ));
                }
                self.output.info(message);
            }
            Err(err) => self.output.error(format!(
                "Could not set up autocomplete in `{}`: {err}",
                project.folder.display()
            )),
        }
    }
    /// Asks for a project folder with the system's folder picker, then opens it.
    pub(super) fn open_with_dialog(&mut self, ctx: &egui::Context) {
        let mut dialog = rfd::FileDialog::new().set_title("Open an Axcenity project folder");
        if let Some(project) = &self.project {
            dialog = dialog.set_directory(&project.folder);
        }
        // `None` means the dialog was cancelled.
        if let Some(folder) = dialog.pick_folder() {
            self.begin_open(ctx, &folder);
        }
    }
    pub(super) fn playing(&self) -> bool {
        self.play.is_some()
    }
    /// Starts the game: saves the project if it has unsaved changes, then
    /// runs its scripts and physics on a copy of the world. If saving fails,
    /// Play doesn't start, so a crash during Play can't lose the changes.
    pub(super) fn play(&mut self) {
        if self.playing() || self.viewport.is_dragging() || self.project.is_none() {
            return;
        }
        // A half-typed value is part of the world being saved, so end its step.
        self.history.seal();
        // The game has its own audio; a preview would play over it.
        self.sound_preview = None;
        // A stroke ends, and a terrain preview is dropped: Play shows the world.
        self.put_terrain_tools_down();
        // The game reads clips from their files.
        if let Some(project) = &self.project {
            let _ = self.animation.write_all(&project.folder, &mut self.output);
        }
        if self.unsaved() && !self.save() {
            self.output
                .error("Play didn't start, because the project couldn't be saved first.");
            return;
        }
        self.start_session(None);
    }
    /// Starts the game on a copy of the edit world, or on `resumed`: a game
    /// world carried over from before a relaunch (see `engine.rs`), whose
    /// Instances have the edit world's IDs where they match. A resumed game
    /// doesn't show the loading screen again.
    pub(super) fn start_session(&mut self, resumed: Option<World>) {
        let Some(project) = &self.project else {
            return;
        };
        // Show the game, not a script.
        self.tabs.show_world();
        self.layout.open(Pane::Scene);
        if resumed.is_some() {
            self.output.info(format!(
                "Play continues in \"{}\" from where it was; its scripts started again. \
                 Press Stop (Shift+F5) to go back to editing.",
                project.name
            ));
        } else {
            self.output.info(format!(
                "Playing \"{}\". Press Stop (Shift+F5) to go back to editing; the world \
                 will be as it was before Play.",
                project.name
            ));
        }
        let no_loading_screen = axcenity_project::LoadingScreen::default();
        let loading_screen = if resumed.is_some() {
            &no_loading_screen
        } else {
            &project.loading_screen
        };
        let game = resumed.as_ref().unwrap_or(&self.world);
        // `project` borrows `self.project` and `output` borrows `self.output`:
        // different fields, so Rust allows both borrows at once.
        let scripts = play::read_scripts(
            &project.folder,
            &project.scripts,
            &self.world,
            &mut self.output,
        );
        let (position, orientation) = self.viewport.camera_view();
        match Session::start(
            game,
            &scripts,
            self.tabs.breakpoints(),
            &mut self.output,
            self.assets.clone(),
            CameraState {
                position,
                orientation,
            },
            loading_screen,
            &project.input_map,
            play_save_folder(&project.name),
            &self.graphics,
            &self.capabilities.unwrap_or_default(),
        ) {
            Ok(session) => {
                self.play = Some(session);
                self.viewport.clear_particles();
                self.viewport.clear_ui();
                self.pending_emits.clear();
                self.pending_ui_activation = None;
                self.pending_ui_events.clear();
                self.focused_text = None;
                self.pending_focus_lost = None;
                self.last_frame = None;
                self.game_keys = GameKeys::default();
                self.profiler.start();
                // A breakpoint in a script's first run pauses before any frame.
                self.tabs.take_breakpoints_changed();
                self.follow_pause();
            }
            Err(crash) => self.crashed(crash),
        }
    }
    /// Whether Play is paused at a breakpoint.
    pub(super) fn paused(&self) -> bool {
        self.tabs.paused.is_some()
    }
    /// After the game ran scripts: if it has just paused at a breakpoint,
    /// says so and shows that line in its tab, with the locals under it.
    pub(super) fn follow_pause(&mut self) {
        let pause = self.play.as_ref().and_then(Session::pause);
        if pause == self.tabs.paused {
            return;
        }
        self.tabs.paused = pause.clone();
        if let Some(pause) = pause {
            self.output.info(format!(
                "Paused at {}:{}, before that line runs; its locals are under it in its \
                 tab. Press Continue (F5) to carry on, or Stop (Shift+F5).",
                pause.path, pause.line
            ));
            self.open_script(&pause.path, Some(pause.line));
        }
    }
    /// Continues the game from the breakpoint it's paused at. It may pause
    /// again straight away, at the next breakpoint it reaches.
    pub(super) fn continue_play(&mut self) {
        if !self.paused() {
            return;
        }
        let Some(session) = &mut self.play else {
            return;
        };
        let result = session.continue_from_pause(&mut self.output);
        // The paused time isn't game time: the next frame is a normal one.
        self.last_frame = Some(Instant::now());
        self.tabs.paused = None;
        match result {
            Ok(()) => self.follow_pause(),
            Err(crash) => self.crashed(crash),
        }
    }
    /// Gives the game breakpoints that changed. Clearing the one Play is
    /// paused at continues.
    pub(super) fn sync_breakpoints(&mut self) {
        if !self.tabs.take_breakpoints_changed() {
            return;
        }
        let Some(session) = &mut self.play else {
            return;
        };
        session.set_breakpoints(self.tabs.breakpoints());
        let breakpoints = self.tabs.breakpoints();
        let cleared = self.tabs.paused.as_ref().is_some_and(|pause| {
            !breakpoints
                .get(&pause.path)
                .is_some_and(|lines| lines.contains(&pause.line))
        });
        if cleared {
            self.continue_play();
        }
    }
    /// Stops the game and throws its world away. The world being edited was
    /// never touched, so it's back exactly as before Play, except for the
    /// values kept from Play, which are copied in as one undo step.
    pub(super) fn stop(&mut self) {
        self.tabs.paused = None;
        if let Some(session) = self.play.take() {
            self.viewport.clear_particles();
            self.viewport.clear_ui();
            self.pending_emits.clear();
            let kept = self.keep_play_edits(&session);
            let gone = session.kept().made_in_play(&self.world);
            drop(session);
            if kept == 0 {
                self.output
                    .info("Stopped. The world is back as it was before Play.");
            } else {
                self.output.info(format!(
                    "Stopped. Kept {kept} {} from Play (Undo takes {}); everything else is \
                     back as it was before Play.",
                    if kept == 1 { "value" } else { "values" },
                    if kept == 1 { "it back" } else { "them back" },
                ));
            }
            if gone > 0 {
                self.output.warning(format!(
                    "{gone} kept {} made during Play, so {} gone with the game.",
                    if gone == 1 {
                        "object was"
                    } else {
                        "objects were"
                    },
                    if gone == 1 { "it's" } else { "they're" },
                ));
            }
            self.focused_text = None;
            self.pending_focus_lost = None;
            self.forget_selection_outside_tree();
        }
    }
    /// Copies the values kept from Play (see `play::kept`) from the game's
    /// world into the edit world, as one undo step. Returns how many.
    fn keep_play_edits(&mut self, session: &Session) -> usize {
        let kept = session.kept().values(session.world(), &self.world);
        if kept.is_empty() {
            return 0;
        }
        let label = match kept.as_slice() {
            [only] => format!(
                "Keep Play changes to {}",
                session
                    .world()
                    .get(only.id)
                    .map_or("", |instance| instance.name())
            ),
            many => format!("Keep Play changes to {} objects", many.len()),
        };
        let mut count = 0;
        let mut changed = Vec::new();
        for item in kept {
            let before = history::snapshot(&self.world, item.id);
            for (field, value) in item.values {
                let edit = match field {
                    Field::Name => Edit::Name(value.as_str().unwrap_or_default().to_owned()),
                    Field::Property(name) => Edit::Property(name, value),
                };
                match edits::set(&mut self.world, item.id, edit) {
                    Ok(()) => count += 1,
                    Err(err) => self
                        .output
                        .error(format!("Could not keep a Play change: {err:#}")),
                }
            }
            let after = history::snapshot(&self.world, item.id);
            changed.push(history::Changed {
                id: item.id,
                before,
                after,
            });
        }
        // Its own step, never merged into the last edit before Play.
        self.history.seal();
        self.history
            .record_many(&mut self.world, Field::Name, label, changed);
        self.history.seal();
        count
    }
    /// During Play, takes the key presses that move the game UI's focus out
    /// of `input` (see `UiInteraction::takes`), for `navigate_play_ui`:
    /// Tab when there's something to focus, and the arrows, Enter and
    /// Escape while something is. Not while a game text box or one of the
    /// editor's own widgets has the keyboard.
    pub(super) fn take_nav_keys(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        if !self.playing()
            || self.paused()
            || self.focused_text.is_some()
            || ctx.memory(|memory| memory.focused().is_some())
        {
            return;
        }
        let Some(session) = &self.play else {
            return;
        };
        let (world, viewport) = (session.world(), &self.viewport);
        let map = session.input_map();
        let mut taken = Vec::new();
        let mut heard = Vec::new();
        input.events.retain(|event| {
            let egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } = event
            else {
                return true;
            };
            match crate::play::nav_key(&map, *key, modifiers.shift) {
                Some(nav) if viewport.ui_takes(world, nav) => {
                    taken.push(nav);
                    // Scripts still hear it, as the UI's.
                    heard.extend(crate::play::script_key(*key).map(|key| {
                        axcenity_core::input::InputEvent::processed(
                            axcenity_core::input::InputKind::Key { key, pressed: true },
                        )
                    }));
                    false
                }
                _ => true,
            }
        });
        self.nav_keys.extend(taken);
        self.pending_game_input.extend(heard);
    }

    /// Moves the game UI's focus by the keys `take_nav_keys` took and the
    /// game controllers, `seconds` after the last frame. Confirm on a
    /// button fires `Activated`, and on a text box starts typing in it;
    /// while typing, a controller's Confirm or Back finishes it.
    pub(super) fn navigate_play_ui(&mut self, seconds: f32) {
        let mut keys = std::mem::take(&mut self.nav_keys);
        if let Some(session) = &self.play {
            // What the UI takes moves focus; the rest is the game's.
            let mut pads = self
                .gamepads
                .get_or_insert_with(axcenity_ui::Gamepads::new)
                .poll();
            keys.extend(self.pad_nav.update(
                self.viewport.ui(),
                session.world(),
                &session.input_map(),
                &mut pads,
                seconds,
            ));
            self.pending_game_input.extend(pads);
        }
        for key in keys {
            if let Some(id) = self.focused_text {
                if matches!(
                    key,
                    axcenity_ui::NavKey::Confirm | axcenity_ui::NavKey::Back
                ) {
                    self.focused_text = None;
                    self.pending_focus_lost = Some(id);
                }
                continue;
            }
            let Some(session) = &mut self.play else {
                return;
            };
            let done = self.viewport.navigate_ui(session.world(), key);
            done.apply(session.world_mut());
            if let Some(button) = done.activated
                && let Err(crash) = session.activate_ui_button(button, &mut self.output)
            {
                return self.crashed(crash);
            }
            if done.text_box.is_some() {
                self.focused_text = done.text_box;
            }
        }
    }

    /// Runs one frame of the game, `seconds` long, with the keys pressed and
    /// released since the last one. LocalScripts see the viewport's camera
    /// as `workspace.CurrentCamera`, and can move it. A crash stops the game.
    pub(super) fn play_frame(
        &mut self,
        seconds: f64,
        input: &[axcenity_core::input::InputEvent],
        activation: Option<InstanceId>,
        focus_lost: Option<InstanceId>,
    ) {
        let patched = self.rebuilds.patched_frames();
        let Some(session) = &mut self.play else {
            return;
        };
        let events = std::mem::take(&mut self.pending_ui_events);
        if !events.is_empty()
            && let Err(crash) = session.ui_events(events, &mut self.output)
        {
            self.crashed(crash);
            return;
        }
        if let Some(button) = activation
            && let Err(crash) = session.activate_ui_button(button, &mut self.output)
        {
            self.crashed(crash);
            return;
        }
        if let Some(text_box) = focus_lost
            && let Err(crash) = session.release_text_box(text_box, &mut self.output)
        {
            self.crashed(crash);
            return;
        }
        let (width, height) = self.viewport.pixel_size();
        session.set_viewport_size(width as f32, height as f32);
        let (position, orientation) = self.viewport.camera_view();
        let camera = CameraState {
            position,
            orientation,
        };
        let (look_dx, look_dy) = self.viewport.take_character_look();
        let zoom = self.viewport.take_character_zoom();
        // The controllers read the input map's actions, with the player's
        // rebinds, as in the player.
        self.game_keys.follow(input);
        let motion = self.game_keys.motion(
            &session.input_map(),
            [look_dx, look_dy],
            zoom,
            seconds as f32,
        );
        let motion = if self.focused_text.is_some() {
            axcenity_character::MoveInput::default()
        } else {
            motion
        };
        // The game's steps (character, physics, scripts) run the newest
        // code dx patched in, so Play carries on across a patch.
        let frame = hot_patch::call(patched, || {
            session.frame(seconds, input, motion, camera, &mut self.output)
        });
        if let Some(cost) = session.cost() {
            self.profiler.record_game(cost, session.load());
        }
        self.pending_emits.extend(session.take_particle_emits());
        match frame {
            Ok(Some(moved)) => self
                .viewport
                .set_camera_view(moved.position, moved.orientation),
            Ok(None) => {}
            Err(crash) => return self.crashed(crash),
        }
        self.follow_pause();
    }
    pub(super) fn update_block_lua(&mut self, path: &str) {
        let Some(source) = self.project.as_ref().and_then(|project| {
            let document = project
                .block_scripts
                .iter()
                .find(|document| document.path == path)?;
            Some(translate(&document.script).ok()?.source)
        }) else {
            return;
        };
        if let Some(project) = &mut self.project
            && let Some(script) = project
                .scripts
                .iter_mut()
                .find(|script| script.path == path)
        {
            script.source = source.clone();
        }
        if let Some(tab) = self.tabs.get_mut(path) {
            tab.text = source;
        }
    }
    /// Works out which scripts use an API that only works on the other side
    /// (from the code in their tab, if open), for the warning badges in the
    /// Explorer and on the tabs.
    pub(super) fn update_warnings(&mut self) {
        let mut badges = HashMap::new();
        let mut by_path = HashMap::new();
        if let Some(project) = &self.project {
            for script in &project.scripts {
                let source = self
                    .tabs
                    .get(&script.path)
                    .map_or(script.source.as_str(), |tab| tab.text.as_str());
                let warnings = self.warnings.get(&script.path, source);
                if !warnings.is_empty() {
                    badges.insert(script.instance, side_warnings::badge_text(warnings));
                    by_path.insert(script.path.clone(), warnings.to_vec());
                }
            }
        }
        self.explorer.warnings = badges;
        self.tabs.warnings = by_path;
    }
    /// The engine panicked during Play: say so, and go back to editing.
    pub(super) fn crashed(&mut self, crash: Crash) {
        self.play = None;
        self.tabs.paused = None;
        self.output.error(format!(
            "The engine crashed during Play: {}. Play stopped, and the world is back \
             as it was before Play. This is a bug in Axcenity; if you started the \
             editor from a terminal, it shows where it happened.",
            crash.0
        ));
        self.forget_selection_outside_tree();
    }
    /// Asks to `discard` the open world. Returns it when it can go ahead now,
    /// or `None` after putting up "Save changes?" because there are unsaved changes.
    pub(super) fn request(&mut self, discard: Discard) -> Option<Discard> {
        let unsaved = match &discard {
            Discard::CloseTab(path) => {
                self.tabs.get(path).is_some_and(|tab| tab.unsaved())
                    || self.project.as_ref().is_some_and(|project| {
                        project.block_scripts.iter().any(|document| {
                            document.path == *path && document.script != document.saved
                        })
                    })
            }
            Discard::Quit
            | Discard::OpenAnother
            | Discard::OpenSample(_)
            | Discard::CloseProject
            | Discard::NewProject => self.unsaved(),
        };
        if unsaved {
            self.asking = Some(discard);
            None
        } else {
            Some(discard)
        }
    }
    /// Takes the answer to "Save changes?" and returns what to go ahead with,
    /// if anything. A failed save goes nowhere, so the changes aren't lost.
    pub(super) fn answer(&mut self, answer: Answer) -> Option<Discard> {
        let discard = self.asking.take()?;
        match answer {
            // Closing a tab only needs that script saved.
            Answer::Save => match &discard {
                Discard::CloseTab(path) => self.save_tab(path).then_some(discard),
                Discard::Quit
                | Discard::OpenAnother
                | Discard::OpenSample(_)
                | Discard::CloseProject
                | Discard::NewProject => self.save().then_some(discard),
            },
            Answer::DontSave => {
                if let Discard::CloseTab(path) = &discard {
                    let source = self
                        .project
                        .as_mut()
                        .and_then(|project| {
                            project
                                .block_scripts
                                .iter_mut()
                                .find(|document| document.path == *path)
                        })
                        .map(|document| {
                            document.script = document.saved.clone();
                            translate(&document.script)
                        });
                    match source {
                        Some(Ok(translation)) => {
                            if let Some(script) = self.project.as_mut().and_then(|project| {
                                project
                                    .scripts
                                    .iter_mut()
                                    .find(|script| script.path == *path)
                            }) {
                                script.source = translation.source;
                            }
                        }
                        Some(Err(err)) => self.output.error(format!(
                            "Could not restore the saved blocks in `{path}`: {err}"
                        )),
                        None => {}
                    }
                }
                Some(discard)
            }
            Answer::Cancel => None,
        }
    }
    pub(super) fn go_ahead(&mut self, ctx: &egui::Context, discard: Discard) {
        match discard {
            Discard::Quit => {
                self.closing = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Discard::OpenAnother => self.open_with_dialog(ctx),
            Discard::OpenSample(folder) => self.begin_open(ctx, &folder),
            Discard::CloseProject => self.close_project(),
            // The start screen's New Project dialog, once this one closes.
            Discard::NewProject => {
                self.close_project();
                self.start
                    .new_project(axcenity_project::Template::Baseplate);
            }
            Discard::CloseTab(path) => self.tabs.close(&path),
        }
    }
    /// Goes back to the start screen. The project's thumbnail is taken first
    /// if the viewport is showing, and the project closes once it's saved.
    pub(super) fn close_project(&mut self) {
        self.thumbnail_wanted = Some(true);
    }
    /// Puts the open project away and shows the start screen.
    pub(super) fn finish_closing_project(&mut self) {
        if let Some(project) = &self.project {
            let _ = self.animation.write_all(&project.folder, &mut self.output);
        }
        self.animation = AnimationStudio::default();
        self.asset_check.clear();
        self.project = None;
        self.play = None;
        self.pending_ui_activation = None;
        self.pending_ui_events.clear();
        self.focused_text = None;
        self.pending_focus_lost = None;
        self.thumbnail = None;
        self.thumbnail_wanted = None;
        self.assets = Assets::default();
        self.asset_browser.open(None);
        self.tabs.close_all();
        self.replace_world(World::new());
        self.start.reload_thumbnails();
    }
    /// Asks for a screenshot when a thumbnail is wanted and the viewport is
    /// showing. Without a viewport to photograph, a pending close just happens.
    pub(super) fn start_thumbnail(&mut self, ctx: &egui::Context) {
        let Some(then_close) = self.thumbnail_wanted.take() else {
            return;
        };
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        match self.viewport_rect {
            Some(area) if !self.playing() => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
                self.thumbnail = Some(ThumbnailShot {
                    folder,
                    area,
                    then_close,
                });
            }
            _ if then_close => self.finish_closing_project(),
            _ => {}
        }
    }
    /// Saves the screenshot asked for by [`Self::start_thumbnail`], when it arrives.
    pub(super) fn finish_thumbnail(&mut self, ctx: &egui::Context) {
        let screenshot = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        let (Some(image), Some(shot)) = (screenshot, self.thumbnail.take()) else {
            return;
        };
        let png = thumbnail::encode(&image, shot.area, ctx.pixels_per_point());
        if let Some(png) = png
            && let Err(err) =
                std::fs::write(shot.folder.join(axcenity_project::THUMBNAIL_FILE), png)
        {
            log::warn!("could not save the project thumbnail: {err}");
        }
        if shot.then_close {
            self.finish_closing_project();
        }
    }
    /// `request` then, if nothing needs asking, `go_ahead`.
    pub(super) fn request_and_go_ahead(&mut self, ctx: &egui::Context, discard: Discard) {
        if let Some(discard) = self.request(discard) {
            self.go_ahead(ctx, discard);
        }
    }
    /// Shows "Save changes?" while something is waiting for an answer.
    pub(super) fn unsaved_changes_prompt(&mut self, ctx: &egui::Context) {
        let Some(discard) = &self.asking else {
            return;
        };
        let project = self.project.as_ref().map_or("", |project| &project.name);
        let (name, doing) = match discard {
            Discard::Quit => (project, "closing"),
            Discard::OpenAnother | Discard::OpenSample(_) | Discard::NewProject => {
                (project, "opening another project")
            }
            Discard::CloseProject => (project, "closing it"),
            Discard::CloseTab(path) => (axcenity_project::script_name(path), "closing it"),
        };
        // What isn't saved: only the tab, when closing a tab.
        let mut items = Vec::new();
        if !matches!(discard, Discard::CloseTab(_)) {
            if !self.history.is_saved() {
                items.push((icons::GLOBE_HEMISPHERE_WEST, "The world".to_owned()));
            }
            for path in self.tabs.unsaved_paths() {
                items.push((icons::FILE_CODE, path));
            }
            if let Some(project) = &self.project {
                for document in &project.block_scripts {
                    if document.script != document.saved {
                        items.push((icons::PUZZLE_PIECE, document.path.clone()));
                    }
                }
            }
        }
        let (icon, tone) = dialog::unsaved_style();
        let message = format!("\"{name}\" has unsaved changes. Save them before {doing}?");
        let outcome = dialog::ask(
            ctx,
            &dialog::Question {
                id: "unsaved changes",
                icon,
                tone,
                title: "Save changes?",
                message: &message,
                items,
            },
            &[
                dialog::Button {
                    icon: icons::X,
                    label: "Cancel",
                    kind: dialog::Kind::Plain,
                },
                dialog::Button {
                    icon: icons::TRASH,
                    label: "Don't Save",
                    kind: dialog::Kind::Plain,
                },
                dialog::Button {
                    icon: icons::FLOPPY_DISK,
                    label: "Save",
                    kind: dialog::Kind::Main,
                },
            ],
        );
        let answer = outcome.map(|outcome| match outcome {
            dialog::Outcome::Picked(2) => Answer::Save,
            dialog::Outcome::Picked(1) => Answer::DontSave,
            _ => Answer::Cancel,
        });
        if let Some(answer) = answer
            && let Some(discard) = self.answer(answer)
        {
            self.go_ahead(ctx, discard);
        }
    }
    pub(super) fn title(&self) -> String {
        // A `*` after the name means unsaved changes, as in most editors.
        let unsaved = if self.unsaved() { "*" } else { "" };
        match &self.project {
            Some(project) => format!("{}{unsaved} - Axcenity Editor", project.name),
            None => "Axcenity Editor".to_string(),
        }
    }
    /// Fills the window under the menu bar with the start screen, and opens
    /// what's picked there.
    pub(super) fn start_screen(&mut self, ctx: &egui::Context) {
        let picked = egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(theme::palette(ctx).panel))
            .show(ctx, |ui| {
                let brand = self
                    .brand
                    .as_ref()
                    .expect("the logo is loaded before the start screen");
                self.start.show(ui, &mut self.recent, brand)
            })
            .inner;
        // A failed open is told on the start screen (see `follow_loading`).
        match picked {
            Some(StartAction::OpenDialog) => self.open_with_dialog(ctx),
            Some(StartAction::Open(folder)) => self.begin_open(ctx, &folder),
            Some(StartAction::OpenSample(folder)) => {
                self.begin_open(ctx, &folder);
                self.learn_open = true;
            }
            Some(StartAction::Settings) => self.settings.open = true,
            None => {}
        }
    }
}

/// Where Play's `SaveService` keeps slots: the player's data for the game,
/// as in the built game, never the project folder.
#[cfg(not(test))]
pub(super) fn play_save_folder(game: &str) -> Option<std::path::PathBuf> {
    axcenity_save::game_folder(game)
}

/// In tests, a folder of this test run's own, so tests never write to (or
/// load from) the real player's data.
#[cfg(test)]
pub(super) fn play_save_folder(game: &str) -> Option<std::path::PathBuf> {
    Some(
        std::env::temp_dir()
            .join(format!("axcenity-editor-saves-{}", std::process::id()))
            .join(game),
    )
}
