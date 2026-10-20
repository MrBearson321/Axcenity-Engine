//! Frame uniforms, tiled lights, shadow cascades and draw batches.
//!
//! CPU-side planning for one frame. The `Frame`, `Light` and instance layouts
//! must stay aligned with `shader.wgsl`. Shared GPU types live in `renderer.rs`.

use std::sync::Arc;

use glam::camera::rh;
use glam::{Mat3, Mat4, Vec2, Vec3};

use crate::draw_list::{
    ChannelMap, DrawItem, DrawList, Light, PostProcessing, Shape, TextureSource,
};

use super::{
    Batch, CASCADE_SPLIT_BLEND, FAR_PLANE, FrameUniform, InstanceRaw, LIGHT_SCALE, LIGHTS_PER_TILE,
    LightRaw, MATERIAL_TEXTURES, MAX_LIGHTS, MAX_SUN_SIZE, MAX_TILES, MIN_TILE_SIZE, MapsUniform,
    MaterialKey, MeshKey, NEAR_PLANE, PostUniform, SHADOW_CASCADES, SHADOW_CASTER_RANGE,
    SHADOW_MAP_SIZE,
};

pub(super) fn view_projection(list: &DrawList, aspect: f32) -> Mat4 {
    let camera = &list.camera;
    let view = rh::view::look_at_mat4(camera.position, camera.look_at, Vec3::Y);
    // wgpu uses DirectX-style clip space: depth from 0 to 1, Y up.
    let projection = rh::proj::directx::perspective(
        camera.fov_y_degrees.to_radians(),
        aspect,
        NEAR_PLANE,
        FAR_PLANE,
    );
    projection * view
}

pub(super) fn frame_uniform(list: &DrawList, aspect: f32) -> FrameUniform {
    let camera = &list.camera;
    let view_proj = view_projection(list, aspect);
    let sun_direction = list.sun.direction.normalize_or(Vec3::Y);
    let sun_radiance = linear_color(list.sun.color) * list.sun.intensity;
    let ambient = linear_color(list.ambient.color) * list.ambient.intensity;
    let shadows = shadow_setup(list, sun_direction, aspect);
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
    }
}

/// The lights to draw, and which of them reach each tile of the screen.
pub(super) struct PlannedLights {
    pub(super) lights: Vec<LightRaw>,
    /// For each tile, row by row: how many lights reach it, then up to
    /// [`LIGHTS_PER_TILE`] indexes into `lights`.
    pub(super) tiles: Vec<u32>,
    pub(super) tile_size: u32,
    pub(super) tiles_across: u32,
}

/// Picks the lights to draw (the nearest [`MAX_LIGHTS`]) and sorts them into
/// screen tiles, so each pixel only loops over the few lights that can
/// reach it instead of all of them. A light goes in every tile that its
/// sphere of reach could touch; tiles with more than [`LIGHTS_PER_TILE`]
/// keep the nearest ones to the camera.
pub(super) fn plan_lights(
    list: &DrawList,
    aspect: f32,
    (width, height): (u32, u32),
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
    };
    if lights.is_empty() || width == 0 || height == 0 {
        return planned;
    }

    let view_proj = view_projection(list, aspect);
    let stride = LIGHTS_PER_TILE + 1;
    planned.tiles = vec![0; (across * down) as usize * stride];
    for light in lights {
        let Some((first, last)) = light_tiles(view_proj, light, (width, height), tile_size) else {
            continue;
        };
        let index = planned.lights.len() as u32;
        planned.lights.push(light_raw(light));
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
    let (cone_edge, direction, cone_inner) = match light.cone {
        None => (-2.0, Vec3::NEG_Y, 0.0),
        Some(cone) => {
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
    }
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
            0.0,
            0.0,
        ],
    }
}

/// The shadow values for the shader (see [`FrameUniform`]).
pub(super) struct ShadowSetup {
    cascades: [Mat4; SHADOW_CASCADES],
    texels: [f32; 4],
    depths: [f32; 4],
    settings: [f32; 4],
}

/// Where each shadow cascade ends, in studs from the camera along the view.
/// The ends are spread so each cascade reaches a few times further than
/// the one before (see [`CASCADE_SPLIT_BLEND`]), and the last ends at `distance`.
pub(super) fn cascade_ends(distance: f32) -> [f32; SHADOW_CASCADES] {
    // From 1 stud, not the near plane, or the first cascade would be tiny.
    let near = 1.0_f32.min(distance * 0.5);
    std::array::from_fn(|index| {
        let fraction = (index + 1) as f32 / SHADOW_CASCADES as f32;
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
/// edges don't shimmer as the camera moves.
pub(super) fn shadow_setup(list: &DrawList, sun_direction: Vec3, aspect: f32) -> ShadowSetup {
    let settings = list.shadows;
    let mut setup = ShadowSetup {
        cascades: [Mat4::IDENTITY; SHADOW_CASCADES],
        texels: [1.0; 4],
        depths: [1.0; 4],
        settings: [0.0; 4],
    };
    if !settings.enabled || !settings.distance.is_finite() || settings.distance <= 0.0 {
        return setup;
    }
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
    for (index, far) in cascade_ends(distance).into_iter().enumerate() {
        // The smallest sphere around the slice of the view between
        // `slice_near` and `far`, centered on the view direction `center` away.
        let center = (0.5 * (far + slice_near) * (1.0 + k2)).min(far);
        let radius = ((far - center).powi(2) + far * far * k2).sqrt().max(1.0);
        let sphere_center = camera.position + forward * center;
        slice_near = far;

        let texel = 2.0 * radius / SHADOW_MAP_SIZE as f32;
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
        SHADOW_CASCADES as f32,
        settings.distance,
    ];
    setup
}

/// Puts the visible items in drawing order and groups them into batches.
///
/// Solid items come first, grouped by mesh and texture. See-through items come last,
/// farthest from the camera first, so each one blends over what's behind it.
/// They're sorted by their centers, so two that overlap can still blend in the
/// wrong order; that's the usual trade-off for this simple approach.
pub(super) fn plan_draws(list: &DrawList) -> (Vec<InstanceRaw>, Vec<Batch>) {
    let (mut transparent, mut opaque): (Vec<&DrawItem>, Vec<&DrawItem>) = list
        .items
        .iter()
        .filter(|item| item.material.opacity > 0.0)
        .partition(|item| item.material.opacity < 1.0);

    opaque.sort_by_key(|item| {
        (
            MeshKey::of(&item.shape),
            material_key(item),
            !item.casts_shadow,
        )
    });
    let distance = |item: &DrawItem| {
        item.transform
            .w_axis
            .truncate()
            .distance_squared(list.camera.position)
    };
    // `total_cmp` orders floats even when one is NaN, so sorting can't panic.
    transparent.sort_by(|a, b| distance(b).total_cmp(&distance(a)));

    let mut instances = Vec::new();
    let mut batches: Vec<Batch> = Vec::new();
    let ordered = opaque
        .into_iter()
        .map(|item| (item, false))
        .chain(transparent.into_iter().map(|item| (item, true)));
    for (item, transparent) in ordered {
        let Some(raw) = instance_raw(item) else {
            continue;
        };
        let index = instances.len() as u32;
        instances.push(raw);
        let (mesh, material) = (MeshKey::of(&item.shape), material_key(item));
        let casts_shadow = item.casts_shadow;
        match batches.last_mut() {
            Some(last)
                if last.mesh == mesh
                    && last.material == material
                    && last.transparent == transparent
                    && last.casts_shadow == casts_shadow =>
            {
                last.instances.end = index + 1;
            }
            _ => batches.push(Batch {
                mesh,
                material,
                transparent,
                casts_shadow,
                instances: index..index + 1,
            }),
        }
    }
    (instances, batches)
}

/// The textures an item is drawn with, in binding order (see
/// [`MATERIAL_TEXTURES`]). Only callers' meshes have texture coordinates,
/// so the built-in shapes never use any.
pub(super) fn material_textures(
    item: &DrawItem,
) -> [Option<&Arc<dyn TextureSource>>; MATERIAL_TEXTURES] {
    if !matches!(item.shape, Shape::Mesh(_)) || item.material.terrain.is_some() {
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
        maps.specular.as_ref(),
    ]
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
    if textures.iter().all(Option::is_none) {
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
        specular: [used(maps.specular.is_some()), 0.0, 0.0, 0.0],
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
        };
    }
    bytemuck::cast(key.maps)
}

/// The GPU data for one item, or `None` if it can't be seen.
pub(super) fn instance_raw(item: &DrawItem) -> Option<InstanceRaw> {
    let model3 = Mat3::from_mat4(item.transform);
    // A shape squashed flat (size 0 on some axis) has no inverse, and can't be seen anyway.
    if model3.determinant().abs() < 1e-12 {
        return None;
    }
    let material = &item.material;
    let base_color = linear_color(material.base_color);
    Some(InstanceRaw {
        model: item.transform.to_cols_array_2d(),
        normal_matrix: model3.inverse().transpose().to_cols_array_2d(),
        base_color_opacity: base_color.extend(material.opacity).to_array(),
        emissive: linear_color(material.emissive).to_array(),
        surface: [
            material.metalness,
            material.roughness,
            material.alpha_cutoff,
        ],
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
