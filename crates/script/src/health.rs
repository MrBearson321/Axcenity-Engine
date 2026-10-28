//! Health and damage (Phase 43): `health:TakeDamage(amount, source)`,
//! `health:Heal(amount)` and scripts setting `Health`, which change the
//! World at once and queue `HealthChanged`, `Damaged` and `Died`; and the
//! per-frame step ([`Healths::step`]) that applies DamageZones and
//! regeneration and keeps health bars bound to their Health.
//!
//! A side's scripts queue events in its app data ([`HealthEvents`]); the
//! runtime collects them and fires them on both sides, as it does change
//! events. `Died` fires when Health goes from above 0 to 0, so it fires once
//! however many hits land: at 0 damage does nothing.

use std::collections::{BTreeSet, HashMap};

use axcenity_assets::Assets;
use axcenity_core::{ClassName, InstanceId, Value, World};
use axcenity_physics::QueryParams;
use mlua::{FromLua, Function, Lua};

use crate::instance::{LuaInstance, destroyed, world_mut};
use crate::types::type_name;

/// The name of the ObjectValue inside a ProgressBar that binds it to a
/// Health: the bar shows that Health out of its MaxHealth.
pub const BAR_BINDING: &str = "Health";

/// Something a Health did, waiting to be fired on both sides.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HealthEvent {
    /// `HealthChanged(new, old)`.
    Changed {
        health: InstanceId,
        new: f64,
        old: f64,
    },
    /// `Damaged(amount, source)`: how much it lost, and what did it.
    Damaged {
        health: InstanceId,
        amount: f64,
        source: Option<InstanceId>,
    },
    /// `Died()`.
    Died { health: InstanceId },
}

/// The events one side's scripts caused, until the runtime collects them.
#[derive(Default)]
pub struct HealthEvents(pub Vec<HealthEvent>);

fn number(world: &World, id: InstanceId, name: &str) -> f64 {
    world
        .get(id)
        .and_then(|instance| instance.property(name))
        .and_then(Value::as_number)
        .filter(|n| n.is_finite())
        .unwrap_or(0.0)
}

fn flag(world: &World, id: InstanceId, name: &str) -> bool {
    world
        .get(id)
        .and_then(|instance| instance.property(name))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn is_health(world: &World, id: InstanceId) -> bool {
    world
        .get(id)
        .is_some_and(|instance| instance.class() == ClassName::Health)
}

/// Sets the Health `id`'s `Health` to `to`, kept between 0 and MaxHealth.
/// Returns `HealthChanged` if it changed, then `Died` if it went from
/// above 0 to 0.
pub fn set_health(world: &mut World, id: InstanceId, to: f64) -> Vec<HealthEvent> {
    let old = number(world, id, "Health");
    let max = number(world, id, "MaxHealth").max(0.0);
    let new = if to.is_finite() {
        to.clamp(0.0, max)
    } else {
        old
    };
    if new == old
        || world
            .set_property(id, "Health", Value::Number(new))
            .is_err()
    {
        return Vec::new();
    }
    let mut events = vec![HealthEvent::Changed {
        health: id,
        new,
        old,
    }];
    if old > 0.0 && new <= 0.0 {
        events.push(HealthEvent::Died { health: id });
    }
    events
}

/// `TakeDamage(amount, source)`: nothing when it's Invulnerable, already
/// dead, or `amount` isn't above 0. Otherwise `HealthChanged`, `Damaged`
/// (with what it lost) and maybe `Died`.
pub fn damage(
    world: &mut World,
    id: InstanceId,
    amount: f64,
    source: Option<InstanceId>,
) -> Vec<HealthEvent> {
    let old = number(world, id, "Health");
    if !is_health(world, id)
        || flag(world, id, "Invulnerable")
        || old <= 0.0
        || !(amount > 0.0 && amount.is_finite())
    {
        return Vec::new();
    }
    let mut events = set_health(world, id, old - amount);
    let lost = old - number(world, id, "Health");
    // After HealthChanged, before Died.
    events.insert(
        1.min(events.len()),
        HealthEvent::Damaged {
            health: id,
            amount: lost,
            source,
        },
    );
    events
}

/// `Heal(amount)`: up to MaxHealth. Nothing once it's dead.
pub fn heal(world: &mut World, id: InstanceId, amount: f64) -> Vec<HealthEvent> {
    let old = number(world, id, "Health");
    if !is_health(world, id) || old <= 0.0 || !(amount > 0.0 && amount.is_finite()) {
        return Vec::new();
    }
    set_health(world, id, old + amount)
}

/// The Health of the Model `id` is part of: a Health inside `id`, or else
/// inside its nearest ancestor that has one.
pub fn health_of(world: &World, id: InstanceId) -> Option<InstanceId> {
    let mut at = Some(id);
    while let Some(current) = at {
        let instance = world.get(current)?;
        if let Some(&health) = instance
            .children()
            .iter()
            .find(|&&child| is_health(world, child))
        {
            return Some(health);
        }
        at = instance.parent();
    }
    None
}

fn push(lua: &Lua, events: Vec<HealthEvent>) {
    if let Some(mut queue) = lua.app_data_mut::<HealthEvents>() {
        queue.0.extend(events);
    }
}

fn amount_arg(lua: &Lua, value: mlua::Value, call: &str) -> mlua::Result<f64> {
    match value {
        mlua::Value::Integer(n) => Ok(n as f64),
        mlua::Value::Number(n) if n.is_finite() => Ok(n),
        other => Err(mlua::Error::runtime(format!(
            "{call} needs a finite number, not {}",
            type_name(lua, &other)
        ))),
    }
}

pub fn methods(lua: &Lua) -> mlua::Result<Vec<(ClassName, &'static str, Function)>> {
    let take = lua.create_function(
        |lua, (this, amount, source): (LuaInstance, mlua::Value, mlua::Value)| {
            let call = "TakeDamage(amount, source)";
            let amount = amount_arg(lua, amount, call)?;
            let source = match source {
                mlua::Value::Nil => None,
                value => Some(
                    LuaInstance::from_lua(value.clone(), lua)
                        .map_err(|_| {
                            mlua::Error::runtime(format!(
                                "{call}'s source must be an Instance or nil, not {}",
                                type_name(lua, &value)
                            ))
                        })?
                        .0,
                ),
            };
            let events = {
                let mut world = world_mut(lua)?;
                world.get(this.0).ok_or_else(destroyed)?;
                damage(&mut world, this.0, amount, source)
            };
            push(lua, events);
            Ok(())
        },
    )?;
    let heal = lua.create_function(|lua, (this, amount): (LuaInstance, mlua::Value)| {
        let amount = amount_arg(lua, amount, "Heal(amount)")?;
        let events = {
            let mut world = world_mut(lua)?;
            world.get(this.0).ok_or_else(destroyed)?;
            heal(&mut world, this.0, amount)
        };
        push(lua, events);
        Ok(())
    })?;
    Ok(vec![
        (ClassName::Health, "TakeDamage", take),
        (ClassName::Health, "Heal", heal),
    ])
}

/// A script setting `health.Health`: kept between 0 and MaxHealth, and
/// fires `HealthChanged` (and `Died` at 0) like damage does.
pub fn set(lua: &Lua, id: InstanceId, value: mlua::Value) -> mlua::Result<()> {
    let to = match value {
        mlua::Value::Integer(n) => n as f64,
        mlua::Value::Number(n) => n,
        other => {
            return Err(mlua::Error::runtime(format!(
                "Health.Health must be a number, not {}",
                type_name(lua, &other)
            )));
        }
    };
    let events = {
        let mut world = world_mut(lua)?;
        world.get(id).ok_or_else(destroyed)?;
        set_health(&mut world, id, to)
    };
    push(lua, events);
    Ok(())
}

/// What the runtime keeps between frames for every Health.
#[derive(Default)]
pub struct Healths {
    /// Seconds since each Health last took damage. One never damaged
    /// regenerates at once.
    since_damage: HashMap<InstanceId, f64>,
    /// Events waiting to be fired on both sides.
    pub pending: Vec<HealthEvent>,
}

impl Healths {
    /// Starts the RegenDelay wait again for each Health in `events` that
    /// took damage.
    pub fn note(&mut self, events: &[HealthEvent]) {
        for event in events {
            if let HealthEvent::Damaged { health, .. } = event {
                self.since_damage.insert(*health, 0.0);
            }
        }
    }

    /// One frame of `seconds`: each DamageZone in the Workspace damages
    /// every Health with a part inside it, then each Health that has gone
    /// RegenDelay seconds without damage regenerates. Returns the events,
    /// which are also noted.
    pub fn step(&mut self, world: &mut World, assets: &Assets, seconds: f64) -> Vec<HealthEvent> {
        let dt = if seconds.is_finite() {
            seconds.max(0.0)
        } else {
            0.0
        };
        let mut healths = Vec::new();
        let mut zones = Vec::new();
        for id in world.descendants(world.workspace()) {
            match world.get(id).map(|instance| instance.class()) {
                Some(ClassName::Health) => healths.push(id),
                Some(ClassName::DamageZone) => zones.push(id),
                _ => {}
            }
        }
        self.since_damage.retain(|id, _| world.contains(*id));
        for since in self.since_damage.values_mut() {
            *since += dt;
        }
        let mut events = Vec::new();
        for zone in zones {
            let rate = number(world, zone, "DamagePerSecond");
            if rate <= 0.0 || dt == 0.0 {
                continue;
            }
            let inside: BTreeSet<InstanceId> =
                axcenity_physics::parts_in_part(world, assets, zone, &QueryParams::default())
                    .into_iter()
                    .filter_map(|part| health_of(world, part))
                    .collect();
            for health in inside {
                events.extend(damage(world, health, rate * dt, Some(zone)));
            }
        }
        self.note(&events);
        for health in healths {
            let (now, max) = (
                number(world, health, "Health"),
                number(world, health, "MaxHealth"),
            );
            if now > max {
                events.extend(set_health(world, health, max));
                continue;
            }
            let rate = number(world, health, "RegenRate");
            let waited = self
                .since_damage
                .get(&health)
                .is_none_or(|&since| since >= number(world, health, "RegenDelay"));
            if waited && rate > 0.0 && now > 0.0 && now < max {
                events.extend(set_health(world, health, now + rate * dt));
            }
        }
        events
    }
}

/// Fall damage for the character controller `controller` landing at
/// `speed` studs a second: from the Health of its Model, if FallDamage is
/// on and `speed` is over FallDamageThreshold.
pub fn fall_damage(world: &mut World, controller: InstanceId, speed: f64) -> Vec<HealthEvent> {
    let Some(health) = health_of(world, controller) else {
        return Vec::new();
    };
    if !flag(world, health, "FallDamage") {
        return Vec::new();
    }
    let over = speed - number(world, health, "FallDamageThreshold");
    if over <= 0.0 {
        return Vec::new();
    }
    damage(
        world,
        health,
        over * number(world, health, "FallDamageScale"),
        None,
    )
}

/// Sets each ProgressBar bound to a Health (an ObjectValue named
/// [`BAR_BINDING`] inside it pointing at one) to show that Health out of
/// its MaxHealth.
pub fn bind_bars(world: &mut World) {
    let mut bound = Vec::new();
    for id in world.descendants(world.root()) {
        let Some(bar) = world.get(id) else { continue };
        if bar.class() != ClassName::ProgressBar {
            continue;
        }
        let health = bar.children().iter().find_map(|&child| {
            let value = world.get(child)?;
            if value.class() != ClassName::ObjectValue || value.name() != BAR_BINDING {
                return None;
            }
            match value.property("Value") {
                Some(Value::Instance(Some(health))) if is_health(world, *health) => Some(*health),
                _ => None,
            }
        });
        if let Some(health) = health {
            bound.push((id, health));
        }
    }
    for (bar, health) in bound {
        let shown = [
            ("MinValue", 0.0),
            ("MaxValue", number(world, health, "MaxHealth")),
            ("Value", number(world, health, "Health")),
        ];
        for (name, value) in shown {
            if number(world, bar, name) != value {
                let _ = world.set_property(bar, name, Value::Number(value));
            }
        }
    }
}
