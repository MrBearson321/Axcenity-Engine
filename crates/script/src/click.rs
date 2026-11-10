//! ClickDetectors (Phase 44): the part under the mouse, hovering it and
//! clicking it.
//!
//! Each frame, after the input, [`Clicks::step`] casts the mouse's ray
//! from the camera. The first part it hits (leaving out the player
//! character) is pointed at when it, or its nearest ancestor that has one,
//! has a ClickDetector and the point is within that detector's
//! `MaxActivationDistance` of the character (or the camera). A click is a
//! press and a release of the same button over the same detector; clicks
//! the UI took never reach here.

use axcenity_assets::Assets;
use axcenity_core::{ClassName, InstanceId, UserInputType, Value, World};
use glam::Vec3;

use crate::camera::Ray;
use crate::cframe::{from_vec3, to_vec3};
use crate::input::PromptInput;
use crate::prompt::{Fired, Viewer};

/// How far the mouse's ray looks, in studs: as far as `Mouse.Target`.
const REACH: f32 = 1000.0;

/// What the mouse points at and presses, kept between frames.
#[derive(Default)]
pub struct Clicks {
    hovered: Option<InstanceId>,
    /// The detector each button (left, right) went down on.
    pressed: [Option<InstanceId>; 2],
}

/// The ClickDetector of `part`: one in it, or else in its nearest ancestor
/// that has one, below the Workspace.
fn detector_of(world: &World, part: InstanceId) -> Option<InstanceId> {
    let mut at = Some(part);
    while let Some(current) = at.filter(|&id| id != world.workspace()) {
        let instance = world.get(current)?;
        if let Some(&detector) = instance.children().iter().find(|&&child| {
            world
                .get(child)
                .is_some_and(|child| child.class() == ClassName::ClickDetector)
        }) {
            return Some(detector);
        }
        at = instance.parent();
    }
    None
}

/// True if the game has a ClickDetector, which [`Clicks::pointed`] needs.
fn any_detector(world: &World) -> bool {
    world.all_of(ClassName::ClickDetector).next().is_some()
}

impl Clicks {
    /// The cursor over the detector pointed at: its `CursorIcon`, or
    /// `pointer` when that's empty. `None` when nothing is pointed at.
    pub fn cursor(&self, world: &World) -> Option<String> {
        let detector = world.get(self.hovered?)?;
        let icon = detector
            .property("CursorIcon")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        Some(if icon.is_empty() { "pointer" } else { icon }.to_owned())
    }

    /// One frame: what the mouse's `ray` (none without a camera) points at,
    /// and this frame's clicks. Returns the events to fire, in order.
    pub fn step(
        &mut self,
        world: &World,
        assets: &Assets,
        input: &PromptInput,
        ray: Option<Ray>,
    ) -> Vec<Fired> {
        // With no detector the ray can't find one, so don't cast it.
        let hovered = ray
            .filter(|_| any_detector(world))
            .and_then(|ray| self.pointed(world, assets, ray));
        let mut fired = Vec::new();
        if hovered != self.hovered {
            if let Some(old) = self.hovered.filter(|&id| world.contains(id)) {
                fired.push(Fired::new(old, "MouseHoverLeave"));
            }
            if let Some(new) = hovered {
                fired.push(Fired::new(new, "MouseHoverEnter"));
            }
            self.hovered = hovered;
        }
        for &(button, down) in &input.clicks {
            let (slot, event) = match button {
                UserInputType::MouseButton1 => (0, "MouseClick"),
                UserInputType::MouseButton2 => (1, "RightMouseClick"),
                _ => continue,
            };
            if down {
                self.pressed[slot] = hovered;
            } else if let Some(detector) = self.pressed[slot].take()
                && Some(detector) == hovered
            {
                fired.push(Fired::new(detector, event));
            }
        }
        fired
    }

    /// The detector `ray` points at within its reach, if any.
    fn pointed(&self, world: &World, assets: &Assets, ray: Ray) -> Option<InstanceId> {
        let viewer = Viewer::find(world, Some(to_vec3(ray.origin)))?;
        let direction = to_vec3(ray.direction).normalize_or_zero() * REACH;
        let hit = axcenity_physics::cast_ray_excluding(
            world,
            assets,
            ray.origin,
            from_vec3(direction),
            &viewer.skip,
        )?;
        let detector = detector_of(world, hit.instance)?;
        let reach = world
            .get(detector)?
            .property("MaxActivationDistance")
            .and_then(Value::as_number)
            .filter(|n| n.is_finite())
            .unwrap_or(32.0) as f32;
        let at: Vec3 = to_vec3(hit.position);
        (at.distance(viewer.position) <= reach).then_some(detector)
    }
}
