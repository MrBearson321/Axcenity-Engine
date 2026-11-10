//! Axcenity's game systems outside the scripting core (roadmap L7). Each
//! one is an [`axcenity_script::GameSystem`]: the services it adds to `game:GetService` and
//! a step that runs once per frame on each side. [`install`] adds them all
//! to a [`ScriptRuntime`]; hosts get one with them through
//! `axcenity_sim::script_runtime`.
//!
//! - `debris`: `Debris:AddItem`, which destroys Instances later.
//! - `story`: dialogues and quests (`DialogueService` in `dialogue`,
//!   `QuestService` in `quest`), with the hooks that run them from the
//!   player's keys and ProximityPrompts.

use axcenity_script::ScriptRuntime;

mod debris;
pub mod dialogue;
pub mod quest;
mod story;

#[cfg(test)]
mod debris_tests;
#[cfg(test)]
mod dialogue_tests;
#[cfg(test)]
mod system_tests;

pub use debris::Debris;
pub use story::{Story, dialogue_active, dialogue_view, quest_tracker};

/// Adds every game system to `runtime`, in their step order. Call it before
/// any script starts.
pub fn install(runtime: &mut ScriptRuntime) {
    runtime.add_system(Debris);
    runtime.add_system(Story::default());
}

/// A [`ScriptRuntime`] with every game system, for hosts and tests.
pub fn script_runtime() -> ScriptRuntime {
    let mut runtime = ScriptRuntime::new();
    install(&mut runtime);
    runtime
}
