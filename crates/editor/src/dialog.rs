//! The editor's question boxes ("Save changes?", "Delete script?"): an
//! icon in a tinted tile, a title and a message, an optional list of what
//! it's about, and a row of buttons, the main one filled. Enter picks the
//! main button; Escape or a click outside cancels. The box fades in (see
//! `theme::MOTION_SECONDS`).

use eframe::egui::{self, CornerRadius, FontId, RichText, Stroke, vec2};

use crate::theme::{self, icons, type_scale};
use crate::widgets::{self, Role};

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

/// The colour of the box's icon, by what kind of question it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    /// Something might be lost, such as unsaved changes.
    Warning,
    /// Something will be destroyed.
    Danger,
}

/// What the box is about, besides its buttons.
pub struct Question<'a> {
    pub id: &'a str,
    pub icon: &'static str,
    pub tone: Tone,
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

const WIDTH: f32 = 420.0;

/// Shows the box. Returns what was picked, if anything, this frame.
pub fn ask(ctx: &egui::Context, question: &Question, buttons: &[Button]) -> Option<Outcome> {
    let mut outcome = None;
    let palette = theme::palette(ctx);
    let accent = match question.tone {
        Tone::Warning => palette.warning,
        Tone::Danger => palette.error,
    };
    let frame = egui::Frame::popup(&ctx.style())
        .corner_radius(12)
        .inner_margin(egui::Margin::same(20));
    let modal = egui::Modal::new(egui::Id::new(question.id))
        .frame(frame)
        .show(ctx, |ui| {
            ui.set_width(WIDTH);
            ui.horizontal_top(|ui| {
                icon_tile(ui, question.icon, accent);
                ui.add_space(4.0);
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(question.title)
                            .font(FontId::new(
                                type_scale::HEADING + 1.0,
                                theme::heading_family(),
                            ))
                            .color(palette.text_strong),
                    );
                    ui.add_space(2.0);
                    ui.add(
                        egui::Label::new(
                            RichText::new(question.message).color(palette.text_secondary),
                        )
                        .wrap(),
                    );
                });
            });
            if !question.items.is_empty() {
                ui.add_space(14.0);
                egui::Frame::new()
                    .fill(palette.inset)
                    .stroke(Stroke::new(1.0_f32, palette.border))
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
            ui.add_space(20.0);
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

/// The box's icon, in a rounded tile tinted with its tone.
fn icon_tile(ui: &mut egui::Ui, icon: &str, accent: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(36.0, 36.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(8), accent.gamma_multiply(0.14));
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon,
        FontId::proportional(19.0),
        accent,
    );
}

fn dialog_button(ui: &mut egui::Ui, button: &Button) -> egui::Response {
    let role = match button.kind {
        Kind::Main => Role::Primary,
        Kind::Danger => Role::Danger,
        Kind::Plain => Role::Secondary,
    };
    let palette = theme::palette(ui.ctx());
    ui.add(
        widgets::button(role, Some(button.icon), button.label, &palette).min_size(vec2(84.0, 30.0)),
    )
}

/// The unsaved-changes box's icon and tone.
pub fn unsaved_style() -> (&'static str, Tone) {
    (icons::FLOPPY_DISK, Tone::Warning)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn question_boxes_draw_and_escape_dismisses_them() {
        let snapshots = std::env::var_os("AXCENITY_SNAPSHOTS").map(std::path::PathBuf::from);
        let (icon, tone) = unsaved_style();
        // Shared with the drawing closure, which pushes each frame's outcome.
        let outcomes = std::cell::RefCell::new(Vec::new());
        let mut draw = |ctx: &egui::Context| {
            egui::CentralPanel::default().show(ctx, |_| {});
            outcomes.borrow_mut().push(ask(
                ctx,
                &Question {
                    id: "test",
                    icon,
                    tone,
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
