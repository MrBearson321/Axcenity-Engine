//! Script tabs: a strip of tabs above the center of the window, with the
//! 3D view ("World") first and one tab per open script, like Roblox Studio.
//! A script tab shows a code editor with Lua syntax highlighting and line
//! numbers in place of the viewport.
//!
//! Tabs are found by their script's file path, not its Instance, because
//! the file is what's being edited: it outlives its Script Instance when the
//! world is reloaded, and a tab can stay open after its file was deleted.
//! [`ScriptTabs`] never touches files; the app reads and writes them.

use std::collections::HashMap;

use axcenity_core::ClassName;
use axcenity_project::script_name;
use eframe::egui;

use crate::lua_syntax;
use crate::side_warnings::SideWarning;
use crate::theme::{self, icons};

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
}

impl ScriptTab {
    /// The Script's name, from its file name.
    pub fn name(&self) -> &str {
        script_name(&self.path)
    }

    pub fn unsaved(&self) -> bool {
        self.text != self.saved || self.disk == Disk::Deleted
    }

    /// The line the cursor goes to in the next frame, if any.
    #[cfg(test)]
    pub fn go_to_line(&self) -> Option<usize> {
        self.go_to_line
    }

    /// The text was written to the file: no unsaved changes now.
    pub fn mark_saved(&mut self) {
        self.saved = self.text.clone();
        self.disk = Disk::Same;
    }
}

/// The open script tabs, and which tab is shown.
#[derive(Default)]
pub struct ScriptTabs {
    tabs: Vec<ScriptTab>,
    /// The tab shown, or `None` for the World tab (the viewport).
    active: Option<usize>,
    /// Each script's uses of APIs that only work on the other side, by
    /// path. The app fills it in each frame; a tab with any shows a warning sign.
    pub warnings: HashMap<String, Vec<SideWarning>>,
}

impl ScriptTabs {
    /// Shows the tab for the script at `path`, opening it with `source` if
    /// it isn't open yet. With a `line`, the cursor goes to the start of that line.
    pub fn open(&mut self, path: &str, source: &str, line: Option<usize>) {
        let index = match self.tabs.iter().position(|tab| tab.path == path) {
            Some(index) => index,
            None => {
                self.tabs.push(ScriptTab {
                    path: path.to_string(),
                    text: source.to_string(),
                    saved: source.to_string(),
                    disk: Disk::Same,
                    go_to_line: None,
                    highlighted: None,
                });
                self.tabs.len() - 1
            }
        };
        if line.is_some() {
            self.tabs[index].go_to_line = line;
        }
        self.active = Some(index);
    }

    /// Shows the World tab: the viewport.
    pub fn show_world(&mut self) {
        self.active = None;
    }

    pub fn world_shown(&self) -> bool {
        self.active.is_none()
    }

    /// The script tab shown, if it isn't the World tab.
    #[cfg(test)]
    pub fn active(&self) -> Option<&ScriptTab> {
        self.tabs.get(self.active?)
    }

    pub fn get(&self, path: &str) -> Option<&ScriptTab> {
        self.tabs.iter().find(|tab| tab.path == path)
    }

    pub fn get_mut(&mut self, path: &str) -> Option<&mut ScriptTab> {
        self.tabs.iter_mut().find(|tab| tab.path == path)
    }

    /// Closes the tab for `path`, if it's open. If it was shown, the tab
    /// that takes its place in the strip is shown instead (or the one before
    /// it, at the end, or the World tab when no script tabs are left).
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

    /// Closes every tab and shows the World tab.
    pub fn close_all(&mut self) {
        self.tabs.clear();
        self.active = None;
    }

    /// The script at `from` was renamed to `to`.
    pub fn rename(&mut self, from: &str, to: &str) {
        if let Some(tab) = self.get_mut(from) {
            tab.path = to.to_string();
        }
    }

    pub fn any_unsaved(&self) -> bool {
        self.tabs.iter().any(ScriptTab::unsaved)
    }

    /// The paths of the tabs with unsaved changes, in tab order.
    pub fn unsaved_paths(&self) -> Vec<String> {
        self.tabs
            .iter()
            .filter(|tab| tab.unsaved())
            .map(|tab| tab.path.clone())
            .collect()
    }

    /// Draws the strip of tabs. Returns the path of a tab whose close
    /// button (or middle click) asked to close it: the app checks for
    /// unsaved changes first.
    pub fn tab_bar(&mut self, ui: &mut egui::Ui) -> Option<String> {
        let mut show = None;
        let mut close = None;
        egui::ScrollArea::horizontal().show(ui, |ui| {
            ui.horizontal(|ui| {
                let world = theme::with_icon(icons::GLOBE_HEMISPHERE_WEST, "World");
                if ui
                    .selectable_label(self.active.is_none(), world)
                    .on_hover_text("The 3D view of the world")
                    .clicked()
                {
                    show = Some(None);
                }
                for (index, tab) in self.tabs.iter().enumerate() {
                    ui.separator();
                    // A dot after the name means unsaved changes, as in VS Code.
                    let name = if tab.unsaved() {
                        format!("{} ●", tab.name())
                    } else {
                        tab.name().to_string()
                    };
                    let class = axcenity_project::script_class(&tab.path);
                    let icon = egui::RichText::new(theme::class_icon(class))
                        .color(theme::class_color(class, ui.visuals()));
                    let mut title = egui::Atoms::new((icon, name));
                    if self.warnings.contains_key(&tab.path) {
                        let warning = ui.visuals().warn_fg_color;
                        title.push_right(egui::RichText::new(icons::WARNING).color(warning));
                    }
                    let label = ui
                        .add(egui::Button::selectable(self.active == Some(index), title))
                        .on_hover_text(&tab.path);
                    if label.clicked() {
                        show = Some(Some(index));
                    }
                    let cross = ui.small_button(icons::X).on_hover_text("Close");
                    if label.middle_clicked() || cross.clicked() {
                        close = Some(tab.path.clone());
                    }
                }
            });
        });
        if let Some(active) = show {
            self.active = active;
        }
        close
    }

    /// Draws the code editor for the tab shown (nothing for the World tab).
    pub fn show_active(&mut self, ui: &mut egui::Ui) {
        let Some(tab) = self.active.and_then(|index| self.tabs.get_mut(index)) else {
            return;
        };
        let warning = ui.visuals().warn_fg_color;
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
        code_editor(ui, tab);
    }
}

/// The ID of the code editor's text box for the script at `path`. egui
/// keeps the cursor and scroll position under it.
pub fn editor_id(path: &str) -> egui::Id {
    egui::Id::new(("script editor", path))
}

/// The text box with the code, colored, with line numbers down the left.
fn code_editor(ui: &mut egui::Ui, tab: &mut ScriptTab) {
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    let id = editor_id(&tab.path);
    // `split` counts an empty last line after a final newline, which is
    // where the cursor can go, so the numbers match the text box's rows.
    let lines = tab.text.split('\n').count();
    let width = lines.to_string().len();
    let numbers: Vec<String> = (1..=lines).map(|n| format!("{n:>width$}")).collect();

    // egui calls this to lay out the text whenever it needs to. It ignores
    // the wrap width: long lines scroll sideways, so they stay one row each
    // and line up with the numbers.
    let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, _wrap_width: f32| {
        let job = lua_syntax::layout_job(text.as_str(), font.clone(), ui.visuals());
        ui.fonts(|fonts| fonts.layout_job(job))
    };

    egui::ScrollArea::both()
        .id_salt(id)
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                let gutter = egui::RichText::new(numbers.join("\n"))
                    .font(font.clone())
                    .color(ui.visuals().weak_text_color());
                ui.add(egui::Label::new(gutter).selectable(false));

                // A placeholder painted before the text, so the jump
                // highlight can go behind it once the text is laid out.
                let behind = ui.painter().add(egui::Shape::Noop);
                let output = egui::TextEdit::multiline(&mut tab.text)
                    .id(id)
                    .font(font.clone())
                    .code_editor()
                    .frame(false)
                    .margin(egui::Margin::ZERO)
                    .desired_width(f32::INFINITY)
                    .min_size(ui.available_size())
                    .layouter(&mut layouter)
                    .show(ui);
                if output.response.changed() {
                    tab.highlighted = None;
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
                if let Some(line) = tab.highlighted {
                    let (_, at) = line_rect(line);
                    let row =
                        egui::Rect::from_x_y_ranges(output.response.rect.x_range(), at.y_range());
                    ui.painter()
                        .set(behind, egui::Shape::rect_filled(row, 0.0, JUMP_LINE_COLOR));
                }
            });
        });
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
        assert!(tabs.any_unsaved());
        assert_eq!(tabs.unsaved_paths(), ["scripts/a.server.lua"]);

        tabs.close("scripts/a.server.lua");
        assert_eq!(tabs.active().unwrap().name(), "b");
        tabs.open("scripts/c.server.lua", "c", None);
        tabs.close("scripts/c.server.lua");
        assert_eq!(tabs.active().unwrap().name(), "b");
        tabs.close("scripts/b.server.lua");
        assert!(tabs.world_shown());
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
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| tabs.show_active(ui));
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
        assert_eq!(cursor.index, char_index_of_line(&code, 150));
        assert!(ctx.memory(|memory| memory.has_focus(editor_id(path))));
    }
}
