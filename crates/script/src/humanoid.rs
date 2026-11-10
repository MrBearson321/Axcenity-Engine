//! Humanoid (Phase 46): a Roblox-familiar front for a character's pieces.
//!
//! A Humanoid finds (or makes) its parent's character controller, Health,
//! Inventory and Animator, and stores nothing of its own but
//! `BreakJointsOnDeath` and `RagdollOnDeath`: its other properties read and
//! write the pieces' ([`CONTROLLER_FIELDS`], [`HEALTH_FIELDS`]), and its
//! methods call theirs. The runtime keeps what it needs between frames in
//! [`Humanoids`]: each `MoveTo` walk (along a path on the navigation mesh,
//! by the controller's `MoveDirection`), and each Humanoid's state, which
//! it works out from the pieces every frame and fires `StateChanged` for.

use std::collections::HashMap;

use axcenity_anim::read_state_machine;
use axcenity_assets::Assets;
use axcenity_character::Controllers;
use axcenity_core::{ClassName, InstanceId, PlayRequest, Value, Vector3, World};
use axcenity_nav::{MeshBuilder, NavMesh, NavSettings, Refresh};
use mlua::{FromLua, Function, Lua};

use crate::health::{self, HealthEvent};
use crate::instance::{LuaInstance, destroyed, world, world_mut};
use crate::types::{EnumType, LuaEnumItem, LuaVector3, type_name};

/// Humanoid properties that are its controller's.
pub const CONTROLLER_FIELDS: [&str; 7] = [
    "WalkSpeed",
    "RunSpeed",
    "JumpHeight",
    "Sit",
    "PlatformStand",
    "AutoRotate",
    "MoveDirection",
];
/// Humanoid properties that are its Health's.
pub const HEALTH_FIELDS: [&str; 4] = [
    "Health",
    "MaxHealth",
    "DisplayName",
    "HealthDisplayDistance",
];

/// A walk that hasn't arrived after this many seconds ends with
/// `MoveToFinished(false)`.
const MOVE_TIMEOUT: f32 = 8.0;
/// How near across the ground the feet come to a walk's end to arrive.
const ARRIVE: f32 = 0.5;
/// How near they come to a corner on the way before heading for the next.
const CORNER: f32 = 0.3;
/// A walk to a part plans again once the part has moved this far.
const REPLAN: f32 = 1.0;
/// What the mesh is built from is read again at most this often (seconds),
/// as agents' is.
const REBUILD_SECONDS: f32 = 0.5;

/// What a Humanoid's character is doing: `Enum.HumanoidStateType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// On the ground, standing or walking.
    Running,
    /// Rising after a jump.
    Jumping,
    /// Falling.
    Freefall,
    /// The frame it lands.
    Landed,
    /// On a Seat.
    Seated,
    /// Reserved for water, which there isn't yet.
    Swimming,
    /// A Ragdoll in it is on.
    Ragdoll,
    /// Its Health is 0.
    Dead,
}

impl State {
    pub const ALL: [State; 8] = [
        State::Running,
        State::Jumping,
        State::Freefall,
        State::Landed,
        State::Seated,
        State::Swimming,
        State::Ragdoll,
        State::Dead,
    ];

    pub fn name(self) -> &'static str {
        match self {
            State::Running => "Running",
            State::Jumping => "Jumping",
            State::Freefall => "Freefall",
            State::Landed => "Landed",
            State::Seated => "Seated",
            State::Swimming => "Swimming",
            State::Ragdoll => "Ragdoll",
            State::Dead => "Dead",
        }
    }

    fn item(self) -> LuaEnumItem {
        LuaEnumItem {
            enum_type: EnumType::HumanoidStateType,
            name: self.name(),
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|state| state.name() == name)
    }
}

/// Something a Humanoid did, waiting to be fired on both sides.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HumanoidEvent {
    /// `MoveToFinished(reached)`.
    MoveToFinished { humanoid: InstanceId, reached: bool },
    /// `StateChanged(old, new)`.
    StateChanged {
        humanoid: InstanceId,
        old: State,
        new: State,
    },
    /// `Died()`.
    Died { humanoid: InstanceId },
}

impl HumanoidEvent {
    pub fn humanoid(self) -> InstanceId {
        match self {
            HumanoidEvent::MoveToFinished { humanoid, .. }
            | HumanoidEvent::StateChanged { humanoid, .. }
            | HumanoidEvent::Died { humanoid } => humanoid,
        }
    }
}

/// What a script asked of a Humanoid's walk, for the runtime to take.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Request {
    /// `MoveTo(target, part?)`: with a part, the target keeps its offset
    /// from the part as it moves.
    MoveTo {
        humanoid: InstanceId,
        target: Vector3,
        part: Option<(InstanceId, Vector3)>,
    },
    /// `Move(direction)`: stop following any `MoveTo`.
    Stop { humanoid: InstanceId },
}

/// One side's scripts' requests, until the runtime collects them.
#[derive(Default)]
pub struct Requests(pub Vec<Request>);

/// Each Humanoid's state as the runtime last worked it out, for
/// `GetState`. The runtime copies it into both sides.
#[derive(Clone, Default)]
pub struct States(pub HashMap<InstanceId, State>);

/// The parent a Humanoid fronts: a Model, or a part.
fn holder(world: &World, humanoid: InstanceId) -> Option<InstanceId> {
    let parent = world.get(humanoid)?.parent()?;
    let class = world.get(parent)?.class();
    (class == ClassName::Model || class.is_base_part()).then_some(parent)
}

fn child_where(
    world: &World,
    holder: InstanceId,
    test: impl Fn(ClassName) -> bool,
) -> Option<InstanceId> {
    world
        .get(holder)?
        .children()
        .iter()
        .copied()
        .find(|&child| {
            world
                .get(child)
                .is_some_and(|instance| test(instance.class()))
        })
}

/// The character's pieces, as they are now.
#[derive(Clone, Copy, Debug, Default)]
pub struct Pieces {
    pub controller: Option<InstanceId>,
    pub health: Option<InstanceId>,
    pub inventory: Option<InstanceId>,
    pub animator: Option<InstanceId>,
}

impl Pieces {
    pub fn of(world: &World, holder: InstanceId) -> Self {
        let animator = world.descendants_iter(holder).find(|&id| {
            world
                .get(id)
                .is_some_and(|instance| instance.class() == ClassName::Animator)
                && axcenity_core::tool::tool_of(world, id).is_none()
        });
        Self {
            controller: child_where(world, holder, ClassName::is_character_controller),
            health: child_where(world, holder, |class| class == ClassName::Health),
            inventory: child_where(world, holder, |class| class == ClassName::Inventory),
            animator,
        }
    }
}

/// The pieces of the Humanoid's character, making each one it lacks in
/// its parent: a ThirdPersonController, a Health, an Inventory and an
/// Animator (with no state machine). `None` when it isn't in a Model or a
/// part.
pub fn ensure_pieces(world: &mut World, humanoid: InstanceId) -> Option<Pieces> {
    let holder = holder(world, humanoid)?;
    let mut pieces = Pieces::of(world, holder);
    let mut make = |class: ClassName| {
        let id = world.create(class).ok()?;
        world.set_name(id, class.as_str()).ok()?;
        world.set_parent(id, Some(holder)).ok()?;
        Some(id)
    };
    if pieces.controller.is_none() {
        pieces.controller = make(ClassName::ThirdPersonController);
    }
    if pieces.health.is_none() {
        pieces.health = make(ClassName::Health);
    }
    if pieces.inventory.is_none() {
        pieces.inventory = make(ClassName::Inventory);
    }
    if pieces.animator.is_none() {
        pieces.animator = make(ClassName::Animator);
    }
    Some(pieces)
}

/// The Humanoid in `holder`, if it has one.
fn humanoid_in(world: &World, holder: InstanceId) -> Option<InstanceId> {
    child_where(world, holder, |class| class == ClassName::Humanoid)
}

/// The piece and its property that the Humanoid property `key` is.
fn field(lua: &Lua, humanoid: InstanceId, key: &str) -> mlua::Result<(InstanceId, &'static str)> {
    let mut world = world_mut(lua)?;
    world.get(humanoid).ok_or_else(destroyed)?;
    let pieces = ensure_pieces(&mut world, humanoid).ok_or_else(|| {
        mlua::Error::runtime(format!(
            "Humanoid.{key} needs the Humanoid to be in a character's Model"
        ))
    })?;
    let (piece, name) = if let Some(&name) = CONTROLLER_FIELDS.iter().find(|&&f| f == key) {
        (pieces.controller, name)
    } else if let Some(&name) = HEALTH_FIELDS.iter().find(|&&f| f == key) {
        (pieces.health, name)
    } else {
        return Err(mlua::Error::runtime(format!(
            "{key} isn't a Humanoid field"
        )));
    };
    piece
        .map(|piece| (piece, name))
        .ok_or_else(|| mlua::Error::runtime(format!("Humanoid.{key}'s piece couldn't be made")))
}

/// Whether `key` is a property a Humanoid reads from its pieces.
pub fn is_field(key: &str) -> bool {
    CONTROLLER_FIELDS.contains(&key) || HEALTH_FIELDS.contains(&key)
}

/// Reading `humanoid.<key>`: the piece's property.
pub fn get(lua: &Lua, humanoid: InstanceId, key: &str) -> mlua::Result<mlua::Value> {
    let (piece, name) = field(lua, humanoid, key)?;
    crate::instance::get(lua, piece, name)
}

/// Writing `humanoid.<key>`: the piece's property, as setting it there
/// would (a Health's `Health` fires `HealthChanged`).
pub fn set(lua: &Lua, humanoid: InstanceId, key: &str, value: mlua::Value) -> mlua::Result<()> {
    let (piece, name) = field(lua, humanoid, key)?;
    crate::instance::set(lua, piece, name, value)
}

fn requests_mut(lua: &Lua) -> mlua::Result<mlua::AppDataRefMut<'_, Requests>> {
    lua.app_data_mut::<Requests>()
        .ok_or_else(|| mlua::Error::runtime("Humanoids aren't set up on this side"))
}

/// The pieces of a Humanoid a method is called on.
fn pieces_for(lua: &Lua, humanoid: InstanceId, call: &str) -> mlua::Result<Pieces> {
    let mut world = world_mut(lua)?;
    world.get(humanoid).ok_or_else(destroyed)?;
    ensure_pieces(&mut world, humanoid).ok_or_else(|| {
        mlua::Error::runtime(format!(
            "{call} needs the Humanoid to be in a character's Model"
        ))
    })
}

fn vector_arg(lua: &Lua, value: mlua::Value, call: &str) -> mlua::Result<Vector3> {
    LuaVector3::from_lua(value.clone(), lua)
        .map(|v| v.0)
        .map_err(|_| {
            mlua::Error::runtime(format!(
                "{call} needs a Vector3, not {}",
                type_name(lua, &value)
            ))
        })
}

fn set_direction(world: &mut World, controller: InstanceId, x: f32, z: f32) {
    let _ = world.set_property(
        controller,
        "MoveDirection",
        Value::Vector3(Vector3::new(x, 0.0, z)),
    );
}

fn position(world: &World, id: InstanceId) -> Option<Vector3> {
    world.get(id)?.property("Position")?.as_vector3()
}

/// The camera's turn about Y on this side, in radians, if it has one.
fn camera_yaw(lua: &Lua) -> Option<f32> {
    let camera = lua.app_data_ref::<crate::camera::Camera>()?;
    Some(camera.state.orientation.y.to_radians())
}

/// Turns every Ragdoll in `holder` on or off.
fn set_ragdolls(world: &mut World, holder: InstanceId, on: bool) -> bool {
    let ragdolls: Vec<InstanceId> = world
        .descendants_iter(holder)
        .filter(|&id| {
            world
                .get(id)
                .is_some_and(|instance| instance.class() == ClassName::Ragdoll)
        })
        .collect();
    for &ragdoll in &ragdolls {
        let _ = world.set_property(ragdoll, "Enabled", Value::Bool(on));
    }
    !ragdolls.is_empty()
}

fn ragdolled(world: &World, holder: InstanceId) -> bool {
    world.descendants_iter(holder).any(|id| {
        world.get(id).is_some_and(|instance| {
            instance.class() == ClassName::Ragdoll
                && instance.property("Enabled").and_then(Value::as_bool) == Some(true)
        })
    })
}

fn flag(world: &World, id: InstanceId, name: &str) -> bool {
    world
        .get(id)
        .and_then(|instance| instance.property(name))
        .and_then(Value::as_bool)
        == Some(true)
}

fn number(world: &World, id: InstanceId, name: &str) -> f64 {
    world
        .get(id)
        .and_then(|instance| instance.property(name))
        .and_then(Value::as_number)
        .filter(|n| n.is_finite())
        .unwrap_or(0.0)
}

fn state_arg(lua: &Lua, value: mlua::Value) -> mlua::Result<State> {
    let name = match &value {
        mlua::Value::String(name) => name.to_string_lossy(),
        _ => match LuaEnumItem::from_lua(value.clone(), lua) {
            Ok(item) if item.enum_type == EnumType::HumanoidStateType => item.name.to_owned(),
            _ => {
                return Err(mlua::Error::runtime(format!(
                    "ChangeState(state) needs an Enum.HumanoidStateType item, not {}",
                    type_name(lua, &value)
                )));
            }
        },
    };
    State::from_name(&name).ok_or_else(|| {
        mlua::Error::runtime(format!(
            "{name} is not a valid member of Enum.HumanoidStateType"
        ))
    })
}

pub fn methods(lua: &Lua) -> mlua::Result<Vec<(ClassName, &'static str, Function)>> {
    let move_to = lua.create_function(
        |lua, (this, target, part): (LuaInstance, mlua::Value, mlua::Value)| {
            let call = "MoveTo(position, part)";
            let target = vector_arg(lua, target, call)?;
            pieces_for(lua, this.0, call)?;
            let part = match part {
                mlua::Value::Nil => None,
                value => {
                    let part = LuaInstance::from_lua(value.clone(), lua).map_err(|_| {
                        mlua::Error::runtime(format!(
                            "{call}'s part must be a part or nil, not {}",
                            type_name(lua, &value)
                        ))
                    })?;
                    let world = world(lua)?;
                    let at = position(&world, part.0).ok_or_else(|| {
                        mlua::Error::runtime(format!("{call}'s part must be a part"))
                    })?;
                    Some((
                        part.0,
                        Vector3::new(target.x - at.x, target.y - at.y, target.z - at.z),
                    ))
                }
            };
            requests_mut(lua)?.0.push(Request::MoveTo {
                humanoid: this.0,
                target,
                part,
            });
            Ok(())
        },
    )?;
    let walk = lua.create_function(
        |lua, (this, direction, relative): (LuaInstance, mlua::Value, Option<bool>)| {
            let call = "Move(direction, relativeToCamera)";
            let direction = vector_arg(lua, direction, call)?;
            let pieces = pieces_for(lua, this.0, call)?;
            let (mut x, mut z) = (direction.x, direction.z);
            if relative == Some(true)
                && let Some(yaw) = camera_yaw(lua)
            {
                let (sin, cos) = yaw.sin_cos();
                (x, z) = (x * cos + z * sin, -x * sin + z * cos);
            }
            if let Some(controller) = pieces.controller {
                set_direction(&mut *world_mut(lua)?, controller, x, z);
            }
            requests_mut(lua)?
                .0
                .push(Request::Stop { humanoid: this.0 });
            Ok(())
        },
    )?;
    let jump = lua.create_function(|lua, this: LuaInstance| {
        let pieces = pieces_for(lua, this.0, "Jump()")?;
        if let Some(controller) = pieces.controller {
            let _ = world_mut(lua)?.set_property(controller, "Jump", Value::Bool(true));
        }
        Ok(())
    })?;
    let take_damage = lua.create_function(
        |lua, (this, amount, source): (LuaInstance, mlua::Value, Option<LuaInstance>)| {
            let call = "TakeDamage(amount)";
            let amount = health::amount_arg(lua, amount, call)?;
            let pieces = pieces_for(lua, this.0, call)?;
            let Some(id) = pieces.health else {
                return Ok(());
            };
            let events = health::damage(&mut *world_mut(lua)?, id, amount, source.map(|s| s.0));
            health::push(lua, events);
            Ok(())
        },
    )?;
    let equip = lua.create_function(|lua, (this, tool): (LuaInstance, mlua::Value)| {
        let call = "EquipTool(tool)";
        let tool = crate::tool::tool_arg(lua, tool, call)?;
        let pieces = pieces_for(lua, this.0, call)?;
        let Some(inventory) = pieces.inventory else {
            return Ok(());
        };
        let assets = lua
            .app_data_ref::<Assets>()
            .map(|assets| assets.clone())
            .unwrap_or_default();
        let mut world = world_mut(lua)?;
        let mut changes = crate::tool::changes_mut(lua)?;
        if !crate::tool::add(&mut world, inventory, tool, &mut changes)
            .map_err(mlua::Error::runtime)?
        {
            return Err(mlua::Error::runtime(
                "EquipTool(tool): the character's Inventory is full",
            ));
        }
        crate::tool::equip(&mut world, &assets, inventory, tool, &mut changes)
            .map_err(mlua::Error::runtime)
    })?;
    let unequip = lua.create_function(|lua, this: LuaInstance| {
        let pieces = pieces_for(lua, this.0, "UnequipTools()")?;
        if let Some(inventory) = pieces.inventory {
            let mut world = world_mut(lua)?;
            let mut changes = crate::tool::changes_mut(lua)?;
            crate::tool::unequip_all(&mut world, inventory, &mut changes);
        }
        Ok(())
    })?;
    let get_state = lua.create_function(|lua, this: LuaInstance| {
        world(lua)?.get(this.0).ok_or_else(destroyed)?;
        let state = lua
            .app_data_ref::<States>()
            .and_then(|states| states.0.get(&this.0).copied())
            .unwrap_or(State::Running);
        Ok(state.item())
    })?;
    let change_state = lua.create_function(|lua, (this, state): (LuaInstance, mlua::Value)| {
        let state = state_arg(lua, state)?;
        let pieces = pieces_for(lua, this.0, "ChangeState(state)")?;
        let holder = holder(&*world(lua)?, this.0).ok_or_else(destroyed)?;
        let mut world = world_mut(lua)?;
        match state {
            State::Running => {
                if let Some(controller) = pieces.controller {
                    let _ = world.set_property(controller, "Sit", Value::Bool(false));
                    let _ = world.set_property(controller, "PlatformStand", Value::Bool(false));
                }
                set_ragdolls(&mut world, holder, false);
            }
            State::Jumping => {
                if let Some(controller) = pieces.controller {
                    let _ = world.set_property(controller, "Jump", Value::Bool(true));
                }
            }
            State::Ragdoll => {
                if !set_ragdolls(&mut world, holder, true) {
                    return Err(mlua::Error::runtime(
                        "ChangeState(Ragdoll) needs a Ragdoll in the character",
                    ));
                }
            }
            State::Dead => {
                if let Some(id) = pieces.health {
                    let events = health::set_health(&mut world, id, 0.0);
                    drop(world);
                    health::push(lua, events);
                }
            }
            other => {
                return Err(mlua::Error::runtime(format!(
                    "ChangeState can't make a Humanoid {}: that follows the character \
                     (it's Running, Jumping, Ragdoll or Dead that can be asked for)",
                    other.name()
                )));
            }
        }
        Ok(())
    })?;
    Ok(vec![
        (ClassName::Humanoid, "MoveTo", move_to),
        (ClassName::Humanoid, "Move", walk),
        (ClassName::Humanoid, "Jump", jump),
        (ClassName::Humanoid, "TakeDamage", take_damage),
        (ClassName::Humanoid, "EquipTool", equip),
        (ClassName::Humanoid, "UnequipTools", unequip),
        (ClassName::Humanoid, "GetState", get_state),
        (ClassName::Humanoid, "ChangeState", change_state),
    ])
}

/// One `MoveTo` being walked.
struct Walk {
    target: Vector3,
    part: Option<(InstanceId, Vector3)>,
    /// The path's corners after the start, and where it was planned to.
    corners: Vec<Vector3>,
    planned_for: Option<Vector3>,
    elapsed: f32,
}

impl Walk {
    fn goal(&self, world: &World) -> Vector3 {
        match self.part {
            Some((part, offset)) => position(world, part).map_or(self.target, |at| {
                Vector3::new(at.x + offset.x, at.y + offset.y, at.z + offset.z)
            }),
            None => self.target,
        }
    }
}

struct Built {
    builder: MeshBuilder,
    age: f32,
}

/// What the runtime keeps between frames for every Humanoid.
#[derive(Default)]
pub struct Humanoids {
    walks: HashMap<InstanceId, Walk>,
    pub states: States,
    /// The Animator state each last asked for (or `None` while seated,
    /// dead or ragdolled), and its feet last frame, to tell walking from
    /// standing.
    playing: HashMap<InstanceId, Option<&'static str>>,
    feet: HashMap<InstanceId, Vector3>,
    built: Option<Built>,
}

fn across(a: Vector3, b: Vector3) -> f32 {
    (b.x - a.x).hypot(b.z - a.z)
}

fn feet_of(world: &World, holder: InstanceId) -> Option<Vector3> {
    axcenity_nav::feet_of(world, &axcenity_character::body_parts(world, holder))
}

impl Humanoids {
    /// Takes scripts' `MoveTo` and `Move` calls.
    pub fn note(&mut self, requests: Vec<Request>) {
        for request in requests {
            match request {
                Request::MoveTo {
                    humanoid,
                    target,
                    part,
                } => {
                    self.walks.insert(
                        humanoid,
                        Walk {
                            target,
                            part,
                            corners: Vec::new(),
                            planned_for: None,
                            elapsed: 0.0,
                        },
                    );
                }
                Request::Stop { humanoid } => {
                    self.walks.remove(&humanoid);
                }
            }
        }
    }

    fn mesh(&mut self, world: &World, assets: &Assets, dt: f32) -> &NavMesh {
        let built = self.built.get_or_insert_with(|| Built {
            builder: MeshBuilder::new(NavSettings::default()),
            // Old enough to build at once.
            age: REBUILD_SECONDS,
        });
        built.age += dt;
        if built.age >= REBUILD_SECONDS && built.builder.update(world, assets) != Refresh::Unchanged
        {
            built.age = 0.0;
        }
        built.builder.mesh()
    }

    /// Frame step 1, after the controllers: makes missing pieces, walks
    /// each `MoveTo` on (setting its controller's `MoveDirection` for the
    /// next step), works out each Humanoid's state and plays the matching
    /// Animator state. Returns the events to fire.
    pub fn step(
        &mut self,
        world: &mut World,
        assets: &Assets,
        controllers: &Controllers,
        seconds: f64,
    ) -> Vec<HumanoidEvent> {
        let dt = if seconds.is_finite() {
            seconds.max(0.0) as f32
        } else {
            0.0
        };
        let humanoids: Vec<InstanceId> =
            world.all_below(world.workspace(), |class| class == ClassName::Humanoid);
        self.walks.retain(|id, _| humanoids.contains(id));
        self.states.0.retain(|id, _| humanoids.contains(id));
        self.playing.retain(|id, _| humanoids.contains(id));
        self.feet.retain(|id, _| humanoids.contains(id));
        let mut events = Vec::new();
        for &humanoid in &humanoids {
            let Some(pieces) = ensure_pieces(world, humanoid) else {
                continue;
            };
            let Some(holder) = holder(world, humanoid) else {
                continue;
            };
            self.walk(world, assets, humanoid, holder, pieces, dt, &mut events);
            let state = derive(
                world,
                controllers,
                holder,
                pieces,
                self.states.0.get(&humanoid),
            );
            events.extend(self.enter(world, humanoid, state));
            self.animate(world, assets, humanoid, holder, pieces, dt);
        }
        events
    }

    /// Records `state` for `humanoid`: `StateChanged` if it changed (not
    /// the first time it's seen), and, entering Dead, `Died` and what
    /// dying does ([`die`]).
    fn enter(
        &mut self,
        world: &mut World,
        humanoid: InstanceId,
        state: State,
    ) -> Vec<HumanoidEvent> {
        let old = self.states.0.insert(humanoid, state);
        let mut events = Vec::new();
        if old == Some(state) {
            return events;
        }
        if let Some(old) = old {
            events.push(HumanoidEvent::StateChanged {
                humanoid,
                old,
                new: state,
            });
        }
        if state == State::Dead {
            if self.walks.remove(&humanoid).is_some() {
                events.push(HumanoidEvent::MoveToFinished {
                    humanoid,
                    reached: false,
                });
            }
            die(world, humanoid);
            events.push(HumanoidEvent::Died { humanoid });
        } else if old == Some(State::Dead) {
            revive(world, humanoid);
        }
        events
    }

    /// The Health `health` died: its Humanoid, if its Model has one, dies
    /// now, without waiting for the next step.
    pub fn died(&mut self, world: &mut World, health: InstanceId) -> Vec<HumanoidEvent> {
        let Some(humanoid) = world
            .get(health)
            .and_then(|instance| instance.parent())
            .and_then(|holder| humanoid_in(world, holder))
        else {
            return Vec::new();
        };
        self.enter(world, humanoid, State::Dead)
    }

    #[allow(clippy::too_many_arguments)]
    fn walk(
        &mut self,
        world: &mut World,
        assets: &Assets,
        humanoid: InstanceId,
        holder: InstanceId,
        pieces: Pieces,
        dt: f32,
        events: &mut Vec<HumanoidEvent>,
    ) {
        if !self.walks.contains_key(&humanoid) {
            return;
        }
        let finish = |world: &mut World, events: &mut Vec<HumanoidEvent>, reached: bool| {
            if let Some(controller) = pieces.controller {
                set_direction(world, controller, 0.0, 0.0);
            }
            events.push(HumanoidEvent::MoveToFinished { humanoid, reached });
        };
        let (Some(_), Some(feet)) = (pieces.controller, feet_of(world, holder)) else {
            self.walks.remove(&humanoid);
            finish(world, events, false);
            return;
        };
        let walk = self.walks.get_mut(&humanoid).expect("checked above");
        let goal = walk.goal(world);
        walk.elapsed += dt;
        if across(feet, goal) <= ARRIVE {
            self.walks.remove(&humanoid);
            finish(world, events, true);
            return;
        }
        if walk.elapsed > MOVE_TIMEOUT {
            self.walks.remove(&humanoid);
            finish(world, events, false);
            return;
        }
        let replan = walk
            .planned_for
            .is_none_or(|planned| across(planned, goal) > REPLAN);
        if replan {
            let path = self.mesh(world, assets, dt).find_path(feet, goal);
            let walk = self.walks.get_mut(&humanoid).expect("checked above");
            match path {
                Ok(corners) => {
                    walk.corners = corners.into_iter().skip(1).collect();
                    walk.planned_for = Some(goal);
                }
                Err(_) => {
                    self.walks.remove(&humanoid);
                    finish(world, events, false);
                    return;
                }
            }
        }
        let walk = self.walks.get_mut(&humanoid).expect("checked above");
        // The last corner is where a part has moved to since.
        if let Some(last) = walk.corners.last_mut() {
            *last = goal;
        }
        while walk.corners.len() > 1 && across(feet, walk.corners[0]) <= CORNER {
            walk.corners.remove(0);
        }
        let next = walk.corners.first().copied().unwrap_or(goal);
        let (x, z) = (next.x - feet.x, next.z - feet.z);
        let length = x.hypot(z).max(1e-6);
        if let Some(controller) = pieces.controller {
            set_direction(world, controller, x / length, z / length);
        }
    }

    /// Plays the Animator state that matches the Humanoid's: Idle, Walk or
    /// Run on the ground (by how fast it moved since last frame), Jump and
    /// Fall in the air, if its state machine has a state by that name.
    fn animate(
        &mut self,
        world: &mut World,
        assets: &Assets,
        humanoid: InstanceId,
        holder: InstanceId,
        pieces: Pieces,
        dt: f32,
    ) {
        let feet = feet_of(world, holder);
        let last = match feet {
            Some(feet) => self.feet.insert(humanoid, feet),
            None => self.feet.remove(&humanoid),
        };
        let speed = match (feet, last) {
            (Some(now), Some(last)) if dt > 0.0 => across(last, now) / dt,
            _ => 0.0,
        };
        let state = self.states.0.get(&humanoid).copied();
        let wanted = match state {
            Some(State::Running | State::Landed) => {
                let controller = pieces.controller.unwrap_or(humanoid);
                let walk = number(world, controller, "WalkSpeed") as f32;
                let run = number(world, controller, "RunSpeed") as f32;
                if speed < 0.5 {
                    Some("Idle")
                } else if run > walk && speed > (walk + run) * 0.5 {
                    Some("Run")
                } else {
                    Some("Walk")
                }
            }
            Some(State::Jumping) => Some("Jump"),
            Some(State::Freefall) => Some("Fall"),
            _ => None,
        };
        if self.playing.get(&humanoid) == Some(&wanted) {
            return;
        }
        self.playing.insert(humanoid, wanted);
        let (Some(name), Some(animator)) = (wanted, pieces.animator) else {
            return;
        };
        let path = world
            .get(animator)
            .and_then(|instance| instance.property("StateMachine"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if path.is_empty()
            || read_state_machine(assets, &path).is_ok_and(|file| file.state(name).is_none())
        {
            return;
        }
        if let Some(live) = world.animator_mut(animator) {
            live.play = Some(PlayRequest {
                state: name.to_owned(),
                blend: None,
            });
        }
    }
}

/// The state the pieces say the character is in.
fn derive(
    world: &World,
    controllers: &Controllers,
    holder: InstanceId,
    pieces: Pieces,
    last: Option<&State>,
) -> State {
    if pieces
        .health
        .is_some_and(|health| number(world, health, "Health") <= 0.0)
    {
        return State::Dead;
    }
    if ragdolled(world, holder) {
        return State::Ragdoll;
    }
    let Some(controller) = pieces.controller else {
        return State::Running;
    };
    if flag(world, controller, "Sit") {
        return State::Seated;
    }
    match controllers.in_air(controller) {
        Some(rising) if rising > 0.0 => State::Jumping,
        Some(_) => State::Freefall,
        None if matches!(last, Some(State::Jumping | State::Freefall)) => State::Landed,
        None => State::Running,
    }
}

/// What dying does: the controller stops moving the character
/// (`Movement` is [`axcenity_character::DEAD`]); with `RagdollOnDeath` its
/// Ragdolls go on; with `BreakJointsOnDeath` its welds and joints (but its
/// Ragdolls') turn off and its parts are let go, to fall.
fn die(world: &mut World, humanoid: InstanceId) {
    let Some(holder) = holder(world, humanoid) else {
        return;
    };
    let pieces = Pieces::of(world, holder);
    if let Some(controller) = pieces.controller {
        set_direction(world, controller, 0.0, 0.0);
        let _ = world.set_property(
            controller,
            "Movement",
            Value::String(axcenity_character::DEAD.to_owned()),
        );
    }
    if flag(world, humanoid, "RagdollOnDeath") {
        set_ragdolls(world, holder, true);
    }
    if flag(world, humanoid, "BreakJointsOnDeath") {
        let joints: Vec<InstanceId> = world
            .descendants_iter(holder)
            .filter(|&id| {
                world.get(id).is_some_and(|instance| {
                    let class = instance.class();
                    (class == ClassName::WeldConstraint || class.is_joint())
                        && !in_ragdoll(world, id)
                })
            })
            .collect();
        for joint in joints {
            let _ = world.set_property(joint, "Enabled", Value::Bool(false));
        }
        for part in axcenity_character::body_parts(world, holder) {
            let _ = world.set_property(part, "Anchored", Value::Bool(false));
        }
    }
}

/// Coming back to life (a script set `Health` above 0): the controller
/// moves the character again.
fn revive(world: &mut World, humanoid: InstanceId) {
    let Some(holder) = holder(world, humanoid) else {
        return;
    };
    if let Some(controller) = Pieces::of(world, holder).controller {
        let dead = world
            .get(controller)
            .and_then(|instance| instance.property("Movement"))
            .and_then(Value::as_str)
            == Some(axcenity_character::DEAD);
        if dead {
            let _ = world.set_property(controller, "Movement", Value::String(String::new()));
        }
    }
}

fn in_ragdoll(world: &World, id: InstanceId) -> bool {
    let mut at = world.get(id).and_then(|instance| instance.parent());
    while let Some(parent) = at {
        let Some(instance) = world.get(parent) else {
            return false;
        };
        if instance.class() == ClassName::Ragdoll {
            return true;
        }
        at = instance.parent();
    }
    false
}

/// The event's arguments for the Humanoid's `StateChanged`.
pub fn state_items(old: State, new: State) -> (LuaEnumItem, LuaEnumItem) {
    (old.item(), new.item())
}

/// Whether the Health event `event` killed a Health.
pub fn died_health(event: &HealthEvent) -> Option<InstanceId> {
    match *event {
        HealthEvent::Died { health } => Some(health),
        _ => None,
    }
}
