//! The character system (Phases 43 and 46, moved here by roadmap L7):
//! Health and damage (`health`) and Humanoids (`humanoid`), which depend
//! on each other (a Health that dies kills its Humanoid; a Humanoid's
//! `Health` is its Health's). It adds their methods, keeps `Health.Health`
//! and a Humanoid's fields, and fires their events. Hosts call
//! [`fire_landed`] and [`step_humanoids`] in frame step 1 and
//! [`step_health`] in frame step 4, and share the game's navigation mesh
//! with [`share_nav_mesh`].

use std::cell::RefCell;

use axcenity_character::Controllers;
use axcenity_core::{ClassName, InstanceId, World};
use axcenity_nav::SharedNavMesh;
use axcenity_script::system::{self, GameSystem};
use axcenity_script::{LuaInstance, ScriptRuntime, Side, lent_world};
use mlua::{Function, Lua};

use crate::health::{self, HealthEvent, Healths, SharedHealthEvents};
use crate::humanoid::{self, HumanoidEvent, Humanoids};

/// The properties the system keeps: a Humanoid's (its pieces'), and
/// `Health.Health`, which is one of them.
const FIELDS: [&str; 11] = [
    "WalkSpeed",
    "RunSpeed",
    "JumpHeight",
    "Sit",
    "PlatformStand",
    "AutoRotate",
    "MoveDirection",
    "Health",
    "MaxHealth",
    "DisplayName",
    "HealthDisplayDistance",
];

/// Health and Humanoids: see the module documentation. One per runtime;
/// [`crate::install`] adds it.
#[derive(Default)]
pub struct CharacterSystem {
    /// Health events waiting to fire on both sides, which scripts on both
    /// sides and the steps queue.
    events: SharedHealthEvents,
    /// Every Health's regeneration wait.
    healths: RefCell<Healths>,
    /// Humanoids' walks and states; their requests and states are shared
    /// with both sides.
    humanoids: RefCell<Humanoids>,
}

impl GameSystem for CharacterSystem {
    fn added(&self, lua: &Lua) {
        let humanoids = self.humanoids.borrow();
        lua.set_app_data(self.events.clone());
        lua.set_app_data(humanoids.requests.clone());
        lua.set_app_data(humanoids.states.clone());
    }

    fn methods(&self, lua: &Lua) -> mlua::Result<Vec<(ClassName, &'static str, Function)>> {
        let mut methods = health::methods(lua)?;
        methods.extend(humanoid::methods(lua)?);
        Ok(methods)
    }

    fn field_names(&self) -> &'static [&'static str] {
        &FIELDS
    }

    fn get_field(
        &self,
        lua: &Lua,
        id: InstanceId,
        class: ClassName,
        key: &str,
    ) -> Option<mlua::Result<mlua::Value>> {
        (class == ClassName::Humanoid && humanoid::is_field(key))
            .then(|| humanoid::get(lua, id, key))
    }

    fn sets_field(&self, class: ClassName, key: &str) -> bool {
        match class {
            ClassName::Health => key == "Health",
            ClassName::Humanoid => humanoid::is_field(key),
            _ => false,
        }
    }

    fn set_field(
        &self,
        lua: &Lua,
        id: InstanceId,
        class: ClassName,
        key: &str,
        value: mlua::Value,
    ) -> mlua::Result<()> {
        if class == ClassName::Humanoid {
            humanoid::set(lua, id, key, value)
        } else {
            health::set(lua, id, value)
        }
    }

    /// Before the Players' events, so a character that dies here is
    /// noted for respawning in the same call.
    fn fires_first(&self) -> bool {
        true
    }

    /// Fires the Health events queued, on both sides, noting damage for
    /// regeneration, then kills the Humanoids of the Healths that died.
    fn fire_events(&self, runtime: &mut ScriptRuntime, world: &mut World) -> bool {
        let events = std::mem::take(&mut *self.events.0.borrow_mut());
        if events.is_empty() {
            return false;
        }
        self.healths.borrow_mut().note(&events);
        for side in [Side::Server, Side::Client] {
            runtime.run_on(side, world, |lua| fire_health_events(lua, &events));
            runtime.fire_change_rounds(world);
        }
        // Then the Humanoids of the Healths that died (Phase 46).
        for health in events.iter().filter_map(humanoid::died_health) {
            let fired = self.humanoids.borrow_mut().died(world, health);
            fire_humanoid_events(runtime, world, &fired);
        }
        true
    }
}

/// Fires each Health event on its Health, skipping Healths that are gone.
/// A source that's gone is nil. The World is lent.
fn fire_health_events(lua: &Lua, events: &[HealthEvent]) {
    let exists = |id: InstanceId| lent_world(lua).is_ok_and(|world| world.contains(id));
    for event in events {
        match *event {
            HealthEvent::Changed { health, new, old } if exists(health) => {
                system::fire_instance_event(lua, health, "HealthChanged", (new, old));
            }
            HealthEvent::Damaged {
                health,
                amount,
                source,
            } if exists(health) => {
                let source = source.filter(|&id| exists(id)).map(LuaInstance);
                system::fire_instance_event(lua, health, "Damaged", (amount, source));
            }
            HealthEvent::Died { health } if exists(health) => {
                system::fire_instance_event(lua, health, "Died", ());
            }
            _ => {}
        }
    }
}

/// Fires Humanoid events on both sides. A player's character dying
/// respawns them later (Phase 47).
fn fire_humanoid_events(runtime: &mut ScriptRuntime, world: &mut World, events: &[HumanoidEvent]) {
    if events.is_empty() {
        return;
    }
    for event in events {
        if let HumanoidEvent::Died { humanoid } = *event {
            runtime.humanoid_died(world, humanoid);
        }
    }
    for side in [Side::Server, Side::Client] {
        runtime.run_on(side, world, |lua| {
            for &event in events {
                let humanoid = event.humanoid();
                if !lent_world(lua).is_ok_and(|world| world.contains(humanoid)) {
                    continue;
                }
                match event {
                    HumanoidEvent::MoveToFinished { reached, .. } => {
                        system::fire_instance_event(lua, humanoid, "MoveToFinished", reached);
                    }
                    HumanoidEvent::StateChanged { old, new, .. } => {
                        let args = humanoid::state_items(old, new);
                        system::fire_instance_event(lua, humanoid, "StateChanged", args);
                    }
                    HumanoidEvent::Died { .. } => {
                        system::fire_instance_event(lua, humanoid, "Died", ());
                    }
                }
            }
        });
        runtime.fire_change_rounds(world);
    }
}

/// Queues Health events (from `health::damage`, `heal` or `set_health`)
/// to fire on both sides in the next `fire_changes`.
pub(crate) fn queue_health_events(runtime: &ScriptRuntime, events: Vec<HealthEvent>) {
    if events.is_empty() {
        return;
    }
    if let Some(system) = runtime.system::<CharacterSystem>() {
        system.events.0.borrow_mut().extend(events);
    }
}

/// Frame step 1, after `fire_seated`: for each character controller that
/// landed this frame and how fast it was falling, the Health in its Model
/// takes fall damage (if its `FallDamage` is on), then `Landed(fallSpeed)`
/// fires, in Scripts and then in LocalScripts (Phase 43).
pub fn fire_landed(runtime: &mut ScriptRuntime, world: &mut World, landings: &[(InstanceId, f32)]) {
    runtime.fire_changes(world);
    for &(controller, speed) in landings {
        let speed = f64::from(speed);
        let events = health::fall_damage(world, controller, speed);
        queue_health_events(runtime, events);
        runtime.fire_changes(world);
        for side in [Side::Server, Side::Client] {
            runtime.run_on(side, world, |lua| {
                if lent_world(lua).is_ok_and(|world| world.contains(controller)) {
                    system::fire_instance_event(lua, controller, "Landed", speed);
                }
            });
            runtime.fire_changes(world);
        }
    }
}

/// Frame step 1, after the character controllers and `Landed` (Phase
/// 46): every Humanoid in the Workspace makes the pieces it lacks, walks
/// its `MoveTo` on (by its controller's `MoveDirection`), and fires
/// `MoveToFinished`, `StateChanged` and `Died`, in Scripts and then in
/// LocalScripts; its Animator plays the matching state.
pub fn step_humanoids(
    runtime: &mut ScriptRuntime,
    world: &mut World,
    controllers: &Controllers,
    seconds: f64,
) {
    runtime.fire_changes(world);
    let assets = runtime.assets();
    let Some(system) = runtime.system::<CharacterSystem>() else {
        return;
    };
    let events = system
        .humanoids
        .borrow_mut()
        .step(world, &assets, controllers, seconds);
    fire_humanoid_events(runtime, world, &events);
    runtime.fire_changes(world);
}

/// Has Humanoids' `MoveTo` plan on `nav`, the game's one navigation mesh,
/// which its NavigationAgents walk on too (`axcenity_nav::Agents::nav_mesh`),
/// instead of building a second one (roadmap item M13).
pub fn share_nav_mesh(runtime: &ScriptRuntime, nav: SharedNavMesh) {
    if let Some(system) = runtime.system::<CharacterSystem>() {
        system.humanoids.borrow_mut().share_nav_mesh(nav);
    }
}

/// Frame step 4, after `fire_breaks` and before `Heartbeat`, on the device
/// that runs the server (Phase 43): DamageZones damage the Healths with
/// parts inside them, Healths regenerate, their events fire, Abilities'
/// cooldowns and Effects count down (Phase 76), and health bars
/// (ProgressBars bound to a Health) show where they are now.
pub fn step_health(runtime: &mut ScriptRuntime, world: &mut World, seconds: f64) {
    runtime.fire_changes(world);
    let assets = runtime.assets();
    if let Some(system) = runtime.system::<CharacterSystem>() {
        let events = system.healths.borrow_mut().step(world, &assets, seconds);
        system.events.0.borrow_mut().extend(events);
    }
    runtime.fire_changes(world);
    crate::ability_system::step_server(runtime, world, seconds);
    runtime.fire_changes(world);
    health::bind_bars(world);
    runtime.fire_changes(world);
}
