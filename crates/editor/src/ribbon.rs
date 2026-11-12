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
            painter.rect_filled(rect, theme::radius::CONTROL, fill);
            if response.has_focus() {
                painter.rect_stroke(
                    rect,
                    theme::radius::CONTROL,
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

/// The tallest ribbon row drawn: in the frame before (which this frame's
/// groups grow to) and so far in this one.
#[derive(Clone, Copy, Default)]
struct RowHeights {
    frame: u64,
    last: f32,
    this: f32,
}

/// A group of ribbon buttons (or other controls) with `caption` centred
/// under them, or no caption when the density is compact. Every group's row
/// is as tall as the tallest one (Snapping's two rows of switches, say), so
/// all captions sit on one line at the bottom of the ribbon.
pub fn group(ui: &mut egui::Ui, caption: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    group_tinted(ui, caption, None, add_contents);
}

/// [`group`], with the caption in a category colour (cohesion pass CW2.2).
pub fn group_tinted(
    ui: &mut egui::Ui,
    caption: &str,
    hue: Option<theme::Hue>,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let density = theme::density(ui.ctx());
    let palette = theme::palette(ui.ctx());
    let id = egui::Id::new(("ribbon row heights", density as u8));
    let frame = theme::frame_number(ui.ctx());
    let mut heights = ui.data(|data| data.get_temp::<RowHeights>(id).unwrap_or_default());
    if heights.frame != frame {
        heights = RowHeights {
            frame,
            last: heights.this,
            this: 0.0,
        };
    }
    let row_height = button_height(density).max(heights.last);
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        // The row at its own height (a minimum would feed back: a grid of
        // fields grows with the room it's given), then the space down to
        // the tallest row, so every caption sits on the same line.
        let row = ui
            .horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                add_contents(ui);
            })
            .response;
        // Measured by where the cursor is, which is what the caption follows.
        let used = ui.cursor().top() - ui.min_rect().top();
        heights.this = heights.this.max(used);
        ui.data_mut(|data| data.insert_temp(id, heights));
        ui.add_space((row_height - used).max(0.0));
        if density == Density::Compact {
            return;
        }
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(row.rect.width(), CAPTION_HEIGHT),
            egui::Sense::hover(),
        );
        let color = hue.map_or(palette.text_weak, |hue| {
            theme::mix(hue.ink(&palette), palette.text_weak, 0.35)
        });
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            caption,
            FontId::proportional(type_scale::CAPTION - 0.5),
            color,
        );
        if let Some(hue) = hue {
            // A short bar of the group's colour under its caption.
            let bar = egui::Rect::from_center_size(
                egui::pos2(rect.center().x, rect.bottom() - 0.5),
                egui::vec2(14.0, 2.0),
            );
            ui.painter().rect_filled(
                bar,
                theme::radius::NONE,
                hue.ink(&palette).gamma_multiply(0.8),
            );
        }
    });
}
