//! Cooked textures (roadmap L2): a texture file's mip levels made ahead of
//! time in a GPU block format, so loading it is a file read and an upload.
//!
//! The editor cooks (the `axcenity-cook` crate, which holds the encoder);
//! everything else only reads. A cooked file sits in this computer's
//! `.axcenity/imported/textures/` (see [`cooked_path`]) and remembers the
//! source file it was made from ([`SourceStamp`]), so an edited texture is
//! never shown stale: [`read_cooked`] gives `None` and the source is decoded
//! as before. A cooked [`Image`] decodes its source's pixels only if
//! something on the CPU asks for them ([`Image::rgba`]).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::model::{Image, ImageOrigin, PixelSource};
use crate::safe_path;

/// Where cooked textures go, inside the project folder. Like the rest of
/// `.axcenity/`, it's this computer's: not exported or backed up.
pub const COOKED_FOLDER: &str = ".axcenity/imported/textures";

/// The file extension of a cooked texture.
pub const COOKED_EXTENSION: &str = "axtex";

/// "AXTEX", then the format version.
const MAGIC: &[u8; 8] = b"AXTEX\0\0\x01";

/// The GPU block formats a texture is cooked into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CookedFormat {
    /// BC7: 16 bytes for each 4×4 block of RGBA, a quarter of RGBA8. Read
    /// through an sRGB view for colors and a plain one for values, like
    /// RGBA8. Every desktop GPU has it.
    Bc7,
    /// BC5: 16 bytes for each 4×4 block of two channels, red and green,
    /// each kept with twice BC7's care. For normal maps: blue is worked out
    /// from them when drawn (a unit normal's X and Y give its Z).
    Bc5,
}

impl CookedFormat {
    fn code(self) -> u32 {
        match self {
            CookedFormat::Bc7 => 1,
            CookedFormat::Bc5 => 2,
        }
    }

    fn from_code(code: u32) -> Option<CookedFormat> {
        match code {
            1 => Some(CookedFormat::Bc7),
            2 => Some(CookedFormat::Bc5),
            _ => None,
        }
    }

    /// The bytes of a `width`×`height` level: every 4×4 block, a part
    /// block at an edge counting whole.
    pub fn level_bytes(self, width: u32, height: u32) -> usize {
        match self {
            CookedFormat::Bc7 | CookedFormat::Bc5 => {
                width.div_ceil(4) as usize * height.div_ceil(4) as usize * 16
            }
        }
    }
}

/// A texture's mip levels in a block format, largest first, each half the
/// last (at least 1), down to 1×1.
pub struct CookedTexture {
    pub format: CookedFormat,
    pub width: u32,
    pub height: u32,
    pub levels: Vec<Vec<u8>>,
    /// Whether any pixel of the source is see-through (alpha below 255).
    pub has_alpha: bool,
    /// The source's [`sampled_average`], so loading a model can tell a
    /// normal map from a bump map without decoding it.
    pub average: [u8; 4],
}

impl CookedTexture {
    /// The size of mip `level`.
    pub fn level_size(&self, level: usize) -> (u32, u32) {
        ((self.width >> level).max(1), (self.height >> level).max(1))
    }

    /// How many levels a `width`×`height` texture has, down to 1×1.
    pub fn level_count(width: u32, height: u32) -> usize {
        (32 - width.max(height).max(1).leading_zeros()) as usize
    }

    /// Whether the levels are the right number and size for the format.
    fn is_whole(&self) -> bool {
        self.width > 0
            && self.height > 0
            && self.levels.len() == Self::level_count(self.width, self.height)
            && self.levels.iter().enumerate().all(|(level, bytes)| {
                let (width, height) = self.level_size(level);
                bytes.len() == self.format.level_bytes(width, height)
            })
    }
}

/// The source file a cooked texture was made from: its length and modified
/// time, which are checked first, and a hash of its bytes, which decides
/// when only the time differs (a copied or unpacked project).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceStamp {
    pub len: u64,
    /// Nanoseconds since 1970, or 0 when the system doesn't say.
    pub modified: u64,
    pub hash: u64,
}

impl SourceStamp {
    /// The stamp of a source file's `bytes`, read at `path`.
    pub fn of(path: &Path, bytes: &[u8]) -> SourceStamp {
        SourceStamp {
            len: bytes.len() as u64,
            modified: fs::metadata(path).map_or(0, |meta| modified_nanos(&meta)),
            hash: content_hash(bytes),
        }
    }
}

fn modified_nanos(meta: &fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |since| since.as_nanos() as u64)
}

/// A quick 64-bit hash of `bytes`, eight at a time, to tell whether a file
/// changed. Not for anything an attacker picks.
pub fn content_hash(bytes: &[u8]) -> u64 {
    let (words, rest) = bytes.as_chunks::<8>();
    let mut hasher = ContentHasher::new(bytes.len() as u64);
    for word in words {
        hasher.word(u64::from_le_bytes(*word));
    }
    let mut last = [0u8; 8];
    last[..rest.len()].copy_from_slice(rest);
    hasher.word(u64::from_le_bytes(last));
    hasher.finish()
}

/// [`content_hash`] fed a word at a time, for data that isn't one slice
/// of bytes (such as a mesh's corners and triangles).
pub(crate) struct ContentHasher(u64);

impl ContentHasher {
    const K: u64 = 0x9E37_79B9_7F4A_7C15;

    /// Starts a hash of data `len` long.
    pub(crate) fn new(len: u64) -> Self {
        Self(len.wrapping_mul(Self::K))
    }

    pub(crate) fn word(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(23) ^ word).wrapping_mul(Self::K);
    }

    /// Two 32-bit values as one word.
    pub(crate) fn pair(&mut self, low: u32, high: u32) {
        self.word(u64::from(low) | (u64::from(high) << 32));
    }

    pub(crate) fn finish(self) -> u64 {
        // Spread the last words' bits through the whole hash.
        let mut hash = self.0;
        hash ^= hash >> 29;
        hash = hash.wrapping_mul(Self::K);
        hash ^ (hash >> 32)
    }
}

/// `name` with anything but letters, digits, `-`, `_` and `.` made `_`,
/// to start a cached file's name.
pub(crate) fn plain_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "-_.".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Where the cooked copy of the project file `relative` goes: the file's
/// name and a hash of its whole path, so two `Color.png`s in different
/// folders don't meet.
pub fn cooked_path(project: &Path, relative: &str) -> PathBuf {
    project
        .join(COOKED_FOLDER)
        .join(format!("{}.{COOKED_EXTENSION}", cache_stem(relative)))
}

/// How every cached file made from the project file `relative` starts: its
/// name and a hash of its whole path (`Color.png-0123456789abcdef`).
pub(crate) fn cache_stem(relative: &str) -> String {
    let relative = relative.replace('\\', "/");
    let name = relative.rsplit('/').next().unwrap_or_default();
    let name: String = plain_name(name).chars().take(64).collect();
    let hash = content_hash(relative.to_lowercase().as_bytes());
    format!("{name}-{hash:016x}")
}

/// Where the cooked copy of `relative` goes inside the project, with `/`
/// between folders: [`cooked_path`] without the project folder, for
/// putting it in a packed game.
pub fn cooked_relative_path(relative: &str) -> String {
    cooked_path(Path::new(""), relative)
        .to_string_lossy()
        .replace('\\', "/")
}

/// The cooked file's bytes: [`MAGIC`], the stamp, the format, size and
/// facts, each level's length, then the levels. Little-endian.
pub fn encode_cooked(stamp: SourceStamp, texture: &CookedTexture) -> Vec<u8> {
    let data: usize = texture.levels.iter().map(Vec::len).sum();
    let mut out = Vec::with_capacity(64 + texture.levels.len() * 8 + data);
    out.extend_from_slice(MAGIC);
    for value in [stamp.len, stamp.modified, stamp.hash] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    for value in [
        texture.format.code(),
        texture.width,
        texture.height,
        u32::from(texture.has_alpha),
        texture.levels.len() as u32,
    ] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out.extend_from_slice(&texture.average);
    for level in &texture.levels {
        out.extend_from_slice(&(level.len() as u64).to_le_bytes());
    }
    for level in &texture.levels {
        out.extend_from_slice(level);
    }
    out
}

/// Reads [`encode_cooked`]'s bytes back, or says why they're not a whole
/// cooked texture.
pub fn decode_cooked(bytes: &[u8]) -> Result<(SourceStamp, CookedTexture), String> {
    let mut at = 0;
    let mut take = |count: usize| -> Result<&[u8], String> {
        let part = bytes
            .get(at..at + count)
            .ok_or("the cooked texture is cut short")?;
        at += count;
        Ok(part)
    };
    if take(MAGIC.len())? != MAGIC {
        return Err("it isn't a cooked texture of this version".to_owned());
    }
    let mut u64s = [0u64; 3];
    for value in &mut u64s {
        *value = u64::from_le_bytes(take(8)?.try_into().expect("8 bytes"));
    }
    let mut u32s = [0u32; 5];
    for value in &mut u32s {
        *value = u32::from_le_bytes(take(4)?.try_into().expect("4 bytes"));
    }
    let average: [u8; 4] = take(4)?.try_into().expect("4 bytes");
    let [format, width, height, has_alpha, count] = u32s;
    let format = CookedFormat::from_code(format).ok_or("its format isn't known")?;
    if count > 32 {
        return Err("it has too many levels".to_owned());
    }
    let mut lengths = Vec::with_capacity(count as usize);
    for _ in 0..count {
        lengths.push(u64::from_le_bytes(take(8)?.try_into().expect("8 bytes")));
    }
    let mut levels = Vec::with_capacity(count as usize);
    for length in lengths {
        let length = usize::try_from(length).map_err(|_| "a level is too big")?;
        levels.push(take(length)?.to_vec());
    }
    let texture = CookedTexture {
        format,
        width,
        height,
        levels,
        has_alpha: has_alpha != 0,
        average,
    };
    if !texture.is_whole() {
        return Err("its levels don't match its size".to_owned());
    }
    let [len, modified, hash] = u64s;
    Ok((
        SourceStamp {
            len,
            modified,
            hash,
        },
        texture,
    ))
}

/// Writes the cooked copy of `relative` (see [`cooked_path`]). It's written
/// beside and then renamed into place, so a reader never sees half of it.
pub fn write_cooked(
    project: &Path,
    relative: &str,
    stamp: SourceStamp,
    texture: &CookedTexture,
) -> io::Result<()> {
    write_whole(
        &cooked_path(project, relative),
        &encode_cooked(stamp, texture),
    )
}

/// Writes `bytes` to `path` (making its folder) so that a reader finds the
/// old file or the whole new one, never part of it.
pub(crate) fn write_whole(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    // Named for this writer, so two cooking the same file at once (the
    // editor's thread and Build Game) don't write into one file.
    static WRITES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let write = WRITES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let partial = path.with_extension(format!("{}-{write}.partial", std::process::id()));
    fs::write(&partial, bytes)?;
    fs::rename(&partial, path).inspect_err(|_| {
        let _ = fs::remove_file(&partial);
    })
}

/// The cooked copy of the project file `relative`, if there's one made from
/// the file as it is now. Anything else (none, damaged, or from an older
/// version of the file) is `None`, and the file is decoded instead.
pub fn read_cooked(project: &Path, relative: &str) -> Option<CookedTexture> {
    let bytes = fs::read(cooked_path(project, relative)).ok()?;
    let (stamp, texture) = decode_cooked(&bytes).ok()?;
    is_current(project, relative, stamp).then_some(texture)
}

/// Whether `relative` has a current cooked copy. Only the cooked file's
/// start is read.
pub fn is_cooked(project: &Path, relative: &str) -> bool {
    cooked_format(project, relative).is_some()
}

/// The format of `relative`'s current cooked copy, if it has one. Only the
/// cooked file's start is read.
pub fn cooked_format(project: &Path, relative: &str) -> Option<CookedFormat> {
    use std::io::Read;
    let mut start = [0u8; MAGIC.len() + 24 + 4];
    let read = fs::File::open(cooked_path(project, relative))
        .and_then(|mut file| file.read_exact(&mut start));
    if read.is_err() || &start[..MAGIC.len()] != MAGIC {
        return None;
    }
    let word = |at: usize| {
        let at = MAGIC.len() + at * 8;
        u64::from_le_bytes(start[at..at + 8].try_into().expect("8 bytes"))
    };
    let stamp = SourceStamp {
        len: word(0),
        modified: word(1),
        hash: word(2),
    };
    let at = MAGIC.len() + 24;
    let format = CookedFormat::from_code(u32::from_le_bytes(
        start[at..at + 4].try_into().expect("4 bytes"),
    ))?;
    is_current(project, relative, stamp).then_some(format)
}

/// Whether the project file `relative` is still what `stamp` was made from.
pub(crate) fn is_current(project: &Path, relative: &str, stamp: SourceStamp) -> bool {
    let Some(source) = safe_path(relative).map(|path| project.join(path)) else {
        return false;
    };
    let Ok(meta) = fs::metadata(&source) else {
        return false;
    };
    stamp.len == meta.len()
        && (stamp.modified == modified_nanos(&meta)
            || fs::read(&source).is_ok_and(|bytes| content_hash(&bytes) == stamp.hash))
}

/// The project file `relative` as a cooked [`Image`], if it has a current
/// cooked copy. Its pixels are decoded from the file only if asked for.
pub(crate) fn cooked_image(project: &Path, relative: &str) -> Option<Image> {
    let texture = read_cooked(project, relative)?;
    let source = project.join(safe_path(relative)?);
    let origin = ImageOrigin::File(relative.replace('\\', "/"));
    Some(Image::from_cooked(texture, PixelSource::File(source)).with_origin(origin))
}

/// A texture embedded in a model as `bytes`, from the cooked copy of the
/// project file `relative` that importing copied those bytes out to, if
/// that copy was made from the same bytes and the file is still there.
pub(crate) fn cooked_embedded_image(project: &Path, relative: &str, bytes: &[u8]) -> Option<Image> {
    let source = project.join(safe_path(relative)?);
    if !source.is_file() {
        return None;
    }
    let cooked = fs::read(cooked_path(project, relative)).ok()?;
    let (stamp, texture) = decode_cooked(&cooked).ok()?;
    let origin = ImageOrigin::CookedEmbedded {
        path: relative.replace('\\', "/"),
        hash: stamp.hash,
    };
    (stamp.len == bytes.len() as u64 && stamp.hash == content_hash(bytes))
        .then(|| Image::from_cooked(texture, PixelSource::File(source)).with_origin(origin))
}

/// The cooked copy of the project file `relative`, if it was made from
/// bytes with this `hash` (an embedded texture's, see
/// [`cooked_embedded_image`]) and that file is still there.
pub(crate) fn cooked_embedded_by_hash(project: &Path, relative: &str, hash: u64) -> Option<Image> {
    let source = project.join(safe_path(relative)?);
    if !source.is_file() {
        return None;
    }
    let cooked = fs::read(cooked_path(project, relative)).ok()?;
    let (stamp, texture) = decode_cooked(&cooked).ok()?;
    let origin = ImageOrigin::CookedEmbedded {
        path: relative.replace('\\', "/"),
        hash,
    };
    (stamp.hash == hash)
        .then(|| Image::from_cooked(texture, PixelSource::File(source)).with_origin(origin))
}

/// Where the cooked copy of an image embedded in the project model file
/// `model` goes, by the hash of the image's bytes: after the model's own
/// [`cache_stem`], so it's kept and deleted with the model's other cached
/// files.
pub fn embedded_cooked_path(project: &Path, model: &str, hash: u64) -> PathBuf {
    project.join(COOKED_FOLDER).join(format!(
        "{}-{hash:016x}.{COOKED_EXTENSION}",
        cache_stem(model)
    ))
}

/// [`embedded_cooked_path`] without the project folder, with `/` between
/// folders, for putting it in a packed game.
pub fn embedded_cooked_relative_path(model: &str, hash: u64) -> String {
    embedded_cooked_path(Path::new(""), model, hash)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Writes the cooked copy of the image `bytes` embedded in the project
/// model file `model`. It's stamped with the image's own bytes (length and
/// hash), not the model's: the copy stays current whatever else in the
/// model changes.
pub fn write_embedded_cooked(
    project: &Path,
    model: &str,
    bytes: &[u8],
    texture: &CookedTexture,
) -> io::Result<()> {
    let stamp = SourceStamp {
        len: bytes.len() as u64,
        modified: 0,
        hash: content_hash(bytes),
    };
    write_whole(
        &embedded_cooked_path(project, model, stamp.hash),
        &encode_cooked(stamp, texture),
    )
}

/// The format of the cooked copy of the image `bytes` embedded in `model`,
/// if it has one. Only the cooked file's start is read.
pub fn embedded_cooked_format(project: &Path, model: &str, bytes: &[u8]) -> Option<CookedFormat> {
    use std::io::Read;
    let hash = content_hash(bytes);
    let mut start = [0u8; MAGIC.len() + 24 + 4];
    fs::File::open(embedded_cooked_path(project, model, hash))
        .and_then(|mut file| file.read_exact(&mut start))
        .ok()?;
    let word = |at: usize| {
        let at = MAGIC.len() + at * 8;
        u64::from_le_bytes(start[at..at + 8].try_into().expect("8 bytes"))
    };
    let at = MAGIC.len() + 24;
    let format = u32::from_le_bytes(start[at..at + 4].try_into().expect("4 bytes"));
    (&start[..MAGIC.len()] == MAGIC && word(0) == bytes.len() as u64 && word(2) == hash)
        .then(|| CookedFormat::from_code(format))
        .flatten()
}

/// The image `bytes` embedded in the project model file `model`, from its
/// own cooked copy (see [`embedded_cooked_path`]), if there's one made from
/// those bytes. Its pixels are decoded from the model only if asked for.
pub(crate) fn cooked_in_model(project: &Path, model: &str, bytes: &[u8]) -> Option<Image> {
    embedded_cooked_format(project, model, bytes)?;
    cooked_in_model_by_hash(project, model, content_hash(bytes))
}

/// The cooked copy of an image embedded in `model`, made from bytes with
/// this `hash`, if there's one and the model is still there.
pub(crate) fn cooked_in_model_by_hash(project: &Path, model: &str, hash: u64) -> Option<Image> {
    let source = project.join(safe_path(model)?);
    if !source.is_file() {
        return None;
    }
    let cooked = fs::read(embedded_cooked_path(project, model, hash)).ok()?;
    let (stamp, texture) = decode_cooked(&cooked).ok()?;
    (stamp.hash == hash).then(|| {
        Image::from_cooked(
            texture,
            PixelSource::InModel {
                model: source,
                hash,
            },
        )
        .with_origin(ImageOrigin::InModel { hash })
    })
}

/// The cooked copies of images embedded in the project model file `model`
/// (see [`embedded_cooked_path`]), as project paths with `/` between
/// folders.
pub fn embedded_cooked_files(project: &Path, model: &str) -> Vec<String> {
    let prefix = format!("{}-", cache_stem(model));
    let end = format!(".{COOKED_EXTENSION}");
    let Ok(entries) = fs::read_dir(project.join(COOKED_FOLDER)) else {
        return Vec::new();
    };
    let mut found: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_owned();
            (name.starts_with(&prefix) && name.ends_with(&end))
                .then(|| format!("{COOKED_FOLDER}/{name}"))
        })
        .collect();
    found.sort();
    found
}

/// Where the note that a model's embedded images are all cooked goes (see
/// [`mark_embedded_cooked`]).
fn embedded_done_path(project: &Path, model: &str) -> PathBuf {
    project
        .join(COOKED_FOLDER)
        .join(format!("{}-embedded.done", cache_stem(model)))
}

/// Notes that every image embedded in the project model file `model`, as
/// it is now (`stamp`), has a cooked copy: those made from bytes with
/// these `hashes`. Cooking can then skip reading the model until it
/// changes or one of the copies goes.
pub fn mark_embedded_cooked(
    project: &Path,
    model: &str,
    stamp: SourceStamp,
    hashes: &[u64],
) -> io::Result<()> {
    let mut out = MAGIC.to_vec();
    for value in [stamp.len, stamp.modified, stamp.hash] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    for hash in hashes {
        out.extend_from_slice(&hash.to_le_bytes());
    }
    write_whole(&embedded_done_path(project, model), &out)
}

/// Whether [`mark_embedded_cooked`] noted `model` as it is now, and each
/// copy it named is still there.
pub fn embedded_cooked_done(project: &Path, model: &str) -> bool {
    let Ok(bytes) = fs::read(embedded_done_path(project, model)) else {
        return false;
    };
    let header = MAGIC.len() + 24;
    if bytes.len() < header || &bytes[..MAGIC.len()] != MAGIC {
        return false;
    }
    let words: Vec<u64> = bytes[MAGIC.len()..]
        .chunks_exact(8)
        .map(|word| u64::from_le_bytes(word.try_into().expect("8 bytes")))
        .collect();
    let stamp = SourceStamp {
        len: words[0],
        modified: words[1],
        hash: words[2],
    };
    is_current(project, model, stamp)
        && words[3..]
            .iter()
            .all(|&hash| embedded_cooked_path(project, model, hash).is_file())
}

/// The average of about 4,096 pixels spread over `rgba` (every pixel of a
/// small image), for telling what a map holds at a glance.
pub fn sampled_average(rgba: &[u8]) -> [u8; 4] {
    let pixels = rgba.as_chunks::<4>().0;
    let step = (pixels.len() / 4096).max(1);
    let mut sums = [0u64; 4];
    let mut count = 0u64;
    for pixel in pixels.iter().step_by(step) {
        for (sum, &value) in sums.iter_mut().zip(pixel) {
            *sum += u64::from(value);
        }
        count += 1;
    }
    sums.map(|sum| (sum / count.max(1)) as u8)
}

/// Whether any pixel of `rgba` is see-through.
pub fn has_alpha(rgba: &[u8]) -> bool {
    rgba.as_chunks::<4>().0.iter().any(|pixel| pixel[3] < 255)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checker(width: u32, height: u32) -> CookedTexture {
        let levels = (0..CookedTexture::level_count(width, height))
            .map(|level| {
                let (w, h) = ((width >> level).max(1), (height >> level).max(1));
                vec![level as u8; CookedFormat::Bc7.level_bytes(w, h)]
            })
            .collect();
        CookedTexture {
            format: CookedFormat::Bc7,
            width,
            height,
            levels,
            has_alpha: true,
            average: [1, 2, 3, 4],
        }
    }

    fn folder(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("axcenity-cooked-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(folder.join("assets/textures")).unwrap();
        folder
    }

    #[test]
    fn a_cooked_texture_reads_back_as_written() {
        let texture = checker(64, 16);
        let stamp = SourceStamp {
            len: 10,
            modified: 20,
            hash: 30,
        };
        let (read_stamp, read) = decode_cooked(&encode_cooked(stamp, &texture)).unwrap();
        assert_eq!(read_stamp, stamp);
        assert_eq!(read.levels, texture.levels);
        assert_eq!((read.width, read.height), (64, 16));
        assert_eq!(read.levels.len(), 7, "64, 32, 16, 8, 4, 2, 1 wide");
        assert!(read.has_alpha);
        assert_eq!(read.average, [1, 2, 3, 4]);
    }

    #[test]
    fn a_cut_or_mismatched_cooked_file_is_refused() {
        let bytes = encode_cooked(
            SourceStamp {
                len: 1,
                modified: 1,
                hash: 1,
            },
            &checker(8, 8),
        );
        assert!(decode_cooked(&bytes[..bytes.len() - 1]).is_err());
        assert!(decode_cooked(b"PNG").is_err());
        let mut wrong = checker(8, 8);
        wrong.levels.pop();
        let bytes = encode_cooked(
            SourceStamp {
                len: 1,
                modified: 1,
                hash: 1,
            },
            &wrong,
        );
        assert!(decode_cooked(&bytes).is_err());
    }

    #[test]
    fn a_cooked_copy_is_used_only_while_its_source_is_unchanged() {
        let project = folder("current");
        let relative = "assets/textures/wood.png";
        let source = project.join(relative);
        fs::write(&source, b"first version").unwrap();
        let stamp = SourceStamp::of(&source, b"first version");
        write_cooked(&project, relative, stamp, &checker(8, 8)).unwrap();
        assert!(is_cooked(&project, relative));

        // Copied or unpacked: a new time, the same bytes. Still current.
        let moved = SourceStamp {
            modified: stamp.modified ^ 1,
            ..stamp
        };
        write_cooked(&project, relative, moved, &checker(8, 8)).unwrap();
        assert!(is_cooked(&project, relative));

        // Edited, same length: the hash decides.
        fs::write(&source, b"other version").unwrap();
        let mut stale = SourceStamp::of(&source, b"first version");
        stale.modified ^= 1;
        write_cooked(&project, relative, stale, &checker(8, 8)).unwrap();
        assert!(!is_cooked(&project, relative));

        fs::remove_file(&source).unwrap();
        assert!(!is_cooked(&project, relative), "no source, no cooked copy");
        let _ = fs::remove_dir_all(project);
    }

    #[test]
    fn a_bc5_copy_reads_back_and_says_its_format() {
        let project = folder("bc5");
        let relative = "assets/textures/Wood_Normal.png";
        let source = project.join(relative);
        fs::write(&source, b"normal map").unwrap();
        let mut texture = checker(8, 8);
        texture.format = CookedFormat::Bc5;
        let stamp = SourceStamp::of(&source, b"normal map");
        write_cooked(&project, relative, stamp, &texture).unwrap();
        assert_eq!(cooked_format(&project, relative), Some(CookedFormat::Bc5));
        let read = read_cooked(&project, relative).unwrap();
        assert_eq!(read.format, CookedFormat::Bc5);
        assert_eq!(read.levels, texture.levels);

        fs::write(&source, b"edited map").unwrap();
        assert_eq!(cooked_format(&project, relative), None, "out of date");
        let _ = fs::remove_dir_all(project);
    }

    #[test]
    fn cooked_paths_differ_by_folder_and_stay_inside_the_cache() {
        let project = Path::new("project");
        let a = cooked_path(project, "assets/a/Color.png");
        let b = cooked_path(project, "assets/b/Color.png");
        assert_ne!(a, b);
        assert!(a.starts_with(project.join(COOKED_FOLDER)));
        assert_eq!(a, cooked_path(project, "assets\\a\\Color.png"));
        assert_eq!(
            Path::new("project").join(cooked_relative_path("assets/a/Color.png")),
            a
        );
        assert!(cooked_relative_path("assets/a/Color.png").starts_with(".axcenity/imported/"));
        let name = a.file_name().unwrap().to_str().unwrap();
        assert!(
            name.starts_with("Color.png-") && name.ends_with(".axtex"),
            "{name}"
        );
    }

    #[test]
    fn the_content_hash_sees_every_byte() {
        let base = vec![7u8; 1001];
        let hash = content_hash(&base);
        for at in [0, 500, 999, 1000] {
            let mut changed = base.clone();
            changed[at] ^= 1;
            assert_ne!(content_hash(&changed), hash, "byte {at}");
        }
        assert_ne!(content_hash(&base[..1000]), hash);
    }
}
