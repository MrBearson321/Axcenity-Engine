//! Turns the Instance tree into a draw list for `render`.
//!
//! This is the only place that knows about both Instances and the renderer,
//! which keeps `render` free of engine types. The player and the editor
//! viewport both use it, so they draw a world the same way.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use axcenity_assets::{AlphaMode, Assets, ChannelTexture, Image, Model, ModelMaterial, Posed};
use axcenity_core::{ClassName, Instance, InstanceId, PartType, TextureChannel, Value, World};
use axcenity_render::glam::{Mat4, Vec3};
use axcenity_render::{
    AmbientLight, Camera, Channel, ChannelMap, Cone, DrawItem, DrawList, Light, Material,
    MeshSource, NormalMap, PostProcessing, Shadows, Shape, Sky, Sun, TextureMaps, TextureSource,
};
use axcenity_ui::{Stylesheet, UiLayout, layout_ui_interactive};

use crate::orientation::rotation_from_orientation;
use crate::terrain::TerrainCache;

const SUN: Sun = Sun {
    direction: Vec3::new(0.5, 1.0, 0.25),
    color: [1.0, 0.97, 0.92],
    intensity: 3.0,
};
// A little flat light on top of the sky's, so shadowed corners aren't black.
const AMBIENT: AmbientLight = AmbientLight {
    color: [0.75, 0.85, 1.0],
    intensity: 0.05,
};
/// How much brighter than its color a glowing part is drawn when bloom is
/// on, so it goes past the bloom threshold and glows.
const GLOW_BOOST: f32 = 2.5;

/// Animated MeshParts' posed models, by MeshPart (the host's animators make
/// them). A MeshPart in here is drawn posed; the rest are drawn still.
pub type Poses = HashMap<InstanceId, Arc<Posed>>;

/// Everything the Lighting service's properties set.
struct LightingSettings {
    shadows: Shadows,
    sky: Sky,
    post: PostProcessing,
}

fn lighting_settings(world: &World) -> LightingSettings {
    let mut settings = LightingSettings {
        shadows: Shadows::default(),
        sky: Sky::default(),
        post: PostProcessing::default(),
    };
    let Some(lighting) = world.get(world.lighting()) else {
        return settings;
    };
    let bool_of = |name, default| match lighting.property(name) {
        Some(Value::Bool(value)) => *value,
        _ => default,
    };
    let number_of = |name, default: f32| match lighting.property(name) {
        Some(Value::Number(value)) => *value as f32,
        _ => default,
    };
    let color_of = |name, default| match lighting.property(name).and_then(Value::as_color3) {
        Some(color) => [color.r, color.g, color.b],
        None => default,
    };
    let (shadows, sky, post) = (&mut settings.shadows, &mut settings.sky, &mut settings.post);
    shadows.enabled = bool_of("ShadowsEnabled", shadows.enabled);
    shadows.distance = number_of("ShadowDistance", shadows.distance);
    shadows.softness = number_of("ShadowSoftness", shadows.softness);
    sky.top_color = color_of("SkyTopColor", sky.top_color);
    sky.horizon_color = color_of("SkyHorizonColor", sky.horizon_color);
    sky.ground_color = color_of("GroundColor", sky.ground_color);
    sky.environment_intensity = number_of("EnvironmentIntensity", sky.environment_intensity);
    post.enabled = bool_of("PostProcessingEnabled", post.enabled);
    post.exposure = number_of("Exposure", post.exposure);
    post.bloom_enabled = bool_of("BloomEnabled", post.bloom_enabled);
    post.bloom_intensity = number_of("BloomIntensity", post.bloom_intensity);
    post.bloom_threshold = number_of("BloomThreshold", post.bloom_threshold);
    settings
}

/// The light a PointLight or SpotLight gives off from the middle of its
/// parent part, or `None` if it's switched off or isn't inside a part.
fn light_of(world: &World, instance: &Instance) -> Option<Light> {
    if instance.property("Enabled").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let parent = world.get(instance.parent()?)?;
    let (_, transform) = part_placement(parent)?;
    let position = light_origin(transform);
    let (_, rotation, _) = transform.to_scale_rotation_translation();
    let color = instance.property("Color").and_then(Value::as_color3)?;
    let number = |name| instance.property(name).and_then(Value::as_number);
    let cone = match instance.class() {
        ClassName::SpotLight => {
            let direction = instance.property("Direction").and_then(Value::as_vector3)?;
            Some(Cone {
                // Relative to the part, so the beam turns with it.
                direction: rotation * Vec3::new(direction.x, direction.y, direction.z),
                angle_degrees: number("Angle")? as f32,
            })
        }
        _ => None,
    };
    Some(Light {
        position,
        color: [color.r, color.g, color.b],
        brightness: number("Brightness")? as f32,
        range: number("Range")? as f32,
        cone,
    })
}

/// Where a PointLight or SpotLight in the Workspace is, for the editor to
/// show it: a light has no shape of its own, so it's drawn as an icon.
#[derive(Clone, Debug, PartialEq)]
pub struct LightMarker {
    pub id: InstanceId,
    /// The part the light is in.
    pub part: InstanceId,
    pub spot: bool,
    pub enabled: bool,
    /// Just above the parent part, where the light shines from.
    pub position: Vec3,
    pub color: [f32; 3],
    pub range: f32,
    /// For a SpotLight: the way it points (one stud long), and its cone's
    /// whole angle in degrees.
    pub cone: Option<(Vec3, f32)>,
}

/// Every PointLight and SpotLight inside a part in the Workspace, switched
/// on or not.
pub fn light_markers(world: &World) -> Vec<LightMarker> {
    let mut markers = Vec::new();
    for id in world.descendants(world.workspace()) {
        let Some(instance) = world.get(id) else {
            continue;
        };
        let spot = match instance.class() {
            ClassName::PointLight => false,
            ClassName::SpotLight => true,
            _ => continue,
        };
        let Some(part) = instance.parent() else {
            continue;
        };
        let Some((_, transform)) = world.get(part).and_then(part_placement) else {
            continue;
        };
        let position = light_origin(transform);
        let (_, rotation, _) = transform.to_scale_rotation_translation();
        let number = |name| {
            instance
                .property(name)
                .and_then(Value::as_number)
                .unwrap_or(0.0) as f32
        };
        let cone = spot.then(|| {
            let direction = instance
                .property("Direction")
                .and_then(Value::as_vector3)
                .map_or(Vec3::NEG_Y, |d| Vec3::new(d.x, d.y, d.z));
            (
                (rotation * direction).normalize_or(Vec3::NEG_Y),
                number("Angle"),
            )
        });
        let color = instance
            .property("Color")
            .and_then(Value::as_color3)
            .map_or([1.0; 3], |color| [color.r, color.g, color.b]);
        markers.push(LightMarker {
            id,
            part,
            spot,
            enabled: instance.property("Enabled").and_then(Value::as_bool) == Some(true),
            position,
            color,
            range: number("Range"),
            cone,
        });
    }
    markers
}

/// The draw list for everything in the Workspace, seen from `camera`.
/// MeshParts' models come from `assets`, loaded the first time they're drawn.
/// Terrains are built from scratch; hosts that draw every frame keep a
/// [`TerrainCache`] and use [`build_draw_list_posed`].
pub fn build_draw_list(world: &World, assets: &Assets, camera: Camera) -> DrawList {
    build_draw_list_posed(
        world,
        assets,
        camera,
        &Poses::new(),
        &mut TerrainCache::new(),
    )
}

/// Like [`build_draw_list`], drawing the MeshParts in `poses` as posed, and
/// Terrains with what `terrain` kept from earlier frames.
pub fn build_draw_list_posed(
    world: &World,
    assets: &Assets,
    camera: Camera,
    poses: &Poses,
    terrain: &mut TerrainCache,
) -> DrawList {
    let settings = lighting_settings(world);
    let glow = if settings.post.enabled && settings.post.bloom_enabled {
        GLOW_BOOST
    } else {
        1.0
    };
    let mut items = Vec::new();
    let mut lights = Vec::new();
    for (id, instance) in world
        .descendants(world.workspace())
        .into_iter()
        .filter_map(|id| Some((id, world.get(id)?)))
    {
        match instance.class() {
            ClassName::MeshPart => {
                let posed = poses.get(&id).map(|posed| (id, posed));
                mesh_part_draw_items(world, instance, assets, posed, &mut items)
            }
            ClassName::PointLight | ClassName::SpotLight => {
                lights.extend(light_of(world, instance))
            }
            // Drawn chunk by chunk below.
            ClassName::Terrain => {}
            _ => {
                if let Some(mut item) = part_draw_item(world, instance, assets, glow) {
                    if let Some(extra) = fixture_glow(world, instance) {
                        for (channel, added) in item.material.emissive.iter_mut().zip(extra) {
                            *channel = (*channel + added).min(4.0);
                        }
                    }
                    items.push(item);
                }
            }
        }
    }

    terrain.draw_all(world, assets, &camera, &mut items);

    DrawList {
        sky: settings.sky,
        camera,
        sun: SUN,
        ambient: AMBIENT,
        shadows: settings.shadows,
        lights,
        post: settings.post,
        items,
        sprites: Vec::new(),
        ui: Vec::new(),
    }
}

/// Builds the shared scene list and lays out its UI for a physical-pixel viewport.
pub fn build_draw_list_with_ui(
    world: &World,
    assets: &Assets,
    camera: Camera,
    viewport_size: (u32, u32),
    stylesheets: &[Stylesheet],
) -> DrawList {
    build_draw_list_with_ui_interaction(
        world,
        assets,
        camera,
        viewport_size,
        stylesheets,
        None,
        None,
        None,
        &Poses::new(),
        &mut TerrainCache::new(),
    )
    .0
}

/// Builds a scene and interactive UI layout for the given physical-pixel
/// pointer position and button currently held, with `poses` drawn posed.
#[allow(clippy::too_many_arguments)] // Each is a different input to the frame.
pub fn build_draw_list_with_ui_interaction(
    world: &World,
    assets: &Assets,
    camera: Camera,
    viewport_size: (u32, u32),
    stylesheets: &[Stylesheet],
    pointer: Option<[f32; 2]>,
    pressed_button: Option<InstanceId>,
    focused: Option<InstanceId>,
    poses: &Poses,
    terrain: &mut TerrainCache,
) -> (DrawList, UiLayout) {
    let mut list = build_draw_list_posed(world, assets, camera, poses, terrain);
    let layout = layout_ui_interactive(
        world,
        viewport_size.0,
        viewport_size.1,
        stylesheets,
        pointer,
        pressed_button,
        focused,
        Some(&camera),
    );
    list.ui = layout.draw_items.clone();
    attach_ui_textures(&mut list.ui, assets);
    (list, layout)
}

/// Where a light shines from: just above the parent part, so the part's own
/// top and the things around it are lit. A light at the part's center sits
/// inside the mesh, and the outer faces point away from it.
fn light_origin(transform: Mat4) -> Vec3 {
    let (scale, rotation, position) = transform.to_scale_rotation_translation();
    position + rotation * Vec3::Y * (scale.y.abs() * 0.5 + 0.05)
}

/// Color added to a part that contains a switched-on light, so the fixture
/// itself is visible and not only the surfaces it happens to reach.
fn fixture_glow(world: &World, instance: &Instance) -> Option<[f32; 3]> {
    let mut glow = [0.0_f32; 3];
    let mut any = false;
    for child_id in instance.children() {
        let Some(light) = world.get(*child_id) else {
            continue;
        };
        if !matches!(light.class(), ClassName::PointLight | ClassName::SpotLight) {
            continue;
        }
        if light.property("Enabled").and_then(Value::as_bool) != Some(true) {
            continue;
        }
        let Some(color) = light.property("Color").and_then(Value::as_color3) else {
            continue;
        };
        let brightness = light
            .property("Brightness")
            .and_then(Value::as_number)
            .unwrap_or(1.0) as f32;
        let scale = (brightness * 0.35).clamp(0.2, 1.5);
        glow[0] += color.r * scale;
        glow[1] += color.g * scale;
        glow[2] += color.b * scale;
        any = true;
    }
    any.then_some(glow)
}

/// Fills in UI image textures from project files. Paths that fail to load
/// are reported by [`Assets`](axcenity_assets::Assets) and those images are
/// not drawn.
pub fn attach_ui_textures(items: &mut [axcenity_render::UiDrawItem], assets: &Assets) {
    for item in items {
        let axcenity_render::UiDrawItem::Image(image) = item else {
            continue;
        };
        if image.texture.is_some() || image.path.is_empty() {
            continue;
        }
        image.texture = assets
            .texture(&image.path)
            .map(|image| Arc::new(ImageTexture(image)) as Arc<dyn axcenity_render::TextureSource>);
    }
}

/// A Part's shape and the transform that places its 1×1×1 mesh in the world,
/// or `None` for anything that isn't a part. Drawing and picking both use
/// this, so a click hits exactly what's drawn. A MeshPart counts as a Block:
/// its model fills that box, and it's picked and outlined by the box.
pub(crate) fn part_placement(instance: &Instance) -> Option<(PartType, Mat4)> {
    let vector = |name| {
        let v = instance.property(name).and_then(Value::as_vector3)?;
        Some(Vec3::new(v.x, v.y, v.z))
    };
    // Only parts have a Shape (or are MeshParts), so anything else stops here.
    let shape = match instance.class() {
        ClassName::MeshPart => PartType::Block,
        _ => instance.property("Shape").and_then(Value::as_part_type)?,
    };
    let orientation = instance
        .property("Orientation")
        .and_then(Value::as_vector3)?;
    let transform = Mat4::from_scale_rotation_translation(
        mesh_scale(shape, vector("Size")?),
        rotation_from_orientation(orientation),
        vector("Position")?,
    );
    Some((shape, transform))
}

/// Whether a Part or MeshPart casts a sun shadow: its `CastShadow`, on
/// unless it's been turned off.
fn casts_shadow(instance: &Instance) -> bool {
    instance.property("CastShadow").and_then(Value::as_bool) != Some(false)
}

/// The draw item for a Part, or `None` for any other class. `glow` is how
/// much brighter than its color a glowing part is drawn. A SurfaceAppearance
/// inside the part replaces its flat color with that appearance's maps.
fn part_draw_item(
    world: &World,
    instance: &Instance,
    assets: &Assets,
    glow: f32,
) -> Option<DrawItem> {
    let number = |name| instance.property(name).and_then(Value::as_number);
    let (shape, transform) = part_placement(instance)?;
    let color = instance.property("Color").and_then(Value::as_color3)?;
    let preset = instance
        .property("Material")
        .and_then(Value::as_material)?
        .preset();

    let color = [color.r, color.g, color.b];
    let opacity = preset.opacity * visibility(instance);
    if opacity <= 0.0 {
        return None;
    }
    let mut emissive = if preset.glows {
        color.map(|channel| channel * glow)
    } else {
        [0.0; 3]
    };
    let emission = number("Emission").unwrap_or(0.0).max(0.0) as f32;
    if emission > 0.0
        && let Some(tint) = instance
            .property("EmissiveColor")
            .and_then(Value::as_color3)
    {
        for (channel, added) in emissive.iter_mut().zip([tint.r, tint.g, tint.b]) {
            *channel += added * emission;
        }
    }
    let mut material = Material {
        // A glowing part shows its color as light, the same on every side,
        // instead of being shaded by the sun.
        base_color: if preset.glows { [0.0; 3] } else { color },
        emissive,
        metalness: number("Metalness")? as f32,
        roughness: number("Roughness")? as f32,
        opacity,
        ..Material::default()
    };
    let appearance = instance.children().iter().find_map(|&id| {
        let child = world.get(id)?;
        (child.class() == ClassName::SurfaceAppearance).then_some(child)
    });
    if let Some(appearance) = appearance {
        overlay_appearance(&mut material, appearance, assets);
    }
    let uv_tile = if has_texture_maps(&material) {
        let tile = number("TileSize").unwrap_or(8.0);
        if tile.is_finite() && tile > 0.0 {
            tile as f32
        } else {
            0.0
        }
    } else {
        0.0
    };
    Some(DrawItem {
        shape: match shape {
            PartType::Block => Shape::Block,
            PartType::Ball => Shape::Sphere,
            PartType::Cylinder => Shape::Cylinder,
            PartType::Wedge => Shape::Wedge,
        },
        transform,
        material,
        uv_tile,
        casts_shadow: casts_shadow(instance),
    })
}

/// True when `material` samples a texture, so a built-in shape should
/// tile its coordinates in studs.
fn has_texture_maps(material: &Material) -> bool {
    let maps = &material.maps;
    material.base_color_texture.is_some()
        || maps.normal.is_some()
        || maps.metalness.is_some()
        || maps.roughness.is_some()
        || maps.occlusion.is_some()
        || maps.emissive.is_some()
        || maps.opacity.is_some()
        || maps.specular.is_some()
}

/// Puts `appearance`'s maps onto `material`. A metalness or roughness map
/// gives that value alone (factor 1). An empty map leaves what `material`
/// already had.
fn overlay_appearance(material: &mut Material, appearance: &Instance, assets: &Assets) {
    let texture_of = |name| {
        let path = appearance.property(name).and_then(Value::as_str)?;
        assets.texture(path).map(|image| texture(&image))
    };
    let number = |name, default: f32| {
        appearance
            .property(name)
            .and_then(Value::as_number)
            .map_or(default, |n| n as f32)
    };
    let flag = |name| appearance.property(name).and_then(Value::as_bool) == Some(true);
    let channel = |name| match appearance
        .property(name)
        .and_then(Value::as_texture_channel)
    {
        Some(TextureChannel::Green) => Channel::Green,
        Some(TextureChannel::Blue) => Channel::Blue,
        Some(TextureChannel::Alpha) => Channel::Alpha,
        Some(TextureChannel::Red) | None => Channel::Red,
    };
    let channel_map = |map, channel_property, invert| {
        texture_of(map).map(|texture| ChannelMap {
            texture,
            channel: channel(channel_property),
            invert,
        })
    };
    if let Some(tint) = appearance.property("Color").and_then(Value::as_color3) {
        let [r, g, b] = material.base_color;
        material.base_color = [r * tint.r, g * tint.g, b * tint.b];
    }
    if let Some(image) = texture_of("ColorMap") {
        material.base_color_texture = Some(image);
    }
    if let Some(image) = texture_of("NormalMap") {
        material.maps.normal = Some(NormalMap {
            texture: image,
            scale: number("NormalStrength", 1.0),
            flip_green: flag("NormalMapDirectX"),
        });
    }
    if let Some(map) = channel_map("MetalnessMap", "MetalnessChannel", false) {
        material.maps.metalness = Some(map);
        material.metalness = 1.0;
    }
    if let Some(map) = channel_map("RoughnessMap", "RoughnessChannel", flag("InvertRoughness")) {
        material.maps.roughness = Some(map);
        material.roughness = 1.0;
    }
    if let Some(map) = channel_map("AmbientOcclusionMap", "AmbientOcclusionChannel", false) {
        material.maps.occlusion = Some(map);
        material.maps.occlusion_strength = number("AmbientOcclusionStrength", 1.0);
    }
    if let Some(image) = texture_of("EmissiveMap") {
        material.maps.emissive = Some(image);
        let strength = number("EmissiveStrength", 1.0).max(0.0);
        if material.emissive.iter().all(|&channel| channel <= 0.0) {
            material.emissive = [strength; 3];
        } else {
            material.emissive = material.emissive.map(|channel| channel * strength);
        }
    }
    if let Some(map) = channel_map("OpacityMap", "OpacityChannel", flag("InvertOpacity")) {
        material.maps.opacity = Some(map);
        if material.opacity >= 1.0 && material.alpha_cutoff == 0.0 {
            material.alpha_cutoff = 0.5;
        }
    }
    if let Some(image) = texture_of("SpecularMap") {
        material.maps.specular = Some(image);
    }
}

/// A MeshPart's draw items, one per part of its model, each with the
/// model's own material, or with the maps of the SurfaceAppearance inside
/// it for that material. Until it has a model (no `MeshId` yet, or one
/// that can't be loaded), it's drawn as a plain grey box, so it can still
/// be seen and picked. With `posed` (the MeshPart's ID and a pose of its
/// model), the model is drawn in that pose.
fn mesh_part_draw_items(
    world: &World,
    instance: &Instance,
    assets: &Assets,
    posed: Option<(InstanceId, &Arc<Posed>)>,
    items: &mut Vec<DrawItem>,
) {
    let Some((_, transform)) = part_placement(instance) else {
        return;
    };
    let visible = visibility(instance);
    if visible <= 0.0 {
        return;
    }
    let first = items.len();
    let mesh_id = instance
        .property("MeshId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let Some(model) = assets.model(mesh_id) else {
        items.push(DrawItem {
            shape: Shape::Block,
            transform,
            material: Material::default(),
            uv_tile: 0.0,
            casts_shadow: casts_shadow(instance),
        });
        return;
    };
    let appearances: Vec<&Instance> = instance
        .children()
        .iter()
        .filter_map(|&id| world.get(id))
        .filter(|child| child.class() == ClassName::SurfaceAppearance)
        .collect();
    // A pose of another model (the MeshId just changed) isn't used.
    let posed = posed.filter(|(_, posed)| {
        posed.model_key == model.key() && posed.primitives.len() == model.primitives.len()
    });
    for (index, primitive) in model.primitives.iter().enumerate() {
        let appearance = appearances.iter().find(|appearance| {
            appearance.property("MaterialName").and_then(Value::as_str)
                == Some(primitive.material.name.as_str())
        });
        let material = match appearance {
            Some(appearance) => surface_material(&primitive.material, appearance, assets),
            None => model_material(&primitive.material),
        };
        let shape: Arc<dyn MeshSource> = match posed {
            Some((part, posed)) => Arc::new(PosedMesh {
                key: posed_key(part, primitive.key()),
                model: model.clone(),
                posed: posed.clone(),
                index,
            }),
            None => Arc::new(ModelMesh {
                model: model.clone(),
                index,
            }),
        };
        items.push(DrawItem {
            shape: Shape::Mesh(shape),
            transform,
            material,
            uv_tile: 0.0,
            casts_shadow: casts_shadow(instance),
        });
    }
    for item in &mut items[first..] {
        item.material.opacity *= visible;
    }
}

/// How much of a part shows: 1 minus its `Transparency`, from 0 to 1.
fn visibility(instance: &Instance) -> f32 {
    let transparency = instance
        .property("Transparency")
        .and_then(Value::as_number)
        .unwrap_or(0.0) as f32;
    (1.0 - transparency).clamp(0.0, 1.0)
}

/// A model's material with a SurfaceAppearance's maps in place of its own.
/// A map the SurfaceAppearance leaves empty is no map (metalness and
/// roughness then take the material's own values); one it sets gives the
/// value alone. Its textures come from `assets`.
fn surface_material(file: &ModelMaterial, appearance: &Instance, assets: &Assets) -> Material {
    let texture_of = |name| {
        let path = appearance.property(name).and_then(Value::as_str)?;
        assets.texture(path).map(|image| texture(&image))
    };
    let number = |name, default: f32| {
        appearance
            .property(name)
            .and_then(Value::as_number)
            .map_or(default, |n| n as f32)
    };
    let flag = |name| appearance.property(name).and_then(Value::as_bool) == Some(true);
    let channel = |name| match appearance
        .property(name)
        .and_then(Value::as_texture_channel)
    {
        Some(TextureChannel::Green) => Channel::Green,
        Some(TextureChannel::Blue) => Channel::Blue,
        Some(TextureChannel::Alpha) => Channel::Alpha,
        Some(TextureChannel::Red) | None => Channel::Red,
    };
    let channel_map = |map, channel_property, invert| {
        texture_of(map).map(|texture| ChannelMap {
            texture,
            channel: channel(channel_property),
            invert,
        })
    };

    let mut material = model_material(file);
    if let Some(tint) = appearance.property("Color").and_then(Value::as_color3) {
        let [r, g, b] = material.base_color;
        material.base_color = [r * tint.r, g * tint.g, b * tint.b];
    }
    material.base_color_texture = texture_of("ColorMap");
    let maps = &mut material.maps;
    maps.normal = texture_of("NormalMap").map(|texture| NormalMap {
        texture,
        scale: number("NormalStrength", 1.0),
        flip_green: flag("NormalMapDirectX"),
    });
    maps.metalness = channel_map("MetalnessMap", "MetalnessChannel", false);
    if maps.metalness.is_some() {
        material.metalness = 1.0;
    }
    maps.roughness = channel_map("RoughnessMap", "RoughnessChannel", flag("InvertRoughness"));
    if maps.roughness.is_some() {
        material.roughness = 1.0;
    }
    maps.occlusion = channel_map("AmbientOcclusionMap", "AmbientOcclusionChannel", false);
    maps.occlusion_strength = number("AmbientOcclusionStrength", 1.0);
    maps.emissive = texture_of("EmissiveMap");
    // A glow map on a material that doesn't glow glows in the map's colors.
    let glow = if maps.emissive.is_some() && file.emissive.iter().all(|&c| c <= 0.0) {
        [1.0; 3]
    } else {
        file.emissive
    };
    let strength = number("EmissiveStrength", 1.0).max(0.0);
    material.emissive = srgb(glow.map(|c| c * strength));
    maps.opacity = channel_map("OpacityMap", "OpacityChannel", flag("InvertOpacity"));
    // An opacity map on a solid material cuts it out.
    if maps.opacity.is_some() && material.opacity >= 1.0 && material.alpha_cutoff == 0.0 {
        material.alpha_cutoff = 0.5;
    }
    maps.specular = texture_of("SpecularMap");
    material
}

/// A model's primitives at `transform`, using the file's own materials.
pub(crate) fn push_model(model: &Arc<Model>, transform: Mat4, items: &mut Vec<DrawItem>) {
    for (index, primitive) in model.primitives.iter().enumerate() {
        items.push(DrawItem {
            shape: Shape::Mesh(Arc::new(ModelMesh {
                model: model.clone(),
                index,
            })),
            transform,
            material: model_material(&primitive.material),
            uv_tile: 0.0,
            casts_shadow: true,
        });
    }
}

/// glTF's material values, as the renderer takes them.
fn model_material(material: &ModelMaterial) -> Material {
    let [r, g, b, alpha] = material.base_color;
    let (opacity, alpha_cutoff) = match material.alpha_mode {
        AlphaMode::Opaque => (1.0, 0.0),
        AlphaMode::Mask { cutoff } => (1.0, cutoff),
        // Kept below 1 so the item is drawn see-through, even when only its
        // texture has the transparency.
        AlphaMode::Blend => (alpha.min(0.999), 0.0),
    };
    Material {
        // glTF colors are linear light; draw lists take sRGB.
        base_color: srgb([r, g, b]),
        base_color_texture: material.base_color_texture.as_ref().map(texture),
        metalness: material.metalness,
        roughness: material.roughness,
        emissive: srgb(material.emissive),
        opacity,
        alpha_cutoff,
        maps: TextureMaps {
            normal: material.normal_texture.as_ref().map(|image| NormalMap {
                texture: texture(image),
                scale: material.normal_scale,
                flip_green: material.normal_flip_green,
            }),
            metalness: material.metalness_texture.as_ref().map(channel_map),
            roughness: material.roughness_texture.as_ref().map(channel_map),
            occlusion: material.occlusion_texture.as_ref().map(channel_map),
            occlusion_strength: material.occlusion_strength,
            emissive: material.emissive_texture.as_ref().map(texture),
            opacity: material.opacity_texture.as_ref().map(channel_map),
            specular: material.specular_texture.as_ref().map(texture),
        },
        terrain: None,
    }
}

/// A model's image, handed to the renderer without copying it.
fn texture(image: &Arc<Image>) -> Arc<dyn TextureSource> {
    Arc::new(ImageTexture(image.clone()))
}

/// A project image as a texture the renderer can draw, for particles.
pub(crate) fn image_texture(image: &Arc<Image>) -> Arc<dyn TextureSource> {
    texture(image)
}

fn channel_map(map: &ChannelTexture) -> ChannelMap {
    ChannelMap {
        texture: texture(&map.image),
        channel: match map.channel {
            axcenity_assets::Channel::Red => Channel::Red,
            axcenity_assets::Channel::Green => Channel::Green,
            axcenity_assets::Channel::Blue => Channel::Blue,
            axcenity_assets::Channel::Alpha => Channel::Alpha,
        },
        invert: map.invert,
    }
}

/// Converts linear light to sRGB, the opposite of what the renderer does.
fn srgb(linear: [f32; 3]) -> [f32; 3] {
    linear.map(|c| {
        if c <= 0.0031308 {
            c * 12.92
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        }
    })
}

/// One primitive of a model, handed to the renderer without copying it.
struct ModelMesh {
    model: Arc<Model>,
    index: usize,
}

impl MeshSource for ModelMesh {
    fn key(&self) -> u64 {
        self.model.primitives[self.index].key()
    }
    fn positions(&self) -> &[[f32; 3]] {
        &self.model.primitives[self.index].positions
    }
    fn normals(&self) -> &[[f32; 3]] {
        &self.model.primitives[self.index].normals
    }
    fn uvs(&self) -> &[[f32; 2]] {
        &self.model.primitives[self.index].uvs
    }
    fn tangents(&self) -> &[[f32; 4]] {
        &self.model.primitives[self.index].tangents
    }
    fn indices(&self) -> &[u32] {
        &self.model.primitives[self.index].indices
    }
}

/// One primitive of a posed model: the pose's corners, with the model's
/// texture coordinates and triangles. Each MeshPart's copy has its own
/// key, so two parts posed differently don't share one GPU copy.
struct PosedMesh {
    key: u64,
    model: Arc<Model>,
    posed: Arc<Posed>,
    index: usize,
}

impl MeshSource for PosedMesh {
    fn key(&self) -> u64 {
        self.key
    }
    fn positions(&self) -> &[[f32; 3]] {
        &self.posed.primitives[self.index].positions
    }
    fn normals(&self) -> &[[f32; 3]] {
        &self.posed.primitives[self.index].normals
    }
    fn uvs(&self) -> &[[f32; 2]] {
        &self.model.primitives[self.index].uvs
    }
    fn tangents(&self) -> &[[f32; 4]] {
        &self.posed.primitives[self.index].tangents
    }
    fn indices(&self) -> &[u32] {
        &self.model.primitives[self.index].indices
    }
    fn version(&self) -> u64 {
        self.posed.version
    }
}

/// The key of `part`'s posed copy of a primitive. The top bit is set, so it
/// never matches a still model's key (those count up from 1).
fn posed_key(part: InstanceId, primitive: u64) -> u64 {
    let mut hasher = DefaultHasher::new();
    (part, primitive).hash(&mut hasher);
    hasher.finish() | 1 << 63
}

/// A model's texture, handed to the renderer without copying it.
struct ImageTexture(Arc<Image>);

impl TextureSource for ImageTexture {
    fn key(&self) -> u64 {
        self.0.key()
    }
    fn size(&self) -> (u32, u32) {
        (self.0.width, self.0.height)
    }
    fn rgba(&self) -> &[u8] {
        &self.0.rgba
    }
}

/// How much to stretch the 1×1×1 mesh. Balls stay round and cylinders stay
/// circular, whatever their `Size`: they use the smallest size that fits.
fn mesh_scale(shape: PartType, size: Vec3) -> Vec3 {
    match shape {
        PartType::Block | PartType::Wedge => size,
        PartType::Ball => Vec3::splat(size.min_element()),
        PartType::Cylinder => {
            let diameter = size.y.min(size.z);
            Vec3::new(size.x, diameter, diameter)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_core::{ClassName, Color3, InstanceId, Material as PartMaterial, Vector3};

    fn camera() -> Camera {
        Camera {
            position: Vec3::Z,
            look_at: Vec3::ZERO,
            fov_y_degrees: 50.0,
        }
    }

    fn part(world: &mut World, properties: Vec<(&str, Value)>) -> InstanceId {
        let part = world.create(ClassName::Part).unwrap();
        world.set_parent(part, Some(world.workspace())).unwrap();
        for (name, value) in properties {
            world.set_property(part, name, value).unwrap();
        }
        part
    }

    fn only_item(world: &World) -> DrawItem {
        only_item_with(world, &Assets::default())
    }

    fn only_item_with(world: &World, assets: &Assets) -> DrawItem {
        let list = build_draw_list(world, assets, camera());
        assert_eq!(list.items.len(), 1);
        list.items[0].clone()
    }

    fn vector(x: f32, y: f32, z: f32) -> Value {
        Value::Vector3(Vector3::new(x, y, z))
    }

    #[test]
    fn parts_become_draw_items_with_their_properties() {
        let mut world = World::new();
        part(
            &mut world,
            vec![
                ("Position", vector(1.0, 2.0, 3.0)),
                ("Size", vector(4.0, 5.0, 6.0)),
                ("Color", Value::Color3(Color3::new(1.0, 0.5, 0.0))),
                ("Metalness", Value::Number(1.0)),
                ("Roughness", Value::Number(0.25)),
            ],
        );

        let item = only_item(&world);
        assert_eq!(item.shape, Shape::Block);
        assert_eq!(
            item.transform.transform_point3(Vec3::splat(0.5)),
            Vec3::new(3.0, 4.5, 6.0)
        );
        assert_eq!(item.material.base_color, [1.0, 0.5, 0.0]);
        assert_eq!(item.material.metalness, 1.0);
        assert_eq!(item.material.roughness, 0.25);
        assert_eq!(item.material.opacity, 1.0);
    }

    #[test]
    fn orientation_turns_around_y_after_x_after_z() {
        let mut world = World::new();
        part(
            &mut world,
            vec![
                ("Size", vector(4.0, 1.0, 2.0)),
                ("Orientation", vector(90.0, 90.0, 0.0)),
            ],
        );
        let item = only_item(&world);
        // The part's long (+X) side: X then Y turns it from +X to -Z.
        let end = item.transform.transform_point3(Vec3::new(0.5, 0.0, 0.0));
        assert!(end.abs_diff_eq(Vec3::new(0.0, 0.0, -2.0), 1e-5), "{end}");
        // Its top (+Y): 90° around X points it at +Z, then 90° around Y at +X.
        let top = item.transform.transform_point3(Vec3::new(0.0, 0.5, 0.0));
        assert!(top.abs_diff_eq(Vec3::new(0.5, 0.0, 0.0), 1e-5), "{top}");
    }

    #[test]
    fn balls_and_cylinders_keep_round() {
        assert_eq!(
            mesh_scale(PartType::Ball, Vec3::new(4.0, 1.0, 2.0)),
            Vec3::ONE
        );
        assert_eq!(
            mesh_scale(PartType::Cylinder, Vec3::new(4.0, 3.0, 2.0)),
            Vec3::new(4.0, 2.0, 2.0)
        );
        assert_eq!(
            mesh_scale(PartType::Wedge, Vec3::new(4.0, 3.0, 2.0)),
            Vec3::new(4.0, 3.0, 2.0)
        );
    }

    #[test]
    fn glass_is_see_through_and_neon_glows_in_its_color() {
        let color = Color3::new(0.2, 0.9, 1.0);
        for (material, opacity, glows) in [
            (PartMaterial::Glass, 0.3, false),
            (PartMaterial::Neon, 1.0, true),
        ] {
            let mut world = World::new();
            part(
                &mut world,
                vec![
                    ("Material", Value::Material(material)),
                    ("Color", Value::Color3(color)),
                ],
            );
            let item = only_item(&world);
            assert_eq!(item.material.opacity, opacity, "{material:?}");
            let expected_glow = if glows { [0.2, 0.9, 1.0] } else { [0.0; 3] };
            let boosted = expected_glow.map(|channel| channel * GLOW_BOOST);
            assert_eq!(item.material.emissive, boosted, "{material:?}");
        }
    }

    #[test]
    fn every_shape_has_a_mesh() {
        let mut world = World::new();
        for shape in PartType::ALL {
            part(&mut world, vec![("Shape", Value::PartType(shape))]);
        }
        let shapes: Vec<Shape> = build_draw_list(&world, &Assets::default(), camera())
            .items
            .iter()
            .map(|item| item.shape.clone())
            .collect();
        assert_eq!(
            shapes,
            [Shape::Block, Shape::Sphere, Shape::Cylinder, Shape::Wedge]
        );
    }

    #[test]
    fn parts_outside_workspace_are_not_drawn() {
        let mut world = World::new();
        world.create(ClassName::Part).unwrap();
        assert!(
            build_draw_list(&world, &Assets::default(), camera())
                .items
                .is_empty()
        );
    }

    fn example_assets() -> Assets {
        Assets::new(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-mesh"),
        )
    }

    fn mesh_part(world: &mut World, mesh_id: &str) -> InstanceId {
        let part = world.create(ClassName::MeshPart).unwrap();
        world.set_parent(part, Some(world.workspace())).unwrap();
        world
            .set_property(part, "MeshId", Value::String(mesh_id.to_owned()))
            .unwrap();
        world
            .set_property(part, "Size", vector(8.0, 6.0, 8.0))
            .unwrap();
        part
    }

    #[test]
    fn a_mesh_part_draws_its_model_with_the_files_material() {
        let mut world = World::new();
        mesh_part(&mut world, "assets/models/pyramid.gltf");
        let assets = example_assets();
        let item = only_item_with(&world, &assets);
        let Shape::Mesh(mesh) = &item.shape else {
            panic!("drawn as {:?}", item.shape);
        };
        assert_eq!(mesh.positions().len(), mesh.uvs().len());
        // Size stretches the model's 1×1×1 box, like a block's.
        assert_eq!(
            item.transform.transform_point3(Vec3::splat(0.5)),
            Vec3::new(4.0, 3.0, 4.0)
        );
        assert!(item.material.base_color_texture.is_some());
        assert_eq!(item.material.roughness, 0.8);
        assert_eq!(item.material.opacity, 1.0);
        assert!(assets.take_errors().is_empty());
    }

    #[test]
    fn a_surface_appearance_replaces_its_materials_maps() {
        // A project with the pyramid imported, so its texture is a file in it.
        let project = std::env::temp_dir().join("axcenity-scene-surface");
        let _ = std::fs::remove_dir_all(&project);
        std::fs::create_dir_all(&project).unwrap();
        let pyramid = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/hello-mesh/assets/models/pyramid.gltf");
        let (mesh_id, model) = axcenity_assets::import_model(&project, &pyramid).unwrap();
        let base = model.primitives[0]
            .material
            .base_color_texture
            .clone()
            .unwrap();
        let texture = format!("assets/models/pyramid/{}", base.file.as_ref().unwrap());
        let assets = Assets::new(&project);

        let mut world = World::new();
        let part = mesh_part(&mut world, &mesh_id);
        let appearance = world.create(ClassName::SurfaceAppearance).unwrap();
        world.set_parent(appearance, Some(part)).unwrap();
        let set = |world: &mut World, name, value| {
            world.set_property(appearance, name, value).unwrap();
        };
        // For another material: the file's own maps are drawn.
        set(&mut world, "MaterialName", Value::String("Other".into()));
        assert!(
            only_item_with(&world, &assets)
                .material
                .base_color_texture
                .is_some()
        );

        set(
            &mut world,
            "MaterialName",
            Value::String("Sandstone".into()),
        );
        set(&mut world, "RoughnessMap", Value::String(texture));
        set(
            &mut world,
            "RoughnessChannel",
            Value::TextureChannel(TextureChannel::Green),
        );
        set(&mut world, "InvertRoughness", Value::Bool(true));
        let material = only_item_with(&world, &assets).material;
        // Its ColorMap is empty: no color texture.
        assert!(material.base_color_texture.is_none());
        let roughness = material.maps.roughness.unwrap();
        assert_eq!(
            (roughness.channel, roughness.invert),
            (Channel::Green, true)
        );
        assert_eq!(material.roughness, 1.0);
        assert!(assets.take_errors().is_empty());
        let _ = std::fs::remove_dir_all(&project);
    }

    #[test]
    fn a_mesh_part_without_a_model_is_a_grey_box_and_the_reason_is_reported() {
        let assets = example_assets();
        for mesh_id in ["", "assets/models/missing.glb"] {
            let mut world = World::new();
            mesh_part(&mut world, mesh_id);
            let item = only_item_with(&world, &assets);
            assert_eq!(item.shape, Shape::Block, "{mesh_id:?}");
        }
        let errors = assets.take_errors();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("missing.glb"), "{errors:?}");
    }

    #[test]
    fn a_part_draws_its_surface_appearance_color_map() {
        let project = std::env::temp_dir().join(format!(
            "axcenity-scene-part-texture-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&project);
        std::fs::create_dir_all(project.join("assets/textures")).unwrap();
        let image = image::RgbaImage::from_pixel(1, 1, image::Rgba([200, 40, 40, 255]));
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        std::fs::write(project.join("assets/textures/swatch.png"), png.into_inner()).unwrap();
        let assets = Assets::new(&project);

        let mut world = World::new();
        let part = part(&mut world, vec![]);
        let appearance = world.create(ClassName::SurfaceAppearance).unwrap();
        world.set_parent(appearance, Some(part)).unwrap();
        world
            .set_property(
                appearance,
                "ColorMap",
                Value::String("assets/textures/swatch.png".into()),
            )
            .unwrap();

        let item = only_item_with(&world, &assets);
        assert!(item.material.base_color_texture.is_some());
        assert_eq!(item.uv_tile, 8.0);
        assert!(assets.take_errors().is_empty());
        let _ = std::fs::remove_dir_all(&project);
    }

    #[test]
    fn linear_to_srgb_undoes_the_renderers_conversion() {
        let percent = srgb([0.0, 1.0, 0.214]).map(|c| (c * 100.0).round());
        assert_eq!(percent, [0.0, 100.0, 50.0]);
    }
}
