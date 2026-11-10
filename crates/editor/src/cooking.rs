//! Cooking the open project's textures in the background (roadmap L2).
//!
//! When a project opens, a thread cooks every texture under `assets/` that
//! has no current cooked copy (`axcenity_cook::cook_project`), on fewer
//! threads than the machine has so editing stays smooth. What it cooks is
//! used the next time each texture loads: the next Play run's new textures,
//! the next open, and the player. Opening another project stops it between
//! textures. `axcenity-editor --cook <folder>` does the same and exits.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use axcenity_cook::{Outcome, Report, cook_project};

/// The command-line flag that cooks a project and exits.
pub const COOK_FLAG: &str = "--cook";

/// The thread cooking a project. Dropping it asks the thread to stop after
/// the texture it's on; nothing waits for that.
pub struct Cooker {
    stop: Arc<AtomicBool>,
}

impl Cooker {
    /// Starts cooking `folder`'s textures on a thread of its own.
    pub fn start(folder: PathBuf) -> Cooker {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let spawned = std::thread::Builder::new()
            .name("texture cooking".to_owned())
            .spawn(move || {
                // Half the cores, so the editor keeps the rest.
                let threads = std::thread::available_parallelism()
                    .map_or(2, |cores| (cores.get() / 2).max(1));
                let pool = rayon::ThreadPoolBuilder::new()
                    .num_threads(threads)
                    .thread_name(|index| format!("texture cooking {index}"))
                    .build();
                let run = || {
                    cook_project(&folder, &flag, |path, outcome| {
                        if let Outcome::Failed(reason) = outcome {
                            log::warn!("couldn't cook the texture {path}: {reason}");
                        }
                    })
                };
                let report = match pool {
                    Ok(pool) => pool.install(run),
                    Err(_) => run(),
                };
                if report.cooked > 0 {
                    log::info!("cooked {} textures in {}", report.cooked, folder.display());
                }
            });
        if let Err(err) = spawned {
            log::warn!("textures aren't cooked: {err}");
        }
        Cooker { stop }
    }
}

impl Drop for Cooker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// `--cook <folder>`: cooks the project's textures, printing each, and says
/// how it went. False if any texture couldn't be cooked.
pub fn cook_from_command_line(folder: &Path) -> bool {
    let start = std::time::Instant::now();
    let stop = AtomicBool::new(false);
    let report: Report = cook_project(folder, &stop, |path, outcome| match outcome {
        Outcome::Cooked => println!("cooked {path}"),
        Outcome::NotBlockSized => println!("left as it is (not whole 4×4 blocks): {path}"),
        Outcome::Failed(reason) => eprintln!("couldn't cook {path}: {reason}"),
    });
    println!(
        "{} cooked, {} already current, {} left as they are, {} failed, in {:.1} s",
        report.cooked,
        report.current,
        report.skipped,
        report.failed.len(),
        start.elapsed().as_secs_f64()
    );
    report.failed.is_empty()
}
