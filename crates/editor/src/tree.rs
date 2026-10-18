//! The rules for editing the Instance tree: what the Insert buttons and
//! menus make and where it goes, the default names of new Instances, and
//! which Instances can be deleted, duplicated, renamed or moved, and where to.
//!
//! Pure functions of the world, so the rules are easy to test. The app does
//! the changes, and records them for undo.

use axcenity_core::{ClassName, InstanceId, PartType, Value, Vector3, World};

/// Something the toolbar or the Explorer's Insert menu can add.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Insertable {
    Part(PartType),
    /// An imported model. Import Model makes these (with its `MeshId`), not
    /// the Insert buttons and menus.
    MeshPart,
    Model,
    Folder,
    RemoteEvent,
}

impl Insertable {
    /// The primitives, in toolbar order.
    pub const PARTS: [Insertable; 4] = [
        Insertable::Part(PartType::Block),
        Insertable::Part(PartType::Ball),
        Insertable::Part(PartType::Cylinder),
        Insertable::Part(PartType::Wedge),
    ];
    /// The classes that group other Instances.
    pub const GROUPS: [Insertable; 2] = [Insertable::Model, Insertable::Folder];

    /// What the buttons and menus call it. A Ball is a "Sphere", as in Studio.
    pub fn label(self) -> &'static str {
        match self {
            Insertable::Part(PartType::Block) => "Block",
            Insertable::Part(PartType::Ball) => "Sphere",
            Insertable::Part(PartType::Cylinder) => "Cylinder",
            Insertable::Part(PartType::Wedge) => "Wedge",
            Insertable::MeshPart => "MeshPart",
            Insertable::Model => "Model",
            Insertable::Folder => "Folder",
            Insertable::RemoteEvent => "RemoteEvent",
        }
    }

    pub fn class(self) -> ClassName {
        match self {
            Insertable::Part(_) => ClassName::Part,
            Insertable::MeshPart => ClassName::MeshPart,
            Insertable::Model => ClassName::Model,
            Insertable::Folder => ClassName::Folder,
            Insertable::RemoteEvent => ClassName::RemoteEvent,
        }
    }
}

/// The size a new part of each shape starts with, in studs. Blocks and
/// wedges are the default Part size; spheres are 2 across; cylinders are 4
/// long (along X) and 2 across, so they look like cylinders straight away.
fn new_part_size(shape: PartType) -> Vector3 {
    match shape {
        PartType::Block | PartType::Wedge => Vector3::new(4.0, 1.0, 2.0),
        PartType::Ball => Vector3::new(2.0, 2.0, 2.0),
        PartType::Cylinder => Vector3::new(4.0, 2.0, 2.0),
    }
}

/// Makes a new `what`, with no parent yet. A part gets its shape and size,
/// and is centered on `position`, and so is a MeshPart.
pub fn create(world: &mut World, what: Insertable, position: Vector3) -> InstanceId {
    let id = world
        .create(what.class())
        .expect("only non-services are insertable");
    let properties = match what {
        Insertable::Part(shape) => vec![
            ("Shape", Value::PartType(shape)),
            ("Size", Value::Vector3(new_part_size(shape))),
            ("Position", Value::Vector3(position)),
        ],
        Insertable::MeshPart => vec![("Position", Value::Vector3(position))],
        _ => Vec::new(),
    };
    for (name, value) in properties {
        world
            .set_property(id, name, value)
            .expect("every part has these properties");
    }
    id
}

/// Where a new `what` goes: into `near` if it's ReplicatedStorage, or a
/// Model or Folder in a place that's saved (see [`is_saved`]). Otherwise a
/// RemoteEvent goes into ReplicatedStorage (where both sides find it, as in
/// Roblox), and anything else into the Workspace.
pub fn insert_parent(world: &World, near: Option<InstanceId>, what: Insertable) -> InstanceId {
    let default = match what {
        Insertable::RemoteEvent => world.replicated_storage(),
        _ => world.workspace(),
    };
    near.filter(|&id| {
        let is_group = world.get(id).is_some_and(|instance| {
            matches!(instance.class(), ClassName::Model | ClassName::Folder)
        });
        id == world.replicated_storage() || (is_group && is_saved(world, id))
    })
    .unwrap_or(default)
}

/// True if `id` is in the Workspace or ReplicatedStorage: the only places
/// the world file saves.
pub fn is_saved(world: &World, id: InstanceId) -> bool {
    world.is_descendant_of(id, world.workspace())
        || world.is_descendant_of(id, world.replicated_storage())
}

/// A name that none of `parent`'s children has: `wanted` itself if it's
/// free, otherwise with the first free number from 2 on the end ("Part",
/// "Part2", "Part3"). A number already on the end is replaced, so a copy of
/// "Part2" is "Part3", not "Part22".
pub fn unique_name(world: &World, parent: InstanceId, wanted: &str) -> String {
    numbered_names(wanted)
        .find(|name| world.find_first_child(parent, name).is_none())
        .expect("some number is always free")
}

/// `wanted`, then `wanted` with 2, 3, 4… on the end, without end. A number
/// already on the end is replaced.
pub fn numbered_names(wanted: &str) -> impl Iterator<Item = String> {
    let stem = wanted.trim_end_matches(|c: char| c.is_ascii_digit());
    // A name that is all digits, such as "7", keeps them: "72", "73".
    let stem = if stem.is_empty() { wanted } else { stem }.to_string();
    std::iter::once(wanted.to_string()).chain((2u64..).map(move |number| format!("{stem}{number}")))
}

/// Whether `id` can be deleted, duplicated or moved, or why not, in words
/// for a tooltip.
pub fn check_editable(world: &World, id: InstanceId) -> Result<(), &'static str> {
    let instance = world.get(id).ok_or("It no longer exists.")?;
    if instance.class().is_service() {
        return Err("Services are always there, so they can't be deleted, copied or moved.");
    }
    if instance.class().is_script() {
        // A script is its `.lua` file, and its Instance always sits in its
        // service; copying it would mean copying the file.
        return Err("Scripts can't be copied or moved yet. Insert a new one instead.");
    }
    if instance.class() == ClassName::Player {
        return Err("Players only exist while the game runs.");
    }
    if !is_saved(world, id) {
        return Err(
            "Only things in the Workspace and ReplicatedStorage are saved, so only those can be changed.",
        );
    }
    Ok(())
}

/// Whether `id` can be deleted, or why not. Scripts can, unlike in
/// [`check_editable`]: deleting one deletes its file.
pub fn check_delete(world: &World, id: InstanceId) -> Result<(), &'static str> {
    match world.get(id) {
        Some(instance) if instance.class().is_script() => Ok(()),
        _ => check_editable(world, id),
    }
}

/// Whether `id` can be moved into `parent`, or why not.
pub fn check_move(world: &World, id: InstanceId, parent: InstanceId) -> Result<(), &'static str> {
    check_editable(world, id)?;
    if parent == id || world.is_descendant_of(parent, id) {
        return Err("Something can't go inside itself.");
    }
    if world
        .get(parent)
        .is_some_and(|parent| parent.class().is_script())
    {
        // A script is a file; what's inside it couldn't be saved.
        return Err("Scripts can't hold other things. Use a Folder or a Model.");
    }
    let saved_service = parent == world.workspace() || parent == world.replicated_storage();
    if !saved_service && !is_saved(world, parent) {
        // The world file only holds those two, so it would be lost on saving.
        return Err(
            "Only things in the Workspace and ReplicatedStorage are saved, so it has to stay in one of them.",
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn child(world: &mut World, parent: InstanceId, class: ClassName, name: &str) -> InstanceId {
        let id = world.create(class).unwrap();
        world.set_name(id, name).unwrap();
        world.set_parent(id, Some(parent)).unwrap();
        id
    }

    #[test]
    fn new_names_are_unique_among_siblings() {
        let mut world = World::new();
        let workspace = world.workspace();
        assert_eq!(unique_name(&world, workspace, "Part"), "Part");
        child(&mut world, workspace, ClassName::Part, "Part");
        assert_eq!(unique_name(&world, workspace, "Part"), "Part2");
        child(&mut world, workspace, ClassName::Part, "Part2");
        assert_eq!(unique_name(&world, workspace, "Part"), "Part3");
        // The number on the end is replaced, not added to.
        assert_eq!(unique_name(&world, workspace, "Part2"), "Part3");
        // Only siblings count: an empty Model has room for "Part".
        let model = child(&mut world, workspace, ClassName::Model, "Model");
        assert_eq!(unique_name(&world, model, "Part"), "Part");
        child(&mut world, workspace, ClassName::Part, "7");
        assert_eq!(unique_name(&world, workspace, "7"), "72");
    }

    #[test]
    fn each_primitive_gets_its_shape_size_and_position() {
        let mut world = World::new();
        let at = Vector3::new(1.0, 2.0, 3.0);
        for what in Insertable::PARTS {
            let id = create(&mut world, what, at);
            let part = world.get(id).unwrap();
            let Insertable::Part(shape) = what else {
                unreachable!()
            };
            assert_eq!(part.property("Shape"), Some(&Value::PartType(shape)));
            assert_eq!(part.property("Position"), Some(&Value::Vector3(at)));
            assert_eq!(
                part.property("Size"),
                Some(&Value::Vector3(new_part_size(shape)))
            );
            assert_eq!(part.parent(), None);
        }
        let model = create(&mut world, Insertable::Model, at);
        assert_eq!(world.get(model).unwrap().class(), ClassName::Model);
    }

    #[test]
    fn new_things_go_into_a_selected_model_or_folder_otherwise_the_workspace() {
        let mut world = World::new();
        let workspace = world.workspace();
        let folder = child(&mut world, workspace, ClassName::Folder, "Folder");
        let model = child(&mut world, folder, ClassName::Model, "Model");
        let part = child(&mut world, model, ClassName::Part, "Part");
        let block = Insertable::Part(PartType::Block);
        assert_eq!(insert_parent(&world, Some(folder), block), folder);
        assert_eq!(insert_parent(&world, Some(model), block), model);
        assert_eq!(insert_parent(&world, Some(part), block), workspace);
        assert_eq!(insert_parent(&world, None, block), workspace);
        assert_eq!(insert_parent(&world, Some(world.root()), block), workspace);
        // A Folder outside the saved places wouldn't be saved.
        let loose = world.create(ClassName::Folder).unwrap();
        assert_eq!(insert_parent(&world, Some(loose), block), workspace);
    }

    #[test]
    fn remote_events_go_into_replicated_storage_unless_a_group_is_selected() {
        let mut world = World::new();
        let storage = world.replicated_storage();
        let remote = Insertable::RemoteEvent;
        assert_eq!(insert_parent(&world, None, remote), storage);
        assert_eq!(
            insert_parent(&world, Some(world.workspace()), remote),
            storage
        );
        let remotes = child(&mut world, storage, ClassName::Folder, "Remotes");
        assert_eq!(insert_parent(&world, Some(remotes), remote), remotes);
        // Right-clicking ReplicatedStorage inserts anything there.
        let folder = Insertable::Folder;
        assert_eq!(insert_parent(&world, Some(storage), folder), storage);
        let id = create(&mut world, remote, Vector3::new(0.0, 0.0, 0.0));
        assert_eq!(world.get(id).unwrap().class(), ClassName::RemoteEvent);
    }

    #[test]
    fn moves_into_your_own_descendant_or_out_of_the_workspace_are_rejected() {
        let mut world = World::new();
        let workspace = world.workspace();
        let model = child(&mut world, workspace, ClassName::Model, "Car");
        let wheel = child(&mut world, model, ClassName::Part, "Wheel");
        let hub = child(&mut world, wheel, ClassName::Part, "Hub");
        let other = child(&mut world, workspace, ClassName::Folder, "Props");

        assert_eq!(check_move(&world, wheel, other), Ok(()));
        assert_eq!(check_move(&world, hub, workspace), Ok(()));
        assert!(check_move(&world, model, model).is_err());
        assert!(check_move(&world, model, wheel).is_err());
        assert!(check_move(&world, model, hub).is_err());
        assert!(check_move(&world, model, world.server_script_service()).is_err());
        assert!(check_move(&world, model, world.root()).is_err());
        assert!(check_move(&world, workspace, other).is_err());
        // ReplicatedStorage is saved too.
        let storage = world.replicated_storage();
        assert_eq!(check_move(&world, model, storage), Ok(()));
        let remotes = child(&mut world, storage, ClassName::Folder, "Remotes");
        assert_eq!(check_move(&world, wheel, remotes), Ok(()));
        assert_eq!(check_editable(&world, remotes), Ok(()));
    }

    #[test]
    fn services_cannot_be_deleted_copied_or_moved_and_scripts_can_only_be_deleted() {
        let mut world = World::new();
        let script_service = world.server_script_service();
        let script = child(&mut world, script_service, ClassName::Script, "door");
        let client = world.starter_player_scripts();
        let local_script = child(&mut world, client, ClassName::LocalScript, "keys");
        let services = [
            world.root(),
            world.workspace(),
            world.players(),
            world.replicated_storage(),
            script_service,
            client,
        ];
        for id in services.into_iter().chain([script, local_script]) {
            assert!(check_editable(&world, id).is_err());
        }
        for id in services {
            assert!(check_delete(&world, id).is_err());
        }
        assert_eq!(check_delete(&world, script), Ok(()));
        assert_eq!(check_delete(&world, local_script), Ok(()));
        let workspace = world.workspace();
        let part = child(&mut world, workspace, ClassName::Part, "Part");
        assert_eq!(check_editable(&world, part), Ok(()));
    }
}
