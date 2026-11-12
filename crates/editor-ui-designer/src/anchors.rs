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
    let anchor = match pin {
        Pin::Start => [0.0, 0.0],
        Pin::Center => [0.5, 0.5],
        Pin::End => [1.0, 1.0],
        Pin::Stretch => [0.0, 1.0],
    };
    anchor_axis(anchor, start, length, parent)
}

/// Where an element is anchored on one axis, as fractions of its parent:
/// where its start and its end are pinned (Unity's anchor min and max).
/// Equal for a pinned element; apart for one that stretches.
pub fn anchor_of(position: UDim, size: UDim) -> [f32; 2] {
    [position.scale, position.scale + size.scale]
}

/// `Position` and `Size` on one axis anchored at `anchor` (see
/// [`anchor_of`]), with the element staying where it is. As
/// [`pin_axis`], in design pixels.
pub fn anchor_axis(anchor: [f32; 2], start: f32, length: f32, parent: f32) -> (UDim, UDim) {
    let [min, max] = anchor.map(tidy);
    let stretch = tidy(max - min);
    (
        UDim::new(min, tidy(start - min * parent)),
        UDim::new(stretch, tidy(length - stretch * parent)),
    )
}

/// Which ends of an axis's anchor a drag moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ends {
    Min,
    Max,
    /// Both together: a joined anchor moved as one.
    Both,
}

/// `anchor` with `ends` moved to `to` (kept within the parent). An end
/// dragged past the other pushes it along, so the start never passes the
/// end.
pub fn move_ends(anchor: [f32; 2], ends: Ends, to: f32) -> [f32; 2] {
    let to = to.clamp(0.0, 1.0);
    let [min, max] = anchor;
    match ends {
        Ends::Min => [to, max.max(to)],
        Ends::Max => [min.min(to), to],
        Ends::Both => [to, to],
    }
}

/// Where a dragged anchor end at `value` settles: on the nearest of
/// `targets` within `within`, else at a whole percent. Returns it, and
/// the target it snapped to.
pub fn snap_anchor(value: f32, targets: &[f32], within: f32) -> (f32, Option<f32>) {
    let nearest = targets
        .iter()
        .copied()
        .filter(|target| (target - value).abs() <= within)
        .min_by(|a, b| (a - value).abs().total_cmp(&(b - value).abs()));
    match nearest {
        Some(target) => (target, Some(target)),
        None => (tidy(value), None),
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
    fn any_anchor_keeps_the_element_in_place_and_reads_back() {
        let near = |a: f32, b: f32| (a - b).abs() < 0.01;
        for anchor in [[0.0, 0.0], [0.25, 0.25], [0.3, 0.7], [1.0, 1.0], [0.0, 1.0]] {
            let (position, size) = anchor_axis(anchor, 600.0, 100.0, 1000.0);
            let (start, length) = place(position, size, 1000.0);
            assert!(near(start, 600.0) && near(length, 100.0), "{anchor:?}");
            let [min, max] = anchor_of(position, size);
            assert!(near(min, anchor[0]) && near(max, anchor[1]), "{anchor:?}");
        }
        // Stretched from 30% to 70%: on a parent twice as long it keeps
        // its distances from those points, so it grows with the parent.
        let (position, size) = anchor_axis([0.3, 0.7], 250.0, 500.0, 1000.0);
        let (start, length) = place(position, size, 2000.0);
        assert!(
            near(start, 550.0) && near(length, 900.0),
            "{start}, {length}"
        );
    }

    #[test]
    fn a_dragged_end_pushes_the_other_and_snaps_to_its_targets() {
        assert_eq!(move_ends([0.0, 0.0], Ends::Max, 1.0), [0.0, 1.0]);
        assert_eq!(move_ends([0.2, 0.4], Ends::Min, 0.9), [0.9, 0.9]);
        assert_eq!(move_ends([0.6, 0.8], Ends::Max, 0.1), [0.1, 0.1]);
        assert_eq!(move_ends([0.0, 0.0], Ends::Both, 1.4), [1.0, 1.0]);

        let targets = [0.0, 0.5, 1.0];
        assert_eq!(snap_anchor(0.97, &targets, 0.05), (1.0, Some(1.0)));
        assert_eq!(snap_anchor(0.52, &targets, 0.05), (0.5, Some(0.5)));
        assert_eq!(snap_anchor(0.333, &targets, 0.05), (0.33, None));
        assert_eq!(snap_anchor(0.97, &targets, 0.0), (0.97, None));
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
