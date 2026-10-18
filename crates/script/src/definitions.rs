//! Writes the Lua Language Server definitions file: `---@class` and
//! `---@field` annotations describing the whole script API, so editors such
//! as VS Code can autocomplete it and point out mistakes.
//!
//! - Classes, properties, events and enums come straight from `core`, so they
//!   can't fall out of date. A new property only needs a line in
//!   `PROPERTY_DOCS`, and a new event one in `EVENT_DOCS`.
//! - APIs that only work on one side (see `sides`) say so in their
//!   description, added from `SIDE_ONLY_APIS`.
//! - Functions, globals and value types are described by hand below.
//! - Tests check the descriptions against what scripts really get, and that
//!   the checked-in file matches this generator.
//!
//! The format is documented at <https://luals.github.io/wiki/annotations/>.

use axcenity_core::{ClassName, ValueKind, World};

use crate::sides::{Side, side_only};
use crate::types::EnumType;

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

/// A type of value that isn't an Instance, such as `Vector3`, or an engine
/// object that isn't one, such as `UserInputService`.
struct ValueType {
    class: &'static str,
    doc: &'static str,
    fields: &'static [Field],
    /// Lua Language Server `---@operator` lines, such as `add(Vector3): Vector3`.
    operators: &'static [&'static str],
    methods: &'static [Function],
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
        methods: &[],
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
        methods: &[],
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
        methods: &[],
    },
    ValueType {
        class: "UserInputService",
        doc: "The player's keyboard, from `game:GetService(\"UserInputService\")`. LocalScripts only.",
        fields: &[
            Field {
                name: "InputBegan",
                ty: "Signal",
                doc: "Fires when a key goes down. The handler gets an InputObject and `gameProcessedEvent` (always false for now): `function(input, gameProcessedEvent) ... end`.",
            },
            Field {
                name: "InputEnded",
                ty: "Signal",
                doc: "Fires when a key comes back up, with the same arguments as InputBegan.",
            },
        ],
        operators: &[],
        methods: &[Function {
            name: "IsKeyDown",
            doc: "True while `key` is held down, such as `Enum.KeyCode.W`.",
            params: &[("key", "EnumItem.KeyCode")],
            returns: "boolean",
        }],
    },
    ValueType {
        class: "RunService",
        doc: "The game's frame loop, from `game:GetService(\"RunService\")`.",
        fields: &[Field {
            name: "Heartbeat",
            ty: "Signal",
            doc: "Fires once every frame, after physics. The handler gets the frame's length in seconds: `function(deltaTime) ... end`.",
        }],
        operators: &[],
        methods: &[],
    },
    ValueType {
        class: "InputObject",
        doc: "One key going down or up, as `InputBegan` and `InputEnded` handlers get it.",
        fields: &[Field {
            name: "KeyCode",
            ty: "EnumItem.KeyCode",
            doc: "Which key, such as `Enum.KeyCode.W`.",
        }],
        operators: &[],
        methods: &[],
    },
    ValueType {
        class: "Camera",
        doc: "What the player sees through, `workspace.CurrentCamera`. LocalScripts only. Setting Position or Orientation moves the view; the player can still fly on from there.",
        fields: &[
            Field {
                name: "Position",
                ty: "Vector3",
                doc: "Where the camera is, in studs.",
            },
            Field {
                name: "Orientation",
                ty: "Vector3",
                doc: "Which way it looks, in degrees like a Part's: X looks up (positive) and down, Y turns left (positive) and right. Z is ignored.",
            },
            Field {
                name: "LookVector",
                ty: "Vector3",
                doc: "The direction the camera looks in, one stud long. Read-only.",
            },
        ],
        operators: &[],
        methods: &[],
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

const INSTANCE_METHODS: &[Function] = &[
    Function {
        name: "FindFirstChild",
        doc: "The first child with this name, or nil. With `recursive`, searches every descendant.",
        params: &[("name", "string"), ("recursive?", "boolean")],
        returns: "Instance?",
    },
    Function {
        name: "GetPropertyChangedSignal",
        doc: "An event that fires when this one property (or `Name` or `Parent`) gets a different value. The handler gets nothing: read the property to see its new value.",
        params: &[("property", "string")],
        returns: "Signal",
    },
];

/// What each event every Instance has means (`core`'s `INSTANCE_EVENTS`).
/// A test checks they match.
const INSTANCE_EVENT_DOCS: &[(&str, &str)] = &[
    (
        "Changed",
        "Fires when a property (or `Name` or `Parent`) gets a different value, whether a script, physics or the editor changed it. The handler gets the property's name: `function(property) ... end`.",
    ),
    (
        "ChildAdded",
        "Fires when an Instance is put inside this one. The handler gets the child.",
    ),
    (
        "ChildRemoved",
        "Fires when an Instance is taken out of this one. The handler gets the child.",
    ),
];

/// What `part.Touched` and other events are.
const SIGNAL_DOC: &str =
    "An event, such as `part.Touched`. Connect a function to run it every time the event fires.";

const SIGNAL_METHODS: &[Function] = &[Function {
    name: "Connect",
    doc: "Runs `callback` every time the event fires, each time as its own task (so it may `task.wait`). Returns a Connection for stopping it.",
    params: &[("callback", "function")],
    returns: "Connection",
}];

const CONNECTION_DOC: &str = "A function connected to an event, returned by `Connect`.";

const CONNECTION_FIELDS: &[Field] = &[Field {
    name: "Connected",
    ty: "boolean",
    doc: "True until `Disconnect` is called (or the Instance is destroyed).",
}];

const CONNECTION_METHODS: &[Function] = &[Function {
    name: "Disconnect",
    doc: "Stops the function from running when the event fires again. Calling it twice is harmless.",
    params: &[],
    returns: "nil",
}];

/// Globals that are Instances: name, Lua Language Server type, description.
const INSTANCE_GLOBALS: &[(&str, &str, &str)] = &[
    (
        "game",
        "DataModel",
        "The root of the game. Every service is inside it.",
    ),
    (
        "workspace",
        "Workspace",
        "The service that holds the 3D world (same as `game.Workspace`).",
    ),
    (
        "script",
        "Script|LocalScript|ModuleScript",
        "The Script, LocalScript or ModuleScript that is running this code.",
    ),
];

/// Global functions the engine adds or replaces.
const GLOBAL_FUNCTIONS: &[Function] = &[Function {
    name: "require",
    doc: "Runs a ModuleScript (a `.module.lua` file, such as `require(game.ReplicatedStorage.Utils)`) the first time, and returns what it returned. Later calls on the same side return the same value.",
    // `Instance`, not `ModuleScript`: a child found by name could be any
    // class (see the Instance indexer), and `require` checks it when it runs.
    params: &[("module", "Instance")],
    returns: "any",
}];

/// Methods only some classes have.
const CLASS_METHODS: &[(ClassName, Function)] = &[
    (
        ClassName::DataModel,
        Function {
            name: "GetService",
            doc: "The service with this class name, such as `\"Players\"`, `\"RunService\"` or `\"UserInputService\"`.",
            params: &[("className", "string")],
            returns: "Instance",
        },
    ),
    (
        ClassName::RemoteEvent,
        Function {
            name: "FireServer",
            doc: "Sends the arguments to the server, where `OnServerEvent` fires with this player first.",
            params: &[("...", "any")],
            returns: "nil",
        },
    ),
    (
        ClassName::RemoteEvent,
        Function {
            name: "FireClient",
            doc: "Sends the arguments to one player, where `OnClientEvent` fires.",
            params: &[("player", "Player"), ("...", "any")],
            returns: "nil",
        },
    ),
    (
        ClassName::RemoteEvent,
        Function {
            name: "FireAllClients",
            doc: "Sends the arguments to every player, where `OnClientEvent` fires.",
            params: &[("...", "any")],
            returns: "nil",
        },
    ),
];

/// Fields only some classes have, besides their properties and events.
const CLASS_FIELDS: &[(ClassName, Field)] = &[
    (
        ClassName::DataModel,
        Field {
            name: "UserInputService",
            ty: "UserInputService",
            doc: "The player's keyboard (same as `game:GetService(\"UserInputService\")`).",
        },
    ),
    (
        ClassName::DataModel,
        Field {
            name: "RunService",
            ty: "RunService",
            doc: "The game's frame loop, with `Heartbeat` (same as `game:GetService(\"RunService\")`).",
        },
    ),
    (
        ClassName::Workspace,
        Field {
            name: "CurrentCamera",
            ty: "Camera",
            doc: "The camera the player sees through.",
        },
    ),
    (
        ClassName::Players,
        Field {
            name: "LocalPlayer",
            ty: "Player",
            doc: "The player on this device.",
        },
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

const ENUM_GLOBAL_DOC: &str =
    "Every enum, such as `Enum.Material`, `Enum.PartType` and `Enum.KeyCode`.";

/// What each property means. A test checks every property in `core` is here.
const PROPERTY_DOCS: &[(ClassName, &str, &str)] = &[
    (
        ClassName::Part,
        "Anchored",
        "If true, the part stays where it is. If false, it falls and collides with other parts.",
    ),
    (
        ClassName::Part,
        "Shape",
        "Block, Ball, Cylinder (along X) or Wedge.",
    ),
    (
        ClassName::Part,
        "Position",
        "Where the part's center is, in studs. Setting it on an unanchored part teleports it and stops it moving.",
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
    (
        ClassName::MeshPart,
        "Anchored",
        "If true, the part stays where it is. If false, it falls and collides with other parts.",
    ),
    (
        ClassName::MeshPart,
        "MeshId",
        "The model's glTF or FBX file, inside the project folder, such as \"assets/models/chair.glb\"; `#` after it picks objects in the file (\"assets/models/props.fbx#Bench\"). The editor's Import Model sets it.",
    ),
    (
        ClassName::MeshPart,
        "Position",
        "Where the center of the model's box is, in studs. Setting it on an unanchored part teleports it and stops it moving.",
    ),
    (
        ClassName::MeshPart,
        "Orientation",
        "Rotation in degrees around X, Y and Z, applied Z first, then X, then Y.",
    ),
    (
        ClassName::MeshPart,
        "Size",
        "The size of the model's box in studs along X, Y and Z. The model stretches to fill it.",
    ),
    (
        ClassName::SurfaceAppearance,
        "MaterialName",
        "The name of the material of the parent MeshPart's model whose maps this replaces. Import Model sets it.",
    ),
    (
        ClassName::SurfaceAppearance,
        "Color",
        "Multiplies the material's color (and its ColorMap).",
    ),
    (
        ClassName::SurfaceAppearance,
        "ColorMap",
        "The base color texture, a file inside the project folder such as \"assets/textures/wood.png\". Empty for none.",
    ),
    (
        ClassName::SurfaceAppearance,
        "NormalMap",
        "The normal map's file, for bumps and grooves. Empty for none.",
    ),
    (
        ClassName::SurfaceAppearance,
        "MetalnessMap",
        "The metalness map's file (read from MetalnessChannel). Empty for the model's own metalness.",
    ),
    (
        ClassName::SurfaceAppearance,
        "RoughnessMap",
        "The roughness map's file (read from RoughnessChannel). Empty for the model's own roughness.",
    ),
    (
        ClassName::SurfaceAppearance,
        "EmissiveMap",
        "The file of colors the surface glows in. Empty for the model's own glow.",
    ),
    (
        ClassName::SurfaceAppearance,
        "AmbientOcclusionMap",
        "The ambient occlusion map's file: dark where ambient light can't reach. Empty for none.",
    ),
    (
        ClassName::SurfaceAppearance,
        "OpacityMap",
        "The opacity map's file (read from OpacityChannel): the surface is cut out where it's below half. Empty for the color map's alpha.",
    ),
    (
        ClassName::SurfaceAppearance,
        "SpecularMap",
        "How strongly non-metal parts shine, for older materials: mid grey is the usual. Empty for none.",
    ),
    (
        ClassName::SurfaceAppearance,
        "NormalStrength",
        "How far the normal map tips the surface: 1 as made, 0 flat.",
    ),
    (
        ClassName::SurfaceAppearance,
        "NormalMapDirectX",
        "True for normal maps made for DirectX, whose green points the other way.",
    ),
    (
        ClassName::SurfaceAppearance,
        "MetalnessChannel",
        "Which channel of MetalnessMap holds metalness, for packed textures.",
    ),
    (
        ClassName::SurfaceAppearance,
        "RoughnessChannel",
        "Which channel of RoughnessMap holds roughness, for packed textures.",
    ),
    (
        ClassName::SurfaceAppearance,
        "InvertRoughness",
        "True if RoughnessMap is a glossiness or smoothness map: white is smooth.",
    ),
    (
        ClassName::SurfaceAppearance,
        "AmbientOcclusionChannel",
        "Which channel of AmbientOcclusionMap holds it, for packed textures.",
    ),
    (
        ClassName::SurfaceAppearance,
        "AmbientOcclusionStrength",
        "How much AmbientOcclusionMap darkens, from 0 to 1.",
    ),
    (
        ClassName::SurfaceAppearance,
        "OpacityChannel",
        "Which channel of OpacityMap holds opacity.",
    ),
    (
        ClassName::SurfaceAppearance,
        "InvertOpacity",
        "True if OpacityMap is white where the surface is see-through.",
    ),
    (
        ClassName::SurfaceAppearance,
        "EmissiveStrength",
        "Multiplies the glow.",
    ),
];

/// What each event means, including what its handler receives. A test
/// checks every event in `core` is here.
const EVENT_DOCS: &[(ClassName, &str, &str)] = &[
    (
        ClassName::Part,
        "Touched",
        "Fires when another part starts touching this one, once per new contact (not every frame while they touch). The handler gets the other part: `part.Touched:Connect(function(otherPart) ... end)`.",
    ),
    (
        ClassName::MeshPart,
        "Touched",
        "Fires when another part starts touching this one, once per new contact (not every frame while they touch). The handler gets the other part: `part.Touched:Connect(function(otherPart) ... end)`.",
    ),
    (
        ClassName::RemoteEvent,
        "OnServerEvent",
        "Fires when a LocalScript calls `FireServer`. The handler gets the Player who sent it, then the arguments: `function(player, ...) ... end`.",
    ),
    (
        ClassName::RemoteEvent,
        "OnClientEvent",
        "Fires when a Script calls `FireClient` or `FireAllClients`. The handler gets the arguments.",
    ),
];

/// What each class is. A `match` with no `_` case, so adding a class won't
/// compile until it has a description here.
fn class_doc(class: ClassName) -> &'static str {
    match class {
        ClassName::DataModel => "The root of the game, called `game` in scripts.",
        ClassName::Workspace => "The service that holds the 3D world. Parts inside it are drawn.",
        ClassName::Players => "The service with a Player for everyone in the game.",
        ClassName::ReplicatedStorage => {
            "The service for things Scripts and LocalScripts share, such as RemoteEvents and ModuleScripts."
        }
        ClassName::ServerScriptService => {
            "The service that holds server scripts (`scripts/*.server.lua`)."
        }
        ClassName::StarterPlayer => {
            "The service with what each player starts with, such as StarterPlayerScripts."
        }
        ClassName::StarterPlayerScripts => {
            "Holds the LocalScripts (`scripts/*.client.lua`), which run on the player's side."
        }
        ClassName::Part => "A solid shape in the world.",
        ClassName::MeshPart => {
            "A solid 3D model imported from a glTF or FBX file, drawn with the file's own materials (or its SurfaceAppearances'). It collides as the model's convex hull."
        }
        ClassName::Script => "Lua code that runs on the server, from a `.server.lua` file.",
        ClassName::LocalScript => {
            "Lua code that runs on the player's side (input, camera), from a `.client.lua` file."
        }
        ClassName::ModuleScript => {
            "Lua code that scripts load with `require`, from a `.module.lua` file. It runs once per side and returns one value."
        }
        ClassName::RemoteEvent => {
            "Carries messages between Scripts and LocalScripts. Keep it in ReplicatedStorage."
        }
        ClassName::Player => "Someone playing the game, in Players. Made when they join.",
        ClassName::Model => "Groups the parts of one thing, such as a car or a house.",
        ClassName::Folder => "Groups Instances to keep the tree tidy. Has no effect on the game.",
        ClassName::SurfaceAppearance => {
            "Inside a MeshPart: the texture maps of one of its model's materials (MaterialName), drawn in place of the file's own. Delete it to go back to the file's."
        }
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
        ValueKind::Material | ValueKind::PartType | ValueKind::TextureChannel => {
            let enum_type = EnumType::of_kind(kind).expect("an enum kind");
            let mut options = vec![format!("EnumItem.{}", enum_type.name())];
            options.extend(
                enum_type
                    .item_names()
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
            field.name,
            field.ty,
            with_side(field.name, field.doc)
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
        self.function_named(&format!("{owner}{separator}"), function, overloads);
    }

    /// A function whose full name is `prefix` and then its name: `""` for a global.
    fn function_named(&mut self, prefix: &str, function: &Function, overloads: &[String]) {
        self.doc(&with_side(function.name, function.doc));
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
            "function {prefix}{}({}) end",
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
        if !value_type.methods.is_empty() {
            let owner = format!("{}Class", value_type.class);
            w.line(&format!("local {owner} = {{}}"));
            w.line("");
            for method in value_type.methods {
                w.function(&owner, ':', method, &[]);
            }
        }
        w.line("");
    }

    w.heading("Enums");
    for enum_type in EnumType::ALL {
        let name = enum_type.name();
        w.line(&format!("---@class EnumItem.{name}: EnumItem"));
        w.line("");
        w.doc(&format!("The choices of `Enum.{name}`."));
        w.line(&format!("---@class Enum.{name}"));
        for item in enum_type.item_names() {
            w.line(&format!("---@field {item} EnumItem.{name}"));
        }
        w.line("");
    }
    w.doc(ENUM_GLOBAL_DOC);
    w.line("---@class EnumLibrary");
    for enum_type in EnumType::ALL {
        let name = enum_type.name();
        w.line(&format!("---@field {name} Enum.{name}"));
    }
    w.line("");

    w.heading("Instances");
    w.doc("Anything in the game tree. Every class below is a kind of Instance.");
    w.line("---@class Instance");
    for field in INSTANCE_FIELDS {
        w.field(field);
    }
    for (event, doc) in INSTANCE_EVENT_DOCS {
        w.line(&format!("---@field {event} Signal {doc}"));
    }
    // A child could be of any class. Typed as all of them, not as plain
    // `Instance`, so `workspace.Pad.Color = ...` isn't flagged as setting a
    // field Instance doesn't have, and `Pad`'s Part properties autocomplete.
    let children: Vec<&str> = ClassName::ALL
        .into_iter()
        .filter(|class| *class != ClassName::DataModel)
        .map(ClassName::as_str)
        .collect();
    w.line(&format!(
        "---@field [string] {} A child, found by its name.",
        children.join("|")
    ));
    // A different name from the global `Instance` library, which this would hide.
    w.line("local InstanceClass = {}");
    w.line("");
    for method in INSTANCE_METHODS {
        w.function("InstanceClass", ':', method, &[]);
    }

    w.doc(SIGNAL_DOC);
    w.line("---@class Signal");
    w.line("local SignalClass = {}");
    w.line("");
    for method in SIGNAL_METHODS {
        w.function("SignalClass", ':', method, &[]);
    }
    w.doc(CONNECTION_DOC);
    w.line("---@class Connection");
    for field in CONNECTION_FIELDS {
        w.field(field);
    }
    w.line("local ConnectionClass = {}");
    w.line("");
    for method in CONNECTION_METHODS {
        w.function("ConnectionClass", ':', method, &[]);
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
        for (_, field) in CLASS_FIELDS.iter().filter(|(c, _)| *c == class) {
            w.field(field);
        }
        for &event in class.events() {
            let doc = event_doc(class, event).unwrap_or("");
            w.line(&format!(
                "---@field {event} Signal {}",
                with_side(event, doc)
            ));
        }
        let methods: Vec<&Function> = CLASS_METHODS
            .iter()
            .filter(|(c, _)| *c == class)
            .map(|(_, method)| method)
            .collect();
        if !methods.is_empty() {
            let owner = format!("{class}Class");
            w.line(&format!("local {owner} = {{}}"));
            w.line("");
            for method in methods {
                w.function(&owner, ':', method, &[]);
            }
        }
        w.line("");
    }

    w.heading("Globals");
    for &(name, ty, doc) in INSTANCE_GLOBALS {
        w.doc(doc);
        w.line(&format!("---@type {ty}"));
        w.line(&format!("{name} = nil"));
        w.line("");
    }
    w.doc(ENUM_GLOBAL_DOC);
    w.line("---@type EnumLibrary");
    w.line("Enum = nil");
    w.line("");
    for function in GLOBAL_FUNCTIONS {
        w.function_named("", function, &[]);
    }

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
        // Services always exist, and Players are made when someone joins.
        .filter(|class| !class.is_service() && *class != ClassName::Player)
        .map(|class| format!("fun(className: \"{class}\"): {class}"))
        .collect()
}

/// `doc`, saying where it works if `name` is a side-only API.
fn with_side(name: &str, doc: &str) -> String {
    match side_only(name).map(|api| api.side) {
        Some(Side::Client) => format!("{doc} LocalScripts only."),
        Some(Side::Server) => format!("{doc} Scripts only."),
        None => doc.to_owned(),
    }
}

fn event_doc(class: ClassName, event: &str) -> Option<&'static str> {
    EVENT_DOCS
        .iter()
        .find(|(c, e, _)| *c == class && *e == event)
        .map(|(_, _, doc)| *doc)
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
    use crate::client::KeyEvent;
    use crate::context::Context;
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

    #[test]
    fn every_event_has_a_description() {
        let in_core: BTreeSet<_> = ClassName::ALL
            .into_iter()
            .flat_map(|class| class.events().iter().map(move |e| (class.as_str(), *e)))
            .collect();
        let described: BTreeSet<_> = EVENT_DOCS
            .iter()
            .map(|(c, e, _)| (c.as_str(), *e))
            .collect();
        assert_eq!(
            in_core, described,
            "EVENT_DOCS doesn't match the events in core"
        );
        let described: Vec<&str> = INSTANCE_EVENT_DOCS.iter().map(|(e, _)| *e).collect();
        assert_eq!(
            described,
            axcenity_core::INSTANCE_EVENTS,
            "INSTANCE_EVENT_DOCS doesn't match core's INSTANCE_EVENTS"
        );
    }

    /// What Phase 16 promises VS Code users: `Instance.new` gives the
    /// class it was asked for, Parts have their properties, and `task.*` is
    /// there.
    #[test]
    fn definitions_cover_instance_new_part_properties_and_task() {
        let text = lua_definitions();
        for line in [
            "---@overload fun(className: \"Part\"): Part",
            "function Instance.new(className) end",
            "---@class Part: Instance",
            "---@field Position Vector3 ",
            "---@field Material EnumItem.Material|\"Plastic\"",
            "function task.wait(seconds) end",
            "function task.spawn(callback, ...) end",
            "function task.delay(seconds, callback, ...) end",
            "---@field Changed Signal ",
            "function InstanceClass:GetPropertyChangedSignal(property) end",
            "---@field Heartbeat Signal ",
        ] {
            assert!(text.contains(line), "the definitions have no `{line}`");
        }
    }

    /// One script environment, as a script would see it.
    fn environment(context: &Context) -> Table {
        let world = World::new();
        let ids = ScriptIds {
            game: world.root(),
            workspace: world.workspace(),
            script: world.workspace(),
        };
        sandbox::environment(&context.lua, ids).unwrap()
    }

    fn keys(table: &Table) -> BTreeSet<String> {
        table
            .pairs::<String, Value>()
            .map(|pair| pair.unwrap().0)
            .collect()
    }

    #[test]
    fn every_engine_global_is_described() {
        let runtime = Context::new(Side::Server);
        let standard: BTreeSet<String> = SAFE_FUNCTIONS
            .iter()
            .chain(SAFE_LIBRARIES)
            // Standard names that the sandbox replaces or trims.
            .chain(&[
                "os",
                "_G",
                "pcall",
                "xpcall",
                "print",
                "setmetatable",
                "collectgarbage",
            ])
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
        described.extend(GLOBAL_FUNCTIONS.iter().map(|f| f.name.to_owned()));
        assert_eq!(engine, described);
    }

    #[test]
    fn every_library_function_is_described() {
        let runtime = Context::new(Side::Server);
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
    /// field and method exists in a real script, on the side it works on.
    #[test]
    fn described_fields_and_methods_exist() {
        // Checks for the server (a Script) and the client (a LocalScript).
        let mut server = Vec::new();
        let mut client = Vec::new();
        let mut check = |name: &str, line: String| match side_only(name).map(|api| api.side) {
            Some(Side::Client) => client.push(line),
            _ => server.push(line),
        };
        for field in INSTANCE_FIELDS {
            check(
                field.name,
                format!("assert(part.{0} ~= nil, 'Instance.{0}')", field.name),
            );
        }
        for method in INSTANCE_METHODS {
            check(
                method.name,
                format!(
                    "assert(type(part.{0}) == 'function', 'Instance:{0}')",
                    method.name
                ),
            );
        }
        for (event, _) in INSTANCE_EVENT_DOCS {
            check(
                event,
                format!("assert(part.{event}.Connect ~= nil, 'Instance.{event}')"),
            );
        }
        check(
            "",
            "local connection = part.Touched:Connect(function() end)".to_owned(),
        );
        for method in SIGNAL_METHODS {
            check(
                method.name,
                format!(
                    "assert(type(part.Touched.{0}) == 'function', 'Signal:{0}')",
                    method.name
                ),
            );
        }
        for field in CONNECTION_FIELDS {
            check(
                field.name,
                format!(
                    "assert(connection.{0} ~= nil, 'Connection.{0}')",
                    field.name
                ),
            );
        }
        for method in CONNECTION_METHODS {
            check(
                method.name,
                format!(
                    "assert(type(connection.{0}) == 'function', 'Connection:{0}')",
                    method.name
                ),
            );
        }
        // An Instance of each class with its own methods or fields.
        let instances = [
            (ClassName::DataModel, "game"),
            (ClassName::Workspace, "workspace"),
            (ClassName::Players, "game.Players"),
            (ClassName::RemoteEvent, "Instance.new('RemoteEvent')"),
        ];
        let instance = |class: ClassName| {
            instances
                .iter()
                .find(|(c, _)| *c == class)
                .map(|(_, sample)| *sample)
                .expect("add a sample Instance for the new class")
        };
        for (class, method) in CLASS_METHODS {
            check(
                method.name,
                format!(
                    "assert(type(({0}).{1}) == 'function', '{2}:{1}')",
                    instance(*class),
                    method.name,
                    class
                ),
            );
        }
        for (class, field) in CLASS_FIELDS {
            // `LocalPlayer` is nil only before the player joins.
            check(
                field.name,
                format!(
                    "assert(({0}).{1} ~= nil, '{2}.{1}')",
                    instance(*class),
                    field.name,
                    class
                ),
            );
        }
        // InputObjects only exist inside input handlers: those checks run
        // when the test presses a key.
        let mut on_input = Vec::new();
        let samples = [
            ("Vector3", "Vector3.new(1, 2, 3)"),
            ("Color3", "Color3.new(1, 0, 0)"),
            ("EnumItem", "Enum.Material.Neon"),
            ("UserInputService", "game:GetService('UserInputService')"),
            ("RunService", "game:GetService('RunService')"),
            ("InputObject", "input"),
            ("Camera", "workspace.CurrentCamera"),
        ];
        for value_type in VALUE_TYPES {
            let (_, sample) = samples
                .iter()
                .find(|(class, _)| *class == value_type.class)
                .expect("add a sample value for the new value type");
            let mut lines = Vec::new();
            for field in value_type.fields {
                lines.push(format!(
                    "assert({sample}.{0} ~= nil, '{1}.{0}')",
                    field.name, value_type.class
                ));
            }
            for method in value_type.methods {
                lines.push(format!(
                    "assert(type({sample}.{0}) == 'function', '{1}:{0}')",
                    method.name, value_type.class
                ));
            }
            for line in lines {
                match value_type.class {
                    "InputObject" => on_input.push(line),
                    // Reached through `workspace.CurrentCamera`, so client only.
                    "Camera" => check("CurrentCamera", line),
                    other => check(other, line),
                }
            }
        }
        client.push(format!(
            "game:GetService('UserInputService').InputBegan:Connect(function(input)\n{}\nprint('checked input')\nend)",
            on_input.join("\n")
        ));

        // Parented, so that every Instance field (including `Parent`) has a value.
        let start = "local part = Instance.new('Part')\npart.Parent = workspace\n";
        let mut world = World::new();
        let mut runtime = ScriptRuntime::new();
        let script = world.create(ClassName::Script).unwrap();
        let local_script = world.create(ClassName::LocalScript).unwrap();
        let source = format!("{start}{}", server.join("\n"));
        runtime.start_script(&mut world, script, "check.server.lua", &source);
        let source = format!("{start}{}", client.join("\n"));
        runtime.start_script(&mut world, local_script, "check.client.lua", &source);
        let key = KeyEvent {
            key: axcenity_core::KeyCode::W,
            pressed: true,
        };
        runtime.input(&mut world, &[key]);

        let output = runtime.take_output();
        let errors: Vec<&str> = output
            .iter()
            .filter(|line| line.kind == OutputKind::Error)
            .map(|line| line.text.as_str())
            .collect();
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(output.last().unwrap().text, "checked input");
    }
}
