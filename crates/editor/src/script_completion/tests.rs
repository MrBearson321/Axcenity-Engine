use axcenity_core::{ClassName, World};

use super::*;

/// A world with a Part named Door, and a Folder named Lights holding a
/// PointLight, in the Workspace.
fn world() -> World {
    let mut world = World::new();
    let workspace = world.workspace();
    let door = world.create(ClassName::Part).unwrap();
    world.set_name(door, "Door").unwrap();
    world.set_parent(door, Some(workspace)).unwrap();
    let lights = world.create(ClassName::Folder).unwrap();
    world.set_name(lights, "Lights").unwrap();
    world.set_parent(lights, Some(workspace)).unwrap();
    let light = world.create(ClassName::PointLight).unwrap();
    world.set_name(light, "Lamp").unwrap();
    world.set_parent(light, Some(lights)).unwrap();
    world
}

/// The labels offered at the end of `source`.
fn labels(source: &str) -> Vec<String> {
    complete(source, source.len(), &world())
        .map(|found| found.items.into_iter().map(|item| item.label).collect())
        .unwrap_or_default()
}

fn offers(source: &str, label: &str) -> bool {
    labels(source).iter().any(|offered| offered == label)
}

#[test]
fn an_instance_path_offers_its_children_then_its_members() {
    let found = labels("workspace.");
    assert!(found.contains(&"Door".to_owned()), "{found:?}");
    assert!(found.contains(&"Lights".to_owned()));
    assert!(found.contains(&"Name".to_owned()), "Instance's own");
    assert!(!found.contains(&"Destroy".to_owned()), "methods follow `:`");
    assert!(offers("workspace.Door.", "Anchored"));
    assert!(offers("workspace.Lights.Lamp.", "Brightness"));
    assert!(offers("workspace.Door:", "Destroy"));
    assert!(offers("workspace[\"Lights\"].", "Lamp"));
}

#[test]
fn only_what_starts_with_the_typed_word_and_its_start() {
    let source = "workspace.Door.Anch";
    let found = complete(source, source.len(), &world()).unwrap();
    assert_eq!(found.start, "workspace.Door.".len());
    assert_eq!(
        found
            .items
            .iter()
            .map(|item| item.label.as_str())
            .collect::<Vec<_>>(),
        ["Anchored"]
    );
    assert_eq!(found.items[0].kind, ItemKind::Property);
    assert!(
        found.items[0].detail.contains("Anchored: boolean"),
        "{:?}",
        found.items[0]
    );
    // Typed out in full, there's nothing left to offer.
    assert_eq!(complete("workspace.Door.Anchored", 23, &world()), None);
}

#[test]
fn services_by_get_service_and_by_name() {
    assert!(offers("game:GetService(\"Players\"):", "GetPlayers"));
    assert!(offers("game.Players:", "GetPlayers"));
    assert!(offers("game:GetService('Workspace').", "Door"));
    assert!(offers("game:", "GetService"));
}

#[test]
fn locals_are_followed_to_their_types() {
    assert!(offers("local door = workspace.Door\ndoor.", "Anchored"));
    assert!(offers(
        "local folder = workspace:WaitForChild(\"Lights\")\nfolder.",
        "Lamp"
    ));
    assert!(offers(
        "local part = Instance.new(\"Part\")\npart:",
        "Destroy"
    ));
    assert!(offers("local part = Instance.new(\"Part\")\npart.", "Size"));
    assert!(offers("local light: PointLight = nil\nlight.", "Range"));
    assert!(offers(
        "local function open(part: Part)\n  part.",
        "Anchored"
    ));
    assert!(offers(
        "for _, child in workspace:GetChildren() do\n  child:",
        "Destroy"
    ));
    assert!(offers(
        "local copy = workspace.Door:Clone()\ncopy.",
        "Anchored"
    ));
    // A loop of locals set to each other ends.
    assert!(labels("local a = b\nlocal b = a\na.").is_empty());
}

#[test]
fn values_libraries_and_enums() {
    assert!(offers("Vector3.", "new"));
    assert!(offers("local v = Vector3.new(1, 2, 3)\nv.", "X"));
    assert!(offers("local v = Vector3.new(1, 2, 3)\nv.", "Magnitude"));
    assert!(offers("workspace.Door.Position.", "Y"));
    assert!(offers("task.", "wait"));
    assert!(offers("Enum.Mat", "Material"));
    assert!(offers("Enum.Material.", "Neon"));
    assert!(offers("workspace.Door.Touched:", "Connect"));
}

#[test]
fn a_bare_word_offers_keywords_locals_and_globals() {
    assert!(offers("wh", "while"));
    assert!(offers("pri", "print"));
    assert!(offers("work", "workspace"));
    assert!(offers("local health = 3\nhea", "health"));
    assert!(offers("local function openDoor()\nend\nopen", "openDoor"));
    assert!(offers("local function f(speed, angle)\n  ang", "angle"));
    assert_eq!(labels(""), Vec::<String>::new(), "nothing typed");
}

#[test]
fn nothing_inside_strings_or_comments_or_numbers() {
    assert!(labels("print(\"workspace.").is_empty());
    assert!(labels("-- workspace.").is_empty());
    assert!(labels("local x = 1.").is_empty());
    assert!(labels("local s = \"a\"..").is_empty());
    // After a closed string, completion is back.
    assert!(offers("print(\"hi\") workspace.", "Door"));
    assert!(!labels("workspace.Nowhere.").contains(&"Name".to_owned()));
}

#[test]
fn the_signature_of_the_call_the_cursor_is_in() {
    let at = |source: &str| signature_at(source, source.len(), &world());
    let found = at("workspace:FindFirstChild(\"Door\", ").unwrap();
    assert!(
        found.label.contains("FindFirstChild(name: string"),
        "{found:?}"
    );
    assert_eq!(found.argument, 1);
    let found = at("Vector3.new(").unwrap();
    assert!(found.label.starts_with("Vector3.new("), "{found:?}");
    assert_eq!(found.argument, 0);
    let found = at("task.wait(math.max(1, 2), ").unwrap();
    assert!(found.label.starts_with("task.wait("), "{found:?}");
    assert_eq!(found.argument, 1, "the inner call's comma doesn't count");
    assert_eq!(at("local t = { 1, "), None);
    assert_eq!(at("print(\"(\""), None, "inside a string");
    assert_eq!(at("workspace.Door"), None, "not in a call");
}

#[test]
fn completion_stays_fast_in_a_long_script() {
    let mut source = String::new();
    for i in 0..5_000 {
        source.push_str(&format!(
            "local value{i} = workspace.Door.Position.X + {i}\n"
        ));
    }
    source.push_str("local door = workspace.Door\ndoor.");
    let world = world();
    let started = std::time::Instant::now();
    let found = complete(&source, source.len(), &world).unwrap();
    let signature = signature_at(&source, source.len(), &world);
    let elapsed = started.elapsed();
    assert!(found.items.iter().any(|item| item.label == "Anchored"));
    assert_eq!(signature, None);
    // A debug build; a release build is many times faster.
    assert!(elapsed.as_millis() < 250, "{elapsed:?}");
}
