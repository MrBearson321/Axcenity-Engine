//! What the frame schedule asks of the player as host ([`HostIo`]): its
//! window, renderer, audio, camera and network, step by step.

use super::*;

impl HostIo for App {
    fn sim(&mut self) -> &mut Sim {
        &mut self.sim
    }

    fn following(&self) -> bool {
        self.following
    }

    /// What scripts asked of NetworkService last frame, then who joined or
    /// left (Phase 63).
    fn net_receive(&mut self, seconds: f64) -> ops::ControlFlow<()> {
        self.network
            .receive(&mut self.sim.world, &mut self.sim.scripts, seconds);
        print_network_log(&mut self.network);
        print_output(&mut self.sim.scripts);
        // `Player:Kick` on this device's player ends the game.
        if let Some(reason) = self.network.take_kicked() {
            logs::line(
                LogLevel::Warning,
                None,
                &format!("You were kicked from the game: {reason}"),
            );
            return ops::ControlFlow::Break(());
        }
        if !axcenity_ui::chat_enabled(&self.sim.world) {
            self.chat_typing = None;
        }
        if self.sim.scripts.is_client_only() && !self.following {
            self.follow_server();
        }
        ops::ControlFlow::Continue(())
    }

    /// A character controller moves first and owns the camera, by the
    /// input map's actions (with the player's rebinds). In a hosted or
    /// joined game, only this device's player's character takes input, and
    /// other devices move their own (Phase 66).
    fn character_motion(&mut self, seconds: f32) -> MoveInput {
        let motion = self.controller_input(seconds);
        let others = self
            .network
            .other_characters(&self.sim.world, &self.sim.scripts);
        self.sim
            .characters
            .set_players(self.sim.scripts.local_player(), others);
        self.network.set_running(motion.run);
        motion
    }

    fn flush_output(&mut self) {
        print_output(&mut self.sim.scripts);
    }

    fn agent_warning(&mut self, warning: String) {
        logs::line(LogLevel::Warning, None, &format!("warning: {warning}"));
    }

    /// Without a controller the fly camera moves, and LocalScripts see that.
    fn frame_input(&mut self, seconds: f64) -> Vec<InputEvent> {
        let size = (self.frame_size.width, self.frame_size.height);
        self.begin_camera(seconds as f32, size);
        let pointer = self.apply_cursor(size);
        self.input.take_events(pointer)
    }

    /// Parts another device moves are held still here, and physics sleeps
    /// far from the camera and every other player.
    fn prepare_physics(&mut self) {
        let held = self.network.held_parts(&self.sim.world, &self.sim.scripts);
        self.sim.physics.set_held(held);
        self.set_far_physics_sleep();
    }

    fn owns_physics(&self) -> bool {
        self.network.simulates_physics()
    }

    fn audio(&mut self) {
        let view = self.shown_view();
        let heard = listener_of_view(view);
        let walls = self
            .sim
            .occlusion
            .step(&self.sim.world, &self.sim.assets, view.position);
        self.audio.set_walls(walls);
        apply_audio(
            &mut self.sim.scripts,
            &mut self.sim.world,
            &self.sim.assets,
            &mut self.audio,
            heard,
            std::mem::take(&mut self.ui_sounds),
        );
    }

    /// Big emitters move on the GPU, where it runs compute shaders.
    fn particles(&mut self, seconds: f64) {
        self.particles.set_gpu(
            self.renderer
                .as_ref()
                .is_some_and(|renderer| renderer.capabilities().compute_shaders),
        );
        for (emitter, count) in self.sim.scripts.take_particle_emits() {
            self.particles.emit(&self.sim.world, emitter, count);
        }
        self.particles.update(&self.sim.world, seconds as f32);
    }

    fn animator_error(&mut self, error: String) {
        logs::line(LogLevel::Error, None, &format!("error: {error}"));
    }

    /// A level loaded this frame shows the loading screen again.
    fn level_loads(&mut self) {
        self.follow_level_loads();
    }

    /// The loading screen goes once its time is up and, when the game
    /// waits for one, a LocalScript has said it's ready. The splash or
    /// loading screen also waits for the preload.
    fn loading_screen(&mut self, seconds: f64) {
        let ready = self.sim.scripts.take_loading_screen_removed();
        if ready {
            self.splash.release();
        }
        let loading_assets = self.preload_loading();
        self.frame_loading_assets = loading_assets;
        self.splash.set_loading(loading_assets);
        if let Some(loading) = &mut self.loading_gui {
            loading.set_assets_loading(loading_assets);
            loading.advance(&mut self.sim.world, seconds as f32, ready);
        }
    }

    /// A LocalScript's move of a Custom camera goes to the fly camera; a
    /// character controller owns the view otherwise.
    fn camera_moved(&mut self, moved: Option<CameraState>) {
        self.apply_camera_move(moved);
    }

    fn game_camera(&mut self, camera: GameCamera) {
        self.game_camera = camera;
    }

    /// A LocalScript's graphics change (`UserGameSettings`) is saved and
    /// drawn from this frame.
    fn graphics_changed(&mut self) {
        if let Some(change) = self.sim.scripts.take_graphics_change()
            && self.quality.is_none()
            && let Some(renderer) = &mut self.renderer
        {
            let capabilities = *renderer.capabilities();
            let settings = graphics::changed(self.data_folder.as_deref(), change, &capabilities);
            set_graphics(renderer, &settings);
        }
    }

    fn streaming(&mut self, seconds: f64) {
        let render_camera = self.shown_camera();
        let heard = listener_of_view(self.shown_view());
        self.audio.set_listener(heard);
        self.step_streaming(render_camera.position, seconds);
    }

    /// Builds the draw list at this window's current pixel size, from the
    /// camera scripts left.
    fn draw_scene(&mut self, seconds: f64) {
        // A replay on screen is drawn instead of the game, which goes on
        // underneath (Phase 81).
        let free = self.shown_camera();
        if let Some(view) = &mut self.replay {
            if view.step(seconds) {
                let camera = view.camera(free);
                let draw_list = axcenity_scene::build_draw_list_posed(
                    view.playback.world(),
                    &self.sim.assets,
                    camera,
                    &axcenity_scene::Poses::new(),
                    &mut view.terrain,
                );
                self.frame_draw = Some(draw_list);
                self.frame_ui_layout = Some(axcenity_ui::UiLayout::default());
                return;
            }
            self.replay = None;
        }
        let size = self.frame_size;
        let on_top = self.loading_gui.as_ref().and_then(LoadingGui::on_top);
        let camera = self.shown_camera();
        self.terrain.set_blends(blends_to_draw(&self.sim));
        let (draw_list, ui_layout) = axcenity_scene::build_draw_list_with_ui_interaction(
            &self.sim.world,
            &self.sim.assets,
            camera,
            (size.width, size.height),
            &self.stylesheets,
            UiFrame {
                interaction: &mut self.ui,
                input: UiInput {
                    pointer: self.pointer,
                    pressed: self.ui_pressed_button,
                    focused: self.text_focus,
                    on_top,
                },
                seconds: seconds as f32,
            },
            self.sim.animators.poses(),
            &mut self.terrain,
        );
        self.frame_draw = Some(draw_list);
        self.frame_ui_layout = Some(ui_layout);
    }

    /// The game's overlays, the particles, the console and the splash or
    /// loading screen, over the draw list.
    fn draw_ui(&mut self, seconds: f64) {
        let size = self.frame_size;
        let render_camera = self.shown_camera();
        let mut draw_list = self.frame_draw.take().expect("the scene was drawn first");
        let ui_layout = self
            .frame_ui_layout
            .take()
            .expect("the scene was drawn first");
        self.note_ui_warnings(&ui_layout.warnings);
        // The UI's cursor, else a ClickDetector's under the mouse.
        let click_cursor = self.sim.scripts.click_cursor(&self.sim.world);
        let cursor = match (ui_layout.cursor, click_cursor) {
            (UiCursor::Default, Some(name)) => name.parse().unwrap_or(CursorIcon::Pointer),
            (cursor, _) => cursor_icon(cursor),
        };
        if cursor != self.cursor
            && let Some(window) = &self.window
        {
            self.cursor = cursor;
            window.set_cursor(cursor);
        }
        // The hotbar, the ability bar, the quest tracker and dialogue box,
        // the leaderboard, the chat window, then ProximityPrompts that show.
        let inventory = axcenity_gameplay::player_inventory(&self.sim.world);
        let abilities = axcenity_gameplay::ability_bar(&self.sim.scripts, &self.sim.world);
        let quests = axcenity_gameplay::quest_tracker(&self.sim.scripts, &self.sim.world);
        let dialogue = axcenity_gameplay::dialogue_view(&self.sim.scripts, &self.sim.world);
        let lines: Vec<(Option<String>, String)> = self
            .sim
            .scripts
            .chat_lines()
            .into_iter()
            .map(|line| (line.from, line.text))
            .collect();
        let prompts = prompt_views(self.sim.scripts.shown_prompts());
        draw_list.ui.extend(axcenity_ui::overlay_items(
            &self.sim.world,
            &axcenity_ui::OverlayView {
                inventory,
                abilities: &abilities,
                quests: &quests,
                dialogue: dialogue.as_ref(),
                leaderboard: self.sim.scripts.leaderboard_shown(),
                chat: Some((&lines, self.chat_typing.as_deref())),
                prompts: &prompts,
            },
            &self.stylesheets,
            &render_camera,
            size.width,
            size.height,
        ));
        self.particles
            .draw(&mut draw_list, &self.sim.world, &self.sim.assets);
        self.particles.add_weather(&mut draw_list);
        self.shader_time += seconds as f32;
        draw_list.time = self.shader_time;
        // The console's outlines and readout, then the console, over the
        // game's UI (Phase 71).
        self.take_log();
        draw_list
            .ui
            .extend(self.console_items(render_camera, (size.width, size.height)));
        if let Some(view) = &self.replay {
            draw_list
                .ui
                .extend(view.timeline_items(size.width as f32, size.height as f32));
        }
        let loading_assets = self.frame_loading_assets;
        if self.show_splash {
            // The first drawn frame starts the splash clock.
            self.splash.tick();
            // Once its bar is full, a splash still waiting for assets says
            // how many are in.
            let caption = match &self.preload {
                Some(preload) if self.splash.waiting_for_assets() => preload.progress().caption(),
                _ => "Made with Axcenity".to_owned(),
            };
            draw_list.ui.extend(
                self.splash
                    .items(size.width as f32, size.height as f32, &caption),
            );
        } else if loading_assets
            && self.loading_gui.is_none()
            && let Some(preload) = &self.preload
        {
            // No splash and no loading screen of the game's own: a plain
            // one, so textures don't pop in.
            draw_list.ui.extend(preload::screen_items(
                size.width as f32,
                size.height as f32,
                preload.progress(),
            ));
        }
        // Models that couldn't be loaded, by physics or for drawing.
        for error in self.sim.assets.take_errors() {
            logs::line(LogLevel::Error, None, &format!("error: {error}"));
        }
        self.frame_draw = Some(draw_list);
    }

    /// After the frame's scripts, physics and UI layout, before rendering.
    fn net_send(&mut self) {
        self.network.send(&self.sim.world, &mut self.sim.scripts);
        // A script's `ReplayService:PlayBuffer()`: the kill cam shows from
        // the next frame (Phase 81).
        if let Some(play) = self.network.take_buffer_play() {
            match replay_view::ReplayView::kill_cam(play, &self.sim.world) {
                Ok(view) => self.replay = Some(view),
                Err(why) => logs::line(LogLevel::Warning, None, &format!("warning: {why}")),
            }
        }
    }
}
