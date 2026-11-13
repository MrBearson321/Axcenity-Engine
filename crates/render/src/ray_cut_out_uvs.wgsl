// Ray traced cut-outs (renderer/ray_cut_outs.rs): copies a cut-out mesh's
// texture coordinates, three a triangle in the mesh's index order, into the
// ray traced scene's shared buffer, so a ray hitting its triangle `t` finds
// them at `first + t`. Once per mesh, when its BLAS is built.

// Must match `ExtractUniform` in renderer/ray_cut_outs.rs.
struct Extract {
    // x: the mesh's first triangle in `uvs`, y: its triangles, z: 1 for
    // 32-bit indices, 0 for 16-bit.
    mesh: vec4<u32>,
}

// The mesh's corners as floats: `Vertex` in mesh.rs, 12 a corner, its
// texture coordinates the 7th and 8th.
@group(0) @binding(0) var<storage, read> vertices: array<f32>;
// Its indices, two 16-bit ones to a word for the built-in shapes.
@group(0) @binding(1) var<storage, read> indices: array<u32>;
@group(0) @binding(2) var<storage, read_write> uvs: array<vec2<f32>>;
@group(0) @binding(3) var<uniform> extract: Extract;

const VERTEX_FLOATS: u32 = 12u;
const UV_AT: u32 = 6u;

fn index_at(i: u32) -> u32 {
    if extract.mesh.z == 1u {
        return indices[i];
    }
    let word = indices[i / 2u];
    return select(word & 0xFFFFu, word >> 16u, (i & 1u) == 1u);
}

@compute @workgroup_size(64)
fn cs_extract(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= extract.mesh.y * 3u {
        return;
    }
    let corner = index_at(id.x) * VERTEX_FLOATS + UV_AT;
    uvs[extract.mesh.x * 3u + id.x] = vec2<f32>(vertices[corner], vertices[corner + 1u]);
}
