//! Material files on parts: drawing a part's `MaterialVariant`, and making
//! a material file from a part's look (the editor's Save as Material).

use axcenity_assets::{
    Assets, Channel as FileChannel, MapFile, MapSettings, MaterialFile, MaterialMaps,
    Shading as FileShading, StandardMaterial,
};
use axcenity_core::{ClassName, Instance, InstanceId, TextureChannel, Value, World};
use axcenity_render::glam::Mat4;
use axcenity_render::{
    Channel, ChannelMap, Material, NormalMap, SURFACE_PARAMETERS, Shading, SurfaceShader, Sway,
    TextureMaps, Toon, Water,
};

use crate::draw_list::{GLOW_BOOST, texture};

/// The material file `instance`'s `MaterialVariant` names, put together
/// with its bases, or `None` when it names none or it can't be read (then
/// the part draws as if it had none).
pub(crate) fn variant_of(
    instance: &Instance,
    assets: &Assets,
) -> Option<std::sync::Arc<StandardMaterial>> {
    let path = instance
        .property("MaterialVariant")
        .and_then(Value::as_str)?;
    assets.material(path)
}

/// `standard` as the renderer takes it, with its maps' textures from
/// `assets`. A map whose texture isn't loaded (yet) is left out.
pub(crate) fn variant_material(standard: &StandardMaterial, assets: &Assets) -> Material {
    let image = |map: &Option<MapSettings>| {
        let map = map.as_ref()?;
        assets.texture(&map.path).map(|image| texture(&image))
    };
    let channel_map = |map: &Option<MapSettings>| {
        let settings = map.as_ref()?;
        Some(ChannelMap {
            texture: image(map)?,
            channel: match settings.channel {
                FileChannel::Red => Channel::Red,
                FileChannel::Green => Channel::Green,
                FileChannel::Blue => Channel::Blue,
                FileChannel::Alpha => Channel::Alpha,
            },
            invert: settings.invert,
        })
    };
    let maps = &standard.maps;
    Material {
        base_color: standard.color,
        base_color_texture: image(&maps.color),
        metalness: standard.metalness,
        roughness: standard.roughness,
        emissive: standard
            .emissive_color
            .map(|channel| channel * standard.emissive_strength.max(0.0)),
        opacity: standard.opacity.clamp(0.0, 1.0),
        alpha_cutoff: standard.alpha_cutoff,
        maps: TextureMaps {
            normal: maps.normal.as_ref().and_then(|settings| {
                Some(NormalMap {
                    texture: image(&maps.normal)?,
                    scale: settings.strength,
                    flip_green: settings.direct_x,
                })
            }),
            metalness: channel_map(&maps.metalness),
            roughness: channel_map(&maps.roughness),
            occlusion: channel_map(&maps.occlusion),
            occlusion_strength: maps.occlusion.as_ref().map_or(1.0, |map| map.strength),
            emissive: image(&maps.emissive),
            opacity: channel_map(&maps.opacity),
            specular: image(&maps.specular),
        },
        uv_scale: standard.tiling,
        uv_rotation: 0.0,
        uv_offset: standard.offset,
        double_sided: standard.double_sided,
        terrain: None,
        surface_shader: standard
            .shader
            .as_ref()
            .map(|shader| SurfaceShader::new(&shader.file, &shader.code)),
        surface_parameters: surface_parameters(standard),
        water: standard.water.map(|water| Water {
            deep_color: water.deep_color,
            clarity: water.clarity,
            wave_height: water.wave_height,
            wave_length: water.wave_length,
            wave_speed: water.wave_speed,
            wave_direction: water.wave_direction,
            foam: water.foam,
            refraction: water.refraction,
        }),
        shading: match standard.shading {
            FileShading::Standard => Shading::Standard,
            FileShading::Unlit => Shading::Unlit,
            FileShading::Toon => Shading::Toon(Toon {
                steps: standard.toon.steps,
                // A ramp not loaded (yet) leaves the bands.
                ramp: standard
                    .toon
                    .ramp
                    .as_ref()
                    .and_then(|ramp| assets.texture(ramp))
                    .map(|image| texture(&image)),
                softness: standard.toon.softness,
                rim: standard.toon.rim,
                highlight: standard.toon.highlight,
            }),
        },
    }
}

/// How a part drawn with `standard` at `transform` (its 1×1×1 box) sways
/// in the wind: by height over the bottom of its box, as much as the
/// material's `sway` says. `None` when it says 0.
pub(crate) fn variant_sway(standard: &StandardMaterial, transform: Mat4) -> Option<Sway> {
    // `filter` leaves out a NaN too.
    let strength = Some(standard.sway).filter(|&sway| sway > 0.0)?;
    // Half the box's height: how far up each of its axes reaches.
    let half_height =
        0.5 * (transform.x_axis.y.abs() + transform.y_axis.y.abs() + transform.z_axis.y.abs());
    Some(Sway {
        base: transform.w_axis.y - half_height,
        height: 2.0 * half_height,
        strength,
    })
}

/// `standard`'s parameters as its shader reads them, in their order; past
/// the renderer's [`SURFACE_PARAMETERS`] they're left out.
fn surface_parameters(standard: &StandardMaterial) -> [[f32; 4]; SURFACE_PARAMETERS] {
    let mut out = [[0.0; 4]; SURFACE_PARAMETERS];
    for (slot, (_, value)) in out.iter_mut().zip(&standard.parameters) {
        *slot = *value;
    }
    out
}

/// A material file with the look of the Part `id`, for Save as Material:
/// a Part with that file as its `MaterialVariant` and a white `Color`
/// draws the same (with bloom on, as a glowing Neon part's strength is
/// the bloom boost). `None` for anything but a Part.
///
/// A Part already using a material file gets an instance of it: a file
/// whose `base` is that one, overriding only the color when the part's
/// `Color` tints it.
pub fn part_material_file(world: &World, id: InstanceId, assets: &Assets) -> Option<MaterialFile> {
    let instance = world.get(id)?;
    if !matches!(
        instance.class(),
        ClassName::Part
            | ClassName::Seat
            | ClassName::VehicleSeat
            | ClassName::DamageZone
            | ClassName::ReverbZone
    ) {
        return None;
    }
    let color = instance.property("Color").and_then(Value::as_color3)?;
    let color = [color.r, color.g, color.b];
    let appearance = instance.children().iter().find_map(|&child| {
        let child = world.get(child)?;
        (child.class() == ClassName::SurfaceAppearance).then_some(child)
    });
    if appearance.is_none()
        && let Some(path) = instance
            .property("MaterialVariant")
            .and_then(Value::as_str)
            .filter(|path| !path.is_empty())
        && let Some(standard) = assets.material(path)
    {
        let tinted = color != [1.0; 3];
        return Some(MaterialFile {
            base: Some(path.to_owned()),
            color: tinted.then(|| std::array::from_fn(|i| standard.color[i] * color[i])),
            ..MaterialFile::default()
        });
    }

    let number = |name| {
        instance
            .property(name)
            .and_then(Value::as_number)
            .map(|n| n as f32)
    };
    let preset = instance
        .property("Material")
        .and_then(Value::as_material)?
        .preset();
    // A Neon part glows in its color, and Emission adds EmissiveColor.
    let mut glow = if preset.glows {
        color.map(|channel| channel * GLOW_BOOST)
    } else {
        [0.0; 3]
    };
    let emission = number("Emission").unwrap_or(0.0).max(0.0);
    if emission > 0.0
        && let Some(tint) = instance
            .property("EmissiveColor")
            .and_then(Value::as_color3)
    {
        for (channel, added) in glow.iter_mut().zip([tint.r, tint.g, tint.b]) {
            *channel += added * emission;
        }
    }
    let mut file = MaterialFile {
        color: Some(if preset.glows { [0.0; 3] } else { color }),
        metalness: number("Metalness"),
        roughness: number("Roughness"),
        opacity: Some(preset.opacity),
        alpha_cutoff: Some(0.0),
        tiling: Some([1.0, 1.0]),
        offset: Some([0.0, 0.0]),
        double_sided: Some(false),
        ..MaterialFile::default()
    };
    set_glow(&mut file, glow);
    if let Some(appearance) = appearance {
        add_appearance(&mut file, appearance);
    }
    Some(file)
}

/// Writes `glow` (sRGB, any brightness) as a color of at most 1 and a
/// strength.
fn set_glow(file: &mut MaterialFile, glow: [f32; 3]) {
    let strength = glow.into_iter().fold(0.0, f32::max);
    file.emissive_color = Some(if strength > 0.0 {
        glow.map(|channel| channel / strength)
    } else {
        [1.0; 3]
    });
    file.emissive_strength = Some(strength);
}

/// Adds a Part's SurfaceAppearance to `file`, as the part draws it: its
/// maps, its tint, and a metalness or roughness map giving the value alone.
fn add_appearance(file: &mut MaterialFile, appearance: &Instance) {
    let text = |name| {
        appearance
            .property(name)
            .and_then(Value::as_str)
            .filter(|path| !path.is_empty())
            .map(str::to_owned)
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
        Some(TextureChannel::Green) => FileChannel::Green,
        Some(TextureChannel::Blue) => FileChannel::Blue,
        Some(TextureChannel::Alpha) => FileChannel::Alpha,
        Some(TextureChannel::Red) | None => FileChannel::Red,
    };
    let channel_map = |map, channel_property, invert| {
        text(map).map(|path| MapFile {
            path: Some(path),
            channel: Some(channel(channel_property)),
            invert: Some(invert),
            ..MapFile::default()
        })
    };
    if let (Some(tint), Some(color)) = (
        appearance.property("Color").and_then(Value::as_color3),
        file.color.as_mut(),
    ) {
        *color = [color[0] * tint.r, color[1] * tint.g, color[2] * tint.b];
    }
    let mut maps = MaterialMaps {
        color: text("ColorMap").map(MapFile::of),
        normal: text("NormalMap").map(|path| MapFile {
            path: Some(path),
            strength: Some(number("NormalStrength", 1.0)),
            direct_x: Some(flag("NormalMapDirectX")),
            ..MapFile::default()
        }),
        metalness: channel_map("MetalnessMap", "MetalnessChannel", false),
        roughness: channel_map("RoughnessMap", "RoughnessChannel", flag("InvertRoughness")),
        occlusion: channel_map("AmbientOcclusionMap", "AmbientOcclusionChannel", false),
        emissive: text("EmissiveMap").map(MapFile::of),
        opacity: channel_map("OpacityMap", "OpacityChannel", flag("InvertOpacity")),
        specular: text("SpecularMap").map(MapFile::of),
    };
    if let Some(occlusion) = &mut maps.occlusion {
        occlusion.strength = Some(number("AmbientOcclusionStrength", 1.0));
    }
    if maps.metalness.is_some() {
        file.metalness = Some(1.0);
    }
    if maps.roughness.is_some() {
        file.roughness = Some(1.0);
    }
    if maps.emissive.is_some() {
        let strength = number("EmissiveStrength", 1.0).max(0.0);
        let glow = file
            .emissive_color
            .unwrap_or([1.0; 3])
            .map(|channel| channel * file.emissive_strength.unwrap_or(0.0));
        if glow.iter().all(|&channel| channel <= 0.0) {
            set_glow(file, [strength; 3]);
        } else {
            set_glow(file, glow.map(|channel| channel * strength));
        }
    }
    if maps.opacity.is_some() && file.opacity.unwrap_or(1.0) >= 1.0 {
        file.alpha_cutoff = Some(0.5);
    }
    let vector = |name, default| {
        appearance
            .property(name)
            .and_then(Value::as_vector2)
            .map_or(default, |v| [v.x, v.y])
    };
    file.tiling = Some(vector("Tiling", [1.0, 1.0]));
    file.offset = Some(vector("Offset", [0.0, 0.0]));
    file.maps = maps;
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use axcenity_assets::{MATERIALS_FOLDER, write_material_file};
    use axcenity_core::{Color3, Material as PartMaterial, Vector2};
    use axcenity_render::glam::Vec3;
    use axcenity_render::{Camera, DrawItem};

    use super::*;
    use crate::build_draw_list;

    const BASE: &str = "assets/materials/brick.material.json";

    /// A new project folder with `assets/textures/swatch.png`, a 1×1 image.
    fn project(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "axcenity-scene-material-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(folder.join("assets/textures")).unwrap();
        std::fs::create_dir_all(folder.join(MATERIALS_FOLDER)).unwrap();
        let image = image::RgbaImage::from_pixel(1, 1, image::Rgba([200, 40, 40, 255]));
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        std::fs::write(folder.join("assets/textures/swatch.png"), png.into_inner()).unwrap();
        folder
    }

    fn write(folder: &Path, path: &str, material: &MaterialFile) {
        write_material_file(&folder.join(path), material).unwrap();
    }

    fn part(world: &mut World, properties: Vec<(&str, Value)>) -> InstanceId {
        let part = world.create(ClassName::Part).unwrap();
        world.set_parent(part, Some(world.workspace())).unwrap();
        for (name, value) in properties {
            world.set_property(part, name, value).unwrap();
        }
        part
    }

    fn variant(path: &str) -> (&'static str, Value) {
        ("MaterialVariant", Value::String(path.to_owned()))
    }

    fn items(world: &World, assets: &Assets) -> Vec<DrawItem> {
        let camera = Camera {
            position: Vec3::Z,
            look_at: Vec3::ZERO,
            fov_y_degrees: 50.0,
        };
        build_draw_list(world, assets, camera).items
    }

    fn brick() -> MaterialFile {
        MaterialFile {
            color: Some([0.5, 0.5, 0.5]),
            metalness: Some(1.0),
            roughness: Some(0.3),
            maps: MaterialMaps {
                color: Some(MapFile::of("assets/textures/swatch.png")),
                ..MaterialMaps::default()
            },
            tiling: Some([2.0, 2.0]),
            ..MaterialFile::default()
        }
    }

    #[test]
    fn two_instances_of_one_base_draw_their_own_overrides_and_follow_the_base() {
        let folder = project("instances");
        write(&folder, BASE, &brick());
        let instance = |overrides: MaterialFile| MaterialFile {
            base: Some(BASE.into()),
            ..overrides
        };
        write(
            &folder,
            "assets/materials/worn.material.json",
            &instance(MaterialFile {
                roughness: Some(0.9),
                ..MaterialFile::default()
            }),
        );
        write(
            &folder,
            "assets/materials/red.material.json",
            &instance(MaterialFile {
                color: Some([1.0, 0.0, 0.0]),
                ..MaterialFile::default()
            }),
        );
        let assets = Assets::new(&folder);
        let mut world = World::new();
        let white = Value::Color3(Color3::new(1.0, 1.0, 1.0));
        part(
            &mut world,
            vec![
                ("Color", white.clone()),
                variant("assets/materials/worn.material.json"),
            ],
        );
        // Its Color tints the material, and Transparency still fades it.
        part(
            &mut world,
            vec![
                ("Color", Value::Color3(Color3::new(0.5, 1.0, 1.0))),
                ("Transparency", Value::Number(0.5)),
                variant("assets/materials/red.material.json"),
            ],
        );

        let drawn = items(&world, &assets);
        let (worn, red) = (&drawn[0].material, &drawn[1].material);
        assert_eq!(worn.base_color, [0.5, 0.5, 0.5]);
        assert_eq!((worn.metalness, worn.roughness), (1.0, 0.9));
        assert_eq!(worn.uv_scale, [2.0, 2.0]);
        assert!(worn.base_color_texture.is_some());
        assert_eq!(drawn[0].uv_tile, 8.0);
        assert_eq!(red.base_color, [0.5, 0.0, 0.0]);
        assert_eq!((red.metalness, red.roughness), (1.0, 0.3));
        assert_eq!(red.opacity, 0.5);
        assert!(assets.take_errors().is_empty());

        // The base file changes: both follow it, each keeping its override.
        write(
            &folder,
            BASE,
            &MaterialFile {
                metalness: Some(0.0),
                roughness: Some(0.1),
                ..brick()
            },
        );
        assets.forget(BASE);
        let drawn = items(&world, &assets);
        let (worn, red) = (&drawn[0].material, &drawn[1].material);
        assert_eq!((worn.metalness, worn.roughness), (0.0, 0.9));
        assert_eq!((red.metalness, red.roughness), (0.0, 0.1));
        assert_eq!(red.base_color, [0.5, 0.0, 0.0]);
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn a_material_with_sway_sways_its_parts_by_their_height_and_wind_reaches_the_draw_list() {
        let folder = project("sway");
        let reeds = "assets/materials/reeds.material.json";
        write(
            &folder,
            reeds,
            &MaterialFile {
                sway: Some(0.7),
                ..MaterialFile::default()
            },
        );
        write(&folder, BASE, &brick());
        let assets = Assets::new(&folder);
        let mut world = World::new();
        let size = Value::Vector3(axcenity_core::Vector3::new(1.0, 6.0, 1.0));
        let position = Value::Vector3(axcenity_core::Vector3::new(4.0, 5.0, 0.0));
        part(
            &mut world,
            vec![
                variant(reeds),
                ("Size", size.clone()),
                ("Position", position),
            ],
        );
        part(&mut world, vec![variant(BASE), ("Size", size)]);
        let wind = axcenity_core::Vector3::new(12.0, 0.0, -3.0);
        let workspace = world.workspace();
        world
            .set_property(workspace, "GlobalWind", Value::Vector3(wind))
            .unwrap();

        let camera = Camera {
            position: Vec3::Z,
            look_at: Vec3::ZERO,
            fov_y_degrees: 50.0,
        };
        let list = build_draw_list(&world, &assets, camera);
        assert_eq!(list.wind, Vec3::new(12.0, 0.0, -3.0));
        // Rooted at the bottom of its box, 6 studs tall.
        let sway = list.items[0].sway.expect("the reeds sway");
        assert!((sway.base - 2.0).abs() < 1e-4, "{sway:?}");
        assert!((sway.height - 6.0).abs() < 1e-4, "{sway:?}");
        assert_eq!(sway.strength, 0.7);
        assert_eq!(list.items[1].sway, None, "brick doesn't sway");
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn a_water_material_draws_its_part_as_water_on_the_wave_grid() {
        let folder = project("water");
        let pool = "assets/materials/pool.material.json";
        write(
            &folder,
            pool,
            &MaterialFile {
                water: Some(axcenity_assets::WaterFile {
                    wave_height: Some(0.4),
                    ..Default::default()
                }),
                ..MaterialFile::default()
            },
        );
        write(&folder, BASE, &brick());
        let assets = Assets::new(&folder);
        let mut world = World::new();
        part(&mut world, vec![variant(pool)]);
        part(&mut world, vec![variant(BASE)]);
        let camera = Camera {
            position: Vec3::Z,
            look_at: Vec3::ZERO,
            fov_y_degrees: 50.0,
        };
        let list = build_draw_list(&world, &assets, camera);
        let water = list.items[0].material.water.expect("the pool is water");
        assert_eq!(water.wave_height, 0.4);
        assert_eq!(
            water.clarity,
            axcenity_assets::StandardWater::default().clarity
        );
        let axcenity_render::Shape::Mesh(mesh) = &list.items[0].shape else {
            panic!("water is drawn on the wave grid");
        };
        assert_eq!(mesh.key(), axcenity_render::water_surface().key());
        assert_eq!(list.items[1].material.water, None, "brick isn't water");
        assert!(matches!(list.items[1].shape, axcenity_render::Shape::Block));
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn a_materials_shading_reaches_the_draw_list_and_files_without_one_stay_standard() {
        let folder = project("shading");
        let cel = "assets/materials/cel.material.json";
        let flat = "assets/materials/flat.material.json";
        write(
            &folder,
            cel,
            &MaterialFile {
                shading: Some(FileShading::Toon),
                toon: Some(axcenity_assets::ToonFile {
                    steps: Some(4),
                    ramp: Some("assets/textures/swatch.png".into()),
                    rim: Some(0.6),
                    ..Default::default()
                }),
                ..MaterialFile::default()
            },
        );
        write(
            &folder,
            flat,
            &MaterialFile {
                shading: Some(FileShading::Unlit),
                ..MaterialFile::default()
            },
        );
        write(&folder, BASE, &brick());
        let assets = Assets::new(&folder);
        let mut world = World::new();
        part(&mut world, vec![variant(cel)]);
        part(&mut world, vec![variant(flat)]);
        part(&mut world, vec![variant(BASE)]);
        part(&mut world, vec![]);
        let items = items(&world, &assets);
        let Shading::Toon(toon) = &items[0].material.shading else {
            panic!("cel is toon: {:?}", items[0].material.shading);
        };
        assert_eq!((toon.steps, toon.rim), (4, 0.6));
        assert!(toon.ramp.is_some(), "with its ramp");
        assert!(matches!(items[1].material.shading, Shading::Unlit));
        assert!(
            matches!(items[2].material.shading, Shading::Standard),
            "a material file without shading is standard"
        );
        assert!(matches!(items[3].material.shading, Shading::Standard));
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn a_surface_appearance_wins_and_a_missing_file_leaves_the_parts_own_look() {
        let folder = project("wins");
        write(&folder, BASE, &brick());
        let assets = Assets::new(&folder);
        let mut world = World::new();
        let dressed = part(&mut world, vec![variant(BASE)]);
        let look = world.create(ClassName::SurfaceAppearance).unwrap();
        world.set_parent(look, Some(dressed)).unwrap();
        part(
            &mut world,
            vec![variant("assets/materials/gone.material.json")],
        );
        let mut plain = World::new();
        part(&mut plain, vec![]);
        let plain = items(&plain, &assets)[0].material.clone();
        for _ in 0..2 {
            for item in items(&world, &assets) {
                assert_eq!(item.material.base_color, plain.base_color);
                assert_eq!(item.material.metalness, plain.metalness);
                assert_eq!(item.material.roughness, plain.roughness);
                assert_eq!(item.material.uv_scale, [1.0, 1.0]);
            }
        }
        let errors = assets.take_errors();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("gone.material.json"), "{errors:?}");
        let _ = std::fs::remove_dir_all(folder);
    }

    /// The parts of `a` and `b` that make a look, side by side.
    fn assert_same_look(a: &DrawItem, b: &DrawItem, what: &str) {
        let close = |x: &[f32], y: &[f32]| x.iter().zip(y).all(|(x, y)| (x - y).abs() < 1e-5);
        let (a_uv, b_uv) = (a.uv_tile, b.uv_tile);
        let (a, b) = (&a.material, &b.material);
        assert!(
            close(&a.base_color, &b.base_color),
            "{what}: {:?} {:?}",
            a.base_color,
            b.base_color
        );
        assert!(
            close(&a.emissive, &b.emissive),
            "{what}: {:?} {:?}",
            a.emissive,
            b.emissive
        );
        assert_eq!(
            [a.metalness, a.roughness, a.opacity, a.alpha_cutoff],
            [b.metalness, b.roughness, b.opacity, b.alpha_cutoff],
            "{what}"
        );
        assert_eq!(
            (a.uv_scale, a.uv_offset, a_uv),
            (b.uv_scale, b.uv_offset, b_uv),
            "{what}"
        );
        assert_eq!(a.double_sided, b.double_sided, "{what}");
        let key = |texture: &Option<std::sync::Arc<dyn axcenity_render::TextureSource>>| {
            texture.as_ref().map(|texture| texture.key())
        };
        assert_eq!(
            key(&a.base_color_texture),
            key(&b.base_color_texture),
            "{what}"
        );
        let normal = |m: &Material| {
            m.maps
                .normal
                .as_ref()
                .map(|n| (n.texture.key(), n.scale, n.flip_green))
        };
        assert_eq!(normal(a), normal(b), "{what}");
        let roughness = |m: &Material| {
            m.maps
                .roughness
                .as_ref()
                .map(|r| (r.texture.key(), r.channel, r.invert))
        };
        assert_eq!(roughness(a), roughness(b), "{what}");
    }

    #[test]
    fn save_as_material_keeps_a_parts_look_on_a_white_part() {
        let folder = project("save");
        let assets = Assets::new(&folder);
        let swatch = Value::String("assets/textures/swatch.png".into());
        let mut world = World::new();
        let metal = part(
            &mut world,
            vec![
                ("Material", Value::Material(PartMaterial::Metal)),
                ("Color", Value::Color3(Color3::new(0.2, 0.6, 0.9))),
            ],
        );
        let look = world.create(ClassName::SurfaceAppearance).unwrap();
        world.set_parent(look, Some(metal)).unwrap();
        for (name, value) in [
            ("ColorMap", swatch.clone()),
            ("NormalMap", swatch.clone()),
            ("NormalStrength", Value::Number(0.5)),
            ("RoughnessMap", swatch.clone()),
            (
                "RoughnessChannel",
                Value::TextureChannel(TextureChannel::Green),
            ),
            ("InvertRoughness", Value::Bool(true)),
            ("Color", Value::Color3(Color3::new(1.0, 0.5, 0.5))),
            ("Tiling", Value::Vector2(Vector2::new(2.0, 1.0))),
            ("Offset", Value::Vector2(Vector2::new(0.25, 0.0))),
        ] {
            world.set_property(look, name, value).unwrap();
        }
        let neon = part(
            &mut world,
            vec![
                ("Material", Value::Material(PartMaterial::Neon)),
                ("Color", Value::Color3(Color3::new(0.2, 0.9, 1.0))),
                ("Emission", Value::Number(0.5)),
            ],
        );
        let glass = part(
            &mut world,
            vec![("Material", Value::Material(PartMaterial::Glass))],
        );
        let originals = items(&world, &assets);
        assert!(part_material_file(&world, look, &assets).is_none());

        let mut copies = World::new();
        for (index, id) in [metal, neon, glass].into_iter().enumerate() {
            let file = part_material_file(&world, id, &assets).unwrap();
            let path = format!("assets/materials/saved{index}.material.json");
            write(&folder, &path, &file);
            part(
                &mut copies,
                vec![
                    ("Color", Value::Color3(Color3::new(1.0, 1.0, 1.0))),
                    variant(&path),
                ],
            );
        }
        let saved = items(&copies, &assets);
        for (index, what) in ["metal", "neon", "glass"].into_iter().enumerate() {
            assert_same_look(&originals[index], &saved[index], what);
        }

        // A part already using a file saves an instance of it, which
        // keeps its tint.
        let mut tinted = World::new();
        let red = part(
            &mut tinted,
            vec![
                ("Color", Value::Color3(Color3::new(1.0, 0.0, 0.0))),
                variant("assets/materials/saved0.material.json"),
            ],
        );
        let file = part_material_file(&tinted, red, &assets).unwrap();
        assert_eq!(
            file.base.as_deref(),
            Some("assets/materials/saved0.material.json")
        );
        assert_eq!(file.metalness, None);
        write(&folder, "assets/materials/red.material.json", &file);
        let before = items(&tinted, &assets);
        tinted
            .set_property(red, "Color", Value::Color3(Color3::new(1.0, 1.0, 1.0)))
            .unwrap();
        tinted
            .set_property(
                red,
                "MaterialVariant",
                Value::String("assets/materials/red.material.json".into()),
            )
            .unwrap();
        assert_same_look(&before[0], &items(&tinted, &assets)[0], "instance");
        assert!(assets.take_errors().is_empty());
        let _ = std::fs::remove_dir_all(folder);
    }
}
