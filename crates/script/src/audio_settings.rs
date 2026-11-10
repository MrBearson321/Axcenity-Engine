//! The default audio settings UI and volume sliders (Phase 52).
//!
//! A Slider with an ObjectValue named [`GROUP_BINDING`] inside it, whose
//! `Value` is a SoundGroup, is that group's volume slider; one with a child
//! named [`MASTER_BINDING`] is `UserGameSettings.MasterVolume`'s. Each
//! frame the slider runs from 0 to 1 and shows the volume, and when the
//! player drags it, the volume follows. A script changing the volume moves
//! the slider.
//!
//! When the game starts with a SoundGroup that's `UserAdjustable`, and the
//! Workspace has no [`GUI_NAME`] already, the engine makes one: a ScreenGui
//! (off until a script sets its `Enabled`) with a panel of labelled
//! sliders, the master volume first, then each adjustable group in tree
//! order. Its panel has the class `audio-settings` for stylesheets.

use std::collections::HashMap;

use axcenity_core::sound as mix;
use axcenity_core::{ClassName, Color3, InstanceId, UDim2, Value, World};

/// The ScreenGui the engine makes.
pub const GUI_NAME: &str = "AudioSettings";
/// The ObjectValue inside a Slider that binds it to a SoundGroup's `Volume`.
pub const GROUP_BINDING: &str = "SoundGroup";
/// The child of a Slider that binds it to the master volume.
pub const MASTER_BINDING: &str = "MasterVolume";

/// What a volume slider sets.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Target {
    Master,
    Group(InstanceId),
}

/// Volume sliders' state across frames.
#[derive(Default)]
pub struct VolumeSliders {
    /// Whether the start-of-game check for the default UI has run.
    started: bool,
    /// The value each slider showed after the last frame, so a change
    /// since is the player's.
    shown: HashMap<InstanceId, f64>,
}

impl VolumeSliders {
    /// One frame: makes the default UI the first time, then keeps every
    /// volume slider and its volume in line.
    pub fn step(&mut self, world: &mut World) {
        if !self.started {
            self.started = true;
            make_settings_gui(world);
        }
        self.bind(world);
    }

    fn bind(&mut self, world: &mut World) {
        let sliders: Vec<(InstanceId, Target)> = world
            .descendants_iter(world.root())
            .filter_map(|id| Some((id, target(world, id)?)))
            .collect();
        self.shown
            .retain(|id, _| sliders.iter().any(|(slider, _)| slider == id));
        for (slider, target) in sliders {
            let value = number(world, slider, "Value");
            let volume = match target {
                Target::Master => world.master_volume(),
                Target::Group(group) => number(world, group, "Volume"),
            };
            for (name, bound) in [("MinValue", 0.0), ("MaxValue", 1.0)] {
                if number(world, slider, name) != bound {
                    let _ = world.set_property(slider, name, Value::Number(bound));
                }
            }
            let moved = self.shown.get(&slider).is_some_and(|&last| last != value);
            if moved {
                let chosen = value.clamp(0.0, 1.0);
                match target {
                    Target::Master => world.set_master_volume(chosen),
                    Target::Group(group) => {
                        let _ = world.set_property(group, "Volume", Value::Number(chosen));
                    }
                }
            } else {
                // The slider runs to 1; a louder group shows full.
                let shown = volume.clamp(0.0, 1.0);
                if value != shown {
                    let _ = world.set_property(slider, "Value", Value::Number(shown));
                }
            }
            // What it holds now (the widget may keep it to its Step).
            self.shown.insert(slider, number(world, slider, "Value"));
        }
    }
}

fn number(world: &World, id: InstanceId, name: &str) -> f64 {
    world
        .get(id)
        .and_then(|instance| instance.property(name))
        .and_then(Value::as_number)
        .filter(|n| n.is_finite())
        .unwrap_or(0.0)
}

/// What the Slider `id` is bound to, if it's a volume slider.
fn target(world: &World, id: InstanceId) -> Option<Target> {
    let slider = world.get(id)?;
    if slider.class() != ClassName::Slider {
        return None;
    }
    slider.children().iter().find_map(|&child| {
        let binding = world.get(child)?;
        match binding.name() {
            MASTER_BINDING => Some(Target::Master),
            GROUP_BINDING if binding.class() == ClassName::ObjectValue => {
                match binding.property("Value") {
                    Some(Value::Instance(Some(group))) if mix::is_group(world, *group) => {
                        Some(Target::Group(*group))
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    })
}

/// Makes the default audio settings ScreenGui in the Workspace, when a
/// SoundGroup is `UserAdjustable` and there isn't one already. Returns it.
pub fn make_settings_gui(world: &mut World) -> Option<InstanceId> {
    let adjustable: Vec<(InstanceId, String)> = mix::groups(world)
        .into_iter()
        .filter(|group| group.user_adjustable)
        .map(|group| (group.id, group.name))
        .collect();
    let workspace = world.workspace();
    if adjustable.is_empty() || world.find_first_child(workspace, GUI_NAME).is_some() {
        return None;
    }

    const ROW: f64 = 44.0;
    const WIDTH: f64 = 420.0;
    let rows = adjustable.len() + 1;
    let height = 64.0 + ROW * rows as f64 + 16.0;

    let gui = make(world, ClassName::ScreenGui, GUI_NAME, workspace)?;
    set(world, gui, "Enabled", Value::Bool(false));
    set(world, gui, "DisplayOrder", Value::Number(10.0));
    let panel = make(world, ClassName::Frame, "Panel", gui)?;
    set(
        world,
        panel,
        "Class",
        Value::String("audio-settings".to_owned()),
    );
    set(
        world,
        panel,
        "Position",
        Value::UDim2(UDim2::new(
            0.5,
            -WIDTH as f32 / 2.0,
            0.5,
            -height as f32 / 2.0,
        )),
    );
    set(
        world,
        panel,
        "Size",
        Value::UDim2(UDim2::new(0.0, WIDTH as f32, 0.0, height as f32)),
    );
    set(
        world,
        panel,
        "BackgroundColor3",
        Value::Color3(Color3::new(0.08, 0.09, 0.12)),
    );
    set(world, panel, "BackgroundTransparency", Value::Number(0.1));
    let title = make(world, ClassName::TextLabel, "Title", panel)?;
    set(world, title, "Text", Value::String("Audio".to_owned()));
    set(world, title, "TextSize", Value::Number(24.0));
    set(
        world,
        title,
        "Position",
        Value::UDim2(UDim2::new(0.0, 20.0, 0.0, 16.0)),
    );
    set(
        world,
        title,
        "Size",
        Value::UDim2(UDim2::new(0.0, WIDTH as f32 - 40.0, 0.0, 32.0)),
    );

    let rows = std::iter::once((None, "Master volume".to_owned()))
        .chain(adjustable.into_iter().map(|(id, name)| (Some(id), name)));
    for (index, (group, name)) in rows.enumerate() {
        let top = (64.0 + ROW * index as f64) as f32;
        let label = make(world, ClassName::TextLabel, &format!("{name}Label"), panel)?;
        set(world, label, "Text", Value::String(name.clone()));
        set(
            world,
            label,
            "Position",
            Value::UDim2(UDim2::new(0.0, 20.0, 0.0, top)),
        );
        set(
            world,
            label,
            "Size",
            Value::UDim2(UDim2::new(0.0, 140.0, 0.0, 28.0)),
        );
        let slider = make(world, ClassName::Slider, &name, panel)?;
        set(
            world,
            slider,
            "Position",
            Value::UDim2(UDim2::new(0.0, 170.0, 0.0, top + 2.0)),
        );
        set(
            world,
            slider,
            "Size",
            Value::UDim2(UDim2::new(0.0, WIDTH as f32 - 190.0, 0.0, 24.0)),
        );
        set(world, slider, "Step", Value::Number(0.01));
        set(world, slider, "MaxValue", Value::Number(1.0));
        match group {
            Some(group) => {
                let binding = make(world, ClassName::ObjectValue, GROUP_BINDING, slider)?;
                set(world, binding, "Value", Value::Instance(Some(group)));
            }
            None => {
                make(world, ClassName::BoolValue, MASTER_BINDING, slider)?;
            }
        }
    }
    Some(gui)
}

fn make(world: &mut World, class: ClassName, name: &str, parent: InstanceId) -> Option<InstanceId> {
    let id = world.create(class).ok()?;
    world.set_name(id, name).ok()?;
    world.set_parent(id, Some(parent)).ok()?;
    Some(id)
}

fn set(world: &mut World, id: InstanceId, name: &str, value: Value) {
    let _ = world.set_property(id, name, value);
}

#[cfg(test)]
mod tests;
