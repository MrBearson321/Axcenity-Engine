//! The toolbar's look, a quieter take on Roblox Studio's ribbon: icon
//! buttons with their name underneath, in groups with a small caption
//! ("Tools", "Parts"). When the density is compact, buttons are icons only
//! (their name is in the tooltip) and groups have no caption.

use eframe::egui::{self, Color32, FontFamily, FontId};

use crate::theme::{self, Density, type_scale};

/// How tall a ribbon button is, in points, for each density.
pub fn button_height(density: Density) -> f32 {
    match density {
        Density::Comfortable => 44.0,
        Density::Compact => 28.0,
    }
}
const MIN_BUTTON_WIDTH: f32 = 46.0;
const ICON_SIZE: f32 = 18.0;
const CAPTION_HEIGHT: f32 = 13.0;

/// An icon with its name under it. Use it with `ui.add` or
/// `ui.add_enabled`, which greys it out and still shows the disabled hover text.
pub struct RibbonButton<'a> {
    icon: &'a str,
    label: &'a str,
    selected: bool,
    color: Option<Color32>,
    family: FontFamily,
    dropdown: bool,
}

impl<'a> RibbonButton<'a> {
    pub fn new(icon: &'a str, label: &'a str) -> Self {
        Self {
            icon,
            label,
            selected: false,
            color: None,
            family: FontFamily::Proportional,
            dropdown: false,
        }
    }

    /// Highlighted with the accent, like the tool that's picked.
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// The icon's colour, for buttons whose colour means something (Play's
    /// green, Stop's red). Others stay neutral.
    pub fn color(mut self, color: Color32) -> Self {
        self.color = Some(color);
        self
    }

    /// Draws the icon from this font family, such as [`crate::theme::filled`].
    pub fn family(mut self, family: FontFamily) -> Self {
        self.family = family;
        self
    }

    /// Marked with a small caret, for a button that opens a menu.
    pub fn dropdown(mut self) -> Self {
        self.dropdown = true;
        self
    }
}

// `Widget` is egui's trait for things `ui.add` can draw. Implementing it is
// what lets `ui.add_enabled(false, RibbonButton::new(..))` work.
impl egui::Widget for RibbonButton<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let density = theme::density(ui.ctx());
        let palette = theme::palette(ui.ctx());
        let compact = density == Density::Compact;
        let caption = if self.dropdown && !compact {
            format!("{} {}", self.label, theme::icons::CARET_DOWN)
        } else {
            self.label.to_string()
        };
        let label = ui.painter().layout_no_wrap(
            caption,
            FontId::proportional(type_scale::CAPTION),
            // Replaced by the real colour when it's painted.
            Color32::PLACEHOLDER,
        );
        let height = button_height(density);
        let width = if compact {
            height
        } else {
            (label.size().x + 14.0).max(MIN_BUTTON_WIDTH)
        };
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
        // For screen readers.
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::Button,
                ui.is_enabled(),
                self.selected,
                self.label,
            )
        });

        if ui.is_rect_visible(rect) {
            let enabled = ui.is_enabled();
            let hovered = response.hovered() && enabled;
            let painter = ui.painter();
            let hover = theme::ease(ui.ctx(), response.id.with("hover"), hovered);
            let fill = if self.selected {
                palette.accent_soft
            } else if response.is_pointer_button_down_on() {
                palette.control_active
            } else {
                palette.control_hover.gamma_multiply(hover)
            };
            painter.rect_filled(rect, 6, fill);
            if response.has_focus() {
                painter.rect_stroke(
                    rect,
                    6,
                    egui::Stroke::new(1.0_f32, palette.accent),
                    egui::StrokeKind::Inside,
                );
            }
            let text_color = if !enabled {
                palette.text_muted
            } else if self.selected {
                palette.accent_text
            } else if hovered {
                palette.text_strong
            } else {
                palette.text_secondary
            };
            let icon_color = match self.color {
                Some(color) if enabled => color,
                Some(color) => color.gamma_multiply(0.4),
                None if self.selected => palette.accent,
                None => text_color,
            };
            let icon_center = if compact {
                rect.center()
            } else {
                egui::pos2(rect.center().x, rect.top() + 16.0)
            };
            painter.text(
                icon_center,
                egui::Align2::CENTER_CENTER,
                self.icon,
                FontId::new(ICON_SIZE, self.family),
                icon_color,
            );
            if compact && self.dropdown {
                painter.text(
                    rect.right_bottom() - egui::vec2(3.0, 2.0),
                    egui::Align2::RIGHT_BOTTOM,
                    theme::icons::CARET_DOWN,
                    FontId::proportional(8.0),
                    text_color,
                );
            }
            if !compact {
                let at = egui::pos2(
                    rect.center().x - label.size().x / 2.0,
                    rect.bottom() - 5.0 - label.size().y,
                );
                painter.galley(at, label, text_color);
            }
        }
        if compact {
            // The name is only in the tooltip, so say it there first.
            response.on_hover_text(self.label)
        } else {
            response
        }
    }
}

/// A group of ribbon buttons (or other controls) with `caption` centred
/// under them, or no caption when the density is compact.
pub fn group(ui: &mut egui::Ui, caption: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    let density = theme::density(ui.ctx());
    let palette = theme::palette(ui.ctx());
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let row = ui
            .horizontal(|ui| {
                ui.set_min_height(button_height(density));
                ui.spacing_mut().item_spacing.x = 2.0;
                add_contents(ui);
            })
            .response;
        if density == Density::Compact {
            return;
        }
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(row.rect.width(), CAPTION_HEIGHT),
            egui::Sense::hover(),
        );
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            caption,
            FontId::proportional(type_scale::CAPTION - 0.5),
            palette.text_muted,
        );
    });
}
