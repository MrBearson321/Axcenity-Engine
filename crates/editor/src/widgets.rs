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
    .corner_radius(theme::radius::CONTROL)
    .min_size(vec2(0.0, theme::size::BUTTON_HEIGHT))
}

/// The name of a row in an inspector (Properties, the Material tab), with
/// an optional icon button after it, such as ↺. The name stays where every
/// other row's name is. When `changed` (the value differs from its default
/// or its base) a thin accent bar marks the row in the gutter at its left:
/// the one way every inspector shows a changed value. `name` adds the label.
pub fn row_name<R>(
    ui: &mut egui::Ui,
    changed: bool,
    name: impl FnOnce(&mut egui::Ui) -> R,
    after: impl FnOnce(&mut egui::Ui),
) -> R {
    let row = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let label = name(ui);
        after(ui);
        label
    });
    if changed {
        changed_bar(ui, row.response.rect);
    }
    row.inner
}

/// The changed-value mark beside a row whose content is `row`: a 2-point
/// accent bar in the gutter, 4 points left of it so it doesn't read as a
/// text cursor.
pub fn changed_bar(ui: &egui::Ui, row: Rect) {
    let palette = theme::palette(ui.ctx());
    let bar = Rect::from_min_max(
        egui::pos2(row.left() - 6.0, row.top() + 2.0),
        egui::pos2(row.left() - 4.0, row.bottom() - 2.0),
    );
    // The gutter is outside the row's own clip: paint on its layer, clipped
    // only to the panel's sides.
    let mut painter = ui.ctx().layer_painter(ui.layer_id());
    painter.set_clip_rect(ui.clip_rect().expand2(vec2(8.0, 0.0)));
    painter.rect_filled(bar, theme::radius::NONE, palette.accent);
}

/// The ↺ after a changed row's name: sets the value back. `tooltip` says to
/// what. Returns whether it was clicked.
pub fn reset_button(ui: &mut egui::Ui, tooltip: &str) -> bool {
    icon_button(ui, icons::ARROW_COUNTER_CLOCKWISE, tooltip, false).clicked()
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
        paint_hover(ui, rect, &response, selected, theme::radius::CONTROL);
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
            FontId::proportional(theme::size::ICON_MEDIUM),
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
    radius: u8,
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
            palette.focus_stroke(),
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
    segmented_tips(ui, value, options, &[])
}

/// [`segmented_at`], with `tips[k]` shown on hovering option `k` (an
/// option past the end of `tips` has none).
pub fn segmented_tips<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    value: &mut T,
    options: &[(T, Option<&str>, &str)],
    tips: &[&str],
) -> (bool, Vec<egui::Rect>) {
    let palette = theme::palette(ui.ctx());
    let mut changed = false;
    let mut rects = Vec::with_capacity(options.len());
    egui::Frame::new()
        .fill(palette.inset)
        .stroke(Stroke::new(1.0_f32, palette.border))
        .corner_radius(theme::radius::CARD)
        .inner_margin(egui::Margin::same(theme::space::XXS as i8))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.horizontal(|ui| {
                for (k, &(option, icon, label)) in options.iter().enumerate() {
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
                    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click());
                    if let Some(tip) = tips.get(k) {
                        response = response.on_hover_text(*tip);
                    }
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
                        ui.painter().rect_filled(
                            rect,
                            theme::radius::CONTROL,
                            palette.control_active.gamma_multiply(t),
                        );
                    } else if response.hovered() {
                        ui.painter()
                            .rect_filled(rect, theme::radius::CONTROL, palette.control);
                    }
                    if response.has_focus() {
                        ui.painter().rect_stroke(
                            rect,
                            theme::radius::CONTROL,
                            palette.focus_stroke(),
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

/// `text` as a micro-label: uppercase, tracked-out JetBrains Mono.
pub fn micro(text: &str, color: Color32) -> RichText {
    RichText::new(text.to_uppercase())
        .family(egui::FontFamily::Monospace)
        .size(type_scale::CAPTION)
        .extra_letter_spacing(theme::MICRO_TRACKING)
        .color(color)
}

/// A quiet label that leads a group of controls, like "APPEARANCE".
pub fn section_label(ui: &mut egui::Ui, text: &str) {
    let palette = theme::palette(ui.ctx());
    ui.label(micro(text, palette.text_weak));
}

/// A soft pool of light under the pointer, inside `rect`, `strength` from 0
/// to 1 (a hover's fade). Off with Reduce motion. The last position is kept
/// so the pool fades where it was when the pointer leaves.
pub fn spotlight(ui: &egui::Ui, rect: Rect, id: egui::Id, strength: f32) {
    const RADIUS: f32 = 170.0;
    const SEGMENTS: u32 = 40;
    if strength <= 0.0 || ui.ctx().global_style().animation_time <= 0.0 {
        return;
    }
    let id = id.with("spotlight");
    let ctx = ui.ctx();
    if ui.rect_contains_pointer(rect)
        && let Some(pointer) = ctx.pointer_latest_pos()
    {
        ctx.data_mut(|data| data.insert_temp(id, pointer));
    }
    let Some(center) = ctx.data(|data| data.get_temp::<egui::Pos2>(id)) else {
        return;
    };
    let palette = theme::palette(ctx);
    let alpha = if palette.dark { 0.09 } else { 0.06 };
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(center, palette.text_strong.gamma_multiply(alpha * strength));
    for segment in 0..=SEGMENTS {
        let angle = segment as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
        mesh.colored_vertex(
            center + RADIUS * egui::Vec2::angled(angle),
            Color32::TRANSPARENT,
        );
    }
    for segment in 0..SEGMENTS {
        mesh.add_triangle(0, segment + 1, segment + 2);
    }
    // Shrunk so the pool stays inside a card's rounded corners.
    ui.painter().with_clip_rect(rect.shrink(2.0)).add(mesh);
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
        ui.painter()
            .rect_filled(rect, theme::radius::CONTROL, fill.gamma_multiply(t));
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            theme::radius::CONTROL,
            palette.focus_stroke(),
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
        FontId::proportional(theme::size::ICON_MEDIUM),
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

/// The hover of a navigation icon at `at`: a soft accent glow behind it
/// (faint while `selected`), and the icon drawn toward the pointer and a
/// little larger, as if it floats over the bar. Returns where to draw the
/// icon and how far it has lifted (0 to 1). Reduce motion keeps the glow
/// and leaves the icon still.
fn icon_hover(
    ui: &egui::Ui,
    id: egui::Id,
    at: egui::Pos2,
    hovered: bool,
    selected: bool,
) -> (egui::Pos2, f32) {
    const PULL: f32 = 2.5;
    let palette = theme::palette(ui.ctx());
    let lift = theme::ease(ui.ctx(), id.with("icon lift"), hovered);
    let glow = (lift + if selected { 0.35 } else { 0.0 }).min(1.0);
    if glow > 0.0 {
        let alpha = if palette.dark { 0.26 } else { 0.18 };
        soft_pool(
            ui.painter(),
            at,
            20.0 + 4.0 * lift,
            palette.accent.gamma_multiply(alpha * glow),
        );
    }
    if lift <= 0.0 || ui.ctx().global_style().animation_time <= 0.0 {
        return (at, 0.0);
    }
    let toward =
        ui.ctx()
            .pointer_latest_pos()
            .filter(|_| hovered)
            .map_or(egui::Vec2::ZERO, |pointer| {
                let offset = (pointer - at) / 16.0;
                offset * (1.0 / offset.length().max(1.0))
            });
    (at + toward * PULL * lift, lift)
}

/// A sidebar page item that folds without a hard cut. Icons stay centred
/// until labels have room (`open` past ~0.55); then the icon slides left
/// and the title fades in, so nothing is clipped mid-slide.
pub fn nav_rail(
    ui: &mut egui::Ui,
    icon: &str,
    title: &str,
    count: Option<usize>,
    selected: bool,
    open: f32,
) -> egui::Response {
    let open = open.clamp(0.0, 1.0);
    // Labels only once the rail is wide enough (matches start/nav.rs).
    let label = smoothstep(0.55, 0.90, open);
    let palette = theme::palette(ui.ctx());
    let width = ui.available_width();
    let height = 34.0 - 4.0 * label;
    let (row, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, title)
    });
    let chip = Rect::from_center_size(row.center(), vec2(36.0_f32.min(width), height));
    let highlight = Rect {
        min: egui::pos2(
            chip.min.x + (row.min.x - chip.min.x) * label,
            chip.min.y + (row.min.y - chip.min.y) * label,
        ),
        max: egui::pos2(
            chip.max.x + (row.max.x - chip.max.x) * label,
            chip.max.y + (row.max.y - chip.max.y) * label,
        ),
    };
    let hovered = response.hovered();
    let t = theme::ease(ui.ctx(), response.id.with("hover"), hovered || selected);
    if t > 0.0 {
        let fill = if selected {
            palette.control_active
        } else {
            palette.control_hover
        };
        ui.painter()
            .rect_filled(highlight, theme::radius::CONTROL, fill.gamma_multiply(t));
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            highlight,
            theme::radius::CONTROL,
            palette.focus_stroke(),
            egui::StrokeKind::Inside,
        );
    }
    let color = if selected || hovered {
        palette.text_strong
    } else {
        palette.text_secondary
    };
    let icon_color = if selected { palette.accent } else { color };
    let icon_open = row.left_center() + vec2(18.0, 0.0);
    let icon_folded = row.center();
    let icon_pos = egui::pos2(
        icon_folded.x + (icon_open.x - icon_folded.x) * label,
        icon_folded.y + (icon_open.y - icon_folded.y) * label,
    );
    let icon_size = theme::size::ICON_MEDIUM + 2.0 * (1.0 - label);
    let (icon_pos, lift) = icon_hover(ui, response.id, icon_pos, hovered, selected);
    ui.painter().text(
        icon_pos,
        egui::Align2::CENTER_CENTER,
        icon,
        FontId::proportional(icon_size + 1.5 * lift),
        icon_color,
    );
    if label > 0.01 {
        ui.painter().text(
            row.left_center() + vec2(34.0, 0.0),
            egui::Align2::LEFT_CENTER,
            title,
            FontId::new(type_scale::BODY, theme::medium_family()),
            color.gamma_multiply(label),
        );
        if let Some(count) = count {
            ui.painter().text(
                row.right_center() - vec2(10.0, 0.0),
                egui::Align2::RIGHT_CENTER,
                count.to_string(),
                FontId::proportional(type_scale::SMALL),
                palette.text_weak.gamma_multiply(label),
            );
        }
    }
    response
}

/// Hermite smoothstep from 0 to 1 between `edge0` and `edge1`.
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if edge1 <= edge0 {
        return if x >= edge1 { 1.0 } else { 0.0 };
    }
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A soft radial pool as a triangle fan: glows, spotlights and the start
/// screen's background light.
pub fn soft_pool(painter: &egui::Painter, center: egui::Pos2, radius: f32, color: Color32) {
    const SEGMENTS: u32 = 48;
    if color.a() == 0 || radius <= 0.0 {
        return;
    }
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(center, color);
    for segment in 0..=SEGMENTS {
        let angle = segment as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
        mesh.colored_vertex(
            center + radius * egui::Vec2::angled(angle),
            Color32::TRANSPARENT,
        );
    }
    for segment in 0..SEGMENTS {
        mesh.add_triangle(0, segment + 1, segment + 2);
    }
    painter.add(mesh);
}

/// A tab in a top bar: an icon, the name if `shown`, and an optional count,
/// with an accent line along the bar's bottom edge (`bar_bottom`) while
/// it's the page shown. `title` names it for screen readers.
pub fn nav_tab(
    ui: &mut egui::Ui,
    icon: &str,
    title: &str,
    shown: Option<&str>,
    count: Option<usize>,
    selected: bool,
    bar_bottom: f32,
) -> egui::Response {
    let palette = theme::palette(ui.ctx());
    let font = FontId::new(type_scale::BODY, theme::medium_family());
    let name = shown.map(|text| {
        ui.painter()
            .layout_no_wrap(text.to_owned(), font, Color32::WHITE)
    });
    let count_text = count.filter(|_| shown.is_some()).map(|count| {
        ui.painter().layout_no_wrap(
            count.to_string(),
            FontId::proportional(type_scale::SMALL),
            Color32::WHITE,
        )
    });
    let icon_width = theme::size::ICON_MEDIUM + 2.0;
    let mut width = 12.0 + icon_width + 12.0;
    if let Some(name) = &name {
        width += 6.0 + name.size().x;
    }
    if let Some(count) = &count_text {
        width += 6.0 + count.size().x + 10.0;
    }
    let (rect, response) = ui.allocate_exact_size(vec2(width, 32.0), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, title)
    });
    let hovered = response.hovered();
    let t = theme::ease(ui.ctx(), response.id.with("hover"), hovered);
    if t > 0.0 {
        ui.painter().rect_filled(
            rect,
            theme::radius::CONTROL,
            palette.control_hover.gamma_multiply(t),
        );
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            theme::radius::CONTROL,
            palette.focus_stroke(),
            egui::StrokeKind::Inside,
        );
    }
    let on = theme::ease(ui.ctx(), response.id.with("on"), selected);
    if on > 0.0 {
        let line = Rect::from_min_max(
            egui::pos2(rect.left() + 6.0, bar_bottom - 2.0),
            egui::pos2(rect.right() - 6.0, bar_bottom),
        );
        ui.painter()
            .rect_filled(line, theme::radius::NONE, palette.accent.gamma_multiply(on));
    }
    let color = if selected || hovered {
        palette.text_strong
    } else {
        palette.text_secondary
    };
    let mut x = rect.left() + 12.0;
    let (icon_pos, lift) = icon_hover(
        ui,
        response.id,
        egui::pos2(x + icon_width / 2.0, rect.center().y),
        hovered,
        selected,
    );
    ui.painter().text(
        icon_pos,
        egui::Align2::CENTER_CENTER,
        icon,
        FontId::proportional(theme::size::ICON_MEDIUM + 1.5 * lift),
        if selected { palette.accent } else { color },
    );
    x += icon_width;
    if let Some(name) = name {
        x += 6.0;
        let at = egui::pos2(x, rect.center().y - name.size().y / 2.0);
        x += name.size().x;
        ui.painter()
            .galley_with_override_text_color(at, name, color);
    }
    if let Some(count) = count_text {
        x += 6.0;
        let pill = Rect::from_min_size(
            egui::pos2(x, rect.center().y - 8.0),
            vec2(count.size().x + 10.0, 16.0),
        );
        ui.painter()
            .rect_filled(pill, theme::radius::CARD, palette.inset);
        ui.painter().galley_with_override_text_color(
            pill.center() - count.size() / 2.0,
            count,
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
        FontId::proportional(theme::size::ICON_SMALL),
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

/// The square `rect`'s picture of the mark.
fn mark_square(rect: Rect) -> Rect {
    let side = rect.width().min(rect.height());
    Rect::from_center_size(rect.center(), vec2(side, side))
}

/// The mark cutting in diagonally from its top-left corner, `t` from 0 to 1.
pub fn paint_mark_cut(
    painter: &egui::Painter,
    rect: Rect,
    texture: &egui::TextureHandle,
    color: Color32,
    t: f32,
) {
    if t >= 1.0 {
        paint_mark(painter, rect, texture, color);
        return;
    }
    if t <= 0.0 {
        return;
    }
    // The edge is the line x + y = reach, in the picture's 0 to 1 square.
    let reach = t * 2.0;
    let corners: Vec<(f32, f32)> = if reach <= 1.0 {
        vec![(0.0, 0.0), (reach, 0.0), (0.0, reach)]
    } else {
        vec![
            (0.0, 0.0),
            (1.0, 0.0),
            (1.0, reach - 1.0),
            (reach - 1.0, 1.0),
            (0.0, 1.0),
        ]
    };
    let dest = mark_square(rect);
    let mut mesh = egui::Mesh::with_texture(texture.id());
    for (u, v) in &corners {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: dest.min + vec2(u * dest.width(), v * dest.height()),
            uv: egui::pos2(*u, *v),
            color,
        });
    }
    for i in 1..corners.len() as u32 - 1 {
        mesh.add_triangle(0, i, i + 1);
    }
    painter.add(mesh);
}

/// The logo loader: the mark in `base` with a band of `band` crossing it
/// from left to right, `position` from 0 (not arrived) to 1 (gone past).
pub fn paint_mark_sweep(
    painter: &egui::Painter,
    rect: Rect,
    texture: &egui::TextureHandle,
    (base, band): (Color32, Color32),
    position: f32,
) {
    const STRIPS: u32 = 28;
    // The band is 30% of a gradient three times the mark's width, so it is
    // off the mark at both ends. The band leans a little, top ahead.
    let color = |u: f32, lean: f32| {
        let along = (1.0 - position) * 2.0 / 3.0 + (u + lean) / 3.0;
        theme::mix(
            base,
            band,
            (1.0 - (along - 0.5).abs() / 0.15).clamp(0.0, 1.0),
        )
    };
    let dest = mark_square(rect);
    let mut mesh = egui::Mesh::with_texture(texture.id());
    for strip in 0..=STRIPS {
        let u = strip as f32 / STRIPS as f32;
        let x = dest.left() + u * dest.width();
        for (v, y, lean) in [(0.0, dest.top(), 0.09), (1.0, dest.bottom(), -0.09)] {
            mesh.vertices.push(egui::epaint::Vertex {
                pos: egui::pos2(x, y),
                uv: egui::pos2(u, v),
                color: color(u, lean),
            });
        }
    }
    for strip in 0..STRIPS {
        let i = strip * 2;
        mesh.add_triangle(i, i + 1, i + 2);
        mesh.add_triangle(i + 1, i + 2, i + 3);
    }
    painter.add(mesh);
}

/// The mark and the name "Axcenity", in the theme's colours. The mark cuts
/// in (`cut`, 0 to 1) and the name wipes in from the left (`wipe`, 0 to 1).
pub fn wordmark(
    ui: &mut egui::Ui,
    mark: &egui::TextureHandle,
    size: f32,
    cut: f32,
    wipe: f32,
) -> egui::Response {
    let palette = theme::palette(ui.ctx());
    let galley = ui.painter().layout_no_wrap(
        "Axcenity".to_owned(),
        FontId::new(size, theme::display_family()),
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
    paint_mark_cut(
        ui.painter(),
        Rect::from_min_size(rect.left_top(), vec2(mark_side, mark_side)),
        mark,
        palette.text_strong,
        cut,
    );
    let name_left = rect.left() + mark_side + gap;
    let mut wiped = rect;
    wiped.set_right(name_left + galley.size().x * wipe);
    ui.painter().with_clip_rect(wiped).galley(
        egui::pos2(name_left, rect.center().y - galley.size().y / 2.0),
        galley,
        palette.text_strong,
    );
    response
}

/// How a tool window is sized.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Sizing {
    /// Opens at this size (height 0: as tall as its content) and the user
    /// can drag its edges.
    Resizable(egui::Vec2),
    /// As big as its content, for short forms.
    Fit(f32),
    /// Always this size, for windows that lay out their own columns.
    Fixed(egui::Vec2),
}

/// The open tool windows, back to front, each with the frame it was last
/// drawn in: which one Escape closes. egui keeps its own order, but it
/// keeps closed windows in it too.
#[derive(Clone, Default)]
struct ToolWindowStack(Vec<(egui::LayerId, u64)>);

impl ToolWindowStack {
    /// Notes `layer` drawn in `pass`, on top if it's new or `raised` (egui
    /// put it in front, as a click does), or takes it out once it's closed.
    fn raise(&mut self, layer: egui::LayerId, raised: bool, pass: u64, open: bool) {
        let at = self.0.iter().position(|(entry, _)| *entry == layer);
        if !open {
            self.forget(layer);
            return;
        }
        match at {
            Some(at) if !raised => self.0[at].1 = pass,
            Some(at) => {
                self.0.remove(at);
                self.0.push((layer, pass));
            }
            None => self.0.push((layer, pass)),
        }
    }

    fn forget(&mut self, layer: egui::LayerId) {
        self.0.retain(|(entry, _)| *entry != layer);
    }

    /// The front window drawn this frame or the last one (a window closed
    /// by its own code, not through `show`, stops counting after a frame).
    fn top(&self, pass: u64) -> Option<egui::LayerId> {
        self.0
            .iter()
            .rev()
            .find(|(_, drawn)| *drawn + 1 >= pass)
            .map(|(layer, _)| *layer)
    }
}

/// One of the editor's tool windows (Settings, Input, Find in Scripts…),
/// so they all open, look and close the same way: an icon and a title in
/// the heading font, no collapse arrow, centred and kept on the screen the
/// first time, and Escape closes the window on top unless a text field, a
/// menu or a question box wants that Escape. Build with `tool_window`.
#[must_use]
pub struct ToolWindow {
    id: egui::Id,
    title: String,
    sizing: Sizing,
    min_width: Option<f32>,
}

/// A tool window titled `title` with `icon` in front. Its id comes from the
/// title (as egui's own windows do), so give `id` when the title changes.
pub fn tool_window(icon: &str, title: &str) -> ToolWindow {
    ToolWindow {
        id: egui::Id::new(title),
        title: theme::with_icon(icon, title),
        sizing: Sizing::Resizable(vec2(420.0, 0.0)),
        min_width: None,
    }
}

impl ToolWindow {
    pub fn id(mut self, id: impl std::hash::Hash + std::fmt::Debug) -> Self {
        self.id = egui::Id::new(id);
        self
    }

    /// Opens `size` big (a height of 0 fits the content); the user can
    /// resize it.
    pub fn size(mut self, size: egui::Vec2) -> Self {
        self.sizing = Sizing::Resizable(size);
        self
    }

    /// As big as its content, `width` wide, and not resizable.
    pub fn fit(mut self, width: f32) -> Self {
        self.sizing = Sizing::Fit(width);
        self
    }

    /// Always exactly `size`.
    pub fn fixed(mut self, size: egui::Vec2) -> Self {
        self.sizing = Sizing::Fixed(size);
        self
    }

    pub fn min_width(mut self, width: f32) -> Self {
        self.min_width = Some(width);
        self
    }

    /// Shows the window while `open`; the × and Escape clear it. Returns
    /// what `add` returned, if the window was drawn.
    pub fn show<R>(
        self,
        ctx: &egui::Context,
        open: &mut bool,
        add: impl FnOnce(&mut egui::Ui) -> R,
    ) -> Option<R> {
        if !*open {
            let layer = egui::LayerId::new(egui::Order::Middle, self.id);
            ctx.data_mut(|data| {
                data.get_temp_mut_or_default::<ToolWindowStack>(egui::Id::NULL)
                    .forget(layer)
            });
            return None;
        }
        // Asked before drawing: a menu that Escape closes this frame still
        // had it. A text field's focus is already gone (egui drops it on
        // Escape before the frame starts), so that's from last frame.
        let typing = self.id.with("typing");
        let escape_is_taken = ctx.data(|data| data.get_temp(typing).unwrap_or(false))
            || ctx.egui_wants_keyboard_input()
            || ctx.any_popup_open()
            || egui::Popup::is_any_open(ctx)
            || ctx.memory(|memory| memory.top_modal_layer().is_some());
        let screen = ctx.content_rect().shrink(theme::space::XL);
        let clamp = |size: egui::Vec2| size.min(screen.size()).max(vec2(160.0, 0.0));
        let mut window = egui::Window::new(self.title)
            .id(self.id)
            .open(open)
            .collapsible(false)
            .constrain(true)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(screen.center());
        window = match self.sizing {
            Sizing::Resizable(size) => {
                let size = clamp(size);
                let window = window.resizable(true).default_width(size.x);
                if size.y > 0.0 {
                    window.default_height(size.y)
                } else {
                    window
                }
            }
            Sizing::Fit(width) => window
                .resizable(false)
                .default_width(clamp(vec2(width, 0.0)).x),
            Sizing::Fixed(size) => window.resizable(false).fixed_size(clamp(size)),
        };
        if let Some(width) = self.min_width {
            window = window.min_width(width);
        }
        let layer = egui::LayerId::new(egui::Order::Middle, self.id);
        let shown = window.show(ctx, add);
        let pass = theme::frame_number(ctx);
        let raised = ctx.top_layer_id() == Some(layer);
        let on_top = ctx.data_mut(|data| {
            let stack = data.get_temp_mut_or_default::<ToolWindowStack>(egui::Id::NULL);
            stack.raise(layer, raised, pass, shown.is_some() && *open);
            stack.top(pass) == Some(layer)
        });
        let shown = shown?;
        let typing_now = ctx.egui_wants_keyboard_input();
        ctx.data_mut(|data| data.insert_temp(typing, typing_now));
        if on_top
            && !escape_is_taken
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            *open = false;
        }
        if !*open {
            ctx.data_mut(|data| {
                data.get_temp_mut_or_default::<ToolWindowStack>(egui::Id::NULL)
                    .forget(layer)
            });
        }
        shown.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs one frame of `draw` in a 400×600 window, with `events`.
    fn frame(ctx: &egui::Context, events: Vec<egui::Event>, draw: impl FnMut(&mut egui::Ui)) {
        let mut draw = draw;
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(400.0, 600.0))),
            events,
            ..Default::default()
        };
        let _ = crate::theme::run(ctx, input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| draw(ui));
        });
    }

    fn click(at: egui::Pos2) -> Vec<Vec<egui::Event>> {
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        vec![
            vec![egui::Event::PointerMoved(at)],
            vec![button(true)],
            vec![button(false)],
        ]
    }

    /// The shared section (design system V19): a remembered one keeps its
    /// state under its id, so the same section folded for one Instance is
    /// folded for the next; a forced one ignores clicks.
    #[test]
    fn a_remembered_section_folds_on_a_click_and_stays_folded() {
        let ctx = theme::test_context();
        let id = egui::Id::new("test section");
        let header = std::cell::Cell::new(Rect::NOTHING);
        let open = std::cell::Cell::new(true);
        let mut draw = |ui: &mut egui::Ui| {
            let top = ui.cursor().top();
            open.set(
                section(icons::GEAR, "Behavior")
                    .id(id)
                    .remember()
                    .show(ui, |ui| {
                        ui.label("inside");
                    }),
            );
            header.set(Rect::from_min_size(
                egui::pos2(ui.max_rect().left(), top + 6.0),
                vec2(200.0, 20.0),
            ));
        };
        frame(&ctx, Vec::new(), &mut draw);
        assert!(open.get(), "open at first");
        for events in click(header.get().center()) {
            frame(&ctx, events, &mut draw);
        }
        frame(&ctx, Vec::new(), &mut draw);
        assert!(!open.get(), "folded by the click");
        assert_eq!(ctx.data(|d| d.get_temp::<bool>(id)), Some(false));
    }

    #[test]
    fn an_empty_state_says_how_to_fill_it_and_its_button_works() {
        let ctx = theme::test_context();
        let clicked = std::cell::Cell::new(false);
        let button_at = std::cell::Cell::new(egui::Pos2::ZERO);
        let mut draw = |ui: &mut egui::Ui| {
            if empty_state(
                ui,
                icons::TAG,
                "No tags",
                "Add one in Properties.",
                Some((icons::PLUS, "Add Tag")),
            ) {
                clicked.set(true);
            }
            // The button is the last thing drawn, just above the cursor.
            button_at.set(egui::pos2(
                ui.max_rect().center().x,
                ui.cursor().top() - 12.0,
            ));
        };
        frame(&ctx, Vec::new(), &mut draw);
        for events in click(button_at.get()) {
            frame(&ctx, events, &mut draw);
        }
        assert!(clicked.get());
    }

    #[test]
    fn the_logo_entrances_and_the_spotlight_draw() {
        let pointer = egui::pos2(340.0, 200.0);
        let mut draw = |ui: &mut egui::Ui| {
            let ctx = &ui.ctx().clone();
            let brand = crate::start::Brand::load(ctx);
            egui::CentralPanel::default().show(ui, |ui| {
                let palette = theme::palette(ui.ctx());
                let (top, ink) = (ui.max_rect().top() + 20.0, palette.text_strong);
                let at = |index: usize, y: f32| {
                    Rect::from_min_size(
                        egui::pos2(20.0 + index as f32 * 100.0, y),
                        vec2(80.0, 80.0),
                    )
                };
                for (index, t) in [0.25, 0.5, 0.75, 1.0].into_iter().enumerate() {
                    paint_mark_cut(ui.painter(), at(index, top), brand.mark(), ink, t);
                }
                for (index, position) in [0.3, 0.5, 0.7, 0.0].into_iter().enumerate() {
                    let colors = (palette.text_muted, ink);
                    paint_mark_sweep(
                        ui.painter(),
                        at(index, top + 100.0),
                        brand.mark(),
                        colors,
                        position,
                    );
                }
                spotlight(
                    ui,
                    Rect::from_center_size(pointer, vec2(220.0, 120.0)),
                    egui::Id::new("card"),
                    1.0,
                );
                ui.horizontal(|ui| {
                    let _ = nav_rail(ui, icons::SQUARES_FOUR, "Projects", Some(3), true, 1.0);
                    let _ = nav_rail(ui, icons::GEAR_SIX, "Settings", None, false, 0.0);
                });
            });
        };
        let moved = vec![egui::Event::PointerMoved(pointer)];
        match std::env::var_os("AXCENITY_SNAPSHOTS").map(std::path::PathBuf::from) {
            Some(folder) => crate::snapshot::snapshot(
                &folder.join("logo-entrances.png"),
                vec2(460.0, 280.0),
                vec![moved, Vec::new()],
                &mut draw,
            ),
            None => {
                let ctx = theme::test_context();
                let _ = crate::theme::run(&ctx, Default::default(), |ui| draw(ui));
            }
        }
    }

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
            let _ = crate::theme::run(ctx, input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
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

/// A section that folds (design system V19): a divider, then a header with
/// an icon and the chevron, then its contents while open. Built with
/// [`section`]; the open state is egui's, under the section's id, unless
/// [`Section::remember`] keeps it across every Instance shown.
pub struct Section<'a> {
    icon: &'a str,
    title: &'a str,
    salt: Salt<'a>,
    default_open: bool,
    forced: Option<bool>,
    remember: bool,
    hint: Option<&'a str>,
}

/// What a [`Section`]'s state is kept under: kept as given, so a section
/// moved onto it keeps the id it had.
#[derive(Clone, Copy)]
enum Salt<'a> {
    Text(&'a str),
    Id(egui::Id),
}

/// A [`Section`] headed `icon` and `title`, open at first, kept under its
/// title (where it's drawn).
pub fn section<'a>(icon: &'a str, title: &'a str) -> Section<'a> {
    Section {
        icon,
        title,
        salt: Salt::Text(title),
        default_open: true,
        forced: None,
        remember: false,
        hint: None,
    }
}

impl<'a> Section<'a> {
    /// Keeps its open state under `salt` instead of its title, so the
    /// header can say more ("Advanced · 2 changed") without forgetting it.
    pub fn id_salt(mut self, salt: &'a str) -> Self {
        self.salt = Salt::Text(salt);
        self
    }

    /// Keeps its open state under `id`, as [`id_salt`](Self::id_salt).
    pub fn id(mut self, id: egui::Id) -> Self {
        self.salt = Salt::Id(id);
        self
    }

    /// Starts folded, or open, until clicked.
    pub fn default_open(mut self, open: bool) -> Self {
        self.default_open = open;
        self
    }

    /// Open (or folded) whatever was clicked, such as while filtering.
    pub fn forced(mut self, open: Option<bool>) -> Self {
        self.forced = open;
        self
    }

    /// Keeps the open state in egui's memory under its id alone, so it
    /// stays as it was left from one Instance to the next.
    pub fn remember(mut self) -> Self {
        self.remember = true;
        self
    }

    /// What's in it, on hover over the header.
    pub fn hint(mut self, hint: &'a str) -> Self {
        self.hint = Some(hint);
        self
    }

    /// Draws it, `contents` inside while open. Returns whether it's open.
    pub fn show(self, ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui)) -> bool {
        let palette = theme::palette(ui.ctx());
        ui.add_space(4.0);
        divider(ui);
        let header = egui::RichText::new(theme::with_icon(self.icon, self.title))
            .family(theme::medium_family())
            .color(palette.text_secondary);
        let memory = match self.salt {
            Salt::Text(text) => egui::Id::new(text),
            Salt::Id(id) => id,
        };
        let remembered = self.remember.then(|| {
            ui.data(|d| d.get_temp::<bool>(memory))
                .unwrap_or(self.default_open)
        });
        let header = egui::CollapsingHeader::new(header);
        let header = match self.salt {
            Salt::Text(text) => header.id_salt(text),
            Salt::Id(id) => header.id_salt(id),
        };
        let response = header
            .default_open(self.default_open)
            .open(self.forced.or(remembered))
            .icon(chevron)
            .show_unindented(ui, contents);
        let mut open = response.openness > 0.0;
        if let Some(was) = remembered {
            open = was;
            // Drawn as it was this frame; the click shows next frame.
            if response.header_response.clicked() && self.forced.is_none() {
                ui.data_mut(|d| d.insert_temp(memory, !was));
            }
        }
        if let Some(hint) = self.hint {
            response.header_response.on_hover_text(hint);
        }
        open
    }
}

/// What a panel shows when there's nothing in it yet (design system V19):
/// an icon, a short title and a sentence on how to get something there,
/// centred, with a button when `action` names one. Returns whether the
/// button was clicked.
pub fn empty_state(
    ui: &mut egui::Ui,
    icon: &str,
    title: &str,
    sentence: &str,
    action: Option<(&'static str, &str)>,
) -> bool {
    let palette = theme::palette(ui.ctx());
    let mut clicked = false;
    ui.add_space(theme::space::L);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(icon).size(22.0).color(palette.text_muted));
        ui.add_space(theme::space::XXS);
        ui.label(
            RichText::new(title)
                .family(theme::medium_family())
                .color(palette.text_secondary),
        );
        ui.add(egui::Label::new(RichText::new(sentence).color(palette.text_weak)).wrap());
        if let Some((icon, label)) = action {
            ui.add_space(theme::space::S);
            clicked = ui
                .add(button(Role::Secondary, Some(icon), label, &palette))
                .clicked();
        }
    });
    clicked
}

/// A choice in a row of filters, such as a category in Assets (design
/// system V19): quiet text, filled while it's picked.
pub fn chip(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let palette = theme::palette(ui.ctx());
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::FontId::new(theme::type_scale::SMALL + 0.5, theme::medium_family()),
        Color32::PLACEHOLDER,
    );
    let size = vec2(galley.size().x + 16.0, 24.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, selected, label)
    });
    paint_hover(ui, rect, &response, false, theme::radius::CARD);
    if selected {
        ui.painter()
            .rect_filled(rect, rect.height() / 2.0, palette.control_active);
    }
    let color = if selected || response.hovered() {
        palette.text_strong
    } else {
        palette.text_weak
    };
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, color);
    response
}
