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
    // x: 1 if there's a specular map.
    specular: vec4<f32>,
    // The columns of a 2x2 matrix that scales and turns the texture
    // coordinates (read by the vertex shader).
    uv_matrix: vec4<f32>,
    // xy: added to the texture coordinates after `uv_matrix`.
    uv_offset: vec4<f32>,
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

// Plants sway in the wind (`DrawItem::sway`). The most a plant of strength
// 1 leans, as a share of its height, and the wind speed in studs a second
// at which it leans about two thirds of that.
const SWAY_LEAN: f32 = 0.25;
const SWAY_WIND: f32 = 20.0;

// Where the world-space corner `world` of a plant (its instance's `sway`:
// the height of its ground, 1 over its height, its strength) is bent by
// the frame's wind. It leans downwind, more the stronger the wind and the
// higher up the plant, and stays still in no wind. Gusts roll over the
// ground with the wind, and a flutter shakes it. Both vary smoothly over
// the world, so a plant drawn as several items bends as one. The farthest
// a corner goes is `Sway::reach` in draw_list.rs: keep them matching.
fn swayed(world: vec3<f32>, sway: vec4<f32>) -> vec3<f32> {
    let wind = frame.clock.yw;
    let speed = length(wind);
    if sway.z <= 0.0 || speed < 1e-3 {
        return world;
    }
    let up = clamp((world.y - sway.x) * sway.y, 0.0, 1.0);
    // Stiff at the foot, giving more toward the top.
    let bend = up * up;
    let time = frame.clock.x;
    let lean = SWAY_LEAN * sway.z / sway.y * (1.0 - exp(-speed / SWAY_WIND));
    let gust = cloud_noise2((world.xz - wind * time) * 0.03);
    let flutter = sin(time * (1.7 + speed * 0.05) + dot(world.xz, vec2<f32>(0.21, 0.17)));
    let along = lean * bend * (0.7 + 0.5 * gust + 0.15 * flutter);
    let offset = wind / speed * along;
    // Bending lowers the top a little, so the plant keeps its length.
    let drop = 0.5 * along * along * sway.y;
    return world + vec3<f32>(offset.x, -drop, offset.y);
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
    // How it sways in the wind (see `swayed`).
    @location(15) sway: vec4<f32>,
}

struct VertexOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) base_color: vec4<f32>,
    @location(3) emissive: vec3<f32>,
    @location(4) surface: vec3<f32>,
    @location(5) uv: vec2<f32>,
    @location(6) tangent: vec4<f32>,
    @location(7) @interpolate(flat) fade: f32,
}

@vertex
fn vs_main(vertex: VertexIn, instance: InstanceIn) -> VertexOut {
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

    let color = light_surface(in.world_position, normalize(in.normal) * facing, shaded.normal,
        in.clip_position.xy, shaded.base_color, metalness, roughness, occlusion, dielectric,
        solid);

    // Solid items stay solid whatever the texture's alpha, so the window's
    // own alpha stays 1.
    let alpha = select(clamp(shaded.opacity, 0.0, 1.0), 1.0, solid);
    let seen = fogged(frame.fog, frame.camera_position.xyz, in.world_position, color + shaded.emissive);
    return vec4<f32>(seen, alpha);
}

// Whether a fading item leaves out the pixel at `pixel`, in a fine pattern,
// so two meshes cross-fade without sorting or blending (level of detail, and
// foliage near its cull distance). A fade of 1 draws every pixel. Below 1,
// only the pixels whose noise is under it are drawn; above 1, only the
// others. So a mesh fading in at `t` and one fading out at `1 + t` draw
// each pixel exactly once between them.
fn dithered_out(fade: f32, pixel: vec2<f32>) -> bool {
    let noise = pixel_noise(pixel);
    return select(noise >= fade, noise < fade - 1.0, fade > 1.0);
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
    return prepass_normal(normalize(in.normal) * facing);
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
