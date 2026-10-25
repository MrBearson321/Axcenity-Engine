//! The Import Model dialog, shown when a model file holds more than one
//! object, has texture maps or is animated. Its choices are split into
//! steps (Objects, Bring in as, Texture maps, Animation; only the ones the
//! file needs), with a preview of the model and a list of exactly what
//! importing makes beside them. Import works from any step; Back and Next
//! only move between them.
//!
//! The ticking rules and what importing makes are plain methods, so
//! they're easy to test; the app turns the answer into Instances.

use std::ops::Range;
use std::path::PathBuf;

use axcenity_assets::{Model, ModelObject, Take};
use axcenity_core::ClassName;
use eframe::egui;
use eframe::egui_wgpu::RenderState;

use crate::anim_import::AnimationImport;
use crate::surface::MaterialMaps;
use crate::theme::{self, icons};
use crate::widgets;

mod outcome;
mod preview;
mod steps;
#[cfg(test)]
mod tests;

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

/// A part of the dialog, shown one at a time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Which objects to import.
    Objects,
    /// How they arrive ([`Placement`]).
    Placement,
    /// Each material's texture maps.
    Materials,
    /// The skeleton, takes and clips to make.
    Animation,
}

impl Step {
    pub fn title(self) -> &'static str {
        match self {
            Step::Objects => "Objects",
            Step::Placement => "Bring in as",
            Step::Materials => "Texture maps",
            Step::Animation => "Animation",
        }
    }
}

/// The widest the dialog gets, in points.
const DIALOG_WIDTH: f32 = 960.0;
/// The least room left between the dialog and the window's edges.
const WINDOW_GAP: f32 = 16.0;
/// Room kept under the body for the separator and the buttons.
const FOOTER_HEIGHT: f32 = 52.0;
/// The tallest the body (preview and step) gets, so it doesn't fill a big
/// window with empty space.
const BODY_HEIGHT: f32 = 560.0;
/// From this width the preview sits beside the step; narrower, above it.
const SIDE_BY_SIDE_FROM: f32 = 700.0;
/// The preview column's width when it's beside the step.
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
    /// One per object. Only the ticks of objects with meshes matter; a
    /// group shows its contents' ticks.
    ticked: Vec<bool>,
    pub placement: Placement,
    /// Only objects whose names contain this are shown (ignoring case).
    filter: String,
    /// False for a file with one object: then there's nothing to choose,
    /// and the Objects and Bring in as steps aren't shown.
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
    /// The step shown (the first step if it isn't one of [`steps`](Self::steps)).
    pub step: Step,
    /// The model drawn beside the steps; `None` without one (an
    /// animation-only file, or a test).
    preview: Option<ImportPreview>,
    /// The clip row the preview plays.
    previewing: usize,
    /// Where the button row and the Import button were last drawn, so
    /// tests can check they're in the window and click Import.
    buttons: Option<egui::Rect>,
    import_button: Option<egui::Rect>,
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
            filter: String::new(),
            pick_objects: meshes > 1,
            found: materials.clone(),
            materials,
            animation: None,
            animation_only: false,
            step: Step::Objects,
            preview: None,
            previewing: 0,
            buttons: None,
            import_button: None,
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

    /// The steps this file needs, in order.
    pub fn steps(&self) -> Vec<Step> {
        let mut steps = Vec::new();
        if self.pick_objects {
            steps.extend([Step::Objects, Step::Placement]);
        }
        if !self.animation_only && !self.materials.is_empty() {
            steps.push(Step::Materials);
        }
        if self.animation.is_some() {
            steps.push(Step::Animation);
        }
        steps
    }

    /// The step shown now.
    pub fn current(&self) -> Step {
        let steps = self.steps();
        if steps.contains(&self.step) {
            self.step
        } else {
            steps.first().copied().unwrap_or(Step::Materials)
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
        if !self.animation_only && self.pick_objects && self.chosen().is_empty() {
            return Some("Tick at least one object to import.".to_owned());
        }
        self.animation.as_ref().and_then(AnimationImport::problem)
    }

    /// The step a problem is on, to mark it in the step bar.
    fn problem_step(&self) -> Option<Step> {
        if !self.animation_only && self.pick_objects && self.chosen().is_empty() {
            Some(Step::Objects)
        } else if self
            .animation
            .as_ref()
            .is_some_and(|animation| animation.problem().is_some())
        {
            Some(Step::Animation)
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
    fn summary(&self) -> String {
        let mut parts = Vec::new();
        if !self.animation_only {
            let meshes = self.objects.iter().filter(|o| o.has_mesh()).count();
            let triangles: usize = self.objects.iter().map(|o| o.triangles).sum();
            parts.push(plural(meshes.max(1), "object"));
            if triangles > 0 {
                parts.push(format!("{} triangles", thousands(triangles)));
            }
            parts.push(plural(self.materials.len(), "material"));
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
    /// most what's left, each side scrolls on its own, and the buttons stay
    /// pinned under it, so they can be reached however small the window is.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        render_state: Option<&RenderState>,
    ) -> Option<Answer> {
        let mut answer = None;
        let frame = egui::Frame::popup(&ctx.style());
        // The frame's margins and outline, around the contents.
        let edges = frame.total_margin().sum();
        let room = ctx.screen_rect().size() - egui::Vec2::splat(2.0 * WINDOW_GAP) - edges;
        let modal = egui::Modal::new(egui::Id::new("import model"))
            .frame(frame)
            .show(ctx, |ui| {
                let width = room.x.clamp(260.0, DIALOG_WIDTH);
                ui.set_width(width);
                self.header(ui);
                let body_height =
                    (room.y - ui.min_rect().height() - FOOTER_HEIGHT).clamp(80.0, BODY_HEIGHT);
                if width >= SIDE_BY_SIDE_FROM {
                    self.side_by_side(ui, render_state, body_height);
                } else {
                    self.stacked(ui, render_state, body_height);
                }
                ui.separator();
                self.buttons = Some(self.footer(ui, &mut answer));
            });
        // Escape or a click outside the box means Cancel.
        if answer.is_none() && modal.should_close() {
            answer = Some(Answer::Cancel);
        }
        answer
    }

    /// The title, what the file holds, and the step bar.
    fn header(&mut self, ui: &mut egui::Ui) {
        let palette = theme::palette(ui.ctx());
        let file_name = self
            .file
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        ui.heading(format!("Import {file_name}"));
        ui.label(
            egui::RichText::new(self.summary())
                .size(theme::type_scale::SMALL)
                .color(palette.text_weak),
        );
        let steps = self.steps();
        if steps.len() > 1 {
            ui.add_space(8.0);
            let current = self.current();
            let problem = self.problem_step();
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                for (index, &step) in steps.iter().enumerate() {
                    if index > 0 {
                        ui.label(egui::RichText::new(icons::CARET_RIGHT).color(palette.text_muted));
                    }
                    if step_pill(ui, index + 1, step, step == current, problem == Some(step))
                        .clicked()
                    {
                        self.step = step;
                    }
                }
            });
        }
        ui.add_space(6.0);
        ui.separator();
    }

    /// The preview and what you'll get on the left, the step on the right.
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
                        .id_salt("import body")
                        .max_height(body_height)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_max_width(width - 12.0);
                            self.step_body(ui);
                        });
                },
            );
        });
    }

    /// In a narrow window: the preview, the step and what you'll get,
    /// scrolling together.
    fn stacked(&mut self, ui: &mut egui::Ui, render_state: Option<&RenderState>, body_height: f32) {
        egui::ScrollArea::vertical()
            .id_salt("import body")
            .max_height(body_height)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                self.preview_pane(ui, render_state, 180.0);
                ui.separator();
                self.step_body(ui);
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

    /// The step shown: its title, what it's for, and its controls.
    fn step_body(&mut self, ui: &mut egui::Ui) {
        let step = self.current();
        let palette = theme::palette(ui.ctx());
        ui.label(
            egui::RichText::new(step.title())
                .size(theme::type_scale::HEADING)
                .family(theme::medium_family())
                .color(palette.text_strong),
        );
        ui.add_space(2.0);
        match step {
            Step::Objects => steps::objects(self, ui),
            Step::Placement => steps::placement(self, ui),
            Step::Materials => steps::materials(self, ui),
            Step::Animation => {
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

    /// Cancel at the left; Back, Next and Import at the right. Returns
    /// where the row is.
    fn footer(&mut self, ui: &mut egui::Ui, answer: &mut Option<Answer>) -> egui::Rect {
        let palette = theme::palette(ui.ctx());
        let steps = self.steps();
        let at = steps.iter().position(|&step| step == self.current());
        let back = at.and_then(|at| at.checked_sub(1)).map(|at| steps[at]);
        let next = at.and_then(|at| steps.get(at + 1)).copied();
        let problem = self.problem();
        ui.horizontal(|ui| {
            if ui
                .add(widgets::button(
                    widgets::Role::Ghost,
                    None,
                    "Cancel",
                    &palette,
                ))
                .clicked()
            {
                *answer = Some(Answer::Cancel);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Added right to left: Import is at the far right.
                let import = ui
                    .add_enabled(
                        problem.is_none(),
                        widgets::button(
                            widgets::Role::Primary,
                            Some(icons::DOWNLOAD_SIMPLE),
                            "Import",
                            &palette,
                        ),
                    )
                    .on_hover_text("Import now, with the choices as they are in every step.")
                    .on_disabled_hover_text(problem.clone().unwrap_or_default());
                self.import_button = Some(import.rect);
                if import.clicked() {
                    *answer = Some(Answer::Import);
                }
                if let Some(next) = next
                    && ui
                        .add(widgets::button(
                            widgets::Role::Secondary,
                            Some(icons::ARROW_RIGHT),
                            &format!("Next: {}", next.title()),
                            &palette,
                        ))
                        .clicked()
                {
                    self.step = next;
                }
                if let Some(back) = back
                    && ui
                        .add(widgets::button(
                            widgets::Role::Ghost,
                            Some(icons::ARROW_LEFT),
                            "Back",
                            &palette,
                        ))
                        .clicked()
                {
                    self.step = back;
                }
                if let Some(problem) = &problem {
                    ui.label(egui::RichText::new(problem).color(palette.warning));
                }
            });
        })
        .response
        .rect
    }
}

/// One step in the step bar: its number in a circle and its title,
/// filled while it's shown, the number warning-coloured while it has a
/// problem.
fn step_pill(
    ui: &mut egui::Ui,
    number: usize,
    step: Step,
    selected: bool,
    problem: bool,
) -> egui::Response {
    let palette = theme::palette(ui.ctx());
    let font = egui::FontId::new(theme::type_scale::BODY, theme::medium_family());
    let galley =
        ui.painter()
            .layout_no_wrap(step.title().to_owned(), font, egui::Color32::PLACEHOLDER);
    let circle = 18.0;
    let size = egui::vec2(8.0 + circle + 6.0 + galley.size().x + 12.0, 28.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, step.title())
    });
    widgets::paint_hover(ui, rect, &response, false, 14.0);
    if selected {
        ui.painter().rect_filled(rect, 14.0, palette.control_active);
    }
    let center = egui::pos2(rect.min.x + 8.0 + circle / 2.0, rect.center().y);
    let (fill, number_color) = if problem {
        (palette.warning, palette.on_accent)
    } else if selected {
        (palette.accent_solid, palette.on_accent)
    } else {
        (palette.inset, palette.text_secondary)
    };
    ui.painter().circle(
        center,
        circle / 2.0,
        fill,
        egui::Stroke::new(1.0_f32, palette.border),
    );
    ui.painter().text(
        center,
        egui::Align2::CENTER_CENTER,
        if problem {
            "!".to_owned()
        } else {
            number.to_string()
        },
        egui::FontId::new(theme::type_scale::CAPTION, theme::medium_family()),
        number_color,
    );
    let color = if selected || response.hovered() {
        palette.text_strong
    } else {
        palette.text_secondary
    };
    ui.painter().galley(
        egui::pos2(
            center.x + circle / 2.0 + 6.0,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        color,
    );
    response
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
