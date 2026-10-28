use axcenity_render::SurfaceShader;

use super::*;

/// Wires `from` (a node and output) to `to` (a node and input).
fn wire(graph: &mut NodeGraph, from: (u32, &str), to: (u32, &str)) {
    graph
        .connect(
            &MaterialLibrary,
            PinRef::new(from.0, from.1),
            PinRef::new(to.0, to.1),
        )
        .unwrap_or_else(|why| panic!("{from:?} → {to:?}: {why}"));
}

fn add(graph: &mut NodeGraph, kind: &str) -> u32 {
    let spec = MaterialLibrary.spec(kind).expect("a material node");
    graph.add_node(spec, [0.0, 0.0])
}

fn set(graph: &mut NodeGraph, node: u32, name: &str, value: &str) {
    graph
        .node_mut(node)
        .unwrap()
        .values
        .insert(name.to_owned(), value.to_owned());
}

/// Compiles `graph`, checks its shader with naga, and gives it back.
fn compiled(graph: &NodeGraph) -> CompiledGraph {
    let compiled = compile(graph).unwrap_or_else(|errors| panic!("{errors:#?}"));
    if let Err(error) = SurfaceShader::new("graph", &compiled.shader).check() {
        panic!("{error}\n{}", compiled.shader);
    }
    compiled
}

fn errors_of(graph: &NodeGraph) -> Vec<GraphError> {
    compile(graph).expect_err("the graph has an error")
}

/// Stone and moss, mixed by Perlin noise, tinted by a parameter.
pub(crate) fn mossy_stone(stone: &str, moss: &str) -> NodeGraph {
    let mut graph = NodeGraph::default();
    let output = add(&mut graph, OUTPUT);
    let stone_node = add(&mut graph, "texture");
    set(&mut graph, stone_node, "Texture", stone);
    let moss_node = add(&mut graph, "texture");
    set(&mut graph, moss_node, "Texture", moss);
    let tint = add(&mut graph, COLOR_PARAMETER);
    set(&mut graph, tint, "Name", "Moss Tint");
    set(&mut graph, tint, "Color", "0.35, 0.7, 0.2");
    let tinted = add(&mut graph, "multiply");
    wire(&mut graph, (moss_node, "Color"), (tinted, "A"));
    wire(&mut graph, (tint, "Color"), (tinted, "B"));
    let noise = add(&mut graph, "noise");
    set(&mut graph, noise, "Scale", "4");
    let mask = add(&mut graph, "smooth_step");
    set(&mut graph, mask, "Edge 0", "0.4");
    set(&mut graph, mask, "Edge 1", "0.6");
    wire(&mut graph, (noise, "Value"), (mask, "Value"));
    let mix = add(&mut graph, "mix");
    wire(&mut graph, (stone_node, "Color"), (mix, "A"));
    wire(&mut graph, (tinted, "Result"), (mix, "B"));
    wire(&mut graph, (mask, "Result"), (mix, "Factor"));
    wire(&mut graph, (mix, "Result"), (output, "Base Color"));
    let rough = add(&mut graph, NUMBER_PARAMETER);
    set(&mut graph, rough, "Name", "Roughness");
    set(&mut graph, rough, "Value", "0.85");
    wire(&mut graph, (rough, "Value"), (output, "Roughness"));
    // Laid out left to right, as it reads.
    for (node, at) in [
        (stone_node, [0.0, 0.0]),
        (moss_node, [0.0, 200.0]),
        (tint, [0.0, 400.0]),
        (noise, [0.0, 560.0]),
        (tinted, [320.0, 280.0]),
        (mask, [320.0, 500.0]),
        (mix, [600.0, 160.0]),
        (rough, [600.0, 420.0]),
        (output, [900.0, 160.0]),
    ] {
        graph.node_mut(node).unwrap().position = at;
    }
    graph
}

/// hello-material-graph's folder.
fn example() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-material-graph")
}

#[test]
fn hello_material_graphs_materials_are_what_their_graph_makes() {
    use axcenity_assets::{read_material_file, write_material_file};
    let (mossy, autumn) = (
        "assets/materials/MossyStone.material.json",
        "assets/materials/AutumnStone.material.json",
    );
    if std::env::var_os("AXCENITY_WRITE_EXAMPLES").is_some() {
        let mut file = MaterialFile {
            color: Some([1.0; 3]),
            metalness: Some(0.0),
            roughness: Some(0.85),
            graph: Some(graph_value(&mossy_stone(
                "assets/textures/stone.png",
                "assets/textures/moss.png",
            ))),
            ..MaterialFile::default()
        };
        compiled(&graph_of(file.graph.as_ref().unwrap()).unwrap()).apply_to(&mut file);
        write_material_file(&example().join(mossy), &file).unwrap();
        let instance = MaterialFile {
            base: Some(mossy.to_owned()),
            parameters: BTreeMap::from([(
                "Moss Tint".to_owned(),
                ParamValue::Vector(vec![1.0, 0.45, 0.1]),
            )]),
            ..MaterialFile::default()
        };
        write_material_file(&example().join(autumn), &instance).unwrap();
    }
    let file = read_material_file(&example().join(mossy)).unwrap();
    let graph = graph_of(file.graph.as_ref().expect("it has a graph")).unwrap();
    let mut made = file.clone();
    compiled(&graph).apply_to(&mut made);
    assert_eq!(
        made, file,
        "its shader, maps and parameters are its graph's"
    );
    let instance = read_material_file(&example().join(autumn)).unwrap();
    assert_eq!(instance.base.as_deref(), Some(mossy));
    assert!(instance.graph.is_none() && instance.shader.is_none());
}

#[test]
fn a_graph_mixing_two_textures_by_noise_compiles_to_a_surface_shader_reading_both() {
    let graph = mossy_stone("assets/textures/stone.png", "assets/textures/moss.png");
    let compiled = compiled(&graph);
    // Both are colors, so both go where they're read as linear light.
    assert_eq!(
        compiled.maps.color,
        Some(MapFile::of("assets/textures/stone.png"))
    );
    assert_eq!(
        compiled.maps.emissive,
        Some(MapFile::of("assets/textures/moss.png"))
    );
    assert_eq!(compiled.maps.normal, None);
    let shader = &compiled.shader;
    assert!(
        shader.contains("textureSample(base_color_texture"),
        "{shader}"
    );
    assert!(
        shader.contains("textureSample(emissive_texture"),
        "{shader}"
    );
    assert!(shader.contains("graph_perlin_noise("), "{shader}");
    assert!(
        !shader.contains("graph_voronoi_noise"),
        "only the helpers it uses"
    );
    // Parameters in name order: Moss Tint, then Roughness.
    assert!(shader.contains("srgb(parameter(0).xyz)"), "{shader}");
    assert!(
        shader.contains("out.roughness = parameter(1).x;"),
        "{shader}"
    );
    assert!(
        shader.contains("out.metalness = input.metalness;"),
        "{shader}"
    );
    assert_eq!(
        compiled.parameters,
        BTreeMap::from([
            (
                "Moss Tint".to_owned(),
                ParamValue::Vector(vec![0.35, 0.7, 0.2])
            ),
            ("Roughness".to_owned(), ParamValue::Number(0.85)),
        ])
    );
    // The same graph makes the same code.
    assert_eq!(compile(&graph).unwrap(), compiled);
}

#[test]
fn every_node_compiles_to_wgsl_that_naga_accepts() {
    for spec in SPECS.iter().filter(|spec| spec.kind != OUTPUT) {
        let mut variants: Vec<(&str, &str)> = vec![("", "")];
        for field in spec.fields {
            if let FieldKind::Choice(choices) = field.kind {
                variants = choices.iter().map(|choice| (field.name, *choice)).collect();
            }
        }
        for (field_name, choice) in variants {
            let mut graph = NodeGraph::default();
            let output = add(&mut graph, OUTPUT);
            let node = add(&mut graph, spec.kind);
            if !field_name.is_empty() {
                set(&mut graph, node, field_name, choice);
            }
            for field in spec.fields {
                if field.kind == FieldKind::Image {
                    set(&mut graph, node, field.name, "assets/textures/a.png");
                }
            }
            for pin in spec.inputs.iter().filter(|pin| pin.required) {
                let value = if pin.ty == PinType::Vector3 {
                    "0.5, 0.25, 1"
                } else {
                    "0.5"
                };
                set(&mut graph, node, pin.name, value);
            }
            // Every output, each into an input it fits.
            for (index, pin) in spec.outputs.iter().enumerate() {
                let into = match pin.ty {
                    PinType::Number => ["Roughness", "Metalness", "Opacity"][index % 3],
                    PinType::Vector2 => {
                        let split = add(&mut graph, "split");
                        wire(&mut graph, (node, pin.name), (split, "Vector"));
                        wire(&mut graph, (split, "X"), (output, "Opacity"));
                        continue;
                    }
                    _ => ["Base Color", "Emissive"][index % 2],
                };
                wire(&mut graph, (node, pin.name), (output, into));
            }
            compiled(&graph);
        }
    }
}

#[test]
fn images_beyond_the_color_slots_are_read_in_the_others_and_data_as_it_is() {
    let mut graph = NodeGraph::default();
    let output = add(&mut graph, OUTPUT);
    let mut last = None;
    for (i, read_as) in ["Color", "Color", "Color", "Data", "Data"]
        .iter()
        .enumerate()
    {
        let texture = add(&mut graph, "texture");
        set(
            &mut graph,
            texture,
            "Texture",
            &format!("assets/textures/{i}.png"),
        );
        set(&mut graph, texture, "Read as", read_as);
        // The sum so far first, so the images are read in order.
        let sum = add(&mut graph, "add");
        wire(&mut graph, (texture, "Color"), (sum, "B"));
        if let Some(last) = last {
            wire(&mut graph, (last, "Result"), (sum, "A"));
        }
        last = Some(sum);
    }
    wire(
        &mut graph,
        (last.unwrap(), "Result"),
        (output, "Base Color"),
    );
    let compiled = compiled(&graph);
    let path = |map: &Option<MapFile>| map.as_ref().and_then(|map| map.path.clone());
    assert_eq!(
        path(&compiled.maps.color).as_deref(),
        Some("assets/textures/0.png")
    );
    assert_eq!(
        path(&compiled.maps.emissive).as_deref(),
        Some("assets/textures/1.png")
    );
    // The third color is read as values and turned into linear light.
    assert_eq!(
        path(&compiled.maps.metalness).as_deref(),
        Some("assets/textures/2.png")
    );
    assert!(compiled.shader.contains("srgb(n"), "{}", compiled.shader);
    assert_eq!(
        path(&compiled.maps.roughness).as_deref(),
        Some("assets/textures/3.png")
    );
    assert_eq!(
        path(&compiled.maps.opacity).as_deref(),
        Some("assets/textures/4.png")
    );

    // Seven images don't fit.
    for i in 5..7 {
        let texture = add(&mut graph, "texture");
        set(
            &mut graph,
            texture,
            "Texture",
            &format!("assets/textures/{i}.png"),
        );
        let sum = add(&mut graph, "add");
        wire(&mut graph, (texture, "Color"), (sum, "B"));
        wire(&mut graph, (last.unwrap(), "Result"), (sum, "A"));
        last = Some(sum);
    }
    wire(
        &mut graph,
        (last.unwrap(), "Result"),
        (output, "Base Color"),
    );
    let errors = errors_of(&graph);
    assert_eq!(errors.len(), 1, "{errors:#?}");
    assert!(errors[0].message.contains("up to 6 images"), "{errors:#?}");
}

#[test]
fn an_unconnected_output_is_an_error_on_the_output_node() {
    let mut graph = NodeGraph::default();
    let errors = errors_of(&graph);
    assert_eq!(errors[0].node, None, "no output at all is the graph's");
    assert!(errors[0].message.contains("Add a Material Output"));

    let output = add(&mut graph, OUTPUT);
    let color = add(&mut graph, "color");
    let errors = errors_of(&graph);
    assert_eq!(errors.len(), 1, "{errors:#?}");
    assert_eq!(errors[0].node, Some(output));
    assert!(
        errors[0]
            .message
            .contains("Nothing is wired into the Material Output")
    );

    wire(&mut graph, (color, "Color"), (output, "Base Color"));
    compiled(&graph);
    let second = add(&mut graph, OUTPUT);
    let errors = errors_of(&graph);
    assert_eq!(errors.len(), 1, "{errors:#?}");
    assert_eq!(
        errors[0].node,
        Some(second),
        "a second output is pointed at"
    );
}

#[test]
fn a_type_mismatch_is_an_error_on_the_node_where_the_types_meet() {
    // A Vector2 plus a Vector3.
    let mut graph = NodeGraph::default();
    let output = add(&mut graph, OUTPUT);
    let (uv, position, sum) = (
        add(&mut graph, "uv"),
        add(&mut graph, "world_position"),
        add(&mut graph, "add"),
    );
    wire(&mut graph, (uv, "UV"), (sum, "A"));
    wire(&mut graph, (position, "Position"), (sum, "B"));
    wire(&mut graph, (sum, "Result"), (output, "Base Color"));
    let errors = errors_of(&graph);
    assert_eq!(errors.len(), 1, "{errors:#?}");
    assert_eq!(errors[0].node, Some(sum));
    assert_eq!(
        errors[0].message,
        "A is a Vector2 but B is a Vector3: they must match, or one be a number."
    );

    // A sum of Vector2s into a color.
    graph.disconnect(&PinRef::new(sum, "B"), false);
    wire(&mut graph, (uv, "UV"), (sum, "B"));
    let errors = errors_of(&graph);
    assert_eq!(errors.len(), 1, "{errors:#?}");
    assert_eq!(errors[0].node, Some(output));
    assert_eq!(
        errors[0].message,
        "Base Color needs a Vector3, but is given a Vector2."
    );

    // And into another node's input.
    let hue = add(&mut graph, "hue_shift");
    wire(&mut graph, (sum, "Result"), (hue, "Color"));
    wire(&mut graph, (hue, "Color"), (output, "Base Color"));
    let errors = errors_of(&graph);
    assert_eq!(errors[0].node, Some(hue), "{errors:#?}");
    assert!(errors[0].message.starts_with("Color needs a Vector3"));

    // A number fills a vector, so a number into a color is fine.
    graph.disconnect(&PinRef::new(hue, "Color"), false);
    let time = add(&mut graph, "time");
    wire(&mut graph, (time, "Seconds"), (hue, "Color"));
    compiled(&graph);
}

#[test]
fn other_mistakes_are_on_their_nodes() {
    let mut graph = NodeGraph::default();
    let output = add(&mut graph, OUTPUT);
    let blend = add(&mut graph, "blend");
    let texture = add(&mut graph, "texture");
    wire(&mut graph, (texture, "Color"), (blend, "Blend"));
    wire(&mut graph, (blend, "Color"), (output, "Base Color"));
    let tiling = add(&mut graph, "tiling_offset");
    set(&mut graph, tiling, "Tiling", "two");
    wire(&mut graph, (tiling, "UV"), (texture, "UV"));
    let (a, b) = (
        add(&mut graph, NUMBER_PARAMETER),
        add(&mut graph, NUMBER_PARAMETER),
    );
    set(&mut graph, b, "Value", "0.2");
    let errors = errors_of(&graph);
    let on = |node: u32| -> Vec<&str> {
        errors
            .iter()
            .filter(|error| error.node == Some(node))
            .map(|error| error.message.as_str())
            .collect()
    };
    assert_eq!(
        on(blend),
        ["Base isn't connected: wire a value in, or type one."]
    );
    assert_eq!(
        on(tiling),
        ["Tiling needs two numbers, such as `1, 0.5`, not `two`."]
    );
    assert!(on(a).is_empty(), "the first Amount is fine: {errors:#?}");
    assert_eq!(
        on(b),
        ["Another parameter is named `Amount`: give this one its own name."]
    );
    // The texture's own error waits until its UV works out.
    set(&mut graph, tiling, "Tiling", "2, 2");
    let errors = errors_of(&graph);
    assert!(
        errors
            .iter()
            .any(|error| error.node == Some(texture) && error.message == "Pick the image it reads."),
        "{errors:#?}"
    );
}

#[test]
fn applying_a_graph_keeps_the_files_occlusion_and_empties_a_bases_unused_slots() {
    let graph = mossy_stone("assets/textures/stone.png", "assets/textures/moss.png");
    let compiled = compiled(&graph);
    let mut file = MaterialFile {
        maps: MaterialMaps {
            color: Some(MapFile::of("assets/textures/old.png")),
            normal: Some(MapFile::of("assets/textures/old_n.png")),
            occlusion: Some(MapFile::of("assets/textures/ao.png")),
            ..MaterialMaps::default()
        },
        ..MaterialFile::default()
    };
    compiled.apply_to(&mut file);
    assert_eq!(file.shader.as_deref(), Some(compiled.shader.as_str()));
    assert_eq!(file.maps.color, compiled.maps.color);
    assert_eq!(
        file.maps.normal, None,
        "the graph's maps replace the file's"
    );
    assert_eq!(
        file.maps.occlusion,
        Some(MapFile::of("assets/textures/ao.png"))
    );
    assert_eq!(file.parameters, compiled.parameters);

    let mut instance = MaterialFile {
        base: Some("assets/materials/base.material.json".into()),
        ..MaterialFile::default()
    };
    compiled.apply_to(&mut instance);
    assert_eq!(
        instance.maps.normal,
        Some(MapFile::of("")),
        "not its base's"
    );
    assert_eq!(
        instance.maps.occlusion, None,
        "occlusion still comes from the base"
    );
}

#[test]
fn a_parameter_set_by_name_changes_its_node_and_a_starter_graph_compiles() {
    let mut graph = mossy_stone("assets/textures/stone.png", "assets/textures/moss.png");
    assert!(set_parameter(
        &mut graph,
        "Moss Tint",
        &ParamValue::Vector(vec![1.0, 0.5, 0.0])
    ));
    assert!(set_parameter(
        &mut graph,
        "Roughness",
        &ParamValue::Number(0.25)
    ));
    assert!(!set_parameter(
        &mut graph,
        "Missing",
        &ParamValue::Number(1.0)
    ));
    let made = compiled(&graph);
    assert_eq!(
        made.parameters["Moss Tint"],
        ParamValue::Vector(vec![1.0, 0.5, 0.0])
    );
    assert_eq!(made.parameters["Roughness"], ParamValue::Number(0.25));
    assert_eq!(graph_of(&graph_value(&graph)).unwrap(), graph);

    let plain = starter_graph(&MaterialFile::default());
    let shader = compiled(&plain).shader;
    assert!(shader.contains("out.base_color = input.color;"), "{shader}");
    let textured = starter_graph(&MaterialFile {
        maps: MaterialMaps {
            color: Some(MapFile::of("assets/textures/brick.png")),
            ..MaterialMaps::default()
        },
        ..MaterialFile::default()
    });
    let made = compiled(&textured);
    assert_eq!(
        made.maps.color,
        Some(MapFile::of("assets/textures/brick.png"))
    );
}
