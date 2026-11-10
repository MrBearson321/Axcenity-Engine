//! The editor's dialogs. Every one is built from the same pieces, so the
//! title, the buttons and the keys are in the same place in all of them:
//! `modal` (the box; Escape or a click outside cancels), `header` (an icon
//! in a tinted tile, a title and a message), the dialog's own body, and
//! `buttons` (a row at the bottom right, the main one filled; Enter picks
//! it). `ask` puts them together for the question boxes ("Save changes?",
//! "Delete script?"). The box fades in (see `theme::MOTION_SECONDS`).

use eframe::egui::{self, FontId, RichText, Stroke, vec2};

use crate::theme::{self, icons, space, type_scale};
use crate::widgets::{self, Role};

/// How a button looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Filled in the accent color: what Enter does.
    Main,
    /// Filled red: something that can't be undone. Enter never picks it.
    Danger,
    /// Just an outline.
    Plain,
}

pub struct Button<'a> {
    pub icon: &'static str,
    pub label: &'a str,
    pub kind: Kind,
    /// Greyed out, with `tooltip` saying why.
    pub enabled: bool,
    pub tooltip: Option<&'a str>,
}

impl<'a> Button<'a> {
    pub fn new(icon: &'static str, label: &'a str, kind: Kind) -> Self {
        Self {
            icon,
            label,
            kind,
            enabled: true,
            tooltip: None,
        }
    }

    /// The Cancel button: an outline with an ×.
    pub fn cancel() -> Self {
        Self::new(icons::X, "Cancel", Kind::Plain)
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn tooltip(mut self, tooltip: &'a str) -> Self {
        self.tooltip = Some(tooltip);
        self
    }
}

/// The colour of the box's icon, by what kind of question it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    /// Something might be lost, such as unsaved changes.
    Warning,
    /// Something will be destroyed.
    Danger,
    /// A change that undo takes back, asked about so it isn't a surprise,
    /// or a form.
    Question,
}

/// When Enter picks the main button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Enter {
    /// Unless a text field has the keyboard: there Enter finishes the
    /// field. For dialogs with several fields.
    Main,
    /// From the dialog's text field too, as it's the only thing to fill in.
    FromField,
}

/// What a question box is about, besides its buttons.
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

/// The width of a question box.
pub const WIDTH: f32 = 420.0;

/// Whether a text field had the keyboard last frame, kept by `modal` for
/// `buttons`: by the time the row is drawn, Enter has already taken the
/// focus from a one-line field.
fn typing_id() -> egui::Id {
    egui::Id::new("dialog typing")
}

/// The frame every dialog is drawn in.
pub fn frame(ctx: &egui::Context) -> egui::Frame {
    egui::Frame::popup(&ctx.style())
        .corner_radius(theme::radius::CARD)
        .inner_margin(egui::Margin::same(20))
}

/// Shows a dialog `width` wide with `body` in it, over everything, the
/// rest of the editor dimmed. Returns what `body` returned and whether
/// Escape or a click outside asked to close it.
pub fn modal<R>(
    ctx: &egui::Context,
    id: impl std::hash::Hash,
    width: f32,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> (R, bool) {
    let shown = egui::Modal::new(egui::Id::new(id))
        .frame(frame(ctx))
        .show(ctx, |ui| {
            ui.set_width(width);
            body(ui)
        });
    let typing = ctx.wants_keyboard_input();
    ctx.data_mut(|data| data.insert_temp(typing_id(), typing));
    let dismissed = shown.should_close();
    (shown.inner, dismissed)
}

/// The top of a dialog: its icon in a tile tinted by `tone`, the title,
/// and a line or two under it.
pub fn header(ui: &mut egui::Ui, icon: &str, tone: Tone, title: &str, message: Option<&str>) {
    let palette = theme::palette(ui.ctx());
    let accent = match tone {
        Tone::Warning => palette.warning,
        Tone::Danger => palette.error,
        Tone::Question => palette.accent,
    };
    ui.horizontal_top(|ui| {
        icon_tile(ui, icon, accent);
        ui.add_space(space::XS);
        ui.vertical(|ui| {
            ui.label(
                RichText::new(title)
                    .font(FontId::new(
                        type_scale::HEADING + 1.0,
                        theme::display_family(),
                    ))
                    .extra_letter_spacing(theme::TITLE_TRACKING)
                    .color(palette.text_strong),
            );
            if let Some(message) = message {
                ui.add_space(space::XXS);
                ui.add(
                    egui::Label::new(RichText::new(message).color(palette.text_secondary)).wrap(),
                );
            }
        });
    });
    ui.add_space(space::M);
}

/// The row of buttons at the bottom right of a dialog, in the order given
/// (the main one last, so it's at the far right). Returns the index of the
/// one clicked, or of the main one when Enter is pressed (see `Enter`).
pub fn buttons(ui: &mut egui::Ui, buttons: &[Button], enter: Enter) -> Option<usize> {
    button_row(ui, buttons, enter).picked
}

/// What `button_row` drew.
pub struct ButtonRow {
    pub picked: Option<usize>,
    /// Where each button is, in the order given.
    pub rects: Vec<egui::Rect>,
}

/// `buttons`, also saying where each button is.
pub fn button_row(ui: &mut egui::Ui, buttons: &[Button], enter: Enter) -> ButtonRow {
    let mut picked = None;
    let mut rects = vec![egui::Rect::NOTHING; buttons.len()];
    ui.add_space(space::L);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = space::S;
        // Right to left, so the main button ends up at the right.
        for (index, button) in buttons.iter().enumerate().rev() {
            let response = dialog_button(ui, button);
            rects[index] = response.rect;
            if response.clicked() {
                picked = Some(index);
            }
        }
    });
    let typing = ui.data(|data| data.get_temp(typing_id()).unwrap_or(false));
    let main = buttons
        .iter()
        .position(|button| button.kind == Kind::Main && button.enabled);
    let enter_counts = !typing || enter == Enter::FromField;
    if picked.is_none()
        && enter_counts
        && let Some(main) = main
        && ui.input(|input| input.key_pressed(egui::Key::Enter))
    {
        picked = Some(main);
    }
    ButtonRow { picked, rects }
}

/// Shows a question box. Returns what was picked, if anything, this frame.
pub fn ask(ctx: &egui::Context, question: &Question, buttons: &[Button]) -> Option<Outcome> {
    let (picked, dismissed) = modal(ctx, question.id, WIDTH, |ui| {
        let palette = theme::palette(ui.ctx());
        header(
            ui,
            question.icon,
            question.tone,
            question.title,
            Some(question.message),
        );
        if !question.items.is_empty() {
            egui::Frame::new()
                .fill(palette.inset)
                .stroke(Stroke::new(1.0_f32, palette.border))
                .corner_radius(theme::radius::CONTROL)
                .inner_margin(space::margin(space::M, space::S))
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
        self::buttons(ui, buttons, Enter::Main)
    });
    match picked {
        Some(index) => Some(Outcome::Picked(index)),
        None => dismissed.then_some(Outcome::Dismissed),
    }
}

/// The box's icon, in a rounded tile tinted with its tone.
fn icon_tile(ui: &mut egui::Ui, icon: &str, accent: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(36.0, 36.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, theme::radius::CARD, accent.gamma_multiply(0.14));
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon,
        FontId::proportional(theme::size::ICON_LARGE),
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
    let response = ui.add_enabled(
        button.enabled,
        widgets::button(role, Some(button.icon), button.label, &palette).min_size(vec2(84.0, 30.0)),
    );
    match button.tooltip {
        Some(tooltip) if button.enabled => response.on_hover_text(tooltip),
        Some(tooltip) => response.on_disabled_hover_text(tooltip),
        None => response,
    }
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
                    Button::new(icons::X, "Cancel", Kind::Plain),
                    Button::new(icons::TRASH, "Don't Save", Kind::Plain),
                    Button::new(icons::FLOPPY_DISK, "Save", Kind::Main),
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

    /// Enter in a dialog's text field picks the main button only when the
    /// dialog says the field is its only input.
    #[test]
    fn enter_in_a_text_field_picks_the_main_button_only_from_the_only_field() {
        let enter = egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        };
        for (rule, expected) in [(Enter::Main, None), (Enter::FromField, Some(1))] {
            let ctx = theme::test_context();
            let mut text = String::from("name");
            let mut picked = Vec::new();
            for (frame, events) in [vec![], vec![], vec![enter.clone()]]
                .into_iter()
                .enumerate()
            {
                let input = egui::RawInput {
                    events,
                    ..Default::default()
                };
                let _ = ctx.run(input, |ctx| {
                    let ((), _) = modal(ctx, "enter test", WIDTH, |ui| {
                        let field = ui.text_edit_singleline(&mut text);
                        if frame == 0 {
                            field.request_focus();
                        }
                        let buttons = [
                            Button::cancel(),
                            Button::new(icons::CHECK, "Rename", Kind::Main),
                        ];
                        picked.push(self::buttons(ui, &buttons, rule));
                    });
                });
            }
            assert_eq!(picked, [None, None, expected], "{rule:?}");
        }
    }

    /// With no text field, Enter picks the main button, and never a
    /// Danger one or a greyed-out one.
    #[test]
    fn enter_picks_only_a_main_button_that_can_be_pressed() {
        let enter = egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        };
        let cases: [(Vec<Button>, Option<usize>); 3] = [
            (
                vec![
                    Button::cancel(),
                    Button::new(icons::CHECK, "OK", Kind::Main),
                ],
                Some(1),
            ),
            (
                vec![
                    Button::cancel(),
                    Button::new(icons::TRASH, "Delete", Kind::Danger),
                ],
                None,
            ),
            (
                vec![
                    Button::cancel(),
                    Button::new(icons::CHECK, "OK", Kind::Main).enabled(false),
                ],
                None,
            ),
        ];
        for (buttons, expected) in cases {
            let ctx = theme::test_context();
            let mut picked = None;
            for events in [vec![], vec![enter.clone()]] {
                let input = egui::RawInput {
                    events,
                    ..Default::default()
                };
                let _ = ctx.run(input, |ctx| {
                    picked = modal(ctx, "enter test", WIDTH, |ui| {
                        self::buttons(ui, &buttons, Enter::Main)
                    })
                    .0;
                });
            }
            assert_eq!(picked, expected);
        }
    }

    /// Every dialog goes through `modal` and every tool window through
    /// `widgets::tool_window`, so they all look and close alike.
    #[test]
    fn dialogs_and_tool_windows_are_built_from_the_shared_pieces() {
        // Spelled in two pieces so this test doesn't find itself.
        let (modal_call, window_call) = (
            concat!("egui::Modal", "::new"),
            concat!("egui::Window", "::new"),
        );
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut stack = vec![src.clone()];
        let mut found = Vec::new();
        while let Some(folder) = stack.pop() {
            for entry in std::fs::read_dir(&folder).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let name = path
                    .strip_prefix(&src)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                let text = std::fs::read_to_string(&path).unwrap_or_default();
                // The pieces themselves, and the fallback for a panel
                // whose OS window can't open.
                let allowed: &[&str] = match name.as_str() {
                    "dialog.rs" => &[modal_call],
                    "widgets.rs" | "app/floating.rs" => &[window_call],
                    _ => &[],
                };
                for call in [modal_call, window_call] {
                    if text.contains(call) && !allowed.contains(&call) {
                        found.push(format!("{name}: {call}"));
                    }
                }
            }
        }
        assert!(found.is_empty(), "{found:#?}");
    }
}
