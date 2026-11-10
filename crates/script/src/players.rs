//! Players, spawns and teams (Phase 47), for one local player now and
//! many later.
//!
//! When the player joins, they're put on the AutoAssignable Team with the
//! fewest players and `PlayerAdded` fires. A character already placed in
//! the Workspace becomes theirs (and a copy of it, kept out of the game, is
//! what they respawn as); otherwise, with `Players.CharacterAutoLoads` on,
//! they get one at once. [`load_character`] (`Player:LoadCharacter()`, and
//! each respawn `Players.RespawnTime` seconds after the character's
//! Humanoid dies) copies the StarterCharacter (the prefab hosts put in
//! StarterPlayer, else the placed character's copy, else the character
//! Insert > Character makes) onto a SpawnLocation of the player's team,
//! chosen at random, with a ForceField for its `Duration`. The events wait
//! in [`Spawned`] for the runtime to fire on both sides.

use axcenity_core::{ClassName, Color3, InstanceId, STARTER_CHARACTER, Value, Vector3, World};
use mlua::{Function, Lua};

use crate::instance::{LuaInstance, destroyed, world, world_mut};
use crate::random::LuaRandom;
use crate::tree::ClonedScripts;
use crate::types::type_name;
use crate::{OutputKind, push_output};

/// Something that happened to a Player, waiting to fire on both sides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerEvent {
    /// `Players.PlayerAdded(player)`.
    Added { player: InstanceId },
    /// `Players.PlayerRemoving(player)`, as they leave a hosted game
    /// (Phase 63). The runtime destroys the Player (and its character)
    /// once this has fired.
    Removing { player: InstanceId },
    /// `Player.CharacterRemoving(character)`. The character is already out
    /// of the Workspace; the runtime destroys it once this has fired.
    CharacterRemoving {
        player: InstanceId,
        character: InstanceId,
    },
    /// `Player.CharacterAdded(character)`.
    CharacterAdded {
        player: InstanceId,
        character: InstanceId,
    },
}

/// What [`load_character`] did, for the runtime to take: events to fire,
/// and each new ForceField with the seconds it lasts.
#[derive(Default)]
pub struct Spawned {
    pub events: Vec<PlayerEvent>,
    pub shields: Vec<(InstanceId, f64)>,
}

/// The server side's spawning state: the copy of a placed character that
/// players respawn as (with no StarterCharacter), and the random choice of
/// SpawnLocation.
pub struct Spawning {
    pub template: Option<InstanceId>,
    random: LuaRandom,
}

impl Default for Spawning {
    fn default() -> Self {
        Self {
            template: None,
            random: LuaRandom::new(LuaRandom::fresh_seed()),
        }
    }
}

fn flag(world: &World, id: InstanceId, name: &str, default: bool) -> bool {
    world
        .get(id)
        .and_then(|instance| instance.property(name))
        .and_then(Value::as_bool)
        .unwrap_or(default)
}

fn number(world: &World, id: InstanceId, name: &str) -> f64 {
    world
        .get(id)
        .and_then(|instance| instance.property(name))
        .and_then(Value::as_number)
        .unwrap_or(0.0)
}

fn color(world: &World, id: InstanceId, name: &str) -> Option<Color3> {
    match world.get(id)?.property(name)? {
        Value::Color3(color) => Some(*color),
        _ => None,
    }
}

fn instance_property(world: &World, id: InstanceId, name: &str) -> Option<InstanceId> {
    match world.get(id)?.property(name)? {
        Value::Instance(target) => target.filter(|&target| world.contains(target)),
        _ => None,
    }
}

fn is_class(world: &World, id: InstanceId, class: ClassName) -> bool {
    world
        .get(id)
        .is_some_and(|instance| instance.class() == class)
}

fn children_of(world: &World, parent: InstanceId, class: ClassName) -> Vec<InstanceId> {
    world.get(parent).map_or_else(Vec::new, |instance| {
        instance
            .children()
            .iter()
            .copied()
            .filter(|&child| is_class(world, child, class))
            .collect()
    })
}

/// Two TeamColors match when each channel is within a 255th.
fn same_color(a: Color3, b: Color3) -> bool {
    const NEAR: f32 = 0.5 / 255.0;
    (a.r - b.r).abs() <= NEAR && (a.g - b.g).abs() <= NEAR && (a.b - b.b).abs() <= NEAR
}

/// Every Player in Players.
pub fn players(world: &World) -> Vec<InstanceId> {
    children_of(world, world.players(), ClassName::Player)
}

/// The Team `player` is on: none while it's `Neutral`.
pub fn team_of(world: &World, player: InstanceId) -> Option<InstanceId> {
    if flag(world, player, "Neutral", true) {
        return None;
    }
    instance_property(world, player, "Team").filter(|&team| is_class(world, team, ClassName::Team))
}

/// The players on `team` (`Team:GetPlayers()`).
pub fn team_players(world: &World, team: InstanceId) -> Vec<InstanceId> {
    players(world)
        .into_iter()
        .filter(|&player| team_of(world, player) == Some(team))
        .collect()
}

/// The Player whose `Character` is `character`, if any.
pub fn player_of(world: &World, character: InstanceId) -> Option<InstanceId> {
    players(world)
        .into_iter()
        .find(|&player| instance_property(world, player, "Character") == Some(character))
}

/// Puts a neutral `player` on the AutoAssignable Team with the fewest
/// players (the first of those tied), if there is one.
pub fn assign_team(world: &mut World, player: InstanceId) {
    if team_of(world, player).is_some() {
        return;
    }
    let teams = children_of(world, world.teams(), ClassName::Team);
    let Some(team) = teams
        .into_iter()
        .filter(|&team| flag(world, team, "AutoAssignable", true))
        .min_by_key(|&team| team_players(world, team).len())
    else {
        return;
    };
    set_team(world, player, Some(team));
}

/// Sets `player`'s Team, and Neutral to whether it's on none.
pub fn set_team(world: &mut World, player: InstanceId, team: Option<InstanceId>) {
    let _ = world.set_property(player, "Team", Value::Instance(team));
    let _ = world.set_property(player, "Neutral", Value::Bool(team.is_none()));
}

/// The enabled SpawnLocations `player` may spawn on: their team's (by
/// TeamColor), or else the Neutral ones. A neutral player only has the
/// Neutral ones.
pub fn spawn_locations(world: &World, player: InstanceId) -> Vec<InstanceId> {
    let enabled: Vec<InstanceId> = world
        .descendants_iter(world.workspace())
        .filter(|&id| {
            is_class(world, id, ClassName::SpawnLocation) && flag(world, id, "Enabled", true)
        })
        .collect();
    let neutral = |id: InstanceId| flag(world, id, "Neutral", true);
    if let Some(team_color) =
        team_of(world, player).and_then(|team| color(world, team, "TeamColor"))
    {
        let own: Vec<InstanceId> = enabled
            .iter()
            .copied()
            .filter(|&id| {
                !neutral(id)
                    && color(world, id, "TeamColor").is_some_and(|c| same_color(c, team_color))
            })
            .collect();
        if !own.is_empty() {
            return own;
        }
    }
    enabled.into_iter().filter(|&id| neutral(id)).collect()
}

fn vector(world: &World, id: InstanceId, name: &str) -> Option<Vector3> {
    world.get(id)?.property(name)?.as_vector3()
}

/// The middle of the bottom of `character`'s parts' boxes.
fn feet(world: &World, character: InstanceId) -> Option<Vector3> {
    let mut low = [f32::INFINITY; 3];
    let mut high = [f32::NEG_INFINITY; 3];
    for part in axcenity_character::body_parts(world, character) {
        let (Some(position), Some(size)) =
            (vector(world, part, "Position"), vector(world, part, "Size"))
        else {
            continue;
        };
        for (axis, (p, s)) in [
            (position.x, size.x),
            (position.y, size.y),
            (position.z, size.z),
        ]
        .into_iter()
        .enumerate()
        {
            low[axis] = low[axis].min(p - s * 0.5);
            high[axis] = high[axis].max(p + s * 0.5);
        }
    }
    low[0]
        .is_finite()
        .then(|| Vector3::new((low[0] + high[0]) * 0.5, low[1], (low[2] + high[2]) * 0.5))
}

/// Moves `character`'s parts together so its feet are at `to`.
pub(crate) fn stand_at(world: &mut World, character: InstanceId, to: Vector3) {
    let Some(from) = feet(world, character) else {
        return;
    };
    let shift = Vector3::new(to.x - from.x, to.y - from.y, to.z - from.z);
    for part in axcenity_character::body_parts(world, character) {
        if let Some(position) = vector(world, part, "Position") {
            let moved = Vector3::new(
                position.x + shift.x,
                position.y + shift.y,
                position.z + shift.z,
            );
            let _ = world.set_property(part, "Position", Value::Vector3(moved));
        }
    }
}

/// The middle of the top of a SpawnLocation.
pub(crate) fn top_of(world: &World, spawn: InstanceId) -> Vector3 {
    let position = vector(world, spawn, "Position").unwrap_or_default();
    let size = vector(world, spawn, "Size").unwrap_or_default();
    Vector3::new(position.x, position.y + size.y * 0.5, position.z)
}

/// Copies `template` and everything in it, noting each copied script for
/// the runtime to start once the copy is in the Workspace.
fn copy_of(lua: &Lua, world: &mut World, template: InstanceId) -> Option<InstanceId> {
    let copy = world.duplicate(template).ok()?;
    let originals = std::iter::once(template).chain(world.descendants_iter(template));
    let copies = std::iter::once(copy).chain(world.descendants_iter(copy));
    let scripts: Vec<(InstanceId, InstanceId)> = originals
        .zip(copies)
        .filter(|&(original, _)| {
            world
                .get(original)
                .is_some_and(|instance| instance.class().is_script())
        })
        .map(|(original, copy)| (copy, original))
        .collect();
    if let Some(mut cloned) = lua.app_data_mut::<ClonedScripts>() {
        cloned.0.extend(scripts);
    }
    Some(copy)
}

/// `player:LoadCharacter()`: the old character (if any) leaves the
/// Workspace (`CharacterRemoving`, then it's destroyed), and a new copy of
/// the StarterCharacter stands on one of the player's SpawnLocations,
/// chosen at random, with a ForceField for the SpawnLocation's `Duration`
/// (`CharacterAdded`). With no SpawnLocation it stands at the origin, and
/// Output warns, or (as a copy of a placed character) where that was.
pub fn load_character(lua: &Lua, player: InstanceId) -> mlua::Result<InstanceId> {
    let mut events = Vec::new();
    let mut warning = None;
    let shield;
    let character = {
        let mut world = world_mut(lua)?;
        if !is_class(&world, player, ClassName::Player) {
            return Err(destroyed());
        }
        if let Some(old) = instance_property(&world, player, "Character") {
            let _ = world.set_parent(old, None);
            events.push(PlayerEvent::CharacterRemoving {
                player,
                character: old,
            });
        }
        let starter = world
            .find_first_child(world.starter_player(), STARTER_CHARACTER)
            .filter(|&model| is_class(&world, model, ClassName::Model));
        let placed = lua
            .app_data_ref::<Spawning>()
            .and_then(|spawning| spawning.template)
            .filter(|&template| world.contains(template));
        let template = starter.or(placed);
        let character = match template.and_then(|template| copy_of(lua, &mut world, template)) {
            Some(copy) => copy,
            None => axcenity_character::rig::character(&mut world, Vector3::default(), ""),
        };
        if world
            .get(world.starter_player())
            .and_then(|starter| starter.property("CharacterFootPlacement"))
            .and_then(Value::as_bool)
            == Some(true)
        {
            let controllers: Vec<InstanceId> = world
                .descendants_iter(character)
                .filter(|&id| {
                    world
                        .get(id)
                        .is_some_and(|instance| instance.class().is_character_controller())
                })
                .collect();
            for controller in controllers {
                let _ = world.set_property(controller, "FootPlacement", Value::Bool(true));
            }
        }
        let name = world
            .get(player)
            .map(|instance| instance.name().to_owned())
            .unwrap_or_default();
        let _ = world.set_name(character, name.clone());

        let spawn = pick_spawn(lua, &world, player);
        match spawn {
            Some(spawn) => {
                let top = top_of(&world, spawn);
                stand_at(&mut world, character, top);
            }
            // A copy of a placed character stands where that one was.
            None if starter.is_none() && placed.is_some() => {}
            None => {
                stand_at(&mut world, character, Vector3::default());
                warning = Some(format!(
                    "{name} has no enabled SpawnLocation to spawn on{}, so their character \
                     is at the origin. Insert > SpawnLocation (with Neutral on, or their \
                     team's TeamColor) to choose where.",
                    match team_of(&world, player).and_then(|team| world.get(team)) {
                        Some(team) => format!(" for the {} team", team.name()),
                        None => String::new(),
                    }
                ));
            }
        }
        shield = spawn
            .map(|spawn| number(&world, spawn, "Duration"))
            .filter(|&seconds| seconds > 0.0 && seconds.is_finite())
            .map(|seconds| {
                let field = world.create(ClassName::ForceField).expect("not a service");
                world
                    .set_parent(field, Some(character))
                    .expect("both exist");
                (field, seconds)
            });
        let workspace = world.workspace();
        world
            .set_parent(character, Some(workspace))
            .expect("both exist");
        let _ = world.set_property(player, "Character", Value::Instance(Some(character)));
        events.push(PlayerEvent::CharacterAdded { player, character });
        character
    };
    if let Some(text) = warning {
        push_output(lua, OutputKind::Warning, text);
    }
    if let Some(mut spawned) = lua.app_data_mut::<Spawned>() {
        spawned.events.extend(events);
        spawned.shields.extend(shield);
    }
    Ok(character)
}

/// One of `player`'s SpawnLocations (see [`spawn_locations`]), chosen at
/// random, or `None` without one.
pub(crate) fn pick_spawn(lua: &Lua, world: &World, player: InstanceId) -> Option<InstanceId> {
    let choices = spawn_locations(world, player);
    match choices.len() {
        0 => None,
        count => lua.app_data_mut::<Spawning>().map(|mut spawning| {
            let index = spawning.random.next_integer(0, count as i64 - 1);
            choices[index as usize]
        }),
    }
}

fn list(lua: &Lua, ids: Vec<InstanceId>) -> mlua::Result<mlua::Table> {
    lua.create_sequence_from(ids.into_iter().map(LuaInstance))
}

/// `Player:Kick` calls not yet carried out: each Player, with the reason.
#[derive(Default)]
pub struct Kicks(pub Vec<(InstanceId, String)>);

pub fn methods(lua: &Lua) -> mlua::Result<Vec<(ClassName, &'static str, Function)>> {
    let load = lua.create_function(|lua, this: LuaInstance| {
        crate::sides::check(lua, "LoadCharacter")?;
        load_character(lua, this.0)?;
        Ok(())
    })?;
    let kick = lua.create_function(|lua, (this, reason): (LuaInstance, Option<String>)| {
        crate::sides::check(lua, "Kick")?;
        if !is_class(&*world(lua)?, this.0, ClassName::Player) {
            return Err(destroyed());
        }
        let reason = reason
            .filter(|reason| !reason.trim().is_empty())
            .unwrap_or_else(|| "You were kicked from the game.".to_owned());
        if let Some(mut kicks) = lua.app_data_mut::<Kicks>() {
            kicks.0.push((this.0, reason));
        }
        Ok(())
    })?;
    let get_players = lua.create_function(|lua, _: LuaInstance| {
        let ids = players(&*world(lua)?);
        list(lua, ids)
    })?;
    let team_get_players = lua.create_function(|lua, this: LuaInstance| {
        let ids = team_players(&*world(lua)?, this.0);
        list(lua, ids)
    })?;
    let stream_around = crate::network::stream_around_function(lua)?;
    Ok(vec![
        (ClassName::Player, "LoadCharacter", load),
        (ClassName::Player, "Kick", kick),
        (ClassName::Player, "RequestStreamAroundAsync", stream_around),
        (ClassName::Players, "GetPlayers", get_players),
        (ClassName::Team, "GetPlayers", team_get_players),
    ])
}

/// `player.Team = team`: a Team or nil, which also sets `Neutral`.
pub fn set(lua: &Lua, player: InstanceId, value: mlua::Value) -> mlua::Result<()> {
    let team = match &value {
        mlua::Value::Nil => None,
        mlua::Value::UserData(data) => data
            .borrow::<LuaInstance>()
            .ok()
            .map(|team| team.0)
            .filter(|&team| world(lua).is_ok_and(|world| is_class(&world, team, ClassName::Team))),
        _ => None,
    };
    if team.is_none() && !value.is_nil() {
        return Err(mlua::Error::runtime(format!(
            "Player.Team must be a Team or nil, not {}",
            type_name(lua, &value)
        )));
    }
    set_team(&mut *world_mut(lua)?, player, team);
    Ok(())
}

/// What the runtime keeps between frames: respawns on their way, the
/// ForceFields that will go, and whether Tab has hidden the leaderboard.
#[derive(Default)]
pub struct Players {
    /// Each player waiting to respawn, and the seconds left.
    respawns: Vec<(InstanceId, f64)>,
    /// Each spawn ForceField, and the seconds it has left.
    shields: Vec<(InstanceId, f64)>,
    pub leaderboard_hidden: bool,
    tab_held: bool,
    /// Whether the first step has given the player their character.
    pub started: bool,
}

impl Players {
    /// Takes what [`load_character`] did: a player who got a character
    /// isn't waiting to respawn any more.
    pub fn note(&mut self, spawned: Spawned) -> Vec<PlayerEvent> {
        self.shields.extend(spawned.shields);
        for event in &spawned.events {
            if let PlayerEvent::CharacterAdded { player, .. } = *event {
                self.respawns.retain(|&(waiting, _)| waiting != player);
            }
        }
        spawned.events
    }

    /// The Humanoid `humanoid` died: if it's a player's character's, that
    /// player respawns after `Players.RespawnTime`, while
    /// `CharacterAutoLoads` is on.
    pub fn died(&mut self, world: &World, humanoid: InstanceId) {
        let Some(character) = world.get(humanoid).and_then(|instance| instance.parent()) else {
            return;
        };
        let Some(player) = player_of(world, character) else {
            return;
        };
        let service = world.players();
        if !flag(world, service, "CharacterAutoLoads", true)
            || self.respawns.iter().any(|&(waiting, _)| waiting == player)
        {
            return;
        }
        let wait = number(world, service, "RespawnTime").max(0.0);
        self.respawns.push((player, wait));
    }

    /// Moves the clocks on by `seconds`: ForceFields whose time is up are
    /// destroyed, and the players whose respawn is due are returned.
    pub fn step(&mut self, world: &mut World, seconds: f64) -> Vec<InstanceId> {
        let dt = if seconds.is_finite() {
            seconds.max(0.0)
        } else {
            0.0
        };
        for (field, left) in &mut self.shields {
            *left -= dt;
            if *left <= 0.0 && world.contains(*field) {
                let _ = world.destroy(*field);
            }
        }
        self.shields.retain(|&(_, left)| left > 0.0);
        let mut due = Vec::new();
        for (player, left) in &mut self.respawns {
            *left -= dt;
            if *left <= 0.0 {
                due.push(*player);
            }
        }
        self.respawns.retain(|&(_, left)| left > 0.0);
        due.retain(|&player| world.contains(player));
        due
    }

    /// Tab (pressed, not held) shows or hides the leaderboard.
    pub fn follow_tab(&mut self, tab_down: bool) {
        if tab_down && !self.tab_held {
            self.leaderboard_hidden = !self.leaderboard_hidden;
        }
        self.tab_held = tab_down;
    }
}
