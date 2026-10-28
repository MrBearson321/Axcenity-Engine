//! Clouds (graphics Phase 26): the frame's copy of the draw list's
//! [`Clouds`] (`CloudsRaw`, `Clouds` in clouds.wgsl), which the sky pass
//! draws and every lit surface takes the sun's light through, and how far
//! the wind has carried them (`CloudDrift`). They add no pass or target:
//! without clouds, the shaders skip them on one uniform test.

use super::lighting::linear_color;
use super::*;
use crate::CloudQuality;
use crate::draw_list::Clouds;

/// How much light the thickest cloud takes away per stud at `Density` 1.
const EXTINCTION: f32 = 0.05;

/// The clouds as the shaders read them. Must match `Clouds` in
/// clouds.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub(super) struct CloudsRaw {
    pub(super) shape: [f32; 4],
    pub(super) color: [f32; 4],
    pub(super) offset: [f32; 4],
    pub(super) quality: [f32; 4],
}

impl CloudsRaw {
    /// `clouds`, carried `offset` studs along x and z by the wind, drawn at
    /// `quality`.
    pub(super) fn new(clouds: &Clouds, offset: [f32; 2], quality: CloudQuality) -> Self {
        if !clouds.is_drawn() {
            return Self::default();
        }
        let (steps, light_steps) = quality.steps();
        let detail = quality >= CloudQuality::High;
        Self {
            shape: [
                clouds.cover.min(1.0),
                clouds.density.min(1.0) * EXTINCTION,
                Clouds::BOTTOM,
                Clouds::TOP,
            ],
            color: linear_color(clouds.color).extend(1.0).to_array(),
            offset: [offset[0], offset[1], 0.0, 0.0],
            quality: [
                steps as f32,
                light_steps as f32,
                f32::from(u8::from(detail)),
                0.0,
            ],
        }
    }
}

/// How far the wind has carried the clouds, kept from frame to frame so a
/// change of wind changes how fast they drift, not where they are.
#[derive(Debug, Default)]
pub(super) struct CloudDrift {
    offset: [f64; 2],
    /// The draw list's time at the last frame; `None` before the first.
    last_time: Option<f32>,
}

impl CloudDrift {
    /// Moves the clouds on by `clouds`' wind for the time since the last
    /// frame (`time` is [`DrawList::time`]; when it goes back, as when
    /// a host starts counting again, they stay where they are), and gives
    /// how far they've gone.
    pub(super) fn advance(&mut self, clouds: &Clouds, time: f32) -> [f32; 2] {
        if let Some(last) = self.last_time {
            let seconds = f64::from(time - last);
            if seconds > 0.0 && clouds.wind.is_finite() {
                self.offset[0] += f64::from(clouds.wind.x) * seconds;
                self.offset[1] += f64::from(clouds.wind.z) * seconds;
            }
        }
        if time.is_finite() {
            self.last_time = Some(time);
        }
        self.offset.map(|studs| studs as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_clouds_by_default_or_without_cover() {
        let raw = CloudsRaw::new(&Clouds::default(), [0.0; 2], CloudQuality::High);
        assert_eq!(raw, CloudsRaw::default());
        let thin_air = Clouds {
            cover: 0.5,
            density: 0.0,
            ..Clouds::default()
        };
        assert_eq!(
            CloudsRaw::new(&thin_air, [0.0; 2], CloudQuality::High).color[3],
            0.0
        );
    }

    #[test]
    fn quality_sets_the_steps_and_detail() {
        let clouds = Clouds {
            cover: 0.5,
            ..Clouds::default()
        };
        let quality = |quality| CloudsRaw::new(&clouds, [0.0; 2], quality).quality;
        assert_eq!(quality(CloudQuality::Low), [0.0, 0.0, 0.0, 0.0]);
        assert_eq!(quality(CloudQuality::Medium), [16.0, 0.0, 0.0, 0.0]);
        assert_eq!(quality(CloudQuality::Ultra), [64.0, 6.0, 1.0, 0.0]);
        let raw = CloudsRaw::new(&clouds, [3.0, -4.0], CloudQuality::Low);
        assert_eq!(
            raw.shape,
            [0.5, 0.7 * EXTINCTION, Clouds::BOTTOM, Clouds::TOP]
        );
        assert_eq!(raw.offset, [3.0, -4.0, 0.0, 0.0]);
        assert_eq!(raw.color, [1.0; 4]);
    }

    #[test]
    fn the_wind_carries_the_clouds_by_the_time_passed() {
        let mut clouds = Clouds {
            cover: 0.5,
            wind: Vec3::new(10.0, 5.0, -2.0),
            ..Clouds::default()
        };
        let mut drift = CloudDrift::default();
        assert_eq!(
            drift.advance(&clouds, 3.0),
            [0.0, 0.0],
            "not before a frame"
        );
        assert_eq!(drift.advance(&clouds, 5.0), [20.0, -4.0], "level wind only");
        // A new wind moves them on from where they are.
        clouds.wind = Vec3::new(-1.0, 0.0, 0.0);
        assert_eq!(drift.advance(&clouds, 9.0), [16.0, -4.0]);
        // Time counted again from 0 leaves them there.
        assert_eq!(drift.advance(&clouds, 0.0), [16.0, -4.0]);
        assert_eq!(drift.advance(&clouds, 1.0), [15.0, -4.0]);
    }
}
