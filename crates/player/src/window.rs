//! The window's events ([`ApplicationHandler`]): resizing, keys, the mouse,
//! focus and redraws, which run a frame; plus the small helpers they use.

use super::*;

/// Seconds since `last_frame` (0 for the first frame), and records now as the latest frame.
///
/// This takes only the `last_frame` field rather than all of `App`, so it can
/// be called while other fields of `App` are borrowed.
/// Draws with `settings` from the next frame, logging anything the GPU
/// can't do.
pub(crate) fn set_graphics(renderer: &mut Renderer, settings: &GraphicsSettings) {
    renderer.set_graphics(settings);
    for note in &renderer.graphics().notes {
        log::info!("graphics: {note}");
    }
}

pub(crate) fn frame_seconds(last_frame: &mut Option<Instant>) -> f32 {
    let now = Instant::now();
    let seconds = last_frame.map_or(0.0, |last| (now - last).as_secs_f32());
    *last_frame = Some(now);
    seconds.min(MAX_FRAME_SECONDS)
}

/// Records a key press or release. Keys are physical positions, so the same
/// keys work on AZERTY and other layouts.
pub(crate) fn set_move_key(keys: &mut MoveKeys, key: KeyCode, pressed: bool) {
    let held = match key {
        KeyCode::KeyW => &mut keys.forward,
        KeyCode::KeyS => &mut keys.back,
        KeyCode::KeyA => &mut keys.left,
        KeyCode::KeyD => &mut keys.right,
        KeyCode::KeyE => &mut keys.up,
        KeyCode::KeyQ => &mut keys.down,
        KeyCode::ShiftLeft | KeyCode::ShiftRight => &mut keys.slow,
        _ => return,
    };
    *held = pressed;
}

/// The camera as `workspace.CurrentCamera` shows it to LocalScripts.
pub(crate) fn camera_state(camera: &FlyCamera) -> CameraState {
    let (position, orientation) = camera.view();
    CameraState {
        position,
        orientation,
    }
}

/// Mouse wheel notches in a scroll, away from the player (toward the
/// screen) positive. Touchpads report pixels instead of notches.
/// How far the wheel scrolls a ScrollingFrame, in pixels across and down
/// (down moves its content up).
pub(crate) fn wheel_pixels(delta: MouseScrollDelta) -> [f32; 2] {
    match delta {
        MouseScrollDelta::LineDelta(x, y) => [-x * PIXELS_PER_NOTCH, -y * PIXELS_PER_NOTCH],
        MouseScrollDelta::PixelDelta(position) => [-position.x as f32, -position.y as f32],
    }
}

pub(crate) fn wheel_notches(delta: MouseScrollDelta) -> f32 {
    match delta {
        MouseScrollDelta::LineDelta(_, y) => y,
        MouseScrollDelta::PixelDelta(position) => position.y as f32 / PIXELS_PER_NOTCH,
    }
}

/// While looking around, hides the cursor and keeps it from leaving the window.
pub(crate) fn capture_mouse(window: &Window, captured: bool) {
    let result = if captured {
        // `Locked` keeps the cursor still (macOS, Wayland). Windows and X11
        // only offer `Confined`, which keeps it inside the window.
        window
            .set_cursor_grab(CursorGrabMode::Locked)
            .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined))
    } else {
        window.set_cursor_grab(CursorGrabMode::None)
    };
    // Looking around still works without the grab, so this isn't an error.
    if let Err(err) = result {
        log::debug!("could not grab the mouse cursor: {err}");
    }
    window.set_cursor_visible(!captured);
}

impl ApplicationHandler for App {
    /// Called when the app is ready to create windows (at startup on desktop).
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let mut attributes = Window::default_attributes()
            .with_title(&self.title)
            .with_inner_size(DEFAULT_WINDOW_SIZE)
            .with_window_icon(icon::game_icon(self.icon.as_deref()));
        // `--size`, else `AXCENITY_WINDOW_SIZE=1920x1080` (physical pixels),
        // for benchmarks that need the same window every run, in every engine.
        let size = self.window_size.or_else(|| {
            window_size_override(std::env::var("AXCENITY_WINDOW_SIZE").ok().as_deref())
        });
        if let Some((width, height)) = size {
            attributes = attributes.with_inner_size(winit::dpi::PhysicalSize::new(width, height));
        }
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(err) => return self.fail(event_loop, err.into()),
        };

        // GPU setup is `async`; `pollster::block_on` simply waits for it to finish.
        let size = window.inner_size();
        match pollster::block_on(Renderer::new(window.clone(), size.width, size.height)) {
            Ok(mut renderer) => {
                let folder = self.data_folder.as_deref();
                let capabilities = *renderer.capabilities();
                // A LocalScript may have changed them as it started.
                let settings = match self.sim.scripts.take_graphics_change() {
                    Some(change) => graphics::changed(folder, change, &capabilities),
                    None => graphics::start(folder, &capabilities),
                };
                // `--quality` or `--graphics`: held for the run, not saved.
                let settings = match &self.fixed_graphics {
                    Some(fixed) => graphics::fit_machine(fixed.clone(), &capabilities),
                    None => settings,
                };
                if self.bench.is_some() {
                    // Frames as fast as they're drawn, each timed on the GPU.
                    renderer.set_vsync(false);
                    renderer.set_gpu_timing(true);
                }
                self.sim.scripts.set_graphics(&settings, &capabilities);
                set_graphics(&mut renderer, &settings);
                self.renderer = Some(renderer);
            }
            Err(err) => return self.fail(event_loop, err),
        }

        window.request_redraw();
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if self.window.is_none() || self.renderer.is_none() {
            return;
        }

        if self.show_splash && self.splash.covers() {
            match &event {
                WindowEvent::KeyboardInput { event: key, .. }
                    if key.state == ElementState::Pressed && !key.repeat =>
                {
                    self.splash.skip();
                }
                WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    ..
                } => self.splash.skip(),
                _ => {}
            }
            match &event {
                WindowEvent::CloseRequested
                | WindowEvent::Resized(_)
                | WindowEvent::RedrawRequested
                | WindowEvent::Focused(_) => {}
                _ => return,
            }
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(size.width, size.height);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.pointer = Some([position.x as f32, position.y as f32]);
            }
            WindowEvent::CursorLeft { .. } => self.pointer = None,
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state == ElementState::Pressed;
                let key = match event.physical_key {
                    PhysicalKey::Code(code) => Some(code),
                    PhysicalKey::Unidentified(_) => None,
                };
                // F9, and every key while the Developer Console is open
                // (Phase 71).
                if self.console_key(key, pressed, event.repeat, event.text.as_deref()) {
                    return;
                }
                if self.chat_typing.is_some() {
                    self.type_chat(&event);
                    if let Some(key) = key.and_then(script_key) {
                        self.push_key(key, pressed, true);
                    }
                } else if pressed
                    && key == Some(KeyCode::Slash)
                    && axcenity_ui::chat_enabled(&self.sim.world)
                {
                    // `/` opens the chat (Phase 67).
                    self.chat_typing = Some(String::new());
                } else if self.text_focus.is_some() {
                    self.type_text(&event);
                    // Scripts hear typed keys, as the UI's.
                    if let Some(key) = key.and_then(script_key) {
                        self.push_key(key, pressed, true);
                    }
                } else if let Some(code) = key {
                    // A press that moves focus is the UI's, not the game's.
                    let shift = self.held_keys.contains(&axcenity_core::KeyCode::LeftShift)
                        || self.held_keys.contains(&axcenity_core::KeyCode::RightShift);
                    let script = script_key(code);
                    if pressed
                        && let Some(key) = script
                        && let Some(nav) = axcenity_ui::nav_key(&self.input.map, key, shift)
                        && self.ui.takes(&self.sim.world, nav)
                    {
                        self.nav_keys.push(nav);
                        self.push_key(key, true, true);
                        return;
                    }
                    set_move_key(&mut self.keys, code, pressed);
                    if let Some(key) = script {
                        self.push_key(key, pressed, false);
                    }
                }
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => self.ui_mouse_events.push(state == ElementState::Pressed),
            WindowEvent::MouseInput { state, button, .. } => {
                let pressed = state == ElementState::Pressed;
                // The right button looks around, unless a script holds the cursor.
                if button == MouseButton::Right {
                    self.looking = pressed;
                    if self.mouse.0 == MouseBehavior::Default
                        && let Some(window) = &self.window
                    {
                        capture_mouse(window, self.looking);
                        window.set_cursor_visible(!self.looking && self.mouse.1);
                    }
                }
                if let Some(button) = input::mouse_button(button) {
                    self.input.push(InputEvent::new(InputKind::MouseButton {
                        button,
                        pressed,
                        position: self.pointer.unwrap_or_default(),
                    }));
                }
            }
            // Over a ScrollingFrame the wheel scrolls it; with a Tool in
            // hand it goes through the hotbar; elsewhere it zooms.
            WindowEvent::MouseWheel { delta, .. } => {
                let scrolled = self.pointer.is_some_and(|pointer| {
                    self.ui
                        .wheel(&self.sim.world, pointer, wheel_pixels(delta))
                        .apply(&mut self.sim.world)
                });
                if !scrolled && !tool_in_hand(&self.sim.world) {
                    self.zoom += wheel_notches(delta);
                }
                self.input.push(InputEvent {
                    kind: InputKind::Wheel {
                        position: self.pointer.unwrap_or_default(),
                        notches: wheel_notches(delta),
                    },
                    processed: scrolled,
                });
            }
            WindowEvent::Touch(touch) => self.input.push(InputEvent::new(input::touch(&touch))),
            WindowEvent::Focused(true) => {
                self.focused = true;
                // Locked again, if a script asked for it.
                self.mouse = (MouseBehavior::Default, true);
            }
            // Key releases aren't reported while another window has focus,
            // so let go of everything to avoid the camera drifting on its own.
            WindowEvent::Focused(false) => {
                self.focused = false;
                self.release_game_keys();
                if let Some(window) = &self.window {
                    capture_mouse(window, false);
                }
                self.ui_mouse_events.push(false);
            }
            // One frame: the host's input first, then the shared schedule
            // (`axcenity_sim`, docs/architecture/frame-loop.md), then render.
            WindowEvent::RedrawRequested => self.frame(event_loop),
            _ => {}
        }
    }

    /// Raw mouse movement, reported even when the cursor is held still.
    /// Scripts hear it (`GetMouseDelta`) while the window has focus; it
    /// turns the camera while the right button is held.
    fn device_event(&mut self, _event_loop: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        let DeviceEvent::MouseMotion { delta: (dx, dy) } = event else {
            return;
        };
        if !self.focused {
            return;
        }
        let (dx, dy) = (dx as f32, dy as f32);
        self.input.add_motion(dx, dy);
        if self.looking && !self.sim.scripts.camera_scripted() {
            if self.sim.characters.view(&self.sim.world).is_some() {
                self.look_dx += dx;
                self.look_dy += dy;
            } else {
                self.camera.look(dx, dy);
            }
        }
    }

    /// Called once all pending events are handled: ask for the next frame.
    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}
