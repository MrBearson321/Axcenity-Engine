//! What a host does differently: input, sound, network and drawing.
//!
//! A step reaches these through [`FrameCtx::io`], so it never names the
//! window, the renderer or the audio device. Each method a host doesn't
//! need keeps its default, which is what a host with nothing there wants:
//! a headless test runner, for one, lets sounds it was asked for go.

use std::ops::ControlFlow;

use axcenity_character::MoveInput;
use axcenity_core::input::InputEvent;
use axcenity_script::{CameraState, GameCamera};

use crate::Sim;
use crate::schedule::Hosts;

/// What a frame's steps need from the host running them.
pub trait HostIo {
    /// The simulation this host runs.
    fn sim(&mut self) -> &mut Sim;

    /// True when this device follows a server (Phase 64): it runs its
    /// LocalScripts, and the server simulates the rest. Read once per frame,
    /// right after network receive.
    fn following(&self) -> bool {
        false
    }

    /// Network receive: what the other devices sent, and whatever scripts
    /// asked of `NetworkService` last frame. Break ends the frame there.
    fn net_receive(&mut self, _seconds: f64) -> ControlFlow<()> {
        ControlFlow::Continue(())
    }

    /// This frame's input for the character controllers, and who plays them.
    fn character_motion(&mut self, _seconds: f32) -> MoveInput {
        MoveInput::default()
    }

    /// Moves what scripts printed to wherever this host shows it.
    fn flush_output(&mut self) {}

    /// A problem a navigation agent ran into.
    fn agent_warning(&mut self, _warning: String) {}

    /// Hands LocalScripts the camera and the view's size, and returns the
    /// input since the last frame.
    fn frame_input(&mut self, _seconds: f64) -> Vec<InputEvent> {
        Vec::new()
    }

    /// Tells physics which parts other devices move and where it can sleep.
    fn prepare_physics(&mut self) {}

    /// True if this device, following a server, simulates some of the
    /// world itself (an assembly the server gave it, such as a car it drives).
    fn owns_physics(&self) -> bool {
        false
    }

    /// Notes how many fixed physics steps this frame took.
    fn physics_stepped(&mut self, _steps: u32) {}

    /// Plays what scripts asked for and syncs the Sounds. Without an audio
    /// device, what they asked for is let go.
    fn audio(&mut self) {
        let _ = self.sim().scripts.take_sound_actions();
    }

    /// Emits and moves the particles. Without drawing, emits are let go.
    fn particles(&mut self, _seconds: f64) {
        let _ = self.sim().scripts.take_particle_emits();
    }

    /// A problem an Animator ran into.
    fn animator_error(&mut self, _error: String) {}

    /// Shows the loading screen for the levels scripts loaded this frame.
    fn level_loads(&mut self) {
        let _ = self.sim().scripts.take_level_loads();
    }

    /// Moves the loading screen on.
    fn loading_screen(&mut self, _seconds: f64) {
        let _ = self.sim().scripts.take_loading_screen_removed();
    }

    /// Where a LocalScript moved the camera, if one did.
    fn camera_moved(&mut self, _moved: Option<CameraState>) {}

    /// What scripts did to the camera (`CameraType`, field of view, shake).
    fn game_camera(&mut self, _camera: GameCamera) {}

    /// Applies a LocalScript's graphics change (`UserGameSettings`).
    fn graphics_changed(&mut self) {}

    /// World streaming around the camera.
    fn streaming(&mut self, _seconds: f64) {}

    /// Builds the scene to draw.
    fn draw_scene(&mut self, _seconds: f64) {}

    /// Adds the UI to the scene to draw.
    fn draw_ui(&mut self, _seconds: f64) {}

    /// Network send: the world's changes and remote messages.
    fn net_send(&mut self) {}
}

/// What the steps of one frame are given.
pub struct FrameCtx<'h> {
    /// How long this frame is, in game time.
    pub seconds: f64,
    /// The host running the frame.
    pub host: Hosts,
    pub io: &'h mut dyn HostIo,
    /// See [`HostIo::following`]; set by the network receive step.
    pub following: bool,
    ended: bool,
}

impl<'h> FrameCtx<'h> {
    pub fn new(seconds: f64, host: Hosts, io: &'h mut dyn HostIo) -> Self {
        Self {
            seconds,
            host,
            io,
            following: false,
            ended: false,
        }
    }

    /// Stops the frame after the running step.
    pub fn end_frame(&mut self) {
        self.ended = true;
    }

    /// True once a step has stopped the frame.
    pub fn ended(&self) -> bool {
        self.ended
    }
}
