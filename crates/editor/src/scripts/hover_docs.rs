//! Hover docs (scripting P2 PR 5): resting the pointer on an engine name in
//! a Lua code tab (`part.Touched`, `task.wait`, `workspace:Raycast`) shows
//! its API Reference entry beside it: how it's written, what it does, and
//! its example when it has one. F1 opens the full entry. The name is found
//! the way Go to Definition finds it (`script_completion::definition_at`),
//! so it follows the same chains and no more.

use std::hash::{Hash, Hasher};

use axcenity_core::World;
use eframe::egui;

use crate::lua_syntax;
use crate::script_completion::{self, Definition};
use crate::theme::{self, type_scale};

/// How long the pointer must rest before the docs show.
const REST_SECONDS: f64 = 0.4;
/// How many lines of an example the popup shows.
const EXAMPLE_LINES: usize = 8;

/// The API entry the pointer rests on, by its API Reference name, as the
/// last frame worked it out for this text.
#[derive(Clone)]
struct Found {
    byte: usize,
    text: u64,
    name: Option<String>,
}

fn text_hash(text: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// The byte of `text` the pointer is over, if it's over a character (not
/// past a line's end).
fn byte_under(
    text: &str,
    output: &egui::text_edit::TextEditOutput,
    pointer: egui::Pos2,
) -> Option<usize> {
    let offset = output.galley_pos.to_vec2();
    let cursor = output.galley.cursor_from_pos(pointer.to_vec2() - offset);
    let index = cursor.index.0;
    let here = output.galley.pos_from_cursor(cursor).translate(offset);
    let next = output
        .galley
        .pos_from_cursor(egui::text::CCursor::new(index + 1))
        .translate(offset);
    // The cursor snaps to the nearest gap between characters; the
    // character under the pointer is the one after or before that gap.
    let index = if pointer.x < here.left() {
        index.checked_sub(1)?
    } else {
        index
    };
    let character = text.chars().nth(index)?;
    if character == '\n' || (pointer.y < here.top() || pointer.y > here.bottom()) {
        return None;
    }
    if pointer.x > next.left().max(here.left()) + 1.0 && next.top() == here.top() {
        return None;
    }
    Some(super::assist::byte_of_char(text, index))
}

/// A doc with its `code` spans in the code font.
fn doc_job(doc: &str, ui: &egui::Ui) -> egui::text::LayoutJob {
    let palette = theme::palette(ui.ctx());
    let mut job = egui::text::LayoutJob::default();
    for (index, part) in doc.split('`').enumerate() {
        let format = if index % 2 == 1 {
            egui::TextFormat::simple(
                egui::FontId::monospace(type_scale::BODY - 1.0),
                palette.text_strong,
            )
        } else {
            egui::TextFormat::simple(egui::FontId::proportional(type_scale::BODY), palette.text)
        };
        job.append(part, 0.0, format);
    }
    job
}

/// Draws the docs for the name under the pointer, once it has rested
/// there. Returns the entry's name when one is shown.
pub(super) fn show(
    ui: &egui::Ui,
    text: &str,
    output: &egui::text_edit::TextEditOutput,
    world: &World,
) -> Option<String> {
    let pointer = ui.ctx().pointer_hover_pos()?;
    let dragging = ui.input(|input| input.pointer.any_down());
    if !output.response.rect.contains(pointer) || dragging {
        return None;
    }
    let rested = ui.input(|input| input.pointer.time_since_last_movement()) as f64;
    if rested < REST_SECONDS {
        ui.ctx()
            .request_repaint_after_secs((REST_SECONDS - rested) as f32 + 0.01);
        return None;
    }
    let byte = byte_under(text, output, pointer)?;
    let id = output.response.id.with("hover docs");
    let hash = text_hash(text);
    let cached = ui
        .data(|data| data.get_temp::<Found>(id))
        .filter(|found| found.byte == byte && found.text == hash);
    let name = match cached {
        Some(found) => found.name,
        None => {
            let name = match script_completion::definition_at(text, byte, world) {
                Some(Definition::Api(name)) => Some(name),
                _ => None,
            };
            let found = Found {
                byte,
                text: hash,
                name: name.clone(),
            };
            ui.data_mut(|data| data.insert_temp(id, found));
            name
        }
    }?;
    let entry = script_completion::api_entry(&name)?;
    let palette = theme::palette(ui.ctx());
    egui::Area::new(id.with("popup"))
        .order(egui::Order::Tooltip)
        .fixed_pos(pointer + egui::vec2(0.0, -10.0))
        .pivot(egui::Align2::LEFT_BOTTOM)
        .interactable(false)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_max_width(440.0);
                ui.label(
                    egui::RichText::new(format!("{}  ·  {}", entry.kind.label(), name))
                        .size(type_scale::SMALL)
                        .color(palette.text_weak),
                );
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(&entry.signature)
                            .monospace()
                            .color(palette.text_strong),
                    )
                    .wrap(),
                );
                if !entry.doc.is_empty() {
                    ui.add_space(4.0);
                    ui.add(egui::Label::new(doc_job(&entry.doc, ui)).wrap());
                }
                if let Some(example) = axcenity_script::api_example(&name) {
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new("Example")
                            .size(type_scale::SMALL)
                            .color(palette.text_secondary)
                            .strong(),
                    );
                    let mut shown: Vec<&str> = example.lines().take(EXAMPLE_LINES).collect();
                    if example.lines().count() > EXAMPLE_LINES {
                        shown.push("…");
                    }
                    let font = egui::FontId::monospace(type_scale::BODY - 1.0);
                    let job = lua_syntax::layout_job(&shown.join("\n"), font, ui.visuals());
                    egui::Frame::new()
                        .fill(palette.inset)
                        .corner_radius(theme::radius::CONTROL)
                        .inner_margin(egui::Margin::same(6))
                        .show(ui, |ui| ui.label(job));
                }
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("F1 opens it in the API Reference")
                        .size(type_scale::SMALL)
                        .color(palette.text_weak),
                );
            });
        });
    Some(name)
}
