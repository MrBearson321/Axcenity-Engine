//! The Lighting service's settings for the draw list, and the lights,
//! reflection probes and light markers placed from Instances.

use super::*;

/// Everything the Lighting service's properties (and its Atmosphere) set,
/// with the graphics volumes around the camera blended into its look.
pub(super) struct LightingSettings {
    pub(super) look: Look,
    pub(super) shadows: Shadows,
    pub(super) clouds: Clouds,
    pub(super) ambient: AmbientLight,
}

pub(super) fn lighting_settings(
    world: &World,
    assets: &Assets,
    camera: &Camera,
) -> LightingSettings {
    let mut settings = LightingSettings {
        look: Look {
            sun: crate::time_of_day::default_sun(),
            sky: Sky::default(),
            fog: Fog::default(),
            post: PostProcessing::default(),
            grading: None,
        },
        shadows: Shadows::default(),
        clouds: Clouds::default(),
        ambient: AMBIENT,
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
    let look = &mut settings.look;
    let (shadows, sky, post) = (&mut settings.shadows, &mut look.sky, &mut look.post);
    shadows.enabled = bool_of("ShadowsEnabled", shadows.enabled);
    shadows.distance = number_of("ShadowDistance", shadows.distance);
    shadows.softness = number_of("ShadowSoftness", shadows.softness);
    let ambient = &mut settings.ambient;
    ambient.occlusion = number_of("AmbientOcclusionIntensity", ambient.occlusion).clamp(0.0, 2.0);
    sky.top_color = color_of("SkyTopColor", sky.top_color);
    sky.horizon_color = color_of("SkyHorizonColor", sky.horizon_color);
    sky.ground_color = color_of("GroundColor", sky.ground_color);
    sky.environment_intensity = number_of("EnvironmentIntensity", sky.environment_intensity);
    if let Some(Value::String(path)) = lighting.property("SkyImage") {
        sky.image = assets.sky_image(path).map(|image| SkyImage {
            key: image.key(),
            width: image.width,
            height: image.height,
            rgb: image.rgb.clone(),
        });
    }
    post.enabled = bool_of("PostProcessingEnabled", post.enabled);
    post.exposure = number_of("Exposure", post.exposure);
    if let Some(curve) = lighting
        .property("ToneMapping")
        .and_then(Value::as_tone_mapping)
    {
        post.tone_mapping = tone_mapping(curve);
    }
    post.bloom_enabled = bool_of("BloomEnabled", post.bloom_enabled);
    post.bloom_intensity = number_of("BloomIntensity", post.bloom_intensity);
    post.bloom_threshold = number_of("BloomThreshold", post.bloom_threshold);
    look.sun = crate::time_of_day::sun_and_sky(world, lighting, &mut look.sky);
    look.fog = crate::fog::fog_of(world, lighting);
    look.grading = crate::color_grading::grading_of(world, assets);
    settings.clouds = crate::clouds::clouds_of(world, lighting);
    settings.look =
        crate::graphics_volumes::blend(world, assets, lighting, camera.position, settings.look);
    settings
}

pub(super) fn tone_mapping(curve: axcenity_core::ToneMapping) -> axcenity_render::ToneMapping {
    use axcenity_core::ToneMapping as Curve;
    match curve {
        Curve::Classic => axcenity_render::ToneMapping::Classic,
        Curve::Neutral => axcenity_render::ToneMapping::Neutral,
        Curve::Filmic => axcenity_render::ToneMapping::Filmic,
    }
}

/// Where a light or ParticleEmitter works from: inside a part, the part
/// (placed by the transform that stretches the 1×1×1 cube to fill it);
/// anywhere else, its own `Position` and `Orientation`, as a light needs
/// no mesh in Unreal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mount {
    Part { id: InstanceId, transform: Mat4 },
    Free { position: Vec3, rotation: Quat },
}

impl Mount {
    /// Its turn in the world.
    pub fn rotation(&self) -> Quat {
        match *self {
            Mount::Part { transform, .. } => transform.to_scale_rotation_translation().1,
            Mount::Free { rotation, .. } => rotation,
        }
    }

    /// The part it's in, if it's in one.
    pub fn part(&self) -> Option<InstanceId> {
        match *self {
            Mount::Part { id, .. } => Some(id),
            Mount::Free { .. } => None,
        }
    }
}

/// Where the light or ParticleEmitter `instance` works from (see
/// [`Mount`]). `None` for one whose Position or Orientation isn't a
/// finite Vector3, or with no parent.
pub fn mount_of(world: &World, instance: &Instance) -> Option<Mount> {
    let parent_id = instance.parent()?;
    let parent = world.get(parent_id)?;
    if let Some((_, transform)) = part_placement(parent) {
        return Some(Mount::Part {
            id: parent_id,
            transform,
        });
    }
    let vector = |name| instance.property(name).and_then(Value::as_vector3);
    let position = vector("Position")?;
    let position = Vec3::new(position.x, position.y, position.z);
    let rotation = rotation_from_orientation(vector("Orientation")?);
    (position.is_finite() && rotation.is_finite()).then_some(Mount::Free { position, rotation })
}

/// Where a light shines from, and the way a SpotLight points or a
/// RectLight faces (turned with what it's on), with a RectLight's right.
pub(super) struct Placed {
    position: Vec3,
    direction: Vec3,
    right: Vec3,
}

/// Where `instance` (a light) shines from on `mount`: just above its part,
/// so the part's own top and the things around it are lit (a RectLight on
/// the side of its part it faces), or at its own Position.
pub(super) fn place_light(instance: &Instance, mount: Mount) -> Placed {
    let rotation = mount.rotation();
    let local = instance
        .property("Direction")
        .and_then(Value::as_vector3)
        .map_or(Vec3::NEG_Y, |d| Vec3::new(d.x, d.y, d.z))
        .normalize_or(Vec3::NEG_Y);
    // A rect's width runs along X, or along Z when it faces along X.
    let across = if local.x.abs() > 0.9 {
        Vec3::Z
    } else {
        Vec3::X
    };
    let direction = rotation * local;
    let right = rotation * across;
    let position = match mount {
        Mount::Free { position, .. } => position,
        Mount::Part { transform, .. } if instance.class() == ClassName::RectLight => {
            // On the face it looks out of, a hair off it.
            // How far the box reaches from its middle along `local`: to
            // the first of its faces that way.
            let (scale, _, center) = transform.to_scale_rotation_translation();
            let half = scale.abs() * 0.5;
            let reach = (0..3)
                .filter(|&axis| local[axis].abs() > 1e-6)
                .map(|axis| half[axis] / local[axis].abs())
                .fold(f32::INFINITY, f32::min);
            let reach = if reach.is_finite() { reach } else { 0.0 };
            center + direction * (reach + 0.05)
        }
        Mount::Part { transform, .. } => light_origin(transform),
    };
    Placed {
        position,
        direction,
        right,
    }
}

/// The light a PointLight, SpotLight or RectLight gives off (see
/// [`place_light`]), or `None` if it's switched off or has nowhere to be.
pub(super) fn light_of(world: &World, instance: &Instance) -> Option<Light> {
    if instance.property("Enabled").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let mount = mount_of(world, instance)?;
    let placed = place_light(instance, mount);
    let position = placed.position;
    // How far along `direction` its own part reaches: shadows start there.
    // A free-standing light has no part to keep out of its shadows.
    let near = |direction: Vec3| match mount {
        Mount::Part { transform, .. } => shadow_near(transform, position, direction),
        Mount::Free { .. } => 0.0,
    };
    let color = instance.property("Color").and_then(Value::as_color3)?;
    let number = |name| instance.property(name).and_then(Value::as_number);
    let shadows = instance.property("Shadows").and_then(Value::as_bool) == Some(true);
    let cone = match instance.class() {
        ClassName::SpotLight => Some(Cone {
            direction: placed.direction,
            angle_degrees: number("Angle")? as f32,
            shadows: shadows.then(|| SpotShadows {
                near: near(placed.direction),
            }),
        }),
        _ => None,
    };
    let rect = match instance.class() {
        ClassName::RectLight => Some(RectArea {
            direction: placed.direction,
            right: placed.right,
            width: number("Width")?.max(0.0) as f32,
            height: number("Height")?.max(0.0) as f32,
        }),
        _ => None,
    };
    // Each way along each world axis, as the point light's shadow faces are.
    let point_shadows = (shadows && cone.is_none()).then(|| PointShadows {
        near: [
            Vec3::X,
            Vec3::NEG_X,
            Vec3::Y,
            Vec3::NEG_Y,
            Vec3::Z,
            Vec3::NEG_Z,
        ]
        .map(near),
    });
    Some(Light {
        position,
        color: [color.r, color.g, color.b],
        brightness: number("Brightness")? as f32,
        range: number("Range")? as f32,
        cone,
        rect,
        point_shadows,
    })
}

/// The box a ReflectionProbe reflects in: its parent part's, with the bake
/// its `Bake` names. `None` if it isn't inside a part, or has no bake that
/// can be read (a missing file is reported once by `assets`), so the sky
/// is reflected there instead.
pub(super) fn reflection_probe_of(
    world: &World,
    instance: &Instance,
    assets: &Assets,
) -> Option<ReflectionProbe> {
    let (_, transform) = part_placement(world.get(instance.parent()?)?)?;
    let Some(Value::String(path)) = instance.property("Bake") else {
        return None;
    };
    let image = assets.reflection_probe(path)?;
    let (size, rotation, center) = transform.to_scale_rotation_translation();
    let blend_distance = instance
        .property("BlendDistance")
        .and_then(Value::as_number)
        .unwrap_or(0.0) as f32;
    Some(ReflectionProbe {
        center,
        rotation,
        half_size: size.abs() * 0.5,
        blend_distance: blend_distance.max(0.0),
        image: ProbeImage {
            key: image.key(),
            size: image.size,
            rgb: image.rgb.clone(),
        },
    })
}

/// What a [`LightMarker`] stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkerKind {
    Point,
    Spot,
    Rect,
    Emitter,
}

/// Where a light or ParticleEmitter in the Workspace is, for the editor to
/// show it: neither has a shape of its own, so one standing free is drawn
/// as an icon, and a light's reach as rings, a cone or a rectangle.
#[derive(Clone, Debug, PartialEq)]
pub struct LightMarker {
    pub id: InstanceId,
    pub kind: MarkerKind,
    /// The part it's in, or `None` when it stands free (and has an icon).
    pub part: Option<InstanceId>,
    pub enabled: bool,
    /// Where the light shines from (just above its part, or its own
    /// Position), or an emitter's middle.
    pub position: Vec3,
    pub color: [f32; 3],
    pub range: f32,
    /// For a SpotLight: the way it points (one stud long), and its cone's
    /// whole angle in degrees.
    pub cone: Option<(Vec3, f32)>,
    /// For a RectLight: the way it faces and its right (each one stud
    /// long), and its width and height.
    pub rect: Option<(Vec3, Vec3, f32, f32)>,
}

/// Every light and ParticleEmitter in the Workspace, switched on or not,
/// that has somewhere to be.
pub fn light_markers(world: &World) -> Vec<LightMarker> {
    let mut markers = Vec::new();
    for id in world.descendants(world.workspace()) {
        if axcenity_core::csg::out_of_world(world, id) {
            continue;
        }
        let Some(instance) = world.get(id) else {
            continue;
        };
        let kind = match instance.class() {
            ClassName::PointLight => MarkerKind::Point,
            ClassName::SpotLight => MarkerKind::Spot,
            ClassName::RectLight => MarkerKind::Rect,
            ClassName::ParticleEmitter => MarkerKind::Emitter,
            _ => continue,
        };
        let Some(mount) = mount_of(world, instance) else {
            continue;
        };
        let number = |name| {
            instance
                .property(name)
                .and_then(Value::as_number)
                .unwrap_or(0.0) as f32
        };
        let color = instance
            .property("Color")
            .and_then(Value::as_color3)
            .map_or([1.0; 3], |color| [color.r, color.g, color.b]);
        let enabled = instance.property("Enabled").and_then(Value::as_bool) == Some(true);
        if kind == MarkerKind::Emitter {
            let position = match mount {
                Mount::Part { transform, .. } => transform.transform_point3(Vec3::ZERO),
                Mount::Free { position, .. } => position,
            };
            markers.push(LightMarker {
                id,
                kind,
                part: mount.part(),
                enabled,
                position,
                color,
                range: 0.0,
                cone: None,
                rect: None,
            });
            continue;
        }
        let placed = place_light(instance, mount);
        let direction = placed.direction.normalize_or(Vec3::NEG_Y);
        let right = (placed.right - direction * placed.right.dot(direction))
            .normalize_or(direction.any_orthogonal_vector());
        markers.push(LightMarker {
            id,
            kind,
            part: mount.part(),
            enabled,
            position: placed.position,
            color,
            range: number("Range"),
            cone: (kind == MarkerKind::Spot).then(|| (direction, number("Angle"))),
            rect: (kind == MarkerKind::Rect)
                .then(|| (direction, right, number("Width"), number("Height"))),
        });
    }
    markers
}
