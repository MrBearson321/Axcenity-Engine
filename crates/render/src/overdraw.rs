//! Numbers from the overdraw debug view ([`DebugView::Overdraw`]): how many
//! surfaces cover each pixel, read back from the picture.
//!
//! **What it counts.** The overdraw view draws every batch the scene drew
//! again, with no depth test and back faces culled as the scene culls
//! them, adding [`OVERDRAW_STEP`] of light per surface. A pixel's count is
//! its *depth complexity*: how many triangles of the scene's surfaces
//! cover it, cut-out holes (`alpha_cutoff`) and dithered fades left out.
//!
//! **What it doesn't.** It isn't how often the GPU ran the lit shader
//! there. The scene pass draws cut-out materials' depth first and then
//! lights each pixel once (`PrefillPipelines`), and an opaque surface in
//! front stops what's behind it before shading (early depth test), so the
//! lit shader runs far fewer times than this count. Shadow maps, the
//! prepass, MSAA samples, terrain's own passes, sprites and the sky aren't
//! counted, and see-through items count as solid. Read it as "how many
//! layers of geometry the camera looks through", the input the depth
//! tests then reject.
//!
//! **How exact.** Each layer adds 0.02 to the blue channel in linear
//! light, so counts read back exactly up to [`MAX_LAYERS`] (where blue is
//! full) from an 8-bit sRGB picture; past it, a pixel reads as
//! `MAX_LAYERS`. Each addition is rounded to the target's 8 bits, so a
//! high count can be off by a layer or two.
//!
//! [`DebugView::Overdraw`]: crate::DebugView::Overdraw

use std::fmt::Write as _;

/// Light each layer adds in the overdraw view, in linear red, green and
/// blue. Must match `OVERDRAW_STEP` in debug_views.wgsl.
pub const OVERDRAW_STEP: [f32; 3] = [0.12, 0.05, 0.02];

/// The most layers a pixel can be read as: blue is full past it.
pub const MAX_LAYERS: u32 = 50;

/// The bands the legend and the histogram use: a pixel with this many
/// layers or more, up to the next.
pub const OVERDRAW_BANDS: [u32; 6] = [1, 2, 4, 8, 16, 32];

/// The grid the screen is cut into to find its busiest regions.
const GRID: (u32, u32) = (8, 6);

/// What the overdraw view shows, as numbers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OverdrawStats {
    pub width: u32,
    pub height: u32,
    /// Pixels with at least one surface.
    pub covered: u64,
    /// Layers summed over every pixel.
    pub layers: u64,
    /// The most layers on one pixel (at most [`MAX_LAYERS`]).
    pub max: u32,
    /// Pixels with no surface, then in each of [`OVERDRAW_BANDS`].
    pub bands: [u64; OVERDRAW_BANDS.len() + 1],
    /// Pixels at [`MAX_LAYERS`], which may have more.
    pub saturated: u64,
    /// The average layers of each cell of an 8 × 6 grid over the screen,
    /// row by row from the top left.
    pub cells: Vec<f32>,
}

/// The layers an overdraw pixel's blue byte (sRGB-encoded) stands for.
pub fn layers_of(blue: u8) -> u32 {
    let encoded = f32::from(blue) / 255.0;
    let linear = if encoded <= 0.04045 {
        encoded / 12.92
    } else {
        ((encoded + 0.055) / 1.055).powf(2.4)
    };
    ((linear / OVERDRAW_STEP[2]).round() as u32).min(MAX_LAYERS)
}

impl OverdrawStats {
    /// Reads RGBA8 `pixels` (sRGB-encoded, row by row from the top left, as
    /// `TextureRenderer::read_pixels` gives them) of the overdraw view.
    pub fn from_pixels(pixels: &[u8], width: u32, height: u32) -> Self {
        let mut stats = Self {
            width,
            height,
            ..Self::default()
        };
        let (columns, rows) = GRID;
        let mut cell_sums = vec![0u64; (columns * rows) as usize];
        let mut cell_pixels = vec![0u64; (columns * rows) as usize];
        for (index, pixel) in pixels.chunks_exact(4).enumerate() {
            let index = index as u32;
            let (x, y) = (index % width.max(1), index / width.max(1));
            let layers = layers_of(pixel[2]);
            stats.layers += u64::from(layers);
            stats.max = stats.max.max(layers);
            if layers > 0 {
                stats.covered += 1;
            }
            if layers >= MAX_LAYERS {
                stats.saturated += 1;
            }
            let band = OVERDRAW_BANDS
                .iter()
                .rposition(|&low| layers >= low)
                .map_or(0, |band| band + 1);
            stats.bands[band] += 1;
            let cell = ((y * rows / height.max(1)) * columns + x * columns / width.max(1)) as usize;
            if let Some(sum) = cell_sums.get_mut(cell) {
                *sum += u64::from(layers);
                cell_pixels[cell] += 1;
            }
        }
        stats.cells = cell_sums
            .iter()
            .zip(&cell_pixels)
            .map(|(&sum, &count)| {
                if count > 0 {
                    sum as f32 / count as f32
                } else {
                    0.0
                }
            })
            .collect();
        stats
    }

    /// Average layers over every pixel of the picture.
    pub fn mean_per_pixel(&self) -> f64 {
        let pixels = u64::from(self.width) * u64::from(self.height);
        if pixels == 0 {
            0.0
        } else {
            self.layers as f64 / pixels as f64
        }
    }

    /// Average layers over the pixels something covers.
    pub fn mean_per_covered(&self) -> f64 {
        if self.covered == 0 {
            0.0
        } else {
            self.layers as f64 / self.covered as f64
        }
    }

    /// The share of all pixels with at least `layers` layers.
    pub fn share_at_least(&self, layers: u32) -> f64 {
        let pixels = u64::from(self.width) * u64::from(self.height);
        let Some(first) = OVERDRAW_BANDS.iter().position(|&low| low >= layers) else {
            return 0.0;
        };
        let count: u64 = self.bands[first + 1..].iter().sum();
        if pixels == 0 {
            0.0
        } else {
            count as f64 / pixels as f64
        }
    }

    /// The `n` busiest cells of the grid: (column, row, average layers),
    /// busiest first.
    pub fn busiest_cells(&self, n: usize) -> Vec<(u32, u32, f32)> {
        let mut cells: Vec<(u32, u32, f32)> = self
            .cells
            .iter()
            .enumerate()
            .map(|(index, &mean)| (index as u32 % GRID.0, index as u32 / GRID.0, mean))
            .collect();
        cells.sort_by(|a, b| b.2.total_cmp(&a.2));
        cells.truncate(n);
        cells
    }

    /// Lines for people.
    pub fn report(&self) -> String {
        let pixels = (u64::from(self.width) * u64::from(self.height)).max(1) as f64;
        let mut text = format!(
            "  layers per pixel: {:.2} over the picture, {:.2} where covered; max {}{}\n",
            self.mean_per_pixel(),
            self.mean_per_covered(),
            self.max,
            if self.saturated > 0 { "+" } else { "" }
        );
        let _ = write!(
            text,
            "  pixels: none {:.1}%",
            self.bands[0] as f64 * 100.0 / pixels
        );
        for (index, low) in OVERDRAW_BANDS.iter().enumerate() {
            let high = OVERDRAW_BANDS
                .get(index + 1)
                .map_or("+".to_owned(), |high| {
                    if *high == low + 1 {
                        String::new()
                    } else {
                        format!("-{}", high - 1)
                    }
                });
            let _ = write!(
                text,
                ", {low}{high} {:.1}%",
                self.bands[index + 1] as f64 * 100.0 / pixels
            );
        }
        text.push('\n');
        let busiest: Vec<String> = self
            .busiest_cells(4)
            .into_iter()
            .map(|(column, row, mean)| format!("({column}, {row}) {mean:.1}"))
            .collect();
        let _ = writeln!(
            text,
            "  busiest of an {}x{} grid (column, row from the top left): {}",
            GRID.0,
            GRID.1,
            busiest.join(", ")
        );
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The byte a pixel of `layers` layers reads as.
    fn blue_of(layers: u32) -> u8 {
        let linear = (layers as f32 * OVERDRAW_STEP[2]).min(1.0);
        let encoded = if linear <= 0.003_130_8 {
            linear * 12.92
        } else {
            1.055 * linear.powf(1.0 / 2.4) - 0.055
        };
        (encoded * 255.0).round() as u8
    }

    #[test]
    fn every_count_up_to_the_most_reads_back_from_its_byte() {
        for layers in 0..=MAX_LAYERS {
            assert_eq!(layers_of(blue_of(layers)), layers, "{layers}");
        }
        assert_eq!(layers_of(255), MAX_LAYERS);
    }

    #[test]
    fn the_numbers_count_layers_bands_and_the_busiest_region() {
        // 16 × 12: the top left quarter has 10 layers, the rest 1, one
        // pixel none.
        let (width, height) = (16u32, 12u32);
        let mut pixels = Vec::new();
        for y in 0..height {
            for x in 0..width {
                let layers = if x == 15 && y == 11 {
                    0
                } else if x < 8 && y < 6 {
                    10
                } else {
                    1
                };
                pixels.extend([0, 0, blue_of(layers), 255]);
            }
        }
        let stats = OverdrawStats::from_pixels(&pixels, width, height);
        assert_eq!(stats.covered, 191);
        assert_eq!(stats.max, 10);
        assert_eq!(stats.layers, 48 * 10 + 143);
        assert_eq!(stats.bands[0], 1, "one pixel with none");
        assert_eq!(stats.bands[1], 143, "143 with one");
        assert_eq!(stats.bands[4], 48, "48 in 8-15");
        assert!((stats.share_at_least(8) - 48.0 / 192.0).abs() < 1e-9);
        let (column, row, mean) = stats.busiest_cells(1)[0];
        assert!(column < 4 && row < 3 && (mean - 10.0).abs() < 1e-6);
        assert!(stats.report().contains("max 10"), "{}", stats.report());
    }
}
