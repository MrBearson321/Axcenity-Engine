//! The editor's question boxes ("Save changes?", "Delete script?"): an
//! icon in a colored tile, a title and a message, an optional list of what
//! it's about, and a row of buttons with icons, the main one filled.
//! Enter picks the main button; Escape or a click outside cancels.

use eframe::egui::{self, Color32, CornerRadius, FontId, RichText, Stroke, vec2};

use crate::theme::{self, icons};

/// How a button looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Filled in the accent color: what Enter does.
    Main,
    /// Filled red: something that can't be undone.
    Danger,
    /// Just an outline.
    Plain,
}

pub struct Button<'a> {
    pub icon: &'static str,
    pub label: &'a str,
    pub kind: Kind,
}

/// What the box is about, besides its buttons.
pub struct Question<'a> {
    pub id: &'a str,
    pub icon: &'static str,
    pub accent: Color32,
    pub title: &'a str,
    pub message: &'a str,
    /// Things to list under the message (such as the unsaved files), each
    /// with its icon.
    pub items: Vec<(&'static str, String)>,
}

/// What happened to the box this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The button at this index was picked.
    Picked(usize),
    /// Escape, or a click outside the box.
    Dismissed,
}

const WIDTH: f32 = 440.0;

/// Shows the box. Returns what was picked, if anything, this frame.
pub fn ask(ctx: &egui::Context, question: &Question, buttons: &[Button]) -> Option<Outcome> {
    let mut outcome = None;
    let frame = egui::Frame::popup(&ctx.style())
        .corner_radius(14)
        .inner_margin(egui::Margin::same(22));
    let modal = egui::Modal::new(egui::Id::new(question.id))
        .frame(frame)
        .show(ctx, |ui| {
            ui.set_width(WIDTH);
            ui.horizontal_top(|ui| {
                icon_tile(ui, question.icon, question.accent);
                ui.add_space(6.0);
                ui.vertical(|ui| {
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(question.title)
                            .font(FontId::new(18.0, theme::heading_family()))
                            .strong(),
                    );
                    ui.add_space(4.0);
                    ui.add(egui::Label::new(question.message).wrap());
                });
            });
            if !question.items.is_empty() {
                ui.add_space(12.0);
                egui::Frame::new()
                    .fill(ui.visuals().extreme_bg_color)
                    .corner_radius(8)
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        for (icon, text) in question.items.iter().take(6) {
                            ui.label(theme::with_icon(icon, text));
                        }
                        if question.items.len() > 6 {
                            ui.weak(format!("and {} more", question.items.len() - 6));
                        }
                    });
            }
            ui.add_space(18.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                // Right to left, so the main button ends up at the right.
                for (index, button) in buttons.iter().enumerate().rev() {
                    if dialog_button(ui, button).clicked() {
                        outcome = Some(Outcome::Picked(index));
                    }
                }
            });
        });
    if outcome.is_none() {
        let enter = ctx.input(|input| input.key_pressed(egui::Key::Enter));
        let main = buttons.iter().position(|button| button.kind == Kind::Main);
        if let (true, Some(main)) = (enter, main) {
            outcome = Some(Outcome::Picked(main));
        } else if modal.should_close() {
            outcome = Some(Outcome::Dismissed);
        }
    }
    outcome
}

/// The box's icon, in a rounded tile of its accent color.
fn icon_tile(ui: &mut egui::Ui, icon: &str, accent: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(44.0, 44.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(12), accent.gamma_multiply(0.18));
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon,
        FontId::new(24.0, theme::filled()),
        accent,
    );
}

fn dialog_button(ui: &mut egui::Ui, button: &Button) -> egui::Response {
    let text = theme::with_icon(button.icon, button.label);
    let (fill, color) = match button.kind {
        Kind::Main => (theme::BRAND_BLUE_BRIGHT, Color32::WHITE),
        Kind::Danger => (theme::STOP_RED, Color32::WHITE),
        Kind::Plain => (Color32::TRANSPARENT, ui.visuals().text_color()),
    };
    let mut widget = egui::Button::new(RichText::new(text).color(color))
        .corner_radius(8)
        .min_size(vec2(96.0, 32.0));
    widget = if button.kind == Kind::Plain {
        widget.stroke(ui.visuals().widgets.inactive.bg_stroke)
    } else {
        widget.fill(fill).stroke(Stroke::NONE)
    };
    ui.add(widget)
}

/// The unsaved-changes box's icon and color.
pub fn unsaved_style() -> (&'static str, Color32) {
    (icons::FLOPPY_DISK, Color32::from_rgb(240, 170, 50))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn question_boxes_draw_and_escape_dismisses_them() {
        let snapshots = std::env::var_os("AXCENITY_SNAPSHOTS").map(std::path::PathBuf::from);
        let (icon, accent) = unsaved_style();
        // Shared with the drawing closure, which pushes each frame's outcome.
        let outcomes = std::cell::RefCell::new(Vec::new());
        let mut draw = |ctx: &egui::Context| {
            egui::CentralPanel::default().show(ctx, |_| {});
            outcomes.borrow_mut().push(ask(
                ctx,
                &Question {
                    id: "test",
                    icon,
                    accent,
                    title: "Save changes?",
                    message: "\"Space Race\" has unsaved changes. Save them before closing?",
                    items: vec![
                        (icons::GLOBE_HEMISPHERE_WEST, "The world".to_owned()),
                        (icons::FILE_CODE, "scripts/Score.server.lua".to_owned()),
                    ],
                },
                &[
                    Button {
                        icon: icons::X,
                        label: "Cancel",
                        kind: Kind::Plain,
                    },
                    Button {
                        icon: icons::TRASH,
                        label: "Don't Save",
                        kind: Kind::Plain,
                    },
                    Button {
                        icon: icons::FLOPPY_DISK,
                        label: "Save",
                        kind: Kind::Main,
                    },
                ],
            ));
        };
        if let Some(folder) = &snapshots {
            crate::snapshot::snapshot(
                &folder.join("dialog-save.png"),
                vec2(700.0, 420.0),
                vec![Vec::new(); 3],
                &mut draw,
            );
        }
        let ctx = theme::test_context();
        let escape = egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        };
        outcomes.borrow_mut().clear();
        for events in [vec![], vec![escape]] {
            let input = egui::RawInput {
                events,
                ..Default::default()
            };
            let _ = ctx.run(input, &mut draw);
        }
        assert_eq!(*outcomes.borrow(), [None, Some(Outcome::Dismissed)]);
    }
}
