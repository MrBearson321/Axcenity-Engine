//! The MCP tools on copies of the examples, called as a client would:
//! a tool name and JSON arguments, checked by the same `Args::check`.

use axcenity_core::{ClassName, InstanceId};
use axcenity_mcp::{Args, Code, Profile, ToolError, text::estimate_tokens, tools_list_json};
use serde_json::{Value, json};

use super::super::EditorApp;
use super::super::tests::copy_of_example;
use super::refs::id_text;
use super::{TOOLS, run, tools_for};

struct Opened {
    app: EditorApp,
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
    Opened { app, folder }
}

/// Calls `tool` with `args` as a client would: arguments checked first.
fn call(app: &mut EditorApp, tool: &str, args: Value) -> Result<String, ToolError> {
    let spec = TOOLS
        .iter()
        .find(|entry| entry.spec.name == tool)
        .unwrap_or_else(|| panic!("no tool {tool}"))
        .spec;
    let args = Args::new(args.as_object().cloned().unwrap_or_default());
    args.check(spec)?;
    run(app, spec, &args)
}

fn ok(app: &mut EditorApp, tool: &str, args: Value) -> String {
    call(app, tool, args).unwrap_or_else(|error| panic!("{tool} failed: {error}"))
}

fn add(app: &mut EditorApp, class: ClassName, name: &str, parent: InstanceId) -> InstanceId {
    let id = app.world.create(class).unwrap();
    app.world.set_name(id, name).unwrap();
    app.world.set_parent(id, Some(parent)).unwrap();
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
        .world
        .find_first_child(opened.app.world.workspace(), "Block")
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
    let workspace = app.world.workspace();
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
    let workspace = app.world.workspace();
    let far = add(app, ClassName::Part, "FarRock", workspace);
    app.world
        .set_property(
            far,
            "Position",
            axcenity_core::Value::Vector3(axcenity_core::Vector3::new(500.0, 0.0, 0.0)),
        )
        .unwrap();
    app.world.add_tag(far, "Rocks").unwrap();
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
    let mut app = EditorApp::new(None);
    // No project: project_info explains instead of panicking.
    let error = call(&mut app, "project_info", json!({})).unwrap_err();
    assert_eq!(error.code, Code::NotFound);
}
