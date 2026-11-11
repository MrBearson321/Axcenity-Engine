//! Abilities and Effects (Phase 76): a small version of a gameplay ability
//! system over the attributes and tags Instances already have.
//!
//! An **Ability** is held by the Model or Player it's parented to. Its
//! holders are that parent plus, for a Player, their character, and for a
//! player's character, their Player: the cost is paid from the first holder
//! with the `CostAttribute`, and any holder having one of `BlockedByTags`
//! stops it. Activating spends the cost, starts the cooldown and, for an
//! ability with a `Duration`, gives the character its `GrantsTags` until it
//! ends. Only the server activates: a LocalScript's `Activate()` and the
//! player's `ActivationKey` send a request ((`MessageKind::Activate`)), and
//! the server checks it again, so a modified client can't skip a cooldown.
//!
//! An **Effect** is active while its Parent is a Model in the Workspace
//! (templates live elsewhere: ReplicatedStorage, ServerStorage, or inside
//! an Ability). It changes an attribute of that Model, or a property of its
//! Health or character controller (what a Humanoid's fields are). An
//! instant one changes once and is gone; one with a `Duration` and a
//! `Period` changes again every period, for good; one with a `Duration`
//! and no period changes once for as long as it lasts, and that change is
//! undone when it ends or is removed. `Model:ApplyEffect(effect)` clones a
//! template onto a Model, or adds a stack to the copy already there.
//!
//! The server runs it all: [`Abilities::step`] counts cooldowns, active
//! abilities and effects down from the runtime's `step_health`, which a
//! device following a server doesn't call. What it fires waits in
//! [`Abilities::pending`] (and Health's events in [`Abilities::health`])
//! for the runtime to fire on both sides.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use axcenity_core::input::{InputMap, InputState};
use axcenity_core::{ClassName, InstanceId, Value, World};
use mlua::{FromLua, Function, Lua};

use axcenity_script::system::{self, HealthEvent, destroyed, type_name};
use axcenity_script::{LuaInstance, Side, lent_world as world, lent_world_mut as world_mut};

/// Something an Ability did, waiting to fire on both sides.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AbilityEvent {
    /// `Activated(player)`: the Player who asked, nil for a Script.
    Activated {
        ability: InstanceId,
        player: Option<InstanceId>,
    },
    /// `Ended(cancelled)`.
    Ended {
        ability: InstanceId,
        cancelled: bool,
    },
    /// `CooldownChanged(left)`, as a cooldown starts and as it ends.
    CooldownChanged { ability: InstanceId, left: f64 },
}

/// Why an Ability can't activate now, worded for `CanActivate`'s second
/// value.
#[derive(Clone, Debug, PartialEq)]
pub enum Blocked {
    /// It isn't held by a Model or Player in the game.
    NoHolder,
    Cooldown(f64),
    Active,
    Tag(String),
    /// Its holders haven't `amount` of the attribute.
    Cost {
        attribute: String,
        amount: f64,
    },
}

impl Blocked {
    pub fn reason(&self) -> String {
        match self {
            Blocked::NoHolder => "it isn't held by a Model or Player in the game".to_owned(),
            Blocked::Cooldown(left) => format!("it's cooling down ({left:.1} s left)"),
            Blocked::Active => "it's already active".to_owned(),
            Blocked::Tag(tag) => format!("its holder has the {tag} tag"),
            Blocked::Cost { attribute, amount } => {
                format!("its holder hasn't {amount} {attribute}")
            }
        }
    }
}

/// A player asking the server to activate an Ability.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Request {
    pub ability: InstanceId,
    pub player: InstanceId,
}

/// Where an Effect's change goes.
#[derive(Clone, Debug, PartialEq)]
enum Place {
    /// A number property of an Instance: a Health or a character
    /// controller.
    Property(InstanceId, String),
    /// A number attribute of the Model.
    Attribute(InstanceId, String),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Operation {
    Add,
    Multiply,
}

/// An active Effect, as the server follows it.
#[derive(Clone, Debug)]
struct EffectState {
    /// The Model it's in.
    target: InstanceId,
    place: Option<Place>,
    operation: Operation,
    /// Seconds since it started or was last stacked.
    elapsed: f64,
    /// Seconds since its last periodic change.
    since_tick: f64,
    /// The lasting change it made, to undo: the amount added, or the
    /// factor multiplied by.
    applied: Option<f64>,
}

/// A tag something gave a holder, so it's taken off only when nothing
/// else gives it.
#[derive(Clone, Debug, PartialEq)]
struct Grant {
    source: InstanceId,
    holder: InstanceId,
    tag: String,
}

/// The server's (and a client's predicted) ability state.
#[derive(Default)]
pub struct Abilities {
    /// Seconds each active Ability has left.
    active: HashMap<InstanceId, f64>,
    effects: HashMap<InstanceId, EffectState>,
    grants: Vec<Grant>,
    /// Players' requests, waiting for the server's next step.
    pub requests: Vec<Request>,
    /// On a player's device: cooldowns started there at once, before the
    /// server's arrive, so the ability bar sweeps without waiting.
    predicted: HashMap<InstanceId, f64>,
    /// The actions held last frame, so a key activates once per press.
    held: HashSet<String>,
    /// Events waiting to be fired on both sides.
    pub pending: Vec<AbilityEvent>,
    /// Health events Effects caused, for the runtime's Health events.
    pub health: Vec<HealthEvent>,
}

/// Both sides' handle on the one [`Abilities`].
#[derive(Clone, Default)]
pub struct SharedAbilities(pub Rc<RefCell<Abilities>>);

fn number(world: &World, id: InstanceId, name: &str) -> f64 {
    world
        .get(id)
        .and_then(|instance| instance.property(name))
        .and_then(Value::as_number)
        .filter(|n| n.is_finite())
        .unwrap_or(0.0)
}

fn text(world: &World, id: InstanceId, name: &str) -> String {
    world
        .get(id)
        .and_then(|instance| instance.property(name))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_owned()
}

fn class_of(world: &World, id: InstanceId) -> Option<ClassName> {
    world.get(id).map(|instance| instance.class())
}

/// The tags a comma-separated property lists.
pub fn tag_list(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(str::to_owned)
        .collect()
}

fn character_of(world: &World, player: InstanceId) -> Option<InstanceId> {
    match world.get(player)?.property("Character")? {
        Value::Instance(Some(character)) if world.contains(*character) => Some(*character),
        _ => None,
    }
}

/// Who holds `ability`: its Model or Player parent first, then that
/// Player's character or that character's Player. Empty outside the game.
pub fn holders(world: &World, ability: InstanceId) -> Vec<InstanceId> {
    let Some(parent) = world.get(ability).and_then(|instance| instance.parent()) else {
        return Vec::new();
    };
    if !world.is_descendant_of(parent, world.root()) {
        return Vec::new();
    }
    match class_of(world, parent) {
        Some(ClassName::Player) => {
            let mut holders = vec![parent];
            holders.extend(character_of(world, parent));
            holders
        }
        Some(ClassName::Model) => {
            let mut holders = vec![parent];
            holders.extend(system::player_of(world, parent));
            holders
        }
        _ => Vec::new(),
    }
}

/// The holder an Ability gives its tags to: the character, if there is
/// one, else its parent.
fn tag_holder(world: &World, holders: &[InstanceId]) -> Option<InstanceId> {
    holders
        .iter()
        .copied()
        .find(|&id| class_of(world, id) == Some(ClassName::Model))
        .or_else(|| holders.first().copied())
}

/// Whether `player` may ask for `ability`: it's theirs or their
/// character's.
pub fn belongs_to(world: &World, ability: InstanceId, player: InstanceId) -> bool {
    holders(world, ability).contains(&player)
}

/// The holder that pays the cost, and what it has: the first with a
/// number attribute `name`.
fn payer(world: &World, holders: &[InstanceId], name: &str) -> Option<(InstanceId, f64)> {
    holders.iter().find_map(|&holder| {
        let value = world.get(holder)?.attribute(name)?.as_number()?;
        Some((holder, value))
    })
}

/// Whether `ability` can activate now, and if not, why.
pub fn can_activate(world: &World, ability: InstanceId) -> Result<(), Blocked> {
    let holders = holders(world, ability);
    if holders.is_empty() || class_of(world, ability) != Some(ClassName::Ability) {
        return Err(Blocked::NoHolder);
    }
    let left = number(world, ability, "CooldownLeft");
    if left > 0.0 {
        return Err(Blocked::Cooldown(left));
    }
    if world
        .get(ability)
        .and_then(|instance| instance.property("Active"))
        .and_then(Value::as_bool)
        == Some(true)
    {
        return Err(Blocked::Active);
    }
    for tag in tag_list(&text(world, ability, "BlockedByTags")) {
        if holders
            .iter()
            .any(|&holder| world.get(holder).is_some_and(|h| h.has_tag(&tag)))
        {
            return Err(Blocked::Tag(tag));
        }
    }
    let attribute = text(world, ability, "CostAttribute");
    let amount = number(world, ability, "CostAmount");
    if !attribute.is_empty() && amount > 0.0 {
        let has = payer(world, &holders, &attribute).map_or(0.0, |(_, value)| value);
        if has < amount {
            return Err(Blocked::Cost { attribute, amount });
        }
    }
    Ok(())
}

fn operation(world: &World, effect: InstanceId) -> Operation {
    match world
        .get(effect)
        .and_then(|instance| instance.property("Operation"))
    {
        Some(Value::EnumItem(item)) if item.item == "Multiply" => Operation::Multiply,
        _ => Operation::Add,
    }
}

/// Where an Effect on `model` changing `name` changes it: its Health's
/// property, its character controller's, or its own attribute.
fn place(world: &World, model: InstanceId, name: &str) -> Option<Place> {
    if name.is_empty() {
        return None;
    }
    let has_number = |id: InstanceId| {
        world
            .get(id)
            .and_then(|instance| instance.property(name))
            .is_some_and(|value| value.as_number().is_some())
    };
    let children = world.get(model)?.children().to_vec();
    let health = children
        .iter()
        .copied()
        .find(|&child| class_of(world, child) == Some(ClassName::Health));
    if let Some(health) = health.filter(|&health| has_number(health)) {
        return Some(Place::Property(health, name.to_owned()));
    }
    let controller = children
        .iter()
        .copied()
        .find(|&child| class_of(world, child).is_some_and(ClassName::is_character_controller));
    if let Some(controller) = controller.filter(|&controller| has_number(controller)) {
        return Some(Place::Property(controller, name.to_owned()));
    }
    Some(Place::Attribute(model, name.to_owned()))
}

fn read(world: &World, place: &Place) -> Option<f64> {
    match place {
        Place::Property(id, name) => world.get(*id)?.property(name)?.as_number(),
        Place::Attribute(id, name) => {
            let instance = world.get(*id)?;
            match instance.attribute(name) {
                Some(value) => value.as_number(),
                None => Some(0.0),
            }
        }
    }
}

fn is_health_value(world: &World, place: &Place) -> Option<InstanceId> {
    match place {
        Place::Property(id, name)
            if name == "Health" && class_of(world, *id) == Some(ClassName::Health) =>
        {
            Some(*id)
        }
        _ => None,
    }
}

/// Sets `place` to `to`, as setting it from a script would: a Health's
/// `Health` through [`system::set_health`].
fn write(world: &mut World, place: &Place, to: f64) -> Vec<HealthEvent> {
    if !to.is_finite() {
        return Vec::new();
    }
    if let Some(health) = is_health_value(world, place) {
        return system::set_health(world, health, to);
    }
    match place {
        Place::Property(id, name) => {
            let _ = world.set_property(*id, name, Value::Number(to));
        }
        Place::Attribute(id, name) => {
            let _ = world.set_attribute(*id, name, Some(Value::Number(to)));
        }
    }
    Vec::new()
}

/// Changes `place` by `amount` (`stacks` times), for good: on a Health's
/// `Health`, adding less than 0 is damage from `source` and more is a heal.
/// Returns the change made (the amount added, or the factor) and Health's
/// events.
fn change(
    world: &mut World,
    place: &Place,
    operation: Operation,
    amount: f64,
    stacks: f64,
    source: InstanceId,
) -> (f64, Vec<HealthEvent>) {
    let Some(now) = read(world, place) else {
        return (identity(operation), Vec::new());
    };
    match operation {
        Operation::Add => {
            let by = amount * stacks;
            if let Some(health) = is_health_value(world, place) {
                let events = if by < 0.0 {
                    system::damage(world, health, -by, Some(source))
                } else {
                    system::heal(world, health, by)
                };
                let after = read(world, place).unwrap_or(now);
                return (after - now, events);
            }
            (by, write(world, place, now + by))
        }
        Operation::Multiply => {
            let factor = amount.powf(stacks);
            (factor, write(world, place, now * factor))
        }
    }
}

fn identity(operation: Operation) -> f64 {
    match operation {
        Operation::Add => 0.0,
        Operation::Multiply => 1.0,
    }
}

/// Takes back a lasting change: subtracts what was added, or divides by
/// the factor.
fn undo(world: &mut World, place: &Place, operation: Operation, applied: f64) -> Vec<HealthEvent> {
    let Some(now) = read(world, place) else {
        return Vec::new();
    };
    let to = match operation {
        Operation::Add => now - applied,
        Operation::Multiply if applied != 0.0 => now / applied,
        Operation::Multiply => return Vec::new(),
    };
    write(world, place, to)
}

impl Abilities {
    /// Activates `ability` for `player` (nil for a Script), if it can:
    /// spends the cost, starts the cooldown and, with a Duration, makes it
    /// active and gives its tags. Returns why not, otherwise.
    pub fn activate(
        &mut self,
        world: &mut World,
        ability: InstanceId,
        player: Option<InstanceId>,
    ) -> Result<(), Blocked> {
        can_activate(world, ability)?;
        let holders = holders(world, ability);
        let attribute = text(world, ability, "CostAttribute");
        let amount = number(world, ability, "CostAmount");
        if !attribute.is_empty()
            && amount > 0.0
            && let Some((holder, has)) = payer(world, &holders, &attribute)
        {
            let _ = world.set_attribute(holder, &attribute, Some(Value::Number(has - amount)));
        }
        self.pending
            .push(AbilityEvent::Activated { ability, player });
        let cooldown = number(world, ability, "Cooldown").max(0.0);
        if cooldown > 0.0 {
            let _ = world.set_property(ability, "CooldownLeft", Value::Number(cooldown));
            self.pending.push(AbilityEvent::CooldownChanged {
                ability,
                left: cooldown,
            });
        }
        let duration = number(world, ability, "Duration");
        if duration > 0.0 {
            let _ = world.set_property(ability, "Active", Value::Bool(true));
            self.active.insert(ability, duration);
            if let Some(holder) = tag_holder(world, &holders) {
                let tags = tag_list(&text(world, ability, "GrantsTags"));
                self.grant(world, ability, holder, &tags);
            }
        } else {
            self.pending.push(AbilityEvent::Ended {
                ability,
                cancelled: false,
            });
        }
        Ok(())
    }

    /// Ends an active `ability` early (`Cancel()`): `Ended(true)`. Nothing
    /// if it isn't active.
    pub fn cancel(&mut self, world: &mut World, ability: InstanceId) {
        if self.active.remove(&ability).is_some() {
            self.end(world, ability, true);
        }
    }

    fn end(&mut self, world: &mut World, ability: InstanceId, cancelled: bool) {
        let _ = world.set_property(ability, "Active", Value::Bool(false));
        self.release(world, ability);
        self.pending
            .push(AbilityEvent::Ended { ability, cancelled });
    }

    /// Gives `holder` each of `tags` for `source`.
    fn grant(
        &mut self,
        world: &mut World,
        source: InstanceId,
        holder: InstanceId,
        tags: &[String],
    ) {
        for tag in tags {
            let given_already = self
                .grants
                .iter()
                .any(|grant| grant.holder == holder && &grant.tag == tag);
            // A tag the holder had from elsewhere isn't ours to take off.
            if world.add_tag(holder, tag).unwrap_or(false) || given_already {
                self.grants.push(Grant {
                    source,
                    holder,
                    tag: tag.clone(),
                });
            }
        }
    }

    /// Takes off the tags `source` gave that nothing else still gives.
    fn release(&mut self, world: &mut World, source: InstanceId) {
        let (gone, kept): (Vec<Grant>, Vec<Grant>) = std::mem::take(&mut self.grants)
            .into_iter()
            .partition(|grant| grant.source == source);
        self.grants = kept;
        for grant in gone {
            let still = self
                .grants
                .iter()
                .any(|other| other.holder == grant.holder && other.tag == grant.tag);
            if !still {
                let _ = world.remove_tag(grant.holder, &grant.tag);
            }
        }
    }

    /// `model:ApplyEffect(template)`: adds a stack to the copy of it
    /// already on `model` (up to its MaxStacks) and starts its Duration
    /// over, or puts a new copy on `model`. An instant Effect changes at
    /// once and is gone, so it returns `None`.
    pub fn apply_effect(
        &mut self,
        world: &mut World,
        model: InstanceId,
        template: InstanceId,
    ) -> Option<InstanceId> {
        let name = world.get(template)?.name().to_owned();
        let existing = world.get(model)?.children().iter().copied().find(|&child| {
            child != template
                && self.effects.contains_key(&child)
                && world
                    .get(child)
                    .is_some_and(|c| c.class() == ClassName::Effect && c.name() == name)
        });
        if let Some(effect) = existing {
            let max = number(world, effect, "MaxStacks").max(1.0).floor();
            let stacks = (number(world, effect, "Stacks").max(1.0) + 1.0).min(max);
            let _ = world.set_property(effect, "Stacks", Value::Number(stacks));
            let mut state = self.effects.remove(&effect)?;
            state.elapsed = 0.0;
            // A lasting change follows the new stacks.
            if let (Some(place), Some(applied)) = (state.place.clone(), state.applied) {
                self.health
                    .extend(undo(world, &place, state.operation, applied));
                let amount = number(world, effect, "Amount");
                let (made, events) = change(world, &place, state.operation, amount, stacks, effect);
                state.applied = Some(made);
                self.health.extend(events);
            }
            self.effects.insert(effect, state);
            let duration = number(world, effect, "Duration");
            let _ = world.set_property(effect, "TimeLeft", Value::Number(duration.max(0.0)));
            return Some(effect);
        }
        let copy = world.duplicate(template).ok()?;
        let _ = world.set_property(copy, "Stacks", Value::Number(1.0));
        world.set_parent(copy, Some(model)).ok()?;
        self.start(world, copy, model);
        world.contains(copy).then_some(copy)
    }

    /// An Effect that's just come into `model`: an instant one changes and
    /// is destroyed; a lasting one changes now; each gives its tags.
    fn start(&mut self, world: &mut World, effect: InstanceId, model: InstanceId) {
        let name = text(world, effect, "Attribute");
        let place = place(world, model, &name);
        let operation = operation(world, effect);
        let amount = number(world, effect, "Amount");
        let stacks = number(world, effect, "Stacks").max(1.0);
        let duration = number(world, effect, "Duration");
        let period = number(world, effect, "Period");
        if duration == 0.0 {
            if let Some(place) = &place {
                let (_, events) = change(world, place, operation, amount, stacks, effect);
                self.health.extend(events);
            }
            let _ = world.destroy(effect);
            return;
        }
        let mut state = EffectState {
            target: model,
            place,
            operation,
            elapsed: 0.0,
            since_tick: 0.0,
            applied: None,
        };
        if period <= 0.0
            && let Some(place) = &state.place
        {
            let (made, events) = change(world, place, operation, amount, stacks, effect);
            state.applied = Some(made);
            self.health.extend(events);
        }
        let tags = tag_list(&text(world, effect, "GrantsTags"));
        self.grant(world, effect, model, &tags);
        let _ = world.set_property(effect, "TimeLeft", Value::Number(duration.max(0.0)));
        self.effects.insert(effect, state);
    }

    /// An Effect that ended or was removed: its lasting change is undone
    /// and its tags taken off.
    fn stop(&mut self, world: &mut World, effect: InstanceId) {
        let Some(state) = self.effects.remove(&effect) else {
            return;
        };
        if let (Some(place), Some(applied)) = (&state.place, state.applied) {
            self.health
                .extend(undo(world, place, state.operation, applied));
        }
        self.release(world, effect);
    }

    /// The server's step of `seconds`: players' requests, then cooldowns,
    /// active abilities and Effects. What fires waits in `pending` and
    /// `health`.
    pub fn step(&mut self, world: &mut World, seconds: f64) {
        let dt = if seconds.is_finite() {
            seconds.max(0.0)
        } else {
            0.0
        };
        self.serve_requests(world);
        let mut abilities = Vec::new();
        let mut effects = Vec::new();
        for place in [world.workspace(), world.players()] {
            for id in world.descendants_iter(place) {
                match class_of(world, id) {
                    Some(ClassName::Ability) => abilities.push(id),
                    Some(ClassName::Effect) => effects.push(id),
                    _ => {}
                }
            }
        }
        for &ability in &abilities {
            let left = number(world, ability, "CooldownLeft");
            if left > 0.0 {
                let now = (left - dt).max(0.0);
                let _ = world.set_property(ability, "CooldownLeft", Value::Number(now));
                if now == 0.0 {
                    self.pending
                        .push(AbilityEvent::CooldownChanged { ability, left: 0.0 });
                }
            }
        }
        // Active abilities count down, and end when they're out of time
        // or gone.
        let mut ended = Vec::new();
        self.active.retain(|&ability, left| {
            *left -= dt;
            let keep = *left > 0.0 && world.contains(ability);
            if !keep {
                ended.push(ability);
            }
            keep
        });
        for ability in ended {
            if world.contains(ability) {
                self.end(world, ability, false);
            } else {
                self.release(world, ability);
            }
        }
        // Effects that went (destroyed, or out of their Model) stop first.
        let present: HashSet<InstanceId> = effects.iter().copied().collect();
        let gone: Vec<InstanceId> = self
            .effects
            .iter()
            .filter(|(id, state)| {
                !present.contains(id)
                    || world.get(**id).and_then(|effect| effect.parent()) != Some(state.target)
            })
            .map(|(id, _)| *id)
            .collect();
        for effect in gone {
            self.stop(world, effect);
        }
        for effect in effects {
            if self.effects.contains_key(&effect) {
                self.tick(world, effect, dt);
                continue;
            }
            let Some(model) = world.get(effect).and_then(|e| e.parent()) else {
                continue;
            };
            if class_of(world, model) == Some(ClassName::Model) {
                self.start(world, effect, model);
            }
        }
    }

    /// One Effect's `dt`: its periodic changes, then its end.
    fn tick(&mut self, world: &mut World, effect: InstanceId, dt: f64) {
        // A little slack, so the last period of a whole number of them
        // lands before the end despite rounding.
        const SLACK: f64 = 1e-6;
        let duration = number(world, effect, "Duration");
        let period = number(world, effect, "Period");
        let Some(state) = self.effects.get_mut(&effect) else {
            return;
        };
        state.elapsed += dt;
        let elapsed = state.elapsed;
        let mut ticks = 0;
        if period > 0.0 {
            state.since_tick += dt;
            while state.since_tick + SLACK >= period {
                state.since_tick -= period;
                ticks += 1;
            }
        }
        let (place, operation) = (state.place.clone(), state.operation);
        if let Some(place) = place {
            let amount = number(world, effect, "Amount");
            let stacks = number(world, effect, "Stacks").max(1.0);
            for _ in 0..ticks {
                let (_, events) = change(world, &place, operation, amount, stacks, effect);
                self.health.extend(events);
            }
        }
        if duration > 0.0 {
            let left = (duration - elapsed).max(0.0);
            let _ = world.set_property(effect, "TimeLeft", Value::Number(left));
            if elapsed + SLACK >= duration {
                self.stop(world, effect);
                let _ = world.destroy(effect);
            }
        }
    }

    /// Activates what players asked for, for those it belongs to.
    pub fn serve_requests(&mut self, world: &mut World) {
        for request in std::mem::take(&mut self.requests) {
            if belongs_to(world, request.ability, request.player) {
                let _ = self.activate(world, request.ability, Some(request.player));
            }
        }
    }

    /// On a player's device: predicted cooldowns count down.
    pub fn step_predicted(&mut self, seconds: f64) {
        self.predicted.retain(|_, left| {
            *left -= seconds;
            *left > 0.0
        });
    }

    /// Starts a predicted cooldown for `ability`, so the ability bar
    /// sweeps at once.
    pub fn predict(&mut self, world: &World, ability: InstanceId) {
        let cooldown = number(world, ability, "Cooldown");
        if cooldown > 0.0 {
            self.predicted.insert(ability, cooldown);
        }
    }

    /// The cooldown the ability bar shows: the server's, or the one this
    /// device predicted, whichever is longer.
    pub fn shown_cooldown(&self, world: &World, ability: InstanceId) -> f64 {
        let left = number(world, ability, "CooldownLeft");
        left.max(self.predicted.get(&ability).copied().unwrap_or(0.0))
    }

    /// The abilities whose `ActivationKey` action was pressed this frame
    /// (not held from before) among `abilities`.
    pub fn pressed(
        &mut self,
        world: &World,
        abilities: &[InstanceId],
        map: &InputMap,
        state: &InputState,
    ) -> Vec<InstanceId> {
        let mut held = HashSet::new();
        let mut pressed = Vec::new();
        for &ability in abilities {
            let action = text(world, ability, "ActivationKey");
            if action.is_empty() || !map.is_pressed(&action, state) {
                continue;
            }
            if !self.held.contains(&action) {
                pressed.push(ability);
            }
            held.insert(action);
        }
        self.held = held;
        pressed
    }
}

/// The Abilities `player` holds (theirs and their character's), in tree
/// order.
pub fn player_abilities(world: &World, player: InstanceId) -> Vec<InstanceId> {
    let mut places = vec![player];
    places.extend(character_of(world, player));
    places
        .into_iter()
        .flat_map(|place| {
            world
                .get(place)
                .map(|p| p.children().to_vec())
                .unwrap_or_default()
        })
        .filter(|&child| class_of(world, child) == Some(ClassName::Ability))
        .collect()
}

fn shared(lua: &Lua) -> mlua::Result<SharedAbilities> {
    lua.app_data_ref::<SharedAbilities>()
        .map(|shared| shared.clone())
        .ok_or_else(|| mlua::Error::runtime("Abilities aren't set up on this side"))
}

fn side(lua: &Lua) -> Side {
    system::side(lua)
}

fn ability_arg(lua: &Lua, value: mlua::Value, call: &str) -> mlua::Result<InstanceId> {
    LuaInstance::from_lua(value.clone(), lua)
        .map(|instance| instance.0)
        .map_err(|_| {
            mlua::Error::runtime(format!(
                "{call} needs an Effect, not {}",
                type_name(lua, &value)
            ))
        })
}

/// `ability:CanActivate()`, `Activate()` and `Cancel()`, and
/// `model:ApplyEffect(effect)`.
pub fn methods(lua: &Lua) -> mlua::Result<Vec<(ClassName, &'static str, Function)>> {
    let can = lua.create_function(|lua, this: LuaInstance| {
        let world = world(lua)?;
        world.get(this.0).ok_or_else(destroyed)?;
        Ok(match can_activate(&world, this.0) {
            Ok(()) => (true, None),
            Err(blocked) => (false, Some(blocked.reason())),
        })
    })?;
    let activate = lua.create_function(|lua, this: LuaInstance| {
        let shared = shared(lua)?;
        let mut world = world_mut(lua)?;
        world.get(this.0).ok_or_else(destroyed)?;
        match side(lua) {
            Side::Server => Ok(shared
                .0
                .borrow_mut()
                .activate(&mut world, this.0, None)
                .is_ok()),
            // A LocalScript asks the server, which checks again; the bar
            // sweeps at once.
            Side::Client => {
                let Some(player) = system::local_player(lua) else {
                    return Ok(false);
                };
                if !belongs_to(&world, this.0, player) || can_activate(&world, this.0).is_err() {
                    return Ok(false);
                }
                shared.0.borrow_mut().predict(&world, this.0);
                drop(world);
                system::send_activation(lua, this.0, player);
                Ok(true)
            }
        }
    })?;
    let cancel = lua.create_function(|lua, this: LuaInstance| {
        if side(lua) == Side::Client {
            return Err(mlua::Error::runtime(
                "Cancel can only be called from a Script: abilities run on the server",
            ));
        }
        let shared = shared(lua)?;
        let mut world = world_mut(lua)?;
        world.get(this.0).ok_or_else(destroyed)?;
        shared.0.borrow_mut().cancel(&mut world, this.0);
        Ok(())
    })?;
    let apply = lua.create_function(|lua, (this, effect): (LuaInstance, mlua::Value)| {
        let call = "ApplyEffect(effect)";
        let effect = ability_arg(lua, effect, call)?;
        if side(lua) == Side::Client {
            return Err(mlua::Error::runtime(
                "ApplyEffect can only be called from a Script: effects run on the server",
            ));
        }
        let shared = shared(lua)?;
        let mut world = world_mut(lua)?;
        world.get(this.0).ok_or_else(destroyed)?;
        if class_of(&world, effect) != Some(ClassName::Effect) {
            return Err(mlua::Error::runtime(format!(
                "{call} needs an Effect, not a {}",
                class_of(&world, effect).map_or("destroyed Instance", ClassName::as_str)
            )));
        }
        let applied = shared
            .0
            .borrow_mut()
            .apply_effect(&mut world, this.0, effect);
        Ok(applied.map(LuaInstance))
    })?;
    Ok(vec![
        (ClassName::Ability, "CanActivate", can),
        (ClassName::Ability, "Activate", activate),
        (ClassName::Ability, "Cancel", cancel),
        (ClassName::Model, "ApplyEffect", apply),
    ])
}

/// Properties the runtime keeps, which scripts only read.
pub fn read_only(class: ClassName, key: &str) -> Option<String> {
    match (class, key) {
        (ClassName::Ability, "CooldownLeft" | "Active") => Some(format!(
            "Ability.{key} is read-only: Activate starts the cooldown and Cancel ends it early"
        )),
        (ClassName::Effect, "Stacks" | "TimeLeft") => Some(format!(
            "Effect.{key} is read-only: model:ApplyEffect(effect) adds stacks and starts the time over"
        )),
        _ => None,
    }
}
