//! The Learn page: a learning path of chapters, one for each part of the
//! engine (Markdown pages in `documentation/learn/`, drawn by
//! `super::docs`), and short step-by-step tutorials, each one task in a
//! sample project from `examples/` (the words and pictures are in
//! `learn/tutorials.rs` and `learn/pictures.rs`). The same page shows on the
//! start screen and in Help > Learn over an open project, so a tutorial
//! stays in view after its sample opens.

mod pictures;
mod tutorials;

pub(super) use tutorials::TUTORIALS;
use tutorials::{START_HERE, Tutorial};

use super::docs::{self, DocAction, Library, markdown};
use super::reference::doc_text;
use super::*;

/// How tall a tutorial's card is, in points.
const TUTORIAL_CARD_HEIGHT: f32 = 148.0;
/// How tall a chapter's card is, in points.
const CHAPTER_CARD_HEIGHT: f32 = 120.0;
/// Cards are at least this wide; a row fits as many as it can.
const LEARN_CARD_WIDTH: f32 = 260.0;
/// How far a step's words and picture are indented past its number.
const STEP_INDENT: f32 = 36.0;

/// The page's state: the open chapter or tutorial, and where each sample
/// was found.
#[derive(Default)]
pub(super) struct Learn {
    /// An index into [`TUTORIALS`].
    pub(super) open: Option<usize>,
    /// An index into the documentation's pages: a chapter of the path.
    pub(super) chapter: Option<usize>,
    /// Where each sample was found, looked for once.
    samples: HashMap<String, Option<PathBuf>>,
    /// Set when a tutorial or chapter opens, so it's shown from the top.
    scroll_top: bool,
    /// A heading of the open chapter to bring into view.
    scroll_to: Option<String>,
}

/// What a click on the Learn page asks for.
pub(super) enum LearnAction {
    /// Open the sample project in this folder.
    OpenSample(PathBuf),
    /// Open this entry in the API Reference.
    LookUp(String),
    /// Open this page of the Documentation, at a heading if one is given.
    Doc(usize, Option<String>),
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

/// The chapters of the learning path, in order: the pages of every
/// `Learn: …` section of `SUMMARY.md`, with the section's group name.
pub(super) fn chapters(library: &Library) -> Vec<(&str, usize)> {
    library
        .sections
        .iter()
        .filter_map(|section| {
            let group = section.title.strip_prefix(docs::LEARN_SECTION)?;
            let group = group.trim_start_matches(':').trim();
            Some(section.pages.iter().map(move |&page| (group, page)))
        })
        .flatten()
        .collect()
}

impl Learn {
    /// Opens a chapter of the learning path, at a heading if one is given.
    pub(super) fn open_chapter(&mut self, index: usize, anchor: Option<String>) {
        self.chapter = Some(index);
        self.open = None;
        self.scroll_top = anchor.is_none();
        self.scroll_to = anchor;
    }

    /// Draws the learning path and the tutorials, or the open chapter or
    /// tutorial. `sample_state` says whether the sample in a folder can be
    /// opened from where the page is.
    pub(super) fn show(
        &mut self,
        ui: &mut egui::Ui,
        sample_state: impl Fn(&Path) -> SampleState,
    ) -> Option<LearnAction> {
        let library = docs::library();
        if let Some(chapter) = self.chapter.filter(|&index| index < library.pages.len()) {
            return self.chapter_page(ui, library, chapter, &sample_state);
        }
        self.chapter = None;
        let Some(tutorial) = self.open.and_then(|index| TUTORIALS.get(index)) else {
            self.open = None;
            match learn_list(ui, library) {
                Some(Picked::Tutorial(index)) => {
                    self.open = Some(index);
                    self.scroll_top = true;
                }
                Some(Picked::Chapter(index)) => self.open_chapter(index, None),
                None => {}
            }
            return None;
        };
        let index = self.open?;
        let folder = self.sample_folder(tutorial.sample);
        let state = folder.as_deref().map(sample_state);
        let scroll_top = std::mem::take(&mut self.scroll_top);
        let (nav, action) = tutorial_page(ui, index, tutorial, folder, state, scroll_top);
        if let Some(open) = nav {
            self.open = open;
            self.scroll_top = true;
        }
        action
    }

    /// Where the sample `examples/<name>` is, looked for once.
    fn sample_folder(&mut self, name: &str) -> Option<PathBuf> {
        self.samples
            .entry(name.to_owned())
            .or_insert_with(|| find_sample(name))
            .clone()
    }

    /// A chapter: the way back, its page, the samples and tutorials that go
    /// with it, and the chapters either side.
    fn chapter_page(
        &mut self,
        ui: &mut egui::Ui,
        library: &Library,
        index: usize,
        sample_state: &impl Fn(&Path) -> SampleState,
    ) -> Option<LearnAction> {
        let palette = theme::palette(ui.ctx());
        let mut action = None;
        let chapters = chapters(library);
        let at = chapters.iter().position(|&(_, page)| page == index);
        let back = ui.add(widgets::button(
            Role::Ghost,
            Some(icons::ARROW_LEFT),
            "All of Learn",
            &palette,
        ));
        if std::mem::take(&mut self.scroll_top) {
            back.scroll_to_me(Some(egui::Align::TOP));
        }
        if back.clicked() {
            self.chapter = None;
            return None;
        }
        ui.add_space(theme::space::M);
        if let Some(at) = at {
            widgets::section_label(
                ui,
                &format!(
                    "Chapter {} of {}  ·  {}",
                    at + 1,
                    chapters.len(),
                    chapters[at].0
                ),
            );
        }
        let scroll_to = self.scroll_to.take();
        if let Some(clicked) = markdown::show_page(ui, library, index, scroll_to.as_deref()) {
            action = Some(clicked);
        }

        let page = &library.pages[index];
        if !page.samples.is_empty() {
            ui.add_space(28.0);
            widgets::section_label(ui, "Try it in a sample");
            ui.add_space(6.0);
            for sample in &page.samples {
                let folder = self.sample_folder(sample);
                let state = folder.as_deref().map(sample_state);
                let tutorial = TUTORIALS.iter().position(|t| t.sample == sample);
                match chapter_sample(ui, sample, tutorial, folder.as_deref(), state, &palette) {
                    Some(SampleClick::Open) => {
                        if let Some(folder) = folder {
                            return Some(LearnAction::OpenSample(folder));
                        }
                    }
                    Some(SampleClick::Tutorial(tutorial)) => {
                        self.chapter = None;
                        self.open = Some(tutorial);
                        self.scroll_top = true;
                        return None;
                    }
                    None => {}
                }
                ui.add_space(theme::space::S);
            }
        }

        ui.add_space(28.0);
        widgets::divider(ui);
        ui.add_space(12.0);
        ui.horizontal_wrapped(|ui| {
            if let Some(before) = at.and_then(|at| at.checked_sub(1)) {
                let label = format!("Back: {}", library.pages[chapters[before].1].title);
                if ui
                    .add(widgets::button(
                        Role::Secondary,
                        Some(icons::ARROW_LEFT),
                        &label,
                        &palette,
                    ))
                    .clicked()
                {
                    action = Some(DocAction::Open(chapters[before].1, None));
                }
            }
            if let Some(&(_, next)) = at.and_then(|at| chapters.get(at + 1)) {
                let label = format!("Next: {}", library.pages[next].title);
                if ui
                    .add(widgets::button(Role::Primary, None, &label, &palette))
                    .clicked()
                {
                    action = Some(DocAction::Open(next, None));
                }
            }
        });
        match action? {
            DocAction::Open(page, anchor) => {
                if chapters.iter().any(|&(_, chapter)| chapter == page) {
                    self.open_chapter(page, anchor);
                    None
                } else {
                    Some(LearnAction::Doc(page, anchor))
                }
            }
            DocAction::LookUp(name) => Some(LearnAction::LookUp(name)),
            DocAction::OpenSample(name) => self.sample_folder(&name).map(LearnAction::OpenSample),
        }
    }
}

/// What was picked on the list.
enum Picked {
    Tutorial(usize),
    Chapter(usize),
}

/// What a chapter's sample row asks for.
enum SampleClick {
    Open,
    Tutorial(usize),
}

/// A sample a chapter follows: its name, Open sample, and its tutorial if
/// it has one.
fn chapter_sample(
    ui: &mut egui::Ui,
    sample: &str,
    tutorial: Option<usize>,
    folder: Option<&Path>,
    state: Option<SampleState>,
    palette: &Palette,
) -> Option<SampleClick> {
    let mut clicked = None;
    egui::Frame::new()
        .fill(palette.surface)
        .stroke(Stroke::new(1.0_f32, palette.border))
        .corner_radius(CARD_RADIUS)
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let name = find_sample_name(folder).unwrap_or_else(|| sample.to_owned());
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(icons::FOLDER_OPEN).color(palette.accent_text));
                ui.label(
                    RichText::new(name)
                        .family(theme::medium_family())
                        .color(palette.text_strong),
                );
                ui.label(
                    RichText::new(format!("examples/{sample}"))
                        .monospace()
                        .size(type_scale::SMALL)
                        .color(palette.text_weak),
                );
            });
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                let (label, hover) = match state {
                    Some(SampleState::Ready) => ("Open sample", None),
                    Some(SampleState::AlreadyOpen) => {
                        ("Sample is open", Some("This sample is the open project."))
                    }
                    Some(SampleState::Playing) => ("Open sample", Some("Press Stop first.")),
                    None => (
                        "Open sample",
                        Some("The examples folder wasn't found near the editor."),
                    ),
                };
                let open = ui.add_enabled(
                    state == Some(SampleState::Ready),
                    widgets::button(Role::Secondary, Some(icons::FOLDER_OPEN), label, palette),
                );
                let open = match hover {
                    Some(text) => open.on_disabled_hover_text(text),
                    None => open,
                };
                if open.clicked() {
                    clicked = Some(SampleClick::Open);
                }
                if let Some(index) = tutorial {
                    let label = format!("Tutorial: {}", TUTORIALS[index].title);
                    if ui
                        .add(widgets::button(
                            Role::Ghost,
                            Some(icons::LIST_NUMBERS),
                            &label,
                            palette,
                        ))
                        .clicked()
                    {
                        clicked = Some(SampleClick::Tutorial(index));
                    }
                }
            });
        });
    clicked
}

/// The name a sample's `axcenity.json` gives it.
fn find_sample_name(folder: Option<&Path>) -> Option<String> {
    axcenity_project::read_manifest(folder?).map(|manifest| manifest.name)
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

/// The heading, the learning path's chapters as cards by group, and the
/// tutorials as cards. Returns what was clicked.
fn learn_list(ui: &mut egui::Ui, library: &Library) -> Option<Picked> {
    let palette = theme::palette(ui.ctx());
    ui.label(
        RichText::new("Learn")
            .family(theme::display_family())
            .size(type_scale::DISPLAY)
            .extra_letter_spacing(theme::DISPLAY_TRACKING)
            .color(palette.text_strong),
    );
    ui.add(
        egui::Label::new(
            RichText::new(
                "A learning path through every part of Axcenity, a chapter each, and short \
                 tutorials that each do one task in a sample project you can open with one \
                 click. Everything here stays open in Help > Learn while you work.",
            )
            .color(palette.text_secondary),
        )
        .wrap(),
    );
    let mut clicked = None;
    let chapters = chapters(library);
    let mut groups: Vec<(&str, Vec<(usize, usize)>)> = Vec::new();
    for (number, &(group, page)) in chapters.iter().enumerate() {
        match groups.last_mut() {
            Some((name, pages)) if *name == group => pages.push((number, page)),
            _ => groups.push((group, vec![(number, page)])),
        }
    }
    for (group, pages) in groups {
        ui.add_space(theme::space::XL);
        widgets::section_label(ui, group);
        ui.add_space(6.0);
        let card_width = card_width(ui);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(CARD_SPACING, CARD_SPACING);
            for (number, page) in pages {
                if chapter_card(ui, number, &library.pages[page], card_width).clicked() {
                    clicked = Some(Picked::Chapter(page));
                }
            }
        });
    }
    for (heading, range) in [
        ("Tutorials: start here", 0..START_HERE),
        ("More tutorials", START_HERE..TUTORIALS.len()),
    ] {
        ui.add_space(theme::space::XL + theme::space::S);
        widgets::section_label(ui, heading);
        ui.add_space(6.0);
        let card_width = card_width(ui);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(CARD_SPACING, CARD_SPACING);
            for index in range {
                if tutorial_card(ui, index, &TUTORIALS[index], card_width).clicked() {
                    clicked = Some(Picked::Tutorial(index));
                }
            }
        });
    }
    clicked
}

/// A chapter's card: its number and icon, title, description and status.
fn chapter_card(
    ui: &mut egui::Ui,
    number: usize,
    page: &docs::library::Page,
    width: f32,
) -> egui::Response {
    let palette = theme::palette(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(vec2(width, CHAPTER_CARD_HEIGHT), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &page.title));
    let hover = theme::ease(ui.ctx(), response.id.with("hover"), response.hovered());
    card_frame(ui, rect, &response, hover, &palette);
    let painter = ui.painter().with_clip_rect(rect.shrink(1.0));
    painter.text(
        rect.min + vec2(16.0, 24.0),
        egui::Align2::LEFT_CENTER,
        markdown::icon(&page.icon),
        FontId::proportional(16.0),
        palette.accent,
    );
    // The title stops short of the chapter number at the right.
    let title = widgets::elided(
        ui,
        page.title.clone(),
        FontId::new(type_scale::BODY, theme::medium_family()),
        palette.text_strong,
        (rect.width() - 40.0 - 36.0).max(40.0),
        1,
    );
    let title_top = rect.top() + 24.0 - title.size().y / 2.0;
    painter.galley(
        pos2(rect.left() + 40.0, title_top),
        title,
        palette.text_strong,
    );
    painter.text(
        rect.right_top() + vec2(-16.0, 24.0),
        egui::Align2::RIGHT_CENTER,
        format!("{}", number + 1),
        FontId::proportional(type_scale::CAPTION),
        palette.text_weak,
    );
    let summary = widgets::elided(
        ui,
        page.description.clone(),
        FontId::proportional(type_scale::SMALL + 0.5),
        palette.text_secondary,
        rect.width() - 32.0,
        3,
    );
    painter.galley(rect.min + vec2(16.0, 42.0), summary, palette.text_secondary);
    let status = page.status;
    if status != docs::library::Status::Stable {
        let color = match status {
            docs::library::Status::Partial => palette.warning,
            _ => palette.accent_text,
        };
        painter.text(
            rect.left_bottom() + vec2(16.0, -16.0),
            egui::Align2::LEFT_CENTER,
            status.label(),
            FontId::proportional(type_scale::CAPTION),
            color,
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
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
                .family(theme::display_family())
                .size(type_scale::DISPLAY)
                .extra_letter_spacing(theme::DISPLAY_TRACKING)
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
                    action = Some(LearnAction::LookUp((*name).to_owned()));
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
                        "The examples folder wasn't found near the editor. Press Open \
                         Project and pick the sample's folder instead.",
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
