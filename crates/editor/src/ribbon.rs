//! The toolbar's look, like Roblox Studio's ribbon: big icon buttons with
//! their name underneath, in groups with a caption ("Tools", "Insert").

use eframe::egui::{self, Color32, FontFamily, FontId};

/// How tall a ribbon button is, in points. Groups are this tall plus the caption.
pub const BUTTON_HEIGHT: f32 = 50.0;
const MIN_BUTTON_WIDTH: f32 = 50.0;
const ICON_SIZE: f32 = 22.0;
const LABEL_SIZE: f32 = 11.0;
const CAPTION_HEIGHT: f32 = 14.0;

/// A big icon with its name under it. Use it with `ui.add` or
/// `ui.add_enabled`, which greys it out and still shows the disabled hover text.
pub struct RibbonButton<'a> {
    icon: &'a str,
    label: &'a str,
    selected: bool,
    color: Option<Color32>,
    family: FontFamily,
}

impl<'a> RibbonButton<'a> {
    pub fn new(icon: &'a str, label: &'a str) -> Self {
        Self {
            icon,
            label,
            selected: false,
            color: None,
            family: FontFamily::Proportional,
        }
    }

    /// Highlighted, like the tool that's picked.
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// The icon's colour, instead of the text colour.
    pub fn color(mut self, color: Color32) -> Self {
        self.color = Some(color);
        self
    }

    /// Draws the icon from this font family, such as [`crate::theme::filled`].
    pub fn family(mut self, family: FontFamily) -> Self {
        self.family = family;
        self
    }
}

// `Widget` is egui's trait for things `ui.add` can draw. Implementing it is
// what lets `ui.add_enabled(false, RibbonButton::new(..))` work.
impl egui::Widget for RibbonButton<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let label = ui.painter().layout_no_wrap(
            self.label.to_string(),
            FontId::proportional(LABEL_SIZE),
            // Replaced by the real colour when it's painted.
            Color32::PLACEHOLDER,
        );
        let width = (label.size().x + 16.0).max(MIN_BUTTON_WIDTH);
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(width, BUTTON_HEIGHT), egui::Sense::click());
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
            let visuals = ui.style().interact_selectable(&response, self.selected);
            let painter = ui.painter();
            if self.selected || response.hovered() || response.has_focus() {
                painter.rect(
                    rect,
                    visuals.corner_radius,
                    visuals.weak_bg_fill,
                    visuals.bg_stroke,
                    egui::StrokeKind::Inside,
                );
            }
            let text_color = visuals.text_color();
            let scale = crate::theme::icon_scale(ui, &response);
            painter.text(
                egui::pos2(rect.center().x, rect.top() + 19.0),
                egui::Align2::CENTER_CENTER,
                self.icon,
                FontId::new(ICON_SIZE * scale, self.family),
                self.color.unwrap_or(text_color),
            );
            let at = egui::pos2(
                rect.center().x - label.size().x / 2.0,
                rect.bottom() - 6.0 - label.size().y,
            );
            painter.galley(at, label, text_color);
        }
        response
    }
}

/// A group of ribbon buttons (or other controls) with `caption` centred under them.
pub fn group(ui: &mut egui::Ui, caption: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let row = ui
            .horizontal(|ui| {
                ui.set_min_height(BUTTON_HEIGHT);
                ui.spacing_mut().item_spacing.x = 2.0;
                add_contents(ui);
            })
            .response;
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(row.rect.width(), CAPTION_HEIGHT),
            egui::Sense::hover(),
        );
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            caption,
            FontId::proportional(10.0),
            ui.visuals().weak_text_color(),
        );
    });
}
