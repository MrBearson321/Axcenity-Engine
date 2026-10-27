//! Small notes in the bottom-right corner that confirm something happened
//! ("Saved", "Imported `crate.glb`") and go away on their own. They never
//! take clicks or keys; the full message is always in Output too.
//!
//! Under them, work still going on ([`Busy`], such as textures loading)
//! shows with a spinner and a bar until it's done. At the bottom, an
//! [`Offer`] (such as Relaunch after the engine was rebuilt) stays until
//! its button or its × is clicked.

use eframe::egui::{self, FontId};

use crate::theme::{self, icons, type_scale};

/// How long a toast stays, and how long it takes to come and go, in seconds.
const SHOWN_SECONDS: f64 = 2.4;
const FADE_SECONDS: f64 = 0.18;
/// At most this many show at once; older ones go first.
const MAX_SHOWN: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Success,
    Error,
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

/// What was clicked on the [`Offer`] this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OfferClick {
    pub accepted: bool,
    pub dismissed: bool,
}

#[derive(Default)]
pub struct Toasts {
    toasts: Vec<Toast>,
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
            self.toasts.remove(0);
        }
    }

    pub fn success(&mut self, text: impl Into<String>) {
        self.push(Kind::Success, text);
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.push(Kind::Error, text);
    }

    #[cfg(test)]
    pub fn texts(&self) -> Vec<&str> {
        self.toasts
            .iter()
            .map(|toast| toast.text.as_str())
            .collect()
    }

    /// Draws the toasts, newest at the bottom, and forgets finished ones.
    /// Returns what was clicked on the offer.
    pub fn show(&mut self, ctx: &egui::Context) -> OfferClick {
        let mut click = OfferClick::default();
        if self.toasts.is_empty() && self.busy.is_none() && self.offer.is_none() {
            return click;
        }
        let now = ctx.input(|input| input.time);
        let reduce_motion = ctx.style().animation_time <= 0.0;
        self.toasts.retain(|toast| {
            toast
                .shown_at
                .is_none_or(|at| now - at < SHOWN_SECONDS + FADE_SECONDS)
        });
        let palette = theme::palette(ctx);
        let screen = ctx.screen_rect();
        let mut bottom = screen.bottom() - 16.0;
        if let Some(offer) = &self.offer {
            let area = egui::Area::new(egui::Id::new("toast offer"))
                .order(egui::Order::Foreground)
                .pivot(egui::Align2::RIGHT_BOTTOM)
                .fixed_pos(egui::pos2(screen.right() - 16.0, bottom))
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style())
                        .inner_margin(egui::Margin::symmetric(12, 8))
                        .corner_radius(8)
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
                        .inner_margin(egui::Margin::symmetric(12, 8))
                        .corner_radius(8)
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
        for (index, toast) in self.toasts.iter_mut().enumerate().rev() {
            let age = now - *toast.shown_at.get_or_insert(now);
            // In over FADE_SECONDS, out over the last FADE_SECONDS.
            let fade_in = (age / FADE_SECONDS).min(1.0) as f32;
            let fade_out =
                ((SHOWN_SECONDS + FADE_SECONDS - age) / FADE_SECONDS).clamp(0.0, 1.0) as f32;
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
                Kind::Error => (icons::WARNING_CIRCLE, palette.error),
            };
            let area = egui::Area::new(egui::Id::new(("toast", index)))
                .order(egui::Order::Foreground)
                .interactable(false)
                .fade_in(false)
                .pivot(egui::Align2::RIGHT_BOTTOM)
                .fixed_pos(egui::pos2(screen.right() - 16.0, bottom + rise))
                .show(ctx, |ui| {
                    ui.set_opacity(opacity);
                    egui::Frame::popup(ui.style())
                        .inner_margin(egui::Margin::symmetric(12, 8))
                        .corner_radius(8)
                        .show(ui, |ui| {
                            ui.set_max_width(360.0);
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 8.0;
                                ui.label(egui::RichText::new(icon).size(15.0).color(color));
                                ui.label(
                                    egui::RichText::new(&toast.text)
                                        .font(FontId::new(type_scale::BODY, theme::medium_family()))
                                        .color(palette.text_strong),
                                );
                            });
                        });
                });
            bottom -= area.response.rect.height() + 8.0;
        }
        // Keep drawing until the last one has gone.
        if !self.toasts.is_empty() || self.busy.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
        click
    }
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
            let _ = ctx.run(input, |ctx| {
                toasts.show(ctx);
            });
        }
        assert!(toasts.texts().is_empty());
    }
}
