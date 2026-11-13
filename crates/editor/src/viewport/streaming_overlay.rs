//! The Streaming overlay (roadmap R16): with `Workspace.StreamingEnabled`
//! on, what streams around the camera, while editing as in Play. Each
//! streaming unit's box is coloured by where it stands from the camera
//! (`axcenity_core::streaming::Reach`), the parts that are always there
//! are outlined, both radii lie at the camera's feet, and a legend counts
//! what a player standing here has. The split is `core::streaming`'s, the
//! same one the server and the host's screen use: `docs/architecture/streaming.md`.

use axcenity_core::streaming::{LARGE_UNIT, Partition, Reach, Settings, Summary};
use axcenity_core::{InstanceId, World};
use axcenity_render::glam::{Quat, Vec3};
use eframe::egui::{self, Color32, RichText};

use super::legend::thousands;
use super::overlays::{bounds_edges, v3};
use crate::angular;
use crate::gizmo::Pose;
use crate::screen::Screen;

/// Within the minimum radius: there at once.
pub(super) const NEAR_COLOR: Color32 = Color32::from_rgb(110, 220, 120);
/// Within the target radius: loads, nearest first.
pub(super) const LOADS_COLOR: Color32 = Color32::from_rgb(90, 170, 255);
/// Between the target and unload radii: kept if it was loaded.
const KEPT_COLOR: Color32 = Color32::from_rgb(150, 150, 165);
/// Parts in no unit (a Persistent Model, code a device runs): always there.
const ALWAYS_COLOR: Color32 = Color32::from_rgb(255, 160, 60);
/// At most this many unit boxes are drawn, the nearest first, so a big
/// world's overlay stays cheap; past the unload radius none are.
const MAX_BOXES: usize = 3000;
/// At most this many always-there parts are outlined.
const MAX_ALWAYS: usize = 500;

/// The overlay's split of the world, kept between frames: it's rebuilt
/// when the Workspace's tree or the World changes, and otherwise only a
/// share of its boxes is measured each frame.
#[derive(Default)]
pub(super) struct StreamingOverlay {
    partition: Option<Partition>,
}

impl StreamingOverlay {
    /// Brings the split up to date with `world`; `None` (and the split
    /// dropped) while streaming is off there.
    fn refresh(&mut self, world: &World) -> Option<(Settings, &Partition)> {
        let Some(settings) = Settings::of(world) else {
            self.partition = None;
            return None;
        };
        let partition = self.partition.get_or_insert_with(Partition::default);
        partition.refresh(world);
        Some((settings, partition))
    }

    /// Drops the split, while the overlay is off.
    pub(super) fn clear(&mut self) {
        self.partition = None;
    }

    /// Draws the overlay over `rect` for a camera at `eye`, and its legend.
    pub(super) fn paint(
        &mut self,
        ui: &egui::Ui,
        rect: egui::Rect,
        screen: &Screen,
        world: &World,
        eye: Vec3,
    ) {
        let point = axcenity_core::Vector3::new(eye.x, eye.y, eye.z);
        let lines = match self.refresh(world) {
            Some((settings, partition)) => {
                let painter = ui.painter_at(rect);
                paint_units(&painter, screen, world, partition, &settings, eye);
                super::overlays::paint_streaming_radius(&painter, screen, &settings, eye);
                legend_lines(&partition.summary(&settings, point), &settings, world)
            }
            None => vec![(
                None,
                "Streaming is off. Turn on the Workspace's StreamingEnabled to stream a big world."
                    .to_owned(),
            )],
        };
        show_legend(ui, rect, &lines);
    }
}

/// The colour a unit `reach` away is drawn in, if it's drawn.
pub(super) fn reach_color(reach: Reach) -> Option<Color32> {
    match reach {
        Reach::Near => Some(NEAR_COLOR),
        Reach::Loads => Some(LOADS_COLOR),
        Reach::Kept => Some(KEPT_COLOR),
        Reach::Out => None,
    }
}

/// Each unit's box within the unload radius, nearest first, in its reach's
/// colour, and the always-there parts in orange.
fn paint_units(
    painter: &egui::Painter,
    screen: &Screen,
    world: &World,
    partition: &Partition,
    settings: &Settings,
    eye: Vec3,
) {
    let point = axcenity_core::Vector3::new(eye.x, eye.y, eye.z);
    let mut near: Vec<(f32, usize)> = partition
        .units()
        .iter()
        .enumerate()
        .filter(|(_, unit)| unit.is_placed())
        .map(|(index, unit)| (unit.distance(point), index))
        .filter(|&(distance, _)| distance <= settings.unload_radius())
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    for &(distance, index) in near.iter().take(MAX_BOXES) {
        let Some(color) = reach_color(settings.reach(distance)) else {
            continue;
        };
        let (min, max) = partition.units()[index].bounds();
        let (min, max) = (v3(min), v3(max));
        let edges = angular::box_edges(Pose {
            position: (min + max) / 2.0,
            rotation: Quat::IDENTITY,
            size: max - min,
        });
        let stroke = egui::Stroke::new(1.0_f32, color.gamma_multiply(0.85));
        for (a, b) in edges {
            if let Some(line) = screen.line(a, b) {
                painter.line_segment(line, stroke);
            }
        }
    }
    let stroke = egui::Stroke::new(1.5_f32, ALWAYS_COLOR);
    for &id in partition.always_there().iter().take(MAX_ALWAYS) {
        let Some(edges) = world.get(id).and_then(bounds_edges) else {
            continue;
        };
        for (a, b) in edges {
            if let Some(line) = screen.line(a, b) {
                painter.line_segment(line, stroke);
            }
        }
    }
}

/// The legend's lines: the radii, what a player here has, what's always
/// there, and the biggest unit if it's worth splitting.
pub(super) fn legend_lines(
    summary: &Summary,
    settings: &Settings,
    world: &World,
) -> Vec<(Option<Color32>, String)> {
    let count = |value: usize| thousands(value as u64);
    let mut lines = vec![
        (
            Some(NEAR_COLOR),
            format!("Within {:.0} studs: there at once", settings.min_radius),
        ),
        (
            Some(LOADS_COLOR),
            format!(
                "Within {:.0} studs: loads, nearest first",
                settings.target_radius
            ),
        ),
        (
            Some(KEPT_COLOR),
            format!(
                "Within {:.0} studs: kept if loaded ({} units)",
                settings.unload_radius(),
                count(summary.kept_units)
            ),
        ),
        (
            Some(ALWAYS_COLOR),
            format!("Always there: {} parts", count(summary.always_there)),
        ),
        (
            None,
            format!(
                "A player here has {} of {} units ({} of {} Instances).",
                count(summary.loaded_units),
                count(summary.units),
                count(summary.loaded_members),
                count(summary.members)
            ),
        ),
    ];
    if let Some((root, members)) = summary.largest
        && members > LARGE_UNIT
    {
        lines.push((
            None,
            format!(
                "{} loads all at once ({} Instances): put its parts in Models or Folders so they stream apart.",
                name_of(world, root),
                count(members)
            ),
        ));
    }
    lines
}

fn name_of(world: &World, id: InstanceId) -> String {
    world
        .get(id)
        .map(|instance| instance.name().to_owned())
        .unwrap_or_else(|| "A unit".to_owned())
}

fn show_legend(ui: &egui::Ui, rect: egui::Rect, lines: &[(Option<Color32>, String)]) {
    egui::Area::new(egui::Id::new("streaming overlay legend"))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::LEFT_BOTTOM)
        .fixed_pos(rect.left_bottom() + egui::vec2(10.0, -10.0))
        .constrain_to(rect)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_max_width(320.0);
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.label(RichText::new("Streaming").strong());
                for (swatch, text) in lines {
                    ui.horizontal(|ui| {
                        if let Some(color) = swatch {
                            let (square, _) = ui
                                .allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                            ui.painter().rect_filled(square, 2.0, *color);
                        }
                        ui.add(egui::Label::new(RichText::new(text).small()).wrap());
                    });
                }
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_core::{ClassName, Value, Vector3};

    fn part_at(world: &mut World, parent: InstanceId, x: f32) {
        let part = world.create(ClassName::Part).unwrap();
        world
            .set_property(part, "Position", Value::Vector3(Vector3::new(x, 0.0, 0.0)))
            .unwrap();
        world.set_parent(part, Some(parent)).unwrap();
    }

    #[test]
    fn the_overlay_is_off_until_the_workspace_streams_and_then_counts_around_the_camera() {
        let mut world = World::new();
        let workspace = world.workspace();
        part_at(&mut world, workspace, 0.0);
        part_at(&mut world, workspace, 5000.0);
        let mut overlay = StreamingOverlay::default();
        assert!(overlay.refresh(&world).is_none());
        world
            .set_property(workspace, "StreamingEnabled", Value::Bool(true))
            .unwrap();
        let (settings, partition) = overlay.refresh(&world).unwrap();
        let summary = partition.summary(&settings, Vector3::default());
        assert_eq!((summary.units, summary.loaded_units), (2, 1));
        let lines = legend_lines(&summary, &settings, &world);
        assert!(lines[0].1.contains("64 studs"), "{lines:?}");
        assert!(
            lines
                .iter()
                .any(|(_, text)| text == "A player here has 1 of 2 units (1 of 2 Instances)."),
            "{lines:?}"
        );
        world
            .set_property(workspace, "StreamingEnabled", Value::Bool(false))
            .unwrap();
        assert!(overlay.refresh(&world).is_none());
        assert!(overlay.partition.is_none());
    }

    #[test]
    fn a_unit_too_big_to_stream_well_is_named_in_the_legend() {
        let mut world = World::new();
        let workspace = world.workspace();
        world
            .set_property(workspace, "StreamingEnabled", Value::Bool(true))
            .unwrap();
        let castle = world.create(ClassName::Model).unwrap();
        world.set_name(castle, "Castle").unwrap();
        world.set_parent(castle, Some(workspace)).unwrap();
        for index in 0..=LARGE_UNIT {
            part_at(&mut world, castle, index as f32);
        }
        let settings = Settings::of(&world).unwrap();
        let summary = Partition::new(&world).summary(&settings, Vector3::default());
        let lines = legend_lines(&summary, &settings, &world);
        let last = &lines.last().unwrap().1;
        assert!(
            last.starts_with("Castle loads all at once (1,001 Instances)"),
            "{last}"
        );
        assert_eq!(reach_color(Reach::Out), None);
        assert_eq!(reach_color(Reach::Near), Some(NEAR_COLOR));
    }
}
