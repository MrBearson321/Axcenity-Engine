//! Axcenity's game systems outside the scripting core (roadmap L7). Each
//! one is an [`axcenity_script::GameSystem`]: the services it adds to `game:GetService` and
//! a step that runs once per frame on each side. [`install`] adds them all
//! to a [`ScriptRuntime`]; hosts get one with them through
//! `axcenity_sim::script_runtime`.
//!
//! - `debris`: `Debris:AddItem`, which destroys Instances later.
//! - `character_system`: Health and damage (`health`) and Humanoids
//!   (`humanoid`), with fall damage, regeneration and DamageZones.
//! - `ability_system`: Abilities and Effects (`ability`), their methods,
//!   the server's step and players' activation requests.
//! - `tool_system`: Tools and Inventories (`tool`), their methods and a
//!   Humanoid's `EquipTool`, pickups, the hotbar and the Tool in hand.
//! - `story`: dialogues and quests (`DialogueService` in `dialogue`,
//!   `QuestService` in `quest`), with the hooks that run them from the
//!   player's keys and ProximityPrompts.

use axcenity_script::ScriptRuntime;

pub mod ability;
mod ability_system;
mod character_system;
mod debris;
pub mod dialogue;
pub mod health;
pub mod humanoid;
pub mod quest;
mod story;
pub mod tool;
mod tool_system;

#[cfg(test)]
mod ability_tests;
#[cfg(test)]
mod debris_tests;
#[cfg(test)]
mod dialogue_tests;
#[cfg(test)]
mod health_tests;
#[cfg(test)]
mod humanoid_tests;
#[cfg(test)]
mod system_tests;
#[cfg(test)]
mod tool_tests;

pub use ability_system::{AbilitySystem, ability_bar, ability_requests, step_abilities};
pub use character_system::{
    CharacterSystem, fire_landed, share_nav_mesh, step_health, step_humanoids,
};
pub use debris::Debris;
pub use story::{Story, dialogue_active, dialogue_view, quest_tracker};
pub use tool_system::{ToolSystem, place_tools, player_inventory, step_tools};

/// Adds every game system to `runtime`, in their step order. Call it before
/// any script starts.
pub fn install(runtime: &mut ScriptRuntime) {
    runtime.add_system(Debris);
    // Abilities before Health: both fire first, and the Health events
    // Effects cause fire after the Abilities' in the same round.
    runtime.add_system(AbilitySystem::default());
    runtime.add_system(CharacterSystem::default());
    runtime.add_system(ToolSystem::default());
    runtime.add_system(Story::default());
}

/// A [`ScriptRuntime`] with every game system, for hosts and tests.
pub fn script_runtime() -> ScriptRuntime {
    let mut runtime = ScriptRuntime::new();
    install(&mut runtime);
    runtime
}
