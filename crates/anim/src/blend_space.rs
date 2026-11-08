//! 2D blend spaces (Phase 78): clips placed at points of two Number
//! parameters, mixed across the triangles between those points.
//!
//! The points are joined into triangles (a Delaunay triangulation, so the
//! triangles are as even as the points allow). A value inside a triangle
//! shares the pose among its three corners by how close it is to each
//! (barycentric weights), so at a point that clip plays alone, exactly.
//! Outside every triangle, the value is moved to the nearest edge and its
//! two ends share it. Points all on one line blend as a 1D blend tree along
//! that line.

use crate::files::blend_weights;

/// Each point's weight at `value`: up to three are above 0, and they add up
/// to 1 (none for no points). A value on a point gives that point 1.
pub fn blend_space_weights(points: &[[f64; 2]], value: [f64; 2]) -> Vec<f64> {
    let mut weights = vec![0.0; points.len()];
    if points.is_empty() {
        return weights;
    }
    let value = value.map(|v| if v.is_finite() { v } else { 0.0 });
    let points: Vec<[f64; 2]> = points
        .iter()
        .map(|p| p.map(|v| if v.is_finite() { v } else { 0.0 }))
        .collect();
    if let Some(exact) = points.iter().position(|&p| distance2(p, value) < 1e-18) {
        weights[exact] = 1.0;
        return weights;
    }
    let triangles = triangulate(&points);
    if triangles.is_empty() {
        return along_a_line(&points, value);
    }
    for &[a, b, c] in &triangles {
        let Some([u, v, w]) = barycentric(points[a], points[b], points[c], value) else {
            continue;
        };
        if u >= -1e-9 && v >= -1e-9 && w >= -1e-9 {
            let (u, v, w) = (u.max(0.0), v.max(0.0), w.max(0.0));
            let total = u + v + w;
            weights[a] += u / total;
            weights[b] += v / total;
            weights[c] += w / total;
            return weights;
        }
    }
    // Outside: the nearest point on any triangle's edge (it's on the hull).
    let mut best = (f64::INFINITY, 0, 0, 0.0);
    for &[a, b, c] in &triangles {
        for (p, q) in [(a, b), (b, c), (c, a)] {
            let t = closest_on_segment(points[p], points[q], value);
            let on = lerp(points[p], points[q], t);
            let d = distance2(on, value);
            if d < best.0 {
                best = (d, p, q, t);
            }
        }
    }
    let (_, p, q, t) = best;
    weights[p] += 1.0 - t;
    weights[q] += t;
    weights
}

/// The triangles joining `points` (indices into it), none if they're all on
/// one line. A point given twice is used once.
pub fn triangulate(points: &[[f64; 2]]) -> Vec<[usize; 3]> {
    let unique: Vec<usize> = (0..points.len())
        .filter(|&i| !points[..i].iter().any(|&p| distance2(p, points[i]) < 1e-18))
        .collect();
    if unique.len() < 3 {
        return Vec::new();
    }
    // Bowyer-Watson: start with one triangle around everything, add the
    // points one by one, each time taking out the triangles whose circle
    // holds the point and joining the hole's edges to it.
    let (mut low, mut high) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for &i in &unique {
        for axis in 0..2 {
            low[axis] = low[axis].min(points[i][axis]);
            high[axis] = high[axis].max(points[i][axis]);
        }
    }
    let size = (high[0] - low[0]).max(high[1] - low[1]).max(1.0) * 1000.0;
    let middle = [(low[0] + high[0]) / 2.0, (low[1] + high[1]) / 2.0];
    let mut all: Vec<[f64; 2]> = points.to_vec();
    let outer = all.len();
    all.push([middle[0] - 2.0 * size, middle[1] - size]);
    all.push([middle[0] + 2.0 * size, middle[1] - size]);
    all.push([middle[0], middle[1] + 2.0 * size]);
    let mut triangles: Vec<[usize; 3]> = vec![[outer, outer + 1, outer + 2]];
    for &i in &unique {
        let p = all[i];
        let (bad, good): (Vec<[usize; 3]>, Vec<[usize; 3]>) = triangles
            .into_iter()
            .partition(|&t| in_circumcircle(all[t[0]], all[t[1]], all[t[2]], p));
        triangles = good;
        // The hole's edges are those of exactly one bad triangle.
        let edges: Vec<(usize, usize)> = bad
            .iter()
            .flat_map(|t| [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])])
            .collect();
        for &(a, b) in &edges {
            let shared = edges
                .iter()
                .filter(|&&(c, d)| (c == a && d == b) || (c == b && d == a))
                .count();
            if shared == 1 {
                triangles.push([a, b, i]);
            }
        }
    }
    triangles.retain(|t| {
        t.iter().all(|&v| v < outer) && area2(all[t[0]], all[t[1]], all[t[2]]).abs() > 1e-12
    });
    triangles
}

/// Points all on one line: a 1D blend along it.
fn along_a_line(points: &[[f64; 2]], value: [f64; 2]) -> Vec<f64> {
    let (mut from, mut to, mut far) = (points[0], points[0], 0.0);
    for &p in points {
        for &q in points {
            let d = distance2(p, q);
            if d > far {
                (from, to, far) = (p, q, d);
            }
        }
    }
    if far <= 0.0 {
        // Every point in one place: the first counts.
        let mut weights = vec![0.0; points.len()];
        weights[0] = 1.0;
        return weights;
    }
    let along =
        |p: [f64; 2]| (p[0] - from[0]) * (to[0] - from[0]) + (p[1] - from[1]) * (to[1] - from[1]);
    let thresholds: Vec<f64> = points.iter().map(|&p| along(p)).collect();
    blend_weights(&thresholds, along(value))
}

fn distance2(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)
}

fn lerp(a: [f64; 2], b: [f64; 2], t: f64) -> [f64; 2] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

/// Twice the signed area of the triangle `a b c`.
fn area2(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])
}

/// How far along `a`→`b` the point nearest `p` is, from 0 to 1.
fn closest_on_segment(a: [f64; 2], b: [f64; 2], p: [f64; 2]) -> f64 {
    let length2 = distance2(a, b);
    if length2 <= 0.0 {
        return 0.0;
    }
    (((p[0] - a[0]) * (b[0] - a[0]) + (p[1] - a[1]) * (b[1] - a[1])) / length2).clamp(0.0, 1.0)
}

/// `p`'s weights for the corners `a`, `b` and `c` (adding up to 1; some
/// below 0 when `p` is outside). `None` for a flat triangle.
fn barycentric(a: [f64; 2], b: [f64; 2], c: [f64; 2], p: [f64; 2]) -> Option<[f64; 3]> {
    let area = area2(a, b, c);
    if area.abs() <= 1e-12 {
        return None;
    }
    let u = area2(p, b, c) / area;
    let v = area2(a, p, c) / area;
    Some([u, v, 1.0 - u - v])
}

/// Whether `p` is strictly inside the circle through `a`, `b` and `c`.
fn in_circumcircle(a: [f64; 2], b: [f64; 2], c: [f64; 2], p: [f64; 2]) -> bool {
    let d = 2.0 * area2(a, b, c);
    if d.abs() <= 1e-12 {
        return false;
    }
    let (a2, b2, c2) = (
        a[0] * a[0] + a[1] * a[1],
        b[0] * b[0] + b[1] * b[1],
        c[0] * c[0] + c[1] * c[1],
    );
    let center = [
        (a2 * (b[1] - c[1]) + b2 * (c[1] - a[1]) + c2 * (a[1] - b[1])) / d,
        (a2 * (c[0] - b[0]) + b2 * (a[0] - c[0]) + c2 * (b[0] - a[0])) / d,
    ];
    let radius2 = distance2(center, a);
    distance2(center, p) < radius2 * (1.0 - 1e-12)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Eight directions and the middle, on a 3×3 grid.
    fn grid() -> Vec<[f64; 2]> {
        let mut points = vec![[0.0, 0.0]];
        for x in [-1.0, 0.0, 1.0] {
            for y in [-1.0, 0.0, 1.0] {
                if x != 0.0 || y != 0.0 {
                    points.push([x, y]);
                }
            }
        }
        points
    }

    fn mixed(points: &[[f64; 2]], weights: &[f64]) -> [f64; 2] {
        points.iter().zip(weights).fold([0.0, 0.0], |sum, (p, w)| {
            [sum[0] + p[0] * w, sum[1] + p[1] * w]
        })
    }

    #[test]
    fn each_point_plays_alone_at_its_place() {
        let points = grid();
        for (i, &p) in points.iter().enumerate() {
            let weights = blend_space_weights(&points, p);
            let mut want = vec![0.0; points.len()];
            want[i] = 1.0;
            assert_eq!(weights, want, "{p:?}");
        }
    }

    #[test]
    fn inside_the_grid_the_weights_add_up_to_the_value() {
        let points = grid();
        assert_eq!(triangulate(&points).len(), 8);
        for step_x in 0..=20 {
            for step_y in 0..=20 {
                let value = [-1.0 + step_x as f64 * 0.1, -1.0 + step_y as f64 * 0.1];
                let weights = blend_space_weights(&points, value);
                let total: f64 = weights.iter().sum();
                assert!((total - 1.0).abs() < 1e-9, "{value:?} {weights:?}");
                assert!(weights.iter().filter(|w| **w > 0.0).count() <= 3);
                assert!(weights.iter().all(|w| *w >= 0.0));
                let at = mixed(&points, &weights);
                assert!(
                    distance2(at, value) < 1e-18,
                    "{value:?} mixed to {at:?} by {weights:?}"
                );
            }
        }
    }

    #[test]
    fn outside_the_points_the_nearest_edge_counts() {
        let points = grid();
        // Far ahead of forward-right: forward-right alone.
        let weights = blend_space_weights(&points, [3.0, 3.0]);
        assert_eq!(
            weights[points.iter().position(|p| *p == [1.0, 1.0]).unwrap()],
            1.0
        );
        // Beyond the right edge, halfway up: right and forward-right share.
        let weights = blend_space_weights(&points, [2.0, 0.5]);
        let right = points.iter().position(|p| *p == [1.0, 0.0]).unwrap();
        let forward_right = points.iter().position(|p| *p == [1.0, 1.0]).unwrap();
        assert!((weights[right] - 0.5).abs() < 1e-9, "{weights:?}");
        assert!((weights[forward_right] - 0.5).abs() < 1e-9, "{weights:?}");
    }

    #[test]
    fn points_on_a_circle_and_a_line_and_too_few() {
        // Eight directions on the unit circle, all on one circle round the
        // middle.
        let mut points = vec![[0.0, 0.0]];
        for i in 0..8 {
            let angle = i as f64 * std::f64::consts::FRAC_PI_4;
            points.push([angle.cos(), angle.sin()]);
        }
        let weights = blend_space_weights(&points, [0.3, 0.2]);
        let at = mixed(&points, &weights);
        assert!(distance2(at, [0.3, 0.2]) < 1e-18, "{weights:?}");
        // On a line, as a 1D blend.
        let line = [[0.0, 0.0], [2.0, 0.0], [4.0, 0.0]];
        assert_eq!(blend_space_weights(&line, [1.0, 5.0]), vec![0.5, 0.5, 0.0]);
        assert_eq!(blend_space_weights(&[[1.0, 1.0]], [9.0, 9.0]), vec![1.0]);
        assert!(blend_space_weights(&[], [0.0, 0.0]).is_empty());
    }
}
