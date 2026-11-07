//! Splines (Phase 73) for scripts, and SplineFollowers.
//!
//! A Spline's `Length` is worked out from its points (not stored), and
//! `GetPositionAt(t)`, `GetCFrameAt(distance)` and
//! `GetClosestDistance(position)` measure its curve
//! (`axcenity_core::spline`). `agent:Patrol(spline)` sets a
//! NavigationAgent's `PatrolSpline`; `nav` walks it.
//!
//! [`Followers`] moves each enabled SplineFollower's parent (a Part and the
//! parts welded to it, or a Model's parts) along its Spline at `Speed`, on
//! the server, after both sides' scripts (frame step 2). It starts at the
//! point of the curve nearest where the parent stands, keeps the parts'
//! places relative to each other, and turns them to face along the curve
//! when `FaceAlong` is on. `Reached` fires with "End" or "Start" as it gets
//! to an end: a `Loop` goes on from the start again (round a Closed spline
//! that's coming back to the start), a `PingPong` turns back, and `Once`
//! stops.

use std::collections::BTreeMap;

use axcenity_core::spline::{SplineCurve, facing};
use axcenity_core::{ClassName, InstanceId, Value, World};
use glam::Quat;
use mlua::{FromLua, Function, IntoLua, Lua};

use crate::cframe::{LuaCFrame, from_vec3, rotation_from_orientation, to_vec3};
use crate::instance::{LuaInstance, destroyed, world, world_mut};
use crate::types::{LuaVector3, type_name};

/// The Spline `id`'s curve, or an error naming `method` when it has no
/// points.
fn curve(world: &World, id: InstanceId, method: &str) -> mlua::Result<SplineCurve> {
    world.get(id).ok_or_else(destroyed)?;
    SplineCurve::of(world, id).ok_or_else(|| {
        mlua::Error::runtime(format!(
            "{method} needs the Spline to have a SplinePoint in it"
        ))
    })
}

fn number(lua: &Lua, value: mlua::Value, method: &str) -> mlua::Result<f32> {
    match value {
        mlua::Value::Integer(n) => Ok(n as f32),
        mlua::Value::Number(n) if n.is_finite() => Ok(n as f32),
        other => Err(mlua::Error::runtime(format!(
            "{method} needs a number, not a {}",
            type_name(lua, &other)
        ))),
    }
}

/// The CFrame `distance` studs along `curve`, facing along it.
pub(crate) fn cframe_at(curve: &SplineCurve, distance: f32) -> LuaCFrame {
    LuaCFrame::new(
        to_vec3(curve.position_at_distance(distance)),
        rotation_from_orientation(curve.orientation_at_distance(distance)),
    )
}

pub fn methods(lua: &Lua) -> mlua::Result<Vec<(ClassName, &'static str, Function)>> {
    let position_at = lua.create_function(|lua, (this, t): (LuaInstance, mlua::Value)| {
        let t = number(lua, t, "GetPositionAt(t)")?;
        let curve = curve(&*world(lua)?, this.0, "GetPositionAt")?;
        Ok(LuaVector3(curve.position_at(t.clamp(0.0, 1.0))))
    })?;
    let cframe_at_fn =
        lua.create_function(|lua, (this, distance): (LuaInstance, mlua::Value)| {
            let distance = number(lua, distance, "GetCFrameAt(distance)")?;
            let curve = curve(&*world(lua)?, this.0, "GetCFrameAt")?;
            Ok(cframe_at(&curve, distance))
        })?;
    let closest = lua.create_function(|lua, (this, position): (LuaInstance, mlua::Value)| {
        let position = LuaVector3::from_lua(position.clone(), lua).map_err(|_| {
            mlua::Error::runtime(format!(
                "GetClosestDistance needs a Vector3, not a {}",
                type_name(lua, &position)
            ))
        })?;
        let curve = curve(&*world(lua)?, this.0, "GetClosestDistance")?;
        Ok(curve.closest_distance(position.0))
    })?;
    let patrol = lua.create_function(|lua, (this, spline): (LuaInstance, mlua::Value)| {
        let wrong = || {
            mlua::Error::runtime(format!(
                "Patrol needs a Spline (or nil to stop), not {}",
                type_name(lua, &spline)
            ))
        };
        let spline = match &spline {
            mlua::Value::Nil => None,
            value => Some(
                LuaInstance::from_lua(value.clone(), lua)
                    .map_err(|_| wrong())?
                    .0,
            ),
        };
        let is_spline = spline.is_none_or(|spline| {
            world(lua)
                .is_ok_and(|world| world.get(spline).map(|i| i.class()) == Some(ClassName::Spline))
        });
        if !is_spline {
            return Err(wrong());
        }
        let mut world = world_mut(lua)?;
        world.get(this.0).ok_or_else(destroyed)?;
        world
            .set_property(this.0, "PatrolSpline", Value::Instance(spline))
            .map_err(|why| mlua::Error::runtime(why.to_string()))
    })?;
    Ok(vec![
        (ClassName::Spline, "GetPositionAt", position_at),
        (ClassName::Spline, "GetCFrameAt", cframe_at_fn),
        (ClassName::Spline, "GetClosestDistance", closest),
        (ClassName::NavigationAgent, "Patrol", patrol),
    ])
}

/// A Spline's `Length`, worked out from its points; `None` for any other
/// field or class.
pub fn computed(lua: &Lua, id: InstanceId, key: &str) -> mlua::Result<Option<mlua::Value>> {
    if key != "Length" {
        return Ok(None);
    }
    let world = world(lua)?;
    if world.get(id).map(|i| i.class()) != Some(ClassName::Spline) {
        return Ok(None);
    }
    let length = SplineCurve::of(&world, id).map_or(0.0, |curve| curve.length());
    Ok(Some(f64::from(length).into_lua(lua)?))
}

/// What a SplineFollower does at the ends of its spline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Loop,
    PingPong,
    Once,
}

impl Mode {
    fn of(value: Option<&Value>) -> Self {
        match value {
            Some(Value::EnumItem(item)) if item.item == "Loop" => Mode::Loop,
            Some(Value::EnumItem(item)) if item.item == "Once" => Mode::Once,
            _ => Mode::PingPong,
        }
    }
}

/// One follower's run. Not saved: Play starts each where its parent is.
struct Run {
    spline: InstanceId,
    target: InstanceId,
    /// Studs along the curve.
    distance: f32,
    /// 1 forward, -1 back (a PingPong on its way back).
    direction: f32,
    /// A `Once` that got to its end.
    done: bool,
    /// The parent's rotation when it started, kept while `FaceAlong` is off.
    rotation: Quat,
    /// Each part moved, and where it is from the parent's pivot.
    parts: Vec<(InstanceId, LuaCFrame)>,
}

/// Moves SplineFollowers on. The runtime owns one and steps it on the
/// server (not on a device following one: the server's motion moves the
/// parts there).
#[derive(Default)]
pub(crate) struct Followers {
    runs: BTreeMap<InstanceId, Run>,
}

impl Followers {
    /// Moves every enabled SplineFollower in the Workspace on by
    /// `seconds`, and returns the `Reached` events to fire.
    pub(crate) fn step(
        &mut self,
        world: &mut World,
        seconds: f64,
    ) -> Vec<(InstanceId, &'static str, Vec<String>)> {
        let dt = if seconds.is_finite() && seconds > 0.0 {
            seconds as f32
        } else {
            0.0
        };
        self.runs.retain(|&id, _| world.contains(id));
        let followers: Vec<InstanceId> = world
            .descendants(world.workspace())
            .into_iter()
            .filter(|&id| {
                world
                    .get(id)
                    .is_some_and(|i| i.class() == ClassName::SplineFollower)
            })
            .collect();
        let mut events = Vec::new();
        for id in followers {
            if let Some(end) = self.step_one(world, id, dt) {
                events.push((id, "Reached", vec![end.to_owned()]));
            }
        }
        events
    }

    /// Moves one follower on. Returns "End" or "Start" when it got to one.
    fn step_one(&mut self, world: &mut World, id: InstanceId, dt: f32) -> Option<&'static str> {
        let follower = world.get(id)?;
        if follower.property("Enabled") != Some(&Value::Bool(true)) {
            return None;
        }
        let target = follower.parent()?;
        let spline = follower.property("Spline").and_then(Value::as_instance)?;
        let speed = follower
            .property("Speed")
            .and_then(Value::as_number)
            .filter(|speed| speed.is_finite())
            .unwrap_or(0.0) as f32;
        let mode = Mode::of(follower.property("Mode"));
        let face_along = follower.property("FaceAlong") == Some(&Value::Bool(true));
        let curve = SplineCurve::of(world, spline)?;
        let length = curve.length();
        let restart = self
            .runs
            .get(&id)
            .is_none_or(|run| run.spline != spline || run.target != target);
        if restart {
            let run = start(world, &curve, spline, target)?;
            self.runs.insert(id, run);
        }
        let run = self.runs.get_mut(&id)?;
        if run.done || length <= 0.0 {
            return None;
        }
        run.distance += run.direction * speed * dt;
        let mut reached = None;
        match mode {
            Mode::Loop => {
                if run.distance >= length {
                    reached = Some("End");
                } else if run.distance < 0.0 {
                    reached = Some("Start");
                }
                run.distance = run.distance.rem_euclid(length);
            }
            Mode::PingPong => {
                // A step longer than the curve turns back more than once.
                for _ in 0..8 {
                    if run.distance > length {
                        run.distance = 2.0 * length - run.distance;
                        run.direction = -run.direction;
                        reached = Some("End");
                    } else if run.distance < 0.0 {
                        run.distance = -run.distance;
                        run.direction = -run.direction;
                        reached = Some("Start");
                    } else {
                        break;
                    }
                }
                run.distance = run.distance.clamp(0.0, length);
            }
            Mode::Once => {
                if run.distance >= length {
                    run.distance = length;
                    run.done = true;
                    reached = Some("End");
                } else if run.distance <= 0.0 && run.direction * speed < 0.0 {
                    run.distance = 0.0;
                    run.done = true;
                    reached = Some("Start");
                }
            }
        }
        let rotation = if face_along {
            // Facing the way it goes: back along the curve on the way back.
            let along = curve.direction_at_distance(run.distance);
            let going = run.direction * speed.signum();
            let way = if going < 0.0 {
                axcenity_core::Vector3::new(-along.x, -along.y, -along.z)
            } else {
                along
            };
            rotation_from_orientation(facing(way))
        } else {
            run.rotation
        };
        let pivot = LuaCFrame::new(to_vec3(curve.position_at_distance(run.distance)), rotation);
        for &(part, offset) in &run.parts {
            let _ = (pivot * offset).write_to(world, part);
        }
        reached
    }
}

/// A new run for a follower of `spline` moving `target`, from the point
/// of the curve nearest where `target` is.
fn start(
    world: &World,
    curve: &SplineCurve,
    spline: InstanceId,
    target: InstanceId,
) -> Option<Run> {
    let instance = world.get(target)?;
    let (pivot, parts) = if instance.class().is_base_part() {
        let pivot = LuaCFrame::of_instance(instance)?;
        let parts = axcenity_physics::assembly::connected_parts(world, target, true);
        (pivot, parts)
    } else {
        let parts: Vec<InstanceId> = world
            .descendants(target)
            .into_iter()
            .filter(|&id| world.get(id).is_some_and(|i| i.class().is_base_part()))
            .collect();
        let primary = instance
            .property("PrimaryPart")
            .and_then(Value::as_instance)
            .and_then(|id| world.get(id))
            .and_then(LuaCFrame::of_instance);
        let pivot = match primary {
            Some(pivot) => pivot,
            None => crate::cframe::pivot(world, target).ok()?,
        };
        (pivot, parts)
    };
    let to_pivot = pivot.inverse();
    let parts = parts
        .into_iter()
        .filter_map(|id| {
            let cf = world.get(id).and_then(LuaCFrame::of_instance)?;
            Some((id, to_pivot * cf))
        })
        .collect();
    Some(Run {
        spline,
        target,
        distance: curve.closest_distance(from_vec3(pivot.position)),
        direction: 1.0,
        done: false,
        rotation: pivot.rotation,
        parts,
    })
}
