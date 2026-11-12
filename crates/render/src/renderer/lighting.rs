//! Frame uniforms, tiled lights, shadow cascades and draw batches.
//!
//! CPU-side planning for one frame. The `Frame`, `Light` and instance layouts
//! must stay aligned with `shader.wgsl`. Shared GPU types live in `renderer.rs`.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use glam::camera::rh;
use glam::{Mat3, Mat4, Vec2, Vec3};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::DecalRole;
use crate::draw_list::{
    Camera, ChannelMap, DrawItem, DrawList, Light, Material, PostProcessing, Shading, Shadows,
    Skin, TextureSource,
};
use crate::graphics::{AmbientOcclusion, EffectiveGraphics, ShadowMapSize, ShadowQuality};

use super::culling::{Bounds, CullingBounds, Frustum, ShadowView, Sphere};
use super::lod::{Draw, OPAQUE};
use super::parallel::{map_each, map_indexes, map_parts, sort_unstable, worth_sharing};
use super::skinning::JointRaw;
use super::{
    Batch, CASCADE_SPLIT_BLEND, CONTACT_SHADOW_LENGTH, CONTACT_SHADOW_THICKNESS, FAR_PLANE,
    FrameUniform, InstanceRaw, LIGHT_SCALE, LIGHTS_PER_TILE, LightRaw, MATERIAL_TEXTURES,
    MAX_LIGHTS, MAX_POINT_SHADOWS, MAX_SPOT_SHADOW_DEGREES, MAX_SPOT_SHADOWS, MAX_SUN_SIZE,
    MAX_TILES, MIN_TILE_SIZE, MapsUniform, MaterialKey, MeshKey, NEAR_PLANE, POINT_SHADOW_SPREAD,
    PointShadow, PostUniform, SHADING_STANDARD, SHADING_TOON, SHADING_UNLIT, SHADOW_CASCADES,
    SHADOW_CASTER_RANGE, SpotShadow,
};
use crate::SURFACE_PARAMETERS;

/// The camera's view (world to view space) and projection (view to clip
/// space).
pub(super) fn camera_matrices(camera: &Camera, aspect: f32) -> (Mat4, Mat4) {
    let view = rh::view::look_at_mat4(camera.position, camera.look_at, Vec3::Y);
    // wgpu uses DirectX-style clip space: depth from 0 to 1, Y up.
    let projection = rh::proj::directx::perspective(
        camera.fov_y_degrees.to_radians(),
        aspect,
        NEAR_PLANE,
        FAR_PLANE,
    );
    (view, projection)
}

pub(super) fn view_projection(list: &DrawList, aspect: f32) -> Mat4 {
    camera_view_projection(&list.camera, aspect)
}

pub(super) fn camera_view_projection(camera: &Camera, aspect: f32) -> Mat4 {
    let (view, projection) = camera_matrices(camera, aspect);
    projection * view
}

/// How strongly ambient occlusion darkens ambient light as drawn: the
/// world's strength (0 to 2), or 0 while the graphics settings have it off.
pub(super) fn occlusion_drawn(list: &DrawList, graphics: &EffectiveGraphics) -> f32 {
    let strength = list.ambient.occlusion;
    if graphics.ambient_occlusion == AmbientOcclusion::Off || !strength.is_finite() {
        return 0.0;
    }
    strength.clamp(0.0, 2.0)
}

/// The sun's shadows as drawn: the world's request, within the graphics
/// settings' ceiling. Shadows draw only if both allow them, at the smaller
/// distance, with the settings' map. `None` when they're off.
pub(super) fn shadows_drawn(
    world: Shadows,
    graphics: &EffectiveGraphics,
) -> Option<(Shadows, ShadowMapSize)> {
    let map = graphics.shadow_quality.map()?;
    let usable = world.enabled && world.distance.is_finite() && world.distance > 0.0;
    let distance = world.distance.min(graphics.shadow_distance);
    (usable && distance > 0.0).then_some((Shadows { distance, ..world }, map))
}

/// Whether contact shadows are drawn: when the world's shadows are on and
/// the graphics settings allow them, whatever the shadow maps' quality.
pub(super) fn contact_drawn(world: Shadows, graphics: &EffectiveGraphics) -> bool {
    world.enabled && graphics.contact_shadows
}

/// The world's post-processing, turned off if the graphics settings don't allow it.
pub(super) fn post_drawn(world: &PostProcessing, graphics: &EffectiveGraphics) -> PostProcessing {
    PostProcessing {
        enabled: world.enabled && graphics.post_processing,
        ..*world
    }
}

pub(super) fn frame_uniform(
    list: &DrawList,
    aspect: f32,
    graphics: &EffectiveGraphics,
) -> FrameUniform {
    let camera = &list.camera;
    let (view, projection) = camera_matrices(camera, aspect);
    let view_proj = projection * view;
    let sun_direction = list.sun.direction.normalize_or(Vec3::Y);
    let sun_radiance = linear_color(list.sun.color) * list.sun.intensity;
    let ambient = linear_color(list.ambient.color) * list.ambient.intensity;
    // A NaN or infinite wind would throw swaying plants out of sight.
    let wind = if list.wind.is_finite() {
        list.wind
    } else {
        Vec3::ZERO
    };
    let shadows = shadow_setup(
        list,
        shadows_drawn(list.shadows, graphics),
        sun_direction,
        aspect,
    );
    let sky = &list.sky;
    let environment = if sky.environment_intensity.is_finite() {
        sky.environment_intensity.max(0.0)
    } else {
        0.0
    };

    FrameUniform {
        view_proj: view_proj.to_cols_array_2d(),
        camera_position: camera.position.extend(1.0).to_array(),
        sun_direction: sun_direction.extend(0.0).to_array(),
        sun_radiance: sun_radiance.extend(0.0).to_array(),
        ambient: ambient.extend(0.0).to_array(),
        cascades: shadows.cascades.map(|cascade| cascade.to_cols_array_2d()),
        cascade_texels: shadows.texels,
        cascade_depths: shadows.depths,
        shadow: shadows.settings,
        inv_view_proj: view_proj.inverse().to_cols_array_2d(),
        sky_top: linear_color(sky.top_color).extend(environment).to_array(),
        sky_horizon: linear_color(sky.horizon_color).extend(0.0).to_array(),
        sky_ground: linear_color(sky.ground_color).extend(0.0).to_array(),
        lights: [0; 4],
        spot_shadows: [Mat4::IDENTITY.to_cols_array_2d(); MAX_SPOT_SHADOWS],
        view: view.to_cols_array_2d(),
        // The scene's size is filled in when it's drawn.
        ambient_occlusion: [occlusion_drawn(list, graphics), 0.0, 1.0, 1.0],
        // The environment maps fill these in when they're on.
        environment: [0.0; 4],
        sky_light: [[0.0; 4]; 9],
        reflection_probes: [0; 4],
        probes: Default::default(),
        point_shadows: Default::default(),
        contact: [
            f32::from(u8::from(contact_drawn(list.shadows, graphics))),
            CONTACT_SHADOW_LENGTH,
            CONTACT_SHADOW_THICKNESS,
            0.0,
        ],
        light_probes: [0; 4],
        volumes: Default::default(),
        clock: [list.time, wind.x, wind.y, wind.z],
        // The sky's look fills this in.
        night: [0.0; 4],
        // So does the fog, which takes the sky's colors.
        fog: Default::default(),
        // And the clouds, which drift with the wind.
        clouds: Default::default(),
        weather: weather_uniform(&list.weather),
        // Filled in while ray traced shadows are drawn.
        ray_shadows: [0.0; 4],
    }
}

/// The weather as `Frame::weather` holds it: wetness and snow cover, each
/// held to 0 to 1 (a NaN is none).
fn weather_uniform(weather: &crate::Weather) -> [f32; 4] {
    let share = |value: f32| if value > 0.0 { value.min(1.0) } else { 0.0 };
    [share(weather.wetness), share(weather.snow), 0.0, 0.0]
}

/// The lights to draw, and which of them reach each tile of the screen.
pub(super) struct PlannedLights {
    pub(super) lights: Vec<LightRaw>,
    /// For each tile, row by row: how many lights reach it, then up to
    /// [`LIGHTS_PER_TILE`] indexes into `lights`.
    pub(super) tiles: Vec<u32>,
    pub(super) tile_size: u32,
    pub(super) tiles_across: u32,
    /// The spot lights casting shadows, by their layer of the spot shadow map.
    pub(super) spot_shadows: Vec<SpotShadow>,
    /// The point lights casting shadows, by their place in the point shadow
    /// map (six layers each).
    pub(super) point_shadows: Vec<PointShadow>,
}

/// Picks the lights to draw (the nearest [`MAX_LIGHTS`]) and sorts them into
/// screen tiles, so each pixel only loops over the few lights that can
/// reach it instead of all of them. A light goes in every tile that its
/// sphere of reach could touch; tiles with more than [`LIGHTS_PER_TILE`]
/// keep the nearest ones to the camera. Of the spot lights that cast
/// shadows, the nearest in view get a layer of `quality`'s spot map each,
/// as many as it has, and likewise the point lights six layers each of its
/// point map; the rest light without shadows.
pub(super) fn plan_lights(
    list: &DrawList,
    aspect: f32,
    (width, height): (u32, u32),
    quality: ShadowQuality,
) -> PlannedLights {
    let camera = list.camera.position;
    let mut lights: Vec<&Light> = list
        .lights
        .iter()
        .filter(|light| {
            light.position.is_finite()
                && light.range.is_finite()
                && light.range > 0.0
                && light.brightness.is_finite()
                && light.brightness > 0.0
        })
        .collect();
    lights.sort_by(|a, b| {
        let (a, b) = (
            a.position.distance_squared(camera),
            b.position.distance_squared(camera),
        );
        a.total_cmp(&b)
    });
    lights.truncate(MAX_LIGHTS);

    let mut tile_size = MIN_TILE_SIZE;
    while (width.div_ceil(tile_size) * height.div_ceil(tile_size)) as usize > MAX_TILES {
        tile_size *= 2;
    }
    let (across, down) = (width.div_ceil(tile_size), height.div_ceil(tile_size));
    let mut planned = PlannedLights {
        lights: Vec::new(),
        tiles: Vec::new(),
        tile_size,
        tiles_across: across,
        spot_shadows: Vec::new(),
        point_shadows: Vec::new(),
    };
    if lights.is_empty() || width == 0 || height == 0 {
        return planned;
    }

    let view_proj = view_projection(list, aspect);
    let stride = LIGHTS_PER_TILE + 1;
    planned.tiles = vec![0; (across * down) as usize * stride];
    // Lights are nearest first, so the nearest that cast shadows get layers.
    let (spot_map, point_map) = (quality.spot_map(), quality.point_map());
    let spot_layers = spot_map.map_or(0, |map| (map.lights as usize).min(MAX_SPOT_SHADOWS));
    let point_lights = point_map.map_or(0, |map| (map.lights as usize).min(MAX_POINT_SHADOWS));
    for light in lights {
        let Some((first, last)) = light_tiles(view_proj, light, (width, height), tile_size) else {
            continue;
        };
        let index = planned.lights.len() as u32;
        let mut raw = light_raw(light);
        if planned.spot_shadows.len() < spot_layers
            && let Some(map) = spot_map
            && let Some((shadow, texel)) = spot_shadow(light, map.size)
        {
            raw.shadow = [planned.spot_shadows.len() as f32, texel, 0.0, 0.0];
            planned.spot_shadows.push(shadow);
        } else if planned.point_shadows.len() < point_lights
            && let Some(map) = point_map
            && let Some((shadow, texel)) = point_shadow(light, map.size)
        {
            raw.shadow = [planned.point_shadows.len() as f32, texel, 0.0, 0.0];
            planned.point_shadows.push(shadow);
        }
        planned.lights.push(raw);
        for y in first.1..=last.1 {
            for x in first.0..=last.0 {
                let tile = (y * across + x) as usize * stride;
                let count = planned.tiles[tile] as usize;
                if count < LIGHTS_PER_TILE {
                    planned.tiles[tile + 1 + count] = index;
                    planned.tiles[tile] += 1;
                }
            }
        }
    }
    planned
}

/// The first and last tile (column, row) a light's sphere of reach could
/// touch, or `None` if it's entirely out of view. Works from the sphere's
/// bounding box, which is slightly generous but never misses.
pub(super) fn light_tiles(
    view_proj: Mat4,
    light: &Light,
    (width, height): (u32, u32),
    tile_size: u32,
) -> Option<((u32, u32), (u32, u32))> {
    let (mut min, mut max) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
    let mut behind = 0;
    for corner in 0..8 {
        let offset = Vec3::new(
            if corner & 1 == 0 { -1.0 } else { 1.0 },
            if corner & 2 == 0 { -1.0 } else { 1.0 },
            if corner & 4 == 0 { -1.0 } else { 1.0 },
        );
        let clip = view_proj * (light.position + offset * light.range).extend(1.0);
        if clip.w <= NEAR_PLANE * 0.5 {
            behind += 1;
            continue;
        }
        let ndc = Vec2::new(clip.x, clip.y) / clip.w;
        min = min.min(ndc);
        max = max.max(ndc);
    }
    if behind == 8 {
        return None;
    }
    // A box that reaches behind the camera could cover anywhere on screen.
    if behind > 0 {
        (min, max) = (Vec2::splat(-1.0), Vec2::splat(1.0));
    }
    if max.x < -1.0 || min.x > 1.0 || max.y < -1.0 || min.y > 1.0 {
        return None;
    }
    let to_pixels = |ndc: Vec2| {
        Vec2::new(
            (ndc.x * 0.5 + 0.5) * width as f32,
            (0.5 - ndc.y * 0.5) * height as f32,
        )
    };
    let (a, b) = (to_pixels(min), to_pixels(max));
    let tile = |pixel: f32, limit: u32| -> u32 {
        ((pixel / tile_size as f32).floor().max(0.0) as u32).min(limit.div_ceil(tile_size) - 1)
    };
    // The screen's Y grows downward, so the NDC maximum is the top.
    Some((
        (tile(a.x, width), tile(b.y, height)),
        (tile(b.x, width), tile(a.y, height)),
    ))
}

pub(super) fn light_raw(light: &Light) -> LightRaw {
    let radiance = linear_color(light.color) * light.brightness * LIGHT_SCALE;
    let mut rect = [0.0; 4];
    let (cone_edge, direction, cone_inner) = match (light.cone, light.rect) {
        (None, None) => (-2.0, Vec3::NEG_Y, 0.0),
        (None, Some(area)) => {
            let facing = area.direction.normalize_or(Vec3::NEG_Y);
            // Square to the way it faces, however `right` was given.
            let right = (area.right - facing * area.right.dot(facing))
                .normalize_or(facing.any_orthogonal_vector());
            let half_width = area.width.max(0.0) * 0.5;
            rect = (right * half_width)
                .extend(area.height.max(0.0) * 0.5)
                .to_array();
            (-3.0, facing, 0.0)
        }
        (Some(cone), _) => {
            let half = (cone.angle_degrees.clamp(1.0, 179.0) * 0.5).to_radians();
            (
                half.cos(),
                cone.direction.normalize_or(Vec3::NEG_Y),
                (half * 0.8).cos(),
            )
        }
    };
    LightRaw {
        position_range: light.position.extend(light.range).to_array(),
        radiance_cone: radiance.extend(cone_edge).to_array(),
        direction_inner: direction.extend(cone_inner).to_array(),
        shadow: [-1.0, 0.0, 0.0, 0.0],
        rect,
    }
}

/// A spot light's shadow view, if it casts shadows, with the width of one of
/// its `size`² map's texels per stud from the light. The view looks down
/// the beam with a perspective as wide as the cone (at most
/// [`MAX_SPOT_SHADOW_DEGREES`]), from the shadows' near distance out to the
/// light's range.
pub(super) fn spot_shadow(light: &Light, size: u32) -> Option<(SpotShadow, f32)> {
    let cone = light.cone?;
    let shadows = cone.shadows?;
    let direction = cone.direction.normalize_or(Vec3::NEG_Y);
    let angle = cone.angle_degrees.clamp(1.0, 179.0);
    // A little wider than the beam, so filtering at its edge stays inside.
    let fov = (angle * 1.05).min(MAX_SPOT_SHADOW_DEGREES).to_radians();
    let near = if shadows.near.is_finite() {
        shadows.near.max(0.05)
    } else {
        0.05
    };
    if near >= light.range {
        return None;
    }
    let up = if direction.y.abs() > 0.99 {
        Vec3::X
    } else {
        Vec3::Y
    };
    let view = rh::view::look_at_mat4(light.position, light.position + direction, up);
    let projection = rh::proj::directx::perspective(fov, 1.0, near, light.range);
    let texel = 2.0 * (fov * 0.5).tan() / size as f32;
    let shadow = SpotShadow {
        view_proj: projection * view,
        bounds: cone_bounds(light.position, direction, angle, light.range),
    };
    Some((shadow, texel))
}

/// A point light's six shadow views, if it casts shadows, with the width of
/// one of its `size`² faces' texels per stud from the light. Each face
/// looks along one way of a world axis (+X, -X, +Y, -Y, +Z, -Z), a little
/// over 90° wide ([`POINT_SHADOW_SPREAD`]), from that side's near distance
/// out to the light's range.
pub(super) fn point_shadow(light: &Light, size: u32) -> Option<(PointShadow, f32)> {
    if light.cone.is_some() {
        return None;
    }
    let shadows = light.point_shadows?;
    let fov = 2.0 * POINT_SHADOW_SPREAD.atan();
    let axes = [
        (Vec3::X, Vec3::NEG_Y),
        (Vec3::NEG_X, Vec3::NEG_Y),
        (Vec3::Y, Vec3::Z),
        (Vec3::NEG_Y, Vec3::NEG_Z),
        (Vec3::Z, Vec3::NEG_Y),
        (Vec3::NEG_Z, Vec3::NEG_Y),
    ];
    let mut faces = [Mat4::IDENTITY; 6];
    for (face, ((direction, up), near)) in faces.iter_mut().zip(axes.into_iter().zip(shadows.near))
    {
        // Past the range nothing is lit, so that side casts nothing.
        let near = if near.is_finite() {
            near.max(0.05)
        } else {
            0.05
        };
        let near = near.min(light.range * 0.99);
        let view = rh::view::look_at_mat4(light.position, light.position + direction, up);
        let projection = rh::proj::directx::perspective(fov, 1.0, near, light.range);
        *face = projection * view;
    }
    let texel = 2.0 * POINT_SHADOW_SPREAD / size as f32;
    let shadow = PointShadow {
        faces,
        bounds: (light.position, light.range),
    };
    Some((shadow, texel))
}

/// The smallest sphere around a beam from `apex` along `direction`,
/// `angle_degrees` wide, reaching `range` studs: the part of the light's
/// sphere of reach inside the cone.
pub(super) fn cone_bounds(
    apex: Vec3,
    direction: Vec3,
    angle_degrees: f32,
    range: f32,
) -> (Vec3, f32) {
    let half = (angle_degrees * 0.5).to_radians();
    let (sin, cos) = half.sin_cos();
    if half <= std::f32::consts::FRAC_PI_4 {
        // Narrow: the sphere through the apex and the far rim.
        let radius = range / (2.0 * cos);
        (apex + direction * radius, radius)
    } else if half < std::f32::consts::FRAC_PI_2 {
        // Wide: the sphere around the rim, which holds the apex and the cap.
        (apex + direction * range * cos, range * sin)
    } else {
        (apex, range)
    }
}

/// One shadow view's casters: their instance data and how they're drawn.
pub(super) struct ViewCasters {
    /// Their instances, in pieces (each thread's share).
    pub(super) instances: Vec<Vec<InstanceRaw>>,
    /// Which casters to draw, grouped by mesh, counting from the view's
    /// first instance.
    pub(super) groups: Vec<(MeshKey, Range<u32>)>,
    /// How many casters were left out.
    pub(super) left_out: u32,
}

/// Each of `views`' casters, in order. Each view and each share of the
/// casters is its own job for the CPU's threads.
pub(super) fn casters_in_views(casters: &[Vec<Caster>], views: &[ShadowView]) -> Vec<ViewCasters> {
    let share = worth_sharing(casters.iter().map(Vec::len).sum());
    map_each(views, share, |view| {
        if matches!(view, ShadowView::Skipped) {
            return ViewCasters {
                instances: Vec::new(),
                groups: Vec::new(),
                left_out: 0,
            };
        }
        let parts = map_each(casters, share, |casters| {
            let mut instances = Vec::with_capacity(casters.len());
            let mut groups: Vec<(MeshKey, Range<u32>)> = Vec::new();
            let mut left_out = 0;
            for caster in casters {
                if !view.touches(caster.sphere) {
                    left_out += 1;
                    continue;
                }
                let copy = instances.len() as u32;
                instances.push(caster.raw);
                match groups.last_mut() {
                    Some((mesh, range)) if *mesh == caster.mesh && range.end == copy => {
                        range.end = copy + 1;
                    }
                    _ => groups.push((caster.mesh, copy..copy + 1)),
                }
            }
            (instances, groups, left_out)
        });
        let mut joined = ViewCasters {
            instances: Vec::with_capacity(parts.len()),
            groups: Vec::new(),
            left_out: 0,
        };
        let mut count = 0;
        for (instances, groups, left_out) in parts {
            joined.left_out += left_out;
            for (mesh, range) in groups {
                let range = range.start + count..range.end + count;
                match joined.groups.last_mut() {
                    // A group cut in two by where a share of the work ended.
                    Some((last, joined)) if *last == mesh && joined.end == range.start => {
                        joined.end = range.end;
                    }
                    _ => joined.groups.push((mesh, range)),
                }
            }
            count += instances.len() as u32;
            joined.instances.push(instances);
        }
        joined
    })
}

pub(super) fn post_uniform(post: &PostProcessing) -> PostUniform {
    let flag = |on: bool| if on { 1.0 } else { 0.0 };
    let exposure = if post.enabled && post.exposure.is_finite() {
        post.exposure.max(0.0)
    } else {
        1.0
    };
    PostUniform {
        values: [
            post.bloom_threshold.max(0.0),
            post.bloom_intensity.max(0.0),
            exposure,
            0.0,
        ],
        flags: [
            flag(post.enabled && post.bloom_enabled),
            flag(post.enabled),
            post.tone_mapping.index(),
            0.0,
        ],
        // No lens effects until `lens::add_lens` adds them.
        lens: [0.0, 0.0, 0.0, 1.0],
        vignette: [0.0; 4],
        vignette_color: [0.0; 4],
    }
}

/// The shadow values for the shader (see [`FrameUniform`]).
pub(super) struct ShadowSetup {
    cascades: [Mat4; SHADOW_CASCADES],
    texels: [f32; 4],
    depths: [f32; 4],
    settings: [f32; 4],
}

/// Where each of `count` shadow cascades ends, in studs from the camera
/// along the view. The ends are spread so each cascade reaches a few times
/// further than the one before (see [`CASCADE_SPLIT_BLEND`]), and the last
/// ends at `distance`. Entries past `count` are `distance` too.
pub(super) fn cascade_ends(distance: f32, count: usize) -> [f32; SHADOW_CASCADES] {
    // From 1 stud, not the near plane, or the first cascade would be tiny.
    let near = 1.0_f32.min(distance * 0.5);
    let count = count.clamp(1, SHADOW_CASCADES);
    std::array::from_fn(|index| {
        let fraction = ((index + 1) as f32 / count as f32).min(1.0);
        let even = near + (distance - near) * fraction;
        let growing = near * (distance / near).powf(fraction);
        even + (growing - even) * CASCADE_SPLIT_BLEND
    })
}

/// The sun's views of the shadow cascades and the shadow values for the
/// shader. Each cascade covers one slice of the view, so the nearest
/// shadows get the most texels per stud and are the sharpest. The area of
/// each is a sphere around its slice, so turning the camera doesn't change
/// the map's scale, and its position is snapped to whole texels so shadow
/// edges don't shimmer as the camera moves. `drawn` is the shadows as
/// drawn and their map (see [`shadows_drawn`]), or `None` when they're off.
pub(super) fn shadow_setup(
    list: &DrawList,
    drawn: Option<(Shadows, ShadowMapSize)>,
    sun_direction: Vec3,
    aspect: f32,
) -> ShadowSetup {
    let mut setup = ShadowSetup {
        cascades: [Mat4::IDENTITY; SHADOW_CASCADES],
        texels: [1.0; 4],
        depths: [1.0; 4],
        settings: [0.0; 4],
    };
    let Some((settings, map)) = drawn else {
        return setup;
    };
    let cascades = map.cascades as usize;
    let camera = &list.camera;
    let forward = (camera.look_at - camera.position).normalize_or(Vec3::NEG_Z);
    let distance = settings.distance.clamp(NEAR_PLANE * 4.0, FAR_PLANE);
    let tan_half = (camera.fov_y_degrees.to_radians() * 0.5).tan();
    let k2 = tan_half * tan_half * (1.0 + aspect * aspect);

    // Looking from the sun toward the scene.
    let toward_sun = sun_direction;
    let up_hint = if toward_sun.y.abs() > 0.99 {
        Vec3::X
    } else {
        Vec3::Y
    };
    let right = (-toward_sun).cross(up_hint).normalize();
    let up = right.cross(-toward_sun);

    let mut slice_near = NEAR_PLANE;
    let ends = cascade_ends(distance, cascades);
    for (index, far) in ends.into_iter().take(cascades).enumerate() {
        // The smallest sphere around the slice of the view between
        // `slice_near` and `far`, centered on the view direction `center` away.
        let center = (0.5 * (far + slice_near) * (1.0 + k2)).min(far);
        let radius = ((far - center).powi(2) + far * far * k2).sqrt().max(1.0);
        let sphere_center = camera.position + forward * center;
        slice_near = far;

        let texel = 2.0 * radius / map.size as f32;
        let snap = |distance: f32| (distance / texel).round() * texel;
        let snapped_center = right * snap(sphere_center.dot(right))
            + up * snap(sphere_center.dot(up))
            + toward_sun * sphere_center.dot(toward_sun);

        let back = radius + SHADOW_CASTER_RANGE;
        let eye = snapped_center + toward_sun * back;
        let view = rh::view::look_at_mat4(eye, snapped_center, up);
        let (near, far_depth) = (0.1, back + radius);
        let projection =
            rh::proj::directx::orthographic(-radius, radius, -radius, radius, near, far_depth);
        setup.cascades[index] = projection * view;
        setup.texels[index] = texel;
        setup.depths[index] = far_depth - near;
    }
    let softness = if settings.softness.is_finite() {
        settings.softness.clamp(0.0, 1.0)
    } else {
        0.0
    };
    setup.settings = [
        1.0,
        softness * MAX_SUN_SIZE,
        cascades as f32,
        settings.distance,
    ];
    setup
}

/// What a frame's draws use, each thing once, so that the work per material
/// and per mesh follows how many there are and not how many draws share them.
pub(super) struct FrameUses<'a> {
    /// Every distinct material binding. The first is the plain one.
    pub(super) materials: Vec<MaterialKey>,
    /// Which of `materials` each draw is drawn with.
    pub(super) material_of: Vec<u32>,
    /// A draw using each of `materials`, the first one (0 for the plain
    /// material, which may have none).
    pub(super) first_draw: Vec<u32>,
    /// Every distinct mesh of a caller's the draws use, by address.
    pub(super) meshes: Vec<&'a Arc<dyn crate::MeshSource>>,
}

impl<'a> FrameUses<'a> {
    /// Works out what `draws` use, on the CPU's threads.
    pub(super) fn of(draws: &'a [Draw<'_>]) -> Self {
        // A part's materials, each once, with the first draw of the part
        // to use it.
        let parts = map_parts(draws, |draws| {
            let mut distinct: Vec<(MaterialKey, u32)> = Vec::new();
            let mut indexes: FxHashMap<MaterialKey, u32> = FxHashMap::default();
            let mut material_of = Vec::with_capacity(draws.len());
            let mut seen_meshes: FxHashSet<*const ()> = FxHashSet::default();
            let mut meshes = Vec::new();
            for (at, draw) in draws.iter().enumerate() {
                let key = material_key(draw.item);
                let index = *indexes.entry(key).or_insert_with(|| {
                    distinct.push((key, at as u32));
                    (distinct.len() - 1) as u32
                });
                material_of.push(index);
                if let Some(source) = draw.source()
                    && seen_meshes.insert(Arc::as_ptr(source).cast())
                {
                    meshes.push(source);
                }
            }
            (draws.len() as u32, distinct, material_of, meshes)
        });
        let mut uses = Self {
            materials: vec![MaterialKey::default()],
            material_of: Vec::with_capacity(draws.len()),
            first_draw: vec![0],
            meshes: Vec::new(),
        };
        let mut indexes: FxHashMap<MaterialKey, u32> = FxHashMap::default();
        indexes.insert(MaterialKey::default(), 0);
        let mut seen_meshes: FxHashSet<*const ()> = FxHashSet::default();
        let mut start = 0;
        for (len, distinct, material_of, meshes) in parts {
            let remap: Vec<u32> = distinct
                .into_iter()
                .map(|(key, first)| {
                    *indexes.entry(key).or_insert_with(|| {
                        uses.materials.push(key);
                        uses.first_draw.push(start + first);
                        (uses.materials.len() - 1) as u32
                    })
                })
                .collect();
            uses.material_of
                .extend(material_of.into_iter().map(|index| remap[index as usize]));
            uses.meshes.extend(
                meshes
                    .into_iter()
                    .filter(|source| seen_meshes.insert(Arc::as_ptr(source).cast())),
            );
            start += len;
        }
        uses
    }
}

/// A solid item that casts a shadow, whether or not the camera sees it.
#[derive(Clone, Copy)]
pub(super) struct Caster {
    pub(super) mesh: MeshKey,
    pub(super) sphere: Sphere,
    pub(super) raw: InstanceRaw,
}

/// What a frame draws (see [`plan_draws`]).
#[derive(Default)]
pub(super) struct DrawPlan {
    /// The scene pass's instances, in pieces (each thread's share, in
    /// order). Shadow views copy theirs after these.
    pub(super) instances: Vec<Vec<InstanceRaw>>,
    /// The scene pass's batches: the items in the camera's view.
    pub(super) batches: Vec<Batch>,
    /// Every solid item that casts a shadow, in view or not, grouped by
    /// mesh: one out of view can still shadow what's in it. In pieces, as
    /// `instances` are.
    pub(super) casters: Vec<Vec<Caster>>,
    /// Items outside the camera's view.
    pub(super) culled: u32,
    /// Every drawn skin's joints, each skin's after the one before
    /// (`InstanceRaw::skin` says where an item's start).
    pub(super) joints: Vec<JointRaw>,
    /// Where each drawn skin's joints are in `joints` (start and count), by
    /// the skin's address, for the motion vectors' instances.
    pub(super) skins: HashMap<*const Skin, [u32; 2]>,
    /// Items in view with levels of detail, by level (see
    /// [`RenderStats::lod_items`](crate::RenderStats::lod_items)).
    pub(super) lod_items: [u32; crate::LOD_LEVELS],
}

/// Puts the `draws` (`list`'s items at their levels of detail, see
/// [`LodStates::resolve`](super::lod::LodStates::resolve)) that `view` can
/// see in drawing order and groups them into batches, and lists the shadow
/// casters for the shadow views to pick from.
///
/// Solid items come first, grouped by mesh, texture and sidedness. See-through items come last,
/// farthest from the camera first, so each one blends over what's behind it.
/// They're sorted by their centers, so two that overlap can still blend in the
/// wrong order; that's the usual trade-off for this simple approach.
///
/// An item is culled when its bounding sphere is wholly outside `view`.
/// `bounds` gives each shape's box in its own space, posed by the item's
/// skin if it has one: the unit cube for the built-in shapes, its corners'
/// box for a caller's mesh. Each item's sphere and whether it was drawn go
/// in `seen`, if given.
pub(super) fn plan_draws(
    list: &DrawList,
    draws: &[Draw],
    uses: &FrameUses,
    view: &Frustum,
    bounds: impl Fn(MeshKey, Option<&Skin>) -> Bounds + Sync,
    seen: Option<&mut Vec<CullingBounds>>,
) -> DrawPlan {
    debug_assert_eq!(draws.len(), uses.material_of.len());
    // Each draw's sort key, worked out once: its mesh, its material as an
    // index among the frame's distinct materials (the plain one is 0), and
    // its pipeline. A material key is hundreds of bytes, so the sort
    // compares these instead.
    let keys = map_indexes(draws.len(), |index| {
        let draw = &draws[index];
        let key = SortKey {
            band: distance_band(draw.item.transform.w_axis.truncate(), list.camera.position),
            mesh: draw.mesh(),
            material: uses.material_of[index],
            shader: shader_key(draw.item),
            double_sided: draw.item.material.double_sided,
            casts_shadow: draw.casts_shadow,
            cut_out: draw.item.material.alpha_cutoff > 0.0,
        };
        (key, index as u32)
    });
    // Water is drawn with the see-through items, before them (see
    // renderer/water.rs).
    let water = |index: u32| draws[index as usize].item.material.water.is_some();
    // A Part's Decal is blended by its texture's alpha even when solid.
    let decal = |index: u32| draws[index as usize].item.decal_role == DecalRole::Decal;
    let (mut transparent, mut opaque): (Vec<_>, Vec<_>) = keys
        .iter()
        .copied()
        .filter(|(_, index)| draws[*index as usize].item.material.opacity > 0.0)
        .partition(|(_, index)| {
            draws[*index as usize].item.material.opacity < 1.0 || water(*index) || decal(*index)
        });

    // By key then index, which keeps the draws' order where keys are equal.
    sort_unstable(&mut opaque);
    let distance = |index: u32| {
        draws[index as usize]
            .item
            .transform
            .w_axis
            .truncate()
            .distance_squared(list.camera.position)
    };
    // `total_cmp` orders floats even when one is NaN, so sorting can't panic.
    transparent.sort_by(|(_, a), (_, b)| {
        water(*b)
            .cmp(&water(*a))
            .then_with(|| distance(*b).total_cmp(&distance(*a)))
    });

    let ordered: Vec<(SortKey, u32, bool)> = opaque
        .into_iter()
        .map(|(key, index)| (key, index, false))
        .chain(
            transparent
                .into_iter()
                .map(|(key, index)| (key, index, true)),
        )
        .collect();
    let keep_seen = seen.is_some();
    let parts = map_parts(&ordered, |part| {
        plan_part(part, draws, uses, view, &bounds, keep_seen)
    });
    join_plan(parts, seen)
}

/// What makes two draws one batch.
type BatchKind = (SortKey, bool, bool, bool);

/// A skin's joints wanted by an instance or caster of a [`PlanPart`], each
/// counted from the part's start.
struct SkinUse<'a> {
    skin: &'a Skin,
    instance: Option<u32>,
    caster: Option<u32>,
}

/// One thread's share of [`plan_draws`], with its instances and casters
/// counted from its own start.
#[derive(Default)]
struct PlanPart<'a> {
    instances: Vec<InstanceRaw>,
    batches: Vec<(BatchKind, Batch)>,
    casters: Vec<Caster>,
    culled: u32,
    lod_items: [u32; crate::LOD_LEVELS],
    seen: Vec<CullingBounds>,
    skins: Vec<SkinUse<'a>>,
}

/// Plans the draws `ordered` picks, in that order.
fn plan_part<'a>(
    ordered: &[(SortKey, u32, bool)],
    draws: &[Draw<'a>],
    uses: &FrameUses,
    view: &Frustum,
    bounds: &(impl Fn(MeshKey, Option<&Skin>) -> Bounds + Sync),
    keep_seen: bool,
) -> PlanPart<'a> {
    let mut part = PlanPart::default();
    // Items sharing a mesh are next to each other, so its bounds are looked
    // up once per run of them (unless a skin moves them).
    let mut last_bounds: Option<(MeshKey, Bounds)> = None;
    let mut colors = LinearColors::default();
    // The last batch's key and kind, to add to it without comparing the
    // batch's material key.
    let mut last_batch: Option<BatchKind> = None;
    for &(key, at, transparent) in ordered {
        let draw = &draws[at as usize];
        let mesh = key.mesh;
        let item = draw.item;
        let skin = item.skin.as_deref();
        let shape_bounds = match (last_bounds, skin) {
            (_, Some(skin)) => bounds(mesh, Some(skin)),
            (Some((last, bounds)), None) if last == mesh => bounds,
            (_, None) => {
                let bounds = bounds(mesh, None);
                last_bounds = Some((mesh, bounds));
                bounds
            }
        };
        let (center, radius) = shape_bounds.sphere(item.transform);
        // A swaying plant reaches past where it rests.
        let sphere = (center, radius + item.sway.map_or(0.0, |sway| sway.reach()));
        let visible = view.touches(sphere);
        if keep_seen {
            part.seen.push(CullingBounds {
                center: sphere.0,
                radius: sphere.1,
                drawn: visible,
            });
        }
        let caster = !transparent && draw.casts_shadow;
        if !visible {
            part.culled += 1;
            if !caster {
                continue;
            }
        }
        let Some(mut raw) = instance_raw_with(item, &mut colors) else {
            continue;
        };
        let caster_at = caster.then(|| {
            part.casters.push(Caster { mesh, sphere, raw });
            (part.casters.len() - 1) as u32
        });
        raw.emissive_fade[3] = draw.fade;
        let mut instance_at = None;
        if visible {
            // The copy fading out of a switch isn't counted.
            if let Some(level) = draw.level
                && draw.fade <= OPAQUE
            {
                part.lod_items[level] += 1;
            }
            let index = part.instances.len() as u32;
            instance_at = Some(index);
            part.instances.push(raw);
            let water = item.material.water.is_some();
            let outline = !transparent && outline_uniform(&item.material)[3] > 0.0;
            let kind = (key, transparent, water, outline);
            match part.batches.last_mut() {
                Some((_, last)) if last_batch == Some(kind) => {
                    last.instances.end = index + 1;
                }
                _ => {
                    last_batch = Some(kind);
                    part.batches.push((
                        kind,
                        Batch {
                            mesh,
                            material: uses.materials[key.material as usize],
                            transparent,
                            double_sided: key.double_sided,
                            casts_shadow: key.casts_shadow,
                            shader: key.shader,
                            water,
                            outline,
                            // A surface shader or terrain decides its own
                            // holes, and water isn't drawn with depth.
                            prefilled: key.cut_out
                                && !transparent
                                && !water
                                && key.shader.is_none()
                                && item.material.terrain.is_none(),
                            instances: index..index + 1,
                        },
                    ));
                }
            }
        }
        if let Some(skin) = skin {
            part.skins.push(SkinUse {
                skin,
                instance: instance_at,
                caster: caster_at,
            });
        }
    }
    part
}

/// Puts `parts` together, in order: a batch cut in two by where a share of
/// the work ended is made whole, and each skin's joints are added once.
fn join_plan(parts: Vec<PlanPart>, seen: Option<&mut Vec<CullingBounds>>) -> DrawPlan {
    let mut plan = DrawPlan::default();
    let mut first_instance = 0;
    let mut last_kind: Option<BatchKind> = None;
    // Where each skin's joints start, by the skin's address: items posed
    // alike share one.
    let mut skins: HashMap<*const Skin, [u32; 2]> = HashMap::new();
    let mut seen = seen;
    for mut part in parts {
        plan.culled += part.culled;
        for (level, count) in part.lod_items.into_iter().enumerate() {
            plan.lod_items[level] += count;
        }
        if let Some(seen) = seen.as_deref_mut() {
            seen.append(&mut part.seen);
        }
        for (kind, mut batch) in part.batches {
            batch.instances =
                batch.instances.start + first_instance..batch.instances.end + first_instance;
            match plan.batches.last_mut() {
                Some(last) if last_kind == Some(kind) => last.instances.end = batch.instances.end,
                _ => {
                    last_kind = Some(kind);
                    plan.batches.push(batch);
                }
            }
        }
        for used in part.skins {
            let joints = &mut plan.joints;
            let skin = *skins
                .entry(std::ptr::from_ref(used.skin))
                .or_insert_with(|| {
                    let first = joints.len() as u32;
                    joints.extend(JointRaw::of(used.skin));
                    [first, joints.len() as u32 - first]
                });
            if let Some(at) = used.instance {
                part.instances[at as usize].skin = skin;
            }
            if let Some(at) = used.caster {
                part.casters[at as usize].raw.skin = skin;
            }
        }
        first_instance += part.instances.len() as u32;
        plan.instances.push(part.instances);
        plan.casters.push(part.casters);
    }
    plan.skins = skins;
    plan
}

/// What a draw is sorted and batched by: draws with equal keys draw as one
/// batch. Small, so sorting thousands of them compares little.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct SortKey {
    /// Drawn nearest first: what's hidden behind near items never reaches
    /// the (costly) surface shader. Equal distances batch together.
    band: u8,
    mesh: MeshKey,
    /// Its material's index among the frame's distinct material keys.
    material: u32,
    /// Its surface shader's key, or `None` for the standard material.
    shader: Option<u64>,
    double_sided: bool,
    casts_shadow: bool,
    /// Whether it has holes by its texture's alpha.
    cut_out: bool,
}

/// Which distance band a point `from` the camera is in, the nearer the
/// smaller: bands widen by half with distance, so a forest's many layers of
/// trees are drawn near to far in a handful of batches of each mesh.
fn distance_band(point: Vec3, camera: Vec3) -> u8 {
    let distance = point.distance(camera);
    if distance.is_nan() {
        return u8::MAX;
    }
    (((distance / NEAREST_BAND).max(1.0).log2() * BANDS_PER_DOUBLING) as u8).min(u8::MAX - 1)
}

/// How far the nearest distance band reaches, in studs.
const NEAREST_BAND: f32 = 12.0;
/// How many distance bands each doubling of the distance is cut into.
const BANDS_PER_DOUBLING: f32 = 1.7;

/// The key of the surface shader `item` is drawn with, or `None` for the
/// standard material's. Terrain has its own shader.
fn shader_key(item: &DrawItem) -> Option<u64> {
    let material = &item.material;
    if material.terrain.is_some() || material.water.is_some() {
        return None;
    }
    material.surface_shader.as_ref().map(|shader| shader.key())
}

/// The textures an item is drawn with, in binding order (see
/// [`MATERIAL_TEXTURES`]). Terrain draws its own layers instead.
pub(super) fn material_textures(
    item: &DrawItem,
) -> [Option<&Arc<dyn TextureSource>>; MATERIAL_TEXTURES] {
    if item.material.terrain.is_some() {
        return [None; MATERIAL_TEXTURES];
    }
    let material = &item.material;
    let maps = &material.maps;
    [
        material.base_color_texture.as_ref(),
        maps.normal.as_ref().map(|map| &map.texture),
        maps.metalness.as_ref().map(|map| &map.texture),
        maps.roughness.as_ref().map(|map| &map.texture),
        maps.occlusion.as_ref().map(|map| &map.texture),
        maps.emissive.as_ref(),
        maps.opacity.as_ref().map(|map| &map.texture),
        // A toon ramp takes the specular map's place.
        match &material.shading {
            Shading::Toon(toon) if material.water.is_none() => toon.ramp.as_ref(),
            _ => maps.specular.as_ref(),
        },
    ]
}

/// [`MapsUniform::shading`] and [`MapsUniform::toon`] for `material`.
/// Water draws its own way, as standard.
fn shading_uniform(material: &Material) -> ([f32; 4], [f32; 4]) {
    if material.water.is_some() {
        return ([SHADING_STANDARD, 0.0, 0.0, 0.0], [0.0; 4]);
    }
    match &material.shading {
        Shading::Standard => (
            [
                SHADING_STANDARD,
                material.translucency.clamp(0.0, 1.0),
                0.0,
                0.0,
            ],
            [0.0; 4],
        ),
        Shading::Unlit => ([SHADING_UNLIT, 0.0, 0.0, 0.0], [0.0; 4]),
        Shading::Toon(toon) => {
            let steps = if toon.ramp.is_some() {
                0.0
            } else {
                toon.steps.max(1) as f32
            };
            (
                [
                    SHADING_TOON,
                    steps,
                    toon.softness.clamp(0.0, 1.0),
                    toon.highlight.clamp(0.0, 1.0),
                ],
                [toon.rim.max(0.0), 0.0, 0.0, 0.0],
            )
        }
    }
}

/// [`MapsUniform::outline`] for `material`: none for water and terrain.
fn outline_uniform(material: &Material) -> [f32; 4] {
    match material.outline {
        Some(outline)
            if outline.width > 0.0 && material.water.is_none() && material.terrain.is_none() =>
        {
            linear_color(outline.color).extend(outline.width).to_array()
        }
        _ => [0.0; 4],
    }
}

/// The material binding an item is drawn with (see [`MaterialKey`]).
pub(super) fn material_key(item: &DrawItem) -> MaterialKey {
    if let Some(surface) = &item.material.terrain {
        return MaterialKey {
            terrain: super::terrain::terrain_key(surface),
            ..MaterialKey::default()
        };
    }
    let textures = material_textures(item).map(|source| source.map(|source| source.key()));
    // Water's values ride in the surface shader's place: water has none.
    let mut params = item.material.surface_parameters;
    if let Some(water) = &item.material.water {
        params = [[0.0; 4]; SURFACE_PARAMETERS];
        params[..3].copy_from_slice(&super::water::water_params(water));
    }
    let (shading, toon) = shading_uniform(&item.material);
    let outline = outline_uniform(&item.material);
    if textures.iter().all(Option::is_none)
        && params == [[0.0; 4]; SURFACE_PARAMETERS]
        && shading == [SHADING_STANDARD, 0.0, 0.0, 0.0]
        && outline[3] == 0.0
    {
        return MaterialKey::default();
    }
    let maps = &item.material.maps;
    let mask = |map: &Option<ChannelMap>| {
        let mut mask = [0.0; 4];
        mask[map.as_ref().map_or(0, |map| map.channel as usize)] = 1.0;
        mask
    };
    let inverted = |map: &Option<ChannelMap>| f32::from(map.as_ref().is_some_and(|map| map.invert));
    let used = |used: bool| f32::from(used);
    let uniform = MapsUniform {
        metalness: mask(&maps.metalness),
        roughness: mask(&maps.roughness),
        occlusion: mask(&maps.occlusion),
        opacity: mask(&maps.opacity),
        invert: [
            inverted(&maps.metalness),
            inverted(&maps.roughness),
            inverted(&maps.occlusion),
            inverted(&maps.opacity),
        ],
        settings: [
            maps.normal.as_ref().map_or(0.0, |map| map.scale),
            if maps.normal.as_ref().is_some_and(|map| map.flip_green) {
                -1.0
            } else {
                1.0
            },
            if maps.occlusion.is_some() {
                maps.occlusion_strength.clamp(0.0, 1.0)
            } else {
                0.0
            },
            used(maps.opacity.is_some()),
        ],
        specular: [
            used(maps.specular.is_some() && shading[0] == SHADING_STANDARD),
            // A BC5 normal map has no blue: the shader works it out.
            used(
                maps.normal
                    .as_ref()
                    .is_some_and(|map| !map.texture.bc5_levels().is_empty()),
            ),
            0.0,
            0.0,
        ],
        uv_matrix: uv_matrix(&item.material),
        uv_offset: [
            item.material.uv_offset[0],
            item.material.uv_offset[1],
            0.0,
            0.0,
        ],
        shading,
        toon,
        outline,
        params,
    };
    MaterialKey {
        textures,
        maps: bytemuck::cast(uniform),
        terrain: Default::default(),
    }
}

/// The [`MapsUniform`] a key was made from. The default key's is the plain
/// material's: no maps.
pub(super) fn maps_uniform_of(key: &MaterialKey) -> MapsUniform {
    if *key == MaterialKey::default() {
        let red = [1.0, 0.0, 0.0, 0.0];
        return MapsUniform {
            metalness: red,
            roughness: red,
            occlusion: red,
            opacity: red,
            invert: [0.0; 4],
            settings: [0.0, 1.0, 0.0, 0.0],
            specular: [0.0; 4],
            uv_matrix: [1.0, 0.0, 0.0, 1.0],
            uv_offset: [0.0; 4],
            shading: [SHADING_STANDARD, 0.0, 0.0, 0.0],
            toon: [0.0; 4],
            outline: [0.0; 4],
            params: [[0.0; 4]; SURFACE_PARAMETERS],
        };
    }
    bytemuck::cast(key.maps)
}

/// The columns of the 2×2 matrix that scales, then turns, a material's
/// texture coordinates. Turning by `r` takes (u, v) to
/// (u cos r + v sin r, v cos r - u sin r): counter-clockwise in the
/// texture, whose V runs down (glTF's `KHR_texture_transform`).
pub(super) fn uv_matrix(material: &Material) -> [f32; 4] {
    let [su, sv] = material.uv_scale;
    let (sin, cos) = material.uv_rotation.sin_cos();
    [cos * su, -sin * su, sin * sv, cos * sv]
}

/// The GPU data for one item, or `None` if it can't be seen.
pub(super) fn instance_raw(item: &DrawItem) -> Option<InstanceRaw> {
    instance_raw_with(item, &mut LinearColors::default())
}

/// The last base and glow colors turned to linear light, kept because
/// items drawn one after another mostly share them (a batch's) and the
/// conversion takes a power per channel.
#[derive(Default)]
pub(super) struct LinearColors {
    base: Option<([f32; 3], Vec3)>,
    emissive: Option<([f32; 3], Vec3)>,
}

impl LinearColors {
    fn convert(slot: &mut Option<([f32; 3], Vec3)>, color: [f32; 3]) -> Vec3 {
        match slot {
            Some((last, linear)) if *last == color => *linear,
            _ => {
                let linear = linear_color(color);
                *slot = Some((color, linear));
                linear
            }
        }
    }
}

/// [`instance_raw`], converting colors through `colors`.
pub(super) fn instance_raw_with(item: &DrawItem, colors: &mut LinearColors) -> Option<InstanceRaw> {
    let model3 = Mat3::from_mat4(item.transform);
    // A shape squashed flat (size 0 on some axis) has no inverse, and can't be seen anyway.
    if model3.determinant().abs() < 1e-12 {
        return None;
    }
    let material = &item.material;
    let base_color = LinearColors::convert(&mut colors.base, material.base_color);
    let emissive = LinearColors::convert(&mut colors.emissive, material.emissive);
    Some(InstanceRaw {
        model: item.transform.to_cols_array_2d(),
        normal_matrix: model3.inverse().transpose().to_cols_array_2d(),
        base_color_opacity: base_color.extend(material.opacity).to_array(),
        emissive_fade: emissive.extend(1.0).to_array(),
        surface: [
            material.metalness,
            material.roughness,
            material.alpha_cutoff,
            item.uv_tile,
        ],
        skin: [0, 0],
        sway: {
            let mut sway = match item.sway {
                Some(sway) if !sway.is_still() => {
                    // The flutter rides in the fraction of the decal role.
                    [
                        sway.base,
                        1.0 / sway.height,
                        sway.strength,
                        sway.flutter.clamp(0.0, 0.95),
                    ]
                }
                _ => [0.0; 4],
            };
            // `ROLE_CHARACTER` and `ROLE_DECAL` in shader.wgsl.
            sway[3] += match item.decal_role {
                DecalRole::Surface => 0.0,
                DecalRole::Character => 1.0,
                DecalRole::Decal => 2.0,
            };
            sway
        },
    })
}

/// Converts an sRGB color (as people pick it) to linear light (what shading
/// math needs).
pub(super) fn linear_color([r, g, b]: [f32; 3]) -> Vec3 {
    fn channel(c: f32) -> f32 {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    Vec3::new(channel(r), channel(g), channel(b))
}
