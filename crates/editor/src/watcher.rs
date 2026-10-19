//! Noticing changes made to the project's files by other programs, such as
//! a script edited in VS Code.
//!
//! A background thread looks at the world file and every script file twice
//! a second, and compares each file's size and modification time with the
//! last look. It works the same on every OS and needs no extra library.
//! When something changed, it sends the changed paths to the editor and
//! wakes it up, so an idle editor stays idle otherwise.
//!
//! The watcher only says *which* files changed. The editor reads them and
//! compares the text with what it already has, so its own saves (which
//! change the files too) do nothing.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime};

use eframe::egui;

/// How often the thread looks at the files.
pub const INTERVAL: Duration = Duration::from_millis(500);

/// A file's modification time (if the OS reports one) and size.
type Stamp = (Option<SystemTime>, u64);

/// The stamp of each watched file, by its path in the project (such as
/// `world.json` or `scripts/pad.server.lua`). Files that aren't there are left out.
pub type Stamps = BTreeMap<String, Stamp>;

/// The stamps of the world file, every script file and every stylesheet in
/// `folder` now.
pub fn stamps(folder: &Path) -> Stamps {
    // Folders that can't be read just look empty; the editor reports
    // problems when it reads the files themselves.
    let mut ignored = Vec::new();
    let scripts = axcenity_project::find_scripts(folder, &mut ignored);
    let stylesheets = axcenity_project::find_stylesheets(folder, &mut ignored);
    std::iter::once(axcenity_project::WORLD_FILE.to_string())
        .chain(scripts)
        .chain(stylesheets)
        .filter_map(|path| {
            let metadata = std::fs::metadata(folder.join(&path)).ok()?;
            Some((path, (metadata.modified().ok(), metadata.len())))
        })
        .collect()
}

/// The paths that are new, gone, or have a different stamp in `new`, sorted.
pub fn changed(old: &Stamps, new: &Stamps) -> Vec<String> {
    let paths: BTreeSet<&String> = old.keys().chain(new.keys()).collect();
    paths
        .into_iter()
        .filter(|path| old.get(*path) != new.get(*path))
        .cloned()
        .collect()
}

/// Watches one project folder until it's dropped.
pub struct Watcher {
    changes: Receiver<Vec<String>>,
    /// Set when the watcher is dropped, to end the thread.
    stop: Arc<AtomicBool>,
    /// The editor's egui context, to wake it up with. It's given once the
    /// editor has one (tests have none), so the thread waits for it.
    waker: Arc<OnceLock<egui::Context>>,
}

impl Watcher {
    /// Starts watching `folder`. Changes from now on are reported.
    pub fn start(folder: PathBuf) -> Watcher {
        let (sender, changes) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let waker = Arc::new(OnceLock::<egui::Context>::new());
        // The thread gets its own handles to `stop` and `waker`: an `Arc` is
        // a shared pointer, and cloning it only adds another owner.
        let (thread_stop, thread_waker) = (stop.clone(), waker.clone());
        let spawned = std::thread::Builder::new()
            .name("project watcher".to_string())
            // `move` gives the closure ownership of what it uses, because
            // the thread can outlive this function.
            .spawn(move || {
                let mut last = stamps(&folder);
                loop {
                    std::thread::sleep(INTERVAL);
                    if thread_stop.load(Ordering::Relaxed) {
                        return;
                    }
                    let now = stamps(&folder);
                    let paths = changed(&last, &now);
                    if paths.is_empty() {
                        continue;
                    }
                    // Fails only when the editor has dropped the watcher.
                    if sender.send(paths).is_err() {
                        return;
                    }
                    if let Some(ctx) = thread_waker.get() {
                        ctx.request_repaint();
                    }
                    last = now;
                }
            });
        if let Err(err) = spawned {
            // The editor still works, just without noticing outside changes.
            log::error!("could not start watching the project folder: {err}");
        }
        Watcher {
            changes,
            stop,
            waker,
        }
    }

    /// Lets the watcher wake `ctx` up when something changes.
    pub fn wake(&self, ctx: &egui::Context) {
        if self.waker.get().is_none() {
            // Fails only if it was already set, which is fine.
            let _ = self.waker.set(ctx.clone());
        }
    }

    /// Every path that changed since the last call, sorted, without repeats.
    pub fn changes(&self) -> Vec<String> {
        let paths: BTreeSet<String> = self.changes.try_iter().flatten().collect();
        paths.into_iter().collect()
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        // The thread notices on its next look, within `INTERVAL`.
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    fn folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "axcenity-watcher-{name}-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(folder.join("scripts")).unwrap();
        folder
    }

    #[test]
    fn new_changed_and_deleted_files_are_reported_and_other_files_are_not() {
        let folder = folder("stamps");
        std::fs::write(folder.join("world.json"), "{}").unwrap();
        std::fs::write(folder.join("scripts/a.server.lua"), "a").unwrap();
        std::fs::write(folder.join("scripts/b.server.lua"), "b").unwrap();
        let before = stamps(&folder);

        // A different size counts even where file times are coarse.
        std::fs::write(folder.join("scripts/a.server.lua"), "a changed").unwrap();
        std::fs::remove_file(folder.join("scripts/b.server.lua")).unwrap();
        std::fs::write(folder.join("scripts/c.server.lua"), "c").unwrap();
        std::fs::write(folder.join("scripts/notes.txt"), "not watched").unwrap();
        let after = stamps(&folder);
        let _ = std::fs::remove_dir_all(&folder);

        assert_eq!(
            changed(&before, &after),
            [
                "scripts/a.server.lua",
                "scripts/b.server.lua",
                "scripts/c.server.lua"
            ]
        );
        assert!(changed(&after, &after).is_empty());
    }

    #[test]
    fn the_thread_reports_a_change_within_a_second() {
        let folder = folder("thread");
        std::fs::write(folder.join("scripts/a.server.lua"), "a").unwrap();
        let watcher = Watcher::start(folder.clone());
        // Let the thread take its first look before changing anything.
        std::thread::sleep(Duration::from_millis(100));

        std::fs::write(folder.join("scripts/a.server.lua"), "edited elsewhere").unwrap();
        let written = Instant::now();
        let mut seen = Vec::new();
        while seen.is_empty() && written.elapsed() < Duration::from_secs(3) {
            std::thread::sleep(Duration::from_millis(20));
            seen = watcher.changes();
        }
        let took = written.elapsed();
        drop(watcher);
        let _ = std::fs::remove_dir_all(&folder);

        assert_eq!(seen, ["scripts/a.server.lua"]);
        assert!(took < Duration::from_secs(1), "took {took:?}");
    }
}
