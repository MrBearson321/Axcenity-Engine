//! Material graphs: nodes that work out a material's surface, compiled to
//! the WGSL `surface` function a material file's `shader` holds
//! (`docs/rendering/models.md`, Surface shaders and Material graphs).
//!
//! The graph is the editor's: the Material tab keeps it in the material
//! file's `graph` and writes what [`compile`] makes of it (the shader, the
//! maps its Texture Sample nodes read, and its parameters) beside it, so the
//! player reads only WGSL. Shader graphs never become Lua, and gameplay
//! graphs never become WGSL; they share only the node-graph editor.
//!
//! Values are numbers, Vector2s and Vector3s (colors, positions and
//! normals). Math nodes take any of them (`Any` pins) and give back what
//! they're given, so a type mismatch (a Vector2 added to a Vector3) is
//! found here, and reported on the node, rather than when wiring. A number
//! fills every part of a vector.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use axcenity_assets::{MapFile, MaterialFile, MaterialMaps, ParamValue};
use axcenity_blocks::node_graph::{
    FieldKind, FieldSpec, GraphNode, NodeGraph, NodeLibrary, NodeSpec, PinRef, PinSpec, PinType,
};
use axcenity_render::SURFACE_PARAMETERS;

use crate::node_graph::{numbers_text, parse_numbers};

#[cfg(test)]
pub(crate) mod tests;

const NUMBER: PinType = PinType::Number;
const VECTOR2: PinType = PinType::Vector2;
const VECTOR3: PinType = PinType::Vector3;
const ANY: PinType = PinType::Any;

const fn field(name: &'static str, kind: FieldKind, default: &'static str) -> FieldSpec {
    FieldSpec {
        name,
        kind,
        default,
    }
}

const fn node(
    kind: &'static str,
    title: &'static str,
    category: &'static str,
    help: &'static str,
    fields: &'static [FieldSpec],
    inputs: &'static [PinSpec],
    outputs: &'static [PinSpec],
) -> NodeSpec {
    NodeSpec {
        kind,
        title,
        category,
        help,
        fields,
        inputs,
        outputs,
    }
}

const fn out(name: &'static str, ty: PinType) -> PinSpec {
    PinSpec::value(name, ty)
}

/// An unwired UV input reads the mesh's texture coordinates.
const UV_IN: PinSpec = PinSpec::value("UV", VECTOR2);
const RESULT: &[PinSpec] = &[out("Result", ANY)];

pub const OUTPUT: &str = "material_output";
const NUMBER_PARAMETER: &str = "number_parameter";
const COLOR_PARAMETER: &str = "color_parameter";

pub const BLEND_MODES: &[&str] = &["Mix", "Multiply", "Screen", "Overlay", "Add"];
pub const NOISE_TYPES: &[&str] = &["Perlin", "Value", "Voronoi"];

/// A and B, B starting at 0, 1 or 2.
const A_B: [&[PinSpec]; 3] = [
    &[
        PinSpec::value("A", ANY).with_default("0"),
        PinSpec::value("B", ANY).with_default("0"),
    ],
    &[
        PinSpec::value("A", ANY).with_default("0"),
        PinSpec::value("B", ANY).with_default("1"),
    ],
    &[
        PinSpec::value("A", ANY).with_default("0"),
        PinSpec::value("B", ANY).with_default("2"),
    ],
];

const fn binary(kind: &'static str, title: &'static str, help: &'static str, b: usize) -> NodeSpec {
    node(kind, title, "Math", help, &[], A_B[b], RESULT)
}

const fn unary(kind: &'static str, title: &'static str, help: &'static str) -> NodeSpec {
    node(kind, title, "Math", help, &[], VALUE_IN, RESULT)
}

const VALUE_IN: &[PinSpec] = &[PinSpec::required("Value", ANY)];

/// Every node a material graph can hold.
pub static SPECS: &[NodeSpec] = &[
    node(
        OUTPUT,
        "Material Output",
        "Output",
        "The surface the graph works out. An input left unwired keeps the material's own value \
         (its Color, Metalness and so on, without maps).",
        &[],
        &[
            PinSpec::value("Base Color", VECTOR3).wired_only(),
            PinSpec::value("Metalness", NUMBER).wired_only(),
            PinSpec::value("Roughness", NUMBER).wired_only(),
            PinSpec::value("Normal", VECTOR3).wired_only(),
            PinSpec::value("Emissive", VECTOR3).wired_only(),
            PinSpec::value("Opacity", NUMBER).wired_only(),
        ],
        &[],
    ),
    node(
        "material_input",
        "Material Input",
        "Inputs",
        "The part's color times the material's, and the material's own values, before maps.",
        &[],
        &[],
        &[
            out("Color", VECTOR3),
            out("Opacity", NUMBER),
            out("Metalness", NUMBER),
            out("Roughness", NUMBER),
            out("Emissive", VECTOR3),
        ],
    ),
    node(
        "uv",
        "UV",
        "Inputs",
        "The mesh's texture coordinates, after the material's Tiling and Offset.",
        &[],
        &[],
        &[out("UV", VECTOR2)],
    ),
    node(
        "tiling_offset",
        "Tiling and Offset",
        "Inputs",
        "Repeats texture coordinates Tiling times as often, then shifts them by Offset.",
        &[],
        &[
            UV_IN,
            PinSpec::value("Tiling", VECTOR2).with_default("1, 1"),
            PinSpec::value("Offset", VECTOR2).with_default("0, 0"),
        ],
        &[out("UV", VECTOR2)],
    ),
    node(
        "world_position",
        "World Position",
        "Inputs",
        "Where the pixel is in the world, in studs.",
        &[],
        &[],
        &[out("Position", VECTOR3)],
    ),
    node(
        "normal",
        "Normal",
        "Inputs",
        "The way the surface faces, in the world, before normal maps.",
        &[],
        &[],
        &[out("Normal", VECTOR3)],
    ),
    node(
        "time",
        "Time",
        "Inputs",
        "Seconds since the game (or the editor) started, for surfaces that move.",
        &[],
        &[],
        &[out("Seconds", NUMBER)],
    ),
    node(
        "texture",
        "Texture Sample",
        "Textures",
        "Reads an image at UV. Read as Color for pictures, Data for masks and other values. \
         A graph can read up to 6 images.",
        &[
            field("Texture", FieldKind::Image, ""),
            field("Read as", FieldKind::Choice(&["Color", "Data"]), "Color"),
        ],
        &[UV_IN],
        &[out("Color", VECTOR3), out("Alpha", NUMBER)],
    ),
    node(
        "number",
        "Number",
        "Values",
        "A number.",
        &[field("Value", FieldKind::Text, "1")],
        &[],
        &[out("Value", NUMBER)],
    ),
    node(
        "color",
        "Color",
        "Values",
        "A color, as picked (sRGB, like a part's Color).",
        &[field("Color", FieldKind::Color, "1, 1, 1")],
        &[],
        &[out("Color", VECTOR3)],
    ),
    node(
        "make_vector2",
        "Make Vector2",
        "Values",
        "Two numbers as a Vector2.",
        &[],
        &[
            PinSpec::value("X", NUMBER).with_default("0"),
            PinSpec::value("Y", NUMBER).with_default("0"),
        ],
        &[out("Vector", VECTOR2)],
    ),
    node(
        "make_vector3",
        "Make Vector3",
        "Values",
        "Three numbers as a Vector3 (or a color).",
        &[],
        &[
            PinSpec::value("X", NUMBER).with_default("0"),
            PinSpec::value("Y", NUMBER).with_default("0"),
            PinSpec::value("Z", NUMBER).with_default("0"),
        ],
        &[out("Vector", VECTOR3)],
    ),
    node(
        "split",
        "Split",
        "Values",
        "A vector's numbers (a color's red, green and blue). A Vector2's Z is 0.",
        &[],
        &[PinSpec::required("Vector", ANY)],
        &[out("X", NUMBER), out("Y", NUMBER), out("Z", NUMBER)],
    ),
    binary("add", "Add", "A + B.", 0),
    binary("subtract", "Subtract", "A − B.", 0),
    binary("multiply", "Multiply", "A × B.", 1),
    binary("divide", "Divide", "A ÷ B.", 1),
    binary("power", "Power", "A to the power of B.", 2),
    binary("minimum", "Minimum", "The smaller of A and B.", 0),
    binary("maximum", "Maximum", "The larger of A and B.", 0),
    node(
        "mix",
        "Mix",
        "Math",
        "A at Factor 0, B at 1, and in between.",
        &[],
        &[
            PinSpec::required("A", ANY),
            PinSpec::required("B", ANY),
            PinSpec::value("Factor", NUMBER).with_default("0.5"),
        ],
        RESULT,
    ),
    unary("one_minus", "One Minus", "1 − Value."),
    unary("sine", "Sine", "The sine of Value (in radians)."),
    unary("absolute", "Absolute", "Value without its minus sign."),
    unary("fraction", "Fraction", "What's after the decimal point."),
    unary("saturate", "Clamp 0 to 1", "Value kept between 0 and 1."),
    node(
        "smooth_step",
        "Smooth Step",
        "Math",
        "0 below Edge 0, 1 above Edge 1, and a smooth rise between: a soft mask.",
        &[],
        &[
            PinSpec::value("Edge 0", NUMBER).with_default("0"),
            PinSpec::value("Edge 1", NUMBER).with_default("1"),
            PinSpec::required("Value", ANY),
        ],
        RESULT,
    ),
    node(
        "blend",
        "Blend",
        "Color",
        "Blend over Base, as a paint program's layer: by Factor (0 is Base alone).",
        &[field("Mode", FieldKind::Choice(BLEND_MODES), "Mix")],
        &[
            PinSpec::required("Base", VECTOR3),
            PinSpec::required("Blend", VECTOR3),
            PinSpec::value("Factor", NUMBER).with_default("1"),
        ],
        &[out("Color", VECTOR3)],
    ),
    node(
        "hue_shift",
        "Hue Shift",
        "Color",
        "Turns the color's hue around the color wheel by Shift (1 is all the way around).",
        &[],
        &[
            PinSpec::required("Color", VECTOR3),
            PinSpec::value("Shift", NUMBER).with_default("0.1"),
        ],
        &[out("Color", VECTOR3)],
    ),
    node(
        "contrast",
        "Contrast",
        "Color",
        "Pushes the color away from mid-grey (above 1) or toward it (below 1).",
        &[],
        &[
            PinSpec::required("Color", VECTOR3),
            PinSpec::value("Amount", NUMBER).with_default("1.5"),
        ],
        &[out("Color", VECTOR3)],
    ),
    node(
        "noise",
        "Noise",
        "Noise",
        "A pattern from 0 to 1 over UV, Scale cells across: Perlin is smooth and cloudy, Value \
         blockier, Voronoi the distance to the nearest of scattered points (cells and pebbles).",
        &[field("Type", FieldKind::Choice(NOISE_TYPES), "Perlin")],
        &[UV_IN, PinSpec::value("Scale", NUMBER).with_default("8")],
        &[out("Value", NUMBER)],
    ),
    node(
        NUMBER_PARAMETER,
        "Number Parameter",
        "Parameters",
        "A number the material shows by Name, which instances of it can change.",
        &[
            field("Name", FieldKind::Text, "Amount"),
            field("Value", FieldKind::Text, "0.5"),
        ],
        &[],
        &[out("Value", NUMBER)],
    ),
    node(
        COLOR_PARAMETER,
        "Color Parameter",
        "Parameters",
        "A color the material shows by Name, which instances of it can change.",
        &[
            field("Name", FieldKind::Text, "Tint"),
            field("Color", FieldKind::Color, "1, 1, 1"),
        ],
        &[],
        &[out("Color", VECTOR3)],
    ),
];

/// The nodes material graphs hold.
pub struct MaterialLibrary;

impl NodeLibrary for MaterialLibrary {
    fn specs(&self) -> &[NodeSpec] {
        SPECS
    }
}

/// What a graph compiles to: what goes in its material file.
#[derive(Clone, Debug, PartialEq)]
pub struct CompiledGraph {
    /// The WGSL `surface` function (and the helpers it calls).
    pub shader: String,
    /// The images its Texture Sample nodes read, in the map slots the
    /// shader reads them from.
    pub maps: MaterialMaps,
    /// Its parameters by name, with the values its nodes give them.
    pub parameters: BTreeMap<String, ParamValue>,
}

impl CompiledGraph {
    /// Puts what the graph made into `file`: its shader, its parameters,
    /// and its maps in place of the file's, but for the occlusion and
    /// specular maps (which the shader doesn't read, so they're the
    /// file's). The slots it leaves free are emptied of a base's maps.
    pub fn apply_to(&self, file: &mut MaterialFile) {
        let mut maps = self.maps.clone();
        maps.occlusion = file.maps.occlusion.clone();
        maps.specular = file.maps.specular.clone();
        if file.base.as_deref().is_some_and(|base| !base.is_empty()) {
            for slot in SLOTS {
                let map = slot.of(&mut maps);
                if map.is_none() {
                    *map = Some(MapFile::of(""));
                }
            }
        }
        file.maps = maps;
        file.shader = Some(self.shader.clone());
        file.parameters = self.parameters.clone();
    }
}

/// Why a graph doesn't compile, on a node or (`None`) on the whole graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphError {
    pub node: Option<u32>,
    pub message: String,
}

/// The map slots a graph's images go in, and what the shader calls each.
/// Color and glow maps are read through sRGB views (their samples are
/// linear light), the rest as plain values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Slot {
    Color,
    Emissive,
    Metalness,
    Roughness,
    Opacity,
    Normal,
}

const SLOTS: [Slot; 6] = [
    Slot::Color,
    Slot::Emissive,
    Slot::Metalness,
    Slot::Roughness,
    Slot::Opacity,
    Slot::Normal,
];

impl Slot {
    fn texture(self) -> &'static str {
        match self {
            Slot::Color => "base_color_texture",
            Slot::Emissive => "emissive_texture",
            Slot::Metalness => "metalness_texture",
            Slot::Roughness => "roughness_texture",
            Slot::Opacity => "opacity_texture",
            Slot::Normal => "normal_texture",
        }
    }

    fn srgb(self) -> bool {
        matches!(self, Slot::Color | Slot::Emissive)
    }

    fn of(self, maps: &mut MaterialMaps) -> &mut Option<MapFile> {
        match self {
            Slot::Color => &mut maps.color,
            Slot::Emissive => &mut maps.emissive,
            Slot::Metalness => &mut maps.metalness,
            Slot::Roughness => &mut maps.roughness,
            Slot::Opacity => &mut maps.opacity,
            Slot::Normal => &mut maps.normal,
        }
    }
}

/// A value's type in the shader.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ty {
    Number,
    Vector2,
    Vector3,
}

impl Ty {
    fn of_pin(ty: PinType) -> Option<Ty> {
        match ty {
            PinType::Number => Some(Ty::Number),
            PinType::Vector2 => Some(Ty::Vector2),
            PinType::Vector3 => Some(Ty::Vector3),
            _ => None,
        }
    }

    fn of_size(size: usize) -> Option<Ty> {
        match size {
            1 => Some(Ty::Number),
            2 => Some(Ty::Vector2),
            3 => Some(Ty::Vector3),
            _ => None,
        }
    }

    fn wgsl(self) -> &'static str {
        match self {
            Ty::Number => "f32",
            Ty::Vector2 => "vec2<f32>",
            Ty::Vector3 => "vec3<f32>",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Ty::Number => "a number",
            Ty::Vector2 => "a Vector2",
            Ty::Vector3 => "a Vector3",
        }
    }

    /// `code` (a number) as this type: a number fills every part.
    fn splat(self, code: &str) -> String {
        match self {
            Ty::Number => code.to_owned(),
            ty => format!("{}({code})", ty.wgsl()),
        }
    }
}

/// A WGSL expression and its type.
#[derive(Clone, Debug, PartialEq)]
struct Value {
    code: String,
    ty: Ty,
}

impl Value {
    fn new(code: impl Into<String>, ty: Ty) -> Self {
        Self {
            code: code.into(),
            ty,
        }
    }

    /// This value as `ty`, if it is one or is a number.
    fn as_ty(&self, ty: Ty) -> Option<String> {
        if self.ty == ty {
            Some(self.code.clone())
        } else if self.ty == Ty::Number {
            Some(ty.splat(&self.code))
        } else {
            None
        }
    }
}

/// A number as WGSL writes an `f32` (`1.0f`: without the `f` it's an
/// abstract number, which a call of nothing but numbers can't be typed by).
fn literal(number: f32) -> String {
    format!("{number:?}f")
}

fn vector_literal(numbers: &[f32]) -> Value {
    let ty = Ty::of_size(numbers.len()).expect("one to three numbers");
    match ty {
        Ty::Number => Value::new(literal(numbers[0]), ty),
        _ => {
            let parts: Vec<String> = numbers.iter().map(|&n| literal(n)).collect();
            Value::new(format!("{}({})", ty.wgsl(), parts.join(", ")), ty)
        }
    }
}

/// The functions nodes call, by name, each with the ones it calls first.
const HELPERS: &[(&str, &[&str], &str)] = &[
    (
        "graph_hash",
        &[],
        "// A random number from 0 to 1 for each point (cell corner).
fn graph_hash(p: vec2<f32>) -> f32 {
    var q = fract(p * vec2<f32>(123.34, 456.21));
    q = q + dot(q, q + 45.32);
    return fract(q.x * q.y);
}",
    ),
    (
        "graph_value_noise",
        &["graph_hash"],
        "// Random values at cell corners, blended smoothly between them.
fn graph_value_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = graph_hash(i);
    let b = graph_hash(i + vec2<f32>(1.0, 0.0));
    let c = graph_hash(i + vec2<f32>(0.0, 1.0));
    let d = graph_hash(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}",
    ),
    (
        "graph_perlin_noise",
        &["graph_hash"],
        "// Random slopes at cell corners (Perlin's gradient noise), 0 to 1.
fn graph_corner_slope(corner: vec2<f32>, offset: vec2<f32>) -> f32 {
    let angle = graph_hash(corner) * 6.2831853;
    return dot(vec2<f32>(cos(angle), sin(angle)), offset);
}

fn graph_perlin_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    let a = graph_corner_slope(i, f);
    let b = graph_corner_slope(i + vec2<f32>(1.0, 0.0), f - vec2<f32>(1.0, 0.0));
    let c = graph_corner_slope(i + vec2<f32>(0.0, 1.0), f - vec2<f32>(0.0, 1.0));
    let d = graph_corner_slope(i + vec2<f32>(1.0, 1.0), f - vec2<f32>(1.0, 1.0));
    return clamp(mix(mix(a, b, u.x), mix(c, d, u.x), u.y) * 0.7071 + 0.5, 0.0, 1.0);
}",
    ),
    (
        "graph_voronoi_noise",
        &["graph_hash"],
        "// The distance to the nearest of one random point per cell, 0 to 1.
fn graph_voronoi_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    var nearest = 1.0;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let cell = vec2<f32>(f32(x), f32(y));
            let h = graph_hash(i + cell);
            let point = vec2<f32>(h, graph_hash(i + cell + h + 17.0));
            nearest = min(nearest, length(cell + point - f));
        }
    }
    return nearest;
}",
    ),
    (
        "graph_hue_shift",
        &[],
        "// Turns a color around the grey axis: its hue, by `turns` of the wheel.
fn graph_hue_shift(color: vec3<f32>, turns: f32) -> vec3<f32> {
    let axis = vec3<f32>(0.57735027);
    let angle = turns * 6.2831853;
    let c = cos(angle);
    return color * c + cross(axis, color) * sin(angle) + axis * dot(axis, color) * (1.0 - c);
}",
    ),
    (
        "graph_overlay",
        &[],
        "// A paint program's Overlay: multiply in the darks, screen in the lights.
fn graph_overlay(base: vec3<f32>, blend: vec3<f32>) -> vec3<f32> {
    let dark = 2.0 * base * blend;
    let light = 1.0 - 2.0 * (1.0 - base) * (1.0 - blend);
    return select(light, dark, base < vec3<f32>(0.5));
}",
    ),
    (
        "graph_encode_srgb",
        &[],
        "// Linear light back to sRGB, for an image read as data through a color slot.
fn graph_encode_srgb(color: vec3<f32>) -> vec3<f32> {
    let c = max(color, vec3<f32>(0.0));
    return select(1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055, c * 12.92,
        c <= vec3<f32>(0.0031308));
}",
    ),
];

/// A parameter node's name, type and value.
struct Parameter {
    node: u32,
    name: String,
    value: ParamValue,
}

struct Compiler<'a> {
    graph: &'a NodeGraph,
    /// The body's statements.
    lines: Vec<String>,
    /// Each node's outputs, in its spec's order, once worked out (`None`
    /// when it has an error).
    done: HashMap<u32, Option<Vec<Value>>>,
    /// The nodes being worked out, to find a value depending on itself.
    visiting: BTreeSet<u32>,
    errors: Vec<GraphError>,
    /// The images read: path, read as color, and the slot.
    textures: Vec<(String, bool, Slot)>,
    /// Each parameter's slot, by name.
    parameters: BTreeMap<String, (usize, ParamValue)>,
    helpers: BTreeSet<&'static str>,
}

/// Compiles `graph` to a material's surface shader, or says what's wrong
/// with it, on the nodes where it's wrong.
pub fn compile(graph: &NodeGraph) -> Result<CompiledGraph, Vec<GraphError>> {
    let mut compiler = Compiler {
        graph,
        lines: Vec::new(),
        done: HashMap::new(),
        visiting: BTreeSet::new(),
        errors: Vec::new(),
        textures: Vec::new(),
        parameters: BTreeMap::new(),
        helpers: BTreeSet::new(),
    };
    for node in &graph.nodes {
        if MaterialLibrary.spec(&node.kind).is_none() {
            compiler.error(
                Some(node.id),
                format!("A material graph has no `{}` node.", node.kind),
            );
        }
    }
    compiler.collect_parameters();
    let outputs: Vec<&GraphNode> = graph.nodes.iter().filter(|n| n.kind == OUTPUT).collect();
    let Some(output) = outputs.first() else {
        compiler.error(
            None,
            "Add a Material Output node and wire into it: it's the surface the graph works out.",
        );
        return Err(compiler.errors);
    };
    for extra in &outputs[1..] {
        compiler.error(
            Some(extra.id),
            "Only one Material Output is used: delete this one.",
        );
    }
    let surface = compiler.output(output);
    match surface {
        Some(body) if compiler.errors.is_empty() => Ok(compiler.finish(body)),
        _ => Err(compiler.errors),
    }
}

impl Compiler<'_> {
    fn error(&mut self, node: Option<u32>, message: impl Into<String>) {
        let message = message.into();
        if !self
            .errors
            .iter()
            .any(|error| error.node == node && error.message == message)
        {
            self.errors.push(GraphError { node, message });
        }
    }

    /// Every parameter node's slot, by name order, so they're the same
    /// whatever is wired.
    fn collect_parameters(&mut self) {
        let mut found: Vec<Parameter> = Vec::new();
        for node in &self.graph.nodes {
            let value = match node.kind.as_str() {
                NUMBER_PARAMETER => match node.value("Value").trim().parse::<f32>() {
                    Ok(number) if number.is_finite() => ParamValue::Number(number),
                    _ => {
                        self.error(
                            Some(node.id),
                            format!("Value isn't a number: `{}`.", node.value("Value")),
                        );
                        continue;
                    }
                },
                COLOR_PARAMETER => match parse_numbers::<3>(node.value("Color")) {
                    Some(color) => ParamValue::Vector(color.to_vec()),
                    None => {
                        self.error(Some(node.id), "Color isn't a color: pick one.");
                        continue;
                    }
                },
                _ => continue,
            };
            let name = node.value("Name").trim().to_owned();
            if name.is_empty() {
                self.error(Some(node.id), "Give the parameter a Name.");
                continue;
            }
            if found.iter().any(|other| other.name == name) {
                self.error(
                    Some(node.id),
                    format!("Another parameter is named `{name}`: give this one its own name."),
                );
                continue;
            }
            found.push(Parameter {
                node: node.id,
                name,
                value,
            });
        }
        found.sort_by(|a, b| a.name.cmp(&b.name));
        for (slot, parameter) in found.into_iter().enumerate() {
            if slot >= SURFACE_PARAMETERS {
                self.error(
                    Some(parameter.node),
                    format!("A material can have up to {SURFACE_PARAMETERS} parameters."),
                );
                continue;
            }
            self.parameters
                .insert(parameter.name, (slot, parameter.value));
        }
    }

    /// The Material Output's assignments, or `None` with an error.
    fn output(&mut self, node: &GraphNode) -> Option<Vec<String>> {
        let spec = MaterialLibrary.spec(OUTPUT).expect("the output is a node");
        let wired = spec
            .inputs
            .iter()
            .any(|pin| self.wire_into(node.id, pin.name).is_some());
        if !wired {
            self.error(
                Some(node.id),
                "Nothing is wired into the Material Output: wire in what the surface should be, \
                 such as its Base Color.",
            );
            return None;
        }
        let fields = [
            ("Base Color", "base_color", "input.color"),
            ("Metalness", "metalness", "input.metalness"),
            ("Roughness", "roughness", "input.roughness"),
            ("Normal", "normal", "input.normal"),
            ("Emissive", "emissive", "input.emissive"),
            ("Opacity", "opacity", "input.opacity"),
        ];
        let mut body = Vec::new();
        let mut failed = false;
        for (pin, field, own) in fields {
            let pin_spec = spec.input(pin).expect("the output has this pin");
            let wired = self.wire_into(node.id, pin).is_some();
            let typed = !node.value(pin).trim().is_empty();
            let code = if wired || typed {
                match self.input(node, pin_spec) {
                    Some(code) if field == "normal" => format!("normalize({code})"),
                    Some(code) => code,
                    None => {
                        failed = true;
                        continue;
                    }
                }
            } else {
                own.to_owned()
            };
            body.push(format!("    out.{field} = {code};"));
        }
        (!failed).then_some(body)
    }

    fn wire_into(&self, node: u32, pin: &str) -> Option<&PinRef> {
        let to = PinRef::new(node, pin);
        self.graph
            .wires
            .iter()
            .find(|wire| wire.to == to)
            .map(|wire| &wire.from)
    }

    /// Input `pin` of `node` as code of the pin's type, or `None` with an
    /// error (an `Any` pin's value is given as it is: see [`Self::value_of`]).
    fn input(&mut self, node: &GraphNode, pin: &PinSpec) -> Option<String> {
        let value = self.value_of(node, pin)?;
        let Some(ty) = Ty::of_pin(pin.ty) else {
            return Some(value.code);
        };
        match value.as_ty(ty) {
            Some(code) => Some(code),
            None => {
                self.error(
                    Some(node.id),
                    format!(
                        "{} needs {}, but is given {}.",
                        pin.name,
                        ty.name(),
                        value.ty.name()
                    ),
                );
                None
            }
        }
    }

    /// Input `pin` of `node`: what's wired in, or what's typed, or its
    /// default. An unwired UV reads the mesh's coordinates.
    fn value_of(&mut self, node: &GraphNode, pin: &PinSpec) -> Option<Value> {
        if let Some(from) = self.wire_into(node.id, pin.name).cloned() {
            let source = self.graph.node(from.node)?;
            let spec = MaterialLibrary.spec(&source.kind)?;
            let index = spec.outputs.iter().position(|out| out.name == from.pin)?;
            let source = source.clone();
            let outputs = self.node_outputs(&source)?;
            return outputs.get(index).cloned();
        }
        let typed = node.value(pin.name).trim().to_owned();
        let text = if typed.is_empty() {
            if pin.required {
                self.error(
                    Some(node.id),
                    format!(
                        "{} isn't connected: wire a value in, or type one.",
                        pin.name
                    ),
                );
                return None;
            }
            if pin.ty == PinType::Vector2 && pin.name == "UV" {
                return Some(Value::new("input.uv", Ty::Vector2));
            }
            if pin.default.is_empty() {
                "0".to_owned()
            } else {
                pin.default.to_owned()
            }
        } else {
            typed
        };
        let parsed = match pin.ty {
            PinType::Number => parse_numbers::<1>(&text).map(|n| n.to_vec()),
            PinType::Vector2 => parse_numbers::<2>(&text).map(|n| n.to_vec()),
            PinType::Vector3 => parse_numbers::<3>(&text).map(|n| n.to_vec()),
            _ => {
                let numbers: Option<Vec<f32>> = text
                    .split(',')
                    .map(|part| part.trim().parse::<f32>().ok().filter(|n| n.is_finite()))
                    .collect();
                numbers.filter(|numbers| (1..=3).contains(&numbers.len()))
            }
        };
        match parsed {
            Some(numbers) => Some(vector_literal(&numbers)),
            None => {
                let wanted = match pin.ty {
                    PinType::Number => "a number",
                    PinType::Vector2 => "two numbers, such as `1, 0.5`",
                    PinType::Vector3 => "three numbers, such as `1, 0.5, 0`",
                    _ => "one to three numbers, such as `0.5` or `1, 0.5, 0`",
                };
                self.error(
                    Some(node.id),
                    format!("{} needs {wanted}, not `{text}`.", pin.name),
                );
                None
            }
        }
    }

    /// `node`'s outputs, worked out once.
    fn node_outputs(&mut self, node: &GraphNode) -> Option<Vec<Value>> {
        if let Some(done) = self.done.get(&node.id) {
            return done.clone();
        }
        if !self.visiting.insert(node.id) {
            self.error(
                Some(node.id),
                "This node's value depends on itself: break the loop.",
            );
            return None;
        }
        let outputs = self.work_out(node);
        self.visiting.remove(&node.id);
        self.done.insert(node.id, outputs.clone());
        outputs
    }

    /// Adds `let n<id> = code;` and gives back its name.
    fn bind(&mut self, node: &GraphNode, title: &str, code: String) -> String {
        let name = format!("n{}", node.id);
        self.lines
            .push(format!("    let {name} = {code}; // {title}"));
        name
    }

    fn helper(&mut self, name: &'static str) {
        self.helpers.insert(name);
    }

    /// Two values of one type: equal types, or a number filling a vector.
    fn matched(&mut self, node: &GraphNode, a: &Value, b: &Value) -> Option<(String, String, Ty)> {
        let ty = if a.ty == Ty::Number { b.ty } else { a.ty };
        match (a.as_ty(ty), b.as_ty(ty)) {
            (Some(a), Some(b)) => Some((a, b, ty)),
            _ => {
                self.error(
                    Some(node.id),
                    format!(
                        "A is {} but B is {}: they must match, or one be a number.",
                        a.ty.name(),
                        b.ty.name()
                    ),
                );
                None
            }
        }
    }

    /// The node's input values, in its spec's order.
    fn inputs(&mut self, node: &GraphNode, spec: &NodeSpec) -> Option<Vec<Value>> {
        let mut values = Vec::new();
        let mut failed = false;
        for pin in spec.inputs {
            match self.value_of(node, pin) {
                Some(value) => match Ty::of_pin(pin.ty) {
                    Some(ty) => match value.as_ty(ty) {
                        Some(code) => values.push(Value::new(code, ty)),
                        None => {
                            self.error(
                                Some(node.id),
                                format!(
                                    "{} needs {}, but is given {}.",
                                    pin.name,
                                    ty.name(),
                                    value.ty.name()
                                ),
                            );
                            failed = true;
                        }
                    },
                    None => values.push(value),
                },
                None => failed = true,
            }
        }
        (!failed).then_some(values)
    }

    fn work_out(&mut self, node: &GraphNode) -> Option<Vec<Value>> {
        let spec = MaterialLibrary.spec(&node.kind)?;
        let title = spec.title;
        let inputs = self.inputs(node, spec)?;
        let code = |i: usize| inputs[i].code.clone();
        let one = |name: String, ty| Some(vec![Value::new(name, ty)]);
        match spec.kind {
            "material_input" => Some(vec![
                Value::new("input.color", Ty::Vector3),
                Value::new("input.opacity", Ty::Number),
                Value::new("input.metalness", Ty::Number),
                Value::new("input.roughness", Ty::Number),
                Value::new("input.emissive", Ty::Vector3),
            ]),
            "uv" => one("input.uv".into(), Ty::Vector2),
            "world_position" => one("input.world_position".into(), Ty::Vector3),
            "normal" => one("input.normal".into(), Ty::Vector3),
            "time" => one("input.time".into(), Ty::Number),
            "tiling_offset" => {
                let name = self.bind(
                    node,
                    title,
                    format!("{} * {} + {}", code(0), code(1), code(2)),
                );
                one(name, Ty::Vector2)
            }
            "texture" => self.texture(node, title, &code(0)),
            "number" => match node.value("Value").trim().parse::<f32>() {
                Ok(number) if number.is_finite() => one(literal(number), Ty::Number),
                _ => {
                    self.error(
                        Some(node.id),
                        format!("Value isn't a number: `{}`.", node.value("Value")),
                    );
                    None
                }
            },
            "color" => match parse_numbers::<3>(node.value("Color")) {
                Some(color) => one(
                    format!("srgb({})", vector_literal(&color).code),
                    Ty::Vector3,
                ),
                None => {
                    self.error(Some(node.id), "Color isn't a color: pick one.");
                    None
                }
            },
            "make_vector2" => one(format!("vec2<f32>({}, {})", code(0), code(1)), Ty::Vector2),
            "make_vector3" => one(
                format!("vec3<f32>({}, {}, {})", code(0), code(1), code(2)),
                Ty::Vector3,
            ),
            "split" => {
                let vector = &inputs[0];
                let name = self.bind(node, title, vector.code.clone());
                let part = |axis: &str| Value::new(format!("{name}.{axis}"), Ty::Number);
                Some(match vector.ty {
                    Ty::Number => vec![Value::new(name.clone(), Ty::Number); 3],
                    Ty::Vector2 => vec![part("x"), part("y"), Value::new("0.0f", Ty::Number)],
                    Ty::Vector3 => vec![part("x"), part("y"), part("z")],
                })
            }
            "add" | "subtract" | "multiply" | "divide" | "power" | "minimum" | "maximum" => {
                let (a, b, ty) = self.matched(node, &inputs[0], &inputs[1])?;
                let expression = match spec.kind {
                    "add" => format!("{a} + {b}"),
                    "subtract" => format!("{a} - {b}"),
                    "multiply" => format!("{a} * {b}"),
                    "divide" => format!("{a} / {b}"),
                    "power" => format!("pow({a}, {b})"),
                    "minimum" => format!("min({a}, {b})"),
                    _ => format!("max({a}, {b})"),
                };
                let name = self.bind(node, title, expression);
                one(name, ty)
            }
            "mix" => {
                let (a, b, ty) = self.matched(node, &inputs[0], &inputs[1])?;
                let factor = ty.splat(&code(2));
                let name = self.bind(node, title, format!("mix({a}, {b}, {factor})"));
                one(name, ty)
            }
            "one_minus" | "sine" | "absolute" | "fraction" | "saturate" => {
                let value = &inputs[0];
                let expression = match spec.kind {
                    "one_minus" => format!("{} - {}", value.ty.splat("1.0f"), value.code),
                    "sine" => format!("sin({})", value.code),
                    "absolute" => format!("abs({})", value.code),
                    "fraction" => format!("fract({})", value.code),
                    _ => format!("saturate({})", value.code),
                };
                let ty = value.ty;
                let name = self.bind(node, title, expression);
                one(name, ty)
            }
            "smooth_step" => {
                let value = &inputs[2];
                let ty = value.ty;
                let expression = format!(
                    "smoothstep({}, {}, {})",
                    ty.splat(&code(0)),
                    ty.splat(&code(1)),
                    value.code
                );
                let name = self.bind(node, title, expression);
                one(name, ty)
            }
            "blend" => {
                let (base, blend, factor) = (code(0), code(1), code(2));
                let blended = match node.value("Mode") {
                    "Multiply" => format!("{base} * {blend}"),
                    "Screen" => format!("1.0 - (1.0 - {base}) * (1.0 - {blend})"),
                    "Overlay" => {
                        self.helper("graph_overlay");
                        format!("graph_overlay({base}, {blend})")
                    }
                    "Add" => format!("{base} + {blend}"),
                    _ => blend.clone(),
                };
                let expression = format!("mix({base}, {blended}, vec3<f32>({factor}))");
                let name = self.bind(node, title, expression);
                one(name, Ty::Vector3)
            }
            "hue_shift" => {
                self.helper("graph_hue_shift");
                let name = self.bind(
                    node,
                    title,
                    format!("graph_hue_shift({}, {})", code(0), code(1)),
                );
                one(name, Ty::Vector3)
            }
            "contrast" => {
                // Around mid-grey (sRGB 0.5), in linear light.
                let expression = format!(
                    "max(({} - vec3<f32>(0.214)) * {} + vec3<f32>(0.214), vec3<f32>(0.0))",
                    code(0),
                    code(1)
                );
                let name = self.bind(node, title, expression);
                one(name, Ty::Vector3)
            }
            "noise" => {
                let function = match node.value("Type") {
                    "Value" => "graph_value_noise",
                    "Voronoi" => "graph_voronoi_noise",
                    _ => "graph_perlin_noise",
                };
                self.helper(function);
                let name = self.bind(
                    node,
                    title,
                    format!("{function}({} * {})", code(0), code(1)),
                );
                one(name, Ty::Number)
            }
            NUMBER_PARAMETER | COLOR_PARAMETER => {
                let name = node.value("Name").trim();
                let (slot, _) = self.parameters.get(name)?;
                if node.kind == NUMBER_PARAMETER {
                    one(format!("parameter({slot}).x"), Ty::Number)
                } else {
                    one(format!("srgb(parameter({slot}).xyz)"), Ty::Vector3)
                }
            }
            _ => None,
        }
    }

    /// A Texture Sample's outputs: its image in a slot, read at `uv`.
    fn texture(&mut self, node: &GraphNode, title: &str, uv: &str) -> Option<Vec<Value>> {
        let path = node.value("Texture").trim().to_owned();
        if path.is_empty() {
            self.error(Some(node.id), "Pick the image it reads.");
            return None;
        }
        let color = node.value("Read as") != "Data";
        let slot = match self
            .textures
            .iter()
            .find(|(known, as_color, _)| *known == path && *as_color == color)
        {
            Some((_, _, slot)) => *slot,
            None => {
                let free = |slot: &Slot| !self.textures.iter().any(|(_, _, used)| used == slot);
                // Colors go where they're read as linear light, and values
                // where they're read as they are, while there's room.
                let slot = SLOTS
                    .iter()
                    .filter(|slot| free(slot))
                    .find(|slot| slot.srgb() == color)
                    .or_else(|| SLOTS.iter().find(|slot| free(slot)))
                    .copied();
                let Some(slot) = slot else {
                    self.error(
                        Some(node.id),
                        format!("A material graph can read up to {} images.", SLOTS.len()),
                    );
                    return None;
                };
                self.textures.push((path, color, slot));
                slot
            }
        };
        let name = self.bind(
            node,
            title,
            format!("textureSample({}, material_sampler, {uv})", slot.texture()),
        );
        let rgb = match (color, slot.srgb()) {
            (true, false) => format!("srgb({name}.rgb)"),
            (false, true) => {
                self.helper("graph_encode_srgb");
                format!("graph_encode_srgb({name}.rgb)")
            }
            _ => format!("{name}.rgb"),
        };
        Some(vec![
            Value::new(rgb, Ty::Vector3),
            Value::new(format!("{name}.a"), Ty::Number),
        ])
    }

    fn finish(self, body: Vec<String>) -> CompiledGraph {
        let mut shader = String::from(
            "// Made by this material's graph (the Material tab's Graph view): edit the graph,\n\
             // not this code, which is made again from it.\n\
             fn surface(input: SurfaceInput) -> Surface {\n",
        );
        for line in self
            .lines
            .iter()
            .chain(&[String::from("    var out: Surface;")])
            .chain(&body)
        {
            shader.push_str(line);
            shader.push('\n');
        }
        shader.push_str("    return out;\n}\n");
        // Each helper once, after those it calls.
        let mut wanted: BTreeSet<&str> = BTreeSet::new();
        for name in &self.helpers {
            wanted.insert(*name);
            if let Some((_, needs, _)) = HELPERS.iter().find(|(helper, ..)| helper == name) {
                wanted.extend(needs.iter().copied());
            }
        }
        for (name, _, code) in HELPERS {
            if wanted.contains(name) {
                shader.push('\n');
                shader.push_str(code);
                shader.push('\n');
            }
        }
        let mut maps = MaterialMaps::default();
        for (path, _, slot) in &self.textures {
            *slot.of(&mut maps) = Some(MapFile::of(path.clone()));
        }
        let parameters = self
            .parameters
            .into_iter()
            .map(|(name, (_, value))| (name, value))
            .collect();
        CompiledGraph {
            shader,
            maps,
            parameters,
        }
    }
}

/// The graph a material that has none starts with: its own color (times
/// its color map, if it has one) into the Material Output.
pub fn starter_graph(file: &MaterialFile) -> NodeGraph {
    let library = MaterialLibrary;
    let spec = |kind| library.spec(kind).expect("a material node");
    let mut graph = NodeGraph::default();
    let input = graph.add_node(spec("material_input"), [40.0, 60.0]);
    let output = graph.add_node(spec(OUTPUT), [560.0, 60.0]);
    let color_map = file
        .maps
        .color
        .as_ref()
        .and_then(|map| map.path.clone())
        .filter(|path| !path.is_empty());
    let color = match color_map {
        Some(path) => {
            let texture = graph.add_node(spec("texture"), [40.0, 260.0]);
            if let Some(node) = graph.node_mut(texture) {
                node.values.insert("Texture".into(), path);
            }
            let multiply = graph.add_node(spec("multiply"), [320.0, 120.0]);
            let wire = |graph: &mut NodeGraph, from: (u32, &str), to: (u32, &str)| {
                let _ = graph.connect(
                    &library,
                    PinRef::new(from.0, from.1),
                    PinRef::new(to.0, to.1),
                );
            };
            wire(&mut graph, (input, "Color"), (multiply, "A"));
            wire(&mut graph, (texture, "Color"), (multiply, "B"));
            PinRef::new(multiply, "Result")
        }
        None => PinRef::new(input, "Color"),
    };
    let _ = graph.connect(&library, color, PinRef::new(output, "Base Color"));
    graph
}

/// The graph in a material file's `graph`, or why it can't be read.
pub fn graph_of(value: &serde_json::Value) -> Result<NodeGraph, String> {
    serde_json::from_value(value.clone()).map_err(|err| err.to_string())
}

/// `graph` as a material file keeps it.
pub fn graph_value(graph: &NodeGraph) -> serde_json::Value {
    serde_json::to_value(graph).expect("a graph is plain values, which always serialize")
}

/// Sets the parameter `name`'s value on its node in `graph`. False when no
/// parameter node has that name.
pub fn set_parameter(graph: &mut NodeGraph, name: &str, value: &ParamValue) -> bool {
    let Some(node) = graph.nodes.iter_mut().find(|node| {
        matches!(node.kind.as_str(), NUMBER_PARAMETER | COLOR_PARAMETER)
            && node.value("Name").trim() == name
    }) else {
        return false;
    };
    let (field, text) = match value {
        ParamValue::Number(number) => ("Value", format!("{number}")),
        ParamValue::Vector(numbers) => ("Color", numbers_text(numbers)),
    };
    node.values.insert(field.to_owned(), text);
    true
}
