//! The MCP tools on copies of the examples, called as a client would:
//! a tool name and JSON arguments, checked by the same `Args::check`.

use axcenity_core::{ClassName, InstanceId};
use axcenity_mcp::{Args, Code, Mode, Profile, ToolError, text::estimate_tokens, tools_list_json};
use serde_json::{Value, json};

use super::super::EditorApp;
use super::super::tests::copy_of_example;
use super::refs::id_text;
use super::{Host, TOOLS, run, tools_for};

struct Opened {
    app: Host,
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
        app: Host::new(app, Mode::Edit),
        folder,
    }
}

/// Calls `tool` with `args` as a client would: arguments checked first.
fn call(app: &mut Host, tool: &str, args: Value) -> Result<String, ToolError> {
    let spec = TOOLS
        .iter()
        .find(|entry| entry.spec.name == tool)
        .unwrap_or_else(|| panic!("no tool {tool}"))
        .spec;
    let args = Args::new(args.as_object().cloned().unwrap_or_default());
    args.check(spec)?;
    run(app, spec, &args, Some("test"))
}

fn ok(app: &mut Host, tool: &str, args: Value) -> String {
    call(app, tool, args).unwrap_or_else(|error| panic!("{tool} failed: {error}"))
}

fn add(host: &mut Host, class: ClassName, name: &str, parent: InstanceId) -> InstanceId {
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
        (Profile::Lite, 2000),
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
    let mut app = Host::new(EditorApp::new(None), Mode::Edit);
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

fn objects(host: &Host) -> usize {
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
