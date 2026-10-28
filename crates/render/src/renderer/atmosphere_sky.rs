//! What the sky looks like this frame (`SkyLook`): the gradient's colors,
//! dimmed at night, or an atmosphere's sky worked out on the CPU
//! (`crate::atmosphere`) only when the air or the sun and moon move.
//!
//! Above Low reflection quality, an atmosphere's sky is a sky image (the
//! environment maps bake and draw it, `environment.rs`), with the sun or
//! moon and the stars drawn over it. At Low, its colors straight up, along
//! the horizon and on the ground stand in for the gradient's.

use std::f32::consts::PI;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use glam::Vec3;

use crate::atmosphere::{Air, Atmosphere};
use crate::draw_list::{Sky, SkyImage};

use super::lighting::linear_color;

/// An atmosphere's sky image size: 1.4 degrees a pixel.
pub(super) const SKY_WIDTH: u32 = 256;
pub(super) const SKY_HEIGHT: u32 = 128;
/// How finely the sun's and moon's directions are told apart before the
/// sky is worked out again: about a tenth of a degree.
const DIRECTION_STEPS: f32 = 512.0;

/// The sky drawn and lit this frame.
#[derive(Clone, Debug)]
pub(super) struct SkyLook {
    /// The gradient's top, horizon and ground colors, in linear light.
    pub colors: [Vec3; 3],
    /// The sky image drawn and reflected in place of the colors, while the
    /// environment maps are on.
    pub image: Option<SkyImage>,
    /// True when `image` is an atmosphere's, which has no sun, moon or
    /// stars of its own.
    pub scattered: bool,
}

/// Keeps an atmosphere's air and sky between frames.
#[derive(Default)]
pub(super) struct AtmosphereSky {
    /// The transmittance table for the air last drawn.
    air: Option<Air>,
    /// The sky last worked out: its key, the image (when one was wanted)
    /// and its colors.
    sky: Option<(u64, Option<SkyImage>, [Vec3; 3])>,
    /// How many times a sky has been worked out, for tests.
    pub(super) skies: u32,
}

impl AtmosphereSky {
    /// What `sky` looks like, with an image for the environment maps when
    /// `maps` (the reflection quality is above Low).
    pub(super) fn look(&mut self, sky: &Sky, maps: bool) -> SkyLook {
        let ground = linear_color(sky.ground_color);
        let Some(atmosphere) = &sky.atmosphere else {
            self.air = None;
            self.sky = None;
            let daylight = if sky.daylight.is_finite() {
                sky.daylight.max(0.0)
            } else {
                1.0
            };
            let colors = [sky.top_color, sky.horizon_color, sky.ground_color]
                .map(|color| linear_color(color) * daylight);
            return SkyLook {
                colors,
                image: sky.image.clone(),
                scattered: false,
            };
        };
        // A sky image is drawn in place of the atmosphere's.
        let wants_image = maps && sky.image.is_none();
        let key = sky_key(atmosphere, ground, wants_image);
        if self.sky.as_ref().map(|(made, ..)| *made) != Some(key) {
            let medium = atmosphere.medium();
            if self.air.as_ref().map(Air::medium) != Some(medium) {
                self.air = Some(Air::new(medium));
            }
            let air = self.air.as_ref().expect("made above");
            let colors = sky_colors(air, atmosphere, ground);
            let image = wants_image.then(|| SkyImage {
                key,
                width: SKY_WIDTH,
                height: SKY_HEIGHT,
                rgb: Arc::new(sky_pixels(air, atmosphere, colors[2])),
            });
            self.sky = Some((key, image, colors));
            self.skies += 1;
        }
        let (_, image, colors) = self.sky.as_ref().expect("made above");
        match &sky.image {
            Some(own) => SkyLook {
                colors: *colors,
                image: Some(own.clone()),
                scattered: false,
            },
            None => SkyLook {
                colors: *colors,
                image: image.clone(),
                scattered: image.is_some(),
            },
        }
    }
}

/// Different for every sky an atmosphere draws (and every sky image's key,
/// almost surely): its air, the sun and moon to about a tenth of a degree,
/// the ground's color, and whether an image is made.
fn sky_key(atmosphere: &Atmosphere, ground: Vec3, image: bool) -> u64 {
    let mut hasher = DefaultHasher::new();
    "atmosphere sky".hash(&mut hasher);
    for direction in [atmosphere.sun_direction, atmosphere.moon_direction] {
        let direction = direction.normalize_or(Vec3::Y) * DIRECTION_STEPS;
        direction
            .to_array()
            .map(|c| c.round() as i32)
            .hash(&mut hasher);
    }
    let numbers = [
        atmosphere.sun_intensity,
        atmosphere.moon_brightness,
        atmosphere.density,
        atmosphere.altitude,
        atmosphere.glare,
        atmosphere.haze,
    ];
    let colors = [atmosphere.tint, atmosphere.decay, ground.to_array()];
    for number in numbers.into_iter().chain(colors.into_iter().flatten()) {
        number.to_bits().hash(&mut hasher);
    }
    image.hash(&mut hasher);
    hasher.finish()
}

/// The atmosphere's colors straight up, around the horizon (averaged) and
/// on the ground, for the gradient's place.
fn sky_colors(air: &Air, atmosphere: &Atmosphere, ground_color: Vec3) -> [Vec3; 3] {
    let top = air.radiance(atmosphere, Vec3::Y);
    let around: Vec<Vec3> = (0..8)
        .map(|i| {
            let angle = i as f32 / 8.0 * 2.0 * PI;
            Vec3::new(angle.cos(), 0.05, angle.sin()).normalize()
        })
        .collect();
    let horizon = around
        .iter()
        .map(|&dir| air.radiance(atmosphere, dir))
        .sum::<Vec3>()
        / around.len() as f32;
    // The sky's light on the ground, about: half from straight up and half
    // from low around it, over the whole sky (pi).
    let sky_light = PI * 0.5 * (top + horizon);
    let mut sunlight = Vec3::ZERO;
    for (toward, strength) in [
        (atmosphere.sun_direction, 1.0),
        (atmosphere.moon_direction, atmosphere.moon_brightness),
    ] {
        let toward = toward.normalize_or(Vec3::Y);
        sunlight +=
            atmosphere.sunlight(toward) * toward.y.max(0.0) * atmosphere.sun_intensity * strength;
    }
    let ground = ground_color * (sky_light + sunlight) / PI;
    [top, horizon, ground]
}

/// The atmosphere's sky as a `SKY_WIDTH` × `SKY_HEIGHT` image's linear rgb
/// (see `SkyImage`). Below the horizon it fades into `ground` as the
/// gradient does.
fn sky_pixels(air: &Air, atmosphere: &Atmosphere, ground: Vec3) -> Vec<f32> {
    let (width, height) = (SKY_WIDTH, SKY_HEIGHT);
    let mut rgb = vec![0.0; (width * height * 3) as usize];
    let mut set = |x: u32, y: u32, color: Vec3| {
        let at = ((y * width + x) * 3) as usize;
        rgb[at..at + 3].copy_from_slice(&color.to_array());
    };
    for x in 0..width {
        let level = super::environment::pixel_direction(x, height / 2, width, height);
        let along_horizon = air.radiance(atmosphere, Vec3::new(level.x, 0.0, level.z).normalize());
        for y in 0..height {
            let dir = super::environment::pixel_direction(x, y, width, height);
            let color = if dir.y >= 0.0 {
                air.radiance(atmosphere, dir)
            } else {
                let t = (-dir.y / 0.2).clamp(0.0, 1.0);
                along_horizon.lerp(ground, t * t * (3.0 - 2.0 * t))
            };
            set(x, y, color);
        }
    }
    rgb
}

#[cfg(test)]
mod tests {
    use super::*;

    fn atmosphere_sky(sun: Vec3) -> Sky {
        Sky {
            atmosphere: Some(Atmosphere {
                sun_direction: sun,
                moon_direction: -sun,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn without_an_atmosphere_the_colors_are_the_skys_dimmed_by_daylight() {
        let mut sky = AtmosphereSky::default();
        let day = Sky::default();
        let look = sky.look(&day, true);
        assert_eq!(look.colors[0], linear_color(day.top_color));
        assert!(look.image.is_none() && !look.scattered);
        let night = Sky {
            daylight: 0.25,
            ..Default::default()
        };
        assert_eq!(
            sky.look(&night, true).colors[1],
            linear_color(day.horizon_color) * 0.25
        );
        assert_eq!(sky.skies, 0);
    }

    #[test]
    fn an_atmosphere_is_worked_out_once_until_the_sun_moves() {
        let mut sky = AtmosphereSky::default();
        let noon = atmosphere_sky(Vec3::new(0.2, 1.0, 0.1));
        let look = sky.look(&noon, true);
        let image = look.image.expect("an image for the environment maps");
        assert!(look.scattered);
        assert_eq!(image.rgb.len(), (SKY_WIDTH * SKY_HEIGHT * 3) as usize);
        for _ in 0..5 {
            assert_eq!(sky.look(&noon, true).image.unwrap().key, image.key);
        }
        assert_eq!(sky.skies, 1);
        // A hundredth of a degree is the same sky; a degree isn't.
        let nudged = atmosphere_sky(Vec3::new(0.2002, 1.0, 0.1));
        sky.look(&nudged, true);
        assert_eq!(sky.skies, 1);
        let moved = atmosphere_sky(Vec3::new(0.22, 1.0, 0.1));
        assert_ne!(sky.look(&moved, true).image.unwrap().key, image.key);
        assert_eq!(sky.skies, 2);
    }

    #[test]
    fn at_low_an_atmosphere_gives_colors_and_no_image() {
        let mut sky = AtmosphereSky::default();
        let look = sky.look(&atmosphere_sky(Vec3::new(0.2, 1.0, 0.1)), false);
        assert!(look.image.is_none() && !look.scattered);
        let [top, horizon, ground] = look.colors;
        assert!(top.z > top.x, "a blue sky: {top}");
        assert!(horizon.length() > 0.0 && ground.length() > 0.0);
    }

    #[test]
    fn a_sky_image_is_drawn_in_place_of_an_atmosphere() {
        let mut sky = AtmosphereSky::default();
        let own = SkyImage {
            key: 7,
            width: 2,
            height: 1,
            rgb: Arc::new(vec![1.0; 6]),
        };
        let mut both = atmosphere_sky(Vec3::Y);
        both.image = Some(own);
        let look = sky.look(&both, true);
        assert_eq!(look.image.unwrap().key, 7);
        assert!(!look.scattered);
    }

    #[test]
    fn below_the_horizon_the_image_fades_into_the_ground() {
        let mut sky = AtmosphereSky::default();
        let look = sky.look(&atmosphere_sky(Vec3::new(0.2, 1.0, 0.1)), true);
        let image = look.image.unwrap();
        let pixel = |x: u32, y: u32| {
            let at = ((y * SKY_WIDTH + x) * 3) as usize;
            Vec3::from_slice(&image.rgb[at..at + 3])
        };
        let bottom = pixel(10, SKY_HEIGHT - 1);
        assert!((bottom - look.colors[2]).abs().max_element() < 1e-5);
        let top = pixel(10, 0);
        assert!(top.z > top.x);
    }
}
