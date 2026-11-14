//! Splines (Phase 73): a smooth curve through a `Spline`'s `SplinePoint`
//! children, in child order, and back to the first when `Closed`.
//!
//! Each stretch between two points is a cubic Bézier curve. A point's
//! `TangentMode` decides its handles: `Auto` makes the curve pass smoothly
//! through it (Catmull-Rom, from its neighbours), `Linear` points them at
//! its neighbours (two Linear points are joined by a straight line), and
//! `Free` uses its own `InTangent` and `OutTangent`, offsets in studs from
//! the point to each handle.
//!
//! Distances are measured along the curve from the first point. A table of
//! lengths, sampled [`SAMPLES`] times a stretch, turns a distance into a
//! place on a stretch, so moving at an even distance a second is an even
//! speed however the points are spaced. Points are where they are in the
//! world; a Spline has no position of its own.

use crate::{ClassName, InstanceId, Value, Vector3, World};

/// Samples a stretch is measured with. Enough that the midpoint of a
/// known curve is within a hundredth of a stud.
pub const SAMPLES: usize = 64;

/// How a point's handles are made.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TangentMode {
    /// Smooth through the point, from its neighbours.
    #[default]
    Auto,
    /// Pointing at its neighbours, so the stretches beside it are straight
    /// where their other ends are Linear too.
    Linear,
    /// The point's own `InTangent` and `OutTangent`.
    Free,
}

impl TangentMode {
    pub const NAMES: [&'static str; 3] = ["Auto", "Linear", "Free"];

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "Auto" => Some(Self::Auto),
            "Linear" => Some(Self::Linear),
            "Free" => Some(Self::Free),
            _ => None,
        }
    }
}

/// One `SplinePoint`, as the curve uses it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ControlPoint {
    pub position: Vector3,
    pub mode: TangentMode,
    /// From the point to its incoming handle, in studs (`Free` only).
    pub in_tangent: Vector3,
    /// From the point to its outgoing handle, in studs (`Free` only).
    pub out_tangent: Vector3,
}

impl ControlPoint {
    pub fn at(position: Vector3) -> Self {
        Self {
            position,
            ..Self::default()
        }
    }

    /// The `SplinePoint` `id` as a control point.
    pub fn of(world: &World, id: InstanceId) -> Option<Self> {
        let point = world.get(id)?;
        if point.class() != ClassName::SplinePoint {
            return None;
        }
        let vector = |name| point.property(name).and_then(Value::as_vector3);
        let mode = match point.property("TangentMode") {
            Some(Value::EnumItem(item)) => TangentMode::from_name(&item.item),
            _ => None,
        };
        Some(Self {
            position: vector("Position")?,
            mode: mode.unwrap_or_default(),
            in_tangent: vector("InTangent").unwrap_or_default(),
            out_tangent: vector("OutTangent").unwrap_or_default(),
        })
    }
}

/// One Bézier stretch and the lengths along it.
#[derive(Clone, Debug)]
struct Segment {
    controls: [Vector3; 4],
    /// Distance from the curve's start to the stretch's start.
    start: f32,
    /// Distance from the stretch's start at each of `SAMPLES + 1` evenly
    /// spaced curve parameters.
    lengths: Vec<f32>,
}

impl Segment {
    fn length(&self) -> f32 {
        self.lengths.last().copied().unwrap_or(0.0)
    }

    /// The curve parameter (0 to 1) at `distance` from the stretch's start.
    fn parameter_at(&self, distance: f32) -> f32 {
        let i = self.lengths.partition_point(|&l| l < distance);
        if i == 0 {
            return 0.0;
        }
        if i > SAMPLES {
            return 1.0;
        }
        let (a, b) = (self.lengths[i - 1], self.lengths[i]);
        let within = if b > a { (distance - a) / (b - a) } else { 0.0 };
        (i as f32 - 1.0 + within) / SAMPLES as f32
    }

    /// The distance from the stretch's start at curve parameter `u`.
    fn distance_at(&self, u: f32) -> f32 {
        let scaled = u.clamp(0.0, 1.0) * SAMPLES as f32;
        let i = (scaled.floor() as usize).min(SAMPLES - 1);
        let within = scaled - i as f32;
        self.lengths[i] + (self.lengths[i + 1] - self.lengths[i]) * within
    }

    fn point(&self, u: f32) -> Vector3 {
        let [p0, c0, c1, p1] = self.controls;
        let v = 1.0 - u;
        add(
            add(scale(p0, v * v * v), scale(c0, 3.0 * v * v * u)),
            add(scale(c1, 3.0 * v * u * u), scale(p1, u * u * u)),
        )
    }

    /// The curve's direction of travel at `u` (not normalized).
    fn derivative(&self, u: f32) -> Vector3 {
        let [p0, c0, c1, p1] = self.controls;
        let v = 1.0 - u;
        add(
            add(
                scale(sub(c0, p0), 3.0 * v * v),
                scale(sub(c1, c0), 6.0 * v * u),
            ),
            scale(sub(p1, c1), 3.0 * u * u),
        )
    }
}

/// A spline's curve, ready to measure.
#[derive(Clone, Debug)]
pub struct SplineCurve {
    /// The first point, for a spline with one point (or several in the
    /// same place).
    first: Vector3,
    segments: Vec<Segment>,
    closed: bool,
    length: f32,
}

impl SplineCurve {
    /// The curve of the `Spline` `id`, or `None` when it isn't a Spline or
    /// has no points.
    pub fn of(world: &World, id: InstanceId) -> Option<Self> {
        let spline = world.get(id)?;
        if spline.class() != ClassName::Spline {
            return None;
        }
        let closed = spline.property("Closed") == Some(&Value::Bool(true));
        let points: Vec<ControlPoint> = spline
            .children()
            .iter()
            .filter_map(|&child| ControlPoint::of(world, child))
            .collect();
        Self::new(&points, closed)
    }

    /// The curve through `points`, or `None` with none.
    pub fn new(points: &[ControlPoint], closed: bool) -> Option<Self> {
        let first = points.first()?.position;
        let n = points.len();
        let count = match n {
            1 => 0,
            _ if closed => n,
            _ => n - 1,
        };
        let mut segments = Vec::with_capacity(count);
        let mut start = 0.0;
        for i in 0..count {
            let j = (i + 1) % n;
            let (a, b) = (points[i], points[j]);
            let controls = [
                a.position,
                add(a.position, handle(points, i, closed, true)),
                add(b.position, handle(points, j, closed, false)),
                b.position,
            ];
            let mut segment = Segment {
                controls,
                start,
                lengths: Vec::with_capacity(SAMPLES + 1),
            };
            let mut total = 0.0;
            let mut last = controls[0];
            segment.lengths.push(0.0);
            for s in 1..=SAMPLES {
                let here = segment.point(s as f32 / SAMPLES as f32);
                total += length(sub(here, last));
                segment.lengths.push(total);
                last = here;
            }
            start += total;
            segments.push(segment);
        }
        Some(Self {
            first,
            segments,
            closed,
            length: start,
        })
    }

    /// How long the curve is, in studs (`Length`).
    pub fn length(&self) -> f32 {
        self.length
    }

    pub fn closed(&self) -> bool {
        self.closed
    }

    /// `distance` brought onto the curve: wrapped round a closed one,
    /// clamped to the ends of an open one.
    pub fn wrap(&self, distance: f32) -> f32 {
        if !distance.is_finite() || self.length <= 0.0 {
            return 0.0;
        }
        if self.closed {
            distance.rem_euclid(self.length)
        } else {
            distance.clamp(0.0, self.length)
        }
    }

    /// The stretch and its curve parameter at `distance` along the curve.
    fn locate(&self, distance: f32) -> Option<(&Segment, f32)> {
        let distance = self.wrap(distance);
        let i = self
            .segments
            .partition_point(|segment| segment.start + segment.length() < distance)
            .min(self.segments.len().checked_sub(1)?);
        let segment = &self.segments[i];
        Some((segment, segment.parameter_at(distance - segment.start)))
    }

    /// The point `distance` studs along the curve.
    pub fn position_at_distance(&self, distance: f32) -> Vector3 {
        match self.locate(distance) {
            Some((segment, u)) => segment.point(u),
            None => self.first,
        }
    }

    /// The point a fraction `t` (0 at the start, 1 at the end) of the way
    /// along the curve, by distance (`GetPositionAt`).
    pub fn position_at(&self, t: f32) -> Vector3 {
        self.position_at_distance(t * self.length)
    }

    /// Which way the curve goes `distance` studs along it, at length 1.
    /// Forward (-Z) where it has no direction.
    pub fn direction_at_distance(&self, distance: f32) -> Vector3 {
        let Some((segment, u)) = self.locate(distance) else {
            return Vector3::new(0.0, 0.0, -1.0);
        };
        // A handle on its point makes the derivative zero there; a step
        // in finds the way the curve leaves it.
        let mut direction = segment.derivative(u);
        if length(direction) < 1e-5 {
            let nudged = if u < 0.5 { u + 1e-3 } else { u - 1e-3 };
            direction = segment.derivative(nudged);
        }
        if length(direction) < 1e-5 {
            direction = sub(segment.controls[3], segment.controls[0]);
        }
        normalize(direction).unwrap_or(Vector3::new(0.0, 0.0, -1.0))
    }

    /// The `Orientation` (degrees) facing along the curve `distance` studs
    /// along it, upright (`GetCFrameAt`'s rotation).
    pub fn orientation_at_distance(&self, distance: f32) -> Vector3 {
        facing(self.direction_at_distance(distance))
    }

    /// The distance along the curve of its point nearest `position`
    /// (`GetClosestDistance`).
    pub fn closest_distance(&self, position: Vector3) -> f32 {
        let mut best = (f32::INFINITY, 0.0);
        for segment in &self.segments {
            // The nearest sample, then a closer look either side of it.
            let mut near = (f32::INFINITY, 0.0);
            for s in 0..=SAMPLES {
                let u = s as f32 / SAMPLES as f32;
                let away = length(sub(segment.point(u), position));
                if away < near.0 {
                    near = (away, u);
                }
            }
            let step = 1.0 / SAMPLES as f32;
            let (mut low, mut high) = ((near.1 - step).max(0.0), (near.1 + step).min(1.0));
            // Where the way to `position` is square to the curve: the
            // slope of the distance changes sign there.
            let slope = |u: f32| dot(sub(segment.point(u), position), segment.derivative(u));
            let u = if slope(low) >= 0.0 {
                low
            } else if slope(high) <= 0.0 {
                high
            } else {
                for _ in 0..32 {
                    let middle = (low + high) / 2.0;
                    if slope(middle) < 0.0 {
                        low = middle;
                    } else {
                        high = middle;
                    }
                }
                (low + high) / 2.0
            };
            let away = length(sub(segment.point(u), position));
            if away < best.0 {
                best = (away, segment.start + segment.distance_at(u));
            }
        }
        best.1
    }

    /// Points along the curve no further than `spacing` studs apart, from
    /// its start to its end (back at the start when closed), for drawing.
    pub fn polyline(&self, spacing: f32) -> Vec<Vector3> {
        if self.segments.is_empty() {
            return vec![self.first];
        }
        let steps = ((self.length / spacing.max(0.01)).ceil() as usize).clamp(1, 4096);
        (0..=steps)
            .map(|i| {
                let distance = self.length * i as f32 / steps as f32;
                // The end of a closed curve is its start; wrapping would
                // jump there early, so stay on the last stretch.
                if i == steps {
                    let last = &self.segments[self.segments.len() - 1];
                    last.point(1.0)
                } else {
                    self.position_at_distance(distance)
                }
            })
            .collect()
    }
}

/// The handle of point `i`, from the point: its outgoing one when `out`.
fn handle(points: &[ControlPoint], i: usize, closed: bool, out: bool) -> Vector3 {
    let n = points.len();
    let point = points[i];
    let here = point.position;
    let prev = if i > 0 {
        Some(points[i - 1].position)
    } else if closed {
        Some(points[n - 1].position)
    } else {
        None
    };
    let next = if i + 1 < n {
        Some(points[i + 1].position)
    } else if closed {
        Some(points[0].position)
    } else {
        None
    };
    // The direction of travel through the point (a Hermite tangent); its
    // Bézier handle is a third of it.
    let tangent = match point.mode {
        TangentMode::Free => {
            return if out {
                point.out_tangent
            } else {
                point.in_tangent
            };
        }
        TangentMode::Auto => match (prev, next) {
            (Some(prev), Some(next)) => scale(sub(next, prev), 0.5),
            (None, Some(next)) => sub(next, here),
            (Some(prev), None) => sub(here, prev),
            (None, None) => Vector3::default(),
        },
        TangentMode::Linear => match (out, prev, next) {
            (true, _, Some(next)) => sub(next, here),
            (false, Some(prev), _) => sub(here, prev),
            (true, Some(prev), None) => sub(here, prev),
            (false, None, Some(next)) => sub(next, here),
            _ => Vector3::default(),
        },
    };
    scale(tangent, if out { 1.0 / 3.0 } else { -1.0 / 3.0 })
}

/// The upright `Orientation` (degrees) whose front (-Z) points along
/// `direction`: pitched up or down to it, turned round Y to it, never
/// rolled.
pub fn facing(direction: Vector3) -> Vector3 {
    let Some(d) = normalize(direction) else {
        return Vector3::default();
    };
    let pitch = d.y.clamp(-1.0, 1.0).asin().to_degrees();
    let yaw = if d.x.abs() < 1e-6 && d.z.abs() < 1e-6 {
        0.0
    } else {
        (-d.x).atan2(-d.z).to_degrees()
    };
    // `+ 0.0` turns -0.0 into 0.0.
    Vector3::new(pitch + 0.0, yaw + 0.0, 0.0)
}

/// A SplineMesh or SplineScatter (Phase 74), or a BuildingGenerator
/// (procedural worlds PW6): its children are what it made from its
/// settings (and its spline), made again whenever they change, and never
/// saved, sent to other devices or kept in a save slot.
pub fn is_generator(class: ClassName) -> bool {
    matches!(
        class,
        ClassName::SplineMesh | ClassName::SplineScatter | ClassName::BuildingGenerator
    )
}

/// The generator (see [`is_generator`]) that made `id`, if it's inside one.
pub fn generator_of(world: &World, id: InstanceId) -> Option<InstanceId> {
    let mut at = world.get(id)?.parent();
    while let Some(current) = at {
        let instance = world.get(current)?;
        if is_generator(instance.class()) {
            return Some(current);
        }
        at = instance.parent();
    }
    None
}

/// True when `id` was made by a generator (see [`is_generator`]).
pub fn is_generated(world: &World, id: InstanceId) -> bool {
    generator_of(world, id).is_some()
}

fn add(a: Vector3, b: Vector3) -> Vector3 {
    Vector3::new(a.x + b.x, a.y + b.y, a.z + b.z)
}

fn sub(a: Vector3, b: Vector3) -> Vector3 {
    Vector3::new(a.x - b.x, a.y - b.y, a.z - b.z)
}

fn scale(a: Vector3, s: f32) -> Vector3 {
    Vector3::new(a.x * s, a.y * s, a.z * s)
}

fn dot(a: Vector3, b: Vector3) -> f32 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

fn length(a: Vector3) -> f32 {
    (a.x * a.x + a.y * a.y + a.z * a.z).sqrt()
}

fn normalize(a: Vector3) -> Option<Vector3> {
    let l = length(a);
    (l > 1e-9 && l.is_finite()).then(|| scale(a, 1.0 / l))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Vector3, b: Vector3, within: f32) -> bool {
        length(sub(a, b)) <= within
    }

    fn line() -> SplineCurve {
        SplineCurve::new(
            &[
                ControlPoint::at(Vector3::new(0.0, 0.0, 0.0)),
                ControlPoint::at(Vector3::new(10.0, 0.0, 0.0)),
            ],
            false,
        )
        .unwrap()
    }

    /// An arch, the same on both sides of its middle point.
    fn arch() -> SplineCurve {
        SplineCurve::new(
            &[
                ControlPoint::at(Vector3::new(0.0, 0.0, 0.0)),
                ControlPoint::at(Vector3::new(5.0, 5.0, 0.0)),
                ControlPoint::at(Vector3::new(10.0, 0.0, 0.0)),
            ],
            false,
        )
        .unwrap()
    }

    #[test]
    fn two_auto_points_make_a_straight_line_of_their_distance() {
        let curve = line();
        assert!((curve.length() - 10.0).abs() < 1e-3);
        assert!(close(
            curve.position_at_distance(2.5),
            Vector3::new(2.5, 0.0, 0.0),
            1e-3
        ));
        // Facing +X is turned -90° round Y.
        let facing = curve.orientation_at_distance(5.0);
        assert!(
            close(facing, Vector3::new(0.0, -90.0, 0.0), 1e-3),
            "{facing:?}"
        );
    }

    #[test]
    fn half_the_length_along_a_symmetric_arch_is_its_middle_point() {
        let curve = arch();
        let middle = curve.position_at_distance(curve.length() / 2.0);
        assert!(
            close(middle, Vector3::new(5.0, 5.0, 0.0), 0.01),
            "{middle:?}"
        );
        assert!(close(curve.position_at(0.5), middle, 1e-4));
        // At the top it's going straight across.
        let direction = curve.direction_at_distance(curve.length() / 2.0);
        assert!(
            close(direction, Vector3::new(1.0, 0.0, 0.0), 1e-3),
            "{direction:?}"
        );
    }

    #[test]
    fn linear_points_join_with_straight_lines() {
        let linear = |x, z| ControlPoint {
            mode: TangentMode::Linear,
            ..ControlPoint::at(Vector3::new(x, 0.0, z))
        };
        let curve = SplineCurve::new(
            &[linear(0.0, 0.0), linear(10.0, 0.0), linear(10.0, 10.0)],
            false,
        )
        .unwrap();
        assert!((curve.length() - 20.0).abs() < 1e-3);
        assert!(close(
            curve.position_at_distance(15.0),
            Vector3::new(10.0, 0.0, 5.0),
            1e-3
        ));
    }

    #[test]
    fn a_closed_curve_wraps_distances_and_an_open_one_clamps_them() {
        let square: Vec<ControlPoint> = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]
            .into_iter()
            .map(|(x, z)| ControlPoint {
                mode: TangentMode::Linear,
                ..ControlPoint::at(Vector3::new(x, 0.0, z))
            })
            .collect();
        let closed = SplineCurve::new(&square, true).unwrap();
        assert!((closed.length() - 40.0).abs() < 1e-3);
        assert!(close(
            closed.position_at_distance(45.0),
            Vector3::new(5.0, 0.0, 0.0),
            1e-3
        ));
        let open = SplineCurve::new(&square, false).unwrap();
        assert!(close(
            open.position_at_distance(99.0),
            Vector3::new(0.0, 0.0, 10.0),
            1e-3
        ));
    }

    #[test]
    fn the_closest_distance_finds_the_nearest_point_on_the_curve() {
        let curve = line();
        let d = curve.closest_distance(Vector3::new(3.0, 4.0, 0.0));
        assert!((d - 3.0).abs() < 1e-3, "{d}");
        let arch = arch();
        let distance = arch.closest_distance(Vector3::new(5.0, 9.0, 0.0));
        assert!((distance - arch.length() / 2.0).abs() < 0.01);
    }

    #[test]
    fn free_handles_bend_the_curve_their_way() {
        let curve = SplineCurve::new(
            &[
                ControlPoint {
                    mode: TangentMode::Free,
                    out_tangent: Vector3::new(0.0, 0.0, -5.0),
                    ..ControlPoint::at(Vector3::new(0.0, 0.0, 0.0))
                },
                ControlPoint {
                    mode: TangentMode::Free,
                    in_tangent: Vector3::new(0.0, 0.0, -5.0),
                    ..ControlPoint::at(Vector3::new(10.0, 0.0, 0.0))
                },
            ],
            false,
        )
        .unwrap();
        // It leaves forward (-Z) and bows out that way.
        assert!(close(
            curve.direction_at_distance(0.0),
            Vector3::new(0.0, 0.0, -1.0),
            1e-3
        ));
        assert!(curve.position_at(0.5).z < -3.0);
    }

    #[test]
    fn one_point_is_a_curve_of_no_length_at_that_point() {
        let curve =
            SplineCurve::new(&[ControlPoint::at(Vector3::new(1.0, 2.0, 3.0))], true).unwrap();
        assert_eq!(curve.length(), 0.0);
        assert_eq!(curve.position_at(0.7), Vector3::new(1.0, 2.0, 3.0));
        assert!(SplineCurve::new(&[], false).is_none());
    }

    #[test]
    fn what_a_spline_mesh_or_scatter_holds_is_generated() {
        let mut world = World::new();
        let spline = world.create(ClassName::Spline).unwrap();
        world.set_parent(spline, Some(world.workspace())).unwrap();
        let scatter = world.create(ClassName::SplineScatter).unwrap();
        world.set_parent(scatter, Some(spline)).unwrap();
        let tree = world.create(ClassName::MeshPart).unwrap();
        world.set_parent(tree, Some(scatter)).unwrap();
        assert_eq!(generator_of(&world, tree), Some(scatter));
        assert!(is_generated(&world, tree));
        assert!(!is_generated(&world, scatter));
        assert!(!is_generated(&world, spline));
        // The curve's points are only the SplinePoints.
        assert!(SplineCurve::of(&world, spline).is_none());
    }
}
