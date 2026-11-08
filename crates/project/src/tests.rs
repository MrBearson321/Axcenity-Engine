use super::*;
use axcenity_core::{ClassName, Material, PartType, ToneMapping, Value, World};

fn examples_folder() -> PathBuf {
    // CARGO_MANIFEST_DIR is this crate's folder: crates/project.
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

#[test]
fn hello_part_example_loads() {
    let project = load(&examples_folder().join("hello-part")).unwrap();
    assert_eq!(project.manifest.name, "Hello Part");
    assert!(project.problems.is_empty() && project.world_loaded);

    let world = &project.world;
    let parts: Vec<_> = world
        .descendants(world.workspace())
        .into_iter()
        .filter(|&id| world.get(id).unwrap().class() == ClassName::Part)
        .collect();
    assert_eq!(parts.len(), 1);
}

#[test]
fn hello_ui_example_loads_with_client_script_and_stylesheet() {
    let project = load(&examples_folder().join("hello-ui")).unwrap();
    assert!(project.problems.is_empty() && project.world_loaded);
    assert_eq!(project.scripts.len(), 1);
    assert_eq!(project.scripts[0].path, "scripts/menu.client.lua");
    assert_eq!(project.stylesheets.len(), 1);
    assert!(
        project
            .world
            .descendants(project.world.workspace())
            .into_iter()
            .filter_map(|id| project.world.get(id))
            .any(|instance| instance.class() == ClassName::TextButton)
    );
}

#[test]
fn splash_defaults_on_and_can_be_changed_in_the_manifest() {
    let folder = project_with_scripts("splash-setting", &[]);
    assert!(load(&folder).unwrap().manifest.show_splash);

    set_show_splash(&folder, false).unwrap();
    assert!(!load(&folder).unwrap().manifest.show_splash);

    set_show_splash(&folder, true).unwrap();
    assert!(load(&folder).unwrap().manifest.show_splash);
    fs::remove_dir_all(folder).unwrap();
}

#[test]
fn the_developer_console_defaults_off_and_can_be_turned_on_in_the_manifest() {
    let folder = project_with_scripts("developer-console-setting", &[]);
    assert!(!load(&folder).unwrap().manifest.developer_console);

    set_developer_console(&folder, true).unwrap();
    assert!(load(&folder).unwrap().manifest.developer_console);
    fs::remove_dir_all(folder).unwrap();
}

#[test]
fn the_loading_screen_defaults_to_the_splash_and_keeps_a_chosen_screen_gui() {
    let folder = project_with_scripts("loading-screen-setting", &[]);
    assert_eq!(
        load(&folder).unwrap().manifest.loading_screen,
        LoadingScreen::default()
    );

    let chosen = LoadingScreen {
        screen_gui: "Loading".to_owned(),
        min_seconds: 1.5,
        wait_for_script: true,
    };
    set_loading_screen(&folder, &chosen).unwrap();
    let manifest = load(&folder).unwrap().manifest;
    assert_eq!(manifest.loading_screen, chosen);
    assert!(manifest.show_splash, "other settings are kept");
    fs::remove_dir_all(folder).unwrap();
}

#[test]
fn project_without_a_scripts_folder_has_no_scripts() {
    let folder = project_with_scripts("no-scripts", &[]);
    let project = load(&folder).unwrap();
    fs::remove_dir_all(folder).unwrap();
    assert!(project.scripts.is_empty());
}

#[test]
fn new_stylesheets_never_replace_a_file() {
    let folder = project_with_scripts("new-stylesheet", &[]);
    let path = create_stylesheet(&folder, "Stylesheet", NEW_STYLESHEET_SOURCE).unwrap();
    assert_eq!(path, "Stylesheet.css");
    assert!(is_stylesheet(&path));
    assert!(
        create_stylesheet(&folder, "Stylesheet", "")
            .unwrap_err()
            .already_exists()
    );
    write_stylesheet(&folder, &path, ".hud { width: 10px }").unwrap();
    assert_eq!(
        load(&folder).unwrap().stylesheets[0].source,
        ".hud { width: 10px }"
    );
    fs::remove_dir_all(folder).unwrap();
}

#[test]
fn project_loads_nested_css_files_in_stable_order() {
    let folder = project_with_scripts("stylesheets", &[]);
    fs::write(folder.join("base.css"), ".root { opacity: 1 }").unwrap();
    fs::create_dir_all(folder.join("styles")).unwrap();
    fs::write(folder.join("styles/hud.CSS"), ".hud { display: flex }").unwrap();

    let project = load(&folder).unwrap();
    assert_eq!(
        project
            .stylesheets
            .iter()
            .map(|sheet| sheet.path.as_str())
            .collect::<Vec<_>>(),
        ["base.css", "styles/hud.CSS"]
    );
    assert_eq!(project.stylesheets[1].source, ".hud { display: flex }");
    fs::remove_dir_all(folder).unwrap();
}

/// A project with a manifest, an empty world and these script files.
fn project_with_scripts(name: &str, scripts: &[(&str, &[u8])]) -> PathBuf {
    let folder = temp_folder(name);
    fs::write(
        folder.join(MANIFEST_FILE),
        format!(r#"{{"name": "T", "format_version": {FORMAT_VERSION}}}"#),
    )
    .unwrap();
    fs::write(folder.join(WORLD_FILE), r#"{"format_version": 1}"#).unwrap();
    for (path, text) in scripts {
        let path = folder.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    folder
}

#[test]
fn server_scripts_become_scripts_and_client_scripts_local_scripts() {
    let folder = project_with_scripts(
        "scripts",
        &[
            ("scripts/main.server.lua", b"print('main')"),
            ("scripts/doors/open.server.lua", b"print('door')"),
            ("scripts/notes.txt", b"not a script"),
            ("scripts/camera.client.lua", b"print('camera')"),
            ("scripts/a/keys.client.lua", b"print('keys')"),
        ],
    );
    let scripts = folder.join(SCRIPTS_FOLDER);

    let project = load(&folder);
    // Edited after loading: `read_script` gets the new text by its path.
    fs::write(scripts.join("main.server.lua"), "print('edited')").unwrap();
    let reread = read_script(&folder, "scripts/main.server.lua");
    fs::remove_dir_all(&folder).unwrap();
    let project = project.unwrap();

    // Server scripts start first, then client scripts, each by path.
    let paths: Vec<&str> = project.scripts.iter().map(|s| s.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "scripts/doors/open.server.lua",
            "scripts/main.server.lua",
            "scripts/a/keys.client.lua",
            "scripts/camera.client.lua",
        ]
    );
    assert_eq!(project.scripts[1].source, "print('main')");
    assert_eq!(reread.unwrap(), "print('edited')");

    let world = &project.world;
    let names = |service| -> Vec<&str> {
        world
            .get(service)
            .unwrap()
            .children()
            .iter()
            .map(|&id| world.get(id).unwrap().name())
            .collect()
    };
    assert_eq!(names(world.server_script_service()), ["open", "main"]);
    assert_eq!(names(world.starter_player_scripts()), ["keys", "camera"]);
    for script in &project.scripts {
        let instance = world.get(script.instance).unwrap();
        let class = script_class(&script.path);
        assert_eq!(instance.class(), class);
        assert_eq!(instance.parent(), Some(script_service(world, class)));
    }
}

#[test]
fn module_files_become_module_scripts_in_replicated_storage_and_load_first() {
    let folder = project_with_scripts(
        "modules",
        &[
            ("scripts/main.server.lua", b"print('main')"),
            ("scripts/shared/utils.module.lua", b"return {}"),
            ("scripts/keys.client.lua", b"print('keys')"),
        ],
    );
    let project = load(&folder);
    fs::remove_dir_all(&folder).unwrap();
    let project = project.unwrap();

    let paths: Vec<&str> = project.scripts.iter().map(|s| s.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "scripts/shared/utils.module.lua",
            "scripts/main.server.lua",
            "scripts/keys.client.lua",
        ]
    );
    let world = &project.world;
    let utils = world.get(project.scripts[0].instance).unwrap();
    assert_eq!(utils.class(), ClassName::ModuleScript);
    assert_eq!(utils.name(), "utils");
    assert_eq!(utils.parent(), Some(world.replicated_storage()));
    // ModuleScripts are files, so the world file doesn't save them.
    assert!(!write_world(world).contains("utils"));
    assert_eq!(
        script_path("utils", ClassName::ModuleScript),
        "scripts/utils.module.lua"
    );
}

#[test]
fn every_template_makes_a_project_that_loads_cleanly() {
    let parent = temp_folder("templates");
    for template in Template::ALL {
        let folder = create_project_from(&parent, template.name(), template).unwrap();
        let project = load(&folder).unwrap();
        assert!(
            project.problems.is_empty(),
            "{template:?}: {:?}",
            project.problems
        );
        assert!(project.world_loaded, "{template:?}");
        assert_eq!(
            tone_mapping(&project.world),
            ToneMapping::Neutral,
            "{template:?}"
        );
        let parts = project.world.descendants(project.world.workspace()).len();
        match template {
            Template::Empty => assert_eq!(parts, 0),
            Template::Baseplate => assert_eq!(parts, 1),
            Template::Starter => {
                assert!(parts > 3);
                let script = &project.scripts[0];
                assert!(script.source.contains("workspace.Spinner.Orientation"));
                assert!(script.source.contains("workspace.Pad.Touched:Connect"));
            }
            // The genre templates' own tests are `axcenity-testing`'s
            // `templates` tests; here, what they all have.
            _ => {
                assert!(parts > 10, "{template:?}");
                let menu = project
                    .world
                    .find_first_child(project.world.workspace(), "GameMenu")
                    .unwrap();
                // The placed copy is filled in from the shared prefab.
                assert!(project.world.find_first_child(menu, "Pause").is_some());
                assert!(project.world.find_first_child(menu, "Settings").is_some());
                let paths: Vec<_> = project.scripts.iter().map(|s| s.path.as_str()).collect();
                for shared in [
                    "scripts/GameMenu.client.lua",
                    "scripts/GameSaves.server.lua",
                    "scripts/LocalGui.module.lua",
                    "scripts/MenuSettings.module.lua",
                ] {
                    assert!(paths.contains(&shared), "{template:?}: {paths:?}");
                }
                assert!(
                    paths.iter().any(|path| path.ends_with(".blocks.json")),
                    "{template:?} has a block script: {paths:?}"
                );
                assert!(folder.join("tests/menu.spec.lua").exists());
                assert!(folder.join(THUMBNAIL_FILE).exists());
            }
        }
    }
    fs::remove_dir_all(parent).unwrap();
}

/// A genre template's own file wins over the shared one of the same path:
/// the Arena's Save keeps players' stats, not a world slot.
#[test]
fn a_genre_templates_own_files_replace_the_shared_ones() {
    let parent = temp_folder("template-files");
    let arena = create_project_from(&parent, "Arena", Template::Arena).unwrap();
    let saves = fs::read_to_string(arena.join("scripts/GameSaves.server.lua")).unwrap();
    assert!(saves.contains("stats.save(player)"));
    let first_person = create_project_from(&parent, "Shooter", Template::FirstPerson).unwrap();
    let saves = fs::read_to_string(first_person.join("scripts/GameSaves.server.lua")).unwrap();
    assert!(saves.contains("SaveService:SaveSlot(slot)"));
    // The manifest is the new project's, whatever the template.
    let manifest = read_manifest(&arena).unwrap();
    assert_eq!(manifest.name, "Arena");
    fs::remove_dir_all(parent).unwrap();
}

/// Each genre template's picture for New Project is a 320 × 180 PNG.
#[test]
fn every_genre_template_has_a_320_by_180_picture() {
    for template in Template::GAMES {
        let png = template.picture().unwrap_or_else(|| panic!("{template:?}"));
        assert_eq!(&png[1..4], b"PNG", "{template:?}");
        // The IHDR chunk's width and height, big-endian, after the
        // 8-byte signature and the chunk's length and type.
        let width = u32::from_be_bytes(png[16..20].try_into().unwrap());
        let height = u32::from_be_bytes(png[20..24].try_into().unwrap());
        assert_eq!((width, height), (320, 180), "{template:?}");
    }
    for template in Template::BLANK {
        assert!(template.picture().is_none());
        assert!(template.files().is_empty());
    }
}

fn tone_mapping(world: &World) -> ToneMapping {
    world
        .get(world.lighting())
        .and_then(|lighting| lighting.property("ToneMapping"))
        .and_then(Value::as_tone_mapping)
        .unwrap()
}

/// Worlds saved before tone mapping could be chosen have no `ToneMapping`,
/// and keep the Classic curve they were made with, through a save too.
#[test]
fn a_world_saved_without_tone_mapping_opens_with_classic() {
    let parent = temp_folder("classic-tone-mapping");
    let folder = create_project(&parent, "Old").unwrap();
    fs::write(
        folder.join(WORLD_FILE),
        r#"{"format_version": 1, "lighting": {"properties": {"Exposure": 1.5}}}"#,
    )
    .unwrap();
    let project = load(&folder).unwrap();
    assert!(project.problems.is_empty(), "{:?}", project.problems);
    assert_eq!(tone_mapping(&project.world), ToneMapping::Classic);
    save_world(&folder, &project.world).unwrap();
    assert_eq!(
        tone_mapping(&load(&folder).unwrap().world),
        ToneMapping::Classic
    );
    fs::remove_dir_all(parent).unwrap();
}

/// Lighting's time of day and an Atmosphere inside it are saved with the
/// world and come back when it's opened. A world without them is written
/// as before: no `lighting` at all.
#[test]
fn the_time_of_day_and_an_atmosphere_save_with_the_world() {
    let mut world = World::new();
    assert!(!write_world(&world).contains("lighting"));
    let lighting = world.lighting();
    world
        .set_property(lighting, "ClockTime", Value::Number(17.5))
        .unwrap();
    let atmosphere = world.create(ClassName::Atmosphere).unwrap();
    world.set_parent(atmosphere, Some(lighting)).unwrap();
    world
        .set_property(atmosphere, "Haze", Value::Number(2.5))
        .unwrap();

    let parent = temp_folder("atmosphere");
    let folder = create_project(&parent, "Dusk").unwrap();
    save_world(&folder, &world).unwrap();
    let project = load(&folder).unwrap();
    assert!(project.problems.is_empty(), "{:?}", project.problems);
    let world = &project.world;
    let lighting = world.get(world.lighting()).unwrap();
    assert_eq!(lighting.property("ClockTime"), Some(&Value::Number(17.5)));
    let atmosphere = world
        .find_first_child(world.lighting(), "Atmosphere")
        .and_then(|id| world.get(id))
        .expect("the Atmosphere comes back");
    assert_eq!(atmosphere.class(), ClassName::Atmosphere);
    assert_eq!(atmosphere.property("Haze"), Some(&Value::Number(2.5)));

    // An Atmosphere alone (every setting its default) is saved too.
    let mut only = World::new();
    let air = only.create(ClassName::Atmosphere).unwrap();
    only.set_parent(air, Some(only.lighting())).unwrap();
    let text = write_world(&only);
    let back = parse_world(&text).unwrap();
    assert!(
        back.find_first_child(back.lighting(), "Atmosphere")
            .is_some(),
        "{text}"
    );
    fs::remove_dir_all(parent).unwrap();
}

#[test]
fn a_new_project_loads_and_its_settings_can_change() {
    let parent = temp_folder("new-project");
    let folder = create_project(&parent, "My: Game").unwrap();
    assert_eq!(folder.file_name().unwrap(), "My- Game");
    assert!(
        create_project(&parent, "My: Game")
            .unwrap_err()
            .already_exists()
    );
    assert!(create_project(&parent, "  ").is_err());

    set_name(&folder, "Renamed").unwrap();
    set_description(&folder, "A test").unwrap();
    set_show_splash(&folder, false).unwrap();
    let manifest = read_manifest(&folder).unwrap();
    assert_eq!(manifest.name, "Renamed");
    assert_eq!(manifest.description, "A test");
    assert!(!manifest.show_splash);
    assert_eq!(manifest.format_version, FORMAT_VERSION);
    let project = load(&folder).unwrap();
    assert!(project.migrated_from.is_none());
    assert!(project.migration_backup.is_none());
    assert!(project.problems.is_empty(), "{:?}", project.problems);
    assert!(project.world_loaded);
    let _ = fs::remove_dir_all(&parent);
}

/// An empty folder in the system's temporary folder, unique to this test run.
fn temp_folder(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!("axcenity-{name}-test-{}", std::process::id()));
    // Left over from an earlier run that failed partway, if it exists.
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).unwrap();
    folder
}

#[test]
fn missing_folder_names_the_manifest() {
    let err = load(&examples_folder().join("does-not-exist"))
        .err()
        .unwrap();
    assert!(matches!(err, LoadError::Read { .. }));
    assert!(err.to_string().contains(MANIFEST_FILE));
}

#[test]
fn project_names_come_from_the_manifest() {
    let examples = examples_folder();
    assert_eq!(
        project_name(&examples.join("hello-touched")).as_deref(),
        Some("Hello Touched")
    );
    assert_eq!(project_name(&examples.join("does-not-exist")), None);
}

#[test]
fn a_corrupted_world_file_is_reported_and_the_scripts_still_load() {
    let folder = project_with_scripts("broken-world", &[("scripts/a.server.lua", b"print('a')")]);
    fs::write(
        folder.join(WORLD_FILE),
        "{\n  \"format_version\": 1,\n  oops\n}",
    )
    .unwrap();
    let project = load(&folder);
    fs::remove_dir_all(&folder).unwrap();
    let project = project.unwrap();

    assert!(!project.world_loaded);
    assert_eq!(project.problems.len(), 1);
    let problem = format!("{:#}", anyhow_chain(&project.problems[0]));
    assert!(
        problem.contains("world.json") && problem.contains("line 3"),
        "{problem}"
    );
    // The Workspace is empty, and the script is there as usual.
    let world = &project.world;
    assert!(world.get(world.workspace()).unwrap().children().is_empty());
    assert_eq!(project.scripts.len(), 1);
    assert_eq!(project.scripts[0].source, "print('a')");
}

#[test]
fn a_corrupted_script_is_reported_and_skipped_and_the_rest_still_load() {
    // 0xFF never appears in UTF-8 text, so this file can't be read as text.
    let folder = project_with_scripts(
        "broken-script",
        &[
            ("scripts/a.server.lua", b"print('a')"),
            ("scripts/broken.server.lua", b"print('\xFF\xFE')"),
            ("scripts/c.server.lua", b"print('c')"),
        ],
    );
    let project = load(&folder);
    fs::remove_dir_all(&folder).unwrap();
    let project = project.unwrap();

    assert!(project.world_loaded);
    let paths: Vec<&str> = project.scripts.iter().map(|s| s.path.as_str()).collect();
    assert_eq!(paths, ["scripts/a.server.lua", "scripts/c.server.lua"]);
    assert_eq!(project.problems.len(), 1);
    let problem = format!("{:#}", anyhow_chain(&project.problems[0]));
    assert!(problem.contains("broken.server.lua"), "{problem}");
    let service = project.world.server_script_service();
    assert_eq!(project.world.get(service).unwrap().children().len(), 2);
}

#[test]
fn a_corrupted_manifest_is_reported_and_the_project_takes_its_folders_name() {
    let folder = project_with_scripts("broken-manifest", &[]);
    fs::write(folder.join(MANIFEST_FILE), "{ \"name\": ").unwrap();
    let project = load(&folder);
    fs::remove_dir_all(&folder).unwrap();
    let project = project.unwrap();
    assert_eq!(
        project.manifest.name,
        folder.file_name().unwrap().to_string_lossy()
    );
    assert!(matches!(project.problems[..], [LoadError::Manifest { .. }]));
    assert!(project.world_loaded);
}

#[test]
fn a_newer_project_still_refuses_to_load() {
    let folder = project_with_scripts("too-new", &[]);
    fs::write(
        folder.join(MANIFEST_FILE),
        r#"{"name": "T", "format_version": 99}"#,
    )
    .unwrap();
    let result = load(&folder);
    fs::remove_dir_all(&folder).unwrap();
    assert!(matches!(result, Err(LoadError::TooNew { found: 99, .. })));
}

/// The error with its causes, the way the editor and player print them.
fn anyhow_chain(err: &LoadError) -> String {
    let mut text = err.to_string();
    let mut source = std::error::Error::source(err);
    while let Some(cause) = source {
        text.push_str(&format!(": {cause}"));
        source = cause.source();
    }
    text
}

#[test]
fn scripts_are_created_renamed_and_deleted_as_files() {
    let folder = project_with_scripts("script-files", &[]);
    let server = ClassName::Script;
    let path = create_script(&folder, "Script", server, "print('hi')").unwrap();
    assert_eq!(path, "scripts/Script.server.lua");
    // Never replaces a file that's already there.
    let again = create_script(&folder, "Script", server, "print('other')").unwrap_err();
    assert!(again.already_exists());

    write_script(&folder, &path, "print('edited')").unwrap();
    assert_eq!(read_script(&folder, &path).unwrap(), "print('edited')");

    // A LocalScript's file keeps its `.client.lua` when renamed.
    let client = create_script(&folder, "Keys", ClassName::LocalScript, "").unwrap();
    assert_eq!(client, "scripts/Keys.client.lua");
    let client = rename_script(&folder, &client, "Input").unwrap();
    assert_eq!(client, "scripts/Input.client.lua");
    delete_script(&folder, &client).unwrap();

    let other = create_script(&folder, "Other", server, "").unwrap();
    assert!(
        rename_script(&folder, &path, "Other")
            .unwrap_err()
            .already_exists()
    );
    let renamed = rename_script(&folder, &path, "Door").unwrap();
    assert_eq!(renamed, "scripts/Door.server.lua");
    // Only the capitals change: that's the same file, so it's allowed.
    let capitals = rename_script(&folder, &renamed, "DOOR").unwrap();
    assert_eq!(capitals, "scripts/DOOR.server.lua");

    delete_script(&folder, &other).unwrap();
    let mut problems = Vec::new();
    let found = find_scripts(&folder, &mut problems);
    let text = read_script(&folder, &capitals);
    let leftover = folder.join("scripts/DOOR.server.lua.saving").exists();
    fs::remove_dir_all(&folder).unwrap();

    assert_eq!(found, ["scripts/DOOR.server.lua"]);
    assert_eq!(text.unwrap(), "print('edited')");
    assert!(problems.is_empty() && !leftover);
}

#[test]
fn block_scripts_load_as_lua_scripts_and_keep_their_class_when_renamed() {
    let folder = project_with_scripts("block-script-files", &[]);
    let script = BlockScript {
        blocks: vec![Block::new(BlockKind::WhenStarted {
            body: vec![Block::new(BlockKind::PlaySound {
                target: "Music".to_owned(),
            })],
        })],
        ..Default::default()
    };
    let path = create_block_script(&folder, "Music", ClassName::LocalScript, &script).unwrap();
    assert_eq!(path, "scripts/Music.client.blocks.json");

    let loaded = load(&folder).unwrap();
    assert_eq!(loaded.block_scripts.len(), 1);
    assert_eq!(loaded.scripts.len(), 1);
    assert_eq!(
        loaded
            .world
            .get(loaded.scripts[0].instance)
            .unwrap()
            .class(),
        ClassName::LocalScript
    );
    assert!(loaded.scripts[0].source.contains("workspace.Music:Play()"));

    let renamed = rename_block_script(&folder, &path, "Background").unwrap();
    assert_eq!(renamed, "scripts/Background.client.blocks.json");
    assert!(!folder.join(&path).exists());
    assert_eq!(read_block_script(&folder, &renamed).unwrap(), script);
    delete_script(&folder, &renamed).unwrap();
    fs::remove_dir_all(folder).unwrap();
}

#[test]
fn script_names_must_work_as_file_names_on_every_os() {
    for good in ["Script", "door opener", "Pad2", "über", "a.b"] {
        assert_eq!(check_script_name(good), Ok(()), "{good}");
    }
    for bad in [
        "",
        "  ",
        "a/b",
        r"a\b",
        "what?",
        "x:y",
        "end.",
        "end ",
        " start",
        "CON",
        "com1",
        "Lpt9.txt",
        "tab\there",
    ] {
        assert!(check_script_name(bad).is_err(), "{bad:?}");
    }
    assert_eq!(script_name("scripts/doors/open.server.lua"), "open");
    assert_eq!(script_name("scripts/keys.client.lua"), "keys");
    assert_eq!(
        script_path("open", ClassName::Script),
        "scripts/open.server.lua"
    );
    assert_eq!(
        script_path("keys", ClassName::LocalScript),
        "scripts/keys.client.lua"
    );
}

#[test]
fn the_world_file_is_kept_under_a_free_name() {
    let folder = temp_folder("damaged-world");
    assert_eq!(keep_world_file(&folder).unwrap(), None);
    fs::write(folder.join(WORLD_FILE), "broken one").unwrap();
    let first = keep_world_file(&folder).unwrap();
    fs::write(folder.join(WORLD_FILE), "broken two").unwrap();
    let second = keep_world_file(&folder).unwrap();
    let texts = (
        fs::read_to_string(folder.join(KEPT_WORLD_FILE)),
        fs::read_to_string(folder.join("world.backup2.json")),
    );
    let world_left = folder.join(WORLD_FILE).exists();
    fs::remove_dir_all(&folder).unwrap();

    assert_eq!(first.as_deref(), Some(KEPT_WORLD_FILE));
    assert_eq!(second.as_deref(), Some("world.backup2.json"));
    assert_eq!(texts.0.unwrap(), "broken one");
    assert_eq!(texts.1.unwrap(), "broken two");
    assert!(!world_left);
}

/// Every folder in `examples/` must load.
fn every_example() -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = fs::read_dir(examples_folder())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_dir())
        .collect();
    folders.sort();
    assert!(!folders.is_empty());
    folders
}

#[test]
fn every_example_loads_without_problems() {
    for folder in every_example() {
        let project = load(&folder).unwrap();
        assert!(
            project.problems.is_empty(),
            "{}: {:?}",
            folder.display(),
            project.problems
        );
    }
}

#[test]
fn every_example_round_trips_through_saving() {
    for folder in every_example() {
        let first = write_world(&load(&folder).unwrap().world);
        let second = write_world(&parse_world(&first).unwrap());
        assert_eq!(first, second, "{} changed when saved", folder.display());
    }
}

/// The primitives example doubles as a reference for the saved layout.
#[test]
fn primitives_example_is_already_in_saved_form() {
    let folder = examples_folder().join("primitives");
    // Git may check text files out with Windows line endings.
    let text = fs::read_to_string(folder.join(WORLD_FILE))
        .unwrap()
        .replace("\r\n", "\n");
    assert_eq!(
        write_world(&load(&folder).unwrap().world),
        text,
        "re-save examples/primitives/world.json with `project::save_world`"
    );
}

#[test]
fn primitives_example_has_every_shape_and_material() {
    let world = load(&examples_folder().join("primitives")).unwrap().world;
    let parts: Vec<_> = world
        .descendants(world.workspace())
        .into_iter()
        .map(|id| world.get(id).unwrap())
        .collect();
    for shape in PartType::ALL {
        let value = Value::PartType(shape);
        assert!(parts.iter().any(|p| p.property("Shape") == Some(&value)));
    }
    for material in Material::ALL {
        let value = Value::Material(material);
        assert!(parts.iter().any(|p| p.property("Material") == Some(&value)));
    }
}

#[test]
fn save_then_load_gives_the_same_world_file() {
    let folder = std::env::temp_dir().join(format!("axcenity-save-test-{}", std::process::id()));
    fs::create_dir_all(&folder).unwrap();
    let world = load(&examples_folder().join("primitives")).unwrap().world;

    save_world(&folder, &world).unwrap();
    let saved = fs::read_to_string(folder.join(WORLD_FILE)).unwrap();
    let leftover = folder.join(format!("{WORLD_FILE}.saving")).exists();
    fs::remove_dir_all(&folder).unwrap();

    assert_eq!(saved, write_world(&world));
    assert_eq!(write_world(&parse_world(&saved).unwrap()), saved);
    assert!(!leftover, "the temporary file was left behind");
}

#[test]
fn the_string_table_loads_into_the_world_and_survives_save_and_reload() {
    let folder = project_with_scripts("string-table", &[]);
    // No file: an empty table, and saving doesn't make one.
    let project = load(&folder).unwrap();
    assert!(project.world.strings().is_empty());
    save_strings(&folder, project.world.strings()).unwrap();
    assert!(!folder.join(STRINGS_FILE).exists());

    let mut table = axcenity_core::StringTable::default();
    table.add_language("English");
    table.add_language("Deutsch");
    table.set("menu.play", "English", "Play");
    table.set("menu.play", "Deutsch", "Spielen");
    table.set("menu.quit", "English", "Quit");
    save_strings(&folder, &table).unwrap();
    let text = fs::read_to_string(folder.join(STRINGS_FILE)).unwrap();
    assert_eq!(text, write_strings(&table));

    let reloaded = load(&folder).unwrap();
    assert!(reloaded.problems.is_empty());
    assert_eq!(**reloaded.world.strings(), table);
    assert_eq!(reloaded.world.language(), "English");
    // Its own file: the manifest's format doesn't change.
    assert_eq!(reloaded.manifest.format_version, FORMAT_VERSION);
    assert_eq!(
        read_manifest(&folder).unwrap().format_version,
        FORMAT_VERSION
    );

    // Emptied, the file stays, empty, so the emptying is saved too.
    save_strings(&folder, &axcenity_core::StringTable::default()).unwrap();
    assert!(load(&folder).unwrap().world.strings().is_empty());
    fs::remove_dir_all(folder).unwrap();
}

#[test]
fn a_damaged_string_table_is_reported_and_the_rest_loads() {
    let folder = project_with_scripts("damaged-string-table", &[]);
    fs::write(folder.join(STRINGS_FILE), "{ \"languages\": [").unwrap();
    let project = load(&folder).unwrap();
    assert!(project.world_loaded);
    assert!(project.world.strings().is_empty());
    assert!(
        matches!(project.problems.as_slice(), [LoadError::Strings { .. }]),
        "{:?}",
        project.problems
    );
    fs::remove_dir_all(folder).unwrap();
}

#[test]
fn hello_strings_loads_its_table_and_a_built_game_keeps_it() {
    let example = examples_folder().join("hello-strings");
    let project = load(&example).unwrap();
    assert!(project.problems.is_empty(), "{:?}", project.problems);
    let world = &project.world;
    assert_eq!(
        world.strings().languages,
        ["English", "Français", "Español"]
    );
    assert_eq!(world.translate("menu.play"), Some("Play"));

    let built = temp_folder("hello-strings-built");
    unpack_game(&pack_game(&example).unwrap(), &built).unwrap();
    assert!(built.join(STRINGS_FILE).is_file());
    assert_eq!(load(&built).unwrap().world.strings(), world.strings());
    fs::remove_dir_all(built).unwrap();
}

/// SoundGroups in SoundService, with their effects, save with the world,
/// and a Sound's `SoundGroup` points at the same group when it's opened
/// (Phase 52). A world without groups has no `sound_service` at all.
#[test]
fn sound_groups_and_their_effects_save_with_the_world() {
    let mut world = World::new();
    assert!(!write_world(&world).contains("sound_service"));
    let service = world.sound_service();
    let master = world.create(ClassName::SoundGroup).unwrap();
    world.set_name(master, "Master").unwrap();
    world.set_parent(master, Some(service)).unwrap();
    let music = world.create(ClassName::SoundGroup).unwrap();
    world.set_name(music, "Music").unwrap();
    world.set_parent(music, Some(master)).unwrap();
    world
        .set_property(music, "Volume", Value::Number(0.4))
        .unwrap();
    let muffle = world.create(ClassName::LowPassSoundEffect).unwrap();
    world.set_parent(muffle, Some(music)).unwrap();
    world
        .set_property(muffle, "Enabled", Value::Bool(false))
        .unwrap();
    let theme = world.create(ClassName::Sound).unwrap();
    world.set_parent(theme, Some(world.workspace())).unwrap();
    world
        .set_property(theme, "SoundGroup", Value::Instance(Some(music)))
        .unwrap();

    let text = write_world(&world);
    let back = parse_world(&text).unwrap();
    let master = back
        .find_first_child(back.sound_service(), "Master")
        .expect("Master comes back");
    let music = back.find_first_child(master, "Music").unwrap();
    assert_eq!(
        back.get(music).unwrap().property("Volume"),
        Some(&Value::Number(0.4))
    );
    let muffle = back
        .find_first_child(music, "LowPassSoundEffect")
        .and_then(|id| back.get(id))
        .unwrap();
    assert_eq!(muffle.property("Enabled"), Some(&Value::Bool(false)));
    let theme = back.find_first_child(back.workspace(), "Sound").unwrap();
    assert_eq!(
        back.get(theme).unwrap().property("SoundGroup"),
        Some(&Value::Instance(Some(music))),
        "{text}"
    );
}

/// The editor's plugins (`plugins/`, Phase 54) never reach the game: a
/// built game has none of their files, and their stylesheets don't style
/// the game.
#[test]
fn built_games_and_the_game_ui_have_no_plugin_files() {
    let example = examples_folder().join("hello-plugins");
    assert!(example.join("plugins/part-tools/widget.css").is_file());
    let project = load(&example).unwrap();
    assert!(project.problems.is_empty(), "{:?}", project.problems);
    let mut problems = Vec::new();
    let stylesheets = find_stylesheets(&example, &mut problems);
    assert!(
        stylesheets.iter().all(|path| !path.starts_with("plugins/")),
        "{stylesheets:?}"
    );

    let built = temp_folder("hello-plugins-built");
    unpack_game(&pack_game(&example).unwrap(), &built).unwrap();
    assert!(built.join(WORLD_FILE).is_file());
    assert!(!built.join(PLUGINS_FOLDER).exists());
    fs::remove_dir_all(built).unwrap();
}

/// World streaming's settings (Phase 80) are saved with the world, and a
/// world from before them has streaming off.
#[test]
fn streaming_settings_save_with_the_world_and_old_worlds_have_it_off() {
    let old = parse_world(r#"{"format_version": 1}"#).unwrap();
    assert_eq!(axcenity_core::streaming::Settings::of(&old), None);

    let mut world = World::new();
    assert!(!write_world(&world).contains("Streaming"));
    let workspace = world.workspace();
    world
        .set_property(workspace, "StreamingEnabled", Value::Bool(true))
        .unwrap();
    world
        .set_property(workspace, "StreamingTargetRadius", Value::Number(500.0))
        .unwrap();
    let model = world.create(ClassName::Model).unwrap();
    world
        .set_property(
            model,
            "ModelStreamingMode",
            axcenity_core::streaming::streaming_mode("Persistent"),
        )
        .unwrap();
    world.set_parent(model, Some(workspace)).unwrap();

    let text = write_world(&world);
    let back = parse_world(&text).unwrap();
    let settings = axcenity_core::streaming::Settings::of(&back).unwrap();
    assert_eq!(settings.target_radius, 500.0);
    let model = back.get(back.workspace()).unwrap().children()[0];
    assert_eq!(
        back.get(model).unwrap().property("ModelStreamingMode"),
        Some(&axcenity_core::streaming::streaming_mode("Persistent"))
    );
}

#[test]
fn hello_streaming_example_loads_with_streaming_on() {
    let project = load(&examples_folder().join("hello-streaming")).unwrap();
    assert!(
        project.problems.is_empty() && project.world_loaded,
        "{:?}",
        project.problems
    );
    let settings = axcenity_core::streaming::Settings::of(&project.world).unwrap();
    assert_eq!(settings.target_radius, 400.0);
}
