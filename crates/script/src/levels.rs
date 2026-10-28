//! `game:GetService("LevelService")` (Phase 48): the game's levels.
//!
//! - `LoadLevel(name, spawnName?)` puts the level's Workspace in place of
//!   the current one. The services outside the Workspace (ReplicatedStorage,
//!   Players, Lighting, StarterPlayer, Teams) stay as they are, and so do
//!   the Workspace's ScreenGuis (the game's UI) and the players'
//!   characters, which go to the Instance called `spawnName` in the new
//!   level, else to one of their SpawnLocations there. `LevelLoaded(name)`
//!   fires on both sides once it's in.
//! - `AddLevel(name)` copies a level into a Model of that name in the
//!   Workspace, beside the current level; `RemoveLevel(name)` takes it out.
//! - `CurrentLevel` is the loaded level's name.
//!
//! Hosts hand over every level with
//! [`ScriptRuntime::set_levels`](crate::ScriptRuntime::set_levels). Save
//! slots keep the current level and the added ones ([`SavedLevels`]):
//! `LoadSlot` goes to them before it restores the rest ([`go_to_saved`]).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use axcenity_core::{ClassName, InstanceId, Value, World};
use axcenity_save::SavedLevels;
use mlua::{Lua, MetaMethod, UserData, UserDataFields, UserDataMethods};

use crate::events::{LuaSignal, Owner};
use crate::instance::{LuaInstance, world_mut};
use crate::players::{pick_spawn, stand_at, top_of};
use crate::types::type_name;
use crate::{OutputKind, push_output, save};

/// The game's levels and where it is among them.
#[derive(Default)]
pub struct Levels {
    /// The start level's name.
    start: String,
    /// Every level by name, the start level among them, each a world of
    /// its own. Only their Workspaces are used.
    worlds: BTreeMap<String, World>,
    /// The level in the Workspace now.
    current: String,
    /// The levels added beside it, in the order they were added, each with
    /// the Model it's in.
    added: Vec<(String, InstanceId)>,
    /// Levels loaded whose `LevelLoaded` hasn't fired yet.
    pub(crate) loaded: Vec<String>,
}

impl Levels {
    /// The levels `worlds`, with `start` (one of them) loaded now.
    pub fn new(start: &str, worlds: BTreeMap<String, World>) -> Self {
        Self {
            start: start.to_owned(),
            current: start.to_owned(),
            worlds,
            added: Vec::new(),
            loaded: Vec::new(),
        }
    }

    /// Says `name` is the level in the Workspace now (the editor's Play of
    /// an open level). An unknown name is kept too, for `CurrentLevel`.
    pub fn set_current(&mut self, name: &str) {
        self.current = name.to_owned();
    }

    /// The level in the Workspace now: the start level until one loads.
    pub fn current(&self) -> &str {
        &self.current
    }

    /// The start level's name.
    pub fn start(&self) -> &str {
        &self.start
    }

    /// The levels added beside the current one, in order.
    pub fn added(&self) -> impl Iterator<Item = &str> {
        self.added.iter().map(|(name, _)| name.as_str())
    }

    /// What a save slot keeps: `None` for a game without levels.
    pub fn saved(&self) -> Option<SavedLevels> {
        (!self.worlds.is_empty()).then(|| SavedLevels {
            current: self.current.clone(),
            added: self.added().map(str::to_owned).collect(),
        })
    }

    /// The level `name`'s world, or the error a script sees.
    fn world(&self, name: &str) -> mlua::Result<&World> {
        self.worlds.get(name).ok_or_else(|| {
            let names: Vec<&str> = self.worlds.keys().map(String::as_str).collect();
            mlua::Error::runtime(if names.is_empty() {
                format!("there is no level called \"{name}\"; this game has no levels")
            } else {
                format!(
                    "there is no level called \"{name}\"; the levels are {}",
                    names.join(", ")
                )
            })
        })
    }
}

/// The levels, shared by both sides' interpreters.
#[derive(Clone, Default)]
pub(crate) struct SharedLevels(pub Rc<RefCell<Levels>>);

fn levels(lua: &Lua) -> mlua::Result<Rc<RefCell<Levels>>> {
    lua.app_data_ref::<SharedLevels>()
        .map(|shared| shared.0.clone())
        .ok_or_else(|| mlua::Error::runtime("levels aren't ready"))
}

/// Tells the save slots where the game is now.
fn note_in_saves(lua: &Lua, levels: &Levels) {
    if let Ok(saves) = save::saves(lua) {
        saves.borrow_mut().levels = levels.saved();
    }
}

/// The players' characters in the Workspace, which stay through a load.
fn characters(world: &World) -> Vec<InstanceId> {
    crate::players::players(world)
        .into_iter()
        .filter_map(|player| match world.get(player)?.property("Character")? {
            Value::Instance(Some(character)) => Some(*character),
            _ => None,
        })
        .filter(|&character| world.is_descendant_of(character, world.workspace()))
        .collect()
}

/// Puts `level`'s Workspace in place of `world`'s, keeping the Workspace's
/// ScreenGuis and `kept` (the characters). A ScreenGui of the level's with
/// the name of one kept isn't brought in.
fn replace_workspace(world: &mut World, level: &World, kept: &[InstanceId]) {
    let workspace = world.workspace();
    let children = world
        .get(workspace)
        .map(|instance| instance.children().to_vec())
        .unwrap_or_default();
    let mut guis = Vec::new();
    for child in children {
        let Some(instance) = world.get(child) else {
            continue;
        };
        if instance.class() == ClassName::ScreenGui {
            guis.push(instance.name().to_owned());
        } else if !kept.contains(&child) {
            let _ = world.destroy(child);
        }
    }
    if let Some(instance) = level.get(level.workspace()) {
        let properties: Vec<_> = instance
            .properties()
            .map(|(name, value)| (name, value.clone()))
            .collect();
        for (name, value) in properties {
            let _ = world.set_property(workspace, name, value);
        }
    }
    let children = level
        .get(level.workspace())
        .map(|instance| instance.children().to_vec())
        .unwrap_or_default();
    for child in children {
        let duplicate_gui = level.get(child).is_some_and(|instance| {
            instance.class() == ClassName::ScreenGui
                && guis.iter().any(|name| name == instance.name())
        });
        if duplicate_gui {
            continue;
        }
        if let Ok(copy) = world.copy_from(level, child) {
            let _ = world.set_parent(copy, Some(workspace));
        }
    }
}

/// Loads the level `name` into `world` (see the module docs), without
/// moving characters. `LevelLoaded` waits in `levels.loaded`.
fn load(levels: &mut Levels, world: &mut World, name: &str) -> mlua::Result<()> {
    let level = levels.world(name)?;
    let kept = characters(world);
    replace_workspace(world, level, &kept);
    // Their Models went with the rest of the Workspace.
    levels.added.clear();
    levels.current = name.to_owned();
    levels.loaded.push(name.to_owned());
    Ok(())
}

/// Copies the level `name` into a new Model of that name in the Workspace.
pub(crate) fn add(levels: &mut Levels, world: &mut World, name: &str) -> mlua::Result<InstanceId> {
    if levels.added.iter().any(|(added, _)| added == name) {
        return Err(mlua::Error::runtime(format!(
            "the level \"{name}\" is added already; RemoveLevel(\"{name}\") takes it out"
        )));
    }
    let level = levels.world(name)?;
    let model = world.create(ClassName::Model).expect("not a service");
    world.set_name(model, name).expect("just made");
    let children = level
        .get(level.workspace())
        .map(|instance| instance.children().to_vec())
        .unwrap_or_default();
    for child in children {
        if let Ok(copy) = world.copy_from(level, child) {
            let _ = world.set_parent(copy, Some(model));
        }
    }
    let workspace = world.workspace();
    world
        .set_parent(model, Some(workspace))
        .expect("both exist");
    levels.added.push((name.to_owned(), model));
    Ok(model)
}

/// Takes the added level `name` out of the Workspace.
pub(crate) fn remove(levels: &mut Levels, world: &mut World, name: &str) -> mlua::Result<()> {
    let Some(index) = levels.added.iter().position(|(added, _)| added == name) else {
        return Err(mlua::Error::runtime(format!(
            "the level \"{name}\" isn't added; AddLevel(\"{name}\") adds it"
        )));
    };
    let (_, model) = levels.added.remove(index);
    // A script may have destroyed it already.
    let _ = world.destroy(model);
    Ok(())
}

/// Before a save slot is restored: goes to the level it was saved in, if
/// that isn't the current one, and adds and removes levels so the added
/// ones are the slot's. A level the game no longer has is skipped.
pub(crate) fn go_to_saved(lua: &Lua, world: &mut World, saved: &SavedLevels) {
    let Ok(levels) = levels(lua) else {
        return;
    };
    let mut levels = levels.borrow_mut();
    if levels.worlds.is_empty() {
        return;
    }
    if saved.current != levels.current && load(&mut levels, world, &saved.current).is_err() {
        return;
    }
    let extra: Vec<String> = levels
        .added()
        .filter(|name| !saved.added.iter().any(|kept| kept == name))
        .map(str::to_owned)
        .collect();
    for name in extra {
        let _ = remove(&mut levels, world, &name);
    }
    for name in &saved.added {
        if !levels.added().any(|added| added == name) {
            let _ = add(&mut levels, world, name);
        }
    }
    note_in_saves(lua, &levels);
}

/// A level's name given to `method`.
fn name_arg(lua: &Lua, value: &mlua::Value, method: &str) -> mlua::Result<String> {
    match value {
        mlua::Value::String(name) => Ok(name.to_string_lossy()),
        other => Err(mlua::Error::runtime(format!(
            "{method} needs a level's name, such as LevelService:{method}(\"Dungeon\"), not a {}",
            type_name(lua, other)
        ))),
    }
}

/// Stands each character in the new level: at the Instance called `spawn`
/// (the top of a part, or a Checkpoint's Position), else on one of its
/// player's SpawnLocations. With neither it stays where it was.
fn place_characters(lua: &Lua, world: &mut World, spawn: Option<&str>) {
    let workspace = world.workspace();
    let kept = characters(world);
    let named = spawn.and_then(|name| {
        world
            .descendants(workspace)
            .into_iter()
            .filter(|&id| {
                !kept
                    .iter()
                    .any(|&c| world.is_descendant_of(id, c) || id == c)
            })
            .find(|&id| {
                world
                    .get(id)
                    .is_some_and(|instance| instance.name() == name)
            })
    });
    if let (Some(name), None) = (spawn, named) {
        push_output(
            lua,
            OutputKind::Warning,
            format!(
                "LoadLevel: the level has nothing called \"{name}\" to spawn at, so characters \
                 go to a SpawnLocation"
            ),
        );
    }
    for player in crate::players::players(world) {
        let Some(Value::Instance(Some(character))) = world
            .get(player)
            .and_then(|instance| instance.property("Character"))
            .cloned()
        else {
            continue;
        };
        let at = match named {
            Some(id) if world.get(id).and_then(|i| i.property("Size")).is_some() => {
                Some(top_of(world, id))
            }
            Some(id) => world
                .get(id)
                .and_then(|instance| instance.property("Position"))
                .and_then(Value::as_vector3),
            None => pick_spawn(lua, world, player).map(|spawn| top_of(world, spawn)),
        };
        if let Some(at) = at {
            stand_at(world, character, at);
        }
    }
}

/// `LevelService`, as scripts see it. Both sides have one.
#[derive(Clone, Copy)]
pub struct LuaLevelService;

impl UserData for LuaLevelService {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("CurrentLevel", |lua, _| {
            Ok(levels(lua)?.borrow().current.clone())
        });
        fields.add_field_method_get("LevelLoaded", |_, _| {
            Ok(LuaSignal::named(Owner::LevelService, "LevelLoaded"))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method(
            "LoadLevel",
            |lua, _, (name, spawn): (mlua::Value, mlua::Value)| {
                let name = name_arg(lua, &name, "LoadLevel")?;
                let spawn = match &spawn {
                    mlua::Value::Nil => None,
                    mlua::Value::String(spawn) => Some(spawn.to_string_lossy()),
                    other => {
                        return Err(mlua::Error::runtime(format!(
                            "LoadLevel's spawnName is the name of the Instance to arrive at, \
                             such as LevelService:LoadLevel(\"Castle\", \"Gate\"), not a {}",
                            type_name(lua, other)
                        )));
                    }
                };
                let shared = levels(lua)?;
                let mut levels = shared.borrow_mut();
                let mut world = world_mut(lua)?;
                load(&mut levels, &mut world, &name)?;
                place_characters(lua, &mut world, spawn.as_deref());
                note_in_saves(lua, &levels);
                Ok(())
            },
        );
        methods.add_method("AddLevel", |lua, _, name: mlua::Value| {
            let name = name_arg(lua, &name, "AddLevel")?;
            let shared = levels(lua)?;
            let mut levels = shared.borrow_mut();
            let model = {
                let mut world = world_mut(lua)?;
                add(&mut levels, &mut world, &name)?
            };
            note_in_saves(lua, &levels);
            Ok(LuaInstance(model))
        });
        methods.add_method("RemoveLevel", |lua, _, name: mlua::Value| {
            let name = name_arg(lua, &name, "RemoveLevel")?;
            let shared = levels(lua)?;
            let mut levels = shared.borrow_mut();
            let mut world = world_mut(lua)?;
            remove(&mut levels, &mut world, &name)?;
            note_in_saves(lua, &levels);
            Ok(())
        });
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("LevelService"));
    }
}
