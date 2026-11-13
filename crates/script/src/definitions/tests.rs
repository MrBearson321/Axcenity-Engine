use std::collections::BTreeSet;
use std::path::Path;

use axcenity_core::World;
use mlua::{Table, Value};

use super::*;
use crate::context::Context;
use crate::input::KeyEvent;
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
    // What a class inherits is described once, for its doc parent.
    let inherited = |class: ClassName, name: &str| {
        super::doc_parent(class).is_some_and(|parent| {
            parent
                .default_properties()
                .iter()
                .any(|(property, _)| *property == name)
        })
    };
    let mut in_core = BTreeSet::new();
    for class in ClassName::ALL {
        for (name, _) in class.default_properties() {
            if !inherited(class, name) {
                in_core.insert((class.as_str(), name));
            }
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
        .flat_map(|class| {
            let parent = super::doc_parent(class).map_or(&[][..], ClassName::events);
            class
                .events()
                .iter()
                .filter(move |e| !parent.contains(e))
                .map(move |e| (class.as_str(), *e))
        })
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
            .map(|f| f.name)
            .chain(library.constants.iter().map(|c| c.name))
            .map(str::to_owned)
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
        (ClassName::RemoteFunction, "Instance.new('RemoteFunction')"),
        (ClassName::BindableEvent, "Instance.new('BindableEvent')"),
        (
            ClassName::BindableFunction,
            "Instance.new('BindableFunction')",
        ),
        (ClassName::Sound, "Instance.new('Sound')"),
        (
            ClassName::ParticleEmitter,
            "Instance.new('ParticleEmitter')",
        ),
        (ClassName::Animator, "Instance.new('Animator')"),
        (ClassName::Part, "Instance.new('Part')"),
        (ClassName::Seat, "Instance.new('Seat')"),
        (ClassName::VehicleSeat, "Instance.new('VehicleSeat')"),
        (ClassName::VehicleWheel, "Instance.new('VehicleWheel')"),
        (ClassName::Ragdoll, "Instance.new('Ragdoll')"),
        (ClassName::IKControl, "Instance.new('IKControl')"),
        (ClassName::Fracture, "Instance.new('Fracture')"),
        (ClassName::MeshPart, "Instance.new('MeshPart')"),
        (ClassName::Model, "Instance.new('Model')"),
        (ClassName::Attachment, "Instance.new('Attachment')"),
        (
            ClassName::HingeConstraint,
            "Instance.new('HingeConstraint')",
        ),
        (
            ClassName::PrismaticConstraint,
            "Instance.new('PrismaticConstraint')",
        ),
        (
            ClassName::CylindricalConstraint,
            "Instance.new('CylindricalConstraint')",
        ),
        (
            ClassName::SpringConstraint,
            "Instance.new('SpringConstraint')",
        ),
        (ClassName::RopeConstraint, "Instance.new('RopeConstraint')"),
        (ClassName::RodConstraint, "Instance.new('RodConstraint')"),
        (ClassName::Health, "Instance.new('Health')"),
        (ClassName::Ability, "Instance.new('Ability')"),
        (ClassName::Inventory, "Instance.new('Inventory')"),
        (ClassName::Tool, "Instance.new('Tool')"),
        // Its fields are its Model's pieces'.
        (
            ClassName::Humanoid,
            "(function() local h = Instance.new('Humanoid'); h.Parent = Instance.new('Model'); return h end)()",
        ),
        (ClassName::DamageZone, "Instance.new('DamageZone')"),
        (ClassName::SpawnLocation, "Instance.new('SpawnLocation')"),
        (ClassName::Players, "game:GetService('Players')"),
        (
            ClassName::TextChatService,
            "game:GetService('TextChatService')",
        ),
        (ClassName::Team, "Instance.new('Team')"),
        (ClassName::Animation, "Instance.new('Animation')"),
        (ClassName::Sequence, "Instance.new('Sequence')"),
        (ClassName::Spline, "Instance.new('Spline')"),
        (
            ClassName::NavigationAgent,
            "Instance.new('NavigationAgent')",
        ),
        (
            ClassName::AnimationTrack,
            "Instance.new('Animator'):LoadAnimation(Instance.new('Animation'))",
        ),
        (ClassName::Camera, "workspace.CurrentCamera"),
        (ClassName::Player, "game.Players.LocalPlayer"),
    ];
    // `CurrentCamera` and `LocalPlayer` are only on the player's side.
    let side_of = |class: ClassName, name: &'static str| match class {
        ClassName::Camera => "CurrentCamera",
        ClassName::Player => "LocalPlayer",
        _ => name,
    };
    // A field that may be nil (`Instance?`) only has to be readable, and a
    // callback (a function type) can only be set.
    let read = |sample: &str, field: &Field, owner: &str| {
        if field.ty.starts_with('(') {
            format!(";({sample}).{} = function() end", field.name)
        } else if field.ty.ends_with('?') {
            format!("local _ = ({sample}).{}", field.name)
        } else {
            format!("assert(({sample}).{0} ~= nil, '{owner}.{0}')", field.name)
        }
    };
    let instance = |class: ClassName| -> &'static str {
        instances
            .iter()
            .find(|(c, _)| *c == class)
            .map(|(_, sample)| *sample)
            .expect("add a sample Instance for the new class")
    };
    for (class, method) in CLASS_METHODS {
        // Game systems' methods: the `gameplay` crate's tests check them.
        if crate::system::DESCRIBED_SYSTEM_METHODS.contains(&(*class, method.name)) {
            continue;
        }
        // Scripts have no LocalPlayer: they reach a Player once it joins.
        if *class == ClassName::Player && matches!(method.name, "LoadCharacter" | "Kick") {
            check(
                method.name,
                format!(
                    "game.Players.PlayerAdded:Connect(function(player)\n\
                     assert(type(player.{0}) == 'function', 'Player:{0}')\n\
                     print('checked player')\nend)",
                    method.name
                ),
            );
            continue;
        }
        check(
            side_of(*class, method.name),
            format!(
                "assert(type(({0}).{1}) == 'function', '{2}:{1}')",
                instance(*class),
                method.name,
                class
            ),
        );
    }
    for (class, field) in CLASS_FIELDS {
        // `game.Debris` and the like come from game systems, which a bare
        // `ScriptRuntime` lacks: the `gameplay` crate's tests check them.
        if *class == ClassName::DataModel
            && crate::system::DESCRIBED_SYSTEM_SERVICES.contains(&field.name)
        {
            continue;
        }
        // A Humanoid's fields, which the character system keeps.
        if crate::system::DESCRIBED_SYSTEM_FIELDS.contains(&(*class, field.name)) {
            continue;
        }
        // `LocalPlayer` is nil only before the player joins.
        check(
            side_of(*class, field.name),
            read(instance(*class), field, class.as_str()),
        );
    }
    // InputObjects only exist inside input handlers: those checks run
    // when the test presses a key.
    let mut on_input = Vec::new();
    let samples = [
        ("Vector3", "Vector3.new(1, 2, 3)"),
        ("Vector2", "Vector2.new(1, 2)"),
        ("Color3", "Color3.new(1, 0, 0)"),
        ("CFrame", "CFrame.new(1, 2, 3)"),
        ("Random", "Random.new(1)"),
        ("UDim", "UDim2.new(0, 1, 0, 1).X"),
        ("UDim2", "UDim2.new(0, 1, 0, 1)"),
        ("EnumItem", "Enum.Material.Neon"),
        ("UserInputService", "game:GetService('UserInputService')"),
        (
            "ContextActionService",
            "game:GetService('ContextActionService')",
        ),
        ("RunService", "game:GetService('RunService')"),
        ("TweenService", "game:GetService('TweenService')"),
        ("CollectionService", "game:GetService('CollectionService')"),
        ("SaveService", "game:GetService('SaveService')"),
        ("LevelService", "game:GetService('LevelService')"),
        ("NetworkService", "game:GetService('NetworkService')"),
        ("ReplayService", "game:GetService('ReplayService')"),
        (
            "LocalizationService",
            "game:GetService('LocalizationService')",
        ),
        ("GuiService", "game:GetService('GuiService')"),
        ("HttpService", "game:GetService('HttpService')"),
        ("DataService", "game:GetService('DataService')"),
        ("ConsoleService", "game:GetService('ConsoleService')"),
        (
            "DataTable",
            "game:GetService('DataService'):GetTable('sample')",
        ),
        (
            "DataRow",
            "game:GetService('DataService'):GetTable('sample'):Get('row')",
        ),
        ("ContentProvider", "game:GetService('ContentProvider')"),
        ("UserGameSettings", "game:GetService('UserGameSettings')"),
        ("ReplicatedFirst", "game:GetService('ReplicatedFirst')"),
        (
            "Tween",
            "game:GetService('TweenService'):Create(part, TweenInfo.new(), {})",
        ),
        ("TweenInfo", "TweenInfo.new()"),
        ("InputObject", "input"),
        ("Ray", "workspace.CurrentCamera:ScreenPointToRay(1, 1)"),
        ("Mouse", "game.Players.LocalPlayer:GetMouse()"),
        (
            "RaycastResult",
            "workspace:Raycast(Vector3.new(0, 10, 0), Vector3.new(0, -20, 0))",
        ),
        ("PhysicalProperties", "PhysicalProperties.new(1, 0.5, 0.25)"),
        ("NumberSequence", "NumberSequence.new(1, 0)"),
        (
            "NumberSequenceKeypoint",
            "NumberSequenceKeypoint.new(0.5, 2, 0.1)",
        ),
        (
            "ColorSequence",
            "ColorSequence.new(Color3.new(1, 0, 0), Color3.new(0, 0, 1))",
        ),
        (
            "ColorSequenceKeypoint",
            "ColorSequenceKeypoint.new(0.5, Color3.new())",
        ),
        ("PhysicsService", "game:GetService('PhysicsService')"),
        ("RaycastParams", "RaycastParams.new()"),
        ("OverlapParams", "OverlapParams.new()"),
    ];
    for value_type in VALUE_TYPES {
        if crate::system::DESCRIBED_SYSTEM_SERVICES.contains(&value_type.class) {
            continue;
        }
        let (_, sample) = samples
            .iter()
            .find(|(class, _)| *class == value_type.class)
            .expect("add a sample value for the new value type");
        // Each line with the member it checks, for members (such as
        // `RenderStepped`) that only work on one side.
        let mut lines = Vec::new();
        for field in value_type.fields {
            lines.push((field.name, read(sample, field, value_type.class)));
        }
        for method in value_type.methods {
            lines.push((
                method.name,
                format!(
                    "assert(type({sample}.{0}) == 'function', '{1}:{0}')",
                    method.name, value_type.class
                ),
            ));
        }
        for (member, line) in lines {
            match value_type.class {
                "InputObject" => on_input.push(line),
                // Reached through `workspace.CurrentCamera` and
                // `LocalPlayer`, so client only.
                "Ray" => check("CurrentCamera", line),
                "Mouse" => check("LocalPlayer", line),
                _ if side_only(member).is_some() => check(member, line),
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
    // A project with a data table, for the DataTable and DataRow samples.
    let folder = std::env::temp_dir().join(format!("axcenity-definitions-{}", std::process::id()));
    let mut table = axcenity_assets::DataTable::default();
    table.add_row("row", None);
    axcenity_assets::write_data_table(&folder.join("assets/data/sample.data.json"), &table)
        .unwrap();
    runtime.set_assets(axcenity_assets::Assets::new(&folder));
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
    let printed: Vec<&str> = output.iter().map(|line| line.text.as_str()).collect();
    assert!(printed.contains(&"checked input"), "{printed:?}");
    assert!(printed.contains(&"checked player"), "{printed:?}");
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

#[test]
fn every_example_names_a_real_entry_and_compiles() {
    let names: BTreeSet<String> = api_reference()
        .iter()
        .map(ApiEntry::qualified_name)
        .collect();
    let mut seen = BTreeSet::new();
    for (name, code) in examples::EXAMPLES {
        assert!(
            names.contains(*name),
            "the example for `{name}` names nothing"
        );
        assert!(seen.insert(*name), "`{name}` has two examples");
        if let Some(error) = crate::syntax_error(name, code) {
            panic!("the example for `{name}` doesn't compile: {error}\n{code}");
        }
    }
    assert!(api_example("task.wait").is_some_and(|code| code.contains("task.wait(1)")));
    assert_eq!(api_example("Part.NoSuchThing"), None);
}

/// The pages in `documentation/api/` and the API section of the summary are
/// what the generator writes now, and no page is left from something that
/// was removed.
#[test]
fn api_pages_are_up_to_date() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let folder = root.join(API_DOCS_PATH);
    let pages = api_markdown();
    let mut stale = Vec::new();
    for (name, text) in &pages {
        let saved = std::fs::read_to_string(folder.join(name))
            .unwrap_or_default()
            .replace("\r\n", "\n");
        if saved != *text {
            stale.push(name.clone());
        }
    }
    let generated: BTreeSet<&str> = pages.iter().map(|(name, _)| name.as_str()).collect();
    let mut on_disk = Vec::new();
    markdown_files(&folder, "", &mut on_disk);
    for name in on_disk {
        if !generated.contains(name.as_str()) {
            stale.push(format!("{name} (nothing generates it)"));
        }
    }
    let summary = std::fs::read_to_string(root.join(SUMMARY_PATH))
        .unwrap_or_default()
        .replace("\r\n", "\n");
    if !summary.ends_with(&api_summary()) {
        stale.push(SUMMARY_PATH.to_owned());
    }
    assert!(
        stale.is_empty(),
        "the API pages are out of date with the script API; run `{REGENERATE}`: {stale:?}"
    );
}

fn markdown_files(folder: &Path, prefix: &str, files: &mut Vec<String>) {
    let Ok(read) = std::fs::read_dir(folder) else {
        return;
    };
    for item in read.flatten() {
        let name = item.file_name().to_string_lossy().into_owned();
        let path = format!("{prefix}{name}");
        if item.path().is_dir() {
            markdown_files(&item.path(), &format!("{path}/"), files);
        } else if name.ends_with(".md") {
            files.push(path);
        }
    }
}

#[test]
fn the_api_summary_replaces_only_its_own_section() {
    let summary = "# Table of contents\n\n## Learn\n\n* [A](a.md)\n\n## API reference\n\n* [old](api/README.md)\n";
    let new = with_api_summary(summary);
    assert!(new.starts_with("# Table of contents\n\n## Learn\n\n* [A](a.md)\n\n## API reference"));
    assert!(new.ends_with(&api_summary()));
    assert!(!new.contains("[old]"));
    assert_eq!(with_api_summary(&new), new);
    let pages = api_markdown();
    let part = pages
        .iter()
        .find(|(name, _)| name == "classes/Part.md")
        .map(|(_, text)| text.as_str())
        .unwrap_or_default();
    assert!(part.contains("#### Touched"), "{part}");
    assert!(
        part.contains("pad.Touched:Connect"),
        "Part.Touched's example is on its page"
    );
}
