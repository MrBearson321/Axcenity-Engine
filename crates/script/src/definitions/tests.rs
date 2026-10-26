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

/// No Luau parser for definitions is built into the engine, so this checks
/// the shape: every `declare extern type` is closed, and nothing is left
/// over from the old Lua Language Server format. (Checked by hand with
/// `luau-lsp analyze --platform=standard
/// --definitions:@axcenity=definitions/axcenity.d.luau`.)
#[test]
fn definitions_are_well_formed() {
    let text = lua_definitions();
    let mut open = None;
    for (number, line) in text.lines().enumerate() {
        assert!(!line.starts_with("---@"), "line {number}: `{line}`");
        if let Some(class) = line.strip_prefix("declare extern type ") {
            assert!(open.is_none(), "line {number}: `{class}` inside a class");
            open = Some(class);
        } else if line == "end" {
            assert!(
                open.take().is_some(),
                "line {number}: `end` outside a class"
            );
        } else if open.is_some() && !line.is_empty() {
            assert!(
                line.starts_with('\t'),
                "line {number}: `{line}` isn't indented"
            );
        }
    }
    assert!(open.is_none(), "`{open:?}` is never closed");
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
        "\tnew: ((className: \"Part\") -> Part) & ",
        "& ((className: string) -> Instance)\n",
        "declare Instance: InstanceLibrary\n",
        "declare extern type Part extends Instance with\n",
        "\tPosition: Vector3\n",
        "\tMaterial: EnumMaterial | \"Plastic\" | ",
        "\twait: (seconds: number?) -> number\n",
        "\tspawn: (callback: (...any) -> (), ...any) -> thread\n",
        "\tdelay: (seconds: number, callback: (...any) -> (), ...any) -> thread\n",
        "\tChanged: Signal\n",
        "\tfunction GetPropertyChangedSignal(self, property: string): Signal\n",
        "\tHeartbeat: Signal\n",
        "\tfunction __mul(self, other: Vector3 | number): Vector3\n",
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
            "typeof",
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
        (ClassName::Sound, "Instance.new('Sound')"),
        (
            ClassName::ParticleEmitter,
            "Instance.new('ParticleEmitter')",
        ),
        (ClassName::Animator, "Instance.new('Animator')"),
        (ClassName::Animation, "Instance.new('Animation')"),
        (
            ClassName::AnimationTrack,
            "Instance.new('Animator'):LoadAnimation(Instance.new('Animation'))",
        ),
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
        ("Vector2", "Vector2.new(1, 2)"),
        ("Color3", "Color3.new(1, 0, 0)"),
        ("UDim", "UDim2.new(0, 1, 0, 1).X"),
        ("UDim2", "UDim2.new(0, 1, 0, 1)"),
        ("EnumItem", "Enum.Material.Neon"),
        ("UserInputService", "game:GetService('UserInputService')"),
        ("RunService", "game:GetService('RunService')"),
        ("TweenService", "game:GetService('TweenService')"),
        ("SaveService", "game:GetService('SaveService')"),
        ("ReplicatedFirst", "game:GetService('ReplicatedFirst')"),
        (
            "Tween",
            "game:GetService('TweenService'):Create(part, TweenInfo.new(), {})",
        ),
        ("TweenInfo", "TweenInfo.new()"),
        ("InputObject", "input"),
        ("Camera", "workspace.CurrentCamera"),
        (
            "RaycastResult",
            "workspace:Raycast(Vector3.new(0, 10, 0), Vector3.new(0, -20, 0))",
        ),
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

#[test]
fn the_reference_lists_what_the_definitions_declare_in_their_words() {
    let text = lua_definitions();
    let reference = api_reference();
    let find = |owner: &str, name: &str| {
        reference
            .iter()
            .find(|entry| entry.owner == owner && entry.name == name)
            .unwrap_or_else(|| panic!("`{owner}.{name}` is missing from the reference"))
    };

    let part = find("", "Part");
    assert_eq!(part.kind, ApiKind::Class);
    assert_eq!(part.signature, "Instance.new(\"Part\")");
    assert_eq!(find("", "Workspace").signature, "game.Workspace");
    assert_eq!(find("Part", "Size").kind, ApiKind::Property);
    assert_eq!(find("Part", "Touched").kind, ApiKind::Event);
    let new = find("Instance", "new");
    assert_eq!(new.kind, ApiKind::Function);
    assert_eq!(new.signature, "Instance.new(className: string): Instance");
    assert_eq!(
        find("Instance", "FindFirstChild").signature,
        "Instance:FindFirstChild(name: string, recursive: boolean?): Instance?"
    );
    assert_eq!(find("", "task").kind, ApiKind::Library);
    assert_eq!(find("task", "wait").kind, ApiKind::Function);
    assert_eq!(
        find("Enum.Material", "Neon").signature,
        "Enum.Material.Neon"
    );
    assert_eq!(find("", "require").kind, ApiKind::Function);
    // Vector3 is a type, so `Vector3.new` belongs to it, not to a library.
    assert_eq!(find("", "Vector3").kind, ApiKind::Type);
    assert_eq!(find("Vector3", "new").kind, ApiKind::Function);

    for entry in &reference {
        assert!(
            entry.owner.is_empty() || reference.iter().any(|o| o.name == entry.owner),
            "`{}` belongs to nothing in the reference",
            entry.qualified_name()
        );
        assert!(
            entry.doc.is_empty() || text.contains(&format!("--- {}\n", entry.doc)),
            "`{}`'s description isn't the definitions' own",
            entry.qualified_name()
        );
    }
}
