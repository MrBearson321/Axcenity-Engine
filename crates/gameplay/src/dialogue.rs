//! `game:GetService("DialogueService")` (Phase 77): dialogues, read from
//! `.dialogue.json` files and run one at a time for the local player.
//!
//! A dialogue is a node graph (made in the editor's Dialogue tab; its nodes
//! are `blocks::dialogue`). From its Start, white exec wires lead through:
//!
//! - **Line**: shows a speaker's text (string table keys are shown in the
//!   player's language, worked out each frame, so switching language
//!   changes the box at once) and waits for the player to go on. Audio
//!   plays from the speaker; Animation plays that Animator state. A Line
//!   wired straight to a Choice shows the choices under it.
//! - **Choice**: waits for the player to pick one of its options.
//! - **Conditions** (`Attribute?`, `Has Tag?`, `Data Table?`, `Quest?`,
//!   `Lua Condition`) go on from True or False.
//! - **Actions** (`Set Attribute`, `Give Item`, `Start Quest`, `Complete
//!   Quest`, `Run Lua`) do something and go on.
//! - **End**: finishes with its Ending, which `DialogueService:Start`
//!   returns. An unwired pin finishes with `""`.
//!
//! Wires may loop back; a run of more than [`MAX_STEPS`] nodes without one
//! the player reads stops the dialogue with an error. A ProximityPrompt
//! with a `Dialogue` starts it when triggered, with the prompt's Model as
//! the speaker. The player goes on with Interact or UIConfirm (E, Enter, or
//! a controller's X or A), moves between choices with UINavigate (arrows,
//! D-pad) and picks one with the same keys or 1 to 4. The character stands
//! still while a dialogue runs ([`Dialogues::active`]).

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::rc::Rc;
use std::sync::Arc;

use axcenity_assets::{Assets, Cell};
use axcenity_core::dialogue::DialogueView;
use axcenity_core::input::{InputMap, InputState, UI_CONFIRM, UI_NAVIGATE};
use axcenity_core::{ClassName, InstanceId, KeyCode, Value, World};
use mlua::{
    Function, IntoLua, Lua, MetaMethod, Table, Thread, UserData, UserDataFields, UserDataMethods,
};

use axcenity_script::system;
use axcenity_script::{LuaInstance, Side, lent_world as world, lent_world_mut as world_mut};

use crate::quest::{self, QuestState, translated, with_quests};

/// A dialogue's file ending.
pub const DIALOGUE_SUFFIX: &str = ".dialogue.json";
/// The newest dialogue file this engine reads.
const FORMAT_VERSION: u32 = 1;
/// The most nodes run in a row without a Line or Choice to wait at.
pub const MAX_STEPS: usize = 1000;

/// A node kind: its name, its output pins and its fields.
pub type NodeKind = (
    &'static str,
    &'static [&'static str],
    &'static [&'static str],
);

const BRANCHES: &[&str] = &["True", "False"];

/// Every node kind, as `blocks::dialogue` must offer them.
pub const DIALOGUE_NODE_KINDS: &[NodeKind] = &[
    ("start", &[""], &[]),
    ("line", &[""], &["Speaker", "Text", "Audio", "Animation"]),
    (
        "choice",
        &["1", "2", "3", "4"],
        &["Option 1", "Option 2", "Option 3", "Option 4"],
    ),
    ("attribute_check", BRANCHES, &["Of", "Attribute", "Equals"]),
    ("tag_check", BRANCHES, &["Of", "Tag"]),
    (
        "data_check",
        BRANCHES,
        &["Table", "Key", "Column", "Equals"],
    ),
    ("quest_check", BRANCHES, &["Quest", "State"]),
    ("lua_check", BRANCHES, &["Code"]),
    ("set_attribute", &[""], &["On", "Attribute", "Value"]),
    ("give_item", &[""], &["Item"]),
    ("start_quest", &[""], &["Quest"]),
    ("complete_quest", &[""], &["Quest"]),
    ("run_lua", &[""], &["Code"]),
    ("end", &[], &["Ending"]),
];

/// A problem with a dialogue, on one of its nodes (by id) or the whole.
#[derive(Clone, Debug, PartialEq)]
pub struct DialogueError {
    pub node: Option<u32>,
    pub message: String,
}

impl DialogueError {
    fn on(node: u32, message: impl Into<String>) -> Self {
        Self {
            node: Some(node),
            message: message.into(),
        }
    }

    fn whole(message: impl Into<String>) -> Self {
        Self {
            node: None,
            message: message.into(),
        }
    }
}

impl fmt::Display for DialogueError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.node {
            Some(node) => write!(f, "node {node}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

/// The file, as far as running it goes: positions and comments are the
/// editor's, and ignored here.
struct DialogueFile {
    format_version: u32,
    nodes: Vec<FileNode>,
    wires: Vec<FileWire>,
}

struct FileNode {
    id: u32,
    kind: String,
    values: BTreeMap<String, String>,
}

struct FileEnd {
    node: u32,
    pin: String,
}

struct FileWire {
    from: FileEnd,
    to: FileEnd,
}

fn parse_file(text: &str) -> Result<DialogueFile, String> {
    use serde_json::Value as Json;
    let json: Json = serde_json::from_str(text).map_err(|err| err.to_string())?;
    let object = json.as_object().ok_or("it isn't a JSON object")?;
    let id = |value: Option<&Json>| -> Result<u32, String> {
        value
            .and_then(Json::as_u64)
            .and_then(|id| u32::try_from(id).ok())
            .ok_or_else(|| "a node or wire is missing its node id".to_owned())
    };
    let format_version = match object.get("format_version") {
        None => FORMAT_VERSION,
        Some(version) => id(Some(version))?,
    };
    let mut nodes = Vec::new();
    for node in object
        .get("nodes")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
    {
        let values = node
            .get("values")
            .and_then(Json::as_object)
            .map(|values| {
                values
                    .iter()
                    .filter_map(|(name, value)| Some((name.clone(), value.as_str()?.to_owned())))
                    .collect()
            })
            .unwrap_or_default();
        nodes.push(FileNode {
            id: id(node.get("id"))?,
            kind: node
                .get("kind")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_owned(),
            values,
        });
    }
    let end = |value: Option<&Json>| -> Result<FileEnd, String> {
        let value = value.ok_or("a wire is missing an end")?;
        Ok(FileEnd {
            node: id(value.get("node"))?,
            pin: value
                .get("pin")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_owned(),
        })
    };
    let mut wires = Vec::new();
    for wire in object
        .get("wires")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
    {
        wires.push(FileWire {
            from: end(wire.get("from"))?,
            to: end(wire.get("to"))?,
        });
    }
    Ok(DialogueFile {
        format_version,
        nodes,
        wires,
    })
}

/// One checked node: its kind, its fields and where each output goes.
#[derive(Clone, Debug)]
struct Node {
    kind: &'static str,
    values: BTreeMap<String, String>,
    next: HashMap<String, u32>,
}

impl Node {
    fn value(&self, name: &str) -> &str {
        self.values.get(name).map_or("", |value| value.trim())
    }

    /// The options a Choice shows: each one's text and pin.
    fn options(&self) -> Vec<(String, String)> {
        (1..=4)
            .filter_map(|n| {
                let text = self.value(&format!("Option {n}"));
                (!text.is_empty()).then(|| (text.to_owned(), n.to_string()))
            })
            .collect()
    }
}

/// A checked dialogue, ready to run.
#[derive(Clone, Debug)]
pub struct Dialogue {
    nodes: HashMap<u32, Node>,
    start: u32,
}

impl Dialogue {
    /// Reads and checks a dialogue file's text. Every problem comes back,
    /// each with its node where it has one.
    pub fn from_json(text: &str) -> Result<Self, Vec<DialogueError>> {
        let file = parse_file(text)
            .map_err(|err| vec![DialogueError::whole(format!("it isn't a dialogue: {err}"))])?;
        if file.format_version > FORMAT_VERSION {
            return Err(vec![DialogueError::whole(
                "it was made by a newer version of Axcenity",
            )]);
        }
        let mut errors = Vec::new();
        let mut nodes = HashMap::new();
        let mut starts = Vec::new();
        for node in &file.nodes {
            let Some(&(kind, _, _)) = DIALOGUE_NODE_KINDS
                .iter()
                .find(|(kind, _, _)| *kind == node.kind)
            else {
                errors.push(DialogueError::on(
                    node.id,
                    format!("there's no kind of node called \"{}\"", node.kind),
                ));
                continue;
            };
            if kind == "start" {
                starts.push(node.id);
            }
            nodes.insert(
                node.id,
                Node {
                    kind,
                    values: node.values.clone(),
                    next: HashMap::new(),
                },
            );
        }
        for wire in &file.wires {
            let Some(to) = nodes.get(&wire.to.node) else {
                errors.push(DialogueError::whole(format!(
                    "a wire goes to node {}, which isn't there",
                    wire.to.node
                )));
                continue;
            };
            if to.kind == "start" || !wire.to.pin.is_empty() {
                errors.push(DialogueError::on(
                    wire.to.node,
                    "a wire goes into a pin this node doesn't have",
                ));
                continue;
            }
            let Some(from) = nodes.get_mut(&wire.from.node) else {
                errors.push(DialogueError::whole(format!(
                    "a wire comes from node {}, which isn't there",
                    wire.from.node
                )));
                continue;
            };
            let outputs = DIALOGUE_NODE_KINDS
                .iter()
                .find(|(kind, _, _)| *kind == from.kind)
                .map_or(&[][..], |(_, outputs, _)| *outputs);
            if !outputs.contains(&wire.from.pin.as_str()) {
                errors.push(DialogueError::on(
                    wire.from.node,
                    format!(
                        "a wire comes out of a pin it doesn't have (\"{}\")",
                        wire.from.pin
                    ),
                ));
            } else if from
                .next
                .insert(wire.from.pin.clone(), wire.to.node)
                .is_some()
            {
                errors.push(DialogueError::on(
                    wire.from.node,
                    "two wires come out of one pin; a pin leads to one node",
                ));
            }
        }
        match starts.as_slice() {
            [] => errors.push(DialogueError::whole("it has no Start node")),
            [_] => {}
            [_, rest @ ..] => {
                for &extra in rest {
                    errors.push(DialogueError::on(extra, "a dialogue has one Start"));
                }
            }
        }
        if let Some(&start) = starts.first()
            && nodes.get(&start).is_some_and(|node| node.next.is_empty())
        {
            errors.push(DialogueError::on(start, "wire the Start to the first node"));
        }
        let mut ids: Vec<&u32> = nodes.keys().collect();
        ids.sort();
        for &id in ids {
            let node = &nodes[&id];
            let required: &[&str] = match node.kind {
                "line" => &["Text"],
                "attribute_check" | "set_attribute" => &["Attribute"],
                "tag_check" => &["Tag"],
                "data_check" => &["Table", "Key", "Column"],
                "quest_check" | "start_quest" | "complete_quest" => &["Quest"],
                "lua_check" | "run_lua" => &["Code"],
                "give_item" => &["Item"],
                _ => &[],
            };
            for field in required {
                if node.value(field).is_empty() {
                    errors.push(DialogueError::on(id, format!("fill in {field}")));
                }
            }
            if node.kind == "choice" {
                let options = node.options();
                if options.is_empty() {
                    errors.push(DialogueError::on(id, "a Choice needs an option"));
                }
                for (_, pin) in options {
                    if !node.next.contains_key(&pin) {
                        errors.push(DialogueError::on(
                            id,
                            format!("Option {pin} isn't wired to what comes next"),
                        ));
                    }
                }
            }
        }
        if errors.is_empty() {
            Ok(Self {
                nodes,
                start: starts[0],
            })
        } else {
            Err(errors)
        }
    }

    /// Reads the dialogue at `path` in the project. The error says why it
    /// can't run.
    pub fn load(assets: &Assets, path: &str) -> Result<Self, String> {
        if !path.to_lowercase().ends_with(DIALOGUE_SUFFIX) {
            return Err(format!(
                "\"{path}\" isn't a dialogue (a {DIALOGUE_SUFFIX} file, made in the Dialogue tab)"
            ));
        }
        let bytes = assets.read_file(path)?;
        let text = String::from_utf8(bytes).map_err(|_| "it isn't text".to_owned())?;
        Self::from_json(&text).map_err(|errors| {
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ")
        })
    }

    fn node(&self, id: u32) -> Option<&Node> {
        self.nodes.get(&id)
    }
}

/// The dialogue running now.
struct Running {
    path: String,
    dialogue: Arc<Dialogue>,
    speaker: Option<InstanceId>,
    player: Option<InstanceId>,
    /// The last Line shown.
    line: Option<u32>,
    /// The Choice being picked from, if one is.
    choice: Option<u32>,
    /// The option the keyboard or controller is on.
    selected: usize,
    /// Scripts waiting in `Start`.
    waiting: Vec<(Thread, Option<InstanceId>, Side)>,
}

/// A finished dialogue's waiting scripts and its Ending.
pub(crate) struct Finished {
    pub(crate) waiting: Vec<(Thread, Option<InstanceId>, Side)>,
    pub(crate) ending: String,
}

/// The dialogue running, the keys held, and dialogues read.
#[derive(Default)]
pub struct Dialogues {
    running: Option<Running>,
    /// True while a node runs Lua, so a second dialogue can't start then.
    busy: bool,
    /// Which of the dialogue's keys were down last frame.
    held: HashSet<&'static str>,
    pub(crate) finished: Vec<Finished>,
}

impl Dialogues {
    /// Whether a dialogue is running: the character stands still and the
    /// box shows.
    pub fn active(&self) -> bool {
        self.running.is_some() || self.busy
    }
}

#[derive(Clone, Default)]
pub(crate) struct SharedDialogues(pub Rc<RefCell<Dialogues>>);

fn shared(lua: &Lua) -> mlua::Result<SharedDialogues> {
    lua.app_data_ref::<SharedDialogues>()
        .map(|shared| shared.clone())
        .ok_or_else(|| mlua::Error::runtime("dialogues aren't set up on this side"))
}

fn report(lua: &Lua, path: &str, message: impl fmt::Display) {
    system::report(lua, format!("{path}: {message}"));
}

/// The local player: the Player whose Character is the player character,
/// else the first Player.
pub(crate) fn player_of(lua: &Lua) -> Option<InstanceId> {
    let world = world(lua).ok()?;
    let players = world.get(world.players())?.children().to_vec();
    let character = axcenity_character::player_character(&world);
    let is_player = |id: &InstanceId| {
        world
            .get(*id)
            .is_some_and(|p| p.class() == ClassName::Player)
    };
    players
        .iter()
        .copied()
        .filter(is_player)
        .find(|&id| {
            character.is_some()
                && world
                    .get(id)
                    .and_then(|p| p.property("Character"))
                    .and_then(Value::as_instance)
                    == character
        })
        .or_else(|| players.iter().copied().find(is_player))
}

/// Who a prompt's dialogue is said by, by name: its Model (or part), else
/// the prompt itself.
pub fn speaker_name(world: &World, prompt: InstanceId) -> String {
    speaker_of(world, prompt)
        .or(Some(prompt))
        .and_then(|id| world.get(id))
        .map(|instance| instance.name().to_owned())
        .unwrap_or_default()
}

/// The nearest Model at or above `id`, else the part itself.
pub(crate) fn speaker_of(world: &World, id: InstanceId) -> Option<InstanceId> {
    let mut at = world.get(id)?.parent();
    let mut part = None;
    while let Some(here) = at {
        let instance = world.get(here)?;
        if here == world.workspace() || here == world.root() {
            break;
        }
        if instance.class() == ClassName::Model {
            return Some(here);
        }
        part.get_or_insert(here);
        at = instance.parent();
    }
    part
}

/// Starts the dialogue at `path`, said by `speaker`, for `player`. The
/// world is lent to `lua`. `Ok(Some(ending))` when it ends before waiting
/// for the player.
pub(crate) fn start(
    lua: &Lua,
    path: &str,
    speaker: Option<InstanceId>,
    player: Option<InstanceId>,
    thread: Option<(Thread, Option<InstanceId>, Side)>,
) -> mlua::Result<Option<String>> {
    let shared = shared(lua)?;
    if shared.0.borrow().active() {
        return Err(mlua::Error::runtime(format!(
            "can't start the dialogue \"{path}\": another dialogue is running"
        )));
    }
    let assets = lua
        .app_data_ref::<Assets>()
        .map(|assets| assets.clone())
        .ok_or_else(|| mlua::Error::runtime("dialogues need a project, and none is open"))?;
    let dialogue = Dialogue::load(&assets, path)
        .map_err(|why| mlua::Error::runtime(format!("the dialogue \"{path}\" can't run: {why}")))?;
    // Talking to someone is a Talk objective's progress.
    let name =
        speaker.and_then(|speaker| world(lua).ok()?.get(speaker).map(|s| s.name().to_owned()));
    if let Some(name) = name {
        with_quests(lua, |quests| {
            quests.talked(&name);
            Ok(())
        })?;
    }
    let first = dialogue
        .node(dialogue.start)
        .and_then(|node| node.next.get("").copied());
    let running = Running {
        path: path.to_owned(),
        dialogue: Arc::new(dialogue),
        speaker,
        player,
        line: None,
        choice: None,
        selected: 0,
        waiting: thread.into_iter().collect(),
    };
    Ok(run(lua, &shared, running, first))
}

/// Runs from `node` until the dialogue waits for the player (it's put
/// back as the running one) or ends (`Some(ending)`, its waiting scripts
/// queued to go on).
fn run(
    lua: &Lua,
    shared: &SharedDialogues,
    mut running: Running,
    mut node: Option<u32>,
) -> Option<String> {
    shared.0.borrow_mut().busy = true;
    let mut ending = None;
    let mut steps = 0;
    loop {
        steps += 1;
        if steps > MAX_STEPS {
            report(
                lua,
                &running.path,
                format!(
                    "ran {MAX_STEPS} nodes without a Line or Choice to wait at, so it stopped; \
                     a loop needs a Line or Choice in it"
                ),
            );
            ending = Some(String::new());
            break;
        }
        let Some(id) = node else {
            ending = Some(String::new());
            break;
        };
        let Some(current) = running.dialogue.node(id).cloned() else {
            ending = Some(String::new());
            break;
        };
        let next = |pin: &str| current.next.get(pin).copied();
        match current.kind {
            "start" => node = next(""),
            "line" => {
                running.line = Some(id);
                running.choice = None;
                running.selected = 0;
                let after =
                    next("").and_then(|n| running.dialogue.node(n).map(|node| (n, node.kind)));
                if let Some((choice, "choice")) = after {
                    running.choice = Some(choice);
                }
                let audio = current.value("Audio").to_owned();
                let animation = current.value("Animation").to_owned();
                if let Err(err) = helper(lua, "line", (to_lua(running.speaker), audio, animation)) {
                    report(lua, &running.path, err);
                }
                break;
            }
            "choice" => {
                running.choice = Some(id);
                running.selected = 0;
                break;
            }
            "attribute_check" | "tag_check" | "data_check" | "quest_check" | "lua_check" => {
                match check(lua, &running, &current) {
                    Ok(true) => node = next("True"),
                    Ok(false) => node = next("False"),
                    Err(why) => {
                        report(lua, &running.path, format!("node {id}: {why}"));
                        ending = Some(String::new());
                        break;
                    }
                }
            }
            "end" => {
                ending = Some(current.value("Ending").to_owned());
                break;
            }
            _ => {
                if let Err(why) = act(lua, &running, &current) {
                    report(lua, &running.path, format!("node {id}: {why}"));
                }
                node = next("");
            }
        }
    }
    let mut dialogues = shared.0.borrow_mut();
    dialogues.busy = false;
    match ending {
        Some(ending) => {
            dialogues.finished.push(Finished {
                waiting: std::mem::take(&mut running.waiting),
                ending: ending.clone(),
            });
            drop(dialogues);
            // The last line's voice stops with it.
            let _ = helper(
                lua,
                "line",
                (to_lua(running.speaker), String::new(), String::new()),
            );
            Some(ending)
        }
        None => {
            dialogues.running = Some(running);
            None
        }
    }
}

fn to_lua(id: Option<InstanceId>) -> Option<LuaInstance> {
    id.map(LuaInstance)
}

/// The Instance whose attribute or tag a node reads or sets: the player's
/// character (else the Player), or the speaker.
fn subject(world: &World, running: &Running, who: &str) -> Option<InstanceId> {
    if who == "Speaker" {
        return running.speaker;
    }
    axcenity_character::player_character(world).or(running.player)
}

/// Whether `value` is what `equals` says: empty is "there and not false
/// or 0".
fn matches(value: Option<&Value>, equals: &str) -> bool {
    let Some(value) = value else {
        return false;
    };
    if equals.is_empty() {
        return !matches!(value, Value::Bool(false))
            && value.as_number().is_none_or(|n| n != 0.0)
            && value.as_str().is_none_or(|s| !s.is_empty());
    }
    match value {
        Value::Bool(b) => equals.eq_ignore_ascii_case(if *b { "true" } else { "false" }),
        Value::Number(n) => equals.parse::<f64>().is_ok_and(|e| (e - n).abs() < 1e-9),
        Value::String(s) => s == equals,
        other => format!("{other:?}") == equals,
    }
}

fn cell_value(cell: Cell) -> Value {
    match cell {
        Cell::Number(n) => Value::Number(n),
        Cell::Integer(n) => Value::Number(n as f64),
        Cell::Text(text) => Value::String(text),
        Cell::Bool(b) => Value::Bool(b),
        Cell::Vector3(v) | Cell::Color3(v) => {
            Value::String(format!("{}, {}, {}", v[0], v[1], v[2]))
        }
    }
}

fn check(lua: &Lua, running: &Running, node: &Node) -> Result<bool, String> {
    match node.kind {
        "attribute_check" => {
            let world = world(lua).map_err(|err| err.to_string())?;
            let id = subject(&world, running, node.value("Of"));
            let value = id
                .and_then(|id| world.get(id))
                .and_then(|instance| instance.attribute(node.value("Attribute")));
            Ok(matches(value, node.value("Equals")))
        }
        "tag_check" => {
            let world = world(lua).map_err(|err| err.to_string())?;
            let id = subject(&world, running, node.value("Of"));
            Ok(id
                .and_then(|id| world.get(id))
                .is_some_and(|instance| instance.has_tag(node.value("Tag"))))
        }
        "data_check" => {
            let assets = lua
                .app_data_ref::<Assets>()
                .map(|assets| assets.clone())
                .ok_or("data tables need a project")?;
            let name = node.value("Table");
            let table = assets
                .data_table(name)
                .map_err(|why| format!("can't read the data table \"{name}\": {why}"))?;
            let value = table.row(node.value("Key")).and_then(|row| {
                let column = table.column(node.value("Column"))?;
                Some(cell_value(row.cell(column)))
            });
            Ok(matches(value.as_ref(), node.value("Equals")))
        }
        "quest_check" => {
            let key = node.value("Quest").to_owned();
            let state = with_quests(lua, |quests| {
                quest::quest(quests.assets, &key)?;
                Ok(quest::state(quests.saves, &key))
            })
            .map_err(|err| err.to_string())?;
            let wanted = match node.value("State") {
                "NotStarted" => QuestState::NotStarted,
                "Completed" => QuestState::Completed,
                _ => QuestState::Active,
            };
            Ok(state == wanted)
        }
        "lua_check" => {
            let value: mlua::Value =
                run_code(lua, running, &format!("return ({})", node.value("Code")))?;
            Ok(!matches!(
                value,
                mlua::Value::Nil | mlua::Value::Boolean(false)
            ))
        }
        _ => Ok(false),
    }
}

fn act(lua: &Lua, running: &Running, node: &Node) -> Result<(), String> {
    match node.kind {
        "set_attribute" => {
            let mut world = world_mut(lua).map_err(|err| err.to_string())?;
            let id =
                subject(&world, running, node.value("On")).ok_or("there's no one to set it on")?;
            let text = node.value("Value");
            let value = if let Ok(n) = text.parse::<f64>() {
                Value::Number(n)
            } else if text.eq_ignore_ascii_case("true") {
                Value::Bool(true)
            } else if text.eq_ignore_ascii_case("false") {
                Value::Bool(false)
            } else {
                Value::String(text.to_owned())
            };
            world
                .set_attribute(id, node.value("Attribute"), Some(value))
                .map_err(|err| err.to_string())
        }
        "give_item" => {
            let character = {
                let world = world(lua).map_err(|err| err.to_string())?;
                axcenity_character::player_character(&world)
            };
            helper(
                lua,
                "give",
                (to_lua(character), node.value("Item").to_owned()),
            )
        }
        "start_quest" => {
            let key = node.value("Quest").to_owned();
            with_quests(lua, |quests| quests.start(&key).map(|_| ())).map_err(|err| err.to_string())
        }
        "complete_quest" => {
            let key = node.value("Quest").to_owned();
            with_quests(lua, |quests| quests.complete(&key).map(|_| ()))
                .map_err(|err| err.to_string())
        }
        "run_lua" => run_code::<mlua::Value>(lua, running, node.value("Code")).map(|_| ()),
        _ => Ok(()),
    }
}

/// Runs a node's Lua in a script's environment with `player`, `character`
/// and `speaker`. It can't wait.
fn run_code<R: mlua::FromLuaMulti>(lua: &Lua, running: &Running, code: &str) -> Result<R, String> {
    let character = {
        let world = world(lua).map_err(|err| err.to_string())?;
        axcenity_character::player_character(&world)
    };
    let env = system::engine_environment(lua).map_err(|err| err.to_string())?;
    let set = |name: &str, id: Option<InstanceId>| env.raw_set(name, to_lua(id));
    set("script", None)
        .and_then(|()| set("player", running.player))
        .and_then(|()| set("character", character))
        .and_then(|()| set("speaker", running.speaker))
        .map_err(|err| err.to_string())?;
    let function = lua
        .load(code)
        .set_name(format!("={}", running.path))
        .set_mode(mlua::chunk::ChunkMode::Text)
        .set_environment(env)
        .into_function()
        .map_err(|err| system::error_message(&err))?;
    function.call(()).map_err(|err| system::error_message(&err))
}

/// What dialogues do through the scripting API: give items, play voices
/// and animation.
const HELPERS: &str = r#"
local helpers = {}
local voice

local function find(root, name)
    for _, item in root:GetDescendants() do
        if item.Name == name and item:IsA("Tool") then
            return item
        end
    end
    return nil
end

function helpers.give(character, name)
    local item = find(game:GetService("ServerStorage"), name)
        or find(game:GetService("ReplicatedStorage"), name)
    if not item then
        error("there's no Tool called " .. name .. " in ServerStorage or ReplicatedStorage", 0)
    end
    local inventory = character and character:FindFirstChildWhichIsA("Inventory")
    if not inventory then
        error("the player character has no Inventory to give " .. name .. " to", 0)
    end
    if not inventory:Add(item:Clone()) then
        error("the player's Inventory is full, so " .. name .. " wasn't given", 0)
    end
end

function helpers.line(speaker, audio, animation)
    if voice then
        voice:Destroy()
        voice = nil
    end
    if audio ~= "" then
        local holder = workspace
        if speaker then
            holder = speaker:IsA("BasePart") and speaker
                or speaker:FindFirstChildWhichIsA("BasePart", true)
                or workspace
        end
        voice = Instance.new("Sound")
        voice.Name = "DialogueVoice"
        voice.SoundId = audio
        voice.Parent = holder
        voice:Play()
    end
    if animation ~= "" and speaker then
        local animator = speaker:FindFirstChildWhichIsA("Animator", true)
        if animator then
            animator:Play(animation)
        end
    end
end

return helpers
"#;

struct Helpers(Table);

fn helper(lua: &Lua, name: &str, args: impl mlua::IntoLuaMulti) -> Result<(), String> {
    let table = match lua.app_data_ref::<Helpers>() {
        Some(helpers) => helpers.0.clone(),
        None => {
            let env = system::engine_environment(lua).map_err(|err| err.to_string())?;
            let table: Table = lua
                .load(HELPERS)
                .set_name("=[engine]")
                .set_environment(env)
                .call(())
                .map_err(|err| err.to_string())?;
            lua.set_app_data(Helpers(table.clone()));
            table
        }
    };
    let function: Function = table.raw_get(name).map_err(|err| err.to_string())?;
    function
        .call::<()>(args)
        .map_err(|err| system::error_message(&err))
}

/// Goes on from the line shown, or picks the selected choice.
pub(crate) fn advance(lua: &Lua) -> mlua::Result<()> {
    let shared = shared(lua)?;
    let Some(running) = shared.0.borrow_mut().running.take() else {
        return Ok(());
    };
    if running.choice.is_some() {
        let selected = running.selected;
        return pick(lua, &shared, running, selected);
    }
    let next = running
        .line
        .and_then(|line| running.dialogue.node(line))
        .and_then(|node| node.next.get("").copied());
    run(lua, &shared, running, next);
    Ok(())
}

/// Picks the choice shown `index` (from 0).
pub(crate) fn choose(lua: &Lua, index: usize) -> mlua::Result<()> {
    let shared = shared(lua)?;
    let Some(running) = shared.0.borrow_mut().running.take() else {
        return Err(mlua::Error::runtime("no dialogue is running"));
    };
    pick(lua, &shared, running, index)
}

fn pick(lua: &Lua, shared: &SharedDialogues, running: Running, index: usize) -> mlua::Result<()> {
    let options = running
        .choice
        .and_then(|choice| running.dialogue.node(choice))
        .map(Node::options)
        .unwrap_or_default();
    let Some((_, pin)) = options.get(index) else {
        let count = options.len();
        shared.0.borrow_mut().running = Some(running);
        return Err(mlua::Error::runtime(if count == 0 {
            "the dialogue isn't showing choices now".to_owned()
        } else {
            format!("the dialogue shows {count} choices; choose 1 to {count}")
        }));
    };
    let next = running
        .choice
        .and_then(|choice| running.dialogue.node(choice))
        .and_then(|node| node.next.get(pin).copied());
    run(lua, shared, running, next);
    Ok(())
}

/// Ends the running dialogue at once, with the ending `""`.
pub(crate) fn stop(lua: &Lua) -> mlua::Result<()> {
    let shared = shared(lua)?;
    let Some(running) = shared.0.borrow_mut().running.take() else {
        return Ok(());
    };
    run(lua, &shared, running, None);
    Ok(())
}

/// What the dialogue box shows now, in the player's language.
pub fn view(dialogues: &Dialogues, world: &World) -> Option<DialogueView> {
    let running = dialogues.running.as_ref()?;
    let line = running.line.and_then(|line| running.dialogue.node(line));
    let speaker = match line.map(|line| line.value("Speaker")) {
        Some(name) if !name.is_empty() => translated(world, name),
        _ => running
            .speaker
            .and_then(|speaker| world.get(speaker))
            .map(|speaker| speaker.name().to_owned())
            .unwrap_or_default(),
    };
    let choices: Vec<String> = running
        .choice
        .and_then(|choice| running.dialogue.node(choice))
        .map(Node::options)
        .unwrap_or_default()
        .into_iter()
        .map(|(text, _)| translated(world, &text))
        .collect();
    Some(DialogueView {
        speaker,
        text: line
            .map(|line| translated(world, line.value("Text")))
            .unwrap_or_default(),
        selected: running.selected.min(choices.len().saturating_sub(1)),
        choices,
    })
}

/// The player's presses this frame: go on, move between choices, or pick
/// one by number.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Presses {
    pub(crate) confirm: bool,
    pub(crate) step: i32,
    pub(crate) number: Option<usize>,
}

impl Dialogues {
    /// Reads this frame's presses (each once per press).
    pub(crate) fn presses(&mut self, map: &InputMap, state: &InputState) -> Presses {
        let mut down = HashSet::new();
        if map.is_pressed("Interact", state) || map.is_pressed(UI_CONFIRM, state) {
            down.insert("confirm");
        }
        let axis = map.axis(UI_NAVIGATE, state);
        if axis[1] > 0.5 {
            down.insert("up");
        } else if axis[1] < -0.5 {
            down.insert("down");
        }
        let numbers = [
            ("1", KeyCode::One),
            ("2", KeyCode::Two),
            ("3", KeyCode::Three),
            ("4", KeyCode::Four),
        ];
        for (name, key) in numbers {
            if state.is_key_down(key) {
                down.insert(name);
            }
        }
        let new = |name: &str| down.contains(name) && !self.held.contains(name);
        let presses = Presses {
            confirm: new("confirm"),
            step: i32::from(new("down")) - i32::from(new("up")),
            number: numbers.iter().position(|(name, _)| new(name)),
        };
        self.held = down;
        presses
    }

    /// Moves the selection by `step` choices, round the ends.
    pub(crate) fn move_selection(&mut self, step: i32) {
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let count = running
            .choice
            .and_then(|choice| running.dialogue.node(choice))
            .map_or(0, |node| node.options().len());
        if count == 0 || step == 0 {
            return;
        }
        let at = running.selected as i32 + step;
        running.selected = at.rem_euclid(count as i32) as usize;
    }

    pub(crate) fn showing_choices(&self) -> bool {
        self.running
            .as_ref()
            .is_some_and(|running| running.choice.is_some())
    }
}

/// Starts `Start`'s dialogue for the script calling it, or ends at once.
const START: &str = r#"
local running, yield, error, start = ...
return function(self, dialogue, speaker)
    local thread, is_main = running()
    if is_main then
        error("DialogueService:Start waits for the dialogue to end, so it can only be called from a script", 2)
    end
    local ended, ending = start(thread, dialogue, speaker)
    if ended then
        return ending
    end
    return yield()
end
"#;

/// `DialogueService`, as scripts see it. Both sides share one.
#[derive(Clone, Copy)]
pub struct LuaDialogueService;

impl UserData for LuaDialogueService {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Active", |lua, _| Ok(shared(lua)?.0.borrow().active()));
        // A Lua function, so the script can wait in it.
        fields.add_field_method_get("Start", |lua, _| start_function(lua));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("Advance", |lua, _, ()| advance(lua));
        methods.add_method("Choose", |lua, _, index: f64| {
            if !index.is_finite() || index < 1.0 {
                return Err(mlua::Error::runtime(
                    "DialogueService:Choose needs the choice's number, from 1",
                ));
            }
            choose(lua, index as usize - 1)
        });
        methods.add_method("Stop", |lua, _, ()| stop(lua));
        methods.add_method("GetLine", |lua, _, ()| {
            let view = {
                let shared = shared(lua)?;
                let dialogues = shared.0.borrow();
                let world = world(lua)?;
                view(&dialogues, &world)
            };
            let Some(view) = view else {
                return Ok(mlua::Value::Nil);
            };
            let table = lua.create_table()?;
            table.raw_set("Speaker", view.speaker)?;
            table.raw_set("Text", view.text)?;
            table.raw_set("Choices", lua.create_sequence_from(view.choices)?)?;
            table.raw_set("Selected", view.selected + 1)?;
            table.into_lua(lua)
        });
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("DialogueService"));
    }
}

struct StartFunction(Function);

fn start_function(lua: &Lua) -> mlua::Result<Function> {
    if let Some(start) = lua.app_data_ref::<StartFunction>() {
        return Ok(start.0.clone());
    }
    let coroutine: Table = lua.globals().raw_get("coroutine")?;
    let begin = lua.create_function(
        |lua, (thread, path, speaker): (Thread, mlua::Value, Option<LuaInstance>)| {
            let mlua::Value::String(path) = path else {
                return Err(mlua::Error::runtime(
                    "DialogueService:Start needs a dialogue file, such as \
                     DialogueService:Start(\"assets/dialogue/Farmer.dialogue.json\", workspace.Farmer)",
                ));
            };
            let path = path.to_string_lossy();
            let player = player_of(lua);
            let script = system::running_script(lua);
            let side = system::side(lua);
            system::waiting(lua, &thread);
            let ending = start(
                lua,
                &path,
                speaker.map(|speaker| speaker.0),
                player,
                Some((thread, script, side)),
            )?;
            match ending {
                Some(ending) => {
                    // It ended at once: nothing waits, so the script goes on.
                    if let Ok(shared) = shared(lua) {
                        shared.0.borrow_mut().finished.pop();
                    }
                    Ok((true, Some(ending)))
                }
                None => Ok((false, None)),
            }
        },
    )?;
    let start: Function = lua.load(START).set_name("=[engine]").call((
        coroutine.raw_get::<Function>("running")?,
        coroutine.raw_get::<Function>("yield")?,
        lua.globals().raw_get::<Function>("error")?,
        begin,
    ))?;
    lua.set_app_data(StartFunction(start.clone()));
    Ok(start)
}

/// Hands each finished dialogue's Ending to the scripts on `side`
/// waiting in Start.
pub(crate) fn resume_finished(lua: &Lua, side: Side, finished: &[Finished]) {
    for done in finished {
        for (thread, script, _) in done.waiting.iter().filter(|(_, _, s)| *s == side) {
            system::resume_as(lua, thread, done.ending.clone(), *script);
        }
    }
}
