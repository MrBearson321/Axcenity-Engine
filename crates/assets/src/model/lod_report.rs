//! What a model's levels of detail are, and what might be wrong with them:
//! the numbers the editor's LOD inspector and the `lod_report` tool show.
//! Warnings are advice, never a rejection: an asset with a warning still
//! loads and draws as it did.

use std::fmt::Write as _;

#[cfg(test)]
use super::ModelMaterial;
use super::{AlphaMode, Model, Primitive};

/// Bytes the GPU keeps per corner and per index of a mesh (position,
/// normal, texture coordinate, tangent; 32-bit indices), for the memory
/// estimate.
const VERTEX_BYTES: u64 = 12 + 12 + 8 + 16;
const INDEX_BYTES: u64 = 4;

/// Below this, a primitive costs too little to need simpler copies.
const NEEDS_LODS: usize = 5_000;
/// A level keeping more than this share of the level before saves too little.
const ALMOST_THE_SAME: f32 = 0.85;
/// A level keeping less than this share of the level before may lose its shape.
const TOO_AGGRESSIVE: f32 = 0.1;
/// The simplest level still this big is costly wherever it's drawn.
const HEAVY_AT_DISTANCE: usize = 50_000;
/// A level whose box is smaller than the full one's by more than this share
/// on any side has a different outline.
const SILHOUETTE_CHANGE: f32 = 0.1;

/// One level of a primitive.
#[derive(Clone, Debug, PartialEq)]
pub struct LevelReport {
    pub triangles: usize,
    pub vertices: usize,
    /// Its share of the full primitive's triangles (1 for the full one).
    pub share: f32,
    /// The GPU bytes its vertices and indices take, estimated.
    pub bytes: u64,
}

/// One primitive and its levels.
#[derive(Clone, Debug, PartialEq)]
pub struct PrimitiveReport {
    pub material: String,
    /// Cut out by its texture's alpha (leaves, grass cards).
    pub cut_out: bool,
    pub double_sided: bool,
    /// The full primitive, then each simpler copy.
    pub levels: Vec<LevelReport>,
}

/// Something about a model's levels worth a look.
#[derive(Clone, Debug, PartialEq)]
pub enum LodWarning {
    /// A primitive of this many triangles has no simpler copies (leaf
    /// cards, which edge collapsing can't simplify, often end up so).
    Missing { primitive: usize, triangles: usize },
    /// Level `level` keeps `share` of the level before: barely simpler.
    AlmostTheSame {
        primitive: usize,
        level: usize,
        share: f32,
    },
    /// Level `level` keeps only `share` of the level before.
    TooAggressive {
        primitive: usize,
        level: usize,
        share: f32,
    },
    /// The simplest level of the model still has this many triangles.
    HeavyAtDistance { triangles: usize },
    /// Level `level`'s box is smaller than the full one's by `shrink` on
    /// its most-changed side: its outline differs.
    Silhouette {
        primitive: usize,
        level: usize,
        shrink: f32,
    },
}

impl LodWarning {
    /// A sentence for people.
    pub fn text(&self) -> String {
        match self {
            Self::Missing {
                primitive,
                triangles,
            } => format!("primitive {primitive} has {triangles} triangles and no simpler copies"),
            Self::AlmostTheSame {
                primitive,
                level,
                share,
            } => format!(
                "primitive {primitive}'s level {level} keeps {:.0}% of the level before: barely simpler",
                share * 100.0
            ),
            Self::TooAggressive {
                primitive,
                level,
                share,
            } => format!(
                "primitive {primitive}'s level {level} keeps only {:.0}% of the level before: check its shape",
                share * 100.0
            ),
            Self::HeavyAtDistance { triangles } => format!(
                "its simplest levels still have {triangles} triangles, drawn (and shadowed) even far away"
            ),
            Self::Silhouette {
                primitive,
                level,
                shrink,
            } => format!(
                "primitive {primitive}'s level {level} is {:.0}% smaller on one side: its outline changes",
                shrink * 100.0
            ),
        }
    }
}

/// A model's levels of detail.
#[derive(Clone, Debug, PartialEq)]
pub struct LodReport {
    pub primitives: Vec<PrimitiveReport>,
    pub warnings: Vec<LodWarning>,
}

impl LodReport {
    /// The report for `model`.
    pub fn of(model: &Model) -> Self {
        let mut primitives = Vec::new();
        let mut warnings = Vec::new();
        for (index, primitive) in model.primitives.iter().enumerate() {
            let full = triangles(primitive);
            let mut levels = vec![level(primitive, full)];
            levels.extend(primitive.lods.iter().map(|copy| level(copy, full)));
            if primitive.lods.is_empty() && full >= NEEDS_LODS {
                warnings.push(LodWarning::Missing {
                    primitive: index,
                    triangles: full,
                });
            }
            let full_box = bounds(primitive);
            for (number, pair) in levels.windows(2).enumerate() {
                let level = number + 1;
                let share = pair[1].triangles as f32 / pair[0].triangles.max(1) as f32;
                if share > ALMOST_THE_SAME {
                    warnings.push(LodWarning::AlmostTheSame {
                        primitive: index,
                        level,
                        share,
                    });
                } else if share < TOO_AGGRESSIVE {
                    warnings.push(LodWarning::TooAggressive {
                        primitive: index,
                        level,
                        share,
                    });
                }
                if let (Some(full_box), Some(copy_box)) =
                    (full_box, bounds(&primitive.lods[number]))
                {
                    let shrink = (0..3)
                        .map(|axis| {
                            let size = full_box.1[axis] - full_box.0[axis];
                            let copy = copy_box.1[axis] - copy_box.0[axis];
                            if size > 1e-6 { 1.0 - copy / size } else { 0.0 }
                        })
                        .fold(0.0f32, f32::max);
                    if shrink > SILHOUETTE_CHANGE {
                        warnings.push(LodWarning::Silhouette {
                            primitive: index,
                            level,
                            shrink,
                        });
                    }
                }
            }
            primitives.push(PrimitiveReport {
                material: primitive.material.name.clone(),
                cut_out: matches!(primitive.material.alpha_mode, AlphaMode::Mask { .. }),
                double_sided: primitive.material.double_sided,
                levels,
            });
        }
        let report = Self {
            primitives,
            warnings,
        };
        let simplest = report.triangles_at(usize::MAX);
        let mut report = report;
        if simplest >= HEAVY_AT_DISTANCE {
            report.warnings.push(LodWarning::HeavyAtDistance {
                triangles: simplest,
            });
        }
        report
    }

    /// The most levels any primitive has (1 for none simplified).
    pub fn level_count(&self) -> usize {
        self.primitives
            .iter()
            .map(|primitive| primitive.levels.len())
            .max()
            .unwrap_or(0)
    }

    /// The model's triangles drawn at `level`: each primitive at that level,
    /// or its simplest if it has fewer.
    pub fn triangles_at(&self, level: usize) -> usize {
        self.primitives
            .iter()
            .filter_map(|primitive| {
                primitive
                    .levels
                    .get(level)
                    .or(primitive.levels.last())
                    .map(|level| level.triangles)
            })
            .sum()
    }

    /// The model's GPU bytes for every level, estimated.
    pub fn bytes(&self) -> u64 {
        self.primitives
            .iter()
            .flat_map(|primitive| &primitive.levels)
            .map(|level| level.bytes)
            .sum()
    }

    /// Lines for people: one per level with its triangles and reduction,
    /// the primitives, then the warnings.
    pub fn text(&self) -> String {
        let mut text = String::new();
        let full = self.triangles_at(0).max(1);
        for level in 0..self.level_count().max(1) {
            let triangles = self.triangles_at(level);
            let _ = writeln!(
                text,
                "  LOD{level}: {triangles} triangles ({:.0}% of LOD0)",
                triangles as f64 * 100.0 / full as f64
            );
        }
        let _ = writeln!(
            text,
            "  {} primitives, about {:.1} MB on the GPU with every level",
            self.primitives.len(),
            self.bytes() as f64 / (1024.0 * 1024.0)
        );
        for (index, primitive) in self.primitives.iter().enumerate() {
            let levels: Vec<String> = primitive
                .levels
                .iter()
                .map(|level| level.triangles.to_string())
                .collect();
            let _ = writeln!(
                text,
                "    {index}: `{}`{}{} {}",
                primitive.material,
                if primitive.cut_out { " cut-out" } else { "" },
                if primitive.double_sided {
                    " double-sided"
                } else {
                    ""
                },
                levels.join(" > ")
            );
        }
        for warning in &self.warnings {
            let _ = writeln!(text, "  warning: {}", warning.text());
        }
        text
    }
}

fn triangles(primitive: &Primitive) -> usize {
    primitive.indices.len() / 3
}

fn level(primitive: &Primitive, full: usize) -> LevelReport {
    let triangles = triangles(primitive);
    // A simpler copy keeps only the corners its triangles use.
    let vertices = primitive.positions.len();
    LevelReport {
        triangles,
        vertices,
        share: triangles as f32 / full.max(1) as f32,
        bytes: vertices as u64 * VERTEX_BYTES + primitive.indices.len() as u64 * INDEX_BYTES,
    }
}

/// The box of the corners `primitive`'s triangles use.
fn bounds(primitive: &Primitive) -> Option<([f32; 3], [f32; 3])> {
    let mut low = [f32::INFINITY; 3];
    let mut high = [f32::NEG_INFINITY; 3];
    for &index in &primitive.indices {
        let corner = primitive.positions.get(index as usize)?;
        for axis in 0..3 {
            low[axis] = low[axis].min(corner[axis]);
            high[axis] = high[axis].max(corner[axis]);
        }
    }
    (low[0] <= high[0]).then_some((low, high))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A strip of `count` quads (two triangles each) `width` wide.
    fn strip(count: usize, width: f32) -> Primitive {
        let (mut positions, mut indices) = (Vec::new(), Vec::new());
        for quad in 0..count {
            let x = quad as f32 / count as f32 * width;
            let base = positions.len() as u32;
            positions.extend([
                [x, 0.0, 0.0],
                [x + 0.1, 0.0, 0.0],
                [x, 1.0, 0.0],
                [x + 0.1, 1.0, 0.0],
            ]);
            indices.extend([base, base + 1, base + 2, base + 1, base + 3, base + 2]);
        }
        let normals = vec![[0.0, 0.0, 1.0]; positions.len()];
        let material = ModelMaterial {
            name: "Leaves".into(),
            ..ModelMaterial::default()
        };
        Primitive::new(positions, normals, Vec::new(), indices, material)
    }

    /// A model of `full` with exactly `lods` as its copies, all fitted
    /// alike as a file's copies are.
    fn model(full: Primitive, lods: Vec<Primitive>) -> Model {
        let mut primitives = vec![full];
        primitives.extend(lods);
        let mut model = Model::from_primitives(primitives).unwrap();
        let copies: Vec<Primitive> = model.primitives.drain(1..).collect();
        model.primitives[0].lods = copies;
        model
    }

    #[test]
    fn levels_are_counted_with_their_share_of_the_full_one() {
        let report = LodReport::of(&model(
            strip(4000, 10.0),
            vec![strip(2000, 10.0), strip(1000, 10.0)],
        ));
        assert_eq!(report.level_count(), 3);
        assert_eq!(report.triangles_at(0), 8000);
        assert_eq!(report.triangles_at(1), 4000);
        assert_eq!(report.triangles_at(2), 2000);
        assert_eq!(report.triangles_at(9), 2000, "past the last, the simplest");
        assert!((report.primitives[0].levels[2].share - 0.25).abs() < 1e-6);
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert!(report.text().contains("LOD1: 4000 triangles (50% of LOD0)"));
    }

    #[test]
    fn a_big_primitive_without_copies_and_a_heavy_far_model_are_flagged() {
        let report = LodReport::of(&model(strip(30_000, 10.0), Vec::new()));
        assert!(report.warnings.contains(&LodWarning::Missing {
            primitive: 0,
            triangles: 60_000
        }));
        assert!(
            report
                .warnings
                .contains(&LodWarning::HeavyAtDistance { triangles: 60_000 })
        );
        // A small one is fine without.
        assert!(
            LodReport::of(&model(strip(100, 1.0), Vec::new()))
                .warnings
                .is_empty()
        );
    }

    #[test]
    fn copies_too_alike_too_simple_or_smaller_in_outline_are_flagged() {
        let report = LodReport::of(&model(
            strip(4000, 10.0),
            vec![strip(3800, 10.0), strip(100, 5.0)],
        ));
        let kinds: Vec<&LodWarning> = report.warnings.iter().collect();
        assert!(
            matches!(kinds[0], LodWarning::AlmostTheSame { level: 1, .. }),
            "{kinds:?}"
        );
        assert!(
            kinds
                .iter()
                .any(|w| matches!(w, LodWarning::TooAggressive { level: 2, .. })),
            "{kinds:?}"
        );
        assert!(
            kinds
                .iter()
                .any(|w| matches!(w, LodWarning::Silhouette { level: 2, .. })),
            "{kinds:?}"
        );
    }
}
