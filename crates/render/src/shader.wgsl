// Parts and imported models: physically based materials with texture maps,
// drawn into the HDR scene. Compiled after `lighting.wgsl`, which lights
// each pixel (`light_surface`). Also the shadow pass's and the sky's entry points.
// Followed by one `surface` function: surface.wgsl's, or a material's own
// (renderer/surface_shaders.rs), so this file must not define one.

// How the item's texture maps are read. Must match `MapsUniform` in
// renderer/types.rs.
struct Maps {
    // Which channel of each map holds its value: 1 in that channel, 0 in
    // the others.
    metalness: vec4<f32>,
    roughness: vec4<f32>,
    occlusion: vec4<f32>,
    opacity: vec4<f32>,
    // 1 to use 1 minus the value, for metalness, roughness, occlusion and opacity.
    invert: vec4<f32>,
    // x: normal map scale (0: no normal map), y: 1, or -1 to flip the
    // normal map's green, z: occlusion strength, w: 1 if opacity comes from
    // its own map rather than the base color's alpha.
    settings: vec4<f32>,
    // x: 1 if there's a specular map. y: 1 if the normal map holds only
    // red and green (BC5), so its blue is worked out from them.
    specular: vec4<f32>,
    // The columns of a 2x2 matrix that scales and turns the texture
    // coordinates (read by the vertex shader).
    uv_matrix: vec4<f32>,
    // xy: added to the texture coordinates after `uv_matrix`.
    uv_offset: vec4<f32>,
    // How it's lit. x: the model (SHADING_STANDARD, _UNLIT or _TOON); for
    // toon, y: its bands (0: read its ramp, bound in the specular map's
    // place), z: softness, w: highlight size.
    shading: vec4<f32>,
    // x: toon's rim light.
    toon: vec4<f32>,
    // The outline's linear color, then its width (0: none) in pixels of a
    // view 1080 pixels tall.
    outline: vec4<f32>,
    // The surface shader's values (`Material::surface_parameters`).
    params: array<vec4<f32>, 8>,
}

// The item's material textures. Color ones (base color, emission) are read
// through sRGB views, so samples are linear light; the rest hold plain
// values. A map an item doesn't have is a single white pixel, which
// changes nothing.
@group(1) @binding(0) var base_color_texture: texture_2d<f32>;
@group(1) @binding(1) var material_sampler: sampler;
@group(1) @binding(2) var<uniform> maps: Maps;
@group(1) @binding(3) var normal_texture: texture_2d<f32>;
@group(1) @binding(4) var metalness_texture: texture_2d<f32>;
@group(1) @binding(5) var roughness_texture: texture_2d<f32>;
@group(1) @binding(6) var occlusion_texture: texture_2d<f32>;
@group(1) @binding(7) var emissive_texture: texture_2d<f32>;
@group(1) @binding(8) var opacity_texture: texture_2d<f32>;
@group(1) @binding(9) var specular_texture: texture_2d<f32>;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(12) uv: vec2<f32>,
    @location(13) tangent: vec4<f32>,
    @builtin(vertex_index) index: u32,
}

// Animated meshes are posed here, on the GPU (see renderer/skinning.rs).
// One joint of a skin drawn this frame: where it moves a corner, and how it
// turns normals. Must match `JointRaw` in renderer/skinning.rs.
struct Joint {
    position: mat4x4<f32>,
    normal: mat3x3<f32>,
}

// Every skin's joints this frame. An item's start at `skin.x` of its instance.
@group(0) @binding(9) var<storage, read> frame_joints: array<Joint>;

// One corner's joints (of its item's skin) and how much each counts. Must
// match `SkinCorner` in renderer/skinning.rs.
struct SkinCorner {
    joints: vec4<u32>,
    weights: vec4<f32>,
}

// The mesh's corners' joints and weights: one corner weighted to nothing
// for a mesh that isn't animated.
@group(2) @binding(0) var<storage, read> skin_corners: array<SkinCorner>;

struct Posed {
    position: vec3<f32>,
    normal: vec3<f32>,
    tangent: vec3<f32>,
}

// Corner `index` moved by the item's `skin` (where its joints start in
// `frame_joints`, and how many it has; none for an item that isn't
// animated): each of its joints moves it, blended by their weights, as
// `Posed::skin` in assets' rig.rs does on the CPU. Joints past the skin's
// are left out, and a corner weighted to nothing stays where it is.
fn pose_corner(index: u32, skin: vec2<u32>, position: vec3<f32>, normal: vec3<f32>,
    tangent: vec3<f32>) -> Posed {
    var posed = Posed(position, normal, tangent);
    if skin.y == 0u {
        return posed;
    }
    let corner = skin_corners[index];
    var matrix = mat4x4<f32>();
    var normal_matrix = mat3x3<f32>();
    var total = 0.0;
    for (var i = 0u; i < 4u; i += 1u) {
        let weight = corner.weights[i];
        let joint = corner.joints[i];
        if weight <= 0.0 || joint >= skin.y {
            continue;
        }
        let moved = frame_joints[skin.x + joint];
        matrix += moved.position * weight;
        normal_matrix += moved.normal * weight;
        total += weight;
    }
    if total <= 1e-6 {
        return posed;
    }
    posed.position = (matrix * vec4<f32>(position, 1.0)).xyz / total;
    posed.normal = unit_or_zero(normal_matrix * normal);
    posed.tangent = unit_or_zero((matrix * vec4<f32>(tangent, 0.0)).xyz);
    return posed;
}

fn unit_or_zero(v: vec3<f32>) -> vec3<f32> {
    let length_squared = dot(v, v);
    return select(vec3<f32>(0.0), v * inverseSqrt(length_squared), length_squared > 0.0);
}

// Where `world` is bent by the frame's wind: `swayed_at` (sway.wgsl) at
// this frame's time.
fn swayed(world: vec3<f32>, sway: vec4<f32>) -> vec3<f32> {
    return swayed_at(world, sway, frame.clock);
}

// Per draw item. Must match `InstanceRaw` in renderer.rs.
struct InstanceIn {
    @location(2) model_0: vec4<f32>,
    @location(3) model_1: vec4<f32>,
    @location(4) model_2: vec4<f32>,
    @location(5) model_3: vec4<f32>,
    @location(6) normal_0: vec3<f32>,
    @location(7) normal_1: vec3<f32>,
    @location(8) normal_2: vec3<f32>,
    @location(9) base_color: vec4<f32>, // rgb, opacity
    // Light given off, then how much of the item is drawn (see `dithered_out`).
    @location(10) emissive_fade: vec4<f32>,
    // Metalness, roughness, alpha cutoff, and how many studs one copy of
    // the texture covers (0 keeps the mesh's own coordinates).
    @location(11) surface: vec4<f32>,
    // Where the item's skin's joints start, and how many (0: not animated).
    @location(14) skin: vec2<u32>,
    // How it sways in the wind (see `swayed`); w's whole part is its decal role
    // (`ROLE_CHARACTER`, `ROLE_DECAL`, else 0).
    @location(15) sway: vec4<f32>,
}

// `DrawItem::decal_role` (renderer/lighting.rs's `instance_raw`): a
// character's item, which projected decals that ignore characters leave
// bare, and a Part's Decal, drawn a hair toward the camera.
const ROLE_CHARACTER: f32 = 1.0;
const ROLE_DECAL: f32 = 2.0;
// The prepass normal's w on a cut-out's pixel (1 on others, 0.5 on a
// character's). Must match `PREPASS_CUT_OUT` in ray_shadows.wgsl and
// ray_ao.wgsl.
const PREPASS_CUT_OUT: f32 = 0.875;
// How far toward the camera a Decal's depth is moved, in clip-space depth
// (0 to 1): many times the depth's rounding, so it always wins over the
// face it lies on, yet far less than anything placed in front of it.
const DECAL_DEPTH_BIAS: f32 = 1e-6;

struct VertexOut {
    // Invariant, so the prepass and the moving items' motion pass
    // (`vs_motion`) put a corner at exactly the same depth.
    @builtin(position) @invariant clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    // Flat: the item's own, the same at every corner. Interpolated, the
    // opacity of a solid item could come out a hair under 1 in places,
    // which then weren't treated as solid.
    @location(2) @interpolate(flat) base_color: vec4<f32>,
    @location(3) emissive: vec3<f32>,
    @location(4) surface: vec3<f32>,
    @location(5) uv: vec2<f32>,
    @location(6) tangent: vec4<f32>,
    @location(7) @interpolate(flat) fade: f32,
    // Its decal role (`ROLE_CHARACTER`, `ROLE_DECAL`, else 0).
    @location(8) @interpolate(flat) role: f32,
    // The middle of the item and half its size, for leaves (`leaf_canopy`).
    @location(9) @interpolate(flat) item_middle: vec3<f32>,
    @location(10) @interpolate(flat) item_half_size: vec3<f32>,
}

@vertex
fn vs_main(vertex: VertexIn, instance: InstanceIn) -> VertexOut {
    var out = item_vertex(vertex, instance);
    if out.role == ROLE_DECAL {
        out.clip_position.z -= DECAL_DEPTH_BIAS * out.clip_position.w;
    }
    return out;
}

// A corner of an item, posed, swayed and placed (`vs_main`'s work, shared
// with `vs_outline`).
fn item_vertex(vertex: VertexIn, instance: InstanceIn) -> VertexOut {
    let model = mat4x4<f32>(instance.model_0, instance.model_1, instance.model_2, instance.model_3);
    let normal_matrix = mat3x3<f32>(instance.normal_0, instance.normal_1, instance.normal_2);
    let posed = pose_corner(vertex.index, instance.skin, vertex.position, vertex.normal,
        vertex.tangent.xyz);
    let world_position = vec4<f32>(swayed((model * vec4<f32>(posed.position, 1.0)).xyz,
        instance.sway), 1.0);

    var out: VertexOut;
    out.clip_position = frame.view_proj * world_position;
    out.world_position = world_position.xyz;
    out.normal = normal_matrix * posed.normal;
    out.base_color = instance.base_color;
    out.emissive = instance.emissive_fade.xyz;
    out.fade = instance.emissive_fade.w;
    out.role = floor(instance.sway.w);
    out.item_middle = instance.model_3.xyz;
    out.item_half_size = 0.5 * vec3<f32>(length(instance.model_0.xyz),
        length(instance.model_1.xyz), length(instance.model_2.xyz));
    out.surface = instance.surface.xyz;
    let tile = instance.surface.w;
    var uv = vertex.uv;
    if (tile > 0.0) {
        // The coordinates are distances on the unit mesh. Scale them by
        // how long those edges are on this part, so the texture repeats
        // every `tile` studs instead of stretching once across it.
        let along_u = (model * vec4<f32>(posed.tangent, 0.0)).xyz;
        let v_dir = cross(posed.normal, posed.tangent) * vertex.tangent.w;
        let along_v = (model * vec4<f32>(v_dir, 0.0)).xyz;
        uv = uv * vec2<f32>(length(along_u), length(along_v)) / tile;
    }
    // Then the material's own tiling, turn and offset.
    out.uv = mat2x2<f32>(maps.uv_matrix.xy, maps.uv_matrix.zw) * uv + maps.uv_offset.xy;
    // A direction along the surface, so it stretches with the shape.
    out.tangent = vec4<f32>((model * vec4<f32>(posed.tangent, 0.0)).xyz, vertex.tangent.w);
    return out;
}

// An outline's corner: the item's, pushed out on screen along its normal
// by the outline's width, the same number of pixels however far away it
// is. Only back faces are kept (`fs_outline`), so the line shows just past
// the item's silhouette, hidden by the item itself everywhere else.
@vertex
fn vs_outline(vertex: VertexIn, instance: InstanceIn) -> VertexOut {
    var out = item_vertex(vertex, instance);
    // `frame.ambient_occlusion.zw` is 1 over the scene's size in pixels.
    let size = 1.0 / frame.ambient_occlusion.zw;
    // Never under one pixel, so a small view still shows it.
    let pixels = max(maps.outline.w * size.y / 1080.0, 1.0);
    // The normal's direction on screen, in pixels (so the view's shape
    // doesn't squash it).
    let across = (frame.view_proj * vec4<f32>(out.normal, 0.0)).xy * size;
    if dot(across, across) > 1e-12 {
        let push = normalize(across) * pixels * 2.0 / size;
        out.clip_position = vec4<f32>(out.clip_position.xy + push * out.clip_position.w,
            out.clip_position.zw);
    }
    return out;
}

// The outline's color, unlit but fogged, on back faces only. Cut-out holes
// aren't followed: the outline is of the whole mesh.
@fragment
fn fs_outline(in: VertexOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    if front || dithered_out(in.fade, in.clip_position.xy) {
        discard;
    }
    let seen = fogged(frame.fog, frame.camera_position.xyz, in.world_position, maps.outline.rgb);
    return vec4<f32>(seen, 1.0);
}

// The shadow pass's cascade: turns world space to that cascade's view from
// the sun. Bound with a different offset for each cascade.
@group(0) @binding(5) var<uniform> shadow_cascade: mat4x4<f32>;

// Draws solid items into one cascade of the shadow map, from the sun's point of view.
@vertex
fn vs_shadow(
    @builtin(vertex_index) index: u32,
    @location(0) position: vec3<f32>,
    @location(2) model_0: vec4<f32>,
    @location(3) model_1: vec4<f32>,
    @location(4) model_2: vec4<f32>,
    @location(5) model_3: vec4<f32>,
    @location(14) skin: vec2<u32>,
    @location(15) sway: vec4<f32>,
) -> @builtin(position) vec4<f32> {
    let model = mat4x4<f32>(model_0, model_1, model_2, model_3);
    let posed = pose_corner(index, skin, position, vec3<f32>(0.0), vec3<f32>(0.0));
    // Swaying plants' shadows sway with them.
    let world = swayed((model * vec4<f32>(posed.position, 1.0)).xyz, sway);
    return shadow_cascade * vec4<f32>(world, 1.0);
}

// One map's value: the channel picked by `mask`, inverted if `invert` is 1.
fn map_value(texel: vec4<f32>, mask: vec4<f32>, invert: f32) -> f32 {
    let value = dot(texel, mask);
    return mix(value, 1.0 - value, invert);
}

// The surface normal at `uv`, tipped by the normal map if there is one,
// from the mesh's normal `n` (unit length) and `tangent`.
fn surface_normal(n: vec3<f32>, tangent: vec4<f32>, uv: vec2<f32>) -> vec3<f32> {
    let texel = textureSample(normal_texture, material_sampler, uv).xyz;
    // The tangent, made perpendicular to the normal.
    let along = tangent.xyz - n * dot(n, tangent.xyz);
    if maps.settings.x == 0.0 || dot(along, along) < 1e-12 {
        return n;
    }
    let t = normalize(along);
    let b = cross(n, t) * tangent.w;
    var tipped = texel * 2.0 - 1.0;
    if maps.specular.y != 0.0 {
        // A unit normal's Z from its X and Y.
        tipped.z = sqrt(saturate(1.0 - dot(tipped.xy, tipped.xy)));
    }
    tipped = vec3<f32>(tipped.x, tipped.y * maps.settings.y, tipped.z);
    tipped = vec3<f32>(tipped.xy * maps.settings.x, max(tipped.z, 0.0));
    let mapped = mat3x3<f32>(t, b, n) * tipped;
    if dot(mapped, mapped) < 1e-12 {
        return n;
    }
    return normalize(mapped);
}

// What a material's `surface` function is given about the pixel it shades.
// Colors are linear light. Documented in docs/rendering/models.md.
struct SurfaceInput {
    // The texture coordinates, already tiled and offset by the material.
    uv: vec2<f32>,
    world_position: vec3<f32>,
    // The mesh's normal (unit length, facing the camera), before the normal map.
    normal: vec3<f32>,
    // Along the texture's U; w is the side its V is on. Turned with `normal`.
    tangent: vec4<f32>,
    // Seconds, for surfaces that move (`DrawList::time`).
    time: f32,
    // The item's own values, before its maps: the part's color and
    // opacity, metalness, roughness and glow.
    color: vec3<f32>,
    opacity: f32,
    metalness: f32,
    roughness: f32,
    emissive: vec3<f32>,
}

// What a `surface` function gives back, lit by the scene's lights.
struct Surface {
    base_color: vec3<f32>,
    metalness: f32,
    roughness: f32,
    // Unit length, in world space.
    normal: vec3<f32>,
    // Light given off, added after lighting.
    emissive: vec3<f32>,
    // Below the material's cutoff, the pixel is cut out. An item is only
    // see-through when its material's opacity is below 1.
    opacity: f32,
}

// The item's own material at `input`: its values times its maps, read at
// `input.uv`. A custom surface can call it and change what it gives back.
fn standard_surface(input: SurfaceInput) -> Surface {
    let uv = input.uv;
    let texel = textureSample(base_color_texture, material_sampler, uv);
    let metalness_texel = textureSample(metalness_texture, material_sampler, uv);
    let roughness_texel = textureSample(roughness_texture, material_sampler, uv);
    let emissive_texel = textureSample(emissive_texture, material_sampler, uv);
    let opacity_texel = textureSample(opacity_texture, material_sampler, uv);
    let opacity_map = map_value(opacity_texel, maps.opacity, maps.invert.w);
    var out: Surface;
    out.base_color = input.color * texel.rgb;
    out.metalness = input.metalness * map_value(metalness_texel, maps.metalness, maps.invert.x);
    out.roughness = input.roughness * map_value(roughness_texel, maps.roughness, maps.invert.y);
    out.normal = surface_normal(input.normal, input.tangent, uv);
    out.emissive = input.emissive * emissive_texel.rgb;
    out.opacity = input.opacity * mix(texel.a, opacity_map, maps.settings.w);
    return out;
}

// The material's value `index` (0 to 7: its `parameters` in name order).
// A number is in x, a color or vector in xyz (colors as written, sRGB).
fn parameter(index: i32) -> vec4<f32> {
    return maps.params[index];
}

// An sRGB color (as a Part's `Color` is written) as linear light.
fn srgb(color: vec3<f32>) -> vec3<f32> {
    return select(pow((color + 0.055) / 1.055, vec3<f32>(2.4)), color / 12.92,
        color <= vec3<f32>(0.04045));
}

// The pixel's surface: the material's `surface` function (appended after
// this file, see renderer/surface_shaders.rs) at the pixel. Only
// double-sided items show their backs, which are lit as fronts facing the
// other way, so the normal and tangent are turned to face the camera.
fn pixel_surface(in: VertexOut, facing: f32) -> Surface {
    var input: SurfaceInput;
    input.uv = in.uv;
    input.world_position = in.world_position;
    input.normal = normalize(in.normal) * facing;
    input.tangent = in.tangent * facing;
    input.time = frame.clock.x;
    input.color = in.base_color.rgb;
    input.opacity = in.base_color.a;
    input.metalness = in.surface.x;
    input.roughness = in.surface.y;
    input.emissive = in.emissive;
    return surface(input);
}

@fragment
fn fs_main(in: VertexOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    // Every texture is sampled before anything branches, as WGSL requires.
    let facing = select(-1.0, 1.0, front);
    let shaded = pixel_surface(in, facing);
    let occlusion_texel = textureSample(occlusion_texture, material_sampler, in.uv);
    let specular_texel = textureSample(specular_texture, material_sampler, in.uv);

    // Cut-out materials (leaves, fences) leave holes where the texture is clear.
    if shaded.opacity < in.surface.z || dithered_out(in.fade, in.clip_position.xy) {
        discard;
    }
    return lit_pixel(in, facing, shaded, occlusion_texel, specular_texel);
}

// A cut-out surface, in the scene pass. Its depth was drawn just before
// (`fs_depth_only`), so the depth test (equal) already passes only its
// nearest layer, and only where it isn't cut out. With no `discard` here
// the GPU tests depth before running this, so the leaves hidden behind
// the nearest one are never lit.
@fragment
fn fs_prefilled(in: VertexOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    let facing = select(-1.0, 1.0, front);
    let shaded = pixel_surface(in, facing);
    let occlusion_texel = textureSample(occlusion_texture, material_sampler, in.uv);
    let specular_texel = textureSample(specular_texture, material_sampler, in.uv);
    return lit_pixel(in, facing, shaded, occlusion_texel, specular_texel);
}

// A cut-out surface's opacity at this pixel, as `standard_surface` and
// `fs_main` work it out.
fn cut_out_opacity(in: VertexOut) -> f32 {
    let texel = textureSample(base_color_texture, material_sampler, in.uv);
    let opacity_texel = textureSample(opacity_texture, material_sampler, in.uv);
    let opacity_map = map_value(opacity_texel, maps.opacity, maps.invert.w);
    return in.base_color.a * mix(texel.a, opacity_map, maps.settings.w);
}

// Draws a cut-out surface's depth before the scene is lit, with one sample
// per pixel. What it cuts out must match `standard_surface` and `fs_main`.
@fragment
fn fs_depth_only(in: VertexOut) -> @location(0) vec4<f32> {
    if cut_out_opacity(in) < in.surface.z || dithered_out(in.fade, in.clip_position.xy) {
        discard;
    }
    return vec4<f32>(0.0);
}

// `fs_depth_only` when multisampled. MSAA smooths the edges of triangles,
// not the holes a texture cuts in them, so the returned alpha says how much
// of the pixel the surface covers and the GPU writes depth to that share of
// its samples (alpha to coverage). The cut-off's edge is ramped over one
// pixel, at the same place as `fs_depth_only`'s, so distant leaves don't thin
// out. `fs_prefilled` then lights just the samples this left its depth in.
@fragment
fn fs_depth_coverage(in: VertexOut) -> @location(0) vec4<f32> {
    let opacity = cut_out_opacity(in);
    let coverage = (opacity - in.surface.z) / max(fwidth(opacity), 0.0001) + 0.5;
    if coverage <= 0.0 || dithered_out(in.fade, in.clip_position.xy) {
        discard;
    }
    return vec4<f32>(0.0, 0.0, 0.0, min(coverage, 1.0));
}

// The lit color of a pixel of a surface: `shaded`, its two remaining maps'
// texels already read.
fn lit_pixel(
    in: VertexOut,
    facing: f32,
    shaded: Surface,
    occlusion_texel: vec4<f32>,
    specular_texel: vec4<f32>,
) -> vec4<f32> {
    let metalness = clamp(shaded.metalness, 0.0, 1.0);
    // Perfectly smooth surfaces make the highlight infinitely small, so keep a minimum.
    let roughness = clamp(shaded.roughness, 0.04, 1.0);
    // The map's, and for solid items the screen-space ambient occlusion's.
    let solid = in.base_color.a >= 1.0;
    let occlusion = mix(
        1.0, map_value(occlusion_texel, maps.occlusion, maps.invert.z), maps.settings.z)
        * select(1.0, screen_occlusion(in.clip_position.xy), solid);
    // Non-metals reflect about 4% of light, or as their specular map says.
    let dielectric = mix(vec3<f32>(0.04), 0.08 * specular_texel.rgb, maps.specular.x);

    let geometric = normalize(in.normal) * facing;
    var color: vec3<f32>;
    if maps.shading.x == SHADING_UNLIT {
        color = shaded.base_color;
    } else if maps.shading.x == SHADING_TOON {
        color = toon_surface(in.world_position, geometric, shaded.normal, in.clip_position.xy,
            shaded.base_color, occlusion, solid);
    } else if maps.shading.y > 0.0 {
        color = lit_leaf(in, shaded, geometric, metalness, roughness, occlusion, dielectric,
            solid, maps.shading.y);
    } else {
        color = light_surface(in.world_position, geometric, shaded.normal, in.clip_position.xy,
            shaded.base_color, metalness, roughness, occlusion, dielectric, solid);
    }

    // Solid items stay solid whatever the texture's alpha, so the window's
    // own alpha stays 1.
    let alpha = select(clamp(shaded.opacity, 0.0, 1.0), 1.0, solid);
    let seen = fogged(frame.fog, frame.camera_position.xyz, in.world_position, color + shaded.emissive);
    return vec4<f32>(seen, alpha);
}

// How much of what a translucent surface passes on it no longer reflects.
const LEAF_REFLECTION_LOSS: f32 = 0.6;

// How much of each color a leaf lets through, compared with what it
// reflects: green and red pass, blue is taken, so lit from behind it glows
// a brighter, yellower green.
const THROUGH_LEAF_TINT: vec3<f32> = vec3<f32>(1.0, 1.12, 0.35);

// How far a leaf's light is turned from its own card toward the outside of
// its plant. Foliage is a volume: it's lit by where in the crown it is far
// more than by which way one card happens to face.
const CANOPY_BEND: f32 = 0.6;
// How much sky reaches the very middle of a crown, compared with its outside.
const CANOPY_DEPTH_SHADE: f32 = 0.5;
// A crown's mass sits above the middle of the plant, by this share of half
// its height: the trunk or stems are below.
const CANOPY_LIFT: f32 = 0.3;
// How rough a leaf is at least, on the outside of a crown and deep inside it.
// Rough enough, a leaf seen edge-on doesn't mirror a white sky as grey.
const LEAF_ROUGHNESS_OUTSIDE: f32 = 0.72;
const LEAF_ROUGHNESS_INSIDE: f32 = 0.9;
// How much individual leaves differ in brightness and in hue.
const LEAF_TONE_VARIATION: f32 = 0.2;
const LEAF_HUE_VARIATION: f32 = 0.18;

// Where a leaf is in its plant.
struct Canopy {
    // Away from the crown's middle, in world space.
    normal: vec3<f32>,
    // 0 deep inside the crown, 1 on its outside.
    openness: f32,
}

// The crown is taken as the ball filling the item, a little above its middle.
fn leaf_canopy(world_position: vec3<f32>, middle: vec3<f32>, half_size: vec3<f32>, fallback: vec3<f32>)
    -> Canopy {
    let safe = max(half_size, vec3<f32>(0.001));
    let along = (world_position - middle) / safe - vec3<f32>(0.0, CANOPY_LIFT, 0.0);
    let reach = length(along);
    if reach < 0.001 {
        return Canopy(fallback, 0.0);
    }
    return Canopy(along / reach, smoothstep(0.2, 0.95, reach));
}

// A leaf's own brightness and hue: every card faces its own way, so the way
// it faces tells them apart without any data of its own, and where it is
// shifts whole sprays of them together. Smooth, so a bent card has no seams.
fn leaf_tone(unflipped_normal: vec3<f32>, world_position: vec3<f32>, base_color: vec3<f32>)
    -> vec3<f32> {
    let n = unflipped_normal;
    let spray = sin(dot(world_position, vec3<f32>(1.13, 0.71, 1.37)));
    let tone = 0.65 * sin(dot(n, vec3<f32>(41.3, 67.1, 29.7))) + 0.35 * spray;
    let hue = 0.65 * sin(dot(n, vec3<f32>(23.9, 13.7, 57.1))) + 0.35 * spray;
    // Olive one way, blue-green the other.
    let tint = vec3<f32>(1.0 + LEAF_HUE_VARIATION * hue, 1.0, 1.0 - LEAF_HUE_VARIATION * hue);
    return base_color * tint * (1.0 + LEAF_TONE_VARIATION * tone);
}

// A leaf's light: reflected, and come through it from behind.
fn lit_leaf(
    in: VertexOut,
    shaded: Surface,
    geometric: vec3<f32>,
    metalness: f32,
    roughness: f32,
    occlusion: f32,
    dielectric: vec3<f32>,
    solid: bool,
    amount: f32,
) -> vec3<f32> {
    let canopy = leaf_canopy(in.world_position, in.item_middle, in.item_half_size, shaded.normal);
    let bent = normalize(mix(shaded.normal, canopy.normal, CANOPY_BEND));
    let base_color = leaf_tone(normalize(in.normal), in.world_position, shaded.base_color);
    let open = mix(CANOPY_DEPTH_SHADE, 1.0, canopy.openness);
    let rough = max(roughness,
        mix(LEAF_ROUGHNESS_INSIDE, LEAF_ROUGHNESS_OUTSIDE, canopy.openness));
    let reflected_light = light_surface(in.world_position, geometric, bent, in.clip_position.xy,
        base_color, metalness, rough, occlusion * open, dielectric, solid);
    // What passes through isn't also reflected.
    return reflected_light * (1.0 - LEAF_REFLECTION_LOSS * amount)
        + through_leaf(in.world_position, geometric, shaded.normal, bent, in.clip_position.xy,
            base_color, occlusion, open, amount);
}

// The light that comes through a thin surface from its far side, from the
// sun and from the sky behind it, scattered evenly toward the viewer. The
// sun lights each leaf by its own card (`n`), a sharp thing; the sky by the
// crown around it (`bent`, `open`), a soft one. Occlusion counts by half:
// what's close in front of a leaf doesn't block what reaches it from behind.
fn through_leaf(
    world_position: vec3<f32>,
    geometric: vec3<f32>,
    n: vec3<f32>,
    bent: vec3<f32>,
    pixel: vec2<f32>,
    base_color: vec3<f32>,
    occlusion: f32,
    open: f32,
    amount: f32,
) -> vec3<f32> {
    let sun_visible = soft_sun_visibility(world_position, -geometric, pixel)
        * cloud_shadow(frame.clouds, world_position, frame.sun_direction.xyz);
    let from_sun = frame.sun_radiance.rgb * sun_visible
        * max(dot(-n, frame.sun_direction.xyz), 0.0) / PI;
    let from_sky = surrounding_diffuse(world_position, -bent, frame.sky_top.w) * open;
    return amount * base_color * THROUGH_LEAF_TINT * mix(1.0, occlusion, 0.5) * (from_sun + from_sky);
}

// `Maps::shading`'s models. Must match `SHADING_*` in renderer/types.rs.
const SHADING_STANDARD: f32 = 0.0;
const SHADING_UNLIT: f32 = 1.0;
const SHADING_TOON: f32 = 2.0;

// How far round from the camera a toon surface's rim light starts (in
// 1 - cos of the angle), and how large a highlight of size 1 is (its
// half-angle in radians).
const TOON_RIM_WIDTH: f32 = 0.35;
const TOON_HIGHLIGHT_ANGLE: f32 = 0.5;

// Cuts `x` (0 to 1) into the material's bands, or reads its ramp at `x`
// (its middle row, left to right, one pixel unblended).
fn toon_band(x: f32) -> vec3<f32> {
    let light = clamp(x, 0.0, 1.0);
    let steps = maps.shading.y;
    if steps < 1.0 {
        let size = textureDimensions(specular_texture);
        let at = vec2<u32>(u32(round(light * f32(size.x - 1u))), size.y / 2u);
        return textureLoad(specular_texture, at, 0).rgb;
    }
    // Any light at all lifts a point to the first band, and each band's
    // lower edge blends over `softness` of a band.
    let scaled = light * steps;
    let below = floor(scaled);
    let blend = clamp((scaled - below) / max(maps.shading.z, 1e-4), 0.0, 1.0);
    return vec3<f32>(min((below + blend) / steps, 1.0));
}

// 0 below `edge`, 1 above it, blended over `softness` of `width`.
fn toon_step(value: f32, edge: f32, width: f32) -> f32 {
    return clamp((value - edge) / max(maps.shading.z * width, 1e-4), 0.0, 1.0);
}

// One light on a toon surface: its banded share (how squarely it falls,
// times its shadow) of the base color, plus the highlight, in its color.
fn toon_light(n: vec3<f32>, v: vec3<f32>, arrival: Arrival, base_color: vec3<f32>) -> vec3<f32> {
    let n_dot_l = max(dot(n, arrival.l), 0.0);
    let diffuse = base_color / PI * toon_band(n_dot_l * arrival.visible);
    let h = normalize(v + arrival.l);
    let cut = cos(maps.shading.w * TOON_HIGHLIGHT_ANGLE);
    let highlight = toon_step(dot(n, h), cut, 1.0 - cut) * arrival.visible
        * select(0.0, 1.0, n_dot_l > 0.0 && maps.shading.w > 0.0);
    return (diffuse + vec3<f32>(highlight / PI)) * arrival.radiance;
}

// The light a toon surface gives back toward the camera, from the same
// lights and shadows as `light_surface` (the weather too), but cut into
// bands, with a rim light around its outline and flat ambient light.
fn toon_surface(
    world_position: vec3<f32>,
    geometric: vec3<f32>,
    normal: vec3<f32>,
    pixel: vec2<f32>,
    base_color: vec3<f32>,
    occlusion: f32,
    solid: bool,
) -> vec3<f32> {
    let weather = weathered(geometric, normal, base_color, 0.0, 1.0);
    let n = weather.normal;
    let color = weather.base_color;
    let v = normalize(frame.camera_position.xyz - world_position);

    let sun = sun_arrival(world_position, geometric, pixel, solid);
    var lit = toon_light(n, v, sun, color);
    let lamps = tile_lamps(pixel);
    var slot = 0u;
    for (var i = 0u; i < lamps.count; i++) {
        let lamp = lamp_arrival(lamps, i, slot, world_position, geometric, pixel, solid);
        slot += lamp_slots(lamps, i);
        if lamp.visible > 0.0 {
            lit += toon_light(n, v, lamp, color);
        }
    }

    let surrounding = frame.ambient.rgb
        + surrounding_diffuse(world_position, n, frame.sky_top.w);
    let ambient = occlusion * surrounding * color;
    // The rim catches the sun (where it isn't shadowed) and the sky.
    let rim = maps.toon.x * toon_step(1.0 - max(dot(n, v), 0.0), 1.0 - TOON_RIM_WIDTH,
        TOON_RIM_WIDTH) * (sun.radiance * sun.visible / PI + surrounding);
    return lit + ambient + rim;
}

// Whether a fading item leaves out the pixel at `pixel`, in a fine pattern,
// so two meshes cross-fade without sorting or blending (level of detail, and
// foliage near its cull distance). A fade of 1 draws every pixel. Below 1,
// only the pixels whose noise is under it are drawn; above 1, only the
// others. So a mesh fading in at `t` and one fading out at `1 + t` draw
// each pixel exactly once between them.
fn dithered_out(fade: f32, pixel: vec2<f32>) -> bool {
    let noise = pixel_noise(pixel);
    // The comparison is in brackets so WGSL doesn't read `noise < ... >` as a
    // template list.
    return select(noise >= fade, (noise < fade - 1.0), fade > 1.0);
}

// The depth and normal prepass, for ambient occlusion: solid items' depth
// and view-space normals. Cut-out pixels are left out, as in `fs_main`.
@fragment
fn fs_prepass(in: VertexOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    let facing = select(-1.0, 1.0, front);
    let shaded = pixel_surface(in, facing);
    if shaded.opacity < in.surface.z || dithered_out(in.fade, in.clip_position.xy) {
        discard;
    }
    var normal = prepass_normal(normalize(in.normal) * facing);
    // A plain solid pixel keeps its roughness, 1 plus it, for ray traced
    // reflections (ray_reflections.wgsl); anything reading the flags below
    // sees it as solid (1 or more).
    normal.w = 1.0 + clamp(shaded.roughness, 0.0, 0.999);
    // A character's pixels are marked for projected decals (decal.wgsl).
    if in.role == ROLE_CHARACTER {
        normal.w = 0.5;
    } else if in.surface.z > 0.0 {
        // A cut-out's pixel, for ray traced shadows and AO
        // (ray_shadows.wgsl's `cut_out_near`); decals treat it as solid.
        normal.w = PREPASS_CUT_OUT;
    }
    return normal;
}

// Motion vectors for items that moved since last frame (renderer/temporal.rs):
// how far each pixel of a moved item went across the screen, drawn over
// the camera's motion where the prepass shows the item.

// Must match the start of `TemporalUniform` in renderer/temporal.rs: this
// frame's and last frame's views, without TAA's jitter.
struct MotionView {
    view_proj: mat4x4<f32>,
    previous_view_proj: mat4x4<f32>,
    // Last frame's time and wind (as `Frame::clock`), for swayed plants.
    previous_clock: vec4<f32>,
}

@group(3) @binding(0) var<uniform> motion_view: MotionView;
// Last frame's joints, where this frame's are in `frame_joints`.
@group(3) @binding(1) var<storage, read> previous_joints: array<Joint>;

// A moved item: where it is now and where it was. Must match
// `MotionInstance` in renderer/temporal.rs.
struct MotionIn {
    @location(2) model_0: vec4<f32>,
    @location(3) model_1: vec4<f32>,
    @location(4) model_2: vec4<f32>,
    @location(5) model_3: vec4<f32>,
    @location(6) previous_0: vec4<f32>,
    @location(7) previous_1: vec4<f32>,
    @location(8) previous_2: vec4<f32>,
    @location(9) previous_3: vec4<f32>,
    @location(10) skin: vec2<u32>,
    @location(11) sway: vec4<f32>,
    @location(12) fade: f32,
}

struct MotionOut {
    @builtin(position) @invariant clip_position: vec4<f32>,
    @location(0) now: vec4<f32>,
    @location(1) before: vec4<f32>,
    @location(2) @interpolate(flat) fade: f32,
}

// Corner `index` posed by last frame's joints (`pose_corner`'s position).
fn previous_corner(index: u32, skin: vec2<u32>, position: vec3<f32>) -> vec3<f32> {
    if skin.y == 0u {
        return position;
    }
    let corner = skin_corners[index];
    var matrix = mat4x4<f32>();
    var total = 0.0;
    for (var i = 0u; i < 4u; i += 1u) {
        let weight = corner.weights[i];
        let joint = corner.joints[i];
        if weight <= 0.0 || joint >= skin.y {
            continue;
        }
        matrix += previous_joints[skin.x + joint].position * weight;
        total += weight;
    }
    if total <= 1e-6 {
        return position;
    }
    return (matrix * vec4<f32>(position, 1.0)).xyz / total;
}

@vertex
fn vs_motion(@builtin(vertex_index) index: u32, @location(0) position: vec3<f32>,
    instance: MotionIn) -> MotionOut {
    let model = mat4x4<f32>(instance.model_0, instance.model_1, instance.model_2, instance.model_3);
    let previous = mat4x4<f32>(instance.previous_0, instance.previous_1, instance.previous_2,
        instance.previous_3);
    // Placed as `item_vertex` places it, so the depth matches the prepass's.
    let posed = pose_corner(index, instance.skin, position, vec3<f32>(0.0), vec3<f32>(0.0));
    let world = vec4<f32>(swayed((model * vec4<f32>(posed.position, 1.0)).xyz, instance.sway), 1.0);
    let was = previous_corner(index, instance.skin, position);
    let world_before = vec4<f32>(swayed_at((previous * vec4<f32>(was, 1.0)).xyz, instance.sway,
        motion_view.previous_clock), 1.0);
    var out: MotionOut;
    out.clip_position = frame.view_proj * world;
    out.now = motion_view.view_proj * world;
    out.before = motion_view.previous_view_proj * world_before;
    out.fade = instance.fade;
    return out;
}

// How far the pixel moved since last frame, in texture coordinates (0 to 1
// across the scene, down the screen).
@fragment
fn fs_motion(in: MotionOut) -> @location(0) vec4<f32> {
    if dithered_out(in.fade, in.clip_position.xy) {
        discard;
    }
    let now = in.now.xy / in.now.w;
    let before = in.before.xy / in.before.w;
    return vec4<f32>((now - before) * vec2<f32>(0.5, -0.5), 0.0, 0.0);
}

// A full-screen triangle at the far plane, for drawing the sky.
struct SkyOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) clip_xy: vec2<f32>,
}

@vertex
fn vs_sky(@builtin(vertex_index) index: u32) -> SkyOut {
    let xy = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u)) * 2.0 - 1.0;
    var out: SkyOut;
    out.clip_position = vec4<f32>(xy, 1.0, 1.0);
    out.clip_xy = xy;
    return out;
}

@fragment
fn fs_sky(in: SkyOut) -> @location(0) vec4<f32> {
    let far = frame.inv_view_proj * vec4<f32>(in.clip_xy, 1.0, 1.0);
    let dir = normalize(far.xyz / far.w - frame.camera_position.xyz);
    // How far the direction turns from one pixel to the next: about the
    // angle each pixel covers (taken here, where every pixel runs together).
    let spread = length(fwidth(dir));
    var sky = sky_radiance(dir, spread);
    if frame.clouds.color.w != 0.0 {
        sky = clouded_sky(frame.clouds, frame.camera_position.xyz, dir, sky, frame.sun_direction.xyz,
            frame.sun_radiance.rgb, frame.sky_top.rgb, frame.sky_horizon.rgb,
            pixel_noise(in.clip_position.xy));
    }
    return vec4<f32>(fogged_along(frame.fog, frame.camera_position.xyz, dir, FOG_SKY_DISTANCE, sky), 1.0);
}
