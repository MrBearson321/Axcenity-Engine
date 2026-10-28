//! Instances as Lua values. A Lua Instance only holds an [`InstanceId`];
//! every read and write looks the Instance up in the World, which is lent to
//! Lua while scripts run (see `with_world` in `lib.rs`).

use axcenity_audio::SoundAction;
use axcenity_core::{ClassName, INSTANCE_EVENTS, InstanceId, Parameter, Value, World};
use mlua::{
    AppDataRef, AppDataRefMut, FromLua, Function, IntoLua, Lua, MetaMethod, UserData,
    UserDataMethods,
};

use crate::camera::LuaCamera;
use crate::cframe::LuaCFrame;
use crate::client::{LocalPlayer, LuaReplicatedFirst};
use crate::collection::LuaCollectionService;
use crate::data::LuaDataService;
use crate::debris::LuaDebris;
use crate::events::{Event, LuaSignal, Owner};
use crate::gui_service::LuaGuiService;
use crate::http::LuaHttpService;
use crate::input::{LuaContextActionService, LuaUserInputService};
use crate::localization::LuaLocalizationService;
use crate::run_service::LuaRunService;
use crate::save::LuaSaveService;
use crate::sides;
use crate::tween::LuaTweenService;
use crate::types::{type_name, value_from_lua, value_to_lua};
use crate::user_settings::LuaUserGameSettings;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LuaInstance(pub InstanceId);

/// The World, for reading. Only fails if called while no script is running.
pub fn world(lua: &Lua) -> mlua::Result<AppDataRef<'_, World>> {
    lua.app_data_ref::<World>().ok_or_else(no_world)
}

/// The World, for changing. Drop the returned guard before calling back into
/// Lua, because the World can't be borrowed twice at once.
pub fn world_mut(lua: &Lua) -> mlua::Result<AppDataRefMut<'_, World>> {
    lua.app_data_mut::<World>().ok_or_else(no_world)
}

fn no_world() -> mlua::Error {
    mlua::Error::runtime("the game world can only be used while scripts are running")
}

pub(crate) fn destroyed() -> mlua::Error {
    mlua::Error::runtime("this Instance has been destroyed")
}

impl FromLua for LuaInstance {
    fn from_lua(value: mlua::Value, _: &Lua) -> mlua::Result<Self> {
        if let mlua::Value::UserData(data) = &value
            && let Ok(instance) = data.borrow::<LuaInstance>()
        {
            return Ok(*instance);
        }
        Err(mlua::Error::FromLuaConversionError {
            from: value.type_name(),
            to: "Instance".to_owned(),
            message: None,
        })
    }
}

impl UserData for LuaInstance {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // Methods are looked up before `__index` below, so they win over
        // properties and children with the same name.
        methods.add_method(
            "FindFirstChild",
            |lua, this, (name, recursive): (String, Option<bool>)| {
                let world = world(lua)?;
                let found = if recursive.unwrap_or(false) {
                    world.find_first_descendant(this.0, &name)
                } else {
                    world.find_first_child(this.0, &name)
                };
                Ok(found.map(LuaInstance))
            },
        );
        // `GetChildren`, `IsA`, `Clone`, `Destroy` and the rest of the tree.
        crate::tree::add_methods(methods);
        // Attributes and the tag shortcuts.
        crate::collection::add_methods(methods);
        // `part:GetPropertyChangedSignal("Position")`: an event for one
        // property, whose handlers get no arguments.
        methods.add_method("GetPropertyChangedSignal", |lua, this, property: String| {
            let world = world(lua)?;
            let instance = world.get(this.0).ok_or_else(destroyed)?;
            // `&'static str`s, so the signal can keep the one that matches.
            let names: Vec<&'static str> = ["Name", "Parent"]
                .into_iter()
                .chain(instance.properties().map(|(name, _)| name))
                .collect();
            match names.iter().find(|name| **name == property) {
                Some(&name) => Ok(LuaSignal {
                    owner: Owner::Instance(this.0),
                    event: Event::PropertyChanged(name),
                }),
                None => Err(mlua::Error::runtime(format!(
                    "{property} is not a property of {}; choose from {}",
                    instance.class(),
                    names.join(", ")
                ))),
            }
        });

        // `instance.Key`: a field, then a property, then a member only
        // some classes have, then an event, then a child with that name.
        methods.add_meta_method(MetaMethod::Index, |lua, this, key: String| {
            get(lua, this.0, &key)
        });
        // `instance.Key = value`.
        methods.add_meta_method(
            MetaMethod::NewIndex,
            |lua, this, (key, value): (String, mlua::Value)| set(lua, this.0, &key, value),
        );
        methods.add_meta_method(MetaMethod::Eq, |lua, this, other: mlua::Value| {
            Ok(LuaInstance::from_lua(other, lua).is_ok_and(|other| other == *this))
        });
        // Like Roblox, an Instance prints as its name.
        methods.add_meta_method(MetaMethod::ToString, |lua, this, ()| {
            let world = world(lua)?;
            Ok(world.get(this.0).map_or_else(
                || "(destroyed Instance)".to_owned(),
                |i| i.name().to_owned(),
            ))
        });
    }
}

fn get(lua: &Lua, id: InstanceId, key: &str) -> mlua::Result<mlua::Value> {
    let world = world(lua)?;
    let instance = world.get(id).ok_or_else(destroyed)?;
    match key {
        "Name" => return instance.name().into_lua(lua),
        "ClassName" => return instance.class().as_str().into_lua(lua),
        "Parent" => return instance.parent().map(LuaInstance).into_lua(lua),
        // Lua, so it can wait (see `tree`).
        "WaitForChild" => return crate::tree::wait_for_child(lua)?.into_lua(lua),
        _ => {}
    }
    // Empty is the element's Name (see `Instance::accessible_name`).
    if key == "AccessibleName"
        && let Some(name) = instance.accessible_name()
    {
        return name.into_lua(lua);
    }
    if let Some(value) = instance.property(key) {
        return value_to_lua(lua, value);
    }
    // Made from Position and Orientation, not stored (see `cframe`).
    if key == "CFrame"
        && let Some(cf) = LuaCFrame::of_instance(instance)
    {
        return cf.into_lua(lua);
    }
    // Worked out from the attachment's part (see `attachment`).
    if let Some(value) = crate::attachment::computed(lua, id, key)? {
        return Ok(value);
    }
    // Worked out from the part's shape and material (see `physical`).
    if let Some(value) = crate::physical::computed(lua, id, key)? {
        return Ok(value);
    }
    // Worked out from where a joint's attachments are now.
    if let Some(value) = axcenity_physics::joints::current(&world, id, key) {
        return value.into_lua(lua);
    }
    let class = instance.class();
    // Live animator state, not a saved property (see `core::AnimatorState`).
    if class == ClassName::Animator && key == "CurrentState" {
        let state = world.animator(id).map(|s| s.current_state.clone());
        return state.unwrap_or_default().into_lua(lua);
    }
    // A track's live playback (see `core::AnimationTrackState`).
    if class == ClassName::AnimationTrack
        && let Some(track) = world.animation_track(id)
    {
        let value = match key {
            "IsPlaying" => Some(track.playing.into_lua(lua)?),
            "Length" => Some(track.length.into_lua(lua)?),
            "Looped" => Some(track.looped.unwrap_or(false).into_lua(lua)?),
            "Speed" => Some(track.speed.into_lua(lua)?),
            "TimePosition" => Some(track.time.into_lua(lua)?),
            "WeightCurrent" => Some(track.weight.into_lua(lua)?),
            "WeightTarget" => Some(track.target_weight.into_lua(lua)?),
            "Animation" => Some(LuaInstance(track.animation).into_lua(lua)?),
            _ => None,
        };
        if let Some(value) = value {
            return Ok(value);
        }
    }
    if let Some(member) = crate::bindable::callback_member(class, key) {
        return Err(crate::bindable::read_callback_error(class, member));
    }
    if let Some(member) = class_member(lua, class, key)? {
        return Ok(member);
    }
    let mut events = class.events().iter().chain(&INSTANCE_EVENTS);
    if let Some(&event) = events.find(|&&event| event == key) {
        // `OnServerEvent` only works on the server, and `OnClientEvent` only
        // on the client.
        sides::check(lua, key)?;
        return LuaSignal::named(Owner::Instance(id), event).into_lua(lua);
    }
    if let Some(child) = world.find_first_child(id, key) {
        return LuaInstance(child).into_lua(lua);
    }
    Err(not_a_member(&world, id, key))
}

fn set(lua: &Lua, id: InstanceId, key: &str, value: mlua::Value) -> mlua::Result<()> {
    // `OnInvoke` and `OnServerInvoke` hold a function, not a property.
    let class = world(lua)?.get(id).map(|instance| instance.class());
    if let Some(class) = class
        && let Some(member) = crate::bindable::callback_member(class, key)
    {
        return crate::bindable::set_callback(lua, id, class, member, value);
    }
    // Converting `value` may need to read the World (to name an Instance's
    // class in a message), so convert first and borrow the World mutably after.
    match key {
        "Name" => {
            let mlua::Value::String(name) = &value else {
                return Err(mlua::Error::runtime(format!(
                    "Name must be a string, not a {}",
                    type_name(lua, &value)
                )));
            };
            let name = name.to_string_lossy();
            world_mut(lua)?.set_name(id, name).map_err(|_| destroyed())
        }
        "Parent" => {
            let parent = match &value {
                mlua::Value::Nil => None,
                other => match LuaInstance::from_lua(other.clone(), lua) {
                    Ok(parent) => Some(parent.0),
                    Err(_) => {
                        return Err(mlua::Error::runtime(format!(
                            "Parent must be an Instance or nil, not a {}",
                            type_name(lua, other)
                        )));
                    }
                },
            };
            let mut world = world_mut(lua)?;
            if !world.contains(id) {
                return Err(mlua::Error::runtime(
                    "can't set Parent: this Instance has been destroyed, so its Parent is locked",
                ));
            }
            if parent.is_some_and(|parent| !world.contains(parent)) {
                return Err(mlua::Error::runtime(
                    "can't set Parent: the new Parent has been destroyed",
                ));
            }
            world
                .set_parent(id, parent)
                .map_err(|err| mlua::Error::runtime(format!("can't set Parent: {err}")))
        }
        "ClassName" => Err(mlua::Error::runtime("ClassName can't be changed")),
        "CFrame"
            if world(lua)?
                .get(id)
                .and_then(LuaCFrame::of_instance)
                .is_some() =>
        {
            let class = world(lua)?.get(id).ok_or_else(destroyed)?.class();
            let cf = LuaCFrame::from_lua(value.clone(), lua).map_err(|_| {
                mlua::Error::runtime(format!(
                    "{class}.CFrame must be a CFrame, not a {}",
                    type_name(lua, &value)
                ))
            })?;
            cf.write_to(&mut *world_mut(lua)?, id)
                .map_err(mlua::Error::runtime)
        }
        key if crate::attachment::FIELDS.contains(&key) && class == Some(ClassName::Attachment) => {
            crate::attachment::set(lua, id, key, value)
        }
        key if crate::physical::COMPUTED.contains(&key)
            && class.is_some_and(|class| class.is_base_part()) =>
        {
            Err(mlua::Error::runtime(format!(
                "{key} is read-only: it comes from the part's size, shape and physical properties"
            )))
        }
        key if class.is_some_and(|class| axcenity_physics::joints::has_current(class, key)) => {
            Err(mlua::Error::runtime(format!(
                "{key} is read-only: it's where the constraint's attachments are now"
            )))
        }
        "Health" if class == Some(ClassName::Health) => crate::health::set(lua, id, value),
        "Occupant" if class.is_some_and(ClassName::is_seat) => {
            Err(mlua::Error::runtime(crate::seat::OCCUPANT_READ_ONLY))
        }
        "CurrentState"
            if world(lua)?
                .get(id)
                .is_some_and(|i| i.class() == ClassName::Animator) =>
        {
            Err(mlua::Error::runtime(
                "Animator.CurrentState can't be set; use animator:Play(\"StateName\") to change state",
            ))
        }
        "Looped" | "TimePosition" | "IsPlaying" | "Length" | "Speed" | "WeightCurrent"
        | "WeightTarget" | "Animation"
            if world(lua)?
                .get(id)
                .is_some_and(|i| i.class() == ClassName::AnimationTrack) =>
        {
            set_track_field(lua, id, key, value)
        }
        _ => {
            let (class, kind, enum_name) = {
                let world = world(lua)?;
                let instance = world.get(id).ok_or_else(destroyed)?;
                let class = instance.class();
                match instance.property(key) {
                    Some(current) => (
                        class,
                        current.kind(),
                        match current {
                            Value::EnumItem(item) => Some(item.enum_name.clone()),
                            _ => None,
                        },
                    ),
                    None if class.events().contains(&key) || INSTANCE_EVENTS.contains(&key) => {
                        return Err(mlua::Error::runtime(format!(
                            "{class}.{key} is an event and can't be set; use {key}:Connect(function) to react to it"
                        )));
                    }
                    None => return Err(not_a_member(&world, id, key)),
                }
            };
            // An enum item property (`ActuatorType`) takes an item of its
            // own enum, or that item's name.
            let new_value = match enum_name {
                Some(enum_name) => crate::types::enum_item_from_lua(lua, value, &enum_name),
                None => value_from_lua(lua, value, kind),
            }
            .map_err(|message| mlua::Error::runtime(format!("{class}.{key} {message}")))?;
            if key == "CurrentPage"
                && class == ClassName::PageLayout
                && let Value::String(name) = &new_value
                && !name.is_empty()
            {
                let found = {
                    let world = world(lua)?;
                    world.get(id).is_some_and(|layout| {
                        layout.children().iter().any(|&child| {
                            world.get(child).is_some_and(|page| {
                                page.class() == ClassName::Frame && page.name() == name
                            })
                        })
                    })
                };
                if !found {
                    return Err(mlua::Error::runtime(format!(
                        "PageLayout has no page Frame named \"{name}\""
                    )));
                }
            }
            world_mut(lua)?
                .set_property(id, key, new_value)
                .map_err(mlua::Error::runtime)
        }
    }
}

/// Methods only some classes have, such as `remote:FireServer` and
/// `game:GetService`. Made once per interpreter (see `context.rs`) and kept
/// in its app data.
pub struct ClassMethods(pub Vec<(ClassName, &'static str, Function)>);

/// A member of `class` that isn't a property or an event: a method such as
/// `remote:FireServer`, or a field such as `workspace.CurrentCamera`.
/// Side-only members check the side first.
fn class_member(lua: &Lua, class: ClassName, key: &str) -> mlua::Result<Option<mlua::Value>> {
    let value = match (class, key) {
        // Roblox code uses `game:GetService("UserInputService")`, but
        // `game.UserInputService` works there too.
        (ClassName::DataModel, "UserInputService") => LuaUserInputService.into_lua(lua)?,
        (ClassName::DataModel, "ContextActionService") => LuaContextActionService.into_lua(lua)?,
        (ClassName::DataModel, "RunService") => LuaRunService.into_lua(lua)?,
        (ClassName::DataModel, "TweenService") => LuaTweenService.into_lua(lua)?,
        (ClassName::DataModel, "CollectionService") => LuaCollectionService.into_lua(lua)?,
        (ClassName::DataModel, "SaveService") => LuaSaveService.into_lua(lua)?,
        (ClassName::DataModel, "LocalizationService") => LuaLocalizationService.into_lua(lua)?,
        (ClassName::DataModel, "GuiService") => LuaGuiService.into_lua(lua)?,
        (ClassName::DataModel, "Debris") => LuaDebris.into_lua(lua)?,
        (ClassName::DataModel, "HttpService") => LuaHttpService.into_lua(lua)?,
        (ClassName::DataModel, "DataService") => LuaDataService.into_lua(lua)?,
        (ClassName::DataModel, "ReplicatedFirst") => LuaReplicatedFirst.into_lua(lua)?,
        (ClassName::DataModel, "UserGameSettings") => LuaUserGameSettings.into_lua(lua)?,
        (ClassName::Workspace, "CurrentCamera") => LuaCamera.into_lua(lua)?,
        (ClassName::Players, "LocalPlayer") => lua
            .app_data_ref::<LocalPlayer>()
            .map(|player| LuaInstance(player.0))
            .into_lua(lua)?,
        _ => {
            let method = lua.app_data_ref::<ClassMethods>().and_then(|methods| {
                methods
                    .0
                    .iter()
                    .find(|(c, name, _)| *c == class && *name == key)
                    .map(|(_, _, function)| function.clone())
            });
            match method {
                Some(function) => mlua::Value::Function(function),
                None => return Ok(None),
            }
        }
    };
    sides::check(lua, key)?;
    Ok(Some(value))
}

/// The methods exposed by Sound Instances, which queue host audio actions.
pub fn sound_methods(lua: &Lua) -> mlua::Result<Vec<(ClassName, &'static str, Function)>> {
    let play = lua.create_function(|lua, this: LuaInstance| {
        let (sound_id, volume, looped) = {
            let world = world(lua)?;
            let sound = world.get(this.0).ok_or_else(destroyed)?;
            let sound_id = sound
                .property("SoundId")
                .and_then(|value| value.as_str())
                .ok_or_else(|| mlua::Error::runtime("Sound.SoundId must be a file path"))?;
            let volume = sound
                .property("Volume")
                .and_then(|value| value.as_number())
                .ok_or_else(|| mlua::Error::runtime("Sound.Volume must be a number"))?;
            let looped = sound
                .property("Looped")
                .and_then(|value| value.as_bool())
                .ok_or_else(|| mlua::Error::runtime("Sound.Looped must be a boolean"))?;
            (sound_id.to_owned(), volume, looped)
        };
        if sound_id.is_empty() {
            return Err(mlua::Error::runtime(
                "Sound.SoundId must name an audio file inside the project folder",
            ));
        }
        if !volume.is_finite() || volume < 0.0 {
            return Err(mlua::Error::runtime(
                "Sound.Volume must be a finite number greater than or equal to 0",
            ));
        }
        set_playing(lua, this.0, true)?;
        let mut actions = lua
            .app_data_mut::<crate::SoundActions>()
            .ok_or_else(|| mlua::Error::runtime("sound actions aren't ready"))?;
        actions.0.push(SoundAction::Play {
            instance: this.0,
            sound_id,
            volume,
            looped,
        });
        Ok(())
    })?;
    let stop = lua.create_function(|lua, this: LuaInstance| {
        set_playing(lua, this.0, false)?;
        let mut actions = lua
            .app_data_mut::<crate::SoundActions>()
            .ok_or_else(|| mlua::Error::runtime("sound actions aren't ready"))?;
        actions.0.push(SoundAction::Stop(this.0));
        Ok(())
    })?;
    // Pausing and resuming are `Playing` changes; the host's audio follows it.
    let pause = lua.create_function(|lua, this: LuaInstance| set_playing(lua, this.0, false))?;
    let resume = lua.create_function(|lua, this: LuaInstance| set_playing(lua, this.0, true))?;
    Ok(vec![
        (ClassName::Sound, "Play", play),
        (ClassName::Sound, "Stop", stop),
        (ClassName::Sound, "Pause", pause),
        (ClassName::Sound, "Resume", resume),
    ])
}

/// `animator:Play(state, blend?)`, which asks the host's animator to
/// switch to a state, blending over `blend` seconds (default a quarter).
pub fn animator_methods(lua: &Lua) -> mlua::Result<Vec<(ClassName, &'static str, Function)>> {
    let play = lua.create_function(
        |lua, (this, state, blend): (LuaInstance, mlua::Value, Option<f64>)| {
            let mlua::Value::String(state) = &state else {
                return Err(mlua::Error::runtime(format!(
                    "Play needs the name of a state, such as animator:Play(\"Walk\"), not a {}",
                    type_name(lua, &state)
                )));
            };
            if let Some(blend) = blend
                && !(blend.is_finite() && blend >= 0.0)
            {
                return Err(mlua::Error::runtime(
                    "Play's blend must be a number of seconds, 0 or more",
                ));
            }
            let state = state.to_string_lossy();
            let mut world = world_mut(lua)?;
            let live = world.animator_mut(this.0).ok_or_else(destroyed)?;
            live.play = Some(axcenity_core::PlayRequest { state, blend });
            Ok(())
        },
    )?;
    let set_bool = lua.create_function(
        |lua, (this, name, value): (LuaInstance, mlua::Value, mlua::Value)| {
            let name = parameter_name(lua, &name, "SetBool")?;
            let mlua::Value::Boolean(value) = value else {
                return Err(mlua::Error::runtime(format!(
                    "SetBool needs true or false, not a {}",
                    type_name(lua, &value)
                )));
            };
            set_parameter(lua, this.0, name, Parameter::Bool(value))
        },
    )?;
    let set_number = lua.create_function(
        |lua, (this, name, value): (LuaInstance, mlua::Value, mlua::Value)| {
            let name = parameter_name(lua, &name, "SetNumber")?;
            let number = match value {
                mlua::Value::Number(n) => n,
                mlua::Value::Integer(n) => n as f64,
                other => {
                    return Err(mlua::Error::runtime(format!(
                        "SetNumber needs a number, not a {}",
                        type_name(lua, &other)
                    )));
                }
            };
            if !number.is_finite() {
                return Err(mlua::Error::runtime("SetNumber needs a finite number"));
            }
            set_parameter(lua, this.0, name, Parameter::Number(number))
        },
    )?;
    let set_trigger = lua.create_function(|lua, (this, name): (LuaInstance, mlua::Value)| {
        let name = parameter_name(lua, &name, "SetTrigger")?;
        set_parameter(lua, this.0, name, Parameter::Trigger(true))
    })?;
    let reset_trigger = lua.create_function(|lua, (this, name): (LuaInstance, mlua::Value)| {
        let name = parameter_name(lua, &name, "ResetTrigger")?;
        set_parameter(lua, this.0, name, Parameter::Trigger(false))
    })?;
    let get_bool = lua.create_function(|lua, (this, name): (LuaInstance, mlua::Value)| {
        let name = parameter_name(lua, &name, "GetBool")?;
        match get_parameter(lua, this.0, &name, "Bool")? {
            Parameter::Bool(value) => Ok(value),
            _ => unreachable!("get_parameter checks the kind"),
        }
    })?;
    let get_number = lua.create_function(|lua, (this, name): (LuaInstance, mlua::Value)| {
        let name = parameter_name(lua, &name, "GetNumber")?;
        match get_parameter(lua, this.0, &name, "Number")? {
            Parameter::Number(value) => Ok(value),
            _ => unreachable!("get_parameter checks the kind"),
        }
    })?;
    let mut methods = vec![
        (ClassName::Animator, "Play", play),
        (ClassName::Animator, "SetBool", set_bool),
        (ClassName::Animator, "GetBool", get_bool),
        (ClassName::Animator, "SetNumber", set_number),
        (ClassName::Animator, "GetNumber", get_number),
        (ClassName::Animator, "SetTrigger", set_trigger),
        (ClassName::Animator, "ResetTrigger", reset_trigger),
    ];
    methods.extend(animator_track_methods(lua)?);
    Ok(methods)
}

/// `animator:LoadAnimation(animation)` and
/// `animator:GetPlayingAnimationTracks()`.
fn animator_track_methods(lua: &Lua) -> mlua::Result<Vec<(ClassName, &'static str, Function)>> {
    let load = lua.create_function(|lua, (this, animation): (LuaInstance, mlua::Value)| {
        let animation = match LuaInstance::from_lua(animation.clone(), lua) {
            Ok(animation) => animation,
            Err(_) => {
                return Err(mlua::Error::runtime(format!(
                    "LoadAnimation needs an Animation, such as animator:LoadAnimation(door.Open), not a {}",
                    type_name(lua, &animation)
                )));
            }
        };
        let mut world = world_mut(lua)?;
        if world.get(this.0).is_none() || world.get(animation.0).is_none() {
            return Err(destroyed());
        }
        world
            .load_animation(this.0, animation.0)
            .map(LuaInstance)
            .ok_or_else(|| {
                mlua::Error::runtime("LoadAnimation needs an Animation, not another kind of Instance")
            })
    })?;
    let playing = lua.create_function(|lua, this: LuaInstance| {
        let tracks: Vec<LuaInstance> = {
            let world = world(lua)?;
            let state = world.animator(this.0);
            state
                .into_iter()
                .flat_map(|state| state.tracks.iter().copied())
                .filter(|&track| world.animation_track(track).is_some_and(|t| t.playing))
                .map(LuaInstance)
                .collect()
        };
        lua.create_sequence_from(tracks)
    })?;
    Ok(vec![
        (ClassName::Animator, "LoadAnimation", load),
        (ClassName::Animator, "GetPlayingAnimationTracks", playing),
    ])
}

/// An optional number argument of `method`: finite and 0 or more.
fn seconds_or(value: Option<f64>, default: f64, what: &str, method: &str) -> mlua::Result<f64> {
    let value = value.unwrap_or(default);
    if !(value.is_finite() && value >= 0.0) {
        return Err(mlua::Error::runtime(format!(
            "{method}'s {what} must be a number, 0 or more"
        )));
    }
    Ok(value)
}

/// The seconds a track fades over when a script doesn't say, as in Roblox.
const DEFAULT_TRACK_FADE: f64 = 0.1;

/// AnimationTrack methods: `Play(fadeTime?, weight?, speed?)`,
/// `Stop(fadeTime?)`, `AdjustSpeed(speed)`, `AdjustWeight(weight, fadeTime?)`
/// and `GetMarkerReachedSignal(name)`.
pub fn animation_track_methods(
    lua: &Lua,
) -> mlua::Result<Vec<(ClassName, &'static str, Function)>> {
    let play = lua.create_function(
        |lua, (this, fade, weight, speed): (LuaInstance, Option<f64>, Option<f64>, Option<f64>)| {
            let fade = seconds_or(fade, DEFAULT_TRACK_FADE, "fade time", "Play")?;
            let weight = seconds_or(weight, 1.0, "weight", "Play")?;
            let speed = speed.unwrap_or(1.0);
            if !speed.is_finite() {
                return Err(mlua::Error::runtime("Play's speed must be a number"));
            }
            let mut world = world_mut(lua)?;
            let animator = world
                .animation_track(this.0)
                .ok_or_else(destroyed)?
                .animator;
            let order = world.animator_mut(animator).map_or(0, |a| a.start_order());
            let track = world.animation_track_mut(this.0).ok_or_else(destroyed)?;
            track.playing = true;
            track.stopping = false;
            track.time = 0.0;
            track.speed = speed;
            track.target_weight = weight;
            track.fade = fade;
            if fade <= 0.0 {
                track.weight = weight;
            }
            track.order = order;
            Ok(())
        },
    )?;
    let stop = lua.create_function(|lua, (this, fade): (LuaInstance, Option<f64>)| {
        let fade = seconds_or(fade, DEFAULT_TRACK_FADE, "fade time", "Stop")?;
        let mut world = world_mut(lua)?;
        let track = world.animation_track_mut(this.0).ok_or_else(destroyed)?;
        if track.playing || track.weight > 0.0 {
            track.playing = false;
            track.stopping = true;
            track.target_weight = 0.0;
            track.fade = fade;
            if fade <= 0.0 {
                track.weight = 0.0;
            }
        }
        Ok(())
    })?;
    let adjust_speed = lua.create_function(|lua, (this, speed): (LuaInstance, Option<f64>)| {
        let speed = speed.unwrap_or(1.0);
        if !speed.is_finite() {
            return Err(mlua::Error::runtime("AdjustSpeed needs a number"));
        }
        let mut world = world_mut(lua)?;
        world
            .animation_track_mut(this.0)
            .ok_or_else(destroyed)?
            .speed = speed;
        Ok(())
    })?;
    let adjust_weight = lua.create_function(
        |lua, (this, weight, fade): (LuaInstance, Option<f64>, Option<f64>)| {
            let weight = seconds_or(weight, 1.0, "weight", "AdjustWeight")?;
            let fade = seconds_or(fade, DEFAULT_TRACK_FADE, "fade time", "AdjustWeight")?;
            let mut world = world_mut(lua)?;
            let track = world.animation_track_mut(this.0).ok_or_else(destroyed)?;
            track.target_weight = weight;
            track.fade = fade;
            if fade <= 0.0 {
                track.weight = weight;
            }
            Ok(())
        },
    )?;
    let marker = lua.create_function(|lua, (this, name): (LuaInstance, mlua::Value)| {
        let mlua::Value::String(name) = &name else {
            return Err(mlua::Error::runtime(format!(
                "GetMarkerReachedSignal needs an event's name, such as track:GetMarkerReachedSignal(\"Footstep\"), not a {}",
                type_name(lua, &name)
            )));
        };
        if world(lua)?.get(this.0).is_none() {
            return Err(destroyed());
        }
        let id = crate::events::name_id(lua, &name.to_string_lossy());
        Ok(LuaSignal {
            owner: Owner::Instance(this.0),
            event: Event::Marker(id),
        })
    })?;
    Ok(vec![
        (ClassName::AnimationTrack, "Play", play),
        (ClassName::AnimationTrack, "Stop", stop),
        (ClassName::AnimationTrack, "AdjustSpeed", adjust_speed),
        (ClassName::AnimationTrack, "AdjustWeight", adjust_weight),
        (ClassName::AnimationTrack, "GetMarkerReachedSignal", marker),
    ])
}

/// Sets an AnimationTrack's `Looped` or `TimePosition`; the rest are read-only.
fn set_track_field(lua: &Lua, id: InstanceId, key: &str, value: mlua::Value) -> mlua::Result<()> {
    let message =
        |hint: &str| mlua::Error::runtime(format!("AnimationTrack.{key} can't be set{hint}"));
    match key {
        "Looped" => {
            let mlua::Value::Boolean(looped) = value else {
                return Err(mlua::Error::runtime(format!(
                    "AnimationTrack.Looped must be true or false, not a {}",
                    type_name(lua, &value)
                )));
            };
            world_mut(lua)?
                .animation_track_mut(id)
                .ok_or_else(destroyed)?
                .looped = Some(looped);
            Ok(())
        }
        "TimePosition" => {
            let time = match value {
                mlua::Value::Number(n) => n,
                mlua::Value::Integer(n) => n as f64,
                other => {
                    return Err(mlua::Error::runtime(format!(
                        "AnimationTrack.TimePosition must be a number of seconds, not a {}",
                        type_name(lua, &other)
                    )));
                }
            };
            if !time.is_finite() {
                return Err(mlua::Error::runtime(
                    "AnimationTrack.TimePosition must be a finite number of seconds",
                ));
            }
            let mut world = world_mut(lua)?;
            let track = world.animation_track_mut(id).ok_or_else(destroyed)?;
            track.time = if track.length > 0.0 {
                time.clamp(0.0, track.length)
            } else {
                time.max(0.0)
            };
            Ok(())
        }
        "Speed" => Err(message("; use track:AdjustSpeed(speed)")),
        "WeightCurrent" | "WeightTarget" => Err(message("; use track:AdjustWeight(weight)")),
        _ => Err(message(": it's read-only")),
    }
}

/// Tells scripts once, per side, that `animation:Play()` and the rest
/// still work but `animator:LoadAnimation(animation)` is the new way.
fn note_legacy_animation(lua: &Lua, method: &str) {
    struct Noted;
    if lua.app_data_ref::<Noted>().is_some() {
        return;
    }
    lua.set_app_data(Noted);
    crate::push_output(
        lua,
        crate::OutputKind::Print,
        format!(
            "animation:{method}() still works, through the Animator beside the Animation; animator:LoadAnimation(animation) is the new way, and gives an AnimationTrack to play, stop and blend."
        ),
    );
}

/// `Animation` methods kept from before AnimationTracks: Play, Stop, Pause
/// and Resume, played by the Animator on the same parent.
pub fn property_animation_methods(
    lua: &Lua,
) -> mlua::Result<Vec<(ClassName, &'static str, Function)>> {
    let play = lua.create_function(|lua, this: LuaInstance| {
        note_legacy_animation(lua, "Play");
        let mut world = world_mut(lua)?;
        let live = world.property_animation_mut(this.0).ok_or_else(destroyed)?;
        live.play = true;
        Ok(())
    })?;
    let stop = lua.create_function(|lua, this: LuaInstance| {
        note_legacy_animation(lua, "Stop");
        let mut world = world_mut(lua)?;
        let live = world.property_animation_mut(this.0).ok_or_else(destroyed)?;
        live.stop = true;
        Ok(())
    })?;
    let pause = lua.create_function(|lua, this: LuaInstance| {
        note_legacy_animation(lua, "Pause");
        let mut world = world_mut(lua)?;
        let live = world.property_animation_mut(this.0).ok_or_else(destroyed)?;
        live.pause = true;
        Ok(())
    })?;
    let resume = lua.create_function(|lua, this: LuaInstance| {
        note_legacy_animation(lua, "Resume");
        let mut world = world_mut(lua)?;
        let live = world.property_animation_mut(this.0).ok_or_else(destroyed)?;
        live.resume = true;
        Ok(())
    })?;
    Ok(vec![
        (ClassName::Animation, "Play", play),
        (ClassName::Animation, "Stop", stop),
        (ClassName::Animation, "Pause", pause),
        (ClassName::Animation, "Resume", resume),
    ])
}

/// A parameter's name, given to `method`.
fn parameter_name(lua: &Lua, name: &mlua::Value, method: &str) -> mlua::Result<String> {
    match name {
        mlua::Value::String(name) => Ok(name.to_string_lossy()),
        other => Err(mlua::Error::runtime(format!(
            "{method} needs a parameter's name, such as animator:{method}(\"Speed\", ...), not a {}",
            type_name(lua, other)
        ))),
    }
}

/// Sets the animator's parameter `name`, which mustn't be of another kind.
/// A parameter the animator doesn't have yet is added (the host reports
/// it if its controller doesn't have it either).
fn set_parameter(lua: &Lua, id: InstanceId, name: String, value: Parameter) -> mlua::Result<()> {
    let mut world = world_mut(lua)?;
    let live = world.animator_mut(id).ok_or_else(destroyed)?;
    if let Some(existing) = live.parameters.get(&name)
        && existing.kind() != value.kind()
    {
        let kind = existing.kind();
        return Err(mlua::Error::runtime(format!(
            "`{name}` is a {kind} parameter; use {}",
            match kind {
                "Bool" => "SetBool(name, true or false)",
                "Number" => "SetNumber(name, number)",
                _ => "SetTrigger(name) or ResetTrigger(name)",
            }
        )));
    }
    live.parameters.insert(name, value);
    Ok(())
}

/// The animator's parameter `name`, which must be a `kind` one.
fn get_parameter(lua: &Lua, id: InstanceId, name: &str, kind: &str) -> mlua::Result<Parameter> {
    let world = world(lua)?;
    if world.get(id).is_none() {
        return Err(destroyed());
    }
    let parameters = world.animator(id).map(|live| &live.parameters);
    match parameters.and_then(|parameters| parameters.get(name)) {
        Some(value) if value.kind() == kind => Ok(*value),
        Some(value) => Err(mlua::Error::runtime(format!(
            "`{name}` is a {} parameter, not a {kind} one",
            value.kind()
        ))),
        None => {
            let names: Vec<&str> = parameters
                .into_iter()
                .flat_map(|parameters| parameters.keys().map(String::as_str))
                .collect();
            Err(mlua::Error::runtime(if names.is_empty() {
                format!(
                    "the Animator has no parameter named `{name}` (it gets its state machine's parameters when the game runs)"
                )
            } else {
                format!(
                    "the Animator has no parameter named `{name}`; it has {}",
                    names.join(", ")
                )
            }))
        }
    }
}

/// The most particles one `Emit` call gives off, so a typo can't freeze the game.
pub const MAX_EMIT: u32 = 1000;

/// `emitter:Emit(count)`, which asks the host to give off particles now.
pub fn particle_methods(lua: &Lua) -> mlua::Result<Vec<(ClassName, &'static str, Function)>> {
    let emit = lua.create_function(|lua, (this, count): (LuaInstance, f64)| {
        if world(lua)?.get(this.0).is_none() {
            return Err(destroyed());
        }
        if !count.is_finite() || count < 0.0 {
            return Err(mlua::Error::runtime(
                "Emit needs a number of particles, 0 or more",
            ));
        }
        let count = (count.round() as u32).min(MAX_EMIT);
        let mut emits = lua
            .app_data_mut::<crate::ParticleEmits>()
            .ok_or_else(|| mlua::Error::runtime("particles aren't ready"))?;
        emits.0.push((this.0, count));
        Ok(())
    })?;
    Ok(vec![(ClassName::ParticleEmitter, "Emit", emit)])
}

/// Sets a Sound's `Playing`, which fires its change events like any property.
fn set_playing(lua: &Lua, sound: InstanceId, playing: bool) -> mlua::Result<()> {
    world_mut(lua)?
        .set_property(sound, "Playing", Value::Bool(playing))
        .map_err(|_| destroyed())
}

/// `game:GetService(name)`: a service by its class name, such as
/// `"Players"`, or `"UserInputService"`, `"ContextActionService"` or
/// `"UserGameSettings"` (LocalScripts only), `"RunService"`, `"TweenService"`, `"CollectionService"`, `"SaveService"`,
/// `"LocalizationService"`, `"GuiService"`, `"Debris"`, `"HttpService"`,
/// `"DataService"` or `"ReplicatedFirst"`, which aren't Instances.
pub fn get_service(lua: &Lua) -> mlua::Result<Function> {
    lua.create_function(|lua, (_game, name): (mlua::Value, String)| {
        match name.as_str() {
            "UserInputService" => {
                sides::check(lua, &name)?;
                return LuaUserInputService.into_lua(lua);
            }
            "ContextActionService" => {
                sides::check(lua, &name)?;
                return LuaContextActionService.into_lua(lua);
            }
            "UserGameSettings" => {
                sides::check(lua, &name)?;
                return LuaUserGameSettings.into_lua(lua);
            }
            "RunService" => return LuaRunService.into_lua(lua),
            "TweenService" => return LuaTweenService.into_lua(lua),
            "CollectionService" => return LuaCollectionService.into_lua(lua),
            "SaveService" => return LuaSaveService.into_lua(lua),
            "LocalizationService" => return LuaLocalizationService.into_lua(lua),
            "GuiService" => return LuaGuiService.into_lua(lua),
            "Debris" => return LuaDebris.into_lua(lua),
            "HttpService" => return LuaHttpService.into_lua(lua),
            "DataService" => return LuaDataService.into_lua(lua),
            "PhysicsService" => return crate::query::LuaPhysicsService.into_lua(lua),
            "ReplicatedFirst" => return LuaReplicatedFirst.into_lua(lua),
            _ => {}
        }
        let world = world(lua)?;
        let services: Vec<InstanceId> = world
            .get(world.root())
            .map(|game| game.children().to_vec())
            .unwrap_or_default();
        let found = services.iter().copied().find(|&id| {
            world
                .get(id)
                .is_some_and(|service| service.class().as_str() == name)
        });
        match found {
            Some(id) => LuaInstance(id).into_lua(lua),
            None => {
                let mut names: Vec<&str> = services
                    .iter()
                    .filter_map(|&id| world.get(id).map(|s| s.class().as_str()))
                    .collect();
                names.extend([
                    "CollectionService",
                    "ContextActionService",
                    "DataService",
                    "Debris",
                    "GuiService",
                    "HttpService",
                    "LocalizationService",
                    "PhysicsService",
                    "ReplicatedFirst",
                    "RunService",
                    "SaveService",
                    "TweenService",
                    "UserGameSettings",
                    "UserInputService",
                ]);
                Err(mlua::Error::runtime(format!(
                    "there is no service named \"{name}\"; choose from {}",
                    names.join(", ")
                )))
            }
        }
    })
}

/// The error for `instance.Key` when `Key` is neither a property nor a child,
/// worded like Roblox's so it's familiar.
fn not_a_member(world: &World, id: InstanceId, key: &str) -> mlua::Error {
    let Some(instance) = world.get(id) else {
        return destroyed();
    };
    let class = instance.class();
    // `full_name` leaves `game` out, so `game` itself has an empty one.
    let name = match world.full_name(id) {
        Some(name) if !name.is_empty() => name,
        _ => instance.name().to_owned(),
    };
    mlua::Error::runtime(format!("{key} is not a valid member of {class} \"{name}\""))
}
