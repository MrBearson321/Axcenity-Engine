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

#[test]
fn a_normal_map_cooked_as_bc5_is_handed_over_as_bc5_only_as_a_normal_map() {
    let folder = std::env::temp_dir().join(format!("axcenity-scene-bc5-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(folder.join("assets")).unwrap();
    let path = "assets/Stone_Normal.png";
    image::RgbaImage::from_pixel(4, 4, image::Rgba([128, 128, 255, 255]))
        .save(folder.join(path))
        .unwrap();
    let bytes = std::fs::read(folder.join(path)).unwrap();
    let cooked = axcenity_assets::CookedTexture {
        format: axcenity_assets::CookedFormat::Bc5,
        width: 4,
        height: 4,
        levels: vec![vec![128; 16]; 3],
        has_alpha: false,
        average: [128, 128, 255, 255],
    };
    let stamp = axcenity_assets::SourceStamp::of(&folder.join(path), &bytes);
    axcenity_assets::write_cooked(&folder, path, stamp, &cooked).unwrap();
    let image = Assets::new(&folder).texture_blocking(path).unwrap();
    assert!(image.cooked().is_some());

    let normal = normal_texture(&image);
    assert_eq!(normal.bc5_levels().len(), 3);
    assert!(normal.bc7_levels().is_empty());
    // Anywhere else, the same image is decoded: BC5 has no blue.
    let color = texture(&image);
    assert!(color.bc5_levels().is_empty() && color.bc7_levels().is_empty());
    assert_ne!(normal.key(), color.key(), "kept apart on the GPU");
    assert_eq!(color.rgba(), [128, 128, 255, 255].repeat(16));
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn ambient_occlusion_intensity_sets_the_ambient_lights_occlusion_within_0_to_2() {
    let mut world = World::new();
    let occlusion = |world: &World| {
        build_draw_list(world, &Assets::default(), camera())
            .ambient
            .occlusion
    };
    assert_eq!(occlusion(&world), 1.0);
    let lighting = world.lighting();
    for (set, drawn) in [(0.5, 0.5), (2.0, 2.0), (5.0, 2.0), (-1.0, 0.0)] {
        world
            .set_property(lighting, "AmbientOcclusionIntensity", Value::Number(set))
            .unwrap();
        assert_eq!(occlusion(&world), drawn);
    }
}

#[test]
fn lightings_sky_image_is_handed_to_the_renderer_read_once() {
    let folder = std::env::temp_dir().join(format!("axcenity-scene-sky-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(folder.join("assets/sky")).unwrap();
    let pixels = vec![image::Rgb([3.0f32, 2.0, 1.0]); 8 * 4];
    let mut bytes = Vec::new();
    image::codecs::hdr::HdrEncoder::new(&mut bytes)
        .encode(&pixels, 8, 4)
        .unwrap();
    std::fs::write(folder.join("assets/sky/noon.hdr"), bytes).unwrap();
    let assets = Assets::new(&folder);
    let mut world = World::new();
    let sky = |world: &World| build_draw_list(world, &assets, camera()).sky;
    assert_eq!(sky(&world).image, None, "no image by default");

    let lighting = world.lighting();
    world
        .set_property(
            lighting,
            "SkyImage",
            Value::String("assets/sky/noon.hdr".into()),
        )
        .unwrap();
    let image = sky(&world).image.expect("the sky image is drawn");
    assert_eq!(
        (image.width, image.height, image.rgb.len()),
        (8, 4, 8 * 4 * 3)
    );
    assert_eq!(sky(&world).image.unwrap().key, image.key, "read once");

    world
        .set_property(
            lighting,
            "SkyImage",
            Value::String("assets/sky/gone.hdr".into()),
        )
        .unwrap();
    assert_eq!(
        sky(&world).image,
        None,
        "a missing image falls back to the colors"
    );
    assert_eq!(assets.take_errors().len(), 1);
    let _ = std::fs::remove_dir_all(&folder);
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
    // A capsule is a cylinder and a ball at each end.
    assert_eq!(
        shapes,
        [
            Shape::Block,
            Shape::Sphere,
            Shape::Cylinder,
            Shape::Wedge,
            Shape::Cylinder,
            Shape::Sphere,
            Shape::Sphere,
            Shape::CornerWedge
        ]
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
    Assets::new(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-mesh"))
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

#[test]
fn a_reflection_probe_reflects_in_its_parts_box_and_a_missing_bake_falls_back_to_the_sky() {
    let folder = std::env::temp_dir().join(format!("axcenity-scene-probe-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    let path = axcenity_assets::probe_path("Hall");
    axcenity_assets::write_probe(&folder, &path, 2, &[0.5; 2 * 2 * 6 * 3]).unwrap();
    let assets = Assets::new(&folder);
    let mut world = World::new();
    let room = part(
        &mut world,
        vec![
            ("Size", vector(12.0, 6.0, 8.0)),
            ("Position", vector(1.0, 3.0, -2.0)),
            ("Orientation", vector(0.0, 90.0, 0.0)),
        ],
    );
    let probe = world.create(ClassName::ReflectionProbe).unwrap();
    world.set_parent(probe, Some(room)).unwrap();
    let probes = |world: &World| build_draw_list(world, &assets, camera()).reflection_probes;
    assert!(probes(&world).is_empty(), "not baked yet: the sky");

    world
        .set_property(probe, "Bake", Value::String(path.clone()))
        .unwrap();
    let drawn = probes(&world);
    assert_eq!(drawn.len(), 1);
    assert_eq!(drawn[0].center, Vec3::new(1.0, 3.0, -2.0));
    assert!(
        drawn[0]
            .half_size
            .abs_diff_eq(Vec3::new(6.0, 3.0, 4.0), 1e-5)
    );
    assert!((drawn[0].rotation * Vec3::X).abs_diff_eq(Vec3::NEG_Z, 1e-5));
    assert_eq!(drawn[0].blend_distance, 2.0);
    assert_eq!(drawn[0].image.size, 2);
    assert_eq!(probes(&world)[0].image.key, drawn[0].image.key, "read once");
    assert!(assets.take_errors().is_empty());

    world
        .set_property(
            probe,
            "Bake",
            Value::String("assets/probes/gone.probe.hdr".into()),
        )
        .unwrap();
    assert!(probes(&world).is_empty(), "a missing bake reflects the sky");
    assert!(probes(&world).is_empty());
    let errors = assets.take_errors();
    assert_eq!(errors.len(), 1, "reported once: {errors:?}");
    assert!(errors[0].contains("gone.probe.hdr"), "{errors:?}");
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_light_with_no_part_shines_from_its_own_position_and_turn() {
    let mut world = World::new();
    let light = world.create(ClassName::SpotLight).unwrap();
    world.set_parent(light, Some(world.workspace())).unwrap();
    world
        .set_property(light, "Position", vector(5.0, 8.0, -3.0))
        .unwrap();
    // Turned a quarter about X: its Direction (down) points along -Z.
    world
        .set_property(light, "Orientation", vector(90.0, 0.0, 0.0))
        .unwrap();
    let list = build_draw_list(&world, &Assets::default(), camera());
    assert_eq!(list.lights[0].position, Vec3::new(5.0, 8.0, -3.0));
    let direction = list.lights[0].cone.unwrap().direction;
    assert!(direction.abs_diff_eq(Vec3::NEG_Z, 1e-5), "{direction}");
    let markers = light_markers(&world);
    assert_eq!(markers[0].part, None, "it stands free, so it has an icon");
    assert_eq!(markers[0].kind, MarkerKind::Spot);

    // Put in a part, it shines from the part instead.
    let lamp = part(&mut world, vec![("Position", vector(0.0, 1.0, 0.0))]);
    world.set_parent(light, Some(lamp)).unwrap();
    let list = build_draw_list(&world, &Assets::default(), camera());
    assert!((list.lights[0].position - Vec3::new(0.0, 1.55, 0.0)).length() < 1e-4);
    assert_eq!(light_markers(&world)[0].part, Some(lamp));
}

#[test]
fn a_rect_light_in_a_part_sits_on_the_face_it_looks_out_of() {
    let mut world = World::new();
    let panel = part(
        &mut world,
        vec![
            ("Size", vector(4.0, 1.0, 2.0)),
            ("Position", vector(0.0, 10.0, 0.0)),
        ],
    );
    let light = world.create(ClassName::RectLight).unwrap();
    world.set_parent(light, Some(panel)).unwrap();
    let list = build_draw_list(&world, &Assets::default(), camera());
    let lit = list.lights[0];
    // Down out of its bottom, a hair below it, and as big as it says.
    assert!(
        (lit.position - Vec3::new(0.0, 9.45, 0.0)).length() < 1e-4,
        "{}",
        lit.position
    );
    let area = lit.rect.unwrap();
    assert_eq!((area.direction, area.right), (Vec3::NEG_Y, Vec3::X));
    assert_eq!((area.width, area.height), (4.0, 4.0));
    assert!(lit.cone.is_none());
    // Facing along X, its width runs along Z.
    world
        .set_property(light, "Direction", vector(1.0, 0.0, 0.0))
        .unwrap();
    let list = build_draw_list(&world, &Assets::default(), camera());
    assert!((list.lights[0].position - Vec3::new(2.05, 10.0, 0.0)).length() < 1e-4);
    assert_eq!(list.lights[0].rect.unwrap().right, Vec3::Z);
}

#[test]
fn a_spot_lights_shadows_start_past_its_own_part() {
    let mut world = World::new();
    let lamp = part(&mut world, vec![("Size", vector(1.0, 2.0, 1.0))]);
    let light = world.create(ClassName::SpotLight).unwrap();
    world.set_parent(light, Some(lamp)).unwrap();
    let cone = |world: &World| {
        let list = build_draw_list(world, &Assets::default(), camera());
        list.lights[0].cone.unwrap()
    };
    assert_eq!(cone(&world).shadows, None, "off by default");

    world
        .set_property(light, "Shadows", Value::Bool(true))
        .unwrap();
    // The light sits 0.05 above the 2-stud lamp and points down through
    // it, so shadows start past the lamp's bottom.
    let near = cone(&world).shadows.unwrap().near;
    assert!((near - 2.1).abs() < 1e-4, "{near}");

    world
        .set_property(light, "Direction", vector(0.0, 1.0, 0.0))
        .unwrap();
    assert_eq!(cone(&world).shadows.unwrap().near, 0.0, "pointing away");
}

#[test]
fn a_point_lights_shadows_start_past_its_own_part_on_each_side() {
    let mut world = World::new();
    let lamp = part(&mut world, vec![("Size", vector(1.0, 2.0, 1.0))]);
    let light = world.create(ClassName::PointLight).unwrap();
    world.set_parent(light, Some(lamp)).unwrap();
    let shadows = |world: &World| {
        let list = build_draw_list(world, &Assets::default(), camera());
        list.lights[0].point_shadows
    };
    assert_eq!(shadows(&world), None, "off by default");

    world
        .set_property(light, "Shadows", Value::Bool(true))
        .unwrap();
    // The light sits 0.05 above the 2-stud lamp: below it the lamp
    // reaches 2.05 studs, to each side half its width, and above it
    // not at all.
    let near = shadows(&world).unwrap().near;
    let expected = [0.55, 0.55, 0.0, 2.1, 0.55, 0.55];
    for (near, expected) in near.iter().zip(expected) {
        assert!((near - expected).abs() < 1e-4, "{near:?}");
    }
}

fn examples() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

#[test]
fn a_mesh_part_hands_over_its_models_simpler_copies_with_an_id_of_its_own() {
    let mut world = World::new();
    let helmet = "assets/models/DamagedHelmet/DamagedHelmet.glb";
    let (a, b) = (mesh_part(&mut world, helmet), mesh_part(&mut world, helmet));
    let assets = Assets::new(examples().join("hello-pbr"));
    let list = build_draw_list(&world, &assets, camera());
    assert_eq!(list.items.len(), 2);
    let [first, second] = [&list.items[0], &list.items[1]];
    assert_ne!(first.id, 0);
    assert_ne!(first.id, second.id, "each part keeps its own level");
    let ids: Vec<u64> = list.items.iter().map(|item| item.id).collect();
    assert!(
        ids.contains(&draw_id(a)) && ids.contains(&draw_id(b)),
        "{ids:?}"
    );
    let Shape::Mesh(mesh) = &first.shape else {
        panic!("a mesh");
    };
    assert_eq!(mesh.lod_count(), 2);
    let triangles = |mesh: &dyn MeshSource| mesh.indices().len() / 3;
    let full = triangles(mesh.as_ref());
    let half = mesh.lod(1).unwrap();
    let quarter = mesh.lod(2).unwrap();
    assert!(triangles(half.as_ref()) <= full / 2, "{full}");
    assert!(triangles(quarter.as_ref()) <= full / 4, "{full}");
    assert_ne!(half.key(), mesh.key());
    assert_eq!(half.lod_count(), 0);
    assert!(mesh.lod(3).is_none() && half.lod(1).is_none());
}

fn csg_assets() -> Assets {
    Assets::new(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-csg"))
}

#[test]
fn a_negated_part_is_drawn_pink_and_see_through() {
    let mut world = World::new();
    let part = world.create(ClassName::NegateOperation).unwrap();
    world.set_parent(part, Some(world.workspace())).unwrap();
    let item = only_item_with(&world, &Assets::default());
    assert_eq!(item.shape, Shape::Block);
    assert_eq!(item.material.opacity, NEGATE_OPACITY);
    let [r, g, b] = item.material.base_color;
    assert!(r > g && b > g, "pink: {:?}", item.material.base_color);
}

#[test]
fn a_union_draws_its_model_and_not_the_parts_inside_it() {
    let mut world = World::new();
    let union = mesh_part(&mut world, "assets/models/Doorway.union.glb");
    // Swapped for a UnionOperation with the same properties.
    let doorway = world.create(ClassName::UnionOperation).unwrap();
    for (name, value) in world
        .get(union)
        .unwrap()
        .properties()
        .map(|(n, v)| (n, v.clone()))
        .collect::<Vec<_>>()
    {
        if world.get(doorway).unwrap().property(name).is_some() {
            world.set_property(doorway, name, value).unwrap();
        }
    }
    world.set_parent(union, None).unwrap();
    world.set_parent(doorway, Some(world.workspace())).unwrap();
    let wall = world.create(ClassName::Part).unwrap();
    world.set_parent(wall, Some(doorway)).unwrap();

    let assets = csg_assets();
    let list = build_draw_list(&world, &assets, camera());
    // One item per look: the wall's and the doorway's sides.
    assert_eq!(list.items.len(), 2);
    assert!(
        list.items
            .iter()
            .all(|item| matches!(item.shape, Shape::Mesh(_)))
    );
    let colors: Vec<[f32; 3]> = list.items.iter().map(|i| i.material.base_color).collect();
    assert_ne!(colors[0], colors[1]);

    // UsePartColor paints it all in its Color.
    world
        .set_property(doorway, "UsePartColor", Value::Bool(true))
        .unwrap();
    world
        .set_property(
            doorway,
            "Color",
            Value::Color3(axcenity_core::Color3::new(0.0, 1.0, 0.0)),
        )
        .unwrap();
    let list = build_draw_list(&world, &assets, camera());
    for item in &list.items {
        assert_eq!(item.material.base_color, [0.0, 1.0, 0.0]);
    }
}
