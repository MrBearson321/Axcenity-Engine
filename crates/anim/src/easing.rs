//! Easing: how far along a key's motion is, `0` to `1`, at a moment `0` to
//! `1` of the way to the next key. Clip keys use these, and so do tweens.
//!
//! Each style is written as its "In" curve; Out is In played backwards and
//! upside down, and InOut is In for the first half and Out for the second.
//! Back and Elastic go past `0` and `1` on the way.

use serde::{Deserialize, Serialize};

/// The shape of an eased key's motion.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EasingStyle {
    Sine,
    #[default]
    Quad,
    Cubic,
    Back,
    Bounce,
    Elastic,
}

impl EasingStyle {
    pub const ALL: [EasingStyle; 6] = [
        EasingStyle::Sine,
        EasingStyle::Quad,
        EasingStyle::Cubic,
        EasingStyle::Back,
        EasingStyle::Bounce,
        EasingStyle::Elastic,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            EasingStyle::Sine => "Sine",
            EasingStyle::Quad => "Quad",
            EasingStyle::Cubic => "Cubic",
            EasingStyle::Back => "Back",
            EasingStyle::Bounce => "Bounce",
            EasingStyle::Elastic => "Elastic",
        }
    }

    /// The "In" curve: slow at the start.
    fn ease_in(self, t: f32) -> f32 {
        use std::f32::consts::PI;
        match self {
            EasingStyle::Sine => 1.0 - (t * PI / 2.0).cos(),
            EasingStyle::Quad => t * t,
            EasingStyle::Cubic => t * t * t,
            EasingStyle::Back => {
                const C1: f32 = 1.70158;
                (C1 + 1.0) * t * t * t - C1 * t * t
            }
            EasingStyle::Bounce => 1.0 - bounce_out(1.0 - t),
            EasingStyle::Elastic => {
                if t <= 0.0 || t >= 1.0 {
                    t
                } else {
                    let c4 = 2.0 * PI / 3.0;
                    -(2.0_f32.powf(10.0 * t - 10.0)) * ((t * 10.0 - 10.75) * c4).sin()
                }
            }
        }
    }
}

/// Which end of the motion is eased.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EasingDirection {
    In,
    Out,
    #[default]
    InOut,
}

impl EasingDirection {
    pub const ALL: [EasingDirection; 3] = [
        EasingDirection::In,
        EasingDirection::Out,
        EasingDirection::InOut,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            EasingDirection::In => "In",
            EasingDirection::Out => "Out",
            EasingDirection::InOut => "InOut",
        }
    }
}

/// A style and a direction, as an eased key stores them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Easing {
    pub style: EasingStyle,
    pub direction: EasingDirection,
}

impl Easing {
    pub fn new(style: EasingStyle, direction: EasingDirection) -> Self {
        Self { style, direction }
    }

    /// How far along the motion is at `t` (clamped to `0..=1`).
    pub fn apply(self, t: f32) -> f32 {
        ease(self.style, self.direction, t)
    }
}

/// How far along `style` eased in `direction` is at `t` (clamped to
/// `0..=1`): `0` at the start and `1` at the end.
pub fn ease(style: EasingStyle, direction: EasingDirection, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match direction {
        EasingDirection::In => style.ease_in(t),
        EasingDirection::Out => 1.0 - style.ease_in(1.0 - t),
        EasingDirection::InOut => {
            if t < 0.5 {
                style.ease_in(2.0 * t) / 2.0
            } else {
                1.0 - style.ease_in(2.0 - 2.0 * t) / 2.0
            }
        }
    }
}

fn bounce_out(t: f32) -> f32 {
    const N1: f32 = 7.5625;
    const D1: f32 = 2.75;
    if t < 1.0 / D1 {
        N1 * t * t
    } else if t < 2.0 / D1 {
        let t = t - 1.5 / D1;
        N1 * t * t + 0.75
    } else if t < 2.5 / D1 {
        let t = t - 2.25 / D1;
        N1 * t * t + 0.9375
    } else {
        let t = t - 2.625 / D1;
        N1 * t * t + 0.984375
    }
}

/// The handles a Bezier key starts with: slow out of the key and into the
/// next.
pub const DEFAULT_BEZIER: [f32; 4] = [0.42, 0.0, 0.58, 1.0];

/// A Bezier key's motion at `t` (clamped to `0..=1`), from its handles
/// `[x1, y1, x2, y2]` as CSS's `cubic-bezier` has them: the curve runs from
/// `(0, 0)` to `(1, 1)`, `x` is time and `y` how far along. `x1` and `x2`
/// are clamped to `0..=1` so the curve never goes back in time.
pub fn cubic_bezier(handles: [f32; 4], t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let [x1, y1, x2, y2] = handles;
    let (x1, x2) = (x1.clamp(0.0, 1.0), x2.clamp(0.0, 1.0));
    // One coordinate of the curve at parameter `s`, and its slope.
    let at = |a: f32, b: f32, s: f32| {
        let u = 1.0 - s;
        3.0 * u * u * s * a + 3.0 * u * s * s * b + s * s * s
    };
    let slope = |a: f32, b: f32, s: f32| {
        let u = 1.0 - s;
        3.0 * u * u * a + 6.0 * u * s * (b - a) + 3.0 * s * s * (1.0 - b)
    };
    // Find the parameter whose x is `t`: Newton's method, then halving if
    // it hasn't settled (a flat slope).
    let mut s = t;
    for _ in 0..8 {
        let error = at(x1, x2, s) - t;
        if error.abs() < 1e-6 {
            return at(y1, y2, s);
        }
        let d = slope(x1, x2, s);
        if d.abs() < 1e-6 {
            break;
        }
        s = (s - error / d).clamp(0.0, 1.0);
    }
    let (mut low, mut high) = (0.0_f32, 1.0_f32);
    s = t;
    for _ in 0..40 {
        let x = at(x1, x2, s);
        if (x - t).abs() < 1e-6 {
            break;
        }
        if x < t {
            low = s;
        } else {
            high = s;
        }
        s = (low + high) / 2.0;
    }
    at(y1, y2, s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_easing_starts_at_0_and_ends_at_1() {
        for style in EasingStyle::ALL {
            for direction in EasingDirection::ALL {
                let start = ease(style, direction, 0.0);
                let end = ease(style, direction, 1.0);
                assert!(
                    start.abs() < 1e-4,
                    "{style:?} {direction:?} starts at {start}"
                );
                assert!(
                    (end - 1.0).abs() < 1e-4,
                    "{style:?} {direction:?} ends at {end}"
                );
            }
        }
    }

    #[test]
    fn easings_have_their_known_values() {
        let close = |a: f32, b: f32| (a - b).abs() < 1e-4;
        assert!(close(
            ease(EasingStyle::Quad, EasingDirection::In, 0.5),
            0.25
        ));
        assert!(close(
            ease(EasingStyle::Quad, EasingDirection::Out, 0.5),
            0.75
        ));
        assert!(close(
            ease(EasingStyle::Cubic, EasingDirection::InOut, 0.25),
            0.0625
        ));
        assert!(close(
            ease(EasingStyle::Sine, EasingDirection::InOut, 0.5),
            0.5
        ));
        assert!(close(
            ease(EasingStyle::Bounce, EasingDirection::Out, 0.5),
            0.765625
        ));
        // Back pulls back before it goes; Elastic overshoots.
        assert!(ease(EasingStyle::Back, EasingDirection::In, 0.2) < 0.0);
        assert!(
            (0..100)
                .map(|i| ease(EasingStyle::Elastic, EasingDirection::Out, i as f32 / 100.0))
                .any(|v| v > 1.0)
        );
    }

    #[test]
    fn bezier_handles_shape_the_motion() {
        let close = |a: f32, b: f32| (a - b).abs() < 1e-3;
        // Handles on the straight line are linear.
        let line = [1.0 / 3.0, 1.0 / 3.0, 2.0 / 3.0, 2.0 / 3.0];
        for t in [0.0, 0.1, 0.5, 0.9, 1.0] {
            assert!(close(cubic_bezier(line, t), t));
        }
        // CSS's ease-in-out is symmetric and slow at both ends.
        assert!(close(cubic_bezier(DEFAULT_BEZIER, 0.5), 0.5));
        assert!(cubic_bezier(DEFAULT_BEZIER, 0.1) < 0.05);
        // Handles above 1 overshoot.
        assert!(cubic_bezier([0.3, 1.6, 0.7, 1.6], 0.5) > 1.0);
        assert!(close(cubic_bezier([0.3, 1.6, 0.7, 1.6], 1.0), 1.0));
    }
}
