//! The editor's own controls, drawn from the theme's tokens (see
//! `theme.rs`): buttons by role, icon buttons, a segmented control, section
//! labels, badges, sidebar items and a search field. Screens build from these
//! instead of styling egui's widgets one by one, so they all look and move alike.

use eframe::egui::{self, Color32, FontId, Rect, RichText, Sense, Stroke, vec2};

use crate::theme::{self, Palette, icons, type_scale};

/// What a button is for, which decides how loud it looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// The one main action of a screen or dialog: filled with the accent.
    Primary,
    /// Other actions: a quiet fill and a thin outline.
    Secondary,
    /// Actions that shouldn't draw the eye until hovered: no fill.
    Ghost,
    /// Something that can't be undone: filled red.
    Danger,
}

/// A text button (with an optional icon in front) in the look of its role.
/// Use with `ui.add` or `ui.add_enabled`.
pub fn button(
    role: Role,
    icon: Option<&str>,
    label: &str,
    palette: &Palette,
) -> egui::Button<'static> {
    let text = match icon {
        Some(icon) => theme::with_icon(icon, label),
        None => label.to_owned(),
    };
    let text = RichText::new(text).family(theme::medium_family());
    match role {
        Role::Primary => egui::Button::new(text.color(palette.on_accent))
            .fill(palette.accent_solid)
            .stroke(Stroke::NONE),
        Role::Danger => egui::Button::new(text.color(Color32::WHITE))
            .fill(if palette.dark {
                theme::mix(palette.error, Color32::BLACK, 0.35)
            } else {
                palette.error
            })
            .stroke(Stroke::NONE),
        Role::Secondary => egui::Button::new(text),
        Role::Ghost => egui::Button::new(text).frame(false),
    }
    .corner_radius(6)
    .min_size(vec2(0.0, 28.0))
}

/// A square button with only an icon, which fades in a background when
/// hovered and keeps one while `selected`. `tooltip` names it for the mouse
/// and for screen readers.
pub fn icon_button(ui: &mut egui::Ui, icon: &str, tooltip: &str, selected: bool) -> egui::Response {
    let size = ui.spacing().interact_size.y;
    let (rect, response) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), selected, tooltip)
    });
    if ui.is_rect_visible(rect) {
        let palette = theme::palette(ui.ctx());
        paint_hover(ui, rect, &response, selected, 6.0);
        let color = if !ui.is_enabled() {
            palette.text_muted
        } else if selected {
            palette.accent_text
        } else if response.hovered() {
            palette.text_strong
        } else {
            palette.text_secondary
        };
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            icon,
            FontId::proportional(15.0),
            color,
        );
    }
    response.on_hover_text(tooltip)
}

/// Paints `rect`'s background for a row or item: the selection colour while
/// `selected`, otherwise a hover fill that fades in and out, and a focus
/// ring when it has keyboard focus.
pub fn paint_hover(
    ui: &egui::Ui,
    rect: Rect,
    response: &egui::Response,
    selected: bool,
    radius: f32,
) {
    let palette = theme::palette(ui.ctx());
    let hovered = response.hovered() && ui.is_enabled();
    let t = theme::ease(ui.ctx(), response.id.with("hover"), hovered || selected);
    if t > 0.0 {
        let fill = if selected {
            palette.accent_soft
        } else {
            palette.control_hover
        };
        ui.painter()
            .rect_filled(rect, radius, fill.gamma_multiply(t));
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            radius,
            Stroke::new(1.0_f32, palette.accent),
            egui::StrokeKind::Inside,
        );
    }
}

/// A row of mutually exclusive choices, like Dark | Light | System. Returns
/// true when the choice changed.
pub fn segmented<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    value: &mut T,
    options: &[(T, Option<&str>, &str)],
) -> bool {
    segmented_at(ui, value, options).0
}

/// [`segmented`], also returning where each option was drawn, in order.
pub fn segmented_at<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    value: &mut T,
    options: &[(T, Option<&str>, &str)],
) -> (bool, Vec<egui::Rect>) {
    let palette = theme::palette(ui.ctx());
    let mut changed = false;
    let mut rects = Vec::with_capacity(options.len());
    egui::Frame::new()
        .fill(palette.inset)
        .stroke(Stroke::new(1.0_f32, palette.border))
        .corner_radius(7)
        .inner_margin(egui::Margin::same(2))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.horizontal(|ui| {
                for &(option, icon, label) in options {
                    let selected = *value == option;
                    let text = match icon {
                        Some(icon) => theme::with_icon(icon, label),
                        None => label.to_owned(),
                    };
                    let color = if selected {
                        palette.text_strong
                    } else {
                        palette.text_secondary
                    };
                    let galley = ui.painter().layout_no_wrap(
                        text,
                        FontId::new(type_scale::BODY, theme::medium_family()),
                        color,
                    );
                    let height = ui.spacing().interact_size.y - 2.0;
                    let size = vec2(galley.size().x + 20.0, height);
                    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
                    rects.push(rect);
                    response.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::WidgetType::RadioButton,
                            ui.is_enabled(),
                            selected,
                            label,
                        )
                    });
                    let t = theme::ease(ui.ctx(), response.id.with("on"), selected);
                    if t > 0.0 {
                        ui.painter()
                            .rect_filled(rect, 5, palette.control_active.gamma_multiply(t));
                    } else if response.hovered() {
                        ui.painter().rect_filled(rect, 5, palette.control);
                    }
                    if response.has_focus() {
                        ui.painter().rect_stroke(
                            rect,
                            5,
                            Stroke::new(1.0_f32, palette.accent),
                            egui::StrokeKind::Inside,
                        );
                    }
                    ui.painter()
                        .galley(rect.center() - galley.size() / 2.0, galley, color);
                    if response.clicked() && !selected {
                        *value = option;
                        changed = true;
                    }
                }
            });
        });
    (changed, rects)
}

/// A quiet label that leads a group of controls, like "APPEARANCE".
pub fn section_label(ui: &mut egui::Ui, text: &str) {
    let palette = theme::palette(ui.ctx());
    ui.label(
        RichText::new(text.to_uppercase())
            .family(theme::medium_family())
            .size(type_scale::CAPTION)
            .extra_letter_spacing(0.6)
            .color(palette.text_weak),
    );
}

/// An item in a sidebar: an icon and a title, highlighted while it's the
/// page shown, with an optional count at the right.
pub fn nav_item(
    ui: &mut egui::Ui,
    icon: &str,
    title: &str,
    count: Option<usize>,
    selected: bool,
) -> egui::Response {
    let palette = theme::palette(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, title)
    });
    let hovered = response.hovered();
    let t = theme::ease(ui.ctx(), response.id.with("hover"), hovered || selected);
    if t > 0.0 {
        let fill = if selected {
            palette.control_active
        } else {
            palette.control_hover
        };
        ui.painter().rect_filled(rect, 6, fill.gamma_multiply(t));
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            6,
            Stroke::new(1.0_f32, palette.accent),
            egui::StrokeKind::Inside,
        );
    }
    let color = if selected || hovered {
        palette.text_strong
    } else {
        palette.text_secondary
    };
    let icon_color = if selected { palette.accent } else { color };
    ui.painter().text(
        rect.left_center() + vec2(18.0, 0.0),
        egui::Align2::CENTER_CENTER,
        icon,
        FontId::proportional(15.0),
        icon_color,
    );
    ui.painter().text(
        rect.left_center() + vec2(34.0, 0.0),
        egui::Align2::LEFT_CENTER,
        title,
        FontId::new(type_scale::BODY, theme::medium_family()),
        color,
    );
    if let Some(count) = count {
        ui.painter().text(
            rect.right_center() - vec2(10.0, 0.0),
            egui::Align2::RIGHT_CENTER,
            count.to_string(),
            FontId::proportional(type_scale::SMALL),
            palette.text_weak,
        );
    }
    response
}

/// The room [`search_field`] adds around its text, left and right together.
pub const SEARCH_FIELD_MARGINS: f32 = 48.0;

/// A text field with a magnifying glass in front and a clear button once
/// something is typed. `width` is the text's; the field is
/// [`SEARCH_FIELD_MARGINS`] wider.
pub fn search_field(
    ui: &mut egui::Ui,
    text: &mut String,
    hint: &str,
    width: f32,
) -> egui::Response {
    let palette = theme::palette(ui.ctx());
    let response = ui.add(
        egui::TextEdit::singleline(text)
            .hint_text(RichText::new(hint).color(palette.text_muted))
            .desired_width(width)
            .margin(egui::Margin {
                left: 26,
                right: 22,
                top: 4,
                bottom: 4,
            }),
    );
    ui.painter().text(
        response.rect.left_center() + vec2(13.0, 0.0),
        egui::Align2::CENTER_CENTER,
        icons::MAGNIFYING_GLASS,
        FontId::proportional(13.0),
        palette.text_weak,
    );
    if !text.is_empty() {
        let clear = Rect::from_center_size(
            response.rect.right_center() - vec2(12.0, 0.0),
            vec2(16.0, 16.0),
        );
        let cleared = ui
            .interact(clear, response.id.with("clear"), Sense::click())
            .on_hover_text("Clear");
        let color = if cleared.hovered() {
            palette.text_strong
        } else {
            palette.text_weak
        };
        ui.painter().text(
            clear.center(),
            egui::Align2::CENTER_CENTER,
            icons::X,
            FontId::proportional(12.0),
            color,
        );
        if cleared.clicked() {
            text.clear();
        }
    }
    response
}

/// `text` laid out on at most `rows` lines of `max_width`, ending in "…"
/// if it doesn't fit, for names and paths in fixed-size cards and rows.
pub fn elided(
    ui: &egui::Ui,
    text: impl Into<String>,
    font: FontId,
    color: Color32,
    max_width: f32,
    rows: usize,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple(text.into(), font, color, max_width);
    job.wrap = egui::text::TextWrapping {
        max_width,
        max_rows: rows,
        break_anywhere: rows == 1,
        overflow_character: Some('…'),
    };
    ui.painter().layout_job(job)
}

/// The open/close arrow of a collapsible section: a chevron that turns
/// down as the section opens (egui animates `openness` from 0 to 1). Use
/// with `CollapsingHeader::icon`.
pub fn chevron(ui: &mut egui::Ui, openness: f32, response: &egui::Response) {
    let palette = theme::palette(ui.ctx());
    let color = if response.hovered() {
        palette.text_strong
    } else {
        palette.text_weak
    };
    let center = response.rect.center();
    let angle = openness * std::f32::consts::FRAC_PI_2;
    let rotate = |x: f32, y: f32| {
        let (sin, cos) = angle.sin_cos();
        center + vec2(x * cos - y * sin, x * sin + y * cos)
    };
    let points = vec![rotate(-1.5, -3.5), rotate(2.0, 0.0), rotate(-1.5, 3.5)];
    ui.painter()
        .add(egui::Shape::line(points, Stroke::new(1.5_f32, color)));
}

/// A thin line across the available width, in the border colour.
pub fn divider(ui: &mut egui::Ui) {
    let palette = theme::palette(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        Stroke::new(1.0_f32, palette.border),
    );
}

/// The logo's A (`assets/branding/mark-white-small.png`, white on
/// transparent) in `color`, fitted into `rect`.
pub fn paint_mark(
    painter: &egui::Painter,
    rect: Rect,
    texture: &egui::TextureHandle,
    color: Color32,
) {
    let side = rect.width().min(rect.height());
    let dest = Rect::from_center_size(rect.center(), vec2(side, side));
    painter.image(
        texture.id(),
        dest,
        Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        color,
    );
}

/// The mark and the name "Axcenity", in the theme's colours.
pub fn wordmark(ui: &mut egui::Ui, mark: &egui::TextureHandle, size: f32) -> egui::Response {
    let palette = theme::palette(ui.ctx());
    let galley = ui.painter().layout_no_wrap(
        "Axcenity".to_owned(),
        FontId::new(size, theme::heading_family()),
        palette.text_strong,
    );
    let mark_side = size * 1.25;
    let gap = size * 0.5;
    let (rect, response) = ui.allocate_exact_size(
        vec2(
            mark_side + gap + galley.size().x,
            mark_side.max(galley.size().y),
        ),
        Sense::hover(),
    );
    paint_mark(
        ui.painter(),
        Rect::from_min_size(rect.left_top(), vec2(mark_side, mark_side)),
        mark,
        palette.text_strong,
    );
    ui.painter().galley(
        egui::pos2(
            rect.left() + mark_side + gap,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        palette.text_strong,
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_segmented_control_switches_on_click_and_reports_the_change() {
        let ctx = theme::test_context();
        let options = [(0, None, "One"), (1, None, "Two")];
        // Draws one frame; returns whether it changed and where it started.
        fn frame(
            ctx: &egui::Context,
            value: &mut i32,
            options: &[(i32, Option<&str>, &str)],
            events: Vec<egui::Event>,
        ) -> (bool, egui::Pos2) {
            let mut out = (false, egui::Pos2::ZERO);
            let input = egui::RawInput {
                events,
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let start = ui.cursor().min;
                    out = (segmented(ui, value, options), start);
                });
            });
            out
        }
        let mut value = 0;
        let (changed, start) = frame(&ctx, &mut value, &options, Vec::new());
        assert!(!changed);
        // The second option is right of the first: click well inside it.
        let at = start + vec2(70.0, 12.0);
        let click = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let pressed = vec![egui::Event::PointerMoved(at), click(true)];
        assert!(!frame(&ctx, &mut value, &options, pressed).0);
        assert!(frame(&ctx, &mut value, &options, vec![click(false)]).0);
        assert_eq!(value, 1);
    }
}
