//! Files a project's model is cached as in `.axcenity/imported/` (its
//! simpler copies, [`super::lod_cache`]; the whole model,
//! [`super::cooked_model`]), each starting with its magic and the
//! [`SourceStamp`] of the model file it was made from.

use std::fs;
use std::path::{Path, PathBuf};

use crate::cooked::{self, SourceStamp};

/// One kind of cached file.
pub(super) struct Cache {
    /// Inside the project folder.
    pub folder: &'static str,
    pub extension: &'static str,
    pub magic: &'static [u8; 8],
}

impl Cache {
    /// Where the cached file `name_end` of the model file `relative` goes.
    /// Each of a model's files starts with its name and a hash of its whole
    /// path, so two `rock.glb`s in different folders don't meet.
    pub fn path(&self, project: &Path, relative: &str, name_end: &str) -> PathBuf {
        project
            .join(self.folder)
            .join(format!("{}{name_end}.{}", prefix(relative), self.extension))
    }

    /// The bytes before a file's own: the magic and `stamp`.
    pub fn header(&self, stamp: SourceStamp) -> Vec<u8> {
        let mut out = self.magic.to_vec();
        for value in [stamp.len, stamp.modified, stamp.hash] {
            out.extend_from_slice(&value.to_le_bytes());
        }
        out
    }

    /// How many bytes [`header`](Self::header) takes.
    pub fn header_len(&self) -> usize {
        self.magic.len() + 3 * 8
    }

    /// The stamp in `bytes`' header, if they start with this cache's magic.
    pub fn stamp(&self, bytes: &[u8]) -> Option<SourceStamp> {
        if bytes.len() < self.header_len() || &bytes[..self.magic.len()] != self.magic {
            return None;
        }
        let word = |index: usize| {
            let at = self.magic.len() + index * 8;
            u64::from_le_bytes(bytes[at..at + 8].try_into().expect("8 bytes"))
        };
        Some(SourceStamp {
            len: word(0),
            modified: word(1),
            hash: word(2),
        })
    }

    /// Deletes the model `relative`'s cached files, other than `keep`,
    /// made from an older version of it.
    pub fn clear_stale(&self, project: &Path, relative: &str, keep: &Path) {
        for (other, stamp) in self.files(project, relative) {
            if other != keep && !cooked::is_current(project, relative, stamp) {
                let _ = fs::remove_file(other);
            }
        }
    }

    /// The model `relative`'s cached files made from it as it is now, as
    /// project paths with `/` between folders.
    pub fn current(&self, project: &Path, relative: &str) -> Vec<String> {
        let mut found: Vec<String> = self
            .files(project, relative)
            .into_iter()
            .filter(|(_, stamp)| cooked::is_current(project, relative, *stamp))
            .filter_map(|(path, _)| {
                let name = path.file_name()?.to_str()?;
                Some(format!("{}/{name}", self.folder))
            })
            .collect();
        found.sort();
        found
    }

    /// The model `relative`'s cached files, each with its stamp.
    fn files(&self, project: &Path, relative: &str) -> Vec<(PathBuf, SourceStamp)> {
        let prefix = prefix(relative);
        let end = format!(".{}", self.extension);
        let Ok(entries) = fs::read_dir(project.join(self.folder)) else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter(|entry| {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                name.starts_with(&prefix) && name.ends_with(&end)
            })
            .filter_map(|entry| Some((entry.path(), self.read_stamp(&entry.path())?)))
            .collect()
    }

    /// The stamp at the start of a cached file, reading only that much.
    fn read_stamp(&self, path: &Path) -> Option<SourceStamp> {
        use std::io::Read;
        let mut start = vec![0u8; self.header_len()];
        fs::File::open(path).ok()?.read_exact(&mut start).ok()?;
        self.stamp(&start)
    }
}

/// The start of every cached file of the model file `relative`.
fn prefix(relative: &str) -> String {
    format!("{}-", cooked::cache_stem(relative))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_keep_two_models_of_one_name_apart() {
        assert_ne!(prefix("assets/a/rock.glb"), prefix("assets/b/rock.glb"));
        assert!(prefix("assets/a/rock.glb").starts_with("rock.glb-"));
    }
}
