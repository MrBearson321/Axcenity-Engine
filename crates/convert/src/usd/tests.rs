//! USD scenes converted and read back by the engine's own glTF reader.

use std::path::{Path, PathBuf};

use axcenity_assets::{ModelFormat, list_objects, load_model};

fn temp_folder(name: &str) -> PathBuf {
    let folder =
        std::env::temp_dir().join(format!("axcenity-usd-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    folder
}

/// A 2×2 PNG of one color.
fn png(rgba: [u8; 4]) -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(2, 2, image::Rgba(rgba));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    bytes.into_inner()
}

/// A box from `low` to `high` as a USD mesh body (points, quads, st).
fn box_mesh(low: [f32; 3], high: [f32; 3]) -> String {
    let mut points = Vec::new();
    for x in [low[0], high[0]] {
        for y in [low[1], high[1]] {
            for z in [low[2], high[2]] {
                points.push(format!("({x}, {y}, {z})"));
            }
        }
    }
    format!(
        r#"
        int[] faceVertexCounts = [4, 4, 4, 4, 4, 4]
        int[] faceVertexIndices = [0, 1, 3, 2, 4, 6, 7, 5, 0, 4, 5, 1, 2, 3, 7, 6, 0, 2, 6, 4, 1, 5, 7, 3]
        point3f[] points = [{}]
        texCoord2f[] primvars:st = [(0, 0), (1, 0), (1, 1), (0, 1)] (
            interpolation = "faceVarying"
        )
        int[] primvars:st:indices = [0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3]
"#,
        points.join(", ")
    )
}

/// A kitchen: a textured Table (2 × 1 × 2, in centimeters, Z up) and a red
/// Chair moved along X, inside a `World` Xform, with UsdPreviewSurface
/// materials.
fn kitchen_usda() -> String {
    format!(
        r#"#usda 1.0
(
    defaultPrim = "World"
    metersPerUnit = 0.01
    upAxis = "Z"
)

def Xform "World"
{{
    def Xform "Furniture"
    {{
        def Mesh "Table" (
            prepend apiSchemas = ["MaterialBindingAPI"]
        )
        {{
            {table}
            rel material:binding = </World/Looks/Wood>
        }}

        def Mesh "Chair"
        {{
            {chair}
            rel material:binding = </World/Looks/Paint>
            double3 xformOp:translate = (300, 0, 0)
            uniform token[] xformOpOrder = ["xformOp:translate"]
        }}

        def Mesh "Hidden"
        {{
            {chair}
            token visibility = "invisible"
        }}
    }}

    def Scope "Looks"
    {{
        def Material "Wood"
        {{
            token outputs:surface.connect = </World/Looks/Wood/Surface.outputs:surface>

            def Shader "Surface"
            {{
                uniform token info:id = "UsdPreviewSurface"
                color3f inputs:diffuseColor.connect = </World/Looks/Wood/Color.outputs:rgb>
                float inputs:roughness.connect = </World/Looks/Wood/Packed.outputs:g>
                float inputs:metallic.connect = </World/Looks/Wood/Packed.outputs:b>
                token outputs:surface
            }}

            def Shader "Color"
            {{
                uniform token info:id = "UsdUVTexture"
                asset inputs:file = @./textures/wood.png@
                float3 outputs:rgb
            }}

            def Shader "Packed"
            {{
                uniform token info:id = "UsdUVTexture"
                asset inputs:file = @textures/orm.png@
                float outputs:g
                float outputs:b
            }}
        }}

        def Material "Paint"
        {{
            token outputs:surface.connect = </World/Looks/Paint/Surface.outputs:surface>

            def Shader "Surface"
            {{
                uniform token info:id = "UsdPreviewSurface"
                color3f inputs:diffuseColor = (1, 0, 0)
                float inputs:metallic = 1
                float inputs:roughness = 0.25
                float inputs:opacity = 0.5
                token outputs:surface
            }}
        }}
    }}
}}
"#,
        table = box_mesh([-100.0, -100.0, 0.0], [100.0, 100.0, 100.0]),
        chair = box_mesh([-50.0, -50.0, 0.0], [50.0, 50.0, 50.0]),
    )
}

fn kitchen_folder(name: &str) -> PathBuf {
    let folder = temp_folder(name);
    std::fs::write(folder.join("kitchen.usda"), kitchen_usda()).unwrap();
    std::fs::create_dir_all(folder.join("textures")).unwrap();
    std::fs::write(folder.join("textures/wood.png"), png([200, 150, 100, 255])).unwrap();
    std::fs::write(folder.join("textures/orm.png"), png([255, 80, 30, 255])).unwrap();
    folder
}

/// Converts and loads the result with the engine's glTF reader.
fn convert_and_load(file: &Path) -> (crate::Converted, axcenity_assets::Model) {
    let converted = crate::convert(file).unwrap();
    let out = file.with_extension("glb");
    std::fs::write(&out, &converted.glb).unwrap();
    let model = load_model(&converted.glb, out.parent().unwrap(), ModelFormat::Gltf).unwrap();
    (converted, model)
}

#[test]
fn a_usd_scene_becomes_a_glb_the_engine_reads_with_units_and_up_axis() {
    let folder = kitchen_folder("kitchen");
    let (converted, model) = convert_and_load(&folder.join("kitchen.usda"));
    assert!(converted.warnings.is_empty(), "{:?}", converted.warnings);
    // Centimeters to meters, Z up to Y up: the table is 2 wide, 1 tall and
    // 2 deep, the chair 0.5 tall beside it at x = 3, the hidden one left out.
    for (size, expected) in model.size.iter().zip([3.5 + 1.0, 1.0, 2.0]) {
        assert!((size - expected).abs() < 1e-3, "{:?}", model.size);
    }
    assert_eq!(model.primitives.len(), 2);

    let objects = list_objects(&converted.glb, ModelFormat::Gltf).unwrap();
    let paths: Vec<&str> = objects.iter().map(|object| object.path.as_str()).collect();
    assert_eq!(
        paths,
        ["World", "World/Furniture", "World/Furniture/Table", "World/Furniture/Chair"]
    );
}

#[test]
fn preview_surfaces_become_materials_with_their_textures_packed() {
    let folder = kitchen_folder("materials");
    let (_, model) = convert_and_load(&folder.join("kitchen.usda"));
    let material = |name: &str| {
        &model
            .primitives
            .iter()
            .find(|primitive| primitive.material.name == name)
            .unwrap_or_else(|| panic!("a {name} primitive"))
            .material
    };
    let wood = material("Wood");
    let color = wood.base_color_texture.as_ref().expect("its color map");
    assert_eq!(&color.rgba()[..4], &[200, 150, 100, 255]);
    // Roughness from green, metalness from blue, as the shader wired them.
    let metal = wood.metalness_texture.as_ref().expect("a metalness map");
    let rough = wood.roughness_texture.as_ref().expect("a roughness map");
    let pick = |texture: &axcenity_assets::ChannelTexture| {
        let channel = match texture.channel {
            axcenity_assets::Channel::Red => 0,
            axcenity_assets::Channel::Green => 1,
            axcenity_assets::Channel::Blue => 2,
            axcenity_assets::Channel::Alpha => 3,
        };
        texture.image.rgba()[channel]
    };
    assert_eq!((pick(metal), pick(rough)), (30, 80));

    let paint = material("Paint");
    assert!(paint.base_color_texture.is_none());
    assert!((paint.base_color[0] - 1.0).abs() < 1e-4 && paint.base_color[1] < 1e-4);
    assert!((paint.base_color[3] - 0.5).abs() < 1e-4);
    assert!((paint.metalness - 1.0).abs() < 1e-4 && (paint.roughness - 0.25).abs() < 1e-4);
    assert_eq!(paint.alpha_mode, axcenity_assets::AlphaMode::Blend);
}

#[test]
fn a_usdz_reads_its_textures_from_inside_the_archive() {
    use std::io::Write;
    let folder = temp_folder("usdz");
    let path = folder.join("kitchen.usdz");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    // USDZ is stored, never compressed.
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("kitchen.usda", options).unwrap();
    zip.write_all(kitchen_usda().as_bytes()).unwrap();
    zip.start_file("textures/wood.png", options).unwrap();
    zip.write_all(&png([10, 20, 30, 255])).unwrap();
    zip.start_file("textures/orm.png", options).unwrap();
    zip.write_all(&png([255, 255, 0, 255])).unwrap();
    zip.finish().unwrap();

    let (converted, model) = convert_and_load(&path);
    assert!(converted.warnings.is_empty(), "{:?}", converted.warnings);
    let wood = model
        .primitives
        .iter()
        .find(|primitive| primitive.material.name == "Wood")
        .unwrap();
    let color = wood.material.base_color_texture.as_ref().unwrap();
    assert_eq!(&color.rgba()[..4], &[10, 20, 30, 255]);
}

#[test]
fn missing_textures_and_unsupported_prims_are_warnings() {
    let folder = temp_folder("warnings");
    let text = kitchen_usda().replace(
        "    def Scope \"Looks\"",
        "    def Sphere \"Ball\"\n    {\n        double radius = 5\n    }\n\n    def Scope \"Looks\"",
    );
    std::fs::write(folder.join("kitchen.usda"), text).unwrap();
    let (converted, model) = convert_and_load(&folder.join("kitchen.usda"));
    assert_eq!(model.primitives.len(), 2);
    assert!(
        converted
            .warnings
            .iter()
            .any(|warning| warning == "its texture `./textures/wood.png` isn't there, so it's drawn without it"),
        "{:?}",
        converted.warnings
    );
    assert!(
        converted
            .warnings
            .iter()
            .any(|warning| warning.contains("left out what glTF import doesn't bring in: Sphere")),
        "{:?}",
        converted.warnings
    );
}

#[test]
fn bad_files_are_errors_not_crashes() {
    let folder = temp_folder("bad");
    let empty = folder.join("empty.usda");
    std::fs::write(&empty, "#usda 1.0\n").unwrap();
    assert_eq!(
        crate::convert(&empty).unwrap_err().to_string(),
        "it has no meshes to draw"
    );
    let garbage = folder.join("garbage.usdc");
    std::fs::write(&garbage, b"PXR-USDC this is not really a crate file").unwrap();
    assert!(
        crate::convert(&garbage)
            .unwrap_err()
            .to_string()
            .starts_with("it isn't a valid USD file"),
    );
    let broken = folder.join("broken.usda");
    std::fs::write(
        &broken,
        "#usda 1.0\ndef Mesh \"M\" {\n int[] faceVertexCounts = [3]\n int[] faceVertexIndices = [0, 1, 9]\n point3f[] points = [(0,0,0), (1,0,0), (0,1,0)]\n}\n",
    )
    .unwrap();
    let error = crate::convert(&broken).unwrap_err().to_string();
    assert_eq!(error, "it has no meshes to draw");
    assert!(matches!(
        crate::convert(Path::new("model.fbx")),
        Err(crate::ConvertError::NotConverted)
    ));
}

#[test]
fn polygons_are_fanned_and_left_handed_meshes_turned_round() {
    let folder = temp_folder("winding");
    let file = folder.join("pentagon.usda");
    let mesh = |orientation: &str| {
        format!(
            "#usda 1.0\n(\n    metersPerUnit = 1\n)\ndef Mesh \"P\" {{\n uniform token orientation = \"{orientation}\"\n int[] faceVertexCounts = [5]\n int[] faceVertexIndices = [0, 1, 2, 3, 4]\n point3f[] points = [(0,0,0), (2,0,0), (2,1,0), (1,2,0), (0,1,0)]\n}}\n"
        )
    };
    std::fs::write(&file, mesh("rightHanded")).unwrap();
    let (_, right) = convert_and_load(&file);
    std::fs::write(&file, mesh("leftHanded")).unwrap();
    let (_, left) = convert_and_load(&file);
    assert_eq!(right.primitives[0].indices.len(), 9);
    // Facing +Z when right-handed; the left-handed one faces -Z.
    assert!(right.primitives[0].normals[0][2] > 0.9, "{:?}", right.primitives[0].normals);
    assert!(left.primitives[0].normals[0][2] < -0.9, "{:?}", left.primitives[0].normals);
}
