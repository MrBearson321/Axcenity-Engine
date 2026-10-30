//! The view cube in the viewport's top-right corner: a small cube turned
//! like the world as the camera sees it, with each face named (Front,
//! Back, Left, Right, Top, Bottom) and the world's X, Y and Z axes in red,
//! green and blue, so it always says which way the camera faces. Clicking
//! a face swings the camera round to look at its focus point from that
//! side, from as far away as it was.
//!
//! Front is the +Z face: the one seen when looking toward -Z, which is
//! where a new camera and Studio's look, so the default view reads Front,
//! with Right on the right.
//!
//! It's drawn with a little perspective, as if seen from [`EYE`] away, so
//! it reads as a box at every angle: drawn flat, a level camera sees the
//! top edge-on and the cube turns into two rectangles. The axis tips sit
//! just outside its outline, the way each axis points on screen, so they
//! never cover a face or its name.

use axcenity_render::glam::Vec3;
use axcenity_scene::FlyCamera;
use eframe::egui;

use crate::theme;

/// How far each face is from the cube's middle, in points.
const HALF: f32 = 26.0;
/// How far the eye looking at the cube is from its middle, in points: near
/// enough for some perspective, far enough that it isn't distorted.
const EYE: f32 = 5.0 * HALF;
/// How far past the cube's outline the axis tips sit, in points.
const TIP_GAP: f32 = 13.0;
/// Axes shorter than this share of a face's reach on screen point nearly
/// at (or away from) the camera: their face shows the way, so no tip.
const MIN_AXIS_SHARE: f32 = 0.25;
/// How far the cube's middle is in from the viewport's corner.
const INSET: f32 = 74.0;
/// Faces narrower than this on screen, in points, are edge-on: not drawn.
const MIN_FACE_WIDTH: f32 = 3.0;

/// One face: which way it faces, and its name.
const FACES: [(Vec3, &str); 6] = [
    (Vec3::Z, "Front"),
    (Vec3::NEG_Z, "Back"),
    (Vec3::X, "Right"),
    (Vec3::NEG_X, "Left"),
    (Vec3::Y, "Top"),
    (Vec3::NEG_Y, "Bottom"),
];

/// Red, green and blue for X, Y and Z, as on the Move handles.
const AXES: [(Vec3, &str, egui::Color32); 3] = [
    (Vec3::X, "X", egui::Color32::from_rgb(235, 65, 65)),
    (Vec3::Y, "Y", egui::Color32::from_rgb(70, 205, 70)),
    (Vec3::Z, "Z", egui::Color32::from_rgb(55, 125, 245)),
];

/// The camera's right, up and forward, for turning points around the
/// cube's middle into points on screen.
struct Basis {
    right: Vec3,
    up: Vec3,
    forward: Vec3,
}

impl Basis {
    fn of(forward: Vec3) -> Basis {
        let right = forward.cross(Vec3::Y).normalize_or(Vec3::X);
        Basis {
            right,
            up: right.cross(forward),
            forward,
        }
    }

    /// Where `at` (from the cube's middle, in points) is drawn, seen from
    /// [`EYE`] away: nearer things a little bigger.
    fn point(&self, center: egui::Pos2, at: Vec3) -> egui::Pos2 {
        let scale = EYE / (EYE + at.dot(self.forward)).max(1.0);
        center + egui::vec2(at.dot(self.right), -at.dot(self.up)) * scale
    }

    /// How much `normal` faces the camera: 1 straight at it, 0 edge-on.
    fn facing(&self, normal: Vec3) -> f32 {
        -normal.dot(self.forward)
    }

    /// Whether the eye sees the face facing `normal`: it's on that face's
    /// outer side, and the face is wider than a sliver on screen. Seen from
    /// near, a face only just turned toward the camera can still be edge-on
    /// or behind, such as the top when level; and egui's anti-aliasing
    /// stretches a nearly flat face into long spikes.
    fn sees(&self, normal: Vec3) -> bool {
        let eye = -self.forward * EYE;
        (eye - normal * HALF).dot(normal) > 0.0
            && area(&corners(self, egui::Pos2::ZERO, normal)) > MIN_FACE_WIDTH * 2.0 * HALF
    }
}

/// A face's four corners on screen, going round it.
fn corners(basis: &Basis, center: egui::Pos2, normal: Vec3) -> [egui::Pos2; 4] {
    let (a, b) = if normal.y.abs() > 0.5 {
        (Vec3::X, Vec3::Z)
    } else {
        (Vec3::Y, normal.cross(Vec3::Y))
    };
    [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)]
        .map(|(s, t)| basis.point(center, (normal + a * s + b * t) * HALF))
}

/// The area `corners` cover on screen, in square points.
fn area(corners: &[egui::Pos2; 4]) -> f32 {
    let twice: f32 = (0..4)
        .map(|i| {
            let (a, b) = (corners[i], corners[(i + 1) % 4]);
            a.x * b.y - b.x * a.y
        })
        .sum();
    twice.abs() / 2.0
}

/// Whether `point` is inside the convex shape `corners` (either way round).
fn inside(corners: &[egui::Pos2; 4], point: egui::Pos2) -> bool {
    let sides: Vec<f32> = (0..4)
        .map(|i| {
            let (a, b) = (corners[i], corners[(i + 1) % 4]);
            (b - a).x * (point - a).y - (b - a).y * (point - a).x
        })
        .collect();
    sides.iter().all(|&side| side >= 0.0) || sides.iter().all(|&side| side <= 0.0)
}

/// The face under `point`, of those the eye sees.
fn face_at(basis: &Basis, center: egui::Pos2, point: egui::Pos2) -> Option<Vec3> {
    FACES
        .iter()
        .map(|&(normal, _)| normal)
        .filter(|&normal| basis.sees(normal))
        .find(|&normal| inside(&corners(basis, center, normal), point))
}

/// Where the tip of the axis along `direction` goes: just outside the
/// cube's outline, the way the axis points on screen. `None` for an axis
/// pointing nearly at or away from the camera.
fn tip_at(basis: &Basis, center: egui::Pos2, direction: Vec3) -> Option<egui::Pos2> {
    let along = basis.point(center, direction * HALF) - center;
    if along.length() < HALF * MIN_AXIS_SHARE {
        return None;
    }
    let way = along.normalized();
    // How far the outline reaches that way: its furthest corner.
    let signs = [-1.0_f32, 1.0];
    let reach = signs
        .iter()
        .flat_map(|&x| {
            signs
                .iter()
                .flat_map(move |&y| signs.map(|z| Vec3::new(x, y, z)))
        })
        .map(|corner| (basis.point(center, corner * HALF) - center).dot(way))
        .fold(0.0, f32::max);
    Some(center + way * (reach + TIP_GAP))
}

/// Where the cube's middle is in the viewport `rect`, or `None` if the
/// viewport is too small to hold it.
fn middle(rect: egui::Rect) -> Option<egui::Pos2> {
    let center = egui::pos2(rect.right() - INSET, rect.top() + INSET);
    let area = egui::Rect::from_center_size(center, egui::Vec2::splat(2.0 * INSET - 8.0));
    rect.contains_rect(area).then_some(center)
}

/// The face of the cube in viewport `rect` under `point`, for a camera
/// looking along `forward`: the side a click there looks from. The
/// viewport asks before anything else, so a click on the cube never
/// selects what's behind it.
pub(super) fn face_under(rect: egui::Rect, forward: Vec3, point: egui::Pos2) -> Option<Vec3> {
    face_at(&Basis::of(forward), middle(rect)?, point)
}

/// Draws the cube in the top-right of `rect` for a camera looking along
/// `forward`, with the `hovered` face lit.
pub(super) fn paint(ui: &egui::Ui, rect: egui::Rect, forward: Vec3, hovered: Option<Vec3>) {
    let Some(center) = middle(rect) else {
        return;
    };
    let basis = Basis::of(forward);
    let palette = theme::palette(ui.ctx());
    let painter = ui.painter_at(rect);
    // The axes run out from the middle, under the cube, so only the part
    // past its outline shows; those pointing away are dimmed.
    let tips: Vec<(egui::Pos2, &str, egui::Color32)> = AXES
        .iter()
        .filter_map(|&(direction, name, color)| {
            let at = tip_at(&basis, center, direction)?;
            let color = if direction.dot(basis.forward) > 0.0 {
                color.gamma_multiply(0.5)
            } else {
                color
            };
            Some((at, name, color))
        })
        .collect();
    for &(at, _, color) in &tips {
        painter.line_segment([center, at], egui::Stroke::new(2.0_f32, color));
    }
    // The faces the eye sees, furthest first.
    let mut shown: Vec<(Vec3, &str)> = FACES
        .iter()
        .copied()
        .filter(|&(normal, _)| basis.sees(normal))
        .collect();
    shown.sort_by(|a, b| basis.facing(a.0).total_cmp(&basis.facing(b.0)));
    for (normal, name) in shown {
        let facing = basis.facing(normal);
        let fill = if hovered == Some(normal) {
            palette.accent_soft
        } else {
            theme::mix(palette.surface, palette.elevated, facing)
        };
        // Filled as a plain mesh and outlined edge by edge: egui's
        // anti-aliased polygons grow spikes at a narrow face's sharp
        // corners. The outline covers the mesh's hard edges.
        let points = corners(&basis, center, normal);
        let mut mesh = egui::Mesh::default();
        for point in points {
            mesh.colored_vertex(point, fill.gamma_multiply(0.94));
        }
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        painter.add(egui::Shape::mesh(mesh));
        for i in 0..4 {
            painter.line_segment(
                [points[i], points[(i + 1) % 4]],
                egui::Stroke::new(1.0_f32, palette.border_strong),
            );
        }
        // A name is only drawn where it fits inside its face.
        let color = if hovered == Some(normal) {
            palette.accent_text
        } else {
            palette.text_strong
        };
        let galley = painter.layout_no_wrap(
            name.to_owned(),
            egui::FontId::new(8.5 + 2.5 * facing, theme::medium_family()),
            color.gamma_multiply(facing.clamp(0.5, 1.0)),
        );
        let middle = basis.point(center, normal * HALF);
        if fits(&points, middle, galley.size()) {
            painter.galley(middle - galley.size() / 2.0, galley, color);
        }
    }
    for (at, name, color) in tips {
        painter.circle_filled(at, 8.0, color);
        painter.text(
            at,
            egui::Align2::CENTER_CENTER,
            name,
            egui::FontId::new(10.0, theme::medium_family()),
            egui::Color32::WHITE,
        );
    }
}

/// Whether a name of `size`, centred on `middle`, fits inside the face
/// `corners` with a little room round it, so no other face cuts it off.
fn fits(corners: &[egui::Pos2; 4], middle: egui::Pos2, size: egui::Vec2) -> bool {
    let room = egui::Rect::from_center_size(middle, size + egui::vec2(6.0, 4.0));
    [
        room.left_top(),
        room.right_top(),
        room.right_bottom(),
        room.left_bottom(),
    ]
    .iter()
    .all(|&point| inside(corners, point))
}

/// `camera` swung round to look at its focus point from the `side` a view
/// cube face faces, as far from it as before, at the same flying speed.
pub fn looking_from(camera: &FlyCamera, side: Vec3) -> FlyCamera {
    let focus = camera.focus();
    let distance = camera.position.distance(focus);
    // Straight down or up has no way to face: lean a hair toward -Z, so
    // the top view keeps Front at the bottom.
    let side = if side.y.abs() > 0.5 {
        (side + Vec3::Z * 1e-3).normalize()
    } else {
        side
    };
    let mut turned = FlyCamera::looking_at(focus + side * distance, focus);
    // The camera never looks quite straight down, so put it back along
    // the way it does look, keeping the focus point where it was.
    turned.position = focus - turned.forward() * distance;
    turned.set_speed(camera.speed());
    turned
}

#[cfg(test)]
mod tests {
    use super::*;

    const CENTER: egui::Pos2 = egui::pos2(100.0, 100.0);

    fn seen(basis: &Basis) -> Vec<Vec3> {
        FACES
            .iter()
            .map(|&(normal, _)| normal)
            .filter(|&normal| basis.sees(normal))
            .collect()
    }

    #[test]
    fn looking_toward_minus_z_shows_front_with_right_on_the_right() {
        let basis = Basis::of(Vec3::NEG_Z);
        assert_eq!(face_at(&basis, CENTER, CENTER), Some(Vec3::Z));
        let right = tip_at(&basis, CENTER, Vec3::X).unwrap();
        assert!(right.x > CENTER.x + HALF);
        // Up on screen is +Y (egui's Y runs down).
        assert!(tip_at(&basis, CENTER, Vec3::Y).unwrap().y < CENTER.y - HALF);
        // Z points at the camera: Front says so, with no tip over it.
        assert_eq!(tip_at(&basis, CENTER, Vec3::Z), None);
        // From behind, the middle is Back.
        assert_eq!(
            face_at(&Basis::of(Vec3::Z), CENTER, CENTER),
            Some(Vec3::NEG_Z)
        );
    }

    #[test]
    fn looking_down_from_above_and_to_one_side_shows_top_and_that_side() {
        let basis = Basis::of(Vec3::new(-1.0, -1.0, 0.0).normalize());
        assert_eq!(seen(&basis), [Vec3::X, Vec3::Y]);
    }

    #[test]
    fn level_and_at_an_angle_it_still_looks_like_a_box_with_its_tips_clear_of_it() {
        // Level, between Front and Right: no top or bottom, and the near
        // edge stands taller than the far ones.
        let basis = Basis::of(Vec3::new(-1.0, 0.0, -1.0).normalize());
        assert_eq!(seen(&basis), [Vec3::Z, Vec3::X]);
        let height = |corner: Vec3| {
            let top = basis.point(CENTER, (corner + Vec3::Y) * HALF);
            let bottom = basis.point(CENTER, (corner - Vec3::Y) * HALF);
            bottom.y - top.y
        };
        let near = height(Vec3::new(1.0, 0.0, 1.0));
        let far = height(Vec3::new(-1.0, 0.0, 1.0));
        assert!(near > far + 2.0, "{near} {far}");
        // Every tip is clear of every face it could cover, at many angles.
        for yaw in 0..24 {
            for pitch in -4..=4 {
                let (yaw, pitch) = (yaw as f32 * 0.26, pitch as f32 * 0.35);
                let forward = Vec3::new(
                    -yaw.sin() * pitch.cos(),
                    pitch.sin(),
                    -yaw.cos() * pitch.cos(),
                );
                let basis = Basis::of(forward);
                for (direction, _, _) in AXES {
                    let Some(tip) = tip_at(&basis, CENTER, direction) else {
                        continue;
                    };
                    for normal in seen(&basis) {
                        let face = corners(&basis, CENTER, normal);
                        assert!(
                            !inside(&face, tip),
                            "{direction} over {normal} at {forward}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_face_turning_edge_on_is_hidden_before_it_becomes_a_sliver() {
        // Turning slowly away from looking straight at Right, Front comes
        // into view on the left; it's never drawn thinner than a few points.
        for step in 0..400 {
            let yaw = step as f32 * 0.001;
            let basis = Basis::of(Vec3::new(-yaw.cos(), 0.0, -yaw.sin()));
            for normal in seen(&basis) {
                let face = corners(&basis, CENTER, normal);
                // Level, every face seen stands upright: corners 0 and 1
                // are its top and bottom.
                let width = area(&face) / (face[0].y - face[1].y).abs().max(1.0);
                assert!(width >= MIN_FACE_WIDTH, "{normal} is {width} wide at {yaw}");
            }
        }
    }

    #[test]
    fn a_name_is_only_drawn_where_it_fits_inside_its_face() {
        // Looking a little down, the top is a thin strip: wide enough
        // across for a name, but not tall enough.
        let label = egui::vec2(18.0, 10.0);
        let basis = Basis::of(Vec3::new(-0.6, -0.3, -0.75).normalize());
        assert!(seen(&basis).contains(&Vec3::Y));
        let top = corners(&basis, CENTER, Vec3::Y);
        assert!(!fits(&top, basis.point(CENTER, Vec3::Y * HALF), label));
        // Straight on, the front has plenty of room.
        let basis = Basis::of(Vec3::NEG_Z);
        let front = corners(&basis, CENTER, Vec3::Z);
        assert!(fits(&front, basis.point(CENTER, Vec3::Z * HALF), label));
    }

    #[test]
    fn a_face_swings_the_camera_round_its_focus_point() {
        let camera = FlyCamera::looking_at(Vec3::new(10.0, 8.0, 16.0), Vec3::new(1.0, 2.0, 3.0));
        let focus = camera.focus();
        let distance = camera.position.distance(focus);
        for (side, _) in FACES {
            let turned = looking_from(&camera, side);
            assert!(turned.focus().distance(focus) < 1e-2, "{side}");
            assert!((turned.position.distance(focus) - distance).abs() < 1e-2);
            // It looks back along the face's direction.
            assert!(turned.forward().dot(-side) > 0.99, "{side}");
            assert_eq!(turned.speed(), camera.speed());
        }
    }
}
