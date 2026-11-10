//! Deleting cached copies whose source file is gone (roadmap L2).
//!
//! Each file in `.axcenity/imported/` (a cooked texture, a model's simpler
//! copies, a whole cooked model) is named after the project file it was
//! made from: [`cache_stem`], its name and a hash of its path. A copy made
//! from a file that was deleted, renamed or moved is never read again, so
//! [`remove_orphaned_copies`] deletes the copies no project file names.

use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

use crate::cooked::{COOKED_FOLDER, cache_stem};
use crate::model::{MESH_CACHE_FOLDER, MODEL_CACHE_FOLDER};

/// Every folder of cached copies named by [`cache_stem`].
const CACHE_FOLDERS: [&str; 3] = [COOKED_FOLDER, MESH_CACHE_FOLDER, MODEL_CACHE_FOLDER];

/// A half-written copy this old was left by a writer that stopped (see
/// `cooked::write_whole`); a younger one may still be being written.
const ABANDONED_AFTER: Duration = Duration::from_secs(60 * 60);

/// Deletes each cached copy in `.axcenity/imported/` whose source file
/// isn't in the project any more, and half-written copies left behind.
/// Returns how many files it deleted.
pub fn remove_orphaned_copies(project: &Path) -> usize {
    let stems = project_stems(project);
    let now = SystemTime::now();
    let mut removed = 0;
    for folder in CACHE_FOLDERS {
        let Ok(entries) = fs::read_dir(project.join(folder)) else {
            continue;
        };
        for entry in entries.flatten() {
            if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_lowercase();
            let orphaned = if name.ends_with(".partial") {
                entry
                    .metadata()
                    .and_then(|meta| meta.modified())
                    .is_ok_and(|modified| {
                        now.duration_since(modified)
                            .is_ok_and(|age| age > ABANDONED_AFTER)
                    })
            } else {
                !stems_in(&name).any(|stem| stems.contains(stem))
            };
            if orphaned && fs::remove_file(entry.path()).is_ok() {
                removed += 1;
            }
        }
    }
    removed
}

/// The [`cache_stem`] of every file in the project, lowercase, leaving out
/// folders whose names start with `.` (`.axcenity/` itself, `.git/`).
fn project_stems(project: &Path) -> HashSet<String> {
    let mut stems = HashSet::new();
    let mut folders = vec![(String::new(), project.to_path_buf())];
    while let Some((relative, folder)) = folders.pop() {
        let Ok(entries) = fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = if relative.is_empty() {
                name.clone()
            } else {
                format!("{relative}/{name}")
            };
            match entry.file_type() {
                Ok(kind) if kind.is_dir() && !name.starts_with('.') => {
                    folders.push((path, entry.path()));
                }
                Ok(kind) if kind.is_file() => {
                    stems.insert(cache_stem(&path).to_lowercase());
                }
                _ => {}
            }
        }
    }
    stems
}

/// Each way a cached file's (lowercase) name can start with a stem: up to a
/// `-` and 16 hex digits followed by `.` or `-`. A file's own name may hold
/// such a run too, so every one is tried.
fn stems_in(name: &str) -> impl Iterator<Item = &str> {
    let bytes = name.as_bytes();
    (0..bytes.len()).filter_map(move |dash| {
        let end = dash + 17;
        let hex = bytes.get(dash + 1..end)?;
        (bytes[dash] == b'-'
            && hex.iter().all(u8::is_ascii_hexdigit)
            && matches!(bytes.get(end), Some(b'.' | b'-')))
        .then(|| &name[..end])
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cooked::cooked_path;

    #[test]
    fn a_stem_is_found_even_after_a_name_that_looks_like_one() {
        let stem = cache_stem("assets/a-0123456789abcdef-b.glb").to_lowercase();
        let name = format!("{stem}-fedcba9876543210.axlod");
        assert!(stems_in(&name).any(|found| found == stem));
        assert_eq!(stems_in("rock.png.axtex").count(), 0);
    }

    #[test]
    fn copies_of_files_that_are_gone_are_deleted_and_the_rest_kept() {
        let project =
            std::env::temp_dir().join(format!("axcenity-orphaned-{}", std::process::id()));
        let _ = fs::remove_dir_all(&project);
        fs::create_dir_all(project.join("assets/textures")).unwrap();
        fs::write(project.join("assets/textures/Kept.png"), b"png").unwrap();
        fs::write(project.join("assets/rock.glb"), b"glb").unwrap();

        let write = |path: &Path| {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"copy").unwrap();
        };
        // Named the way a different case of the path would name it too.
        let kept = cooked_path(&project, "assets/textures/kept.PNG");
        let gone = cooked_path(&project, "assets/textures/gone.png");
        let model = |relative: &str, folder: &str, end: &str| {
            project
                .join(folder)
                .join(format!("{}-{end}", cache_stem(relative)))
        };
        let kept_lod = model(
            "assets/rock.glb",
            MESH_CACHE_FOLDER,
            "00000000000000aa.axlod",
        );
        let kept_model = model(
            "assets/rock.glb",
            MODEL_CACHE_FOLDER,
            "00000000000000bb.axmodel",
        );
        let moved_lod = model(
            "assets/old/rock.glb",
            MESH_CACHE_FOLDER,
            "00000000000000aa.axlod",
        );
        let half_written = project.join(COOKED_FOLDER).join("x.axtex.1-2.partial");
        for path in [
            &kept,
            &gone,
            &kept_lod,
            &kept_model,
            &moved_lod,
            &half_written,
        ] {
            write(path);
        }

        assert_eq!(remove_orphaned_copies(&project), 2);
        for (path, there) in [
            (&kept, true),
            (&gone, false),
            (&kept_lod, true),
            (&kept_model, true),
            (&moved_lod, false),
            (&half_written, true),
        ] {
            assert_eq!(path.exists(), there, "{}", path.display());
        }
        assert_eq!(remove_orphaned_copies(&project), 0);
        let _ = fs::remove_dir_all(project);
    }
}
