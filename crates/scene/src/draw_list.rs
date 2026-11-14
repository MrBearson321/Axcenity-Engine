//! Turns the Instance tree into a draw list for `render`.
//!
//! This is the only place that knows about both Instances and the renderer,
//! which keeps `render` free of engine types. The player and the editor
//! viewport both use it, so they draw a world the same way.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, OnceLock};

use axcenity_assets::{
    AlphaMode, Assets, ChannelTexture, Image, Model, ModelMaterial, Posed, Primitive,
};
use axcenity_core::{ClassName, Instance, InstanceId, PartType, TextureChannel, Value, World};
use axcenity_render::glam::{Mat4, Quat, Vec3};
use axcenity_render::{
    AmbientLight, Camera, Channel, ChannelMap, Clouds, Cone, DecalRole, DrawItem, DrawList, Fog,
    Light, Material, MeshSource, NormalMap, PointShadows, PostProcessing, ProbeImage, RectArea,
    ReflectionProbe, Shadows, Shape, Sky, SkyImage, SpotShadows, TextureMaps, TextureSource,
};
use axcenity_ui::{Stylesheet, UiInput, UiInteraction, UiLayout};

use crate::graphics_volumes::Look;
use crate::material;
use crate::mesh_cache::{MeshCache, ModelLookup};
use crate::orientation::rotation_from_orientation;
use crate::terrain::TerrainCache;

mod lights;
mod materials;

use lights::*;
pub use lights::{LightMarker, MarkerKind, Mount, light_markers, mount_of};
use materials::*;
pub(crate) use materials::{image_texture, srgb, texture};

// A little flat light on top of the sky's, so shadowed corners aren't black.
const AMBIENT: AmbientLight = AmbientLight {
    color: [0.75, 0.85, 1.0],
    intensity: 0.05,
    occlusion: 1.0,
};
/// How much brighter than its color a glowing part is drawn when bloom is
/// on, so it goes past the bloom threshold and glows.
pub(crate) const GLOW_BOOST: f32 = 2.5;

/// Animated MeshParts' posed models, by MeshPart (the host's animators make
/// them). A MeshPart in here is drawn posed; the rest are drawn still.
pub type Poses = HashMap<InstanceId, Arc<Posed>>;

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
        &mut TerrainCache::for_one_frame(),
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
    profiling::scope!("build_draw_list");
    let settings = lighting_settings(world, assets, &camera);
    let look = settings.look;
    let glow = if look.post.enabled && look.post.bloom_enabled {
        GLOW_BOOST
    } else {
        1.0
    };
    let frame = InstanceFrame {
        world,
        assets,
        poses,
        glow,
        blends: &terrain.blends,
    };
    let mut items = Vec::new();
    let mut other = OtherDrawn::default();
    if terrain.draw_cache.enabled() {
        let (cache, streaming) = (&mut terrain.draw_cache, &terrain.streaming);
        cache.add_to(
            &frame,
            streaming,
            &mut terrain.meshes,
            &mut items,
            &mut other,
        );
    } else {
        let mut characters = crate::decals::Characters::default();
        terrain.meshes.start_frame();
        for (id, instance) in world
            .descendants(world.workspace())
            .into_iter()
            // A Tool put away in an Inventory isn't drawn (Phase 45), nor are
            // the parts kept inside a UnionOperation (Phase 50), nor what
            // streaming has unloaded (Phase 80).
            .filter(|&id| !terrain.streaming.is_hidden(id))
            .filter(|&id| !axcenity_core::csg::out_of_world(world, id))
            .filter_map(|id| Some((id, world.get(id)?)))
        {
            let first = items.len();
            add_instance(
                &frame,
                id,
                instance,
                &mut terrain.meshes,
                &mut characters,
                &mut items,
                &mut other,
            );
            frame
                .blends
                .shift_items(world, id, instance, &mut items[first..]);
        }
        terrain.meshes.end_frame();
    }
    let OtherDrawn {
        lights,
        reflection_probes,
        light_probe_volumes,
        decals,
    } = other;

    let mut chunks = Vec::new();
    terrain.draw_all(world, assets, &camera, &mut items, &mut chunks);

    DrawList {
        sky: look.sky,
        fog: look.fog,
        clouds: settings.clouds,
        weather: crate::weather::settled(world),
        wind: crate::particles::global_wind(world),
        camera,
        sun: look.sun,
        ambient: settings.ambient,
        shadows: settings.shadows,
        lights,
        post: look.post,
        edges: crate::outlines::edges_of(world),
        grading: look.grading,
        lens: crate::lens::lens_of(world),
        items,
        chunks,
        sprites: Vec::new(),
        particle_emitters: Vec::new(),
        decals,
        ui: Vec::new(),
        fonts: assets
            .fonts()
            .iter()
            .map(|file| axcenity_render::UiFont {
                path: file.path.clone(),
                data: file.data.clone(),
            })
            .collect(),
        reflection_probes,
        light_probe_volumes,
        time: 0.0,
        grid: None,
    }
}

/// What's the same for every Instance drawn in a frame.
pub(crate) struct InstanceFrame<'a> {
    pub(crate) world: &'a World,
    pub(crate) assets: &'a Assets,
    pub(crate) poses: &'a Poses,
    /// How much brighter than its color a glowing part is drawn.
    pub(crate) glow: f32,
    /// Moving parts drawn between physics steps. [`add_instance`] moves
    /// only lights by them: callers move the draw items they hand on, so
    /// kept items stay where the world has their parts.
    pub(crate) blends: &'a crate::blend::Blends,
}

/// What a frame's Instances add to the draw list besides draw items.
#[derive(Default)]
pub(crate) struct OtherDrawn {
    pub(crate) lights: Vec<Light>,
    pub(crate) reflection_probes: Vec<ReflectionProbe>,
    pub(crate) light_probe_volumes: Vec<axcenity_render::LightProbeVolume>,
    pub(crate) decals: Vec<axcenity_render::Decal>,
}

/// Adds what `instance` draws to the frame: its draw items to `items`, and
/// anything else to `other`. Everything that decides how an Instance is
/// drawn is here, so a frame built from scratch and one using the
/// [`DrawCache`](crate::retained::DrawCache) can't draw it differently.
pub(crate) fn add_instance(
    frame: &InstanceFrame<'_>,
    id: InstanceId,
    instance: &Instance,
    meshes: &mut MeshCache,
    characters: &mut crate::decals::Characters,
    items: &mut Vec<DrawItem>,
    other: &mut OtherDrawn,
) {
    let InstanceFrame {
        world,
        assets,
        poses,
        glow,
        blends,
    } = *frame;
    let first_item = items.len();
    match instance.class() {
        ClassName::Decal => {
            let part = instance.parent().and_then(|parent| world.get(parent));
            items.extend(
                part.and_then(|part| crate::decals::face_decal_item(id, instance, part, assets)),
            );
        }
        ClassName::ProjectedDecal => {
            other
                .decals
                .extend(crate::decals::projected_decal(instance, assets));
        }
        class if class.has_model() => {
            let posed = poses.get(&id);
            mesh_part_draw_items(world, id, instance, assets, posed, meshes, items)
        }
        class if class.is_light() => {
            let first = other.lights.len();
            other.lights.extend(light_of(world, instance));
            blends.shift_lights(world, id, instance, &mut other.lights[first..]);
        }
        ClassName::ReflectionProbe => other
            .reflection_probes
            .extend(reflection_probe_of(world, instance, assets)),
        ClassName::LightProbeVolume => {
            other
                .light_probe_volumes
                .extend(crate::light_probes::light_probe_volume_of(
                    world, instance, assets,
                ))
        }
        // Drawn chunk by chunk, by the terrain cache.
        ClassName::Terrain => {}
        class if class.is_joint() => items.extend(crate::constraints::draw_items(world, instance)),
        _ => {
            if let Some(mut item) = part_draw_item(world, instance, assets, glow) {
                if let Some(extra) = fixture_glow(world, instance) {
                    for (channel, added) in item.material.emissive.iter_mut().zip(extra) {
                        *channel = (*channel + added).min(4.0);
                    }
                }
                // A capsule is a cylinder and a ball at each end.
                let shape = instance.property("Shape").and_then(Value::as_part_type);
                if shape == Some(PartType::Capsule) {
                    for (piece, transform) in part_pieces(PartType::Capsule, item.transform) {
                        items.push(DrawItem {
                            shape: built_in_shape(piece),
                            transform,
                            ..item.clone()
                        });
                    }
                } else {
                    items.push(item);
                }
            }
        }
    }
    // A character's parts are marked, so projected decals that ignore
    // characters leave them bare.
    if items.len() > first_item && instance.class().is_base_part() && characters.contains(world, id)
    {
        for item in &mut items[first_item..] {
            item.decal_role = DecalRole::Character;
        }
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
        UiFrame {
            interaction: &mut UiInteraction::default(),
            input: UiInput::default(),
            seconds: 0.0,
        },
        &Poses::new(),
        &mut TerrainCache::for_one_frame(),
    )
    .0
}

/// One frame of the UI: the host's [`UiInteraction`], the pointer and
/// keyboard this frame, and how long the frame was, in seconds.
pub struct UiFrame<'a> {
    pub interaction: &'a mut UiInteraction,
    pub input: UiInput,
    pub seconds: f32,
}

/// Builds a scene and interactive UI layout for the pointer, held button
/// and focused text box in `ui`, with `poses` drawn posed. The UI's
/// transitions move on by `ui.seconds`.
#[allow(clippy::too_many_arguments)] // Each is a different input to the frame.
pub fn build_draw_list_with_ui_interaction(
    world: &World,
    assets: &Assets,
    camera: Camera,
    viewport_size: (u32, u32),
    stylesheets: &[Stylesheet],
    ui: UiFrame<'_>,
    poses: &Poses,
    terrain: &mut TerrainCache,
) -> (DrawList, UiLayout) {
    let mut list = build_draw_list_posed(world, assets, camera, poses, terrain);
    profiling::scope!("UiInteraction::layout");
    let layout = ui.interaction.layout(
        world,
        viewport_size,
        stylesheets,
        ui.input,
        Some(&camera),
        ui.seconds,
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

/// How far along a spot light's beam (or one of a point light's shadow
/// faces' directions) from `origin` its own part (placed by `transform`)
/// reaches, plus a little: shadows start there, so the part the light sits
/// on doesn't shade the whole beam. 0 when the beam points away from the
/// part.
fn shadow_near(transform: Mat4, origin: Vec3, direction: Vec3) -> f32 {
    let direction = direction.normalize_or_zero();
    let farthest = (0..8)
        .map(|corner| {
            let half = |bit: i32| if corner & bit == 0 { -0.5 } else { 0.5 };
            let corner = transform.transform_point3(Vec3::new(half(1), half(2), half(4)));
            (corner - origin).dot(direction)
        })
        .fold(0.0_f32, f32::max);
    if farthest > 0.0 { farthest + 0.05 } else { 0.0 }
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
        if !light.class().is_light() {
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
        class if class.has_model() => PartType::Block,
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

/// The built-in mesh drawn for a part shape (a capsule's middle).
fn built_in_shape(shape: PartType) -> Shape {
    match shape {
        PartType::Block => Shape::Block,
        PartType::Ball => Shape::Sphere,
        PartType::Cylinder | PartType::Capsule => Shape::Cylinder,
        PartType::Wedge => Shape::Wedge,
        PartType::CornerWedge => Shape::CornerWedge,
    }
}

/// The shapes a part of `shape` placed by `transform` (from
/// [`part_placement`]) is made of, each with its own transform: the part
/// itself, or for a capsule, the cylinder between its round ends and a
/// ball at each end. Drawing and picking both use these.
pub(crate) fn part_pieces(shape: PartType, transform: Mat4) -> Vec<(PartType, Mat4)> {
    if shape != PartType::Capsule {
        return vec![(shape, transform)];
    }
    // The capsule's box is its length along X by its diameter twice, so in
    // the 1×1×1 mesh's space the ends are `reach` from the middle.
    let length = transform.x_axis.truncate().length();
    let diameter = transform.y_axis.truncate().length();
    if !(length > 0.0 && diameter > 0.0) {
        return vec![(PartType::Cylinder, transform)];
    }
    let reach = ((length - diameter) / 2.0).max(0.0) / length;
    let ball = diameter / length;
    let mut pieces = vec![(
        PartType::Cylinder,
        transform * Mat4::from_scale(Vec3::new(2.0 * reach, 1.0, 1.0)),
    )];
    for end in [-reach, reach] {
        pieces.push((
            PartType::Ball,
            transform
                * Mat4::from_translation(Vec3::new(end, 0.0, 0.0))
                * Mat4::from_scale(Vec3::new(ball, 1.0, 1.0)),
        ));
    }
    pieces
}

/// Whether a Part or MeshPart casts a sun shadow: its `CastShadow`, on
/// unless it's been turned off.
fn casts_shadow(instance: &Instance) -> bool {
    instance.property("CastShadow").and_then(Value::as_bool) != Some(false)
}

/// The draw item for a Part, or `None` for any other class. `glow` is how
/// much brighter than its color a glowing part is drawn. A SurfaceAppearance
/// inside the part replaces its flat color with that appearance's maps;
/// without one, a `MaterialVariant` replaces its Material (and its
/// Metalness, Roughness and Emission), tinted by its Color.
/// How a NegateOperation is drawn: pink, and half see-through.
const NEGATE_COLOR: [f32; 3] = [1.0, 0.4, 0.7];
const NEGATE_OPACITY: f32 = 0.5;

fn part_draw_item(
    world: &World,
    instance: &Instance,
    assets: &Assets,
    glow: f32,
) -> Option<DrawItem> {
    let number = |name| instance.property(name).and_then(Value::as_number);
    let (shape, transform) = part_placement(instance)?;
    let color = instance.property("Color").and_then(Value::as_color3)?;
    let kind = instance.property("Material").and_then(Value::as_material)?;
    let appearance = instance.children().iter().find_map(|&id| {
        let child = world.get(id)?;
        (child.class() == ClassName::SurfaceAppearance).then_some(child)
    });
    let variant = match appearance {
        Some(_) => None,
        None => material::variant_of(instance, assets),
    };

    // A NegateOperation shows pink and see-through (Phase 50).
    let negated = instance.class() == ClassName::NegateOperation;
    let color = if negated {
        NEGATE_COLOR
    } else {
        [color.r, color.g, color.b]
    };
    let sway = variant
        .as_ref()
        .and_then(|standard| material::variant_sway(standard, transform));
    let blueprint = appearance
        .is_none()
        .then(|| material::blueprint_of(instance, assets))
        .flatten();
    let material = match (appearance, variant) {
        (None, _) if blueprint.is_some() => Material {
            base_color: color.map(|channel| (channel / BLUEPRINT_PLAIN_GREY).min(1.0)),
            base_color_texture: blueprint.as_ref().map(texture),
            metalness: 0.0,
            roughness: BLUEPRINT_ROUGHNESS,
            emissive: added_glow(instance),
            opacity: visibility(instance),
            ..Material::default()
        },
        (None, Some(standard)) => {
            let mut material = material::variant_material(&standard, assets);
            material.base_color = std::array::from_fn(|i| material.base_color[i] * color[i]);
            material.opacity *= visibility(instance);
            material
        }
        (appearance, _) => preset_material(instance, appearance, assets, kind, color, glow)?,
    };
    let mut material = material;
    if negated {
        material.opacity *= NEGATE_OPACITY;
    }
    if material.opacity <= 0.0 {
        return None;
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
    // Water is the top of the part's box, cut fine enough to make waves.
    let shape = if material.water.is_some() {
        Shape::Mesh(axcenity_render::water_surface())
    } else {
        built_in_shape(shape)
    };
    Some(DrawItem {
        shape,
        transform,
        material,
        uv_tile,
        casts_shadow: casts_shadow(instance),
        id: 0,
        cull_distance: 0.0,
        skin: None,
        sway,
        decal_role: Default::default(),
    })
}

/// A new part's Color: on a blueprint it's no tint at all, so the grid's
/// lines stay white on a part nobody has painted.
const BLUEPRINT_PLAIN_GREY: f32 = 0.64;
/// How rough the editor's blueprint texture is drawn.
const BLUEPRINT_ROUGHNESS: f32 = 0.8;

/// The light a part's `Emission` and `EmissiveColor` give off.
fn added_glow(instance: &Instance) -> [f32; 3] {
    let emission = instance
        .property("Emission")
        .and_then(Value::as_number)
        .unwrap_or(0.0)
        .max(0.0) as f32;
    match instance
        .property("EmissiveColor")
        .and_then(Value::as_color3)
    {
        Some(tint) if emission > 0.0 => [tint.r, tint.g, tint.b].map(|c| c * emission),
        _ => [0.0; 3],
    }
}

/// A MeshPart's draw items, one per part of its model, each with the
/// model's own material, or its `MaterialVariant` in place of them all, or
/// with the maps of the SurfaceAppearance inside it for that material. While its model
/// is still loading, nothing is drawn for it. With no model at all (no
/// `MeshId` yet, or one that can't be loaded), it's drawn as a plain grey
/// box, so it can still be seen and picked. With `posed` (a pose of its model), the model is
/// drawn in that pose.
fn mesh_part_draw_items(
    world: &World,
    id: InstanceId,
    instance: &Instance,
    assets: &Assets,
    posed: Option<&Arc<Posed>>,
    meshes: &mut MeshCache,
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
    let (model, pieces) = match meshes.models.get(assets, mesh_id) {
        ModelLookup::Ready(model, pieces) => (model, pieces),
        ModelLookup::Loading => return,
        ModelLookup::Failed => {
            items.push(DrawItem {
                shape: Shape::Block,
                transform,
                material: Material::default(),
                uv_tile: 0.0,
                casts_shadow: casts_shadow(instance),
                id: 0,
                cull_distance: 0.0,
                skin: None,
                sway: None,
                decal_role: Default::default(),
            });
            return;
        }
    };
    let appearances: Vec<&Instance> = instance
        .children()
        .iter()
        .filter_map(|&id| world.get(id))
        .filter(|child| child.class() == ClassName::SurfaceAppearance)
        .collect();
    let variant = material::variant_of(instance, assets);
    let sway = variant
        .as_ref()
        .and_then(|standard| material::variant_sway(standard, transform));
    // A UnionOperation with UsePartColor is drawn all in its Color.
    let whole_color = (instance.property("UsePartColor").and_then(Value::as_bool) == Some(true))
        .then(|| instance.property("Color").and_then(Value::as_color3))
        .flatten()
        .map(|color| [color.r, color.g, color.b]);
    // A pose of another model (the MeshId just changed) isn't used. Every
    // part of the model moves with the pose's joints, on the GPU.
    // A model with blend shapes the file leans toward, and nothing posing
    // it, is drawn leaning toward them.
    let skin = posed
        .filter(|posed| posed.model_key == model.key())
        .map(|posed| meshes.skins.get(id, posed))
        .or_else(|| meshes.skins.rest(model));
    // The model's meshes and file materials are made once and shared by
    // every MeshPart showing it, frame after frame (I8).
    for (primitive, piece) in model.primitives.iter().zip(pieces) {
        let appearance = appearances.iter().find(|appearance| {
            appearance.property("MaterialName").and_then(Value::as_str)
                == Some(primitive.material.name.as_str())
        });
        let mut material = match (appearance, &variant) {
            (Some(appearance), _) => surface_material(&primitive.material, appearance, assets),
            (None, Some(standard)) => material::variant_material(standard, assets),
            (None, None) => piece.material.clone(),
        };
        if let Some(color) = whole_color {
            material.base_color = color;
            material.base_color_texture = None;
        }
        items.push(DrawItem {
            shape: piece.shape.clone(),
            transform,
            material,
            uv_tile: 0.0,
            casts_shadow: casts_shadow(instance),
            id: draw_id(id),
            cull_distance: 0.0,
            skin: skin.clone(),
            sway: match &variant {
                Some(_) => sway,
                None => material::model_sway(&primitive.material, transform),
            },
            decal_role: Default::default(),
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

/// A model's primitives at `transform`, using the file's own materials.
/// `id` is the same for the same thing each frame (see [`DrawItem::id`]),
/// and `cull_distance` is how far away it's still drawn (0 for always).
pub(crate) fn push_model(
    pieces: &[ModelPiece],
    transform: Mat4,
    id: u64,
    cull_distance: f32,
    items: &mut Vec<DrawItem>,
) {
    for piece in pieces {
        items.push(DrawItem {
            shape: piece.shape.clone(),
            transform,
            material: piece.material.clone(),
            uv_tile: 0.0,
            casts_shadow: true,
            id,
            cull_distance,
            skin: None,
            sway: material::box_sway(piece.sway, piece.flutter, transform),
            decal_role: Default::default(),
        });
    }
}

/// What every placed copy of one primitive of a model shares. A forest
/// paints tens of thousands of copies of a few models, so this is worked
/// out once a frame per model rather than once per copy.
pub(crate) struct ModelPiece {
    pub(crate) shape: Shape,
    material: Material,
    sway: f32,
    flutter: f32,
}

/// The [`ModelPiece`]s of `model`, one per primitive.
pub(crate) fn model_pieces(model: &Arc<Model>) -> Vec<ModelPiece> {
    model
        .primitives
        .iter()
        .enumerate()
        .map(|(index, primitive)| ModelPiece {
            shape: Shape::Mesh(Arc::new(ModelMesh::new(model, index))),
            material: model_material(&primitive.material),
            sway: primitive.material.sway,
            flutter: primitive.material.flutter,
        })
        .collect()
}

/// The [`DrawItem::id`] of an Instance's items (a MeshPart's every
/// primitive shares it), for asking the renderer about them (the editor's
/// LOD inspector: `TextureRenderer::lod_level_of`, `preview_lod`).
pub fn item_id(id: InstanceId) -> u64 {
    draw_id(id)
}

/// A [`DrawItem::id`] for `thing`: the same for the same thing, and never
/// 0 (which is none).
pub(crate) fn draw_id(thing: impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    thing.hash(&mut hasher);
    hasher.finish().max(1)
}

/// One primitive of a model at one level of detail, handed to the renderer
/// without copying it.
struct ModelMesh {
    model: Arc<Model>,
    index: usize,
    /// 0 for the primitive itself, else which of its simpler copies.
    level: usize,
    /// Its simpler copies, each made the first time it's drawn and kept:
    /// the renderer asks for one per draw.
    copies: Vec<OnceLock<Arc<dyn MeshSource>>>,
}

impl ModelMesh {
    fn new(model: &Arc<Model>, index: usize) -> Self {
        Self {
            model: model.clone(),
            index,
            level: 0,
            copies: (0..model.primitives[index].lods.len())
                .map(|_| OnceLock::new())
                .collect(),
        }
    }

    fn primitive(&self) -> &Primitive {
        let full = &self.model.primitives[self.index];
        match self.level {
            0 => full,
            level => &full.lods[level - 1],
        }
    }
}

impl MeshSource for ModelMesh {
    fn key(&self) -> u64 {
        self.primitive().key()
    }
    fn positions(&self) -> &[[f32; 3]] {
        &self.primitive().positions
    }
    fn normals(&self) -> &[[f32; 3]] {
        &self.primitive().normals
    }
    fn uvs(&self) -> &[[f32; 2]] {
        &self.primitive().uvs
    }
    fn tangents(&self) -> &[[f32; 4]] {
        &self.primitive().tangents
    }
    fn indices(&self) -> &[u32] {
        &self.primitive().indices
    }
    fn skin_joints(&self) -> &[[u16; 4]] {
        &self.primitive().joints
    }
    fn skin_weights(&self) -> &[[f32; 4]] {
        &self.primitive().weights
    }
    fn morph_count(&self) -> usize {
        self.primitive().morphs.len()
    }
    fn morph(&self, index: usize) -> Option<axcenity_render::MorphView<'_>> {
        let morph = self.primitive().morphs.get(index)?;
        Some(axcenity_render::MorphView {
            target: morph.target,
            corners: &morph.corners,
            positions: &morph.positions,
            normals: &morph.normals,
        })
    }
    fn lod_count(&self) -> usize {
        self.copies.len()
    }
    fn lod(&self, level: usize) -> Option<&Arc<dyn MeshSource>> {
        let copy = self.copies.get(level.checked_sub(1)?)?;
        Some(copy.get_or_init(|| {
            Arc::new(ModelMesh {
                model: self.model.clone(),
                index: self.index,
                level,
                copies: Vec::new(),
            })
        }))
    }
}

/// How much to stretch the 1×1×1 mesh. Balls stay round and cylinders stay
/// circular, whatever their `Size`: they use the smallest size that fits.
fn mesh_scale(shape: PartType, size: Vec3) -> Vec3 {
    match shape {
        PartType::Block | PartType::Wedge | PartType::CornerWedge => size,
        PartType::Ball => Vec3::splat(size.min_element()),
        PartType::Cylinder | PartType::Capsule => {
            let diameter = size.y.min(size.z);
            Vec3::new(size.x, diameter, diameter)
        }
    }
}

#[cfg(test)]
mod tests;
