//! Universal search (Ctrl+K), the command palette (Ctrl+Shift+P) and Quick
//! Open (Ctrl+P): one searchable window over the whole editor. Its rows come
//! from search providers (`app/search.rs`): commands and tools, scene
//! objects, assets, scripts, levels, settings and the selected object's
//! properties, each row tagged with its [`Category`]. Ranking is a fuzzy
//! match over each row's title, aliases (synonyms included), where it lives
//! and what it does, done once per change of the query or the index, not
//! every frame.

use eframe::egui;

use crate::theme::{self, icons, type_scale};

/// How many recent entries are remembered.
const RECENT_LIMIT: usize = 20;
/// How many rows the list shows at most.
const SHOWN_LIMIT: usize = 200;
/// What a query starting with this searches: commands and tools only.
pub const COMMAND_PREFIX: char = '>';

/// What kind of thing a row is, for the chips that narrow the list and the
/// label at its right.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Category {
    /// Menu and ribbon commands, and plugin toolbar buttons.
    Command,
    /// Editor tools: the transform tools, the Terrain Studio brushes, Model
    /// Studio, the Constraint tool, the ribbon tabs.
    Tool,
    /// Instances in the world shown (the Explorer's rows).
    Object,
    /// Files in the project's `assets` folder and global assets.
    Asset,
    Script,
    Level,
    /// A row of the Settings window.
    Setting,
    /// A property of what's selected.
    Property,
}

impl Category {
    pub const ALL: [Category; 8] = [
        Category::Command,
        Category::Tool,
        Category::Object,
        Category::Asset,
        Category::Script,
        Category::Level,
        Category::Setting,
        Category::Property,
    ];

    /// Its chip's label.
    pub fn label(self) -> &'static str {
        match self {
            Category::Command => "Commands",
            Category::Tool => "Tools",
            Category::Object => "Objects",
            Category::Asset => "Assets",
            Category::Script => "Scripts",
            Category::Level => "Levels",
            Category::Setting => "Settings",
            Category::Property => "Properties",
        }
    }

    /// What a row of it says at the right in the All list.
    pub fn singular(self) -> &'static str {
        match self {
            Category::Command => "Command",
            Category::Tool => "Tool",
            Category::Object => "Object",
            Category::Asset => "Asset",
            Category::Script => "Script",
            Category::Level => "Level",
            Category::Setting => "Setting",
            Category::Property => "Property",
        }
    }

    /// A little extra for its rows when several categories match equally,
    /// so a command beats an object that happens to share its name: there
    /// are many more objects than commands.
    fn weight(self) -> i32 {
        match self {
            Category::Command | Category::Tool => 6,
            Category::Setting => 4,
            Category::Script => 3,
            Category::Asset | Category::Level => 2,
            Category::Object | Category::Property => 0,
        }
    }

    /// Whether a `>` query searches it.
    fn is_command(self) -> bool {
        matches!(self, Category::Command | Category::Tool)
    }
}

/// One row of the palette.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// What it's called, as the search matches it.
    pub title: String,
    /// Where it comes from ("File", "Ribbon › Lights & Sky"), or a path;
    /// searched too, after the title.
    pub detail: String,
    /// Other names it goes by: what its menu item or ribbon button says,
    /// when that isn't the title, and synonyms ("remove" for Delete).
    /// Searched like the title.
    pub aliases: Vec<String>,
    /// What it does, shown under the list while it's highlighted. Searched
    /// too, scoring least.
    pub description: String,
    /// Its keyboard shortcut, as the menus write it.
    pub shortcut: Option<String>,
    /// Off: shown dimmed, with why in `disabled`.
    pub enabled: bool,
    pub disabled: String,
    /// Names it among recent entries, whatever its title says now (such as
    /// "Undo Move Pad").
    pub key: String,
    pub icon: &'static str,
    pub category: Category,
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
            description: String::new(),
            shortcut: None,
            enabled: true,
            disabled: String::new(),
            key: key.into(),
            icon: "",
            category: Category::Command,
        }
    }

    pub fn in_category(mut self, category: Category) -> Self {
        self.category = category;
        self
    }
}

/// Where a ranked row is: its bucket (one per provider) and its place in it.
pub type Hit = (usize, usize);

/// The last ranking, kept until the query, the chip or the index change.
#[derive(Debug, Default)]
struct RankCache {
    query: String,
    scope: Option<Category>,
    generation: u64,
    ranked: Vec<Hit>,
    /// How many rows match in each category, for the chips.
    counts: Vec<(Category, usize)>,
}

/// The palette's window.
#[derive(Debug, Default)]
pub struct Palette {
    pub open: bool,
    pub query: String,
    /// The chip picked: only rows of this category show. `None` is All.
    pub scope: Option<Category>,
    /// The highlighted row, among the ranked rows.
    selected: usize,
    /// The keys of recently run or opened entries, newest first.
    recent: Vec<String>,
    focus: bool,
    cache: Option<RankCache>,
    /// Bumped by [`Palette::used`], so the ranking is made again.
    recent_version: u64,
}

impl Palette {
    /// Opens it on everything (Ctrl+K, Ctrl+P), or closes it if it's open
    /// that way.
    pub fn toggle(&mut self) {
        self.toggle_with("");
    }

    /// Opens it on commands and tools (Ctrl+Shift+P): the query starts
    /// with `>`, which a backspace takes away to search everything.
    pub fn toggle_commands(&mut self) {
        self.toggle_with(&format!("{COMMAND_PREFIX} "));
    }

    fn toggle_with(&mut self, query: &str) {
        let commands = query.starts_with(COMMAND_PREFIX);
        if self.open && self.query.starts_with(COMMAND_PREFIX) == commands {
            self.open = false;
        } else {
            self.open = true;
            self.query = query.to_owned();
            self.scope = None;
            self.selected = 0;
            self.focus = true;
        }
    }

    /// Notes that `key` was just run or opened, so it leads the list.
    pub fn used(&mut self, key: &str) {
        self.recent.retain(|old| old != key);
        self.recent.insert(0, key.to_owned());
        self.recent.truncate(RECENT_LIMIT);
        self.recent_version += 1;
    }

    /// The recent keys, one a line, to save between runs.
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
        self.recent_version += 1;
    }

    /// The rows for the query and chip, best first ([`rank`]). Ranked
    /// again only when the query, the chip, the recent list or
    /// `generation` (which the index bumps whenever a bucket changes)
    /// differ from the last time.
    pub fn ranked(&mut self, buckets: &[&[Entry]], generation: u64) -> &[Hit] {
        let generation = generation.wrapping_add(self.recent_version << 32);
        let fresh = self.cache.as_ref().is_some_and(|cache| {
            cache.query == self.query && cache.scope == self.scope && cache.generation == generation
        });
        if !fresh {
            let (ranked, counts) = rank_counted(buckets, &self.query, self.scope, &self.recent);
            self.cache = Some(RankCache {
                query: self.query.clone(),
                scope: self.scope,
                generation,
                ranked,
                counts,
            });
        }
        &self.cache.as_ref().expect("just ranked").ranked
    }

    /// How many rows match the query in each category (for the chips).
    fn counts(&self) -> &[(Category, usize)] {
        self.cache.as_ref().map_or(&[], |cache| &cache.counts)
    }

    /// Draws the palette over `buckets` and returns the row picked (with
    /// Enter or a click). Escape or a click outside closes it. Tab and
    /// Shift+Tab step through the chips that have matches.
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        buckets: &[&[Entry]],
        generation: u64,
    ) -> Option<Hit> {
        if !self.open {
            return None;
        }
        let palette = theme::palette(ctx);
        let (down, up, enter, escape, tab, back_tab) = ctx.input_mut(|input| {
            // Shift+Tab first: a plain Tab check takes it too (egui
            // matches keys ignoring Shift unless it's asked for).
            let back_tab = input.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab);
            (
                input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                input.key_pressed(egui::Key::Enter),
                input.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
                input.consume_key(egui::Modifiers::NONE, egui::Key::Tab),
                back_tab,
            )
        });
        if escape {
            self.open = false;
            return None;
        }
        // Rank first for this frame's chips; Tab moves among them.
        self.ranked(buckets, generation);
        if tab || back_tab {
            self.step_scope(if tab { 1 } else { -1 });
            self.selected = 0;
            self.focus = true;
        }
        let ranked = self.ranked(buckets, generation).to_vec();
        if down && self.selected + 1 < ranked.len().min(SHOWN_LIMIT) {
            self.selected += 1;
        }
        if up {
            self.selected = self.selected.saturating_sub(1);
        }
        self.selected = self.selected.min(ranked.len().saturating_sub(1));

        let mut picked = None;
        let screen = ctx.screen_rect();
        let width = (screen.width() - 40.0).clamp(240.0, 640.0);
        let area = egui::Area::new(egui::Id::new("command palette"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(
                screen.center().x - width / 2.0,
                screen.top() + 60.0,
            ))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(width);
                    let hint = if self.query.starts_with(COMMAND_PREFIX) {
                        "Run a command or pick a tool"
                    } else {
                        "Search commands, objects, assets, scripts and settings"
                    };
                    let before = self.query.clone();
                    let field = ui.add(
                        egui::TextEdit::singleline(&mut self.query)
                            .hint_text(hint)
                            .desired_width(f32::INFINITY),
                    );
                    if std::mem::take(&mut self.focus) {
                        field.request_focus();
                        // Keep a `>` the caret is after.
                        if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), field.id) {
                            let end = egui::text::CCursor::new(self.query.chars().count());
                            state
                                .cursor
                                .set_char_range(Some(egui::text::CCursorRange::one(end)));
                            state.store(ui.ctx(), field.id);
                        }
                    }
                    if self.query != before {
                        self.selected = 0;
                    }
                    self.chips_ui(ui);
                    if enter && let Some(&hit) = ranked.get(self.selected) {
                        picked = Some(hit);
                    }
                    ui.separator();
                    if ranked.is_empty() {
                        ui.weak("Nothing matches.");
                    }
                    let mut highlighted = None;
                    egui::ScrollArea::vertical()
                        .max_height(360.0)
                        .show(ui, |ui| {
                            for (row, &(bucket, index)) in
                                ranked.iter().take(SHOWN_LIMIT).enumerate()
                            {
                                let entry = &buckets[bucket][index];
                                let selected = row == self.selected;
                                let response =
                                    row_ui(ui, entry, selected, self.scope.is_none(), &palette);
                                if selected {
                                    highlighted = Some(entry);
                                    if down || up {
                                        response.scroll_to_me(None);
                                    }
                                }
                                if response.clicked() {
                                    picked = Some((bucket, index));
                                }
                            }
                        });
                    footer_ui(ui, highlighted, &palette);
                });
            });
        let clicked_outside = ctx.input(|input| input.pointer.any_pressed())
            && !area.response.rect.contains(
                ctx.input(|input| input.pointer.interact_pos())
                    .unwrap_or_default(),
            );
        if clicked_outside {
            self.open = false;
        }
        if let Some((bucket, index)) = picked {
            let entry = &buckets[bucket][index];
            if !entry.enabled {
                return None;
            }
            self.used(&entry.key.clone());
            self.open = false;
        }
        picked
    }

    /// Moves the chip `step` places among All and the categories with
    /// matches, wrapping round.
    fn step_scope(&mut self, step: i32) {
        let mut stops: Vec<Option<Category>> = vec![None];
        stops.extend(
            self.counts()
                .iter()
                .filter(|(_, count)| *count > 0)
                .map(|(category, _)| Some(*category)),
        );
        if let Some(scope) = self.scope
            && !stops.contains(&Some(scope))
        {
            stops.push(Some(scope));
        }
        let at = stops
            .iter()
            .position(|stop| *stop == self.scope)
            .unwrap_or(0) as i32;
        let next = (at + step).rem_euclid(stops.len() as i32) as usize;
        self.scope = stops[next];
    }

    /// All, then a chip for each category with matches (and the picked one).
    fn chips_ui(&mut self, ui: &mut egui::Ui) {
        let counts = self.counts().to_vec();
        let total: usize = counts.iter().map(|(_, count)| count).sum();
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
            let all = if self.scope.is_none() {
                format!("All {total}")
            } else {
                "All".to_owned()
            };
            if crate::widgets::chip(ui, &all, self.scope.is_none())
                .on_hover_text("Search every category (Tab and Shift+Tab step through these)")
                .clicked()
            {
                self.scope = None;
                self.selected = 0;
                self.focus = true;
            }
            for (category, count) in counts {
                if count == 0 && self.scope != Some(category) {
                    continue;
                }
                let picked = self.scope == Some(category);
                let label = format!("{} {count}", category.label());
                if crate::widgets::chip(ui, &label, picked).clicked() {
                    self.scope = if picked { None } else { Some(category) };
                    self.selected = 0;
                    self.focus = true;
                }
            }
        });
    }
}

/// One row: the icon and title, the detail after it, then at the right
/// the category (in the All list) and the shortcut.
fn row_ui(
    ui: &mut egui::Ui,
    entry: &Entry,
    selected: bool,
    show_category: bool,
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
            theme::radius::CONTROL,
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
    if show_category {
        let shown = ui.painter().text(
            egui::pos2(right, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            entry.category.singular(),
            small.clone(),
            palette.text_muted,
        );
        right = shown.left() - 12.0;
    }
    // The title, cut with "…" rather than running under what's at the right.
    let title_room = (right - x).max(24.0);
    let title = one_line(ui, &entry.title, font, strong, title_room);
    let title_rect = egui::Rect::from_min_size(
        egui::pos2(x, rect.center().y - title.size().y / 2.0),
        title.size(),
    );
    ui.painter().galley(title_rect.min, title, strong);
    if !entry.detail.is_empty() && right > title_rect.right() + 24.0 {
        let room = right - title_rect.right() - 12.0;
        let galley = one_line(ui, &entry.detail, small, palette.text_weak, room);
        ui.painter().galley(
            egui::pos2(
                title_rect.right() + 12.0,
                rect.center().y - galley.size().y / 2.0,
            ),
            galley,
            palette.text_weak,
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

/// `text` on one line at most `width` wide, ending in "…" if it's cut.
fn one_line(
    ui: &egui::Ui,
    text: &str,
    font: egui::FontId,
    color: egui::Color32,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple(text.to_owned(), font, color, width);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    ui.fonts(|fonts| fonts.layout_job(job))
}

/// Under the list: what the highlighted row does (or why it's off), then
/// the keys.
fn footer_ui(ui: &mut egui::Ui, highlighted: Option<&Entry>, palette: &theme::Palette) {
    ui.separator();
    if let Some(entry) = highlighted {
        let text = if !entry.enabled && !entry.disabled.is_empty() {
            Some((icons::PROHIBIT, entry.disabled.as_str(), palette.warning))
        } else if !entry.description.is_empty() {
            Some((
                icons::INFO,
                entry.description.as_str(),
                palette.text_secondary,
            ))
        } else {
            None
        };
        if let Some((icon, text, color)) = text {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(theme::with_icon(icon, text))
                        .size(type_scale::CAPTION)
                        .color(color),
                )
                .wrap(),
            );
        }
    }
    ui.label(
        egui::RichText::new(
            "↑↓ to move · Enter to open · Tab for categories · > for commands · Esc to close",
        )
        .size(type_scale::CAPTION)
        .color(palette.text_muted),
    );
}

/// The rows that match `query` in `buckets`, best first: see
/// [`rank_counted`].
#[cfg(test)]
pub fn rank(
    buckets: &[&[Entry]],
    query: &str,
    scope: Option<Category>,
    recent: &[String],
) -> Vec<Hit> {
    rank_counted(buckets, query, scope, recent).0
}

/// The rows that match `query`, best first, and how many match in each
/// category whatever the chip. A query starting with `>` searches only
/// commands and tools; `scope` narrows it to one category.
///
/// With no query, the recent rows lead (newest first), then the rest in
/// their order. Otherwise each row is scored by [`fuzzy_score`] on its
/// title or aliases or, scoring less, on its detail and title together and
/// then on what it does; recent rows and some categories
/// ([`Category::weight`]) get a little more.
fn rank_counted(
    buckets: &[&[Entry]],
    query: &str,
    scope: Option<Category>,
    recent: &[String],
) -> (Vec<Hit>, Vec<(Category, usize)>) {
    let recency = |entry: &Entry| recent.iter().position(|key| *key == entry.key);
    let mut query = query.trim();
    let commands_only = query.starts_with(COMMAND_PREFIX);
    if commands_only {
        query = query[COMMAND_PREFIX.len_utf8()..].trim();
    }
    let wanted = |entry: &Entry| !commands_only || entry.category.is_command();
    let rows = buckets.iter().enumerate().flat_map(|(bucket, entries)| {
        entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| wanted(entry))
            .map(move |(index, entry)| ((bucket, index), entry))
    });
    let mut counts: Vec<(Category, usize)> = Category::ALL
        .iter()
        .map(|&category| (category, 0))
        .collect();
    let mut count = |category: Category| {
        if let Some(slot) = counts.iter_mut().find(|(c, _)| *c == category) {
            slot.1 += 1;
        }
    };
    if query.is_empty() {
        let mut order: Vec<(usize, Hit)> = Vec::new();
        for (hit, entry) in rows {
            count(entry.category);
            if scope.is_none_or(|scope| scope == entry.category) {
                order.push((recency(entry).unwrap_or(usize::MAX), hit));
            }
        }
        // A stable sort keeps the rest in the order they came.
        order.sort_by_key(|(recent, _)| *recent);
        return (order.into_iter().map(|(_, hit)| hit).collect(), counts);
    }
    let lower_query = query.to_lowercase();
    let describe = lower_query.chars().filter(|c| !c.is_whitespace()).count() >= 3;
    let mut scored: Vec<(i32, Hit)> = Vec::new();
    for (hit, entry) in rows {
        let named = std::iter::once(&entry.title)
            .chain(&entry.aliases)
            .filter_map(|name| Some(fuzzy_score(query, name)? + whole_bonus(query, name)))
            .max();
        let score = named
            .or_else(|| {
                let both = format!("{} {}", entry.detail, entry.title);
                fuzzy_score(query, &both).map(|score| score - 40)
            })
            .or_else(|| {
                // What it does: only whole words of the query, in it.
                (describe && words_in(&lower_query, &entry.description)).then_some(-20)
            });
        let Some(score) = score else {
            continue;
        };
        count(entry.category);
        if scope.is_some_and(|scope| scope != entry.category) {
            continue;
        }
        let bonus = recency(entry).map_or(0, |at| 30 - at.min(29) as i32);
        scored.push((score + bonus + entry.category.weight(), hit));
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    (scored.into_iter().map(|(_, hit)| hit).collect(), counts)
}

/// More for a name that is the query (most if the case matches too), and
/// some for one that starts with it, so typing an object's whole name
/// finds it first.
fn whole_bonus(query: &str, name: &str) -> i32 {
    if name == query {
        return 45;
    }
    let (name, query) = (name.to_lowercase(), query.to_lowercase());
    if name == query {
        40
    } else if name.starts_with(&query) {
        15
    } else {
        0
    }
}

/// Whether every word of `query` (already lower case) is in `text`,
/// ignoring case.
fn words_in(query: &str, text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    let text = text.to_lowercase();
    query.split_whitespace().all(|word| text.contains(word))
}

/// How well `query` matches `text`, ignoring case: `None` unless every
/// character of the query is in the text, in order. Letters that start a
/// word, follow the last match, or start the text score more; gaps cost.
pub fn fuzzy_score(query: &str, text: &str) -> Option<i32> {
    // Most titles and paths are ASCII: match their bytes without copying.
    if text.is_ascii() && query.is_ascii() {
        let bytes = text.as_bytes();
        return score_chars(
            query.bytes().map(char::from),
            bytes.len(),
            |i| char::from(bytes[i]),
            |i| char::from(bytes[i].to_ascii_lowercase()),
        );
    }
    let text: Vec<char> = text.chars().collect();
    let lower: Vec<char> = text.iter().flat_map(|c| c.to_lowercase()).collect();
    // `to_lowercase` keeps the count for the text this matches (titles and paths).
    if lower.len() != text.len() {
        return None;
    }
    score_chars(query.chars(), text.len(), |i| text[i], |i| lower[i])
}

/// [`fuzzy_score`] over a text `len` characters long, read by `at` (as it
/// is) and `lower` (lower case).
fn score_chars(
    query: impl Iterator<Item = char>,
    len: usize,
    at: impl Fn(usize) -> char,
    lower: impl Fn(usize) -> char,
) -> Option<i32> {
    let mut score = 0;
    let mut from = 0;
    let mut last: Option<usize> = None;
    for wanted in query.flat_map(char::to_lowercase) {
        if wanted == ' ' {
            continue;
        }
        let found = (from..len).find(|&i| lower(i) == wanted)?;
        let starts_word = found == 0
            || !at(found - 1).is_alphanumeric()
            || (at(found).is_uppercase() && at(found - 1).is_lowercase());
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
        from = found + 1;
    }
    // Shorter texts win ties.
    Some(score - (len as i32 / 8))
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

    fn titles(list: &[Entry], hits: &[Hit]) -> Vec<String> {
        hits.iter().map(|&(_, i)| list[i].title.clone()).collect()
    }

    #[test]
    fn fuzzy_matching_needs_every_letter_in_order() {
        assert!(fuzzy_score("ckas", "Check Assets").is_some());
        assert!(fuzzy_score("sack", "Check Assets").is_none());
        assert!(fuzzy_score("", "Anything").is_some());
        // Not ASCII: the same rules.
        assert!(fuzzy_score("ckas", "Check Assets…").is_some());
        assert_eq!(
            fuzzy_score("ckas", "Check Assets…").is_some(),
            fuzzy_score("ckas", "Check Assets...").is_some()
        );
        assert!(fuzzy_score("ü", "Grün").is_some());
    }

    #[test]
    fn the_ascii_path_scores_like_the_general_one() {
        for (query, text) in [
            ("ckas", "Check Assets"),
            ("blo", "Insert Block"),
            ("wS", "WalkSpeed"),
        ] {
            let chars: Vec<char> = text.chars().collect();
            let lower: Vec<char> = chars.iter().flat_map(|c| c.to_lowercase()).collect();
            let general = score_chars(query.chars(), chars.len(), |i| chars[i], |i| lower[i]);
            assert_eq!(fuzzy_score(query, text), general, "{query} in {text}");
        }
    }

    #[test]
    fn word_starts_and_runs_rank_first() {
        let list = entries(&["Insert Block", "Bake Probes", "Block"]);
        let ranked = rank(&[&list], "blo", None, &[]);
        assert_eq!(titles(&list, &ranked), ["Block", "Insert Block"]);
        let list = entries(&["Save", "Version History…", "Check Assets"]);
        assert_eq!(
            titles(&list, &rank(&[&list], "ca", None, &[]))[0],
            "Check Assets"
        );
    }

    #[test]
    fn recent_entries_lead_an_empty_query_newest_first() {
        let list = entries(&["Save", "Play", "Stop", "Undo"]);
        let mut palette = Palette::default();
        palette.used("Stop");
        palette.used("Undo");
        let ranked = palette.ranked(&[&list], 0).to_vec();
        assert_eq!(titles(&list, &ranked), ["Undo", "Stop", "Save", "Play"]);
        // And survive saving.
        let mut again = Palette::default();
        again.load_recent(&palette.recent_text());
        assert_eq!(again.ranked(&[&list], 0), ranked);
    }

    #[test]
    fn a_whole_name_beats_a_command_that_spells_it_out() {
        let command = Entry::new("c", "Panic during Play", "Debug").in_category(Category::Command);
        let object = Entry::new("o", "Pad", "Workspace.Pad").in_category(Category::Object);
        let list = vec![command, object];
        assert_eq!(titles(&list, &rank(&[&list], "Pad", None, &[]))[0], "Pad");
        assert_eq!(titles(&list, &rank(&[&list], "pad", None, &[]))[0], "Pad");
    }

    #[test]
    fn the_detail_matches_too_but_less_than_the_title() {
        let list = vec![
            Entry::new("a", "Point Light", "Ribbon › Lights & Sky"),
            Entry::new("b", "Lights", "View"),
        ];
        let ranked = rank(&[&list], "lights", None, &[]);
        assert_eq!(ranked, [(0, 1), (0, 0)]);
    }

    #[test]
    fn synonyms_and_descriptions_find_a_command_by_other_words() {
        let mut delete = Entry::new("delete", "Delete", "Edit");
        delete.aliases.push("Remove".to_owned());
        let mut build = Entry::new("build", "Build Game for This OS", "File › Build Game");
        build.description = "Packages the game so it runs without the editor.".to_owned();
        let list = vec![delete, build, Entry::new("save", "Save", "File")];
        assert_eq!(
            titles(&list, &rank(&[&list], "remove", None, &[])),
            ["Delete"]
        );
        assert_eq!(
            titles(&list, &rank(&[&list], "package", None, &[])),
            ["Build Game for This OS"]
        );
        // Short queries don't search descriptions: too many would match.
        assert!(rank(&[&list], "pa", None, &[]).iter().all(|&(_, i)| i != 1));
    }

    #[test]
    fn chips_and_the_command_prefix_narrow_the_list_and_count_matches() {
        let command = Entry::new("c", "Insert Door", "Ribbon").in_category(Category::Command);
        let object = Entry::new("o", "Door", "Workspace.Door").in_category(Category::Object);
        let asset = Entry::new("a", "door.glb", "assets/door.glb").in_category(Category::Asset);
        let commands = vec![command];
        let world = vec![object, asset];
        let buckets: [&[Entry]; 2] = [&commands, &world];
        let all = rank(&buckets, "door", None, &[]);
        assert_eq!(all.len(), 3);
        assert_eq!(
            rank(&buckets, "door", Some(Category::Object), &[]),
            [(1, 0)]
        );
        assert_eq!(rank(&buckets, "> door", None, &[]), [(0, 0)]);
        assert_eq!(rank(&buckets, ">", None, &[]), [(0, 0)]);
        let (_, counts) = rank_counted(&buckets, "door", Some(Category::Asset), &[]);
        let count = |category| counts.iter().find(|(c, _)| *c == category).unwrap().1;
        assert_eq!(
            [
                count(Category::Command),
                count(Category::Object),
                count(Category::Asset)
            ],
            [1, 1, 1]
        );
    }

    #[test]
    fn the_ranking_is_kept_until_the_query_or_the_index_changes() {
        let list = entries(&["Save", "Play"]);
        let mut palette = Palette {
            query: "pl".to_owned(),
            ..Default::default()
        };
        assert_eq!(palette.ranked(&[&list], 1), [(0, 1)]);
        // The same generation: the cached list, even for other entries.
        let other = entries(&["Plant", "Play"]);
        assert_eq!(palette.ranked(&[&other], 1), [(0, 1)]);
        // A new generation ranks again.
        assert_eq!(palette.ranked(&[&other], 2).len(), 2);
        palette.query = "pla".to_owned();
        assert_eq!(palette.ranked(&[&other], 2).len(), 2);
    }

    #[test]
    fn tab_steps_through_the_chips_that_have_matches() {
        let objects = vec![Entry::new("o", "Door", "").in_category(Category::Object)];
        let scripts = vec![Entry::new("s", "DoorScript", "").in_category(Category::Script)];
        let mut palette = Palette {
            query: "door".to_owned(),
            ..Default::default()
        };
        palette.ranked(&[&objects, &scripts], 0);
        palette.step_scope(1);
        assert_eq!(palette.scope, Some(Category::Object));
        palette.step_scope(1);
        assert_eq!(palette.scope, Some(Category::Script));
        palette.step_scope(1);
        assert_eq!(palette.scope, None);
        palette.step_scope(-1);
        assert_eq!(palette.scope, Some(Category::Script));
    }
}
