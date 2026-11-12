//! The engine-facing side of physics (cohesion pass CW6.1): what the
//! editor, scripts, the player and the server may use, in `core` types
//! only. rapier, the simulation backend, stays inside this crate's other
//! modules, so a backend change rewrites those and not their callers.
//!
//! The layers, from the bottom (`docs/architecture/physics.md`, "Layers"):
//!
//! 1. **Backend**: rapier (vendored under `vendor/`), used only by
//!    this crate's modules (`sync`, `shapes`, `joints`, `movers`, ...).
//! 2. **Engine-facing API**: this module and the [`Physics`] methods and
//!    query functions re-exported at the crate root, in `core` types.
//! 3. **Simulation and world management**: [`Physics::step`] reads the
//!    Workspace's parts, steps, and writes motion back.
//! 4. **Editor-facing properties**: each class's properties and defaults in
//!    `core` (`class.rs`), grouped by the editor's Properties panel.
//! 5. **Lua**: `script` (`physical.rs`, the definitions) turns calls into
//!    property changes physics reads at its next step.
//!
//! [`Physics`]: crate::Physics
//! [`Physics::step`]: crate::Physics::step

use axcenity_core::{Value, World};

/// How long one physics step is: physics always runs at 60 steps a second.
pub const STEP_SECONDS: f64 = 1.0 / 60.0;

/// Downward pull in studs per second squared (about 1 stud = 0.28 m): the
/// default of the Workspace's `Gravity`, which [`gravity`] reads.
pub const GRAVITY: f32 = 196.2;

/// How hard the wind pushes: the force on a part is this times its average
/// face area times how much faster the wind moves than the part. With
/// density 1, a 50 studs/s wind blows a thin 2×0.2×2 sheet along the
/// ground, while a 4×1×2 block stays put there and only drifts in the air.
pub const WIND_DRAG: f32 = 2.0;

/// How much the last [`Physics::step`] read from the world to catch up, for
/// tests and profiling.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SyncStats {
    /// How many parts' properties were read.
    pub parts_read: usize,
    /// How many bodies were built, one per assembly.
    pub bodies_built: usize,
    /// True if all of the Workspace was read again.
    pub full_sync: bool,
    /// How many parts the write-back looked at.
    pub parts_written: usize,
}

/// The Workspace's `Gravity` in studs a second squared, or [`GRAVITY`] if
/// it isn't a finite number. The character controllers fall by it too.
pub fn gravity(world: &World) -> f32 {
    world
        .get(world.workspace())
        .and_then(|workspace| workspace.property("Gravity"))
        .and_then(Value::as_number)
        .map(|gravity| gravity as f32)
        .filter(|gravity| gravity.is_finite())
        .unwrap_or(GRAVITY)
}

#[cfg(test)]
mod tests {
    /// The API speaks `core` types only: rapier never shows through it.
    #[test]
    fn the_api_names_no_backend_type() {
        let source = include_str!("api.rs");
        let code = &source[..source.find("#[cfg(test)]").unwrap()];
        assert!(!code.contains(concat!("rap", "ier3d")));
        assert!(!code.contains("glamx"));
    }
}
