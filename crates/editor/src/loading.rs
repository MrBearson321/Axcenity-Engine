//! Opening a project in the background, with a loading screen that says
//! what's being read.
//!
//! A thread reads the project's files (`project::load_with_progress`), then
//! every model and texture the Workspace draws (`scene::asset_files`), then
//! builds each Terrain's chunks as the starting camera sees them. Those used
//! to happen in the first frame, which froze the window on an empty scene.
//! The thread writes what it's on into a shared [`Status`] and asks egui for
//! a frame each time, so the screen follows along. The app takes the
//! [`Loaded`] project when the thread is done (see `app/session.rs`).
//!
//! The bar is real progress: reading files is the first part of it, models
//! and textures the middle, and terrains the end. Each part fills by how
//! many of its files are done.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use axcenity_assets::Assets;
use axcenity_core::{ClassName, InstanceId};
use axcenity_project::{LoadError, LoadStep, Project};
use axcenity_render::Camera;
use axcenity_scene::{AssetFile, TerrainCache, asset_files};
use eframe::egui::{self, Align2, FontId, Rect, Sense, vec2};

use crate::theme::{self, type_scale};
use crate::widgets::{self, Role};

/// Where each part of loading starts on the bar, and where it ends.
const FILES: (f32, f32) = (0.0, 0.35);
const ASSETS: (f32, f32) = (0.35, 0.85);
const TERRAINS: (f32, f32) = (0.85, 1.0);
/// How many finished files are listed under the bar.
const RECENT: usize = 5;

/// A project read in the background, ready to open.
pub struct Loaded {
    pub project: Project,
    /// Holding every model and texture the Workspace draws.
    pub assets: Assets,
    /// Holding every Terrain's chunks, as seen from the starting camera.
    pub terrain: TerrainCache,
}

/// What the loading thread is on, read by the screen each frame.
#[derive(Clone, Debug, Default)]
struct Status {
    /// Such as "Loading model assets/models/chair.glb".
    doing: String,
    /// The file being read and how many of its kind there are.
    count: Option<(usize, usize)>,
    /// How full the bar is, from 0 to 1.
    fill: f32,
    /// The last files finished, newest last.
    recent: Vec<String>,
}

/// A project being opened. Dropping it cancels the load.
pub struct Loading {
    pub folder: PathBuf,
    /// The project's name, or its folder's when the manifest can't be read.
    pub name: String,
    status: Arc<Mutex<Status>>,
    cancel: Arc<AtomicBool>,
    done: Receiver<Result<Loaded, LoadError>>,
    started: Instant,
}

impl Loading {
    /// Starts reading the project in `folder`. `terrain` is an empty cache
    /// with the viewport's settings; `camera` is where the viewport starts.
    pub fn start(
        ctx: &egui::Context,
        folder: &Path,
        terrain: TerrainCache,
        camera: Camera,
    ) -> Self {
        let name = axcenity_project::read_manifest(folder)
            .map(|manifest| manifest.name)
            .or_else(|| {
                folder
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "project".to_owned());
        let status = Arc::new(Mutex::new(Status {
            doing: "Starting".to_owned(),
            ..Status::default()
        }));
        let cancel = Arc::new(AtomicBool::new(false));
        let (sender, done) = mpsc::channel();
        let job = Job {
            folder: folder.to_owned(),
            status: status.clone(),
            cancel: cancel.clone(),
            ctx: ctx.clone(),
        };
        // `move` gives the closure its own `job`, `terrain` and `sender`,
        // so they live on the thread. If the app gave up on this load, the
        // receiver is gone and `send` fails, which is fine.
        std::thread::spawn(move || {
            let result = job.run(terrain, camera);
            let _ = sender.send(result);
            job.ctx.request_repaint();
        });
        Self {
            folder: folder.to_owned(),
            name,
            status,
            cancel,
            done,
            started: Instant::now(),
        }
    }

    /// The loaded project once the thread is done, or `None` while it's
    /// still reading.
    pub fn poll(&self) -> Option<Result<Loaded, String>> {
        match self.done.try_recv() {
            Ok(result) => Some(result.map_err(|err| {
                // `{:#}` shows the whole chain of causes on one line.
                format!("{:#}", anyhow::Error::new(err))
            })),
            Err(TryRecvError::Empty) => None,
            // The thread ended without sending: it panicked.
            Err(TryRecvError::Disconnected) => Some(Err("loading stopped unexpectedly".to_owned())),
        }
    }

    fn status(&self) -> Status {
        lock(&self.status).clone()
    }

    /// Draws the loading screen over `rect`. Returns true when Cancel was
    /// clicked.
    pub fn show(&self, ui: &mut egui::Ui, rect: Rect) -> bool {
        let status = self.status();
        paint(
            ui,
            rect,
            &self.name,
            &status,
            Some(self.started.elapsed().as_secs_f32()),
        )
    }
}

impl Drop for Loading {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// The screen held for a frame after the project opens, while its scene is
/// sent to the graphics card.
pub fn show_finishing(ui: &mut egui::Ui, rect: Rect, name: &str) {
    let status = Status {
        doing: "Preparing the scene".to_owned(),
        fill: 1.0,
        ..Status::default()
    };
    paint(ui, rect, name, &status, None);
}

fn lock(status: &Mutex<Status>) -> std::sync::MutexGuard<'_, Status> {
    // A panic on the thread leaves a readable status, so carry on with it.
    status.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What the loading thread owns.
struct Job {
    folder: PathBuf,
    status: Arc<Mutex<Status>>,
    cancel: Arc<AtomicBool>,
    ctx: egui::Context,
}

impl Job {
    fn run(&self, mut terrain: TerrainCache, camera: Camera) -> Result<Loaded, LoadError> {
        let project = axcenity_project::load_with_progress(&self.folder, &mut |step| {
            let (doing, count, at) = describe(step);
            self.report(doing, count, FILES, at);
        })?;

        let assets = Assets::new(&project.folder);
        let files = asset_files(&project.world);
        for (index, file) in files.iter().enumerate() {
            if self.cancelled() {
                break;
            }
            let kind = match file {
                AssetFile::Model(_) => "model",
                AssetFile::Texture(_) => "texture",
                AssetFile::Material(_) => "material",
            };
            let at = index as f32 / files.len() as f32;
            self.report(
                format!("Loading {kind} {}", file.path()),
                Some((index, files.len())),
                ASSETS,
                at,
            );
            file.load(&assets);
            self.finished(file.path().to_owned());
        }

        let world = &project.world;
        let terrains: Vec<InstanceId> = world
            .descendants(world.workspace())
            .into_iter()
            .filter(|&id| world.get(id).map(|i| i.class()) == Some(ClassName::Terrain))
            .collect();
        for (index, &id) in terrains.iter().enumerate() {
            if self.cancelled() {
                break;
            }
            let name = world.get(id).map_or("", |i| i.name()).to_owned();
            let at = index as f32 / terrains.len() as f32;
            self.report(
                format!("Building terrain {name}"),
                Some((index, terrains.len())),
                TERRAINS,
                at,
            );
            terrain.prepare(world, id, &assets, &camera);
            self.finished(format!("Terrain {name}"));
        }
        Ok(Loaded {
            project,
            assets,
            terrain,
        })
    }

    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// Says what's being read now: `at` (0 to 1) of the way through `part`.
    fn report(&self, doing: String, count: Option<(usize, usize)>, part: (f32, f32), at: f32) {
        {
            let mut status = lock(&self.status);
            status.doing = doing;
            status.count = count;
            // Never back: a part with nothing to do is skipped over.
            status.fill = status
                .fill
                .max(part.0 + (part.1 - part.0) * at.clamp(0.0, 1.0));
        }
        self.ctx.request_repaint();
    }

    fn finished(&self, what: String) {
        let mut status = lock(&self.status);
        status.recent.push(what);
        let extra = status.recent.len().saturating_sub(RECENT);
        status.recent.drain(..extra);
    }
}

/// A step of reading the project's files, as the screen says it: what,
/// how many, and how far through the files it is (0 to 1).
fn describe(step: LoadStep) -> (String, Option<(usize, usize)>, f32) {
    let through = |start: f32, end: f32, done: usize, total: usize| {
        start + (end - start) * done as f32 / total.max(1) as f32
    };
    match step {
        LoadStep::Manifest => (
            format!("Reading {}", axcenity_project::MANIFEST_FILE),
            None,
            0.0,
        ),
        LoadStep::Upgrade { from } => (format!("Upgrading from project format {from}"), None, 0.05),
        LoadStep::World => (
            format!("Reading {}", axcenity_project::WORLD_FILE),
            None,
            0.1,
        ),
        LoadStep::Terrain { name, done, total } => (
            format!("Reading terrain {name}"),
            Some((done, total)),
            through(0.3, 0.6, done, total),
        ),
        LoadStep::Script { path, done, total } => (
            format!("Reading {path}"),
            Some((done, total)),
            through(0.6, 0.95, done, total),
        ),
        LoadStep::Stylesheets => ("Reading stylesheets".to_owned(), None, 0.95),
    }
}

/// The screen: the project's name, what's loading with how many, the bar
/// with its percentage, the last files read, and the time taken with Cancel.
/// `elapsed` is `None` once loading is done (no Cancel then).
fn paint(ui: &mut egui::Ui, rect: Rect, name: &str, status: &Status, elapsed: Option<f32>) -> bool {
    let palette = theme::palette(ui.ctx());
    // Covers what's under it, so nothing there can be clicked.
    ui.allocate_rect(rect, Sense::click());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0, palette.app_bg);

    let width = (rect.width() - 64.0).clamp(240.0, 420.0);
    let left = rect.center().x - width * 0.5;
    let mut y = rect.center().y - 90.0;
    painter.text(
        egui::pos2(left, y),
        Align2::LEFT_TOP,
        "Opening",
        FontId::proportional(type_scale::CAPTION),
        palette.text_muted,
    );
    y += 18.0;
    let title = widgets::elided(
        ui,
        name.to_owned(),
        FontId::new(type_scale::DISPLAY, theme::heading_family()),
        palette.text_strong,
        width,
        1,
    );
    let title_height = title.size().y;
    painter.galley(egui::pos2(left, y), title, palette.text_strong);
    y += title_height + 22.0;

    let count_text = status
        .count
        .map(|(done, total)| format!("{} of {total}", (done + 1).min(total)));
    let count_width = count_text.as_ref().map_or(0.0, |text| {
        painter
            .layout_no_wrap(text.clone(), FontId::monospace(11.0), palette.text_muted)
            .size()
            .x
            + 12.0
    });
    let doing = widgets::elided(
        ui,
        format!("{}…", status.doing),
        FontId::new(type_scale::BODY, theme::medium_family()),
        palette.text_secondary,
        width - count_width,
        1,
    );
    painter.galley(egui::pos2(left, y), doing, palette.text_secondary);
    if let Some(text) = count_text {
        painter.text(
            egui::pos2(left + width, y + 1.0),
            Align2::RIGHT_TOP,
            text,
            FontId::monospace(11.0),
            palette.text_muted,
        );
    }
    y += 24.0;

    let fill = status.fill.clamp(0.0, 1.0);
    let bar = Rect::from_min_size(egui::pos2(left, y), vec2(width, 4.0));
    painter.rect_filled(bar, 2, palette.inset);
    if fill > 0.0 {
        let filled = Rect::from_min_size(bar.min, vec2(width * fill, bar.height()));
        painter.rect_filled(filled, 2, palette.accent);
    }
    y += 12.0;
    painter.text(
        egui::pos2(left + width, y),
        Align2::RIGHT_TOP,
        format!("{}%", (fill * 100.0).round() as u32),
        FontId::monospace(11.0),
        palette.text_muted,
    );

    // The last files read, the newest brightest.
    for (age, done) in status.recent.iter().rev().enumerate() {
        let fade = 1.0 - age as f32 / (RECENT as f32 + 1.0);
        let color = palette.text_muted.gamma_multiply(fade);
        let line = widgets::elided(
            ui,
            theme::with_icon(theme::icons::CHECK, done),
            FontId::proportional(type_scale::CAPTION),
            color,
            width - 48.0,
            1,
        );
        painter.galley(egui::pos2(left, y + age as f32 * 17.0), line, color);
    }

    let Some(elapsed) = elapsed else {
        return false;
    };
    let below = y + RECENT as f32 * 17.0 + 18.0;
    painter.text(
        egui::pos2(left, below + 6.0),
        Align2::LEFT_TOP,
        format!("{elapsed:.0}s"),
        FontId::monospace(11.0),
        palette.text_muted,
    );
    let cancel = Rect::from_min_size(egui::pos2(left + width - 80.0, below), vec2(80.0, 28.0));
    ui.put(
        cancel,
        widgets::button(Role::Secondary, None, "Cancel", &palette),
    )
    .clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading_the_files_moves_forward_through_the_first_part_of_the_bar() {
        let steps = [
            LoadStep::Manifest,
            LoadStep::World,
            LoadStep::Terrain {
                name: "Island",
                done: 0,
                total: 1,
            },
            LoadStep::Script {
                path: "Main.server.lua",
                done: 0,
                total: 2,
            },
            LoadStep::Script {
                path: "Hud.client.lua",
                done: 1,
                total: 2,
            },
            LoadStep::Stylesheets,
        ];
        let mut last = 0.0;
        for step in steps {
            let (_, _, at) = describe(step);
            assert!((last..=1.0).contains(&at), "{step:?} went back");
            last = at;
        }
        assert_eq!(describe(steps[3]).0, "Reading Main.server.lua");
    }

    #[test]
    fn the_loading_screen_draws() {
        let status = Status {
            doing: "Loading model assets/models/castle_gate_with_a_long_name.glb".to_owned(),
            count: Some((6, 24)),
            fill: 0.52,
            recent: (1..=RECENT)
                .map(|n| format!("assets/textures/stone_{n}.png"))
                .collect(),
        };
        let mut draw = |ctx: &egui::Context| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    paint(ui, ui.max_rect(), "Castle Siege", &status, Some(12.4));
                });
        };
        match std::env::var_os("AXCENITY_SNAPSHOTS").map(PathBuf::from) {
            Some(folder) => crate::snapshot::snapshot(
                &folder.join("loading.png"),
                vec2(960.0, 600.0),
                vec![Vec::new(); 2],
                &mut draw,
            ),
            None => {
                let ctx = theme::test_context();
                let _ = ctx.run(Default::default(), &mut draw);
            }
        }
    }
}
