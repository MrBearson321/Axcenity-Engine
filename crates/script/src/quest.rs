//! `game:GetService("QuestService")` (Phase 77): quests from data tables,
//! their objectives and the quest tracker.
//!
//! Quests are rows of the data table `quests` (`Title`, optional
//! `Description`); their objectives are rows of `quest_objectives`, in
//! order: `Quest` (the quest's key), `Kind` (`Count`, `Reach` or `Talk`),
//! `Target`, `Count` (1 when there's none) and `Text` (what the tracker
//! shows; the Target when empty). Titles and texts can be string table
//! keys, shown in the player's language.
//!
//! - **Count**: `QuestService:Progress(target, amount)` adds to it.
//! - **Reach**: done when the player character's middle is inside the part
//!   at the path `Target` from the Workspace (`Farm.Gate`), checked each
//!   frame.
//! - **Talk**: done when a dialogue starts with a speaker named `Target`.
//!
//! A quest whose objectives are all done completes. State is the player's
//! data in [`Saves`] (`Quest/<key>` is `"Active"` or `"Completed"`,
//! `Quest/<key>/<objective>` the progress), so save slots and checkpoints
//! keep it with no more work, and `LoadSlot` puts it back. Events wait in
//! [`Quests::events`] and fire on both sides from `fire_changes`.

use std::cell::RefCell;
use std::rc::Rc;

use axcenity_assets::{Assets, Cell, DataTable};
use axcenity_core::dialogue::{ObjectiveView, QuestView};
use axcenity_core::{InstanceId, Value, World};
use axcenity_save::Saves;
use mlua::{Lua, MetaMethod, UserData, UserDataFields, UserDataMethods};

use crate::events::{LuaSignal, Owner};
use crate::instance::world;
use crate::save::saves;

/// The data table quests are rows of.
pub const QUESTS_TABLE: &str = "quests";
/// The data table their objectives are rows of.
pub const OBJECTIVES_TABLE: &str = "quest_objectives";
const PREFIX: &str = "Quest/";

/// What an objective asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectiveKind {
    Count,
    Reach,
    Talk,
}

impl ObjectiveKind {
    fn name(self) -> &'static str {
        match self {
            ObjectiveKind::Count => "Count",
            ObjectiveKind::Reach => "Reach",
            ObjectiveKind::Talk => "Talk",
        }
    }
}

/// One objective, as its row says.
#[derive(Clone, Debug, PartialEq)]
pub struct Objective {
    pub key: String,
    pub kind: ObjectiveKind,
    pub target: String,
    pub count: u32,
    pub text: String,
}

/// One quest, as its rows say.
#[derive(Clone, Debug, PartialEq)]
pub struct Quest {
    pub key: String,
    pub title: String,
    pub description: String,
    pub objectives: Vec<Objective>,
}

/// Where a quest is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuestState {
    NotStarted,
    Active,
    Completed,
}

impl QuestState {
    pub fn name(self) -> &'static str {
        match self {
            QuestState::NotStarted => "NotStarted",
            QuestState::Active => "Active",
            QuestState::Completed => "Completed",
        }
    }
}

/// An event waiting to fire on both sides.
#[derive(Clone, Debug, PartialEq)]
pub enum QuestEvent {
    Started(String),
    Progressed {
        quest: String,
        objective: String,
        progress: u32,
    },
    Completed(String),
}

/// The events QuestService has to fire.
#[derive(Default)]
pub struct Quests {
    pub events: Vec<QuestEvent>,
}

#[derive(Clone, Default)]
pub(crate) struct SharedQuests(pub Rc<RefCell<Quests>>);

fn text_cell(table: &DataTable, row: &axcenity_assets::Row, column: &str) -> String {
    match table.column(column).map(|column| row.cell(column)) {
        Some(Cell::Text(text)) => text,
        Some(Cell::Number(n)) => n.to_string(),
        Some(Cell::Integer(n)) => n.to_string(),
        Some(Cell::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

/// The quest `key`, read from the data tables in use now.
pub fn quest(assets: &Assets, key: &str) -> Result<Quest, String> {
    let quests = assets.data_table(QUESTS_TABLE).map_err(|why| {
        format!("quests come from the data table \"{QUESTS_TABLE}\", which can't be read: {why}")
    })?;
    let row = quests.row(key).ok_or_else(|| {
        let keys: Vec<&str> = quests.rows.iter().map(|row| row.key.as_str()).collect();
        format!(
            "there's no quest \"{key}\" in the data table \"{QUESTS_TABLE}\"; choose from {}",
            if keys.is_empty() {
                "(none yet)".to_owned()
            } else {
                keys.join(", ")
            }
        )
    })?;
    let mut quest = Quest {
        key: key.to_owned(),
        title: text_cell(&quests, row, "Title"),
        description: text_cell(&quests, row, "Description"),
        objectives: Vec::new(),
    };
    if quest.title.is_empty() {
        quest.title = key.to_owned();
    }
    // No objectives table: quests only complete when asked to.
    let Ok(objectives) = assets.data_table(OBJECTIVES_TABLE) else {
        return Ok(quest);
    };
    for row in &objectives.rows {
        if text_cell(&objectives, row, "Quest") != key {
            continue;
        }
        let kind = match text_cell(&objectives, row, "Kind").as_str() {
            "Reach" => ObjectiveKind::Reach,
            "Talk" => ObjectiveKind::Talk,
            _ => ObjectiveKind::Count,
        };
        let count = match objectives.column("Count").map(|column| row.cell(column)) {
            Some(Cell::Integer(n)) => n.max(1) as u32,
            Some(Cell::Number(n)) if n.is_finite() => (n.round() as i64).max(1) as u32,
            _ => 1,
        };
        let count = if kind == ObjectiveKind::Count {
            count
        } else {
            1
        };
        let target = text_cell(&objectives, row, "Target");
        let mut text = text_cell(&objectives, row, "Text");
        if text.is_empty() {
            text = target.clone();
        }
        quest.objectives.push(Objective {
            key: row.key.clone(),
            kind,
            target,
            count,
            text,
        });
    }
    Ok(quest)
}

fn state_key(quest: &str) -> String {
    format!("{PREFIX}{quest}")
}

fn progress_key(quest: &str, objective: &str) -> String {
    format!("{PREFIX}{quest}/{objective}")
}

/// Where the quest `key` is, by the player's data.
pub fn state(saves: &Saves, key: &str) -> QuestState {
    match saves
        .player_data
        .get(&state_key(key))
        .and_then(Value::as_str)
    {
        Some("Active") => QuestState::Active,
        Some("Completed") => QuestState::Completed,
        _ => QuestState::NotStarted,
    }
}

/// How far the objective is.
pub fn progress(saves: &Saves, quest: &str, objective: &str) -> u32 {
    saves
        .player_data
        .get(&progress_key(quest, objective))
        .and_then(Value::as_number)
        .filter(|n| n.is_finite() && *n > 0.0)
        .map_or(0, |n| n as u32)
}

/// The quests that are active, in key order.
pub fn active(saves: &Saves) -> Vec<String> {
    saves
        .player_data
        .iter()
        .filter_map(|(name, value)| {
            let quest = name.strip_prefix(PREFIX)?;
            (!quest.contains('/') && value.as_str() == Some("Active")).then(|| quest.to_owned())
        })
        .collect()
}

/// What QuestService works with: the player's data, the tables, and the
/// events it queues.
pub struct QuestContext<'a> {
    pub saves: &'a mut Saves,
    pub assets: &'a Assets,
    pub events: &'a mut Vec<QuestEvent>,
}

impl QuestContext<'_> {
    /// Starts `key`. False when it's active or done already.
    pub fn start(&mut self, key: &str) -> Result<bool, String> {
        let quest = quest(self.assets, key)?;
        if state(self.saves, key) != QuestState::NotStarted {
            return Ok(false);
        }
        self.saves
            .player_data
            .insert(state_key(key), Value::String("Active".to_owned()));
        for objective in &quest.objectives {
            self.saves
                .player_data
                .insert(progress_key(key, &objective.key), Value::Number(0.0));
        }
        self.events.push(QuestEvent::Started(key.to_owned()));
        if !quest.objectives.is_empty() {
            self.complete_if_done(&quest);
        }
        Ok(true)
    }

    /// Completes `key`, whatever its objectives say. False when it's done
    /// already.
    pub fn complete(&mut self, key: &str) -> Result<bool, String> {
        quest(self.assets, key)?;
        if state(self.saves, key) == QuestState::Completed {
            return Ok(false);
        }
        self.saves
            .player_data
            .insert(state_key(key), Value::String("Completed".to_owned()));
        self.events.push(QuestEvent::Completed(key.to_owned()));
        Ok(true)
    }

    /// Adds `amount` to the objective of each active quest that `done`
    /// picks, up to its Count.
    fn advance(&mut self, amount: u32, done: impl Fn(&Objective) -> bool) {
        for key in active(self.saves) {
            let Ok(quest) = quest(self.assets, &key) else {
                continue;
            };
            let mut changed = false;
            for objective in quest.objectives.iter().filter(|o| done(o)) {
                let before = progress(self.saves, &key, &objective.key);
                let after = before.saturating_add(amount).min(objective.count);
                if after == before {
                    continue;
                }
                self.saves.player_data.insert(
                    progress_key(&key, &objective.key),
                    Value::Number(f64::from(after)),
                );
                self.events.push(QuestEvent::Progressed {
                    quest: key.clone(),
                    objective: objective.key.clone(),
                    progress: after,
                });
                changed = true;
            }
            if changed {
                self.complete_if_done(&quest);
            }
        }
    }

    fn complete_if_done(&mut self, quest: &Quest) {
        let done = quest
            .objectives
            .iter()
            .all(|objective| progress(self.saves, &quest.key, &objective.key) >= objective.count);
        if done && state(self.saves, &quest.key) == QuestState::Active {
            let _ = self.complete(&quest.key);
        }
    }

    /// `QuestService:Progress(target, amount)`: Count objectives aiming at
    /// `target`.
    pub fn progress(&mut self, target: &str, amount: u32) {
        self.advance(amount, |objective| {
            objective.kind == ObjectiveKind::Count && objective.target == target
        });
    }

    /// A dialogue started with a speaker called `name`.
    pub fn talked(&mut self, name: &str) {
        self.advance(1, |objective| {
            objective.kind == ObjectiveKind::Talk && objective.target == name
        });
    }

    /// Reach objectives whose part the player character is in.
    pub fn step(&mut self, world: &World) {
        let Some(middle) = crate::prompt::Viewer::find(world, None).map(|viewer| viewer.position)
        else {
            return;
        };
        let needs_check = active(self.saves).iter().any(|key| {
            quest(self.assets, key).is_ok_and(|quest| {
                quest
                    .objectives
                    .iter()
                    .any(|objective| objective.kind == ObjectiveKind::Reach)
            })
        });
        if !needs_check {
            return;
        }
        self.advance(1, |objective| {
            objective.kind == ObjectiveKind::Reach
                && find_path(world, &objective.target)
                    .is_some_and(|part| inside(world, part, middle))
        });
    }

    /// What the quest tracker shows: each active quest, in the player's
    /// language.
    pub fn tracker(&self, world: &World) -> Vec<QuestView> {
        active(self.saves)
            .into_iter()
            .filter_map(|key| {
                let quest = quest(self.assets, &key).ok()?;
                Some(QuestView {
                    title: translated(world, &quest.title),
                    objectives: quest
                        .objectives
                        .iter()
                        .map(|objective| ObjectiveView {
                            text: translated(world, &objective.text),
                            progress: progress(self.saves, &key, &objective.key),
                            count: objective.count,
                        })
                        .collect(),
                    quest: key,
                })
            })
            .collect()
    }
}

/// `text` in the player's language, if it's a string table key, else
/// itself.
pub fn translated(world: &World, text: &str) -> String {
    world.translate(text).unwrap_or(text).to_owned()
}

/// The Instance at `path` (names joined by `.` or `/`) from the Workspace.
pub(crate) fn find_path(world: &World, path: &str) -> Option<InstanceId> {
    let mut at = world.workspace();
    for name in path.split(['.', '/']).filter(|name| !name.is_empty()) {
        at = world
            .get(at)?
            .children()
            .iter()
            .copied()
            .find(|&child| world.get(child).is_some_and(|c| c.name() == name))?;
    }
    (at != world.workspace()).then_some(at)
}

/// Whether `point` is inside the box of `part` (ignoring its rotation).
fn inside(world: &World, part: InstanceId, point: glam::Vec3) -> bool {
    let Some(instance) = world.get(part) else {
        return false;
    };
    let (Some(position), Some(size)) = (
        instance.property("Position").and_then(Value::as_vector3),
        instance.property("Size").and_then(Value::as_vector3),
    ) else {
        return false;
    };
    let p = glam::Vec3::new(position.x, position.y, position.z);
    let half = glam::Vec3::new(size.x, size.y, size.z).abs() * 0.5;
    let d = (point - p).abs();
    d.x <= half.x && d.y <= half.y && d.z <= half.z
}

fn shared(lua: &Lua) -> mlua::Result<SharedQuests> {
    lua.app_data_ref::<SharedQuests>()
        .map(|shared| shared.clone())
        .ok_or_else(|| mlua::Error::runtime("quests aren't set up on this side"))
}

fn assets(lua: &Lua) -> mlua::Result<Assets> {
    lua.app_data_ref::<Assets>()
        .map(|assets| assets.clone())
        .ok_or_else(|| mlua::Error::runtime("quests need a project, and none is open"))
}

/// Runs `f` with the quests' context.
pub(crate) fn with_quests<R>(
    lua: &Lua,
    f: impl FnOnce(&mut QuestContext) -> Result<R, String>,
) -> mlua::Result<R> {
    let saves = saves(lua)?;
    let assets = assets(lua)?;
    let quests = shared(lua)?;
    let mut saves = saves.borrow_mut();
    let mut quests = quests.0.borrow_mut();
    let mut context = QuestContext {
        saves: &mut saves,
        assets: &assets,
        events: &mut quests.events,
    };
    f(&mut context).map_err(mlua::Error::runtime)
}

fn key_arg(value: mlua::Value, method: &str) -> mlua::Result<String> {
    match value {
        mlua::Value::String(key) => Ok(key.to_string_lossy()),
        _ => Err(mlua::Error::runtime(format!(
            "QuestService:{method} needs a quest's key from the \"{QUESTS_TABLE}\" data table, \
             such as QuestService:{method}(\"apples\")"
        ))),
    }
}

/// `QuestService`, as scripts see it. Both sides share one.
#[derive(Clone, Copy)]
pub struct LuaQuestService;

impl UserData for LuaQuestService {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        for event in ["QuestStarted", "QuestProgressed", "QuestCompleted"] {
            fields.add_field_method_get(event, move |_, _| {
                Ok(LuaSignal::named(Owner::QuestService, event))
            });
        }
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("Start", |lua, _, key: mlua::Value| {
            let key = key_arg(key, "Start")?;
            with_quests(lua, |quests| quests.start(&key))
        });
        methods.add_method("Complete", |lua, _, key: mlua::Value| {
            let key = key_arg(key, "Complete")?;
            with_quests(lua, |quests| quests.complete(&key))
        });
        methods.add_method(
            "Progress",
            |lua, _, (target, amount): (mlua::Value, Option<f64>)| {
                let mlua::Value::String(target) = target else {
                    return Err(mlua::Error::runtime(
                        "QuestService:Progress needs the Target of Count objectives, such as \
                         QuestService:Progress(\"Apple\", 1)",
                    ));
                };
                let amount = amount.unwrap_or(1.0);
                if !amount.is_finite() || amount < 0.0 {
                    return Err(mlua::Error::runtime(
                        "QuestService:Progress's amount must be a number 0 or more",
                    ));
                }
                let target = target.to_string_lossy();
                with_quests(lua, |quests| {
                    quests.progress(&target, amount.round() as u32);
                    Ok(())
                })
            },
        );
        methods.add_method("GetState", |lua, _, key: mlua::Value| {
            let key = key_arg(key, "GetState")?;
            with_quests(lua, |quests| {
                quest(quests.assets, &key)?;
                Ok(state(quests.saves, &key).name())
            })
        });
        methods.add_method("GetActive", |lua, _, ()| {
            let keys = with_quests(lua, |quests| Ok(active(quests.saves)))?;
            lua.create_sequence_from(keys)
        });
        methods.add_method("GetObjectives", |lua, _, key: mlua::Value| {
            let key = key_arg(key, "GetObjectives")?;
            let (quest, progresses) = with_quests(lua, |quests| {
                let quest = quest(quests.assets, &key)?;
                let progresses: Vec<u32> = quest
                    .objectives
                    .iter()
                    .map(|objective| progress(quests.saves, &key, &objective.key))
                    .collect();
                Ok((quest, progresses))
            })?;
            let world = world(lua)?;
            let list = lua.create_table()?;
            for (objective, progress) in quest.objectives.iter().zip(progresses) {
                let row = lua.create_table()?;
                row.raw_set("Key", objective.key.as_str())?;
                row.raw_set("Kind", objective.kind.name())?;
                row.raw_set("Target", objective.target.as_str())?;
                row.raw_set("Text", translated(&world, &objective.text))?;
                row.raw_set("Count", objective.count)?;
                row.raw_set("Progress", progress)?;
                row.raw_set("Done", progress >= objective.count)?;
                list.raw_push(row)?;
            }
            Ok(list)
        });
        methods.add_method("GetTitle", |lua, _, key: mlua::Value| {
            let key = key_arg(key, "GetTitle")?;
            let title = with_quests(lua, |quests| Ok(quest(quests.assets, &key)?.title))?;
            Ok(translated(&*world(lua)?, &title))
        });
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("QuestService"));
    }
}
