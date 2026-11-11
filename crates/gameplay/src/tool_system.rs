//! The Tools system (Phase 45, moved here by roadmap L7): Tools and
//! Inventories (`tool`), their methods and a Humanoid's `EquipTool` and
//! `UnequipTools`, and the events they fire. Hosts call [`step_tools`] in
//! frame step 1 and [`place_tools`] after the Animators pose the
//! characters, and draw the hotbar for [`player_inventory`].

use std::cell::RefCell;

use axcenity_anim::Poses;
use axcenity_core::{ClassName, InstanceId, World};
use axcenity_script::system::{self, GameSystem};
use axcenity_script::{MAX_CHANGE_ROUNDS, ScriptRuntime, Side, lent_world};
use mlua::{Function, Lua};

use crate::tool::{self, Fired, SharedToolChanges, Tools};

/// Tools and Inventories: see the module documentation. One per runtime;
/// [`crate::install`] adds it.
#[derive(Default)]
pub struct ToolSystem {
    /// What scripts on both sides changed, until the next sync.
    changes: SharedToolChanges,
    /// What was last fired, and the player's hotbar keys.
    tools: RefCell<Tools>,
}

impl ToolSystem {
    /// Fires `fired` on both sides, for the Instances still there.
    fn fire(runtime: &mut ScriptRuntime, world: &mut World, fired: &[Fired]) {
        if fired.is_empty() {
            return;
        }
        for side in [Side::Server, Side::Client] {
            runtime.run_on(side, world, |lua| {
                for event in fired {
                    if lent_world(lua).is_ok_and(|world| world.contains(event.instance)) {
                        system::fire_instance_event(lua, event.instance, event.event, ());
                    }
                }
            });
            runtime.fire_change_rounds(world);
        }
    }
}

impl GameSystem for ToolSystem {
    fn added(&self, lua: &Lua) {
        lua.set_app_data(self.changes.clone());
    }

    fn methods(&self, lua: &Lua) -> mlua::Result<Vec<(ClassName, &'static str, Function)>> {
        tool::methods(lua)
    }

    fn read_only(&self, class: ClassName, key: &str) -> Option<String> {
        (class == ClassName::Inventory && key == "Equipped")
            .then(|| tool::EQUIPPED_READ_ONLY.to_owned())
    }

    /// Fires `Unequipped` and `Equipped` for the Inventories scripts and
    /// the tools step changed.
    fn fire_events(&self, runtime: &mut ScriptRuntime, world: &mut World) -> bool {
        sync(runtime, world)
    }
}

/// Fires `Unequipped` and `Equipped` once for the Inventories that
/// changed (`Tools::sync`); false when none did.
fn sync(runtime: &mut ScriptRuntime, world: &mut World) -> bool {
    let fired = {
        let Some(system) = runtime.system::<ToolSystem>() else {
            return false;
        };
        let mut tools = system.tools.borrow_mut();
        tools.note(std::mem::take(&mut *system.changes.0.borrow_mut()));
        if tools.dirty.is_empty() {
            return false;
        }
        tools.sync(world)
    };
    ToolSystem::fire(runtime, world, &fired);
    true
}

/// Frame step 1, after `step_prompts`: characters with an Inventory pick
/// up the dropped Tools they touch, while there's room; the player's
/// number keys, mouse wheel and controller bumpers pick a hotbar slot,
/// Drop drops the Tool in hand, and Primary fires its `Activated` and
/// `Deactivated`, in Scripts and then in LocalScripts, after `Equipped`
/// and `Unequipped`.
pub fn step_tools(runtime: &mut ScriptRuntime, world: &mut World) {
    runtime.fire_changes(world);
    let Some(system) = runtime.system::<ToolSystem>() else {
        return;
    };
    let input = runtime.prompt_input();
    let assets = runtime.assets();
    let fired = system.tools.borrow_mut().step(world, &assets, &input);
    // Equipped and Unequipped first, then Activated.
    for _ in 0..MAX_CHANGE_ROUNDS {
        if !sync(runtime, world) {
            break;
        }
    }
    ToolSystem::fire(runtime, world, &fired);
}

/// Frame step 4, after the Animators pose the characters: each Tool in
/// hand moves to its character's right hand, the bone posed as `poses`
/// has it.
pub fn place_tools(runtime: &mut ScriptRuntime, world: &mut World, poses: &Poses) {
    runtime.fire_changes(world);
    tool::place_held(world, &runtime.assets(), poses);
    runtime.fire_changes(world);
}

/// The player character's Inventory, which the default hotbar shows
/// unless its `HotbarEnabled` is off.
pub fn player_inventory(world: &World) -> Option<InstanceId> {
    tool::player_inventory(world)
}
