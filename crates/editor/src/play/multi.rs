//! Play with several clients (Phase 64): **Clients: 1–8** in Play's
//! settings. The first device is the server, with its own player on it (a
//! listen server, as `axcenity-player --host` is: the one-client Play of
//! before). Each other client is a [`Session`] of its own, with its own
//! world, that joins the first over the in-memory loopback and follows it,
//! as `--join` does, so RemoteEvents, replication and characters work as
//! between computers. The viewport shows one device at a time
//! ([`PlayView`]): the server's world seen from the fly camera, or a
//! client's game, which gets the keyboard and mouse.
//!
//! [`Play`] dereferences to the shown device, so the panels and the
//! viewport read and edit whichever world is shown.

use std::ops::{Deref, DerefMut};

use axcenity_character::MoveInput;
use axcenity_core::input::InputEvent;
use axcenity_core::{ClassName, World};
use axcenity_net::{Conditions, GameInfo, PeerStats, ScriptCode};
use axcenity_project::ScriptFile;
use axcenity_script::{Breakpoints, CameraState};

use super::{Crash, Session, play_user_id};
use crate::output::Output;

/// Most clients Play runs at once.
pub const MAX_CLIENTS: usize = 8;

/// The game every Play device says it is, so they always match.
pub(super) fn play_game() -> GameInfo {
    GameInfo {
        name: "Play".to_owned(),
        version: "1.0".to_owned(),
    }
}

/// How Play runs: how many clients, and how bad a network between them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlaySettings {
    /// 1 to [`MAX_CLIENTS`]: the first one's player is on the server.
    pub clients: usize,
    /// The network emulated between the clients (Phase 67).
    pub conditions: Conditions,
}

impl Default for PlaySettings {
    fn default() -> Self {
        Self {
            clients: 1,
            conditions: Conditions::default(),
        }
    }
}

/// What the viewport shows during Play.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayView {
    /// The server's world, from the fly camera, with no player's input.
    Server,
    /// Client `n` (0 is the first, whose player is on the server).
    Client(usize),
}

/// The game while it plays, on one device or several.
pub struct Play {
    devices: Vec<Session>,
    view: PlayView,
}

impl Play {
    /// Starts Play as [`Session::start`] does, then, for more than one
    /// client, hosts on the first device and joins the others to it.
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        edit_world: &World,
        scripts: &[ScriptFile],
        breakpoints: &Breakpoints,
        output: &mut Output,
        assets: axcenity_assets::Assets,
        camera: CameraState,
        loading: &axcenity_project::LoadingScreen,
        input_map: &axcenity_core::input::InputMap,
        save_folder: Option<std::path::PathBuf>,
        graphics: &axcenity_render::GraphicsSettings,
        capabilities: &axcenity_render::Capabilities,
        levels: super::PlayLevels,
        settings: PlaySettings,
    ) -> Result<Play, Crash> {
        let clients = settings.clients.clamp(1, MAX_CLIENTS);
        let mut first = Session::start(
            edit_world,
            scripts,
            breakpoints,
            output,
            assets.clone(),
            camera,
            loading,
            input_map,
            save_folder.clone(),
            graphics,
            capabilities,
            levels.clone(),
        )?;
        let mut devices = Vec::new();
        if clients > 1 {
            let Session { sim, network, .. } = &mut first;
            let mut loopback =
                Some(network.host_in_memory(&mut sim.world, &mut sim.scripts, true));
            network.emulate(settings.conditions);
            devices.push(first);
            // The code each client starts again on the server's world.
            let code: Vec<ScriptCode> = scripts
                .iter()
                .filter(|script| {
                    matches!(
                        edit_world.get(script.instance).map(|i| i.class()),
                        Some(ClassName::LocalScript | ClassName::ModuleScript)
                    )
                })
                .map(|script| ScriptCode {
                    instance: script.instance,
                    path: script.path.clone(),
                    source: script.source.clone(),
                })
                .collect();
            for n in 1..clients {
                let mut client = Session::start(
                    edit_world,
                    &[],
                    breakpoints,
                    output,
                    assets.clone(),
                    camera,
                    loading,
                    input_map,
                    save_folder.clone(),
                    graphics,
                    capabilities,
                    levels.clone(),
                )?;
                client.label = Some(format!("client {}", n + 1));
                client.sim.scripts.set_user_id(play_user_id() + n as f64);
                let link = match loopback.take() {
                    Some(link) => link,
                    None => devices[0]
                        .network
                        .connect_loopback()
                        .expect("the first device hosts in memory"),
                };
                let Session { sim, network, .. } = &mut client;
                network.set_client_scripts(code.clone());
                network.join_through(&mut sim.world, &mut sim.scripts, link, None);
                devices.push(client);
            }
        } else {
            devices.push(first);
        }
        Ok(Play {
            devices,
            view: PlayView::Client(0),
        })
    }

    /// How many clients play.
    pub fn clients(&self) -> usize {
        self.devices.len()
    }

    pub fn view(&self) -> PlayView {
        self.view
    }

    /// Shows another device; one that isn't there shows the first client.
    pub fn set_view(&mut self, view: PlayView) {
        self.view = match view {
            PlayView::Client(n) if n >= self.devices.len() => PlayView::Client(0),
            view => view,
        };
        self.devices[0].observing = self.view == PlayView::Server;
    }

    /// The views to choose from, each with its name: the server (with
    /// more than one client), then each client by its player's name.
    pub fn views(&self) -> Vec<(PlayView, String)> {
        let mut views = Vec::new();
        if self.devices.len() > 1 {
            views.push((PlayView::Server, "Server".to_owned()));
        }
        for (n, device) in self.devices.iter().enumerate() {
            let name = device
                .sim
                .scripts
                .local_player()
                .and_then(|player| device.sim.world.get(player))
                .map(|player| player.name().to_owned())
                .unwrap_or_else(|| format!("Client {}", n + 1));
            views.push((PlayView::Client(n), name));
        }
        views
    }

    fn shown_index(&self) -> usize {
        match self.view {
            PlayView::Server => 0,
            PlayView::Client(n) => n.min(self.devices.len() - 1),
        }
    }

    /// The first device: the server, and the first client's game.
    pub fn server(&self) -> &Session {
        &self.devices[0]
    }

    pub fn server_mut(&mut self) -> &mut Session {
        &mut self.devices[0]
    }

    /// Client `n`'s device (0 is the server's).
    #[cfg(test)]
    pub fn device(&self, n: usize) -> Option<&Session> {
        self.devices.get(n)
    }

    /// One frame on every device, the server first: the shown one gets the
    /// `input` and `motion`, the others none. Returns where a LocalScript
    /// on the shown device moved the camera (see [`Session::frame`]).
    pub fn frame(
        &mut self,
        seconds: f64,
        input: &[InputEvent],
        motion: MoveInput,
        camera: CameraState,
        output: &mut Output,
    ) -> Result<Option<CameraState>, Crash> {
        let shown = self.shown_index();
        let mut moved = None;
        for (n, device) in self.devices.iter_mut().enumerate() {
            let (input, motion) = if n == shown {
                (input, motion)
            } else {
                (&[][..], MoveInput::default())
            };
            let result = device.frame(seconds, input, motion, camera, output)?;
            if n == shown {
                moved = result;
            }
            // A client's lines carry its player's name once it has one.
            if n > 0
                && let Some(name) = device
                    .sim
                    .scripts
                    .local_player()
                    .and_then(|player| device.sim.world.get(player))
                    .map(|player| player.name().to_owned())
            {
                device.label = Some(name);
            }
        }
        Ok(moved)
    }

    pub fn set_breakpoints(&mut self, breakpoints: &Breakpoints) {
        for device in &mut self.devices {
            device.set_breakpoints(breakpoints);
        }
    }

    /// Where a script is paused at a breakpoint on any device.
    pub fn paused(&self) -> Option<axcenity_script::Pause> {
        self.devices.iter().find_map(Session::pause)
    }

    /// Continues every paused device.
    pub fn continue_from_pause(&mut self, output: &mut Output) -> Result<(), Crash> {
        for device in &mut self.devices {
            if device.pause().is_some() {
                device.continue_from_pause(output)?;
            }
        }
        Ok(())
    }

    /// The viewport's size, for every device's `Camera.ViewportSize`.
    pub fn set_viewport_size(&mut self, width: f32, height: f32) {
        for device in &mut self.devices {
            device.set_viewport_size(width, height);
        }
    }

    /// A changed asset, on every device. True if any reloaded it.
    pub fn asset_changed(&mut self, path: &str, output: &mut Output) -> bool {
        let mut any = false;
        for device in &mut self.devices {
            any |= device.asset_changed(path, output);
        }
        any
    }

    /// A changed script runs again on every device.
    pub fn reload_script(
        &mut self,
        script: axcenity_core::InstanceId,
        path: &str,
        source: &str,
        output: &mut Output,
    ) -> Result<(), Crash> {
        for (n, device) in self.devices.iter_mut().enumerate() {
            // Clients run only LocalScripts and the modules they need.
            let runs = n == 0
                || matches!(
                    device.sim.world.get(script).map(|i| i.class()),
                    Some(ClassName::LocalScript | ClassName::ModuleScript)
                );
            if runs {
                device.reload_script(script, path, source, output)?;
            }
        }
        Ok(())
    }

    /// Each connection's numbers, from the server, for the Profiler's
    /// Network section (Phase 67), and how many Instances each client has.
    pub fn network_stats(&self) -> Vec<(String, PeerStats, usize)> {
        let server = &self.devices[0];
        server
            .network
            .stats()
            .into_iter()
            .filter_map(|stats| {
                let player = stats.player?;
                let name = server.sim.world.get(player)?.name().to_owned();
                let device = self
                    .devices
                    .iter()
                    .skip(1)
                    .find(|device| device.sim.scripts.local_player() == Some(player));
                let instances = device.map_or(0, |device| device.sim.world.len());
                Some((name, stats, instances))
            })
            .collect()
    }

    /// The network emulated between the clients from now on.
    pub fn emulate(&mut self, conditions: Conditions) {
        self.devices[0].network.emulate(conditions);
    }
}

impl Deref for Play {
    type Target = Session;

    fn deref(&self) -> &Session {
        &self.devices[self.shown_index()]
    }
}

impl DerefMut for Play {
    fn deref_mut(&mut self) -> &mut Session {
        let shown = self.shown_index();
        &mut self.devices[shown]
    }
}
