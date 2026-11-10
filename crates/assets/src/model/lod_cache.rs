//! The mesh cache (roadmap L2): a project model's simplified copies
//! ([`super::lod`]), kept in `.axcenity/imported/meshes/` so the next load
//! reads their triangles instead of simplifying again.
//!
//! A cached file is named for the model file and a hash of the full
//! primitives it was made from (after fitting, so each pick of objects
//! from the file has its own), and holds each primitive's simpler copies
//! as lists of its corners. It's stamped with the model file it came from:
//! writing a new one deletes the model's files whose stamp no longer
//! matches, and Build Game ships only current ones.

use std::fs;
use std::path::{Path, PathBuf};

use super::InProject;
use super::stamped::Cache;
use crate::cooked::{self, SourceStamp};

/// Where cached simpler copies go, inside the project folder.
pub const MESH_CACHE_FOLDER: &str = ".axcenity/imported/meshes";

const CACHE: Cache = Cache {
    folder: MESH_CACHE_FOLDER,
    extension: "axlod",
    magic: b"AXLOD\0\0\x01",
};

/// The magic, the stamp's three words and the key.
const HEADER: usize = 8 + 4 * 8;

/// Simpler copies of each primitive, as lists of its corners (three a
/// triangle), simplest last.
pub(super) type Levels = Vec<Vec<Vec<u32>>>;

fn path(at: InProject<'_>, key: u64) -> PathBuf {
    CACHE.path(at.project, at.relative, &format!("{key:016x}"))
}

/// The cached copies for `key`, if there are some for primitives with
/// `corners` corners each. A missing, damaged or mismatched file is `None`.
pub(super) fn read(at: InProject<'_>, key: u64, corners: &[usize]) -> Option<Levels> {
    let bytes = fs::read(path(at, key)).ok()?;
    decode(&bytes, key, corners)
}

/// Keeps `levels` for `key`, and deletes the model's cached files made
/// from an older version of it. Failing (a read-only folder) only means
/// the next load simplifies again.
pub(super) fn write(at: InProject<'_>, key: u64, levels: &Levels) {
    let path = path(at, key);
    let source = at.project.join(at.relative);
    let stamp = SourceStamp::of(&source, at.bytes);
    if cooked::write_whole(&path, &encode(stamp, key, levels)).is_ok() {
        CACHE.clear_stale(at.project, at.relative, &path);
    }
}

/// The cached files of the project model `relative` made from it as it is
/// now, as project paths with `/` between folders: what Build Game ships
/// with it.
pub fn cached_meshes(project: &Path, relative: &str) -> Vec<String> {
    CACHE.current(project, relative)
}

/// The magic, the stamp, the key, then the primitive count and for each
/// its corner count, level count, and each level's length and corners.
/// Numbers are little-endian.
fn encode(stamp: SourceStamp, key: u64, levels: &Levels) -> Vec<u8> {
    let corners: usize = levels.iter().flatten().map(Vec::len).sum();
    let mut out = Vec::with_capacity(HEADER + 8 + corners * 4 + levels.len() * 16);
    out.extend_from_slice(&CACHE.header(stamp));
    out.extend_from_slice(&key.to_le_bytes());
    out.extend_from_slice(&(levels.len() as u32).to_le_bytes());
    for primitive in levels {
        // The corner count each level's indices are checked against.
        let count = primitive.iter().flatten().max().map_or(0, |&most| most + 1);
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&(primitive.len() as u32).to_le_bytes());
        for level in primitive {
            out.extend_from_slice(&(level.len() as u32).to_le_bytes());
            for index in level {
                out.extend_from_slice(&index.to_le_bytes());
            }
        }
    }
    out
}

/// [`encode`]'s bytes back, if they're whole, for `key`, and each list
/// only names corners its primitive has (`corners`).
fn decode(bytes: &[u8], key: u64, corners: &[usize]) -> Option<Levels> {
    CACHE.stamp(bytes)?;
    let start = CACHE.header_len();
    if bytes.get(start..HEADER)? != key.to_le_bytes() {
        return None;
    }
    let mut at = HEADER;
    let mut next = || {
        let value = u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?);
        at += 4;
        Some(value)
    };
    let count = next()? as usize;
    if count != corners.len() {
        return None;
    }
    let mut levels = Vec::with_capacity(count);
    for &corners in corners {
        let used = next()? as usize;
        if used > corners {
            return None;
        }
        let level_count = next()?;
        if level_count > u32::from(super::lod::LEVELS) {
            return None;
        }
        let mut primitive = Vec::new();
        for _ in 0..level_count {
            let len = next()? as usize;
            if !len.is_multiple_of(3) || len > bytes.len() / 4 {
                return None;
            }
            let level: Vec<u32> = (0..len).map(|_| next()).collect::<Option<_>>()?;
            if level.iter().any(|&index| index as usize >= corners) {
                return None;
            }
            primitive.push(level);
        }
        levels.push(primitive);
    }
    Some(levels)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stamp() -> SourceStamp {
        SourceStamp {
            len: 3,
            modified: 4,
            hash: 5,
        }
    }

    #[test]
    fn cached_copies_read_back_only_for_their_key_and_corners() {
        let levels = vec![vec![vec![0, 1, 2, 2, 1, 3], vec![0, 1, 2]], vec![]];
        let bytes = encode(stamp(), 7, &levels);
        assert_eq!(decode(&bytes, 7, &[4, 9]), Some(levels.clone()));
        assert_eq!(decode(&bytes, 8, &[4, 9]), None, "another key");
        assert_eq!(decode(&bytes, 7, &[3, 9]), None, "too few corners");
        assert_eq!(decode(&bytes, 7, &[4]), None, "another primitive count");
        for cut in [0, 10, HEADER, bytes.len() - 1] {
            assert_eq!(decode(&bytes[..cut], 7, &[4, 9]), None, "cut at {cut}");
        }
    }
}
