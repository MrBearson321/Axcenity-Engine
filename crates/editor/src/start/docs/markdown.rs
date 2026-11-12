//! Draws a [`Page`] of the built-in documentation: headings, paragraphs,
//! lists, code with Lua colours and a Copy button, notes, tables and links.
//! Text in backticks that names something in the API Reference (`Part`,
//! `Part.Touched`, `task.wait()`) is a link to its entry.

use std::collections::{HashMap, HashSet};

use eframe::egui::{self, FontId, RichText, Stroke, vec2};

use super::library::{Block, Library, LinkTarget, Page, Span, Status};
use crate::lua_syntax;
use crate::theme::{self, Palette, icons, type_scale};

/// What a click in a page asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DocAction {
    /// Show this page, at this heading if one is given.
    Open(usize, Option<String>),
    /// Open this entry in the API Reference.
    LookUp(String),
    /// Open the sample project `examples/<name>`.
    OpenSample(String),
}

/// The names the API Reference has an entry for, as `Part`, `Part.Touched`
/// and `task.wait` (methods with `.`, as the reference searches them).
pub(crate) fn api_names() -> &'static HashSet<String> {
    static NAMES: std::sync::OnceLock<HashSet<String>> = std::sync::OnceLock::new();
    NAMES.get_or_init(|| {
        axcenity_script::api_reference()
            .iter()
            .map(|entry| entry.qualified_name())
            .collect()
    })
}

/// The API Reference entry a piece of code names, if any: `Part.Touched`,
/// `Instance:FindFirstChild(name)` or `task.wait()`. `Enum.Material.Neon`
/// names the enum item.
pub(crate) fn api_name(code: &str) -> Option<String> {
    let name = code.split('(').next()?.trim().replace(':', ".");
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
    {
        return None;
    }
    let name = name.strip_prefix("Enum.").unwrap_or(&name);
    let name = match name.split_once('.') {
        Some((global, member)) => match global_class(global) {
            Some(class) => format!("{class}.{member}"),
            None => name.to_owned(),
        },
        None => name.to_owned(),
    };
    api_names().contains(&name).then_some(name)
}

/// Pages whose `code` names APIs only to show how the documentation works,
/// so they aren't offered as reading about those APIs.
const NOT_EXPLAINING: &[&str] = &[super::HOME, "reference/maintaining-docs.md"];

/// The pages (indices into the library) whose `code` names the API entry
/// called `name`, in page order: the documentation that explains it.
pub(crate) fn pages_mentioning(name: &str) -> &'static [usize] {
    static MENTIONS: std::sync::OnceLock<HashMap<String, Vec<usize>>> = std::sync::OnceLock::new();
    let mentions = MENTIONS.get_or_init(|| {
        let mut mentions: HashMap<String, Vec<usize>> = HashMap::new();
        for (index, page) in super::library().pages.iter().enumerate() {
            // These name APIs as examples of how the pages work.
            if NOT_EXPLAINING.contains(&page.path.as_str()) {
                continue;
            }
            for code in page.code_spans() {
                if let Some(name) = api_name(code) {
                    let pages = mentions.entry(name).or_default();
                    if !pages.contains(&index) {
                        pages.push(index);
                    }
                }
            }
        }
        mentions
    });
    mentions.get(name).map_or(&[], Vec::as_slice)
}

/// The class whose members a global's are: `workspace:Raycast()` is
/// `Workspace.Raycast`, `game:GetService()` is `DataModel.GetService`.
pub(crate) fn global_class(global: &str) -> Option<&'static str> {
    match global {
        "workspace" => Some("Workspace"),
        "game" => Some("DataModel"),
        "script" => Some("Instance"),
        _ => None,
    }
}

/// The icon a page's `icon:` line names, or an open book.
pub(crate) fn icon(name: &str) -> &'static str {
    match name {
        "rocket" => icons::ROCKET_LAUNCH,
        "layout" => icons::LAYOUT,
        "folder" => icons::FOLDER_OPEN,
        "cube" => icons::CUBE,
        "tree" => icons::TREE_STRUCTURE,
        "palette" => icons::PALETTE,
        "sun" => icons::SUN,
        "atom" => icons::ATOM,
        "person" => icons::PERSON_SIMPLE_RUN,
        "speaker" => icons::SPEAKER_HIGH,
        "mountains" => icons::MOUNTAINS,
        "cursor" => icons::CURSOR_CLICK,
        "code" => icons::CODE,
        "puzzle" => icons::PUZZLE_PIECE,
        "download" => icons::DOWNLOAD_SIMPLE,
        "bug" => icons::BUG,
        "package" => icons::PACKAGE,
        "stack" => icons::STACK,
        "gear" => icons::GEAR_SIX,
        "books" => icons::BOOKS,
        "file" => icons::FILE_TEXT,
        "gauge" => icons::GAUGE,
        "globe" => icons::GLOBE,
        "image" => icons::IMAGE,
        "brackets" => icons::BRACKETS_CURLY,
        "users" => icons::USERS_THREE,
        _ => icons::BOOK_OPEN,
    }
}

/// Every name [`icon`] knows, for the test that checks pages use them.
#[cfg(test)]
pub(crate) const ICON_NAMES: &[&str] = &[
    "rocket",
    "layout",
    "folder",
    "cube",
    "tree",
    "palette",
    "sun",
    "atom",
    "person",
    "speaker",
    "mountains",
    "cursor",
    "code",
    "puzzle",
    "download",
    "bug",
    "package",
    "stack",
    "gear",
    "books",
    "file",
    "gauge",
    "globe",
    "image",
    "brackets",
    "users",
];

/// A small pill naming a page's status. Stable pages have none.
pub(crate) fn status_badge(ui: &mut egui::Ui, status: Status, palette: &Palette) {
    let (color, icon, tip) = match status {
        Status::Stable => return,
        Status::Partial => (
            palette.warning,
            icons::CIRCLE_HALF,
            "Built and usable, with a known part missing or off by default. The page says which.",
        ),
        Status::Experimental => (
            palette.accent_text,
            icons::FLASK,
            "Built, but early: it may change, and may be off by default. The page says how to try it.",
        ),
    };
    egui::Frame::new()
        .fill(color.gamma_multiply(0.14))
        .stroke(Stroke::new(1.0_f32, color.gamma_multiply(0.5)))
        .corner_radius(theme::radius::CARD)
        .inner_margin(theme::space::margin(theme::space::S, theme::space::XXS))
        .show(ui, |ui| {
            ui.label(
                RichText::new(theme::with_icon(icon, status.label()))
                    .size(type_scale::CAPTION)
                    .color(color),
            )
        })
        .response
        .on_hover_text(tip);
}

/// Draws `page`'s title, description, status and blocks. `scroll_to` is a
/// heading's anchor to bring into view (once). Returns what was clicked.
pub(crate) fn show_page(
    ui: &mut egui::Ui,
    library: &Library,
    index: usize,
    scroll_to: Option<&str>,
) -> Option<DocAction> {
    let page = &library.pages[index];
    let palette = theme::palette(ui.ctx());
    let mut action = None;
    ui.add(
        egui::Label::new(
            RichText::new(&page.title)
                .family(theme::display_family())
                .size(type_scale::DISPLAY)
                .extra_letter_spacing(theme::DISPLAY_TRACKING)
                .color(palette.text_strong),
        )
        .wrap(),
    );
    if !page.description.is_empty() {
        ui.add(
            egui::Label::new(RichText::new(&page.description).color(palette.text_secondary)).wrap(),
        );
    }
    if page.status != Status::Stable {
        ui.add_space(theme::space::S);
        status_badge(ui, page.status, &palette);
    }
    ui.add_space(theme::space::M);
    for (number, block) in page.blocks.iter().enumerate() {
        if let Some(clicked) = draw_block(ui, library, page, number, block, scroll_to, &palette) {
            action = Some(clicked);
        }
    }
    action
}

/// One block, with the space before it. Returns what was clicked.
fn draw_block(
    ui: &mut egui::Ui,
    library: &Library,
    page: &Page,
    number: usize,
    block: &Block,
    scroll_to: Option<&str>,
    palette: &Palette,
) -> Option<DocAction> {
    let mut action = None;
    match block {
        Block::Heading {
            level,
            text,
            anchor,
        } => {
            let (size, space) = match level {
                2 => (type_scale::HEADING + 3.0, theme::space::XL),
                3 => (type_scale::HEADING, theme::space::L),
                _ => (type_scale::BODY, theme::space::M),
            };
            ui.add_space(space);
            let response = ui.add(
                egui::Label::new(
                    RichText::new(text)
                        .family(theme::medium_family())
                        .size(size)
                        .color(palette.text_strong),
                )
                .wrap(),
            );
            if scroll_to == Some(anchor.as_str()) {
                response.scroll_to_me(Some(egui::Align::TOP));
            }
            ui.add_space(theme::space::XS);
        }
        Block::Paragraph(spans) => {
            ui.add_space(theme::space::S);
            action = spans_ui(ui, library, page, spans, palette);
        }
        Block::List { ordered, items } => {
            ui.add_space(theme::space::XS);
            for (index, item) in items.iter().enumerate() {
                ui.add_space(theme::space::XS);
                ui.horizontal_top(|ui| {
                    let marker = if *ordered {
                        format!("{}.", index + 1)
                    } else {
                        "•".to_owned()
                    };
                    ui.add_sized(
                        vec2(18.0, 16.0),
                        egui::Label::new(RichText::new(marker).color(palette.text_weak)),
                    );
                    ui.vertical(|ui| {
                        if let Some(clicked) = spans_ui(ui, library, page, item, palette) {
                            action = Some(clicked);
                        }
                    });
                });
            }
        }
        Block::Code { lang, text } => {
            ui.add_space(theme::space::S);
            code_block(ui, number, lang, text, palette);
        }
        Block::Note(spans) => {
            ui.add_space(theme::space::S);
            let frame = egui::Frame::new()
                .fill(palette.accent_soft.gamma_multiply(0.45))
                .corner_radius(theme::radius::CARD)
                .inner_margin(theme::space::margin(theme::space::M, theme::space::S))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal_top(|ui| {
                        ui.label(RichText::new(icons::INFO).color(palette.accent_text));
                        ui.vertical(|ui| spans_ui(ui, library, page, spans, palette))
                            .inner
                    })
                    .inner
                });
            let rect = frame.response.rect;
            ui.painter().vline(
                rect.left() + 1.0,
                rect.y_range().shrink(4.0),
                Stroke::new(2.0_f32, palette.accent),
            );
            action = frame.inner;
        }
        Block::Table { header, rows } => {
            ui.add_space(theme::space::S);
            action = table(ui, library, page, header, rows, palette);
        }
        Block::Rule => {
            ui.add_space(theme::space::M);
            crate::widgets::divider(ui);
            ui.add_space(theme::space::XS);
        }
    }
    action
}

/// A run of text, wrapping, with its links. Returns what was clicked.
fn spans_ui(
    ui: &mut egui::Ui,
    library: &Library,
    page: &Page,
    spans: &[Span],
    palette: &Palette,
) -> Option<DocAction> {
    let mut action = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for span in spans {
            match span {
                Span::Text(text) => {
                    ui.add(egui::Label::new(RichText::new(text).color(palette.text)).wrap());
                }
                Span::Strong(text) => {
                    ui.add(
                        egui::Label::new(
                            RichText::new(text)
                                .family(theme::medium_family())
                                .color(palette.text_strong),
                        )
                        .wrap(),
                    );
                }
                Span::Code(code) => match api_name(code) {
                    Some(name) => {
                        let link = ui
                            .add(egui::Link::new(code_text(code, palette.accent_text)))
                            .on_hover_text(format!("Open {name} in the API Reference"));
                        if link.clicked() {
                            action = Some(DocAction::LookUp(name));
                        }
                    }
                    None => {
                        ui.add(egui::Label::new(code_text(code, palette.text_strong)).wrap());
                    }
                },
                Span::Link { text, target, code } => {
                    let label = if *code {
                        code_text(text, palette.accent_text)
                    } else {
                        RichText::new(text).color(palette.accent_text)
                    };
                    let resolved = library.resolve(&page.path, target);
                    let hover = match &resolved {
                        LinkTarget::Page(index, _) => library.pages[*index].title.clone(),
                        LinkTarget::Anchor(_) => "Further down this page".to_owned(),
                        LinkTarget::Sample(name) => format!("Open the sample examples/{name}"),
                        LinkTarget::Api(_) => "Open the API Reference".to_owned(),
                        LinkTarget::Url(url) => url.clone(),
                        LinkTarget::Missing(path) => format!("{path} isn't built in"),
                    };
                    if ui
                        .add(egui::Link::new(label))
                        .on_hover_text(hover)
                        .clicked()
                    {
                        action = follow(ui, library, page, resolved);
                    }
                }
            }
        }
    });
    action
}

/// What a click on a link does.
fn follow(ui: &egui::Ui, library: &Library, page: &Page, target: LinkTarget) -> Option<DocAction> {
    let here = library.index_of(&page.path)?;
    match target {
        LinkTarget::Page(index, anchor) => Some(DocAction::Open(index, anchor)),
        LinkTarget::Anchor(anchor) => Some(DocAction::Open(here, Some(anchor))),
        LinkTarget::Sample(name) => Some(DocAction::OpenSample(name)),
        LinkTarget::Api(anchor) => Some(DocAction::LookUp(anchor)),
        LinkTarget::Url(url) => {
            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
            None
        }
        LinkTarget::Missing(_) => None,
    }
}

fn code_text(text: &str, color: egui::Color32) -> RichText {
    RichText::new(text)
        .monospace()
        .size(type_scale::BODY - 0.5)
        .color(color)
}

/// A code block: Lua in its colours, with a Copy button.
pub(crate) fn code_block(
    ui: &mut egui::Ui,
    number: usize,
    lang: &str,
    text: &str,
    palette: &Palette,
) {
    let font = FontId::monospace(type_scale::BODY - 0.5);
    let job = if matches!(lang, "lua" | "luau") {
        lua_syntax::layout_job(text, font, &ui.style().visuals)
    } else {
        egui::text::LayoutJob::simple(text.to_owned(), font, palette.text, f32::INFINITY)
    };
    let frame = egui::Frame::new()
        .fill(palette.inset)
        .stroke(Stroke::new(1.0_f32, palette.border))
        .corner_radius(theme::radius::CARD)
        .inner_margin(theme::space::margin(theme::space::M, theme::space::M))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::ScrollArea::horizontal()
                .id_salt(("doc code", number))
                .show(ui, |ui| ui.label(job));
        });
    let rect = frame.response.rect;
    let copy = egui::Rect::from_min_size(rect.right_top() + vec2(-30.0, 6.0), vec2(24.0, 22.0));
    let response = ui
        .interact(
            copy,
            ui.id().with(("copy code", number)),
            egui::Sense::click(),
        )
        .on_hover_text("Copy");
    let color = if response.hovered() {
        palette.text_strong
    } else {
        palette.text_weak
    };
    ui.painter().text(
        copy.center(),
        egui::Align2::CENTER_CENTER,
        icons::COPY,
        FontId::proportional(theme::size::ICON_SMALL),
        color,
    );
    if response.clicked() {
        ui.ctx().copy_text(text.to_owned());
    }
}

/// A pipe table. Columns share the width by how much text they hold.
fn table(
    ui: &mut egui::Ui,
    library: &Library,
    page: &Page,
    header: &[Vec<Span>],
    rows: &[Vec<Vec<Span>>],
    palette: &Palette,
) -> Option<DocAction> {
    let mut action = None;
    let columns = header.len().max(1);
    let length = |cell: &[Span]| -> usize {
        cell.iter()
            .map(|span| match span {
                Span::Text(t) | Span::Strong(t) | Span::Code(t) => t.len(),
                Span::Link { text, .. } => text.len(),
            })
            .sum()
    };
    let mut weights = vec![4usize; columns];
    for row in std::iter::once(header).chain(rows.iter().map(Vec::as_slice)) {
        for (column, cell) in row.iter().enumerate().take(columns) {
            weights[column] = weights[column].max(length(cell).min(60));
        }
    }
    let total: usize = weights.iter().sum();
    let gap = theme::space::M;
    let width = ui.available_width() - gap * (columns as f32 - 1.0);
    let widths: Vec<f32> = weights
        .iter()
        .map(|&weight| (width * weight as f32 / total as f32).max(48.0))
        .collect();
    egui::Frame::new()
        .stroke(Stroke::new(1.0_f32, palette.border))
        .corner_radius(theme::radius::CARD)
        .inner_margin(theme::space::margin(theme::space::S, theme::space::S))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            for (number, row) in std::iter::once(header)
                .chain(rows.iter().map(Vec::as_slice))
                .enumerate()
            {
                if number == 1 {
                    crate::widgets::divider(ui);
                }
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for (column, cell) in row.iter().enumerate().take(columns) {
                        let cell_width = widths[column] - gap / 2.0;
                        ui.allocate_ui_with_layout(
                            vec2(cell_width, 0.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                ui.set_width(cell_width);
                                let clicked = if number == 0 {
                                    ui.label(
                                        RichText::new(spans_plain(cell))
                                            .family(theme::medium_family())
                                            .color(palette.text_strong),
                                    );
                                    None
                                } else {
                                    spans_ui(ui, library, page, cell, palette)
                                };
                                if clicked.is_some() {
                                    action = clicked;
                                }
                            },
                        );
                    }
                });
                ui.add_space(theme::space::XS);
            }
        });
    action
}

fn spans_plain(spans: &[Span]) -> String {
    spans
        .iter()
        .map(|span| match span {
            Span::Text(t) | Span::Strong(t) | Span::Code(t) => t.as_str(),
            Span::Link { text, .. } => text.as_str(),
        })
        .collect()
}
