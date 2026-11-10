//! Cooking a project's textures ahead of time (roadmap L2).
//!
//! A texture file is decoded, halved down to 1×1 with the renderer's box
//! filter, and each level BC7-encoded with Intel's ISPC compressor. The
//! result goes into `.axcenity/imported/textures/` (see
//! [`axcenity_assets::cooked_path`]), where `Assets` finds it the next time
//! the texture loads: a file read and a quarter-size upload instead of a
//! decode, CPU mipmaps and a full RGBA upload, and a quarter of the GPU
//! memory.
//!
//! [`cook_project`] cooks every image under `assets/` that has no current
//! cooked copy. The editor runs it on a thread when a project opens, and
//! `axcenity-editor --cook <folder>` runs it from the command line.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use axcenity_assets::{
    CookedFormat, CookedTexture, SourceStamp, TEXTURE_EXTENSIONS, decode_texture, has_alpha,
    is_cooked, remove_orphaned_copies, safe_path, sampled_average, write_cooked,
};
use intel_tex_2::{RgbaSurface, bc7};
use rayon::prelude::*;

/// Rows of blocks encoded together, a job for one thread.
const BLOCK_ROWS_PER_JOB: u32 = 16;

/// Cooks `rgba` (`width`×`height`, 4 bytes a pixel) into BC7 mip levels,
/// or `None` if the size isn't whole 4×4 blocks (a BC7 texture must start
/// with whole blocks; such textures stay RGBA). Uses rayon's threads.
pub fn cook_rgba(width: u32, height: u32, rgba: &[u8]) -> Option<CookedTexture> {
    let whole = width > 0
        && height > 0
        && width.is_multiple_of(4)
        && height.is_multiple_of(4)
        && rgba.len() == width as usize * height as usize * 4;
    if !whole {
        return None;
    }
    let alpha = has_alpha(rgba);
    let settings = if alpha {
        bc7::alpha_very_fast_settings()
    } else {
        bc7::opaque_very_fast_settings()
    };
    let mut levels = Vec::with_capacity(CookedTexture::level_count(width, height));
    let mut level = (width, height, rgba.to_vec());
    loop {
        let (level_width, level_height, pixels) = &level;
        levels.push(encode_level(&settings, *level_width, *level_height, pixels));
        if (*level_width, *level_height) == (1, 1) {
            break;
        }
        level = half_size(*level_width, *level_height, pixels);
    }
    Some(CookedTexture {
        format: CookedFormat::Bc7,
        width,
        height,
        levels,
        has_alpha: alpha,
        average: sampled_average(rgba),
    })
}

/// One level's BC7 blocks. A level that isn't whole blocks (the smallest
/// ones) is padded by repeating its last row and column.
fn encode_level(settings: &bc7::EncodeSettings, width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let (padded_width, padded_height) = (width.next_multiple_of(4), height.next_multiple_of(4));
    let padded;
    let pixels = if (padded_width, padded_height) == (width, height) {
        rgba
    } else {
        padded = pad(width, height, rgba, padded_width, padded_height);
        &padded
    };
    let row_bytes = padded_width as usize * 4;
    let block_row_bytes = (padded_width / 4) as usize * 16;
    let job_rows = (BLOCK_ROWS_PER_JOB * 4) as usize;
    let mut blocks = vec![0u8; block_row_bytes * (padded_height / 4) as usize];
    blocks
        .par_chunks_mut(block_row_bytes * BLOCK_ROWS_PER_JOB as usize)
        .zip(pixels.par_chunks(row_bytes * job_rows))
        .for_each(|(out, rows)| {
            let surface = RgbaSurface {
                width: padded_width,
                height: (rows.len() / row_bytes) as u32,
                stride: padded_width * 4,
                data: rows,
            };
            bc7::compress_blocks_into(settings, &surface, out);
        });
    blocks
}

/// `rgba` grown to `to_width`×`to_height` by repeating its edge pixels.
fn pad(width: u32, height: u32, rgba: &[u8], to_width: u32, to_height: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(to_width as usize * to_height as usize * 4);
    for y in 0..to_height {
        for x in 0..to_width {
            let at = (y.min(height - 1) as usize * width as usize + x.min(width - 1) as usize) * 4;
            out.extend_from_slice(&rgba[at..at + 4]);
        }
    }
    out
}

/// An image at half the size (at least 1 pixel), each pixel the average of
/// the 2×2 it covers: the renderer's `half_size`, so a cooked texture's
/// levels match what the RGBA path makes.
fn half_size(width: u32, height: u32, rgba: &[u8]) -> (u32, u32, Vec<u8>) {
    let (half_width, half_height) = ((width / 2).max(1), (height / 2).max(1));
    let mut half = vec![0u8; half_width as usize * half_height as usize * 4];
    half.par_chunks_mut(half_width as usize * 4)
        .enumerate()
        .for_each(|(y, row)| {
            let y = y as u32;
            for x in 0..half_width {
                let mut sum = [0u32; 4];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    // An odd last row or column is averaged with itself.
                    let from_x = (x * 2 + dx).min(width - 1);
                    let from_y = (y * 2 + dy).min(height - 1);
                    let at = (from_y as usize * width as usize + from_x as usize) * 4;
                    for (channel, total) in sum.iter_mut().enumerate() {
                        *total += u32::from(rgba[at + channel]);
                    }
                }
                let out = &mut row[x as usize * 4..x as usize * 4 + 4];
                for (pixel, total) in out.iter_mut().zip(sum) {
                    *pixel = ((total + 2) / 4) as u8;
                }
            }
        });
    (half_width, half_height, half)
}

/// What happened to one texture.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Cooked,
    /// Its size isn't whole 4×4 blocks, so it stays RGBA.
    NotBlockSized,
    Failed(String),
}

/// Cooks the project file `relative` (such as `assets/textures/wood.png`)
/// and writes its cooked copy.
pub fn cook_file(project: &Path, relative: &str) -> Outcome {
    let Some(path) = safe_path(relative).map(|path| project.join(path)) else {
        return Outcome::Failed("it isn't inside the project folder".to_owned());
    };
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(err) => return Outcome::Failed(err.to_string()),
    };
    let stamp = SourceStamp::of(&path, &bytes);
    let image = match decode_texture(&bytes, relative) {
        Ok(image) => image,
        Err(reason) => return Outcome::Failed(reason),
    };
    let Some(cooked) = cook_rgba(image.width, image.height, image.rgba()) else {
        return Outcome::NotBlockSized;
    };
    match write_cooked(project, relative, stamp, &cooked) {
        Ok(()) => Outcome::Cooked,
        Err(err) => Outcome::Failed(err.to_string()),
    }
}

/// Every image file under the project's `assets/` folder, as project paths
/// with `/` between folders, in a steady order.
pub fn texture_files(project: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut folders = vec![PathBuf::from("assets")];
    while let Some(folder) = folders.pop() {
        let Ok(entries) = std::fs::read_dir(project.join(&folder)) else {
            continue;
        };
        for entry in entries.flatten() {
            let relative = folder.join(entry.file_name());
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                folders.push(relative);
            } else if is_texture(&relative)
                && let Some(text) = relative.to_str()
            {
                found.push(text.replace('\\', "/"));
            }
        }
    }
    found.sort();
    found
}

fn is_texture(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            TEXTURE_EXTENSIONS
                .iter()
                .any(|known| known.eq_ignore_ascii_case(extension))
        })
}

/// How cooking a project went.
#[derive(Debug, Default)]
pub struct Report {
    /// Textures cooked now.
    pub cooked: usize,
    /// Textures whose cooked copy was already current.
    pub current: usize,
    /// Textures left as they are: not whole 4×4 blocks.
    pub skipped: usize,
    /// Textures that couldn't be cooked, and why.
    pub failed: Vec<(String, String)>,
    /// Cached copies deleted because their source file is gone.
    pub removed: usize,
}

/// Deletes the cached copies of files that are gone, then cooks every
/// texture under `project`'s `assets/` that has no current cooked copy,
/// calling `progress` with each path and its outcome. Stops between
/// textures once `stop` is set.
pub fn cook_project(
    project: &Path,
    stop: &AtomicBool,
    mut progress: impl FnMut(&str, &Outcome),
) -> Report {
    let mut report = Report {
        removed: remove_orphaned_copies(project),
        ..Report::default()
    };
    for relative in texture_files(project) {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        if is_cooked(project, &relative) {
            report.current += 1;
            continue;
        }
        let outcome = cook_file(project, &relative);
        progress(&relative, &outcome);
        match outcome {
            Outcome::Cooked => report.cooked += 1,
            Outcome::NotBlockSized => report.skipped += 1,
            Outcome::Failed(reason) => report.failed.push((relative, reason)),
        }
    }
    report
}

#[cfg(test)]
mod tests;
