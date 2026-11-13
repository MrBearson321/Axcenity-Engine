//! `NumberSequence` and `ColorSequence` (roadmap R17.1): values that change
//! over a span of time from 0 to 1, such as a particle's size over its
//! life. Each is a list of keypoints in time order, the first at 0 and the
//! last at 1, with straight lines between them, as scripts know them.
//!
//! An empty sequence is a property left unset: a ParticleEmitter's
//! `SizeOverLife` empty means `Size` blends to `SizeEnd` as before.

use std::fmt;

use crate::Color3;

/// The most keypoints a sequence holds.
pub const MAX_KEYPOINTS: usize = 20;

/// Why a list of keypoints isn't a sequence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SequenceError {
    /// Fewer than two keypoints, or more than [`MAX_KEYPOINTS`].
    Count(usize),
    /// The first keypoint isn't at time 0 or the last isn't at 1.
    Ends,
    /// A keypoint's time is before the one before it, or not a number.
    Order,
}

impl fmt::Display for SequenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SequenceError::Count(count) => write!(
                f,
                "a sequence needs 2 to {MAX_KEYPOINTS} keypoints, not {count}"
            ),
            SequenceError::Ends => {
                f.write_str("a sequence's keypoints must start at time 0 and end at time 1")
            }
            SequenceError::Order => f.write_str("a sequence's keypoints must be in time order"),
        }
    }
}

impl std::error::Error for SequenceError {}

/// Checks the times of a sequence's keypoints.
fn check(times: impl ExactSizeIterator<Item = f32>) -> Result<(), SequenceError> {
    let count = times.len();
    if !(2..=MAX_KEYPOINTS).contains(&count) {
        return Err(SequenceError::Count(count));
    }
    let times: Vec<f32> = times.collect();
    if times.iter().any(|time| !time.is_finite()) || times.windows(2).any(|pair| pair[1] < pair[0])
    {
        return Err(SequenceError::Order);
    }
    if times[0] != 0.0 || times[count - 1] != 1.0 {
        return Err(SequenceError::Ends);
    }
    Ok(())
}

/// Where `time` falls between two keypoints: the index of the one before
/// and how far on toward the next (0 to 1).
fn between(times: impl Iterator<Item = f32>, time: f32) -> (usize, f32) {
    let times: Vec<f32> = times.collect();
    let time = if time.is_nan() {
        0.0
    } else {
        time.clamp(0.0, 1.0)
    };
    for index in 0..times.len().saturating_sub(1) {
        let (start, end) = (times[index], times[index + 1]);
        if time <= end {
            let span = end - start;
            let along = if span > 0.0 {
                (time - start) / span
            } else {
                1.0
            };
            return (index, along.clamp(0.0, 1.0));
        }
    }
    (times.len().saturating_sub(2), 1.0)
}

/// One point of a [`NumberSequence`]: its `value` at `time`, give or take
/// `envelope` (each particle picks once where in that range it is).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NumberKeypoint {
    pub time: f32,
    pub value: f32,
    pub envelope: f32,
}

impl NumberKeypoint {
    pub const fn new(time: f32, value: f32, envelope: f32) -> Self {
        Self {
            time,
            value,
            envelope,
        }
    }
}

/// A number that changes over time from 0 to 1.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NumberSequence {
    keypoints: Vec<NumberKeypoint>,
}

impl NumberSequence {
    /// A sequence through `keypoints`, which must be 2 to [`MAX_KEYPOINTS`]
    /// in time order from 0 to 1. Envelopes below 0 count as 0.
    pub fn new(mut keypoints: Vec<NumberKeypoint>) -> Result<Self, SequenceError> {
        check(keypoints.iter().map(|point| point.time))?;
        for point in &mut keypoints {
            point.envelope = point.envelope.max(0.0);
        }
        Ok(Self { keypoints })
    }

    /// The same `value` throughout.
    pub fn constant(value: f32) -> Self {
        Self::ramp(value, value)
    }

    /// `from` at 0 to `to` at 1.
    pub fn ramp(from: f32, to: f32) -> Self {
        Self {
            keypoints: vec![
                NumberKeypoint::new(0.0, from, 0.0),
                NumberKeypoint::new(1.0, to, 0.0),
            ],
        }
    }

    /// No keypoints: the property it's in is unset.
    pub fn is_empty(&self) -> bool {
        self.keypoints.is_empty()
    }

    pub fn keypoints(&self) -> &[NumberKeypoint] {
        &self.keypoints
    }

    /// Each keypoint as `[time, value, envelope]`, as files and the network
    /// keep them.
    pub fn to_rows(&self) -> Vec<[f32; 3]> {
        self.keypoints
            .iter()
            .map(|point| [point.time, point.value, point.envelope])
            .collect()
    }

    /// The sequence [`to_rows`](Self::to_rows) wrote; no rows is the empty
    /// sequence.
    pub fn from_rows(rows: &[[f32; 3]]) -> Result<Self, SequenceError> {
        if rows.is_empty() {
            return Ok(Self::default());
        }
        Self::new(
            rows.iter()
                .map(|&[time, value, envelope]| NumberKeypoint::new(time, value, envelope))
                .collect(),
        )
    }

    /// The value and envelope at `time` (0 to 1), on the straight line
    /// between the keypoints around it; `None` for an empty sequence.
    pub fn sample(&self, time: f32) -> Option<(f32, f32)> {
        if self.keypoints.is_empty() {
            return None;
        }
        let (index, along) = between(self.keypoints.iter().map(|point| point.time), time);
        let (a, b) = (self.keypoints[index], self.keypoints[index + 1]);
        Some((
            a.value + (b.value - a.value) * along,
            a.envelope + (b.envelope - a.envelope) * along,
        ))
    }
}

/// One point of a [`ColorSequence`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorKeypoint {
    pub time: f32,
    pub color: Color3,
}

impl ColorKeypoint {
    pub const fn new(time: f32, color: Color3) -> Self {
        Self { time, color }
    }
}

/// A color that changes over time from 0 to 1.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ColorSequence {
    keypoints: Vec<ColorKeypoint>,
}

impl ColorSequence {
    /// A sequence through `keypoints`, which must be 2 to [`MAX_KEYPOINTS`]
    /// in time order from 0 to 1.
    pub fn new(keypoints: Vec<ColorKeypoint>) -> Result<Self, SequenceError> {
        check(keypoints.iter().map(|point| point.time))?;
        Ok(Self { keypoints })
    }

    /// `from` at 0 to `to` at 1.
    pub fn ramp(from: Color3, to: Color3) -> Self {
        Self {
            keypoints: vec![ColorKeypoint::new(0.0, from), ColorKeypoint::new(1.0, to)],
        }
    }

    /// No keypoints: the property it's in is unset.
    pub fn is_empty(&self) -> bool {
        self.keypoints.is_empty()
    }

    pub fn keypoints(&self) -> &[ColorKeypoint] {
        &self.keypoints
    }

    /// Each keypoint as `[time, r, g, b]`, as files and the network keep
    /// them.
    pub fn to_rows(&self) -> Vec<[f32; 4]> {
        self.keypoints
            .iter()
            .map(|point| [point.time, point.color.r, point.color.g, point.color.b])
            .collect()
    }

    /// The sequence [`to_rows`](Self::to_rows) wrote; no rows is the empty
    /// sequence.
    pub fn from_rows(rows: &[[f32; 4]]) -> Result<Self, SequenceError> {
        if rows.is_empty() {
            return Ok(Self::default());
        }
        Self::new(
            rows.iter()
                .map(|&[time, r, g, b]| ColorKeypoint::new(time, Color3::new(r, g, b)))
                .collect(),
        )
    }

    /// The color at `time` (0 to 1), each channel on the straight line
    /// between the keypoints around it; `None` for an empty sequence.
    pub fn sample(&self, time: f32) -> Option<Color3> {
        if self.keypoints.is_empty() {
            return None;
        }
        let (index, along) = between(self.keypoints.iter().map(|point| point.time), time);
        let (a, b) = (self.keypoints[index].color, self.keypoints[index + 1].color);
        let mix = |a: f32, b: f32| a + (b - a) * along;
        Some(Color3::new(mix(a.r, b.r), mix(a.g, b.g), mix(a.b, b.b)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sequence_runs_from_time_0_to_1_in_order() {
        let point = |time| NumberKeypoint::new(time, 1.0, 0.0);
        assert_eq!(
            NumberSequence::new(vec![point(0.0)]),
            Err(SequenceError::Count(1))
        );
        assert_eq!(
            NumberSequence::new(vec![point(0.0), point(0.5)]),
            Err(SequenceError::Ends)
        );
        assert_eq!(
            NumberSequence::new(vec![point(0.0), point(0.6), point(0.4), point(1.0)]),
            Err(SequenceError::Order)
        );
        assert_eq!(
            NumberSequence::new(vec![point(0.0), point(f32::NAN), point(1.0)]),
            Err(SequenceError::Order)
        );
        assert_eq!(
            NumberSequence::new(vec![point(0.0); MAX_KEYPOINTS + 1]),
            Err(SequenceError::Count(MAX_KEYPOINTS + 1))
        );
        let negative =
            NumberSequence::new(vec![NumberKeypoint::new(0.0, 1.0, -2.0), point(1.0)]).unwrap();
        assert_eq!(negative.keypoints()[0].envelope, 0.0);
    }

    #[test]
    fn sampling_follows_straight_lines_between_keypoints() {
        let sequence = NumberSequence::new(vec![
            NumberKeypoint::new(0.0, 0.0, 0.0),
            NumberKeypoint::new(0.25, 4.0, 2.0),
            NumberKeypoint::new(1.0, 1.0, 0.0),
        ])
        .unwrap();
        assert_eq!(sequence.sample(0.0), Some((0.0, 0.0)));
        assert_eq!(sequence.sample(0.125), Some((2.0, 1.0)));
        assert_eq!(sequence.sample(0.25), Some((4.0, 2.0)));
        assert_eq!(sequence.sample(1.0), Some((1.0, 0.0)));
        // Outside 0 to 1 holds the ends.
        assert_eq!(sequence.sample(-1.0), Some((0.0, 0.0)));
        assert_eq!(sequence.sample(9.0), Some((1.0, 0.0)));
        assert_eq!(NumberSequence::default().sample(0.5), None);
        let colors = ColorSequence::ramp(Color3::new(1.0, 0.0, 0.0), Color3::new(0.0, 0.0, 1.0));
        assert_eq!(colors.sample(0.5), Some(Color3::new(0.5, 0.0, 0.5)));
        // A step: two keypoints at the same time.
        let step = NumberSequence::new(vec![
            NumberKeypoint::new(0.0, 0.0, 0.0),
            NumberKeypoint::new(0.5, 0.0, 0.0),
            NumberKeypoint::new(0.5, 1.0, 0.0),
            NumberKeypoint::new(1.0, 1.0, 0.0),
        ])
        .unwrap();
        assert_eq!(step.sample(0.25).unwrap().0, 0.0);
        assert_eq!(step.sample(0.75).unwrap().0, 1.0);
    }
}
