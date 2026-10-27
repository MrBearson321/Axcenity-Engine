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
    ScreenGui,
    Frame,
    TextLabel,
    TextButton,
    ImageLabel,
    TextBox,
    PageLayout,
    /// A Frame parented under a PageLayout, named as a page.
    Page,
    Table,
    ScrollingFrame,
    Slider,
    Toggle,
    Dropdown,
    ProgressBar,
    BillboardGui,
    Sound,
    PointLight,
    SpotLight,
    ParticleEmitter,
    Animator,
    Animation,
    /// A kinematic third-person character controller.
    ThirdPersonController,
    /// The same movement, with the camera at the character's eyes.
    FirstPersonController,
    /// Walks a character along the navigation mesh.
    NavigationAgent,
    /// A place a character is put back at.
    Checkpoint,
    /// An IntValue, StringValue or other Value object.
    Value(ClassName),
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
    /// UI objects, for the toolbar's UI group and the Explorer's Insert menu.
    pub const UI: [Insertable; 15] = [
        Insertable::ScreenGui,
        Insertable::Frame,
        Insertable::TextLabel,
        Insertable::TextButton,
        Insertable::ImageLabel,
        Insertable::TextBox,
        Insertable::PageLayout,
        Insertable::Page,
        Insertable::Table,
        Insertable::ScrollingFrame,
        Insertable::Slider,
        Insertable::Toggle,
        Insertable::Dropdown,
        Insertable::ProgressBar,
        Insertable::BillboardGui,
    ];
    pub const AUDIO: [Insertable; 1] = [Insertable::Sound];
    /// Lights shine from the part they are inside.
    pub const LIGHTS: [Insertable; 2] = [Insertable::PointLight, Insertable::SpotLight];
    /// Effects give off particles from the part they are inside.
    pub const EFFECTS: [Insertable; 1] = [Insertable::ParticleEmitter];
    /// Animators (skeletal) and Animations (property clips).
    pub const ANIMATION: [Insertable; 2] = [Insertable::Animator, Insertable::Animation];
    /// The character controllers: a character has one or the other.
    pub const CHARACTERS: [Insertable; 2] = [
        Insertable::ThirdPersonController,
        Insertable::FirstPersonController,
    ];
    /// Navigation agents, which walk a character to a destination.
    pub const AGENTS: [Insertable; 1] = [Insertable::NavigationAgent];
    /// Checkpoints, which a game returns the player character to.
    pub const CHECKPOINTS: [Insertable; 1] = [Insertable::Checkpoint];
    /// Value objects, which hold one value for scripts.
    pub const VALUES: [Insertable; 8] = [
        Insertable::Value(ClassName::IntValue),
        Insertable::Value(ClassName::NumberValue),
        Insertable::Value(ClassName::StringValue),
        Insertable::Value(ClassName::BoolValue),
        Insertable::Value(ClassName::Vector3Value),
        Insertable::Value(ClassName::Color3Value),
        Insertable::Value(ClassName::CFrameValue),
        Insertable::Value(ClassName::ObjectValue),
    ];

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
            Insertable::ScreenGui => "ScreenGui",
            Insertable::Frame => "Frame",
            Insertable::TextLabel => "TextLabel",
            Insertable::TextButton => "Button",
            Insertable::ImageLabel => "ImageLabel",
            Insertable::TextBox => "TextBox",
            Insertable::PageLayout => "PageLayout",
            Insertable::Page => "Page",
            Insertable::Table => "Table",
            Insertable::ScrollingFrame => "ScrollingFrame",
            Insertable::Slider => "Slider",
            Insertable::Toggle => "Toggle",
            Insertable::Dropdown => "Dropdown",
            Insertable::ProgressBar => "ProgressBar",
            Insertable::BillboardGui => "BillboardGui",
            Insertable::Sound => "Sound",
            Insertable::PointLight => "PointLight",
            Insertable::SpotLight => "SpotLight",
            Insertable::ParticleEmitter => "Particles",
            Insertable::Animator => "Animator",
            Insertable::Animation => "Animation",
            Insertable::ThirdPersonController => "Third Person",
            Insertable::FirstPersonController => "First Person",
            Insertable::NavigationAgent => "Agent",
            Insertable::Checkpoint => "Checkpoint",
            Insertable::Value(class) => class.as_str(),
        }
    }

    pub fn class(self) -> ClassName {
        match self {
            Insertable::Part(_) => ClassName::Part,
            Insertable::MeshPart => ClassName::MeshPart,
            Insertable::Model => ClassName::Model,
            Insertable::Folder => ClassName::Folder,
            Insertable::RemoteEvent => ClassName::RemoteEvent,
            Insertable::ScreenGui => ClassName::ScreenGui,
            Insertable::Frame => ClassName::Frame,
            Insertable::TextLabel => ClassName::TextLabel,
            Insertable::TextButton => ClassName::TextButton,
            Insertable::ImageLabel => ClassName::ImageLabel,
            Insertable::TextBox => ClassName::TextBox,
            Insertable::PageLayout => ClassName::PageLayout,
            Insertable::Page => ClassName::Frame,
            Insertable::Table => ClassName::Table,
            Insertable::ScrollingFrame => ClassName::ScrollingFrame,
            Insertable::Slider => ClassName::Slider,
            Insertable::Toggle => ClassName::Toggle,
            Insertable::Dropdown => ClassName::Dropdown,
            Insertable::ProgressBar => ClassName::ProgressBar,
            Insertable::BillboardGui => ClassName::BillboardGui,
            Insertable::Sound => ClassName::Sound,
            Insertable::PointLight => ClassName::PointLight,
            Insertable::SpotLight => ClassName::SpotLight,
            Insertable::ParticleEmitter => ClassName::ParticleEmitter,
            Insertable::Animator => ClassName::Animator,
            Insertable::Animation => ClassName::Animation,
            Insertable::ThirdPersonController => ClassName::ThirdPersonController,
            Insertable::FirstPersonController => ClassName::FirstPersonController,
            Insertable::NavigationAgent => ClassName::NavigationAgent,
            Insertable::Checkpoint => ClassName::Checkpoint,
            Insertable::Value(class) => class,
        }
    }

    /// UI that shows only inside a ScreenGui: frames, text, buttons and images.
    pub fn is_ui_element(self) -> bool {
        matches!(
            self,
            Insertable::Frame
                | Insertable::TextLabel
                | Insertable::TextButton
                | Insertable::ImageLabel
                | Insertable::TextBox
                | Insertable::PageLayout
                | Insertable::Page
                | Insertable::Table
                | Insertable::ScrollingFrame
                | Insertable::Slider
                | Insertable::Toggle
                | Insertable::Dropdown
                | Insertable::ProgressBar
        )
    }
}

/// True for the classes UI elements can go inside.
fn is_ui_container(world: &World, id: InstanceId) -> bool {
    matches!(
        world.get(id).map(|instance| instance.class()),
        Some(
            ClassName::ScreenGui
                | ClassName::Frame
                | ClassName::PageLayout
                | ClassName::Table
                | ClassName::ScrollingFrame
                | ClassName::BillboardGui
        )
    )
}

/// The ScreenGui a new UI element goes in when nothing UI is selected: the
/// one around `near`, or else the Workspace's first.
fn screen_gui_for(world: &World, near: Option<InstanceId>) -> Option<InstanceId> {
    let is_gui = |id: InstanceId| {
        world
            .get(id)
            .is_some_and(|instance| instance.class() == ClassName::ScreenGui)
    };
    let mut current = near;
    while let Some(id) = current {
        if is_gui(id) && is_saved(world, id) {
            return Some(id);
        }
        current = world.get(id).and_then(|instance| instance.parent());
    }
    world
        .descendants(world.workspace())
        .into_iter()
        .find(|&id| is_gui(id))
}

/// The PageLayout a new page Frame should join: `near` itself, the one
/// that contains it, or else the Workspace's first.
pub fn page_layout_for(world: &World, near: Option<InstanceId>) -> Option<InstanceId> {
    let mut current = near;
    while let Some(id) = current {
        if world
            .get(id)
            .is_some_and(|instance| instance.class() == ClassName::PageLayout)
            && is_saved(world, id)
        {
            return Some(id);
        }
        current = world.get(id).and_then(|instance| instance.parent());
    }
    world
        .descendants(world.workspace())
        .into_iter()
        .find(|&id| {
            world
                .get(id)
                .is_some_and(|instance| instance.class() == ClassName::PageLayout)
        })
}

/// True when a new light would be parented to a part, which is the only
/// place a light is drawn.
pub fn light_has_part_parent(world: &World, near: Option<InstanceId>) -> bool {
    world
        .get(insert_parent(world, near, Insertable::PointLight))
        .is_some_and(|instance| matches!(instance.class(), ClassName::Part | ClassName::MeshPart))
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
        // Scales with the screen, laid out over all of it, so a UI made at
        // one size looks the same at any other.
        Insertable::ScreenGui => vec![("ScaleMode", Value::String("Scale".to_owned()))],
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
/// Model or Folder in a place that's saved (see [`is_saved`]), or for an
/// Animation, a part or a ScreenGui or frame it can animate. Otherwise a
/// RemoteEvent goes into ReplicatedStorage (where both sides find it, as in
/// Roblox), and anything else into the Workspace.
pub fn insert_parent(world: &World, near: Option<InstanceId>, what: Insertable) -> InstanceId {
    let default = match what {
        Insertable::RemoteEvent => world.replicated_storage(),
        what if what.is_ui_element() => screen_gui_for(world, near).unwrap_or(world.workspace()),
        _ => world.workspace(),
    };
    near.filter(|&id| {
        let is_group = world.get(id).is_some_and(|instance| {
            matches!(instance.class(), ClassName::Model | ClassName::Folder)
        });
        let is_ui_container = is_ui_container(world, id);
        let is_sound_container = matches!(
            world.get(id).map(|instance| instance.class()),
            Some(ClassName::Part | ClassName::MeshPart | ClassName::Model)
        );
        id == world.replicated_storage()
            || (is_group && is_saved(world, id))
            // A Value object goes inside whatever is selected.
            || (matches!(what, Insertable::Value(_)) && is_saved(world, id))
            || (is_ui_container
                && (what.is_ui_element() || what == Insertable::Animation)
                && is_saved(world, id))
            || (is_sound_container
                && matches!(
                    what,
                    Insertable::Sound
                        | Insertable::PointLight
                        | Insertable::SpotLight
                        | Insertable::ParticleEmitter
                        | Insertable::BillboardGui
                        | Insertable::Animator
                        | Insertable::Animation
                        | Insertable::ThirdPersonController
                        | Insertable::FirstPersonController
                        | Insertable::NavigationAgent
                )
                && is_saved(world, id))
    })
    .unwrap_or(default)
}

/// The middle of the top of the Part or MeshPart `id`, where a new
/// Checkpoint goes. `None` for anything else.
pub fn top_of(world: &World, id: Option<InstanceId>) -> Option<Vector3> {
    let instance = world.get(id?)?;
    if !matches!(instance.class(), ClassName::Part | ClassName::MeshPart) {
        return None;
    }
    let position = instance.property("Position")?.as_vector3()?;
    let size = instance.property("Size")?.as_vector3()?;
    Some(Vector3::new(
        position.x,
        position.y + size.y.abs() / 2.0,
        position.z,
    ))
}

/// True when a character controller already moves `id`: one is inside it,
/// or inside a Model or part that holds it.
pub fn has_controller(world: &World, id: InstanceId) -> bool {
    let is_controller = |id: InstanceId| {
        world
            .get(id)
            .is_some_and(|instance| instance.class().is_character_controller())
    };
    if world.descendants(id).into_iter().any(is_controller) {
        return true;
    }
    let mut holder = world.get(id).and_then(|instance| instance.parent());
    while let Some(parent) = holder {
        let Some(instance) = world.get(parent) else {
            break;
        };
        if instance
            .children()
            .iter()
            .any(|&child| is_controller(child))
        {
            return true;
        }
        holder = instance.parent();
    }
    false
}

/// True if a new `what` needs a ScreenGui made first: it's UI, and there's
/// no ScreenGui or Frame for it to go in.
pub fn needs_screen_gui(world: &World, near: Option<InstanceId>, what: Insertable) -> bool {
    what.is_ui_element() && !is_ui_container(world, insert_parent(world, near, what))
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

/// True if `id` can go in a prefab: something in the Workspace that isn't
/// a script, a service or UI.
pub fn can_be_prefab(world: &World, id: InstanceId) -> bool {
    let Some(instance) = world.get(id) else {
        return false;
    };
    let class = instance.class();
    !class.is_service()
        && !class.is_script()
        && class != ClassName::ScreenGui
        && world.is_descendant_of(id, world.workspace())
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
    fn ui_insertions_nest_under_screen_guis_and_frames() {
        let mut world = World::new();
        let workspace = world.workspace();
        let gui = child(&mut world, workspace, ClassName::ScreenGui, "Hud");
        let frame = child(&mut world, gui, ClassName::Frame, "Frame");

        assert_eq!(insert_parent(&world, Some(gui), Insertable::Frame), gui);
        assert_eq!(
            insert_parent(&world, Some(frame), Insertable::TextLabel),
            frame
        );
        assert_eq!(
            insert_parent(&world, Some(frame), Insertable::ScreenGui),
            workspace
        );
        // With a part selected, or nothing, UI still goes in the ScreenGui.
        let part = child(&mut world, workspace, ClassName::Part, "Part");
        assert_eq!(
            insert_parent(&world, Some(part), Insertable::TextButton),
            gui
        );
        assert_eq!(insert_parent(&world, None, Insertable::ImageLabel), gui);
        assert!(!needs_screen_gui(&world, None, Insertable::Frame));
        assert!(needs_screen_gui(&World::new(), None, Insertable::Frame));
        assert!(!needs_screen_gui(
            &World::new(),
            None,
            Insertable::ScreenGui
        ));
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
