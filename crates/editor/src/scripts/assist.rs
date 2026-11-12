//! Completion and signature help in a code tab (roadmap G4, stage 1): the
//! list under the cursor and the call's signature above it. What they
//! offer comes from `script_completion`; this keeps them open or closed,
//! takes their keys from the text box, and draws them.
//!
//! The list opens after `.` or `:` and on Ctrl+Space, narrows as a name is
//! typed, and closes when the cursor moves away or something else is typed.
//! Up and Down pick, Enter or Tab puts the pick in, Escape closes. The
//! signature shows after `(` or `,` while the cursor stays in the call.

use axcenity_core::World;
use eframe::egui;

use crate::script_completion::{self, Item, ItemKind, Signature};
use crate::theme::{self, icons};

/// The rows shown before the list scrolls.
const VISIBLE_ROWS: f32 = 8.0;
const WIDTH: f32 = 360.0;

/// A code tab's completion list and signature hint.
#[derive(Default)]
pub struct Assist {
    popup: Option<Popup>,
    signature: Option<Signature>,
    /// The cursor (bytes) in the last frame, to see it move.
    cursor: Option<usize>,
}

struct Popup {
    /// Where the word being completed starts, in bytes.
    start: usize,
    items: Vec<Item>,
    selected: usize,
    /// The pick moved with the keys this frame: scroll to it.
    scroll: bool,
}

/// The keys taken from the text box this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Keys {
    pub open: bool,
    pub up: bool,
    pub down: bool,
    pub accept: bool,
    pub close: bool,
}

impl Assist {
    #[cfg(test)]
    pub fn is_open(&self) -> bool {
        self.popup.is_some()
    }

    /// The labels on offer, in order.
    #[cfg(test)]
    pub fn labels(&self) -> Vec<&str> {
        self.popup
            .iter()
            .flat_map(|popup| popup.items.iter().map(|item| item.label.as_str()))
            .collect()
    }

    #[cfg(test)]
    pub fn signature(&self) -> Option<&Signature> {
        self.signature.as_ref()
    }

    /// Takes the keys the list and hint use, before the text box reads
    /// them, while the box has focus.
    pub fn take_keys(&self, ui: &egui::Ui, focused: bool) -> Keys {
        if !focused {
            return Keys::default();
        }
        let listing = self.popup.is_some();
        let hinting = self.signature.is_some();
        ui.input_mut(|input| {
            let none = egui::Modifiers::NONE;
            let mut keys = Keys {
                open: input.consume_key(egui::Modifiers::CTRL, egui::Key::Space),
                ..Keys::default()
            };
            if listing {
                keys.down = input.consume_key(none, egui::Key::ArrowDown);
                keys.up = input.consume_key(none, egui::Key::ArrowUp);
                keys.accept = input.consume_key(none, egui::Key::Enter)
                    || input.consume_key(none, egui::Key::Tab);
            }
            if listing || hinting {
                keys.close = input.consume_key(none, egui::Key::Escape);
            }
            keys
        })
    }

    /// The cursor (bytes) when the text box last had focus.
    pub fn last_cursor(&self) -> Option<usize> {
        self.cursor
    }

    /// Follows what happened in the text box this frame: `cursor` is where
    /// the cursor is (bytes), or `None` when the box doesn't have focus.
    /// Returns where the cursor goes when a pick was put in.
    ///
    /// Pressing on the list takes focus from the text box before the click
    /// lands, so the list stays open while `pointer_on_list`.
    pub fn update(
        &mut self,
        text: &mut String,
        cursor: Option<usize>,
        changed: bool,
        keys: Keys,
        world: &World,
        pointer_on_list: bool,
    ) -> Option<usize> {
        if cursor.is_none() && pointer_on_list && self.popup.is_some() {
            return None;
        }
        let last = std::mem::replace(&mut self.cursor, cursor);
        let Some(cursor) = cursor.filter(|&cursor| text.is_char_boundary(cursor)) else {
            self.popup = None;
            self.signature = None;
            return None;
        };
        if keys.close {
            self.popup = None;
            self.signature = None;
            return None;
        }
        if let Some(popup) = &mut self.popup {
            let count = popup.items.len();
            if keys.down {
                popup.selected = (popup.selected + 1) % count;
                popup.scroll = true;
            }
            if keys.up {
                popup.selected = (popup.selected + count - 1) % count;
                popup.scroll = true;
            }
            if keys.accept {
                let selected = popup.selected;
                return Some(self.accept(text, cursor, selected));
            }
        }

        let typed = if changed {
            text[..cursor].chars().next_back()
        } else {
            None
        };
        let moved = !changed && last.is_some_and(|last| last != cursor);
        let opener = matches!(typed, Some('.' | ':'));
        let in_word = typed.is_some_and(|c| c.is_alphanumeric() || c == '_');
        if keys.open || opener || (self.popup.is_some() && in_word) {
            self.popup = script_completion::complete(text, cursor, world)
                .filter(|completions| !completions.items.is_empty())
                .map(|completions| Popup {
                    start: completions.start,
                    items: completions.items,
                    selected: 0,
                    scroll: true,
                });
        } else if changed || moved {
            self.popup = None;
        }

        if matches!(typed, Some('(' | ',')) || (self.signature.is_some() && (changed || moved)) {
            self.signature = script_completion::signature_at(text, cursor, world);
        }
        None
    }

    /// Puts item `index` in place of the word before `cursor`, and returns
    /// where the cursor goes: after it.
    pub fn accept(&mut self, text: &mut String, cursor: usize, index: usize) -> usize {
        let Some(popup) = self.popup.take() else {
            return cursor;
        };
        let Some(item) = popup.items.get(index) else {
            return cursor;
        };
        let start = popup.start.min(cursor);
        text.replace_range(start..cursor, &item.label);
        start + item.label.len()
    }

    /// The list's layer, to see whether the pointer is on it.
    pub fn list_layer(id: egui::Id) -> egui::LayerId {
        egui::LayerId::new(egui::Order::Foreground, id.with("completion"))
    }

    /// Draws the list under `at` (the cursor's rectangle on screen) and
    /// the hint above it. Returns the item clicked, if one was.
    pub fn show(&mut self, ui: &egui::Ui, id: egui::Id, at: egui::Rect) -> Option<usize> {
        let palette = theme::palette(ui.ctx());
        if let Some(signature) = &self.signature {
            egui::Area::new(id.with("signature"))
                .order(egui::Order::Foreground)
                .pivot(egui::Align2::LEFT_BOTTOM)
                .fixed_pos(at.left_top())
                .interactable(false)
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_max_width(WIDTH * 1.5);
                        ui.label(signature_job(
                            ui,
                            signature,
                            palette.text,
                            palette.accent_text,
                        ));
                        if !signature.doc.is_empty() {
                            small(ui, &signature.doc, palette.text_weak);
                        }
                    });
                });
        }
        let popup = self.popup.as_mut()?;
        let mut clicked = None;
        egui::Area::new(id.with("completion"))
            .order(egui::Order::Foreground)
            .fixed_pos(at.left_bottom())
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(WIDTH);
                    let font = egui::TextStyle::Monospace.resolve(ui.style());
                    let row = ui.fonts_mut(|fonts| fonts.row_height(&font))
                        + ui.spacing().button_padding.y * 2.0
                        + ui.spacing().item_spacing.y;
                    egui::ScrollArea::vertical()
                        .id_salt(id.with("completion rows"))
                        .max_height(row * VISIBLE_ROWS)
                        .show(ui, |ui| {
                            // Full-width rows, their text on the left.
                            ui.with_layout(
                                egui::Layout::top_down_justified(egui::Align::LEFT),
                                |ui| {
                                    for (index, item) in popup.items.iter().enumerate() {
                                        let selected = index == popup.selected;
                                        let text = egui::RichText::new(format!(
                                            "{}  {}",
                                            icon(item.kind),
                                            item.label
                                        ))
                                        .monospace();
                                        let response =
                                            ui.add(egui::Button::selectable(selected, text));
                                        if selected && popup.scroll {
                                            response.scroll_to_me(None);
                                        }
                                        if response.clicked() {
                                            clicked = Some(index);
                                        }
                                    }
                                },
                            );
                        });
                    popup.scroll = false;
                    if let Some(item) = popup.items.get(popup.selected)
                        && (!item.detail.is_empty() || !item.doc.is_empty())
                    {
                        ui.separator();
                        if !item.detail.is_empty() {
                            ui.label(
                                egui::RichText::new(&item.detail)
                                    .monospace()
                                    .color(palette.text_strong),
                            );
                        }
                        if !item.doc.is_empty() {
                            small(ui, &item.doc, palette.text_weak);
                        }
                    }
                });
            });
        clicked
    }
}

fn small(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    ui.add(
        egui::Label::new(
            egui::RichText::new(text)
                .size(theme::type_scale::SMALL)
                .color(color),
        )
        .wrap(),
    );
}

fn icon(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Keyword => icons::CODE,
        ItemKind::Local => icons::TAG,
        ItemKind::Global => icons::GLOBE,
        ItemKind::Child => icons::CUBE,
        ItemKind::Property => icons::SLIDERS,
        ItemKind::Event => icons::LIGHTNING,
        ItemKind::Function => icons::FUNCTION,
        ItemKind::Enum => icons::LIST_BULLETS,
        ItemKind::EnumItem => icons::DOT_OUTLINE,
        ItemKind::Name => icons::QUOTES,
    }
}

/// The signature in monospace, with the argument the cursor is on in
/// `strong`.
fn signature_job(
    ui: &egui::Ui,
    signature: &Signature,
    text: egui::Color32,
    strong: egui::Color32,
) -> egui::text::LayoutJob {
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    let mut job = egui::text::LayoutJob::default();
    let format = |color| egui::TextFormat::simple(font.clone(), color);
    match argument_range(&signature.label, signature.argument) {
        Some(range) => {
            job.append(&signature.label[..range.start], 0.0, format(text));
            job.append(&signature.label[range.clone()], 0.0, format(strong));
            job.append(&signature.label[range.end..], 0.0, format(text));
        }
        None => job.append(&signature.label, 0.0, format(text)),
    }
    job
}

/// Where argument `index` is in a signature such as
/// `Instance:FindFirstChild(name: string, recursive: boolean?): Instance?`:
/// the parameters split at the commas between its outer brackets.
pub fn argument_range(label: &str, index: usize) -> Option<std::ops::Range<usize>> {
    let open = label.find('(')?;
    let mut depth = 0usize;
    let mut start = open + 1;
    let mut argument = 0;
    for (at, c) in label[open..].char_indices().map(|(at, c)| (at + open, c)) {
        match c {
            '(' | '{' | '[' => depth += 1,
            ')' | '}' | ']' => {
                depth -= 1;
                if depth == 0 {
                    return (argument == index && at > start).then(|| trimmed(label, start..at));
                }
            }
            ',' if depth == 1 => {
                if argument == index {
                    return Some(trimmed(label, start..at));
                }
                argument += 1;
                start = at + 1;
            }
            _ => {}
        }
    }
    None
}

/// `range` without the spaces at its ends.
fn trimmed(label: &str, range: std::ops::Range<usize>) -> std::ops::Range<usize> {
    let text = &label[range.clone()];
    let start = range.start + (text.len() - text.trim_start().len());
    let end = range.end - (text.len() - text.trim_end().len());
    start..end.max(start)
}

/// The byte where char `index` of `text` starts.
pub fn byte_of_char(text: &str, index: usize) -> usize {
    text.char_indices()
        .nth(index)
        .map_or(text.len(), |(byte, _)| byte)
}

#[cfg(test)]
mod tests;
