//! Noticing changes made to the project's files by other programs, such as
//! a script edited in VS Code or a model saved over from Blender.
//!
//! A background thread looks at the watched files and compares each file's
//! size and modification time with the last look. It looks when the OS says
//! something in the folder changed (`notify`: inotify, FSEvents,
//! ReadDirectoryChangesW), and every few seconds in case an event was lost.
//! Where the OS can't watch the folder (out of inotify watches, some network
//! drives), it looks twice a second instead, as it always did before
//! roadmap I10. Either way, a file whose stamp is still changing a moment
//! later is left for the next look, so a half-written file isn't reported.
//! When something changed, it sends the changed paths to the host and wakes
//! it up, so an idle editor stays idle otherwise.
//!
//! The watcher only says *which* files changed. The host reads them and
//! compares them with what it already has, so its own saves (which change
//! the files too) do nothing. It's for development only: the editor
//! watches every project it opens, and the player only with `--watch`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime};

use crate::{WORLD_FILE, find_scripts, find_stylesheets};

/// How often the thread looks at the files where the OS can't say when
/// they change.
pub const INTERVAL: Duration = Duration::from_millis(500);
/// How often the thread looks anyway when the OS does say, in case an
/// event was lost (a network drive, a full event queue).
const SAFETY_INTERVAL: Duration = Duration::from_secs(5);
/// How long a changed file must stay the same to count as written: one
/// that's still changing (a program saving it) waits for the next look.
const SETTLE: Duration = Duration::from_millis(100);

/// The folder of the files [`Watched::Assets`] covers.
const ASSETS_FOLDER: &str = "assets";

/// Which of the project's files a [`Watcher`] looks at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Watched {
    /// The world file, every script and stylesheet, and every file under
    /// `assets/`: the editor.
    Everything,
    /// Every file under `assets/`: `axcenity-player --watch`.
    Assets,
}

/// A file's modification time (if the OS reports one) and size.
type Stamp = (Option<SystemTime>, u64);

/// The stamp of each watched file, by its path in the project (such as
/// `world.json` or `scripts/pad.server.lua`). Files that aren't there are left out.
pub type Stamps = BTreeMap<String, Stamp>;

/// The stamps of the `watched` files in `folder` now.
#[cfg(test)]
fn stamps(folder: &Path, watched: Watched) -> Stamps {
    stamps_of(folder, watched_paths(folder, watched))
}

/// The paths of the `watched` files in `folder`, whether or not they're there.
fn watched_paths(folder: &Path, watched: Watched) -> Vec<String> {
    // Folders that can't be read just look empty; the host reports
    // problems when it reads the files themselves.
    let mut paths = Vec::new();
    if watched == Watched::Everything {
        let mut ignored = Vec::new();
        paths.push(WORLD_FILE.to_string());
        paths.extend(find_scripts(folder, &mut ignored));
        // Scripts inside objects' files (G2).
        crate::scripts::find_script_files(
            &folder.join(crate::OBJECTS_FOLDER),
            crate::OBJECTS_FOLDER,
            &mut paths,
            &mut ignored,
        );
        paths.extend(find_stylesheets(folder, &mut ignored));
    }
    find_files(&folder.join(ASSETS_FOLDER), ASSETS_FOLDER, &mut paths);
    paths
}

/// The stamps of the files at `paths` in `folder` now.
fn stamps_of(folder: &Path, paths: Vec<String>) -> Stamps {
    paths
        .into_iter()
        .filter_map(|path| {
            let metadata = std::fs::metadata(folder.join(&path)).ok()?;
            Some((path, (metadata.modified().ok(), metadata.len())))
        })
        .collect()
}

/// Adds the files in `directory` and its folders to `found`, by their
/// project paths (`path` is `directory`'s). Hidden files and folders (such
/// as `.git`) are skipped.
fn find_files(directory: &Path, path: &str, found: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let inner = format!("{path}/{name}");
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => find_files(&entry.path(), &inner, found),
            Ok(kind) if kind.is_file() => found.push(inner),
            _ => {}
        }
    }
}

/// True for a path [`Watched::Assets`] covers (`assets/…`).
pub fn is_asset_path(path: &str) -> bool {
    path.starts_with("assets/")
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

/// The paths changed since `last` that were the same `now` and a moment
/// later (`settled`), and the stamps to compare the next look with: a file
/// still being written keeps its `last` stamp, so it's reported once it's
/// done.
fn written(last: &Stamps, now: &Stamps, settled: Stamps) -> (Vec<String>, Stamps) {
    let writing = changed(now, &settled);
    let mut next = settled;
    for path in &writing {
        match last.get(path) {
            Some(stamp) => next.insert(path.clone(), *stamp),
            None => next.remove(path),
        };
    }
    let paths = changed(last, &next);
    (paths, next)
}

/// Called on the watcher's thread when something changed.
type Waker = Box<dyn Fn() + Send + Sync>;

/// Names the files a [`Watcher`] looks at in its folder, by their paths
/// inside it. Called at every look, so new files are noticed.
type Lister = Box<dyn Fn(&Path) -> Vec<String> + Send>;

/// The paths the OS says changed (absolute), or none when it only says to
/// look (it lost track, or failed to say what).
type Signal = Vec<PathBuf>;

/// How the thread knows when to look.
enum Trigger {
    /// The OS says when something in the folder changed.
    Events(Receiver<Signal>),
    /// The thread looks every [`INTERVAL`].
    Polling,
}

/// How a [`Watcher`] should find out about changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// The OS's events if it has them for the folder, else polling.
    Events,
    /// Polling only.
    #[cfg_attr(not(test), allow(dead_code))]
    Polling,
}

/// Watches one project folder until it's dropped.
pub struct Watcher {
    changes: Receiver<Vec<String>>,
    /// Set when the watcher is dropped, to end the thread.
    stop: Arc<AtomicBool>,
    /// What wakes the host up. It's given once the host can be woken
    /// (tests have nothing to wake), so the thread waits for it.
    waker: Arc<OnceLock<Waker>>,
    /// The OS's watch on the folder, when it has one. Dropping it ends the
    /// events, which ends the thread.
    events: Option<notify::RecommendedWatcher>,
}

impl Watcher {
    /// Starts watching the `watched` files in `folder`. Changes from now on
    /// are reported.
    pub fn start(folder: PathBuf, watched: Watched) -> Watcher {
        Self::start_listing(folder, move |folder| watched_paths(folder, watched))
    }

    /// Starts watching the files `list` names in `folder`, such as the
    /// engine's source files for the editor's rebuild. Changes from now on
    /// are reported.
    pub fn start_listing(
        folder: PathBuf,
        list: impl Fn(&Path) -> Vec<String> + Send + 'static,
    ) -> Watcher {
        Self::start_with(folder, Box::new(list), Mode::Events)
    }

    fn start_with(folder: PathBuf, list: Lister, mode: Mode) -> Watcher {
        let stamps = move |folder: &Path| stamps_of(folder, list(folder));
        let (sender, changes) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let waker = Arc::new(OnceLock::<Waker>::new());
        // The thread gets its own handles to `stop` and `waker`: an `Arc` is
        // a shared pointer, and cloning it only adds another owner.
        let (thread_stop, thread_waker) = (stop.clone(), waker.clone());
        // Taken here, not on the thread: a file saved just after this call
        // would otherwise be in the first look, and never count as changed.
        // The OS's watch starts before the first look, so nothing saved
        // between the two is missed.
        let (events, trigger) = match mode {
            Mode::Events => match watch_events(&folder) {
                Ok((watcher, signals)) => (Some(watcher), Trigger::Events(signals)),
                Err(err) => {
                    eprintln!(
                        "could not have the OS watch `{}` ({err}); looking at its files twice a second instead",
                        folder.display()
                    );
                    (None, Trigger::Polling)
                }
            },
            Mode::Polling => (None, Trigger::Polling),
        };
        // The folder as the OS may name it in events (macOS reports
        // `/private/var/…` for `/var/…`).
        let real_folder = std::fs::canonicalize(&folder).unwrap_or_else(|_| folder.clone());
        let mut last = stamps(&folder);
        let spawned = std::thread::Builder::new()
            .name("project watcher".to_string())
            // `move` gives the closure ownership of what it uses, because
            // the thread can outlive this function.
            .spawn(move || {
                loop {
                    match &trigger {
                        Trigger::Polling => std::thread::sleep(INTERVAL),
                        Trigger::Events(signals) => match signals.recv_timeout(SAFETY_INTERVAL) {
                            Ok(first) => {
                                // A burst of events is one look: the look
                                // below sees every change made before it.
                                let roots = [folder.as_path(), real_folder.as_path()];
                                let mut look = worth_a_look(&roots, &first, &last);
                                for more in signals.try_iter() {
                                    look = look || worth_a_look(&roots, &more, &last);
                                }
                                if !look {
                                    continue;
                                }
                            }
                            Err(RecvTimeoutError::Timeout) => {}
                            // The watcher was dropped.
                            Err(RecvTimeoutError::Disconnected) => return,
                        },
                    }
                    if thread_stop.load(Ordering::Relaxed) {
                        return;
                    }
                    let now = stamps(&folder);
                    if changed(&last, &now).is_empty() {
                        continue;
                    }
                    std::thread::sleep(SETTLE);
                    let settled = stamps(&folder);
                    let (paths, next) = written(&last, &now, settled);
                    last = next;
                    if paths.is_empty() {
                        continue;
                    }
                    // Fails only when the host has dropped the watcher.
                    if sender.send(paths).is_err() {
                        return;
                    }
                    if let Some(wake) = thread_waker.get() {
                        wake();
                    }
                }
            });
        if let Err(err) = spawned {
            // The host still works, just without noticing outside changes.
            eprintln!("could not start watching the project folder: {err}");
        }
        Watcher {
            changes,
            stop,
            waker,
            events,
        }
    }

    /// Whether the OS says when the files change; false when the thread
    /// polls instead.
    pub fn uses_events(&self) -> bool {
        self.events.is_some()
    }

    /// Calls `wake` (on the watcher's thread) each time something changes.
    /// Only the first call counts.
    pub fn on_change(&self, wake: impl Fn() + Send + Sync + 'static) {
        // Fails only if it was already set, which is fine.
        let _ = self.waker.set(Box::new(wake));
    }

    /// Whether [`on_change`](Self::on_change) was called.
    pub fn wakes(&self) -> bool {
        self.waker.get().is_some()
    }

    /// Every path that changed since the last call, sorted, without repeats.
    pub fn changes(&self) -> Vec<String> {
        let paths: BTreeSet<String> = self.changes.try_iter().flatten().collect();
        paths.into_iter().collect()
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        // A polling thread notices on its next look, within `INTERVAL`; an
        // event thread when the OS's watch (dropped next) stops sending.
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Has the OS watch `folder` and everything in it but hidden folders and
/// the top `target` folder, sending what it says changed. Reading a file
/// (`Access`) isn't a change.
///
/// Each top folder is watched on its own so `.git` and Cargo's `target`
/// (tens of thousands of folders, for the engine's own sources) never use
/// up the OS's watches. A top folder made later is seen being made, and
/// what's in it at each safety look.
fn watch_events(folder: &Path) -> notify::Result<(notify::RecommendedWatcher, Receiver<Signal>)> {
    use notify::{EventKind, RecursiveMode, Watcher as _};

    let (sender, signals) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        let paths = match event {
            Ok(event) if matches!(event.kind, EventKind::Access(_)) => return,
            Ok(event) if event.need_rescan() => Vec::new(),
            Ok(event) => event.paths,
            Err(_) => Vec::new(),
        };
        // Fails only once the thread has ended.
        let _ = sender.send(paths);
    })?;
    watcher.watch(folder, RecursiveMode::NonRecursive)?;
    for entry in std::fs::read_dir(folder)?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_folder = entry.file_type().is_ok_and(|kind| kind.is_dir());
        if is_folder && !name.starts_with('.') && name != "target" {
            watcher.watch(&entry.path(), RecursiveMode::Recursive)?;
        }
    }
    Ok((watcher, signals))
}

/// Whether an event about `paths` (absolute, in the folder at one of
/// `roots`) could change what's watched: a file seen at the `last` look, or
/// a new one outside hidden folders (such as `.git` or `.axcenity`) and the
/// top `target` folder (Cargo's builds, for the engine's own sources). No
/// paths, or one outside the folder, always could.
fn worth_a_look(roots: &[&Path], paths: &[PathBuf], last: &Stamps) -> bool {
    paths.is_empty()
        || paths.iter().any(|path| {
            let Some(inside) = roots.iter().find_map(|root| path.strip_prefix(root).ok()) else {
                return true;
            };
            let parts: Vec<String> = inside
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect();
            let project_path = parts.join("/");
            let ignored = parts.iter().any(|part| part.starts_with('.'))
                || parts.first().is_some_and(|top| top == "target");
            last.contains_key(&project_path) || !ignored
        })
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
        let before = stamps(&folder, Watched::Everything);

        // A different size counts even where file times are coarse.
        std::fs::write(folder.join("scripts/a.server.lua"), "a changed").unwrap();
        std::fs::remove_file(folder.join("scripts/b.server.lua")).unwrap();
        std::fs::write(folder.join("scripts/c.server.lua"), "c").unwrap();
        std::fs::write(folder.join("scripts/notes.txt"), "not watched").unwrap();
        std::fs::create_dir_all(folder.join("assets/materials/stone")).unwrap();
        std::fs::write(
            folder.join("assets/materials/stone/rock.material.json"),
            "{}",
        )
        .unwrap();
        std::fs::create_dir_all(folder.join("assets/.cache")).unwrap();
        std::fs::write(folder.join("assets/.cache/thumb.png"), "hidden").unwrap();
        std::fs::write(folder.join("notes.json"), "{}").unwrap();
        let after = stamps(&folder, Watched::Everything);
        let _ = std::fs::remove_dir_all(&folder);

        assert_eq!(
            changed(&before, &after),
            [
                "assets/materials/stone/rock.material.json",
                "scripts/a.server.lua",
                "scripts/b.server.lua",
                "scripts/c.server.lua"
            ]
        );
        assert!(changed(&after, &after).is_empty());
    }

    #[test]
    fn a_file_still_being_written_waits_for_the_next_look() {
        let stamp = |size| (None, size);
        let last = Stamps::from([("a".to_string(), stamp(1)), ("b".to_string(), stamp(1))]);
        // `a` was saved; `b` is halfway through being written.
        let now = Stamps::from([("a".to_string(), stamp(2)), ("b".to_string(), stamp(5))]);
        let settled = Stamps::from([("a".to_string(), stamp(2)), ("b".to_string(), stamp(9))]);
        let (paths, next) = written(&last, &now, settled);
        assert_eq!(paths, ["a"]);
        assert_eq!(next["b"], stamp(1));
        // Done by the next look: reported then.
        let done = Stamps::from([("a".to_string(), stamp(2)), ("b".to_string(), stamp(9))]);
        let (paths, _) = written(&next, &done, done.clone());
        assert_eq!(paths, ["b"]);
    }

    #[test]
    fn every_kind_of_asset_is_watched_and_the_player_watches_only_assets() {
        let folder = folder("assets");
        std::fs::write(folder.join("world.json"), "{}").unwrap();
        std::fs::write(folder.join("scripts/a.server.lua"), "a").unwrap();
        let files = [
            "assets/models/hero/hero.fbx",
            "assets/models/hero/textures/skin.png",
            "assets/textures/Ground/Ground.texture.json",
            "assets/audio/chime.wav",
            "assets/fonts/Title.ttf",
            "assets/animations/hero/Walk.anim.json",
            "assets/animations/hero/hero.statemachine.json",
            "assets/prefabs/Crate.prefab.json",
        ];
        for file in files {
            let path = folder.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "x").unwrap();
        }
        let everything: Vec<String> = stamps(&folder, Watched::Everything).into_keys().collect();
        let assets: Vec<String> = stamps(&folder, Watched::Assets).into_keys().collect();
        let _ = std::fs::remove_dir_all(&folder);

        let mut expected: Vec<String> = files.iter().map(|file| file.to_string()).collect();
        expected.sort();
        assert_eq!(assets, expected);
        assert!(assets.iter().all(|path| is_asset_path(path)));
        for path in files.iter().chain(&["world.json", "scripts/a.server.lua"]) {
            assert!(everything.iter().any(|seen| seen == path), "{path}");
        }
    }

    #[test]
    fn events_in_hidden_folders_and_target_are_not_worth_a_look_unless_watched() {
        let root = Path::new("/projects/game");
        let roots = [root];
        let last = Stamps::from([(".hidden/known.txt".to_string(), (None, 1))]);
        let look = |paths: &[&str]| {
            let paths: Vec<PathBuf> = paths.iter().map(|path| root.join(path)).collect();
            worth_a_look(&roots, &paths, &last)
        };
        assert!(look(&["assets/models/a.glb"]), "a new or changed file");
        assert!(look(&["assets"]), "a folder: a file may have moved in");
        assert!(!look(&[".axcenity/imported/a.bin"]), "a hidden folder");
        assert!(
            !look(&["assets/.cache/thumb.png"]),
            "a hidden folder inside"
        );
        assert!(!look(&["target/debug/axcenity-editor"]), "Cargo's builds");
        assert!(look(&["crates/target/lib.rs"]), "only the top `target`");
        assert!(look(&[".hidden/known.txt"]), "a file the lister names");
        assert!(look(&[".git/index", "world.json"]), "any one path counts");
        assert!(look(&[]), "the OS lost track: look");
        assert!(
            worth_a_look(&roots, &[PathBuf::from("/elsewhere/a")], &last),
            "a path outside the folder: look"
        );
    }

    /// Waits up to three seconds for `watcher` to report something.
    fn first_changes(watcher: &Watcher) -> (Vec<String>, Duration) {
        let written = Instant::now();
        let mut seen = Vec::new();
        while seen.is_empty() && written.elapsed() < Duration::from_secs(3) {
            std::thread::sleep(Duration::from_millis(5));
            seen = watcher.changes();
        }
        (seen, written.elapsed())
    }

    #[test]
    fn the_polling_fallback_reports_a_change_within_a_second() {
        let folder = folder("polling");
        std::fs::create_dir_all(folder.join("assets/models")).unwrap();
        std::fs::write(folder.join("assets/models/a.glb"), "a").unwrap();
        let watcher = Watcher::start_with(
            folder.clone(),
            Box::new(|folder: &Path| watched_paths(folder, Watched::Assets)),
            Mode::Polling,
        );
        assert!(!watcher.uses_events());
        std::fs::write(folder.join("assets/models/a.glb"), "saved again").unwrap();
        let (seen, took) = first_changes(&watcher);
        drop(watcher);
        let _ = std::fs::remove_dir_all(&folder);
        assert_eq!(seen, ["assets/models/a.glb"]);
        assert!(took < Duration::from_secs(1), "took {took:?}");
    }

    /// Prints how long each way takes to report a saved file:
    /// `cargo test -p axcenity-project --release -- --ignored measure_reload_latency --nocapture`.
    #[test]
    #[ignore = "measurement: run with --ignored measure_reload_latency --nocapture"]
    fn measure_reload_latency() {
        for mode in [Mode::Polling, Mode::Events] {
            let folder = folder(&format!("latency-{mode:?}"));
            std::fs::create_dir_all(folder.join("assets/models")).unwrap();
            std::fs::write(folder.join("assets/models/a.glb"), "a").unwrap();
            let watcher = Watcher::start_with(
                folder.clone(),
                Box::new(|folder: &Path| watched_paths(folder, Watched::Assets)),
                mode,
            );
            let mut times = Vec::new();
            for round in 0..20 {
                // Saves land at different points of the polling cycle.
                std::thread::sleep(Duration::from_millis(137));
                let text = format!("save {round} {}", "x".repeat(round));
                std::fs::write(folder.join("assets/models/a.glb"), text).unwrap();
                let (seen, took) = first_changes(&watcher);
                assert_eq!(seen, ["assets/models/a.glb"]);
                times.push(took);
            }
            drop(watcher);
            let _ = std::fs::remove_dir_all(&folder);
            times.sort();
            println!(
                "{mode:?}: median {:?}, worst {:?} over {} saves",
                times[times.len() / 2],
                times[times.len() - 1],
                times.len()
            );
        }
    }

    #[test]
    fn the_thread_reports_a_change_within_a_second_and_wakes_the_host() {
        let folder = folder("thread");
        std::fs::create_dir_all(folder.join("assets/models")).unwrap();
        std::fs::write(folder.join("assets/models/a.glb"), "a").unwrap();
        let watcher = Watcher::start(folder.clone(), Watched::Assets);
        // Every desktop OS the engine builds for has events.
        assert!(watcher.uses_events());
        let woken = Arc::new(AtomicBool::new(false));
        let flag = woken.clone();
        watcher.on_change(move || flag.store(true, Ordering::Relaxed));
        // Let the thread take its first look before changing anything.
        std::thread::sleep(Duration::from_millis(100));

        std::fs::write(folder.join("assets/models/a.glb"), "saved from Blender").unwrap();
        let written = Instant::now();
        let mut seen = Vec::new();
        while seen.is_empty() && written.elapsed() < Duration::from_secs(3) {
            std::thread::sleep(Duration::from_millis(20));
            seen = watcher.changes();
        }
        let took = written.elapsed();
        drop(watcher);
        let _ = std::fs::remove_dir_all(&folder);

        assert_eq!(seen, ["assets/models/a.glb"]);
        assert!(took < Duration::from_secs(1), "took {took:?}");
        assert!(woken.load(Ordering::Relaxed));
    }
}
