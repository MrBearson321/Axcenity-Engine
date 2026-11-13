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
//! Nothing on the screen is made up. The bar fills by how many of the
//! files of the kind being read are done ("12 of 40"); while the thread is
//! on a step with no count, such as reading `world.json`, it shows a sliding
//! bar that only says it's working. A project that opens in less than
//! [`GRACE`] never shows the screen at all: it stays blank until then.

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

/// How long the logo loader's band takes to cross the mark, in seconds.
const SWEEP_SECONDS: f32 = 1.3;
/// How long a load runs before its screen shows, in seconds: one that's
/// done sooner needs no screen.
pub const GRACE: f32 = 0.3;
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
    /// How many files of the kind being read are done, and how many there
    /// are. `None` for a step with nothing to count.
    count: Option<(usize, usize)>,
    /// The last files finished, newest last.
    recent: Vec<String>,
    /// Why this step takes a while, when it does, under the bar.
    note: Option<String>,
}

/// A project being opened. Dropping it cancels the load.
pub struct Loading {
    pub folder: PathBuf,
    /// The level it opens in (`None`: the start level).
    pub level: Option<String>,
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
        level: Option<String>,
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
            level: level.clone(),
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
            level,
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

    /// Whether the screen is showing: the load has taken [`GRACE`] or more.
    pub fn shown(&self) -> bool {
        self.started.elapsed().as_secs_f32() >= GRACE
    }

    /// Draws the loading screen over `rect`, or only its background for the
    /// first [`GRACE`] seconds. Returns true when Cancel was clicked.
    pub fn show(&self, ui: &mut egui::Ui, rect: Rect, mark: Option<&egui::TextureHandle>) -> bool {
        let elapsed = self.started.elapsed().as_secs_f32();
        if elapsed < GRACE {
            paint_blank(ui, rect);
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f32(GRACE - elapsed));
            return false;
        }
        let status = self.status();
        paint(ui, rect, &self.name, &status, Some(elapsed), mark)
    }
}

impl Drop for Loading {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// The screen held after the project opens, while its scene is sent to
/// the graphics card: `uploads` is how many of the meshes and textures on
/// their way are there, of how many, while any are. After a load too quick
/// to show its screen (`shown` false), only the background, so nothing
/// flashes up.
pub fn show_finishing(
    ui: &mut egui::Ui,
    rect: Rect,
    name: &str,
    shown: bool,
    uploads: Option<(usize, usize)>,
    mark: Option<&egui::TextureHandle>,
) {
    if !shown {
        paint_blank(ui, rect);
        return;
    }
    let status = Status {
        doing: finishing_text(uploads),
        count: uploads,
        ..Status::default()
    };
    paint(ui, rect, name, &status, None, mark);
}

/// What the screen says while the scene goes to the graphics card.
fn finishing_text(uploads: Option<(usize, usize)>) -> String {
    match uploads {
        Some(_) => "Sending models and textures to the graphics card".to_owned(),
        None => "Getting the editor ready".to_owned(),
    }
}

/// How many frames in a row must come quickly for the editor to count as
/// ready after a project opens (see [`Settling`]).
pub const SETTLED_FRAMES: u32 = 5;
/// A frame this quick, in seconds, counts toward [`SETTLED_FRAMES`].
const QUICK_FRAME: f32 = 0.1;

/// Whether the editor's frames have settled after a project opens. The
/// first frames that draw a scene make pipelines, build ray tracing's
/// acceleration structures and so on, each a stall: the loading screen
/// stays up until [`SETTLED_FRAMES`] frames in a row each take no more
/// than [`QUICK_FRAME`] (or twice the quickest frame yet, on a computer
/// that never draws the scene that fast), so the editor answers the
/// moment it's gone.
#[derive(Debug, Default)]
pub struct Settling {
    /// egui's number for the last frame counted, and when it started.
    last: Option<(u64, Instant)>,
    quickest: Option<f32>,
    quick: u32,
}

impl Settling {
    /// Counts frame `number` (egui's `cumulative_frame_nr`: a frame egui
    /// lays out twice is counted once), started at `now`, and says
    /// whether frames have settled.
    pub fn frame(&mut self, number: u64, now: Instant) -> bool {
        match self.last {
            Some((last_number, _)) if last_number == number => {}
            Some((_, last)) => {
                self.last = Some((number, now));
                let seconds = now.saturating_duration_since(last).as_secs_f32();
                let quickest = self.quickest.map_or(seconds, |q| q.min(seconds));
                self.quickest = Some(quickest);
                if seconds <= QUICK_FRAME.max(quickest * 2.0) {
                    self.quick += 1;
                } else {
                    self.quick = 0;
                }
            }
            None => self.last = Some((number, now)),
        }
        self.settled()
    }

    /// Whether the last [`SETTLED_FRAMES`] frames came quickly.
    pub fn settled(&self) -> bool {
        self.quick >= SETTLED_FRAMES
    }
}

/// The screen's background alone, covering what's under it.
fn paint_blank(ui: &mut egui::Ui, rect: Rect) {
    let palette = theme::palette(ui.ctx());
    ui.allocate_rect(rect, Sense::click());
    ui.painter_at(rect).rect_filled(rect, 0, palette.app_bg);
}

fn lock(status: &Mutex<Status>) -> std::sync::MutexGuard<'_, Status> {
    // A panic on the thread leaves a readable status, so carry on with it.
    status.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What the loading thread owns.
struct Job {
    folder: PathBuf,
    level: Option<String>,
    status: Arc<Mutex<Status>>,
    cancel: Arc<AtomicBool>,
    ctx: egui::Context,
}

impl Job {
    fn run(&self, mut terrain: TerrainCache, camera: Camera) -> Result<Loaded, LoadError> {
        let project =
            axcenity_project::load_with_level(&self.folder, self.level.as_deref(), &mut |step| {
                let upgrading = matches!(step, LoadStep::Upgrade { .. });
                let (doing, count) = describe(step);
                self.report(doing, count);
                if upgrading {
                    self.note(UPGRADE_NOTE);
                }
            })?;

        let assets = Assets::new(&project.folder);
        self.load_assets(&assets, &asset_files(&project.world));
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
            self.report(
                format!("Building terrain {name}"),
                Some((index, terrains.len())),
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

    /// Reads `files` into `assets`, as many at once as the asset workers
    /// take, saying which one it's waiting on and how many are done. A
    /// model with no cooked copy yet (the first open in this version of the
    /// engine) is imported from its file, which is slower: the screen says
    /// so, so a long wait isn't a mystery.
    fn load_assets(&self, assets: &Assets, files: &[AssetFile]) {
        // Asked before any loads: a model being loaded is cooked as it goes.
        let importing: Vec<bool> = files
            .iter()
            .map(|file| match file {
                AssetFile::Model(mesh_id) => assets.model_needs_import(mesh_id),
                _ => false,
            })
            .collect();
        let tickets: Vec<_> = files.iter().map(|file| file.request(assets)).collect();
        let mut reported = vec![false; files.len()];
        let mut done = 0;
        while done < files.len() {
            if self.cancelled() {
                return;
            }
            for (index, ticket) in tickets.iter().enumerate() {
                if !reported[index] && ticket.is_finished() {
                    reported[index] = true;
                    done += 1;
                    self.finished(files[index].path().to_owned());
                }
            }
            // The first file still being read is the one named.
            if let Some(index) = (0..files.len()).find(|&index| !reported[index]) {
                self.report(describe_file(&files[index]), Some((done, files.len())));
                if importing[index] {
                    self.note(IMPORT_NOTE);
                }
                std::thread::sleep(std::time::Duration::from_millis(15));
            }
        }
        // Each one in once more, so a load that failed is reported the
        // way drawing would report it (each failure once).
        for file in files {
            file.load(assets);
        }
    }

    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// Says what's being read now, and how many of its kind are done.
    fn report(&self, doing: String, count: Option<(usize, usize)>) {
        {
            let mut status = lock(&self.status);
            status.doing = doing;
            status.count = count;
            status.note = None;
        }
        self.ctx.request_repaint();
    }

    /// Says why the step just reported takes a while.
    fn note(&self, note: &str) {
        lock(&self.status).note = Some(note.to_owned());
    }

    fn finished(&self, what: String) {
        let mut status = lock(&self.status);
        status.recent.push(what);
        let extra = status.recent.len().saturating_sub(RECENT);
        status.recent.drain(..extra);
    }
}

/// A step of reading the project's files, as the screen says it: what, and
/// how many of its kind are done, where there's something to count.
fn describe(step: LoadStep) -> (String, Option<(usize, usize)>) {
    match step {
        LoadStep::Manifest => (format!("Reading {}", axcenity_project::MANIFEST_FILE), None),
        LoadStep::Upgrade { from } => (format!("Upgrading from project format {from}"), None),
        LoadStep::World => (format!("Reading {}", axcenity_project::WORLD_FILE), None),
        LoadStep::Terrain { name, done, total } => {
            (format!("Reading terrain {name}"), Some((done, total)))
        }
        LoadStep::Script { path, done, total } => (format!("Reading {path}"), Some((done, total))),
        LoadStep::Stylesheets => ("Reading stylesheets".to_owned(), None),
    }
}

/// What the screen says under the bar while an older project upgrades.
const UPGRADE_NOTE: &str = "This project was saved by an older Axcenity. Its files are copied \
     to .axcenity/previous first, then upgraded: only this once.";

/// What the screen says under the bar while a model with no cooked copy
/// is imported.
const IMPORT_NOTE: &str = "First open in this version of Axcenity: models are imported from \
     their files and kept, so the next open is quicker.";

/// What the screen says while `file` is read.
fn describe_file(file: &AssetFile) -> String {
    match file {
        AssetFile::Model(path) => format!("Loading model {path}"),
        AssetFile::Texture(path) | AssetFile::LayerTexture(path) => {
            format!("Loading texture {path}")
        }
        AssetFile::Material(path) => format!("Loading material {path}"),
    }
}

/// How full the bar is for `count` (0 to 1), or `None` when there's
/// nothing to count and the bar only slides.
fn fill(count: Option<(usize, usize)>) -> Option<f32> {
    count
        .filter(|&(_, total)| total > 0)
        .map(|(done, total)| (done as f32 / total as f32).clamp(0.0, 1.0))
}

/// The screen: the project's name, what's loading with how many, the bar
/// (with its percentage when there's a count), the last files read, and the
/// time taken with Cancel. `elapsed` is `None` once loading is done (no
/// Cancel then).
fn paint(
    ui: &mut egui::Ui,
    rect: Rect,
    name: &str,
    status: &Status,
    elapsed: Option<f32>,
    mark: Option<&egui::TextureHandle>,
) -> bool {
    let palette = theme::palette(ui.ctx());
    // Every frame, whatever the load is doing: progress reports come at the
    // pace files are read, and that uneven pace flickers on variable
    // refresh rate displays.
    ui.ctx().request_repaint();
    // Covers what's under it, so nothing there can be clicked.
    ui.allocate_rect(rect, Sense::click());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0, palette.app_bg);

    let width = (rect.width() - 64.0).clamp(240.0, 420.0);
    let left = rect.center().x - width * 0.5;
    let mut y = rect.center().y - 90.0;
    if let Some(mark) = mark {
        let side = Rect::from_min_size(egui::pos2(left, y - 56.0), vec2(40.0, 40.0));
        if ui.ctx().global_style().animation_time > 0.0 {
            let time = ui.input(|input| input.time) as f32;
            let position = theme::bezier(theme::curve::SWEEP, (time / SWEEP_SECONDS).fract());
            widgets::paint_mark_sweep(
                &painter,
                side,
                mark,
                (palette.text_muted, palette.text_strong),
                position,
            );
        } else {
            widgets::paint_mark(&painter, side, mark, palette.text_secondary);
        }
    }
    let opening = egui::WidgetText::from(widgets::micro("Opening", palette.text_weak)).into_galley(
        ui,
        None,
        f32::INFINITY,
        egui::TextStyle::Body,
    );
    painter.galley(egui::pos2(left, y), opening, palette.text_weak);
    y += 18.0;
    let title = widgets::elided(
        ui,
        name.to_owned(),
        FontId::new(type_scale::DISPLAY, theme::display_family()),
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

    let bar = Rect::from_min_size(egui::pos2(left, y), vec2(width, 4.0));
    painter.rect_filled(bar, 2, palette.inset);
    match fill(status.count) {
        Some(fill) => {
            if fill > 0.0 {
                let filled = Rect::from_min_size(bar.min, vec2(width * fill, bar.height()));
                painter.rect_filled(filled, 2, palette.accent);
            }
            painter.text(
                egui::pos2(left + width, y + 12.0),
                Align2::RIGHT_TOP,
                format!("{}%", (fill * 100.0).round() as u32),
                FontId::monospace(11.0),
                palette.text_muted,
            );
        }
        None => {
            // Working, with no count to show: a short piece slides along,
            // or sits in the middle with Reduce motion on.
            let piece = width * 0.25;
            let moving = ui.ctx().global_style().animation_time > 0.0;
            let at = if moving {
                let time = ui.input(|input| input.time) as f32;
                (time * 0.8).fract() * (width + piece) - piece
            } else {
                (width - piece) / 2.0
            };
            let start = (left + at).max(left);
            let end = (left + at + piece).min(left + width);
            if end > start {
                let slider = Rect::from_x_y_ranges(start..=end, bar.y_range());
                painter.rect_filled(slider, 2, palette.accent);
            }
        }
    }
    y += 12.0;
    // Why it takes a while, when it does: two lines at most, clear of the
    // percentage.
    if let Some(note) = &status.note {
        y += 6.0;
        let note = widgets::elided(
            ui,
            note.clone(),
            FontId::proportional(type_scale::CAPTION),
            palette.text_secondary,
            width - 48.0,
            2,
        );
        let height = note.size().y;
        painter.galley(egui::pos2(left, y), note, palette.text_secondary);
        y += height + 8.0;
    }

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
    fn frames_settle_once_several_in_a_row_are_quick() {
        let start = Instant::now();
        // Each frame's egui number and seconds since the first frame.
        let settles = |frames: &[(u64, f32)]| {
            let mut settling = Settling::default();
            frames
                .iter()
                .map(|&(number, seconds)| {
                    settling.frame(number, start + std::time::Duration::from_secs_f32(seconds))
                })
                .collect::<Vec<bool>>()
        };
        let quick = SETTLED_FRAMES as u64;
        // Quick frames, then a two-second stall: the count starts again
        // after it, and a frame laid out twice (the same number) counts
        // once.
        let mut frames: Vec<(u64, f32)> = (0..3).map(|n| (n, n as f32 * 0.016)).collect();
        frames.push((3, 2.04));
        frames.extend((1..quick).map(|n| (3 + n, 2.04 + n as f32 * 0.016)));
        let last = frames.last().unwrap().1;
        frames.push((3 + quick - 1, last + 0.001));
        frames.push((3 + quick, last + 0.016));
        let settled = settles(&frames);
        assert!(settled[..settled.len() - 1].iter().all(|&s| !s));
        assert!(settled[settled.len() - 1]);
        // A computer that never draws quicker than half a second settles
        // on frames near its quickest.
        let mut slow: Vec<(u64, f32)> = (0..quick).map(|n| (n, n as f32 * 0.5)).collect();
        slow.push((quick, (quick - 1) as f32 * 0.5 + 0.6));
        let settled = settles(&slow);
        assert!(!settled[settled.len() - 2]);
        assert!(settled[settled.len() - 1]);
    }

    #[test]
    fn the_bar_fills_only_by_what_is_counted() {
        assert_eq!(fill(None), None);
        assert_eq!(fill(Some((0, 0))), None);
        assert_eq!(fill(Some((0, 4))), Some(0.0));
        assert_eq!(fill(Some((3, 4))), Some(0.75));
        // Steps with nothing to count have no made-up fill.
        let (_, count) = describe(LoadStep::World);
        assert_eq!(fill(count), None);
    }

    #[test]
    fn reading_the_files_names_each_file_and_counts_them() {
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
        for step in steps {
            let (doing, count) = describe(step);
            assert!(!doing.is_empty(), "{step:?}");
            if let Some((done, total)) = count {
                assert!(done < total, "{step:?}");
            }
        }
        assert_eq!(
            describe(steps[3]),
            ("Reading Main.server.lua".to_owned(), Some((0, 2)))
        );
    }

    #[test]
    fn the_loading_screen_draws() {
        let status = Status {
            doing: "Loading model assets/models/castle_gate_with_a_long_name.glb".to_owned(),
            count: Some((6, 24)),
            recent: (1..=RECENT)
                .map(|n| format!("assets/textures/stone_{n}.png"))
                .collect(),
            note: Some(IMPORT_NOTE.to_owned()),
        };
        let mut draw = |ui: &mut egui::Ui| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ui, |ui| {
                    let brand = crate::start::Brand::load(ui.ctx());
                    paint(
                        ui,
                        ui.max_rect(),
                        "Castle Siege",
                        &status,
                        Some(12.4),
                        Some(brand.mark()),
                    );
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
                let _ = crate::theme::run(&ctx, Default::default(), |ui| draw(ui));
            }
        }
    }
}
