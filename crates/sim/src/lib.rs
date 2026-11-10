//! The engine's frame, as data (roadmap M1).
//!
//! Every host (the player, editor Play, the dedicated server, the game test
//! runner) runs one [`Schedule`]: an ordered table of named steps, each
//! marked with the hosts that run it. A step works on the [`Sim`] (the
//! world, scripts, physics, characters, agents and animators) and reaches
//! what differs per host (input, sound, network, drawing) through
//! [`HostIo`], so this crate stays free of the renderer and the window.
//! The order is written down in `docs/architecture/frame-loop.md`.

mod host;
mod schedule;
mod steps;

#[cfg(test)]
mod golden;

use axcenity_anim::Animators;
use axcenity_assets::Assets;
use axcenity_character::Controllers;
use axcenity_core::World;
use axcenity_nav::Agents;
use axcenity_physics::Physics;
use axcenity_script::ScriptRuntime;

/// The [`ScriptRuntime`] every host runs: the scripting core with the
/// `gameplay` crate's systems added (roadmap L7). Hosts build theirs with
/// this, never `ScriptRuntime::new`, or scripts miss services such as
/// Debris.
pub use axcenity_gameplay::script_runtime;
pub use host::{FrameCtx, HostIo};
pub use schedule::{Hosts, Schedule, Stage, Step};

/// The game's simulation: what the steps of a frame work on.
pub struct Sim {
    pub world: World,
    /// Runs the scripts, and keeps the ones paused by `task.wait`.
    pub scripts: ScriptRuntime,
    /// Simulates the unanchored parts.
    pub physics: Physics,
    /// Walks characters and owns the camera when one is present.
    pub characters: Controllers,
    /// Walks NavigationAgents along their paths.
    pub agents: Agents,
    /// Plays the Animators, posing skeletons and setting keyed properties.
    pub animators: Animators,
    /// The project's models and sounds.
    pub assets: Assets,
}

impl Sim {
    /// A simulation of `world` with nothing moving yet.
    pub fn new(world: World, mut scripts: ScriptRuntime, assets: Assets) -> Self {
        // Agents and Humanoids walk on one navigation mesh (roadmap M13).
        let agents = Agents::default();
        scripts.share_nav_mesh(agents.nav_mesh());
        Self {
            world,
            scripts,
            physics: Physics::with_assets(assets.clone()),
            characters: Controllers::default(),
            agents,
            animators: Animators::with_ground(axcenity_character::foot_ground),
            assets,
        }
    }

    /// True while a script is stopped at a breakpoint (editor Play only).
    fn paused(&self) -> bool {
        self.scripts.pause().is_some()
    }
}
