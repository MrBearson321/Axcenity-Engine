// The Path Traced view (renderer/path_tracer.rs): copies a mesh's normals,
// three a triangle in the mesh's index order and each packed into two
// words, into the view's shared buffer, so a path hitting its triangle `t`
// finds them at `first + t`. Once per mesh. As ray_cut_out_uvs.wgsl does
// for texture coordinates.

// Must match `ExtractUniform` in renderer/path_tracer.rs.
struct Extract {
    // x: the mesh's first triangle in `normals`, y: its triangles, z: 1 for
    // 32-bit indices, 0 for 16-bit.
    mesh: vec4<u32>,
}

// The mesh's corners as floats: `Vertex` in mesh.rs, 12 a corner, its
// normal the 4th to 6th.
@group(0) @binding(0) var<storage, read> vertices: array<f32>;
// Its indices, two 16-bit ones to a word for the built-in shapes.
@group(0) @binding(1) var<storage, read> indices: array<u32>;
@group(0) @binding(2) var<storage, read_write> normals: array<vec2<u32>>;
@group(0) @binding(3) var<uniform> extract: Extract;

const VERTEX_FLOATS: u32 = 12u;
const NORMAL_AT: u32 = 3u;

fn index_at(i: u32) -> u32 {
    if extract.mesh.z == 1u {
        return indices[i];
    }
    let word = indices[i / 2u];
    return select(word & 0xFFFFu, word >> 16u, (i & 1u) == 1u);
}

@compute @workgroup_size(64)
fn cs_extract_normals(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= extract.mesh.y * 3u {
        return;
    }
    let corner = index_at(id.x) * VERTEX_FLOATS + NORMAL_AT;
    let n = vec3<f32>(vertices[corner], vertices[corner + 1u], vertices[corner + 2u]);
    let unit = select(vec3<f32>(0.0), normalize(n), dot(n, n) > 1e-12);
    normals[extract.mesh.x * 3u + id.x] = vec2<u32>(
        pack2x16snorm(unit.xy),
        pack2x16snorm(vec2<f32>(unit.z, 0.0)),
    );
}
