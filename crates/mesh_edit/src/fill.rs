//! Filling flat outlines with triangles: a cut's cap, and a flat face
//! filled again after its needless corners are left out (`merge`).
//!
//! Edges are joined into loops of point ids, the loops are flattened onto
//! their plane, and ear clipping cuts each outline into triangles, after
//! joining any holes in it to it with a bridge.

use std::collections::HashMap;

use glam::DVec2;

/// Corners whose triangle is smaller than this fraction of the outline's
/// size squared count as in line. The flattened points are f64, so only
/// corners really in line are caught; nearly in line ones make thin but
/// correct triangles (`merge` leaves most of those corners out first).
const FLAT_AREA: f64 = 1e-12;

/// Joins `edges` (pairs of point ids) end to start into closed loops, each
/// listed by its corners. An edge that doesn't close a loop is left out.
pub(crate) fn chain(edges: &[(u32, u32)]) -> Vec<Vec<u32>> {
    let mut leaving: HashMap<u32, Vec<usize>> = HashMap::new();
    for (index, &(a, _)) in edges.iter().enumerate() {
        leaving.entry(a).or_default().push(index);
    }
    let mut used = vec![false; edges.len()];
    let mut loops = Vec::new();
    for first in 0..edges.len() {
        if used[first] {
            continue;
        }
        used[first] = true;
        let (start, mut at) = edges[first];
        let mut lap = vec![start];
        while at != start {
            let next = leaving
                .get(&at)
                .and_then(|out| out.iter().copied().find(|&edge| !used[edge]));
            let Some(next) = next else {
                break;
            };
            used[next] = true;
            lap.push(at);
            at = edges[next].1;
        }
        if at == start && lap.len() >= 3 {
            loops.push(lap);
        }
    }
    loops
}

/// Triangles covering the polygons `loops` (indices into `points`) make:
/// counter-clockwise loops are outlines and clockwise ones holes in the
/// outline around them. The triangles are counter-clockwise.
pub(crate) fn fill(points: &[DVec2], loops: Vec<Vec<u32>>) -> Vec<[u32; 3]> {
    let (min, max) = loops
        .iter()
        .flatten()
        .map(|&point| points[point as usize])
        .fold((DVec2::INFINITY, DVec2::NEG_INFINITY), |(min, max), p| {
            (min.min(p), max.max(p))
        });
    let size = (max - min).max_element();
    if size.is_nan() || size <= 0.0 {
        return Vec::new();
    }
    let flat = FLAT_AREA * size * size;
    let polygon = Polygon { points, flat };

    let (mut outlines, mut holes) = (Vec::new(), Vec::new());
    for lap in loops {
        let area = polygon.area(&lap);
        if area >= 0.0 {
            outlines.push((area, lap, Vec::new()));
        } else {
            holes.push(lap);
        }
    }
    // Each hole belongs to the smallest outline around it. One outside
    // every outline can't be filled, and is left open.
    for hole in holes {
        let inside = points[hole[0] as usize];
        let around = outlines
            .iter_mut()
            .filter(|(_, outline, _)| polygon.contains(outline, inside))
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, _, its_holes)) = around {
            its_holes.push(hole);
        }
    }

    let mut triangles = Vec::new();
    for (_, outline, mut its_holes) in outlines {
        // Rightmost holes first, so each bridge can reach the outline
        // without crossing a hole not joined yet.
        its_holes.sort_by(|a, b| polygon.right(b).total_cmp(&polygon.right(a)));
        let mut ring = outline;
        for k in 0..its_holes.len() {
            ring = polygon.bridge(ring, &its_holes[k], &its_holes[k + 1..]);
        }
        polygon.ear_clip(ring, &mut triangles);
    }
    polygon.split_flat(&mut triangles);
    triangles
}

/// True if `triangles` have exactly `loops` as their outline: every other
/// edge is used once each way, so they close the opening and nothing else.
pub(crate) fn has_outline(triangles: &[[u32; 3]], loops: &[Vec<u32>]) -> bool {
    let mut surplus: HashMap<(u32, u32), i32> = HashMap::new();
    let mut count = |a: u32, b: u32, by: i32| {
        if a != b {
            *surplus.entry((a, b)).or_default() += by;
            *surplus.entry((b, a)).or_default() -= by;
        }
    };
    for &[a, b, c] in triangles {
        count(a, b, 1);
        count(b, c, 1);
        count(c, a, 1);
    }
    for lap in loops {
        for (k, &a) in lap.iter().enumerate() {
            count(a, lap[(k + 1) % lap.len()], -1);
        }
    }
    surplus.values().all(|&n| n == 0)
}

/// Flat geometry on the cap's points.
struct Polygon<'a> {
    points: &'a [DVec2],
    /// Areas this small count as zero.
    flat: f64,
}

impl Polygon<'_> {
    fn at(&self, point: u32) -> DVec2 {
        self.points[point as usize]
    }

    /// Twice the signed area of the triangle a, b, c: positive when it
    /// turns left (counter-clockwise).
    fn turn(&self, a: u32, b: u32, c: u32) -> f64 {
        let (a, b, c) = (self.at(a), self.at(b), self.at(c));
        (b - a).perp_dot(c - a)
    }

    /// The signed area inside `ring`: positive when counter-clockwise.
    fn area(&self, ring: &[u32]) -> f64 {
        let mut sum = 0.0;
        for (k, &a) in ring.iter().enumerate() {
            let b = ring[(k + 1) % ring.len()];
            sum += self.at(a).perp_dot(self.at(b));
        }
        sum / 2.0
    }

    /// The largest x of `ring`'s corners.
    fn right(&self, ring: &[u32]) -> f64 {
        ring.iter()
            .map(|&point| self.at(point).x)
            .fold(f64::NEG_INFINITY, f64::max)
    }

    /// True if `p` is inside `ring` (counting crossings of a ray to +x).
    fn contains(&self, ring: &[u32], p: DVec2) -> bool {
        let mut inside = false;
        for (k, &a) in ring.iter().enumerate() {
            let (a, b) = (self.at(a), self.at(ring[(k + 1) % ring.len()]));
            if (a.y > p.y) != (b.y > p.y) {
                let x = a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x);
                if p.x < x {
                    inside = !inside;
                }
            }
        }
        inside
    }

    /// `ring` with `hole` spliced in by a bridge (an edge there and back)
    /// from the hole's rightmost corner to the nearest corner of `ring` it
    /// can see. `later` are holes still to be joined, which the bridge
    /// mustn't cross either.
    fn bridge(&self, ring: Vec<u32>, hole: &[u32], later: &[Vec<u32>]) -> Vec<u32> {
        let from = (0..hole.len())
            .max_by(|&a, &b| {
                // `max_by` keeps the last of equals; prefer the first.
                self.at(hole[a])
                    .x
                    .total_cmp(&self.at(hole[b]).x)
                    .then(b.cmp(&a))
            })
            .unwrap_or(0);
        let m = hole[from];
        let mut candidates: Vec<usize> = (0..ring.len()).collect();
        candidates.sort_by(|&a, &b| {
            let da = self.at(ring[a]).distance_squared(self.at(m));
            let db = self.at(ring[b]).distance_squared(self.at(m));
            da.total_cmp(&db).then(a.cmp(&b))
        });
        let rings = || {
            std::iter::once(ring.as_slice())
                .chain(std::iter::once(hole))
                .chain(later.iter().map(Vec::as_slice))
        };
        let to = candidates
            .iter()
            .copied()
            .find(|&k| {
                self.opens_toward(&ring, k, self.at(m))
                    && !rings().any(|other| self.crosses(other, ring[k], m))
            })
            .unwrap_or(candidates[0]);

        let mut joined = Vec::with_capacity(ring.len() + hole.len() + 2);
        joined.extend_from_slice(&ring[..=to]);
        joined.extend((0..=hole.len()).map(|k| hole[(from + k) % hole.len()]));
        joined.push(ring[to]);
        joined.extend_from_slice(&ring[to + 1..]);
        joined
    }

    /// True if the inside of counter-clockwise `ring` at its corner `k`
    /// opens toward `p`, so a bridge from there to `p` starts inside.
    fn opens_toward(&self, ring: &[u32], k: usize, p: DVec2) -> bool {
        let n = ring.len();
        let (prev, corner, next) = (
            self.at(ring[(k + n - 1) % n]),
            self.at(ring[k]),
            self.at(ring[(k + 1) % n]),
        );
        let (back, ahead, toward) = (prev - corner, next - corner, p - corner);
        if (corner - prev).perp_dot(next - corner) >= 0.0 {
            // A convex corner: the inside is between `ahead` and `back`.
            ahead.perp_dot(toward) >= 0.0 && toward.perp_dot(back) >= 0.0
        } else {
            // A reflex corner: everything but the outside wedge.
            !(back.perp_dot(toward) > 0.0 && toward.perp_dot(ahead) > 0.0)
        }
    }

    /// True if the segment from `a` to `b` crosses an edge of `ring`
    /// (touching at a shared end doesn't count).
    fn crosses(&self, ring: &[u32], a: u32, b: u32) -> bool {
        let (pa, pb) = (self.at(a), self.at(b));
        (0..ring.len()).any(|k| {
            let (c, d) = (self.at(ring[k]), self.at(ring[(k + 1) % ring.len()]));
            if c == pa || c == pb || d == pa || d == pb {
                return false;
            }
            let side = |p: DVec2, q: DVec2, r: DVec2| (q - p).perp_dot(r - p);
            let (s1, s2) = (side(pa, pb, c), side(pa, pb, d));
            let (s3, s4) = (side(c, d, pa), side(c, d, pb));
            s1 * s2 < 0.0 && s3 * s4 < 0.0
        })
    }

    /// True if no other corner of `ring` is inside or on the triangle a, b, c.
    fn empty(&self, ring: &[u32], [a, b, c]: [u32; 3]) -> bool {
        let corners = [self.at(a), self.at(b), self.at(c)];
        ring.iter().all(|&point| {
            let p = self.at(point);
            if corners.contains(&p) {
                return true;
            }
            let [pa, pb, pc] = corners;
            let inside = (pb - pa).perp_dot(p - pa) >= -self.flat
                && (pc - pb).perp_dot(p - pb) >= -self.flat
                && (pa - pc).perp_dot(p - pc) >= -self.flat;
            !inside
        })
    }

    /// Replaces each flat triangle (a corner `m` on the line between the
    /// other two, `a` and `b`) and its neighbour across `a`–`b` with two
    /// triangles from `m` to the neighbour's far corner. They cover the same
    /// area with the same outline, without a triangle of no area, whose
    /// edges would lie on top of other edges.
    fn split_flat(&self, triangles: &mut [[u32; 3]]) {
        let has_edge =
            |t: &[u32; 3], a: u32, b: u32| (0..3).any(|k| t[k] == a && t[(k + 1) % 3] == b);
        let mut k = 0;
        // Each split can make another flat triangle; this many always ends.
        let mut splits = 0;
        while k < triangles.len() && splits <= 4 * triangles.len() {
            let [p, q, r] = triangles[k];
            let middle = [[p, q, r], [q, r, p], [r, p, q]]
                .into_iter()
                .find(|&[a, m, b]| self.between(a, m, b));
            let flat = self.turn(p, q, r).abs() <= self.flat;
            let neighbour = middle.filter(|_| flat).and_then(|[a, m, b]| {
                let j = triangles.iter().position(|t| has_edge(t, a, b))?;
                Some((j, a, m, b))
            });
            let Some((j, a, m, b)) = neighbour else {
                k += 1;
                continue;
            };
            let far = triangles[j]
                .into_iter()
                .find(|&corner| corner != a && corner != b)
                .unwrap_or(m);
            triangles[j] = [a, m, far];
            triangles[k] = [m, b, far];
            splits += 1;
            k = 0;
        }
    }

    /// True if `m` is on the segment from `a` to `b`, not at either end.
    fn between(&self, a: u32, m: u32, b: u32) -> bool {
        let (a, m, b) = (self.at(a), self.at(m), self.at(b));
        (m - a).dot(b - a) > 0.0 && (m - b).dot(a - b) > 0.0
    }

    /// Cuts counter-clockwise `ring` into triangles by clipping ears (a
    /// corner whose triangle with its neighbours is inside the polygon and
    /// holds no other corner) until one triangle is left.
    fn ear_clip(&self, mut ring: Vec<u32>, triangles: &mut Vec<[u32; 3]>) {
        let mut start = 0;
        while ring.len() > 3 {
            let n = ring.len();
            let corners = |k: usize| [ring[(k + n - 1) % n], ring[k], ring[(k + 1) % n]];
            let order = || (0..n).map(|k| (start + k) % n);
            let turn = |k: usize| {
                let [a, b, c] = corners(k);
                self.turn(a, b, c)
            };
            // A proper ear; else a corner in line with its neighbours,
            // whose triangle is flat; else the least bad corner, so a
            // broken outline still finishes.
            let ear = order()
                .find(|&k| turn(k) > self.flat && self.empty(&ring, corners(k)))
                .or_else(|| order().find(|&k| turn(k).abs() <= self.flat))
                .unwrap_or_else(|| {
                    order()
                        .max_by(|&a, &b| turn(a).total_cmp(&turn(b)))
                        .unwrap_or(0)
                });
            triangles.push(corners(ear));
            ring.remove(ear);
            start = ear.saturating_sub(1);
        }
        if let [a, b, c] = ring[..] {
            triangles.push([a, b, c]);
        }
    }
}
