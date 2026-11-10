//! What's drawn over the viewport while parts are moved, resized or turned:
//! a white guide along the change, the snap points near it, and one number
//! saying how far: studs moved, studs a face has been pulled, or degrees
//! turned. None of it shows until something has changed.
//!
//! Meanwhile the part also stays, solid, as it started, and is drawn
//! see-through as it's becoming ([`ghost_moving`]).

use axcenity_render::DrawList;
use axcenity_render::glam::{Mat4, Quat, Vec3};
use eframe::egui::{self, Color32, Stroke, pos2, vec2};

use crate::screen::Screen;
use crate::snapping::points::{DragKind, DragReport, PointKind, SnapPoint};

const GUIDE_COLOR: Color32 = crate::theme::overlay::MEASURE;
const TARGET_COLOR: Color32 = Color32::from_rgb(235, 235, 240);
const ATTACHMENT_COLOR: Color32 = Color32::from_rgb(110, 240, 140);
const RING_COLOR: Color32 = Color32::from_rgb(255, 214, 40);
const MIDDLE_COLOR: Color32 = Color32::from_rgb(90, 190, 255);
const NUMBER_BACK: Color32 = crate::theme::overlay::LABEL_BACK;

/// How far the guide line reaches each way, in studs.
const GUIDE_REACH: f32 = 4000.0;
/// How far past the part's leading face the number sits, in points.
const NUMBER_GAP: f32 = 34.0;
/// How solid the part is drawn where it's going.
const GHOST_OPACITY: f32 = 0.4;
/// The radius of a turn's arc, in points: the Rotate rings' own.
const TURN_RADIUS: f32 = 90.0;

fn marker(painter: &egui::Painter, at: egui::Pos2, kind: PointKind, color: Color32) {
    let outline = Stroke::new(1.0_f32, Color32::from_black_alpha(190));
    match kind {
        PointKind::Corner => {
            let rect = egui::Rect::from_center_size(at, egui::Vec2::splat(5.0));
            painter.rect(rect, 0.0, color, outline, egui::StrokeKind::Outside);
        }
        PointKind::Edge => {
            let reach = 3.0;
            let corners = vec![
                at + vec2(0.0, -reach),
                at + vec2(reach, 0.0),
                at + vec2(0.0, reach),
                at + vec2(-reach, 0.0),
            ];
            painter.add(egui::Shape::convex_polygon(corners, color, outline));
        }
        PointKind::Face => {
            painter.circle(at, 2.8, color, outline);
        }
        PointKind::Attachment => {
            painter.circle_stroke(at, 5.0, Stroke::new(2.0_f32, color));
            painter.circle_filled(at, 1.8, color);
        }
    }
}

fn markers(painter: &egui::Painter, screen: &Screen, rect: egui::Rect, points: &[SnapPoint]) {
    for point in points {
        let Some(at) = screen.point(point.at).filter(|at| rect.contains(*at)) else {
            continue;
        };
        let tint = if point.kind == PointKind::Attachment {
            ATTACHMENT_COLOR
        } else {
            TARGET_COLOR
        };
        marker(painter, at, point.kind, tint);
    }
}

/// `value` to at most 3 decimals, without trailing zeros.
pub(super) fn studs(value: f32) -> String {
    let rounded = (value * 1000.0).round() / 1000.0;
    // `-0.0` prints with its sign.
    if rounded == 0.0 {
        "0".to_owned()
    } else {
        format!("{rounded}")
    }
}

fn paint_ring(painter: &egui::Painter, at: egui::Pos2) {
    painter.circle_filled(at, 5.0, Color32::WHITE);
    painter.circle_stroke(at, 7.0, Stroke::new(2.5_f32, RING_COLOR));
}

/// The number, past the part's leading face and kept inside the viewport.
fn paint_number(painter: &egui::Painter, rect: egui::Rect, anchor: egui::Pos2, text: String) {
    let galley = painter.layout_no_wrap(text, egui::FontId::proportional(13.0), Color32::WHITE);
    let size = galley.size() + vec2(12.0, 8.0);
    let wanted = egui::Rect::from_center_size(anchor, size);
    let shift = vec2(
        (rect.right() - 6.0 - wanted.right()).min(0.0)
            + (rect.left() + 6.0 - wanted.left()).max(0.0),
        (rect.bottom() - 6.0 - wanted.bottom()).min(0.0)
            + (rect.top() + 6.0 - wanted.top()).max(0.0),
    );
    let back = wanted.translate(shift);
    painter.rect_filled(back, 2.0, NUMBER_BACK);
    painter.galley(
        pos2(back.left() + 6.0, back.top() + 4.0),
        galley,
        Color32::WHITE,
    );
}

/// Draws `report` over the viewport, once something has changed.
pub(super) fn paint(
    painter: &egui::Painter,
    screen: &Screen,
    rect: egui::Rect,
    report: &DragReport,
) {
    if !report.is_moving() {
        return;
    }
    match report.kind {
        DragKind::Move => {
            let way = report.moved.normalize_or_zero();
            paint_line(painter, screen, rect, report, way, studs(report.distance()));
        }
        DragKind::Resize { pulled, outward } => {
            paint_line(painter, screen, rect, report, outward, studs(pulled));
        }
        DragKind::Turn {
            pivot,
            axis,
            from,
            radians,
        } => paint_turn(painter, screen, rect, (pivot, axis, from), radians),
    }
}

fn start_dot(painter: &egui::Painter, at: egui::Pos2) {
    painter.circle(
        at,
        5.0,
        Color32::WHITE,
        Stroke::new(1.0_f32, Color32::from_black_alpha(160)),
    );
}

/// A move or a resize: everything lies along the line through `way`.
fn paint_line(
    painter: &egui::Painter,
    screen: &Screen,
    rect: egui::Rect,
    report: &DragReport,
    way: Vec3,
    number: String,
) {
    let end = report.start + report.moved;
    if let Some(line) = screen.line(report.start - way * GUIDE_REACH, end + way * GUIDE_REACH) {
        painter.line_segment(line, Stroke::new(1.0_f32, GUIDE_COLOR));
    }
    if let Some(at) = screen.point(report.start - way * report.reach) {
        start_dot(painter, at);
    }
    markers(painter, screen, rect, &report.targets);
    let ring = report.link.unwrap_or(report.start + way * report.reach);
    if let Some(at) = screen.point(ring) {
        paint_ring(painter, at);
    }
    if let Some(at) = screen.point(end) {
        painter.circle_filled(at, 2.5, MIDDLE_COLOR);
    }
    let leading = end + way * report.reach;
    if let (Some(face), Some(ahead)) = (screen.point(leading), screen.point(leading + way)) {
        // Straight on, the part hides which way is ahead: go to the right.
        let ahead = (ahead - face).normalized();
        let ahead = if ahead.is_finite() && ahead != egui::Vec2::ZERO {
            ahead
        } else {
            egui::Vec2::X
        };
        paint_number(painter, rect, face + ahead * NUMBER_GAP, number);
    }
}

/// A turn: an arc the size of the Rotate rings, from where it began.
fn paint_turn(
    painter: &egui::Painter,
    screen: &Screen,
    rect: egui::Rect,
    (pivot, axis, from): (Vec3, Vec3, Vec3),
    radians: f32,
) {
    let radius = screen.studs_per_point(pivot) * TURN_RADIUS;
    let sweep = radians.clamp(-std::f32::consts::TAU, std::f32::consts::TAU);
    let on_arc = |angle: f32| pivot + Quat::from_axis_angle(axis, angle) * from * radius;
    let steps = ((sweep.abs() / std::f32::consts::TAU * 96.0).ceil() as usize).max(2);
    let arc: Vec<egui::Pos2> = (0..=steps)
        .filter_map(|i| screen.point(on_arc(sweep * i as f32 / steps as f32)))
        .collect();
    painter.add(egui::Shape::line(arc, Stroke::new(2.0_f32, GUIDE_COLOR)));
    for angle in [0.0, sweep] {
        if let Some(spoke) = screen.line(pivot, on_arc(angle)) {
            painter.line_segment(spoke, Stroke::new(1.0_f32, GUIDE_COLOR));
        }
    }
    if let Some(at) = screen.point(on_arc(0.0)) {
        start_dot(painter, at);
    }
    let middle = screen.point(pivot);
    if let Some(at) = middle {
        painter.circle_filled(at, 2.5, MIDDLE_COLOR);
    }
    if let Some(at) = screen.point(on_arc(sweep)) {
        paint_ring(painter, at);
        let outward = middle.map_or(egui::Vec2::X, |middle| (at - middle).normalized());
        let number = format!("{}°", studs((radians.to_degrees() * 10.0).round() / 10.0));
        paint_number(painter, rect, at + outward * NUMBER_GAP, number);
    }
}

/// While parts change: leaves each solid as it started and draws it
/// see-through as it is now. `moves` is each part's place now and what
/// turns its transform now into the one it started with.
pub(super) fn ghost_moving(list: &mut DrawList, moves: &[(Vec3, Mat4)]) {
    let mut starts = Vec::new();
    for item in &mut list.items {
        let at = item.transform.w_axis.truncate();
        let Some((_, back)) = moves.iter().find(|(now, _)| now.distance(at) < 0.001) else {
            continue;
        };
        let mut start = item.clone();
        start.transform = *back * item.transform;
        start.id = 0;
        starts.push(start);
        item.material.opacity = item.material.opacity.min(GHOST_OPACITY);
        item.casts_shadow = false;
    }
    list.items.extend(starts);
}

/// The snap points of the part under the Constraint tool.
pub(super) fn paint_part_points(
    painter: &egui::Painter,
    screen: &Screen,
    rect: egui::Rect,
    points: &[SnapPoint],
) {
    markers(painter, screen, rect, points);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapping::{self};
    use axcenity_core::{ClassName, Value, Vector3, World};
    use axcenity_render::Camera;
    use axcenity_render::glam::Quat;

    fn report() -> DragReport {
        let obb = snapping::obb(Vec3::new(3.0, 1.0, 0.0), Vec3::splat(2.0), Quat::IDENTITY);
        let other = snapping::obb(Vec3::new(7.0, 1.0, 0.0), Vec3::splat(2.0), Quat::IDENTITY);
        let mut targets = other.snap_points();
        targets.push(SnapPoint {
            at: Vec3::new(5.5, 3.0, 0.0),
            kind: PointKind::Attachment,
        });
        DragReport::new(
            Vec3::new(-2.0, 1.0, 0.0),
            Vec3::new(5.0, 0.0, 0.0),
            Some(&obb),
            targets,
            Some(Vec3::new(6.0, 2.0, 1.0)),
        )
    }

    fn camera() -> Camera {
        Camera {
            position: Vec3::new(2.0, 8.0, 12.0),
            look_at: Vec3::new(2.0, 0.0, 0.0),
            fov_y_degrees: 60.0,
        }
    }

    fn draw(ctx: &egui::Context, report: &DragReport) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let rect = ui.max_rect();
            paint(ui.painter(), &Screen::new(camera(), rect), rect, report);
        });
    }

    #[test]
    fn numbers_and_snap_points_show_only_once_the_part_has_moved() {
        let ctx = crate::theme::test_context();
        let count = |report: &DragReport| {
            let output = ctx.run(Default::default(), |ctx| draw(ctx, report));
            output.shapes.len()
        };
        count(&report());
        let moving = count(&report());
        let at_rest = DragReport {
            moved: Vec3::ZERO,
            ..report()
        };
        let still = count(&at_rest);
        assert_eq!(
            still,
            count(&DragReport::default()),
            "nothing's drawn at rest"
        );
        assert!(moving > still + 25, "{moving} shapes against {still}");

        if let Some(snapshots) = std::env::var_os("AXCENITY_SNAPSHOTS") {
            let path = std::path::PathBuf::from(snapshots).join("move-overlay.png");
            let report = report();
            crate::snapshot::snapshot(
                &path,
                egui::vec2(640.0, 400.0),
                vec![Vec::new(); 2],
                |ctx| draw(ctx, &report),
            );
        }
    }

    #[test]
    fn a_resize_and_a_turn_draw_only_once_they_have_changed_something() {
        let ctx = crate::theme::test_context();
        let count = |report: &DragReport| {
            let output = ctx.run(Default::default(), |ctx| draw(ctx, report));
            output.shapes.len()
        };
        let resize = |pulled| DragReport {
            kind: DragKind::Resize {
                pulled,
                outward: Vec3::X,
            },
            start: Vec3::X,
            moved: Vec3::X * pulled,
            ..Default::default()
        };
        let turn = |radians| DragReport {
            kind: DragKind::Turn {
                pivot: Vec3::ZERO,
                axis: Vec3::Y,
                from: Vec3::X,
                radians,
            },
            ..Default::default()
        };
        count(&resize(2.0));
        let nothing = count(&DragReport::default());
        assert_eq!(count(&resize(0.0)), nothing);
        assert_eq!(count(&turn(0.0)), nothing);
        assert!(count(&resize(2.0)) > nothing + 4);
        assert!(count(&turn(1.0)) > nothing + 4);

        if let Some(snapshots) = std::env::var_os("AXCENITY_SNAPSHOTS") {
            let folder = std::path::PathBuf::from(snapshots);
            let shots = [("resize", resize(2.0)), ("turn", turn(1.0))];
            for (name, report) in shots {
                crate::snapshot::snapshot(
                    &folder.join(format!("{name}-overlay.png")),
                    egui::vec2(640.0, 400.0),
                    vec![Vec::new(); 2],
                    |ctx| draw(ctx, &report),
                );
            }
        }
    }

    #[test]
    fn the_number_is_the_distance_in_studs_tidied() {
        assert_eq!(studs(Vec3::new(3.0, 0.0, -4.0).length()), "5");
        assert_eq!(studs(1.23456), "1.235");
        assert_eq!(studs(-0.0001), "0");
        assert_eq!(studs(-2.0), "-2");
    }

    #[test]
    fn a_moving_part_stays_solid_where_it_started_and_is_see_through_where_it_is_going() {
        let mut world = World::new();
        let part = world.create(ClassName::Part).unwrap();
        let workspace = world.workspace();
        world.set_parent(part, Some(workspace)).unwrap();
        let now = Vec3::new(6.0, 1.0, 0.0);
        world
            .set_property(
                part,
                "Position",
                Value::Vector3(Vector3::new(6.0, 1.0, 0.0)),
            )
            .unwrap();
        let assets = axcenity_assets::Assets::new(std::env::temp_dir().join("axcenity-no-ghosts"));
        let mut list = axcenity_scene::build_draw_list(&world, &assets, camera());
        assert_eq!(list.items.len(), 1);

        ghost_moving(
            &mut list,
            &[(now, Mat4::from_translation(Vec3::new(-4.0, 0.0, 0.0)))],
        );

        assert_eq!(list.items.len(), 2);
        let at = |i: usize| list.items[i].transform.w_axis.truncate();
        assert_eq!(at(0), now);
        assert!(list.items[0].material.opacity < 0.5);
        assert_eq!(at(1), Vec3::new(2.0, 1.0, 0.0));
        assert_eq!(list.items[1].material.opacity, 1.0);
    }
}
