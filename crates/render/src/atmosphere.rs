//! Light scattered by the air: the sky an `Atmosphere` draws in place of the
//! gradient, and the color it gives the sun's light.
//!
//! A physically based model of Earth's air (after Hillaire, "A Scalable and
//! Production Ready Sky and Atmosphere Rendering Technique", 2020): air
//! molecules scatter blue light more than red (Rayleigh), dust and haze
//! scatter every color alike and mostly forward (Mie), and ozone absorbs a
//! little orange. Light from the sun that reaches a point of the air is
//! looked up in a precomputed transmittance table ([`Air`]); the sky along
//! each direction is the sun's and moon's light scattered toward the viewer
//! along that ray. It runs on the CPU, only when the sky changes, and the
//! renderer draws the result as a sky image (`renderer/atmosphere_sky.rs`).
//!
//! Distances are in kilometres, and the geometry is in `f64`: a point on
//! the ground is 6360 km from the planet's middle, which `f32` can't place
//! to the metre.

use glam::Vec3;

/// The planet's radius and the top of its air, in km.
const GROUND_RADIUS: f64 = 6360.0;
const TOP_RADIUS: f64 = 6460.0;
/// How strongly air molecules scatter red, green and blue, per km at the
/// ground, and the height over which their density falls by e, in km.
const RAYLEIGH_SCATTERING: [f64; 3] = [5.802e-3, 13.558e-3, 33.1e-3];
const RAYLEIGH_HEIGHT: f64 = 8.0;
/// How strongly dust and haze scatter and absorb (every color alike), per
/// km at the ground, and the height over which they thin by e.
const MIE_SCATTERING: f64 = 3.996e-3;
const MIE_ABSORPTION: f64 = 0.444e-3;
const MIE_HEIGHT: f64 = 1.2;
/// How much more of the light dust scatters forward than back.
const MIE_FORWARD: f64 = 0.8;
/// How strongly ozone absorbs red, green and blue at its thickest, 25 km up,
/// thinning to nothing 15 km above and below.
const OZONE_ABSORPTION: [f64; 3] = [0.650e-3, 1.881e-3, 0.085e-3];
/// The height of the air above which the sky is seen with `altitude` 1, in km.
const VIEWER_HEIGHT: f64 = RAYLEIGH_HEIGHT;
/// The lowest the viewer stands, in km (2 m), so rays along the horizon
/// don't graze the ground.
const LOWEST_VIEWER: f64 = 0.002;
/// Steps along each ray through the air.
const TRANSMITTANCE_STEPS: u32 = 40;
const SKY_STEPS: u32 = 16;
/// The transmittance table's size: heights, and angles from straight up.
const TABLE_HEIGHTS: usize = 32;
const TABLE_ANGLES: usize = 256;
/// Scales the scattered light to the scene's: a noon sky about as bright
/// as the default gradient's. Light scattered more than once (which the
/// model leaves out) is part of it.
const SKY_BRIGHTNESS: f32 = 4.0;

/// The air an `Atmosphere` draws the sky with, and where the sun and moon
/// are in it. A plain description: the renderer works out the sky from it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Atmosphere {
    /// Toward the sun and the moon. They don't need to be normalized.
    pub sun_direction: Vec3,
    pub moon_direction: Vec3,
    /// How bright the sun is (as `Sun::intensity`), and the moon's light as
    /// a share of the sun's.
    pub sun_intensity: f32,
    pub moon_brightness: f32,
    /// How thick the air is: 1 is Earth's, 0 none.
    pub density: f32,
    /// How high in the air the sky is seen from, from 0 (the ground) to 1
    /// (one height of the air up, where the sky above darkens).
    pub altitude: f32,
    /// Multiplies the whole sky's light (linear red, green and blue: 1 leaves
    /// it as it is).
    pub tint: [f32; 3],
    /// Multiplies the sky's light away from the sun, fading to no change
    /// toward it.
    pub decay: [f32; 3],
    /// Brightens the glow around the sun: 0 as the model has it.
    pub glare: f32,
    /// Extra dust and haze: 0 as Earth's air has it, 1 twice as much.
    pub haze: f32,
}

impl Default for Atmosphere {
    fn default() -> Self {
        Self {
            sun_direction: Vec3::new(0.5, 1.0, 0.25),
            moon_direction: Vec3::new(-0.5, -1.0, -0.25),
            sun_intensity: 3.0,
            moon_brightness: 0.08,
            density: 1.0,
            altitude: 0.0,
            tint: [1.0; 3],
            decay: [1.0; 3],
            glare: 0.0,
            haze: 0.0,
        }
    }
}

impl Atmosphere {
    /// The share of a light's red, green and blue that crosses the air to
    /// the viewer from `direction`, scaled so a light straight overhead
    /// through Earth's air keeps its brightness (it's a little warmer).
    /// Toward 0 below the horizon.
    pub fn sunlight(&self, direction: Vec3) -> Vec3 {
        let up = direction.normalize_or(Vec3::Y).y as f64;
        let medium = self.medium();
        let through = transmittance_along(&medium, self.viewer_height(), up);
        (through * (1.0 / earth_overhead())).as_vec3()
    }

    /// The air's density and haze, as the transmittance table needs them.
    pub(crate) fn medium(&self) -> Medium {
        Medium {
            density: f64::from(self.density.max(0.0)),
            haze: f64::from(self.haze.max(0.0)),
        }
    }

    /// How high the viewer is, in km.
    fn viewer_height(&self) -> f64 {
        (f64::from(self.altitude.clamp(0.0, 1.0)) * VIEWER_HEIGHT).max(LOWEST_VIEWER)
    }
}

/// What the air is made of: how thick it is (1 is Earth's) and how much
/// extra haze it holds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Medium {
    density: f64,
    haze: f64,
}

impl Medium {
    /// Scattering by molecules, scattering by dust, and how much light is
    /// lost to everything (scattered away or absorbed), per km, `height` km
    /// above the ground.
    fn at(&self, height: f64) -> (Light, f64, Light) {
        let height = height.max(0.0);
        let molecules = (-height / RAYLEIGH_HEIGHT).exp() * self.density;
        let dust = (-height / MIE_HEIGHT).exp() * self.density * (1.0 + self.haze);
        let ozone = (1.0 - (height - 25.0).abs() / 15.0).max(0.0) * self.density;
        let rayleigh = Light(RAYLEIGH_SCATTERING) * molecules;
        let mie = MIE_SCATTERING * dust;
        let lost = rayleigh
            + Light::splat((MIE_SCATTERING + MIE_ABSORPTION) * dust)
            + Light(OZONE_ABSORPTION) * ozone;
        (rayleigh, mie, lost)
    }
}

/// Red, green and blue in `f64`, for adding up light along a ray.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Light(pub(crate) [f64; 3]);

impl Light {
    const ZERO: Light = Light([0.0; 3]);

    fn splat(value: f64) -> Self {
        Light([value; 3])
    }

    fn map(self, f: impl Fn(f64) -> f64) -> Self {
        Light(self.0.map(f))
    }

    fn zip(self, other: Light, f: impl Fn(f64, f64) -> f64) -> Self {
        Light([0, 1, 2].map(|i| f(self.0[i], other.0[i])))
    }

    pub(crate) fn as_vec3(self) -> Vec3 {
        let [r, g, b] = self.0.map(|c| c as f32);
        Vec3::new(r, g, b)
    }
}

impl std::ops::Add for Light {
    type Output = Light;
    fn add(self, other: Light) -> Light {
        self.zip(other, |a, b| a + b)
    }
}

impl std::ops::Mul for Light {
    type Output = Light;
    fn mul(self, other: Light) -> Light {
        self.zip(other, |a, b| a * b)
    }
}

impl std::ops::Mul<f64> for Light {
    type Output = Light;
    fn mul(self, by: f64) -> Light {
        self.map(|a| a * by)
    }
}

/// How far a ray from `radius` km from the planet's middle, at `up` (the
/// cosine of its angle from straight up), travels before leaving the top
/// of the air, or `None` if it hits the ground first.
fn ray_length(radius: f64, up: f64) -> Option<f64> {
    // |o + t d|² = r² is t² + 2bt + c = 0, with b = radius · up.
    let b = radius * up;
    let ground = (radius - GROUND_RADIUS) * (radius + GROUND_RADIUS);
    let to_ground = b * b - ground;
    if up < 0.0 && to_ground >= 0.0 && -b - to_ground.sqrt() > 0.0 {
        return None;
    }
    let top = (radius - TOP_RADIUS) * (radius + TOP_RADIUS);
    Some(-b + (b * b - top).max(0.0).sqrt())
}

/// The distances to sample along a ray of `length` in `steps` (more of them
/// near its start, where the air is thickest) and the length each stands for.
fn steps(length: f64, steps: u32) -> impl Iterator<Item = (f64, f64)> {
    (0..steps).map(move |i| {
        let s = (i as f64 + 0.5) / steps as f64;
        (length * s * s, length * 2.0 * s / steps as f64)
    })
}

/// The radius `t` km along a ray that starts `radius` km from the middle at
/// `up`, and the cosine from straight up there of a direction whose cosine
/// is `toward_up` at the start and `toward_ray` with the ray.
fn along(radius: f64, up: f64, t: f64, toward_up: f64, toward_ray: f64) -> (f64, f64) {
    let r = (radius * radius + t * t + 2.0 * radius * t * up).sqrt();
    (
        r,
        ((radius * toward_up + t * toward_ray) / r).clamp(-1.0, 1.0),
    )
}

/// The share of light that crosses the air from the top to a point
/// `height` km up, arriving at `up` from straight up: 0 if the ground is in
/// the way.
fn transmittance_along(medium: &Medium, height: f64, up: f64) -> Light {
    let radius = GROUND_RADIUS + height;
    let Some(length) = ray_length(radius, up) else {
        return Light::ZERO;
    };
    let mut depth = Light::ZERO;
    for (t, dt) in steps(length, TRANSMITTANCE_STEPS) {
        let (r, _) = along(radius, up, t, 0.0, 0.0);
        depth = depth + medium.at(r - GROUND_RADIUS).2 * dt;
    }
    depth.map(|d| (-d).exp())
}

/// How bright (luminance) Earth's air's transmittance is straight up from
/// the ground.
fn earth_overhead() -> f64 {
    let earth = Medium {
        density: 1.0,
        haze: 0.0,
    };
    let [r, g, b] = transmittance_along(&earth, LOWEST_VIEWER, 1.0).0;
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// How much light is scattered by molecules at `cos` from the way it was
/// going, per steradian (Rayleigh).
fn rayleigh_phase(cos: f64) -> f64 {
    3.0 / (16.0 * std::f64::consts::PI) * (1.0 + cos * cos)
}

/// How much light is scattered by dust at `cos` from the way it was going,
/// per steradian, mostly forward (Cornette-Shanks).
fn mie_phase(cos: f64) -> f64 {
    let g = MIE_FORWARD;
    let k = 3.0 / (8.0 * std::f64::consts::PI) * (1.0 - g * g) / (2.0 + g * g);
    k * (1.0 + cos * cos) / (1.0 + g * g - 2.0 * g * cos).powf(1.5)
}

/// One air's transmittance table: the share of light that crosses it from
/// the top to each height, from each angle, so the sky's rays don't march
/// to the sun from every point along them.
pub(crate) struct Air {
    medium: Medium,
    /// Heights (rows) by angles from straight up (columns).
    table: Vec<Light>,
}

impl Air {
    pub(crate) fn new(medium: Medium) -> Self {
        let mut table = Vec::with_capacity(TABLE_HEIGHTS * TABLE_ANGLES);
        for row in 0..TABLE_HEIGHTS {
            for column in 0..TABLE_ANGLES {
                table.push(transmittance_along(
                    &medium,
                    table_height(row as f64),
                    table_up(column as f64),
                ));
            }
        }
        Self { medium, table }
    }

    pub(crate) fn medium(&self) -> Medium {
        self.medium
    }

    /// The table's transmittance at `height` km and `up`, between its
    /// entries. Heights are spread out by their square root and angles
    /// gather around the horizon, where light changes fastest.
    fn transmittance(&self, height: f64, up: f64) -> Light {
        let row = ((height.max(0.0) / (TOP_RADIUS - GROUND_RADIUS)).sqrt()
            * (TABLE_HEIGHTS - 1) as f64)
            .min((TABLE_HEIGHTS - 1) as f64);
        let column = ((up.signum() * up.abs().sqrt() + 1.0) * 0.5 * (TABLE_ANGLES - 1) as f64)
            .clamp(0.0, (TABLE_ANGLES - 1) as f64);
        let (r0, c0) = (row.floor() as usize, column.floor() as usize);
        let (r1, c1) = (
            (r0 + 1).min(TABLE_HEIGHTS - 1),
            (c0 + 1).min(TABLE_ANGLES - 1),
        );
        let (tr, tc) = (row - r0 as f64, column - c0 as f64);
        let at = |r: usize, c: usize| self.table[r * TABLE_ANGLES + c];
        let low = at(r0, c0) * (1.0 - tc) + at(r0, c1) * tc;
        let high = at(r1, c0) * (1.0 - tc) + at(r1, c1) * tc;
        low * (1.0 - tr) + high * tr
    }

    /// The sky's light looking along `dir` (normalized, at or above the
    /// horizon) from `atmosphere`'s viewer, in the scene's linear light.
    pub(crate) fn radiance(&self, atmosphere: &Atmosphere, dir: Vec3) -> Vec3 {
        let height = atmosphere.viewer_height();
        let radius = GROUND_RADIUS + height;
        let up = f64::from(dir.y);
        let length = ray_length(radius, up).unwrap_or_else(|| {
            // To the ground.
            let b = radius * up;
            -b - (b * b - (radius - GROUND_RADIUS) * (radius + GROUND_RADIUS))
                .max(0.0)
                .sqrt()
        });
        let sun_scale = f64::from(atmosphere.sun_intensity.max(0.0)) / earth_overhead();
        let mut lights = Vec::with_capacity(2);
        for (toward, strength) in [
            (atmosphere.sun_direction, 1.0),
            (atmosphere.moon_direction, atmosphere.moon_brightness),
        ] {
            let toward = toward.normalize_or(Vec3::Y);
            // Lights far below the horizon don't reach the air the sky is seen through.
            if strength > 0.0 && toward.y > -0.35 {
                lights.push((toward, sun_scale * f64::from(strength)));
            }
        }
        let mut depth = Light::ZERO;
        let mut light = Light::ZERO;
        for (t, dt) in steps(length, SKY_STEPS) {
            let (r, _) = along(radius, up, t, 0.0, 0.0);
            let (rayleigh, mie, lost) = self.medium.at(r - GROUND_RADIUS);
            // To the viewer: halfway through this step's own air.
            let to_viewer = (depth + lost * (dt * 0.5)).map(|d| (-d).exp());
            depth = depth + lost * dt;
            for &(toward, strength) in &lights {
                let cos = f64::from(dir.dot(toward));
                let (_, toward_up) = along(radius, up, t, f64::from(toward.y), cos);
                let from_light = self.transmittance(r - GROUND_RADIUS, toward_up);
                let glow = 1.0 + f64::from(atmosphere.glare.max(0.0));
                let scattered =
                    rayleigh * rayleigh_phase(cos) + Light::splat(mie * mie_phase(cos) * glow);
                light = light + to_viewer * from_light * scattered * strength * dt;
            }
        }
        let away = (1.0 - dir.dot(atmosphere.sun_direction.normalize_or(Vec3::Y))) * 0.5;
        let tint = Vec3::from(atmosphere.tint).max(Vec3::ZERO);
        let decay = Vec3::ONE.lerp(Vec3::from(atmosphere.decay).max(Vec3::ZERO), away);
        light.as_vec3() * tint * decay * SKY_BRIGHTNESS
    }
}

/// The height of transmittance table row `row`, in km.
fn table_height(row: f64) -> f64 {
    let s = row / (TABLE_HEIGHTS - 1) as f64;
    s * s * (TOP_RADIUS - GROUND_RADIUS)
}

/// The cosine from straight up of transmittance table column `column`.
fn table_up(column: f64) -> f64 {
    let s = column / (TABLE_ANGLES - 1) as f64 * 2.0 - 1.0;
    s.signum() * s * s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn earth() -> Air {
        Air::new(Atmosphere::default().medium())
    }

    /// `color`'s red over its blue.
    fn redness(color: Vec3) -> f32 {
        color.x / color.z.max(1e-6)
    }

    #[test]
    fn a_sun_overhead_keeps_its_color_and_a_low_one_reddens() {
        let atmosphere = Atmosphere::default();
        let overhead = atmosphere.sunlight(Vec3::Y);
        let luminance = overhead.dot(Vec3::new(0.2126, 0.7152, 0.0722));
        assert!((luminance - 1.0).abs() < 0.01, "{overhead}");
        assert!(
            overhead.x > overhead.z && overhead.z > 0.8,
            "a little warm: {overhead}"
        );
        let low = atmosphere.sunlight(Vec3::new(1.0, 0.05, 0.0));
        assert!(redness(low) > 3.0 * redness(overhead), "{low}");
        assert!(low.x < overhead.x);
        // Under the horizon, the ground is in the way.
        assert_eq!(atmosphere.sunlight(Vec3::new(1.0, -0.2, 0.0)), Vec3::ZERO);
    }

    #[test]
    fn without_air_the_sun_is_white_from_every_height() {
        let space = Atmosphere {
            density: 0.0,
            ..Default::default()
        };
        let low = space.sunlight(Vec3::new(1.0, 0.05, 0.0));
        assert!((low.x - low.z).abs() < 1e-6, "{low}");
        let air = Air::new(space.medium());
        assert_eq!(air.radiance(&space, Vec3::Y), Vec3::ZERO);
    }

    #[test]
    fn the_table_matches_marching_to_the_sun() {
        let air = earth();
        for (height, up) in [(0.002, 1.0), (1.0, 0.3), (5.0, 0.02), (30.0, -0.05)] {
            let table = air.transmittance(height, up).as_vec3();
            let marched = transmittance_along(&air.medium(), height, up).as_vec3();
            assert!(
                (table - marched).abs().max_element() < 0.02,
                "{height} km at {up}: {table} vs {marched}"
            );
        }
    }

    #[test]
    fn a_noon_sky_is_blue_and_brightest_toward_the_horizon() {
        let air = earth();
        let atmosphere = Atmosphere {
            sun_direction: Vec3::new(0.3, 1.0, 0.0),
            ..Default::default()
        };
        let zenith = air.radiance(&atmosphere, Vec3::Y);
        let horizon = air.radiance(&atmosphere, Vec3::new(-1.0, 0.02, 0.0).normalize());
        assert!(zenith.z > zenith.y && zenith.y > zenith.x, "{zenith}");
        assert!(horizon.length() > zenith.length(), "{horizon} vs {zenith}");
        // About as bright as the default gradient's top (linear 0.05, 0.21, 0.79).
        let luminance = zenith.dot(Vec3::new(0.2126, 0.7152, 0.0722));
        assert!((0.15..0.4).contains(&luminance), "{zenith}: {luminance}");
    }

    #[test]
    fn a_sunset_sky_reddens_toward_the_sun_and_night_is_dark() {
        let air = earth();
        let noon = Atmosphere {
            sun_direction: Vec3::new(0.3, 1.0, 0.0),
            ..Default::default()
        };
        let dusk = Atmosphere {
            sun_direction: Vec3::new(1.0, 0.03, 0.0),
            ..Default::default()
        };
        let toward_sun = Vec3::new(1.0, 0.05, 0.0).normalize();
        let noon_sky = air.radiance(&noon, toward_sun);
        let dusk_sky = air.radiance(&dusk, toward_sun);
        assert!(
            redness(dusk_sky) > 2.0 * redness(noon_sky),
            "{dusk_sky} vs {noon_sky}"
        );
        let night = Atmosphere {
            sun_direction: Vec3::new(1.0, -0.5, 0.0),
            moon_direction: Vec3::new(-1.0, 0.5, 0.0),
            ..Default::default()
        };
        let night_sky = air.radiance(&night, Vec3::Y);
        assert!(night_sky.length() < 0.1 * air.radiance(&noon, Vec3::Y).length());
        assert!(night_sky.length() > 0.0, "the moon lights the sky a little");
    }

    #[test]
    fn haze_dims_the_sun_and_glare_brightens_its_glow() {
        let clear = Atmosphere::default();
        let hazy = Atmosphere {
            haze: 10.0,
            ..Default::default()
        };
        let low = Vec3::new(1.0, 0.1, 0.0);
        assert!(hazy.sunlight(low).y < clear.sunlight(low).y);
        let air = earth();
        let glare = Atmosphere {
            glare: 5.0,
            ..Default::default()
        };
        let near_sun = (clear.sun_direction.normalize() + Vec3::new(0.0, 0.0, 0.05)).normalize();
        assert!(air.radiance(&glare, near_sun).x > air.radiance(&clear, near_sun).x);
        // Tints multiply it.
        let tinted = Atmosphere {
            tint: [0.5, 1.0, 1.0],
            ..Default::default()
        };
        let plain = air.radiance(&clear, Vec3::Y);
        let half = air.radiance(&tinted, Vec3::Y);
        assert!((half.x - plain.x * 0.5).abs() < 1e-5);
    }
}
