//! The Assets panel's image thumbnails. Decoding a 2K–8K texture takes long
//! enough to stall a frame, so a few background threads read and shrink
//! them; a tile shows its icon until its thumbnail arrives.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use eframe::egui;

/// Thumbnails are scaled down to at most this many pixels across: enough
/// for the hover preview, which is larger than the tile.
pub const THUMBNAIL_PIXELS: u32 = 192;
/// At most this many images are decoded at once, leaving cores for the
/// editor itself.
const WORKERS: usize = 3;

/// `(path, is a texture set)`.
type Request = (String, bool);
type Done = (String, Option<egui::ColorImage>);

#[derive(Default)]
pub struct Thumbnails {
    folder: Option<PathBuf>,
    /// By asset path, `None` for an image that can't be read.
    done: HashMap<String, Option<egui::TextureHandle>>,
    loading: HashSet<String>,
    /// Started on the first request. Dropping them stops the workers.
    channels: Option<(Sender<Request>, Receiver<Done>)>,
}

impl Thumbnails {
    /// Forgets every thumbnail, for a newly opened (or closed) project.
    pub fn open(&mut self, folder: Option<&Path>) {
        *self = Self {
            folder: folder.map(Path::to_owned),
            ..Self::default()
        };
    }

    /// Drops the thumbnail of `path`, so it's read again, after the file
    /// changed.
    pub fn forget(&mut self, path: &str) {
        self.done.remove(path);
    }

    /// Drops the thumbnails of files that are gone. It looks at the disk,
    /// not the panel's tiles: a texture set's images aren't tiles, but a
    /// terrain layer still shows them.
    pub fn forget_missing(&mut self) {
        let Some(folder) = &self.folder else {
            return;
        };
        self.done.retain(|path, _| folder.join(path).is_file());
    }

    /// Turns finished thumbnails into textures. Call once a frame.
    pub fn poll(&mut self, ctx: &egui::Context) {
        let Some((_, results)) = &self.channels else {
            return;
        };
        for (path, image) in results.try_iter() {
            self.loading.remove(&path);
            let handle = image.map(|image| {
                ctx.load_texture(
                    format!("asset thumbnail {path}"),
                    image,
                    egui::TextureOptions::LINEAR,
                )
            });
            self.done.insert(path, handle);
        }
    }

    /// The thumbnail of `path`, asking for it the first time. A texture set
    /// (`set`) shows its color map.
    pub fn get(
        &mut self,
        ctx: &egui::Context,
        path: &str,
        set: bool,
    ) -> Option<egui::TextureHandle> {
        if let Some(done) = self.done.get(path) {
            return done.clone();
        }
        if self.loading.contains(path) {
            return None;
        }
        let folder = self.folder.clone()?;
        let (requests, _) = self
            .channels
            .get_or_insert_with(|| start_workers(ctx, folder));
        if requests.send((path.to_owned(), set)).is_ok() {
            self.loading.insert(path.to_owned());
        }
        None
    }
}

fn start_workers(ctx: &egui::Context, folder: PathBuf) -> (Sender<Request>, Receiver<Done>) {
    let (request_tx, request_rx) = mpsc::channel::<Request>();
    let (done_tx, done_rx) = mpsc::channel();
    // One queue shared by the workers: whichever is free takes the next.
    let requests = Arc::new(Mutex::new(request_rx));
    for _ in 0..WORKERS {
        let requests = Arc::clone(&requests);
        let done = done_tx.clone();
        let folder = folder.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            loop {
                // The lock is only held while waiting, not while decoding.
                let next = requests.lock().map(|requests| requests.recv());
                let Ok(Ok((path, set))) = next else {
                    return;
                };
                let image = load(&folder, &path, set);
                if done.send((path, image)).is_err() {
                    return;
                }
                ctx.request_repaint();
            }
        });
    }
    (request_tx, done_rx)
}

fn load(folder: &Path, path: &str, set: bool) -> Option<egui::ColorImage> {
    let picture = if set {
        let relative = axcenity_assets::safe_path(path)?;
        let set = axcenity_assets::read_texture_set(&folder.join(relative)).ok()?;
        set.path(axcenity_assets::MapSlot::BaseColor)?.to_owned()
    } else {
        path.to_owned()
    };
    let image = axcenity_assets::read_texture(folder, &picture).ok()?;
    shrink(image.width, image.height, &image.rgba, THUMBNAIL_PIXELS)
}

/// `rgba` (`width` × `height`) scaled down to at most `most` pixels across.
/// Each pixel is the average of the block it covers, so a detailed texture
/// still reads as a whole instead of as noise. `None` for an empty or short
/// image.
pub fn shrink(width: u32, height: u32, rgba: &[u8], most: u32) -> Option<egui::ColorImage> {
    if width == 0 || height == 0 || rgba.len() < (width * height * 4) as usize {
        return None;
    }
    let scale = (most as f32 / width.max(height) as f32).min(1.0);
    let (w, h) = (
        ((width as f32 * scale).round() as u32).max(1),
        ((height as f32 * scale).round() as u32).max(1),
    );
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        let (y0, y1) = span(y, h, height);
        for x in 0..w {
            let (x0, x1) = span(x, w, width);
            let mut sum = [0u64; 4];
            for sy in y0..y1 {
                let row = (sy * width) as usize * 4;
                for pixel in rgba[row + x0 as usize * 4..row + x1 as usize * 4]
                    .as_chunks::<4>()
                    .0
                {
                    for (total, &channel) in sum.iter_mut().zip(pixel) {
                        *total += u64::from(channel);
                    }
                }
            }
            let count = u64::from((x1 - x0) * (y1 - y0));
            pixels.extend(sum.map(|total| (total / count) as u8));
        }
    }
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [w as usize, h as usize],
        &pixels,
    ))
}

/// The source pixels `[start, end)` that output pixel `i` of `out` covers,
/// never empty.
fn span(i: u32, out: u32, source: u32) -> (u32, u32) {
    let start = i * source / out;
    let end = ((i + 1) * source / out).clamp(start + 1, source);
    (start, end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumbnails_shrink_and_keep_their_shape() {
        let rgba = vec![255; 200 * 100 * 4];
        let image = shrink(200, 100, &rgba, 96).unwrap();
        assert_eq!(image.size, [96, 48]);
        assert!(shrink(0, 10, &[], 96).is_none());
    }

    #[test]
    fn thumbnails_are_kept_while_their_files_exist() {
        let folder = std::env::temp_dir().join(format!("axcenity-thumbs-{}", std::process::id()));
        std::fs::create_dir_all(folder.join("assets/set")).unwrap();
        std::fs::write(folder.join("assets/set/color.png"), b"").unwrap();
        let mut thumbnails = Thumbnails::default();
        thumbnails.open(Some(&folder));
        thumbnails.done.insert("assets/set/color.png".into(), None);
        thumbnails.done.insert("assets/gone.png".into(), None);
        thumbnails.forget_missing();
        let _ = std::fs::remove_dir_all(&folder);
        assert!(thumbnails.done.contains_key("assets/set/color.png"));
        assert!(!thumbnails.done.contains_key("assets/gone.png"));
    }

    #[test]
    fn shrinking_averages_what_each_pixel_covers() {
        // A 2×2 checker of black and white becomes one grey pixel.
        let rgba = [
            0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 255,
        ];
        let image = shrink(2, 2, &rgba, 1).unwrap();
        assert_eq!(image.pixels[0], egui::Color32::from_gray(127));
    }
}
