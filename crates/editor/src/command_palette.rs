//! The command palette (Ctrl+Shift+P) and Quick Open (Ctrl+P), unified
//! Phase 70: one searchable list over the window, ranked by a fuzzy match,
//! with the most recently run entries first while the query is empty. The
//! app makes the entries (`app/command_palette.rs`): every menu and ribbon
//! command, or the project's scripts, assets, levels and Instances.

use eframe::egui;

use crate::theme::{self, type_scale};

/// How many recent entries are remembered.
const RECENT_LIMIT: usize = 20;
/// How many rows the list shows at most.
const SHOWN_LIMIT: usize = 200;

/// One row of the palette.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// What it's called, as the search matches it.
    pub title: String,
    /// Where it comes from ("File", "Ribbon › Lights & Sky"), or a path;
    /// searched too, after the title.
    pub detail: String,
    /// Other names it goes by: what its menu item or ribbon button says,
    /// when that isn't the title. Searched like the title.
    pub aliases: Vec<String>,
    /// Its keyboard shortcut, as the menus write it.
    pub shortcut: Option<String>,
    /// Off: shown dimmed, with why in `disabled`.
    pub enabled: bool,
    pub disabled: String,
    /// Names it among recent entries, whatever its title says now (such as
    /// "Undo Move Pad").
    pub key: String,
    pub icon: &'static str,
}

impl Entry {
    pub fn new(
        key: impl Into<String>,
        title: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            title: title.into(),
            detail: detail.into(),
            aliases: Vec::new(),
            shortcut: None,
            enabled: true,
            disabled: String::new(),
            key: key.into(),
            icon: "",
        }
    }
}

/// Which list the palette is showing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Commands,
    QuickOpen,
}

/// The palette's window.
#[derive(Debug, Default)]
pub struct Palette {
    pub open: Option<Mode>,
    pub query: String,
    /// The highlighted row, among the ranked rows.
    selected: usize,
    /// The keys of recently run commands, newest first.
    recent: Vec<String>,
    /// The keys of recently opened Quick Open entries, newest first.
    recent_opened: Vec<String>,
    focus: bool,
}

impl Palette {
    /// Opens it on `mode`'s list with an empty query, or closes it if
    /// that list is showing.
    pub fn toggle(&mut self, mode: Mode) {
        if self.open == Some(mode) {
            self.open = None;
        } else {
            self.open = Some(mode);
            self.query.clear();
            self.selected = 0;
            self.focus = true;
        }
    }

    /// Notes that `key` was just run (or opened), so it leads the list.
    pub fn used(&mut self, mode: Mode, key: &str) {
        let recent = match mode {
            Mode::Commands => &mut self.recent,
            Mode::QuickOpen => &mut self.recent_opened,
        };
        recent.retain(|old| old != key);
        recent.insert(0, key.to_owned());
        recent.truncate(RECENT_LIMIT);
    }

    /// The recent command keys, one a line, to save between runs.
    pub fn recent_text(&self) -> String {
        self.recent.join("\n")
    }

    pub fn load_recent(&mut self, text: &str) {
        self.recent = text
            .lines()
            .filter(|line| !line.is_empty())
            .take(RECENT_LIMIT)
            .map(str::to_owned)
            .collect();
    }

    fn recent(&self, mode: Mode) -> &[String] {
        match mode {
            Mode::Commands => &self.recent,
            Mode::QuickOpen => &self.recent_opened,
        }
    }

    /// The rows for `query`, best first: see [`rank`].
    pub fn ranked(&self, mode: Mode, entries: &[Entry]) -> Vec<usize> {
        rank(entries, &self.query, self.recent(mode))
    }

    /// Draws the palette over `entries` and returns the entry picked (with
    /// Enter or a click). Escape or a click outside closes it.
    pub fn ui(&mut self, ctx: &egui::Context, entries: &[Entry]) -> Option<usize> {
        let mode = self.open?;
        let palette = theme::palette(ctx);
        let ranked = self.ranked(mode, entries);
        let (down, up, enter, escape) = ctx.input_mut(|input| {
            (
                input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                input.key_pressed(egui::Key::Enter),
                input.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
            )
        });
        if escape {
            self.open = None;
            return None;
        }
        if down && self.selected + 1 < ranked.len() {
            self.selected += 1;
        }
        if up {
            self.selected = self.selected.saturating_sub(1);
        }
        self.selected = self.selected.min(ranked.len().saturating_sub(1));

        let mut picked = None;
        let screen = ctx.screen_rect();
        let width = (screen.width() - 40.0).clamp(240.0, 560.0);
        let area = egui::Area::new(egui::Id::new("command palette"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(
                screen.center().x - width / 2.0,
                screen.top() + 60.0,
            ))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(width);
                    let hint = match mode {
                        Mode::Commands => "Run a command",
                        Mode::QuickOpen => "Open a script, asset, level or object",
                    };
                    let before = self.query.clone();
                    let field = ui.add(
                        egui::TextEdit::singleline(&mut self.query)
                            .hint_text(hint)
                            .desired_width(f32::INFINITY),
                    );
                    if std::mem::take(&mut self.focus) {
                        field.request_focus();
                    }
                    if self.query != before {
                        self.selected = 0;
                    }
                    if enter && let Some(&index) = ranked.get(self.selected) {
                        picked = Some(index);
                    }
                    ui.separator();
                    if ranked.is_empty() {
                        ui.weak("Nothing matches.");
                    }
                    egui::ScrollArea::vertical()
                        .max_height(360.0)
                        .show(ui, |ui| {
                            for (row, &index) in ranked.iter().take(SHOWN_LIMIT).enumerate() {
                                let entry = &entries[index];
                                let response = row_ui(ui, entry, row == self.selected, &palette);
                                if row == self.selected && (down || up) {
                                    response.scroll_to_me(None);
                                }
                                if response.clicked() {
                                    picked = Some(index);
                                }
                            }
                        });
                });
            });
        let clicked_outside = ctx.input(|input| input.pointer.any_pressed())
            && !area.response.rect.contains(
                ctx.input(|input| input.pointer.interact_pos())
                    .unwrap_or_default(),
            );
        if clicked_outside {
            self.open = None;
        }
        if let Some(index) = picked {
            if !entries[index].enabled {
                return None;
            }
            self.used(mode, &entries[index].key);
            self.open = None;
        }
        picked
    }
}

/// One row: the icon and title, the detail after it, the shortcut at the right.
fn row_ui(
    ui: &mut egui::Ui,
    entry: &Entry,
    selected: bool,
    palette: &theme::Palette,
) -> egui::Response {
    let height = 24.0;
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        egui::Sense::click(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::Button,
            entry.enabled,
            selected,
            &entry.title,
        )
    });
    if selected || response.hovered() {
        ui.painter().rect_filled(
            rect,
            4,
            if selected {
                palette.accent_soft
            } else {
                palette.control_hover
            },
        );
    }
    let strong = if entry.enabled {
        palette.text_strong
    } else {
        palette.text_muted
    };
    let font = egui::FontId::proportional(type_scale::BODY);
    let mut x = rect.left() + 8.0;
    if !entry.icon.is_empty() {
        ui.painter().text(
            egui::pos2(x, rect.center().y),
            egui::Align2::LEFT_CENTER,
            entry.icon,
            font.clone(),
            palette.text_weak,
        );
        x += 20.0;
    }
    let title = ui.painter().text(
        egui::pos2(x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        &entry.title,
        font,
        strong,
    );
    let small = egui::FontId::proportional(type_scale::CAPTION);
    let mut right = rect.right() - 8.0;
    if let Some(shortcut) = &entry.shortcut {
        let shown = ui.painter().text(
            egui::pos2(right, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            shortcut,
            small.clone(),
            palette.text_weak,
        );
        right = shown.left() - 12.0;
    }
    if !entry.detail.is_empty() && right > title.right() + 24.0 {
        let room = right - title.right() - 12.0;
        let galley = ui
            .painter()
            .layout(entry.detail.clone(), small, palette.text_muted, room);
        let mut job = (*galley.job).clone();
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        let galley = ui.fonts(|fonts| fonts.layout_job(job));
        ui.painter().galley(
            egui::pos2(
                title.right() + 12.0,
                rect.center().y - galley.size().y / 2.0,
            ),
            galley,
            palette.text_muted,
        );
    }
    let response = if entry.enabled || entry.disabled.is_empty() {
        response
    } else {
        response.on_hover_text(&entry.disabled)
    };
    if response.hovered() && entry.enabled {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

/// The entries that match `query`, best first. With no query, the recent
/// ones lead (newest first), then the rest in their order. Otherwise each
/// is scored by [`fuzzy_score`] on its title or aliases (or, scoring less, its detail
/// and title together), recent ones getting a little more.
pub fn rank(entries: &[Entry], query: &str, recent: &[String]) -> Vec<usize> {
    let recency = |entry: &Entry| recent.iter().position(|key| *key == entry.key);
    let query = query.trim();
    if query.is_empty() {
        let mut order: Vec<usize> = (0..entries.len()).collect();
        // A stable sort keeps the rest in the order they came.
        order.sort_by_key(|&index| recency(&entries[index]).unwrap_or(usize::MAX));
        return order;
    }
    let mut scored: Vec<(i32, usize)> = entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            let both = format!("{} {}", entry.detail, entry.title);
            let named = std::iter::once(&entry.title)
                .chain(&entry.aliases)
                .filter_map(|name| fuzzy_score(query, name))
                .max();
            let score = named.or_else(|| fuzzy_score(query, &both).map(|score| score - 40))?;
            let bonus = recency(entry).map_or(0, |at| 30 - at.min(29) as i32);
            Some((score + bonus, index))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, index)| index).collect()
}

/// How well `query` matches `text`, ignoring case: `None` unless every
/// character of the query is in the text, in order. Letters that start a
/// word, follow the last match, or start the text score more; gaps cost.
pub fn fuzzy_score(query: &str, text: &str) -> Option<i32> {
    let text: Vec<char> = text.chars().collect();
    let lower: Vec<char> = text.iter().flat_map(|c| c.to_lowercase()).collect();
    // `to_lowercase` keeps the count for the text this matches (titles and paths).
    if lower.len() != text.len() {
        return None;
    }
    let mut score = 0;
    let mut at = 0;
    let mut last: Option<usize> = None;
    for wanted in query.chars().flat_map(char::to_lowercase) {
        if wanted == ' ' {
            continue;
        }
        let found = (at..lower.len()).find(|&i| lower[i] == wanted)?;
        let starts_word = found == 0
            || !text[found - 1].is_alphanumeric()
            || (text[found].is_uppercase() && text[found - 1].is_lowercase());
        score += 10;
        if starts_word {
            score += 15;
        }
        if found == 0 {
            score += 10;
        }
        match last {
            Some(previous) if found == previous + 1 => score += 12,
            Some(previous) => score -= (found - previous - 1).min(10) as i32,
            None => score -= found.min(15) as i32,
        }
        last = Some(found);
        at = found + 1;
    }
    // Shorter texts win ties.
    Some(score - (text.len() as i32 / 8))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(titles: &[&str]) -> Vec<Entry> {
        titles
            .iter()
            .map(|title| Entry::new(*title, *title, ""))
            .collect()
    }

    #[test]
    fn fuzzy_matching_needs_every_letter_in_order() {
        assert!(fuzzy_score("ckas", "Check Assets").is_some());
        assert!(fuzzy_score("sack", "Check Assets").is_none());
        assert!(fuzzy_score("", "Anything").is_some());
    }

    #[test]
    fn word_starts_and_runs_rank_first() {
        let list = entries(&["Insert Block", "Bake Probes", "Block"]);
        let ranked = rank(&list, "blo", &[]);
        assert_eq!(list[ranked[0]].title, "Block");
        assert_eq!(list[ranked[1]].title, "Insert Block");
        assert_eq!(ranked.len(), 2);
        let list = entries(&["Save", "Version History…", "Check Assets"]);
        assert_eq!(list[rank(&list, "ca", &[])[0]].title, "Check Assets");
    }

    #[test]
    fn recent_entries_lead_an_empty_query_newest_first() {
        let list = entries(&["Save", "Play", "Stop", "Undo"]);
        let mut palette = Palette::default();
        palette.used(Mode::Commands, "Stop");
        palette.used(Mode::Commands, "Undo");
        let ranked = palette.ranked(Mode::Commands, &list);
        let titles: Vec<&str> = ranked.iter().map(|&i| list[i].title.as_str()).collect();
        assert_eq!(titles, ["Undo", "Stop", "Save", "Play"]);
        // And survive saving.
        let mut again = Palette::default();
        again.load_recent(&palette.recent_text());
        assert_eq!(again.ranked(Mode::Commands, &list), ranked);
    }

    #[test]
    fn the_detail_matches_too_but_less_than_the_title() {
        let list = vec![
            Entry::new("a", "Point Light", "Ribbon › Lights & Sky"),
            Entry::new("b", "Lights", "View"),
        ];
        let ranked = rank(&list, "lights", &[]);
        assert_eq!(ranked, [1, 0]);
    }
}
