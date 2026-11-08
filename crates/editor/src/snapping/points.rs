//! Snap points: the places on a part that a move can click together, shown
//! as markers while a part is dragged. Every box offers its corners, the
//! middles of its edges and the middles of its faces, and every Attachment
//! offers its own place, so two parts' Attachments can be lined up exactly.

use axcenity_core::{ClassName, InstanceId, World};
use axcenity_render::glam::Vec3;

use super::{Obb, Obstacle};
use crate::constraints;

/// The most markers shown around a drag, nearest first.
const MOST_SHOWN: usize = 240;
/// The most moving boxes whose points are tried, for huge selections.
const MOST_MOVING: usize = 48;
/// A move shorter than this hasn't started, in studs.
const SHOWN_FROM: f32 = 0.0005;

/// What a snap point marks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointKind {
    Corner,
    Edge,
    Face,
    Attachment,
}

/// A place a move can click to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapPoint {
    pub at: Vec3,
    pub kind: PointKind,
}

impl Obb {
    /// Its 8 corners, 12 edge middles and 6 face middles.
    pub fn snap_points(&self) -> Vec<SnapPoint> {
        let along = |i: usize, sign: f32| self.axes[i] * self.half[i] * sign;
        let mut points = Vec::with_capacity(26);
        let signs = [-1.0, 0.0, 1.0];
        for x in signs {
            for y in signs {
                for z in signs {
                    let outward = [x, y, z].iter().filter(|s| **s != 0.0).count();
                    let kind = match outward {
                        3 => PointKind::Corner,
                        2 => PointKind::Edge,
                        1 => PointKind::Face,
                        _ => continue,
                    };
                    points.push(SnapPoint {
                        at: self.center + along(0, x) + along(1, y) + along(2, z),
                        kind,
                    });
                }
            }
        }
        points
    }

    /// The gap between the world boxes around the two, 0 if they meet.
    fn gap(&self, other: &Obb) -> f32 {
        let (a_low, a_high) = self.bounds();
        let (b_low, b_high) = other.bounds();
        let apart = (a_low - b_high).max(b_low - a_high).max(Vec3::ZERO);
        apart.length()
    }
}

/// The Attachments' places when a drag starts: on what's moving, and on
/// everything else. A part being moved takes its Attachments with it.
#[derive(Clone, Debug, Default)]
pub struct Sites {
    pub own: Vec<Vec3>,
    pub others: Vec<Vec3>,
}

/// Where the Attachments in the Workspace are.
pub fn sites(world: &World, moving: &[InstanceId]) -> Sites {
    let mut sites = Sites::default();
    for id in world.descendants(world.workspace()) {
        let Some(instance) = world.get(id) else {
            continue;
        };
        if instance.class() != ClassName::Attachment {
            continue;
        }
        let Some((at, _)) = constraints::world_pose(world, id) else {
            continue;
        };
        if constraints::on_one_of(world, id, moving) {
            sites.own.push(at);
        } else {
            sites.others.push(at);
        }
    }
    sites
}

/// The ways a move can still go to reach a point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Freedom {
    /// Along a handle's line (one stud long).
    Along(Vec3),
    /// Across a surface facing this way (one stud long).
    Across(Vec3),
}

/// Two points a move brings together.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Link {
    /// How much further to move.
    pub shift: Vec3,
    /// The point on something else that it clicks to.
    pub to: Vec3,
}

/// The nearest pair of a point in `own` and one in `targets` that `freedom`
/// can bring together, each within `reach` studs of the other. A pair the
/// move can't quite reach (off its line, or off the surface) doesn't count.
pub fn nearest(
    own: &[SnapPoint],
    targets: &[SnapPoint],
    freedom: Freedom,
    reach: f32,
) -> Option<Link> {
    let mut best: Option<(f32, Link)> = None;
    for from in own {
        for to in targets {
            let gap = to.at - from.at;
            let (shift, left) = match freedom {
                Freedom::Along(way) => {
                    let along = way * gap.dot(way);
                    (along, gap - along)
                }
                Freedom::Across(normal) => {
                    let off = normal * gap.dot(normal);
                    (gap - off, off)
                }
            };
            let distance = shift.length();
            if distance > reach || left.length() > reach {
                continue;
            }
            // Pairs left closest count for most: an exact match off the
            // line beats a nearer one further off it.
            let score = distance + left.length();
            if best.is_none_or(|(known, _)| score < known) {
                best = Some((score, Link { shift, to: to.at }));
            }
        }
    }
    best.map(|(_, link)| link)
}

/// The points on `moving` (each already moved) and its Attachments.
pub fn own_points(moving: &[Obb], attachments: &[Vec3], by: Vec3) -> Vec<SnapPoint> {
    moving
        .iter()
        .take(MOST_MOVING)
        .flat_map(Obb::snap_points)
        .chain(attachments.iter().map(|&at| SnapPoint {
            at: at + by,
            kind: PointKind::Attachment,
        }))
        .collect()
}

/// The points on what's around `moving`, within `radius` studs of it, and
/// the Attachments in `others` within that distance.
pub fn nearby(
    moving: &[Obb],
    obstacles: &[Obstacle],
    others: &[Vec3],
    radius: f32,
) -> Vec<SnapPoint> {
    let touches = |obb: &Obb| moving.iter().any(|m| m.gap(obb) <= radius);
    let mut points: Vec<SnapPoint> = obstacles
        .iter()
        .filter(|obstacle| obstacle.visible && touches(&obstacle.obb))
        .flat_map(|obstacle| obstacle.obb.snap_points())
        .collect();
    points.extend(
        others
            .iter()
            .filter(|&&at| {
                moving.iter().any(|m| {
                    m.gap(&Obb {
                        center: at,
                        axes: [Vec3::X, Vec3::Y, Vec3::Z],
                        half: Vec3::ZERO,
                    }) <= radius
                })
            })
            .map(|&at| SnapPoint {
                at,
                kind: PointKind::Attachment,
            }),
    );
    points
}

/// Which tool a drag is using, and what only that tool knows.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum DragKind {
    #[default]
    Move,
    /// A face pulled `pulled` studs along `outward` (one stud long).
    Resize { pulled: f32, outward: Vec3 },
    /// Turned `radians` around `axis` through `pivot`, measured from the
    /// direction `from` (one stud long, square to the axis).
    Turn {
        pivot: Vec3,
        axis: Vec3,
        from: Vec3,
        radians: f32,
    },
}

/// What a drag has done so far, for drawing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DragReport {
    pub kind: DragKind,
    /// Where the part being dragged started.
    pub start: Vec3,
    /// How far it has come since.
    pub moved: Vec3,
    /// Markers on what's around the part.
    pub targets: Vec<SnapPoint>,
    /// The point it has clicked to.
    pub link: Option<Vec3>,
    /// How far the dragged part reaches from its middle the way it's going.
    pub reach: f32,
}

impl DragReport {
    /// The report for a move that has come `moved` from `start`, keeping
    /// the markers nearest the dragged box `last` (already moved).
    pub fn new(
        start: Vec3,
        moved: Vec3,
        last: Option<&Obb>,
        mut targets: Vec<SnapPoint>,
        link: Option<Vec3>,
    ) -> DragReport {
        let middle = last.map_or(start + moved, |obb| obb.center);
        targets.sort_by(|a, b| {
            a.at.distance_squared(middle)
                .total_cmp(&b.at.distance_squared(middle))
        });
        targets.truncate(MOST_SHOWN);
        DragReport {
            kind: DragKind::Move,
            start,
            moved,
            targets,
            link,
            reach: last.map_or(0.0, |obb| obb.reach(moved.normalize_or_zero())),
        }
    }

    /// True once the part has left how it started. Numbers and markers
    /// show only then.
    pub fn is_moving(&self) -> bool {
        match self.kind {
            DragKind::Move => self.distance() >= SHOWN_FROM,
            DragKind::Resize { pulled, .. } => pulled.abs() >= SHOWN_FROM,
            DragKind::Turn { radians, .. } => radians.abs() >= SHOWN_FROM,
        }
    }

    /// True for a change the grid's printed studs help to measure.
    pub fn measures_studs(&self) -> bool {
        self.is_moving() && !matches!(self.kind, DragKind::Turn { .. })
    }

    /// How far it's come, in studs.
    pub fn distance(&self) -> f32 {
        self.moved.length()
    }
}

/// `point` on `pick`'s box, pulled to the nearest snap point of that box
/// within `reach` studs. For the Constraint tool's clicks.
pub fn pull_to_point(obb: &Obb, point: Vec3, reach: f32) -> Vec3 {
    obb.snap_points()
        .into_iter()
        .map(|candidate| candidate.at)
        .filter(|candidate| candidate.distance(point) <= reach)
        .min_by(|a, b| a.distance(point).total_cmp(&b.distance(point)))
        .unwrap_or(point)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_render::glam::Quat;

    fn cube(center: Vec3, size: f32) -> Obb {
        super::super::obb(center, Vec3::splat(size), Quat::IDENTITY)
    }

    fn point(at: Vec3) -> SnapPoint {
        SnapPoint {
            at,
            kind: PointKind::Corner,
        }
    }

    #[test]
    fn a_box_offers_its_corners_edge_middles_and_face_middles() {
        let points = cube(Vec3::ZERO, 2.0).snap_points();
        let count = |kind| points.iter().filter(|p| p.kind == kind).count();
        assert_eq!(
            (
                count(PointKind::Corner),
                count(PointKind::Edge),
                count(PointKind::Face)
            ),
            (8, 12, 6)
        );
        assert!(points.iter().any(|p| p.at == Vec3::splat(1.0)));
        assert!(
            points
                .iter()
                .any(|p| p.at == Vec3::new(1.0, 0.0, 0.0) && p.kind == PointKind::Face)
        );
    }

    #[test]
    fn a_turned_box_turns_its_points() {
        let turned = super::super::obb(
            Vec3::ZERO,
            Vec3::new(4.0, 2.0, 2.0),
            Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
        );
        assert!(
            turned
                .snap_points()
                .iter()
                .any(|p| p.at.distance(Vec3::new(0.0, 0.0, -2.0)) < 0.001)
        );
    }

    #[test]
    fn a_move_along_a_line_clicks_to_a_point_it_can_reach_on_that_line() {
        let own = [point(Vec3::new(1.0, 0.0, 0.0))];
        let targets = [point(Vec3::new(1.4, 0.0, 0.0))];
        let link = nearest(&own, &targets, Freedom::Along(Vec3::X), 0.5).unwrap();
        assert!(link.shift.distance(Vec3::new(0.4, 0.0, 0.0)) < 0.001);
        assert_eq!(link.to, targets[0].at);
    }

    #[test]
    fn a_point_out_of_reach_or_off_the_line_is_not_snapped_to() {
        let own = [point(Vec3::ZERO)];
        let far = [point(Vec3::new(2.0, 0.0, 0.0))];
        assert_eq!(nearest(&own, &far, Freedom::Along(Vec3::X), 0.5), None);
        let off_line = [point(Vec3::new(0.2, 3.0, 0.0))];
        assert_eq!(nearest(&own, &off_line, Freedom::Along(Vec3::X), 0.5), None);
    }

    #[test]
    fn a_move_across_a_surface_lines_up_without_leaving_it() {
        let own = [point(Vec3::ZERO)];
        let targets = [point(Vec3::new(0.3, 0.2, -0.2))];
        let link = nearest(&own, &targets, Freedom::Across(Vec3::Y), 0.5).unwrap();
        assert!(link.shift.y.abs() < 0.001, "it stays on the surface");
        assert!(link.shift.distance(Vec3::new(0.3, 0.0, -0.2)) < 0.001);
    }

    #[test]
    fn the_closest_pair_wins() {
        let own = [point(Vec3::ZERO), point(Vec3::new(1.0, 0.0, 0.0))];
        let targets = [
            point(Vec3::new(-0.4, 0.0, 0.0)),
            point(Vec3::new(1.1, 0.0, 0.0)),
        ];
        let link = nearest(&own, &targets, Freedom::Along(Vec3::X), 0.5).unwrap();
        assert_eq!(link.to, targets[1].at);
    }

    #[test]
    fn only_visible_parts_near_the_move_offer_points() {
        let mut world = World::new();
        let id = world.create(ClassName::Part).unwrap();
        let near = Obstacle {
            id,
            obb: cube(Vec3::new(3.0, 0.0, 0.0), 1.0),
            solid: true,
            visible: true,
        };
        let ghost = Obstacle {
            visible: false,
            ..near
        };
        let far = Obstacle {
            obb: cube(Vec3::new(50.0, 0.0, 0.0), 1.0),
            ..near
        };
        let moving = [cube(Vec3::ZERO, 2.0)];
        assert_eq!(nearby(&moving, &[near], &[], 4.0).len(), 26);
        assert!(nearby(&moving, &[ghost, far], &[], 4.0).is_empty());
    }

    #[test]
    fn attachments_count_as_points_and_move_with_their_part() {
        let moving = [cube(Vec3::ZERO, 2.0)];
        let around = nearby(&moving, &[], &[Vec3::new(2.0, 0.0, 0.0)], 4.0);
        assert_eq!(around.len(), 1);
        assert_eq!(around[0].kind, PointKind::Attachment);
        let own = own_points(&moving, &[Vec3::ONE], Vec3::X);
        assert!(
            own.iter()
                .any(|p| p.kind == PointKind::Attachment && p.at == Vec3::new(2.0, 1.0, 1.0))
        );
    }

    #[test]
    fn a_report_says_how_far_and_keeps_the_nearest_markers() {
        let targets = (0..400)
            .map(|i| point(Vec3::new(i as f32, 0.0, 0.0)))
            .collect();
        let last = cube(Vec3::ZERO, 1.0);
        let report = DragReport::new(
            Vec3::new(-3.0, 0.0, 0.0),
            Vec3::new(3.0, 0.0, 4.0),
            Some(&last),
            targets,
            None,
        );
        assert_eq!(report.distance(), 5.0);
        assert!(report.is_moving());
        assert!(!DragReport::default().is_moving());
        assert_eq!(report.targets.len(), MOST_SHOWN);
        assert_eq!(report.targets[0].at, Vec3::ZERO);
    }

    #[test]
    fn a_click_near_a_snap_point_lands_on_it_and_one_far_from_any_does_not() {
        let block = cube(Vec3::ZERO, 2.0);
        let near = pull_to_point(&block, Vec3::new(0.9, 1.0, 1.0), 0.2);
        assert_eq!(near, Vec3::new(1.0, 1.0, 1.0));
        let far = Vec3::new(0.5, 1.0, 0.5);
        assert_eq!(pull_to_point(&block, far, 0.2), far);
    }
}
