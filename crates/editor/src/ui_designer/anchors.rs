//! Anchors: where an element is pinned in its parent, so it keeps its place
//! on any screen. Like Unity's anchor presets, but written as the element's
//! ordinary `Position` and `Size` (a `UDim2`'s scale is the anchor, its
//! offset the distance from it), so nothing new is saved and the player
//! needs nothing extra.
//!
//! On each axis an element is pinned to the start (left or top), the centre
//! or the end (right or bottom) of its parent, keeping its size, or
//! stretched between both edges, keeping its margins. Pinning keeps the
//! element where it is at the size it's being designed at.

use axcenity_core::{UDim, UDim2};

/// Where an element is pinned on one axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pin {
    Start,
    Center,
    End,
    /// Both edges: it grows and shrinks with its parent.
    Stretch,
}

impl Pin {
    pub const ALL: [Pin; 4] = [Pin::Start, Pin::Center, Pin::End, Pin::Stretch];

    /// The pin a `position` and `size` on one axis make, if they make one.
    pub fn of(position: UDim, size: UDim) -> Option<Pin> {
        if size.scale == 1.0 && position.scale == 0.0 {
            return Some(Pin::Stretch);
        }
        if size.scale != 0.0 {
            return None;
        }
        match position.scale {
            0.0 => Some(Pin::Start),
            0.5 => Some(Pin::Center),
            1.0 => Some(Pin::End),
            _ => None,
        }
    }

    /// Its name on an axis: `true` for the horizontal one.
    pub fn name(self, horizontal: bool) -> &'static str {
        match (self, horizontal) {
            (Pin::Start, true) => "left",
            (Pin::Start, false) => "top",
            (Pin::Center, _) => "centre",
            (Pin::End, true) => "right",
            (Pin::End, false) => "bottom",
            (Pin::Stretch, true) => "left and right",
            (Pin::Stretch, false) => "top and bottom",
        }
    }
}

/// Tidy to hundredths, so pinning doesn't write long fractions.
fn tidy(value: f32) -> f32 {
    (value * 100.0).round() / 100.0 + 0.0
}

/// `Position` and `Size` on one axis pinned by `pin`, with the element
/// staying where it is: `start` and `length` are where it is in its
/// parent, and `parent` the parent's length, all in design pixels.
pub fn pin_axis(pin: Pin, start: f32, length: f32, parent: f32) -> (UDim, UDim) {
    let fixed = UDim::new(0.0, tidy(length));
    match pin {
        Pin::Start => (UDim::new(0.0, tidy(start)), fixed),
        Pin::Center => (UDim::new(0.5, tidy(start - parent * 0.5)), fixed),
        Pin::End => (UDim::new(1.0, tidy(start - parent)), fixed),
        Pin::Stretch => (
            UDim::new(0.0, tidy(start)),
            UDim::new(1.0, tidy(length - parent)),
        ),
    }
}

/// Where an element is in its parent, in design pixels: its left and top
/// from the parent's, its size, and the parent's size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub start: [f32; 2],
    pub size: [f32; 2],
    pub parent: [f32; 2],
}

/// `Position` and `Size` that pin an element placed at `placement` by `x`
/// and `y` (an axis left `None` keeps what `position` and `size` say).
pub fn pin(
    placement: Placement,
    position: UDim2,
    size: UDim2,
    x: Option<Pin>,
    y: Option<Pin>,
) -> (UDim2, UDim2) {
    let (mut position, mut size) = (position, size);
    if let Some(x) = x {
        (position.x, size.x) = pin_axis(
            x,
            placement.start[0],
            placement.size[0],
            placement.parent[0],
        );
    }
    if let Some(y) = y {
        (position.y, size.y) = pin_axis(
            y,
            placement.start[1],
            placement.size[1],
            placement.parent[1],
        );
    }
    (position, size)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Where `position` and `size` put an element on one axis of a parent
    /// `parent` long, as the layout does.
    fn place(position: UDim, size: UDim, parent: f32) -> (f32, f32) {
        (
            position.scale * parent + position.offset,
            size.scale * parent + size.offset,
        )
    }

    #[test]
    fn pinning_keeps_an_element_in_place_and_says_which_pin_it_is() {
        // 100 long, 600 from the start of a 1000 parent.
        for pin in Pin::ALL {
            let (position, size) = pin_axis(pin, 600.0, 100.0, 1000.0);
            assert_eq!(place(position, size, 1000.0), (600.0, 100.0), "{pin:?}");
            assert_eq!(Pin::of(position, size), Some(pin));
        }
        assert_eq!(Pin::of(UDim::new(0.25, 0.0), UDim::new(0.0, 10.0)), None);
        assert_eq!(Pin::of(UDim::new(0.0, 0.0), UDim::new(0.5, 0.0)), None);
    }

    #[test]
    fn on_a_wider_parent_each_pin_keeps_its_own_distance() {
        // On a parent 400 wider: the start pin keeps 600 from the left, the
        // end pin 300 from the right, the centre pin stays 100 right of
        // centre, and a stretched one keeps both margins.
        let wider = 1400.0;
        let at = |pin| {
            let (position, size) = pin_axis(pin, 600.0, 100.0, 1000.0);
            place(position, size, wider)
        };
        assert_eq!(at(Pin::Start), (600.0, 100.0));
        assert_eq!(at(Pin::End), (wider - 300.0 - 100.0, 100.0));
        assert_eq!(at(Pin::Center), (wider * 0.5 + 100.0, 100.0));
        assert_eq!(at(Pin::Stretch), (600.0, wider - 600.0 - 300.0));
    }

    #[test]
    fn pinning_one_axis_leaves_the_other_alone() {
        let position = UDim2::new(0.3, 5.0, 0.0, 40.0);
        let size = UDim2::new(0.0, 80.0, 0.0, 20.0);
        let placement = Placement {
            start: [100.0, 40.0],
            size: [80.0, 20.0],
            parent: [400.0, 300.0],
        };
        let (new_position, new_size) = pin(placement, position, size, None, Some(Pin::End));
        assert_eq!(new_position.x, position.x);
        assert_eq!(new_size.x, size.x);
        assert_eq!(new_position.y, UDim::new(1.0, -260.0));
        assert_eq!(new_size.y, UDim::new(0.0, 20.0));
    }
}
