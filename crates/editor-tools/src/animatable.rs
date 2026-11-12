//! Which Instances the Animation tab can animate, and their Animator.

use axcenity_core::{ClassName, InstanceId, World};

/// Whether an Instance of `class` can be animated: given an Animator, and
/// keyed by a clip.
pub fn can_animate(class: ClassName) -> bool {
    matches!(
        class,
        ClassName::Part
            | ClassName::MeshPart
            | ClassName::Seat
            | ClassName::VehicleSeat
            | ClassName::DamageZone
            | ClassName::ReverbZone
            | ClassName::Model
            | ClassName::ScreenGui
            | ClassName::Frame
            | ClassName::TextLabel
            | ClassName::TextButton
            | ClassName::TextBox
            | ClassName::ImageLabel
            | ClassName::PageLayout
            | ClassName::Table
            | ClassName::ScrollingFrame
            | ClassName::Slider
            | ClassName::Toggle
            | ClassName::Dropdown
            | ClassName::ProgressBar
    )
}

/// The first Animator in `object`.
pub fn animator_of(world: &World, object: InstanceId) -> Option<InstanceId> {
    world
        .get(object)?
        .children()
        .iter()
        .copied()
        .find(|&child| {
            world
                .get(child)
                .is_some_and(|c| c.class() == ClassName::Animator)
        })
}
