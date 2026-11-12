//! A bounding volume hierarchy over a mesh's triangles: boxes in boxes,
//! so a ray only looks at the triangles near it (rigging's occlusion rays,
//! through [`crate::Reference`]).
//!
//! Each node covers a run of `order` (the triangles sorted so every
//! node's are together), split at the median along its longest side. A
//! node's children come after it in `nodes`.

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
