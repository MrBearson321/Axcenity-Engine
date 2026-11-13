//! Sculpt brushes: what each dab does to the vertices within its radius.
//!
//! A dab works out its moves from where the vertices are before it, for
//! itself and each mirrored copy (symmetry), adds them up and only then
//! moves the vertices, so the mirrored sides get mirrored moves. Each move
//! is held back by its vertex's mask (1 doesn't move at all).

use glam::Vec3;

use super::Sculpt;
use crate::paint::BrushFalloff;

/// How far Draw, Inflate and Crease push a vertex at the middle of a dab
/// at full strength, as a share of the radius.
const DRAW: f32 = 0.05;
/// How far over the surface Clay Strips builds, as a share of the radius.
const CLAY: f32 = 0.15;
/// The share of the way to their plane Flatten and Clay Strips take a
/// vertex at full strength.
const FLATTEN: f32 = 0.5;
/// The share of the way to the middle Pinch (and Crease's pinch) takes a
/// vertex at full strength.
const PINCH: f32 = 0.3;
/// How high Layer raises the surface in one stroke at full strength, as a
/// share of the radius.
const LAYER: f32 = 0.1;
/// The share of what's left to its height Layer takes a vertex each dab.
const LAYER_STEP: f32 = 0.5;
/// Elastic Grab moves what's this many radii away a little too. Further
/// costs more than a frame on a 500k-triangle sculpt (three radii took
/// 200 ms a dab, one and a half 38 ms).
const ELASTIC_REACH: f32 = 1.25;

/// A sculpting brush.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SculptTool {
    /// Pushes the surface out along its normal (Ctrl: in).
    #[default]
    Draw,
    /// Builds up a flat layer, a strip at a time (Ctrl: digs it).
    ClayStrips,
    /// Moves each vertex toward its neighbours' middle.
    Smooth,
    /// Drags what's under the brush when the stroke begins with the mouse.
    Grab,
    /// Pushes each vertex along its own normal (Ctrl: deflates).
    Inflate,
    /// Pushes a groove in and pinches its sides together (Ctrl: a ridge).
    Crease,
    /// Takes the surface toward the plane under the brush (Ctrl: away).
    Flatten,
    /// Pulls the surface toward the brush's middle (Ctrl: pushes away).
    Pinch,
    /// Pulls out what's under the brush along the stroke, as it goes.
    SnakeHook,
    /// Paints the mask, which holds vertices still (Ctrl: takes it off).
    Mask,
    /// Raises the surface to one height a stroke, however often it's gone
    /// over (Ctrl: lowers it).
    Layer,
    /// Pushes out a rounded bump, its sides drawn in (Ctrl: a dent).
    Blob,
    /// Flattens only what stands above the plane under the brush (Ctrl:
    /// fills what's below).
    Scrape,
    /// Fills only what's below the plane under the brush (Ctrl: scrapes
    /// what's above).
    Fill,
    /// Draw with a narrow, crisp middle, for cuts and seams (Ctrl: in).
    DrawSharp,
    /// Pushes the surface along the stroke, over the surface.
    Nudge,
    /// Grab with a soft falloff reaching a quarter of a radius further, so
    /// what's moved stretches instead of tearing.
    ElasticGrab,
}

impl SculptTool {
    pub const ALL: [SculptTool; 17] = [
        SculptTool::Draw,
        SculptTool::DrawSharp,
        SculptTool::ClayStrips,
        SculptTool::Layer,
        SculptTool::Blob,
        SculptTool::Inflate,
        SculptTool::Crease,
        SculptTool::Smooth,
        SculptTool::Flatten,
        SculptTool::Scrape,
        SculptTool::Fill,
        SculptTool::Pinch,
        SculptTool::Grab,
        SculptTool::ElasticGrab,
        SculptTool::SnakeHook,
        SculptTool::Nudge,
        SculptTool::Mask,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SculptTool::Draw => "Draw",
            SculptTool::ClayStrips => "Clay Strips",
            SculptTool::Smooth => "Smooth",
            SculptTool::Grab => "Grab",
            SculptTool::Inflate => "Inflate",
            SculptTool::Crease => "Crease",
            SculptTool::Flatten => "Flatten",
            SculptTool::Pinch => "Pinch",
            SculptTool::SnakeHook => "Snake Hook",
            SculptTool::Mask => "Mask",
            SculptTool::Layer => "Layer",
            SculptTool::Blob => "Blob",
            SculptTool::Scrape => "Scrape",
            SculptTool::Fill => "Fill",
            SculptTool::DrawSharp => "Draw Sharp",
            SculptTool::Nudge => "Nudge",
            SculptTool::ElasticGrab => "Elastic Grab",
        }
    }

    /// What it does, for its button's hover.
    pub fn tip(self) -> &'static str {
        match self {
            SculptTool::Draw => "Push the surface out along its normal (Ctrl: in)",
            SculptTool::ClayStrips => "Build up a flat layer, a strip at a time (Ctrl: dig it)",
            SculptTool::Smooth => "Even the surface out",
            SculptTool::Grab => "Drag what's under the brush with the mouse",
            SculptTool::Inflate => "Push each point out along its own normal (Ctrl: deflate)",
            SculptTool::Crease => "Push a groove in and pinch its sides (Ctrl: a ridge)",
            SculptTool::Flatten => "Take the surface toward the plane under the brush",
            SculptTool::Pinch => "Pull the surface toward the brush's middle (Ctrl: away)",
            SculptTool::SnakeHook => "Pull out what's under the brush along the stroke",
            SculptTool::Mask => "Paint the mask, which holds the surface still (Ctrl: unmask)",
            SculptTool::Layer => {
                "Raise the surface to one height a stroke, however often you go over it \
                 (Ctrl: lower it)"
            }
            SculptTool::Blob => "Push out a rounded bump (Ctrl: a dent)",
            SculptTool::Scrape => "Flatten only what stands above the surface (Ctrl: fill)",
            SculptTool::Fill => "Fill only what's below the surface (Ctrl: scrape)",
            SculptTool::DrawSharp => "Draw a crisp, narrow line, for cuts and seams (Ctrl: in)",
            SculptTool::Nudge => "Push the surface along the stroke",
            SculptTool::ElasticGrab => {
                "Grab with a soft reach past the radius, so the surface stretches"
            }
        }
    }

    /// True for the brushes that move what they hold by the mouse's move
    /// (strength doesn't change them).
    pub fn drags(self) -> bool {
        matches!(
            self,
            SculptTool::Grab | SculptTool::ElasticGrab | SculptTool::SnakeHook
        )
    }
}

/// A sculpting brush and its settings.
#[derive(Clone, Debug, PartialEq)]
pub struct SculptBrush {
    pub tool: SculptTool,
    /// In points on the view.
    pub radius: f32,
    /// 0 to 1.
    pub strength: f32,
    pub falloff: BrushFalloff,
    /// How far apart dabs go along a stroke, as a share of the radius.
    pub spacing: f32,
    /// Also sculpt mirrored across the object's X, Y and Z planes.
    pub symmetry: [bool; 3],
    /// Pen pressure changes the radius, and the strength.
    pub pressure_radius: bool,
    pub pressure_strength: bool,
}

impl Default for SculptBrush {
    fn default() -> Self {
        Self {
            tool: SculptTool::Draw,
            radius: 50.0,
            strength: 0.5,
            falloff: BrushFalloff::Smooth,
            spacing: 0.1,
            symmetry: [true, false, false],
            pressure_radius: false,
            pressure_strength: true,
        }
    }
}

/// One dab of a stroke, in the object's space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SculptDab {
    /// Where the brush is on the surface.
    pub center: Vec3,
    /// The surface's normal there, toward the viewer.
    pub normal: Vec3,
    /// In the mesh's units.
    pub radius: f32,
    /// 0 to 1.
    pub strength: f32,
    /// How far the brush moved since the last dab: what Grab and Snake
    /// Hook drag by, and the way Clay Strips' strips run.
    pub motion: Vec3,
    /// The brush's other way (Ctrl).
    pub invert: bool,
}

impl SculptDab {
    /// This dab and its copies mirrored across the planes `symmetry`
    /// names, each with which axes it's mirrored on.
    pub fn mirrored(&self, symmetry: [bool; 3]) -> Vec<(SculptDab, [bool; 3])> {
        let mut dabs = vec![(*self, [false; 3])];
        for axis in 0..3 {
            if !symmetry[axis] {
                continue;
            }
            let copies: Vec<(SculptDab, [bool; 3])> = dabs
                .iter()
                .map(|(dab, mirror)| {
                    let mut mirror = *mirror;
                    mirror[axis] = !mirror[axis];
                    let flip = |mut v: Vec3| {
                        v[axis] = -v[axis];
                        v
                    };
                    let dab = SculptDab {
                        center: flip(dab.center),
                        normal: flip(dab.normal),
                        motion: flip(dab.motion),
                        ..*dab
                    };
                    (dab, mirror)
                })
                .collect();
            dabs.extend(copies);
        }
        dabs
    }
}

/// `v` mirrored on the axes `mirror` names.
fn mirror(mut v: Vec3, mirror: [bool; 3]) -> Vec3 {
    for axis in 0..3 {
        if mirror[axis] {
            v[axis] = -v[axis];
        }
    }
    v
}

/// How much Elastic Grab moves a point `t` radii from where it took hold:
/// a regularised Kelvinlet's falloff, 1 at the middle and long-tailed,
/// brought down to 0 at [`ELASTIC_REACH`] so there's no step there.
pub(crate) fn elastic(t: f32) -> f32 {
    let kelvinlet = |t: f32| (1.0 + t * t).powf(-1.5);
    let edge = kelvinlet(ELASTIC_REACH);
    ((kelvinlet(t) - edge) / (1.0 - edge)).max(0.0)
}

/// The part of `v` along the plane square to `normal`.
fn along_plane(v: Vec3, normal: Vec3) -> Vec3 {
    v - normal * v.dot(normal)
}

impl Sculpt {
    /// Starts a stroke: Grab takes hold afresh at its first dab, and Layer
    /// starts a new layer.
    pub fn begin_stroke(&mut self) {
        self.grabbed = None;
        self.layer.clear();
    }

    /// One dab of `tool` (and its mirrored copies). True if it moved or
    /// masked anything.
    pub fn dab(
        &mut self,
        tool: SculptTool,
        falloff: BrushFalloff,
        symmetry: [bool; 3],
        dab: &SculptDab,
    ) -> bool {
        let copies = dab.mirrored(symmetry);
        if tool == SculptTool::Mask {
            return self.paint_mask(&copies, falloff);
        }
        let mut moves: Vec<(u32, Vec3)> = Vec::new();
        if matches!(tool, SculptTool::Grab | SculptTool::ElasticGrab) {
            if self.grabbed.is_none() {
                let mut grabbed = Vec::new();
                let elastic_reach = tool == SculptTool::ElasticGrab;
                for (copy, mirrored) in &copies {
                    let radius = copy.radius.max(1e-6);
                    let reach = if elastic_reach {
                        radius * ELASTIC_REACH
                    } else {
                        radius
                    };
                    let center = copy.center;
                    let held = self
                        .gather(center, reach, |p| p.distance(center) / reach)
                        .into_iter()
                        .map(|(v, out)| {
                            let w = if elastic_reach {
                                elastic(out * ELASTIC_REACH)
                            } else {
                                falloff.weight(out)
                            };
                            (v, w)
                        })
                        // What would hardly move isn't held (a frame's
                        // worth of work on a big sculpt).
                        .filter(|&(_, w)| w > 0.005)
                        .collect();
                    grabbed.push((held, *mirrored));
                }
                self.grabbed = Some(grabbed);
            }
            for (held, mirrored) in self.grabbed.iter().flatten() {
                let motion = mirror(dab.motion, *mirrored);
                moves.extend(held.iter().map(|&(v, w)| (v, motion * w)));
            }
        } else {
            for (copy, _) in &copies {
                self.displace(tool, falloff, copy, &mut moves);
            }
        }
        // Copies touching the same vertex add up.
        moves.sort_by_key(|&(v, _)| v);
        let mut moved = Vec::with_capacity(moves.len());
        let mut k = 0;
        while k < moves.len() {
            let v = moves[k].0;
            let mut total = Vec3::ZERO;
            while k < moves.len() && moves[k].0 == v {
                total += moves[k].1;
                k += 1;
            }
            let free = 1.0 - self.mask[v as usize];
            let step = total * free;
            if free > 0.0 && step != Vec3::ZERO && step.is_finite() {
                self.positions[v as usize] += step;
                moved.push(v);
            }
        }
        self.after_move(&moved);
        !moved.is_empty()
    }

    /// Adds the moves one copy of a dab makes to `moves`.
    fn displace(
        &mut self,
        tool: SculptTool,
        falloff: BrushFalloff,
        dab: &SculptDab,
        moves: &mut Vec<(u32, Vec3)>,
    ) {
        let radius = dab.radius.max(1e-6);
        // Clay Strips' square reaches into its corners.
        let reach = if tool == SculptTool::ClayStrips {
            radius * std::f32::consts::SQRT_2
        } else {
            radius
        };
        // Snake Hook pulls what was under the brush to where it is.
        let center = if tool == SculptTool::SnakeHook {
            dab.center - dab.motion
        } else {
            dab.center
        };
        let near = self.gather(center, reach, |p| p.distance(center) / reach);
        if near.is_empty() {
            return;
        }
        // The plane under the brush: the falloff-weighted middle and normal
        // of what's within the radius.
        let (mut normal, mut middle, mut total) = (Vec3::ZERO, Vec3::ZERO, 0.0);
        for &(v, out) in &near {
            let t = out * reach / radius;
            if t >= 1.0 {
                continue;
            }
            let w = falloff.weight(t).max(1e-4);
            normal += self.normals[v as usize] * w;
            middle += self.positions[v as usize] * w;
            total += w;
        }
        let normal = normal.try_normalize().unwrap_or(dab.normal);
        let middle = if total > 0.0 { middle / total } else { center };
        let sign = if dab.invert { -1.0 } else { 1.0 };
        let strength = dab.strength;
        // Clay Strips' square runs along the stroke; its first dab is round.
        let strip = along_plane(dab.motion, normal)
            .try_normalize()
            .map(|along| (along, normal.cross(along)));
        for (v, out) in near {
            let p = self.positions[v as usize];
            let t = out * reach / radius;
            let step = match tool {
                SculptTool::ClayStrips => {
                    let t = match strip {
                        Some((along, across)) => {
                            let local = p - middle;
                            (local.dot(along).abs().max(local.dot(across).abs())) / radius
                        }
                        None => t,
                    };
                    if t >= 1.0 {
                        continue;
                    }
                    let target = middle + normal * (sign * radius * CLAY);
                    let height = (p - target).dot(normal);
                    if height * sign >= 0.0 {
                        continue;
                    }
                    -normal * (height * falloff.weight(t) * strength * FLATTEN)
                }
                _ if t >= 1.0 => continue,
                SculptTool::Draw => normal * (sign * falloff.weight(t) * strength * radius * DRAW),
                SculptTool::Inflate => {
                    self.normals[v as usize] * (sign * falloff.weight(t) * strength * radius * DRAW)
                }
                SculptTool::Flatten => {
                    let height = (p - middle).dot(normal);
                    -normal * (height * sign * falloff.weight(t) * strength * FLATTEN)
                }
                SculptTool::Smooth => {
                    let around = self.neighbors.of(v);
                    if around.is_empty() {
                        continue;
                    }
                    let mean = around
                        .iter()
                        .map(|&n| self.positions[n as usize])
                        .sum::<Vec3>()
                        / around.len() as f32;
                    (mean - p) * (falloff.weight(t) * strength)
                }
                SculptTool::Crease => {
                    let w = falloff.weight(t) * strength;
                    let pinch = along_plane(center - p, normal);
                    normal * (-sign * w * w * radius * DRAW) + pinch * (w * PINCH)
                }
                SculptTool::Pinch => {
                    along_plane(center - p, normal) * (sign * falloff.weight(t) * strength * PINCH)
                }
                SculptTool::SnakeHook => dab.motion * falloff.weight(t),
                SculptTool::Layer => {
                    // Each vertex rises toward its own height for this
                    // stroke, and no further however often it's gone over.
                    let goal = sign * radius * LAYER * strength * falloff.weight(t);
                    let risen = self.layer.get(&v).copied().unwrap_or(0.0);
                    let left = goal - risen;
                    if left * sign <= 0.0 {
                        continue;
                    }
                    let rise = left * LAYER_STEP;
                    self.layer.insert(v, risen + rise);
                    normal * rise
                }
                SculptTool::Blob => {
                    let w = falloff.weight(t) * strength;
                    let out = self.normals[v as usize] * (sign * w * radius * DRAW);
                    out + along_plane(center - p, normal) * (w * PINCH * 0.5)
                }
                SculptTool::Scrape | SculptTool::Fill => {
                    let height = (p - middle).dot(normal);
                    let scrape = (tool == SculptTool::Scrape) != dab.invert;
                    if (scrape && height <= 0.0) || (!scrape && height >= 0.0) {
                        continue;
                    }
                    -normal * (height * falloff.weight(t) * strength * FLATTEN)
                }
                SculptTool::DrawSharp => {
                    let w = falloff.weight(t).powi(3);
                    normal * (sign * w * strength * radius * DRAW)
                }
                SculptTool::Nudge => {
                    along_plane(dab.motion, normal) * (falloff.weight(t) * strength)
                }
                SculptTool::Grab | SculptTool::ElasticGrab | SculptTool::Mask => continue,
            };
            moves.push((v, step));
        }
    }

    /// The Mask brush: adds (or with Ctrl takes off) mask by the falloff.
    fn paint_mask(&mut self, copies: &[(SculptDab, [bool; 3])], falloff: BrushFalloff) -> bool {
        let mut changed = Vec::new();
        for (dab, _) in copies {
            let radius = dab.radius.max(1e-6);
            let center = dab.center;
            let sign = if dab.invert { -1.0 } else { 1.0 };
            for (v, t) in self.gather(center, radius, |p| p.distance(center) / radius) {
                let mask = &mut self.mask[v as usize];
                let next = (*mask + sign * falloff.weight(t) * dab.strength).clamp(0.0, 1.0);
                if next != *mask {
                    *mask = next;
                    changed.push(v);
                }
            }
        }
        let mut chunks: Vec<usize> = changed
            .iter()
            .flat_map(|&v| self.vertex_triangles.of(v))
            .map(|&t| self.chunk_of[t as usize] as usize)
            .collect();
        chunks.sort_unstable();
        chunks.dedup();
        for &chunk in &chunks {
            self.index_chunk(chunk);
            self.dirty[chunk] = true;
        }
        !changed.is_empty()
    }
}
