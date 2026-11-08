//! The measuring grid over the baseplate: thin lines every move step, a
//! heavier line every ten, and printed distances along two rows through the
//! middle of the view. Editing only; a built game never has it.
//!
//! `render` draws the lines (`DrawList::grid`), so parts hide them; egui
//! prints the numbers. Numbers behind a part are left out, by a ray test
//! worked out a few times a second rather than every frame.

use axcenity_core::{Instance, InstanceId, Value, World};
use axcenity_render::glam::Vec3;
use axcenity_render::{GridPlane, GridTint};
use axcenity_scene::Ray;
use eframe::egui;

use crate::screen::Screen;
use crate::theme;

/// Lines per heavier line.
const MAJOR_EVERY: f32 = 10.0;
/// Without a baseplate: how far the ground grid reaches from the camera.
const GROUND_REACH: f32 = 1500.0;
/// How many numbers go out each way along a row.
const LABELS_EACH_WAY: i32 = 10;
/// The fewest points between two numbers on a row.
const LABEL_GAP: f32 = 52.0;
/// How often the numbers' visibility is worked out again, in seconds.
const OCCLUSION_SECONDS: f64 = 0.12;
/// How much nearer than a number something must be to hide it, in studs.
const HIDDEN_BY: f32 = 0.25;
/// How far above the surface the lines sit, in studs.
const LIFT: f32 = 0.002;
/// A surface brighter than this (sRGB, weighted by how bright each channel
/// looks) gets darker lines, and one dimmer gets lighter lines.
const BRIGHT: f32 = 0.35;
/// How far the lines go from the surface's color to white or black.
const LINE_CONTRAST: f32 = 0.55;
/// What's under the grid when the baseplate has no color to go by.
const LIT_GROUND: [f32; 3] = [0.64, 0.64, 0.64];
/// What's under a ground grid with no baseplate: the sky seen past it.
const DARK_GROUND: [f32; 3] = [0.2, 0.2, 0.22];
/// How far past a part's edge (studs) the lines take on its color.
const TINT_REACH: f32 = 12.0;
/// How much of a part's color the lines right by it take.
const TINT_AMOUNT: f32 = 0.35;
/// How much more of it they take when the part glows at full strength.
const TINT_FROM_GLOW: f32 = 0.35;
/// How far from the camera (studs) a glowing part still tints the grid.
const GLOW_REACH: f32 = 80.0;

/// Where the grid is and how it's divided.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spec {
    pub height: f32,
    /// The baseplate's smallest and largest X and Z, or `None` for ground
    /// that goes on around the camera.
    pub area: Option<(Vec3, Vec3)>,
    /// Studs between lines.
    pub spacing: f32,
    /// What the surface under it looks like, in sRGB, glow included.
    pub surface: [f32; 3],
}

impl Spec {
    /// Studs between heavier lines.
    pub fn major(&self) -> f32 {
        self.spacing * MAJOR_EVERY
    }
}

/// A part at least this many studs wide and deep counts as ground.
const WIDE_ENOUGH: f32 = 32.0;

/// `id` if it's a flat, unturned part, with how much ground it covers.
fn flat_area(world: &World, id: InstanceId) -> Option<f32> {
    let part = world.get(id)?;
    let size = part.property("Size")?.as_vector3()?;
    let orientation = part.property("Orientation")?.as_vector3()?;
    let level = [orientation.x, orientation.y, orientation.z]
        .iter()
        .all(|angle| angle.abs() < 0.01);
    level.then_some(size.x * size.z)
}

/// What the grid lies on: the Workspace's Part called Baseplate, or else its
/// widest flat part, if either is flat and unturned.
fn baseplate(world: &World) -> Option<InstanceId> {
    let workspace = world.workspace();
    if let Some(id) = world
        .find_first_child(workspace, "Baseplate")
        .filter(|&id| flat_area(world, id).is_some())
    {
        return Some(id);
    }
    world
        .get(workspace)?
        .children()
        .iter()
        .filter_map(|&id| {
            let size = world.get(id)?.property("Size")?.as_vector3()?;
            let wide = size.x >= WIDE_ENOUGH && size.z >= WIDE_ENOUGH;
            let flat = size.y <= size.x.min(size.z);
            Some((id, flat_area(world, id).filter(|_| wide && flat)?))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
}

/// Where the middle of the view meets the grid, or the spot under the
/// camera when it's looking away from it.
pub fn focus(spec: &Spec, screen: &Screen, rect: egui::Rect, camera: Vec3) -> Vec3 {
    let ray = screen.ray(rect.center());
    let along = (spec.height - ray.origin.y) / ray.direction.y;
    let hit = (along.is_finite() && along > 0.0).then(|| ray.at(along));
    hit.unwrap_or(Vec3::new(camera.x, spec.height, camera.z))
}

/// `value` studs, with the word in the right number.
fn studs_label(value: f32) -> String {
    if value == 1.0 {
        "1 stud".to_owned()
    } else {
        format!("{} studs", number(value))
    }
}

/// The grid to draw for `world`, with lines every `step` studs.
pub fn spec(world: &World, step: f32) -> Spec {
    // A step under a tenth of a stud would only ever be a blur.
    let spacing = if step >= 0.1 { step } else { 1.0 };
    let plate = baseplate(world).and_then(|id| {
        let part = world.get(id)?;
        let at = part.property("Position")?.as_vector3()?;
        let size = part.property("Size")?.as_vector3()?;
        Some((at, size, shine(part).map(|(seen, _)| seen)))
    });
    match plate {
        Some((at, size, seen)) => {
            let top = at.y + size.y / 2.0;
            let half = Vec3::new(size.x / 2.0, 0.0, size.z / 2.0);
            let middle = Vec3::new(at.x, top, at.z);
            Spec {
                height: top,
                area: Some((middle - half, middle + half)),
                spacing,
                surface: seen.unwrap_or(LIT_GROUND),
            }
        }
        None => Spec {
            height: 0.0,
            area: None,
            spacing,
            surface: DARK_GROUND,
        },
    }
}

/// What a part shows, in sRGB, and how much it glows (0 to 1): its Color,
/// going over to the color of its glow as that gets stronger.
fn shine(part: &Instance) -> Option<([f32; 3], f32)> {
    let color = part.property("Color").and_then(Value::as_color3)?;
    let color = [color.r, color.g, color.b];
    let emission = part
        .property("Emission")
        .and_then(Value::as_number)
        .unwrap_or(0.0)
        .clamp(0.0, 1.0) as f32;
    let emitted = part
        .property("EmissiveColor")
        .and_then(Value::as_color3)
        .map_or(color, |c| [c.r, c.g, c.b]);
    if emission > 0.0 {
        let seen = std::array::from_fn(|i| color[i] + (emitted[i] - color[i]) * emission);
        return Some((seen, emission));
    }
    let neon = part
        .property("Material")
        .and_then(Value::as_material)
        .is_some_and(|material| material.preset().glows);
    Some((color, if neon { 1.0 } else { 0.0 }))
}

/// The lines' color for a surface that looks like `surface`, in linear
/// light: the surface's own color moved partway to white or to black, so
/// they're soft, and keep its hue.
fn line_color(surface: [f32; 3]) -> [f32; 3] {
    let [r, g, b] = surface;
    let toward = if 0.3 * r + 0.6 * g + 0.1 * b > BRIGHT {
        0.0
    } else {
        1.0
    };
    surface.map(|channel| to_linear(channel + (toward - channel) * LINE_CONTRAST))
}

fn to_linear(srgb: f32) -> f32 {
    srgb.clamp(0.0, 1.0).powf(2.2)
}

/// The glowing part nearest the camera, worked out a few times a second.
#[derive(Default)]
pub struct NearbyGlow {
    found: Option<InstanceId>,
    worked_out_at: Option<f64>,
}

impl NearbyGlow {
    /// The glowing part to tint the grid with, if one's in reach.
    pub fn update(&mut self, world: &World, camera: Vec3, now: f64) -> Option<InstanceId> {
        if self
            .worked_out_at
            .is_none_or(|at| now - at >= OCCLUSION_SECONDS)
        {
            self.found = nearest_glowing(world, camera);
            self.worked_out_at = Some(now);
        }
        self.found.filter(|&id| world.get(id).is_some())
    }
}

fn nearest_glowing(world: &World, camera: Vec3) -> Option<InstanceId> {
    world
        .descendants(world.workspace())
        .into_iter()
        .filter_map(|id| {
            let part = world.get(id)?;
            if shine(part)?.1 <= 0.0 {
                return None;
            }
            let at = part.property("Position")?.as_vector3()?;
            let distance = Vec3::new(at.x, at.y, at.z).distance(camera);
            (distance <= GLOW_REACH).then_some((id, distance))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
}

/// How the lines near `selected`, or else near the part `glowing`, take on
/// its color. A stronger glow tints more.
pub fn tint(
    world: &World,
    selected: Option<InstanceId>,
    glowing: Option<InstanceId>,
) -> Option<GridTint> {
    let lit = |id: InstanceId| {
        let part = world.get(id)?;
        let (seen, glow) = shine(part)?;
        let at = part.property("Position")?.as_vector3()?;
        let size = part.property("Size")?.as_vector3()?;
        Some(GridTint {
            at: [at.x, at.z],
            radius: size.x.max(size.z) / 2.0 + TINT_REACH,
            amount: TINT_AMOUNT + TINT_FROM_GLOW * glow,
            color: seen.map(to_linear),
        })
    };
    selected.and_then(lit).or_else(|| glowing.and_then(lit))
}

/// What `render` needs to draw the lines, `strength` times as solid as
/// usual, tinted by `tint`.
pub fn plane(spec: &Spec, camera: Vec3, strength: f32, tint: Option<GridTint>) -> GridPlane {
    let (min, max, fade_distance) = match spec.area {
        Some((min, max)) => ([min.x, min.z], [max.x, max.z], 0.0),
        None => (
            [camera.x - GROUND_REACH, camera.z - GROUND_REACH],
            [camera.x + GROUND_REACH, camera.z + GROUND_REACH],
            GROUND_REACH,
        ),
    };
    GridPlane {
        height: spec.height + LIFT,
        min,
        max,
        spacing: spec.spacing,
        major_every: MAJOR_EVERY,
        fade_distance,
        color: line_color(spec.surface),
        strength,
        tint,
    }
}

/// One printed distance, lying at `at`.
#[derive(Clone, Debug, PartialEq)]
pub struct Label {
    pub at: Vec3,
    pub text: String,
    pub along_x: bool,
}

/// How far apart numbers go: the heavy line distance, times 1, 2 or 5 and
/// powers of ten, so that neighbours near `middle` are at least
/// [`LABEL_GAP`] points apart on screen.
fn label_step(spec: &Spec, screen: &Screen, middle: Vec3) -> f32 {
    let studs_per_point = screen.studs_per_point(middle).max(0.0001);
    let wanted = LABEL_GAP * studs_per_point;
    let mut step = spec.major();
    let mut multipliers = [2.0, 2.5, 2.0].into_iter().cycle();
    while step < wanted {
        step *= multipliers.next().expect("cycles");
    }
    step
}

fn number(value: f32) -> String {
    let rounded = (value * 100.0).round() / 100.0;
    if rounded == 0.0 {
        "0".to_owned()
    } else {
        format!("{rounded}")
    }
}

/// The numbers along the X row and the Z row, which cross at the multiple
/// of `step` nearest `focus` and stay on the grid.
pub fn labels(spec: &Spec, focus: Vec3, step: f32) -> Vec<Label> {
    let clamp = |value: f32, low: Option<f32>, high: Option<f32>| {
        let value = low.map_or(value, |low| value.max(low));
        high.map_or(value, |high| value.min(high))
    };
    let (min, max) = spec.area.unzip();
    let on_step = |value: f32, low, high| {
        let snapped = (value / step).round() * step;
        let snapped = clamp(snapped, low, high);
        // Keep the row on a line when the edge isn't on one.
        (snapped / step).trunc() * step
    };
    let row_x = on_step(focus.x, min.map(|m| m.x), max.map(|m| m.x));
    let row_z = on_step(focus.z, min.map(|m| m.z), max.map(|m| m.z));
    let inside = |x: f32, z: f32| {
        spec.area.is_none_or(|(min, max)| {
            let room = 0.001;
            x >= min.x - room && x <= max.x + room && z >= min.z - room && z <= max.z + room
        })
    };
    let mut out = Vec::new();
    for i in -LABELS_EACH_WAY..=LABELS_EACH_WAY {
        let offset = i as f32 * step;
        let (x, z) = (row_x + offset, row_z);
        if inside(x, z) {
            out.push(Label {
                at: Vec3::new(x, spec.height, z),
                text: number(x),
                along_x: true,
            });
        }
        let (x, z) = (row_x, row_z + offset);
        // The crossing is shown once, on the X row.
        if i != 0 && inside(x, z) {
            out.push(Label {
                at: Vec3::new(x, spec.height, z),
                text: number(z),
                along_x: false,
            });
        }
    }
    out
}

/// True if a part stands between the camera and `at`.
fn hidden(world: &World, camera: Vec3, at: Vec3) -> bool {
    let offset = at - camera;
    let distance = offset.length();
    if distance <= HIDDEN_BY {
        return false;
    }
    let ray = Ray {
        origin: camera,
        direction: offset / distance,
    };
    axcenity_scene::pick_hit(world, ray, None)
        .is_some_and(|hit| hit.distance < distance - HIDDEN_BY)
}

/// The numbers worth printing, kept between frames.
#[derive(Default)]
pub struct Labels {
    shown: Vec<Label>,
    worked_out_at: Option<f64>,
    made_for: Option<(Spec, f32, Vec3)>,
}

impl Labels {
    /// Brings the numbers up to date if the grid or the row moved, or
    /// it's been long enough for something to have moved in front of them.
    pub fn update(
        &mut self,
        world: &World,
        spec: &Spec,
        screen: &Screen,
        camera: Vec3,
        focus: Vec3,
        now: f64,
    ) {
        let step = label_step(spec, screen, focus);
        let by_distance = |label: &Label| label.at.distance_squared(focus);
        // The rows change only when `focus` crosses a step.
        let row = (focus / step).round() * step;
        let made_for = Some((*spec, step, row));
        let due = self
            .worked_out_at
            .is_none_or(|at| now - at >= OCCLUSION_SECONDS);
        if made_for == self.made_for && !due {
            return;
        }
        self.shown = labels(spec, focus, step)
            .into_iter()
            .filter(|label| !hidden(world, camera, label.at))
            .collect();
        // Nearest the middle first: those win when numbers would overlap.
        self.shown
            .sort_by(|a, b| by_distance(a).total_cmp(&by_distance(b)));
        self.made_for = made_for;
        self.worked_out_at = Some(now);
    }

    pub fn paint(&self, painter: &egui::Painter, screen: &Screen, rect: egui::Rect, spec: &Spec) {
        let palette = theme::palette(painter.ctx());
        let font = egui::FontId::new(theme::type_scale::SMALL, egui::FontFamily::Monospace);
        let mut taken: Vec<egui::Rect> = Vec::new();
        for label in &self.shown {
            let Some(at) = screen.point(label.at).filter(|at| rect.contains(*at)) else {
                continue;
            };
            let color = if label.along_x {
                egui::Color32::from_rgb(255, 150, 140)
            } else {
                egui::Color32::from_rgb(150, 190, 255)
            };
            let galley = painter.layout_no_wrap(label.text.clone(), font.clone(), color);
            let text_rect = egui::Align2::CENTER_CENTER
                .anchor_size(at + egui::vec2(0.0, -9.0), galley.size())
                .expand2(egui::vec2(3.0, 1.0));
            // Far along a row, numbers bunch up: only the nearest of them show.
            if taken.iter().any(|other| other.intersects(text_rect)) {
                continue;
            }
            taken.push(text_rect);
            painter.rect_filled(text_rect, 0.0, egui::Color32::from_black_alpha(165));
            painter.galley(text_rect.min + egui::vec2(3.0, 1.0), galley, color);
            painter.circle_filled(at, 1.5, color);
        }
        // What one square means, in the corner.
        let text = format!(
            "Grid {}, heavy line every {}",
            studs_label(spec.spacing),
            studs_label(spec.major())
        );
        let galley = painter.layout_no_wrap(text, font, palette.text);
        let min = rect.left_bottom() + egui::vec2(10.0, -10.0 - galley.size().y - 4.0);
        let back = egui::Rect::from_min_size(min, galley.size() + egui::vec2(10.0, 4.0));
        painter.rect_filled(back, 0.0, palette.elevated.gamma_multiply(0.92));
        painter.galley(back.min + egui::vec2(5.0, 2.0), galley, palette.text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_core::{ClassName, Color3, Vector3};

    fn part(world: &mut World) -> InstanceId {
        let id = world.create(ClassName::Part).unwrap();
        let workspace = world.workspace();
        world.set_parent(id, Some(workspace)).unwrap();
        id
    }

    fn baseplate_at(world: &mut World, y: f32, thick: f32) -> InstanceId {
        let id = part(world);
        world.set_name(id, "Baseplate").unwrap();
        world
            .set_property(id, "Position", Value::Vector3(Vector3::new(4.0, y, -6.0)))
            .unwrap();
        world
            .set_property(id, "Size", Value::Vector3(Vector3::new(100.0, thick, 60.0)))
            .unwrap();
        id
    }

    #[test]
    fn the_grid_lies_on_top_of_the_baseplate_and_stops_at_its_edges() {
        let mut world = World::new();
        baseplate_at(&mut world, -8.0, 16.0);
        let spec = spec(&world, 1.0);
        assert_eq!(spec.height, 0.0);
        let (min, max) = spec.area.unwrap();
        assert_eq!((min.x, min.z, max.x, max.z), (-46.0, -36.0, 54.0, 24.0));
        let drawn = plane(&spec, Vec3::ZERO, 1.0, None);
        assert_eq!(drawn.min, [-46.0, -36.0]);
        assert_eq!(drawn.fade_distance, 0.0);
    }

    #[test]
    fn a_turned_baseplate_or_none_gives_a_ground_grid_at_zero_around_the_camera() {
        let mut world = World::new();
        assert_eq!(spec(&world, 1.0).area, None);
        let id = baseplate_at(&mut world, 3.0, 1.0);
        world
            .set_property(
                id,
                "Orientation",
                Value::Vector3(Vector3::new(0.0, 30.0, 0.0)),
            )
            .unwrap();
        let found = spec(&world, 1.0);
        assert_eq!((found.area, found.height), (None, 0.0));
        let drawn = plane(&found, Vec3::new(500.0, 20.0, 0.0), 1.0, None);
        assert_eq!(drawn.min[0], 500.0 - GROUND_REACH);
        assert!(drawn.fade_distance > 0.0);
    }

    #[test]
    fn with_no_baseplate_the_grid_lies_on_the_widest_flat_part() {
        let mut world = World::new();
        let ground = part(&mut world);
        world.set_name(ground, "Ground").unwrap();
        world
            .set_property(
                ground,
                "Position",
                Value::Vector3(Vector3::new(0.0, 4.0, 0.0)),
            )
            .unwrap();
        world
            .set_property(
                ground,
                "Size",
                Value::Vector3(Vector3::new(64.0, 2.0, 64.0)),
            )
            .unwrap();
        let small = part(&mut world);
        world
            .set_property(small, "Size", Value::Vector3(Vector3::new(4.0, 1.0, 4.0)))
            .unwrap();
        let found = spec(&world, 1.0);
        assert_eq!(found.height, 5.0);
        assert!(found.area.is_some());
    }

    #[test]
    fn lines_follow_the_move_step_and_heavy_lines_come_every_ten() {
        let world = World::new();
        let spec = spec(&world, 0.5);
        assert_eq!((spec.spacing, spec.major()), (0.5, 5.0));
        // A step too fine to draw falls back to a stud.
        assert_eq!(super::spec(&world, 0.01).spacing, 1.0);
    }

    fn lines_on(world: &World) -> [f32; 3] {
        plane(&spec(world, 1.0), Vec3::ZERO, 1.0, None).color
    }

    #[test]
    fn lines_are_a_soft_shift_of_the_surface_they_lie_on() {
        let mut world = World::new();
        let id = baseplate_at(&mut world, 0.0, 1.0);
        let color = |r, g, b| Value::Color3(Color3::new(r, g, b));
        world
            .set_property(id, "Color", color(1.0, 1.0, 1.0))
            .unwrap();
        let on_white = lines_on(&world)[0];
        assert!(on_white < 0.3, "darker than the plate: {on_white}");
        assert!(on_white > 0.05, "but never black: {on_white}");
        world
            .set_property(id, "Color", color(0.1, 0.1, 0.1))
            .unwrap();
        let on_dark = lines_on(&world)[0];
        assert!(on_dark > 0.1, "lighter than the plate: {on_dark}");
        assert!(on_dark < 0.5, "but never white: {on_dark}");
        // A red plate gets reddish lines.
        world
            .set_property(id, "Color", color(0.8, 0.1, 0.1))
            .unwrap();
        let [r, g, _] = lines_on(&world);
        assert!(r > g, "keeps the surface's hue: {r} {g}");
    }

    #[test]
    fn a_glowing_baseplate_changes_its_lines() {
        let mut world = World::new();
        let id = baseplate_at(&mut world, 0.0, 1.0);
        world
            .set_property(id, "Color", Value::Color3(Color3::new(0.05, 0.05, 0.05)))
            .unwrap();
        let unlit = lines_on(&world);
        world
            .set_property(
                id,
                "EmissiveColor",
                Value::Color3(Color3::new(0.0, 1.0, 0.0)),
            )
            .unwrap();
        world
            .set_property(id, "Emission", Value::Number(1.0))
            .unwrap();
        let lit = lines_on(&world);
        assert!(lit[1] < unlit[1], "a bright green plate wants darker lines");
        assert!(lit[1] > lit[0], "and they stay green: {lit:?}");
    }

    #[test]
    fn lines_near_the_selected_part_take_its_color_and_a_glow_tints_more() {
        let mut world = World::new();
        let id = part(&mut world);
        world
            .set_property(id, "Color", Value::Color3(Color3::new(1.0, 0.0, 0.0)))
            .unwrap();
        world
            .set_property(id, "Position", Value::Vector3(Vector3::new(3.0, 1.0, -2.0)))
            .unwrap();
        assert_eq!(tint(&world, None, None), None);
        let plain = tint(&world, Some(id), None).unwrap();
        assert_eq!(plain.at, [3.0, -2.0]);
        assert!(plain.color[0] > 0.9 && plain.color[1] < 0.01);
        world
            .set_property(id, "Emission", Value::Number(1.0))
            .unwrap();
        world
            .set_property(
                id,
                "EmissiveColor",
                Value::Color3(Color3::new(0.0, 0.0, 1.0)),
            )
            .unwrap();
        let glowing = tint(&world, Some(id), None).unwrap();
        assert!(glowing.amount > plain.amount);
        assert!(
            glowing.color[2] > 0.9,
            "the glow's color: {:?}",
            glowing.color
        );
    }

    #[test]
    fn the_nearest_glowing_part_in_reach_tints_the_grid_when_nothing_is_selected() {
        let mut world = World::new();
        let dull = part(&mut world);
        let glow = part(&mut world);
        world
            .set_property(glow, "Emission", Value::Number(1.0))
            .unwrap();
        let mut nearby = NearbyGlow::default();
        assert_eq!(nearby.update(&world, Vec3::ZERO, 0.0), Some(glow));
        let far = NearbyGlow::default().update(&world, Vec3::new(500.0, 0.0, 0.0), 0.0);
        assert_eq!(far, None);
        let tinted = tint(&world, Some(dull), Some(glow)).unwrap();
        assert_eq!(tinted.amount, TINT_AMOUNT, "the selection comes first");
    }

    #[test]
    fn numbers_give_each_distance_along_two_rows_and_stay_on_the_grid() {
        let spec = Spec {
            height: 2.0,
            area: Some((Vec3::new(-25.0, 2.0, -15.0), Vec3::new(25.0, 2.0, 15.0))),
            spacing: 1.0,
            surface: LIT_GROUND,
        };
        let shown = labels(&spec, Vec3::new(1.0, 2.0, 2.0), 10.0);
        let row = |along_x| -> Vec<&str> {
            let mut texts: Vec<&str> = shown
                .iter()
                .filter(|label| label.along_x == along_x)
                .map(|label| label.text.as_str())
                .collect();
            texts.sort_by_key(|text| text.parse::<i32>().unwrap());
            texts
        };
        assert_eq!(row(true), ["-20", "-10", "0", "10", "20"]);
        assert_eq!(row(false), ["-10", "10"]);
        assert!(shown.iter().all(|label| label.at.y == 2.0));
    }

    #[test]
    fn a_number_behind_a_part_is_left_out() {
        let mut world = World::new();
        let id = part(&mut world);
        world
            .set_property(id, "Position", Value::Vector3(Vector3::new(0.0, 5.0, 0.0)))
            .unwrap();
        let camera = Vec3::new(0.0, 20.0, 0.0);
        assert!(hidden(&world, camera, Vec3::ZERO));
        assert!(!hidden(&world, camera, Vec3::new(30.0, 0.0, 0.0)));
    }
}
