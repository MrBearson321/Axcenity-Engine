//! The MCP tools on copies of the examples, called as a client would:
//! a tool name and JSON arguments, checked by the same `Args::check`.

use axcenity_core::{ClassName, InstanceId};
use axcenity_mcp::{Args, Code, Mode, Profile, ToolError, text::estimate_tokens, tools_list_json};
use serde_json::{Value, json};

use super::super::EditorApp;
use super::super::tests::copy_of_example;
use super::refs::id_text;
use super::{Headless, TOOLS, run, tools_for};

struct Opened {
    app: Headless,
    folder: std::path::PathBuf,
}

impl Drop for Opened {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.folder);
    }
}

fn open(example: &str, test: &str) -> Opened {
    let folder = copy_of_example(example, test);
    let app = EditorApp::new(Some(&folder));
    assert!(app.project.is_some(), "{example} opens");
    Opened {
        app: Headless::new(app, Mode::Edit),
        folder,
    }
}

/// Calls `tool` with `args` as a client would: arguments checked first.
fn call(app: &mut Headless, tool: &str, args: Value) -> Result<String, ToolError> {
    let spec = TOOLS
        .iter()
        .find(|entry| entry.spec.name == tool)
        .unwrap_or_else(|| panic!("no tool {tool}"))
        .spec;
    let args = Args::new(args.as_object().cloned().unwrap_or_default());
    args.check(spec)?;
    run(&mut app.host(), spec, &args, Some("test"))
}

fn ok(app: &mut Headless, tool: &str, args: Value) -> String {
    call(app, tool, args).unwrap_or_else(|error| panic!("{tool} failed: {error}"))
}

fn add(host: &mut Headless, class: ClassName, name: &str, parent: InstanceId) -> InstanceId {
    let world = &mut host.app.world;
    let id = world.create(class).unwrap();
    world.set_name(id, name).unwrap();
    world.set_parent(id, Some(parent)).unwrap();
    id
}

#[test]
fn project_info_names_the_project_its_levels_and_counts() {
    let mut opened = open("hello-levels", "mcp-project-info");
    let text = ok(&mut opened.app, "project_info", json!({}));
    let first = text.lines().next().unwrap();
    assert!(first.starts_with("project \""), "{text}");
    assert!(first.contains("toolkit 0.1.0"), "{text}");
    assert!(text.contains("levels: "), "{text}");
    assert!(text.contains("objects "), "{text}");
}

#[test]
fn scene_inspect_shows_the_workspace_tree_one_line_per_object() {
    let mut opened = open("hello-part", "mcp-tree");
    let block = opened
        .app
        .app
        .world
        .find_first_child(opened.app.app.world.workspace(), "Block")
        .unwrap();
    let text = ok(&mut opened.app, "scene_inspect", json!({}));
    let mut lines = text.lines();
    assert!(
        lines
            .next()
            .unwrap()
            .contains("Workspace \"Workspace\" · Workspace · 2 children"),
        "{text}"
    );
    assert!(
        text.contains(&format!("\n{} Part \"Block\"", id_text(block))),
        "{text}"
    );
    // Depth 2 opens the Terrain and shows its layers, indented.
    assert!(
        text.contains("\n  ") && text.contains("TerrainLayer \"Grass\""),
        "{text}"
    );
    // Depth 1 doesn't, and says how much is inside.
    let shallow = ok(&mut opened.app, "scene_inspect", json!({"depth": 1}));
    assert!(!shallow.contains("TerrainLayer"), "{shallow}");
    assert!(shallow.contains("Terrain \"Terrain\" (+"), "{shallow}");
}

#[test]
fn scene_inspect_folds_repeats_and_pages_long_answers() {
    let mut opened = open("hello-part", "mcp-fold");
    let app = &mut opened.app;
    let workspace = app.app.world.workspace();
    let fence = add(app, ClassName::Model, "Fence", workspace);
    for _ in 0..40 {
        add(app, ClassName::Part, "Post", fence);
    }
    for i in 0..30 {
        add(app, ClassName::Part, &format!("Rock{i}"), fence);
    }
    let text = ok(app, "scene_inspect", json!({"root": "Workspace/Fence"}));
    assert!(text.contains("Part \"Post\" ×40"), "{text}");
    assert_eq!(
        text.lines()
            .filter(|line| line.contains("\"Post\""))
            .count(),
        1
    );

    let first = ok(
        app,
        "scene_inspect",
        json!({"root": "Workspace/Fence", "limit": 10}),
    );
    assert_eq!(first.lines().count(), 1 + 10 + 1, "{first}");
    assert!(
        first.ends_with("… 21 more lines: call again with cursor \"10\""),
        "{first}"
    );
    let second = ok(
        app,
        "scene_inspect",
        json!({"root": "Workspace/Fence", "limit": 10, "cursor": "10"}),
    );
    assert!(second.contains("\"Rock9\""), "{second}");
    assert!(!second.contains("\"Post\""), "{second}");
}

#[test]
fn scene_inspect_searches_by_class_name_tag_and_distance() {
    let mut opened = open("hello-part", "mcp-search");
    let app = &mut opened.app;
    let workspace = app.app.world.workspace();
    let far = add(app, ClassName::Part, "FarRock", workspace);
    app.app
        .world
        .set_property(
            far,
            "Position",
            axcenity_core::Value::Vector3(axcenity_core::Vector3::new(500.0, 0.0, 0.0)),
        )
        .unwrap();
    app.app.world.add_tag(far, "Rocks").unwrap();
    let light = add(app, ClassName::PointLight, "Lamp", far);

    let parts = ok(app, "scene_inspect", json!({"class": "BasePart"}));
    assert!(parts.contains("Part \"Block\"  Workspace/Block"), "{parts}");
    assert!(
        parts.contains("Part \"FarRock\"  Workspace/FarRock"),
        "{parts}"
    );
    assert!(!parts.contains("Lamp"), "{parts}");

    let lights = ok(app, "scene_inspect", json!({"class": "Light"}));
    assert!(lights.starts_with("1 match under Workspace"), "{lights}");
    assert!(lights.contains(&id_text(light)), "{lights}");

    // Names match in any case: the Terrain's "Rock" layer and FarRock.
    let named = ok(app, "scene_inspect", json!({"name": "rock"}));
    assert!(named.starts_with("2 matches"), "{named}");
    assert!(
        named.contains("TerrainLayer \"Rock\"") && named.contains("FarRock"),
        "{named}"
    );
    let tagged = ok(
        app,
        "scene_inspect",
        json!({"tag": "Rocks", "root": "game"}),
    );
    assert!(tagged.contains("FarRock"), "{tagged}");
    let near = ok(
        app,
        "scene_inspect",
        json!({"near": [0, 0, 0], "radius": 10, "class": "Part"}),
    );
    assert!(
        near.contains("Block") && !near.contains("FarRock"),
        "{near}"
    );

    let error = call(app, "scene_inspect", json!({"class": "PointLite"})).unwrap_err();
    assert_eq!(
        error.to_string(),
        "error not_found: no class \"PointLite\"\nfix: did you mean PointLight?"
    );
    let error = call(app, "scene_inspect", json!({"root": "Workspace/Blok"})).unwrap_err();
    assert_eq!(
        error.fix.as_deref(),
        Some("did you mean \"Workspace/Block\"?")
    );
}

#[test]
fn object_get_shows_only_what_differs_from_the_defaults() {
    let mut opened = open("hello-part", "mcp-get");
    let text = ok(
        &mut opened.app,
        "object_get",
        json!({"id": "Workspace/Block"}),
    );
    let mut lines = text.lines();
    let header = lines.next().unwrap();
    assert!(
        header.contains("Part \"Block\" in ") && header.ends_with("· Workspace/Block"),
        "{text}"
    );
    assert!(text.contains("\n  Color [0.85,0.25,0.2]"), "{text}");
    assert!(text.contains("\n  Anchored true"), "{text}");
    // A default value isn't repeated.
    assert!(!text.contains("Transparency"), "{text}");

    let full = ok(
        &mut opened.app,
        "object_get",
        json!({"id": "Workspace/Block", "detail": "full"}),
    );
    assert!(full.contains("\n  Transparency 0"), "{full}");
    assert!(full.lines().count() > text.lines().count());

    let min = ok(
        &mut opened.app,
        "object_get",
        json!({"ids": ["Workspace/Block", "Workspace/Terrain"], "detail": "min"}),
    );
    assert_eq!(min.lines().count(), 2, "{min}");

    let some = ok(
        &mut opened.app,
        "object_get",
        json!({"id": "Workspace/Block", "props": ["Size", "Transparency"]}),
    );
    assert_eq!(
        some.lines().skip(1).collect::<Vec<_>>(),
        ["  Size [4,2,3]", "  Transparency 0"]
    );

    let error = call(
        &mut opened.app,
        "object_get",
        json!({"id": "Workspace/Block", "props": ["Colour"]}),
    )
    .unwrap_err();
    assert_eq!(error.code, Code::UnknownProperty);
    assert_eq!(error.fix.as_deref(), Some("did you mean Color?"));
    let error = call(&mut opened.app, "object_get", json!({})).unwrap_err();
    assert_eq!(error.code, Code::BadArgs);
}

#[test]
fn docs_describe_classes_properties_and_search_the_api() {
    let mut opened = open("hello-part", "mcp-docs");
    let app = &mut opened.app;
    let part = ok(app, "docs", json!({"class": "Part"}));
    assert!(part.starts_with("Part (BasePart, PVInstance)"), "{part}");
    assert!(
        part.contains("\n  Size vector3 [4,1,2]") || part.contains("\n  Size vector3 "),
        "{part}"
    );
    assert!(part.contains("events: "), "{part}");

    let material = ok(
        app,
        "docs",
        json!({"class": "Part", "property": "Material"}),
    );
    assert!(
        material.starts_with("Part.Material: Material · default \"Plastic\""),
        "{material}"
    );
    assert!(
        material.contains("choices: ") && material.contains("Neon"),
        "{material}"
    );

    // Services and types only scripts see come from the API reference.
    let run_service = ok(app, "docs", json!({"class": "RunService"}));
    assert!(
        run_service.starts_with("RunService") && run_service.contains("Heartbeat"),
        "{run_service}"
    );
    let search = ok(app, "docs", json!({"query": "tween"}));
    assert!(search.contains("TweenService"), "{search}");

    let error = call(app, "docs", json!({"class": "Prat"})).unwrap_err();
    assert!(
        error
            .fix
            .as_deref()
            .is_some_and(|fix| fix.starts_with("did you mean Part")),
        "{error}"
    );
    let error = call(app, "docs", json!({"class": "Part", "property": "Colour"})).unwrap_err();
    assert_eq!(error.fix.as_deref(), Some("did you mean Color?"));
    let index = ok(app, "docs", json!({}));
    assert!(index.contains("PointLight"), "{index}");
    assert!(
        index.contains("\nobject_create kinds: Block, Sphere,"),
        "{index}"
    );
    assert!(!index.contains("HealthBar"), "{index}");
}

#[test]
fn every_tool_is_short_flat_and_in_budget() {
    for tool in TOOLS {
        let spec = tool.spec;
        assert!(
            spec.name.len() <= 20
                && spec
                    .name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '_'),
            "{}",
            spec.name
        );
        assert!(spec.summary.len() <= 140, "{}: summary too long", spec.name);
        assert!(spec.summary.ends_with('.'), "{}", spec.name);
        let schema = Value::Object(spec.input_schema()).to_string();
        assert!(
            schema.len() < 2048,
            "{}: schema is {} bytes",
            spec.name,
            schema.len()
        );
        assert!(!schema.contains("oneOf") && !schema.contains("anyOf") && !schema.contains("$ref"));
    }
    // Some clients put the instructions in every prompt.
    let instructions = estimate_tokens(super::INSTRUCTIONS);
    assert!(
        instructions <= 150,
        "instructions take ~{instructions} tokens"
    );
    // Budgets from docs/research/mcp/tool-inventory.md.
    for (profile, budget) in [
        (Profile::Lite, 2100),
        (Profile::Standard, 4500),
        (Profile::Full, 6500),
    ] {
        let tokens = estimate_tokens(&tools_list_json(&tools_for(profile)));
        assert!(
            tokens <= budget,
            "{} tools take ~{tokens} tokens, over {budget}",
            profile.name()
        );
    }
}

#[test]
fn project_info_without_a_project_says_so() {
    let mut app = Headless::new(EditorApp::new(None), Mode::Edit);
    // No project: project_info explains instead of panicking.
    let error = call(&mut app, "project_info", json!({})).unwrap_err();
    assert_eq!(error.code, Code::NotFound);
}

/// The confirm token in a `needs_confirm` error's fix line.
fn token(error: &ToolError) -> String {
    assert_eq!(error.code, Code::NeedsConfirm, "{error}");
    let fix = error.fix.as_deref().unwrap();
    let start = fix.find("\"confirm\":\"").unwrap() + "\"confirm\":\"".len();
    fix[start..start + 6].to_owned()
}

fn objects(host: &Headless) -> usize {
    let world = &host.app.world;
    world.descendants_iter(world.root()).count()
}

#[test]
fn object_create_inserts_one_undoable_object() {
    let mut opened = open("hello-part", "mcp-create");
    let host = &mut opened.app;
    let text = ok(
        host,
        "object_create",
        json!({"kind": "Block", "name": "Wall", "size": [20, 10, 1], "material": "Brick", "props": {"Anchored": true}}),
    );
    assert!(
        text.starts_with("created ") && text.contains(" Part \"Wall\" in Workspace"),
        "{text}"
    );
    assert!(
        text.ends_with("· undo \"AI (test): Create Wall\""),
        "{text}"
    );
    let wall = ok(
        host,
        "object_get",
        json!({"id": "Workspace/Wall", "props": ["Size", "Material", "Position", "Anchored"]}),
    );
    // Without a position, it rests on the ground at the origin.
    assert!(
        wall.contains("Size [20,10,1]")
            && wall.contains("Material \"Brick\"")
            && wall.contains("Position [0,5,0]"),
        "{wall}"
    );

    // A second Wall gets a name no sibling has; a light goes where Insert puts it.
    let text = ok(
        host,
        "object_create",
        json!({"kind": "block", "name": "Wall", "position": [10, 1, 0]}),
    );
    assert!(text.contains("\"Wall2\""), "{text}");
    let text = ok(
        host,
        "object_create",
        json!({"kind": "PointLight", "parent": "Workspace/Wall"}),
    );
    assert!(
        text.contains("PointLight \"PointLight\" in Workspace/Wall"),
        "{text}"
    );

    let list = ok(host, "history", json!({}));
    assert!(
        list.starts_with("3 steps to undo · unsaved changes\n  \"AI (test): Create PointLight\""),
        "{list}"
    );
    ok(host, "history", json!({"action": "undo", "steps": 2}));
    assert!(call(host, "object_get", json!({"id": "Workspace/Wall2"})).is_err());
    assert!(call(host, "object_get", json!({"id": "Workspace/Wall"})).is_ok());
    let redo = ok(host, "history", json!({"action": "redo"}));
    assert_eq!(redo, "redid \"AI (test): Create Wall\"");
    assert!(call(host, "object_get", json!({"id": "Workspace/Wall2"})).is_ok());

    // The session's first write kept a version of the files first.
    let versions = axcenity_project::list_versions(&opened.folder);
    assert!(
        versions
            .iter()
            .any(|version| version.label == "Before AI session (test)" && version.pinned),
        "{:?}",
        versions.iter().map(|v| &v.label).collect::<Vec<_>>()
    );
}

#[test]
fn a_failed_write_changes_nothing() {
    let mut opened = open("hello-part", "mcp-rollback");
    let host = &mut opened.app;
    let before = objects(host);
    let error = call(
        host,
        "object_create",
        json!({"kind": "Block", "props": {"Colour": [1, 0, 0]}}),
    )
    .unwrap_err();
    assert_eq!(error.code, Code::UnknownProperty);
    assert_eq!(error.fix.as_deref(), Some("did you mean Color?"));
    let error = call(
        host,
        "object_create",
        json!({"kind": "Block", "color": [255, 0, 0]}),
    )
    .unwrap_err();
    assert_eq!(error.code, Code::BadValue);
    assert!(error.message.contains("divide each by 255"), "{error}");
    let error = call(host, "object_create", json!({"kind": "Blok"})).unwrap_err();
    assert!(
        error
            .fix
            .as_deref()
            .unwrap()
            .starts_with("did you mean Block"),
        "{error}"
    );
    let error = call(host, "object_create", json!({"kind": "Character"})).unwrap_err();
    assert_eq!(error.code, Code::Unsupported);
    assert_eq!(objects(host), before);

    // One bad object among several: none of them change.
    let a = add(host, ClassName::Part, "A", host.app.world.workspace());
    let light = add(host, ClassName::PointLight, "L", host.app.world.workspace());
    let error = call(
        host,
        "object_set",
        json!({"ids": [id_text(a), id_text(light)], "props": {"Anchored": true}}),
    )
    .unwrap_err();
    assert_eq!(error.code, Code::UnknownProperty, "{error}");
    assert_eq!(
        host.app.world.get(a).unwrap().property("Anchored"),
        Some(&axcenity_core::Value::Bool(false))
    );
    assert!(ok(host, "history", json!({})).starts_with("0 steps"));
}

#[test]
fn object_set_changes_properties_names_parents_attributes_and_tags() {
    let mut opened = open("hello-part", "mcp-set");
    let host = &mut opened.app;
    let workspace = host.app.world.workspace();
    let model = add(host, ClassName::Model, "House", workspace);
    let text = ok(
        host,
        "object_set",
        json!({"id": "Workspace/Block", "props": {"Color": [0, 0, 1], "Transparency": 0.5}, "name": "Door"}),
    );
    assert!(
        text.starts_with("changed ") && text.contains("Part \"Door\": Color, Transparency, Name"),
        "{text}"
    );
    assert!(text.ends_with("undo \"AI (test): Change Block\""), "{text}");
    let text = ok(
        host,
        "object_set",
        json!({"id": "Workspace/Door", "parent": "Workspace/House", "attrs": {"Open": false}, "tags_add": ["Doors"]}),
    );
    assert!(text.contains("Parent, attribute \"Open\", tags"), "{text}");
    let door = ok(
        host,
        "object_get",
        json!({"id": "Workspace/House/Door", "detail": "full"}),
    );
    assert!(
        door.contains("attribute \"Open\" false") && door.contains("tags \"Doors\""),
        "{door}"
    );

    // Locked objects are refused unless the change unlocks them.
    ok(
        host,
        "object_set",
        json!({"id": "Workspace/House", "props": {"Locked": true}}),
    );
    let error = call(
        host,
        "object_set",
        json!({"id": "Workspace/House/Door", "props": {"Transparency": 0}}),
    )
    .unwrap_err();
    assert_eq!(error.code, Code::Locked, "{error}");
    ok(
        host,
        "object_set",
        json!({"id": "Workspace/House", "props": {"Locked": false}}),
    );

    let error = call(
        host,
        "object_set",
        json!({"id": "Workspace", "name": "World"}),
    )
    .unwrap_err();
    assert_eq!(error.code, Code::NotAllowed);
    let error = call(host, "object_set", json!({"id": id_text(model)})).unwrap_err();
    assert_eq!(error.code, Code::BadArgs);
}

#[test]
fn deleting_much_or_a_script_asks_first_with_a_token_for_that_call() {
    let mut opened = open("hello-part", "mcp-delete");
    let host = &mut opened.app;
    let workspace = host.app.world.workspace();
    // One part goes at once.
    let text = ok(host, "object_delete", json!({"id": "Workspace/Block"}));
    assert_eq!(text, "deleted 1 object · undo \"AI (test): Delete Block\"");

    let fence = add(host, ClassName::Model, "Fence", workspace);
    for _ in 0..6 {
        add(host, ClassName::Part, "Post", fence);
    }
    let gate = add(host, ClassName::ObjectValue, "Gate", workspace);
    host.app
        .world
        .set_property(gate, "Value", axcenity_core::Value::Instance(Some(fence)))
        .unwrap();
    let before = objects(host);
    let error = call(host, "object_delete", json!({"id": "Workspace/Fence"})).unwrap_err();
    assert!(
        error.message.starts_with("preview: deletes ")
            && error.message.contains("with 6 inside (7 objects in all)"),
        "{error}"
    );
    assert!(
        error
            .message
            .contains("1 reference will point at nothing: Workspace/Gate.Value"),
        "{error}"
    );
    assert_eq!(objects(host), before, "a preview changes nothing");
    let first = token(&error);

    // A token is for its own call only, and works once.
    let error = call(
        host,
        "object_delete",
        json!({"ids": ["Workspace/Fence", "Workspace/Gate"], "confirm": first}),
    )
    .unwrap_err();
    assert_eq!(error.code, Code::NeedsConfirm);
    let second = token(&call(host, "object_delete", json!({"id": "Workspace/Fence"})).unwrap_err());
    let text = ok(
        host,
        "object_delete",
        json!({"id": "Workspace/Fence", "confirm": second}),
    );
    assert_eq!(text, "deleted 7 objects · undo \"AI (test): Delete Fence\"");
    assert!(
        call(
            host,
            "object_delete",
            json!({"id": "Workspace/Fence", "confirm": second})
        )
        .is_err()
    );

    ok(host, "history", json!({"action": "undo"}));
    assert_eq!(objects(host), before);
    let error = call(host, "object_delete", json!({"id": "Workspace"})).unwrap_err();
    assert_eq!(error.code, Code::NotAllowed);
}

#[test]
fn modes_decide_what_a_session_may_do() {
    let mut opened = open("hello-part", "mcp-modes");
    opened.app.session.mode = Mode::Read;
    let error = call(&mut opened.app, "object_create", json!({"kind": "Block"})).unwrap_err();
    assert_eq!(error.code, Code::NotAllowed);
    assert!(
        error.fix.as_deref().unwrap().contains("--mode edit"),
        "{error}"
    );
    let info = ok(&mut opened.app, "project_info", json!({}));
    assert!(info.contains("mode read: reads only"), "{info}");

    opened.app.session.mode = Mode::Full;
    let host = &mut opened.app;
    let workspace = host.app.world.workspace();
    let fence = add(host, ClassName::Model, "Fence", workspace);
    for _ in 0..6 {
        add(host, ClassName::Part, "Post", fence);
    }
    let text = ok(host, "object_delete", json!({"id": "Workspace/Fence"}));
    assert!(text.starts_with("deleted 7 objects"), "{text}");
}

#[test]
fn project_save_asks_first_then_writes_the_files() {
    let mut opened = open("hello-part", "mcp-save");
    let host = &mut opened.app;
    ok(
        host,
        "object_create",
        json!({"kind": "Block", "name": "Saved"}),
    );
    let error = call(host, "project_save", json!({})).unwrap_err();
    assert!(
        error
            .message
            .contains("Version History keeps the files from before this AI session"),
        "{error}"
    );
    let token = token(&error);
    let text = ok(host, "project_save", json!({"confirm": token}));
    assert!(text.starts_with("saved \""), "{text}");
    assert!(ok(host, "history", json!({})).contains("· saved"));

    let reopened = EditorApp::new(Some(&opened.folder));
    assert!(
        reopened
            .world
            .find_first_child(reopened.world.workspace(), "Saved")
            .is_some()
    );

    // Every call is a line of the log.
    let log = std::fs::read_to_string(opened.folder.join(super::LOG_FILE)).unwrap();
    let lines: Vec<Value> = log
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let tools: Vec<&str> = lines
        .iter()
        .map(|line| line["tool"].as_str().unwrap())
        .collect();
    assert_eq!(
        tools,
        ["object_create", "project_save", "project_save", "history"]
    );
    assert_eq!(lines[0]["undo"], "AI (test): Create Saved");
    assert_eq!(lines[0]["client"], "test");
    assert_eq!(lines[1]["result"], "needs_confirm");
}

#[test]
fn script_read_lists_scripts_and_shows_numbered_lines() {
    let mut opened = open("hello-touched", "mcp-script-read");
    let host = &mut opened.app;
    let list = ok(host, "script_read", json!({}));
    assert!(
        list.starts_with("1 scripts\nscripts/pad.server.lua · "),
        "{list}"
    );
    assert!(list.contains(" Script · "), "{list}");
    let part = ok(
        host,
        "script_read",
        json!({"path": "scripts/pad.server.lua", "from": 5, "to": 6}),
    );
    let mut lines = part.lines();
    assert!(
        lines
            .next()
            .unwrap()
            .starts_with("scripts/pad.server.lua · "),
        "{part}"
    );
    assert_eq!(lines.next(), Some("5| local pad = workspace.Pad"));
    assert_eq!(lines.next(), Some("6| local colors = {"));
    assert_eq!(lines.next(), None);
    // Its object names it too.
    let by_object = ok(
        host,
        "script_read",
        json!({"path": "ServerScriptService/pad", "to": 1}),
    );
    assert!(
        by_object.contains("\n1| -- The Milestone 1 demo"),
        "{by_object}"
    );
    let error = call(
        host,
        "script_read",
        json!({"path": "scripts/pads.server.lua"}),
    )
    .unwrap_err();
    assert_eq!(
        error.fix.as_deref(),
        Some("did you mean \"scripts/pad.server.lua\"?")
    );
}

#[test]
fn script_write_makes_and_edits_scripts_as_undoable_file_changes() {
    let mut opened = open("hello-touched", "mcp-script-write");
    let host = &mut opened.app;
    let file = opened.folder.join("scripts/Spinner.server.lua");
    let text = ok(
        host,
        "script_write",
        json!({"name": "Spinner", "side": "server", "text": "local speed = 2\nprint(speed)\n"}),
    );
    assert_eq!(
        text,
        "created scripts/Spinner.server.lua (2 lines) · syntax ok · undo \"AI (test): Create Spinner.server.lua\""
    );
    assert!(file.is_file());
    let service = ok(
        host,
        "scene_inspect",
        json!({"root": "ServerScriptService"}),
    );
    assert!(service.contains("Script \"Spinner\""), "{service}");

    let text = ok(
        host,
        "script_write",
        json!({"path": "scripts/Spinner.server.lua", "edits": [{"find": "speed = 2", "replace": "speed = 5"}]}),
    );
    assert!(
        text.starts_with("wrote scripts/Spinner.server.lua (2 lines) · syntax ok"),
        "{text}"
    );
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "local speed = 5\nprint(speed)\n"
    );

    // An edit that doesn't match once changes nothing.
    let error = call(host, "script_write", json!({"path": "scripts/Spinner.server.lua", "edits": [{"find": "speed", "replace": "rate"}]})).unwrap_err();
    assert_eq!(error.message, "edit 1: find matches 2 times, not once");
    // Code that doesn't compile is still written, and the check says where.
    let text = ok(
        host,
        "script_write",
        json!({"path": "scripts/Spinner.server.lua", "text": "local x = \nprint(\n"}),
    );
    assert!(
        text.contains("· syntax error scripts/Spinner.server.lua:"),
        "{text}"
    );
    // A Script using an API only clients have is warned about.
    let text = ok(
        host,
        "script_write",
        json!({"path": "scripts/Spinner.server.lua", "text": "local uis = game:GetService(\"UserInputService\")\n"}),
    );
    assert!(
        text.contains("\nwarning scripts/Spinner.server.lua:1: "),
        "{text}"
    );

    // Undo puts the files back, the last one first; undoing the creation
    // takes the script away.
    ok(host, "history", json!({"action": "undo", "steps": 3}));
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "local speed = 2\nprint(speed)\n"
    );
    let read = ok(
        host,
        "script_read",
        json!({"path": "scripts/Spinner.server.lua"}),
    );
    assert!(read.contains("1| local speed = 2"), "{read}");
    ok(host, "history", json!({"action": "undo"}));
    assert!(!file.exists());
    assert!(
        call(
            host,
            "script_read",
            json!({"path": "scripts/Spinner.server.lua"})
        )
        .is_err()
    );
    ok(host, "history", json!({"action": "redo"}));
    assert!(
        ok(
            host,
            "scene_inspect",
            json!({"root": "ServerScriptService"})
        )
        .contains("\"Spinner\"")
    );

    let error = call(
        host,
        "script_write",
        json!({"name": "Spinner", "side": "server", "text": ""}),
    )
    .unwrap_err();
    assert!(error.message.contains("is already there"), "{error}");
}

#[test]
fn project_check_lists_script_problems_worst_first() {
    let mut opened = open("hello-touched", "mcp-check");
    let host = &mut opened.app;
    assert_eq!(
        ok(host, "project_check", json!({"scope": "scripts"})),
        "no problems found (scripts)"
    );
    ok(
        host,
        "script_write",
        json!({"name": "Broken", "side": "server", "text": "local x = \n"}),
    );
    ok(
        host,
        "script_write",
        json!({"name": "Input", "side": "server", "text": "local uis = game:GetService(\"UserInputService\")\n"}),
    );
    let report = ok(host, "project_check", json!({"scope": "scripts"}));
    let lines: Vec<&str> = report.lines().collect();
    assert_eq!(lines[0], "2 problems (1 error, 1 warning)", "{report}");
    assert!(
        lines[1].starts_with("syntax error scripts/Broken.server.lua:"),
        "{report}"
    );
    assert!(
        lines[2].starts_with("warning scripts/Input.server.lua:1: "),
        "{report}"
    );
}

#[test]
fn object_duplicate_copies_in_place_or_as_a_row() {
    let mut opened = open("hello-part", "mcp-duplicate");
    let host = &mut opened.app;
    ok(
        host,
        "object_create",
        json!({"kind": "Model", "name": "Post"}),
    );
    ok(
        host,
        "object_create",
        json!({"kind": "Block", "name": "Pole", "parent": "Workspace/Post", "position": [0, 2, 0]}),
    );
    // Something placed on its part doesn't move by itself.
    ok(
        host,
        "object_create",
        json!({"kind": "Attachment", "parent": "Workspace/Post/Pole", "props": {"Position": [0, 1, 0]}}),
    );
    let before = objects(host);
    let text = ok(
        host,
        "object_duplicate",
        json!({"id": "Workspace/Post", "count": 3, "offset": [0, 0, 6]}),
    );
    assert!(
        text.starts_with("made ")
            && text.contains("Model \"Post2\"")
            && text.contains("Model \"Post4\"")
            && text.ends_with("· undo \"AI (test): Duplicate Post\""),
        "{text}"
    );
    assert_eq!(objects(host), before + 9, "three copies of three objects");
    for (name, z) in [("Post2", 6), ("Post3", 12), ("Post4", 18)] {
        let pole = ok(
            host,
            "object_get",
            json!({"id": format!("Workspace/{name}/Pole"), "props": ["Position"]}),
        );
        assert!(pole.contains(&format!("Position [0,2,{z}]")), "{pole}");
        let attachment = ok(
            host,
            "object_get",
            json!({"id": format!("Workspace/{name}/Pole/Attachment"), "props": ["Position"]}),
        );
        assert!(attachment.contains("Position [0,1,0]"), "{attachment}");
    }
    // Copies go right after the original, in order.
    let world = &host.app.world;
    let names: Vec<String> = world
        .get(world.workspace())
        .unwrap()
        .children()
        .iter()
        .map(|&id| world.get(id).unwrap().name().to_owned())
        .filter(|name| name.starts_with("Post"))
        .collect();
    assert_eq!(names, ["Post", "Post2", "Post3", "Post4"]);

    ok(host, "history", json!({"action": "undo"}));
    assert_eq!(objects(host), before);

    // Scripts and services can't be copied, as in the editor.
    let error = call(host, "object_duplicate", json!({"id": "Workspace"})).unwrap_err();
    assert_eq!(error.code, Code::NotAllowed, "{error}");
    let error = call(
        host,
        "object_duplicate",
        json!({"id": "Workspace/Post", "count": 500}),
    )
    .unwrap_err();
    assert_eq!(error.code, Code::BadValue, "{error}");
}

#[test]
fn batch_runs_steps_as_one_undo_step_with_names_for_results() {
    let mut opened = open("hello-part", "mcp-batch");
    let host = &mut opened.app;
    let before = objects(host);
    let text = ok(
        host,
        "batch",
        json!({"label": "Build a hut", "ops": [
            {"op": "create", "kind": "Model", "name": "Hut", "as": "hut"},
            {"op": "create", "kind": "Block", "name": "Floor", "parent": "$hut", "size": [10, 1, 10], "as": "floor"},
            {"op": "create", "kind": "Block", "name": "Wall", "parent": "$hut", "position": [0, 3, -5], "size": [10, 5, 1], "as": "wall"},
            {"op": "duplicate", "id": "$wall", "offset": [0, 0, 10]},
            {"op": "set", "ids": ["$floor", "$wall"], "props": {"Anchored": true}},
            {"op": "create", "kind": "ObjectValue", "name": "Door", "parent": "$hut", "props": {"Value": "$wall"}},
            {"op": "delete", "id": "Workspace/Block"}
        ]}),
    );
    assert!(
        text.starts_with("ran 7 steps · undo \"AI (test): Build a hut\"\n1. created "),
        "{text}"
    );
    assert!(
        text.contains("\n4. made ") && text.contains("Part \"Wall2\""),
        "{text}"
    );
    assert!(text.contains("\n7. deleted 1 object"), "{text}");
    // Hut, Floor, Wall, Wall2, Door; the Block went.
    assert_eq!(objects(host), before + 5 - 1);
    let wall2 = ok(
        host,
        "object_get",
        json!({"id": "Workspace/Hut/Wall2", "props": ["Position"]}),
    );
    assert!(wall2.contains("Position [0,3,5]"), "{wall2}");
    let door = ok(
        host,
        "object_get",
        json!({"id": "Workspace/Hut/Door", "props": ["Value"]}),
    );
    assert!(door.contains("Wall"), "{door}");

    // The whole batch is one step.
    let undone = ok(host, "history", json!({"action": "undo"}));
    assert_eq!(undone, "undid \"AI (test): Build a hut\"");
    assert_eq!(objects(host), before);
}

#[test]
fn a_failing_batch_step_changes_nothing_and_says_which() {
    let mut opened = open("hello-part", "mcp-batch-fail");
    let host = &mut opened.app;
    let before = objects(host);
    let revision = host.app.history.revision();
    let error = call(
        host,
        "batch",
        json!({"ops": [
            {"op": "create", "kind": "Block", "name": "A"},
            {"op": "set", "id": "Workspace/A", "props": {"Colour": [1, 0, 0]}}
        ]}),
    )
    .unwrap_err();
    assert!(
        error.message.starts_with("step 2 (set): ")
            && error.message.ends_with("nothing was changed"),
        "{error}"
    );
    assert_eq!(objects(host), before);
    assert_eq!(host.app.history.revision(), revision);

    // Bad steps are caught before anything runs.
    for (ops, says) in [
        (json!([{"kind": "Block"}]), "step 1 needs op"),
        (
            json!([{"op": "create", "kind": "Block", "sise": [1, 1, 1]}]),
            "step 1 (create): object_create has no parameter \"sise\"",
        ),
        (
            json!([{"op": "create", "kind": "Block", "parent": "$nope"}]),
            "no earlier step is \"as\":\"nope\"",
        ),
        (
            json!([{"op": "delete", "id": "Workspace/Block", "confirm": "abcdef"}]),
            "confirm goes on the batch",
        ),
    ] {
        let error = call(host, "batch", json!({"ops": ops})).unwrap_err();
        assert!(error.message.contains(says), "{error}");
    }
    let too_many: Vec<Value> = (0..101)
        .map(|_| json!({"op": "create", "kind": "Block"}))
        .collect();
    let error = call(host, "batch", json!({"ops": too_many})).unwrap_err();
    assert_eq!(error.code, Code::TooLarge);
    assert_eq!(objects(host), before);
}

#[test]
fn a_batch_with_a_big_delete_asks_first() {
    let mut opened = open("hello-part", "mcp-batch-ask");
    let host = &mut opened.app;
    let ops = json!([
        {"op": "create", "kind": "Model", "name": "Pile", "as": "pile"},
        {"op": "create", "kind": "Block", "parent": "$pile", "as": "brick"},
        {"op": "duplicate", "id": "$brick", "count": 6},
        {"op": "delete", "id": "$pile"}
    ]);
    let before = objects(host);
    let error = call(host, "batch", json!({"ops": ops})).unwrap_err();
    assert!(error.message.contains("step 4 deletes "), "{error}");
    assert_eq!(objects(host), before, "asking changed nothing");
    let token = token(&error);
    let text = ok(host, "batch", json!({"ops": ops, "confirm": token}));
    assert!(text.contains("\n4. deleted 8 objects"), "{text}");
    assert_eq!(objects(host), before);
}

#[test]
fn material_edit_makes_changes_and_assigns_material_files_undoably() {
    let mut opened = open("hello-part", "mcp-material");
    let folder = opened.folder.clone();
    let host = &mut opened.app;
    let text = ok(
        host,
        "material_edit",
        json!({"action": "create", "name": "Rust", "values": {"color": [0.5, 0.2, 0.1], "roughness": 0.8}, "id": "Workspace/Block"}),
    );
    let path = "assets/materials/Rust.material.json";
    assert!(
        text.starts_with(&format!("created {path} · drew "))
            && text.contains("\"AI (test): Create material Rust\"")
            && text.contains("\"roughness\":0.8"),
        "{text}"
    );
    let file = std::fs::read_to_string(folder.join(path)).unwrap();
    assert!(file.contains("\"roughness\": 0.8"), "{file}");
    let block = ok(
        host,
        "object_get",
        json!({"id": "Workspace/Block", "props": ["MaterialVariant", "Color"]}),
    );
    assert!(
        block.contains(&format!("MaterialVariant \"{path}\"")) && block.contains("Color [1,1,1]"),
        "{block}"
    );

    // A change keeps what it doesn't name; null takes a value away.
    let text = ok(
        host,
        "material_edit",
        json!({"action": "change", "path": path, "values": {"metalness": 1, "roughness": null}}),
    );
    assert!(
        text.starts_with(&format!("changed {path} · undo")),
        "{text}"
    );
    let file = std::fs::read_to_string(folder.join(path)).unwrap();
    assert!(
        file.contains("\"metalness\": 1")
            && file.contains("\"color\"")
            && !file.contains("roughness"),
        "{file}"
    );

    // Undo puts the file back as it was, then takes it and the part's look away.
    ok(host, "history", json!({"action": "undo"}));
    let file = std::fs::read_to_string(folder.join(path)).unwrap();
    assert!(file.contains("\"roughness\": 0.8"), "{file}");
    ok(host, "history", json!({"action": "undo"}));
    assert!(!folder.join(path).exists());
    let block = ok(
        host,
        "object_get",
        json!({"id": "Workspace/Block", "props": ["MaterialVariant"]}),
    );
    assert!(!block.contains("Rust"), "{block}");

    // Mistakes say what to do and change nothing.
    for (args, code, says) in [
        (
            json!({"action": "create", "name": "X", "values": {"colour": [1, 0, 0]}}),
            Code::BadValue,
            "no field \"colour\"",
        ),
        (
            json!({"action": "create", "name": "X", "values": {"maps": {"color": "assets/nope.png"}}}),
            Code::NotFound,
            "isn't in the project",
        ),
        (
            json!({"action": "create", "name": "X", "values": {"base": "../outside.material.json"}}),
            Code::BadValue,
            "isn't a path in the project",
        ),
        (
            json!({"action": "assign", "path": path, "id": "Workspace/Block"}),
            Code::NotFound,
            "",
        ),
        (
            json!({"action": "assign", "path": "world.json", "id": "Workspace/Block"}),
            Code::BadValue,
            "isn't a material file",
        ),
        (
            json!({"action": "create", "name": "X", "id": "Workspace"}),
            Code::BadValue,
            "isn't a part",
        ),
    ] {
        let error = call(host, "material_edit", args).unwrap_err();
        assert_eq!(error.code, code, "{error}");
        assert!(error.message.contains(says), "{error}");
    }
    assert!(!folder.join("assets/materials/X.material.json").exists());
}

#[test]
fn env_set_changes_the_look_in_one_step_and_makes_sky_objects_when_needed() {
    let mut opened = open("hello-part", "mcp-env");
    let host = &mut opened.app;
    let before = objects(host);
    let text = ok(
        host,
        "env_set",
        json!({"preset": "foggy", "time": 7.5, "weather": "rain", "intensity": 0.8}),
    );
    assert!(
        text.contains("Lighting ClockTime")
            && text.contains("FogEnd")
            && text.contains("made Clouds in Lighting")
            && text.contains("Weather")
            && text.ends_with("· undo \"AI (test): Environment: foggy\""),
        "{text}"
    );
    let lighting = ok(
        host,
        "object_get",
        json!({"id": "Lighting", "props": ["ClockTime", "FogEnd"]}),
    );
    assert!(
        lighting.contains("ClockTime 7.5") && lighting.contains("FogEnd 180"),
        "{lighting}"
    );
    let weather = ok(
        host,
        "object_get",
        json!({"id": "Lighting/Weather", "props": ["Precipitation", "Intensity"]}),
    );
    assert!(
        weather.contains("Precipitation.Rain") && weather.contains("Intensity 0.8"),
        "{weather}"
    );

    // Enum items go by their names alone, and a wrong one lists the right ones.
    ok(
        host,
        "object_set",
        json!({"id": "Lighting/Weather", "props": {"Precipitation": "snow"}}),
    );
    let error = call(
        host,
        "object_set",
        json!({"id": "Lighting/Weather", "props": {"Precipitation": "Hail"}}),
    )
    .unwrap_err();
    assert!(error.message.contains("one of None, Rain, Snow"), "{error}");
    ok(host, "history", json!({"action": "undo"}));

    // A second call changes those objects rather than making more.
    let made = objects(host);
    assert!(made > before);
    let text = ok(
        host,
        "env_set",
        json!({"clouds": 0.9, "weather": "none", "fog": 0}),
    );
    assert!(text.contains("Clouds Cover"), "{text}");
    assert_eq!(objects(host), made);

    ok(host, "history", json!({"action": "undo", "steps": 2}));
    assert_eq!(objects(host), before);

    let error = call(host, "env_set", json!({})).unwrap_err();
    assert_eq!(error.code, Code::BadArgs);
    let error = call(host, "env_set", json!({"time": 30})).unwrap_err();
    assert_eq!(error.code, Code::BadValue);
}

#[test]
fn game_run_plays_for_a_while_and_reports_errors_and_output() {
    let mut opened = open("hello-part", "mcp-play");
    let host = &mut opened.app;
    ok(
        host,
        "script_write",
        json!({"name": "Shout", "side": "server", "text": "print(\"hello from Shout\")\nworkspace.Block.Name = \"Changed\"\nerror(\"boom\")\n"}),
    );
    let before = objects(host);
    let text = ok(host, "game_run", json!({"seconds": 0.5}));
    assert!(
        text.starts_with("played 0.5 s (30 frames; avg ") && text.contains("· 1 error ·"),
        "{text}"
    );
    assert!(text.contains("\nerror ") && text.contains("boom"), "{text}");
    assert!(text.contains("hello from Shout"), "{text}");
    assert!(!text.contains("Playing \""), "{text}");
    // What the game did is gone after.
    assert!(!host.app.playing());
    assert_eq!(objects(host), before);
    assert!(call(host, "object_get", json!({"id": "Workspace/Block"})).is_ok());

    let error = call(host, "game_run", json!({"seconds": 600})).unwrap_err();
    assert_eq!(error.code, Code::BadValue);
}

#[test]
fn game_run_runs_the_game_tests() {
    let mut opened = open("hello-character", "mcp-game-tests");
    let host = &mut opened.app;
    let text = ok(host, "game_run", json!({"mode": "tests"}));
    assert!(text.ends_with(" passed, 0 failed"), "{text}");
    let error = call(
        host,
        "game_run",
        json!({"mode": "tests", "file": "tests/nope.spec.lua"}),
    )
    .unwrap_err();
    assert_eq!(error.code, Code::NotFound);
    assert!(
        error.fix.unwrap().contains("tests/walk.spec.lua"),
        "lists the spec files"
    );
}

/// The ground's height at `x`, `z` on the only terrain.
fn ground(host: &Headless, x: f32, z: f32) -> f32 {
    let world = &host.app.world;
    let id = world
        .descendants_iter(world.workspace())
        .find(|&id| world.terrain(id).is_some())
        .expect("a terrain");
    let placement = axcenity_terrain::Placement::of(world.get(id).unwrap()).unwrap();
    placement
        .ground_y(world.terrain(id).unwrap(), x, z)
        .unwrap()
}

#[test]
fn terrain_edit_sculpts_paints_and_scatters_as_undo_steps() {
    let mut opened = open("hello-part", "mcp-terrain");
    let host = &mut opened.app;
    assert!(ground(host, 0.0, 0.0).abs() < 0.01);
    // Raise lifts the middle by about `amount`, as one undo step.
    let raised = ok(
        host,
        "terrain_edit",
        json!({"action": "sculpt", "tool": "raise", "at": [0, 0, 0], "amount": 10}),
    );
    assert!(
        raised.contains("undo \"AI (test): Raise Terrain\""),
        "{raised}"
    );
    let height = ground(host, 0.0, 0.0);
    assert!((height - 10.0).abs() < 0.5, "{height}: {raised}");
    ok(host, "history", json!({"action": "undo"}));
    assert!(ground(host, 0.0, 0.0).abs() < 0.01);
    // A line of stamps makes a ridge about as high.
    ok(
        host,
        "terrain_edit",
        json!({"action": "sculpt", "at": [-60, 0, 40], "to": [60, 0, 40], "amount": 6}),
    );
    let middle = ground(host, 0.0, 40.0);
    assert!((middle - 6.0).abs() < 1.5, "{middle}");
    // Flatten goes to `at`'s y.
    ok(
        host,
        "terrain_edit",
        json!({"action": "sculpt", "tool": "flatten", "at": [0, 3, 40], "radius": 20, "strength": 1}),
    );
    let flat = ground(host, 0.0, 40.0);
    assert!(flat < middle && flat > 2.0, "{flat}");
    // Paint by layer name; a wrong one lists them.
    let painted = ok(
        host,
        "terrain_edit",
        json!({"action": "paint", "layer": "rock", "at": [10, 0, 10]}),
    );
    assert!(painted.contains("Paint Rock"), "{painted}");
    let wrong = call(
        host,
        "terrain_edit",
        json!({"action": "paint", "layer": "Lava", "at": [10, 0, 10]}),
    )
    .unwrap_err();
    assert_eq!(wrong.code, Code::NotFound);
    assert!(wrong.to_string().contains("Grass, Dirt, Rock"), "{wrong}");
    // Foliage: a built-in prop scattered, then taken away.
    let trees = ok(
        host,
        "terrain_edit",
        json!({"action": "foliage", "item": "tree", "at": [0, 0, -50], "radius": 30, "density": 0.05}),
    );
    assert!(trees.contains("Paint Tree Foliage"), "{trees}");
    assert!(!trees.contains(" 0 foliage in all"), "{trees}");
    let gone = ok(
        host,
        "terrain_edit",
        json!({"action": "foliage", "item": "Tree", "at": [0, 0, -50], "radius": 40, "erase": true}),
    );
    assert!(gone.contains(" 0 foliage in all"), "{gone}");
    let unknown = call(
        host,
        "terrain_edit",
        json!({"action": "foliage", "item": "Unicorn", "at": [0, 0, 0]}),
    )
    .unwrap_err();
    assert_eq!(unknown.code, Code::NotFound);
    // Another action's parameter is a mistake.
    let mixed = call(
        host,
        "terrain_edit",
        json!({"action": "sculpt", "at": [0, 0, 0], "preset": "island"}),
    )
    .unwrap_err();
    assert_eq!(mixed.code, Code::BadArgs, "{mixed}");
    // Off the terrain nothing changes and no step is made.
    let steps = ok(host, "history", json!({}));
    let off = ok(
        host,
        "terrain_edit",
        json!({"action": "sculpt", "at": [5000, 0, 5000]}),
    );
    assert!(off.contains("nothing changed"), "{off}");
    assert_eq!(ok(host, "history", json!({})), steps);
}

#[test]
fn terrain_edit_generates_asking_before_it_replaces_a_landscape() {
    let mut opened = open("hello-part", "mcp-terrain-generate");
    let host = &mut opened.app;
    let args = json!({"action": "generate", "preset": "mountain_range", "seed": 7});
    let asked = call(host, "terrain_edit", args.clone()).unwrap_err();
    assert_eq!(asked.code, Code::NeedsConfirm, "{asked}");
    assert!(
        asked.to_string().contains("Mountain Range (seed 7)"),
        "{asked}"
    );
    let mut args = args;
    args["confirm"] = json!(token(&asked));
    let text = ok(host, "terrain_edit", args);
    assert!(
        text.starts_with("regenerated ") && text.contains(" as Mountain Range "),
        "{text}"
    );
    assert!(text.contains("3 layers"), "{text}");
    ok(host, "history", json!({"action": "undo"}));
    assert!(
        ground(host, 0.0, 0.0).abs() < 0.01,
        "undo brings the flat ground back"
    );
    // Erosion on a landscape, one step.
    ok(host, "history", json!({"action": "redo"}));
    let eroded = ok(
        host,
        "terrain_edit",
        json!({"action": "erode", "kind": "scree"}),
    );
    assert!(
        eroded.contains("undo \"AI (test): Apply Scree Erosion\""),
        "{eroded}"
    );
}

#[test]
fn terrain_edit_generate_makes_a_terrain_when_there_is_none() {
    let mut opened = open("hello-material-graph", "mcp-terrain-new");
    let host = &mut opened.app;
    let sculpt = call(
        host,
        "terrain_edit",
        json!({"action": "sculpt", "at": [0, 0, 0]}),
    )
    .unwrap_err();
    assert_eq!(sculpt.code, Code::NotFound, "{sculpt}");
    assert!(sculpt.to_string().contains("generate"), "{sculpt}");
    let before = objects(host);
    let text = ok(
        host,
        "terrain_edit",
        json!({"action": "generate", "preset": "island", "size": [256, 64, 256]}),
    );
    assert!(
        text.starts_with("made ") && text.contains("Terrain"),
        "{text}"
    );
    assert!(objects(host) > before);
    ok(host, "history", json!({"action": "undo"}));
    assert_eq!(objects(host), before, "one step takes it all away");
}

#[test]
fn anim_edit_animates_keys_and_lists_a_clip_as_file_undo_steps() {
    let mut opened = open("hello-part", "mcp-anim");
    let host = &mut opened.app;
    let made = ok(
        host,
        "object_create",
        json!({"kind": "Block", "name": "Lift", "position": [0, 2, 0]}),
    );
    let lift = made.split_whitespace().nth(1).unwrap().to_owned();
    let animated = ok(host, "anim_edit", json!({"action": "animate", "id": lift}));
    assert!(
        animated.contains("undo \"AI (test): Animate Lift\""),
        "{animated}"
    );
    let clip = "assets/animations/Lift/Lift.anim.json";
    assert!(animated.contains(clip), "{animated}");
    assert!(opened.folder.join(clip).is_file());
    // With no value, a key holds what the target is now.
    let first = ok(
        host,
        "anim_edit",
        json!({"action": "key", "id": lift, "property": "Position", "time": 0}),
    );
    assert!(
        first.starts_with("Key . Position at 0 s = [0, 2, 0]"),
        "{first}"
    );
    let second = ok(
        host,
        "anim_edit",
        json!({"action": "key", "id": lift, "property": "Position", "time": 3, "value": [0, 12, 0], "interp": "ease", "loop": true}),
    );
    assert!(second.contains("clip now 3 s long"), "{second}");
    let info = ok(host, "anim_edit", json!({"action": "info", "clip": clip}));
    assert!(
        info.starts_with(&format!("{clip} · 3 s · loops · 1 track")),
        "{info}"
    );
    assert!(
        info.contains("\n. Position: 0 [0, 2, 0]; 3 [0, 12, 0] ease;"),
        "{info}"
    );
    // Wrong widths, targets and properties say so.
    let wide = call(
        host,
        "anim_edit",
        json!({"action": "key", "id": lift, "property": "Transparency", "time": 1, "value": [1, 2, 3]}),
    )
    .unwrap_err();
    assert_eq!(wide.code, Code::BadValue, "{wide}");
    let nobody = call(
        host,
        "anim_edit",
        json!({"action": "key", "id": lift, "target": "Ghost", "property": "Position", "time": 1}),
    )
    .unwrap_err();
    assert_eq!(nobody.code, Code::NotFound, "{nobody}");
    // Unkey, then undo puts the key back in the file.
    ok(
        host,
        "anim_edit",
        json!({"action": "unkey", "id": lift, "property": "Position", "time": 3}),
    );
    let read = || std::fs::read_to_string(opened.folder.join(clip)).unwrap();
    assert!(!read().contains("12"), "{}", read());
    ok(host, "history", json!({"action": "undo"}));
    assert!(read().contains("12"), "{}", read());
    // Undoing everything takes the clip file and the Animator away.
    ok(host, "history", json!({"action": "undo", "steps": 3}));
    assert!(!opened.folder.join(clip).exists());
    let gone = call(host, "anim_edit", json!({"action": "info", "id": lift})).unwrap_err();
    assert_eq!(gone.code, Code::NotFound, "{gone}");
}

#[test]
fn asset_find_lists_assets_by_kind_name_and_use() {
    let mut opened = open("hello-material-graph", "mcp-assets");
    let host = &mut opened.app;
    let text = ok(host, "asset_find", json!({}));
    assert!(text.starts_with("4 assets\n"), "{text}");
    assert!(
        text.contains("\nassets/materials/MossyStone.material.json · material · ")
            && text.contains("\nassets/textures/moss.png · texture · "),
        "{text}"
    );
    let materials = ok(
        host,
        "asset_find",
        json!({"kind": "material", "name": "moss"}),
    );
    assert!(materials.starts_with("1 asset\n"), "{materials}");
    // Full detail says what uses each one.
    let full = ok(
        host,
        "asset_find",
        json!({"name": "moss.png", "detail": "full"}),
    );
    assert!(full.contains("\n  used by "), "{full}");
    // A new material nothing uses shows as unused.
    ok(
        host,
        "material_edit",
        json!({"action": "create", "name": "Spare", "values": {"roughness": 0.5}}),
    );
    let unused = ok(host, "asset_find", json!({"unused": true}));
    assert!(unused.contains("Spare.material.json"), "{unused}");
    assert!(!unused.contains("moss.png"), "{unused}");
}

/// A folder outside the project with `files` copied from the examples, as
/// a user's downloads would be.
fn import_folder(test: &str, files: &[&str]) -> std::path::PathBuf {
    let folder = std::env::temp_dir().join(format!("axcenity-mcp-import-{test}"));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    let examples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    for file in files {
        let from = examples.join(file);
        std::fs::copy(&from, folder.join(from.file_name().unwrap())).unwrap();
    }
    folder
}

#[test]
fn asset_import_copies_files_only_from_allowed_folders_as_an_undo_step() {
    let mut opened = open("hello-part", "mcp-import");
    let downloads = import_folder(
        "texture",
        &["hello-material-graph/assets/textures/moss.png"],
    );
    let host = &mut opened.app;
    let png = downloads.join("moss.png");
    let png_text = png.to_string_lossy().into_owned();
    // No allowed folder: nothing is read.
    let refused = call(host, "asset_import", json!({"file": png_text})).unwrap_err();
    assert_eq!(refused.code, Code::NotAllowed, "{refused}");
    assert!(refused.to_string().contains("--allow-import"), "{refused}");
    // A file outside the allowed folders, even through `..`, is refused.
    host.session.import_folders = vec![downloads.clone()];
    let outside = downloads.join("..").join("elsewhere.png");
    std::fs::write(&outside, b"not really").unwrap();
    let refused = call(
        host,
        "asset_import",
        json!({"file": outside.to_string_lossy()}),
    )
    .unwrap_err();
    let _ = std::fs::remove_file(&outside);
    assert_eq!(refused.code, Code::NotAllowed, "{refused}");
    // Only a model is placed.
    let wrong = call(
        host,
        "asset_import",
        json!({"file": png_text, "position": [0, 5, 0]}),
    )
    .unwrap_err();
    assert_eq!(wrong.code, Code::BadArgs, "{wrong}");
    // A file that isn't an asset is refused before anything happens.
    let notes = downloads.join("notes.txt");
    std::fs::write(&notes, "hello").unwrap();
    let wrong = call(
        host,
        "asset_import",
        json!({"file": notes.to_string_lossy()}),
    )
    .unwrap_err();
    assert_eq!(wrong.code, Code::Unsupported, "{wrong}");
    // A texture is copied into the project, as one undo step.
    let text = ok(host, "asset_import", json!({"file": png_text}));
    assert!(text.starts_with("imported assets/textures/moss"), "{text}");
    assert!(text.contains("undo \"AI (test): Import moss\""), "{text}");
    let project = opened.folder.clone();
    let copied = text
        .trim_start_matches("imported ")
        .split(" · ")
        .next()
        .unwrap()
        .to_owned();
    assert!(project.join(&copied).is_file(), "{copied}");
    assert!(ok(host, "asset_find", json!({"kind": "texture"})).contains(&copied));
    ok(host, "history", json!({"action": "undo"}));
    assert!(!project.join(&copied).exists(), "undo takes the copy away");
    ok(host, "history", json!({"action": "redo"}));
    assert!(project.join(&copied).is_file(), "redo puts it back");
    let _ = std::fs::remove_dir_all(&downloads);
}

#[test]
fn asset_import_places_a_model_where_asked() {
    let mut opened = open("hello-part", "mcp-import-model");
    let downloads = import_folder("model", &["hello-roads/assets/models/rock.glb"]);
    let host = &mut opened.app;
    host.session.import_folders = vec![downloads.clone()];
    let before = objects(host);
    let text = ok(
        host,
        "asset_import",
        json!({"file": downloads.join("rock.glb").to_string_lossy(), "position": [10, 4, -6]}),
    );
    assert!(text.starts_with("imported "), "{text}");
    assert!(text.contains(" · placed "), "{text}");
    assert!(objects(host) > before, "{text}");
    // Somewhere under the placed model, a part sits where it was asked.
    let world = &host.app.world;
    let at = world.descendants_iter(world.workspace()).find_map(|id| {
        match world.get(id)?.property("Position") {
            Some(axcenity_core::Value::Vector3(at))
                if (at.x - 10.0).abs() < 1e-3 && (at.z + 6.0).abs() < 1e-3 =>
            {
                Some(*at)
            }
            _ => None,
        }
    });
    assert!(at.is_some(), "{text}");
    // One undo step takes the objects and the copied files away.
    ok(host, "history", json!({"action": "undo"}));
    assert_eq!(objects(host), before);
    let left = ok(host, "asset_find", json!({"name": "rock"}));
    assert!(left.starts_with("0 assets"), "{left}");
    let _ = std::fs::remove_dir_all(&downloads);
}

#[test]
fn level_lists_makes_renames_deletes_and_opens_levels() {
    let mut opened = open("hello-levels", "mcp-levels");
    let host = &mut opened.app;
    let list = ok(host, "level", json!({}));
    assert!(
        list.contains("· start · open") && list.contains("\nCastle"),
        "{list}"
    );
    let made = ok(host, "level", json!({"action": "create", "name": "Caves"}));
    assert!(
        made.contains("undo \"AI (test): New level Caves\""),
        "{made}"
    );
    let levels = opened.folder.join("levels");
    assert!(levels.join("Caves.world.json").is_file());
    let taken = call(host, "level", json!({"action": "create", "name": "caves"})).unwrap_err();
    assert_eq!(taken.code, Code::BadValue, "{taken}");
    ok(
        host,
        "level",
        json!({"action": "rename", "name": "Caves", "to": "Mines"}),
    );
    assert!(levels.join("Mines.world.json").is_file() && !levels.join("Caves.world.json").exists());
    // Deleting asks first.
    let asked = call(host, "level", json!({"action": "delete", "name": "Mines"})).unwrap_err();
    assert_eq!(asked.code, Code::NeedsConfirm, "{asked}");
    ok(
        host,
        "level",
        json!({"action": "delete", "name": "Mines", "confirm": token(&asked)}),
    );
    assert!(!levels.join("Mines.world.json").exists());
    ok(host, "history", json!({"action": "undo"}));
    assert!(
        levels.join("Mines.world.json").is_file(),
        "undo brings it back"
    );
    // Opening another level wants unsaved changes saved first.
    let wrong = call(host, "level", json!({"action": "open", "name": "Atlantis"})).unwrap_err();
    assert_eq!(wrong.code, Code::NotFound);
    assert!(wrong.to_string().contains("Castle"), "{wrong}");
    ok(host, "object_create", json!({"kind": "Block"}));
    let unsaved = call(host, "level", json!({"action": "open", "name": "Castle"})).unwrap_err();
    assert_eq!(unsaved.code, Code::Locked, "{unsaved}");
    assert!(unsaved.to_string().contains("project_save"), "{unsaved}");
    let asked = call(host, "project_save", json!({})).unwrap_err();
    ok(host, "project_save", json!({"confirm": token(&asked)}));
    let opened_text = ok(host, "level", json!({"action": "open", "name": "Castle"}));
    assert!(opened_text.starts_with("opened Castle"), "{opened_text}");
    let list = ok(host, "level", json!({}));
    assert!(list.contains("Castle · open"), "{list}");
    // A new world: changes from before it can't be listed.
    let changes = ok(host, "history", json!({"action": "changes", "since": 0}));
    assert!(changes.contains("can't list changes since 0"), "{changes}");
    assert!(changes.contains("a level opened"), "{changes}");
    let changes = ok(host, "history", json!({"action": "changes"}));
    assert!(changes.contains("nothing changed"), "{changes}");
}

#[test]
fn prefab_saves_places_and_applies_copies_as_undo_steps() {
    let mut opened = open("hello-part", "mcp-prefab");
    let host = &mut opened.app;
    let made = ok(
        host,
        "object_create",
        json!({"kind": "Block", "name": "Crate"}),
    );
    let crate_id = made.split_whitespace().nth(1).unwrap().to_owned();
    let saved = ok(host, "prefab", json!({"action": "save", "id": crate_id}));
    assert!(
        saved.starts_with("saved assets/prefabs/Crate.prefab.json"),
        "{saved}"
    );
    let path = "assets/prefabs/Crate.prefab.json";
    assert!(opened.folder.join(path).is_file());
    let placed = ok(
        host,
        "prefab",
        json!({"action": "place", "path": path, "position": [20, 3, 0]}),
    );
    assert!(
        placed.contains("undo \"AI (test): Place Crate\""),
        "{placed}"
    );
    let copy = placed
        .trim_start_matches("placed ")
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned();
    let position = ok(
        host,
        "object_get",
        json!({"id": copy, "props": ["Position"]}),
    );
    assert!(position.contains("Position [20,3,0]"), "{position}");
    // A change on the copy is an override, which apply writes back.
    ok(
        host,
        "object_set",
        json!({"id": copy, "props": {"Transparency": 0.5}}),
    );
    let overrides = ok(host, "prefab", json!({"action": "overrides", "id": copy}));
    assert!(overrides.contains("Transparency"), "{overrides}");
    let asked = call(host, "prefab", json!({"action": "apply", "id": copy})).unwrap_err();
    assert_eq!(asked.code, Code::NeedsConfirm, "{asked}");
    ok(
        host,
        "prefab",
        json!({"action": "apply", "id": copy, "confirm": token(&asked)}),
    );
    let file = std::fs::read_to_string(opened.folder.join(path)).unwrap();
    assert!(file.contains("0.5"), "{file}");
    // The first copy follows it too.
    let first = ok(
        host,
        "object_get",
        json!({"id": crate_id, "props": ["Transparency"]}),
    );
    assert!(first.contains("0.5"), "{first}");
    let unpacked = ok(host, "prefab", json!({"action": "unpack", "id": copy}));
    assert!(unpacked.contains("no longer follows"), "{unpacked}");
    let not_a_copy = call(host, "prefab", json!({"action": "revert", "id": copy})).unwrap_err();
    assert_eq!(not_a_copy.code, Code::BadValue, "{not_a_copy}");
}

#[test]
fn solid_op_unions_and_separates_parts_as_undo_steps() {
    let mut opened = open("hello-part", "mcp-solid");
    let host = &mut opened.app;
    let first = ok(
        host,
        "object_create",
        json!({"kind": "Block", "position": [0, 5, 0], "size": [4, 4, 4]}),
    );
    let second = ok(
        host,
        "object_create",
        json!({"kind": "Block", "position": [2, 5, 0], "size": [4, 4, 4]}),
    );
    let ids: Vec<String> = [first, second]
        .iter()
        .map(|text| text.split_whitespace().nth(1).unwrap().to_owned())
        .collect();
    let one = call(host, "solid_op", json!({"op": "union", "id": ids[0]})).unwrap_err();
    assert_eq!(one.code, Code::BadArgs, "{one}");
    let union = ok(host, "solid_op", json!({"op": "union", "ids": ids}));
    assert!(union.contains("UnionOperation"), "{union}");
    assert!(union.contains("undo \"AI (test): "), "{union}");
    let made = union
        .split("→ ")
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned();
    let separated = ok(host, "solid_op", json!({"op": "separate", "id": made}));
    assert!(separated.starts_with("Separate"), "{separated}");
    ok(host, "history", json!({"action": "undo", "steps": 2}));
    let parts = ok(host, "scene_inspect", json!({"class": "UnionOperation"}));
    assert!(!parts.contains("UnionOperation \""), "{parts}");
}

#[test]
fn style_edit_lists_shows_and_changes_rules_by_selector() {
    let mut opened = open("hello-css", "mcp-style");
    let host = &mut opened.app;
    let list = ok(host, "style_edit", json!({}));
    assert!(
        list.starts_with("ui.css · ") && list.contains("\n#score"),
        "{list}"
    );
    let score = ok(
        host,
        "style_edit",
        json!({"action": "get", "selector": "#score"}),
    );
    assert!(
        score.contains("font-size: 20px;") && score.ends_with("in ui.css"),
        "{score}"
    );
    // Change one in place, add one, remove one, add a :hover rule.
    let set = ok(
        host,
        "style_edit",
        json!({"action": "set", "selector": "#score", "props": {"font-size": "24px", "opacity": 0.8, "font-family": null}}),
    );
    assert!(set.contains("undo \"AI (test): Style #score\""), "{set}");
    ok(
        host,
        "style_edit",
        json!({"action": "set", "selector": "#score:hover", "props": {"color": "#ffffff"}}),
    );
    let score = ok(
        host,
        "style_edit",
        json!({"action": "get", "selector": "#score"}),
    );
    assert!(
        score.contains("font-size: 24px;") && score.contains("opacity: 0.8;"),
        "{score}"
    );
    assert!(!score.contains("font-family"), "{score}");
    assert!(
        score.contains("#score:hover { color: #ffffff; }"),
        "{score}"
    );
    let text = host.app.stylesheet_text("ui.css").unwrap();
    assert_eq!(text.matches("font-size").count(), 1, "{text}");
    // Undo puts the text back.
    ok(host, "history", json!({"action": "undo", "steps": 2}));
    let score = ok(
        host,
        "style_edit",
        json!({"action": "get", "selector": "#score"}),
    );
    assert!(
        score.contains("font-size: 20px;") && score.contains("font-family"),
        "{score}"
    );
    let bad = call(
        host,
        "style_edit",
        json!({"action": "set", "selector": "#score", "props": {"color": "red; } x {"}}),
    )
    .unwrap_err();
    assert_eq!(bad.code, Code::BadValue, "{bad}");
}

#[test]
fn project_settings_changes_input_collision_groups_and_strings() {
    let mut opened = open("hello-part", "mcp-settings");
    let host = &mut opened.app;
    let input = ok(host, "project_settings", json!({"setting": "input"}));
    assert!(input.contains("\nJump: Space | ButtonA"), "{input}");
    // A new action and its key are one step.
    let dash = ok(
        host,
        "project_settings",
        json!({"setting": "input", "action": "add", "name": "Dash", "input": "Q"}),
    );
    assert!(
        dash.contains("undo \"AI (test): Add input to Dash\""),
        "{dash}"
    );
    let input = ok(host, "project_settings", json!({"setting": "input"}));
    assert!(input.contains("\nDash: Q"), "{input}");
    let file = std::fs::read_to_string(opened.folder.join("input.json")).unwrap();
    assert!(file.contains("Dash"), "{file}");
    ok(host, "history", json!({"action": "undo"}));
    let input = ok(host, "project_settings", json!({"setting": "input"}));
    assert!(!input.contains("Dash"), "{input}");
    let wrong = call(
        host,
        "project_settings",
        json!({"setting": "input", "action": "add", "name": "Jump", "input": "Spaceship"}),
    )
    .unwrap_err();
    assert_eq!(wrong.code, Code::BadValue, "{wrong}");
    // Collision groups.
    ok(
        host,
        "project_settings",
        json!({"setting": "collision", "action": "add", "name": "Ghosts"}),
    );
    ok(
        host,
        "project_settings",
        json!({"setting": "collision", "action": "set", "name": "Ghosts", "other": "Default", "collide": false}),
    );
    let groups = ok(host, "project_settings", json!({"setting": "collision"}));
    assert!(
        groups.contains("Ghosts") && groups.contains("don't collide: "),
        "{groups}"
    );
    // Strings.
    let set = ok(
        host,
        "project_settings",
        json!({"setting": "strings", "action": "set", "name": "greeting", "value": "Hello"}),
    );
    assert!(set.contains("unsaved"), "{set}");
    let strings = ok(host, "project_settings", json!({"setting": "strings"}));
    assert!(strings.contains("\ngreeting: \"Hello\""), "{strings}");
    ok(host, "history", json!({"action": "undo"}));
    let strings = ok(host, "project_settings", json!({"setting": "strings"}));
    assert!(!strings.contains("greeting"), "{strings}");
}

#[test]
fn asset_manage_renames_with_references_and_deletes_after_asking() {
    let mut opened = open("hello-material-graph", "mcp-asset-manage");
    let host = &mut opened.app;
    let renamed = ok(
        host,
        "asset_manage",
        json!({"action": "rename", "path": "assets/textures/moss.png", "to": "lichen.png"}),
    );
    assert!(
        renamed.starts_with("assets/textures/moss.png is now assets/textures/lichen.png"),
        "{renamed}"
    );
    let folder = opened.folder.clone();
    assert!(folder.join("assets/textures/lichen.png").is_file());
    let users = ok(
        host,
        "asset_find",
        json!({"name": "lichen", "detail": "full"}),
    );
    assert!(
        users.contains("used by "),
        "what used moss.png follows: {users}"
    );
    // Delete asks first, naming what still uses it.
    let args = json!({"action": "delete", "path": "assets/textures/lichen.png"});
    let asked = call(host, "asset_manage", args.clone()).unwrap_err();
    assert_eq!(asked.code, Code::NeedsConfirm, "{asked}");
    assert!(asked.to_string().contains("still use"), "{asked}");
    let mut args = args;
    args["confirm"] = json!(token(&asked));
    ok(host, "asset_manage", args);
    assert!(!folder.join("assets/textures/lichen.png").exists());
    ok(host, "history", json!({"action": "undo", "steps": 2}));
    assert!(folder.join("assets/textures/moss.png").is_file());
    let missing = call(
        host,
        "asset_manage",
        json!({"action": "delete", "path": "assets/nothing.png"}),
    )
    .unwrap_err();
    assert_eq!(missing.code, Code::NotFound, "{missing}");
}

#[test]
fn history_changes_says_what_changed_since_last_asked_and_by_whom() {
    let mut opened = open("hello-part", "mcp-changes");
    let host = &mut opened.app;
    let first = ok(host, "history", json!({"action": "changes"}));
    assert_eq!(first, "mark 0 · nothing changed since 0");
    // The user (or another client) adds two parts and paints them, between calls.
    let workspace = host.app.world.workspace();
    let a = add(host, ClassName::Part, "Crate", workspace);
    let b = add(host, ClassName::Part, "Crate", workspace);
    let block = host.app.world.find_first_child(workspace, "Block").unwrap();
    host.app
        .world
        .set_property(block, "Transparency", axcenity_core::Value::Number(0.5))
        .unwrap();
    let wall = ok(
        host,
        "object_create",
        json!({"kind": "Block", "name": "Wall"}),
    );
    let wall_id = wall.split_whitespace().nth(1).unwrap().to_owned();
    ok(
        host,
        "object_set",
        json!({"id": id_text(block), "props": {"Color": [1, 0, 0]}}),
    );
    let text = ok(host, "history", json!({"action": "changes"}));
    let lines: Vec<&str> = text.lines().collect();
    assert!(
        lines[0].starts_with("mark ") && lines[0].ends_with(" · since 0:"),
        "{text}"
    );
    // This client's first, then everyone else's; alike objects fold, and
    // what both changed is said once, as someone else's.
    let mine = lines.iter().position(|line| *line == "by you:").unwrap();
    let theirs = lines
        .iter()
        .position(|line| line.starts_with("by others"))
        .unwrap();
    assert!(mine < theirs, "{text}");
    assert!(
        lines[mine + 1].starts_with(&format!("added {wall_id} Part \"Wall\" in Workspace")),
        "{text}"
    );
    assert!(
        lines.contains(
            &format!(
                "added 2× Part \"Crate\" in Workspace ({}, {})",
                id_text(a),
                id_text(b)
            )
            .as_str()
        ),
        "{text}"
    );
    assert!(
        lines.contains(
            &format!("set {} Part \"Block\": Color, Transparency", id_text(block)).as_str()
        ),
        "{text}"
    );
    // Asked again: only what's new since that answer.
    let again = ok(host, "history", json!({"action": "changes"}));
    assert!(again.contains("nothing changed"), "{again}");
    ok(host, "object_delete", json!({"id": wall_id}));
    let removed = ok(host, "history", json!({"action": "changes"}));
    assert!(
        removed.ends_with(&format!("\nremoved {wall_id} Part \"Wall\"")),
        "{removed}"
    );
    // Undo is a change too; an old mark still lists everything since it.
    ok(host, "history", json!({"action": "undo"}));
    let back = ok(host, "history", json!({"action": "changes"}));
    assert!(
        back.contains(&format!("added {wall_id} Part \"Wall\"")),
        "{back}"
    );
    let all = ok(host, "history", json!({"action": "changes", "since": 0}));
    assert!(all.contains("Crate"), "{all}");
    let ahead = ok(
        host,
        "history",
        json!({"action": "changes", "since": 99_999}),
    );
    assert!(ahead.contains("is ahead of it"), "{ahead}");
}

#[test]
fn prompts_name_only_listed_tools_and_resources_mirror_lite_read_tools() {
    use axcenity_mcp::Class;
    let goal = serde_json::Map::from_iter([("goal".to_owned(), json!("a hut"))]);
    let lite = super::prompts::PROMPTS[0]
        .message(goal.clone(), &tools_for(Profile::Lite))
        .unwrap();
    assert!(
        lite.contains("a hut") && lite.contains("object_create"),
        "{lite}"
    );
    // Lite has no batch, but saves.
    assert!(
        !lite.contains("batch") && lite.contains("project_save"),
        "{lite}"
    );
    let standard = super::prompts::PROMPTS[0]
        .message(goal, &tools_for(Profile::Standard))
        .unwrap();
    assert!(
        standard.contains("one batch") && standard.contains("7. project_save"),
        "{standard}"
    );
    let fix = super::prompts::PROMPTS[1]
        .message(serde_json::Map::new(), &tools_for(Profile::Lite))
        .unwrap();
    assert!(
        fix.starts_with("Find and fix") && !fix.contains("asset_find"),
        "{fix}"
    );
    assert!(
        super::prompts::PROMPTS[0]
            .message(serde_json::Map::new(), &[])
            .is_err()
    );
    // Every resource reads through a read-only tool every profile lists,
    // with a parameter that tool has.
    for resource in super::prompts::RESOURCES {
        let spec = tools_for(Profile::Lite)
            .into_iter()
            .find(|spec| spec.name == resource.tool)
            .unwrap_or_else(|| panic!("{} isn't a lite tool", resource.tool));
        assert_eq!(spec.class, Class::Read, "{}", resource.uri);
        if let Some((_, name)) = resource.template() {
            assert!(spec.param(name).is_some(), "{}", resource.uri);
        }
    }
}

#[test]
fn a_resource_reads_the_same_as_its_tool() {
    let mut opened = open("hello-part", "mcp-resources");
    let host = &mut opened.app;
    let object = super::prompts::RESOURCES
        .iter()
        .find(|resource| resource.name == "object")
        .unwrap();
    let args = object
        .args_for("axcenity://object/Workspace%2FBlock")
        .unwrap();
    let spec = TOOLS
        .iter()
        .find(|tool| tool.spec.name == "object_get")
        .unwrap()
        .spec;
    let read = run(&mut host.host(), spec, &Args::new(args), Some("test")).unwrap();
    assert_eq!(
        read,
        ok(host, "object_get", json!({"id": "Workspace/Block"}))
    );
    assert!(read.contains("\"Block\""), "{read}");
}

#[test]
fn check_reports_the_project_and_prints_client_configs_with_full_paths() {
    let opened = open("hello-part", "mcp-check");
    let folder = opened.folder.clone();
    let options = super::parse_options(
        ["--check", folder.to_str().unwrap(), "--profile", "lite"]
            .into_iter()
            .map(std::ffi::OsString::from),
    )
    .unwrap();
    assert!(options.check);
    let exe = std::path::Path::new("/opt/axcenity/axcenity-editor");
    let (report, ok) = super::check::report(&options, exe);
    assert!(ok, "{report}");
    assert!(report.contains("\nok   project \""), "{report}");
    assert!(report.contains("\nok   mode edit"), "{report}");
    assert!(report.contains("\nok   calls are logged to "), "{report}");
    assert!(report.contains("\nok   profile lite: 13 tools"), "{report}");
    assert!(!report.contains("\nwarn"), "{report}");
    let full = std::fs::canonicalize(&folder).unwrap();
    let json_folder = serde_json::to_string(&full.display().to_string()).unwrap();
    assert!(
        report.contains(&format!(
            "\"command\": \"/opt/axcenity/axcenity-editor\", \"args\": [\"--mcp\", {json_folder}, \"--profile\", \"lite\"]"
        )),
        "{report}"
    );
    assert!(
        report.contains("claude mcp add axcenity -- /opt/axcenity/axcenity-editor --mcp "),
        "{report}"
    );
    assert!(report.contains("[mcp_servers.axcenity]"), "{report}");
    // The probe file is gone again.
    assert!(!full.join(".axcenity/.mcp-check").exists());

    // No project, or an import folder that isn't there: FAIL, exit 1.
    let options = super::parse_options(
        [
            "--check",
            "/no/such/project",
            "--allow-import",
            "/no/such/folder",
        ]
        .into_iter()
        .map(std::ffi::OsString::from),
    )
    .unwrap();
    let (report, ok) = super::check::report(&options, exe);
    assert!(!ok, "{report}");
    assert!(
        report.contains("\nFAIL could not open a project in /no/such/project"),
        "{report}"
    );
    assert!(
        report.contains("\nFAIL --allow-import /no/such/folder: not a folder"),
        "{report}"
    );
}

/// A client of the open editor, as the relay connects it: JSON-RPC lines
/// over the editor's loopback connection.
struct LiveClient {
    writer: std::net::TcpStream,
    lines: std::io::Lines<std::io::BufReader<std::net::TcpStream>>,
    next: u64,
}

impl LiveClient {
    fn connect(folder: &std::path::Path, mode: Mode) -> LiveClient {
        let hello = super::live::Hello {
            profile: Profile::Standard,
            mode,
            import_folders: Vec::new(),
            probe: false,
        };
        let stream = super::live::connect(folder, &hello)
            .expect("the editor says ok")
            .expect("the editor is listening");
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(20)))
            .unwrap();
        let mut client = LiveClient {
            writer: stream.try_clone().unwrap(),
            lines: std::io::BufRead::lines(std::io::BufReader::new(stream)),
            next: 0,
        };
        let init = client.request(
            "initialize",
            json!({"protocolVersion": "2025-11-25", "capabilities": {},
                "clientInfo": {"name": "Live Test", "version": "1"}}),
        );
        assert_eq!(init["result"]["serverInfo"]["name"], "axcenity", "{init}");
        client.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
        client
    }

    fn send(&mut self, message: Value) {
        use std::io::Write;
        writeln!(self.writer, "{message}").unwrap();
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        let id = self.next;
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        loop {
            let line = self.lines.next().expect("the editor answers").unwrap();
            let message: Value = serde_json::from_str(&line).unwrap();
            if message["id"] == id {
                return message;
            }
        }
    }

    /// A tool's text, and whether it was an error.
    fn call(&mut self, tool: &str, args: Value) -> (String, bool) {
        let answer = self.request("tools/call", json!({"name": tool, "arguments": args}));
        let result = &answer["result"];
        (
            result["content"][0]["text"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            result["isError"] == true,
        )
    }
}

#[test]
fn the_open_editor_runs_a_clients_calls_between_frames_as_undo_steps() {
    use super::live::{ENDPOINT_FILE, Live, editor_listening};
    use std::sync::mpsc;

    let folder = copy_of_example("hello-part", "mcp-live");
    let mut app = EditorApp::new(Some(&folder));
    assert!(app.project.is_some());
    assert!(!editor_listening(&folder));
    let mut live = Live::start(&folder, axcenity_mcp::Waker(std::sync::Arc::new(|| {}))).unwrap();
    assert!(folder.join(ENDPOINT_FILE).exists());
    assert!(editor_listening(&folder));

    // The client: creates a wall, waits while the user adds a crate, then
    // asks what changed. A second client may only read.
    let (said, heard) = mpsc::channel::<String>();
    let (go, wait) = mpsc::channel::<()>();
    let client_folder = folder.clone();
    let client = std::thread::spawn(move || {
        let mut client = LiveClient::connect(&client_folder, Mode::Edit);
        let (created, failed) =
            client.call("object_create", json!({"kind": "Block", "name": "Wall"}));
        assert!(!failed, "{created}");
        said.send(created).unwrap();
        wait.recv().unwrap();
        let (changes, _) = client.call("history", json!({"action": "changes"}));
        let mut reader = LiveClient::connect(&client_folder, Mode::Read);
        let refused = reader.call("object_create", json!({"kind": "Block"}));
        (changes, refused)
    });
    // The editor's frames: each drains the calls waiting.
    let started = std::time::Instant::now();
    while !client.is_finished() {
        assert!(started.elapsed().as_secs() < 60, "the client finished");
        live.drain(&mut app);
        if let Ok(created) = heard.try_recv() {
            assert!(created.contains("\"Wall\""), "{created}");
            let workspace = app.world.workspace();
            let crate_id = app.world.create(ClassName::Part).unwrap();
            app.world.set_name(crate_id, "Crate").unwrap();
            app.world.set_parent(crate_id, Some(workspace)).unwrap();
            go.send(()).unwrap();
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let (changes, (refused, failed)) = client.join().unwrap();
    // The wall is in the editor's own world, as an undo step of its own.
    let workspace = app.world.workspace();
    assert!(app.world.find_first_child(workspace, "Wall").is_some());
    assert!(
        app.history
            .undo_labels()
            .iter()
            .any(|label| label.starts_with("AI (Live Test)")),
        "{:?}",
        app.history.undo_labels()
    );
    assert!(changes.contains("by you:"), "{changes}");
    assert!(changes.contains("Part \"Wall\""), "{changes}");
    assert!(changes.contains("by others"), "{changes}");
    assert!(changes.contains("Part \"Crate\""), "{changes}");
    assert!(failed && refused.contains("read"), "{refused}");
    // Both clients are listed until their connections end.
    let names: Vec<String> = live.clients().into_iter().map(|c| c.name).collect();
    assert!(names.len() <= 2, "{names:?}");

    // A stale token is refused; stopping removes the endpoint.
    {
        use std::io::{BufRead, Write};
        let text = std::fs::read_to_string(folder.join(ENDPOINT_FILE)).unwrap();
        let port = serde_json::from_str::<Value>(&text).unwrap()["port"]
            .as_u64()
            .unwrap();
        let mut stream = std::net::TcpStream::connect(("127.0.0.1", port as u16)).unwrap();
        writeln!(stream, "{}", json!({"token": "old"})).unwrap();
        let mut answer = String::new();
        std::io::BufReader::new(stream)
            .read_line(&mut answer)
            .unwrap();
        assert!(answer.starts_with("no wrong token"), "{answer}");
    }
    drop(live);
    assert!(!folder.join(ENDPOINT_FILE).exists());
    assert!(!editor_listening(&folder));
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn the_ai_connections_setting_starts_and_stops_listening_for_the_open_project() {
    use super::live::{ENDPOINT_FILE, HEADLESS_FILE, editor_listening};

    let folder = copy_of_example("hello-part", "mcp-live-setting");
    std::fs::create_dir_all(folder.join(".axcenity")).unwrap();
    std::fs::write(folder.join(HEADLESS_FILE), "{}").unwrap();
    let mut app = EditorApp::new(Some(&folder));
    let ctx = crate::theme::test_context();
    // Off: nothing listens, but a windowless server's mark is warned about.
    app.ai_frame(&ctx);
    assert!(app.ai.live.is_none());
    assert!(
        app.output
            .lines()
            .iter()
            .any(|line| line.text.contains("without the window")),
        "warned"
    );
    app.ai.allowed = true;
    app.ai_frame(&ctx);
    assert!(app.ai.live.is_some(), "{:?}", app.ai.error);
    assert!(editor_listening(&folder));
    // The page draws with the project's port and the Claude Code line.
    let _ = crate::theme::run(&ctx, Default::default(), |ui| app.ai_page(ui));
    app.ai.allowed = false;
    app.ai_frame(&ctx);
    assert!(app.ai.live.is_none());
    assert!(!folder.join(ENDPOINT_FILE).exists());
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn render_stats_reads_the_viewports_last_frame_and_says_why_without_a_window() {
    let mut opened = open("hello-part", "mcp-render-stats");
    let host = &mut opened.app;
    let error = call(host, "render_stats", json!({})).unwrap_err();
    assert_eq!(error.code, Code::Unsupported);
    assert!(error.to_string().contains("AI connections"), "{error}");
    // The open editor keeps its viewport's last frame.
    let mut cost = crate::profiler::RenderCost {
        cpu: std::time::Duration::from_micros(2_500),
        lights: 3,
        ..Default::default()
    };
    cost.stats.draw_calls = 42;
    cost.stats.gpu_time = Some(std::time::Duration::from_micros(4_000));
    host.app.last_drawn = Some(super::LastDrawn {
        cost,
        at: std::time::Instant::now(),
        size: (1280, 720),
        playing: false,
    });
    let text = ok(host, "render_stats", json!({}));
    assert!(
        text.starts_with("viewport 1280×720 · editing · last frame "),
        "{text}"
    );
    assert!(
        text.contains("CPU 2.50 ms · GPU 4.00 ms · 3 lights"),
        "{text}"
    );
    assert!(text.contains("Draws 42 calls"), "{text}");
    assert!(text.contains("GPU memory"), "{text}");
    assert!(!text.contains("Play"), "nothing played: {text}");
}
