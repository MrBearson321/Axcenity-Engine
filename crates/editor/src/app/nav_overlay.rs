//! The navigation mesh overlay (View > Overlays > Nav Mesh): builds the
//! mesh of the world the viewport shows, and builds it again after that
//! world changes.

use std::time::Duration;

use axcenity_nav::{NavMesh, NavSettings};

use super::*;

/// The least time between two builds, so parts falling during Play don't
/// rebuild the mesh every frame.
const REBUILD_INTERVAL: Duration = Duration::from_millis(500);

/// The last mesh built, and what it was built from.
pub(super) struct NavCache {
    pub(super) mesh: NavMesh,
    fingerprint: u64,
    built: Instant,
}

impl NavCache {
    /// True when the world has changed since this mesh was built, nothing
    /// is being dragged or painted, and the last build wasn't too recent.
    fn needs_rebuild(&self, fingerprint: u64, dragging: bool, now: Instant) -> bool {
        fingerprint != self.fingerprint
            && !dragging
            && now.duration_since(self.built) >= REBUILD_INTERVAL
    }
}

impl EditorApp {
    /// Keeps the overlay's mesh in step with the shown world while the
    /// overlay is on, and drops it when it's off.
    pub(super) fn refresh_nav_mesh(&mut self, ctx: &egui::Context) {
        if !self.overlays.nav_mesh {
            self.nav_mesh = None;
            return;
        }
        let world = shown(&self.world, &self.play);
        let fingerprint = axcenity_nav::fingerprint(world);
        let now = Instant::now();
        let dragging = ctx.input(|input| input.pointer.any_down());
        let rebuild = match &self.nav_mesh {
            None => true,
            Some(cache) if cache.needs_rebuild(fingerprint, dragging, now) => true,
            Some(cache) => {
                if cache.fingerprint != fingerprint {
                    // Come back once the drag ends or the interval passes.
                    ctx.request_repaint_after(REBUILD_INTERVAL);
                }
                false
            }
        };
        if rebuild {
            self.nav_mesh = Some(NavCache {
                mesh: NavMesh::build(world, &self.assets, NavSettings::default()),
                fingerprint,
                built: now,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache(fingerprint: u64, built: Instant) -> NavCache {
        NavCache {
            mesh: NavMesh::default(),
            fingerprint,
            built,
        }
    }

    #[test]
    fn the_mesh_is_rebuilt_after_a_change_once_the_drag_ends() {
        let built = Instant::now();
        let later = built + REBUILD_INTERVAL;
        let cache = cache(1, built);
        assert!(!cache.needs_rebuild(1, false, later), "nothing changed");
        assert!(!cache.needs_rebuild(2, true, later), "still dragging");
        assert!(!cache.needs_rebuild(2, false, built), "built just now");
        assert!(cache.needs_rebuild(2, false, later));
    }
}
