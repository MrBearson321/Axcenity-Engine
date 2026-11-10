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
use crate::cooked::{self, SourceStamp, content_hash, plain_name};

/// Where cached simpler copies go, inside the project folder.
pub const MESH_CACHE_FOLDER: &str = ".axcenity/imported/meshes";

const EXTENSION: &str = "axlod";

const MAGIC: &[u8; 8] = b"AXLOD\0\0\x01";

/// The magic, the stamp's three words and the key.
const HEADER: usize = MAGIC.len() + 4 * 8;

/// Simpler copies of each primitive, as lists of its corners (three a
/// triangle), simplest last.
pub(super) type Levels = Vec<Vec<Vec<u32>>>;

/// The start of every cached file of the model file `relative`: its name
/// and a hash of its whole path.
fn prefix(relative: &str) -> String {
    let relative = relative.replace('\\', "/");
    let name = relative.rsplit('/').next().unwrap_or_default();
    let name: String = plain_name(name).chars().take(64).collect();
    let hash = content_hash(relative.to_lowercase().as_bytes());
    format!("{name}-{hash:016x}-")
}

fn path(at: InProject<'_>, key: u64) -> PathBuf {
    at.project
        .join(MESH_CACHE_FOLDER)
        .join(format!("{}{key:016x}.{EXTENSION}", prefix(at.relative)))
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
    if cooked::write_whole(&path, &encode(stamp, key, levels)).is_err() {
        return;
    }
    for (other, other_stamp) in cached_files(at.project, at.relative) {
        if other != path && !cooked::is_current(at.project, at.relative, other_stamp) {
            let _ = fs::remove_file(other);
        }
    }
}

/// The project file `relative`'s cached files, each with its stamp.
fn cached_files(project: &Path, relative: &str) -> Vec<(PathBuf, SourceStamp)> {
    let prefix = prefix(relative);
    let Ok(entries) = fs::read_dir(project.join(MESH_CACHE_FOLDER)) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with(&prefix) && name.ends_with(&format!(".{EXTENSION}"))
        })
        .filter_map(|entry| Some((entry.path(), read_stamp(&entry.path())?)))
        .collect()
}

/// The stamp at the start of a cached file, reading only that much.
fn read_stamp(path: &Path) -> Option<SourceStamp> {
    use std::io::Read;
    let mut start = [0u8; HEADER];
    fs::File::open(path).ok()?.read_exact(&mut start).ok()?;
    (&start[..MAGIC.len()] == MAGIC).then(|| SourceStamp {
        len: word(&start, 0),
        modified: word(&start, 1),
        hash: word(&start, 2),
    })
}

fn word(header: &[u8], index: usize) -> u64 {
    let at = MAGIC.len() + index * 8;
    u64::from_le_bytes(header[at..at + 8].try_into().expect("8 bytes"))
}

/// The cached files of the project model `relative` made from it as it is
/// now, as project paths with `/` between folders: what Build Game ships
/// with it.
pub fn cached_meshes(project: &Path, relative: &str) -> Vec<String> {
    let mut found: Vec<String> = cached_files(project, relative)
        .into_iter()
        .filter(|(_, stamp)| cooked::is_current(project, relative, *stamp))
        .filter_map(|(path, _)| {
            let name = path.file_name()?.to_str()?;
            Some(format!("{MESH_CACHE_FOLDER}/{name}"))
        })
        .collect();
    found.sort();
    found
}

/// [`MAGIC`], the stamp, the key, then the primitive count and for each
/// its corner count, level count, and each level's length and corners.
/// Numbers are little-endian.
fn encode(stamp: SourceStamp, key: u64, levels: &Levels) -> Vec<u8> {
    let corners: usize = levels.iter().flatten().map(Vec::len).sum();
    let mut out = Vec::with_capacity(HEADER + 8 + corners * 4 + levels.len() * 16);
    out.extend_from_slice(MAGIC);
    for value in [stamp.len, stamp.modified, stamp.hash, key] {
        out.extend_from_slice(&value.to_le_bytes());
    }
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
    if bytes.len() < HEADER || &bytes[..MAGIC.len()] != MAGIC || word(bytes, 3) != key {
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

    #[test]
    fn names_keep_two_models_of_one_name_apart() {
        assert_ne!(prefix("assets/a/rock.glb"), prefix("assets/b/rock.glb"));
        assert!(prefix("assets/a/rock.glb").starts_with("rock.glb-"));
    }
}
