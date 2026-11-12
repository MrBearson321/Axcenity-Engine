//! Materials for draw items: a Part's preset with its appearance's maps
//! over it, a model's file materials, and the textures they sample.

use super::*;

/// A Part's material from its Material preset and properties, with
/// `appearance`'s maps over it. `None` when it's invisible.
pub(super) fn preset_material(
    instance: &Instance,
    appearance: Option<&Instance>,
    assets: &Assets,
    kind: axcenity_core::Material,
    color: [f32; 3],
    glow: f32,
) -> Option<Material> {
    let number = |name| instance.property(name).and_then(Value::as_number);
    let preset = kind.preset();
    let opacity = preset.opacity * visibility(instance);
    if opacity <= 0.0 {
        return None;
    }
    let mut emissive = if preset.glows {
        color.map(|channel| channel * glow)
    } else {
        [0.0; 3]
    };
    for (channel, added) in emissive.iter_mut().zip(added_glow(instance)) {
        *channel += added;
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
    match appearance {
        Some(appearance) => overlay_appearance(&mut material, appearance, assets),
        None => {
            if let Some(surface) = crate::surfaces::of(kind) {
                surface.apply(&mut material);
            }
        }
    }
    Some(material)
}

/// True when `material` samples a texture, so a built-in shape should
/// tile its coordinates in studs.
pub(super) fn has_texture_maps(material: &Material) -> bool {
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
pub(super) fn overlay_appearance(material: &mut Material, appearance: &Instance, assets: &Assets) {
    let image_of = |name| {
        let path = appearance.property(name).and_then(Value::as_str)?;
        assets.texture(path)
    };
    let texture_of = |name| image_of(name).map(|image| texture(&image));
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
    if let Some(image) = image_of("NormalMap") {
        material.maps.normal = Some(NormalMap {
            texture: normal_texture(&image),
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
    place_maps(material, appearance);
}

/// Places a material's maps by `appearance`'s `Tiling` and `Offset`, in
/// place of any placement of its own.
pub(super) fn place_maps(material: &mut Material, appearance: &Instance) {
    let vector = |name, default| {
        appearance
            .property(name)
            .and_then(Value::as_vector2)
            .map_or(default, |v| [v.x, v.y])
    };
    material.uv_scale = vector("Tiling", [1.0, 1.0]);
    material.uv_rotation = 0.0;
    material.uv_offset = vector("Offset", [0.0, 0.0]);
}

/// A model's material with a SurfaceAppearance's maps in place of its own.
/// A map the SurfaceAppearance leaves empty is no map (metalness and
/// roughness then take the material's own values); one it sets gives the
/// value alone. Its textures come from `assets`.
pub(super) fn surface_material(
    file: &ModelMaterial,
    appearance: &Instance,
    assets: &Assets,
) -> Material {
    let image_of = |name| {
        let path = appearance.property(name).and_then(Value::as_str)?;
        assets.texture(path)
    };
    let texture_of = |name| image_of(name).map(|image| texture(&image));
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
    maps.normal = image_of("NormalMap").map(|image| NormalMap {
        texture: normal_texture(&image),
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
    place_maps(&mut material, appearance);
    material
}

/// glTF's material values, as the renderer takes them.
pub(super) fn model_material(material: &ModelMaterial) -> Material {
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
                texture: normal_texture(image),
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
        uv_scale: material.uv_scale,
        uv_rotation: material.uv_rotation,
        uv_offset: material.uv_offset,
        double_sided: material.double_sided,
        translucency: material.translucency,
        terrain: None,
        ..Material::default()
    }
}

/// A model's image, handed to the renderer without copying it.
pub(crate) fn texture(image: &Arc<Image>) -> Arc<dyn TextureSource> {
    Arc::new(ImageTexture(image.clone()))
}

/// A model's image used as a normal map: its BC5 levels when it was cooked
/// as one (under a key of its own, since they hold only red and green),
/// otherwise the same texture as [`texture`].
pub(crate) fn normal_texture(image: &Arc<Image>) -> Arc<dyn TextureSource> {
    match image.cooked() {
        Some(cooked) if cooked.format == axcenity_assets::CookedFormat::Bc5 => {
            Arc::new(NormalTexture(image.clone()))
        }
        _ => texture(image),
    }
}

/// A project image as a texture the renderer can draw, for particles.
pub(crate) fn image_texture(image: &Arc<Image>) -> Arc<dyn TextureSource> {
    texture(image)
}

pub(super) fn channel_map(map: &ChannelTexture) -> ChannelMap {
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
pub(crate) fn srgb(linear: [f32; 3]) -> [f32; 3] {
    linear.map(|c| {
        if c <= 0.0031308 {
            c * 12.92
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        }
    })
}

/// A model's texture, handed to the renderer without copying it.
pub(super) struct ImageTexture(pub(super) Arc<Image>);

impl TextureSource for ImageTexture {
    fn key(&self) -> u64 {
        self.0.key()
    }
    fn size(&self) -> (u32, u32) {
        (self.0.width, self.0.height)
    }
    fn rgba(&self) -> &[u8] {
        self.0.rgba()
    }
    fn bc7_levels(&self) -> &[Vec<u8>] {
        match self.0.cooked() {
            Some(cooked) if cooked.format == axcenity_assets::CookedFormat::Bc7 => &cooked.levels,
            _ => &[],
        }
    }
}

/// A model's normal map cooked as BC5. Anywhere else the same image is an
/// [`ImageTexture`], decoded to RGBA: BC5 has no blue or alpha.
pub(super) struct NormalTexture(Arc<Image>);

/// Mixed into a [`NormalTexture`]'s key, so the renderer keeps its BC5
/// copy apart from the image's RGBA one.
pub(super) const NORMAL_KEY: u64 = 0x6e6f_726d_616c_0b05;

impl TextureSource for NormalTexture {
    fn key(&self) -> u64 {
        self.0.key() ^ NORMAL_KEY
    }
    fn size(&self) -> (u32, u32) {
        (self.0.width, self.0.height)
    }
    fn rgba(&self) -> &[u8] {
        self.0.rgba()
    }
    fn bc5_levels(&self) -> &[Vec<u8>] {
        self.0.cooked().map_or(&[], |cooked| &cooked.levels)
    }
}
