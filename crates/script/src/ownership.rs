//! Network ownership (Phase 66): which device simulates an unanchored
//! assembly. Server Scripts call `part:SetNetworkOwner(player or nil)`,
//! `GetNetworkOwner()`, `SetNetworkOwnershipAuto()` and
//! `GetNetworkOwnershipAuto()`; the setting belongs to the part's
//! assembly (its root part), so an assembly has one owner.
//!
//! The server's host asks [`owned_parts`] each frame: for an assembly set
//! to a player, that player; for one set to Auto, the player in a
//! VehicleSeat of it, else the nearest player's character within
//! [`AUTO_DISTANCE`]. An assembly never set has the player in its
//! VehicleSeat, else the server.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};
use axcenity_physics::assembly;
use mlua::{Function, Lua};
use rustc_hash::FxHashMap;

use crate::instance::{LuaInstance, destroyed, world};
use crate::types::type_name;

/// Auto ownership goes to the nearest player whose character is this
/// close, in studs.
pub const AUTO_DISTANCE: f32 = 40.0;

/// What a script set an assembly's owner to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    Server,
    Player(InstanceId),
    Auto,
}

/// Each assembly's setting, by its root part, and who owns it now.
#[derive(Clone, Default)]
pub struct SharedOwnership(pub Rc<RefCell<Ownership>>);

#[derive(Default)]
pub struct Ownership {
    settings: BTreeMap<InstanceId, Owner>,
    /// Who owns each assembly now (by root part), as last worked out.
    current: BTreeMap<InstanceId, InstanceId>,
    /// The Workspace's parts as last read.
    parts: PartCache,
    /// The assemblies as last worked out.
    grouped: Grouped,
}

/// The unanchored parts and welds assemblies were last worked out from,
/// and those assemblies.
type Grouped = (
    Vec<InstanceId>,
    Vec<(InstanceId, InstanceId)>,
    Vec<Vec<InstanceId>>,
);

/// The Workspace's parts and whether each is unanchored, so a frame
/// rereads only the parts that changed: a streaming world has tens of
/// thousands, nearly all anchored (Phase 80).
#[derive(Default)]
struct PartCache {
    /// The Workspace's structure version the list is up to date with.
    workspace: Option<u64>,
    /// Each part, its revision when read, and whether it was unanchored
    /// then.
    parts: FxHashMap<InstanceId, (u64, bool)>,
}

impl PartCache {
    /// The unanchored parts, by ID.
    fn unanchored(&mut self, world: &World) -> Vec<InstanceId> {
        let workspace = world.workspace();
        let version = world.structure_version_of(workspace);
        if self.workspace != Some(version) {
            // Catch up from what moved, as physics does; read everything
            // again when that isn't known.
            let moves = self
                .workspace
                .and_then(|before| world.workspace_moves_since(before));
            match moves {
                Some(moves) => {
                    // What left (or was destroyed with what it's in) goes.
                    self.parts
                        .retain(|&id, _| world.is_descendant_of(id, workspace));
                    for top in moves {
                        if !world.is_descendant_of(top, workspace) {
                            continue;
                        }
                        for id in std::iter::once(top).chain(world.descendants_iter(top)) {
                            if let Some(part) = world.get(id)
                                && part.class().is_base_part()
                            {
                                self.parts
                                    .insert(id, (part.revision(), is_unanchored(part)));
                            }
                        }
                    }
                }
                None => {
                    self.parts = world
                        .descendants_iter(workspace)
                        .filter_map(|id| {
                            let part = world.get(id)?;
                            part.class()
                                .is_base_part()
                                .then(|| (id, (part.revision(), is_unanchored(part))))
                        })
                        .collect();
                }
            }
            self.workspace = Some(version);
        }
        let mut unanchored = Vec::new();
        for (&id, (revision, loose)) in &mut self.parts {
            if let Some(part) = world.get(id)
                && part.revision() != *revision
            {
                *revision = part.revision();
                *loose = is_unanchored(part);
            }
            if *loose {
                unanchored.push(id);
            }
        }
        // Assemblies don't depend on the order; comparing with last
        // frame's list does.
        unanchored.sort_unstable();
        unanchored
    }
}

fn is_unanchored(part: &axcenity_core::Instance) -> bool {
    part.property("Anchored").and_then(Value::as_bool) == Some(false)
}

fn ownership(lua: &Lua) -> mlua::Result<Rc<RefCell<Ownership>>> {
    lua.app_data_ref::<SharedOwnership>()
        .map(|shared| shared.0.clone())
        .ok_or_else(|| mlua::Error::runtime("network ownership isn't ready"))
}

/// The root part of `part`'s assembly, or why it has no owner.
fn root_of(world: &World, part: InstanceId, call: &str) -> mlua::Result<InstanceId> {
    let instance = world.get(part).ok_or_else(destroyed)?;
    if instance.property("Anchored").and_then(Value::as_bool) == Some(true) {
        return Err(mlua::Error::runtime(format!(
            "{call}: an anchored part has no network owner; the server keeps it still"
        )));
    }
    Ok(assembly::root_part(world, part))
}

pub fn methods(lua: &Lua) -> mlua::Result<Vec<(ClassName, &'static str, Function)>> {
    let set_owner = lua.create_function(|lua, (this, player): (LuaInstance, mlua::Value)| {
        let call = "SetNetworkOwner(player)";
        crate::sides::check(lua, "SetNetworkOwner")?;
        let wrong = || {
            mlua::Error::runtime(format!(
                "{call} needs a Player or nil, not {}",
                type_name(lua, &player)
            ))
        };
        let owner = match &player {
            mlua::Value::Nil => Owner::Server,
            mlua::Value::UserData(data) => {
                let id = data.borrow::<LuaInstance>().map_err(|_| wrong())?.0;
                let class = world(lua)?.get(id).map(|i| i.class());
                if class != Some(ClassName::Player) {
                    return Err(wrong());
                }
                Owner::Player(id)
            }
            _ => return Err(wrong()),
        };
        let root = root_of(&*world(lua)?, this.0, call)?;
        let shared = ownership(lua)?;
        let mut shared = shared.borrow_mut();
        shared.settings.insert(root, owner);
        match owner {
            Owner::Player(player) => {
                shared.current.insert(root, player);
            }
            _ => {
                shared.current.remove(&root);
            }
        }
        Ok(())
    })?;
    let get_owner = lua.create_function(|lua, this: LuaInstance| {
        crate::sides::check(lua, "GetNetworkOwner")?;
        let root = root_of(&*world(lua)?, this.0, "GetNetworkOwner()")?;
        Ok(ownership(lua)?
            .borrow()
            .current
            .get(&root)
            .copied()
            .map(LuaInstance))
    })?;
    let set_auto = lua.create_function(|lua, this: LuaInstance| {
        crate::sides::check(lua, "SetNetworkOwnershipAuto")?;
        let root = root_of(&*world(lua)?, this.0, "SetNetworkOwnershipAuto()")?;
        ownership(lua)?
            .borrow_mut()
            .settings
            .insert(root, Owner::Auto);
        Ok(())
    })?;
    let get_auto = lua.create_function(|lua, this: LuaInstance| {
        crate::sides::check(lua, "GetNetworkOwnershipAuto")?;
        let root = root_of(&*world(lua)?, this.0, "GetNetworkOwnershipAuto()")?;
        Ok(ownership(lua)?.borrow().settings.get(&root) == Some(&Owner::Auto))
    })?;
    let mut methods = Vec::new();
    for class in [
        ClassName::Part,
        ClassName::MeshPart,
        ClassName::UnionOperation,
        ClassName::Seat,
        ClassName::VehicleSeat,
    ] {
        methods.push((class, "SetNetworkOwner", set_owner.clone()));
        methods.push((class, "GetNetworkOwner", get_owner.clone()));
        methods.push((class, "SetNetworkOwnershipAuto", set_auto.clone()));
        methods.push((class, "GetNetworkOwnershipAuto", get_auto.clone()));
    }
    Ok(methods)
}

/// Where `player`'s character is (its PrimaryPart, else its first part).
fn character_position(world: &World, player: InstanceId) -> Option<Vector3> {
    let character = world.get(player)?.property("Character")?.as_instance()?;
    let root = world
        .get(character)?
        .property("PrimaryPart")
        .and_then(Value::as_instance)
        .or_else(|| {
            world
                .descendants_iter(character)
                .find(|&id| world.get(id).is_some_and(|i| i.class().is_base_part()))
        })?;
    world.get(root)?.property("Position")?.as_vector3()
}

/// The Player whose character the controller `controller` is in.
fn player_of_controller(world: &World, controller: InstanceId) -> Option<InstanceId> {
    let character = world.get(controller)?.parent()?;
    let players = world.get(world.players())?;
    players.children().iter().copied().find(|&player| {
        world
            .get(player)
            .and_then(|p| p.property("Character"))
            .and_then(Value::as_instance)
            == Some(character)
    })
}

/// Works out who owns each unanchored assembly now (see the module docs)
/// and returns every part each player owns.
pub fn owned_parts(
    shared: &SharedOwnership,
    world: &World,
) -> BTreeMap<InstanceId, Vec<InstanceId>> {
    let mut ownership = shared.0.borrow_mut();
    let unanchored = ownership.parts.unanchored(world);
    let welds = assembly::welds(world);
    // The same parts and welds make the same assemblies.
    if ownership.grouped.0 != unanchored || ownership.grouped.1 != welds {
        let groups = assembly::group(&unanchored, &welds);
        ownership.grouped = (unanchored, welds, groups);
    }
    let ownership = &mut *ownership;
    let groups = &ownership.grouped.2;
    let players: Vec<InstanceId> = world
        .get(world.players())
        .map(|service| {
            service
                .children()
                .iter()
                .copied()
                .filter(|&id| world.get(id).map(|i| i.class()) == Some(ClassName::Player))
                .collect()
        })
        .unwrap_or_default();
    let mut owned: BTreeMap<InstanceId, Vec<InstanceId>> = BTreeMap::new();
    let mut current = BTreeMap::new();
    for members in groups {
        let root = assembly::root(world, members);
        let setting = ownership.settings.get(&root).copied();
        let driver = members.iter().find_map(|&id| {
            let seat = world.get(id)?;
            if seat.class() != ClassName::VehicleSeat {
                return None;
            }
            player_of_controller(world, seat.property("Occupant")?.as_instance()?)
        });
        let owner = match setting {
            Some(Owner::Server) => None,
            Some(Owner::Player(player)) => Some(player).filter(|&p| world.contains(p)),
            Some(Owner::Auto) => driver.or_else(|| {
                let at = world.get(root)?.property("Position")?.as_vector3()?;
                players
                    .iter()
                    .filter_map(|&player| {
                        let p = character_position(world, player)?;
                        let d =
                            ((p.x - at.x).powi(2) + (p.y - at.y).powi(2) + (p.z - at.z).powi(2))
                                .sqrt();
                        (d <= AUTO_DISTANCE).then_some((d, player))
                    })
                    .min_by(|a, b| a.0.total_cmp(&b.0))
                    .map(|(_, player)| player)
            }),
            None => driver,
        };
        if let Some(owner) = owner {
            current.insert(root, owner);
            owned
                .entry(owner)
                .or_default()
                .extend(members.iter().copied());
        }
    }
    // Settings for assemblies that are gone go with them.
    ownership.settings.retain(|root, _| world.contains(*root));
    ownership.current = current;
    owned
}
