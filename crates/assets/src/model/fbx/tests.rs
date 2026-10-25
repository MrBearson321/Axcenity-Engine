use super::*;
use crate::model::{ModelFormat, Pick, load_model};

/// An ASCII FBX file of a square pyramid 4 wide and 3 tall, with one
/// red Phong material. `z_up` saves it in centimeters with Z up (as
/// 3ds Max does), otherwise in meters with Y up. With a `texture` path,
/// the material's diffuse color uses it.
pub(crate) fn pyramid_fbx(z_up: bool, texture: Option<&str>) -> String {
    let maps: Vec<(&str, &str)> = texture
        .map(|path| ("DiffuseColor", path))
        .into_iter()
        .collect();
    pyramid_fbx_with_maps(z_up, &maps)
}

/// Like [`pyramid_fbx`], with each `(property, path)` of `maps` a
/// texture linked to that property of the material ("DiffuseColor",
/// "NormalMap", "SpecularColor"…).
pub(crate) fn pyramid_fbx_with_maps(z_up: bool, maps: &[(&str, &str)]) -> String {
    let textures: Vec<TestTexture> = maps
        .iter()
        .map(|&(property, path)| TestTexture {
            property: Some(property),
            path,
            embedded: None,
        })
        .collect();
    pyramid_fbx_with_textures(z_up, &textures)
}

/// A texture in a test FBX file.
pub(crate) struct TestTexture<'a> {
    /// The material property it's linked to, or `None` for none.
    pub(crate) property: Option<&'a str>,
    pub(crate) path: &'a str,
    /// The image file's bytes, if it's embedded in the FBX file.
    pub(crate) embedded: Option<&'a [u8]>,
}

/// Like [`pyramid_fbx`], with `textures`.
pub(crate) fn pyramid_fbx_with_textures(z_up: bool, textures: &[TestTexture]) -> String {
    let (up_axis, front_axis, front_sign, unit, vertices) = if z_up {
        (
            2,
            1,
            -1,
            1,
            "-200,200,0, 200,200,0, 200,-200,0, -200,-200,0, 0,0,300",
        )
    } else {
        (1, 2, 1, 100, "-2,0,-2, 2,0,-2, 2,0,2, -2,0,2, 0,3,0")
    };
    let mut texture_object = String::new();
    let mut texture_connection = String::new();
    for (i, texture) in textures.iter().enumerate() {
        let (id, path) = (4000 + i, texture.path);
        texture_object.push_str(&format!(
            "\tTexture: {id}, \"Texture::Map{i}\", \"\" {{\n\
             \t\tType: \"TextureVideoClip\"\n\
             \t\tVersion: 202\n\
             \t\tTextureName: \"Texture::Map{i}\"\n\
             \t\tFileName: \"C:/Users/artist/Desktop/{path}\"\n\
             \t\tRelativeFilename: \"{path}\"\n\
             \t}}\n"
        ));
        if let Some(property) = texture.property {
            texture_connection.push_str(&format!("\tC: \"OP\",{id},3000, \"{property}\"\n"));
        }
        if let Some(content) = texture.embedded {
            let video = 5000 + i;
            let content = base64(content);
            texture_object.push_str(&format!(
                "\tVideo: {video}, \"Video::Map{i}\", \"Clip\" {{\n\
                 \t\tType: \"Clip\"\n\
                 \t\tFileName: \"C:/Users/artist/Desktop/{path}\"\n\
                 \t\tRelativeFilename: \"{path}\"\n\
                 \t\tContent: , \"{content}\"\n\
                 \t}}\n"
            ));
            texture_connection.push_str(&format!("\tC: \"OO\",{video},{id}\n"));
        }
    }
    format!(
        r#"; FBX 7.4.0 project file
FBXHeaderExtension:  {{
	FBXHeaderVersion: 1003
	FBXVersion: 7400
}}
GlobalSettings:  {{
	Version: 1000
	Properties70:  {{
		P: "UpAxis", "int", "Integer", "",{up_axis}
		P: "UpAxisSign", "int", "Integer", "",1
		P: "FrontAxis", "int", "Integer", "",{front_axis}
		P: "FrontAxisSign", "int", "Integer", "",{front_sign}
		P: "CoordAxis", "int", "Integer", "",0
		P: "CoordAxisSign", "int", "Integer", "",1
		P: "UnitScaleFactor", "double", "Number", "",{unit}
	}}
}}
Objects:  {{
	Geometry: 1000, "Geometry::Pyramid", "Mesh" {{
		Vertices: *15 {{
			a: {vertices}
		}}
		PolygonVertexIndex: *16 {{
			a: 0,1,2,-4,0,4,-2,1,4,-3,2,4,-4,3,4,-1
		}}
		GeometryVersion: 124
		LayerElementUV: 0 {{
			Version: 101
			Name: "UVMap"
			MappingInformationType: "ByPolygonVertex"
			ReferenceInformationType: "IndexToDirect"
			UV: *10 {{
				a: 0,0,1,0,1,1,0,1,0.5,0.5
			}}
			UVIndex: *16 {{
				a: 0,1,2,3,0,4,1,1,4,2,2,4,3,3,4,0
			}}
		}}
		LayerElementMaterial: 0 {{
			Version: 101
			Name: ""
			MappingInformationType: "AllSame"
			ReferenceInformationType: "IndexToDirect"
			Materials: *1 {{
				a: 0
			}}
		}}
		Layer: 0 {{
			Version: 100
			LayerElement:  {{
				Type: "LayerElementUV"
				TypedIndex: 0
			}}
			LayerElement:  {{
				Type: "LayerElementMaterial"
				TypedIndex: 0
			}}
		}}
	}}
	Model: 2000, "Model::Pyramid", "Mesh" {{
		Version: 232
		Properties70:  {{
		}}
	}}
	Material: 3000, "Material::Red", "" {{
		Version: 102
		ShadingModel: "phong"
		Properties70:  {{
			P: "DiffuseColor", "Color", "", "A",1,0,0
		}}
	}}
{texture_object}}}
Connections:  {{
	C: "OO",1000,2000
	C: "OO",2000,0
	C: "OO",3000,2000
{texture_connection}}}
"#
    )
}

/// An ASCII FBX file, in meters with Y up, of two copies of a 2×2×2
/// cube: `Street/Bench` (inside an empty `Street` node, 10 m along X)
/// and `Lamp` (at the origin, with a second, empty `Lamp` after it).
pub(crate) fn two_cubes_fbx() -> String {
    r#"; FBX 7.4.0 project file
FBXHeaderExtension:  {
	FBXHeaderVersion: 1003
	FBXVersion: 7400
}
GlobalSettings:  {
	Version: 1000
	Properties70:  {
		P: "UpAxis", "int", "Integer", "",1
		P: "UpAxisSign", "int", "Integer", "",1
		P: "FrontAxis", "int", "Integer", "",2
		P: "FrontAxisSign", "int", "Integer", "",1
		P: "CoordAxis", "int", "Integer", "",0
		P: "CoordAxisSign", "int", "Integer", "",1
		P: "UnitScaleFactor", "double", "Number", "",100
	}
}
Objects:  {
	Geometry: 1000, "Geometry::Cube", "Mesh" {
		Vertices: *24 {
			a: -1,-1,-1, 1,-1,-1, 1,1,-1, -1,1,-1, -1,-1,1, 1,-1,1, 1,1,1, -1,1,1
		}
		PolygonVertexIndex: *24 {
			a: 0,3,2,-2,4,5,6,-8,0,1,5,-5,2,3,7,-7,1,2,6,-6,0,4,7,-4
		}
		GeometryVersion: 124
	}
	Model: 2000, "Model::Street", "Null" {
		Version: 232
	}
	Model: 2001, "Model::Bench", "Mesh" {
		Version: 232
		Properties70:  {
			P: "Lcl Translation", "Lcl Translation", "", "A",10,0,0
		}
	}
	Model: 2002, "Model::Lamp", "Mesh" {
		Version: 232
	}
	Model: 2003, "Model::Lamp", "Null" {
		Version: 232
	}
	Material: 3000, "Material::Stone", "" {
		Version: 102
		ShadingModel: "phong"
	}
}
Connections:  {
	C: "OO",2000,0
	C: "OO",2001,2000
	C: "OO",2002,0
	C: "OO",2003,0
	C: "OO",1000,2001
	C: "OO",1000,2002
	C: "OO",3000,2001
	C: "OO",3000,2002
}
"#
    .to_owned()
}

/// `bytes` in base64, as ASCII FBX files embed files.
fn base64(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut text = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | u32::from(b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                text.push(DIGITS[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                text.push('=');
            }
        }
    }
    text
}

/// A 2×2 checkered PNG.
pub(crate) fn checker_png() -> Vec<u8> {
    let image = image::RgbaImage::from_fn(2, 2, |x, y| match (x + y) % 2 {
        0 => image::Rgba([255, 255, 255, 255]),
        _ => image::Rgba([0, 0, 0, 255]),
    });
    let mut png = std::io::Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).unwrap();
    png.into_inner()
}

fn temp_folder(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!("axcenity-fbx-{name}"));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    folder
}

fn load_fbx(text: &str, folder: &Path) -> Result<Model, ModelError> {
    load_model(text.as_bytes(), folder, ModelFormat::Fbx)
}

fn assert_size(model: &Model, expected: [f32; 3]) {
    for (size, expected) in model.size.iter().zip(expected) {
        assert!((size - expected).abs() < 1e-4, "{:?}", model.size);
    }
}

#[test]
fn an_fbx_pyramid_loads_fitted_into_the_unit_cube() {
    let folder = temp_folder("pyramid");
    let model = load_fbx(&pyramid_fbx(false, None), &folder).unwrap();
    assert_size(&model, [4.0, 3.0, 4.0]);
    assert_eq!(model.primitives.len(), 1);
    let primitive = &model.primitives[0];
    // The square base is two triangles, plus four sides.
    assert_eq!(primitive.indices.len(), 6 * 3);
    assert_eq!(primitive.normals.len(), primitive.positions.len());
    for corner in &primitive.positions {
        assert!(corner.iter().all(|c| c.abs() <= 0.5 + 1e-6), "{corner:?}");
    }
    assert_eq!(model.hull_points.len(), 5);
    let material = &primitive.material;
    assert_eq!(material.base_color, [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(material.alpha_mode, AlphaMode::Opaque);
    assert!(material.base_color_texture.is_none());
    assert!(primitive.uvs.is_empty());
    assert!(model.warnings.is_empty());
}

#[test]
fn other_axes_and_units_are_converted_to_studs_with_y_up() {
    let folder = temp_folder("z-up");
    let model = load_fbx(&pyramid_fbx(true, None), &folder).unwrap();
    assert_size(&model, [4.0, 3.0, 4.0]);
}

#[test]
fn textures_are_found_next_to_the_model() {
    let folder = temp_folder("texture");
    std::fs::create_dir_all(folder.join("textures")).unwrap();
    std::fs::write(folder.join("textures/checker.png"), checker_png()).unwrap();
    // By its relative path, and by its name in `textures` when the
    // relative path leads outside the folder.
    for path in ["textures/checker.png", "../../maps/checker.png"] {
        let model = load_fbx(&pyramid_fbx(false, Some(path)), &folder).unwrap();
        assert!(model.warnings.is_empty(), "{:?}", model.warnings);
        let primitive = &model.primitives[0];
        let texture = primitive.material.base_color_texture.as_ref().unwrap();
        assert_eq!((texture.width, texture.height), (2, 2));
        assert_eq!(primitive.uvs.len(), primitive.positions.len());
        // V is flipped: FBX's (0, 0) is the bottom left.
        assert!(primitive.uvs.contains(&[0.0, 1.0]), "{:?}", primitive.uvs);
    }
}

/// A 2×2 PNG all of one color.
fn solid_png(rgba: [u8; 4]) -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(2, 2, image::Rgba(rgba));
    let mut png = std::io::Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).unwrap();
    png.into_inner()
}

#[test]
fn a_phong_materials_maps_become_pbr_maps() {
    let folder = temp_folder("phong-maps");
    let files = [
        ("color.png", [200, 100, 50, 255]),
        ("normal.png", [128, 128, 255, 255]),
        ("gloss.png", [255, 255, 255, 255]),
        ("spec.png", [64, 64, 64, 255]),
        ("glow.png", [255, 0, 0, 255]),
        ("cutout.png", [255, 255, 255, 0]),
    ];
    for (name, rgba) in files {
        std::fs::write(folder.join(name), solid_png(rgba)).unwrap();
    }
    let fbx = pyramid_fbx_with_maps(
        false,
        &[
            ("DiffuseColor", "color.png"),
            ("NormalMap", "normal.png"),
            ("ShininessExponent", "gloss.png"),
            ("SpecularColor", "spec.png"),
            ("EmissiveColor", "glow.png"),
            ("TransparentColor", "cutout.png"),
        ],
    );
    let model = load_fbx(&fbx, &folder).unwrap();
    assert!(model.warnings.is_empty(), "{:?}", model.warnings);
    let primitive = &model.primitives[0];
    let material = &primitive.material;
    assert_eq!(material.name, "Red");
    assert!(material.base_color_texture.is_some());
    assert!(material.normal_texture.is_some());
    // Every corner gets a tangent for the normal map.
    assert_eq!(primitive.tangents.len(), primitive.positions.len());
    // Shininess is glossiness: inverted, and the map alone sets it.
    let roughness = material.roughness_texture.as_ref().unwrap();
    assert!(roughness.invert);
    assert_eq!(material.roughness, 1.0);
    assert!(material.specular_texture.is_some());
    // An emission map with no emission color glows in its own colors.
    assert!(material.emissive_texture.is_some());
    assert_eq!(material.emissive, [1.0; 3]);
    // The cut-out map has alpha, so that's the channel used.
    let opacity = material.opacity_texture.as_ref().unwrap();
    assert_eq!(opacity.channel, Channel::Alpha);
    assert_eq!(material.alpha_mode, AlphaMode::Mask { cutoff: 0.5 });
}

#[test]
fn a_bump_map_is_used_only_if_it_is_really_a_normal_map() {
    let folder = temp_folder("bump");
    std::fs::write(folder.join("normal.png"), solid_png([120, 135, 250, 255])).unwrap();
    std::fs::write(folder.join("height.png"), solid_png([90, 90, 90, 255])).unwrap();
    let normal = load_fbx(
        &pyramid_fbx_with_maps(false, &[("Bump", "normal.png")]),
        &folder,
    );
    assert!(
        normal.unwrap().primitives[0]
            .material
            .normal_texture
            .is_some()
    );
    let height = load_fbx(
        &pyramid_fbx_with_maps(false, &[("Bump", "height.png")]),
        &folder,
    );
    let height = height.unwrap();
    assert!(height.primitives[0].material.normal_texture.is_none());
    // With no texture left, it needs no texture coordinates.
    assert!(height.primitives[0].uvs.is_empty());
}

/// A texture file in `folder`, linked to `property`.
fn linked<'a>(property: &'a str, path: &'a str) -> TestTexture<'a> {
    TestTexture {
        property: Some(property),
        path,
        embedded: None,
    }
}

#[test]
fn embedded_textures_are_used_and_listed_for_copying_out() {
    let folder = temp_folder("embedded");
    let png = checker_png();
    let fbx = pyramid_fbx_with_textures(
        false,
        &[TestTexture {
            property: Some("DiffuseColor"),
            path: "wood_BaseColor.png",
            embedded: Some(&png),
        }],
    );
    // Nothing is in the folder: the texture comes from the file itself.
    let model = load_fbx(&fbx, &folder).unwrap();
    assert!(model.warnings.is_empty(), "{:?}", model.warnings);
    let texture = model.primitives[0].material.base_color_texture.as_ref();
    assert_eq!(texture.map(|t| (t.width, t.height)), Some((2, 2)));
    let files = embedded_textures(fbx.as_bytes()).unwrap();
    assert_eq!(files, vec![("wood_BaseColor.png".to_owned(), png)]);
}

#[test]
fn packed_textures_are_split_into_maps_by_their_names() {
    let folder = temp_folder("packed");
    std::fs::write(folder.join("rock_ORM.png"), solid_png([255, 128, 0, 255])).unwrap();
    std::fs::write(
        folder.join("crate_MetallicSmoothness.png"),
        solid_png([255, 0, 0, 128]),
    )
    .unwrap();

    // On a property the engine doesn't read: its name says what it is.
    let orm = load_fbx(
        &pyramid_fbx_with_textures(false, &[linked("CustomPacked", "rock_ORM.png")]),
        &folder,
    )
    .unwrap();
    let material = &orm.primitives[0].material;
    let channel = |map: &Option<ChannelTexture>| map.as_ref().map(|map| map.channel);
    assert_eq!(channel(&material.occlusion_texture), Some(Channel::Red));
    assert_eq!(channel(&material.roughness_texture), Some(Channel::Green));
    assert_eq!(channel(&material.metalness_texture), Some(Channel::Blue));
    assert_eq!((material.metalness, material.roughness), (1.0, 1.0));

    // In a Phong material's specular slot, as Unity-made files have it:
    // the name wins, and it's metalness and smoothness.
    let unity = load_fbx(
        &pyramid_fbx_with_textures(
            false,
            &[linked("SpecularColor", "crate_MetallicSmoothness.png")],
        ),
        &folder,
    )
    .unwrap();
    let material = &unity.primitives[0].material;
    assert!(material.specular_texture.is_none());
    assert_eq!(channel(&material.metalness_texture), Some(Channel::Red));
    let roughness = material.roughness_texture.as_ref().unwrap();
    assert_eq!(
        (roughness.channel, roughness.invert),
        (Channel::Alpha, true)
    );
}

#[test]
fn an_unlinked_embedded_texture_is_used_by_its_name() {
    let folder = temp_folder("loose");
    let png = solid_png([128, 128, 255, 255]);
    let fbx = pyramid_fbx_with_textures(
        false,
        &[
            linked("DiffuseColor", "color.png"),
            TestTexture {
                property: None,
                path: "Red_Normal.png",
                embedded: Some(&png),
            },
        ],
    );
    std::fs::write(folder.join("color.png"), checker_png()).unwrap();
    let model = load_fbx(&fbx, &folder).unwrap();
    assert!(model.primitives[0].material.normal_texture.is_some());
}

#[test]
fn textures_of_unknown_use_and_height_maps_are_noted_and_left_out() {
    let folder = temp_folder("unknown");
    std::fs::write(folder.join("wood.png"), checker_png()).unwrap();
    std::fs::write(folder.join("bricks.png"), solid_png([90, 90, 90, 255])).unwrap();
    let fbx = pyramid_fbx_with_textures(
        false,
        &[
            linked("CustomThing", "wood.png"),
            linked("Bump", "bricks.png"),
        ],
    );
    let model = load_fbx(&fbx, &folder).unwrap();
    let warnings = model.warnings.join("\n");
    assert!(
        warnings.contains("has a texture `wood.png` it doesn't know the use of"),
        "{warnings}"
    );
    assert!(
        warnings.contains("has a height map `bricks.png`"),
        "{warnings}"
    );
    assert!(!model.primitives[0].material.has_textures());
}

#[test]
fn a_missing_or_unreadable_texture_is_a_warning_not_an_error() {
    let folder = temp_folder("no-texture");
    std::fs::write(folder.join("paint.psd"), b"8BPS not really").unwrap();
    let missing = load_fbx(&pyramid_fbx(false, Some("gone.png")), &folder).unwrap();
    assert!(missing.primitives[0].material.base_color_texture.is_none());
    assert_eq!(missing.warnings.len(), 1);
    assert!(
        missing.warnings[0].contains("`gone.png`: it isn't there"),
        "{:?}",
        missing.warnings
    );
    let unreadable = load_fbx(&pyramid_fbx(false, Some("paint.psd")), &folder).unwrap();
    assert!(
        unreadable.warnings[0].contains("only PNG, JPEG, TGA and BMP"),
        "{:?}",
        unreadable.warnings
    );
}

#[test]
fn the_objects_in_an_fbx_are_listed_as_a_tree() {
    let objects = crate::model::list_objects(two_cubes_fbx().as_bytes(), ModelFormat::Fbx).unwrap();
    let rows: Vec<(&str, usize, usize)> = objects
        .iter()
        .map(|object| (object.path.as_str(), object.depth, object.triangles))
        .collect();
    // The empty second Lamp has nothing to import, so it isn't listed.
    assert_eq!(
        rows,
        [("Street", 0, 0), ("Street/Bench", 1, 12), ("Lamp", 0, 12)]
    );
    assert_eq!(objects[1].materials, ["Stone"]);
}

#[test]
fn picked_fbx_objects_load_on_their_own_and_keep_their_places() {
    let folder = temp_folder("picks");
    let picks = [
        Pick::Objects(vec!["Street/Bench".into()]),
        Pick::Objects(vec!["Lamp".into()]),
        Pick::Whole,
    ];
    let models = crate::model::load_models(
        two_cubes_fbx().as_bytes(),
        &folder,
        ModelFormat::Fbx,
        &picks,
    )
    .unwrap();
    let [bench, lamp, whole] = &models[..] else {
        panic!("one model per pick");
    };
    assert_size(bench, [2.0, 2.0, 2.0]);
    assert!((bench.center[0] - 10.0).abs() < 1e-4, "{:?}", bench.center);
    assert!(
        lamp.center.iter().all(|c| c.abs() < 1e-4),
        "{:?}",
        lamp.center
    );
    assert_size(whole, [12.0, 2.0, 2.0]);
    assert_eq!(whole.primitives.len(), 2);

    let missing = crate::model::load_models(
        two_cubes_fbx().as_bytes(),
        &folder,
        ModelFormat::Fbx,
        &[Pick::Objects(vec!["Street".into()])],
    );
    // A group has no mesh of its own.
    assert_eq!(
        missing.unwrap_err().to_string(),
        "it has no object `Street`"
    );
}

#[test]
fn broken_fbx_files_give_errors_not_crashes() {
    let folder = temp_folder("broken");
    let error = |bytes: &[u8]| {
        load_model(bytes, &folder, ModelFormat::Fbx)
            .unwrap_err()
            .to_string()
    };
    assert!(error(b"").contains("isn't a valid FBX file"));
    assert!(error(b"Kaydara FBX Binary  \0\x1a\0").contains("isn't a valid FBX file"));
    // Cut off halfway.
    let text = pyramid_fbx(false, None);
    assert!(load_fbx(&text[..text.len() / 2], &folder).is_err());
    // Valid, but nothing to draw.
    let empty = "; FBX 7.4.0 project file\nFBXHeaderExtension:  {\n\tFBXVersion: 7400\n}\n";
    assert_eq!(error(empty.as_bytes()), "it has no triangles to draw");
}

#[test]
fn hello_pbr_examples_load_their_maps() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-pbr");
    let models = root.join("assets/models");
    let helmet = std::fs::read(models.join("DamagedHelmet/DamagedHelmet.glb")).unwrap();
    let helmet = load_model(&helmet, &models.join("DamagedHelmet"), ModelFormat::Gltf).unwrap();
    let material = &helmet.primitives[0].material;
    assert!(material.base_color_texture.is_some());
    assert!(material.normal_texture.is_some());
    assert!(material.metalness_texture.is_some());
    assert!(material.roughness_texture.is_some());
    assert!(material.occlusion_texture.is_some());
    assert!(material.emissive_texture.is_some());

    let pbr = std::fs::read_to_string(models.join("pbr_panel.fbx")).unwrap();
    let pbr = load_fbx(&pbr, &models).unwrap();
    assert!(pbr.warnings.is_empty(), "{:?}", pbr.warnings);
    let material = &pbr.primitives[0].material;
    assert!(material.base_color_texture.is_some());
    assert!(material.normal_texture.is_some());
    assert!(material.roughness_texture.is_some());
    assert!(material.metalness_texture.is_some());
    assert!(material.emissive_texture.is_some());
    assert!(material.opacity_texture.is_some());
    assert!(material.occlusion_texture.is_some());

    let phong = std::fs::read_to_string(models.join("phong_panel.fbx")).unwrap();
    let phong = load_fbx(&phong, &models).unwrap();
    assert!(phong.warnings.is_empty(), "{:?}", phong.warnings);
    let material = &phong.primitives[0].material;
    let roughness = material.roughness_texture.as_ref().unwrap();
    assert!(roughness.invert, "glossiness should invert to roughness");
    assert!(material.specular_texture.is_some());
}
