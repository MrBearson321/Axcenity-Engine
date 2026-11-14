//! One bot: a headless player's device that joins a server over UDP,
//! follows its world as a replica, walks its own character with made-up
//! input and counts what happens. It runs the player's frame steps of the
//! shared schedule (`axcenity_sim`), with nothing drawn or heard.

use std::ops::ControlFlow;
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use axcenity_assets::Assets;
use axcenity_character::MoveInput;
use axcenity_core::ClassName;
use axcenity_core::input::InputEvent;
use axcenity_net::{GameInfo, Network, ScriptCode};
use axcenity_script::OutputKind;
use axcenity_sim::{FrameCtx, HostIo, Hosts, Schedule, Sim};

use crate::stats::BotSample;

/// What every bot of a run is told.
#[derive(Clone, Debug)]
pub struct BotSettings {
    pub game: std::path::PathBuf,
    pub address: String,
    pub password: Option<String>,
    /// Run the game's LocalScripts on each bot, as a player's device does.
    pub scripts: bool,
    /// Stand still instead of walking about.
    pub idle: bool,
    /// How far from where it spawned a bot wanders before it turns back,
    /// in studs, so bots stay on a small map.
    pub roam: f32,
    /// The first bot's UserId; the others count up from it.
    pub first_user_id: u64,
}

/// Pixels of mouse look that turn a character half round: the controllers
/// turn 0.003 radians a pixel.
const HALF_TURN: f32 = std::f32::consts::PI / 0.003;

/// A bot's made-up input: walks, turns now and then, sometimes runs or
/// jumps, and turns back when it's wandered too far. Seeded by its number,
/// so a run is the same each time.
struct Brain {
    seed: u64,
    /// Seconds until the next change of mind.
    next: f64,
    turning: f32,
    running: bool,
    jump: bool,
    /// Turning back toward where it spawned, until it's near again.
    returning: bool,
    /// Pixels of turning left to do this many frames.
    turn_left: (f32, u32),
}

impl Brain {
    fn new(index: usize) -> Brain {
        Brain {
            seed: (index as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1,
            next: 0.5,
            turning: 0.0,
            running: false,
            jump: false,
            returning: false,
            turn_left: (0.0, 0),
        }
    }

    /// A number from 0 up to 1 (xorshift).
    fn random(&mut self) -> f64 {
        let mut x = self.seed;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.seed = x;
        (x >> 11) as f64 / (1u64 << 53) as f64
    }

    /// This frame's input; `away` is how far it is from where it spawned,
    /// against how far it may roam.
    fn input(&mut self, seconds: f64, idle: bool, away: Option<(f32, f32)>) -> MoveInput {
        if idle {
            return MoveInput::default();
        }
        if let Some((distance, roam)) = away {
            if !self.returning && distance > roam {
                // About turn, over ten frames.
                self.returning = true;
                self.turn_left = (HALF_TURN / 10.0, 10);
            } else if self.returning && distance < roam * 0.5 {
                self.returning = false;
            }
        }
        if self.turn_left.1 > 0 {
            self.turn_left.1 -= 1;
            return MoveInput {
                forward: true,
                look_dx: self.turn_left.0,
                ..MoveInput::default()
            };
        }
        self.next -= seconds;
        self.jump = false;
        if self.next <= 0.0 {
            self.next = 1.0 + self.random() * 3.0;
            // Turn for a moment, either way, at up to 200 pixels a frame.
            self.turning = ((self.random() * 2.0 - 1.0) * 200.0) as f32;
            self.running = self.random() < 0.3;
            self.jump = self.random() < 0.25;
        } else {
            self.turning *= 0.8;
        }
        MoveInput {
            forward: true,
            run: self.running,
            jump: self.jump,
            look_dx: self.turning,
            ..MoveInput::default()
        }
    }
}

/// UserIds between one generation of a bot and the next (`--churn`).
const GENERATION_IDS: u64 = 100_000;

/// One bot's device.
pub struct Bot {
    pub index: usize,
    sim: Sim,
    network: Network,
    schedule: Schedule,
    following: bool,
    /// `Network::follows` when the world last became the server's.
    followed: u64,
    brain: Brain,
    idle: bool,
    roam: f32,
    /// Where its character first stood.
    home: Option<axcenity_core::Vector3>,
    started: Instant,
    /// Seconds from starting to having the server's world, once it has.
    joined_after: Option<f64>,
    /// Why the connection ended, once it has.
    ended: Option<String>,
    /// Errors from the game's LocalScripts on this bot.
    script_errors: u64,
    /// Time spent running this bot's frames since the last sample.
    busy: Duration,
    frames: u64,
}

impl Bot {
    /// Loads the game from `settings.game` and starts joining.
    /// Bot `index`'s `generation`th device: each churned bot (`--churn`) is
    /// a new install, with a UserId of its own.
    pub fn start(index: usize, generation: u64, settings: &BotSettings) -> Result<Bot> {
        let project = axcenity_project::load(&settings.game).with_context(|| {
            format!(
                "could not open the project in `{}`",
                settings.game.display()
            )
        })?;
        let folder: &Path = &project.folder;
        let mut problems = project.problems;
        let start_level = project.manifest.start_level.clone();
        let mut levels: std::collections::BTreeMap<_, _> =
            axcenity_project::load_levels(folder, &mut problems)
                .into_iter()
                .collect();
        levels.insert(start_level.clone(), project.world.clone());
        let mut world = project.world;
        let assets = Assets::new(folder);
        let mut scripts = axcenity_sim::script_runtime();
        scripts.set_assets(assets.clone());
        let user_id = settings.first_user_id + index as u64 + generation * GENERATION_IDS;
        scripts.set_user_id(user_id as f64);
        let _ = axcenity_project::load_starter_character(folder, &mut world);
        scripts.set_input_map(project.input_map.clone());
        let worlds = levels.values().chain([&world]);
        for (path, source) in axcenity_project::level_code(folder, worlds) {
            scripts.know_code(&path, &source);
        }
        scripts.set_levels(&start_level, &start_level, levels);

        let mut network = Network::new(GameInfo {
            name: project.manifest.name.clone(),
            version: project.manifest.version.clone(),
        });
        if settings.scripts {
            network.set_client_scripts(client_scripts(&world, &project.scripts));
        }
        network.set_join_name(format!("Bot{}", index + 1));
        network
            .join(
                &mut world,
                &mut scripts,
                &settings.address,
                settings.password.clone(),
            )
            .map_err(anyhow::Error::msg)?;
        Ok(Bot {
            index,
            sim: Sim::new(world, scripts, assets),
            network,
            schedule: Schedule::for_host(Hosts::PLAYER),
            following: false,
            followed: 0,
            brain: Brain::new(index),
            idle: settings.idle,
            roam: settings.roam,
            home: None,
            started: Instant::now(),
            joined_after: None,
            ended: None,
            script_errors: 0,
            busy: Duration::ZERO,
            frames: 0,
        })
    }

    /// One frame of `seconds`.
    pub fn frame(&mut self, seconds: f64) {
        if self.ended.is_some() {
            return;
        }
        let started = Instant::now();
        let mut laps = axcenity_core::bench::Laps::start();
        let schedule = std::mem::take(&mut self.schedule);
        schedule.run(&mut FrameCtx::new(seconds, Hosts::PLAYER, self), &mut laps);
        self.schedule = schedule;
        self.busy += started.elapsed();
        self.frames += 1;
    }

    /// True once its connection ended (a failure, refusal or kick).
    pub fn is_over(&self) -> bool {
        self.ended.is_some()
    }

    /// Leaves the game, telling the server.
    pub fn leave(&mut self) {
        let Sim { world, scripts, .. } = &mut self.sim;
        self.network.disconnect(world, scripts);
    }

    /// How it's doing now; resets the frame time it counts.
    pub fn sample(&mut self) -> BotSample {
        let link = self
            .network
            .stats()
            .first()
            .map(|stats| stats.link)
            .unwrap_or_default();
        let frame_ms = if self.frames == 0 {
            0.0
        } else {
            self.busy.as_secs_f64() * 1000.0 / self.frames as f64
        };
        self.busy = Duration::ZERO;
        self.frames = 0;
        BotSample {
            bot: self.index,
            joined_after: self.joined_after,
            connected: self.ended.is_none() && self.network.is_joined(),
            ended: self.ended.clone(),
            rtt_ms: link.rtt * 1000.0,
            packet_loss: link.packet_loss,
            sent_per_second: link.bytes_sent_per_second,
            received_per_second: link.bytes_received_per_second,
            corrections: self.network.corrections(),
            rejoins: self.network.rejoins(),
            rejoining: self.network.is_rejoining(),
            script_errors: self.script_errors,
            frame_ms,
            instances: self.sim.world.len(),
            ..BotSample::default()
        }
    }
}

impl Bot {
    /// Where this bot's character's root part is, once it has one.
    fn character_position(&self) -> Option<axcenity_core::Vector3> {
        let world = &self.sim.world;
        let player = self.sim.scripts.local_player()?;
        let character = world.get(player)?.property("Character")?.as_instance()?;
        let root = world
            .get(character)?
            .property("PrimaryPart")
            .and_then(axcenity_core::Value::as_instance)
            .filter(|&id| world.contains(id))
            .or_else(|| world.find_first_child(character, "HumanoidRootPart"))
            .or_else(|| {
                world
                    .descendants_iter(character)
                    .find(|&id| world.get(id).is_some_and(|i| i.class().is_base_part()))
            })?;
        world.get(root)?.property("Position")?.as_vector3()
    }
}

impl HostIo for Bot {
    fn sim(&mut self) -> &mut Sim {
        &mut self.sim
    }

    fn following(&self) -> bool {
        self.following
    }

    fn net_receive(&mut self, seconds: f64) -> ControlFlow<()> {
        let Sim { world, scripts, .. } = &mut self.sim;
        self.network.receive(world, scripts, seconds);
        let mut failure = None;
        for line in self.network.take_log() {
            log::info!("Bot{}: {line}", self.index + 1);
            if let Some(reason) = line.strip_prefix("warning: ") {
                failure = Some(reason.to_owned());
            }
        }
        if let Some(reason) = self.network.take_kicked() {
            self.ended = Some(format!("kicked: {reason}"));
            return ControlFlow::Break(());
        }
        // A failure, refusal or lost connection leaves the session offline.
        if !self.network.is_following() {
            self.ended = Some(failure.unwrap_or_else(|| "the connection ended".to_owned()));
            return ControlFlow::Break(());
        }
        // On joining, and again on each rejoin (a new world).
        if self.sim.scripts.is_client_only() && self.followed != self.network.follows() {
            self.followed = self.network.follows();
            self.following = true;
            self.home = None;
            self.sim.follow_server();
        }
        if self.joined_after.is_none() && self.network.is_joined() && self.following {
            self.joined_after = Some(self.started.elapsed().as_secs_f64());
        }
        ControlFlow::Continue(())
    }

    fn character_motion(&mut self, seconds: f32) -> MoveInput {
        let at = self.character_position();
        if self.home.is_none() {
            self.home = at;
        }
        let away = at
            .zip(self.home)
            .map(|(at, home)| ((at.x - home.x).hypot(at.z - home.z), self.roam));
        let motion = self.brain.input(f64::from(seconds), self.idle, away);
        let others = self
            .network
            .other_characters(&self.sim.world, &self.sim.scripts);
        let player = self.sim.scripts.local_player();
        self.sim.characters.set_players(player, others);
        self.network.set_running(motion.run);
        motion
    }

    fn flush_output(&mut self) {
        for line in self.sim.scripts.take_output() {
            if line.kind == OutputKind::Error {
                self.script_errors += 1;
                log::warn!("Bot{}: error: {}", self.index + 1, line.text);
            }
        }
    }

    fn frame_input(&mut self, _seconds: f64) -> Vec<InputEvent> {
        self.sim.scripts.set_viewport_size(1280.0, 720.0);
        Vec::new()
    }

    fn prepare_physics(&mut self) {
        let held = self.network.held_parts(&self.sim.world, &self.sim.scripts);
        self.sim.physics.set_held(held);
    }

    fn owns_physics(&self) -> bool {
        self.network.simulates_physics()
    }

    fn net_send(&mut self) {
        self.network.send(&self.sim.world, &mut self.sim.scripts);
    }
}

/// The game's LocalScripts and ModuleScripts, which a joined device runs.
fn client_scripts(
    world: &axcenity_core::World,
    scripts: &[axcenity_project::ScriptFile],
) -> Vec<ScriptCode> {
    scripts
        .iter()
        .filter(|script| {
            world.get(script.instance).is_some_and(|instance| {
                matches!(
                    instance.class(),
                    ClassName::ModuleScript | ClassName::LocalScript
                )
            })
        })
        .map(|script| ScriptCode {
            instance: script.instance,
            path: script.path.clone(),
            source: script.source.clone(),
        })
        .collect()
}
