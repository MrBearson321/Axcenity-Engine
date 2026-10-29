//! Packing islands (Phase 58): each island turned to fit its smallest box
//! (round one of its outline's edges, then on its side if it's taller than
//! wide), the boxes laid in rows from the tallest, at the row width that
//! makes the squarest result, and everything scaled into the texture.

use glam::{Affine2, Vec2};

use super::Chart;
use crate::half_edge::EditMesh;

/// Lays out `charts` in the 0 to 1 square of `channel`, scaled alike, with
/// `margin` (a share of the texture's width) between them and half that
/// round the edge. With `rotate`, each is turned to its smallest box.
pub(crate) fn pack(
    mesh: &mut EditMesh,
    charts: &[Chart],
    margin: f32,
    rotate: bool,
    channel: usize,
) {
    if charts.is_empty() {
        return;
    }
    let margin = margin.clamp(0.0, 0.25);
    let turns: Vec<Affine2> = charts
        .iter()
        .map(|chart| {
            let points: Vec<Vec2> = chart.half_edges().map(|c| mesh.uv(c, channel)).collect();
            if rotate {
                Affine2::from_angle(best_angle(&points))
            } else {
                Affine2::IDENTITY
            }
        })
        .collect();
    let boxes: Vec<(Vec2, Vec2)> = charts
        .iter()
        .zip(&turns)
        .map(|(chart, turn)| {
            chart
                .half_edges()
                .map(|c| turn.transform_point2(mesh.uv(c, channel)))
                .fold((Vec2::INFINITY, Vec2::NEG_INFINITY), |(min, max), p| {
                    (min.min(p), max.max(p))
                })
        })
        .collect();
    let sizes: Vec<Vec2> = boxes
        .iter()
        .map(|(min, max)| (*max - *min).max(Vec2::splat(1e-6)))
        .collect();
    let mut order: Vec<usize> = (0..charts.len()).collect();
    order.sort_by(|&a, &b| {
        sizes[b]
            .y
            .total_cmp(&sizes[a].y)
            .then(sizes[b].x.total_cmp(&sizes[a].x))
            .then(a.cmp(&b))
    });
    let total: f32 = sizes.iter().map(|s| s.x * s.y).sum();
    // The padding depends on the scale, which depends on the padding: a
    // few rounds settle it.
    let mut scale = 1.0 / total.sqrt().max(1e-9);
    let (mut places, mut pad) = (Vec::new(), 0.0);
    for _ in 0..6 {
        pad = margin / scale;
        let side;
        (side, places) = shelves(&sizes, &order, pad);
        scale = 1.0 / side.max(1e-9);
    }
    for (k, chart) in charts.iter().enumerate() {
        let matrix = Affine2::from_scale(Vec2::splat(scale))
            * Affine2::from_translation(places[k] + Vec2::splat(pad / 2.0) - boxes[k].0)
            * turns[k];
        for corner in chart.half_edges() {
            let uv = mesh.uv(corner, channel);
            mesh.set_uv(corner, channel, matrix.transform_point2(uv));
        }
    }
}

/// The boxes of `sizes` (each padded by `pad`) in rows, in `order`, at the
/// row width that makes the smallest square: its side, and each box's
/// place (its padded corner).
fn shelves(sizes: &[Vec2], order: &[usize], pad: f32) -> (f32, Vec<Vec2>) {
    let padded: Vec<Vec2> = sizes.iter().map(|s| *s + Vec2::splat(pad)).collect();
    let area: f32 = padded.iter().map(|s| s.x * s.y).sum();
    let widest = padded.iter().map(|s| s.x).fold(0.0, f32::max);
    let mut best: Option<(f32, Vec<Vec2>)> = None;
    for step in 0..=40 {
        let width = (area.sqrt() * (1.0 + step as f32 * 0.025)).max(widest);
        let mut places = vec![Vec2::ZERO; sizes.len()];
        let (mut x, mut y, mut row, mut used) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        for &k in order {
            let size = padded[k];
            if x > 0.0 && x + size.x > width {
                y += row;
                x = 0.0;
                row = 0.0;
            }
            places[k] = Vec2::new(x, y);
            x += size.x;
            row = row.max(size.y);
            used = used.max(x);
        }
        let side = used.max(y + row);
        if best.as_ref().is_none_or(|(b, _)| side < *b) {
            best = Some((side, places));
        }
    }
    best.expect("at least one try")
}

/// The turn (radians) that puts `points` in their smallest box, lying
/// wider than tall.
fn best_angle(points: &[Vec2]) -> f32 {
    let hull = hull(points);
    let mut best = (f32::INFINITY, 0.0);
    for k in 0..hull.len() {
        let edge = hull[(k + 1) % hull.len()] - hull[k];
        if edge.length_squared() < 1e-20 {
            continue;
        }
        let angle = -edge.y.atan2(edge.x);
        let size = box_size(&hull, angle);
        // A hair's preference for the turn already there keeps squares
        // and boxes that are already upright as they are.
        let area = size.x * size.y * if angle.abs() < 1e-6 { 0.999_99 } else { 1.0 };
        if area < best.0 {
            best = (area, angle);
        }
    }
    let angle = best.1;
    let size = box_size(&hull, angle);
    if size.y > size.x * 1.000_1 {
        angle + std::f32::consts::FRAC_PI_2
    } else {
        angle
    }
}

/// The size of the box round `points` turned by `angle`.
fn box_size(points: &[Vec2], angle: f32) -> Vec2 {
    let turn = Vec2::from_angle(angle);
    let (min, max) = points
        .iter()
        .map(|&p| turn.rotate(p))
        .fold((Vec2::INFINITY, Vec2::NEG_INFINITY), |(min, max), p| {
            (min.min(p), max.max(p))
        });
    max - min
}

/// The convex hull of `points`, counter-clockwise (Andrew's monotone
/// chain).
fn hull(points: &[Vec2]) -> Vec<Vec2> {
    let mut sorted = points.to_vec();
    sorted.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    sorted.dedup_by(|a, b| a.distance_squared(*b) < 1e-20);
    if sorted.len() < 3 {
        return sorted;
    }
    let mut lower: Vec<Vec2> = Vec::new();
    for &p in &sorted {
        while lower.len() >= 2
            && (lower[lower.len() - 1] - lower[lower.len() - 2])
                .perp_dot(p - lower[lower.len() - 2])
                <= 0.0
        {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<Vec2> = Vec::new();
    for &p in sorted.iter().rev() {
        while upper.len() >= 2
            && (upper[upper.len() - 1] - upper[upper.len() - 2])
                .perp_dot(p - upper[upper.len() - 2])
                <= 0.0
        {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}
