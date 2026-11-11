//! The start screen's background: a dark wash that warms toward the
//! accent at the bottom, slow aurora light drifting behind the page, and a
//! grid floor running to a horizon, like a level waiting to be built. It
//! doesn't react to the pointer (hover light belongs to the cards and
//! icons). It drifts only while the editor is drawing anyway and never asks
//! for frames of its own ([`drift_clock`]); Reduce motion keeps it still.

use eframe::egui::{self, Color32, Pos2, Rect, ecolor::Hsva, pos2, vec2};

use crate::theme;

/// How far down the page the horizon sits.
const HORIZON: f32 = 0.62;
/// Lines across the floor, and how long one takes to come forward.
const FLOOR_LINES: usize = 14;
const FLOOR_SECONDS: f32 = 6.0;
/// Lines running to the vanishing point, each side of the centre.
const FLOOR_RAYS: i32 = 18;

/// Paints the background over `rect`.
pub(super) fn paint(ui: &egui::Ui, rect: Rect) {
    let ctx = ui.ctx();
    let palette = theme::palette(ctx);
    let painter = ui.painter_at(rect);
    let time = drift_clock(ctx);
    let dark = palette.dark;

    // The wash: the app's background at the top, a breath of the accent at
    // the bottom.
    let bottom = theme::mix(
        palette.app_bg,
        palette.accent,
        if dark { 0.07 } else { 0.05 },
    );
    vertical_gradient(&painter, rect, palette.app_bg, bottom);

    // Aurora: the accent and its two neighbours on the colour wheel.
    let span = rect.width().max(rect.height());
    let orbit = |phase: f32, speed: f32, reach: f32| {
        vec2(
            (time * speed + phase).sin() * reach,
            (time * speed * 0.8 + phase * 1.7).cos() * reach * 0.6,
        )
    };
    let strength = if dark { 1.0 } else { 0.7 };
    let lights = [
        (0.78, 0.18, 0.55, 0.0, 0.16, 0.0),
        (0.20, 0.30, 0.45, 2.2, 0.12, 0.09),
        (0.55, 0.55, 0.40, 4.1, 0.10, -0.08),
    ];
    for (x, y, size, phase, alpha, hue) in lights {
        let center = pos2(
            rect.left() + rect.width() * x,
            rect.top() + rect.height() * y,
        ) + orbit(phase, 0.11, span * 0.05);
        crate::widgets::soft_pool(
            &painter,
            center,
            span * size,
            shift_hue(palette.accent, hue).gamma_multiply(alpha * strength),
        );
    }

    floor(&painter, rect, palette, time);

    // A soft dark edge, so the page's middle reads first.
    let edge = Color32::BLACK.gamma_multiply(if dark { 0.22 } else { 0.05 });
    vignette(&painter, rect, edge);
}

/// The background's own clock: it moves only while frames are being drawn
/// anyway (after input, the editor draws steadily for a second), and it
/// never asks for frames itself. A timer waking the window at its own pace
/// is what makes a variable-refresh display flicker (`docs/editor/shell.md`,
/// Steady frames). Paused, the drift holds where it is and carries on from
/// there, so it never jumps. Stopped with Reduce motion.
fn drift_clock(ctx: &egui::Context) -> f32 {
    if ctx.global_style().animation_time <= 0.0 {
        return 0.0;
    }
    let id = egui::Id::new("start backdrop clock");
    let step = ctx.input(|input| input.stable_dt).clamp(0.0, 1.0 / 30.0);
    ctx.data_mut(|data| {
        let clock = data.get_temp_mut_or(id, 0.0_f32);
        *clock += step;
        *clock
    })
}

/// The grid floor from the horizon to the bottom edge: lines across it
/// that come slowly forward, and lines running to the vanishing point,
/// both fading out toward the horizon.
fn floor(painter: &egui::Painter, rect: Rect, palette: theme::Palette, time: f32) {
    let horizon = rect.top() + rect.height() * HORIZON;
    let depth = rect.bottom() - horizon;
    if depth <= 1.0 {
        return;
    }
    let line = theme::mix(palette.text_strong, palette.accent, 0.55);
    let strongest = if palette.dark { 0.16 } else { 0.12 };
    let vanish = pos2(rect.center().x, horizon);
    // Across: evenly spaced in depth, so perspective packs them near the
    // horizon. `z` runs from 1 (the horizon) to 0 (the bottom edge).
    let scroll = (time / FLOOR_SECONDS).fract();
    for index in 0..FLOOR_LINES {
        let z = ((index as f32 + scroll) / FLOOR_LINES as f32).clamp(0.0, 1.0);
        let near = 1.0 - z;
        let y = horizon + depth * near * near;
        let fade = near.powf(1.5);
        painter.hline(
            rect.x_range(),
            y,
            egui::Stroke::new(1.0_f32, line.gamma_multiply(strongest * fade)),
        );
    }
    // To the vanishing point: a gradient mesh per ray, bright at the bottom.
    let mut mesh = egui::Mesh::default();
    let spread = rect.width() * 1.6;
    for ray in -FLOOR_RAYS..=FLOOR_RAYS {
        let foot = pos2(
            vanish.x + spread * ray as f32 / FLOOR_RAYS as f32,
            rect.bottom(),
        );
        add_fading_line(
            &mut mesh,
            foot,
            vanish,
            1.0,
            line.gamma_multiply(strongest * 0.8),
        );
    }
    painter.add(mesh);
    // The horizon itself: a thin glow.
    let glow = Rect::from_min_max(
        pos2(rect.left(), horizon - 30.0),
        pos2(rect.right(), horizon + 2.0),
    );
    vertical_gradient(
        painter,
        glow,
        Color32::TRANSPARENT,
        palette
            .accent
            .gamma_multiply(if palette.dark { 0.10 } else { 0.06 }),
    );
}

/// A line `width` thick from `from` (in `color`) to `to` (transparent).
fn add_fading_line(mesh: &mut egui::Mesh, from: Pos2, to: Pos2, width: f32, color: Color32) {
    let along = to - from;
    let length = along.length();
    if length <= 0.0 {
        return;
    }
    let side = vec2(-along.y, along.x) / length * (width / 2.0);
    let first = mesh.vertices.len() as u32;
    mesh.colored_vertex(from - side, color);
    mesh.colored_vertex(from + side, color);
    mesh.colored_vertex(to + side, Color32::TRANSPARENT);
    mesh.colored_vertex(to - side, Color32::TRANSPARENT);
    mesh.add_triangle(first, first + 1, first + 2);
    mesh.add_triangle(first, first + 2, first + 3);
}

/// `rect` filled from `top` at its top edge to `bottom` at its bottom.
fn vertical_gradient(painter: &egui::Painter, rect: Rect, top: Color32, bottom: Color32) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(mesh);
}

/// Darkens `rect`'s edges toward `edge`, clear in the middle.
fn vignette(painter: &egui::Painter, rect: Rect, edge: Color32) {
    let band = rect.width().min(rect.height()) * 0.18;
    let inner = rect.shrink(band);
    if !inner.is_positive() {
        return;
    }
    let mut mesh = egui::Mesh::default();
    let outer_corners = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
    ];
    let inner_corners = [
        inner.left_top(),
        inner.right_top(),
        inner.right_bottom(),
        inner.left_bottom(),
    ];
    for (outer, inner) in outer_corners.into_iter().zip(inner_corners) {
        mesh.colored_vertex(outer, edge);
        mesh.colored_vertex(inner, Color32::TRANSPARENT);
    }
    for side in 0..4u32 {
        let (a, b) = (side * 2, ((side + 1) % 4) * 2);
        mesh.add_triangle(a, b, b + 1);
        mesh.add_triangle(a, b + 1, a + 1);
    }
    painter.add(mesh);
}

/// `color` turned `by` of the way round the colour wheel.
fn shift_hue(color: Color32, by: f32) -> Color32 {
    let mut hsva = Hsva::from(color);
    hsva.h = (hsva.h + by).rem_euclid(1.0);
    Color32::from(hsva)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hue_shift_keeps_brightness_and_wraps() {
        let accent = Color32::from_rgb(60, 130, 246);
        assert_eq!(shift_hue(accent, 0.0), accent);
        assert_eq!(shift_hue(accent, 1.0), accent);
        let turned = shift_hue(accent, 0.1);
        assert_ne!(turned, accent);
        let value = |c: Color32| c.r().max(c.g()).max(c.b());
        assert!(value(turned).abs_diff(value(accent)) <= 2);
    }
}
