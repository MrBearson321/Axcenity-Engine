//! `game:GetService("TweenService")`, `TweenInfo.new` and the Tweens they
//! make: a property moving smoothly from where it is to a goal.
//!
//! `TweenService:Create(instance, info, goals)` checks the goals and gives a
//! [`LuaTween`]. `Play` puts it in this side's [`Tweens`], and every frame
//! [`step`] (frame step 2, before paused scripts resume) moves each playing
//! tween on and sets its properties on the Instance, eased with the same
//! `anim` curves clip keys use. So a tween is ordinary property changes:
//! `Changed` fires, and the player needs nothing else.
//!
//! A tween reads its start values when its delay is over. Playing a tween
//! cancels any other tween playing on one of the same properties of the same
//! Instance. `Completed` fires with how the tween ended (Completed, or
//! Cancelled by `Cancel` or by a newer tween), at the next frame step.
//!
//! With the game UI setting reduced motion on (`World::ui_settings`), a
//! tween of a UI element ends as soon as its delay is over: it's set to
//! where it would end and `Completed` fires.
//!
//! A part's `CFrame` tweens as one rotation and one position (see
//! [`Goal::CFrame`]), so it turns the short way round and never flips the
//! way blending three `Orientation` angles can.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use axcenity_anim::{EasingDirection, EasingStyle, ease};
use axcenity_core::{Color3, InstanceId, UDim, UDim2, Value, ValueKind, Vector2, Vector3};
use mlua::{
    FromLua, Lua, MetaMethod, Table, UserData, UserDataFields, UserDataMethods, Value as LuaValue,
};

use crate::cframe::LuaCFrame;
use crate::events::{self, LuaSignal, Owner};
use crate::instance::{LuaInstance, world, world_mut};
use crate::types::{EnumType, LuaEnumItem, type_name, value_from_lua};

/// Number properties that only take whole values, so a tween steps them.
const STEPPED: &[&str] = &["ZIndex", "DisplayOrder"];

/// How a tween's motion is shaped: `Enum.EasingStyle`. Linear isn't a clip
/// key easing, so it's its own case here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TweenStyle {
    Linear,
    Eased(EasingStyle),
}

impl TweenStyle {
    pub fn as_str(self) -> &'static str {
        match self {
            TweenStyle::Linear => "Linear",
            TweenStyle::Eased(style) => style.as_str(),
        }
    }

    /// Every `Enum.EasingStyle` item name, Linear first.
    pub fn names() -> Vec<&'static str> {
        std::iter::once("Linear")
            .chain(EasingStyle::ALL.iter().map(|style| style.as_str()))
            .collect()
    }

    fn from_name(name: &str) -> Option<Self> {
        if name == "Linear" {
            return Some(TweenStyle::Linear);
        }
        EasingStyle::ALL
            .into_iter()
            .find(|style| style.as_str() == name)
            .map(TweenStyle::Eased)
    }
}

/// Where a tween is: `Enum.PlaybackState`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackState {
    /// Made, and not played yet.
    Begin,
    /// Played, and waiting for its `DelayTime`.
    Delayed,
    Playing,
    Paused,
    Completed,
    Cancelled,
}

impl PlaybackState {
    pub const ALL: [PlaybackState; 6] = [
        PlaybackState::Begin,
        PlaybackState::Delayed,
        PlaybackState::Playing,
        PlaybackState::Paused,
        PlaybackState::Completed,
        PlaybackState::Cancelled,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            PlaybackState::Begin => "Begin",
            PlaybackState::Delayed => "Delayed",
            PlaybackState::Playing => "Playing",
            PlaybackState::Paused => "Paused",
            PlaybackState::Completed => "Completed",
            PlaybackState::Cancelled => "Cancelled",
        }
    }

    fn item(self) -> LuaEnumItem {
        LuaEnumItem {
            enum_type: EnumType::PlaybackState,
            name: self.as_str(),
        }
    }
}

/// How a tween plays, as `TweenInfo.new` makes it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TweenInfo {
    /// Seconds one way.
    pub time: f64,
    pub style: TweenStyle,
    pub direction: EasingDirection,
    /// How many more times it plays after the first; below 0, forever.
    pub repeat_count: i64,
    /// Each play goes to the goal and back again.
    pub reverses: bool,
    /// Seconds to wait after `Play` before it starts.
    pub delay: f64,
}

impl Default for TweenInfo {
    /// As in Roblox: one second, Quad, Out.
    fn default() -> Self {
        Self {
            time: 1.0,
            style: TweenStyle::Eased(EasingStyle::Quad),
            direction: EasingDirection::Out,
            repeat_count: 0,
            reverses: false,
            delay: 0.0,
        }
    }
}

impl TweenInfo {
    /// How far along the motion is (eased, usually `0` to `1`) after `run`
    /// seconds of playing (not counting the delay), and whether it's over.
    pub fn progress(&self, run: f64) -> (f32, bool) {
        let one_way = self.time.max(0.0);
        let cycle = if self.reverses {
            one_way * 2.0
        } else {
            one_way
        };
        // Ending on the way back puts it where it started.
        let end = if self.reverses { 0.0 } else { 1.0 };
        let plays = if self.repeat_count < 0 {
            f64::INFINITY
        } else {
            (self.repeat_count + 1) as f64
        };
        if cycle <= 0.0 || run >= cycle * plays {
            return (end, true);
        }
        let within = run % cycle;
        let t = if within <= one_way {
            within / one_way
        } else {
            2.0 - within / one_way
        };
        (self.ease(t as f32), false)
    }

    fn ease(&self, t: f32) -> f32 {
        match self.style {
            TweenStyle::Linear => t.clamp(0.0, 1.0),
            TweenStyle::Eased(style) => ease(style, self.direction, t),
        }
    }
}

/// The value `alpha` of the way from `from` to `to` (past them when an
/// easing overshoots), for the property `name`. `None` for values that
/// don't tween, or two values of different kinds.
pub fn interpolate(name: &str, from: &Value, to: &Value, alpha: f32) -> Option<Value> {
    let lerp = |a: f32, b: f32| a + (b - a) * alpha;
    let udim = |a: UDim, b: UDim| UDim::new(lerp(a.scale, b.scale), lerp(a.offset, b.offset));
    Some(match (from, to) {
        (Value::Number(a), Value::Number(b)) => {
            let value = a + (b - a) * f64::from(alpha);
            Value::Number(if STEPPED.contains(&name) {
                value.round()
            } else {
                value
            })
        }
        (Value::Vector3(a), Value::Vector3(b)) => {
            Value::Vector3(Vector3::new(lerp(a.x, b.x), lerp(a.y, b.y), lerp(a.z, b.z)))
        }
        (Value::Vector2(a), Value::Vector2(b)) => {
            Value::Vector2(Vector2::new(lerp(a.x, b.x), lerp(a.y, b.y)))
        }
        // Colours can't go past their ends, even when the easing does.
        (Value::Color3(a), Value::Color3(b)) => {
            let channel = |a: f32, b: f32| lerp(a, b).clamp(0.0, 1.0);
            Value::Color3(Color3::new(
                channel(a.r, b.r),
                channel(a.g, b.g),
                channel(a.b, b.b),
            ))
        }
        (Value::UDim2(a), Value::UDim2(b)) => Value::UDim2(UDim2 {
            x: udim(a.x, b.x),
            y: udim(a.y, b.y),
        }),
        // A switch only flips once the motion is all the way there.
        (Value::Bool(a), Value::Bool(b)) => Value::Bool(if alpha >= 1.0 { *b } else { *a }),
        _ => return None,
    })
}

/// Whether a property of this kind can be tweened.
fn tweenable(kind: ValueKind) -> bool {
    matches!(
        kind,
        ValueKind::Number
            | ValueKind::Vector3
            | ValueKind::Vector2
            | ValueKind::Color3
            | ValueKind::UDim2
            | ValueKind::Bool
    )
}

/// Where one goal of a tween goes.
#[derive(Clone, Debug, PartialEq)]
enum Goal {
    Value(Value),
    /// A part's `CFrame`, written to its `Position` and `Orientation`.
    CFrame(LuaCFrame),
}

/// Whether two goals move the same property: `CFrame` moves `Position`
/// and `Orientation`.
fn overlaps(a: &str, b: &str) -> bool {
    let moves = |goal: &str, property: &str| {
        goal == property || (goal == "CFrame" && matches!(property, "Position" | "Orientation"))
    };
    moves(a, b) || moves(b, a)
}

struct TweenState {
    id: u64,
    instance: InstanceId,
    info: TweenInfo,
    /// Each property and where it goes, sorted by name.
    goals: Vec<(&'static str, Goal)>,
    /// Where each goal's property was when the delay ended; empty before.
    starts: Vec<Option<Goal>>,
    state: PlaybackState,
    /// Seconds since `Play`, delay included.
    elapsed: f64,
}

/// The tweens of one side. Lives in Lua's app data.
#[derive(Default)]
pub struct Tweens {
    next_id: u64,
    /// Delayed and playing tweens, in the order they were played.
    playing: Vec<Rc<RefCell<TweenState>>>,
    /// Every tween made, so its `Completed` handlers can go once nothing
    /// holds the tween any more.
    made: Vec<(u64, Weak<RefCell<TweenState>>)>,
    /// `Completed` events waiting for the next step: cancelled tweens.
    ended: Vec<(u64, PlaybackState)>,
}

impl Tweens {
    /// The ids of tweens a script may still hold, forgetting the rest.
    pub fn alive(&mut self) -> Vec<u64> {
        self.made.retain(|(_, tween)| tween.strong_count() > 0);
        self.made.iter().map(|(id, _)| *id).collect()
    }

    fn stop(&mut self, tween: &Rc<RefCell<TweenState>>) {
        self.playing.retain(|playing| !Rc::ptr_eq(playing, tween));
    }
}

fn tweens_mut(lua: &Lua) -> mlua::Result<mlua::AppDataRefMut<'_, Tweens>> {
    lua.app_data_mut::<Tweens>()
        .ok_or_else(|| mlua::Error::runtime("tweens aren't available"))
}

/// Frame step 2, before paused scripts resume: moves every playing tween on
/// by `seconds` and sets its properties, then fires `Completed` for the
/// tweens that ended.
pub fn step(lua: &Lua, seconds: f64) {
    let (playing, mut ended) = match lua.app_data_mut::<Tweens>() {
        Some(mut tweens) => (tweens.playing.clone(), std::mem::take(&mut tweens.ended)),
        None => return,
    };
    for tween in playing {
        let (done, cancelled) = advance(lua, &tween, seconds);
        if (done || cancelled)
            && let Some(mut tweens) = lua.app_data_mut::<Tweens>()
        {
            tweens.stop(&tween);
        }
        if done {
            ended.push((tween.borrow().id, PlaybackState::Completed));
        }
    }
    for (id, state) in ended {
        events::fire(lua, Owner::Tween(id), "Completed", state.item());
    }
}

/// Moves one tween on. Returns whether it finished, and whether it had to
/// stop because its Instance is gone.
fn advance(lua: &Lua, tween: &Rc<RefCell<TweenState>>, seconds: f64) -> (bool, bool) {
    let Ok(mut world) = world_mut(lua) else {
        return (false, false);
    };
    let mut tween = tween.borrow_mut();
    let Some(instance) = world.get(tween.instance) else {
        tween.state = PlaybackState::Cancelled;
        return (false, true);
    };
    tween.elapsed += seconds;
    let run = tween.elapsed - tween.info.delay;
    if run < 0.0 {
        return (false, false);
    }
    if tween.state == PlaybackState::Delayed {
        tween.state = PlaybackState::Playing;
        tween.starts = tween
            .goals
            .iter()
            .map(|(name, goal)| match goal {
                Goal::Value(_) => instance.property(name).cloned().map(Goal::Value),
                Goal::CFrame(_) => LuaCFrame::of_instance(instance).map(Goal::CFrame),
            })
            .collect();
    }
    // Reduced motion: a UI element's tween goes straight to where it ends.
    let still = world.ui_settings().reduced_motion && instance.class().is_gui();
    let (alpha, done) = tween.info.progress(if still { f64::INFINITY } else { run });
    let id = tween.instance;
    for ((name, goal), start) in tween.goals.iter().zip(&tween.starts) {
        // Another script may have destroyed it, or its kind can't change,
        // so a failure here only skips this one.
        match (start, goal) {
            (Some(Goal::Value(start)), Goal::Value(goal)) => {
                if let Some(value) = interpolate(name, start, goal, alpha) {
                    let _ = world.set_property(id, name, value);
                }
            }
            (Some(Goal::CFrame(start)), Goal::CFrame(goal)) => {
                let _ = start.lerp(*goal, alpha).write_to(&mut world, id);
            }
            _ => {}
        }
    }
    if done {
        tween.state = PlaybackState::Completed;
    }
    (done, false)
}

/// `TweenService`, as scripts see it. Both sides have one.
#[derive(Clone, Copy)]
pub struct LuaTweenService;

impl UserData for LuaTweenService {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method(
            "Create",
            |lua, _, (instance, info, goals): (LuaValue, LuaValue, LuaValue)| {
                create(lua, instance, info, goals)
            },
        );
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("TweenService"));
    }
}

/// `TweenService:Create(instance, tweenInfo, goals)`.
fn create(
    lua: &Lua,
    instance: LuaValue,
    info: LuaValue,
    goals: LuaValue,
) -> mlua::Result<LuaTween> {
    let wrong = |what: &str, value: &LuaValue| {
        mlua::Error::runtime(format!(
            "TweenService:Create takes {what}, not a {}",
            type_name(lua, value)
        ))
    };
    let Ok(LuaInstance(id)) = LuaInstance::from_lua(instance.clone(), lua) else {
        return Err(wrong("an Instance first", &instance));
    };
    let Ok(LuaTweenInfo(info)) = LuaTweenInfo::from_lua(info.clone(), lua) else {
        return Err(wrong("a TweenInfo second (TweenInfo.new(...))", &info));
    };
    let LuaValue::Table(goals) = goals else {
        return Err(wrong(
            "a table of goals third, such as {Position = Vector3.new(0, 5, 0)}",
            &goals,
        ));
    };
    let goals = read_goals(lua, id, &goals)?;
    let mut tweens = tweens_mut(lua)?;
    let id_number = tweens.next_id;
    tweens.next_id += 1;
    let tween = Rc::new(RefCell::new(TweenState {
        id: id_number,
        instance: id,
        info,
        goals,
        starts: Vec::new(),
        state: PlaybackState::Begin,
        elapsed: 0.0,
    }));
    tweens.made.push((id_number, Rc::downgrade(&tween)));
    Ok(LuaTween(tween))
}

/// Each goal as a property of `id` and a value of that property's kind,
/// sorted by name.
fn read_goals(lua: &Lua, id: InstanceId, goals: &Table) -> mlua::Result<Vec<(&'static str, Goal)>> {
    let mut wanted = Vec::new();
    for pair in goals.pairs::<LuaValue, LuaValue>() {
        let (key, value) = pair?;
        let LuaValue::String(key) = key else {
            return Err(mlua::Error::runtime(format!(
                "TweenService:Create's goals are property names, not a {}",
                type_name(lua, &key)
            )));
        };
        wanted.push((key.to_string_lossy(), value));
    }
    let mut read = Vec::new();
    for (key, value) in wanted {
        let (class, name, kind) = {
            let world = world(lua)?;
            let instance = world
                .get(id)
                .ok_or_else(|| mlua::Error::runtime("this Instance has been destroyed"))?;
            let class = instance.class();
            if key == "CFrame" && LuaCFrame::of_instance(instance).is_some() {
                let cf = LuaCFrame::from_lua(value.clone(), lua).map_err(|_| {
                    mlua::Error::runtime(format!(
                        "{class}.CFrame must be a CFrame, not a {}",
                        type_name(lua, &value)
                    ))
                })?;
                read.push(("CFrame", Goal::CFrame(cf)));
                continue;
            }
            let Some((name, current)) = instance.properties().find(|(name, _)| *name == key) else {
                return Err(mlua::Error::runtime(format!(
                    "{key} is not a property of {class}, so it can't be tweened"
                )));
            };
            (class, name, current.kind())
        };
        if !tweenable(kind) {
            return Err(mlua::Error::runtime(format!(
                "{class}.{name} is a {kind}, which can't be tweened; tween numbers, \
                 Vector3s, Vector2s, Color3s, UDim2s or booleans"
            )));
        }
        let value = value_from_lua(lua, value, kind)
            .map_err(|message| mlua::Error::runtime(format!("{class}.{name} {message}")))?;
        read.push((name, Goal::Value(value)));
    }
    read.sort_by_key(|(name, _)| *name);
    Ok(read)
}

/// A Tween, as scripts see it. Each copy of the Lua value shares one state.
#[derive(Clone)]
pub struct LuaTween(Rc<RefCell<TweenState>>);

impl LuaTween {
    fn play(&self, lua: &Lua) -> mlua::Result<()> {
        let state = self.0.borrow().state;
        match state {
            PlaybackState::Delayed | PlaybackState::Playing => return Ok(()),
            PlaybackState::Paused => {
                let mut tween = self.0.borrow_mut();
                tween.state = if tween.starts.is_empty() {
                    PlaybackState::Delayed
                } else {
                    PlaybackState::Playing
                };
            }
            PlaybackState::Begin | PlaybackState::Completed | PlaybackState::Cancelled => {
                let mut tween = self.0.borrow_mut();
                tween.elapsed = 0.0;
                tween.starts.clear();
                tween.state = PlaybackState::Delayed;
            }
        }
        let mut tweens = tweens_mut(lua)?;
        // A newer tween on the same property wins.
        let (instance, names): (InstanceId, Vec<&str>) = {
            let tween = self.0.borrow();
            (
                tween.instance,
                tween.goals.iter().map(|(name, _)| *name).collect(),
            )
        };
        let replaced: Vec<_> = tweens
            .playing
            .iter()
            .filter(|other| {
                let other = other.borrow();
                other.instance == instance
                    && other
                        .goals
                        .iter()
                        .any(|(name, _)| names.iter().any(|other| overlaps(name, other)))
            })
            .cloned()
            .collect();
        for other in replaced {
            tweens.stop(&other);
            let mut other = other.borrow_mut();
            other.state = PlaybackState::Cancelled;
            tweens.ended.push((other.id, PlaybackState::Cancelled));
        }
        tweens.playing.push(self.0.clone());
        Ok(())
    }

    fn pause(&self, lua: &Lua) -> mlua::Result<()> {
        let playing = matches!(
            self.0.borrow().state,
            PlaybackState::Delayed | PlaybackState::Playing
        );
        if playing {
            tweens_mut(lua)?.stop(&self.0);
            self.0.borrow_mut().state = PlaybackState::Paused;
        }
        Ok(())
    }

    /// Stops where it is. Playing again starts over from there.
    fn cancel(&self, lua: &Lua) -> mlua::Result<()> {
        let running = matches!(
            self.0.borrow().state,
            PlaybackState::Delayed | PlaybackState::Playing | PlaybackState::Paused
        );
        if running {
            let mut tweens = tweens_mut(lua)?;
            tweens.stop(&self.0);
            let mut tween = self.0.borrow_mut();
            tween.state = PlaybackState::Cancelled;
            tween.elapsed = 0.0;
            tweens.ended.push((tween.id, PlaybackState::Cancelled));
        }
        Ok(())
    }
}

impl UserData for LuaTween {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("PlaybackState", |_, this| Ok(this.0.borrow().state.item()));
        fields.add_field_method_get("Completed", |_, this| {
            Ok(LuaSignal::named(
                Owner::Tween(this.0.borrow().id),
                "Completed",
            ))
        });
        fields.add_field_method_get("Instance", |_, this| {
            Ok(LuaInstance(this.0.borrow().instance))
        });
        fields.add_field_method_get("TweenInfo", |_, this| {
            Ok(LuaTweenInfo(this.0.borrow().info))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("Play", |lua, this, ()| this.play(lua));
        methods.add_method("Pause", |lua, this, ()| this.pause(lua));
        methods.add_method("Cancel", |lua, this, ()| this.cancel(lua));
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("Tween"));
    }
}

/// A TweenInfo, as scripts see it. Read-only, as in Roblox.
#[derive(Clone, Copy, Debug)]
pub struct LuaTweenInfo(pub TweenInfo);

impl FromLua for LuaTweenInfo {
    fn from_lua(value: LuaValue, _: &Lua) -> mlua::Result<Self> {
        if let LuaValue::UserData(data) = &value
            && let Ok(info) = data.borrow::<LuaTweenInfo>()
        {
            return Ok(*info);
        }
        Err(mlua::Error::FromLuaConversionError {
            from: value.type_name(),
            to: "TweenInfo".to_owned(),
            message: None,
        })
    }
}

impl UserData for LuaTweenInfo {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Time", |_, info| Ok(info.0.time));
        fields.add_field_method_get("EasingStyle", |_, info| {
            Ok(LuaEnumItem {
                enum_type: EnumType::EasingStyle,
                name: info.0.style.as_str(),
            })
        });
        fields.add_field_method_get("EasingDirection", |_, info| {
            Ok(LuaEnumItem {
                enum_type: EnumType::EasingDirection,
                name: info.0.direction.as_str(),
            })
        });
        fields.add_field_method_get("RepeatCount", |_, info| Ok(info.0.repeat_count));
        fields.add_field_method_get("Reverses", |_, info| Ok(info.0.reverses));
        fields.add_field_method_get("DelayTime", |_, info| Ok(info.0.delay));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::ToString, |_, info, ()| {
            let info = info.0;
            Ok(format!(
                "Time:{} DelayTime:{} RepeatCount:{} Reverses:{} EasingDirection:{} EasingStyle:{}",
                info.time,
                info.delay,
                info.repeat_count,
                info.reverses,
                info.direction.as_str(),
                info.style.as_str()
            ))
        });
    }
}

/// The `TweenInfo` global, with `TweenInfo.new`.
pub fn tween_info_library(lua: &Lua) -> mlua::Result<Table> {
    let library = lua.create_table()?;
    library.raw_set(
        "new",
        lua.create_function(
            |lua,
             (time, style, direction, repeat_count, reverses, delay): (
                Option<f64>,
                LuaValue,
                LuaValue,
                Option<f64>,
                Option<bool>,
                Option<f64>,
            )| {
                let defaults = TweenInfo::default();
                let seconds = |value: Option<f64>, default: f64, name: &str| match value {
                    Some(n) if n.is_finite() && n >= 0.0 => Ok(n),
                    None => Ok(default),
                    Some(_) => Err(mlua::Error::runtime(format!(
                        "TweenInfo.new's {name} must be 0 or more seconds"
                    ))),
                };
                let style = match enum_item(lua, style, EnumType::EasingStyle)? {
                    Some(name) => TweenStyle::from_name(name).expect("an EasingStyle item"),
                    None => defaults.style,
                };
                let direction = match enum_item(lua, direction, EnumType::EasingDirection)? {
                    Some(name) => EasingDirection::ALL
                        .into_iter()
                        .find(|direction| direction.as_str() == name)
                        .expect("an EasingDirection item"),
                    None => defaults.direction,
                };
                let repeat_count = match repeat_count {
                    Some(n) if n.is_finite() => n.floor() as i64,
                    None => 0,
                    Some(_) => {
                        return Err(mlua::Error::runtime(
                            "TweenInfo.new's repeatCount must be a whole number (-1 repeats forever)",
                        ));
                    }
                };
                Ok(LuaTweenInfo(TweenInfo {
                    time: seconds(time, defaults.time, "time")?,
                    style,
                    direction,
                    repeat_count,
                    reverses: reverses.unwrap_or(false),
                    delay: seconds(delay, defaults.delay, "delayTime")?,
                }))
            },
        )?,
    )?;
    Ok(library)
}

/// The item name of an `Enum.<enum_type>` item, its name as a string, or
/// `None` for nil.
fn enum_item(
    lua: &Lua,
    value: LuaValue,
    enum_type: EnumType,
) -> mlua::Result<Option<&'static str>> {
    let found = match &value {
        LuaValue::Nil => return Ok(None),
        LuaValue::String(name) => LuaEnumItem::find(enum_type, &name.to_string_lossy()),
        _ => LuaEnumItem::from_lua(value.clone(), lua)
            .ok()
            .filter(|item| item.enum_type == enum_type),
    };
    found.map(|item| Some(item.name)).ok_or_else(|| {
        mlua::Error::runtime(format!(
            "TweenInfo.new takes an Enum.{0} item here, such as Enum.{0}.{1}, not {2}",
            enum_type.name(),
            enum_type.item_names()[1],
            type_name(lua, &value)
        ))
    })
}
