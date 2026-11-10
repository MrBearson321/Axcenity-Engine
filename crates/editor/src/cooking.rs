//! Cooking the open project's textures in the background (roadmap L2).
//!
//! When a project opens, a thread first gives each imported file under
//! `assets/` an asset ID if it has none (a sidecar next to it,
//! `axcenity_assets::give_asset_ids`), deletes cached copies of files
//! that are gone, then cooks every texture that has no current cooked
//! copy (`axcenity_cook::cook_project`), on fewer threads than the
//! machine has so editing stays smooth. It then waits: when an imported,
//! added, edited or deleted file under `assets/` turns up
//! ([`Cooker::cook_again`], from the editor's file watcher), it does the
//! same again, so a new texture is cooked right away rather than at the
//! next open. What it cooks is used the next time each texture loads: the
//! next Play run's new textures, the next open, and the player. Opening
//! another project stops it between textures. `axcenity-editor --cook
//! <folder>` does one pass and exits.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use axcenity_cook::{Outcome, Report, cook_project};

/// The command-line flag that cooks a project and exits.
pub const COOK_FLAG: &str = "--cook";

/// The thread cooking a project. Dropping it asks the thread to stop after
/// the texture it's on; nothing waits for that.
pub struct Cooker {
    stop: Arc<AtomicBool>,
    again: mpsc::Sender<()>,
}

impl Cooker {
    /// Starts cooking `folder`'s textures on a thread of its own.
    pub fn start(folder: PathBuf) -> Cooker {
        Cooker::start_reporting(folder, |_| {})
    }

    /// [`start`](Self::start), calling `done` with each pass's report.
    fn start_reporting(folder: PathBuf, done: impl Fn(&Report) + Send + 'static) -> Cooker {
        let stop = Arc::new(AtomicBool::new(false));
        let (again, asked) = mpsc::channel();
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
                loop {
                    let report = match &pool {
                        Ok(pool) => pool.install(|| cook_pass(&folder, &flag)),
                        Err(_) => cook_pass(&folder, &flag),
                    };
                    done(&report);
                    // Asked again (or the editor let go of the project).
                    if asked.recv().is_err() {
                        break;
                    }
                    // Changes that came in a burst make one pass.
                    while asked.try_recv().is_ok() {}
                    if flag.load(Ordering::Relaxed) {
                        break;
                    }
                }
            });
        if let Err(err) = spawned {
            log::warn!("textures aren't cooked: {err}");
        }
        Cooker { stop, again }
    }

    /// Asks for another pass over the project once the one running (if
    /// any) is done: for a file under `assets/` that was imported, added,
    /// changed or deleted. Passes asked for while one runs make one.
    pub fn cook_again(&self) {
        let _ = self.again.send(());
    }
}

/// Gives assets IDs and cooks what isn't, logging what happened.
fn cook_pass(folder: &Path, stop: &AtomicBool) -> Report {
    match axcenity_assets::give_asset_ids(folder) {
        Ok(0) => {}
        Ok(given) => log::info!("gave {given} assets IDs in {}", folder.display()),
        Err(err) => log::warn!("couldn't give assets IDs: {err}"),
    }
    let report = cook_project(folder, stop, |path, outcome| {
        if let Outcome::Failed(reason) = outcome {
            log::warn!("couldn't cook the texture {path}: {reason}");
        }
    });
    if report.cooked > 0 {
        log::info!("cooked {} textures in {}", report.cooked, folder.display());
    }
    if report.removed > 0 {
        log::info!(
            "deleted {} cached copies of files no longer in {}",
            report.removed,
            folder.display()
        );
    }
    report
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
        "{} cooked, {} already current, {} left as they are, {} failed, {} stale copies deleted, in {:.1} s",
        report.cooked,
        report.current,
        report.skipped,
        report.failed.len(),
        report.removed,
        start.elapsed().as_secs_f64()
    );
    report.failed.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_texture_added_while_the_project_is_open_is_cooked_when_asked() {
        let folder = std::env::temp_dir().join(format!("axcenity-cooker-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(folder.join("assets/textures")).unwrap();
        let (passes, passed) = mpsc::channel();
        let cooker = Cooker::start_reporting(folder.clone(), move |report| {
            let _ = passes.send(report.cooked);
        });
        let wait = || {
            passed
                .recv_timeout(std::time::Duration::from_secs(60))
                .expect("a pass")
        };
        assert_eq!(wait(), 0, "nothing to cook yet");

        let path = "assets/textures/stone.png";
        let example = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/hello-material-graph/assets/textures/stone.png");
        std::fs::copy(example, folder.join(path)).unwrap();
        cooker.cook_again();
        assert_eq!(wait(), 1);
        assert!(axcenity_assets::is_cooked(&folder, path));
        assert!(
            folder.join(axcenity_assets::import_path(path)).exists(),
            "and it has an asset ID"
        );

        drop(cooker);
        let _ = std::fs::remove_dir_all(folder);
    }
}
