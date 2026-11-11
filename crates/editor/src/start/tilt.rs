//! The start screen cards' hover: the card leans toward the pointer as if
//! pressed there (a perspective tilt), lifts on a soft shadow, and a glare
//! follows the pointer across it. The glow lives on the card, not the page
//! behind it. Reduce motion keeps only the glare's fade, flat.
//!
//! egui draws only flat shapes, so the tilt is done after the card is
//! painted: every shape the card added is tessellated into triangles and
//! each vertex is projected through a small 3D rotation. Text keeps its
//! glyphs flat and only moves with the surface, so the font atlas is never
//! read mid-frame.

use eframe::egui::{
    self, Color32, Pos2, Rect, Vec2, epaint,
    epaint::{ClippedShape, Shape},
    pos2, vec2,
};

use crate::theme;

/// How far a card leans at most, in radians (about 8°).
const MAX_LEAN: f32 = 0.14;
/// How far the eye is from the screen, in points: smaller is stronger.
const EYE: f32 = 900.0;
/// How much a hovered card grows as it lifts.
const LIFT: f32 = 0.018;
/// How quickly the lean follows the pointer (per second).
const FOLLOW: f32 = 14.0;

/// A card's hover state this frame.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Lean {
    /// Where the pointer is over the card, from -1 to 1 each way, smoothed.
    pub pointer: Vec2,
    /// The hover's fade, 0 to 1.
    pub hover: f32,
    /// False with Reduce motion: no tilt, lift or drift.
    pub motion: bool,
}

impl Lean {
    /// How far an inner layer at `depth` points above the card moves, for
    /// parallax: toward the pointer, by the hover's amount.
    pub fn drift(&self, depth: f32) -> Vec2 {
        if self.motion {
            self.pointer * depth * self.hover
        } else {
            Vec2::ZERO
        }
    }
}

/// Reads the pointer over `rect` and eases the card's lean toward it.
pub(super) fn lean(ui: &egui::Ui, rect: Rect, id: egui::Id, hovered: bool) -> Lean {
    let ctx = ui.ctx();
    let motion = ctx.global_style().animation_time > 0.0;
    let hover = theme::ease(ctx, id.with("lean hover"), hovered);
    let target = match ctx.pointer_latest_pos() {
        Some(pointer) if hovered && rect.width() > 0.0 && rect.height() > 0.0 => {
            let local = (pointer - rect.center()) / (rect.size() / 2.0);
            vec2(local.x.clamp(-1.0, 1.0), local.y.clamp(-1.0, 1.0))
        }
        _ => Vec2::ZERO,
    };
    let key = id.with("lean pointer");
    let last: Vec2 = ctx.data(|data| data.get_temp(key)).unwrap_or(target);
    let step = 1.0 - (-FOLLOW * ctx.input(|input| input.stable_dt).min(0.1)).exp();
    let mut pointer = last + (target - last) * step;
    if (pointer - target).length() < 0.002 {
        pointer = target;
    } else if motion {
        ctx.request_repaint();
    }
    ctx.data_mut(|data| data.insert_temp(key, pointer));
    Lean {
        pointer,
        hover,
        motion,
    }
}

/// Paints a card with `paint`, then leans what it drew by `lean`, with a
/// lift shadow under it and the glare over it. `radius` is the card's
/// corner radius.
pub(super) fn leaning<R>(
    ui: &mut egui::Ui,
    rect: Rect,
    radius: u8,
    lean: Lean,
    paint: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let palette = theme::palette(ui.ctx());
    if lean.hover > 0.0 && lean.motion {
        let shadow = egui::Shadow {
            offset: [
                (-lean.pointer.x * 4.0 * lean.hover) as i8,
                (6.0 + 4.0 * lean.hover) as i8,
            ],
            blur: 22,
            spread: 0,
            color: Color32::BLACK
                .gamma_multiply(if palette.dark { 0.45 } else { 0.16 } * lean.hover),
        };
        ui.painter().add(shadow.as_shape(rect.shrink(4.0), radius));
    }
    let layer = ui.layer_id();
    let start = ui
        .ctx()
        .graphics_mut(|graphics| graphics.entry(layer).next_idx());
    let inner = paint(ui);
    glare(ui, rect, radius, lean);
    if lean.motion && lean.hover > 0.0 {
        let end = ui
            .ctx()
            .graphics_mut(|graphics| graphics.entry(layer).next_idx());
        let tilt = Tilt::new(rect, lean);
        warp(ui, start.0..end.0, &tilt);
    }
    inner
}

/// A soft light where the pointer is, faded out toward the card's edge so
/// it keeps inside its rounded corners. A mesh, so it leans with the card.
fn glare(ui: &egui::Ui, rect: Rect, radius: u8, lean: Lean) {
    const COLUMNS: usize = 16;
    const ROWS: usize = 10;
    if lean.hover <= 0.0 {
        return;
    }
    let palette = theme::palette(ui.ctx());
    let peak = if palette.dark { 0.14 } else { 0.10 } * lean.hover;
    let light = rect.center() + lean.pointer * rect.size() / 2.0;
    let reach = rect.size().max_elem() * 0.75;
    let edge = f32::from(radius).max(4.0);
    let mut mesh = egui::Mesh::default();
    for row in 0..=ROWS {
        for column in 0..=COLUMNS {
            let at = pos2(
                rect.left() + rect.width() * column as f32 / COLUMNS as f32,
                rect.top() + rect.height() * row as f32 / ROWS as f32,
            );
            let near = (1.0 - (at - light).length() / reach).clamp(0.0, 1.0);
            let inside = ((at.x - rect.left()).min(rect.right() - at.x))
                .min((at.y - rect.top()).min(rect.bottom() - at.y));
            let fade = (inside / edge).clamp(0.0, 1.0);
            mesh.colored_vertex(
                at,
                palette
                    .text_strong
                    .gamma_multiply(peak * near * near * fade),
            );
        }
    }
    let width = (COLUMNS + 1) as u32;
    for row in 0..ROWS as u32 {
        for column in 0..COLUMNS as u32 {
            let a = row * width + column;
            mesh.add_triangle(a, a + 1, a + width);
            mesh.add_triangle(a + 1, a + width + 1, a + width);
        }
    }
    ui.painter().add(mesh);
}

/// The card's rotation and lift as a point projection.
pub(super) struct Tilt {
    center: Pos2,
    /// sin and cos of the turn about the vertical axis, then the horizontal.
    yaw: (f32, f32),
    pitch: (f32, f32),
    grow: f32,
}

impl Tilt {
    pub(super) fn new(rect: Rect, lean: Lean) -> Self {
        // Pressed where the pointer is: that side goes away from the eye.
        let yaw = -lean.pointer.x * MAX_LEAN * lean.hover;
        let pitch = lean.pointer.y * MAX_LEAN * lean.hover;
        Self {
            center: rect.center(),
            yaw: yaw.sin_cos(),
            pitch: pitch.sin_cos(),
            grow: 1.0 + LIFT * lean.hover,
        }
    }

    /// Where `point`, on the flat card, shows on screen.
    pub(super) fn project(&self, point: Pos2) -> Pos2 {
        let v = (point - self.center) * self.grow;
        let (sin_yaw, cos_yaw) = self.yaw;
        let (sin_pitch, cos_pitch) = self.pitch;
        let x = v.x * cos_yaw;
        let z = -v.x * sin_yaw;
        let y = v.y * cos_pitch - z * sin_pitch;
        let z = v.y * sin_pitch + z * cos_pitch;
        let scale = EYE / (EYE + z);
        self.center + vec2(x, y) * scale
    }
}

/// Replaces the shapes at `range` in the layer with their projected
/// triangles. Text moves with the surface but stays flat.
fn warp(ui: &egui::Ui, range: std::ops::Range<usize>, tilt: &Tilt) {
    let ctx = ui.ctx();
    let pixels_per_point = ctx.pixels_per_point();
    let options = ctx.tessellation_options(|options| *options);
    // No text is tessellated here, so the font atlas isn't needed.
    let mut tessellator = epaint::Tessellator::new(pixels_per_point, options, [1, 1], Vec::new());
    // Grown so corners that lean toward the eye aren't cut off; still
    // inside whatever holds the card (the page's scroll area).
    let outer = ui.clip_rect();
    let layer = ui.layer_id();
    ctx.graphics_mut(|graphics| {
        let list = graphics.entry(layer);
        for index in range {
            list.mutate_shape(egui::layers::ShapeIdx(index), |clipped| {
                let ClippedShape { clip_rect, shape } = clipped;
                *clip_rect = clip_rect.expand(12.0).intersect(outer);
                warp_shape(shape, tilt, &mut tessellator);
            });
        }
    });
}

fn warp_shape(shape: &mut Shape, tilt: &Tilt, tessellator: &mut epaint::Tessellator) {
    match shape {
        Shape::Noop | Shape::Callback(_) => {}
        Shape::Vec(shapes) => {
            for shape in shapes {
                warp_shape(shape, tilt, tessellator);
            }
        }
        Shape::Text(text) => {
            let anchor = text.visual_bounding_rect().center();
            text.pos += tilt.project(anchor) - anchor;
        }
        _ => {
            let mut mesh = egui::Mesh::default();
            tessellator.tessellate_shape(std::mem::replace(shape, Shape::Noop), &mut mesh);
            for vertex in &mut mesh.vertices {
                vertex.pos = tilt.project(vertex.pos);
            }
            *shape = Shape::mesh(mesh);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lean(x: f32, y: f32) -> Lean {
        Lean {
            pointer: vec2(x, y),
            hover: 1.0,
            motion: true,
        }
    }

    #[test]
    fn a_card_at_rest_is_drawn_where_it_is() {
        let rect = Rect::from_min_size(pos2(100.0, 50.0), vec2(240.0, 160.0));
        let tilt = Tilt::new(rect, Lean::default());
        for corner in [rect.left_top(), rect.right_bottom(), rect.center()] {
            assert!((tilt.project(corner) - corner).length() < 1e-3);
        }
    }

    #[test]
    fn the_side_under_the_pointer_is_pressed_away() {
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(240.0, 160.0));
        // Pointer at the right: the right edge is shorter than the left.
        let tilt = Tilt::new(rect, lean(1.0, 0.0));
        let height = |top: Pos2, bottom: Pos2| tilt.project(bottom).y - tilt.project(top).y;
        let right = height(rect.right_top(), rect.right_bottom());
        let left = height(rect.left_top(), rect.left_bottom());
        assert!(right < left, "{right} vs {left}");
        // Pointer at the bottom: the bottom edge is shorter than the top.
        let tilt = Tilt::new(rect, lean(0.0, 1.0));
        let width = |left: Pos2, right: Pos2| tilt.project(right).x - tilt.project(left).x;
        assert!(
            width(rect.left_bottom(), rect.right_bottom())
                < width(rect.left_top(), rect.right_top())
        );
        // The lean stays small: no corner moves more than a few percent.
        let tilt = Tilt::new(rect, lean(1.0, 1.0));
        for corner in [
            rect.left_top(),
            rect.right_top(),
            rect.left_bottom(),
            rect.right_bottom(),
        ] {
            assert!((tilt.project(corner) - corner).length() < 14.0);
        }
    }

    #[test]
    fn inner_layers_drift_toward_the_pointer_only_with_motion() {
        assert_eq!(lean(1.0, -1.0).drift(4.0), vec2(4.0, -4.0));
        let still = Lean {
            motion: false,
            ..lean(1.0, -1.0)
        };
        assert_eq!(still.drift(4.0), Vec2::ZERO);
    }
}
