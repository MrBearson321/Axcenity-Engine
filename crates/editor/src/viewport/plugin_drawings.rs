//! Plugins' viewport drawings (roadmap G10, `plugin:DrawLine` and the rest):
//! lines, boxes, spheres and labels over the Scene View while editing. They
//! sit on top of the scene, like the other overlays, and never hide behind
//! parts, so a plugin's guides are always readable.

use axcenity_plugins::{Drawing, Shape};
use axcenity_render::glam::{Quat, Vec3};
use eframe::egui;

use super::overlays::v3;
use crate::screen::Screen;

/// How many straight pieces make each of a sphere's rings.
const RING_SEGMENTS: usize = 48;

/// Draws every plugin drawing with `painter`.
pub(super) fn paint(painter: &egui::Painter, screen: &Screen, drawings: &[&Drawing]) {
    for drawing in drawings {
        let color = color32(drawing.color);
        let stroke = egui::Stroke::new(drawing.thickness, color);
        match &drawing.shape {
            Shape::Line { from, to } => line(painter, screen, v3(*from), v3(*to), stroke),
            Shape::Box { cframe, size } => {
                let [x, y, z, w] = cframe.rotation;
                let rotation = Quat::from_xyzw(x, y, z, w).normalize();
                for (a, b) in box_edges(v3(cframe.position), rotation, v3(*size) * 0.5) {
                    line(painter, screen, a, b, stroke);
                }
            }
            Shape::Sphere { center, radius } => {
                for ring in sphere_rings(v3(*center), *radius) {
                    for pair in ring.windows(2) {
                        line(painter, screen, pair[0], pair[1], stroke);
                    }
                }
            }
            Shape::Text { at, text } => {
                let Some(point) = screen.point(v3(*at)) else {
                    continue;
                };
                let font = egui::FontId::proportional(drawing.thickness.clamp(6.0, 48.0));
                // A dark copy under it keeps it readable on any background.
                painter.text(
                    point + egui::vec2(1.0, 1.0),
                    egui::Align2::CENTER_BOTTOM,
                    text,
                    font.clone(),
                    egui::Color32::from_black_alpha(180),
                );
                painter.text(point, egui::Align2::CENTER_BOTTOM, text, font, color);
            }
        }
    }
}

fn line(painter: &egui::Painter, screen: &Screen, a: Vec3, b: Vec3, stroke: egui::Stroke) {
    if let Some(segment) = screen.line(a, b) {
        painter.line_segment(segment, stroke);
    }
}

fn color32(color: axcenity_core::Color3) -> egui::Color32 {
    let byte = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    egui::Color32::from_rgb(byte(color.r), byte(color.g), byte(color.b))
}

/// The 12 edges of a box with half-size `half`, turned by `rotation`
/// about `center`.
pub(super) fn box_edges(center: Vec3, rotation: Quat, half: Vec3) -> [(Vec3, Vec3); 12] {
    let corner = |i: usize| {
        let sign = |bit: usize| if i & bit == 0 { -1.0 } else { 1.0 };
        center + rotation * Vec3::new(sign(1) * half.x, sign(2) * half.y, sign(4) * half.z)
    };
    // Corners differing in one bit share an edge.
    let pairs = [
        (0, 1),
        (2, 3),
        (4, 5),
        (6, 7),
        (0, 2),
        (1, 3),
        (4, 6),
        (5, 7),
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ];
    pairs.map(|(a, b)| (corner(a), corner(b)))
}

/// A sphere's three rings, one in each axis plane, each closed (its last
/// point is its first).
pub(super) fn sphere_rings(center: Vec3, radius: f32) -> [Vec<Vec3>; 3] {
    let ring = |axes: fn(f32, f32) -> Vec3| {
        (0..=RING_SEGMENTS)
            .map(|i| {
                let angle = i as f32 / RING_SEGMENTS as f32 * std::f32::consts::TAU;
                center + axes(angle.cos(), angle.sin()) * radius
            })
            .collect()
    };
    [
        ring(|c, s| Vec3::new(c, s, 0.0)),
        ring(|c, s| Vec3::new(c, 0.0, s)),
        ring(|c, s| Vec3::new(0.0, c, s)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_turned_box_has_twelve_edges_of_its_sides_lengths() {
        let rotation = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let edges = box_edges(Vec3::new(1.0, 2.0, 3.0), rotation, Vec3::new(1.0, 2.0, 3.0));
        let mut lengths: Vec<f32> = edges
            .iter()
            .map(|(a, b)| (a.distance(*b) * 1000.0).round() / 1000.0)
            .collect();
        lengths.sort_by(f32::total_cmp);
        assert_eq!(
            lengths,
            [2.0; 4]
                .into_iter()
                .chain([4.0; 4])
                .chain([6.0; 4])
                .collect::<Vec<_>>()
        );
        // Turned a quarter about Y, the 2-stud sides now run along Z.
        assert!(
            edges
                .iter()
                .any(|(a, b)| (*b - *a).abs().abs_diff_eq(Vec3::new(0.0, 0.0, 2.0), 1e-4))
        );
    }

    #[test]
    fn a_spheres_rings_are_closed_and_lie_on_it() {
        let center = Vec3::new(0.0, 5.0, 0.0);
        for ring in sphere_rings(center, 2.0) {
            assert!(ring.first().unwrap().distance(*ring.last().unwrap()) < 1e-4);
            assert!(ring.iter().all(|p| (p.distance(center) - 2.0).abs() < 1e-4));
        }
    }
}
