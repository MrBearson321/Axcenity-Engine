use std::path::{Path, PathBuf};

use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};
use axcenity_script::OutputKind;

use super::*;

fn example() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-plugins")
}

/// A new empty folder for one test.
fn temp_folder(test: &str) -> PathBuf {
    let folder =
        std::env::temp_dir().join(format!("axcenity-plugins-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    folder
}

/// A project in `folder` with one plugin, `name`, whose files are given.
fn project_with_plugin(folder: &Path, name: &str, manifest: &str, init: &str) -> Found {
    let plugin = folder.join(PLUGINS_FOLDER).join(name);
    std::fs::create_dir_all(&plugin).unwrap();
    std::fs::write(plugin.join(MANIFEST_FILE), manifest).unwrap();
    std::fs::write(plugin.join(ENTRY_FILE), init).unwrap();
    find_plugins(Some(folder), None)
        .into_iter()
        .find(|found| found.key == format!("project/{name}"))
        .unwrap()
}

const MANIFEST: &str = r#"{ "name": "Test", "version": "0.1.0" }"#;
const FILES_MANIFEST: &str = r#"{ "name": "Test", "version": "0.1.0", "permissions": ["files"] }"#;

fn start(world: &mut World, found: &Found, project: &Path) -> Plugin {
    world.record_changes();
    let name = found.folder.file_name().unwrap().to_string_lossy();
    Plugin::start(
        world,
        found,
        Start {
            project: Some(project.to_owned()),
            shown: format!("plugins/{name}"),
            ..Start::default()
        },
    )
    .unwrap()
}

fn part(world: &mut World, name: &str, position: Vector3) -> InstanceId {
    let part = world.create(ClassName::Part).unwrap();
    world.set_name(part, name).unwrap();
    world
        .set_property(part, "Position", Value::Vector3(position))
        .unwrap();
    world.set_parent(part, Some(world.workspace())).unwrap();
    part
}

fn position(world: &World, id: InstanceId) -> Vector3 {
    match world.get(id).unwrap().property("Position") {
        Some(Value::Vector3(v)) => *v,
        other => panic!("not a position: {other:?}"),
    }
}

fn errors(plugin: &Plugin) -> Vec<String> {
    plugin
        .take_output()
        .into_iter()
        .filter(|line| line.kind == OutputKind::Error)
        .map(|line| line.text)
        .collect()
}

#[test]
fn the_example_project_has_two_plugins() {
    let found = find_plugins(Some(&example()), None);
    let names: Vec<_> = found.iter().map(|f| (f.key.as_str(), f.name())).collect();
    assert_eq!(
        names,
        [
            ("project/align-to-grid", "Align to Grid".to_owned()),
            ("project/part-tools", "Part Tools".to_owned()),
        ]
    );
    assert!(
        found[1]
            .manifest
            .as_ref()
            .unwrap()
            .allows(Permission::Files)
    );
    assert_eq!(
        plugin_files(&found[1].folder),
        ["init.lua", "plugin.json", "widget.css"]
    );
}

#[test]
fn align_to_grid_snaps_the_selected_parts_to_one_stud_with_one_waypoint() {
    let found = find_plugins(Some(&example()), None).remove(0);
    let mut world = World::new();
    let crate_part = part(&mut world, "Crate", Vector3::new(2.3, 1.6, -3.7));
    let pillar = part(&mut world, "Pillar", Vector3::new(-5.45, 3.2, 1.51));
    let left_alone = part(&mut world, "Other", Vector3::new(0.4, 0.4, 0.4));
    let plugin = start(&mut world, &found, &example());
    let toolbars = plugin.toolbars();
    assert_eq!(toolbars.len(), 1);
    assert_eq!(toolbars[0].name, "Align");
    let button = &toolbars[0].buttons[0];
    assert_eq!(button.name, "Align to Grid");
    assert_eq!(button.icon, "grid-four");

    plugin.set_selection(&mut world, &[crate_part, pillar]);
    world.take_changes();
    plugin.click(&mut world, button.id);

    assert_eq!(position(&world, crate_part), Vector3::new(2.0, 2.0, -4.0));
    assert_eq!(position(&world, pillar), Vector3::new(-5.0, 3.0, 2.0));
    assert_eq!(position(&world, left_alone), Vector3::new(0.4, 0.4, 0.4));
    let requests = plugin.take_requests();
    assert_eq!(requests.len(), 1);
    let Request::Waypoint(waypoint) = &requests[0] else {
        panic!("not a waypoint");
    };
    assert_eq!(waypoint.name, "Align to Grid");
    assert_eq!(waypoint.changes.len(), 2);
    let snapshot = waypoint.world.as_ref().unwrap();
    assert_eq!(position(snapshot, pillar), Vector3::new(-5.0, 3.0, 2.0));
    let printed: Vec<_> = plugin.take_output().into_iter().map(|l| l.text).collect();
    assert_eq!(printed, ["Aligned 2 part(s) to the grid"]);
}

#[test]
fn a_dock_widget_with_a_slider_and_a_button_works() {
    let found = find_plugins(Some(&example()), None).remove(1);
    let mut world = World::new();
    let crate_part = part(&mut world, "Crate", Vector3::new(0.0, 0.0, 0.0));
    let mut plugin = start(&mut world, &found, &example());
    let widgets = plugin.widgets();
    assert_eq!(widgets.len(), 1);
    assert_eq!(widgets[0].title, "Part Tools");
    let gui = widgets[0].gui;
    // Outside the tree: never saved, shown in the Explorer or undone.
    assert_eq!(world.get(gui).unwrap().class(), ClassName::ScreenGui);
    assert_eq!(world.get(gui).unwrap().parent(), None);
    let descendants = world.descendants(gui);
    let find = |class| {
        descendants
            .iter()
            .copied()
            .find(|&id| world.get(id).unwrap().class() == class)
            .unwrap()
    };
    let (slider, apply, label) = (
        find(ClassName::Slider),
        find(ClassName::TextButton),
        find(ClassName::TextLabel),
    );
    let text = |world: &World| world.get(label).unwrap().property("Text").cloned();
    assert_eq!(
        text(&world),
        Some(Value::String("Transparency: 50%".into()))
    );

    // Dragging the slider sets its Value; the plugin hears ValueChanged.
    world.take_changes();
    world
        .set_property(slider, "Value", Value::Number(80.0))
        .unwrap();
    let changes = world.take_changes();
    plugin.fire_changes(&mut world, &changes);
    assert_eq!(
        text(&world),
        Some(Value::String("Transparency: 80%".into()))
    );
    assert_eq!(
        plugin.take_settings().unwrap().get("transparency"),
        Some(&Json::from(80))
    );

    plugin.set_selection(&mut world, &[crate_part]);
    world.take_changes();
    plugin.activate_button(&mut world, apply);
    assert_eq!(
        world.get(crate_part).unwrap().property("Transparency"),
        Some(&Value::Number(0.8))
    );
    assert!(errors(&plugin).is_empty());

    // Stopping it takes its UI away.
    plugin = start(&mut world, &found, &example());
    let second = plugin.widgets()[0].gui;
    plugin.stop(&mut world);
    assert!(!world.contains(second));
}

#[test]
fn errors_name_the_file_and_line_and_the_plugin_keeps_working() {
    let folder = temp_folder("errors");
    let found = project_with_plugin(
        &folder,
        "broken",
        MANIFEST,
        "local bar = plugin:CreateToolbar('Broken')\n\
         local boom = bar:CreateButton('Boom')\n\
         boom.Click:Connect(function()\n\
         \terror('it broke')\n\
         end)\n\
         local fine = bar:CreateButton('Fine')\n\
         fine.Click:Connect(function() print('still here') end)\n",
    );
    let mut world = World::new();
    let plugin = start(&mut world, &found, &folder);
    let buttons = plugin.toolbars()[0].buttons.clone();
    plugin.click(&mut world, buttons[0].id);
    let errors = errors(&plugin);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        errors[0].starts_with("plugins/broken/init.lua:4:"),
        "{}",
        errors[0]
    );
    assert!(errors[0].contains("it broke"));
    plugin.click(&mut world, buttons[1].id);
    let printed: Vec<_> = plugin.take_output().into_iter().map(|l| l.text).collect();
    assert_eq!(printed, ["still here"]);

    // Code that doesn't compile doesn't start, with its file and line.
    let bad = project_with_plugin(&folder, "syntax", MANIFEST, "local x = \nlocal = 2\n");
    let Err(message) = Plugin::start(
        &mut world,
        &bad,
        Start {
            shown: "plugins/syntax".into(),
            ..Start::default()
        },
    ) else {
        panic!("it started");
    };
    assert!(
        message.starts_with("plugins/syntax/init.lua:2:"),
        "{message}"
    );
    // So does a bad plugin.json.
    let unnamed = project_with_plugin(&folder, "unnamed", r#"{ "version": "1" }"#, "");
    assert!(unnamed.manifest.is_err());
    assert!(Plugin::start(&mut world, &unnamed, Start::default()).is_err());
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_plugin_reads_and_writes_only_inside_the_project_with_the_files_permission() {
    let root = temp_folder("files");
    let folder = root.join("game");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("notes.txt"), "hello").unwrap();
    std::fs::write(root.join("secret.txt"), "private").unwrap();
    let found = project_with_plugin(
        &folder,
        "reader",
        FILES_MANIFEST,
        "local function try(f, ...)\n\
         \tlocal ok, result = pcall(f, plugin, ...)\n\
         \tprint(ok, result)\n\
         end\n\
         try(plugin.ReadProjectFile, 'notes.txt')\n\
         try(plugin.WriteProjectFile, 'out/made.txt', 'written')\n\
         try(plugin.ReadProjectFile, '../secret.txt')\n\
         try(plugin.ReadProjectFile, 'plugins/../../secret.txt')\n\
         try(plugin.WriteProjectFile, '../escaped.txt', 'no')\n",
    );
    let mut world = World::new();
    let plugin = start(&mut world, &found, &folder);
    let printed: Vec<_> = plugin.take_output().into_iter().map(|l| l.text).collect();
    assert_eq!(printed[0], "true\thello");
    assert_eq!(printed[1], "true\tnil");
    assert_eq!(
        std::fs::read_to_string(folder.join("out/made.txt")).unwrap(),
        "written"
    );
    for refused in &printed[2..] {
        assert!(refused.starts_with("false\t"), "{refused}");
        assert!(refused.contains("is outside the project"), "{refused}");
    }
    assert!(!root.join("escaped.txt").exists());
    // An absolute path is refused too.
    let outside = root.join("secret.txt").to_string_lossy().replace('\\', "/");
    assert!(files::project_file(&folder, &outside).is_err());

    // Without the permission, nothing.
    let closed = project_with_plugin(
        &folder,
        "closed",
        MANIFEST,
        "print(pcall(plugin.ReadProjectFile, plugin, 'notes.txt'))",
    );
    let plugin = start(&mut world, &closed, &folder);
    let printed: Vec<_> = plugin.take_output().into_iter().map(|l| l.text).collect();
    assert!(printed[0].starts_with("false\t"), "{}", printed[0]);
    assert!(printed[0].contains("can't use files"), "{}", printed[0]);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn settings_selection_destroy_and_the_mouse() {
    let folder = temp_folder("api");
    let found = project_with_plugin(
        &folder,
        "api",
        MANIFEST,
        "print(plugin:GetSetting('count'))\n\
         plugin:SetSetting('count', (plugin:GetSetting('count') or 0) + 1)\n\
         plugin:SetSetting('list', { 'a', 'b' })\n\
         Selection.SelectionChanged:Connect(function()\n\
         \tlocal selected = Selection:Get()\n\
         \tprint('selected', #selected, selected[1] and selected[1].Name)\n\
         \tif selected[1] then selected[1]:Destroy() end\n\
         end)\n\
         local mouse = plugin:GetMouse()\n\
         mouse.Button1Down:Connect(function()\n\
         \tprint('down', mouse.Hit.Position, mouse.Target and mouse.Target.Name)\n\
         \tSelection:Set({ mouse.Target })\n\
         end)\n\
         plugin:Activate(true)\n\
         print(tostring(game:GetService('ChangeHistoryService')))\n",
    );
    let mut world = World::new();
    let pad = part(&mut world, "Pad", Vector3::new(0.0, 0.0, 0.0));
    let crate_part = part(&mut world, "Crate", Vector3::new(4.0, 0.0, 0.0));
    let mut settings = Settings::new();
    settings.insert("count".into(), Json::from(2));
    world.record_changes();
    let plugin = Plugin::start(
        &mut world,
        &found,
        Start {
            settings,
            shown: "plugins/api".into(),
            ..Start::default()
        },
    )
    .unwrap();
    assert!(plugin.mouse_active());
    let saved = plugin.take_settings().unwrap();
    assert_eq!(saved["count"], Json::from(3));
    assert_eq!(saved["list"], serde_json::json!(["a", "b"]));

    plugin.mouse(
        &mut world,
        MouseInput {
            state: MouseState {
                hit: Vector3::new(1.0, 2.0, 3.0),
                target: Some(pad),
            },
            pressed: true,
            ..MouseInput::default()
        },
    );
    let requests = plugin.take_requests();
    assert!(matches!(&requests[..], [Request::Select(ids)] if ids == &[pad]));

    // The plugin's own Set is what it knows already: no SelectionChanged.
    plugin.set_selection(&mut world, &[pad]);
    // Destroy only takes a part out of the tree, so it can be undone.
    plugin.set_selection(&mut world, &[crate_part]);
    assert!(world.contains(crate_part));
    assert_eq!(world.get(crate_part).unwrap().parent(), None);
    let printed: Vec<_> = plugin.take_output().into_iter().map(|l| l.text).collect();
    assert_eq!(
        printed,
        [
            "2",
            "ChangeHistoryService",
            "down\t1, 2, 3\tPad",
            "selected\t1\tCrate"
        ],
    );
    let _ = std::fs::remove_dir_all(&folder);
}

/// The luau-lsp file names every global, method, field and event the
/// plugin API has.
#[test]
fn the_definitions_name_the_whole_plugin_api() {
    let names = [
        "declare plugin: Plugin",
        "declare Selection: Selection",
        "declare ChangeHistoryService: ChangeHistoryService",
        "function CreateToolbar(",
        "function CreateButton(",
        "function CreateDockWidget(",
        "function GetSetting(",
        "function SetSetting(",
        "function ReadProjectFile(",
        "function WriteProjectFile(",
        "function GetMouse(",
        "function Activate(",
        "function Deactivate(",
        "function IsActivated(",
        "function SetActive(",
        "function SetWaypoint(",
        "function Undo(",
        "function Redo(",
        "function Get(",
        "function Set(",
        "function Connect(",
        "function Once(",
        "function Disconnect(",
        "Click: PluginSignal",
        "Enabled: boolean",
        "Deactivation: PluginSignal",
        "SelectionChanged: PluginSignal",
        "Button1Down: PluginSignal",
        "Button1Up: PluginSignal",
        "Move: PluginSignal",
        "Hit: CFrame",
        "Target: Instance?",
        "Connected: boolean",
    ];
    for name in names {
        assert!(
            PLUGIN_DEFINITIONS.contains(name),
            "{name} isn't in {PLUGIN_DEFINITIONS_PATH}"
        );
    }
    // Every method a plugin's Lua can call is in the file.
    let api = include_str!("api.rs");
    for line in api.lines() {
        let line = line.trim();
        for start in ["methods.add_method(\"", "fields.add_field_method_get(\""] {
            if let Some(rest) = line.strip_prefix(start) {
                let name = rest.split('"').next().unwrap();
                assert!(
                    PLUGIN_DEFINITIONS.contains(&format!("function {name}("))
                        || PLUGIN_DEFINITIONS.contains(&format!("	{name}: ")),
                    "{name} isn't in {PLUGIN_DEFINITIONS_PATH}"
                );
            }
        }
    }
}
