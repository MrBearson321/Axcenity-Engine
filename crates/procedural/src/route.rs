//! Route (procedural worlds PW4): a road's points laid over the terrain
//! between two ends so no stretch climbs steeper than a grade, by A* over a
//! grid on the heightfield. Up a hill steeper than the grade the cheapest
//! way is across the slope, so the road climbs in switchbacks.
//!
//! The grid is `step` studs across (a sample, or half the road's width,
//! coarser on long routes so it stays at most [`MAX_NODES`] across), over
//! the box round both ends, grown by their distance apart (96 studs at
//! least) so a road can wind. There are sixteen
//! moves (to the eight nodes round one and the eight a knight's move
//! away), and each move may turn at most one of them from the last (26.6
//! degrees), so a hairpin takes several moves and is about as wide as the
//! road. The road follows the ground as far as the grade lets it, and may
//! be cut into it or built up over it by at most half its width
//! ([`CUT_SHARE`]; Apply then flattens the ground to it). A move costs its length with the
//! climb, a step for each turn (so legs run straight), and twice how far
//! the road is from the ground at its end and in its middle (cutting
//! through a ridge or bridging a dip costs more).
//! The nodes found are then thinned to points at most [`MAX_STRETCH`]
//! studs apart, keeping every turn and every stretch under the grade.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use axcenity_core::TerrainData;
use axcenity_terrain::Placement;
use glam::{Vec2, Vec3};

/// Grid nodes along the longer side of a route's box, at most.
pub const MAX_NODES: usize = 384;
/// Studs between two of a route's points, at most.
pub const MAX_STRETCH: f32 = 40.0;
/// How far a point between two kept ones may be off their straight line
/// (seen from above, or in height) for it to be left out, in studs.
const TOLERANCE: f32 = 1.5;
/// How far the road may be cut into the ground or built up over it where
/// the ground is steeper than the grade (in a hairpin, or across a bump),
/// as a share of the road's width, and at least a stud.
pub const CUT_SHARE: f32 = 0.5;

/// The sixteen moves, round the circle in order, so turning one place
/// along the list is turning at most 26.6 degrees.
const MOVES: [(i32, i32); 16] = [
    (1, 0),
    (2, 1),
    (1, 1),
    (1, 2),
    (0, 1),
    (-1, 2),
    (-1, 1),
    (-2, 1),
    (-1, 0),
    (-2, -1),
    (-1, -1),
    (-1, -2),
    (0, -1),
    (1, -2),
    (1, -1),
    (2, -1),
];
const DIRECTIONS: usize = MOVES.len();

/// Points from `from` to `to` (world positions; only X and Z are used)
/// over the terrain, each on the ground or within half `width` of it (the
/// first on it), so that no stretch between two climbs more than
/// `max_grade` degrees, for a road `width` studs wide
/// (its turns are no tighter than about its width). `None` when the ends
/// are off the terrain or there's no way within the grade.
pub fn route(
    data: &TerrainData,
    placement: &Placement,
    (from, to): (Vec3, Vec3),
    max_grade: f32,
    width: f32,
) -> Option<Vec<Vec3>> {
    let grade = max_grade.clamp(0.5, 60.0).to_radians().tan();
    let ground = |at: Vec2| placement.ground_y(data, at.x, at.y);
    let (start, end) = (Vec2::new(from.x, from.z), Vec2::new(to.x, to.z));
    let (start_y, end_y) = (ground(start)?, ground(end)?);
    // The terrain's footprint, and the box the route may wind in.
    let corner = placement.to_world_xz(data, Vec2::ZERO);
    let (size_x, size_z) = data.size();
    let far = placement.to_world_xz(data, Vec2::new((size_x - 1) as f32, (size_z - 1) as f32));
    let span = start.distance(end);
    let grow = span.max(96.0);
    let min = (start.min(end) - Vec2::splat(grow)).max(corner);
    let max = (start.max(end) + Vec2::splat(grow)).min(far);
    let cell = placement.cell_size(data).max_element();
    let step = cell
        .max(width.max(0.0) / 2.0)
        .max((max - min).max_element() / MAX_NODES as f32);
    // Lined up so the start is a node.
    let min = start - ((start - min) / step).floor() * step;
    let columns = ((max.x - min.x) / step).floor() as i32 + 1;
    let rows = ((max.y - min.y) / step).floor() as i32 + 1;
    let at = |i: i32, j: i32| min + Vec2::new(i as f32, j as f32) * step;
    let node = |p: Vec2| {
        let g = ((p - min) / step).round();
        (
            (g.x as i32).clamp(0, columns - 1),
            (g.y as i32).clamp(0, rows - 1),
        )
    };
    let heights: Vec<f32> = (0..rows)
        .flat_map(|j| (0..columns).map(move |i| (i, j)))
        .map(|(i, j)| ground(at(i, j)).unwrap_or(f32::NAN))
        .collect();
    let index = |i: i32, j: i32| (j * columns + i) as usize;
    let (first, last) = (node(start), node(end));
    let goal = at(last.0, last.1);
    let max_cut = (width * CUT_SHARE).max(1.0);
    // Each turn of one place along MOVES costs this, so legs run straight.
    let turn_cost = step;

    // A state is a node and the move that came into it, so a road only
    // turns a little at a time: one place along MOVES per move. Each
    // state's road height is its cheapest way's, kept within `max_cut` of
    // the ground.
    let states = heights.len() * DIRECTIONS;
    let mut best = vec![f32::INFINITY; states];
    let mut road = vec![f32::NAN; states];
    let mut came = vec![u32::MAX; states];
    let mut open = BinaryHeap::new();
    let first_node = index(first.0, first.1);
    for direction in 0..DIRECTIONS {
        let state = first_node * DIRECTIONS + direction;
        best[state] = 0.0;
        road[state] = heights[first_node];
        open.push(Open {
            estimate: at(first.0, first.1).distance(goal),
            state: state as u32,
        });
    }
    let mut reached = None;
    while let Some(Open { estimate, state }) = open.pop() {
        let (node, direction) = (state as usize / DIRECTIONS, state as usize % DIRECTIONS);
        let (i, j) = (node as i32 % columns, node as i32 / columns);
        let here = at(i, j);
        let cost = best[state as usize];
        if estimate > cost + here.distance(goal) + 1e-3 {
            continue; // Found a cheaper way here since.
        }
        if (i, j) == last {
            reached = Some(state);
            break;
        }
        let h = road[state as usize];
        for turn in [DIRECTIONS - 1, 0, 1] {
            let next_direction = (direction + turn) % DIRECTIONS;
            let (di, dj) = MOVES[next_direction];
            let (ni, nj) = (i + di, j + dj);
            if ni < 0 || nj < 0 || ni >= columns || nj >= rows {
                continue;
            }
            let next = index(ni, nj);
            let below = heights[next];
            if below.is_nan() {
                continue;
            }
            let there = at(ni, nj);
            let length = here.distance(there);
            // The road follows the ground as far as the grade lets it.
            let rise = grade * length;
            let nh = below.clamp(h - rise, h + rise);
            let dug = (nh - below).abs();
            if dug > max_cut {
                continue;
            }
            let middle = ground((here + there) / 2.0).unwrap_or(below);
            let cut = (middle - (h + nh) / 2.0).abs();
            let climb = nh - h;
            let turned = if turn == 0 { 0.0 } else { turn_cost };
            let total =
                cost + (length * length + climb * climb).sqrt() + 2.0 * (cut + dug) + turned;
            let next_state = next * DIRECTIONS + next_direction;
            if total < best[next_state] {
                best[next_state] = total;
                road[next_state] = nh;
                came[next_state] = state;
                open.push(Open {
                    estimate: total + there.distance(goal),
                    state: next_state as u32,
                });
            }
        }
    }
    // Back from the end, then in order.
    let mut chain = vec![reached?];
    while let Some(&state) = chain.last() {
        let previous = came[state as usize];
        if previous == u32::MAX {
            break;
        }
        chain.push(previous);
    }
    chain.reverse();
    let mut points: Vec<Vec3> = chain
        .iter()
        .map(|&state| {
            let node = state as usize / DIRECTIONS;
            let p = at(node as i32 % columns, node as i32 / columns);
            Vec3::new(p.x, road[state as usize], p.y)
        })
        .collect();
    // The ends where they were asked for, not at the nearest node: the
    // start is a node; the end takes the last node's place, its height
    // kept within the grade of the point before it.
    let start_point = Vec3::new(start.x, start_y, start.y);
    let last = if points.len() > 1 {
        points.pop().map_or(end_y, |p| p.y)
    } else {
        end_y
    };
    points[0] = start_point;
    let before = points[points.len() - 1];
    let rise = grade * Vec2::new(before.x, before.z).distance(end);
    points.push(Vec3::new(
        end.x,
        last.clamp(before.y - rise, before.y + rise),
        end.y,
    ));
    Some(thin(&points, grade, max_cut, &ground))
}

/// `points` with those in between left out where the straight stretch
/// past them stays under `grade` (a rise per stud), within [`TOLERANCE`]
/// of them and of the ground, and at most [`MAX_STRETCH`] long. Each
/// stretch of the path itself is always kept.
fn thin(
    points: &[Vec3],
    grade: f32,
    max_cut: f32,
    ground: &impl Fn(Vec2) -> Option<f32>,
) -> Vec<Vec3> {
    let flat = |p: Vec3| Vec2::new(p.x, p.z);
    let fits = |a: usize, b: usize| {
        let (pa, pb) = (points[a], points[b]);
        let run = flat(pa).distance(flat(pb));
        if run > MAX_STRETCH || (pb.y - pa.y).abs() > grade * run + 1e-3 {
            return false;
        }
        let line = |t: f32| pa.lerp(pb, t);
        let between = points[a + 1..b].iter().all(|&p| {
            let along = flat(pb) - flat(pa);
            let t = if along.length_squared() > 1e-9 {
                ((flat(p) - flat(pa)).dot(along) / along.length_squared()).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let on = line(t);
            flat(on).distance(flat(p)) <= TOLERANCE && (on.y - p.y).abs() <= TOLERANCE
        });
        // The ground under the stretch stays near it too, every few studs.
        let steps = (run / 4.0).ceil() as usize;
        let ground_near = (1..steps).all(|s| {
            let on = line(s as f32 / steps as f32);
            ground(flat(on)).is_none_or(|y| (y - on.y).abs() <= max_cut + TOLERANCE)
        });
        between && ground_near
    };
    let mut kept = vec![points[0]];
    let mut a = 0;
    while a + 1 < points.len() {
        let mut b = a + 1;
        while b + 1 < points.len() && fits(a, b + 1) {
            b += 1;
        }
        kept.push(points[b]);
        a = b;
    }
    kept
}

/// A node to look at, cheapest estimate first.
#[derive(Clone, Copy, PartialEq)]
struct Open {
    estimate: f32,
    state: u32,
}

impl Eq for Open {}

impl Ord for Open {
    fn cmp(&self, other: &Self) -> Ordering {
        // BinaryHeap pops the greatest: the cheapest is the greatest here.
        other
            .estimate
            .total_cmp(&self.estimate)
            .then_with(|| other.state.cmp(&self.state))
    }
}

impl PartialOrd for Open {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
