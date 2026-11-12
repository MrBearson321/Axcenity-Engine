//! A bounding volume hierarchy over a mesh's triangles: boxes in boxes,
//! so a ray or a brush's sphere only looks at the triangles near it (a
//! sculpt's brushes, and a [`crate::Reference`]'s rays for baking and
//! rigging's bone heat).
//!
//! Each node covers a run of `order` (the triangles sorted so every
//! node's are together), split at the median along its longest side. A
//! node's children come after it in `nodes`, so going through the nodes
//! backwards visits children before their parents (how `refit` works).

use glam::Vec3;

/// Triangles in a leaf at most.
const LEAF: usize = 8;

#[derive(Clone, Copy, Debug)]
struct Node {
    min: Vec3,
    max: Vec3,
    /// The node's triangles are `order[start..start + count]`.
    start: u32,
    count: u32,
    /// The first child (the second is next to it), or 0 for a leaf.
    left: u32,
    parent: u32,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Bvh {
    nodes: Vec<Node>,
    /// Triangle numbers, each node's together.
    pub(crate) order: Vec<u32>,
    /// The leaf holding each triangle.
    leaf_of: Vec<u32>,
    /// Scratch for `refit`: nodes already marked.
    marked: Vec<bool>,
}

impl Bvh {
    pub(crate) fn build(positions: &[Vec3], triangles: &[[u32; 3]]) -> Bvh {
        let centers: Vec<Vec3> = triangles
            .iter()
            .map(|t| t.iter().map(|&v| positions[v as usize]).sum::<Vec3>() / 3.0)
            .collect();
        let mut bvh = Bvh {
            nodes: Vec::with_capacity(triangles.len() / LEAF * 2 + 1),
            order: (0..triangles.len() as u32).collect(),
            leaf_of: vec![0; triangles.len()],
            marked: Vec::new(),
        };
        bvh.nodes.push(Node {
            min: Vec3::ZERO,
            max: Vec3::ZERO,
            start: 0,
            count: triangles.len() as u32,
            left: 0,
            parent: 0,
        });
        let mut stack = vec![0usize];
        while let Some(index) = stack.pop() {
            let Node { start, count, .. } = bvh.nodes[index];
            let (start, count) = (start as usize, count as usize);
            if count <= LEAF {
                for &triangle in &bvh.order[start..start + count] {
                    bvh.leaf_of[triangle as usize] = index as u32;
                }
                continue;
            }
            let run = &mut bvh.order[start..start + count];
            let (low, high) = run.iter().fold(
                (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
                |(low, high), &t| (low.min(centers[t as usize]), high.max(centers[t as usize])),
            );
            let extent = high - low;
            let axis = if extent.x >= extent.y && extent.x >= extent.z {
                0
            } else if extent.y >= extent.z {
                1
            } else {
                2
            };
            let half = count / 2;
            run.select_nth_unstable_by(half, |&a, &b| {
                centers[a as usize][axis]
                    .total_cmp(&centers[b as usize][axis])
                    .then(a.cmp(&b))
            });
            let left = bvh.nodes.len();
            bvh.nodes[index].left = left as u32;
            for (start, count) in [(start, half), (start + half, count - half)] {
                bvh.nodes.push(Node {
                    min: Vec3::ZERO,
                    max: Vec3::ZERO,
                    start: start as u32,
                    count: count as u32,
                    left: 0,
                    parent: index as u32,
                });
            }
            stack.push(left + 1);
            stack.push(left);
        }
        for index in (0..bvh.nodes.len()).rev() {
            bvh.fit(index, positions, triangles);
        }
        bvh.marked = vec![false; bvh.nodes.len()];
        bvh
    }

    /// The node's box from its triangles (a leaf) or its children.
    fn fit(&mut self, index: usize, positions: &[Vec3], triangles: &[[u32; 3]]) {
        let node = self.nodes[index];
        let (min, max) = if node.left == 0 {
            let run = &self.order[node.start as usize..(node.start + node.count) as usize];
            run.iter()
                .flat_map(|&t| triangles[t as usize])
                .map(|v| positions[v as usize])
                .fold(
                    (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
                    |(min, max), p| (min.min(p), max.max(p)),
                )
        } else {
            let (a, b) = (
                self.nodes[node.left as usize],
                self.nodes[node.left as usize + 1],
            );
            (a.min.min(b.min), a.max.max(b.max))
        };
        self.nodes[index].min = min;
        self.nodes[index].max = max;
    }

    /// Fits the boxes again round `triangles` (which moved) and every box
    /// holding them.
    pub(crate) fn refit(
        &mut self,
        changed: impl IntoIterator<Item = u32>,
        positions: &[Vec3],
        triangles: &[[u32; 3]],
    ) {
        let mut nodes = Vec::new();
        for triangle in changed {
            let mut node = self.leaf_of[triangle as usize] as usize;
            while !self.marked[node] {
                self.marked[node] = true;
                nodes.push(node);
                if node == 0 {
                    break;
                }
                node = self.nodes[node].parent as usize;
            }
        }
        // Children have larger numbers than their parents.
        nodes.sort_unstable_by(|a, b| b.cmp(a));
        for &node in &nodes {
            self.fit(node, positions, triangles);
            self.marked[node] = false;
        }
    }

    /// The nearest triangle `origin + t·direction` hits (from either
    /// side, t > 0), passing over those `skip` says, and its t.
    pub(crate) fn ray(
        &self,
        positions: &[Vec3],
        triangles: &[[u32; 3]],
        origin: Vec3,
        direction: Vec3,
        skip: impl Fn(u32) -> bool,
    ) -> Option<(u32, f32)> {
        if self.order.is_empty() {
            return None;
        }
        let inverse = direction.recip();
        let mut best: Option<(u32, f32)> = None;
        let mut stack = vec![0usize];
        while let Some(index) = stack.pop() {
            let node = &self.nodes[index];
            let Some(near) = slab(node.min, node.max, origin, inverse) else {
                continue;
            };
            if best.is_some_and(|(_, t)| near > t) {
                continue;
            }
            if node.left != 0 {
                stack.push(node.left as usize + 1);
                stack.push(node.left as usize);
                continue;
            }
            for &triangle in &self.order[node.start as usize..(node.start + node.count) as usize] {
                if skip(triangle) {
                    continue;
                }
                let corners = triangles[triangle as usize].map(|v| positions[v as usize]);
                if let Some(t) = hit(origin, direction, corners)
                    && best.is_none_or(|(_, best)| t < best)
                {
                    best = Some((triangle, t));
                }
            }
        }
        best
    }

    /// True if the ray hits any triangle closer than `limit` (a ray
    /// toward the sky, for ambient occlusion).
    pub(crate) fn occluded(
        &self,
        positions: &[Vec3],
        triangles: &[[u32; 3]],
        origin: Vec3,
        direction: Vec3,
        limit: f32,
    ) -> bool {
        if self.order.is_empty() {
            return false;
        }
        let inverse = direction.recip();
        let mut stack = vec![0usize];
        while let Some(index) = stack.pop() {
            let node = &self.nodes[index];
            match slab(node.min, node.max, origin, inverse) {
                Some(near) if near < limit => {}
                _ => continue,
            }
            if node.left != 0 {
                stack.push(node.left as usize + 1);
                stack.push(node.left as usize);
                continue;
            }
            for &triangle in &self.order[node.start as usize..(node.start + node.count) as usize] {
                let corners = triangles[triangle as usize].map(|v| positions[v as usize]);
                if hit(origin, direction, corners).is_some_and(|t| t < limit) {
                    return true;
                }
            }
        }
        false
    }

    /// The triangle nearest `point`, the nearest point on it and that
    /// point's barycentric weights.
    pub(crate) fn nearest(
        &self,
        positions: &[Vec3],
        triangles: &[[u32; 3]],
        point: Vec3,
    ) -> Option<(u32, Vec3, [f32; 3])> {
        if self.order.is_empty() {
            return None;
        }
        let mut best: Option<(u32, Vec3, [f32; 3], f32)> = None;
        let mut stack = vec![0usize];
        let box_distance = |node: &Node| point.clamp(node.min, node.max).distance_squared(point);
        while let Some(index) = stack.pop() {
            let node = &self.nodes[index];
            if best.is_some_and(|(.., d)| box_distance(node) >= d) {
                continue;
            }
            if node.left != 0 {
                // The nearer child first (pushed last), so the other is
                // more often passed over.
                let (a, b) = (node.left as usize, node.left as usize + 1);
                if box_distance(&self.nodes[a]) <= box_distance(&self.nodes[b]) {
                    stack.push(b);
                    stack.push(a);
                } else {
                    stack.push(a);
                    stack.push(b);
                }
                continue;
            }
            for &triangle in &self.order[node.start as usize..(node.start + node.count) as usize] {
                let corners = triangles[triangle as usize].map(|v| positions[v as usize]);
                let (on, weights) = closest_on_triangle(point, corners);
                let distance = on.distance_squared(point);
                if best.is_none_or(|(.., d)| distance < d) {
                    best = Some((triangle, on, weights, distance));
                }
            }
        }
        best.map(|(triangle, on, weights, _)| (triangle, on, weights))
    }

    /// Adds to `out` every triangle in a leaf whose box comes within
    /// `radius` of `center`.
    pub(crate) fn sphere(&self, center: Vec3, radius: f32, out: &mut Vec<u32>) {
        if self.order.is_empty() {
            return;
        }
        let mut stack = vec![0usize];
        while let Some(index) = stack.pop() {
            let node = &self.nodes[index];
            let nearest = center.clamp(node.min, node.max);
            if nearest.distance_squared(center) > radius * radius {
                continue;
            }
            if node.left != 0 {
                stack.push(node.left as usize + 1);
                stack.push(node.left as usize);
            } else {
                out.extend_from_slice(
                    &self.order[node.start as usize..(node.start + node.count) as usize],
                );
            }
        }
    }

    /// The runs of `order` with at most `size` triangles that make up the
    /// whole, each a node's (so its triangles are near each other).
    pub(crate) fn runs(&self, size: usize) -> Vec<std::ops::Range<usize>> {
        let mut runs = Vec::new();
        if self.order.is_empty() {
            return runs;
        }
        let mut stack = vec![0usize];
        while let Some(index) = stack.pop() {
            let node = &self.nodes[index];
            if node.count as usize <= size || node.left == 0 {
                runs.push(node.start as usize..(node.start + node.count) as usize);
            } else {
                stack.push(node.left as usize + 1);
                stack.push(node.left as usize);
            }
        }
        runs
    }
}

/// Where a ray (with `inverse` its direction's reciprocal) enters the box,
/// if it does ahead of its origin.
fn slab(min: Vec3, max: Vec3, origin: Vec3, inverse: Vec3) -> Option<f32> {
    let a = (min - origin) * inverse;
    let b = (max - origin) * inverse;
    // NaN (0 × ∞, a ray along a box's side) is dropped by min and max.
    let near = a.min(b).max_element();
    let far = a.max(b).min_element();
    (far >= near.max(0.0)).then_some(near.max(0.0))
}

/// The point of the triangle `a b c` nearest `p`, and its barycentric
/// weights (Ericson, *Real-Time Collision Detection*, 5.1.5: which of the
/// corners, edges or inside it's nearest, by where `p` is against them).
pub(crate) fn closest_on_triangle(p: Vec3, [a, b, c]: [Vec3; 3]) -> (Vec3, [f32; 3]) {
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return (a, [1.0, 0.0, 0.0]);
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return (b, [0.0, 1.0, 0.0]);
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return (a + ab * v, [1.0 - v, v, 0.0]);
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return (c, [0.0, 0.0, 1.0]);
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return (a + ac * w, [1.0 - w, 0.0, w]);
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return (b + (c - b) * w, [0.0, 1.0 - w, w]);
    }
    let total = va + vb + vc;
    if total.abs() < 1e-30 {
        // No area: its first corner will do.
        return (a, [1.0, 0.0, 0.0]);
    }
    let (v, w) = (vb / total, vc / total);
    (a + ab * v + ac * w, [1.0 - v - w, v, w])
}

/// How far along the ray it hits `a b c`, from either side. Worked out in
/// f64: from a few studs away, f32 loses enough that a ray along the edge
/// between two small triangles would miss both.
pub(crate) fn hit(origin: Vec3, direction: Vec3, [a, b, c]: [Vec3; 3]) -> Option<f32> {
    // Möller–Trumbore: solve origin + t·direction = a + u·(b−a) + v·(c−a).
    let (origin, direction) = (origin.as_dvec3(), direction.as_dvec3());
    let (a, b, c) = (a.as_dvec3(), b.as_dvec3(), c.as_dvec3());
    let (ab, ac) = (b - a, c - a);
    let p = direction.cross(ac);
    let det = ab.dot(p);
    if det.abs() < 1e-18 {
        return None;
    }
    let to_origin = origin - a;
    let u = to_origin.dot(p) / det;
    let q = to_origin.cross(ab);
    let v = direction.dot(q) / det;
    let t = ac.dot(q) / det;
    // A ray through an edge or corner hits the triangles there.
    const EDGE: f64 = 1e-9;
    (u >= -EDGE && v >= -EDGE && u + v <= 1.0 + EDGE && t > 0.0).then_some(t as f32)
}
