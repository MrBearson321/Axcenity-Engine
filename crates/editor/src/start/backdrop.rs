//! The start screen's background, a landscape waiting to be built: a dark
//! wash warming toward the accent, aurora light and twinkling stars, a
//! glow with ripples on the horizon, mountain ridges sliding past at three
//! depths, wireframe solids turning as they drift across the sky, and a
//! grid floor running toward the viewer. It doesn't react to the pointer
//! (hover light belongs to the cards and icons).
//!
//! It never asks for frames itself: while the start screen shows and the
//! window has focus, `EditorApp::keep_frames_steady` keeps frames coming on
//! the steady grid (a timer of its own would make a variable-refresh
//! display flicker; `docs/editor/shell.md`, Steady frames). Its clock only
//! moves on frames that are drawn ([`drift_clock`]), so after a pause it
//! carries on from where it was. Reduce motion keeps it still.

use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2, ecolor::Hsva, pos2, vec2};

use crate::theme;
use crate::widgets::soft_pool;

/// How far down the page the horizon sits.
const HORIZON: f32 = 0.62;
/// Lines across the floor, and how long one takes to come forward.
const FLOOR_LINES: usize = 14;
const FLOOR_SECONDS: f32 = 3.2;
/// Lines running to the vanishing point, each side of the centre.
const FLOOR_RAYS: i32 = 18;
/// How many stars.
const STARS: usize = 90;

/// Paints the background over `rect`.
pub(super) fn paint(ui: &egui::Ui, rect: Rect) {
    let ctx = ui.ctx();
    let palette = theme::palette(ctx);
    let painter = ui.painter_at(rect);
    let time = drift_clock(ctx);
    let dark = palette.dark;
    let horizon = rect.top() + rect.height() * HORIZON;

    // The wash: the app's background at the top, a breath of the accent at
    // the bottom.
    let bottom = theme::mix(
        palette.app_bg,
        palette.accent,
        if dark { 0.08 } else { 0.05 },
    );
    vertical_gradient(&painter, rect, palette.app_bg, bottom);

    stars(&painter, rect, horizon, &palette, time);

    // Aurora: the accent and its two neighbours on the colour wheel,
    // wandering slowly round the sky.
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
        ) + orbit(phase, 0.21, span * 0.09);
        soft_pool(
            &painter,
            center,
            span * size,
            shift_hue(palette.accent, hue).gamma_multiply(alpha * strength),
        );
    }

    horizon_glow(&painter, rect, horizon, &palette, time);
    solids(&painter, rect, horizon, &palette, time);
    ridges(&painter, rect, horizon, &palette, time);
    floor(&painter, rect, horizon, &palette, time);

    // A soft dark edge, so the page's middle reads first.
    let edge = Color32::BLACK.gamma_multiply(if dark { 0.22 } else { 0.05 });
    vignette(&painter, rect, edge);
}

/// The background's own clock, in seconds: it moves only on frames that
/// are drawn (a long gap counts as one short frame), so a pause holds the
/// scene where it was. Stopped with Reduce motion.
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

/// A number from 0 to 1 that looks random but is the same for `seed`.
fn scatter(seed: u32) -> f32 {
    let mut x = seed.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x & 0xFFFF) as f32 / 65535.0
}

/// Tiny stars above the horizon, each twinkling at its own pace.
fn stars(painter: &egui::Painter, rect: Rect, horizon: f32, palette: &theme::Palette, time: f32) {
    let alpha = if palette.dark { 0.55 } else { 0.25 };
    let sky = (horizon - rect.top()).max(1.0);
    let mut mesh = egui::Mesh::default();
    for index in 0..STARS as u32 {
        let x = rect.left() + rect.width() * scatter(index * 3);
        let y = rect.top() + sky * scatter(index * 3 + 1).powf(1.4);
        let phase = scatter(index * 3 + 2) * std::f32::consts::TAU;
        let twinkle = 0.5 + 0.5 * (time * (0.6 + phase * 0.2) + phase).sin();
        // Fainter toward the horizon's glow.
        let height = 1.0 - (y - rect.top()) / sky;
        let size = 1.0 + scatter(index * 7) * 1.4;
        let color = if index % 5 == 0 {
            palette.accent
        } else {
            palette.text_strong
        };
        mesh.add_colored_rect(
            Rect::from_center_size(pos2(x, y), vec2(size, size)),
            color.gamma_multiply(alpha * twinkle * (0.3 + 0.7 * height)),
        );
    }
    painter.add(mesh);
}

/// A glow on the horizon behind the mountains, with rings rippling out
/// from it across the ground.
fn horizon_glow(
    painter: &egui::Painter,
    rect: Rect,
    horizon: f32,
    palette: &theme::Palette,
    time: f32,
) {
    const RINGS: usize = 3;
    const RING_SECONDS: f32 = 7.5;
    let center = pos2(rect.center().x, horizon);
    let size = rect.width().min(rect.height() * 1.6) * 0.28;
    let strength = if palette.dark { 1.0 } else { 0.6 };
    soft_pool(
        painter,
        center,
        size * 1.6,
        palette.accent.gamma_multiply(0.20 * strength),
    );
    soft_pool(
        painter,
        center,
        size * 0.55,
        shift_hue(palette.accent, -0.06).gamma_multiply(0.26 * strength),
    );
    // Rings grow from the glow and fade as they go, one every few seconds,
    // squashed as if lying on the ground.
    for ring in 0..RINGS {
        let age = (time / RING_SECONDS + ring as f32 / RINGS as f32).fract();
        let radius = size * (0.35 + 1.5 * age);
        let fade = (1.0 - age) * (age * 6.0).min(1.0);
        let points: Vec<Pos2> = (0..=72)
            .map(|step| {
                let angle = step as f32 / 72.0 * std::f32::consts::TAU;
                center + vec2(angle.cos() * radius, angle.sin() * radius * 0.22)
            })
            .collect();
        painter.add(egui::Shape::line(
            points,
            Stroke::new(
                1.2_f32,
                palette.accent.gamma_multiply(0.30 * fade * strength),
            ),
        ));
    }
}

/// Wireframe solids drifting across the sky, each turning on two axes and
/// bobbing, smaller and fainter the further off they are. They wrap round
/// from one side to the other.
fn solids(painter: &egui::Painter, rect: Rect, horizon: f32, palette: &theme::Palette, time: f32) {
    let line = theme::mix(palette.text_strong, palette.accent, 0.6);
    let strongest = if palette.dark { 0.42 } else { 0.30 };
    // (shape, where across, how high above the horizon (0 on it, 1 the
    // top), size, depth from 0 near to 1 far, drift in points a second)
    let placed = [
        (Solid::Octahedron, 0.12, 0.62, 46.0, 0.2, 9.0),
        (Solid::Cube, 0.86, 0.72, 38.0, 0.35, -7.0),
        (Solid::Tetrahedron, 0.64, 0.30, 26.0, 0.6, 12.0),
        (Solid::Icosahedron, 0.34, 0.86, 30.0, 0.5, -5.0),
        (Solid::Cube, 0.48, 0.18, 16.0, 0.85, 6.0),
        (Solid::Octahedron, 0.93, 0.24, 18.0, 0.8, -10.0),
    ];
    let sky = horizon - rect.top();
    let wrap = rect.width() + 160.0;
    for (index, (solid, across, up, size, depth, speed)) in placed.into_iter().enumerate() {
        let phase = index as f32 * 1.37;
        let x = rect.left() - 80.0 + (across * rect.width() + time * speed + 80.0).rem_euclid(wrap);
        let bob = (time * 0.6 + phase).sin() * 10.0 * (1.0 - depth * 0.5);
        let center = pos2(x, horizon - sky * up * 0.9 + bob);
        let turn = (
            time * (0.35 + 0.1 * index as f32) + phase,
            time * 0.23 + phase * 2.0,
        );
        let alpha = strongest * (1.0 - depth * 0.65);
        let stroke = Stroke::new(1.2 - depth * 0.5, line.gamma_multiply(alpha));
        let (points, edges) = solid.shape();
        let shown: Vec<Pos2> = points
            .iter()
            .map(|point| center + turned(*point, turn) * size)
            .collect();
        for (a, b) in edges {
            painter.line_segment([shown[*a], shown[*b]], stroke);
        }
        // A faint glow inside, so the nearer ones read as made of light.
        soft_pool(
            painter,
            center,
            size * 1.6,
            palette.accent.gamma_multiply(0.06 * (1.0 - depth)),
        );
    }
}

/// `point` turned by `(yaw, pitch)` and seen in mild perspective.
fn turned(point: [f32; 3], (yaw, pitch): (f32, f32)) -> Vec2 {
    let [x, y, z] = point;
    let (sin_yaw, cos_yaw) = yaw.sin_cos();
    let (x, z) = (x * cos_yaw + z * sin_yaw, -x * sin_yaw + z * cos_yaw);
    let (sin_pitch, cos_pitch) = pitch.sin_cos();
    let (y, z) = (y * cos_pitch - z * sin_pitch, y * sin_pitch + z * cos_pitch);
    let scale = 4.0 / (4.0 + z);
    vec2(x, y) * scale
}

/// The wireframe solids the sky has.
#[derive(Clone, Copy)]
enum Solid {
    Tetrahedron,
    Cube,
    Octahedron,
    Icosahedron,
}

impl Solid {
    /// Corners (about unit distance from the middle) and the edges
    /// between them.
    fn shape(self) -> (&'static [[f32; 3]], &'static [(usize, usize)]) {
        const S: f32 = 0.577;
        // The icosahedron's corners: (0, ±1, ±phi) and its turns, scaled
        // to unit length.
        const P: f32 = 0.526;
        const Q: f32 = 0.851;
        match self {
            Solid::Tetrahedron => (
                &[[S, S, S], [S, -S, -S], [-S, S, -S], [-S, -S, S]],
                &[(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)],
            ),
            Solid::Cube => (
                &[
                    [-S, -S, -S],
                    [S, -S, -S],
                    [S, S, -S],
                    [-S, S, -S],
                    [-S, -S, S],
                    [S, -S, S],
                    [S, S, S],
                    [-S, S, S],
                ],
                &[
                    (0, 1),
                    (1, 2),
                    (2, 3),
                    (3, 0),
                    (4, 5),
                    (5, 6),
                    (6, 7),
                    (7, 4),
                    (0, 4),
                    (1, 5),
                    (2, 6),
                    (3, 7),
                ],
            ),
            Solid::Octahedron => (
                &[
                    [1.0, 0.0, 0.0],
                    [-1.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0],
                    [0.0, -1.0, 0.0],
                    [0.0, 0.0, 1.0],
                    [0.0, 0.0, -1.0],
                ],
                &[
                    (0, 2),
                    (0, 3),
                    (0, 4),
                    (0, 5),
                    (1, 2),
                    (1, 3),
                    (1, 4),
                    (1, 5),
                    (2, 4),
                    (4, 3),
                    (3, 5),
                    (5, 2),
                ],
            ),
            Solid::Icosahedron => (
                &[
                    [-P, Q, 0.0],
                    [P, Q, 0.0],
                    [-P, -Q, 0.0],
                    [P, -Q, 0.0],
                    [0.0, -P, Q],
                    [0.0, P, Q],
                    [0.0, -P, -Q],
                    [0.0, P, -Q],
                    [Q, 0.0, -P],
                    [Q, 0.0, P],
                    [-Q, 0.0, -P],
                    [-Q, 0.0, P],
                ],
                &[
                    (0, 1),
                    (0, 5),
                    (0, 7),
                    (0, 10),
                    (0, 11),
                    (1, 5),
                    (1, 7),
                    (1, 8),
                    (1, 9),
                    (2, 3),
                    (2, 4),
                    (2, 6),
                    (2, 10),
                    (2, 11),
                    (3, 4),
                    (3, 6),
                    (3, 8),
                    (3, 9),
                    (4, 5),
                    (4, 9),
                    (4, 11),
                    (5, 9),
                    (5, 11),
                    (6, 7),
                    (6, 8),
                    (6, 10),
                    (7, 8),
                    (7, 10),
                    (8, 9),
                    (10, 11),
                ],
            ),
        }
    }
}

/// Mountain ridges on the horizon at three depths, sliding sideways at
/// different speeds (the far ones slowest, for parallax), each a soft
/// silhouette with a lit edge.
fn ridges(painter: &egui::Painter, rect: Rect, horizon: f32, palette: &theme::Palette, time: f32) {
    const STEP: f32 = 6.0;
    let dark = palette.dark;
    // (height as a share of the page, wavelength in points, speed in points
    // a second, how much of the accent, edge strength)
    let layers = [
        (0.15, 260.0, 4.0, 0.22, 0.20),
        (0.11, 190.0, 9.0, 0.14, 0.30),
        (0.07, 130.0, 16.0, 0.06, 0.42),
    ];
    for (layer, (height, wavelength, speed, tint, edge)) in layers.into_iter().enumerate() {
        let peak = rect.height() * height;
        let seed = layer as f32 * 2.9;
        let fill_top = theme::mix(palette.app_bg, palette.accent, tint).gamma_multiply(if dark {
            0.95
        } else {
            0.55
        });
        let fill_bottom = palette.app_bg.gamma_multiply(if dark { 0.95 } else { 0.4 });
        let line = palette
            .accent
            .gamma_multiply(edge * if dark { 1.0 } else { 0.7 });
        let mut mesh = egui::Mesh::default();
        let mut outline = Vec::new();
        let mut x = rect.left();
        while x <= rect.right() + STEP {
            let u = (x - rect.left() + time * speed) / wavelength;
            let top = horizon - peak * ridge(u, seed);
            let first = mesh.vertices.len() as u32;
            mesh.colored_vertex(pos2(x, top), fill_top);
            mesh.colored_vertex(pos2(x, horizon + 1.0), fill_bottom);
            if first >= 2 {
                mesh.add_triangle(first - 2, first, first - 1);
                mesh.add_triangle(first, first + 1, first - 1);
            }
            outline.push(pos2(x, top));
            x += STEP;
        }
        painter.add(mesh);
        painter.add(egui::Shape::line(outline, Stroke::new(1.0_f32, line)));
    }
}

/// A ridge's height at `u` (in wavelengths), from 0 to 1: sharp peaks from
/// folded sines, a few sizes laid over each other.
fn ridge(u: f32, seed: f32) -> f32 {
    let fold = |x: f32| 1.0 - x.sin().abs();
    let height = 0.55 * fold(0.9 * u + seed).powf(1.6)
        + 0.30 * fold(1.9 * u + seed * 1.7 + 1.0).powf(2.0)
        + 0.15 * (0.5 + 0.5 * (4.7 * u + seed * 0.6).sin());
    height.clamp(0.0, 1.0)
}

/// The grid floor from the horizon to the bottom edge: lines across it
/// that come forward, and lines running to the vanishing point, both fading
/// out toward the horizon.
fn floor(painter: &egui::Painter, rect: Rect, horizon: f32, palette: &theme::Palette, time: f32) {
    let depth = rect.bottom() - horizon;
    if depth <= 1.0 {
        return;
    }
    let line = theme::mix(palette.text_strong, palette.accent, 0.55);
    let strongest = if palette.dark { 0.18 } else { 0.12 };
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
            Stroke::new(1.0_f32, line.gamma_multiply(strongest * fade)),
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
    // Where the floor meets the sky: a thin glow.
    let glow = Rect::from_min_max(
        pos2(rect.left(), horizon - 24.0),
        pos2(rect.right(), horizon + 2.0),
    );
    vertical_gradient(
        painter,
        glow,
        Color32::TRANSPARENT,
        palette
            .accent
            .gamma_multiply(if palette.dark { 0.12 } else { 0.06 }),
    );
}

/// How opaque the glass is, behind a page's text and under the
/// navigation: enough that text on it keeps WCAG AA contrast whatever the
/// scene behind is doing (`text_on_glass_meets_wcag_aa`), little enough
/// that the scene still shows through as a haze.
pub(super) const GLASS: f32 = 0.9;

/// The glass's tint: a little further from the text than the app's
/// background (darker in the dark theme, lighter in the light one), so
/// what light the scene lets through only brings it back toward it.
fn glass_tint(palette: &theme::Palette) -> Color32 {
    if palette.dark {
        theme::mix(palette.app_bg, Color32::BLACK, 0.35)
    } else {
        theme::mix(palette.app_bg, Color32::WHITE, 0.6)
    }
}

/// The milky haze over the glass at its strongest (the top), drawn
/// fading out downward.
fn glass_haze(palette: &theme::Palette) -> Color32 {
    if palette.dark {
        palette.text_strong.gamma_multiply(0.035)
    } else {
        Color32::WHITE.gamma_multiply(0.3)
    }
}

/// Hazy glass over `rect` with rounded corners `radius`: the scene shows
/// through faintly, and the haze is brightest at the top.
pub(super) fn glass(painter: &egui::Painter, rect: Rect, radius: u8, palette: &theme::Palette) {
    painter.rect_filled(rect, radius, glass_tint(palette).gamma_multiply(GLASS));
    let haze = glass_haze(palette);
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), haze);
    mesh.colored_vertex(rect.right_top(), haze);
    mesh.colored_vertex(rect.right_bottom(), Color32::TRANSPARENT);
    mesh.colored_vertex(rect.left_bottom(), Color32::TRANSPARENT);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter
        .with_clip_rect(rect.shrink(f32::from(radius) / 2.0))
        .add(mesh);
}

/// Hazy glass behind a page's text, over `rect`, with a hairline edge.
pub(super) fn reading_glass(painter: &egui::Painter, rect: Rect, palette: &theme::Palette) {
    let radius = theme::radius::CARD;
    glass(painter, rect, radius, palette);
    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(1.0_f32, palette.border.gamma_multiply(0.8)),
        egui::StrokeKind::Inside,
    );
}

/// The glass over `behind`, at its haziest, as egui blends it (for the
/// contrast test).
#[cfg(test)]
fn glass_over(palette: &theme::Palette, behind: Color32) -> Color32 {
    let over = |top: Color32, under: Color32| {
        let rest = 1.0 - f32::from(top.a()) / 255.0;
        let channel = |a: u8, b: u8| (f32::from(a) + f32::from(b) * rest).round() as u8;
        Color32::from_rgb(
            channel(top.r(), under.r()),
            channel(top.g(), under.g()),
            channel(top.b(), under.b()),
        )
    };
    let glassed = over(glass_tint(palette).gamma_multiply(GLASS), behind);
    over(glass_haze(palette), glassed)
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

    #[test]
    fn text_on_glass_meets_wcag_aa() {
        for (dark, high) in [(true, false), (false, false), (true, true), (false, true)] {
            for accent in theme::Accent::ALL {
                let p = theme::Palette::new(dark, accent).with_contrast(high);
                // The scene at its most extreme behind the glass, well past what
                // it paints: the brightest light (stars, the horizon glow) and
                // the accent laid thick over the background, the vignette at
                // its darkest, and plain background.
                for behind in [
                    theme::mix(p.app_bg, p.text_strong, 0.6),
                    theme::mix(p.app_bg, p.accent, 0.8),
                    theme::mix(p.app_bg, Color32::BLACK, 0.4),
                    p.app_bg,
                ] {
                    {
                        let surface = glass_over(&p, behind);
                        for (name, text) in [
                            ("text", p.text),
                            ("strong", p.text_strong),
                            ("secondary", p.text_secondary),
                            ("weak", p.text_weak),
                        ] {
                            let ratio = theme::contrast(text, surface);
                            assert!(
                                ratio >= 4.5,
                                "{name} on glass over {behind:?}: {ratio:.2} \
                                 ({accent:?}, dark {dark}, increased contrast {high})"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn ridges_stay_between_the_horizon_and_their_peak() {
        for step in 0..2000 {
            let height = ridge(step as f32 * 0.037, 1.3);
            assert!((0.0..=1.0).contains(&height), "{height}");
        }
    }

    #[test]
    fn every_solid_is_about_unit_size_and_its_edges_join_its_corners() {
        for solid in [
            Solid::Tetrahedron,
            Solid::Cube,
            Solid::Octahedron,
            Solid::Icosahedron,
        ] {
            let (points, edges) = solid.shape();
            for [x, y, z] in points {
                let length = (x * x + y * y + z * z).sqrt();
                assert!((0.9..=1.1).contains(&length), "{length}");
            }
            for (a, b) in edges {
                assert!(*a < points.len() && *b < points.len() && a != b);
            }
        }
    }

    #[test]
    fn the_clock_moves_only_with_frames_and_a_pause_holds_it() {
        let ctx = theme::test_context();
        let read = |time: f64| {
            let mut clock = 0.0;
            let input = egui::RawInput {
                time: Some(time),
                ..Default::default()
            };
            let _ = crate::theme::run(&ctx, input, |ui| clock = drift_clock(ui.ctx()));
            clock
        };
        read(0.0);
        let before = read(1.0 / 60.0);
        let after = read(2.0 / 60.0);
        // A long pause moves it no more than one short frame would.
        let paused = read(100.0);
        assert!(after > before);
        assert!(paused - after <= 1.0 / 30.0 + 1e-6, "{after} then {paused}");
    }
}
