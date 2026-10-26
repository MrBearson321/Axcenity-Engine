//! The inspector's text toolbar, under Font: bold, italic, underline and
//! strikethrough, case, alignment, size, colour, outline, shadow, spacing,
//! wrapping and `TextScaled`. Each control writes one CSS property into the
//! element's own `#Id` rule (or sets `TextScaled`), so it's an undo step and
//! Play draws the same. The toolbar shows what the element's styles make of
//! it ([`axcenity_ui::text_style`]), whichever rule they come from.

use axcenity_core::{InstanceId, Value};
use axcenity_render::{TextAlign, TextShadow, UiText, VerticalAlign};
use eframe::egui;

use super::{DesignerAction, DesignerScene};
use crate::properties::Edit;
use crate::responsive;
use crate::theme::icons;
use crate::widgets;

/// The four toggles, also on Ctrl+B, Ctrl+I and Ctrl+U.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Toggle {
    Bold,
    Italic,
    Underline,
    Strikethrough,
}

impl Toggle {
    /// Whether `style` has it on.
    pub fn is_on(self, style: &UiText) -> bool {
        match self {
            Toggle::Bold => style.weight >= 600,
            Toggle::Italic => style.italic,
            Toggle::Underline => style.underline,
            Toggle::Strikethrough => style.line_through,
        }
    }

    /// The CSS property and value that flip it for `style`.
    pub fn flipped(self, style: &UiText) -> (&'static str, String) {
        let on = !self.is_on(style);
        match self {
            Toggle::Bold => ("font-weight", if on { "bold" } else { "normal" }.to_owned()),
            Toggle::Italic => (
                "font-style",
                if on { "italic" } else { "normal" }.to_owned(),
            ),
            Toggle::Underline => ("text-decoration", decoration(on, style.line_through)),
            Toggle::Strikethrough => ("text-decoration", decoration(style.underline, on)),
        }
    }

    /// The action that flips it on `id`, whose text looks like `style`.
    pub fn action(self, id: InstanceId, style: &UiText) -> DesignerAction {
        let (property, value) = self.flipped(style);
        DesignerAction::TextStyle {
            id,
            property,
            value,
            dragging: false,
        }
    }
}

/// `text-decoration` with these lines.
fn decoration(underline: bool, line_through: bool) -> String {
    match (underline, line_through) {
        (true, true) => "underline line-through",
        (true, false) => "underline",
        (false, true) => "line-through",
        (false, false) => "none",
    }
    .to_owned()
}

/// A colour as CSS: `#rrggbb`, or `rgba(…)` when it's see-through.
pub fn css_color([r, g, b, a]: [f32; 4]) -> String {
    let byte = |channel: f32| (channel.clamp(0.0, 1.0) * 255.0).round() as u8;
    if a >= 0.999 {
        format!("#{:02x}{:02x}{:02x}", byte(r), byte(g), byte(b))
    } else {
        format!(
            "rgba({}, {}, {}, {})",
            byte(r),
            byte(g),
            byte(b),
            number(a.clamp(0.0, 1.0))
        )
    }
}

/// A number as CSS writes it: up to two decimals, without trailing zeros.
pub fn number(value: f32) -> String {
    let text = format!("{value:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" { "0" } else { text }.to_owned()
}

/// `text-shadow` for these shadows, or `none`.
pub fn css_shadows(shadows: &[TextShadow]) -> String {
    if shadows.is_empty() {
        return "none".to_owned();
    }
    shadows
        .iter()
        .map(|shadow| {
            format!(
                "{}px {}px {}px {}",
                number(shadow.offset[0]),
                number(shadow.offset[1]),
                number(shadow.blur),
                css_color(shadow.color)
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The toolbar for `id`'s text. Nothing for an element without text.
pub(super) fn text_toolbar(
    ui: &mut egui::Ui,
    scene: &DesignerScene,
    id: InstanceId,
    actions: &mut Vec<DesignerAction>,
) {
    let world = scene.world;
    let Some(style) = axcenity_ui::text_style(world, id, scene.stylesheets) else {
        return;
    };
    let mut scaled_to = None;
    let mut set = |property: &'static str, value: String, dragging: bool| {
        actions.push(DesignerAction::TextStyle {
            id,
            property,
            value,
            dragging,
        });
    };
    ui.add_space(4.0);
    widgets::section_label(ui, "Text");
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (toggle, icon, tip) in [
            (Toggle::Bold, icons::TEXT_B, "Bold (Ctrl+B)"),
            (Toggle::Italic, icons::TEXT_ITALIC, "Italic (Ctrl+I)"),
            (
                Toggle::Underline,
                icons::TEXT_UNDERLINE,
                "Underline (Ctrl+U)",
            ),
            (
                Toggle::Strikethrough,
                icons::TEXT_STRIKETHROUGH,
                "Strikethrough",
            ),
        ] {
            if widgets::icon_button(ui, icon, tip, toggle.is_on(&style)).clicked() {
                let (property, value) = toggle.flipped(&style);
                set(property, value, false);
            }
        }
        ui.menu_button(icons::TEXT_AA, |ui| {
            for (value, label) in [
                ("none", "As typed"),
                ("uppercase", "UPPERCASE"),
                ("lowercase", "lowercase"),
                ("capitalize", "Capitalize Each Word"),
            ] {
                if ui.button(label).clicked() {
                    set("text-transform", value.to_owned(), false);
                    ui.close();
                }
            }
        })
        .response
        .on_hover_text("Case (CSS text-transform)");
    });
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (align, icon, value, tip) in [
            (
                TextAlign::Left,
                icons::TEXT_ALIGN_LEFT,
                "left",
                "Align left",
            ),
            (
                TextAlign::Center,
                icons::TEXT_ALIGN_CENTER,
                "center",
                "Centre",
            ),
            (
                TextAlign::Right,
                icons::TEXT_ALIGN_RIGHT,
                "right",
                "Align right",
            ),
        ] {
            if widgets::icon_button(ui, icon, tip, style.align == align).clicked() {
                set("text-align", value.to_owned(), false);
            }
        }
        ui.add_space(6.0);
        for (align, icon, value, tip) in [
            (
                VerticalAlign::Top,
                icons::ALIGN_TOP,
                "top",
                "Top of the box",
            ),
            (
                VerticalAlign::Middle,
                icons::ALIGN_CENTER_VERTICAL,
                "middle",
                "Middle of the box",
            ),
            (
                VerticalAlign::Bottom,
                icons::ALIGN_BOTTOM,
                "bottom",
                "Bottom of the box",
            ),
        ] {
            if widgets::icon_button(ui, icon, tip, style.vertical_align == align).clicked() {
                set("vertical-align", value.to_owned(), false);
            }
        }
    });
    responsive::form(ui, ("ui designer text", id), |ui| {
        ui.label("Size");
        ui.horizontal(|ui| {
            let mut size = style.font_size;
            let response = ui.add(
                egui::DragValue::new(&mut size)
                    .range(1.0..=400.0)
                    .speed(0.25)
                    .suffix(" px"),
            );
            if response.changed() {
                set(
                    "font-size",
                    format!("{}px", number(size)),
                    response.dragged(),
                );
            }
            let mut color = rgba_bytes(style.color);
            if ui
                .color_edit_button_srgba_unmultiplied(&mut color)
                .on_hover_text("Text colour")
                .changed()
            {
                set("color", css_color(rgba_floats(color)), false);
            }
        });
        ui.end_row();

        ui.label("Outline");
        ui.horizontal(|ui| {
            let stroke = style.stroke;
            let mut width = stroke.map_or(0.0, |stroke| stroke.width);
            let color = stroke.map_or([0.0, 0.0, 0.0, 1.0], |stroke| stroke.color);
            let mut bytes = rgba_bytes(color);
            let response = ui.add(
                egui::DragValue::new(&mut width)
                    .range(0.0..=40.0)
                    .speed(0.1)
                    .suffix(" px"),
            );
            let recolored = ui
                .color_edit_button_srgba_unmultiplied(&mut bytes)
                .on_hover_text("Outline colour")
                .changed();
            if response.changed() || recolored {
                set(
                    "-webkit-text-stroke",
                    format!("{}px {}", number(width), css_color(rgba_floats(bytes))),
                    response.dragged(),
                );
            }
        });
        ui.end_row();

        ui.label("Shadow");
        ui.horizontal_wrapped(|ui| {
            let mut shadows = style.shadows.clone();
            let mut first = shadows.first().copied().unwrap_or(TextShadow {
                offset: [0.0, 0.0],
                blur: 0.0,
                color: [0.0, 0.0, 0.0, 0.6],
            });
            let mut dragging = false;
            let mut changed = false;
            let [x, y] = &mut first.offset;
            for (value, prefix, tip) in [
                (x, "x ", "How far right"),
                (y, "y ", "How far down"),
                (&mut first.blur, "blur ", "How soft"),
            ] {
                let response = ui
                    .add(egui::DragValue::new(value).speed(0.1).prefix(prefix))
                    .on_hover_text(tip);
                changed |= response.changed();
                dragging |= response.dragged();
            }
            first.blur = first.blur.max(0.0);
            let mut bytes = rgba_bytes(first.color);
            if ui
                .color_edit_button_srgba_unmultiplied(&mut bytes)
                .on_hover_text("Shadow colour")
                .changed()
            {
                first.color = rgba_floats(bytes);
                changed = true;
            }
            if changed {
                let none = first.offset == [0.0, 0.0] && first.blur == 0.0;
                match (shadows.is_empty(), none) {
                    (true, true) => {}
                    (true, false) => shadows.push(first),
                    (false, true) if shadows.len() == 1 => shadows.clear(),
                    (false, _) => shadows[0] = first,
                }
                set("text-shadow", css_shadows(&shadows), dragging);
            }
        });
        ui.end_row();

        ui.label("Letter spacing");
        let mut spacing = style.letter_spacing;
        let response = ui.add(
            egui::DragValue::new(&mut spacing)
                .range(-20.0..=100.0)
                .speed(0.05)
                .suffix(" px"),
        );
        if response.changed() {
            set(
                "letter-spacing",
                format!("{}px", number(spacing)),
                response.dragged(),
            );
        }
        ui.end_row();

        ui.label("Line height");
        let mut times = style
            .line_height
            .map_or(1.2, |height| height / style.font_size.max(1.0));
        let response = ui
            .add(
                egui::DragValue::new(&mut times)
                    .range(0.5..=5.0)
                    .speed(0.01)
                    .suffix(" ×"),
            )
            .on_hover_text("Times the text size, from one line to the next");
        if response.changed() {
            set("line-height", number(times), response.dragged());
        }
        ui.end_row();
    });
    ui.horizontal_wrapped(|ui| {
        let mut wrap = style.wrap;
        if ui
            .checkbox(&mut wrap, "Wrap")
            .on_hover_text("Break long text onto more lines (off is CSS white-space: nowrap)")
            .changed()
        {
            set(
                "white-space",
                if wrap { "normal" } else { "nowrap" }.to_owned(),
                false,
            );
        }
        let mut scaled = style.scaled;
        if ui
            .checkbox(&mut scaled, "Text Scaled")
            .on_hover_text("Shrink or grow the text to the largest size that fits, up to Size")
            .changed()
        {
            scaled_to = Some(scaled);
        }
    });
    if let Some(scaled) = scaled_to {
        actions.push(DesignerAction::Edit(
            id,
            Edit::Property("TextScaled", Value::Bool(scaled)),
        ));
    }
}

fn rgba_bytes(color: [f32; 4]) -> [u8; 4] {
    color.map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn rgba_floats(color: [u8; 4]) -> [f32; 4] {
    color.map(|channel| f32::from(channel) / 255.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggles_flip_one_property_and_keep_the_other_line() {
        let plain = UiText::default();
        assert_eq!(
            Toggle::Bold.flipped(&plain),
            ("font-weight", "bold".to_owned())
        );
        let bold = UiText {
            weight: 700,
            ..UiText::default()
        };
        assert_eq!(
            Toggle::Bold.flipped(&bold),
            ("font-weight", "normal".to_owned())
        );
        let struck = UiText {
            line_through: true,
            ..UiText::default()
        };
        assert_eq!(
            Toggle::Underline.flipped(&struck),
            ("text-decoration", "underline line-through".to_owned())
        );
        assert_eq!(
            Toggle::Strikethrough.flipped(&struck),
            ("text-decoration", "none".to_owned())
        );
    }

    #[test]
    fn colours_numbers_and_shadows_are_written_as_css() {
        assert_eq!(css_color([1.0, 0.0, 0.5, 1.0]), "#ff0080");
        assert_eq!(css_color([0.0, 0.0, 0.0, 0.5]), "rgba(0, 0, 0, 0.5)");
        assert_eq!(number(1.5), "1.5");
        assert_eq!(number(2.0), "2");
        assert_eq!(number(-0.001), "0");
        assert_eq!(css_shadows(&[]), "none");
        assert_eq!(
            css_shadows(&[TextShadow {
                offset: [2.0, 3.0],
                blur: 4.0,
                color: [0.0, 0.0, 0.0, 1.0],
            }]),
            "2px 3px 4px #000000"
        );
    }
}
