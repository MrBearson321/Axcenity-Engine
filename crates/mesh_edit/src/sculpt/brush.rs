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
}

impl SculptTool {
    pub const ALL: [SculptTool; 10] = [
        SculptTool::Draw,
        SculptTool::ClayStrips,
        SculptTool::Smooth,
        SculptTool::Grab,
        SculptTool::Inflate,
        SculptTool::Crease,
        SculptTool::Flatten,
        SculptTool::Pinch,
        SculptTool::SnakeHook,
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
        }
    }

    /// True for the brushes that move what they hold by the mouse's move
    /// (strength doesn't change them).
    pub fn drags(self) -> bool {
        matches!(self, SculptTool::Grab | SculptTool::SnakeHook)
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

/// The part of `v` along the plane square to `normal`.
fn along_plane(v: Vec3, normal: Vec3) -> Vec3 {
    v - normal * v.dot(normal)
}

impl Sculpt {
    /// Starts a stroke: Grab takes hold afresh at its first dab.
    pub fn begin_stroke(&mut self) {
        self.grabbed = None;
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
        if tool == SculptTool::Grab {
            if self.grabbed.is_none() {
                let mut grabbed = Vec::new();
                for (copy, mirrored) in &copies {
                    let radius = copy.radius.max(1e-6);
                    let center = copy.center;
                    let held = self
                        .gather(center, radius, |p| p.distance(center) / radius)
                        .into_iter()
                        .map(|(v, t)| (v, falloff.weight(t)))
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
                SculptTool::Grab | SculptTool::Mask => continue,
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
