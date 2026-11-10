//! Tests for Phase 77: dialogues started by a ProximityPrompt or a script,
//! QuestService, the quest state in save slots, and languages.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axcenity_assets::{Assets, Cell, ColumnType, DataTable, write_data_table};
use axcenity_core::input::{InputEvent, InputKind};
use axcenity_core::{ClassName, InstanceId, KeyCode, StringTable, Value, Vector3, World};
use serde_json::json;

use axcenity_script::{OutputKind, ScriptRuntime};

use crate::dialogue::Dialogue;
use crate::quest;
use crate::{dialogue_active, dialogue_view, quest_tracker};

const FRAME: f64 = 1.0 / 60.0;

/// The farmer's dialogue: thanks once the apples quest is done, a nudge
/// while it's active, and otherwise a hello and a choice that gives it.
fn farmer_dialogue() -> serde_json::Value {
    let node = |id: u32, kind: &str, values: serde_json::Value| json!({ "id": id, "kind": kind, "position": [0.0, 0.0], "values": values });
    let wire = |from: u32, pin: &str, to: u32| json!({ "from": { "node": from, "pin": pin }, "to": { "node": to, "pin": "" } });
    json!({
        "format_version": 1,
        "nodes": [
            node(1, "start", json!({})),
            node(2, "quest_check", json!({ "Quest": "apples", "State": "Completed" })),
            node(3, "line", json!({ "Text": "farmer.thanks" })),
            node(4, "end", json!({ "Ending": "thanked" })),
            node(5, "quest_check", json!({ "Quest": "apples", "State": "Active" })),
            node(6, "line", json!({ "Text": "Five apples, please." })),
            node(7, "end", json!({ "Ending": "waiting" })),
            node(8, "line", json!({ "Speaker": "farmer.name", "Text": "farmer.hello" })),
            node(9, "choice", json!({ "Option 1": "farmer.yes", "Option 2": "farmer.no" })),
            node(10, "start_quest", json!({ "Quest": "apples" })),
            node(11, "set_attribute", json!({ "On": "Speaker", "Attribute": "Asked", "Value": "true" })),
            node(12, "line", json!({ "Text": "Off you go!" })),
            node(13, "end", json!({ "Ending": "accepted" })),
            node(14, "end", json!({ "Ending": "declined" })),
        ],
        "wires": [
            wire(1, "", 2),
            wire(2, "True", 3),
            wire(3, "", 4),
            wire(2, "False", 5),
            wire(5, "True", 6),
            wire(6, "", 7),
            wire(5, "False", 8),
            wire(8, "", 9),
            wire(9, "1", 10),
            wire(10, "", 11),
            wire(11, "", 12),
            wire(12, "", 13),
            wire(9, "2", 14),
        ],
    })
}

/// A project with the farmer's dialogue and the apples quest: collect 5
/// apples, then talk to the Farmer.
fn project(name: &str) -> PathBuf {
    let folder =
        std::env::temp_dir().join(format!("axcenity-dialogue-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    let dialogue = folder.join("assets/dialogue/Farmer.dialogue.json");
    std::fs::create_dir_all(dialogue.parent().unwrap()).unwrap();
    std::fs::write(&dialogue, farmer_dialogue().to_string()).unwrap();
    let mut quests = DataTable::default();
    quests.add_column("Title", ColumnType::String).unwrap();
    let apples = quests.add_row("apples", None);
    quests
        .set_cell(&apples, "Title", Cell::Text("quest.apples".into()))
        .unwrap();
    write_data_table(&folder.join("assets/data/quests.data.json"), &quests).unwrap();
    let mut objectives = DataTable::default();
    objectives.add_column("Quest", ColumnType::String).unwrap();
    objectives
        .add_column(
            "Kind",
            ColumnType::Enum {
                options: vec!["Count".into(), "Reach".into(), "Talk".into()],
            },
        )
        .unwrap();
    objectives.add_column("Target", ColumnType::String).unwrap();
    objectives.add_column("Count", ColumnType::Integer).unwrap();
    objectives.add_column("Text", ColumnType::String).unwrap();
    for (key, kind, target, count, text) in [
        ("pick", "Count", "Apple", 5, "Apples"),
        ("return", "Talk", "Farmer", 1, "Back to the Farmer"),
    ] {
        let row = objectives.add_row(key, None);
        objectives
            .set_cell(&row, "Quest", Cell::Text("apples".into()))
            .unwrap();
        objectives
            .set_cell(&row, "Kind", Cell::Text(kind.into()))
            .unwrap();
        objectives
            .set_cell(&row, "Target", Cell::Text(target.into()))
            .unwrap();
        objectives
            .set_cell(&row, "Count", Cell::Integer(count))
            .unwrap();
        objectives
            .set_cell(&row, "Text", Cell::Text(text.into()))
            .unwrap();
    }
    write_data_table(
        &folder.join("assets/data/quest_objectives.data.json"),
        &objectives,
    )
    .unwrap();
    folder
}

fn strings() -> Arc<StringTable> {
    let mut table = StringTable::default();
    table.add_language("English");
    table.add_language("Español");
    for (key, english, spanish) in [
        ("farmer.name", "Farmer", "Granjero"),
        (
            "farmer.hello",
            "Could you pick me some apples?",
            "¿Me recoges unas manzanas?",
        ),
        ("farmer.yes", "Yes", "Sí"),
        ("farmer.no", "No", "No"),
        ("farmer.thanks", "Thank you!", "¡Gracias!"),
        (
            "quest.apples",
            "Apples for the Farmer",
            "Manzanas para el granjero",
        ),
    ] {
        table.set(key, "English", english);
        table.set(key, "Español", spanish);
    }
    Arc::new(table)
}

fn part(world: &mut World, name: &str, parent: InstanceId, x: f32) -> InstanceId {
    let part = world.create(ClassName::Part).unwrap();
    world.set_name(part, name).unwrap();
    world.set_parent(part, Some(parent)).unwrap();
    for (property, value) in [
        ("Position", Value::Vector3(Vector3::new(x, 2.5, 0.0))),
        ("Size", Value::Vector3(Vector3::new(2.0, 5.0, 1.0))),
        ("Anchored", Value::Bool(true)),
    ] {
        world.set_property(part, property, value).unwrap();
    }
    part
}

/// A world with a player character at x = 0 and the Farmer (a Model with
/// a prompt that starts his dialogue) at x = 4.
fn farm(folder: &Path) -> (World, ScriptRuntime, InstanceId) {
    let mut world = World::new();
    world.set_strings(strings());
    let character = world.create(ClassName::Model).unwrap();
    world.set_name(character, "Character").unwrap();
    world
        .set_parent(character, Some(world.workspace()))
        .unwrap();
    part(&mut world, "Body", character, 0.0);
    let controller = world.create(ClassName::ThirdPersonController).unwrap();
    world.set_parent(controller, Some(character)).unwrap();
    let farmer = world.create(ClassName::Model).unwrap();
    world.set_name(farmer, "Farmer").unwrap();
    world.set_parent(farmer, Some(world.workspace())).unwrap();
    let body = part(&mut world, "Body", farmer, 4.0);
    let prompt = world.create(ClassName::ProximityPrompt).unwrap();
    world.set_parent(prompt, Some(body)).unwrap();
    world
        .set_property(
            prompt,
            "Dialogue",
            Value::String("assets/dialogue/Farmer.dialogue.json".into()),
        )
        .unwrap();
    world
        .set_property(prompt, "RequiresLineOfSight", Value::Bool(false))
        .unwrap();
    let mut runtime = crate::script_runtime();
    runtime.set_assets(Assets::new(folder));
    runtime.set_save_folder(Some(folder.join("saves")));
    (world, runtime, farmer)
}

fn start(world: &mut World, runtime: &mut ScriptRuntime, source: &str) {
    let script = world.create(ClassName::Script).unwrap();
    world
        .set_parent(script, Some(world.server_script_service()))
        .unwrap();
    runtime.start_script(world, script, "scripts/test.server.lua", source);
}

fn output(runtime: &mut ScriptRuntime) -> Vec<String> {
    runtime
        .take_output()
        .into_iter()
        .map(|line| match line.kind {
            OutputKind::Error => format!("error: {}", line.text),
            _ => line.text.replace('\t', " "),
        })
        .collect()
}

/// One frame of a host: input, prompts, then scripts.
fn frame(runtime: &mut ScriptRuntime, world: &mut World, events: &[InputEvent]) {
    runtime.input_events(world, events);
    runtime.step_prompts(world, FRAME);
    runtime.step(world, FRAME);
}

/// Presses and lets go of `key`, a frame each, then a frame more.
fn press(runtime: &mut ScriptRuntime, world: &mut World, key: KeyCode) {
    for pressed in [true, false] {
        frame(
            runtime,
            world,
            &[InputEvent::new(InputKind::Key { key, pressed })],
        );
    }
    frame(runtime, world, &[]);
}

fn quest_state(runtime: &ScriptRuntime, quest: &str) -> quest::QuestState {
    quest::state(&runtime.saves(), quest)
}

#[test]
fn a_prompt_runs_a_branching_dialogue_that_gives_a_quest_with_no_script() {
    let folder = project("prompt");
    let (mut world, mut runtime, farmer) = farm(&folder);
    frame(&mut runtime, &mut world, &[]);
    assert_eq!(
        runtime.shown_prompts().len(),
        1,
        "the Farmer's prompt shows"
    );
    assert!(dialogue_view(&runtime, &world).is_none());

    press(&mut runtime, &mut world, KeyCode::E);
    assert!(dialogue_active(&runtime));
    assert!(
        runtime.movement_locked(&world),
        "the character stands still"
    );
    assert!(runtime.shown_prompts().is_empty(), "prompts hide meanwhile");
    let view = dialogue_view(&runtime, &world).expect("the box shows");
    assert_eq!(view.speaker, "Farmer");
    assert_eq!(view.text, "Could you pick me some apples?");
    assert_eq!(
        view.choices,
        ["Yes", "No"],
        "the line's choices show under it"
    );
    assert_eq!(view.selected, 0);

    // Down moves to No, and wraps round to Yes.
    press(&mut runtime, &mut world, KeyCode::Down);
    assert_eq!(dialogue_view(&runtime, &world).unwrap().selected, 1);
    press(&mut runtime, &mut world, KeyCode::Down);
    assert_eq!(dialogue_view(&runtime, &world).unwrap().selected, 0);
    press(&mut runtime, &mut world, KeyCode::Return);
    let view = dialogue_view(&runtime, &world).unwrap();
    assert_eq!(view.text, "Off you go!");
    assert!(view.choices.is_empty());
    assert_eq!(quest_state(&runtime, "apples"), quest::QuestState::Active);
    assert_eq!(
        world.get(farmer).unwrap().attribute("Asked"),
        Some(&Value::Bool(true)),
        "Set Attribute set the speaker's"
    );

    press(&mut runtime, &mut world, KeyCode::E);
    assert!(!dialogue_active(&runtime));
    assert!(!runtime.movement_locked(&world));
    assert_eq!(runtime.shown_prompts().len(), 1, "the prompt shows again");
    let tracker = quest_tracker(&runtime, &world);
    assert_eq!(tracker.len(), 1);
    assert_eq!(tracker[0].title, "Apples for the Farmer");
    assert_eq!(tracker[0].objectives[0].text, "Apples");
    assert_eq!(
        (
            tracker[0].objectives[0].progress,
            tracker[0].objectives[0].count
        ),
        (0, 5)
    );

    // Talking again takes the Active branch.
    press(&mut runtime, &mut world, KeyCode::E);
    assert_eq!(
        dialogue_view(&runtime, &world).unwrap().text,
        "Five apples, please."
    );
    assert_eq!(output(&mut runtime), Vec::<String>::new());
}

#[test]
fn collecting_five_apples_completes_the_quest_and_slots_keep_its_progress() {
    let folder = project("apples");
    let (mut world, mut runtime, _) = farm(&folder);
    start(
        &mut world,
        &mut runtime,
        r#"
        local QuestService = game:GetService("QuestService")
        QuestService.QuestProgressed:Connect(function(quest, objective, progress)
            print("progressed", quest, objective, progress)
        end)
        QuestService.QuestCompleted:Connect(function(quest) print("completed", quest) end)
        _G.QuestService = QuestService
        print(QuestService:Start("apples"), QuestService:Start("apples"))
        for _ = 1, 3 do QuestService:Progress("Apple") end
        game:GetService("SaveService"):SaveSlot(1)
        QuestService:Progress("Apple", 2)
        print(QuestService:GetState("apples"))
        "#,
    );
    frame(&mut runtime, &mut world, &[]);
    assert_eq!(
        output(&mut runtime),
        [
            "true false",
            "Active",
            "progressed apples pick 1",
            "progressed apples pick 2",
            "progressed apples pick 3",
            "progressed apples pick 5",
        ]
    );
    // Five apples, but the Farmer still waits to hear.
    let objectives = quest_tracker(&runtime, &world)[0].objectives.clone();
    assert_eq!((objectives[0].progress, objectives[1].progress), (5, 0));

    // Talking to him is the last objective.
    press(&mut runtime, &mut world, KeyCode::E);
    assert_eq!(
        quest_state(&runtime, "apples"),
        quest::QuestState::Completed
    );
    assert!(
        quest_tracker(&runtime, &world).is_empty(),
        "done quests leave the tracker"
    );
    assert_eq!(
        output(&mut runtime),
        ["progressed apples return 1", "completed apples"]
    );
    // Talking counted as the dialogue started, so it thanks at once.
    assert_eq!(dialogue_view(&runtime, &world).unwrap().text, "Thank you!");
    press(&mut runtime, &mut world, KeyCode::E);
    assert!(!dialogue_active(&runtime));

    // The slot saved at three apples puts them back.
    start(
        &mut world,
        &mut runtime,
        r#"
        local QuestService = game:GetService("QuestService")
        game:GetService("SaveService"):LoadSlot(1)
        local pick = QuestService:GetObjectives("apples")[1]
        print(QuestService:GetState("apples"), pick.Text, pick.Progress, pick.Count, pick.Done)
        QuestService:Progress("Apple", 2)
        print(QuestService:GetObjectives("apples")[1].Done)
        "#,
    );
    frame(&mut runtime, &mut world, &[]);
    assert_eq!(
        output(&mut runtime),
        [
            "Active Apples 3 5 false",
            "true",
            "progressed apples pick 5"
        ]
    );
    let slot = std::fs::read_to_string(folder.join("saves/slot-1.json")).unwrap();
    assert!(slot.contains("Quest/apples/pick"), "{slot}");
}

#[test]
fn switching_language_changes_the_dialogue_text_at_once() {
    let folder = project("language");
    let (mut world, mut runtime, _) = farm(&folder);
    frame(&mut runtime, &mut world, &[]);
    press(&mut runtime, &mut world, KeyCode::E);
    let view = dialogue_view(&runtime, &world).unwrap();
    assert_eq!(
        (view.speaker.as_str(), view.text.as_str()),
        ("Farmer", "Could you pick me some apples?")
    );
    start(
        &mut world,
        &mut runtime,
        r#"
        game:GetService("LocalizationService").Language = "Español"
        local line = game:GetService("DialogueService"):GetLine()
        print(line.Speaker, line.Text, line.Choices[1], line.Selected)
        "#,
    );
    let view = dialogue_view(&runtime, &world).unwrap();
    assert_eq!(view.speaker, "Granjero");
    assert_eq!(view.text, "¿Me recoges unas manzanas?");
    assert_eq!(view.choices, ["Sí", "No"]);
    assert_eq!(
        output(&mut runtime),
        ["Granjero ¿Me recoges unas manzanas? Sí 1"]
    );
    world.set_language("English");
    assert_eq!(dialogue_view(&runtime, &world).unwrap().choices[0], "Yes");
}

#[test]
fn a_script_waits_in_start_for_the_ending_and_can_choose() {
    let folder = project("script");
    let (mut world, mut runtime, _) = farm(&folder);
    start(
        &mut world,
        &mut runtime,
        r#"
        local DialogueService = game:GetService("DialogueService")
        task.spawn(function()
            print("ended", DialogueService:Start("assets/dialogue/Farmer.dialogue.json", workspace.Farmer))
        end)
        print("active", DialogueService.Active)
        DialogueService:Choose(2)
        print("after", DialogueService.Active)
        print(pcall(DialogueService.Choose, DialogueService, 1))
        "#,
    );
    frame(&mut runtime, &mut world, &[]);
    let lines = output(&mut runtime);
    assert_eq!(lines[..2], ["active true", "after false"]);
    assert!(lines[2].starts_with("false"), "{lines:?}");
    assert!(lines[2].contains("no dialogue is running"), "{lines:?}");
    assert_eq!(lines[3], "ended declined");
    assert_eq!(
        quest_state(&runtime, "apples"),
        quest::QuestState::NotStarted
    );
}

#[test]
fn a_dialogue_with_problems_says_each_with_its_node() {
    let mut file = farmer_dialogue();
    // A second Start, an option with nothing wired, and a Line with no text.
    let nodes = file["nodes"].as_array_mut().unwrap();
    nodes.push(json!({ "id": 20, "kind": "start", "values": {} }));
    nodes.push(json!({ "id": 21, "kind": "choice", "values": { "Option 1": "Hm?" } }));
    nodes.push(json!({ "id": 22, "kind": "line", "values": {} }));
    nodes.push(json!({ "id": 23, "kind": "dance", "values": {} }));
    let errors = Dialogue::from_json(&file.to_string()).unwrap_err();
    let messages: Vec<String> = errors.iter().map(ToString::to_string).collect();
    assert!(
        messages.contains(&"node 20: a dialogue has one Start".to_owned()),
        "{messages:?}"
    );
    assert!(
        messages.contains(&"node 21: Option 1 isn't wired to what comes next".to_owned()),
        "{messages:?}"
    );
    assert!(
        messages.contains(&"node 22: fill in Text".to_owned()),
        "{messages:?}"
    );
    assert!(
        messages.contains(&"node 23: there's no kind of node called \"dance\"".to_owned()),
        "{messages:?}"
    );
    assert!(Dialogue::from_json(&farmer_dialogue().to_string()).is_ok());
}

#[test]
fn a_loop_with_nothing_to_read_stops_with_an_error() {
    let folder = project("loop");
    let file = json!({
        "nodes": [
            { "id": 1, "kind": "start", "values": {} },
            { "id": 2, "kind": "set_attribute", "values": { "Attribute": "A", "Value": "1" } },
            { "id": 3, "kind": "set_attribute", "values": { "Attribute": "B", "Value": "1" } },
        ],
        "wires": [
            { "from": { "node": 1, "pin": "" }, "to": { "node": 2, "pin": "" } },
            { "from": { "node": 2, "pin": "" }, "to": { "node": 3, "pin": "" } },
            { "from": { "node": 3, "pin": "" }, "to": { "node": 2, "pin": "" } },
        ],
    });
    std::fs::write(
        folder.join("assets/dialogue/Loop.dialogue.json"),
        file.to_string(),
    )
    .unwrap();
    let (mut world, mut runtime, _) = farm(&folder);
    start(
        &mut world,
        &mut runtime,
        r#"print("ended", game:GetService("DialogueService"):Start("assets/dialogue/Loop.dialogue.json"))"#,
    );
    frame(&mut runtime, &mut world, &[]);
    let lines = output(&mut runtime);
    assert!(
        lines
            .iter()
            .any(|line| line.contains("ran 1000 nodes without a Line or Choice")),
        "{lines:?}"
    );
    assert!(lines.contains(&"ended ".to_owned()), "{lines:?}");
    assert!(!dialogue_active(&runtime));
}
