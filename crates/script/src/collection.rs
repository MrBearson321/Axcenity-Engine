//! Attributes and tags (Phase 29).
//!
//! Every Instance has `SetAttribute`, `GetAttribute`, `GetAttributes`,
//! `GetAttributeChangedSignal` (and the `AttributeChanged` event), plus the
//! tag shortcuts `AddTag`, `RemoveTag`, `HasTag` and `GetTags`.
//! `game:GetService("CollectionService")` has the same tag methods taking
//! the Instance first, and `GetTagged`, `GetAllTags`,
//! `GetInstanceAddedSignal(tag)` and `GetInstanceRemovedSignal(tag)`.
//!
//! An Instance counts as tagged while it is in the game (inside `game`),
//! as Roblox's do. So the added and removed signals fire when a tagged
//! Instance joins or leaves the game (a clone parented to the Workspace,
//! a `Destroy`) as well as when the tag itself is added or removed. Once a
//! tag has a signal, this side keeps the list of Instances it last saw
//! with it ([`Watched`]) and, after each round of change events, fires
//! the difference ([`fire_tag_changes`]). `Destroy` fires its removals
//! itself, while the Instances can still be read ([`removing`]).

use std::collections::BTreeMap;

use axcenity_core::{Change, InstanceId, RecordedChange};
use mlua::{Lua, MetaMethod, UserData, UserDataMethods};

use crate::events::{self, Event, LuaSignal, Owner};
use crate::instance::{LuaInstance, destroyed, world, world_mut};
use crate::types::{attribute_from_lua, type_name, value_to_lua};

/// The tag methods every Instance has, and its attribute methods.
pub fn add_methods<M: UserDataMethods<LuaInstance>>(methods: &mut M) {
    methods.add_method(
        "SetAttribute",
        |lua, this, (name, value): (String, mlua::Value)| {
            let value = attribute_from_lua(lua, &value).map_err(|message| {
                mlua::Error::runtime(format!("SetAttribute(\"{name}\", ...): {message}"))
            })?;
            world_mut(lua)?
                .set_attribute(this.0, &name, value)
                .map_err(|err| mlua::Error::runtime(format!("SetAttribute: {err}")))
        },
    );
    methods.add_method("GetAttribute", |lua, this, name: String| {
        let value = {
            let world = world(lua)?;
            let instance = world.get(this.0).ok_or_else(destroyed)?;
            instance.attribute(&name).cloned()
        };
        match value {
            Some(value) => value_to_lua(lua, &value),
            None => Ok(mlua::Value::Nil),
        }
    });
    methods.add_method("GetAttributes", |lua, this, ()| {
        let attributes = {
            let world = world(lua)?;
            world
                .get(this.0)
                .ok_or_else(destroyed)?
                .attributes()
                .clone()
        };
        let table = lua.create_table()?;
        for (name, value) in attributes {
            table.set(name, value_to_lua(lua, &value)?)?;
        }
        Ok(table)
    });
    methods.add_method("GetAttributeChangedSignal", |lua, this, name: String| {
        if !world(lua)?.contains(this.0) {
            return Err(destroyed());
        }
        Ok(LuaSignal {
            owner: Owner::Instance(this.0),
            event: Event::AttributeChanged(events::name_id(lua, &name)),
        })
    });
    methods.add_method("AddTag", |lua, this, tag: mlua::Value| {
        add_tag(lua, this.0, tag)
    });
    methods.add_method("RemoveTag", |lua, this, tag: mlua::Value| {
        remove_tag(lua, this.0, tag)
    });
    methods.add_method("HasTag", |lua, this, tag: mlua::Value| {
        has_tag(lua, this.0, tag)
    });
    methods.add_method("GetTags", |lua, this, ()| get_tags(lua, this.0));
}

fn tag_arg(lua: &Lua, value: mlua::Value, method: &str) -> mlua::Result<String> {
    match value {
        mlua::Value::String(tag) => Ok(tag.to_string_lossy()),
        other => Err(mlua::Error::runtime(format!(
            "{method} needs a tag string, not a {}",
            type_name(lua, &other)
        ))),
    }
}

fn instance_arg(lua: &Lua, value: mlua::Value, method: &str) -> mlua::Result<InstanceId> {
    match mlua::FromLua::from_lua(value.clone(), lua) {
        Ok(LuaInstance(id)) => Ok(id),
        Err(_) => Err(mlua::Error::runtime(format!(
            "{method} needs an Instance first, not a {}",
            type_name(lua, &value)
        ))),
    }
}

fn add_tag(lua: &Lua, id: InstanceId, tag: mlua::Value) -> mlua::Result<()> {
    let tag = tag_arg(lua, tag, "AddTag")?;
    world_mut(lua)?.add_tag(id, &tag).map_err(|_| destroyed())?;
    Ok(())
}

fn remove_tag(lua: &Lua, id: InstanceId, tag: mlua::Value) -> mlua::Result<()> {
    let tag = tag_arg(lua, tag, "RemoveTag")?;
    world_mut(lua)?
        .remove_tag(id, &tag)
        .map_err(|_| destroyed())?;
    Ok(())
}

fn has_tag(lua: &Lua, id: InstanceId, tag: mlua::Value) -> mlua::Result<bool> {
    let tag = tag_arg(lua, tag, "HasTag")?;
    Ok(world(lua)?.get(id).ok_or_else(destroyed)?.has_tag(&tag))
}

fn get_tags(lua: &Lua, id: InstanceId) -> mlua::Result<Vec<String>> {
    Ok(world(lua)?.get(id).ok_or_else(destroyed)?.tags().to_vec())
}

/// `game:GetService("CollectionService")`. Both sides have one.
#[derive(Clone, Copy, Debug)]
pub struct LuaCollectionService;

impl UserData for LuaCollectionService {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method(
            "AddTag",
            |lua, _, (instance, tag): (mlua::Value, mlua::Value)| {
                add_tag(lua, instance_arg(lua, instance, "AddTag")?, tag)
            },
        );
        methods.add_method(
            "RemoveTag",
            |lua, _, (instance, tag): (mlua::Value, mlua::Value)| {
                remove_tag(lua, instance_arg(lua, instance, "RemoveTag")?, tag)
            },
        );
        methods.add_method(
            "HasTag",
            |lua, _, (instance, tag): (mlua::Value, mlua::Value)| {
                has_tag(lua, instance_arg(lua, instance, "HasTag")?, tag)
            },
        );
        methods.add_method("GetTags", |lua, _, instance: mlua::Value| {
            get_tags(lua, instance_arg(lua, instance, "GetTags")?)
        });
        methods.add_method("GetTagged", |lua, _, tag: mlua::Value| {
            let tag = tag_arg(lua, tag, "GetTagged")?;
            let tagged = world(lua)?.tagged(&tag);
            Ok(tagged.into_iter().map(LuaInstance).collect::<Vec<_>>())
        });
        methods.add_method("GetAllTags", |lua, _, ()| {
            Ok(world(lua)?.all_tags().into_keys().collect::<Vec<_>>())
        });
        methods.add_method("GetInstanceAddedSignal", |lua, _, tag: mlua::Value| {
            let tag = tag_arg(lua, tag, "GetInstanceAddedSignal")?;
            watch(lua, &tag)?;
            Ok(LuaSignal {
                owner: Owner::CollectionService,
                event: Event::TagAdded(events::name_id(lua, &tag)),
            })
        });
        methods.add_method("GetInstanceRemovedSignal", |lua, _, tag: mlua::Value| {
            let tag = tag_arg(lua, tag, "GetInstanceRemovedSignal")?;
            watch(lua, &tag)?;
            Ok(LuaSignal {
                owner: Owner::CollectionService,
                event: Event::TagRemoved(events::name_id(lua, &tag)),
            })
        });
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("CollectionService"));
    }
}

/// The Instances in the game with each tag that has a signal, as this side
/// last saw them, in [`World::tagged`](axcenity_core::World::tagged) order.
#[derive(Default)]
struct Watched(BTreeMap<String, Vec<InstanceId>>);

/// Starts keeping the list for `tag`, from the Instances that have it now.
fn watch(lua: &Lua, tag: &str) -> mlua::Result<()> {
    if lua.app_data_ref::<Watched>().is_none() {
        lua.set_app_data(Watched::default());
    }
    let watching = lua
        .app_data_ref::<Watched>()
        .is_some_and(|watched| watched.0.contains_key(tag));
    if !watching {
        let now = world(lua)?.tagged(tag);
        let mut watched = lua.app_data_mut::<Watched>().expect("made above");
        watched.0.insert(tag.to_owned(), now);
    }
    Ok(())
}

/// After the change events for `changes`: fires the added and removed
/// signals of each watched tag for the Instances that gained or lost it,
/// or joined or left the game with it.
pub fn fire_tag_changes(lua: &Lua, changes: &[RecordedChange]) {
    let Some(last) = changes.last() else {
        return;
    };
    let moved = changes.iter().any(|recorded| {
        matches!(
            recorded.change,
            Change::ChildAdded { .. } | Change::ChildRemoved { .. } | Change::Tag { .. }
        )
    });
    if !moved {
        return;
    }
    // Work out every difference first, then let go of the World and the
    // lists before any handler runs.
    let mut fired = Vec::new();
    {
        let Ok(world) = world(lua) else {
            return;
        };
        let Some(mut watched) = lua.app_data_mut::<Watched>() else {
            return;
        };
        for (tag, before) in &mut watched.0 {
            let now = world.tagged(tag);
            for &gone in before.iter().filter(|id| !now.contains(id)) {
                fired.push((tag.clone(), false, gone));
            }
            for &new in now.iter().filter(|id| !before.contains(id)) {
                fired.push((tag.clone(), true, new));
            }
            *before = now;
        }
    }
    for (tag, added, id) in fired {
        let Some(number) = events::known_name_id(lua, &tag) else {
            continue;
        };
        let event = if added {
            Event::TagAdded(number)
        } else {
            Event::TagRemoved(number)
        };
        let owner = Owner::CollectionService;
        events::fire_change(lua, owner, event, last.number, LuaInstance(id));
    }
}

/// Called by `Destroy` just before `going` leave the game: fires the
/// removed signal for each watched tag they have, while they can still be
/// read, and forgets them so the next round doesn't fire it again.
pub fn removing(lua: &Lua, going: &[InstanceId]) {
    let mut fired = Vec::new();
    {
        let Some(mut watched) = lua.app_data_mut::<Watched>() else {
            return;
        };
        for (tag, members) in &mut watched.0 {
            members.retain(|id| {
                let leaving = going.contains(id);
                if leaving {
                    fired.push((tag.clone(), *id));
                }
                !leaving
            });
        }
    }
    for (tag, id) in fired {
        if let Some(number) = events::known_name_id(lua, &tag) {
            let owner = Owner::CollectionService;
            events::fire_event(lua, owner, Event::TagRemoved(number), LuaInstance(id));
        }
    }
}

/// `AttributeChanged` (with the name) and the attribute's own signal.
pub fn fire_attribute_changed(lua: &Lua, instance: InstanceId, name: &str, number: u64) {
    let owner = Owner::Instance(instance);
    events::fire_change(
        lua,
        owner,
        Event::Named("AttributeChanged"),
        number,
        name.to_owned(),
    );
    if let Some(id) = events::known_name_id(lua, name) {
        events::fire_change(lua, owner, Event::AttributeChanged(id), number, ());
    }
}
