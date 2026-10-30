//! The view cube in the viewport's top-right corner, like Fusion 360's: a
//! small shaded block with rounded edges and corners, as Roblox Studio's
//! is, turned like the world as the camera sees it, each face named
//! (FRONT, BACK, LEFT, RIGHT, TOP, BOTTOM) as if printed on it, with the
//! world's X, Y and Z axes in red, green and blue, so it always says which
//! way the camera faces. It can be hidden (Test > View > View Cube).
//!
//! Each face is split three by three: its middle looks from that face, a
//! strip along an edge looks from that edge (between two faces), and a
//! corner square from that corner (between three), so there are 26 views.
//! The rounded edges and corners belong to the strips and squares beside
//! them. The part under the mouse lights up on every face it's on. A click
//! swings the camera smoothly round its focus point to look from there,
//! from as far away as it was ([`Swing`]). The house above the cube goes
//! back to the starting angle, and while a face is seen straight on,
//! arrows beside it turn to the face next to it that way.
//!
//! The block is a grid of small four-sided cells on each face, bent round
//! at the edges; it's convex, so the cells facing the eye never overlap and
//! need no sorting. Each corner is shaded by which way the block faces
//! there, so the rounding reads smoothly. The names are laid out as text
//! and each corner of their glyphs put onto the face in 3D, so they slant
//! and shrink with it in perspective instead of floating flat over it.
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

use crate::theme::{self, icons};

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
/// Where a face's middle ends and its edge strips start, from its middle
/// (0) to its edge (1).
const BAND: f32 = 0.56;
/// How much a face must face the camera to count as seen straight on, when
/// the arrows to the faces beside it show.
const STRAIGHT_ON: f32 = 0.999;
/// How round the edges and corners are: the radius, as a share of
/// [`HALF`].
const ROUND: f32 = 0.24;
/// How many cells each face's rounded rim is split into across: more is
/// smoother.
const ROUND_STEPS: usize = 3;
/// The faces' names, in points across at [`HALF`] from the eye, before
/// perspective; a long name is made smaller to fit its face.
const LABEL_SIZE: f32 = 11.0;
/// A face is named while it faces the camera at least this much.
const LABEL_FACING: f32 = 0.12;
/// How long a click on the cube takes to swing the camera round, in seconds.
pub(super) const SWING_SECONDS: f32 = 0.45;
/// Where the house button is from the cube's middle, and its size.
const HOME_AT: egui::Vec2 = egui::vec2(-INSET + 16.0, -INSET + 16.0);
const HOME_RADIUS: f32 = 10.0;
/// How far the arrows are past the edge of a face seen straight on.
const ARROW_GAP: f32 = 11.0;
const ARROW_SIZE: f32 = 6.0;

/// One face: which way it faces, and its name.
const FACES: [(Vec3, &str); 6] = [
    (Vec3::Z, "FRONT"),
    (Vec3::NEG_Z, "BACK"),
    (Vec3::X, "RIGHT"),
    (Vec3::NEG_X, "LEFT"),
    (Vec3::Y, "TOP"),
    (Vec3::NEG_Y, "BOTTOM"),
];

/// Red, green and blue for X, Y and Z, as on the Move handles.
const AXES: [(Vec3, &str, egui::Color32); 3] = [
    (Vec3::X, "X", egui::Color32::from_rgb(235, 65, 65)),
    (Vec3::Y, "Y", egui::Color32::from_rgb(70, 205, 70)),
    (Vec3::Z, "Z", egui::Color32::from_rgb(55, 125, 245)),
];

/// Where the cube's light comes from, in the world: above, a little to the
/// front and right, so the top is brightest, as in Fusion.
const LIGHT: Vec3 = Vec3::new(0.35, 0.85, 0.4);

/// What part of the cube is under the mouse.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum CubeHit {
    /// A face, edge or corner, or an arrow to a face: the side to look
    /// from, not yet normalized (each part -1, 0 or 1).
    Side(Vec3),
    /// The house: back to the starting angle.
    Home,
}

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

    /// The face seen straight on, if one is.
    fn straight_on(&self) -> Option<Vec3> {
        FACES
            .iter()
            .map(|&(normal, _)| normal)
            .find(|&normal| self.facing(normal) > STRAIGHT_ON)
    }
}

/// The two directions across the face facing `normal`.
fn across(normal: Vec3) -> (Vec3, Vec3) {
    if normal.y.abs() > 0.5 {
        (Vec3::X, Vec3::Z)
    } else {
        (Vec3::Y, normal.cross(Vec3::Y))
    }
}

/// A face's four corners on screen, going round it.
fn corners(basis: &Basis, center: egui::Pos2, normal: Vec3) -> [egui::Pos2; 4] {
    piece(basis, center, normal, (-1.0, 1.0), (-1.0, 1.0))
}

/// The four corners on screen of the part of a face from `s.0` to `s.1`
/// across it one way and `t.0` to `t.1` the other (each -1 to 1), going
/// round it the same way as [`corners`].
fn piece(
    basis: &Basis,
    center: egui::Pos2,
    normal: Vec3,
    s: (f32, f32),
    t: (f32, f32),
) -> [egui::Pos2; 4] {
    let (a, b) = across(normal);
    [(s.1, t.1), (s.0, t.1), (s.0, t.0), (s.1, t.0)]
        .map(|(s, t)| basis.point(center, (normal + a * s + b * t) * HALF))
}

/// Where a face's grid lines are across it, from -1 to 1: the rounded
/// rim's steps at each end, and the [`BAND`] lines that split the face's
/// middle from its edge strips. Each rim step turns the same angle round
/// the edge, so the rounding is even.
fn ticks() -> Vec<f32> {
    let flat = 1.0 - ROUND;
    let mut ticks: Vec<f32> = (0..=ROUND_STEPS)
        .map(|step| {
            let turn = step as f32 / ROUND_STEPS as f32 * std::f32::consts::FRAC_PI_4;
            flat + ROUND * turn.tan()
        })
        .collect();
    if BAND < flat {
        ticks.insert(0, BAND);
    }
    let mut both: Vec<f32> = ticks.iter().rev().map(|&t| -t).collect();
    both.extend(ticks);
    both
}

/// The point of the rounded block over the cube's surface point `surface`
/// (each part -1 to 1, one of them ±1), and which way the block faces
/// there, one unit long. The block is a cube shrunk by [`ROUND`] with a
/// ball of that radius rolled over it.
fn rounded(surface: Vec3) -> (Vec3, Vec3) {
    let flat = 1.0 - ROUND;
    let inner = surface.clamp(Vec3::splat(-flat), Vec3::splat(flat));
    let normal = (surface - inner).normalize_or(Vec3::Y);
    (inner + normal * ROUND, normal)
}

/// One small four-sided piece of the block: the side a click on it looks
/// from, and its corners (from the cube's middle, in units of [`HALF`])
/// with the way the block faces at each, going round it.
struct Cell {
    side: Vec3,
    corners: [(Vec3, Vec3); 4],
}

impl Cell {
    /// Its corners on screen.
    fn on_screen(&self, basis: &Basis, center: egui::Pos2) -> [egui::Pos2; 4] {
        self.corners.map(|(at, _)| basis.point(center, at * HALF))
    }

    /// Whether its outside faces the eye.
    fn seen(&self, basis: &Basis) -> bool {
        let middle = self.corners.iter().map(|(at, _)| *at).sum::<Vec3>() / 4.0 * HALF;
        let facing = self.corners.iter().map(|(_, n)| *n).sum::<Vec3>();
        (-basis.forward * EYE - middle).dot(facing) > 0.0
    }
}

/// Every cell of the face facing `normal`, rim and all.
fn cells(normal: Vec3) -> Vec<Cell> {
    let (a, b) = across(normal);
    let ticks = ticks();
    // Which third of the face a span from `low` to `high` is in.
    let third = |low: f32, high: f32| {
        let middle = (low + high) / 2.0;
        if middle < -BAND {
            -1.0
        } else if middle > BAND {
            1.0
        } else {
            0.0
        }
    };
    let mut cells = Vec::new();
    for i in 0..ticks.len() - 1 {
        for j in 0..ticks.len() - 1 {
            let (s0, s1, t0, t1) = (ticks[i], ticks[i + 1], ticks[j], ticks[j + 1]);
            let corner = |s: f32, t: f32| rounded(normal + a * s + b * t);
            cells.push(Cell {
                side: normal + a * third(s0, s1) + b * third(t0, t1),
                // The same way round as `corners`.
                corners: [
                    corner(s1, t1),
                    corner(s0, t1),
                    corner(s0, t0),
                    corner(s1, t0),
                ],
            });
        }
    }
    cells
}

/// Every cell of the block that faces the eye.
fn seen_cells(basis: &Basis) -> Vec<Cell> {
    FACES
        .iter()
        .flat_map(|&(normal, _)| cells(normal))
        .filter(|cell| cell.seen(basis))
        .collect()
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
fn inside(corners: &[egui::Pos2], point: egui::Pos2) -> bool {
    let count = corners.len();
    let sides: Vec<f32> = (0..count)
        .map(|i| {
            let (a, b) = (corners[i], corners[(i + 1) % count]);
            (b - a).x * (point - a).y - (b - a).y * (point - a).x
        })
        .collect();
    sides.iter().all(|&side| side >= 0.0) || sides.iter().all(|&side| side <= 0.0)
}

/// The face, edge or corner under `point`, of the block the eye sees: the
/// side a click there looks from.
fn side_at(basis: &Basis, center: egui::Pos2, point: egui::Pos2) -> Option<Vec3> {
    seen_cells(basis)
        .into_iter()
        .find(|cell| inside(&cell.on_screen(basis, center), point))
        .map(|cell| cell.side)
}

/// The world axis nearest `direction`, as a face's normal.
fn nearest_face(direction: Vec3) -> Vec3 {
    let abs = direction.abs();
    if abs.x >= abs.y && abs.x >= abs.z {
        Vec3::X * direction.x.signum()
    } else if abs.y >= abs.z {
        Vec3::Y * direction.y.signum()
    } else {
        Vec3::Z * direction.z.signum()
    }
}

/// While a face is seen straight on, the four arrows beside it: where each
/// is on screen, the way it points there, and the face it turns to.
fn arrows(basis: &Basis, center: egui::Pos2) -> Vec<(egui::Pos2, egui::Vec2, Vec3)> {
    let Some(face) = basis.straight_on() else {
        return Vec::new();
    };
    let reach = (basis.point(center, (face + basis.right) * HALF) - center).x;
    [
        (egui::vec2(0.0, -1.0), basis.up),
        (egui::vec2(0.0, 1.0), -basis.up),
        (egui::vec2(1.0, 0.0), basis.right),
        (egui::vec2(-1.0, 0.0), -basis.right),
    ]
    .into_iter()
    .map(|(way, toward)| {
        (
            center + way * (reach + ARROW_GAP),
            way,
            nearest_face(toward),
        )
    })
    .collect()
}

/// What of the cube is under `point`: the house, an arrow, or a face, edge
/// or corner.
fn hit_at(basis: &Basis, center: egui::Pos2, point: egui::Pos2) -> Option<CubeHit> {
    if point.distance(center + HOME_AT) <= HOME_RADIUS + 2.0 {
        return Some(CubeHit::Home);
    }
    if let Some((_, _, face)) = arrows(basis, center)
        .into_iter()
        .find(|(at, _, _)| point.distance(*at) <= ARROW_SIZE + 4.0)
    {
        return Some(CubeHit::Side(face));
    }
    side_at(basis, center, point).map(CubeHit::Side)
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

/// What of the cube in viewport `rect` is under `point`, for a camera
/// looking along `forward`. The viewport asks before anything else, so a
/// click on the cube never selects what's behind it.
pub(super) fn hit_under(rect: egui::Rect, forward: Vec3, point: egui::Pos2) -> Option<CubeHit> {
    hit_at(&Basis::of(forward), middle(rect)?, point)
}

/// Draws the cube in the top-right of `rect` for a camera looking along
/// `forward`, with what's `hovered` lit.
pub(super) fn paint(ui: &egui::Ui, rect: egui::Rect, forward: Vec3, hovered: Option<CubeHit>) {
    let Some(center) = middle(rect) else {
        return;
    };
    let basis = Basis::of(forward);
    let palette = theme::palette(ui.ctx());
    let painter = ui.painter_at(rect);
    let arrows = arrows(&basis, center);
    // The axes run out from the middle, under the cube, so only the part
    // past its outline shows; those pointing away are dimmed. Seen straight
    // on, the arrows are where the tips would be, and the face says which
    // way it looks.
    let tips: Vec<(egui::Pos2, &str, egui::Color32)> = if arrows.is_empty() {
        AXES.iter()
            .filter_map(|&(direction, name, color)| {
                let at = tip_at(&basis, center, direction)?;
                let color = if direction.dot(basis.forward) > 0.0 {
                    color.gamma_multiply(0.5)
                } else {
                    color
                };
                Some((at, name, color))
            })
            .collect()
    } else {
        Vec::new()
    };
    for &(at, _, color) in &tips {
        painter.line_segment([center, at], egui::Stroke::new(1.5_f32, color));
    }
    let lit_side = match hovered {
        Some(CubeHit::Side(side)) => Some(side),
        _ => None,
    };
    // A light grey block, as in Fusion, in a dark theme or a light one:
    // toward the strong text colour in a dark theme, the panel's in a light.
    let dark = ui.visuals().dark_mode;
    let (shade, lit_fill, ink) = if dark {
        (
            theme::mix(palette.elevated, palette.text_strong, 0.38),
            theme::mix(palette.elevated, palette.text_strong, 0.78),
            palette.app_bg,
        )
    } else {
        (
            theme::mix(palette.elevated, palette.text_strong, 0.18),
            palette.elevated,
            palette.text_strong,
        )
    };
    let light = LIGHT.normalize();
    let color_at = |normal: Vec3| {
        // Lit from above, softly, plus a little toward the eye, so the
        // faces seen straight on are never dull.
        let lit = normal.dot(light).max(0.0) * 0.7 + basis.facing(normal).max(0.0) * 0.3;
        theme::mix(shade, lit_fill, 0.2 + 0.8 * lit)
    };
    let cells = seen_cells(&basis);
    // One mesh for the whole block: neighbouring cells share their corners'
    // places and colours exactly, so no seams show between them.
    let mut block = egui::Mesh::default();
    let mut glow = egui::Mesh::default();
    let mut outline_points = Vec::with_capacity(cells.len() * 4);
    for cell in &cells {
        let points = cell.on_screen(&basis, center);
        outline_points.extend(points);
        let first = block.vertices.len() as u32;
        for (point, (_, normal)) in points.iter().zip(cell.corners) {
            block.colored_vertex(*point, color_at(normal));
        }
        block.add_triangle(first, first + 1, first + 2);
        block.add_triangle(first, first + 2, first + 3);
        if lit_side == Some(cell.side) {
            let first = glow.vertices.len() as u32;
            for point in points {
                glow.colored_vertex(point, palette.accent.gamma_multiply(0.7));
            }
            glow.add_triangle(first, first + 1, first + 2);
            glow.add_triangle(first, first + 2, first + 3);
        }
    }
    painter.add(egui::Shape::mesh(block));
    painter.add(egui::Shape::mesh(glow));
    // Where each face's flat middle meets its rounded rim, faintly, so the
    // block reads as a box at every angle.
    let seam = egui::Stroke::new(
        1.0_f32,
        theme::mix(shade, palette.app_bg, 0.25).gamma_multiply(0.5),
    );
    let flat = 1.0 - ROUND;
    for &(normal, _) in &FACES {
        // Only faces clearly in view: one nearly edge-on is mostly hidden
        // behind the rim in front of it.
        if basis.facing(normal) <= 0.25 {
            continue;
        }
        let (a, b) = across(normal);
        let ring =
            [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)].map(|(s, t): (f32, f32)| {
                basis.point(center, (normal + (a * s + b * t) * flat) * HALF)
            });
        // Side by side, not joined: a joined line's corners spike out
        // where they're sharp.
        for i in 0..4 {
            painter.line_segment([ring[i], ring[(i + 1) % 4]], seam);
        }
    }
    // The outline, anti-aliased, over the mesh's hard edge.
    let hull = convex_hull(outline_points);
    if hull.len() > 2 {
        painter.add(egui::Shape::closed_line(
            hull,
            egui::Stroke::new(1.0_f32, theme::mix(shade, palette.app_bg, 0.5)),
        ));
    }
    for &(normal, name) in &FACES {
        let facing = basis.facing(normal);
        if facing < LABEL_FACING || !basis.sees(normal) {
            continue;
        }
        let color = if lit_side == Some(normal) {
            egui::Color32::WHITE
        } else {
            ink
        };
        // Fades in as the face turns toward the camera.
        let fade = ((facing - LABEL_FACING) / 0.25).clamp(0.0, 1.0);
        paint_label(
            ui,
            &painter,
            &basis,
            center,
            normal,
            name,
            color.gamma_multiply(0.35 + 0.6 * fade),
        );
    }
    for (at, name, color) in tips {
        painter.circle_filled(at, 7.0, color);
        painter.text(
            at,
            egui::Align2::CENTER_CENTER,
            name,
            egui::FontId::new(9.0, theme::medium_family()),
            egui::Color32::WHITE,
        );
    }
    for (at, way, face) in arrows {
        let lit = lit_side == Some(face);
        let color = if lit {
            palette.accent
        } else {
            palette.text_secondary
        };
        let side = egui::vec2(-way.y, way.x) * ARROW_SIZE;
        let tip = at + way * ARROW_SIZE * 0.8;
        let back = at - way * ARROW_SIZE * 0.6;
        painter.add(egui::Shape::convex_polygon(
            vec![tip, back + side, back - side],
            color,
            egui::Stroke::NONE,
        ));
    }
    let home = center + HOME_AT;
    let lit = hovered == Some(CubeHit::Home);
    painter.circle(
        home,
        HOME_RADIUS,
        if lit {
            palette.accent_soft
        } else {
            palette.surface.gamma_multiply(0.85)
        },
        egui::Stroke::new(1.0_f32, palette.border),
    );
    painter.text(
        home,
        egui::Align2::CENTER_CENTER,
        icons::HOUSE,
        egui::FontId::proportional(12.0),
        if lit {
            palette.accent_text
        } else {
            palette.text_secondary
        },
    );
}

/// Writes `name` on the face facing `normal`, as if printed on it: laid
/// out flat, then each corner of its glyphs put on the face in 3D and seen
/// in perspective like the block. Made smaller to fit a long name inside
/// the face's flat middle.
fn paint_label(
    ui: &egui::Ui,
    painter: &egui::Painter,
    basis: &Basis,
    center: egui::Pos2,
    normal: Vec3,
    name: &str,
    color: egui::Color32,
) {
    let ctx = ui.ctx();
    let galley = painter.layout_no_wrap(
        name.to_owned(),
        egui::FontId::new(LABEL_SIZE, theme::medium_family()),
        color,
    );
    let size = galley.size();
    let room = 2.0 * (1.0 - ROUND) * HALF * 0.92;
    let scale = (room / size.x.max(1.0)).min(1.0);
    let mut mesh = egui::Mesh::default();
    let font_size = ctx.fonts(|fonts| fonts.font_image_size());
    let mut tessellator = egui::epaint::Tessellator::new(
        ctx.pixels_per_point(),
        egui::epaint::TessellationOptions::default(),
        font_size,
        Vec::new(),
    );
    tessellator.tessellate_text(
        &egui::epaint::TextShape::new(egui::Pos2::ZERO, galley, color),
        &mut mesh,
    );
    // From the name's middle, along the face: right and up.
    let right = label_right(normal);
    let up = normal.cross(right);
    let middle = size / 2.0;
    for vertex in &mut mesh.vertices {
        let flat = (vertex.pos.to_vec2() - middle) * scale;
        let on_face = normal * HALF + right * flat.x - up * flat.y;
        vertex.pos = basis.point(center, on_face);
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// The smallest convex shape around `points`, going round it.
fn convex_hull(mut points: Vec<egui::Pos2>) -> Vec<egui::Pos2> {
    points.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    points.dedup();
    if points.len() < 3 {
        return points;
    }
    let turn =
        |o: egui::Pos2, a: egui::Pos2, b: egui::Pos2| (a - o).x * (b - o).y - (a - o).y * (b - o).x;
    let mut hull: Vec<egui::Pos2> = Vec::with_capacity(points.len() * 2);
    for pass in [false, true] {
        let start = hull.len();
        let ordered: Box<dyn Iterator<Item = &egui::Pos2>> = if pass {
            Box::new(points.iter().rev())
        } else {
            Box::new(points.iter())
        };
        for &point in ordered {
            while hull.len() >= start + 2
                && turn(hull[hull.len() - 2], hull[hull.len() - 1], point) <= 0.0
            {
                hull.pop();
            }
            hull.push(point);
        }
        hull.pop();
    }
    hull
}

/// Which way a face's name runs, seen from outside it: along X on Top and
/// Bottom, and across each side with the world's Y up.
fn label_right(normal: Vec3) -> Vec3 {
    if normal.y.abs() > 0.5 {
        Vec3::X
    } else {
        Vec3::Y.cross(normal)
    }
}

/// A click on the cube swinging the camera round its focus point, over
/// [`SWING_SECONDS`] (none when motion is reduced), easing in and out as
/// Fusion's does.
#[derive(Clone, Copy, Debug)]
pub(super) struct Swing {
    from: FlyCamera,
    to: FlyCamera,
    /// When it started, on egui's clock, in seconds.
    started: f64,
    seconds: f32,
}

impl Swing {
    pub(super) fn new(from: FlyCamera, to: FlyCamera, now: f64, seconds: f32) -> Swing {
        Swing {
            from,
            to,
            started: now,
            seconds,
        }
    }

    /// The camera at `now`, and whether the swing is over.
    pub(super) fn at(&self, now: f64) -> (FlyCamera, bool) {
        let t = if self.seconds <= 0.0 {
            1.0
        } else {
            ((now - self.started) as f32 / self.seconds).clamp(0.0, 1.0)
        };
        if t >= 1.0 {
            return (self.to, true);
        }
        // Smooth at both ends.
        let eased = t * t * (3.0 - 2.0 * t);
        let angles = |camera: &FlyCamera| {
            let forward = camera.forward();
            (
                f32::atan2(-forward.x, -forward.z),
                forward.y.clamp(-1.0, 1.0).asin(),
            )
        };
        let ((yaw0, pitch0), (yaw1, pitch1)) = (angles(&self.from), angles(&self.to));
        // The short way round.
        let mut turn = (yaw1 - yaw0).rem_euclid(std::f32::consts::TAU);
        if turn > std::f32::consts::PI {
            turn -= std::f32::consts::TAU;
        }
        let yaw = yaw0 + turn * eased;
        let pitch = pitch0 + (pitch1 - pitch0) * eased;
        let forward = Vec3::new(
            -yaw.sin() * pitch.cos(),
            pitch.sin(),
            -yaw.cos() * pitch.cos(),
        );
        let focus = self.from.focus().lerp(self.to.focus(), eased);
        let distance = {
            let (a, b) = (
                self.from.position.distance(self.from.focus()),
                self.to.position.distance(self.to.focus()),
            );
            a + (b - a) * eased
        };
        let mut camera = FlyCamera::looking_at(focus - forward * distance, focus);
        camera.set_speed(self.to.speed());
        (camera, false)
    }
}

/// The side the house button looks from: the starting camera's.
pub(super) fn home_side() -> Vec3 {
    super::START_CAMERA_POSITION.normalize()
}

/// `camera` swung round to look at its focus point from `side` (a face,
/// edge or corner of the view cube, any length), as far from it as before,
/// at the same flying speed.
pub fn looking_from(camera: &FlyCamera, side: Vec3) -> FlyCamera {
    let focus = camera.focus();
    let distance = camera.position.distance(focus);
    let side = side.normalize_or(Vec3::Z);
    // Straight down or up has no way to face: lean a hair toward -Z, so
    // the top view keeps Front at the bottom.
    let side = if side.y.abs() > 0.99 {
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
        assert_eq!(side_at(&basis, CENTER, CENTER), Some(Vec3::Z));
        let right = tip_at(&basis, CENTER, Vec3::X).unwrap();
        assert!(right.x > CENTER.x + HALF);
        // Up on screen is +Y (egui's Y runs down).
        assert!(tip_at(&basis, CENTER, Vec3::Y).unwrap().y < CENTER.y - HALF);
        // Z points at the camera: Front says so, with no tip over it.
        assert_eq!(tip_at(&basis, CENTER, Vec3::Z), None);
        // From behind, the middle is Back.
        assert_eq!(
            side_at(&Basis::of(Vec3::Z), CENTER, CENTER),
            Some(Vec3::NEG_Z)
        );
    }

    #[test]
    fn the_edges_and_corners_of_a_face_look_from_between_its_faces() {
        let basis = Basis::of(Vec3::NEG_Z);
        let front = |s: f32, t: f32| basis.point(CENTER, Vec3::new(s, t, 1.0) * HALF);
        // The top-right corner of Front is between Front, Right and Top.
        assert_eq!(
            side_at(&basis, CENTER, front(0.7, 0.7)),
            Some(Vec3::new(1.0, 1.0, 1.0))
        );
        // Its top edge is between Front and Top.
        assert_eq!(
            side_at(&basis, CENTER, front(0.0, 0.7)),
            Some(Vec3::new(0.0, 1.0, 1.0))
        );
        // Every face of the cube has its middle, four edges and four corners.
        for (normal, _) in FACES {
            let mut sides: Vec<[i32; 3]> = cells(normal)
                .into_iter()
                .map(|cell| {
                    assert!(
                        cell.side.dot(normal) > 0.99,
                        "{} is off {normal}",
                        cell.side
                    );
                    cell.side.round().as_ivec3().to_array()
                })
                .collect();
            sides.sort_unstable();
            sides.dedup();
            assert_eq!(sides.len(), 9);
        }
    }

    #[test]
    fn the_block_has_rounded_edges_and_corners() {
        // A face's middle is where the cube's is; its corner is pulled in.
        let (middle, facing) = rounded(Vec3::Z);
        assert!(middle.abs_diff_eq(Vec3::Z, 1e-6) && facing == Vec3::Z);
        let (corner, facing) = rounded(Vec3::ONE);
        assert!(corner.length() < Vec3::ONE.length() - 0.15, "{corner}");
        assert!(facing.abs_diff_eq(Vec3::ONE.normalize(), 1e-5));
        // Across an edge the rounding meets itself: both faces end at the
        // same point, facing halfway between them.
        let (from_front, n1) = rounded(Vec3::new(1.0, 0.0, 1.0));
        let (from_right, n2) = rounded(Vec3::new(1.0, 0.0, 1.0));
        assert!(from_front.abs_diff_eq(from_right, 1e-6) && n1.abs_diff_eq(n2, 1e-6));
        // Straight on, the outline is round at the corners: no corner of
        // the cube itself is reached.
        let basis = Basis::of(Vec3::NEG_Z);
        let points: Vec<egui::Pos2> = seen_cells(&basis)
            .iter()
            .flat_map(|cell| cell.on_screen(&basis, CENTER))
            .collect();
        let hull = convex_hull(points);
        let square = basis.point(CENTER, Vec3::new(1.0, 1.0, 1.0) * HALF);
        assert!(hull.iter().all(|p| p.distance(square) > 2.0));
        assert!(hull.len() > 8, "rounded, not four corners: {}", hull.len());
    }

    #[test]
    fn a_swing_eases_round_the_short_way_and_ends_where_it_was_asked_to() {
        let from = FlyCamera::looking_at(Vec3::new(0.0, 0.0, 20.0), Vec3::ZERO);
        let to = looking_from(&from, Vec3::new(-1.0, 0.0, 0.0));
        let swing = Swing::new(from, to, 10.0, SWING_SECONDS);
        let (start, done) = swing.at(10.0);
        assert!(!done && start.forward().abs_diff_eq(from.forward(), 1e-4));
        // Halfway, it's looking from between Front and Left, still at the
        // same distance from the same focus point.
        let (half, _) = swing.at(10.0 + f64::from(SWING_SECONDS) / 2.0);
        let from_side = (half.position - half.focus()).normalize();
        assert!(
            from_side.abs_diff_eq(Vec3::new(-1.0, 0.0, 1.0).normalize(), 1e-3),
            "{from_side}"
        );
        assert!((half.position.distance(half.focus()) - 20.0).abs() < 1e-2);
        let (end, done) = swing.at(11.0);
        assert!(done && end.forward().abs_diff_eq(to.forward(), 1e-5));
        // With motion reduced, it's there at once.
        assert!(Swing::new(from, to, 10.0, 0.0).at(10.0).1);
    }

    #[test]
    fn straight_on_the_arrows_turn_to_the_faces_beside_it() {
        let basis = Basis::of(Vec3::NEG_Z);
        let arrows = arrows(&basis, CENTER);
        assert_eq!(arrows.len(), 4);
        let up = arrows.iter().find(|(_, way, _)| way.y < 0.0).unwrap();
        assert_eq!(up.2, Vec3::Y);
        let right = arrows.iter().find(|(_, way, _)| way.x > 0.0).unwrap();
        assert_eq!(right.2, Vec3::X);
        // A click on one picks its face, not the cube under it.
        assert_eq!(
            hit_at(&basis, CENTER, right.0),
            Some(CubeHit::Side(Vec3::X))
        );
        // At an angle there are none.
        let angled = Basis::of(Vec3::new(-1.0, -1.0, -1.0).normalize());
        assert!(super::arrows(&angled, CENTER).is_empty());
        // The house is above the cube, clear of it.
        assert_eq!(
            hit_at(&basis, CENTER, CENTER + HOME_AT),
            Some(CubeHit::Home)
        );
    }

    #[test]
    fn the_cube_draws_at_any_angle_with_what_is_hovered_lit() {
        let views = [
            (
                Vec3::new(-0.6, -0.45, -0.66).normalize(),
                Some(CubeHit::Side(Vec3::new(1.0, 1.0, 1.0))),
            ),
            (Vec3::NEG_Z, Some(CubeHit::Side(Vec3::X))),
            (
                Vec3::new(0.5, -0.3, -0.8).normalize(),
                Some(CubeHit::Side(Vec3::new(0.0, 1.0, 1.0))),
            ),
            (
                Vec3::new(-0.2, -0.95, -0.2).normalize(),
                Some(CubeHit::Home),
            ),
        ];
        let mut draw = |ctx: &egui::Context| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(theme::palette(ctx).inset))
                .show(ctx, |ui| {
                    for (index, (forward, hovered)) in views.iter().enumerate() {
                        let left = ui.max_rect().left() + index as f32 * 2.0 * INSET;
                        let rect = egui::Rect::from_min_size(
                            egui::pos2(left, ui.max_rect().top()),
                            egui::Vec2::splat(2.0 * INSET),
                        );
                        paint(ui, rect, *forward, *hovered);
                    }
                });
        };
        match std::env::var_os("AXCENITY_SNAPSHOTS").map(std::path::PathBuf::from) {
            Some(folder) => crate::snapshot::snapshot(
                &folder.join("view-cube.png"),
                egui::vec2(8.0 * INSET, 2.0 * INSET),
                vec![Vec::new(); 2],
                &mut draw,
            ),
            None => {
                let ctx = theme::test_context();
                let _ = ctx.run(Default::default(), &mut draw);
            }
        }
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
    fn a_face_edge_or_corner_swings_the_camera_round_its_focus_point() {
        let camera = FlyCamera::looking_at(Vec3::new(10.0, 8.0, 16.0), Vec3::new(1.0, 2.0, 3.0));
        let focus = camera.focus();
        let distance = camera.position.distance(focus);
        let sides = FACES
            .iter()
            .map(|&(side, _)| side)
            .chain([Vec3::new(1.0, 1.0, 1.0), Vec3::new(0.0, -1.0, 1.0)]);
        for side in sides {
            let turned = looking_from(&camera, side);
            assert!(turned.focus().distance(focus) < 1e-2, "{side}");
            assert!((turned.position.distance(focus) - distance).abs() < 1e-2);
            // It looks back along the side's direction.
            assert!(turned.forward().dot(-side.normalize()) > 0.99, "{side}");
            assert_eq!(turned.speed(), camera.speed());
        }
    }
}
