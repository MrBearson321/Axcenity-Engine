//! Tools and Inventories (Phase 45).
//!
//! A Tool is carried in an Inventory, as its child (see `core::tool`); the
//! Inventory's `Equipped` is the one in hand. Scripts' `Add`, `Remove`,
//! `Equip`, `UnequipAll` and `Drop` change the World at once and note the
//! Inventory; the runtime then compares each noted Inventory's `Equipped`
//! with what it last fired ([`Tools::sync`]) and fires `Unequipped` and
//! `Equipped` on both sides, so a save slot that loads another equipped
//! Tool fires them too. Each frame [`Tools::step`] picks up the dropped
//! Tools characters touch, follows the player's hotbar keys, Drop and
//! Primary, and [`place_held`] moves each Tool in hand to its character's
//! right hand.

use std::collections::{BTreeSet, HashMap};

use axcenity_anim::Poses;
use axcenity_assets::Assets;
use axcenity_core::tool::{equipped, grip, handle, inventory_holding, inventory_in, tool_of};
use axcenity_core::{ClassName, InstanceId, KeyCode, Parameter, Value, World};
use glam::{Quat, Vec3};
use mlua::{FromLua, Function, Lua};

use crate::cframe::{LuaCFrame, to_vec3};
use crate::input::PromptInput;
use crate::instance::{LuaInstance, destroyed, world_mut};
use crate::prompt::Fired;
use crate::types::type_name;

/// The Animator parameter a character's Tool in hand sets.
pub const EQUIPPED_PARAMETER: &str = "Equipped";

/// How far apart a character and a dropped Tool's Handle may be and still
/// touch, in studs, so one resting against the character counts.
const TOUCH_MARGIN: f32 = 0.25;

/// Studs between the character's front and a dropped Tool's Handle.
const DROP_GAP: f32 = 1.0;

/// The number keys that pick hotbar slots 1 to 10.
const SLOT_KEYS: [KeyCode; 10] = [
    KeyCode::One,
    KeyCode::Two,
    KeyCode::Three,
    KeyCode::Four,
    KeyCode::Five,
    KeyCode::Six,
    KeyCode::Seven,
    KeyCode::Eight,
    KeyCode::Nine,
    KeyCode::Zero,
];

/// What one side's scripts did to Inventories, until the runtime collects
/// it.
#[derive(Default)]
pub struct ToolChanges {
    /// Inventories whose `Equipped` may have changed.
    pub inventories: Vec<InstanceId>,
    /// Tools dropped, and the Inventory each was dropped from.
    pub dropped: Vec<(InstanceId, InstanceId)>,
}

impl ToolChanges {
    fn note(&mut self, inventory: InstanceId) {
        self.inventories.push(inventory);
    }
}

/// The Tools in `inventory`, in order: its hotbar's slots.
pub fn items(world: &World, inventory: InstanceId) -> Vec<InstanceId> {
    world.get(inventory).map_or_else(Vec::new, |instance| {
        instance
            .children()
            .iter()
            .copied()
            .filter(|&child| {
                world
                    .get(child)
                    .is_some_and(|c| c.class() == ClassName::Tool)
            })
            .collect()
    })
}

fn number(world: &World, id: InstanceId, name: &str, default: f64) -> f64 {
    world
        .get(id)
        .and_then(|instance| instance.property(name))
        .and_then(Value::as_number)
        .filter(|n| n.is_finite())
        .unwrap_or(default)
}

fn flag(world: &World, id: InstanceId, name: &str, default: bool) -> bool {
    world
        .get(id)
        .and_then(|instance| instance.property(name))
        .and_then(Value::as_bool)
        .unwrap_or(default)
}

fn is_class(world: &World, id: InstanceId, class: ClassName) -> bool {
    world
        .get(id)
        .is_some_and(|instance| instance.class() == class)
}

/// How many Tools `inventory` holds at most.
fn capacity(world: &World, inventory: InstanceId) -> usize {
    number(world, inventory, "Capacity", 10.0).max(0.0) as usize
}

/// Puts `tool` in `inventory` if there's room, taking it out of any other
/// first. False when it's full; true if it's in it already. Notes each
/// Inventory changed in `changes`.
pub fn add(
    world: &mut World,
    inventory: InstanceId,
    tool: InstanceId,
    changes: &mut ToolChanges,
) -> Result<bool, String> {
    let holding = inventory_holding(world, tool);
    if holding == Some(inventory) {
        return Ok(true);
    }
    if items(world, inventory).len() >= capacity(world, inventory) {
        return Ok(false);
    }
    if let Some(other) = holding {
        remove(world, other, tool, changes);
    }
    world
        .set_parent(tool, Some(inventory))
        .map_err(|err| err.to_string())?;
    changes.note(inventory);
    Ok(true)
}

/// Takes `tool` out of `inventory` (unequipping it first), leaving it with
/// no parent. False if it wasn't in it.
pub fn remove(
    world: &mut World,
    inventory: InstanceId,
    tool: InstanceId,
    changes: &mut ToolChanges,
) -> bool {
    if inventory_holding(world, tool) != Some(inventory) {
        return false;
    }
    if equipped(world, inventory) == Some(tool) {
        set_equipped(world, inventory, None);
    }
    let _ = world.set_parent(tool, None);
    changes.note(inventory);
    true
}

/// Puts `tool`, which must be in `inventory`, in its character's right
/// hand, putting away the one there.
pub fn equip(
    world: &mut World,
    assets: &Assets,
    inventory: InstanceId,
    tool: InstanceId,
    changes: &mut ToolChanges,
) -> Result<(), String> {
    if inventory_holding(world, tool) != Some(inventory) {
        return Err(
            "Equip needs a Tool in this Inventory; put it in with inventory:Add(tool) first"
                .to_owned(),
        );
    }
    if equipped(world, inventory) != Some(tool) {
        set_equipped(world, inventory, Some(tool));
        place_in_hand(world, assets, None, inventory, tool);
        changes.note(inventory);
    }
    Ok(())
}

/// Puts away the Tool in hand, if there is one.
pub fn unequip_all(world: &mut World, inventory: InstanceId, changes: &mut ToolChanges) {
    if equipped(world, inventory).is_some() {
        set_equipped(world, inventory, None);
        changes.note(inventory);
    }
}

fn set_equipped(world: &mut World, inventory: InstanceId, tool: Option<InstanceId>) {
    let _ = world.set_property(inventory, "Equipped", Value::Instance(tool));
}

/// `tool:Drop()`: out of its Inventory (and the hand) and onto the ground
/// in front of its character. False if it can't be dropped
/// (`CanBeDropped` off) or isn't carried.
pub fn drop_tool(world: &mut World, tool: InstanceId, changes: &mut ToolChanges) -> bool {
    let Some(inventory) = inventory_holding(world, tool) else {
        return false;
    };
    if !flag(world, tool, "CanBeDropped", true) {
        return false;
    }
    if equipped(world, inventory) == Some(tool) {
        set_equipped(world, inventory, None);
    }
    let front = world
        .get(inventory)
        .and_then(|instance| instance.parent())
        .and_then(|character| front_of(world, character, tool));
    let workspace = world.workspace();
    if world.set_parent(tool, Some(workspace)).is_err() {
        return false;
    }
    if let Some(target) = front {
        let from = handle(world, tool)
            .and_then(|part| world.get(part))
            .and_then(LuaCFrame::of_instance)
            .map_or_else(|| tool_middle(world, tool), |cf| cf.position);
        let delta = LuaCFrame::new(target - from, Quat::IDENTITY);
        move_tool(world, tool, delta);
    }
    changes.note(inventory);
    changes.dropped.push((tool, inventory));
    true
}

/// Where a Tool's Handle goes when it's dropped by `character`: in front
/// of the character's main part, clear of it.
fn front_of(world: &World, character: InstanceId, tool: InstanceId) -> Option<Vec3> {
    let (root, half) = main_part(world, character)?;
    let reach = handle(world, tool)
        .and_then(|part| world.get(part))
        .and_then(|part| part.property("Size").and_then(Value::as_vector3))
        .map_or(1.0, |size| to_vec3(size).abs().length() * 0.5);
    // A CFrame looks along -Z.
    Some(root.point(Vec3::new(0.0, 0.0, -(half.z + reach + DROP_GAP))))
}

/// The middle of the box round a Tool's parts, for one with no Handle.
fn tool_middle(world: &World, tool: InstanceId) -> Vec3 {
    let parts: Vec<InstanceId> = world
        .descendants_iter(tool)
        .filter(|&id| world.get(id).is_some_and(|i| i.class().is_base_part()))
        .collect();
    bounds(world, &parts).map_or(Vec3::ZERO, |(min, max)| (min + max) * 0.5)
}

/// Moves every part in `tool` by `delta`.
fn move_tool(world: &mut World, tool: InstanceId, delta: LuaCFrame) {
    for id in world.descendants(tool) {
        // An Attachment's Position is on its part, so it moves with it.
        let placed = world
            .get(id)
            .filter(|instance| instance.class() != ClassName::Attachment)
            .and_then(LuaCFrame::of_instance);
        if let Some(cf) = placed {
            let _ = (delta * cf).write_to(world, id);
        }
    }
}

/// A character's biggest part, with half its size: the body the hand and
/// the drop are placed from when there's no hand bone or part.
fn main_part(world: &World, character: InstanceId) -> Option<(LuaCFrame, Vec3)> {
    axcenity_character::body_parts(world, character)
        .into_iter()
        .filter_map(|id| {
            let part = world.get(id)?;
            let size = to_vec3(part.property("Size")?.as_vector3()?).abs();
            Some((LuaCFrame::of_instance(part)?, size * 0.5))
        })
        .filter(|(cf, half)| cf.position.is_finite() && half.is_finite())
        .max_by(|a, b| {
            let volume = |half: Vec3| half.x * half.y * half.z;
            volume(a.1).total_cmp(&volume(b.1))
        })
}

/// Where a character's right hand is: its skinned MeshPart's right hand
/// bone (posed as `poses` has it), else a part named `RightHand` (its
/// middle) or `Right Arm` (its bottom), else beside its main part, at the
/// front of its right side.
fn hand(
    world: &World,
    assets: &Assets,
    poses: Option<&Poses>,
    character: InstanceId,
) -> Option<LuaCFrame> {
    let parts = axcenity_character::body_parts(world, character);
    for &id in &parts {
        let Some(part) = world.get(id) else { continue };
        if part.class() != ClassName::MeshPart {
            continue;
        }
        let mesh = part
            .property("MeshId")
            .and_then(Value::as_str)
            .unwrap_or("");
        let Some(rig) = assets
            .model_blocking(mesh)
            .and_then(|model| model.rig.clone())
        else {
            continue;
        };
        let Some(bone) = axcenity_anim::ragdoll::right_hand(&rig) else {
            continue;
        };
        let posed = poses.and_then(|poses| poses.get(&id)).map(|posed| &**posed);
        if let Some(matrix) = axcenity_anim::ragdoll::bone_in_world(world, id, &rig, posed, bone) {
            let (_, rotation, position) = matrix.to_scale_rotation_translation();
            return Some(LuaCFrame::new(position, rotation));
        }
    }
    for (name, down) in [("RightHand", 0.0), ("Right Arm", 0.5)] {
        let found = parts.iter().find_map(|&id| {
            let part = world.get(id).filter(|part| part.name() == name)?;
            let size = part.property("Size")?.as_vector3()?;
            Some((LuaCFrame::of_instance(part)?, size.y))
        });
        if let Some((cf, height)) = found {
            return Some(cf * LuaCFrame::new(Vec3::new(0.0, -height * down, 0.0), Quat::IDENTITY));
        }
    }
    let (root, half) = main_part(world, character)?;
    Some(
        root * LuaCFrame::new(
            Vec3::new(half.x + 0.5, 0.0, -(half.z + 0.5)),
            Quat::IDENTITY,
        ),
    )
}

/// Where a Tool is held from: its Grip in the world, else its Handle's
/// middle, else the middle of its parts.
fn grip_frame(world: &World, tool: InstanceId) -> LuaCFrame {
    let handle_frame = handle(world, tool)
        .and_then(|part| world.get(part))
        .and_then(LuaCFrame::of_instance);
    match (handle_frame, grip(world, tool)) {
        (Some(part), Some(attachment)) => {
            let offset = world
                .get(attachment)
                .and_then(LuaCFrame::of_instance)
                .unwrap_or(LuaCFrame::IDENTITY);
            part * offset
        }
        (Some(part), None) => part,
        _ => LuaCFrame::new(tool_middle(world, tool), Quat::IDENTITY),
    }
}

/// Moves the Tool in hand so its Grip is at the hand of `inventory`'s
/// character, turned as the hand is.
fn place_in_hand(
    world: &mut World,
    assets: &Assets,
    poses: Option<&Poses>,
    inventory: InstanceId,
    tool: InstanceId,
) {
    let Some(character) = world.get(inventory).and_then(|instance| instance.parent()) else {
        return;
    };
    let Some(hand) = hand(world, assets, poses, character) else {
        return;
    };
    let grip = grip_frame(world, tool);
    // Already there (to rounding): leave the parts as they are, so a still
    // character's Tool doesn't fire Changed every frame.
    if grip.position.distance(hand.position) < 1e-4
        && grip.rotation.dot(hand.rotation).abs() > 1.0 - 1e-7
    {
        return;
    }
    move_tool(world, tool, hand * grip.inverse());
}

/// Moves every Tool in hand in the Workspace to its character's right
/// hand, posed as `poses` has the characters.
pub fn place_held(world: &mut World, assets: &Assets, poses: &Poses) {
    for inventory in inventories(world) {
        if let Some(tool) = equipped(world, inventory) {
            place_in_hand(world, assets, Some(poses), inventory, tool);
        }
    }
}

/// Every Inventory in the Workspace.
fn inventories(world: &World) -> Vec<InstanceId> {
    world
        .descendants_iter(world.workspace())
        .filter(|&id| is_class(world, id, ClassName::Inventory))
        .collect()
}

/// The box round `parts` in the world, turned parts included.
fn bounds(world: &World, parts: &[InstanceId]) -> Option<(Vec3, Vec3)> {
    let mut min = Vec3::INFINITY;
    let mut max = Vec3::NEG_INFINITY;
    for &id in parts {
        let Some(part) = world.get(id) else { continue };
        let (Some(cf), Some(size)) = (
            LuaCFrame::of_instance(part),
            part.property("Size").and_then(Value::as_vector3),
        ) else {
            continue;
        };
        let half = to_vec3(size).abs() * 0.5;
        let turned = glam::Mat3::from_quat(cf.rotation);
        let extent = turned.x_axis.abs() * half.x
            + turned.y_axis.abs() * half.y
            + turned.z_axis.abs() * half.z;
        if cf.position.is_finite() && extent.is_finite() {
            min = min.min(cf.position - extent);
            max = max.max(cf.position + extent);
        }
    }
    min.is_finite().then_some((min, max))
}

fn overlap(a: (Vec3, Vec3), b: (Vec3, Vec3), margin: f32) -> bool {
    (a.0 - margin).cmple(b.1).all() && (b.0 - margin).cmple(a.1).all()
}

/// The player character's Inventory, which the hotbar shows and the
/// player's keys use.
pub fn player_inventory(world: &World) -> Option<InstanceId> {
    inventory_in(world, axcenity_character::player_character(world)?)
}

/// The Animator in `character` (not in a Tool) that has a Bool
/// [`EQUIPPED_PARAMETER`], set to whether a Tool is in hand.
fn set_equipped_parameter(world: &mut World, character: InstanceId, on: bool) {
    let animator = world
        .descendants_iter(character)
        .find(|&id| is_class(world, id, ClassName::Animator) && tool_of(world, id).is_none());
    if let Some(live) = animator.and_then(|id| world.animator_mut(id))
        && let Some(Parameter::Bool(value)) = live.parameters.get_mut(EQUIPPED_PARAMETER)
    {
        *value = on;
    }
}

/// What the runtime keeps between frames for Tools.
#[derive(Default)]
pub struct Tools {
    /// The Tool in hand each Inventory last fired `Equipped` for.
    fired: HashMap<InstanceId, InstanceId>,
    /// Inventories to [`sync`](Self::sync).
    pub dirty: Vec<InstanceId>,
    /// Dropped Tools their Inventory's character still touches, which it
    /// doesn't pick up again until it has stepped off them.
    dropped: HashMap<InstanceId, InstanceId>,
    /// The hotbar keys and buttons held last frame, so a press is the
    /// frame one goes down.
    held: BTreeSet<KeyCode>,
    /// Primary and Drop as held last frame.
    primary: bool,
    drop: bool,
    /// The Tool `Activated` last fired for, until `Deactivated`.
    active: Option<InstanceId>,
}

impl Tools {
    /// Notes what scripts did, to sync and fire.
    pub fn note(&mut self, changes: ToolChanges) {
        self.dirty.extend(changes.inventories);
        self.dropped.extend(changes.dropped);
    }

    /// Fires `Unequipped` and `Equipped` for each dirty Inventory whose
    /// `Equipped` isn't what was last fired (and `Deactivated` first for an
    /// active Tool put away), and sets its character's Animator's
    /// `Equipped` parameter.
    pub fn sync(&mut self, world: &mut World) -> Vec<Fired> {
        let mut fired = Vec::new();
        let mut seen = BTreeSet::new();
        for inventory in std::mem::take(&mut self.dirty) {
            if !seen.insert(inventory) {
                continue;
            }
            let now = equipped(world, inventory);
            let before = self.fired.get(&inventory).copied();
            if now == before {
                continue;
            }
            if let Some(old) = before.filter(|&old| world.contains(old)) {
                if self.active == Some(old) {
                    self.active = None;
                    fired.push(Fired::bare(old, "Deactivated"));
                }
                fired.push(Fired::bare(old, "Unequipped"));
            }
            match now {
                Some(tool) => {
                    self.fired.insert(inventory, tool);
                    fired.push(Fired::bare(tool, "Equipped"));
                }
                None => {
                    self.fired.remove(&inventory);
                }
            }
            if let Some(character) = world.get(inventory).and_then(|i| i.parent()) {
                set_equipped_parameter(world, character, now.is_some());
            }
        }
        self.fired.retain(|inventory, _| world.contains(*inventory));
        fired
    }

    /// One frame: characters with an Inventory pick up the dropped Tools
    /// they touch while there's room, and the player's hotbar keys, Drop
    /// and Primary act on the player character's Inventory. Every
    /// Inventory is marked dirty, so a change from elsewhere (a save slot
    /// loading) fires. Returns `Activated` and `Deactivated`; sync for the
    /// rest.
    pub fn step(&mut self, world: &mut World, assets: &Assets, input: &PromptInput) -> Vec<Fired> {
        let mut changes = ToolChanges::default();
        self.pick_up(world, &mut changes);
        let mut fired = Vec::new();
        self.follow_input(world, assets, input, &mut changes, &mut fired);
        self.note(changes);
        self.dirty.extend(inventories(world));
        fired
    }

    fn pick_up(&mut self, world: &mut World, changes: &mut ToolChanges) {
        let characters: Vec<(InstanceId, (Vec3, Vec3))> = inventories(world)
            .into_iter()
            .filter_map(|inventory| {
                let character = world.get(inventory)?.parent()?;
                let parts = axcenity_character::body_parts(world, character);
                Some((inventory, bounds(world, &parts)?))
            })
            .collect();
        if characters.is_empty() {
            return;
        }
        // Dropped Tools: in the Workspace, not in an Inventory or another Tool.
        let tools: Vec<(InstanceId, (Vec3, Vec3))> = world
            .descendants_iter(world.workspace())
            .filter(|&id| {
                is_class(world, id, ClassName::Tool)
                    && world
                        .get(id)
                        .and_then(|tool| tool.parent())
                        .is_some_and(|parent| tool_of(world, parent).is_none())
                    && !in_inventory(world, id)
            })
            .filter_map(|tool| Some((tool, bounds(world, &[handle(world, tool)?])?)))
            .collect();
        self.dropped.retain(|tool, inventory| {
            characters
                .iter()
                .find(|(id, _)| id == inventory)
                .zip(tools.iter().find(|(id, _)| id == tool))
                .is_some_and(|((_, body), (_, handle))| overlap(*body, *handle, TOUCH_MARGIN))
        });
        for (tool, handle) in tools {
            for &(inventory, body) in &characters {
                if self.dropped.get(&tool) == Some(&inventory)
                    || !overlap(body, handle, TOUCH_MARGIN)
                {
                    continue;
                }
                if add(world, inventory, tool, changes) == Ok(true) {
                    break;
                }
            }
        }
    }

    fn follow_input(
        &mut self,
        world: &mut World,
        assets: &Assets,
        input: &PromptInput,
        changes: &mut ToolChanges,
        fired: &mut Vec<Fired>,
    ) {
        let keys = SLOT_KEYS
            .into_iter()
            .chain([KeyCode::ButtonL1, KeyCode::ButtonR1]);
        let down: BTreeSet<KeyCode> = keys.filter(|&key| input.game.is_down(key)).collect();
        let pressed: Vec<KeyCode> = down.difference(&self.held).copied().collect();
        self.held = down;
        let primary = input.map.is_pressed("Primary", &input.game);
        let drop = input.map.is_pressed("Drop", &input.game);
        let (primary_pressed, drop_pressed) = (primary && !self.primary, drop && !self.drop);
        self.primary = primary;
        self.drop = drop;

        let Some(inventory) = player_inventory(world) else {
            return;
        };
        let tools = items(world, inventory);
        let current = equipped(world, inventory);
        let slot = current.and_then(|tool| tools.iter().position(|&t| t == tool));
        if flag(world, inventory, "HotbarEnabled", true) && !tools.is_empty() {
            let count = tools.len();
            let mut pick: Option<Option<usize>> = None;
            for key in &pressed {
                if let Some(index) = SLOT_KEYS.iter().position(|k| k == key) {
                    if index < count {
                        // The slot in hand puts it away.
                        pick = Some((slot != Some(index)).then_some(index));
                    }
                } else if *key == KeyCode::ButtonR1 {
                    pick = Some(Some(slot.map_or(0, |at| (at + 1) % count)));
                } else if *key == KeyCode::ButtonL1 {
                    pick = Some(Some(slot.map_or(count - 1, |at| (at + count - 1) % count)));
                }
            }
            // The wheel goes through the slots while a Tool is in hand; with
            // none, it's the camera's.
            if let Some(at) = slot
                && input.wheel != 0.0
            {
                let next = if input.wheel < 0.0 {
                    at + 1
                } else {
                    at + count - 1
                };
                pick = Some(Some(next % count));
            }
            match pick {
                Some(Some(index)) => {
                    let _ = equip(world, assets, inventory, tools[index], changes);
                }
                Some(None) => unequip_all(world, inventory, changes),
                None => {}
            }
        }
        let current = equipped(world, inventory);
        if drop_pressed && let Some(tool) = current {
            drop_tool(world, tool, changes);
        }
        let current = equipped(world, inventory);
        if let Some(active) = self.active
            && (!primary || current != Some(active))
        {
            self.active = None;
            if world.contains(active) {
                fired.push(Fired::bare(active, "Deactivated"));
            }
        }
        if primary_pressed && let Some(tool) = current {
            self.active = Some(tool);
            fired.push(Fired::bare(tool, "Activated"));
        }
    }
}

/// True when `tool` is inside an Inventory, however deep.
fn in_inventory(world: &World, tool: InstanceId) -> bool {
    let mut at = world.get(tool).and_then(|instance| instance.parent());
    while let Some(parent) = at {
        if is_class(world, parent, ClassName::Inventory) {
            return true;
        }
        at = world.get(parent).and_then(|instance| instance.parent());
    }
    false
}

pub(crate) fn changes_mut(lua: &Lua) -> mlua::Result<mlua::AppDataRefMut<'_, ToolChanges>> {
    lua.app_data_mut::<ToolChanges>()
        .ok_or_else(|| mlua::Error::runtime("tools aren't set up on this side"))
}

/// The Tool argument of `call`.
pub(crate) fn tool_arg(lua: &Lua, value: mlua::Value, call: &str) -> mlua::Result<InstanceId> {
    let not_tool = |value: &mlua::Value| {
        mlua::Error::runtime(format!(
            "{call} needs a Tool, not {}",
            type_name(lua, value)
        ))
    };
    let tool = LuaInstance::from_lua(value.clone(), lua).map_err(|_| not_tool(&value))?;
    let world = crate::instance::world(lua)?;
    match world.get(tool.0) {
        Some(instance) if instance.class() == ClassName::Tool => Ok(tool.0),
        Some(_) => Err(not_tool(&value)),
        None => Err(destroyed()),
    }
}

/// Runs `change` on the World and this side's [`ToolChanges`].
fn with_changes<T>(
    lua: &Lua,
    this: InstanceId,
    change: impl FnOnce(&mut World, &mut ToolChanges) -> T,
) -> mlua::Result<T> {
    let mut world = world_mut(lua)?;
    world.get(this).ok_or_else(destroyed)?;
    let mut changes = changes_mut(lua)?;
    Ok(change(&mut world, &mut changes))
}

pub fn methods(lua: &Lua) -> mlua::Result<Vec<(ClassName, &'static str, Function)>> {
    let get_items = lua.create_function(|lua, this: LuaInstance| {
        let world = crate::instance::world(lua)?;
        world.get(this.0).ok_or_else(destroyed)?;
        Ok(items(&world, this.0)
            .into_iter()
            .map(LuaInstance)
            .collect::<Vec<_>>())
    })?;
    let add_tool = lua.create_function(|lua, (this, tool): (LuaInstance, mlua::Value)| {
        let tool = tool_arg(lua, tool, "Add(tool)")?;
        with_changes(lua, this.0, |world, changes| {
            add(world, this.0, tool, changes)
        })?
        .map_err(mlua::Error::runtime)
    })?;
    let remove_tool = lua.create_function(|lua, (this, tool): (LuaInstance, mlua::Value)| {
        let tool = tool_arg(lua, tool, "Remove(tool)")?;
        with_changes(lua, this.0, |world, changes| {
            remove(world, this.0, tool, changes)
        })
    })?;
    let equip_tool = lua.create_function(|lua, (this, tool): (LuaInstance, mlua::Value)| {
        let tool = tool_arg(lua, tool, "Equip(tool)")?;
        let assets = lua
            .app_data_ref::<Assets>()
            .map(|assets| assets.clone())
            .unwrap_or_default();
        with_changes(lua, this.0, |world, changes| {
            equip(world, &assets, this.0, tool, changes)
        })?
        .map_err(mlua::Error::runtime)
    })?;
    let unequip = lua.create_function(|lua, this: LuaInstance| {
        with_changes(lua, this.0, |world, changes| {
            unequip_all(world, this.0, changes)
        })
    })?;
    let drop = lua.create_function(|lua, this: LuaInstance| {
        with_changes(lua, this.0, |world, changes| {
            drop_tool(world, this.0, changes)
        })
    })?;
    Ok(vec![
        (ClassName::Inventory, "GetItems", get_items),
        (ClassName::Inventory, "Add", add_tool),
        (ClassName::Inventory, "Remove", remove_tool),
        (ClassName::Inventory, "Equip", equip_tool),
        (ClassName::Inventory, "UnequipAll", unequip),
        (ClassName::Tool, "Drop", drop),
    ])
}

/// What setting `Inventory.Equipped` says.
pub const EQUIPPED_READ_ONLY: &str =
    "Inventory.Equipped is read-only; use inventory:Equip(tool) or inventory:UnequipAll()";
