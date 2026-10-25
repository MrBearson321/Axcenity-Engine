use super::*;

/// The example project's model: a square pyramid 4 wide and 3 tall,
/// with a checkered texture, all embedded in one `.gltf` file.
fn example_folder() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-mesh/assets/models")
}

fn example_bytes() -> Vec<u8> {
    std::fs::read(example_folder().join("pyramid.gltf")).unwrap()
}

#[test]
fn the_example_pyramid_loads_fitted_into_the_unit_cube() {
    let model = load_model(&example_bytes(), &example_folder(), ModelFormat::Gltf).unwrap();
    assert_eq!(model.size, [4.0, 3.0, 4.0]);
    assert_eq!(model.primitives.len(), 1);
    let primitive = &model.primitives[0];
    assert_eq!(primitive.normals.len(), primitive.positions.len());
    assert_eq!(primitive.uvs.len(), primitive.positions.len());
    for corner in &primitive.positions {
        assert!(corner.iter().all(|c| c.abs() <= 0.5 + 1e-6), "{corner:?}");
    }
    // The apex and the 4 base corners.
    assert_eq!(model.hull_points.len(), 5);
    let texture = primitive.material.base_color_texture.as_ref().unwrap();
    assert_eq!(
        texture.rgba.len(),
        texture.width as usize * texture.height as usize * 4
    );
}

#[test]
fn each_load_is_a_different_model() {
    let a = load_model(&example_bytes(), &example_folder(), ModelFormat::Gltf).unwrap();
    let b = load_model(&example_bytes(), &example_folder(), ModelFormat::Gltf).unwrap();
    assert_ne!(a, b);
    assert_ne!(a.primitives[0].key(), b.primitives[0].key());
}

fn error(text: &str) -> String {
    load_model(text.as_bytes(), &example_folder(), ModelFormat::Gltf)
        .unwrap_err()
        .to_string()
}

#[test]
fn broken_files_give_errors_not_crashes() {
    assert!(error("not a model").contains("isn't a valid glTF file"));
    assert!(error("{}").contains("isn't a valid glTF file"));
    // Valid JSON, but the mesh points at an accessor that isn't there.
    // `gltf` panics while checking it, which is caught.
    let bad_accessor = r#"{ "asset": { "version": "2.0" },
        "meshes": [ { "primitives": [ { "attributes": { "POSITION": 7 } } ] } ] }"#;
    let bad_accessor = error(bad_accessor);
    assert!(
        bad_accessor.contains("isn't a valid glTF file") || bad_accessor == "the file is damaged",
        "{bad_accessor}"
    );
    // A model with nothing to draw.
    assert_eq!(
        error(r#"{ "asset": { "version": "2.0" } }"#),
        "it has no triangles to draw"
    );
}

#[test]
fn files_outside_the_folder_or_missing_are_named() {
    let using = |uri: &str| {
        format!(
            r#"{{ "asset": {{ "version": "2.0" }},
                "buffers": [ {{ "uri": "{uri}", "byteLength": 36 }} ] }}"#
        )
    };
    assert!(error(&using("../../secret.bin")).contains("`../../secret.bin`, which isn't inside"));
    assert!(error(&using("C:/secret.bin")).contains("isn't inside"));
    assert!(error(&using("%2e%2e/secret.bin")).contains("isn't inside"));
    assert!(error(&using("gone%20away.bin")).contains("`gone%20away.bin`, which isn't there"));
}

#[test]
fn formats_come_from_the_extension() {
    assert_eq!(
        ModelFormat::of(Path::new("a/chair.GLB")),
        Some(ModelFormat::Gltf)
    );
    assert_eq!(
        ModelFormat::of(Path::new("tree.gltf")),
        Some(ModelFormat::Gltf)
    );
    assert_eq!(
        ModelFormat::of(Path::new("Robot.Fbx")),
        Some(ModelFormat::Fbx)
    );
    assert_eq!(ModelFormat::of(Path::new("notes.txt")), None);
    assert_eq!(ModelFormat::of(Path::new("fbx")), None);
}

#[test]
fn uris_are_percent_decoded() {
    assert_eq!(
        percent_decode("my%20model.bin").as_deref(),
        Some("my model.bin")
    );
    assert_eq!(percent_decode("bad%2"), None);
    assert_eq!(percent_decode("bad%zz"), None);
}

#[test]
fn a_gltf_materials_maps_are_all_read() {
    let folder = std::env::temp_dir().join("axcenity-gltf-maps");
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    // One triangle in the XY plane: positions, normals, then UVs.
    let floats: [f32; 24] = [
        0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, //
        0.0, 1.0, 1.0, 1.0, 0.0, 0.0,
    ];
    let bytes: Vec<u8> = floats.iter().flat_map(|f| f.to_le_bytes()).collect();
    std::fs::write(folder.join("tri.bin"), bytes).unwrap();
    for name in ["color", "orm", "normal", "glow"] {
        std::fs::write(folder.join(format!("{name}.png")), checker_png()).unwrap();
    }
    let gltf = r#"{
        "asset": { "version": "2.0" },
        "buffers": [ { "uri": "tri.bin", "byteLength": 96 } ],
        "bufferViews": [
            { "buffer": 0, "byteOffset": 0, "byteLength": 36 },
            { "buffer": 0, "byteOffset": 36, "byteLength": 36 },
            { "buffer": 0, "byteOffset": 72, "byteLength": 24 }
        ],
        "accessors": [
            { "bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3",
              "min": [0, 0, 0], "max": [1, 1, 0] },
            { "bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC3" },
            { "bufferView": 2, "componentType": 5126, "count": 3, "type": "VEC2" }
        ],
        "images": [ { "uri": "color.png" }, { "uri": "orm.png" },
                    { "uri": "normal.png" }, { "uri": "glow.png" } ],
        "textures": [ { "source": 0 }, { "source": 1 }, { "source": 2 }, { "source": 3 } ],
        "materials": [ {
            "name": "Rusty",
            "pbrMetallicRoughness": {
                "baseColorTexture": { "index": 0 },
                "metallicRoughnessTexture": { "index": 1 }
            },
            "normalTexture": { "index": 2, "scale": 0.5 },
            "occlusionTexture": { "index": 1, "strength": 0.7 },
            "emissiveTexture": { "index": 3, "texCoord": 1 }
        } ],
        "meshes": [ { "primitives": [ {
            "attributes": { "POSITION": 0, "NORMAL": 1, "TEXCOORD_0": 2 },
            "material": 0
        } ] } ]
    }"#;
    let model = load_model(gltf.as_bytes(), &folder, ModelFormat::Gltf).unwrap();
    let primitive = &model.primitives[0];
    let material = &primitive.material;
    assert_eq!(material.name, "Rusty");
    assert!(material.base_color_texture.is_some());
    assert_eq!(material.normal_scale, 0.5);
    // One "ORM" image: occlusion in red, roughness in green, metalness in blue.
    let orm = |map: &Option<ChannelTexture>| {
        let map = map.as_ref().unwrap();
        (map.channel, Arc::as_ptr(&map.image))
    };
    let (metal, image) = orm(&material.metalness_texture);
    assert_eq!(metal, Channel::Blue);
    assert_eq!(orm(&material.roughness_texture), (Channel::Green, image));
    assert_eq!(orm(&material.occlusion_texture), (Channel::Red, image));
    assert_eq!(material.occlusion_strength, 0.7);
    // Placed by another set of texture coordinates, so left out.
    assert!(material.emissive_texture.is_none());
    // Tangents point along U (+X here), with the texture's "up" along +Y.
    assert_eq!(primitive.tangents.len(), 3);
    for tangent in &primitive.tangents {
        assert!((Vec3::new(tangent[0], tangent[1], tangent[2]) - Vec3::X).length() < 1e-5);
        assert_eq!(tangent[3], 1.0);
    }
}

#[test]
fn data_uris_are_decoded() {
    assert_eq!(
        data_uri("data:application/octet-stream;base64,SGk="),
        Some(b"Hi".to_vec())
    );
    assert_eq!(data_uri("buffer.bin"), None);
}

#[test]
fn embedded_images_are_named_for_copying_out() {
    // The example pyramid's texture is in its data: URI buffer.
    let files = embedded_files(&example_bytes(), ModelFormat::Gltf).unwrap();
    assert_eq!(files.len(), 1);
    let (name, png) = &files[0];
    assert!(image::guess_format(png).is_ok(), "{name}");
    let model = load_model(&example_bytes(), &example_folder(), ModelFormat::Gltf).unwrap();
    let texture = model.primitives[0].material.base_color_texture.as_ref();
    assert_eq!(texture.unwrap().file, Some(format!("textures/{name}")));
}

#[test]
fn missing_normals_are_worked_out_facing_outward() {
    // One triangle in the XY plane, counter-clockwise seen from +Z.
    let normals = smooth_normals(
        &[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        &[0, 1, 2],
    );
    assert_eq!(normals, vec![[0.0, 0.0, 1.0]; 3]);
}
