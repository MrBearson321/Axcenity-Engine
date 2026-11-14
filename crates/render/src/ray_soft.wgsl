// Rays traced in software (RL3, renderer/ray_soft.rs,
// docs/rendering/ray-tracing.md): the ray traced scene as bounding volume
// hierarchies (BVHs) in one storage buffer, walked by a compute shader, for
// GPUs without hardware ray queries. The shader including it binds the
// buffer as `soft_scene`.
//
// `soft_scene`, all vec4s (u32s as their bits):
// - [0]: x the top BVH's root, y how many items.
// - A node: two vec4s, (its box's least corner, a) then (its greatest
//   corner, count). With count 0 its two children are at a and a + 2;
//   otherwise it's a leaf of `count` things from a: triangles (three vec4s
//   each: the first corner, then the edges to the second and third) in a
//   mesh's BVH, items (four vec4s each) in the top one.
// - An item: three rows of its world-to-mesh matrix (xyz times a point, w
//   added), then x its mesh's root, y its mask (`MASK_*` in
//   renderer/ray_tracing.rs), z its slot, w 1 for a cut-out.
//
// A cut-out's holes aren't known here: a ray passes one of its triangles
// half the time, by a hash of the ray, so many rays see it half there.

// The deepest a BVH walk goes: renderer/ray_soft.rs builds none deeper.
const SOFT_STACK: u32 = 32u;
// Nothing is hit nearer than this along a ray (studs), so a ray started
// on a surface doesn't find it.
const SOFT_NEAREST: f32 = 0.0001;

// What a ray found: whether it hit, how far along it, the item's slot and
// whether it hit the front (the side the triangle's corners go
// counter-clockwise from, as meshes face outward).
struct SoftHit {
    found: bool,
    t: f32,
    slot: u32,
    front_face: bool,
}

fn soft_u32(value: f32) -> u32 {
    return bitcast<u32>(value);
}

// The ray's direction with no part exactly 0, so 1 / it is finite.
fn soft_inverse(direction: vec3<f32>) -> vec3<f32> {
    let tiny = vec3<f32>(1e-12);
    let safe = select(direction, select(-tiny, tiny, direction >= vec3<f32>(0.0)),
        abs(direction) < tiny);
    return 1.0 / safe;
}

// How far along the ray (from `origin`, 1 / direction `inverse`) it enters
// the box from `low` to `high`, or a huge number if it misses it or only
// past `t_max`.
fn soft_box(low: vec3<f32>, high: vec3<f32>, origin: vec3<f32>, inverse: vec3<f32>, t_max: f32) -> f32 {
    let a = (low - origin) * inverse;
    let b = (high - origin) * inverse;
    let near = max(max(min(a.x, b.x), min(a.y, b.y)), min(a.z, b.z));
    let far = min(min(max(a.x, b.x), max(a.y, b.y)), max(a.z, b.z));
    if far >= max(near, 0.0) && near <= t_max {
        return max(near, 0.0);
    }
    return 3.4e38;
}

// A number from `seed` that looks random (PCG's hash).
fn soft_hash(seed: u32) -> u32 {
    let state = seed * 747796405u + 2891336453u;
    let word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

// Where a ray (in a mesh's own space) meets the triangle at `at`: how far
// along it (or a huge number for a miss) and whether that's its front.
fn soft_triangle(at: u32, origin: vec3<f32>, direction: vec3<f32>) -> vec2<f32> {
    let v0 = soft_scene[at].xyz;
    let e1 = soft_scene[at + 1u].xyz;
    let e2 = soft_scene[at + 2u].xyz;
    let p = cross(direction, e2);
    let det = dot(e1, p);
    if abs(det) < 1e-12 {
        return vec2<f32>(3.4e38, 0.0);
    }
    let inv_det = 1.0 / det;
    let s = origin - v0;
    let u = dot(s, p) * inv_det;
    if u < 0.0 || u > 1.0 {
        return vec2<f32>(3.4e38, 0.0);
    }
    let q = cross(s, e1);
    let v = dot(direction, q) * inv_det;
    if v < 0.0 || u + v > 1.0 {
        return vec2<f32>(3.4e38, 0.0);
    }
    let t = dot(e2, q) * inv_det;
    if t <= SOFT_NEAREST {
        return vec2<f32>(3.4e38, 0.0);
    }
    // Counter-clockwise seen from the ray's start: its front.
    return vec2<f32>(t, select(0.0, 1.0, det > 0.0));
}

// The nearest hit (or, with `any_hit`, the first found) of a ray in a
// mesh's own space with the mesh whose BVH's root is `root`, nearer than
// `t_max`: x how far (huge for none), y 1 for its front. A cut-out's
// triangles (`cut_out`) are passed half the time, by `seed`.
fn soft_mesh(root: u32, origin: vec3<f32>, direction: vec3<f32>, t_max: f32, any_hit: bool, cut_out: bool, seed: u32) -> vec2<f32> {
    var best = vec2<f32>(3.4e38, 0.0);
    var reach = t_max;
    let inverse = soft_inverse(direction);
    var stack: array<u32, SOFT_STACK>;
    stack[0] = root;
    var depth = 1u;
    while depth > 0u {
        depth -= 1u;
        let node = stack[depth];
        let low = soft_scene[node];
        let high = soft_scene[node + 1u];
        if soft_box(low.xyz, high.xyz, origin, inverse, reach) > reach {
            continue;
        }
        let first = soft_u32(low.w);
        let count = soft_u32(high.w);
        if count == 0u {
            if depth + 2u <= SOFT_STACK {
                stack[depth] = first;
                stack[depth + 1u] = first + 2u;
                depth += 2u;
            }
            continue;
        }
        for (var k = 0u; k < count; k++) {
            let at = first + 3u * k;
            let hit = soft_triangle(at, origin, direction);
            if hit.x >= reach {
                continue;
            }
            if cut_out && (soft_hash(seed ^ (at * 2654435761u)) & 1u) == 0u {
                continue;
            }
            best = hit;
            reach = hit.x;
            if any_hit {
                return best;
            }
        }
    }
    return best;
}

// The nearest hit of a ray from `origin` along `direction` nearer than
// `t_max` with the items whose mask shares a bit with `mask`; with
// `any_hit`, the first hit found (for shadow rays).
fn soft_trace(origin: vec3<f32>, direction: vec3<f32>, t_max: f32, mask: u32, any_hit: bool) -> SoftHit {
    var found = SoftHit(false, t_max, 0u, false);
    let header = soft_scene[0];
    if soft_u32(header.y) == 0u {
        return found;
    }
    let inverse = soft_inverse(direction);
    let seed = soft_hash(bitcast<u32>(origin.x) ^ soft_hash(bitcast<u32>(origin.y)
        ^ soft_hash(bitcast<u32>(direction.x) ^ soft_hash(bitcast<u32>(direction.z)))));
    var stack: array<u32, SOFT_STACK>;
    stack[0] = soft_u32(header.x);
    var depth = 1u;
    while depth > 0u {
        depth -= 1u;
        let node = stack[depth];
        let low = soft_scene[node];
        let high = soft_scene[node + 1u];
        if soft_box(low.xyz, high.xyz, origin, inverse, found.t) > found.t {
            continue;
        }
        let first = soft_u32(low.w);
        let count = soft_u32(high.w);
        if count == 0u {
            if depth + 2u <= SOFT_STACK {
                stack[depth] = first;
                stack[depth + 1u] = first + 2u;
                depth += 2u;
            }
            continue;
        }
        for (var k = 0u; k < count; k++) {
            let at = first + 4u * k;
            let item = bitcast<vec4<u32>>(soft_scene[at + 3u]);
            if (item.y & mask) == 0u {
                continue;
            }
            let r0 = soft_scene[at];
            let r1 = soft_scene[at + 1u];
            let r2 = soft_scene[at + 2u];
            // Into the mesh's own space. The direction isn't made one long
            // again, so distances along it stay the world's.
            let o = vec3<f32>(dot(r0.xyz, origin) + r0.w, dot(r1.xyz, origin) + r1.w,
                dot(r2.xyz, origin) + r2.w);
            let d = vec3<f32>(dot(r0.xyz, direction), dot(r1.xyz, direction), dot(r2.xyz, direction));
            let hit = soft_mesh(item.x, o, d, found.t, any_hit, item.w == 1u, seed ^ item.z);
            if hit.x < found.t {
                found = SoftHit(true, hit.x, item.z, hit.y > 0.5);
                if any_hit {
                    return found;
                }
            }
        }
    }
    return found;
}
