//! The Abilities and Effects system (Phase 76, moved here by roadmap L7):
//! the state both sides share (`ability`), Abilities' and Models' methods,
//! the server's step, players' activation requests, and the events they
//! fire. Hosts call [`step_abilities`] in frame step 1 and draw
//! [`ability_bar`].

use axcenity_core::ability::AbilitySlot;
use axcenity_core::{ClassName, InstanceId, Value, World};
use axcenity_script::system::{self, GameSystem};
use axcenity_script::{LuaInstance, ScriptRuntime, Side, lent_world};
use mlua::{Function, Lua};

use crate::ability::{self, AbilityEvent, Request, SharedAbilities};

/// Abilities and Effects: see the module documentation. One per runtime;
/// [`crate::install`] adds it.
#[derive(Default)]
pub struct AbilitySystem {
    abilities: SharedAbilities,
}

impl GameSystem for AbilitySystem {
    fn added(&self, lua: &Lua) {
        lua.set_app_data(self.abilities.clone());
    }

    fn methods(&self, lua: &Lua) -> mlua::Result<Vec<(ClassName, &'static str, Function)>> {
        ability::methods(lua)
    }

    fn read_only(&self, class: ClassName, key: &str) -> Option<String> {
        ability::read_only(class, key)
    }

    /// Before the engine's events, so the Health events Effects cause fire
    /// in the same `fire_changes`.
    fn fires_first(&self) -> bool {
        true
    }

    fn activation_requested(&self, target: InstanceId, player: InstanceId) {
        self.abilities.0.borrow_mut().requests.push(Request {
            ability: target,
            player,
        });
    }

    /// Hands Effects' Health events to the runtime, then fires the
    /// Abilities' `Activated`, `Ended` and `CooldownChanged` on both sides.
    fn fire_events(&self, runtime: &mut ScriptRuntime, world: &mut World) -> bool {
        let (events, health) = {
            let mut abilities = self.abilities.0.borrow_mut();
            (
                std::mem::take(&mut abilities.pending),
                std::mem::take(&mut abilities.health),
            )
        };
        crate::character_system::queue_health_events(runtime, health);
        if events.is_empty() {
            return false;
        }
        for side in [Side::Server, Side::Client] {
            runtime.run_on(side, world, |lua| fire_ability_events(lua, &events));
            runtime.fire_change_rounds(world);
        }
        true
    }
}

/// The server's step (from `step_health`): cooldowns, active abilities
/// and Effects count down.
pub(crate) fn step_server(runtime: &ScriptRuntime, world: &mut World, seconds: f64) {
    if let Some(system) = runtime.system::<AbilitySystem>() {
        system.abilities.0.borrow_mut().step(world, seconds);
    }
}

/// Fires Abilities' events, for those still there. The World is lent.
fn fire_ability_events(lua: &Lua, events: &[AbilityEvent]) {
    let exists = |id: InstanceId| lent_world(lua).is_ok_and(|world| world.contains(id));
    for event in events {
        match *event {
            AbilityEvent::Activated { ability, player } if exists(ability) => {
                let player = player.filter(|&id| exists(id)).map(LuaInstance);
                system::fire_instance_event(lua, ability, "Activated", player);
            }
            AbilityEvent::Ended { ability, cancelled } if exists(ability) => {
                system::fire_instance_event(lua, ability, "Ended", cancelled);
            }
            AbilityEvent::CooldownChanged { ability, left } if exists(ability) => {
                system::fire_instance_event(lua, ability, "CooldownChanged", left);
            }
            _ => {}
        }
    }
}

/// Frame step 1, after `step_tools`, on every device: the player's
/// Abilities whose `ActivationKey` was just pressed ask to activate. The
/// server (this device's, or the one it follows) checks again; a following
/// device starts the cooldown sweep at once. This device's server then
/// serves the requests that came in.
pub fn step_abilities(runtime: &mut ScriptRuntime, world: &mut World, seconds: f64) {
    runtime.fire_changes(world);
    let Some(shared) = runtime
        .system::<AbilitySystem>()
        .map(|system| system.abilities.clone())
    else {
        return;
    };
    let input = runtime.prompt_input();
    let player = runtime.local_player();
    let locked = runtime.input_locked(world);
    let following = runtime.is_client_only();
    let mut asks = Vec::new();
    {
        let mut abilities = shared.0.borrow_mut();
        abilities.step_predicted(seconds);
        let mine = player.map_or_else(Vec::new, |player| ability::player_abilities(world, player));
        let pressed = if locked {
            Vec::new()
        } else {
            abilities.pressed(world, &mine, &input.map, &input.game)
        };
        for target in pressed {
            let Some(player) = player else { break };
            if following {
                if ability::can_activate(world, target).is_ok() {
                    abilities.predict(world, target);
                    asks.push((target, player));
                }
            } else {
                abilities.requests.push(Request {
                    ability: target,
                    player,
                });
            }
        }
        if !following {
            abilities.serve_requests(world);
        }
    }
    // A following device's requests go to the server it follows.
    if !asks.is_empty() {
        runtime.run_on(Side::Client, world, |lua| {
            for (target, player) in asks {
                system::send_activation(lua, target, player);
            }
        });
    }
    runtime.fire_changes(world);
}

/// The local player's Abilities with an `ActivationKey`, for the ability
/// bar: each with the cooldown it shows (the server's, or the one this
/// device started at once).
pub fn ability_bar(runtime: &ScriptRuntime, world: &World) -> Vec<AbilitySlot> {
    let (Some(system), Some(player)) = (runtime.system::<AbilitySystem>(), runtime.local_player())
    else {
        return Vec::new();
    };
    let input = runtime.prompt_input();
    let abilities = system.abilities.0.borrow();
    ability::player_abilities(world, player)
        .into_iter()
        .filter_map(|id| {
            let instance = world.get(id)?;
            let action = instance.property("ActivationKey")?.as_str()?.trim();
            if action.is_empty() {
                return None;
            }
            Some(AbilitySlot {
                ability: id,
                key: input
                    .map
                    .action(action)
                    .map(|action| action.label(input.gamepad))
                    .unwrap_or_default(),
                cooldown_left: abilities.shown_cooldown(world, id) as f32,
                cooldown: instance
                    .property("Cooldown")
                    .and_then(Value::as_number)
                    .unwrap_or(0.0) as f32,
            })
        })
        .collect()
}

/// Players' requests to activate Abilities that this server hasn't served
/// yet, as `(ability, player)`, for tests.
pub fn ability_requests(runtime: &ScriptRuntime) -> Vec<(InstanceId, InstanceId)> {
    runtime
        .system::<AbilitySystem>()
        .map(|system| {
            system
                .abilities
                .0
                .borrow()
                .requests
                .iter()
                .map(|request| (request.ability, request.player))
                .collect()
        })
        .unwrap_or_default()
}
