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
/// accent bar in the gutter just left of it.
pub fn changed_bar(ui: &egui::Ui, row: Rect) {
    let palette = theme::palette(ui.ctx());
    let bar = Rect::from_min_max(
        egui::pos2(row.left() - 5.0, row.top() + 3.0),
        egui::pos2(row.left() - 3.0, row.bottom() - 3.0),
    );
    // The gutter is outside the row's own clip: paint on its layer, clipped
    // only to the panel's sides.
    let mut painter = ui.ctx().layer_painter(ui.layer_id());
    painter.set_clip_rect(ui.clip_rect().expand2(vec2(6.0, 0.0)));
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

/// A faint grid of dots over `rect`, behind a page's content. One mesh
/// rather than a shape per dot, because a large window has thousands.
pub fn dot_grid(painter: &egui::Painter, rect: Rect, color: Color32) {
    const STEP: f32 = 24.0;
    const DOT: f32 = 1.5;
    let mut mesh = egui::Mesh::default();
    let mut y = rect.top() + STEP / 2.0;
    while y < rect.bottom() {
        let mut x = rect.left() + STEP / 2.0;
        while x < rect.right() {
            mesh.add_colored_rect(
                Rect::from_center_size(egui::pos2(x, y), vec2(DOT, DOT)),
                color,
            );
            x += STEP;
        }
        y += STEP;
    }
    painter.add(mesh);
}

/// A soft pool of light under the pointer, inside `rect`, `strength` from 0
/// to 1 (a hover's fade). Off with Reduce motion. The last position is kept
/// so the pool fades where it was when the pointer leaves.
pub fn spotlight(ui: &egui::Ui, rect: Rect, id: egui::Id, strength: f32) {
    const RADIUS: f32 = 170.0;
    const SEGMENTS: u32 = 40;
    if strength <= 0.0 || ui.ctx().style().animation_time <= 0.0 {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_logo_entrances_the_dots_and_the_spotlight_draw() {
        let pointer = egui::pos2(340.0, 200.0);
        let mut draw = |ctx: &egui::Context| {
            let brand = crate::start::Brand::load(ctx);
            egui::CentralPanel::default().show(ctx, |ui| {
                let palette = theme::palette(ui.ctx());
                let (top, ink) = (ui.max_rect().top() + 20.0, palette.text_strong);
                dot_grid(ui.painter(), ui.max_rect(), ink.gamma_multiply(0.04));
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
                let _ = ctx.run(Default::default(), &mut draw);
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
