//! A SplineScatter's layout: which model goes where, turned and scaled how.
//!
//! Spots go along the curve every `Spacing` studs, or, with `FillInside`
//! on a Closed spline, at random inside it, `Density` per 100 square
//! studs. Each spot draws from its own generator, seeded by `Seed` and the
//! spot's number, in a fixed order: (inside, its place,) the model, the
//! turn, the scale, then the sideways offset. Then it's stood on the
//! surface below, upright or along the surface, and left out where that's
//! steeper than `MaxSlope`.

use axcenity_core::spline::SplineCurve;
use glam::{Quat, Vec2, Vec3};

use crate::bend::frame;
use crate::rng::Rng;

/// Models one scatter places, at most.
pub const MAX_MODELS: usize = 5000;
/// The smallest spacing along the curve, in studs.
const MIN_SPACING: f32 = 0.5;
/// Tries at a random place inside before a spot is given up.
const TRIES: usize = 32;

/// A SplineScatter's settings, read from its properties.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub models: Vec<String>,
    pub seed: u64,
    pub spacing: f32,
    pub fill_inside: bool,
    pub density: f32,
    pub rotation: Vec2,
    pub scale: Vec2,
    pub offset: f32,
    pub align: bool,
    pub max_slope: f32,
}

/// One placed model: its file, the middle of its box, its turn, and its
/// size (the model's own, scaled).
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub model: String,
    pub position: Vec3,
    pub rotation: Quat,
    pub size: Vec3,
}

/// The layout along (or inside) `curve`. `size_of` gives a model's own
/// size, or `None` when it can't be loaded (its spots stay empty);
/// `below` the surface under a point and which way it faces.
pub fn layout(
    curve: &SplineCurve,
    settings: &Settings,
    size_of: impl Fn(&str) -> Option<Vec3>,
    below: impl Fn(Vec3) -> Option<(Vec3, Vec3)>,
) -> Vec<Placed> {
    if settings.models.is_empty() {
        return Vec::new();
    }
    let spots = if settings.fill_inside && curve.closed() && curve.length() > 0.0 {
        inside(curve, settings)
    } else {
        along(curve, settings)
    };
    spots
        .into_iter()
        .filter_map(|(mut rng, point, right)| {
            let model = &settings.models[rng.below(settings.models.len())];
            let yaw = rng.range(settings.rotation.x, settings.rotation.y);
            let scale = rng.range(settings.scale.x, settings.scale.y).max(0.01);
            let sideways = rng.range(-settings.offset, settings.offset);
            let point = point + right * sideways;
            let (ground, up) = match below(point) {
                Some((ground, normal)) => {
                    let slope = normal.angle_between(Vec3::Y).to_degrees();
                    if slope > settings.max_slope {
                        return None;
                    }
                    (ground, if settings.align { normal } else { Vec3::Y })
                }
                None => (point, Vec3::Y),
            };
            let size = size_of(model)? * scale;
            let rotation =
                Quat::from_rotation_arc(Vec3::Y, up) * Quat::from_rotation_y(yaw.to_radians());
            Some(Placed {
                model: model.clone(),
                position: ground + up * (size.y / 2.0),
                rotation,
                size,
            })
        })
        .collect()
}

/// Spots every `spacing` studs along the curve (a closed curve's end is
/// its start, so it isn't used twice), with the curve's flat right.
fn along(curve: &SplineCurve, settings: &Settings) -> Vec<(Rng, Vec3, Vec3)> {
    let spacing = settings.spacing.max(MIN_SPACING);
    let length = curve.length();
    let steps = (length / spacing).floor() as usize;
    let count = if curve.closed() {
        steps.max(1)
    } else {
        steps + 1
    };
    (0..count.min(MAX_MODELS))
        .map(|i| {
            let (point, right, _, _) = frame(curve, i as f32 * spacing);
            let flat = Vec3::new(right.x, 0.0, right.z).normalize_or(Vec3::X);
            (Rng::new(settings.seed, i as u64), point, flat)
        })
        .collect()
}

/// Random spots inside a closed curve, seen from above, at `density` per
/// 100 square studs, at the curve's average height.
fn inside(curve: &SplineCurve, settings: &Settings) -> Vec<(Rng, Vec3, Vec3)> {
    let outline: Vec<Vec3> = curve
        .polyline(1.0)
        .into_iter()
        .map(|p| Vec3::new(p.x, p.y, p.z))
        .collect();
    let flat: Vec<Vec2> = outline.iter().map(|p| Vec2::new(p.x, p.z)).collect();
    let height = outline.iter().map(|p| p.y).sum::<f32>() / outline.len().max(1) as f32;
    let (min, max) = flat
        .iter()
        .fold((Vec2::INFINITY, Vec2::NEG_INFINITY), |(min, max), &p| {
            (min.min(p), max.max(p))
        });
    let count =
        ((area(&flat) * settings.density.max(0.0) / 100.0).round() as usize).min(MAX_MODELS);
    (0..count)
        .filter_map(|k| {
            let mut rng = Rng::new(settings.seed, k as u64);
            for _ in 0..TRIES {
                let at = Vec2::new(rng.range(min.x, max.x), rng.range(min.y, max.y));
                if contains(&flat, at) {
                    return Some((rng, Vec3::new(at.x, height, at.y), Vec3::X));
                }
            }
            None
        })
        .collect()
}

/// The area inside a closed outline (the shoelace formula).
fn area(outline: &[Vec2]) -> f32 {
    let n = outline.len();
    (0..n)
        .map(|i| outline[i].perp_dot(outline[(i + 1) % n]))
        .sum::<f32>()
        .abs()
        / 2.0
}

/// Whether `point` is inside the closed outline (even-odd rule).
fn contains(outline: &[Vec2], point: Vec2) -> bool {
    let n = outline.len();
    let mut inside = false;
    for i in 0..n {
        let (a, b) = (outline[i], outline[(i + 1) % n]);
        if (a.y > point.y) != (b.y > point.y)
            && point.x < a.x + (point.y - a.y) / (b.y - a.y) * (b.x - a.x)
        {
            inside = !inside;
        }
    }
    inside
}
