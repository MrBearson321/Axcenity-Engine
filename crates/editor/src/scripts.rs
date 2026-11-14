//! Script tabs: one dock tab per open script or stylesheet, beside the
//! Scene View (see `layout.rs`). A tab shows a code editor with Lua (or CSS)
//! syntax highlighting and line numbers, or a block script's blocks.
//!
//! Tabs are found by their script's file path, not its Instance, because
//! the file is what's being edited: it outlives its Script Instance when the
//! world is reloaded, and a tab can stay open after its file was deleted.
//! Each tab also has an id that never changes, even when its file is
//! renamed, which is how the dock knows it. [`ScriptTabs`] never touches
//! files; the app reads and writes them.
//!
//! Clicking a line number sets or clears a breakpoint there (a red dot).
//! Breakpoints are kept by file path, whether or not the tab is open, and
//! Play stops at them (see `play.rs`). While Play is paused, the paused line
//! has an arrow and its own background.

use std::collections::{BTreeMap, HashMap};

use axcenity_script::{BreakpointRule, BreakpointRules, Breakpoints, Pause};

use axcenity_core::{ClassName, World};
use axcenity_project::{is_stylesheet, script_name};
use eframe::egui;

use crate::luau_check::{Diagnostic, Severity};
use crate::script_completion::{self, Definition};
use crate::side_warnings::SideWarning;
use crate::theme::{self, icons};
use crate::{css_syntax, lua_syntax};
pub use axcenity_editor_kit::widgets::locked_banner;

mod assist;
mod others;

pub use others::OtherCursor;

pub use assist::Assist;

/// What a new script starts with, as in Roblox Studio.
pub const NEW_SCRIPT_SOURCE: &str = "print(\"Hello world!\")\n";
/// What a new ModuleScript starts with: a table to put functions in, which
/// `require` returns.
pub const NEW_MODULE_SOURCE: &str = "local module = {}\n\nreturn module\n";

/// What a new script of `class` starts with.
pub fn new_script_source(class: ClassName) -> &'static str {
    match class {
        ClassName::ModuleScript => NEW_MODULE_SOURCE,
        _ => NEW_SCRIPT_SOURCE,
    }
}
/// The background of the line jumped to from an error in Output.
const JUMP_LINE_COLOR: egui::Color32 = egui::Color32::from_rgba_premultiplied(60, 50, 10, 60);
/// The gutter's space for breakpoint dots, left of the line numbers.
const MARKER_WIDTH: f32 = 14.0;

/// How a tab's file is, compared with the tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disk {
    /// The tab's `saved` text is what's in the file.
    Same,
    /// The file was changed by another program while the tab had unsaved
    /// changes. `saved` holds the new file text, and the tab keeps yours.
    Changed,
    /// The file was deleted by another program while the tab had unsaved
    /// changes. Saving writes it back.
    Deleted,
}

pub struct ScriptTab {
    /// Names the tab's dock tab (`Pane::Script`); kept when the file is renamed.
    pub id: u64,
    /// The script file, as a [`axcenity_project::ScriptFile::path`].
    pub path: String,
    /// The code as it is in the editor.
    pub text: String,
    /// The file's text as last read or written. When `text` differs from
    /// it, the tab has unsaved changes.
    pub saved: String,
    pub disk: Disk,
    /// A line (counting from 1) to put the cursor on in the next frame.
    go_to_line: Option<usize>,
    /// The line last jumped to, highlighted until the code is edited.
    highlighted: Option<usize>,
    /// The completion list and signature hint.
    pub assist: Assist,
    /// What luau-lsp last found in the code (`luau_check.rs`), underlined.
    /// Kept while the code changes, until the next check comes back.
    pub problems: Vec<Diagnostic>,
    /// In a Collaborate session, who else is editing this file: the tab is
    /// read-only until they save or close it. The app sets it each frame.
    pub editing_elsewhere: Option<String>,
    /// In a Collaborate session, the tab edits the file's live document
    /// with everyone at once (C7): Undo and Redo take back only this
    /// member's typing. The app sets it each frame.
    pub live: bool,
    /// Undo (true) or Redo (false) pressed in a live tab, for the app.
    pub live_undo: Option<bool>,
    /// The span others changed, as characters: `start`, the old end and the
    /// new end, so the cursor stays on the text it was on.
    remote_span: Option<(usize, usize, usize)>,
    /// This member's selection while the text box has the focus, as
    /// characters (anchor, head), for others to see (C8).
    pub cursor: Option<(usize, usize)>,
    /// Where Go to Definition (F12, or Ctrl+click) asked to go outside the
    /// tab: an Instance or an API entry, for the editor to open
    /// ([`ScriptTabs::take_definition`]).
    pub definition: Option<Definition>,
    /// Others' cursors in this tab's live document. The app sets them
    /// each frame.
    pub others: Vec<OtherCursor>,
}

impl ScriptTab {
    /// The Script's name, from its file name, or a stylesheet's file name.
    pub fn name(&self) -> &str {
        if self.is_stylesheet() {
            self.path.rsplit('/').next().unwrap_or(&self.path)
        } else {
            script_name(&self.path)
        }
    }

    pub fn is_stylesheet(&self) -> bool {
        is_stylesheet(&self.path)
    }

    pub fn unsaved(&self) -> bool {
        self.text != self.saved || self.disk == Disk::Deleted
    }

    /// The line the cursor goes to in the next frame, if any.
    #[cfg(test)]
    pub fn go_to_line(&self) -> Option<usize> {
        self.go_to_line
    }

    /// Others changed the live document: the tab shows `text` now, with
    /// the cursor kept where it was in the text around it.
    pub fn take_remote_text(&mut self, text: String) {
        if text == self.text {
            return;
        }
        self.remote_span = Some(changed_chars(&self.text, &text));
        self.text = text;
        self.highlighted = None;
    }

    /// The text was written to the file: no unsaved changes now.
    pub fn mark_saved(&mut self) {
        self.saved = self.text.clone();
        self.disk = Disk::Same;
    }
}

/// The open script tabs, and which one was used last.
#[derive(Default)]
pub struct ScriptTabs {
    tabs: Vec<ScriptTab>,
    /// The tab used last, or `None` when the viewport was.
    active: Option<usize>,
    /// The id the next new tab gets.
    next_id: u64,
    /// A tab that was just opened or asked for, to bring to the front of
    /// the dock in the next frame.
    focus: Option<u64>,
    /// Each script's uses of APIs that only work on the other side, by
    /// path. The app fills it in each frame; a tab with any shows a warning sign.
    pub warnings: HashMap<String, Vec<SideWarning>>,
    /// Every breakpoint, by script path, open or not.
    breakpoints: Breakpoints,
    /// Breakpoints' conditions, hit counts and log messages, by path and
    /// line; only for breakpoints that are set.
    rules: BreakpointRules,
    /// The breakpoint whose rule is being edited (right-click on its line
    /// number): path, line, and the boxes' text.
    editing_rule: Option<RuleDraft>,
    /// Set when a breakpoint is added or removed, until the app takes it.
    breakpoints_changed: bool,
    /// Where Play is paused, if it is. The app sets it.
    pub paused: Option<Pause>,
    /// What to say about the Luau language server above Lua code tabs
    /// (`app/luau_checks.rs`). The app sets it each frame.
    pub server: Option<ServerNote>,
    /// The bar's button pressed, until the app takes it.
    pub server_request: Option<ServerRequest>,
}

/// The language server type checking needs, when it's missing.
#[derive(Clone, Debug, PartialEq)]
pub enum ServerNote {
    /// Not found: offer to download it.
    Missing,
    /// Downloading, this far (0 to 1).
    Downloading(f32),
    /// The last download failed, for this reason.
    Failed(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServerRequest {
    Download,
    Dismiss,
}

impl ScriptTabs {
    /// Shows the tab for the script at `path`, opening it with `source` if
    /// it isn't open yet. With a `line`, the cursor goes to the start of that line.
    pub fn open(&mut self, path: &str, source: &str, line: Option<usize>) {
        let index = match self.tabs.iter().position(|tab| tab.path == path) {
            Some(index) => index,
            None => self.push(path, source),
        };
        if line.is_some() {
            self.tabs[index].go_to_line = line;
        }
        self.active = Some(index);
        self.focus = Some(self.tabs[index].id);
    }

    fn push(&mut self, path: &str, source: &str) -> usize {
        self.tabs.push(ScriptTab {
            id: self.next_id,
            path: path.to_string(),
            text: source.to_string(),
            saved: source.to_string(),
            disk: Disk::Same,
            go_to_line: None,
            highlighted: None,
            assist: Assist::default(),
            problems: Vec::new(),
            editing_elsewhere: None,
            live: false,
            live_undo: None,
            remote_span: None,
            cursor: None,
            definition: None,
            others: Vec::new(),
        });
        self.next_id += 1;
        self.tabs.len() - 1
    }

    /// Opens a tab for `path` with `source` without showing it, if it isn't
    /// open yet, and returns it. Dragging a UI handle edits a stylesheet
    /// this way, so its unsaved changes show on its tab.
    pub fn open_in_background(&mut self, path: &str, source: &str) -> &mut ScriptTab {
        let index = match self.tabs.iter().position(|tab| tab.path == path) {
            Some(index) => index,
            None => self.push(path, source),
        };
        &mut self.tabs[index]
    }

    /// The viewport was used last, not a script.
    pub fn show_world(&mut self) {
        self.active = None;
    }

    #[cfg(test)]
    pub fn world_shown(&self) -> bool {
        self.active.is_none()
    }

    /// Where a tab's Go to Definition asked to go outside it, once.
    pub fn take_definition(&mut self) -> Option<Definition> {
        self.tabs.iter_mut().find_map(|tab| tab.definition.take())
    }

    /// The tab with this id was clicked or typed in.
    pub fn activate(&mut self, id: u64) {
        self.active = self.tabs.iter().position(|tab| tab.id == id);
    }

    /// The script tab used last, if the viewport wasn't.
    pub fn active(&self) -> Option<&ScriptTab> {
        self.tabs.get(self.active?)
    }

    #[cfg(test)]
    pub fn active_path(&self) -> Option<&str> {
        Some(&self.tabs.get(self.active?)?.path)
    }

    /// The id of the tab used last.
    pub fn active_id(&self) -> Option<u64> {
        Some(self.tabs.get(self.active?)?.id)
    }

    /// Every open tab's id, in the order they were opened.
    pub fn ids(&self) -> Vec<u64> {
        self.tabs.iter().map(|tab| tab.id).collect()
    }

    /// The tab to bring to the front, once.
    pub fn take_focus(&mut self) -> Option<u64> {
        self.focus.take()
    }

    /// The path of the tab with this id, if it's still open.
    pub fn path_of(&self, id: u64) -> Option<&str> {
        self.tabs
            .iter()
            .find(|tab| tab.id == id)
            .map(|tab| tab.path.as_str())
    }

    /// Every open tab.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut ScriptTab> {
        self.tabs.iter_mut()
    }

    pub fn get(&self, path: &str) -> Option<&ScriptTab> {
        self.tabs.iter().find(|tab| tab.path == path)
    }

    pub fn get_mut(&mut self, path: &str) -> Option<&mut ScriptTab> {
        self.tabs.iter_mut().find(|tab| tab.path == path)
    }

    /// Closes the tab for `path`, if it's open. If it was used last, the
    /// tab opened after it counts as used last instead (or the one before
    /// it, at the end, or the viewport when no script tabs are left).
    pub fn close(&mut self, path: &str) {
        let Some(index) = self.tabs.iter().position(|tab| tab.path == path) else {
            return;
        };
        self.tabs.remove(index);
        self.active = match self.active {
            Some(active) if active == index => {
                (!self.tabs.is_empty()).then(|| index.min(self.tabs.len() - 1))
            }
            Some(active) if active > index => Some(active - 1),
            other => other,
        };
    }

    /// Closes every tab.
    pub fn close_all(&mut self) {
        self.tabs.clear();
        self.active = None;
    }

    /// The script at `from` was renamed to `to`. Its breakpoints go with it.
    pub fn rename(&mut self, from: &str, to: &str) {
        if let Some(tab) = self.get_mut(from) {
            tab.path = to.to_string();
        }
        if let Some(lines) = self.breakpoints.remove(from) {
            self.breakpoints.insert(to.to_string(), lines);
            self.breakpoints_changed = true;
        }
        let moved: Vec<_> = self
            .rules
            .keys()
            .filter(|(path, _)| path == from)
            .cloned()
            .collect();
        for (path, line) in moved {
            if let Some(rule) = self.rules.remove(&(path, line)) {
                self.rules.insert((to.to_string(), line), rule);
            }
        }
    }

    /// Every breakpoint's condition, hit count and log message.
    pub fn breakpoint_rules(&self) -> &BreakpointRules {
        &self.rules
    }

    /// Sets the rule of the breakpoint at `line` of `path`, setting the
    /// breakpoint too; a plain rule leaves a plain breakpoint.
    pub fn set_breakpoint_rule(&mut self, path: &str, line: usize, rule: BreakpointRule) {
        self.breakpoints
            .entry(path.to_string())
            .or_default()
            .insert(line);
        let at = (path.to_string(), line);
        if rule.is_plain() {
            self.rules.remove(&at);
        } else {
            self.rules.insert(at, rule);
        }
        self.breakpoints_changed = true;
    }

    /// How each breakpoint of `path` looks in the gutter.
    fn marks(
        breakpoints: &Breakpoints,
        rules: &BreakpointRules,
        path: &str,
    ) -> BTreeMap<usize, Mark> {
        breakpoints
            .get(path)
            .into_iter()
            .flatten()
            .map(|&line| {
                let mark = match rules.get(&(path.to_string(), line)) {
                    Some(rule) if rule.log.is_some() => Mark::Log,
                    Some(_) => Mark::Conditional,
                    None => Mark::Plain,
                };
                (line, mark)
            })
            .collect()
    }

    pub fn breakpoints(&self) -> &Breakpoints {
        &self.breakpoints
    }

    /// Sets the breakpoint at `line` (from 1) of the script at `path` if
    /// there isn't one, or clears it if there is.
    pub fn toggle_breakpoint(&mut self, path: &str, line: usize) {
        let lines = self.breakpoints.entry(path.to_string()).or_default();
        if !lines.remove(&line) {
            lines.insert(line);
        } else {
            self.rules.remove(&(path.to_string(), line));
        }
        if lines.is_empty() {
            self.breakpoints.remove(path);
        }
        self.breakpoints_changed = true;
    }

    /// Whether breakpoints changed since the last call.
    pub fn take_breakpoints_changed(&mut self) -> bool {
        std::mem::take(&mut self.breakpoints_changed)
    }

    /// The paths of the tabs with unsaved changes, in tab order.
    pub fn unsaved_paths(&self) -> Vec<String> {
        self.tabs
            .iter()
            .filter(|tab| tab.unsaved())
            .map(|tab| tab.path.clone())
            .collect()
    }

    /// The title of the dock tab with this id: an icon in its class's
    /// colour, the name, a dot when unsaved (as in VS Code) and a warning
    /// sign when it uses APIs from the other side.
    pub fn title(&self, id: u64, visuals: &egui::Visuals) -> egui::WidgetText {
        let Some(tab) = self.tabs.iter().find(|tab| tab.id == id) else {
            return "Script".into();
        };
        let (icon, icon_color) = if tab.is_stylesheet() {
            (
                icons::PAINT_BRUSH,
                theme::class_color(ClassName::ScreenGui, visuals),
            )
        } else {
            let class = axcenity_project::script_class(&tab.path);
            (theme::class_icon(class), theme::class_color(class, visuals))
        };
        let font = egui::FontId::new(theme::type_scale::BODY - 0.5, theme::medium_family());
        let format = |color| egui::TextFormat::simple(font.clone(), color);
        let mut job = egui::text::LayoutJob::default();
        job.append(icon, 0.0, format(icon_color));
        // A placeholder colour: the dock paints it in the tab's text colour.
        job.append(tab.name(), 5.0, format(egui::Color32::PLACEHOLDER));
        if tab.unsaved() {
            job.append("\u{25CF}", 5.0, format(egui::Color32::PLACEHOLDER));
        }
        if self.warnings.contains_key(&tab.path) {
            job.append(icons::WARNING, 5.0, format(visuals.warn_fg_color));
        }
        job.into()
    }

    /// Draws the code editor for the tab at `path`.
    /// Draws the tab for `path`. `world` gives completion its Instances.
    pub fn show(&mut self, ui: &mut egui::Ui, path: &str, world: &World) {
        let Some(tab) = self.tabs.iter_mut().find(|tab| tab.path == path) else {
            return;
        };
        let warning = ui.visuals().warn_fg_color;
        if let Some(who) = &tab.editing_elsewhere {
            locked_banner(ui, who);
        }
        match tab.disk {
            Disk::Same => {}
            Disk::Changed => {
                ui.horizontal(|ui| {
                    ui.colored_label(
                        warning,
                        theme::with_icon(
                            icons::WARNING,
                            "This file was changed outside the editor. Saving replaces it \
                             with your version.",
                        ),
                    );
                    if ui
                        .button(theme::with_icon(
                            icons::ARROWS_CLOCKWISE,
                            "Reload from disk",
                        ))
                        .clicked()
                    {
                        tab.text = tab.saved.clone();
                        tab.disk = Disk::Same;
                    }
                });
            }
            Disk::Deleted => {
                ui.colored_label(
                    warning,
                    theme::with_icon(
                        icons::WARNING,
                        "This file was deleted outside the editor. Save to write it back, \
                         or close the tab.",
                    ),
                );
            }
        }
        // Clicking a warning puts the cursor on its line.
        for side_warning in self.warnings.get(&tab.path).into_iter().flatten() {
            let text = format!(
                "{}  Line {}: {}",
                icons::WARNING,
                side_warning.line,
                side_warning.message
            );
            let label = egui::Label::new(egui::RichText::new(text).color(warning))
                .sense(egui::Sense::click());
            if ui.add(label).on_hover_text("Go to this line").clicked() {
                tab.go_to_line = Some(side_warning.line);
            }
        }
        if !tab.is_stylesheet()
            && tab.path.ends_with(".lua")
            && let Some(note) = &self.server
            && let Some(request) = server_bar(ui, note)
        {
            self.server_request = Some(request);
        }
        if let Some(line) = problems_bar(ui, &tab.problems) {
            tab.go_to_line = Some(line);
        }
        let paused_line = self
            .paused
            .as_ref()
            .filter(|pause| pause.path == tab.path)
            .map(|pause| pause.line);
        let path = tab.path.clone();
        let marks = Self::marks(&self.breakpoints, &self.rules, &path);
        match code_editor(ui, tab, &marks, paused_line, world) {
            Some(GutterClick::Toggle(line)) => self.toggle_breakpoint(&path, line),
            Some(GutterClick::Edit(line)) => {
                let rule = self
                    .rules
                    .get(&(path.clone(), line))
                    .cloned()
                    .unwrap_or_default();
                self.editing_rule = Some(RuleDraft::new(path.clone(), line, &rule));
            }
            None => {}
        }
        // Drawn by the tab it belongs to, so it's drawn once a frame.
        if self
            .editing_rule
            .as_ref()
            .is_some_and(|draft| draft.path == path)
        {
            self.rule_window(ui.ctx());
        }
    }

    /// The window editing a breakpoint's condition, hit count and log
    /// message, while one is open.
    fn rule_window(&mut self, ctx: &egui::Context) {
        let Some(draft) = &mut self.editing_rule else {
            return;
        };
        let file = draft
            .path
            .rsplit('/')
            .next()
            .unwrap_or(&draft.path)
            .to_owned();
        let title = format!("Breakpoint on {file}:{}", draft.line);
        let (mut action, dismissed) =
            crate::dialog::modal(ctx, "breakpoint rule", crate::dialog::WIDTH, |ui| {
                rule_form(ui, &title, draft)
            });
        if dismissed {
            action = Some(RuleAction::Cancel);
        }
        match action {
            Some(RuleAction::Save) => {
                let draft = self.editing_rule.take().expect("open above");
                match draft.rule() {
                    Ok(rule) => self.set_breakpoint_rule(&draft.path, draft.line, rule),
                    Err(error) => {
                        self.editing_rule = Some(RuleDraft {
                            error: Some(error),
                            ..draft
                        });
                    }
                }
            }
            Some(RuleAction::Remove) => {
                let draft = self.editing_rule.take().expect("open above");
                let set = self
                    .breakpoints
                    .get(&draft.path)
                    .is_some_and(|lines| lines.contains(&draft.line));
                if set {
                    self.toggle_breakpoint(&draft.path, draft.line);
                }
            }
            Some(RuleAction::Cancel) => self.editing_rule = None,
            None => {}
        }
    }
}

/// How a breakpoint looks beside its line number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mark {
    /// Always pauses: a red dot.
    Plain,
    /// Has a condition or hit count: a red dot with a light middle.
    Conditional,
    /// A logpoint, which never pauses: a red diamond.
    Log,
}

/// What a click on the line numbers asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GutterClick {
    /// Set or clear the breakpoint (a click).
    Toggle(usize),
    /// Edit the breakpoint's condition, hit count or log message (a
    /// right-click).
    Edit(usize),
}

/// A breakpoint rule as typed in its window.
#[derive(Clone, Debug, Default)]
struct RuleDraft {
    path: String,
    line: usize,
    condition: String,
    hit_count: String,
    log: String,
    /// Why Save didn't take it, shown under the boxes.
    error: Option<String>,
}

impl RuleDraft {
    fn new(path: String, line: usize, rule: &BreakpointRule) -> Self {
        Self {
            path,
            line,
            condition: rule.condition.clone().unwrap_or_default(),
            hit_count: rule.hit_count.map(|n| n.to_string()).unwrap_or_default(),
            log: rule.log.clone().unwrap_or_default(),
            error: None,
        }
    }

    /// The rule the boxes describe: empty boxes are left out.
    fn rule(&self) -> Result<BreakpointRule, String> {
        let text = |text: &str| Some(text.trim().to_owned()).filter(|text| !text.is_empty());
        let hit_count = match text(&self.hit_count) {
            None => None,
            Some(count) => Some(
                count
                    .parse::<u32>()
                    .ok()
                    .filter(|&count| count >= 1)
                    .ok_or("The hit count is a whole number, 1 or more.")?,
            ),
        };
        if let Some(condition) = text(&self.condition)
            && axcenity_script::syntax_error("condition", &format!("return ({condition}\n)"))
                .is_some()
        {
            return Err("The condition isn't a Lua expression.".to_owned());
        }
        Ok(BreakpointRule {
            condition: text(&self.condition),
            hit_count,
            log: text(&self.log),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RuleAction {
    Save,
    Remove,
    Cancel,
}

/// The rule window's boxes and buttons.
fn rule_form(ui: &mut egui::Ui, title: &str, draft: &mut RuleDraft) -> Option<RuleAction> {
    use crate::dialog::{self, Button, Enter, Kind, Tone};
    let palette = theme::palette(ui.ctx());
    dialog::header(
        ui,
        icons::BUG,
        Tone::Question,
        title,
        Some("Leave a box empty to leave it out. A breakpoint with none of them always pauses."),
    );
    let field = |ui: &mut egui::Ui, label: &str, text: &mut String, hint: &str, help: &str| {
        ui.label(label);
        ui.add(
            dialog::text_field(text)
                .hint_text(hint)
                .font(egui::TextStyle::Monospace),
        );
        ui.weak(help);
        ui.add_space(6.0);
    };
    field(
        ui,
        "Condition",
        &mut draft.condition,
        "health < 10",
        "A Lua expression, read with this line's variables. Play pauses only when it's true.",
    );
    field(
        ui,
        "Hit count",
        &mut draft.hit_count,
        "3",
        "Pause only from this pass on, counting passes where the condition is true.",
    );
    field(
        ui,
        "Log message",
        &mut draft.log,
        "health is {health}",
        "Write this to Output instead of pausing. {expression} is replaced by its value.",
    );
    if let Some(error) = &draft.error {
        ui.colored_label(palette.error, error);
    }
    match dialog::buttons(
        ui,
        &[
            Button::new(icons::TRASH, "Remove Breakpoint", Kind::Plain),
            Button::cancel(),
            Button::new(icons::CHECK, "Save", Kind::Main),
        ],
        Enter::Main,
    ) {
        Some(0) => Some(RuleAction::Remove),
        Some(1) => Some(RuleAction::Cancel),
        Some(_) => Some(RuleAction::Save),
        None => None,
    }
}

impl ScriptTabs {}

/// How a Script or LocalScript is shown and edited.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptView {
    /// Typed Lua.
    Code,
    /// Scratch-style blocks.
    Blocks,
    /// The same blocks as Blueprint-style nodes and wires.
    Graph,
}

/// The bar above a Script or LocalScript's tab that switches it between
/// Code, Blocks and Graph. Returns the view picked, if it's another one.
pub fn mode_bar(ui: &mut egui::Ui, view: ScriptView, enabled: bool) -> Option<ScriptView> {
    let mut switch = None;
    let palette = theme::palette(ui.ctx());
    ui.horizontal(|ui| {
        ui.add_enabled_ui(enabled, |ui| {
            let mut picked = view;
            let response = ui
                .scope(|ui| {
                    crate::widgets::segmented(
                        ui,
                        &mut picked,
                        &[
                            (ScriptView::Code, Some(icons::CODE), "Code"),
                            (ScriptView::Blocks, Some(icons::PUZZLE_PIECE), "Blocks"),
                            (ScriptView::Graph, Some(icons::GRAPH), "Graph"),
                        ],
                    )
                })
                .response;
            response
                .on_hover_text(
                    "Code: type the script as Lua. Blocks: snap it together from blocks. \
                     Graph: wire nodes together, as in Unreal Blueprints. Lua that has no \
                     blocks shows as grey Lua blocks and nodes, so nothing is lost. A script \
                     inside a part switches for every copy of it, as one undo step. UI Event \
                     Graphs, behavior trees and dialogues are graphs of their own and stay \
                     graphs.",
                )
                .on_disabled_hover_text("Press Stop to switch.");
            switch = (picked != view).then_some(picked);
        });
        ui.label(
            egui::RichText::new("Switch any time: the script runs the same either way.")
                .size(theme::type_scale::SMALL)
                .color(palette.text_weak),
        );
    });
    switch
}

/// Moves the cursor of the text box `id` past a span others changed:
/// `(start, old_end, new_end)` in characters. A cursor after the span moves
/// by its change in length; one inside it goes to its new end.
fn keep_cursor(
    ctx: &egui::Context,
    id: egui::Id,
    (start, old_end, new_end): (usize, usize, usize),
) {
    let Some(mut state) = egui::TextEdit::load_state(ctx, id) else {
        return;
    };
    let Some(range) = state.cursor.char_range() else {
        return;
    };
    let shift = |cursor: egui::text::CCursor| {
        let index = cursor.index.0;
        let moved = if index <= start {
            index
        } else if index >= old_end {
            index - old_end + new_end
        } else {
            new_end
        };
        egui::text::CCursor::new(moved)
    };
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::two(
            shift(range.secondary),
            shift(range.primary),
        )));
    state.store(ctx, id);
}

/// The span of `old` that differs from `new`, in characters: `start`, its
/// end in `old` and its end in `new`.
pub fn changed_chars(old: &str, new: &str) -> (usize, usize, usize) {
    let (start, old_end, new_end) = axcenity_collab::live::changed_span(old, new);
    let start_chars = old[..start].chars().count();
    (
        start_chars,
        start_chars + old[start..old_end].chars().count(),
        start_chars + new[start..new_end].chars().count(),
    )
}

/// The ID of the code editor's text box for the script at `path`. egui
/// keeps the cursor and scroll position under it.
pub fn editor_id(path: &str) -> egui::Id {
    egui::Id::new(("script editor", path))
}

/// The text box with the code, colored, with line numbers down the left,
/// the `breakpoints` beside them, and the line Play is paused on marked.
/// Returns the line whose number was clicked (to set or clear its
/// breakpoint) or right-clicked (to edit its rule).
fn code_editor(
    ui: &mut egui::Ui,
    tab: &mut ScriptTab,
    breakpoints: &BTreeMap<usize, Mark>,
    paused_line: Option<usize>,
    world: &World,
) -> Option<GutterClick> {
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    let id = editor_id(&tab.path);
    // `split` counts an empty last line after a final newline, which is
    // where the cursor can go, so the numbers match the text box's rows.
    let lines = tab.text.split('\n').count();
    let mut clicked = None;

    // egui calls this to lay out the text whenever it needs to. It ignores
    // the wrap width: long lines scroll sideways, so they stay one row each
    // and line up with the numbers.
    let stylesheet = tab.is_stylesheet();
    // A stylesheet's colours each get a swatch before them, in a gap the
    // layout leaves.
    let row = ui.fonts_mut(|fonts| fonts.row_height(&font));
    let swatch = if stylesheet { row + 4.0 } else { 0.0 };
    let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, _wrap_width: f32| {
        let job = if stylesheet {
            css_syntax::layout_job(text.as_str(), font.clone(), ui.visuals(), swatch)
        } else {
            lua_syntax::layout_job(text.as_str(), font.clone(), ui.visuals())
        };
        ui.fonts_mut(|fonts| fonts.layout_job(job))
    };

    egui::ScrollArea::both()
        .id_salt(id)
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                clicked = gutter(ui, &font, lines, breakpoints, paused_line);

                // A placeholder painted before the text, so the jump
                // highlight can go behind it once the text is laid out.
                let behind = ui.painter().add(egui::Shape::Noop);
                // The list's keys are taken before the text box sees them.
                let focused = !stylesheet && ui.memory(|memory| memory.has_focus(id));
                let keys = tab.assist.take_keys(ui, focused);
                // Go to Definition (G4): F12, or Ctrl+click below.
                let mut go_to_definition = focused
                    && ui.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::F12)
                    });
                if let Some(span) = tab.remote_span.take() {
                    keep_cursor(ui.ctx(), id, span);
                }
                // A live tab's Undo and Redo go to its live document, which
                // takes back only this member's typing.
                if tab.live && ui.memory(|memory| memory.has_focus(id)) {
                    tab.live_undo = ui
                        .input_mut(|input| {
                            let redo = input.consume_key(
                                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                                egui::Key::Z,
                            ) || input
                                .consume_key(egui::Modifiers::COMMAND, egui::Key::Y);
                            if redo {
                                Some(false)
                            } else if input.consume_key(egui::Modifiers::COMMAND, egui::Key::Z) {
                                Some(true)
                            } else {
                                None
                            }
                        })
                        .or(tab.live_undo);
                }
                let output = egui::TextEdit::multiline(&mut tab.text)
                    .id(id)
                    .interactive(tab.editing_elsewhere.is_none())
                    .font(font.clone())
                    .code_editor()
                    .frame(egui::Frame::NONE)
                    .desired_width(f32::INFINITY)
                    .min_size(ui.available_size())
                    .layouter(&mut layouter)
                    .show(ui);
                if output.response.changed() {
                    tab.highlighted = None;
                }
                tab.cursor = output
                    .response
                    .has_focus()
                    .then(|| output.state.cursor.char_range())
                    .flatten()
                    .map(|range| (range.secondary.index.0, range.primary.index.0));
                go_to_definition |= !stylesheet
                    && output.response.clicked()
                    && ui.input(|input| input.modifiers.command);
                if go_to_definition && let Some((_, cursor)) = tab.cursor {
                    let byte = assist::byte_of_char(&tab.text, cursor);
                    match script_completion::definition_at(&tab.text, byte, world) {
                        Some(Definition::Here(at)) => {
                            tab.go_to_line = Some(tab.text[..at].matches('\n').count() + 1);
                        }
                        found => tab.definition = found,
                    }
                }
                others::paint(ui, &tab.text, &output, &tab.others);
                if !stylesheet {
                    underline_problems(ui, &tab.text, &tab.problems, &output);
                    assist(ui, tab, &output, keys, world);
                }
                if stylesheet {
                    let galley_pos = output.galley_pos.to_vec2();
                    let at = |text: &str, byte: usize| {
                        let chars = text[..byte].chars().count();
                        output
                            .galley
                            .pos_from_cursor(egui::text::CCursor::new(chars))
                            .translate(galley_pos)
                    };
                    color_swatches(ui, tab, id, swatch, at);
                }
                let line_rect = |line: usize| {
                    let cursor = egui::text::CCursor::new(char_index_of_line(&tab.text, line));
                    let at = output
                        .galley
                        .pos_from_cursor(cursor)
                        .translate(output.galley_pos.to_vec2());
                    (cursor, at)
                };

                if let Some(line) = tab.go_to_line.take() {
                    let (cursor, at) = line_rect(line);
                    let mut state = output.state.clone();
                    state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::one(cursor)));
                    state.store(ui.ctx(), output.response.id);
                    output.response.request_focus();
                    ui.scroll_to_rect(at, Some(egui::Align::Center));
                    tab.highlighted = Some(line);
                }
                // The paused line's background wins over the jump's.
                let warning = theme::palette(ui.ctx()).warning;
                let marked = paused_line
                    .map(|line| (line, warning.gamma_multiply(0.22)))
                    .or_else(|| tab.highlighted.map(|line| (line, JUMP_LINE_COLOR)));
                if let Some((line, color)) = marked {
                    let (_, at) = line_rect(line);
                    let row =
                        egui::Rect::from_x_y_ranges(output.response.rect.x_range(), at.y_range());
                    ui.painter()
                        .set(behind, egui::Shape::rect_filled(row, 0.0, color));
                }
            });
        });
    clicked
}

/// The bar above a Lua code tab while the Luau language server is
/// missing: type checking needs it, so offer the version packages ship.
fn server_bar(ui: &mut egui::Ui, note: &ServerNote) -> Option<ServerRequest> {
    let palette = theme::palette(ui.ctx());
    let mut request = None;
    ui.horizontal_wrapped(|ui| match note {
        ServerNote::Downloading(fraction) => {
            ui.add(
                egui::ProgressBar::new(*fraction)
                    .desired_width(120.0)
                    .show_percentage(),
            );
            ui.label(
                egui::RichText::new("Downloading the Luau language server…")
                    .color(palette.text_weak),
            );
        }
        ServerNote::Missing | ServerNote::Failed(_) => {
            let text = match note {
                ServerNote::Failed(reason) => {
                    format!("Couldn't download the Luau language server: {reason}")
                }
                _ => "Type checking needs the Luau language server, which this build \
                          doesn't include."
                    .to_owned(),
            };
            ui.label(
                egui::RichText::new(theme::with_icon(icons::INFO, &text))
                    .size(theme::type_scale::SMALL)
                    .color(palette.text_weak),
            );
            let size = crate::luau_lsp::pinned()
                .asset()
                .map(|asset| asset.size.div_ceil(1 << 20))
                .unwrap_or(0);
            if ui
                .small_button(format!("Download ({size} MB)"))
                .on_hover_text(
                    "Downloads luau-lsp, the version the editor's packages include, into \
                         the editor's data folder. Then errors are underlined as you type.",
                )
                .clicked()
            {
                request = Some(ServerRequest::Download);
            }
            if ui.small_button("Not now").clicked() {
                request = Some(ServerRequest::Dismiss);
            }
        }
    });
    request
}

/// One line above the code saying how many problems luau-lsp found, and
/// the first; clicking it returns that problem's line.
fn problems_bar(ui: &mut egui::Ui, problems: &[Diagnostic]) -> Option<usize> {
    let first = problems
        .iter()
        .find(|problem| problem.severity == Severity::Error)
        .or(problems.first())?;
    let errors = problems
        .iter()
        .filter(|problem| problem.severity == Severity::Error)
        .count();
    let warnings = problems.len() - errors;
    let palette = theme::palette(ui.ctx());
    let mut counts = Vec::new();
    if errors > 0 {
        counts.push(format!(
            "{errors} error{}",
            if errors == 1 { "" } else { "s" }
        ));
    }
    if warnings > 0 {
        counts.push(format!(
            "{warnings} warning{}",
            if warnings == 1 { "" } else { "s" }
        ));
    }
    let (icon, color) = if errors > 0 {
        (icons::X_CIRCLE, palette.error)
    } else {
        (icons::WARNING, palette.warning)
    };
    let message = first.message.lines().next().unwrap_or("");
    let text = format!(
        "{icon}  {}  ·  Line {}: {message}",
        counts.join(", "),
        first.line
    );
    let label = egui::Label::new(
        egui::RichText::new(text)
            .size(theme::type_scale::SMALL)
            .color(color),
    )
    .truncate()
    .sense(egui::Sense::click());
    let response = ui
        .add(label)
        .on_hover_text("Found by luau-lsp. Click to go to this line.");
    response.clicked().then_some(first.line)
}

/// The bytes a problem covers in `text`: from its first byte to after its
/// last, kept to its first line and inside the text.
fn problem_bytes(text: &str, problem: &Diagnostic) -> std::ops::Range<usize> {
    let line_start = |line: usize| {
        let mut at = 0;
        for _ in 1..line {
            match text[at..].find('\n') {
                Some(next) => at += next + 1,
                None => return text.len(),
            }
        }
        at
    };
    let start_of_line = line_start(problem.line);
    let end_of_line = text[start_of_line..]
        .find('\n')
        .map_or(text.len(), |at| start_of_line + at);
    let floor = |mut at: usize| {
        at = at.min(end_of_line);
        while !text.is_char_boundary(at) {
            at -= 1;
        }
        at
    };
    let start = floor(start_of_line + problem.column.saturating_sub(1));
    let end = if problem.end_line == problem.line {
        floor(start_of_line + problem.end_column)
    } else {
        end_of_line
    };
    start..end.max(start)
}

/// A wavy line under each problem, red for errors and amber for lints;
/// hovering one shows its message.
fn underline_problems(
    ui: &egui::Ui,
    text: &str,
    problems: &[Diagnostic],
    output: &egui::text_edit::TextEditOutput,
) {
    if problems.is_empty() {
        return;
    }
    let palette = theme::palette(ui.ctx());
    let offset = output.galley_pos.to_vec2();
    let at = |byte: usize| {
        let chars = text[..byte].chars().count();
        output
            .galley
            .pos_from_cursor(egui::text::CCursor::new(chars))
            .translate(offset)
    };
    let pointer = ui.ctx().pointer_hover_pos();
    let mut hovered = Vec::new();
    let painter = ui.painter();
    for problem in problems {
        let bytes = problem_bytes(text, problem);
        let (start, end) = (at(bytes.start), at(bytes.end));
        // An empty range (an expected token) still gets a short squiggle.
        let right = end.left().max(start.left() + 6.0);
        let color = match problem.severity {
            Severity::Error => palette.error,
            Severity::Warning => palette.warning,
        };
        let y = start.bottom() - 1.0;
        let mut points = Vec::new();
        let mut x = start.left();
        let mut up = false;
        while x < right {
            points.push(egui::pos2(x, if up { y - 2.0 } else { y }));
            x += 2.0;
            up = !up;
        }
        points.push(egui::pos2(right, if up { y - 2.0 } else { y }));
        painter.add(egui::Shape::line(points, egui::Stroke::new(1.0, color)));
        let area = egui::Rect::from_min_max(
            egui::pos2(start.left(), start.top()),
            egui::pos2(right, start.bottom()),
        );
        if pointer.is_some_and(|pointer| area.contains(pointer)) {
            hovered.push(problem);
        }
    }
    if let (Some(pointer), false) = (pointer, hovered.is_empty()) {
        egui::Area::new(output.response.id.with("problem"))
            .order(egui::Order::Tooltip)
            .fixed_pos(pointer + egui::vec2(12.0, 16.0))
            .interactable(false)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_max_width(420.0);
                    for problem in hovered {
                        ui.label(&problem.message);
                        ui.label(
                            egui::RichText::new(format!("luau-lsp: {}", problem.code))
                                .size(theme::type_scale::SMALL)
                                .color(palette.text_weak),
                        );
                    }
                });
            });
    }
}

/// Follows the text box for the completion list and signature hint, puts
/// a pick in, and draws them at the cursor.
fn assist(
    ui: &egui::Ui,
    tab: &mut ScriptTab,
    output: &egui::text_edit::TextEditOutput,
    keys: assist::Keys,
    world: &World,
) {
    let id = output.response.id;
    let cursor = output
        .cursor_range
        .filter(|_| output.response.has_focus())
        .map(|range| assist::byte_of_char(&tab.text, range.primary.index.0));
    let changed = output.response.changed();
    let list = Assist::list_layer(id);
    let pointer_on_list = ui
        .ctx()
        .pointer_latest_pos()
        .is_some_and(|pos| ui.ctx().layer_id_at(pos).is_some_and(|layer| layer == list));
    let mut moved_to =
        tab.assist
            .update(&mut tab.text, cursor, changed, keys, world, pointer_on_list);
    let char_cursor = output.cursor_range.map_or(0, |range| range.primary.index.0);
    let at = output
        .galley
        .pos_from_cursor(egui::text::CCursor::new(char_cursor))
        .translate(output.galley_pos.to_vec2());
    if let Some(index) = tab.assist.show(ui, id, at)
        && let Some(cursor) = cursor.or(tab.assist.last_cursor())
    {
        moved_to = Some(tab.assist.accept(&mut tab.text, cursor, index));
    }
    if let Some(byte) = moved_to {
        let chars = tab.text[..byte].chars().count();
        let mut state = output.state.clone();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(chars),
            )));
        state.store(ui.ctx(), id);
        ui.memory_mut(|memory| memory.request_focus(id));
        tab.highlighted = None;
    }
}

/// A swatch before each colour in a stylesheet (`at` is where the text at
/// a byte of the text is drawn); clicking one opens a colour picker that rewrites the
/// colour in place, as Roblox Studio's script editor does.
fn color_swatches(
    ui: &mut egui::Ui,
    tab: &mut ScriptTab,
    editor: egui::Id,
    swatch: f32,
    at: impl Fn(&str, usize) -> egui::Rect,
) {
    let palette = theme::palette(ui.ctx());
    let visible = ui.clip_rect();
    let open_id = editor.with("color picker");
    let mut open: Option<usize> = ui.data(|data| data.get_temp(open_id));
    let mut replace = None;
    for (range, color) in css_syntax::colors(&tab.text) {
        let glyph = at(&tab.text, range.start);
        if !visible.intersects(glyph) {
            continue;
        }
        let side = (swatch - 6.0).min(glyph.height() - 2.0).max(4.0);
        let rect = egui::Rect::from_center_size(
            egui::pos2(glyph.left() - swatch * 0.5, glyph.center().y),
            egui::Vec2::splat(side),
        );
        let response = ui
            .interact(
                rect,
                editor.with(("swatch", range.start)),
                egui::Sense::click(),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("Pick a colour");
        // See-through colours show over a checker, as colour pickers do.
        if color.a() < 255 {
            let half = rect.width() * 0.5;
            ui.painter().rect_filled(rect, 2.0, egui::Color32::WHITE);
            for (dx, dy) in [(0.0, 0.0), (half, half)] {
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(
                        rect.min + egui::vec2(dx, dy),
                        egui::Vec2::splat(half),
                    ),
                    0.0,
                    egui::Color32::from_gray(190),
                );
            }
        }
        ui.painter().rect_filled(rect, 2.0, color);
        ui.painter().rect_stroke(
            rect,
            2.0,
            egui::Stroke::new(1.0_f32, palette.border_strong),
            egui::StrokeKind::Outside,
        );
        if response.clicked() {
            open = if open == Some(range.start) {
                None
            } else {
                Some(range.start)
            };
        }
        if open != Some(range.start) {
            continue;
        }
        let area = egui::Area::new(open_id)
            .order(egui::Order::Foreground)
            .fixed_pos(rect.left_bottom() + egui::vec2(0.0, 4.0))
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    let mut picked = color;
                    if egui::color_picker::color_picker_color32(
                        ui,
                        &mut picked,
                        egui::color_picker::Alpha::OnlyBlend,
                    ) && picked != color
                    {
                        replace = Some((range.clone(), css_syntax::color_text(picked)));
                    }
                });
            });
        // A click outside the picker (and not on its swatch) closes it.
        let clicked_away = ui.input(|input| {
            input.pointer.any_pressed()
                && input
                    .pointer
                    .interact_pos()
                    .is_some_and(|pos| !area.response.rect.contains(pos) && !rect.contains(pos))
        });
        if clicked_away || ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            open = None;
        }
    }
    // The colour it was opened on went (the text was edited): close it.
    if open.is_some_and(|start| {
        !css_syntax::colors(&tab.text)
            .iter()
            .any(|(range, _)| range.start == start)
    }) {
        open = None;
    }
    if let Some((range, text)) = replace {
        tab.text.replace_range(range, &text);
        tab.highlighted = None;
    }
    ui.data_mut(|data| match open {
        Some(start) => {
            data.insert_temp(open_id, start);
        }
        None => {
            data.remove::<usize>(open_id);
        }
    });
}

/// The line numbers, one row per line, with a red dot on each breakpoint
/// and an arrow on the paused line. Clicking a row returns its line,
/// right-clicking it asks to edit its breakpoint's rule; hovering one shows a
/// faint dot where a click would put one.
fn gutter(
    ui: &mut egui::Ui,
    font: &egui::FontId,
    lines: usize,
    breakpoints: &BTreeMap<usize, Mark>,
    paused_line: Option<usize>,
) -> Option<GutterClick> {
    let palette = theme::palette(ui.ctx());
    let (row, digit) = ui.fonts_mut(|fonts| (fonts.row_height(font), fonts.glyph_width(font, '0')));
    let numbers = digit * lines.to_string().len() as f32;
    let size = egui::vec2(MARKER_WIDTH + numbers + 4.0, row * lines as f32);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let response = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(
            "Click to set or clear a breakpoint: Play pauses before that line runs. \
             Right-click for a condition, a hit count or a log message.",
        );
    let line_at = |y: f32| {
        let line = ((y - rect.top()) / row).floor() as usize + 1;
        (1..=lines).contains(&line).then_some(line)
    };
    let hovered = response.hover_pos().and_then(|pos| line_at(pos.y));
    let at = || {
        response
            .interact_pointer_pos()
            .and_then(|pos| line_at(pos.y))
    };
    let clicked = if response.clicked() {
        at().map(GutterClick::Toggle)
    } else if response.secondary_clicked() {
        at().map(GutterClick::Edit)
    } else {
        None
    };

    // Only the rows on screen are painted, so long scripts stay quick.
    let painter = ui.painter();
    let visible = ui.clip_rect();
    let first = ((visible.top() - rect.top()) / row).floor().max(0.0) as usize + 1;
    let last = (((visible.bottom() - rect.top()) / row).ceil().max(0.0) as usize).min(lines);
    for line in first..=last {
        let top = rect.top() + row * (line - 1) as f32;
        let dot = egui::pos2(rect.left() + MARKER_WIDTH / 2.0, top + row / 2.0);
        if paused_line == Some(line) {
            painter.text(
                dot,
                egui::Align2::CENTER_CENTER,
                icons::CARET_RIGHT,
                egui::FontId::proportional(row),
                palette.warning,
            );
        } else if let Some(mark) = breakpoints.get(&line) {
            let radius = row * 0.28;
            match mark {
                Mark::Plain => {
                    painter.circle_filled(dot, radius, palette.error);
                }
                Mark::Conditional => {
                    painter.circle_filled(dot, radius, palette.error);
                    painter.circle_filled(dot, radius * 0.4, egui::Color32::WHITE);
                }
                Mark::Log => {
                    let r = radius * 1.15;
                    painter.add(egui::Shape::convex_polygon(
                        vec![
                            dot + egui::vec2(0.0, -r),
                            dot + egui::vec2(r, 0.0),
                            dot + egui::vec2(0.0, r),
                            dot + egui::vec2(-r, 0.0),
                        ],
                        palette.error,
                        egui::Stroke::NONE,
                    ));
                }
            }
        } else if hovered == Some(line) {
            painter.circle_filled(dot, row * 0.28, palette.error.gamma_multiply(0.35));
        }
        let color = if paused_line == Some(line) {
            palette.warning
        } else {
            ui.visuals().weak_text_color()
        };
        painter.text(
            egui::pos2(rect.right() - 4.0, top),
            egui::Align2::RIGHT_TOP,
            line.to_string(),
            font.clone(),
            color,
        );
    }
    clicked
}

/// The index (in characters, which is how egui counts) of the start of
/// `line` in `text`, counting lines from 1. A line past the end gives the end.
pub fn char_index_of_line(text: &str, line: usize) -> usize {
    let mut current = 1;
    let mut index = 0;
    for c in text.chars() {
        if current >= line {
            break;
        }
        index += 1;
        if c == '\n' {
            current += 1;
        }
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_starts_count_characters_not_bytes() {
        let text = "a\nüb\n\nlast";
        assert_eq!(char_index_of_line(text, 1), 0);
        assert_eq!(char_index_of_line(text, 2), 2);
        // `ü` is two bytes but one character.
        assert_eq!(char_index_of_line(text, 3), 5);
        assert_eq!(char_index_of_line(text, 4), 6);
        assert_eq!(char_index_of_line(text, 99), text.chars().count());
    }

    #[test]
    fn tabs_open_once_and_closing_shows_a_neighbor() {
        let mut tabs = ScriptTabs::default();
        assert!(tabs.world_shown());
        tabs.open("scripts/a.server.lua", "a", None);
        tabs.open("scripts/b.server.lua", "b", None);
        tabs.open("scripts/c.server.lua", "c", None);
        // Opening an open script shows its tab, keeping its edits.
        tabs.get_mut("scripts/a.server.lua").unwrap().text = "edited".to_string();
        tabs.open("scripts/a.server.lua", "from disk", None);
        assert_eq!(tabs.active().unwrap().text, "edited");
        assert!(!tabs.unsaved_paths().is_empty());
        assert_eq!(tabs.unsaved_paths(), ["scripts/a.server.lua"]);

        tabs.close("scripts/a.server.lua");
        assert_eq!(tabs.active().unwrap().name(), "b");
        tabs.open("scripts/c.server.lua", "c", None);
        tabs.close("scripts/c.server.lua");
        assert_eq!(tabs.active().unwrap().name(), "b");
        tabs.close("scripts/b.server.lua");
        assert!(tabs.world_shown());
    }

    #[test]
    fn tabs_keep_their_id_when_renamed_and_ask_for_focus_when_opened() {
        let mut tabs = ScriptTabs::default();
        tabs.open("scripts/a.server.lua", "a", None);
        let a = tabs.take_focus().unwrap();
        assert_eq!(tabs.take_focus(), None, "asked for once");
        tabs.open_in_background("styles/b.css", "");
        assert_eq!(tabs.take_focus(), None, "background tabs don't take focus");
        tabs.rename("scripts/a.server.lua", "scripts/z.server.lua");
        assert_eq!(tabs.path_of(a), Some("scripts/z.server.lua"));
        assert_eq!(tabs.ids().len(), 2);
        tabs.activate(a);
        assert_eq!(tabs.active_id(), Some(a));
    }

    #[test]
    fn a_stylesheets_colours_get_swatches_that_open_a_picker() {
        let mut tabs = ScriptTabs::default();
        let css =
            "#title {\n    color: #ffcc00;\n    background-color: rgba(20, 40, 200, 0.5);\n}\n";
        tabs.open("ui.css", css, None);
        let ctx = theme::test_context();
        let size = egui::vec2(700.0, 260.0);
        let frame = |tabs: &mut ScriptTabs, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                events,
                ..Default::default()
            };
            let _ = crate::theme::run(&ctx, input, |ui| {
                egui::CentralPanel::default()
                    .show(ui, |ui| tabs.show(ui, "ui.css", &World::default()));
            });
        };
        frame(&mut tabs, Vec::new());
        frame(&mut tabs, Vec::new());
        let editor = editor_id("ui.css");
        let start = css.find("#ffcc00").unwrap();
        let swatch = ctx
            .read_response(editor.with(("swatch", start)))
            .expect("the colour has a swatch")
            .rect;
        // It sits in the gap before the colour, on its row.
        let second = ctx
            .read_response(editor.with(("swatch", css.find("rgba").unwrap())))
            .unwrap()
            .rect;
        assert!(second.top() > swatch.bottom());
        let press = |pressed| egui::Event::PointerButton {
            pos: swatch.center(),
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        frame(&mut tabs, vec![egui::Event::PointerMoved(swatch.center())]);
        frame(&mut tabs, vec![press(true)]);
        frame(&mut tabs, vec![press(false)]);
        frame(&mut tabs, Vec::new());
        let open: Option<usize> = ctx.data(|data| data.get_temp(editor.with("color picker")));
        assert_eq!(open, Some(start), "the picker opened for that colour");
        // Clicking the swatch didn't edit the text.
        assert_eq!(tabs.get_mut("ui.css").unwrap().text, css);
        if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS").map(std::path::PathBuf::from) {
            crate::snapshot::snapshot(
                &folder.join("css-color-swatches.png"),
                size + egui::vec2(0.0, 220.0),
                vec![
                    vec![egui::Event::PointerMoved(swatch.center())],
                    vec![press(true)],
                    vec![press(false)],
                    Vec::new(),
                ],
                |ui| {
                    egui::CentralPanel::default()
                        .show(ui, |ui| tabs.show(ui, "ui.css", &World::default()));
                },
            );
        }
    }

    #[test]
    fn breakpoints_toggle_by_line_and_follow_a_renamed_script() {
        let mut tabs = ScriptTabs::default();
        assert!(!tabs.take_breakpoints_changed());
        tabs.toggle_breakpoint("scripts/a.server.lua", 3);
        tabs.toggle_breakpoint("scripts/a.server.lua", 7);
        assert!(tabs.take_breakpoints_changed());
        assert!(!tabs.take_breakpoints_changed(), "taken once");
        tabs.toggle_breakpoint("scripts/a.server.lua", 3);
        let lines = |tabs: &ScriptTabs, path: &str| -> Vec<usize> {
            tabs.breakpoints()
                .get(path)
                .map(|lines| lines.iter().copied().collect())
                .unwrap_or_default()
        };
        assert_eq!(lines(&tabs, "scripts/a.server.lua"), [7]);
        // Kept for a script whose tab isn't open, and moved with a rename.
        tabs.take_breakpoints_changed();
        tabs.rename("scripts/a.server.lua", "scripts/b.server.lua");
        assert!(tabs.take_breakpoints_changed());
        assert_eq!(lines(&tabs, "scripts/b.server.lua"), [7]);
        assert!(lines(&tabs, "scripts/a.server.lua").is_empty());
        tabs.toggle_breakpoint("scripts/b.server.lua", 7);
        assert!(
            tabs.breakpoints().is_empty(),
            "no empty entries left behind"
        );
    }

    #[test]
    fn breakpoint_rules_follow_their_breakpoint() {
        let mut tabs = ScriptTabs::default();
        let a = "scripts/a.server.lua";
        let rule = BreakpointRule {
            condition: Some("i > 3".to_owned()),
            ..Default::default()
        };
        tabs.set_breakpoint_rule(a, 4, rule.clone());
        assert!(tabs.take_breakpoints_changed());
        assert!(
            tabs.breakpoints()[a].contains(&4),
            "setting a rule sets the breakpoint"
        );
        assert_eq!(
            ScriptTabs::marks(tabs.breakpoints(), tabs.breakpoint_rules(), a)[&4],
            Mark::Conditional
        );
        tabs.rename(a, "scripts/b.server.lua");
        assert_eq!(
            tabs.breakpoint_rules()
                .get(&("scripts/b.server.lua".to_owned(), 4)),
            Some(&rule)
        );
        tabs.toggle_breakpoint("scripts/b.server.lua", 4);
        assert!(
            tabs.breakpoint_rules().is_empty(),
            "cleared with its breakpoint"
        );
        // A plain rule is a plain breakpoint.
        tabs.set_breakpoint_rule(a, 2, BreakpointRule::default());
        assert!(tabs.breakpoint_rules().is_empty());
        assert_eq!(
            ScriptTabs::marks(tabs.breakpoints(), tabs.breakpoint_rules(), a)[&2],
            Mark::Plain
        );
    }

    #[test]
    fn a_rule_is_read_from_its_boxes() {
        let draft = |condition: &str, hit_count: &str, log: &str| RuleDraft {
            path: "scripts/a.server.lua".to_owned(),
            line: 1,
            condition: condition.to_owned(),
            hit_count: hit_count.to_owned(),
            log: log.to_owned(),
            error: None,
        };
        assert_eq!(
            draft(" health < 10 ", "3", "").rule(),
            Ok(BreakpointRule {
                condition: Some("health < 10".to_owned()),
                hit_count: Some(3),
                log: None,
            })
        );
        assert_eq!(
            draft("", "", "hp {health}").rule().map(|rule| rule.log),
            Ok(Some("hp {health}".to_owned()))
        );
        assert!(draft("", "0", "").rule().is_err());
        assert!(draft("", "two", "").rule().is_err());
        assert!(draft("health <", "", "").rule().is_err());
        assert!(draft("", "", "").rule().unwrap().is_plain());
    }

    #[test]
    fn others_typing_keeps_the_cursor_on_its_text() {
        let path = "scripts/live.server.lua";
        let mut tabs = ScriptTabs::default();
        tabs.open(path, "local a = 1\nlocal b = 2\n", Some(2));
        let ctx = theme::test_context();
        frame(&ctx, &mut tabs);
        frame(&ctx, &mut tabs);
        let cursor = |ctx: &egui::Context| {
            let state = egui::TextEdit::load_state(ctx, editor_id(path)).unwrap();
            state.cursor.char_range().unwrap().primary.index.0
        };
        assert_eq!(cursor(&ctx), 12, "the start of line 2");
        // Someone types above the cursor: it moves down with its line.
        let tab = tabs.get_mut(path).unwrap();
        tab.take_remote_text("-- é☕\nlocal a = 1\nlocal b = 2\n".into());
        frame(&ctx, &mut tabs);
        assert_eq!(cursor(&ctx), 18);
        // Below it: it stays.
        let tab = tabs.get_mut(path).unwrap();
        tab.take_remote_text("-- é☕\nlocal a = 1\nlocal b = 2\nprint(b)\n".into());
        frame(&ctx, &mut tabs);
        assert_eq!(cursor(&ctx), 18);
        // The tab says where its cursor is, for others to see (C8).
        assert_eq!(tabs.get(path).unwrap().cursor, Some((18, 18)));
        assert_eq!(changed_chars("abc", "abXc"), (2, 2, 3));
        assert_eq!(changed_chars("aaa", "aa"), (2, 3, 2));
    }

    #[test]
    fn others_cursors_show_in_the_tab_with_their_names() {
        let path = "scripts/live.server.lua";
        let code = "local door = workspace.Door\nlocal open = false\n\nprint(door)\n";
        let mut tabs = ScriptTabs::default();
        tabs.open(path, code, None);
        let tab = tabs.get_mut(path).unwrap();
        tab.others = vec![
            OtherCursor {
                name: "Sam".into(),
                color: egui::Color32::from_rgb(230, 120, 60),
                anchor: 6,
                head: 6,
            },
            // A selection over three lines, its caret at the start.
            OtherCursor {
                name: "Ana".into(),
                color: egui::Color32::from_rgb(80, 160, 240),
                anchor: 49,
                head: 33,
            },
        ];
        // A selection is one band per line, an empty line included.
        assert_eq!(
            others::selection_rows(code, 33, 49),
            [(33, 46), (47, 47), (48, 49)]
        );
        assert_eq!(others::selection_rows(code, 6, 6), [(6, 6)]);
        let ctx = theme::test_context();
        frame(&ctx, &mut tabs);
        if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS").map(std::path::PathBuf::from) {
            crate::snapshot::snapshot(
                &folder.join("script-others-cursors.png"),
                egui::vec2(420.0, 160.0),
                vec![Vec::new(); 2],
                |ui| {
                    egui::CentralPanel::default()
                        .show(ui, |ui| tabs.show(ui, path, &World::default()));
                },
            );
        }
    }

    #[test]
    fn stylesheet_tabs_are_named_by_file_and_can_open_without_showing() {
        let mut tabs = ScriptTabs::default();
        tabs.open_in_background("styles/hud.css", ".hud {}").text = ".hud { width: 1px }".into();
        assert!(tabs.world_shown());
        let tab = tabs.get("styles/hud.css").unwrap();
        assert_eq!(tab.name(), "hud.css");
        assert!(tab.is_stylesheet() && tab.unsaved());
    }

    /// Draws the shown tab's editor for one frame, with no window or GPU.
    fn frame(ctx: &egui::Context, tabs: &mut ScriptTabs) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        };
        let _ = crate::theme::run(ctx, input, |ui| {
            let path = tabs.active_path().unwrap().to_owned();
            egui::CentralPanel::default().show(ui, |ui| tabs.show(ui, &path, &World::default()));
        });
    }

    #[test]
    fn opening_at_a_line_puts_the_cursor_there() {
        let path = "scripts/pad.server.lua";
        let code = (1..=200)
            .map(|n| format!("print({n})\n"))
            .collect::<String>();
        let mut tabs = ScriptTabs::default();
        tabs.open(path, &code, Some(150));
        let ctx = theme::test_context();
        frame(&ctx, &mut tabs);
        frame(&ctx, &mut tabs);

        let state = egui::TextEdit::load_state(&ctx, editor_id(path)).unwrap();
        let cursor = state.cursor.char_range().unwrap().primary;
        assert_eq!(cursor.index.0, char_index_of_line(&code, 150));
        assert!(ctx.memory(|memory| memory.has_focus(editor_id(path))));
    }

    fn problem(line: usize, column: usize, end_column: usize, severity: Severity) -> Diagnostic {
        Diagnostic {
            line,
            column,
            end_line: line,
            end_column,
            severity,
            code: "TypeError".to_owned(),
            message: "Unknown global 'nope'".to_owned(),
        }
    }

    #[test]
    fn a_problem_covers_its_bytes_on_its_first_line_only() {
        let text = "print(nope)\nlocal é = 1\n";
        let at = |problem: &Diagnostic| &text[problem_bytes(text, problem)];
        assert_eq!(at(&problem(1, 7, 10, Severity::Error)), "nope");
        // Past the end of the line stops at the line's end.
        assert_eq!(at(&problem(1, 7, 99, Severity::Error)), "nope)");
        // Over several lines: to the end of the first.
        let mut long = problem(1, 1, 3, Severity::Error);
        long.end_line = 2;
        assert_eq!(at(&long), "print(nope)");
        // Bytes inside a character are kept to its start.
        assert_eq!(at(&problem(2, 7, 7, Severity::Warning)), "");
        assert_eq!(at(&problem(9, 1, 2, Severity::Error)), "");
    }

    #[test]
    fn problems_are_underlined_counted_and_explained_on_hover() {
        let path = "scripts/pad.server.lua";
        let code = "print(nope)\nlocal unused = 1\n";
        let mut tabs = ScriptTabs::default();
        tabs.open(path, code, None);
        let tab = tabs.get_mut(path).unwrap();
        tab.problems = vec![problem(1, 7, 10, Severity::Error), {
            let mut lint = problem(2, 7, 12, Severity::Warning);
            lint.code = "LocalUnused".to_owned();
            lint.message = "Variable 'unused' is never used".to_owned();
            lint
        }];
        // The pointer rests on `nope`.
        let hover = vec![egui::Event::PointerMoved(egui::pos2(95.0, 40.0))];
        let frames = vec![Vec::new(), hover.clone(), hover];
        match std::env::var_os("AXCENITY_SNAPSHOTS") {
            Some(folder) => crate::snapshot::snapshot(
                &std::path::PathBuf::from(folder).join("script-problems.png"),
                egui::vec2(520.0, 200.0),
                frames,
                |ui| {
                    egui::CentralPanel::default()
                        .show(ui, |ui| tabs.show(ui, path, &World::default()));
                },
            ),
            None => {
                let ctx = theme::test_context();
                for events in frames {
                    let input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(520.0, 200.0),
                        )),
                        events,
                        ..Default::default()
                    };
                    let _ = crate::theme::run(&ctx, input, |ui| {
                        egui::CentralPanel::default()
                            .show(ui, |ui| tabs.show(ui, path, &World::default()));
                    });
                }
            }
        }
        // Clicking the count goes to the first error's line.
        let ctx = theme::test_context();
        let at = egui::pos2(60.0, 10.0);
        for events in [
            vec![egui::Event::PointerMoved(at)],
            vec![egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
            vec![egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
            Vec::new(),
        ] {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(520.0, 200.0),
                )),
                events,
                ..Default::default()
            };
            let _ = crate::theme::run(&ctx, input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| tabs.show(ui, path, &World::default()));
            });
        }
        assert!(ctx.memory(|memory| memory.has_focus(editor_id(path))));
        let state = egui::TextEdit::load_state(&ctx, editor_id(path)).unwrap();
        let cursor = state.cursor.char_range().unwrap().primary;
        assert_eq!(cursor.index.0, char_index_of_line(code, 1));
    }

    /// Without the language server a Lua tab offers it, a stylesheet
    /// doesn't, and a download in progress shows how far it's got.
    #[test]
    fn a_lua_tab_offers_the_language_server_while_its_missing() {
        let mut tabs = ScriptTabs::default();
        let path = "scripts/door.server.lua";
        tabs.open(path, "print('hi')\n", None);
        tabs.server = Some(ServerNote::Missing);
        let ctx = theme::test_context();
        frame(&ctx, &mut tabs);
        assert_eq!(tabs.server_request, None);
        if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS").map(std::path::PathBuf::from) {
            for (name, note) in [
                ("missing", ServerNote::Missing),
                ("downloading", ServerNote::Downloading(0.4)),
            ] {
                tabs.server = Some(note);
                crate::snapshot::snapshot(
                    &folder.join(format!("script-language-server-{name}.png")),
                    egui::vec2(560.0, 140.0),
                    vec![Vec::new(); 2],
                    |ui| {
                        egui::CentralPanel::default()
                            .show(ui, |ui| tabs.show(ui, path, &World::default()));
                    },
                );
            }
        }
    }
}
