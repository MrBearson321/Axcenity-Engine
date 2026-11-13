//! Data layers in a running game (roadmap R16.4, decision D5 of
//! `docs/research/world-regions-r16.md`).
//!
//! A `DataLayer`'s members are the Instances tagged with its `Name`. A
//! switched-off layer's members are **parked**: the topmost tagged Instance
//! of each subtree moves inside the `DataLayer` itself, where nothing is
//! simulated, drawn or sent to players ([`World::is_server_only`]).
//! Switching it on again puts each one back under its old parent, at its
//! old place. An Instance tagged with several layers stays parked while any
//! of them is off.
//!
//! The layer's `Active` property says whether it is on; scripts hear the
//! change as `DataLayerService.LayerChanged` on every device, because the
//! property replicates. Only the server switches layers.

use crate::{ClassName, InstanceId, Value, World};

/// The `DataLayer` named `name`, if the world has one.
pub fn layer(world: &World, name: &str) -> Option<InstanceId> {
    world
        .get(world.data_layer_service())?
        .children()
        .iter()
        .copied()
        .find(|&id| {
            world
                .get(id)
                .is_some_and(|layer| layer.class() == ClassName::DataLayer && layer.name() == name)
        })
}

/// Every `DataLayer`, in order.
pub fn layers(world: &World) -> Vec<InstanceId> {
    world
        .get(world.data_layer_service())
        .map(|service| {
            service
                .children()
                .iter()
                .copied()
                .filter(|&id| {
                    world
                        .get(id)
                        .is_some_and(|i| i.class() == ClassName::DataLayer)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn bool_property(world: &World, id: InstanceId, name: &str) -> bool {
    !matches!(
        world.get(id).and_then(|i| i.property(name)),
        Some(Value::Bool(false))
    )
}

/// True if the layer `layer` is on.
pub fn is_active(world: &World, layer: InstanceId) -> bool {
    bool_property(world, layer, "Active")
}

/// Why a layer couldn't be switched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoSuchLayer(pub String);

impl std::fmt::Display for NoSuchLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "there is no data layer named \"{}\" in DataLayerService",
            self.0
        )
    }
}

impl std::error::Error for NoSuchLayer {}

/// Switches the layer `name` on or off, parking or putting back its
/// members. Returns true if it changed.
pub fn set_active(world: &mut World, name: &str, active: bool) -> Result<bool, NoSuchLayer> {
    let id = layer(world, name).ok_or_else(|| NoSuchLayer(name.to_owned()))?;
    if is_active(world, id) == active {
        return Ok(false);
    }
    let _ = world.set_property(id, "Active", Value::Bool(active));
    if active {
        put_back(world);
    } else {
        park(world, id);
    }
    Ok(true)
}

/// As a game starts: each layer is switched to its `ActiveOnStart`, so
/// the switched-off ones start parked. Hosts call it on the server (or a
/// game with no network) before any script runs.
pub fn start(world: &mut World) {
    for id in layers(world) {
        let on = bool_property(world, id, "ActiveOnStart");
        let _ = world.set_property(id, "Active", Value::Bool(on));
    }
    park_inactive(world);
}

/// Parks what switched-off layers hold that isn't parked yet, such as the
/// members a level just brought in.
pub fn park_inactive(world: &mut World) {
    for id in layers(world) {
        if !is_active(world, id) {
            park(world, id);
        }
    }
}

/// True if `id` is inside a `DataLayer` (parked, or inside something that
/// is).
fn is_inside_a_layer(world: &World, id: InstanceId) -> bool {
    world
        .get(id)
        .and_then(|i| i.parent())
        .is_some_and(|parent| {
            parent != world.data_layer_service()
                && world.is_descendant_of(parent, world.data_layer_service())
        })
}

/// Moves the topmost members of `layer` into it, remembering where each was.
fn park(world: &mut World, layer: InstanceId) {
    forget_destroyed(world);
    let Some(name) = world.get(layer).map(|l| l.name().to_owned()) else {
        return;
    };
    let tagged: Vec<InstanceId> = world.tagged_iter(&name).collect();
    // The topmost of each tagged subtree; one already parked by another
    // layer stays where it is.
    let mut topmost: Vec<InstanceId> = tagged
        .iter()
        .copied()
        .filter(|&id| {
            !world.parked().contains_key(&id)
                && !is_inside_a_layer(world, id)
                && !tagged
                    .iter()
                    .any(|&other| world.is_descendant_of(id, other))
                && world.get(id).is_some_and(|i| !i.class().is_service())
        })
        .collect();
    // Places are read before anything moves, so siblings parked together
    // keep their order when they come back.
    let places: Vec<(InstanceId, (InstanceId, usize))> = topmost
        .drain(..)
        .filter_map(|id| {
            let parent = world.get(id)?.parent()?;
            let index = world
                .get(parent)?
                .children()
                .iter()
                .position(|&c| c == id)?;
            Some((id, (parent, index)))
        })
        .collect();
    for (id, place) in places {
        if world.set_parent(id, Some(layer)).is_ok() {
            world.parked_mut().insert(id, place);
        }
    }
}

/// Puts back every parked Instance none of whose layers is off, in the
/// order of their old places. One whose old parent is gone goes with it.
fn put_back(world: &mut World) {
    let off: Vec<String> = layers(world)
        .into_iter()
        .filter(|&id| !is_active(world, id))
        .filter_map(|id| world.get(id).map(|l| l.name().to_owned()))
        .collect();
    let mut back: Vec<(InstanceId, (InstanceId, usize))> = world
        .parked()
        .iter()
        .filter(|&(&id, _)| {
            world
                .get(id)
                .is_some_and(|i| !off.iter().any(|name| i.has_tag(name)))
        })
        .map(|(&id, &place)| (id, place))
        .collect();
    back.sort_by_key(|&(id, (parent, index))| (parent, index, id));
    for (id, (parent, index)) in back {
        world.parked_mut().remove(&id);
        let returned = world.contains(parent) && world.set_parent_at(id, parent, index).is_ok();
        if !returned {
            let _ = world.destroy(id);
        }
    }
    forget_destroyed(world);
}

/// Forgets parked Instances a script destroyed (or that went with their
/// layer).
fn forget_destroyed(world: &mut World) {
    let stale: Vec<InstanceId> = world
        .parked()
        .keys()
        .copied()
        .filter(|&id| !world.contains(id))
        .collect();
    for id in stale {
        world.parked_mut().remove(&id);
    }
}

/// Forgets (and destroys) what's parked from inside `ancestor`, such as
/// the Workspace a level replaces.
pub fn drop_parked_from(world: &mut World, ancestor: InstanceId) {
    let gone: Vec<InstanceId> = world
        .parked()
        .iter()
        .filter(|&(_, &(parent, _))| parent == ancestor || world.is_descendant_of(parent, ancestor))
        .map(|(&id, _)| id)
        .collect();
    for id in gone {
        world.parked_mut().remove(&id);
        let _ = world.destroy(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(world: &mut World, class: ClassName, parent: InstanceId, name: &str) -> InstanceId {
        let id = world.create(class).unwrap();
        world.set_name(id, name).unwrap();
        world.set_parent(id, Some(parent)).unwrap();
        id
    }

    fn children(world: &World, id: InstanceId) -> Vec<InstanceId> {
        world.get(id).unwrap().children().to_vec()
    }

    #[test]
    fn parking_and_putting_back_keeps_parents_and_order() {
        let mut world = World::new();
        let service = world.data_layer_service();
        let night = make(&mut world, ClassName::DataLayer, service, "Night");
        let workspace = world.workspace();
        let a = make(&mut world, ClassName::Part, workspace, "A");
        let lamps = make(&mut world, ClassName::Model, workspace, "Lamps");
        let lamp = make(&mut world, ClassName::Part, lamps, "Lamp");
        let b = make(&mut world, ClassName::Part, workspace, "B");
        let moon = make(&mut world, ClassName::Part, workspace, "Moon");
        let c = make(&mut world, ClassName::Part, workspace, "C");
        for id in [lamps, lamp, moon] {
            world.add_tag(id, "Night").unwrap();
        }
        let before = children(&world, workspace);

        assert_eq!(set_active(&mut world, "Night", false), Ok(true));
        assert_eq!(children(&world, workspace), [a, b, c]);
        // The Model went with its tagged Lamp inside it.
        assert_eq!(children(&world, night), [lamps, moon]);
        assert_eq!(children(&world, lamps), [lamp]);
        assert!(!is_active(&world, night));
        assert!(world.is_server_only(lamp) && world.is_server_only(moon));
        assert!(
            !world.is_server_only(night),
            "the layer itself goes to players"
        );
        assert_eq!(set_active(&mut world, "Night", false), Ok(false));

        assert_eq!(set_active(&mut world, "Night", true), Ok(true));
        assert_eq!(children(&world, workspace), before);
        assert!(children(&world, night).is_empty());
        assert!(world.parked().is_empty());
        assert_eq!(
            set_active(&mut world, "Day", true),
            Err(NoSuchLayer("Day".to_owned()))
        );
    }

    #[test]
    fn an_instance_in_two_layers_stays_parked_while_either_is_off() {
        let mut world = World::new();
        let service = world.data_layer_service();
        make(&mut world, ClassName::DataLayer, service, "Night");
        make(&mut world, ClassName::DataLayer, service, "Winter");
        let workspace = world.workspace();
        let snowman = make(&mut world, ClassName::Part, workspace, "Snowman");
        world.add_tag(snowman, "Night").unwrap();
        world.add_tag(snowman, "Winter").unwrap();

        set_active(&mut world, "Night", false).unwrap();
        set_active(&mut world, "Winter", false).unwrap();
        set_active(&mut world, "Night", true).unwrap();
        assert!(world.is_server_only(snowman), "Winter is still off");
        set_active(&mut world, "Winter", true).unwrap();
        assert_eq!(world.get(snowman).unwrap().parent(), Some(workspace));
    }

    #[test]
    fn a_game_starts_with_each_layer_as_set_and_a_level_drops_what_it_parked() {
        let mut world = World::new();
        let service = world.data_layer_service();
        let night = make(&mut world, ClassName::DataLayer, service, "Night");
        let day = make(&mut world, ClassName::DataLayer, service, "Day");
        world
            .set_property(night, "ActiveOnStart", Value::Bool(false))
            .unwrap();
        let workspace = world.workspace();
        let moon = make(&mut world, ClassName::Part, workspace, "Moon");
        world.add_tag(moon, "Night").unwrap();
        let sun = make(&mut world, ClassName::Part, workspace, "Sun");
        world.add_tag(sun, "Day").unwrap();

        start(&mut world);
        assert!(!is_active(&world, night) && is_active(&world, day));
        assert_eq!(children(&world, workspace), [sun]);

        // A level replacing the Workspace: what its old one parked goes,
        // and the new one's members are parked.
        drop_parked_from(&mut world, workspace);
        assert!(!world.contains(moon));
        let new_moon = make(&mut world, ClassName::Part, workspace, "Moon");
        world.add_tag(new_moon, "Night").unwrap();
        park_inactive(&mut world);
        assert_eq!(world.get(new_moon).unwrap().parent(), Some(night));
        set_active(&mut world, "Night", true).unwrap();
        assert_eq!(children(&world, workspace), [sun, new_moon]);
    }

    #[test]
    fn one_whose_old_parent_is_gone_goes_with_it() {
        let mut world = World::new();
        let service = world.data_layer_service();
        make(&mut world, ClassName::DataLayer, service, "Night");
        let workspace = world.workspace();
        let house = make(&mut world, ClassName::Model, workspace, "House");
        let lamp = make(&mut world, ClassName::Part, house, "Lamp");
        world.add_tag(lamp, "Night").unwrap();
        set_active(&mut world, "Night", false).unwrap();
        world.destroy(house).unwrap();
        set_active(&mut world, "Night", true).unwrap();
        assert!(!world.contains(lamp));
        assert!(world.parked().is_empty());
    }
}
