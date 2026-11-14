//! The API Reference page: every class, type, enum, global and member that
//! scripts can use, searchable by name. The entries come from
//! `axcenity_script::api_reference`, which is written alongside the luau-lsp
//! definitions file, so the page shows the definitions' own descriptions and
//! changes whenever they do.

use axcenity_script::{ApiEntry, ApiKind};
use eframe::egui::text::{LayoutJob, TextFormat};

use super::docs::{self, DocAction};
use super::*;

/// How many search results are listed at once.
const MAX_RESULTS: usize = 100;
/// How wide the kind column of a row is, in points.
const KIND_WIDTH: f32 = 76.0;
/// The kinds the page lists before anything is searched for, in order.
const TOP_LEVEL: [ApiKind; 5] = [
    ApiKind::Class,
    ApiKind::Type,
    ApiKind::Enum,
    ApiKind::Library,
    ApiKind::Global,
];
/// The kinds of member an entry's page lists, in order.
const MEMBERS: [ApiKind; 4] = [
    ApiKind::Property,
    ApiKind::Event,
    ApiKind::Function,
    ApiKind::EnumItem,
];

/// The page's state: the entries, what's typed in the search box and the
/// entry that's open.
#[derive(Default)]
pub(super) struct Reference {
    /// Read the first time the page is shown.
    pub(super) entries: Vec<ApiEntry>,
    pub(super) query: String,
    /// An index into `entries`.
    pub(super) open: Option<usize>,
    /// The code tab an example can be put into (its file name), when one
    /// is the active tab; set by the editor before each frame.
    pub(super) insert_into: Option<String>,
    /// An example whose Insert button was pressed, for the editor.
    pub(super) inserted: Option<String>,
}

/// Where a click on the page goes.
enum Nav {
    Results,
    Open(usize),
    /// A page of the documentation that explains the entry.
    Doc(usize),
    /// Put this example into the active code tab.
    Insert(&'static str),
}

impl Reference {
    /// Opens the entry called `name` (`Part.Touched`), with it typed in the
    /// search box, as if it had been searched for.
    pub(super) fn look_up(&mut self, name: &str) {
        self.load();
        self.query = name.to_owned();
        self.open = search_entries(&self.entries, name).first().copied();
    }

    fn load(&mut self) {
        if self.entries.is_empty() {
            self.entries = axcenity_script::api_reference();
        }
    }

    /// Draws the page. Returns a documentation page to open, when one of an
    /// entry's "Read more" pages is clicked.
    pub(super) fn show(&mut self, ui: &mut egui::Ui) -> Option<DocAction> {
        self.load();
        let palette = theme::palette(ui.ctx());
        ui.label(
            RichText::new("API Reference")
                .family(theme::display_family())
                .size(type_scale::DISPLAY)
                .extra_letter_spacing(theme::DISPLAY_TRACKING)
                .color(palette.text_strong),
        );
        ui.label(
            RichText::new(
                "Every class, property, event and function scripts can use, from the \
                 engine's Lua definitions. Search by name, such as Part, Size or Instance.new.",
            )
            .color(palette.text_secondary),
        );
        ui.add_space(18.0);
        let width = (ui.available_width() - widgets::SEARCH_FIELD_MARGINS).min(420.0);
        let search = widgets::search_field(
            ui,
            &mut self.query,
            "Search classes, properties, events and functions",
            width,
        );
        let results = search_entries(&self.entries, &self.query);
        if search.changed() {
            self.open = None;
        }
        let entered = ui.input(|input| input.key_pressed(egui::Key::Enter));
        if search.lost_focus() && entered && !self.query.trim().is_empty() {
            self.open = results.first().copied();
        }
        ui.add_space(20.0);

        let nav = match self.open {
            Some(index) => entry_page(
                ui,
                &self.entries,
                index,
                self.insert_into.as_deref(),
                &palette,
            ),
            None if self.query.trim().is_empty() => browse(ui, &self.entries),
            None => result_list(ui, &self.entries, &results, &palette),
        };
        match nav {
            Some(Nav::Results) => self.open = None,
            Some(Nav::Open(index)) => self.open = Some(index),
            Some(Nav::Doc(page)) => return Some(DocAction::Open(page, None)),
            Some(Nav::Insert(example)) => self.inserted = Some(example.to_owned()),
            None => {}
        }
        None
    }
}

/// The entries whose name matches `query`, best first: an exact name, then
/// names that start with it, then names that have it anywhere. `Part.Size`
/// and `Instance:FindFirstChild` match by owner and name too. Classes and
/// other top-level entries come before members that match as well. With
/// nothing typed, it's every top-level entry.
pub(super) fn search_entries(entries: &[ApiEntry], query: &str) -> Vec<usize> {
    let query = query.trim().replace(':', ".").to_lowercase();
    if query.is_empty() {
        return (0..entries.len())
            .filter(|&index| entries[index].owner.is_empty())
            .collect();
    }
    let mut found: Vec<(u8, bool, usize)> = entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            let name = entry.name.to_lowercase();
            let qualified = entry.qualified_name().to_lowercase();
            let rank = if name == query || qualified == query {
                0
            } else if name.starts_with(&query) {
                1
            } else if qualified.starts_with(&query) {
                2
            } else if name.contains(&query) {
                3
            } else if qualified.contains(&query) {
                4
            } else {
                return None;
            };
            Some((rank, !entry.owner.is_empty(), index))
        })
        .collect();
    found.sort_unstable();
    found.into_iter().map(|(_, _, index)| index).collect()
}

/// What a group of entries of `kind` is called.
fn plural(kind: ApiKind) -> &'static str {
    match kind {
        ApiKind::Class => "Classes",
        ApiKind::Type => "Types",
        ApiKind::Enum => "Enums",
        ApiKind::Library => "Libraries",
        ApiKind::Global => "Globals",
        ApiKind::Property => "Properties",
        ApiKind::Event => "Events",
        ApiKind::Function => "Functions",
        ApiKind::EnumItem => "Items",
    }
}

/// The index of the top-level entry called `name`.
fn top_level(entries: &[ApiEntry], name: &str) -> Option<usize> {
    entries
        .iter()
        .position(|entry| entry.owner.is_empty() && entry.name == name)
}

/// Before anything is searched for: every top-level entry, by kind.
fn browse(ui: &mut egui::Ui, entries: &[ApiEntry]) -> Option<Nav> {
    let mut nav = None;
    for kind in TOP_LEVEL {
        let group: Vec<usize> = (0..entries.len())
            .filter(|&index| entries[index].owner.is_empty() && entries[index].kind == kind)
            .collect();
        if group.is_empty() {
            continue;
        }
        widgets::section_label(ui, plural(kind));
        ui.add_space(4.0);
        if let Some(index) = rows(ui, entries, &group) {
            nav = Some(Nav::Open(index));
        }
        ui.add_space(20.0);
    }
    nav
}

fn result_list(
    ui: &mut egui::Ui,
    entries: &[ApiEntry],
    results: &[usize],
    palette: &Palette,
) -> Option<Nav> {
    if results.is_empty() {
        empty_state(
            ui,
            icons::MAGNIFYING_GLASS,
            "Nothing matches",
            "No class, property, event or function has that in its name.",
        );
        return None;
    }
    let count = if results.len() > MAX_RESULTS {
        format!(
            "{} results, showing the first {MAX_RESULTS}. Type more of the name to narrow them.",
            results.len()
        )
    } else if results.len() == 1 {
        "1 result. Press Enter to open it.".to_owned()
    } else {
        format!("{} results. Press Enter to open the first.", results.len())
    };
    ui.label(
        RichText::new(count)
            .size(type_scale::SMALL)
            .color(palette.text_weak),
    );
    ui.add_space(6.0);
    let shown = &results[..results.len().min(MAX_RESULTS)];
    rows(ui, entries, shown).map(Nav::Open)
}

/// A row for each of `indices`. Returns the one clicked.
fn rows(ui: &mut egui::Ui, entries: &[ApiEntry], indices: &[usize]) -> Option<usize> {
    let mut clicked = None;
    ui.spacing_mut().item_spacing.y = 2.0;
    for &index in indices {
        if entry_row(ui, &entries[index]).clicked() {
            clicked = Some(index);
        }
    }
    clicked
}

/// One entry in a list: its kind, its full name and the start of its
/// description.
fn entry_row(ui: &mut egui::Ui, entry: &ApiEntry) -> egui::Response {
    let palette = theme::palette(ui.ctx());
    let size = vec2(ui.available_width(), 46.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let name = entry.qualified_name();
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &name));
    widgets::paint_hover(ui, rect, &response, false, theme::radius::CARD);
    let left = rect.left() + 12.0;
    ui.painter().text(
        pos2(left, rect.top() + 15.0),
        egui::Align2::LEFT_CENTER,
        entry.kind.label(),
        FontId::proportional(type_scale::CAPTION),
        palette.text_weak,
    );
    let text_left = left + KIND_WIDTH;
    let text_width = (rect.right() - 12.0 - text_left).max(40.0);
    let title = widgets::elided(
        ui,
        name,
        FontId::monospace(type_scale::BODY),
        palette.text_strong,
        text_width,
        1,
    );
    ui.painter().galley(
        pos2(text_left, rect.top() + 7.0),
        title,
        palette.text_strong,
    );
    let doc = if entry.doc.is_empty() {
        "No description yet.".to_owned()
    } else {
        entry.doc.replace('`', "")
    };
    let doc = widgets::elided(
        ui,
        doc,
        FontId::proportional(type_scale::SMALL),
        palette.text_secondary,
        text_width,
        1,
    );
    ui.painter().galley(
        pos2(text_left, rect.top() + 25.0),
        doc,
        palette.text_secondary,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// An entry's own page: how it's written or called, what it does, and its
/// members if it has any.
fn entry_page(
    ui: &mut egui::Ui,
    entries: &[ApiEntry],
    index: usize,
    insert_into: Option<&str>,
    palette: &Palette,
) -> Option<Nav> {
    let Some(entry) = entries.get(index) else {
        return Some(Nav::Results);
    };
    let mut nav = None;
    if ui
        .add(widgets::button(
            Role::Ghost,
            Some(icons::ARROW_LEFT),
            "Back to results",
            palette,
        ))
        .clicked()
    {
        nav = Some(Nav::Results);
    }
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        widgets::section_label(ui, entry.kind.label());
        if let Some(owner) = top_level(entries, &entry.owner) {
            ui.label(
                RichText::new("of")
                    .size(type_scale::CAPTION)
                    .color(palette.text_weak),
            );
            if ui
                .link(
                    RichText::new(&entry.owner)
                        .monospace()
                        .color(palette.accent_text),
                )
                .clicked()
            {
                nav = Some(Nav::Open(owner));
            }
        }
    });
    ui.label(
        RichText::new(entry.qualified_name())
            .family(theme::display_family())
            .size(type_scale::DISPLAY)
            .extra_letter_spacing(theme::DISPLAY_TRACKING)
            .color(palette.text_strong),
    );
    ui.add_space(10.0);
    egui::Frame::new()
        .fill(palette.inset)
        .stroke(Stroke::new(1.0_f32, palette.border))
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(
                egui::Label::new(
                    RichText::new(&entry.signature)
                        .monospace()
                        .color(palette.text_strong),
                )
                .wrap(),
            );
        });
    ui.add_space(12.0);
    if entry.doc.is_empty() {
        ui.label(RichText::new("No description yet.").color(palette.text_weak));
    } else {
        ui.add(egui::Label::new(doc_text(&entry.doc, palette)).wrap());
    }
    let name = entry.qualified_name();
    if let Some(example) = axcenity_script::api_example(&name) {
        ui.add_space(theme::space::L);
        widgets::section_label(ui, "Example");
        ui.add_space(4.0);
        docs::markdown::code_block(ui, index, "lua", example, palette);
        if let Some(tab) = insert_into {
            ui.add_space(4.0);
            if ui
                .add(widgets::button(
                    Role::Secondary,
                    Some(icons::TEXT_INDENT),
                    &format!("Insert into {tab}"),
                    palette,
                ))
                .on_hover_text("Put this example into the script at its cursor, as new lines")
                .clicked()
            {
                nav = Some(Nav::Insert(example));
            }
        }
    }
    if let Some(page) = read_more(ui, &name, palette) {
        nav = Some(Nav::Doc(page));
    }

    if entry.owner.is_empty() {
        // Classes inherit Instance's members, which are listed on its page.
        let inherits = entry.kind == ApiKind::Class && entry.name != "Instance";
        if let Some(instance) = top_level(entries, "Instance").filter(|_| inherits) {
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(RichText::new("It also has everything an").color(palette.text_secondary));
                if ui
                    .link(
                        RichText::new("Instance")
                            .monospace()
                            .color(palette.accent_text),
                    )
                    .clicked()
                {
                    nav = Some(Nav::Open(instance));
                }
                ui.label(RichText::new("has.").color(palette.text_secondary));
            });
        }
        for kind in MEMBERS {
            let members: Vec<usize> = (0..entries.len())
                .filter(|&i| entries[i].owner == entry.name && entries[i].kind == kind)
                .collect();
            if members.is_empty() {
                continue;
            }
            ui.add_space(theme::space::XL);
            widgets::section_label(ui, plural(kind));
            ui.add_space(4.0);
            if let Some(member) = rows(ui, entries, &members) {
                nav = Some(Nav::Open(member));
            }
        }
    }
    nav
}

/// "Read more": the documentation pages that name the entry, as links.
/// Returns the one clicked.
fn read_more(ui: &mut egui::Ui, name: &str, palette: &Palette) -> Option<usize> {
    let pages = docs::markdown::pages_mentioning(name);
    if pages.is_empty() {
        return None;
    }
    let library = docs::library();
    let mut clicked = None;
    ui.add_space(theme::space::L);
    widgets::section_label(ui, "Read more");
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        for &page in pages {
            let title = &library.pages[page].title;
            if ui
                .link(RichText::new(title).color(palette.accent_text))
                .on_hover_text("Open in the Documentation")
                .clicked()
            {
                clicked = Some(page);
            }
            ui.add_space(theme::space::M);
        }
    });
    clicked
}

/// A description with its `code` in the code font.
pub(super) fn doc_text(doc: &str, palette: &Palette) -> LayoutJob {
    let mut job = LayoutJob::default();
    // Odd pieces are between backticks.
    for (piece, text) in doc.split('`').enumerate() {
        let (font_id, color) = if piece % 2 == 1 {
            (
                FontId::monospace(type_scale::BODY - 0.5),
                palette.accent_text,
            )
        } else {
            (FontId::proportional(type_scale::BODY), palette.text)
        };
        job.append(
            text,
            0.0,
            TextFormat {
                font_id,
                color,
                ..Default::default()
            },
        );
    }
    job
}
