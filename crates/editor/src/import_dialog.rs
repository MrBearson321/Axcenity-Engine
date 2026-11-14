//! The Import dialog for a model, shown when a model file holds more than
//! one object, has texture maps or is animated. Its choices are on tabs
//! (Objects, Materials, Animation; only the ones the file needs), each
//! scrolling on its own, with a preview of the model and a list of exactly
//! what importing makes beside them. Above Import and Cancel, which stay
//! pinned at the bottom, a sentence says what Import creates and where.
//!
//! An asset package (`.axpkg`) opens the same dialog with one tab, Package
//! (`package.rs`), and no preview.
//!
//! The ticking rules and what importing makes are plain methods, so
//! they're easy to test; the app turns the answer into Instances.

use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};

use axcenity_assets::{Model, ModelObject, Take};
use axcenity_core::ClassName;
use eframe::egui;
use eframe::egui_wgpu::RenderState;

use crate::anim_import::AnimationImport;
use crate::dialog;
use crate::surface::MaterialMaps;
use crate::theme::{self, icons};
use crate::widgets;

pub(crate) mod outcome;
mod package;
mod preview;
mod tabs;
#[cfg(test)]
mod tests;

pub use package::PackageImport;
use preview::ImportPreview;

/// How the ticked objects arrive in the world.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Placement {
    /// A Model named after the file, holding a MeshPart per object where
    /// it is in the file. Groups in the file become Models inside it.
    #[default]
    Grouped,
    /// A MeshPart per object, each at the insert point.
    Separate,
    /// One MeshPart drawing all of them.
    Merged,
}

/// A tab of the dialog, shown one at a time.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum Tab {
    /// Which objects to import, and how they arrive ([`Placement`]).
    Objects,
    /// Each material's texture maps.
    Materials,
    /// The skeleton, takes and clips to make.
    Animation,
    /// What's in an asset package, and what to do with files the project
    /// already has.
    Package,
}

impl Tab {
    pub fn title(self) -> &'static str {
        match self {
            Tab::Objects => "Objects",
            Tab::Materials => "Materials",
            Tab::Animation => "Animation",
            Tab::Package => "Package",
        }
    }
}

/// The widest the dialog gets, in points.
const DIALOG_WIDTH: f32 = 960.0;
/// The least room left between the dialog and the window's edges.
const WINDOW_GAP: f32 = 16.0;
/// Room kept under the body for the separator and the buttons (the
/// summary above them is measured).
const FOOTER_HEIGHT: f32 = 60.0;
/// The line saying what stops Import, when something does.
const PROBLEM_HEIGHT: f32 = 22.0;
/// The tallest the body (preview and tab) gets, so it doesn't fill a big
/// window with empty space.
const BODY_HEIGHT: f32 = 560.0;
/// From this width the preview sits beside the tab; narrower, above it.
const SIDE_BY_SIDE_FROM: f32 = 700.0;
/// The preview column's width when it's beside the tab.
const PREVIEW_WIDTH: f32 = 320.0;

/// What the dialog was answered with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    Import,
    Cancel,
}

pub struct ImportDialog {
    /// The model file being imported, outside the project.
    pub file: PathBuf,
    pub objects: Vec<ModelObject>,
    /// How many blend shapes (morph targets) the model has, once it's
    /// loaded for the preview.
    blend_shapes: usize,
    /// One per object. Only the ticks of objects with meshes matter; a
    /// group shows its contents' ticks.
    ticked: Vec<bool>,
    pub placement: Placement,
    /// With [`Placement::Grouped`], weld the Model's parts to its largest
    /// one, so it moves as one body in the game.
    pub weld: bool,
    /// Only objects whose names contain this are shown (ignoring case).
    filter: String,
    /// False for a file with one object: then there's nothing to choose,
    /// and the Objects tab isn't shown.
    pub pick_objects: bool,
    /// The maps of each of the file's materials, as found, then as changed.
    pub materials: Vec<MaterialMaps>,
    /// The maps as found, to tell when they've been changed.
    found: Vec<MaterialMaps>,
    /// The file's skeleton and takes, and the clips to make, if it's animated.
    pub animation: Option<AnimationImport>,
    /// True for a file with only animation in it (nothing to draw): only
    /// its clips are imported.
    pub animation_only: bool,
    /// The asset package being imported, for a `.axpkg` file (then there's
    /// no model).
    pub package: Option<PackageImport>,
    /// The file `file` was converted from, for a format the engine doesn't
    /// read itself (a USD file made into a `.glb`, ADR-0027): named in the
    /// title and Output, and noted in the copy's sidecar.
    pub source: Option<PathBuf>,
    /// The tab shown (the first tab if it isn't one of [`tabs`](Self::tabs)).
    pub tab: Tab,
    /// The model drawn beside the tabs; `None` without one (an
    /// animation-only file, or a test).
    preview: Option<ImportPreview>,
    /// The clip row the preview plays.
    previewing: usize,
    /// Where the button row and the Import button were last drawn, so
    /// tests can check they're in the window and click Import.
    buttons: Option<egui::Rect>,
    import_button: Option<egui::Rect>,
    /// Where each tab was last drawn in the tab bar, for tests to click.
    tab_rects: HashMap<Tab, egui::Rect>,
}

impl ImportDialog {
    /// The dialog for `file`'s `objects`, with all of them ticked, and
    /// its `materials`. With one object, it comes in as one MeshPart.
    pub fn new(file: PathBuf, objects: Vec<ModelObject>, materials: Vec<MaterialMaps>) -> Self {
        let meshes = objects.iter().filter(|object| object.has_mesh()).count();
        Self {
            file,
            ticked: vec![true; objects.len()],
            objects,
            placement: if meshes > 1 {
                Placement::default()
            } else {
                Placement::Merged
            },
            weld: false,
            filter: String::new(),
            pick_objects: meshes > 1,
            found: materials.clone(),
            materials,
            animation: None,
            animation_only: false,
            package: None,
            source: None,
            tab: Tab::Objects,
            preview: None,
            blend_shapes: 0,
            previewing: 0,
            buttons: None,
            import_button: None,
            tab_rects: HashMap::new(),
        }
    }

    /// The dialog for a file with only animation in it.
    pub fn animation_only(file: PathBuf, animation: AnimationImport) -> Self {
        Self {
            animation: Some(animation),
            animation_only: true,
            ..Self::new(file, Vec::new(), Vec::new())
        }
    }

    /// The dialog for an asset package.
    pub fn package(file: PathBuf, package: PackageImport) -> Self {
        Self {
            package: Some(package),
            ..Self::new(file, Vec::new(), Vec::new())
        }
    }

    /// The file the person picked: the one converted, if it was, else `file`.
    pub fn shown_file(&self) -> &Path {
        self.source.as_deref().unwrap_or(&self.file)
    }

    /// Adds the file's animation. An animated file comes in as one
    /// MeshPart by default, so one Animator moves all of it.
    pub fn with_animation(mut self, animation: Option<AnimationImport>) -> Self {
        if animation.is_some() {
            self.placement = Placement::Merged;
        }
        self.animation = animation;
        self
    }

    /// Adds a preview of the file's `model` (as loaded from it), playing
    /// clips made from its `takes`.
    pub fn with_preview(mut self, model: Model, takes: Vec<Take>) -> Self {
        self.blend_shapes = model.morph_targets.len();
        self.preview = Some(ImportPreview::new(&self.file, model, takes));
        self
    }

    /// True if there's anything to ask: objects to choose from, texture
    /// maps to look over, or clips to make.
    pub fn worth_asking(&self) -> bool {
        self.pick_objects
            || self.animation.is_some()
            || self
                .materials
                .iter()
                .any(|maps| maps.files.iter().any(Option::is_some))
    }

    /// The tabs this file needs, in order.
    pub fn tabs(&self) -> Vec<Tab> {
        if self.package.is_some() {
            return vec![Tab::Package];
        }
        let mut tabs = Vec::new();
        if self.pick_objects {
            tabs.push(Tab::Objects);
        }
        if !self.animation_only && !self.materials.is_empty() {
            tabs.push(Tab::Materials);
        }
        if self.animation.is_some() {
            tabs.push(Tab::Animation);
        }
        tabs
    }

    /// The tab shown now.
    pub fn current(&self) -> Tab {
        let tabs = self.tabs();
        if tabs.contains(&self.tab) {
            self.tab
        } else {
            tabs.first().copied().unwrap_or(Tab::Materials)
        }
    }

    /// The objects inside object `index`, and it: the ones after it that
    /// are deeper.
    pub fn subtree(&self, index: usize) -> Range<usize> {
        let depth = self.objects[index].depth;
        let end = self.objects[index + 1..]
            .iter()
            .position(|object| object.depth <= depth)
            .map_or(self.objects.len(), |after| index + 1 + after);
        index..end
    }

    /// The object `index` is directly inside, if any.
    pub fn parent(&self, index: usize) -> Option<usize> {
        let depth = self.objects[index].depth;
        (0..index).rev().find(|&i| self.objects[i].depth < depth)
    }

    /// Ticks or unticks object `index` and everything inside it.
    pub fn set(&mut self, index: usize, on: bool) {
        for i in self.subtree(index) {
            self.ticked[i] = on;
        }
    }

    /// Whether object `index`'s box shows ticked, and whether it's only
    /// partly (some of what's inside it is ticked, some isn't).
    pub fn state(&self, index: usize) -> (bool, bool) {
        let meshes: Vec<bool> = self
            .subtree(index)
            .filter(|&i| self.objects[i].has_mesh())
            .map(|i| self.ticked[i])
            .collect();
        let all = meshes.iter().all(|&ticked| ticked);
        let any = meshes.iter().any(|&ticked| ticked);
        (all, any && !all)
    }

    /// The ticked objects with meshes, in file order: each becomes a MeshPart.
    pub fn chosen(&self) -> Vec<usize> {
        (0..self.objects.len())
            .filter(|&i| self.ticked[i] && self.objects[i].has_mesh())
            .collect()
    }

    /// True if every object with a mesh is ticked.
    pub fn all_chosen(&self) -> bool {
        self.objects
            .iter()
            .zip(&self.ticked)
            .all(|(object, &ticked)| ticked || !object.has_mesh())
    }

    /// The rows the filter lets through.
    fn shown(&self) -> Vec<usize> {
        let filter = self.filter.trim().to_lowercase();
        (0..self.objects.len())
            .filter(|&i| filter.is_empty() || self.objects[i].name.to_lowercase().contains(&filter))
            .collect()
    }

    /// Why it can't be imported as it is, if it can't.
    pub fn problem(&self) -> Option<String> {
        if let Some(package) = &self.package {
            return package.problem();
        }
        if !self.animation_only && self.pick_objects && self.chosen().is_empty() {
            return Some("Tick at least one object to import.".to_owned());
        }
        self.animation.as_ref().and_then(AnimationImport::problem)
    }

    /// The tab a problem is on, to mark it in the tab bar.
    fn problem_tab(&self) -> Option<Tab> {
        if !self.animation_only && self.pick_objects && self.chosen().is_empty() {
            Some(Tab::Objects)
        } else if self
            .animation
            .as_ref()
            .is_some_and(|animation| animation.problem().is_some())
        {
            Some(Tab::Animation)
        } else {
            None
        }
    }

    /// The name the new MeshPart or Model gets: the file's.
    fn model_name(&self) -> String {
        self.file
            .file_stem()
            .map(|stem| stem.to_string_lossy().trim().to_owned())
            .filter(|stem| !stem.is_empty())
            .unwrap_or_else(|| ClassName::MeshPart.as_str().to_owned())
    }

    /// How many clips an Animator will play, if one is added.
    fn animator_clips(&self) -> Option<usize> {
        self.animation
            .as_ref()
            .filter(|animation| {
                animation.add_animator && !animation.bones.is_empty() && !animation.clips.is_empty()
            })
            .map(|animation| animation.clips.len())
    }

    /// What the file holds, in a few words, for under the title.
    fn holds(&self) -> String {
        if let Some(package) = &self.package {
            return package.holds();
        }
        let mut parts = Vec::new();
        if !self.animation_only {
            let meshes = self.objects.iter().filter(|o| o.has_mesh()).count();
            let triangles: usize = self.objects.iter().map(|o| o.triangles).sum();
            parts.push(plural(meshes.max(1), "object"));
            if triangles > 0 {
                parts.push(format!("{} triangles", thousands(triangles)));
            }
            parts.push(plural(self.materials.len(), "material"));
            if self.blend_shapes > 0 {
                parts.push(plural(self.blend_shapes, "blend shape"));
            }
        }
        if let Some(animation) = &self.animation {
            if !animation.bones.is_empty() {
                parts.push(format!(
                    "a skeleton of {}",
                    plural(animation.bones.len(), "bone")
                ));
            }
            if !animation.takes.is_empty() {
                parts.push(plural(animation.takes.len(), "take"));
            }
        }
        parts.join(" · ")
    }

    /// Shows the dialog. Returns the answer once there is one.
    ///
    /// The dialog fits the window: the body has a fixed height that's at
    /// most what's left, each side scrolls on its own (each tab keeping its
    /// own place), and the summary and buttons stay pinned under it, so
    /// they can be reached however small the window is.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        render_state: Option<&RenderState>,
    ) -> Option<Answer> {
        let mut answer = None;
        // The frame's margins and outline, around the contents.
        let edges = dialog::frame(ctx).total_margin().sum();
        let room = ctx.content_rect().size() - egui::Vec2::splat(2.0 * WINDOW_GAP) - edges;
        let width = room.x.clamp(260.0, DIALOG_WIDTH);
        // Whether to keep room for the problem line (the footer asks again,
        // after the tabs' edits).
        let problem = self.problem().is_some();
        let ((), dismissed) = dialog::modal(ctx, "import model", width, |ui| {
            self.header(ui);
            let summary = self.summary_galley(ui, width);
            let problem_height = if problem { PROBLEM_HEIGHT } else { 0.0 };
            let body_height = (room.y
                - ui.min_rect().height()
                - FOOTER_HEIGHT
                - problem_height
                - summary.size().y)
                .clamp(80.0, BODY_HEIGHT);
            if self.package.is_some() {
                // Nothing to preview: the tab has the whole width.
                egui::ScrollArea::vertical()
                    .id_salt("import package")
                    .max_height(body_height)
                    .auto_shrink([false, true])
                    .show(ui, |ui| self.tab_body(ui));
            } else if width >= SIDE_BY_SIDE_FROM {
                self.side_by_side(ui, render_state, body_height);
            } else {
                self.stacked(ui, render_state, body_height);
            }
            ui.separator();
            ui.label(summary);
            self.footer(ui, &mut answer);
        });
        // Escape or a click outside the box means Cancel.
        if answer.is_none() && dismissed {
            answer = Some(Answer::Cancel);
        }
        answer
    }

    /// The title, what the file holds, and the tabs.
    fn header(&mut self, ui: &mut egui::Ui) {
        let file_name = self
            .shown_file()
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        dialog::header(
            ui,
            icons::DOWNLOAD_SIMPLE,
            dialog::Tone::Question,
            &format!("Import {file_name}"),
            Some(&self.holds()),
        );
        let tabs = self.tabs();
        if tabs.len() > 1 {
            ui.add_space(8.0);
            let problem = self.problem_tab();
            // A tab with a problem shows a warning sign.
            let options: Vec<(Tab, Option<&str>, &str)> = tabs
                .iter()
                .map(|&tab| {
                    let icon = (problem == Some(tab)).then_some(icons::WARNING);
                    (tab, icon, tab.title())
                })
                .collect();
            let mut tab = self.current();
            let (_, rects) = widgets::segmented_at(ui, &mut tab, &options);
            self.tab = tab;
            self.tab_rects = tabs.into_iter().zip(rects).collect();
        }
        ui.add_space(6.0);
        ui.separator();
    }

    /// The summary of what Import creates, laid out to `width`, with the
    /// paths in the code font.
    fn summary_galley(&self, ui: &egui::Ui, width: f32) -> std::sync::Arc<egui::Galley> {
        let palette = theme::palette(ui.ctx());
        let mut job = egui::text::LayoutJob {
            wrap: egui::text::TextWrapping::wrap_at_width(width),
            ..Default::default()
        };
        // The sentence puts paths between backticks.
        let sentence = match &self.package {
            Some(package) => package.sentence(),
            None => self.creates().sentence(),
        };
        for (i, piece) in sentence.split('`').enumerate() {
            let font = if i % 2 == 1 {
                egui::FontId::monospace(theme::type_scale::SMALL)
            } else {
                egui::FontId::proportional(theme::type_scale::BODY)
            };
            job.append(
                piece,
                0.0,
                egui::TextFormat::simple(font, palette.text_secondary),
            );
        }
        ui.fonts_mut(|fonts| fonts.layout_job(job))
    }

    /// The preview and what you'll get on the left, the tab on the right.
    fn side_by_side(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&RenderState>,
        body_height: f32,
    ) {
        ui.horizontal_top(|ui| {
            let left = PREVIEW_WIDTH.min(ui.available_width() * 0.42);
            let layout = egui::Layout::top_down(egui::Align::Min);
            ui.allocate_ui_with_layout(egui::vec2(left, body_height), layout, |ui| {
                ui.set_width(left);
                let top = ui.min_rect().top();
                let preview_height = (body_height * 0.55).clamp(120.0, 300.0);
                self.preview_pane(ui, render_state, preview_height);
                ui.add_space(6.0);
                // What's left of the body, worked out rather than taken
                // from `available_height`, which in a modal grows with
                // last frame's size.
                let used = ui.min_rect().bottom() - top;
                egui::ScrollArea::vertical()
                    .id_salt("import made")
                    .max_height((body_height - used).max(40.0))
                    .auto_shrink([false, true])
                    .show(ui, |ui| self.outcome(ui));
            });
            // A line as tall as the body: `separator` would take the
            // row's whole height, which grows with the modal.
            let (line, _) =
                ui.allocate_exact_size(egui::vec2(9.0, body_height), egui::Sense::hover());
            ui.painter().vline(
                line.center().x,
                line.y_range(),
                egui::Stroke::new(1.0_f32, theme::palette(ui.ctx()).border),
            );
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), body_height),
                layout,
                |ui| {
                    // Both ways, so a wide table (the clips) scrolls
                    // instead of widening the dialog past the window.
                    let width = ui.available_width();
                    egui::ScrollArea::both()
                        .id_salt(("import tab", self.current()))
                        .max_height(body_height)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_max_width(width - 12.0);
                            self.tab_body(ui);
                        });
                },
            );
        });
    }

    /// In a narrow window: the preview, the tab and what you'll get,
    /// scrolling together.
    fn stacked(&mut self, ui: &mut egui::Ui, render_state: Option<&RenderState>, body_height: f32) {
        egui::ScrollArea::vertical()
            .id_salt(("import body", self.current()))
            .max_height(body_height)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                self.preview_pane(ui, render_state, 180.0);
                ui.separator();
                self.tab_body(ui);
                ui.separator();
                self.outcome(ui);
            });
    }

    fn preview_pane(&mut self, ui: &mut egui::Ui, render_state: Option<&RenderState>, height: f32) {
        let changed = self.materials != self.found;
        match &mut self.preview {
            Some(preview) => {
                preview.show(
                    ui,
                    render_state,
                    self.animation.as_ref(),
                    self.previewing,
                    height,
                );
                if changed {
                    ui.label(
                        egui::RichText::new("Your map changes show once it's imported.")
                            .size(theme::type_scale::SMALL)
                            .color(theme::palette(ui.ctx()).text_weak),
                    );
                }
            }
            None => {
                let palette = theme::palette(ui.ctx());
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), height),
                    egui::Sense::hover(),
                );
                ui.painter().rect_filled(rect, 6.0, palette.inset);
                let text = if self.animation_only {
                    "Nothing to draw: this file only has animation."
                } else {
                    "No preview"
                };
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    text,
                    egui::FontId::proportional(theme::type_scale::SMALL),
                    palette.text_weak,
                );
            }
        }
    }

    /// The tab shown: its controls, under a title for each part.
    fn tab_body(&mut self, ui: &mut egui::Ui) {
        match self.current() {
            Tab::Objects => {
                part_title(ui, "Objects");
                tabs::objects(self, ui);
                ui.add_space(12.0);
                part_title(ui, "Bring in as");
                tabs::placement(self, ui);
            }
            Tab::Materials => {
                part_title(ui, "Texture maps");
                tabs::materials(self, ui);
            }
            Tab::Package => {
                part_title(ui, "Package");
                if let Some(package) = &mut self.package {
                    package.show(ui);
                }
            }
            Tab::Animation => {
                part_title(ui, "Animation");
                if self.animation_only {
                    ui.label(
                        "This file has animation but nothing to draw. Its takes become clips, \
                         for a model with the same skeleton.",
                    );
                    ui.add_space(4.0);
                }
                let with_animator = !self.animation_only;
                if let Some(animation) = &mut self.animation {
                    animation.show(ui, with_animator, &mut self.previewing);
                }
            }
        }
    }

    /// What stops Import, if anything, over Cancel and Import at the
    /// bottom right.
    fn footer(&mut self, ui: &mut egui::Ui, answer: &mut Option<Answer>) {
        let palette = theme::palette(ui.ctx());
        let problem = self.problem();
        let problem = problem.as_deref();
        if let Some(problem) = problem {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(theme::with_icon(icons::WARNING, problem))
                        .color(palette.warning),
                )
                .truncate(),
            );
        }
        let row = dialog::button_row(
            ui,
            &[
                dialog::Button::cancel(),
                dialog::Button::new(icons::DOWNLOAD_SIMPLE, "Import", dialog::Kind::Main)
                    .enabled(problem.is_none())
                    .tooltip(
                        problem.unwrap_or("Import now, with the choices as they are on every tab."),
                    ),
            ],
            dialog::Enter::Main,
        );
        self.import_button = Some(row.rects[1]);
        self.buttons = Some(row.rects[0].union(row.rects[1]));
        match row.picked {
            Some(0) => *answer = Some(Answer::Cancel),
            Some(_) => *answer = Some(Answer::Import),
            None => {}
        }
    }
}

/// The title of a part of a tab.
fn part_title(ui: &mut egui::Ui, title: &str) {
    ui.label(
        egui::RichText::new(title)
            .size(theme::type_scale::HEADING)
            .family(theme::medium_family())
            .color(theme::palette(ui.ctx()).text_strong),
    );
    ui.add_space(2.0);
}

/// An object's name from a model file as an Instance name.
fn object_name(name: &str) -> String {
    match name.trim() {
        "" => ClassName::MeshPart.as_str().to_owned(),
        name => name.to_owned(),
    }
}

/// "1 clip", "3 clips".
fn plural(n: usize, thing: &str) -> String {
    if n == 1 {
        format!("1 {thing}")
    } else {
        format!("{n} {thing}s")
    }
}

/// `n` with commas between thousands: 12,345.
fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut text = String::new();
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            text.push(',');
        }
        text.push(digit);
    }
    text
}
