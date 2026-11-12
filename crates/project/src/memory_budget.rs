//! A game's memory budget (competitive roadmap item 5, G7): limits the
//! Profiler's Memory tab compares the playing game with, so it can warn
//! when the game grows past them. Kept in the manifest's `memory_budget`;
//! every limit is optional, and none is set by default.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::SaveError;

/// The manifest's `memory_budget`, in megabytes (1 MB = 2²⁰ bytes).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryBudget {
    /// Everything the game holds that the Profiler counts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_mb: Option<u32>,
    /// Both Lua heaps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lua_mb: Option<u32>,
    /// Texture images, those inside models included.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub textures_mb: Option<u32>,
}

impl MemoryBudget {
    pub fn is_empty(&self) -> bool {
        *self == MemoryBudget::default()
    }
}

/// Keeps `budget` in the project's manifest.
pub fn set_memory_budget(folder: &Path, budget: &MemoryBudget) -> Result<(), SaveError> {
    let value = serde_json::to_value(budget).expect("a budget is plain data");
    crate::update_manifest(folder, "memory_budget", value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_budget_is_kept_in_the_manifest_and_read_back() {
        let temp = std::env::temp_dir().join(format!("axcenity-budget-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp).unwrap();
        let folder = crate::create_project(&temp, "Budgeted").unwrap();
        let before = crate::load(&folder).unwrap();
        assert!(before.manifest.memory_budget.is_empty(), "none by default");
        let text = std::fs::read_to_string(folder.join(crate::MANIFEST_FILE)).unwrap();
        assert!(!text.contains("memory_budget"), "left out when unset");

        let budget = MemoryBudget {
            game_mb: Some(512),
            lua_mb: None,
            textures_mb: Some(256),
        };
        set_memory_budget(&folder, &budget).unwrap();
        let after = crate::load(&folder).unwrap();
        assert_eq!(after.manifest.memory_budget, budget);
        assert_eq!(after.manifest.name, "Budgeted", "the rest is kept");
        let _ = std::fs::remove_dir_all(&temp);
    }
}
