//! Small notes in the bottom-right corner. Success notes ("Saved",
//! "Imported `crate.glb`") go away on their own and never take clicks.
//! Warnings and errors ("Could not save") stay until they're seen: closed
//! with their ×, or clicked, which shows the Console scrolled to the error.
//! The full message is always in the Console too.
//!
//! Under them, work still going on ([`Busy`], such as textures loading)
//! shows with a spinner and a bar until it's done. At the bottom, an
//! [`Offer`] (such as Relaunch after the engine was rebuilt) stays until
//! its button or its × is clicked.

use eframe::egui::{self, FontId};

use crate::theme::{self, icons, space, type_scale};

/// How long a toast stays, and how long it takes to come and go, in seconds.
const SHOWN_SECONDS: f64 = 2.4;
const FADE_SECONDS: f64 = 0.18;
/// At most this many show at once; older ones go first.
const MAX_SHOWN: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Success,
    Warning,
    Error,
}

impl Kind {
    /// Whether it stays until it's closed or clicked, so a failure is never
    /// missed while the user looks elsewhere.
    pub fn stays(self) -> bool {
        matches!(self, Kind::Warning | Kind::Error)
    }
}

struct Toast {
    kind: Kind,
    text: String,
    /// When it was first drawn (egui's clock), set then.
    shown_at: Option<f64>,
}

/// Work going on, shown under the toasts while it lasts.
#[derive(Clone, Debug, PartialEq)]
pub struct Busy {
    /// Such as "Loading textures 2 of 5: Ground108_4K-JPG_Color.jpg".
    pub text: String,
    /// How much is done, from 0 to 1.
    pub fraction: f32,
}

/// Something to do, offered until it's done or turned down.
#[derive(Clone, Debug, PartialEq)]
pub struct Offer {
    /// Such as "The engine was rebuilt."
    pub text: String,
    /// The button's label, such as "Relaunch".
    pub button: String,
}

/// What was clicked on the toasts this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Clicks {
    /// The [`Offer`]'s button.
    pub accepted: bool,
    /// The [`Offer`]'s ×.
    pub dismissed: bool,
    /// A warning or error, or the "more in the Console" note: show the
    /// Console at the newest error.
    pub show_console: bool,
}

#[derive(Default)]
pub struct Toasts {
    toasts: Vec<Toast>,
    /// Warnings and errors that didn't fit and haven't been seen.
    more: usize,
    /// Set each frame by the app; `None` when nothing is going on.
    pub busy: Option<Busy>,
    /// Set by the app; it stays until the app takes it away.
    pub offer: Option<Offer>,
}

impl Toasts {
    pub fn push(&mut self, kind: Kind, text: impl Into<String>) {
        self.toasts.push(Toast {
            kind,
            text: text.into(),
            shown_at: None,
        });
        if self.toasts.len() > MAX_SHOWN {
            // The oldest one that would go anyway; else the oldest error,
            // counted in the "more" note so it isn't lost.
            let old = self.toasts.iter().position(|toast| !toast.kind.stays());
            let gone = self.toasts.remove(old.unwrap_or(0));
            if gone.kind.stays() {
                self.more += 1;
            }
        }
    }

    pub fn success(&mut self, text: impl Into<String>) {
        self.push(Kind::Success, text);
    }

    pub fn warning(&mut self, text: impl Into<String>) {
        self.push(Kind::Warning, text);
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.push(Kind::Error, text);
    }

    /// How many warnings and errors didn't fit and wait in the Console.
    #[cfg(test)]
    pub fn more(&self) -> usize {
        self.more
    }

    #[cfg(test)]
    pub fn texts(&self) -> Vec<&str> {
        self.toasts
            .iter()
            .map(|toast| toast.text.as_str())
            .collect()
    }

    /// Draws the toasts, newest at the bottom, and forgets finished ones.
    /// Returns what was clicked.
    pub fn show(&mut self, ctx: &egui::Context) -> Clicks {
        let mut click = Clicks::default();
        if self.toasts.is_empty() && self.busy.is_none() && self.offer.is_none() && self.more == 0 {
            return click;
        }
        let now = ctx.input(|input| input.time);
        let reduce_motion = ctx.global_style().animation_time <= 0.0;
        self.toasts.retain(|toast| {
            toast.kind.stays()
                || toast
                    .shown_at
                    .is_none_or(|at| now - at < SHOWN_SECONDS + FADE_SECONDS)
        });
        let palette = theme::palette(ctx);
        let screen = ctx.content_rect();
        let mut bottom = screen.bottom() - 16.0;
        if let Some(offer) = &self.offer {
            let area = egui::Area::new(egui::Id::new("toast offer"))
                .order(egui::Order::Foreground)
                .pivot(egui::Align2::RIGHT_BOTTOM)
                .fixed_pos(egui::pos2(screen.right() - 16.0, bottom))
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style())
                        .inner_margin(space::margin(space::M, space::S))
                        .corner_radius(theme::radius::CARD)
                        .show(ui, |ui| {
                            ui.set_max_width(360.0);
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 8.0;
                                ui.label(
                                    egui::RichText::new(icons::CHECK_CIRCLE)
                                        .size(15.0)
                                        .color(palette.success),
                                );
                                ui.label(
                                    egui::RichText::new(&offer.text)
                                        .font(FontId::new(type_scale::BODY, theme::medium_family()))
                                        .color(palette.text_strong),
                                );
                                click.accepted = ui.button(&offer.button).clicked();
                                click.dismissed =
                                    ui.small_button(icons::X).on_hover_text("Not now").clicked();
                            });
                        });
                });
            bottom -= area.response.rect.height() + 8.0;
        }
        if let Some(busy) = &self.busy {
            let area = egui::Area::new(egui::Id::new("toast busy"))
                .order(egui::Order::Foreground)
                .interactable(false)
                .pivot(egui::Align2::RIGHT_BOTTOM)
                .fixed_pos(egui::pos2(screen.right() - 16.0, bottom))
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style())
                        .inner_margin(space::margin(space::M, space::S))
                        .corner_radius(theme::radius::CARD)
                        .show(ui, |ui| {
                            ui.set_width(300.0);
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 8.0;
                                ui.add(egui::Spinner::new().size(14.0));
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&busy.text)
                                            .size(type_scale::SMALL)
                                            .color(palette.text_strong),
                                    )
                                    .truncate(),
                                );
                            });
                            ui.add(
                                egui::ProgressBar::new(busy.fraction.clamp(0.0, 1.0))
                                    .desired_height(4.0)
                                    .fill(palette.accent),
                            );
                        });
                });
            bottom -= area.response.rect.height() + 8.0;
        }
        let mut closed = Vec::new();
        for (index, toast) in self.toasts.iter_mut().enumerate().rev() {
            let age = now - *toast.shown_at.get_or_insert(now);
            // In over FADE_SECONDS, out over the last FADE_SECONDS (if it
            // goes on its own).
            let fade_in = (age / FADE_SECONDS).min(1.0) as f32;
            let fade_out = if toast.kind.stays() {
                1.0
            } else {
                ((SHOWN_SECONDS + FADE_SECONDS - age) / FADE_SECONDS).clamp(0.0, 1.0) as f32
            };
            let opacity = if reduce_motion {
                1.0
            } else {
                fade_in.min(fade_out)
            };
            // Rises a few points as it fades in.
            let rise = if reduce_motion {
                0.0
            } else {
                (1.0 - fade_in) * 8.0
            };
            let (icon, color) = match toast.kind {
                Kind::Success => (icons::CHECK_CIRCLE, palette.success),
                Kind::Warning => (icons::WARNING, palette.warning),
                Kind::Error => (icons::WARNING_CIRCLE, palette.error),
            };
            let stays = toast.kind.stays();
            let area = egui::Area::new(egui::Id::new(("toast", index)))
                .order(egui::Order::Foreground)
                // Only a note that stays takes clicks, and only where it's
                // drawn: the viewport under it still gets the rest.
                .interactable(stays)
                .fade_in(false)
                .pivot(egui::Align2::RIGHT_BOTTOM)
                .fixed_pos(egui::pos2(screen.right() - 16.0, bottom + rise))
                .show(ctx, |ui| {
                    ui.set_opacity(opacity);
                    let (open, close) = note(ui, icon, color, &toast.text, stays);
                    if close {
                        closed.push(index);
                    } else if open {
                        closed.push(index);
                        click.show_console = true;
                    }
                });
            bottom -= area.response.rect.height() + 8.0;
        }
        if self.more > 0 {
            let text = match self.more {
                1 => "1 more error in the Console".to_owned(),
                n => format!("{n} more errors in the Console"),
            };
            egui::Area::new(egui::Id::new("toast more"))
                .order(egui::Order::Foreground)
                .pivot(egui::Align2::RIGHT_BOTTOM)
                .fixed_pos(egui::pos2(screen.right() - 16.0, bottom))
                .show(ctx, |ui| {
                    let (open, close) = note(ui, icons::STACK, palette.error, &text, true);
                    if open || close {
                        self.more = 0;
                    }
                    click.show_console |= open;
                });
        }
        for index in closed {
            self.toasts.remove(index);
        }
        // Keep drawing until the last one that goes on its own has gone.
        // At the display's rate, not a fixed 60: a change of rate partway
        // through flickers on variable refresh rate displays.
        if self.toasts.iter().any(|toast| !toast.kind.stays()) || self.busy.is_some() {
            ctx.request_repaint();
        }
        click
    }
}

/// One note: its icon, its text and, when it `stays`, a × and a click
/// anywhere on it. Returns whether it was clicked and whether its × was.
fn note(
    ui: &mut egui::Ui,
    icon: &str,
    color: egui::Color32,
    text: &str,
    stays: bool,
) -> (bool, bool) {
    let palette = theme::palette(ui.ctx());
    let mut close = false;
    // The note's own click is sensed before its ×, so the × keeps its own.
    let sense = if stays {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let note = ui.scope_builder(egui::UiBuilder::new().sense(sense), |ui| {
        egui::Frame::popup(ui.style())
            .inner_margin(space::margin(space::M, space::S))
            .corner_radius(theme::radius::CARD)
            .show(ui, |ui| {
                ui.set_max_width(360.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.label(egui::RichText::new(icon).size(15.0).color(color));
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(text)
                                .font(FontId::new(type_scale::BODY, theme::medium_family()))
                                .color(palette.text_strong),
                        )
                        .selectable(false),
                    );
                    if stays {
                        close = ui.small_button(icons::X).on_hover_text("Close").clicked();
                    }
                });
            });
    });
    if !stays {
        return (false, false);
    }
    let open = note
        .response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Show in the Console")
        .clicked();
    (open && !close, close)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toasts_go_away_on_their_own_and_only_the_newest_few_show() {
        let ctx = theme::test_context();
        let mut toasts = Toasts::default();
        for n in 0..5 {
            toasts.success(format!("Saved {n}"));
        }
        assert_eq!(toasts.texts(), ["Saved 2", "Saved 3", "Saved 4"]);
        for time in [0.0, 1.0, SHOWN_SECONDS + FADE_SECONDS + 0.1] {
            let input = egui::RawInput {
                time: Some(time),
                ..Default::default()
            };
            let _ = crate::theme::run(&ctx, input, |ui| {
                let ctx = &ui.ctx().clone();
                toasts.show(ctx);
            });
        }
        assert!(toasts.texts().is_empty());
    }
}

#[cfg(test)]
mod staying_tests {
    use super::*;

    /// Draws a frame at `time` with `events`; returns what was clicked.
    fn frame(
        ctx: &egui::Context,
        toasts: &mut Toasts,
        time: f64,
        events: Vec<egui::Event>,
    ) -> Clicks {
        let input = egui::RawInput {
            time: Some(time),
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 800.0),
            )),
            events,
            ..Default::default()
        };
        let mut clicks = Clicks::default();
        let _ = crate::theme::run(ctx, input, |ui| {
            let ctx = &ui.ctx().clone();
            clicks = toasts.show(ctx)
        });
        clicks
    }

    fn click(ctx: &egui::Context, toasts: &mut Toasts, time: f64, at: egui::Pos2) -> Clicks {
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        frame(ctx, toasts, time, vec![egui::Event::PointerMoved(at)]);
        frame(ctx, toasts, time + 0.01, vec![press(true)]);
        frame(ctx, toasts, time + 0.02, vec![press(false)])
    }

    fn rect(ctx: &egui::Context, index: usize) -> egui::Rect {
        ctx.memory(|memory| memory.area_rect(egui::Id::new(("toast", index))))
            .expect("the toast is drawn")
    }

    #[test]
    fn an_error_stays_until_closed_and_a_click_shows_the_console() {
        let ctx = theme::test_context();
        let mut toasts = Toasts::default();
        toasts.success("Saved");
        toasts.error("Could not save. Output says why.");
        for time in [0.0, 1.0, 5.0, 10.0, 12.0] {
            frame(&ctx, &mut toasts, time, Vec::new());
        }
        assert_eq!(toasts.texts(), ["Could not save. Output says why."]);

        // Its × closes it without showing the Console.
        let note = rect(&ctx, 0);
        let close = egui::pos2(note.right() - 20.0, note.center().y);
        let clicks = click(&ctx, &mut toasts, 13.0, close);
        assert!(!clicks.show_console);
        assert!(toasts.texts().is_empty());

        // Clicking a warning anywhere else shows the Console, and it's seen.
        toasts.warning("Save the Event Graph first.");
        frame(&ctx, &mut toasts, 20.0, Vec::new());
        let note = rect(&ctx, 0);
        let clicks = click(
            &ctx,
            &mut toasts,
            21.0,
            note.left_center() + egui::vec2(30.0, 0.0),
        );
        assert!(clicks.show_console);
        assert!(toasts.texts().is_empty());
    }

    #[test]
    fn errors_that_dont_fit_are_counted_and_success_notes_make_room_first() {
        let mut toasts = Toasts::default();
        toasts.error("one");
        toasts.success("Saved");
        toasts.error("two");
        toasts.error("three");
        // The success note went, not an error.
        assert_eq!(toasts.texts(), ["one", "two", "three"]);
        assert_eq!(toasts.more(), 0);
        toasts.error("four");
        toasts.error("five");
        assert_eq!(toasts.texts(), ["three", "four", "five"]);
        assert_eq!(toasts.more(), 2);

        // Clicking the "2 more errors" note shows the Console.
        let ctx = theme::test_context();
        frame(&ctx, &mut toasts, 0.0, Vec::new());
        let more = ctx
            .memory(|memory| memory.area_rect(egui::Id::new("toast more")))
            .unwrap();
        let clicks = click(
            &ctx,
            &mut toasts,
            1.0,
            more.left_center() + egui::vec2(30.0, 0.0),
        );
        assert!(clicks.show_console);
        assert_eq!(toasts.more(), 0);
    }
}
