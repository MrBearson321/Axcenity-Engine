//! The editor's watcher on its open project: `project::Watcher` over every
//! file the editor reads, waking egui when something changed.

use std::path::PathBuf;

use axcenity_project::Watched;
use eframe::egui;

/// Watches one project folder until it's dropped.
pub struct Watcher(axcenity_project::Watcher);

impl Watcher {
    /// Starts watching `folder`: the world file, scripts, stylesheets and
    /// everything under `assets/`. Changes from now on are reported.
    pub fn start(folder: PathBuf) -> Watcher {
        Watcher(axcenity_project::Watcher::start(
            folder,
            Watched::Everything,
        ))
    }

    /// Lets the watcher wake `ctx` up when something changes.
    pub fn wake(&self, ctx: &egui::Context) {
        if !self.0.wakes() {
            let ctx = ctx.clone();
            self.0.on_change(move || ctx.request_repaint());
        }
    }

    /// Every path that changed since the last call, sorted, without repeats.
    pub fn changes(&self) -> Vec<String> {
        self.0.changes()
    }
}
