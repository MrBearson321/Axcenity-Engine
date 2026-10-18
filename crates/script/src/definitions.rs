//! Writes the Lua Language Server definitions file: `---@class` and
//! `---@field` annotations describing the whole script API, so editors such
//! as VS Code can autocomplete it and point out mistakes.
//!
//! - Classes, properties and enums come straight from `core`, so they can't
//!   fall out of date. A new property only needs a line in `PROPERTY_DOCS`.
//! - Functions, globals and value types are described by hand below.
//! - Tests check the descriptions against what scripts really get, and that
//!   the checked-in file matches this generator.
//!
//! The format is documented at <https://luals.github.io/wiki/annotations/>.

use axcenity_core::{ClassName, ValueKind, World};

use crate::types::{ENUM_KINDS, enum_item_names};

/// Where the generated file is kept, relative to the repository root.
pub const DEFINITIONS_PATH: &str = "definitions/axcenity.lua";

/// The command that rewrites the file, shown in its header and in test failures.
const REGENERATE: &str = "cargo run -p axcenity-script --bin lua-definitions";

/// A function as Lua Language Server should show it.
struct Function {
    name: &'static str,
    doc: &'static str,
    /// `(name, type)` pairs. A name ending in `?` may be left out.
    params: &'static [(&'static str, &'static str)],
    returns: &'static str,
}

struct Field {
    name: &'static str,
    ty: &'static str,
    doc: &'static str,
}

/// A type of value that isn't an Instance, such as `Vector3`.
struct ValueType {
    class: &'static str,
    doc: &'static str,
    fields: &'static [Field],
    /// Lua Language Server `---@operator` lines, such as `add(Vector3): Vector3`.
    operators: &'static [&'static str],
}

/// A global table of functions, such as `Vector3` with `Vector3.new`.
struct Library {
    global: &'static str,
    doc: &'static str,
    functions: &'static [Function],
}

const VALUE_TYPES: &[ValueType] = &[
    ValueType {
        class: "Vector3",
        doc: "A position, size or direction in studs. Y is up.",
        fields: &[
            Field {
                name: "X",
                ty: "number",
                doc: "The X component.",
            },
            Field {
                name: "Y",
                ty: "number",
                doc: "The Y component (up).",
            },
            Field {
                name: "Z",
                ty: "number",
                doc: "The Z component.",
            },
            Field {
                name: "Magnitude",
                ty: "number",
                doc: "The length of the vector.",
            },
            Field {
                name: "Unit",
                ty: "Vector3",
                doc: "The same direction with length 1 (the zero vector stays zero).",
            },
        ],
        operators: &[
            "add(Vector3): Vector3",
            "sub(Vector3): Vector3",
            "mul(Vector3): Vector3",
            "mul(number): Vector3",
            "div(Vector3): Vector3",
            "div(number): Vector3",
            "unm: Vector3",
        ],
    },
    ValueType {
        class: "Color3",
        doc: "A color, with each channel from 0 to 1.",
        fields: &[
            Field {
                name: "R",
                ty: "number",
                doc: "Red, from 0 to 1.",
            },
            Field {
                name: "G",
                ty: "number",
                doc: "Green, from 0 to 1.",
            },
            Field {
                name: "B",
                ty: "number",
                doc: "Blue, from 0 to 1.",
            },
        ],
        operators: &[],
    },
    ValueType {
        class: "EnumItem",
        doc: "One choice of an enum, such as `Enum.Material.Neon`.",
        fields: &[Field {
            name: "Name",
            ty: "string",
            doc: "The item's name, such as `\"Neon\"`.",
        }],
        operators: &[],
    },
];

/// Fields every Instance has. Properties come from `core` instead.
const INSTANCE_FIELDS: &[Field] = &[
    Field {
        name: "Name",
        ty: "string",
        doc: "The Instance's name. Children can be found by name: `workspace.Block`.",
    },
    Field {
        name: "ClassName",
        ty: "string",
        doc: "The Instance's class, such as `\"Part\"`. Read-only.",
    },
    Field {
        name: "Parent",
        ty: "Instance?",
        doc: "The Instance this one is inside, or nil. Setting it moves the Instance.",
    },
];

const INSTANCE_METHODS: &[Function] = &[Function {
    name: "FindFirstChild",
    doc: "The first child with this name, or nil. With `recursive`, searches every descendant.",
    params: &[("name", "string"), ("recursive?", "boolean")],
    returns: "Instance?",
}];

/// Globals that are Instances.
const INSTANCE_GLOBALS: &[(&str, ClassName, &str)] = &[
    (
        "game",
        ClassName::DataModel,
        "The root of the game. Every service is inside it.",
    ),
    (
        "workspace",
        ClassName::Workspace,
        "The service that holds the 3D world (same as `game.Workspace`).",
    ),
    (
        "script",
        ClassName::Script,
        "The Script that is running this code.",
    ),
];

const LIBRARIES: &[Library] = &[
    Library {
        global: "Instance",
        doc: "Creates Instances.",
        functions: &[Function {
            name: "new",
            doc: "Creates an Instance of this class with default properties and no parent. Set `Parent` to put it in the world.",
            params: &[("className", "string")],
            returns: "Instance",
        }],
    },
    Library {
        global: "Vector3",
        doc: "Creates Vector3 values.",
        functions: &[Function {
            name: "new",
            doc: "A new Vector3. Left-out components are 0.",
            params: &[("x?", "number"), ("y?", "number"), ("z?", "number")],
            returns: "Vector3",
        }],
    },
    Library {
        global: "Color3",
        doc: "Creates Color3 values.",
        functions: &[
            Function {
                name: "new",
                doc: "A color from channels between 0 and 1. Left-out channels are 0.",
                params: &[("r?", "number"), ("g?", "number"), ("b?", "number")],
                returns: "Color3",
            },
            Function {
                name: "fromRGB",
                doc: "A color from channels between 0 and 255, as in most paint programs.",
                params: &[("r?", "number"), ("g?", "number"), ("b?", "number")],
                returns: "Color3",
            },
        ],
    },
    Library {
        global: "task",
        doc: "Pauses scripts and runs functions later, without stopping the game.",
        functions: &[
            Function {
                name: "wait",
                doc: "Pauses this code for `seconds` (left out: until the next frame), while the game and other scripts keep running. Returns the seconds that really passed.",
                params: &[("seconds?", "number")],
                returns: "number",
            },
            Function {
                name: "spawn",
                doc: "Runs `callback` right away with the given arguments, until it finishes or waits, then carries on here. An error inside it is reported without stopping this code.",
                params: &[("callback", "function"), ("...", "any")],
                returns: "thread",
            },
            Function {
                name: "delay",
                doc: "Runs `callback` with the given arguments after `seconds`, without pausing this code.",
                params: &[
                    ("seconds", "number"),
                    ("callback", "function"),
                    ("...", "any"),
                ],
                returns: "thread",
            },
        ],
    },
];

const ENUM_GLOBAL_DOC: &str = "Every enum, such as `Enum.Material` and `Enum.PartType`.";

/// What each property means. A test checks every property in `core` is here.
const PROPERTY_DOCS: &[(ClassName, &str, &str)] = &[
    (
        ClassName::Part,
        "Shape",
        "Block, Ball, Cylinder (along X) or Wedge.",
    ),
    (
        ClassName::Part,
        "Position",
        "Where the part's center is, in studs.",
    ),
    (
        ClassName::Part,
        "Orientation",
        "Rotation in degrees around X, Y and Z, applied Z first, then X, then Y.",
    ),
    (
        ClassName::Part,
        "Size",
        "The part's size in studs along X, Y and Z.",
    ),
    (ClassName::Part, "Color", "The part's color."),
    (
        ClassName::Part,
        "Material",
        "Plastic, Metal, Glass (see-through) or Neon (glows). Setting it resets Metalness and Roughness.",
    ),
    (
        ClassName::Part,
        "Metalness",
        "From 0 (not metal) to 1 (metal).",
    ),
    (
        ClassName::Part,
        "Roughness",
        "From 0 (mirror-smooth) to 1 (matte).",
    ),
];

/// What each class is. A `match` with no `_` case, so adding a class won't
/// compile until it has a description here.
fn class_doc(class: ClassName) -> &'static str {
    match class {
        ClassName::DataModel => "The root of the game, called `game` in scripts.",
        ClassName::Workspace => "The service that holds the 3D world. Parts inside it are drawn.",
        ClassName::ServerScriptService => {
            "The service that holds server scripts (`scripts/*.server.lua`)."
        }
        ClassName::Part => "A solid shape in the world.",
        ClassName::Script => "Lua code that runs on the server, from a `.server.lua` file.",
    }
}

/// The Lua Language Server type for a property of this kind.
fn lua_type(kind: ValueKind) -> String {
    match kind {
        ValueKind::Bool => "boolean".to_owned(),
        ValueKind::Number => "number".to_owned(),
        ValueKind::String => "string".to_owned(),
        ValueKind::Vector3 => "Vector3".to_owned(),
        ValueKind::Color3 => "Color3".to_owned(),
        // Enum properties read as items and also accept item names.
        ValueKind::Material | ValueKind::PartType => {
            let mut options = vec![format!("EnumItem.{kind}")];
            options.extend(
                enum_item_names(kind)
                    .iter()
                    .map(|name| format!("\"{name}\"")),
            );
            options.join("|")
        }
    }
}

/// Collects the file's text one line at a time.
struct Writer(String);

impl Writer {
    fn line(&mut self, text: &str) {
        self.0.push_str(text);
        self.0.push('\n');
    }

    fn doc(&mut self, text: &str) {
        self.line(&format!("---{text}"));
    }

    fn field(&mut self, field: &Field) {
        self.line(&format!(
            "---@field {} {} {}",
            field.name, field.ty, field.doc
        ));
    }

    fn heading(&mut self, title: &str) {
        // Sections already end with a blank line.
        if !self.0.ends_with("\n\n") {
            self.line("");
        }
        self.line(&format!("-- {title} {}", "-".repeat(74 - title.len())));
        self.line("");
    }

    /// `owner` and `separator` give `function Vector3.new(...)` or `function InstanceClass:FindFirstChild(...)`.
    fn function(
        &mut self,
        owner: &str,
        separator: char,
        function: &Function,
        overloads: &[String],
    ) {
        self.doc(function.doc);
        for (name, ty) in function.params {
            self.line(&format!("---@param {name} {ty}"));
        }
        self.line(&format!("---@return {}", function.returns));
        for overload in overloads {
            self.line(&format!("---@overload {overload}"));
        }
        let names: Vec<&str> = function
            .params
            .iter()
            .map(|(name, _)| name.trim_end_matches('?'))
            .collect();
        self.line(&format!(
            "function {owner}{separator}{}({}) end",
            function.name,
            names.join(", ")
        ));
        self.line("");
    }
}

/// The text of the definitions file for the current API.
pub fn lua_definitions() -> String {
    let mut w = Writer(String::new());
    w.line("---@meta");
    w.line("-- The Axcenity Engine script API, for Lua Language Server (autocomplete");
    w.line("-- and type hints in VS Code and other editors).");
    w.line(&format!(
        "-- Generated by `{REGENERATE}`. Don't edit by hand."
    ));

    w.heading("Values");
    for value_type in VALUE_TYPES {
        w.doc(value_type.doc);
        w.line(&format!("---@class {}", value_type.class));
        for field in value_type.fields {
            w.field(field);
        }
        for operator in value_type.operators {
            w.line(&format!("---@operator {operator}"));
        }
        w.line("");
    }

    w.heading("Enums");
    for kind in ENUM_KINDS {
        w.line(&format!("---@class EnumItem.{kind}: EnumItem"));
        w.line("");
        w.doc(&format!("The choices of `Enum.{kind}`."));
        w.line(&format!("---@class Enum.{kind}"));
        for name in enum_item_names(kind) {
            w.line(&format!("---@field {name} EnumItem.{kind}"));
        }
        w.line("");
    }
    w.doc(ENUM_GLOBAL_DOC);
    w.line("---@class EnumLibrary");
    for kind in ENUM_KINDS {
        w.line(&format!("---@field {kind} Enum.{kind}"));
    }
    w.line("");

    w.heading("Instances");
    w.doc("Anything in the game tree. Every class below is a kind of Instance.");
    w.line("---@class Instance");
    for field in INSTANCE_FIELDS {
        w.field(field);
    }
    w.line("---@field [string] Instance A child, found by its name.");
    // A different name from the global `Instance` library, which this would hide.
    w.line("local InstanceClass = {}");
    w.line("");
    for method in INSTANCE_METHODS {
        w.function("InstanceClass", ':', method, &[]);
    }

    // The services, as `game` holds them, so `game.Workspace` autocompletes.
    let world = World::new();
    let services: Vec<ClassName> = world
        .get(world.root())
        .map(|game| {
            game.children()
                .iter()
                .filter_map(|&id| world.get(id).map(|s| s.class()))
                .collect()
        })
        .unwrap_or_default();

    for class in ClassName::ALL {
        w.doc(class_doc(class));
        w.line(&format!("---@class {class}: Instance"));
        if class == ClassName::DataModel {
            for &service in &services {
                w.line(&format!(
                    "---@field {service} {service} {}",
                    class_doc(service)
                ));
            }
        }
        for (name, default) in class.default_properties() {
            let doc = property_doc(class, name).unwrap_or("");
            w.line(&format!(
                "---@field {name} {} {doc}",
                lua_type(default.kind())
            ));
        }
        w.line("");
    }

    w.heading("Globals");
    for &(name, class, doc) in INSTANCE_GLOBALS {
        w.doc(doc);
        w.line(&format!("---@type {class}"));
        w.line(&format!("{name} = nil"));
        w.line("");
    }
    w.doc(ENUM_GLOBAL_DOC);
    w.line("---@type EnumLibrary");
    w.line("Enum = nil");
    w.line("");

    for library in LIBRARIES {
        w.doc(library.doc);
        w.line(&format!("---@class {}Library", library.global));
        w.line(&format!("{} = {{}}", library.global));
        w.line("");
        for function in library.functions {
            w.function(library.global, '.', function, &overloads(library, function));
        }
    }

    // One blank line at the end, not two.
    let mut text = w.0;
    while text.ends_with("\n\n") {
        text.pop();
    }
    text
}

/// `Instance.new("Part")` returns a `Part`, which a plain `---@return` can't say.
fn overloads(library: &Library, function: &Function) -> Vec<String> {
    if library.global != "Instance" || function.name != "new" {
        return Vec::new();
    }
    ClassName::ALL
        .into_iter()
        .filter(|class| !class.is_service())
        .map(|class| format!("fun(className: \"{class}\"): {class}"))
        .collect()
}

fn property_doc(class: ClassName, property: &str) -> Option<&'static str> {
    PROPERTY_DOCS
        .iter()
        .find(|(c, p, _)| *c == class && *p == property)
        .map(|(_, _, doc)| *doc)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::Path;

    use axcenity_core::World;
    use mlua::{Table, Value};

    use super::*;
    use crate::sandbox::{self, SAFE_FUNCTIONS, SAFE_LIBRARIES, ScriptIds};
    use crate::{OutputKind, ScriptRuntime};

    #[test]
    fn definitions_file_is_up_to_date() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(DEFINITIONS_PATH);
        // Git may check text files out with Windows line endings.
        let saved = std::fs::read_to_string(&path)
            .unwrap_or_default()
            .replace("\r\n", "\n");
        assert!(
            saved == lua_definitions(),
            "{DEFINITIONS_PATH} is out of date with the script API; run `{REGENERATE}`"
        );
    }

    #[test]
    fn definitions_are_valid_lua() {
        mlua::Lua::new().load(lua_definitions()).exec().unwrap();
    }

    #[test]
    fn every_property_has_a_description() {
        let mut in_core = BTreeSet::new();
        for class in ClassName::ALL {
            for (name, _) in class.default_properties() {
                in_core.insert((class.as_str(), name));
            }
        }
        let described: BTreeSet<_> = PROPERTY_DOCS
            .iter()
            .map(|(c, p, _)| (c.as_str(), *p))
            .collect();
        assert_eq!(
            in_core, described,
            "PROPERTY_DOCS doesn't match the properties in core"
        );
    }

    /// One script environment, as a script would see it.
    fn environment(runtime: &ScriptRuntime) -> Table {
        let world = World::new();
        let ids = ScriptIds {
            game: world.root(),
            workspace: world.workspace(),
            script: world.workspace(),
        };
        sandbox::environment(&runtime.lua, &runtime.shared, ids).unwrap()
    }

    fn keys(table: &Table) -> BTreeSet<String> {
        table
            .pairs::<String, Value>()
            .map(|pair| pair.unwrap().0)
            .collect()
    }

    #[test]
    fn every_engine_global_is_described() {
        let runtime = ScriptRuntime::new();
        let standard: BTreeSet<String> = SAFE_FUNCTIONS
            .iter()
            .chain(SAFE_LIBRARIES)
            // Standard names that the sandbox replaces or trims.
            .chain(&["os", "_G", "pcall", "xpcall", "print"])
            .map(|name| name.to_string())
            .collect();
        let engine: BTreeSet<String> = keys(&environment(&runtime))
            .difference(&standard)
            .cloned()
            .collect();

        let mut described: BTreeSet<String> = INSTANCE_GLOBALS
            .iter()
            .map(|(name, _, _)| name.to_string())
            .collect();
        described.extend(LIBRARIES.iter().map(|library| library.global.to_owned()));
        described.insert("Enum".to_owned());
        assert_eq!(engine, described);
    }

    #[test]
    fn every_library_function_is_described() {
        let runtime = ScriptRuntime::new();
        let env = environment(&runtime);
        for library in LIBRARIES {
            let table: Table = env.get(library.global).unwrap();
            let described: BTreeSet<String> = library
                .functions
                .iter()
                .map(|f| f.name.to_owned())
                .collect();
            assert_eq!(keys(&table), described, "{} functions", library.global);
        }
    }

    /// The opposite direction for things mlua can't list: every described
    /// field and method exists in a real script.
    #[test]
    fn described_fields_and_methods_exist() {
        let mut checks = Vec::new();
        for field in INSTANCE_FIELDS {
            checks.push(format!(
                "assert(part.{0} ~= nil, 'Instance.{0}')",
                field.name
            ));
        }
        for method in INSTANCE_METHODS {
            checks.push(format!(
                "assert(type(part.{0}) == 'function', 'Instance:{0}')",
                method.name
            ));
        }
        let samples = [
            ("Vector3", "Vector3.new(1, 2, 3)"),
            ("Color3", "Color3.new(1, 0, 0)"),
            ("EnumItem", "Enum.Material.Neon"),
        ];
        for value_type in VALUE_TYPES {
            let (_, sample) = samples
                .iter()
                .find(|(class, _)| *class == value_type.class)
                .expect("add a sample value for the new value type");
            for field in value_type.fields {
                checks.push(format!(
                    "assert({sample}.{0} ~= nil, '{1}.{0}')",
                    field.name, value_type.class
                ));
            }
        }
        // Parented, so that every Instance field (including `Parent`) has a value.
        let source = format!(
            "local part = Instance.new('Part')\npart.Parent = workspace\n{}",
            checks.join("\n")
        );

        let mut world = World::new();
        let mut runtime = ScriptRuntime::new();
        let script = world.workspace();
        runtime.start_script(&mut world, script, "check.lua", &source);
        let errors: Vec<String> = runtime
            .take_output()
            .into_iter()
            .filter(|line| line.kind == OutputKind::Error)
            .map(|line| line.text)
            .collect();
        assert!(errors.is_empty(), "{errors:?}");
    }
}
