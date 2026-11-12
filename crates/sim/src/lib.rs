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
pub mod occlusion;
mod schedule;
mod steps;

#[cfg(test)]
mod blend_tests;
#[cfg(test)]
mod golden;
#[cfg(test)]
mod memory_tests;

use axcenity_anim::Animators;
use axcenity_assets::Assets;
use axcenity_character::Controllers;
use axcenity_core::memory::{MemoryBreakdown, MemoryUse};
use axcenity_core::{ClassName, World};
use axcenity_nav::Agents;
use axcenity_physics::Physics;
use axcenity_script::ScriptRuntime;

/// The [`ScriptRuntime`] every host runs: the scripting core with the
/// `gameplay` crate's systems added (roadmap L7). Hosts build theirs with
/// this, never `ScriptRuntime::new`, or scripts miss services such as
/// Debris.
pub use axcenity_gameplay::script_runtime;
pub use host::{FrameCtx, HostIo};
pub use occlusion::Occlusion;
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
    /// The walls between each spatial Sound and the listener (roadmap
    /// G8); hosts with audio step it and pass it on.
    pub occlusion: Occlusion,
}

impl Sim {
    /// A simulation of `world` with nothing moving yet.
    pub fn new(mut world: World, scripts: ScriptRuntime, assets: Assets) -> Self {
        // A copy of a Collaborate edit world keeps edits; the game's are
        // never sent, so don't keep them.
        world.stop_recording_edits();
        // Agents and Humanoids walk on one navigation mesh (roadmap M13).
        let agents = Agents::default();
        axcenity_gameplay::share_nav_mesh(&scripts, agents.nav_mesh());
        Self {
            world,
            scripts,
            physics: Physics::with_assets(assets.clone()),
            characters: Controllers::default(),
            agents,
            animators: Animators::with_ground(axcenity_character::foot_ground),
            assets,
            occlusion: Occlusion::default(),
        }
    }

    /// The game's memory by kind (roadmap G7), for the profilers: Lua, the
    /// Instances by class, the asset caches and physics. Sounds are the
    /// host's (`GameAudio::memory`), so they're left at zero. Walks every
    /// Instance and cache, so hosts ask every few frames.
    pub fn memory(&self) -> MemoryBreakdown {
        let assets = self.assets.memory();
        let cache = |used: axcenity_assets::CacheUse| MemoryUse {
            count: used.count,
            bytes: used.bytes,
        };
        MemoryBreakdown {
            lua: self.scripts.used_memory(),
            instances: self.world.memory(),
            models: cache(assets.models),
            textures: cache(assets.textures),
            lighting: cache(assets.lighting),
            sounds: MemoryUse::default(),
            physics: self.physics.memory(),
        }
    }

    /// The moving parts to draw between physics steps this frame (roadmap
    /// G3), as far along as [`Physics::blend_alpha`] says. What the camera
    /// follows is left out, with all that's welded to it: the player's
    /// character (and the car it sits in) and a Follow or Track camera's
    /// subject. Drawn blended under a camera that follows the stepped
    /// part, they would shake on screen.
    pub fn blends(&self) -> Vec<axcenity_physics::Blend> {
        let world = &self.world;
        let mut followed = Vec::new();
        if let Some(character) = self.characters.stepped_character(world) {
            followed.extend(axcenity_character::body_parts(world, character));
        }
        if let Some(subject) = self.scripts.camera_followed()
            && let Some(instance) = world.get(subject)
        {
            // A controller or agent moves its parent's body.
            let class = instance.class();
            let holder = if class.is_character_controller() || class == ClassName::NavigationAgent {
                instance.parent()
            } else {
                Some(subject)
            };
            if let Some(holder) = holder {
                followed.extend(axcenity_character::body_parts(world, holder));
            }
        }
        self.physics.blends(&followed)
    }

    /// True while a script is stopped at a breakpoint (editor Play only).
    fn paused(&self) -> bool {
        self.scripts.pause().is_some()
    }
}
