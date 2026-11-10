//! One navigation mesh for everything in a game that walks on it (roadmap
//! item M13): NavigationAgents ([`crate::Agents`]) and Humanoids' `MoveTo`
//! (`script::humanoid`) read the same mesh instead of each building its
//! own.
//!
//! Looking at what changed reads the world, so it happens on the game's
//! thread. Putting a changed mesh together, the slow part, runs on a worker
//! thread while the game goes on with the old mesh. The new mesh comes in
//! [`APPLY_SECONDS`] of game time after the look that started it, waiting
//! for the worker then if it hasn't finished. So which frame a mesh comes
//! in on depends only on the game's frame times, never on how quickly the
//! worker ran, and a game plays the same however busy the machine is.
//!
//! Only the first mesh is built at once, as nothing can walk without one.

use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;

use axcenity_assets::Assets;
use axcenity_core::World;

use crate::builder::{Assembly, Look};
use crate::{MeshBuilder, NavMesh, NavSettings};

/// The least game time between two looks at what changed, so anchored
/// parts that scripts keep moving don't rebuild the mesh every frame. The
/// editor's overlay waits as long.
pub const REBUILD_SECONDS: f32 = 0.5;
/// How long after its look a rebuilt mesh comes in, in game seconds: six
/// frames at 60 fps, time for the worker to finish while the game runs.
pub const APPLY_SECONDS: f32 = 0.1;

/// A mesh being put together on the worker.
struct Pending {
    /// [`Inner::clock`] when it started.
    started: f64,
    /// What to put together. The worker takes it out; when there's no
    /// worker (none could be started, or [`SharedNavMesh::without_worker`])
    /// it's still here when the mesh is due, and is put together then.
    assembly: Arc<Mutex<Option<Assembly>>>,
    worker: Option<JoinHandle<Option<NavMesh>>>,
}

impl Pending {
    fn start(assembly: Assembly, started: f64, worker: bool) -> Self {
        let assembly = Arc::new(Mutex::new(Some(assembly)));
        let worker = worker.then(|| {
            let assembly = Arc::clone(&assembly);
            std::thread::Builder::new()
                .name("navmesh build".into())
                .spawn(move || take(&assembly).map(Assembly::run))
                .ok()
        });
        Self {
            started,
            assembly,
            worker: worker.flatten(),
        }
    }

    /// The new mesh, waiting for the worker if it's still going.
    fn finish(self) -> NavMesh {
        if let Some(worker) = self.worker {
            match worker.join() {
                Ok(Some(mesh)) => return mesh,
                Ok(None) => {}
                // A panic while assembling is a bug: keep it a panic.
                Err(panic) => std::panic::resume_unwind(panic),
            }
        }
        take(&self.assembly)
            .map(Assembly::run)
            .expect("an assembly no worker took")
    }
}

/// True once `seconds` have passed from `since` to `now`, allowing for
/// rounding, so six frames of 1/60 s make 0.1 s.
fn passed(now: f64, since: f64, seconds: f32) -> bool {
    now - since >= f64::from(seconds) - 1e-6
}

fn take(assembly: &Mutex<Option<Assembly>>) -> Option<Assembly> {
    assembly
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take()
}

struct Inner {
    builder: MeshBuilder,
    /// The mesh to walk on; `None` until the first build.
    mesh: Option<Arc<NavMesh>>,
    /// Counts the meshes after the first, so walkers know when to look
    /// again at what's ahead.
    generation: u64,
    /// Game seconds this mesh has been stepped through
    /// ([`SharedNavMesh::advance`]). An `f64`, so hours of 60 fps frames
    /// still add up.
    clock: f64,
    /// [`Self::clock`] at the last look that found something changed.
    looked: f64,
    pending: Option<Pending>,
    /// False puts every mesh together at once, on the game's thread.
    worker: bool,
}

/// The mesh as it stands, and which build it is.
#[derive(Clone)]
pub struct MeshState {
    pub mesh: Arc<NavMesh>,
    /// 0 for the first mesh, and one more for each rebuild after it.
    pub generation: u64,
}

/// A handle to a game's one navigation mesh. Clones share it. A host makes
/// one per game (an [`Agents`](crate::Agents) makes one), and hands it to
/// whatever else walks on the mesh.
#[derive(Clone)]
pub struct SharedNavMesh {
    inner: Arc<Mutex<Inner>>,
}

impl Default for SharedNavMesh {
    fn default() -> Self {
        Self::new(NavSettings::default())
    }
}

impl SharedNavMesh {
    pub fn new(settings: NavSettings) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                builder: MeshBuilder::new(settings),
                mesh: None,
                generation: 0,
                clock: 0.0,
                looked: 0.0,
                pending: None,
                worker: true,
            })),
        }
    }

    /// Puts every rebuilt mesh together at once, on the calling thread, as
    /// before M13. The mesh still comes in when [`APPLY_SECONDS`] says.
    /// For measuring the worker's gain.
    pub fn without_worker(self) -> Self {
        self.lock().worker = false;
        self
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        // Nothing leaves `Inner` half changed, so a panic elsewhere while
        // it was locked doesn't make it wrong.
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Moves the mesh's clock on by a frame's game time. Exactly one owner
    /// calls it, once a frame, so sharing the mesh doesn't make it age
    /// twice as fast.
    pub fn advance(&self, seconds: f32) {
        if seconds.is_finite() && seconds > 0.0 {
            self.lock().clock += f64::from(seconds);
        }
    }

    /// The mesh, brought up to date: built at once if there's none yet. A
    /// rebuilt mesh that is due comes in. Once [`REBUILD_SECONDS`] have
    /// passed since the last look that found a change, reads again the
    /// ground parts that changed, and starts putting a new mesh together
    /// if they change it. Cheap when nothing changed.
    pub fn refresh(&self, world: &World, assets: &Assets) -> MeshState {
        let mut inner = self.lock();
        let inner = &mut *inner;
        if inner.mesh.is_none() {
            let mesh = match inner.builder.look(world, assets) {
                Look::Changed(assembly) => assembly.run(),
                // Not on a builder's first look; kept for completeness.
                Look::Same | Look::Unchanged => inner.builder.mesh().clone(),
            };
            inner.mesh = Some(Arc::new(mesh));
            inner.looked = inner.clock;
        } else if let Some(pending) = &inner.pending {
            if passed(inner.clock, pending.started, APPLY_SECONDS) {
                let pending = inner.pending.take().expect("checked above");
                inner.mesh = Some(Arc::new(pending.finish()));
                inner.generation += 1;
            }
        } else if passed(inner.clock, inner.looked, REBUILD_SECONDS) {
            match inner.builder.look(world, assets) {
                Look::Unchanged => {}
                Look::Same => inner.looked = inner.clock,
                Look::Changed(assembly) => {
                    inner.looked = inner.clock;
                    inner.pending = Some(Pending::start(assembly, inner.clock, inner.worker));
                }
            }
        }
        MeshState {
            mesh: Arc::clone(inner.mesh.as_ref().expect("built above")),
            generation: inner.generation,
        }
    }

    /// The mesh as it stands, without looking at the world; `None` before
    /// the first [`Self::refresh`].
    pub fn current(&self) -> Option<MeshState> {
        let inner = self.lock();
        inner.mesh.as_ref().map(|mesh| MeshState {
            mesh: Arc::clone(mesh),
            generation: inner.generation,
        })
    }

    /// True while a rebuilt mesh is being put together.
    pub fn is_rebuilding(&self) -> bool {
        self.lock().pending.is_some()
    }
}
