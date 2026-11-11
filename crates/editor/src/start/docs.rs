//! The Documentation page: the technical reference for people making games
//! with Axcenity, by category, searchable, with links between pages and into
//! the API Reference. The pages are the Markdown files in the repository's
//! `documentation/` folder (built in by `build.rs`), which the GitBook site
//! publishes too. Their `Learn` section is the Learn page's learning path;
//! the other sections are this page. How to write and check them:
//! `docs/editor/learn-and-docs.md`.

pub(crate) mod library;
pub(crate) mod markdown;
#[cfg(test)]
mod tests;

use std::sync::OnceLock;

pub(crate) use library::Library;
pub(crate) use markdown::DocAction;

use super::*;

mod files {
    include!(concat!(env!("OUT_DIR"), "/doc_files.rs"));
}

/// How the `SUMMARY.md` sections the Learn page shows as its learning path
/// start: `## Learn: The basics`.
pub(crate) const LEARN_SECTION: &str = "Learn";

/// True for a section of the learning path.
pub(crate) fn is_learn(section: &library::Section) -> bool {
    section.title.starts_with(LEARN_SECTION)
}
/// The page shown before another is picked.
const HOME: &str = "README.md";
/// Below this width the categories fold into a Contents list above the page.
const TWO_COLUMNS: f32 = 720.0;
const NAV_WIDTH: f32 = 210.0;
/// How many search results are listed.
const MAX_HITS: usize = 40;

/// Every built-in page, read once.
pub(crate) fn library() -> &'static Library {
    static LIBRARY: OnceLock<Library> = OnceLock::new();
    LIBRARY.get_or_init(|| Library::load(files::DOC_FILES))
}

/// The page's state: the open page and the search box.
#[derive(Default)]
pub(super) struct Docs {
    /// An index into the library's pages; `None` is the home page.
    pub(super) open: Option<usize>,
    pub(super) query: String,
    /// A heading to bring into view on the next frame.
    scroll_to: Option<String>,
    /// Set when a page opens, so it's shown from the top.
    scroll_top: bool,
}

impl Docs {
    /// Shows `index`, at the heading `anchor` if one is given.
    pub(super) fn open(&mut self, index: usize, anchor: Option<String>) {
        self.open = Some(index);
        self.query.clear();
        self.scroll_top = anchor.is_none();
        self.scroll_to = anchor;
    }

    /// Draws the page. Returns what it can't do itself: an API Reference
    /// entry, a sample, or a Learn chapter to open.
    pub(super) fn show(&mut self, ui: &mut egui::Ui) -> Option<DocAction> {
        let library = library();
        let palette = theme::palette(ui.ctx());
        let heading = ui.label(
            RichText::new("Documentation")
                .family(theme::display_family())
                .size(type_scale::DISPLAY)
                .extra_letter_spacing(theme::DISPLAY_TRACKING)
                .color(palette.text_strong),
        );
        if std::mem::take(&mut self.scroll_top) {
            heading.scroll_to_me(Some(egui::Align::TOP));
        }
        ui.add(
            egui::Label::new(
                RichText::new(
                    "How Axcenity works and what each system can do, for reference while \
                     you build. Search it, or pick a category.",
                )
                .color(palette.text_secondary),
            )
            .wrap(),
        );
        ui.add_space(theme::space::L);
        let width = (ui.available_width() - widgets::SEARCH_FIELD_MARGINS).min(420.0);
        widgets::search_field(ui, &mut self.query, "Search the documentation", width);
        ui.add_space(theme::space::L);

        let action = if !self.query.trim().is_empty() {
            self.results(ui, library, &palette)
        } else if ui.available_width() >= TWO_COLUMNS {
            ui.horizontal_top(|ui| {
                let nav = ui
                    .allocate_ui_with_layout(
                        vec2(NAV_WIDTH, 0.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_width(NAV_WIDTH);
                            self.nav(ui, library, &palette)
                        },
                    )
                    .inner;
                ui.add_space(theme::space::XL);
                let page = ui.vertical(|ui| self.page(ui, library, &palette)).inner;
                nav.or(page)
            })
            .inner
        } else {
            let nav = egui::CollapsingHeader::new(
                RichText::new(theme::with_icon(icons::LIST, "Contents")).color(palette.text),
            )
            .id_salt("docs contents")
            .show(ui, |ui| self.nav(ui, library, &palette))
            .body_returned
            .flatten();
            ui.add_space(theme::space::M);
            nav.or_else(|| self.page(ui, library, &palette))
        };
        self.follow(library, action)
    }

    /// Does what it can of `action` here, and returns the rest.
    fn follow(&mut self, library: &Library, action: Option<DocAction>) -> Option<DocAction> {
        match action? {
            DocAction::Open(index, anchor) => {
                let learn = library
                    .sections
                    .iter()
                    .any(|section| is_learn(section) && section.pages.contains(&index));
                if learn {
                    return Some(DocAction::Open(index, anchor));
                }
                self.open(index, anchor);
                None
            }
            other => Some(other),
        }
    }

    /// The categories and their pages, with the open page's headings.
    fn nav(
        &mut self,
        ui: &mut egui::Ui,
        library: &Library,
        palette: &Palette,
    ) -> Option<DocAction> {
        let mut action = None;
        let home = library.index_of(HOME);
        if let Some(home) = home
            && nav_row(
                ui,
                icons::HOUSE,
                "Overview",
                self.shown(library) == Some(home),
                palette,
            )
            .clicked()
        {
            action = Some(DocAction::Open(home, None));
        }
        for section in &library.sections {
            if is_learn(section) || section.title.is_empty() {
                continue;
            }
            ui.add_space(theme::space::M);
            widgets::section_label(ui, &section.title);
            ui.add_space(theme::space::XS);
            for &index in &section.pages {
                let page = &library.pages[index];
                let open = self.shown(library) == Some(index);
                let mut row = nav_row(ui, markdown::icon(&page.icon), &page.title, open, palette);
                if !page.description.is_empty() {
                    row = row.on_hover_text(&page.description);
                }
                if row.clicked() {
                    action = Some(DocAction::Open(index, None));
                }
                if open {
                    for (level, text, anchor) in page.headings() {
                        if level != 2 {
                            continue;
                        }
                        ui.horizontal(|ui| {
                            ui.add_space(28.0);
                            let link = ui.add(
                                egui::Label::new(
                                    RichText::new(text)
                                        .size(type_scale::SMALL)
                                        .color(palette.text_secondary),
                                )
                                .truncate()
                                .sense(Sense::click()),
                            );
                            if link.clicked() {
                                action = Some(DocAction::Open(index, Some(anchor.to_owned())));
                            }
                            link.on_hover_cursor(egui::CursorIcon::PointingHand);
                        });
                    }
                }
            }
            for (title, _) in &section.elsewhere {
                let row = nav_row(ui, icons::BOOKS, title, false, palette)
                    .on_hover_text("Every class, property, event and function, searchable");
                if row.clicked() {
                    action = Some(DocAction::LookUp(String::new()));
                }
            }
        }
        action
    }

    /// The page that's shown: the open one or the home page.
    fn shown(&self, library: &Library) -> Option<usize> {
        self.open.or_else(|| library.index_of(HOME))
    }

    /// The open page, then the pages before and after it.
    fn page(
        &mut self,
        ui: &mut egui::Ui,
        library: &Library,
        palette: &Palette,
    ) -> Option<DocAction> {
        let index = self.shown(library)?;
        let scroll_to = self.scroll_to.take();
        let mut action = markdown::show_page(ui, library, index, scroll_to.as_deref());
        let home = library.index_of(HOME);
        let order: Vec<usize> = home
            .into_iter()
            .chain(
                library
                    .sections
                    .iter()
                    .filter(|section| !is_learn(section))
                    .flat_map(|section| section.pages.iter().copied())
                    .filter(|&page| Some(page) != home),
            )
            .collect();
        let at = order.iter().position(|&other| other == index);
        ui.add_space(theme::space::XL + theme::space::XS);
        widgets::divider(ui);
        ui.add_space(theme::space::M);
        ui.horizontal_wrapped(|ui| {
            if let Some(before) = at.and_then(|at| at.checked_sub(1)).map(|at| order[at]) {
                let label = format!("Back: {}", library.pages[before].title);
                if ui
                    .add(widgets::button(
                        Role::Secondary,
                        Some(icons::ARROW_LEFT),
                        &label,
                        palette,
                    ))
                    .clicked()
                {
                    action = Some(DocAction::Open(before, None));
                }
            }
            if let Some(next) = at.and_then(|at| order.get(at + 1)).copied() {
                let label = format!("Next: {}", library.pages[next].title);
                if ui
                    .add(widgets::button(Role::Primary, None, &label, palette))
                    .clicked()
                {
                    action = Some(DocAction::Open(next, None));
                }
            }
        });
        action
    }

    /// What matches the search box: pages and headings, then a way into the
    /// API Reference with the same words.
    fn results(
        &mut self,
        ui: &mut egui::Ui,
        library: &Library,
        palette: &Palette,
    ) -> Option<DocAction> {
        let mut action = None;
        let hits = library.search(&self.query);
        let api = super::reference::search_entries(axcenity_script_entries(), &self.query).len();
        if hits.is_empty() {
            ui.label(RichText::new("No page mentions that.").color(palette.text_secondary));
        }
        for hit in hits.iter().take(MAX_HITS) {
            let page = &library.pages[hit.page];
            let title = match &hit.anchor {
                Some(_) => format!("{}  ›  {}", page.title, hit.heading),
                None => page.title.clone(),
            };
            let response = egui::Frame::new()
                .inner_margin(theme::space::margin(
                    theme::space::S,
                    theme::space::XS + 2.0,
                ))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(markdown::icon(&page.icon)).color(palette.accent_text),
                        );
                        ui.label(
                            RichText::new(title)
                                .family(theme::medium_family())
                                .color(palette.text_strong),
                        );
                    });
                    ui.add(
                        egui::Label::new(
                            RichText::new(&hit.snippet)
                                .size(type_scale::SMALL)
                                .color(palette.text_secondary),
                        )
                        .wrap(),
                    );
                })
                .response
                .interact(Sense::click());
            widgets::paint_hover(ui, response.rect, &response, false, theme::radius::CONTROL);
            if response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                action = Some(DocAction::Open(hit.page, hit.anchor.clone()));
            }
        }
        if api > 0 {
            ui.add_space(theme::space::M);
            let label = format!(
                "{api} match{} in the API Reference",
                if api == 1 { "" } else { "es" }
            );
            if ui
                .add(widgets::button(
                    Role::Secondary,
                    Some(icons::BOOKS),
                    &label,
                    palette,
                ))
                .clicked()
            {
                action = Some(DocAction::LookUp(self.query.trim().to_owned()));
            }
        }
        action
    }
}

/// The API Reference's entries, read once.
fn axcenity_script_entries() -> &'static [axcenity_script::ApiEntry] {
    static ENTRIES: OnceLock<Vec<axcenity_script::ApiEntry>> = OnceLock::new();
    ENTRIES.get_or_init(axcenity_script::api_reference)
}

/// A row of the categories list: smaller than the sidebar's.
fn nav_row(
    ui: &mut egui::Ui,
    icon: &str,
    title: &str,
    selected: bool,
    palette: &Palette,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, title)
    });
    let t = theme::ease(
        ui.ctx(),
        response.id.with("hover"),
        response.hovered() || selected,
    );
    if t > 0.0 {
        let fill = if selected {
            palette.control_active
        } else {
            palette.control_hover
        };
        ui.painter()
            .rect_filled(rect, theme::radius::CONTROL, fill.gamma_multiply(t));
    }
    let color = if selected || response.hovered() {
        palette.text_strong
    } else {
        palette.text_secondary
    };
    ui.painter().text(
        rect.left_center() + vec2(14.0, 0.0),
        egui::Align2::CENTER_CENTER,
        icon,
        FontId::proportional(theme::size::ICON_SMALL),
        if selected { palette.accent } else { color },
    );
    let galley = widgets::elided(
        ui,
        title.to_owned(),
        FontId::new(type_scale::BODY - 0.5, theme::medium_family()),
        color,
        rect.width() - 36.0,
        1,
    );
    ui.painter().galley(
        egui::pos2(rect.left() + 28.0, rect.center().y - galley.size().y / 2.0),
        galley,
        color,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}
