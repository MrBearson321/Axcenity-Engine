//! `Rotation`: an element and everything in it turned around its middle,
//! in drawing and hit testing, but not in layout.
//!
//! An element is laid out upright, and then what it and its children draw
//! is turned. Each element keeps the turn that's on it ([`UiTurn`]), so a
//! point on the screen can be turned back into the upright box to test it.

use axcenity_render::{UiClip, UiDrawItem, is_turned, turn_point};

/// How a turned element is drawn: a point laid out at `p` is drawn at `p`
/// turned `degrees` clockwise around the screen's corner, then moved by
/// `offset`. Turns inside turns add up to one of these.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiTurn {
    pub degrees: f32,
    pub offset: [f32; 2],
}

impl UiTurn {
    /// A turn of `degrees` clockwise around `centre`.
    pub fn around(degrees: f32, centre: [f32; 2]) -> UiTurn {
        let turned = turn_point(centre, [0.0, 0.0], degrees);
        UiTurn {
            degrees,
            offset: [centre[0] - turned[0], centre[1] - turned[1]],
        }
    }

    /// Where a point laid out at `point` is drawn.
    pub fn apply(&self, point: [f32; 2]) -> [f32; 2] {
        let [x, y] = turn_point(point, [0.0, 0.0], self.degrees);
        [x + self.offset[0], y + self.offset[1]]
    }

    /// Where a point drawn at `point` was laid out: [`apply`](Self::apply) undone.
    pub fn invert(&self, point: [f32; 2]) -> [f32; 2] {
        turn_point(
            [point[0] - self.offset[0], point[1] - self.offset[1]],
            [0.0, 0.0],
            -self.degrees,
        )
    }

    /// `self`, then `outer`: an element's own turn inside its parent's.
    pub fn then(self, outer: UiTurn) -> UiTurn {
        let moved = turn_point(self.offset, [0.0, 0.0], outer.degrees);
        UiTurn {
            degrees: self.degrees + outer.degrees,
            offset: [moved[0] + outer.offset[0], moved[1] + outer.offset[1]],
        }
    }

    /// The same turn after everything is scaled by `scale` and moved by
    /// `origin` (a ScreenGui in Fit or Scale, or the artboard's zoom).
    pub(crate) fn scaled(self, scale: f32, origin: [f32; 2]) -> UiTurn {
        let turned = turn_point(origin, [0.0, 0.0], self.degrees);
        UiTurn {
            degrees: self.degrees,
            offset: [
                self.offset[0] * scale + origin[0] - turned[0],
                self.offset[1] * scale + origin[1] - turned[1],
            ],
        }
    }
}

/// An element's own `Rotation` in degrees, when it turns anything.
pub(crate) fn rotation(instance: &axcenity_core::Instance) -> Option<f32> {
    instance
        .property("Rotation")
        .and_then(axcenity_core::Value::as_number)
        .map(|degrees| degrees as f32)
        .filter(|&degrees| is_turned(degrees))
}

/// A box (top left and size) whose middle is moved by `turn`, and the
/// box's own turn added to.
fn turn_box(position: &mut [f32; 2], size: [f32; 2], degrees: &mut f32, turn: UiTurn) {
    let centre = [position[0] + size[0] * 0.5, position[1] + size[1] * 0.5];
    let [x, y] = turn.apply(centre);
    *position = [x - size[0] * 0.5, y - size[1] * 0.5];
    *degrees += turn.degrees;
}

/// `clip`, turned with the element it's in.
pub(crate) fn turn_clip(mut clip: UiClip, turn: UiTurn) -> UiClip {
    turn_box(
        &mut clip.position,
        clip.size,
        &mut clip.rotation_degrees,
        turn,
    );
    clip
}

/// A draw item and its clip, turned with the element it's in.
pub(crate) fn turn_draw_item(item: &mut UiDrawItem, turn: UiTurn) {
    let (position, size, degrees, clip) = match item {
        UiDrawItem::Rectangle(rectangle) => (
            &mut rectangle.position,
            rectangle.size,
            &mut rectangle.rotation_degrees,
            &mut rectangle.clip,
        ),
        UiDrawItem::Image(image) => (
            &mut image.position,
            image.size,
            &mut image.rotation_degrees,
            &mut image.clip,
        ),
        UiDrawItem::Text(text) => (
            &mut text.position,
            text.size,
            &mut text.rotation_degrees,
            &mut text.clip,
        ),
    };
    turn_box(position, size, degrees, turn);
    if let Some(clip) = clip {
        *clip = turn_clip(*clip, turn);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3
    }

    #[test]
    fn a_turn_keeps_its_centre_and_undoes_and_adds_up() {
        let turn = UiTurn::around(90.0, [50.0, 50.0]);
        assert!(close(turn.apply([50.0, 50.0]), [50.0, 50.0]));
        // Clockwise on a screen whose y runs down: right goes to below.
        assert!(close(turn.apply([60.0, 50.0]), [50.0, 60.0]));
        assert!(close(turn.invert(turn.apply([12.0, 34.0])), [12.0, 34.0]));
        let outer = UiTurn::around(30.0, [10.0, 0.0]);
        let both = turn.then(outer);
        let point = [70.0, 20.0];
        assert!(close(both.apply(point), outer.apply(turn.apply(point))));
        // Scaled by 2 and moved by (5, 7), it lands where the scaled point does.
        let scaled = turn.scaled(2.0, [5.0, 7.0]);
        let [x, y] = turn.apply(point);
        assert!(close(
            scaled.apply([point[0] * 2.0 + 5.0, point[1] * 2.0 + 7.0]),
            [x * 2.0 + 5.0, y * 2.0 + 7.0]
        ));
    }
}
