//! The measuring grid over the baseplate: thin lines every move step, a
//! heavier line every ten, and printed distances along two rows through the
//! middle of the view. Editing only; a built game never has it.
//!
//! `render` draws the lines (`DrawList::grid`), so parts hide them; egui
//! prints the numbers. Numbers behind a part are left out, by a ray test
//! worked out a few times a second rather than every frame.

use axcenity_core::{InstanceId, Value, World};
use axcenity_render::GridPlane;
use axcenity_render::glam::Vec3;
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

/// Where the grid is and how it's divided.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spec {
    pub height: f32,
    /// The baseplate's smallest and largest X and Z, or `None` for ground
    /// that goes on around the camera.
    pub area: Option<(Vec3, Vec3)>,
    /// Studs between lines.
    pub spacing: f32,
    /// Whether the surface under it is bright, wanting dark lines.
    pub bright: bool,
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
        let color = part.property("Color").and_then(Value::as_color3);
        Some((at, size, color))
    });
    match plate {
        Some((at, size, color)) => {
            let top = at.y + size.y / 2.0;
            let half = Vec3::new(size.x / 2.0, 0.0, size.z / 2.0);
            let middle = Vec3::new(at.x, top, at.z);
            let bright = color.is_none_or(|c| 0.3 * c.r + 0.6 * c.g + 0.1 * c.b > 0.35);
            Spec {
                height: top,
                area: Some((middle - half, middle + half)),
                spacing,
                bright,
            }
        }
        None => Spec {
            height: 0.0,
            area: None,
            spacing,
            bright: false,
        },
    }
}

/// What `render` needs to draw the lines.
pub fn plane(spec: &Spec, camera: Vec3) -> GridPlane {
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
        color: if spec.bright {
            [0.02, 0.02, 0.025]
        } else {
            [0.75, 0.75, 0.8]
        },
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
        let drawn = plane(&spec, Vec3::ZERO);
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
        let drawn = plane(&found, Vec3::new(500.0, 20.0, 0.0));
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

    #[test]
    fn a_dark_baseplate_gets_light_lines_and_a_bright_one_dark_lines() {
        let mut world = World::new();
        let id = baseplate_at(&mut world, 0.0, 1.0);
        let lines = |world: &World| plane(&spec(world, 1.0), Vec3::ZERO).color[0];
        world
            .set_property(id, "Color", Value::Color3(Color3::new(1.0, 1.0, 1.0)))
            .unwrap();
        assert!(lines(&world) < 0.2);
        world
            .set_property(id, "Color", Value::Color3(Color3::new(0.1, 0.1, 0.1)))
            .unwrap();
        assert!(lines(&world) > 0.5);
    }

    #[test]
    fn numbers_give_each_distance_along_two_rows_and_stay_on_the_grid() {
        let spec = Spec {
            height: 2.0,
            area: Some((Vec3::new(-25.0, 2.0, -15.0), Vec3::new(25.0, 2.0, 15.0))),
            spacing: 1.0,
            bright: true,
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
