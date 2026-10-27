//! The Learn page: short tutorials, each one task in a sample project from
//! `examples/` (the words and pictures are in `learn/tutorials.rs` and
//! `learn/pictures.rs`), and short notes on each part of the editor. The
//! same page shows on the start screen and in Help > Learn over an open
//! project, so a tutorial stays in view after its sample opens.

mod pictures;
mod tutorials;

pub(super) use tutorials::TUTORIALS;
use tutorials::{START_HERE, Tutorial};

use super::reference::doc_text;
use super::*;

/// How tall a tutorial's card is, in points.
const TUTORIAL_CARD_HEIGHT: f32 = 148.0;
/// Cards are at least this wide; a row fits as many as it can.
const LEARN_CARD_WIDTH: f32 = 260.0;
/// How far a step's words and picture are indented past its number.
const STEP_INDENT: f32 = 36.0;

/// The page's state: the open tutorial and where each sample was found.
#[derive(Default)]
pub(super) struct Learn {
    /// An index into [`TUTORIALS`].
    pub(super) open: Option<usize>,
    /// Where each sample was found, looked for once.
    samples: HashMap<&'static str, Option<PathBuf>>,
    /// Set when a tutorial opens, so it's shown from the top.
    scroll_top: bool,
}

/// What a click on the Learn page asks for.
pub(super) enum LearnAction {
    /// Open the sample project in this folder.
    OpenSample(PathBuf),
    /// Open this entry in the API Reference.
    LookUp(&'static str),
}

/// How the sample of the open tutorial can be opened from where the page is.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SampleState {
    /// Nothing stops it.
    Ready,
    /// It's the project that's open.
    AlreadyOpen,
    /// The game is playing, so nothing can be opened.
    Playing,
}

impl Learn {
    /// Draws the list of tutorials, or the open one. `sample_state` says
    /// whether the sample in a folder can be opened from where the page is.
    pub(super) fn show(
        &mut self,
        ui: &mut egui::Ui,
        sample_state: impl Fn(&Path) -> SampleState,
    ) -> Option<LearnAction> {
        let Some(tutorial) = self.open.and_then(|index| TUTORIALS.get(index)) else {
            self.open = None;
            if let Some(index) = learn_list(ui) {
                self.open = Some(index);
                self.scroll_top = true;
            }
            return None;
        };
        let index = self.open?;
        let folder = self
            .samples
            .entry(tutorial.sample)
            .or_insert_with(|| find_sample(tutorial.sample))
            .clone();
        let state = folder.as_deref().map(sample_state);
        let scroll_top = std::mem::take(&mut self.scroll_top);
        let (nav, action) = tutorial_page(ui, index, tutorial, folder, state, scroll_top);
        if let Some(open) = nav {
            self.open = open;
            self.scroll_top = true;
        }
        action
    }
}

/// The sample project `examples/<name>`. It's looked for in the folder the
/// editor was started from, then in each folder above the editor's own
/// program (a build from source keeps it in `target/`, under the engine's
/// folder), then in the source tree a debug build came from (when the
/// build folder is elsewhere, as with `CARGO_TARGET_DIR`).
pub(super) fn find_sample(name: &str) -> Option<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(folder) = std::env::current_dir() {
        roots.push(folder);
    }
    if let Ok(program) = std::env::current_exe() {
        roots.extend(program.ancestors().skip(1).map(Path::to_path_buf));
    }
    roots.extend(crate::rebuild::EngineSource::find().map(|source| source.workspace));
    roots
        .into_iter()
        .map(|root| root.join("examples").join(name))
        .find(|folder| folder.join(axcenity_project::MANIFEST_FILE).is_file())
}

/// The heading, the tutorials as cards, and the notes on each part of the
/// editor. Returns the tutorial clicked.
fn learn_list(ui: &mut egui::Ui) -> Option<usize> {
    let palette = theme::palette(ui.ctx());
    ui.label(
        RichText::new("Learn")
            .family(theme::heading_family())
            .size(type_scale::DISPLAY)
            .color(palette.text_strong),
    );
    ui.label(
        RichText::new(
            "Short tutorials, one task each. Every one follows a sample project you can \
             open with one click, and stays open in Help > Learn while you work.",
        )
        .color(palette.text_secondary),
    );
    let mut clicked = None;
    for (heading, range) in [
        ("Start here", 0..START_HERE),
        ("More guides", START_HERE..TUTORIALS.len()),
    ] {
        ui.add_space(24.0);
        widgets::section_label(ui, heading);
        ui.add_space(6.0);
        let card_width = card_width(ui);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(CARD_SPACING, CARD_SPACING);
            for index in range {
                if tutorial_card(ui, index, &TUTORIALS[index], card_width).clicked() {
                    clicked = Some(index);
                }
            }
        });
    }
    ui.add_space(28.0);
    widgets::section_label(ui, "The editor at a glance");
    ui.add_space(6.0);
    topic_cards(ui, &palette);
    clicked
}

/// The width of each card in a row that fits as many as it can.
fn card_width(ui: &egui::Ui) -> f32 {
    let width = ui.available_width();
    let columns = ((width + CARD_SPACING) / (LEARN_CARD_WIDTH + CARD_SPACING))
        .floor()
        .max(1.0);
    (width - CARD_SPACING * (columns - 1.0)) / columns
}

/// A tutorial's card: its number and icon, title, summary, and the sample
/// it follows.
fn tutorial_card(
    ui: &mut egui::Ui,
    index: usize,
    tutorial: &Tutorial,
    width: f32,
) -> egui::Response {
    let palette = theme::palette(ui.ctx());
    let (rect, response) =
        ui.allocate_exact_size(vec2(width, TUTORIAL_CARD_HEIGHT), Sense::click());
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tutorial.title));
    let hover = theme::ease(ui.ctx(), response.id.with("hover"), response.hovered());
    card_frame(ui, rect, &response, hover, &palette);
    let painter = ui.painter().with_clip_rect(rect.shrink(1.0));
    let badge = rect.min + vec2(28.0, 28.0);
    painter.circle_filled(badge, 14.0, palette.accent_soft);
    painter.text(
        badge,
        egui::Align2::CENTER_CENTER,
        tutorial.icon,
        FontId::proportional(15.0),
        palette.accent_text,
    );
    painter.text(
        rect.right_top() + vec2(-16.0, 28.0),
        egui::Align2::RIGHT_CENTER,
        format!("{}", index + 1),
        FontId::proportional(type_scale::CAPTION),
        palette.text_weak,
    );
    painter.text(
        rect.min + vec2(16.0, 58.0),
        egui::Align2::LEFT_CENTER,
        tutorial.title,
        FontId::new(type_scale::BODY, theme::medium_family()),
        palette.text_strong,
    );
    let summary = widgets::elided(
        ui,
        tutorial.summary.to_owned(),
        FontId::proportional(type_scale::SMALL + 0.5),
        palette.text_secondary,
        rect.width() - 32.0,
        2,
    );
    painter.galley(rect.min + vec2(16.0, 70.0), summary, palette.text_secondary);
    painter.text(
        rect.left_bottom() + vec2(16.0, -18.0),
        egui::Align2::LEFT_CENTER,
        format!(
            "{}  {}  ·  {} steps",
            icons::FOLDER,
            tutorial.sample_name,
            tutorial.steps.len()
        ),
        FontId::proportional(type_scale::CAPTION),
        palette.text_weak,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A tutorial: the way back, its sample, its steps, what to look up, and the
/// tutorials either side. Returns where to go (`Some(None)` is the list)
/// and anything else asked for.
fn tutorial_page(
    ui: &mut egui::Ui,
    index: usize,
    tutorial: &Tutorial,
    folder: Option<PathBuf>,
    state: Option<SampleState>,
    scroll_top: bool,
) -> (Option<Option<usize>>, Option<LearnAction>) {
    let palette = theme::palette(ui.ctx());
    let (mut nav, mut action) = (None, None);
    let back = ui.add(widgets::button(
        Role::Ghost,
        Some(icons::ARROW_LEFT),
        "All tutorials",
        &palette,
    ));
    if scroll_top {
        back.scroll_to_me(Some(egui::Align::TOP));
    }
    if back.clicked() {
        nav = Some(None);
    }
    ui.add_space(12.0);
    widgets::section_label(
        ui,
        &format!("Tutorial {} of {}", index + 1, TUTORIALS.len()),
    );
    ui.add(
        egui::Label::new(
            RichText::new(tutorial.title)
                .family(theme::heading_family())
                .size(type_scale::DISPLAY)
                .color(palette.text_strong),
        )
        .wrap(),
    );
    ui.add(egui::Label::new(RichText::new(tutorial.summary).color(palette.text_secondary)).wrap());
    ui.add_space(16.0);
    if sample_box(ui, tutorial, folder.as_deref(), state, &palette) {
        action = folder.map(LearnAction::OpenSample);
    }

    let width = ui.available_width();
    for (number, step) in tutorial.steps.iter().enumerate() {
        ui.add_space(22.0);
        ui.horizontal(|ui| {
            let (badge, _) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::hover());
            ui.painter()
                .circle_filled(badge.center(), 13.0, palette.accent_soft);
            ui.painter().text(
                badge.center(),
                egui::Align2::CENTER_CENTER,
                (number + 1).to_string(),
                FontId::new(type_scale::SMALL, theme::medium_family()),
                palette.accent_text,
            );
            ui.add_space(STEP_INDENT - 26.0 - ui.spacing().item_spacing.x);
            ui.label(
                RichText::new(step.title)
                    .family(theme::medium_family())
                    .size(type_scale::HEADING)
                    .color(palette.text_strong),
            );
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_space(STEP_INDENT - ui.spacing().item_spacing.x);
            ui.vertical(|ui| {
                ui.set_max_width(width - STEP_INDENT);
                ui.add(egui::Label::new(doc_text(step.text, &palette)).wrap());
                if let Some(picture) = step.picture {
                    ui.add_space(10.0);
                    pictures::draw(ui, picture);
                }
            });
        });
    }

    if !tutorial.look_up.is_empty() {
        ui.add_space(28.0);
        widgets::section_label(ui, "Look it up in the API Reference");
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            for name in tutorial.look_up {
                let button = egui::Button::new(
                    RichText::new(theme::with_icon(icons::BOOKS, name))
                        .monospace()
                        .color(palette.accent_text),
                )
                .corner_radius(6);
                if ui
                    .add(button)
                    .on_hover_text(format!("Open {name} in the API Reference"))
                    .clicked()
                {
                    action = Some(LearnAction::LookUp(name));
                }
            }
        });
    }

    ui.add_space(28.0);
    widgets::divider(ui);
    ui.add_space(12.0);
    ui.horizontal_wrapped(|ui| {
        if let Some(before) = index.checked_sub(1) {
            let label = format!("Back: {}", TUTORIALS[before].title);
            if ui
                .add(widgets::button(
                    Role::Secondary,
                    Some(icons::ARROW_LEFT),
                    &label,
                    &palette,
                ))
                .clicked()
            {
                nav = Some(Some(before));
            }
        }
        match TUTORIALS.get(index + 1) {
            Some(next) => {
                let label = format!("Next: {}", next.title);
                if ui
                    .add(widgets::button(Role::Primary, None, &label, &palette))
                    .clicked()
                {
                    nav = Some(Some(index + 1));
                }
            }
            None => {
                if ui
                    .add(widgets::button(
                        Role::Primary,
                        Some(icons::CHECK),
                        "All tutorials",
                        &palette,
                    ))
                    .clicked()
                {
                    nav = Some(None);
                }
            }
        }
    });
    (nav, action)
}

/// The box naming the tutorial's sample, with Open sample. Returns true
/// when Open sample was clicked.
fn sample_box(
    ui: &mut egui::Ui,
    tutorial: &Tutorial,
    folder: Option<&Path>,
    state: Option<SampleState>,
    palette: &Palette,
) -> bool {
    let mut clicked = false;
    let relative = format!("examples/{}", tutorial.sample);
    egui::Frame::new()
        .fill(palette.surface)
        .stroke(Stroke::new(1.0_f32, palette.border))
        .corner_radius(CARD_RADIUS)
        .inner_margin(egui::Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let (button, hover) = match state {
                Some(SampleState::Ready) => ("Open sample", None),
                Some(SampleState::AlreadyOpen) => {
                    ("Sample is open", Some("This sample is the open project."))
                }
                Some(SampleState::Playing) => ("Open sample", Some("Press Stop first.")),
                None => (
                    "Open sample",
                    Some(
                        "The examples folder wasn't found near the editor. Press Open                          Project and pick the sample's folder instead.",
                    ),
                ),
            };
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(icons::FOLDER_OPEN)
                        .size(18.0)
                        .color(palette.accent_text),
                );
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(format!("Sample project: {}", tutorial.sample_name))
                            .family(theme::medium_family())
                            .color(palette.text_strong),
                    );
                    ui.label(
                        RichText::new(&relative)
                            .monospace()
                            .size(type_scale::SMALL)
                            .color(palette.text_weak),
                    );
                });
            });
            ui.add_space(8.0);
            let response = ui.add_enabled(
                state == Some(SampleState::Ready),
                widgets::button(Role::Primary, Some(icons::FOLDER_OPEN), button, palette),
            );
            let response = match (hover, folder) {
                (Some(text), _) => response.on_disabled_hover_text(text),
                (None, Some(folder)) => {
                    response.on_hover_text(format!("Open `{}` in the editor", folder.display()))
                }
                (None, None) => response,
            };
            clicked = response.clicked();
            ui.add_space(6.0);
            ui.add(
                egui::Label::new(
                    RichText::new(
                        "Opening a sample edits it in place: saving changes the sample. It \
                         also runs without the editor, in the player.",
                    )
                    .size(type_scale::SMALL)
                    .color(palette.text_weak),
                )
                .wrap(),
            );
        });
    clicked
}

/// Short notes on each part of the editor: an icon, a title and the note.
pub(super) const TOPICS: [(&str, &str, &str); 7] = [
    (
        icons::CUBE,
        "Build",
        "Insert parts from the Home tab, then move, resize and rotate them with the \
             handles (Ctrl+2, 3, 4). Drag a part by its body to drop it onto others.",
    ),
    (
        icons::PUZZLE_PIECE,
        "Blocks",
        "The bar above a Script's tab switches it to Blocks. Drag blocks from the \
             palette, snap them under an event, and press View as Lua to see what they do.",
    ),
    (
        icons::CODE,
        "Lua scripts",
        "Scripts run on the server, LocalScripts on each player's side. File > Set Up \
             VS Code Autocomplete gives you the whole API as you type.",
    ),
    (
        icons::LAYOUT,
        "UI and CSS",
        "Insert a Frame, Text or Button from the UI tab. Drag its handles to place \
             it, or style it with CSS in the Style section of Properties.",
    ),
    (
        icons::SUN,
        "Lighting",
        "Select Lighting in the Explorer for shadows (with softness), the sky and \
             bloom. Turn off CastShadow on a part to stop it casting a shadow.",
    ),
    (
        icons::SPEAKER_HIGH,
        "Sound",
        "Insert a Sound into a part, pick a file in Properties and press ▶ to hear it. \
             Turn on Playing to start it with the game.",
    ),
    (
        icons::PLAY,
        "Play and build",
        "F5 plays the game in the viewport and Shift+F5 stops it. File > Build Game \
             makes a game anyone can run without the editor.",
    ),
];

/// The [`TOPICS`] as cards.
fn topic_cards(ui: &mut egui::Ui, palette: &Palette) {
    let card_width = card_width(ui);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(CARD_SPACING, CARD_SPACING);
        for (icon, title, text) in TOPICS {
            let (rect, response) = ui.allocate_exact_size(vec2(card_width, 128.0), Sense::hover());
            let hover = theme::ease(ui.ctx(), response.id.with("hover"), response.hovered());
            card_frame(ui, rect, &response, hover, palette);
            ui.painter().text(
                rect.min + vec2(16.0, 24.0),
                egui::Align2::LEFT_CENTER,
                icon,
                FontId::proportional(16.0),
                palette.accent,
            );
            ui.painter().text(
                rect.min + vec2(40.0, 24.0),
                egui::Align2::LEFT_CENTER,
                title,
                FontId::new(type_scale::BODY, theme::medium_family()),
                palette.text_strong,
            );
            let galley = ui.painter().layout(
                text.to_owned(),
                FontId::proportional(type_scale::SMALL + 0.5),
                palette.text_secondary,
                rect.width() - 32.0,
            );
            ui.painter().with_clip_rect(rect.shrink(1.0)).galley(
                rect.min + vec2(16.0, 44.0),
                galley,
                palette.text_secondary,
            );
        }
    });
}
