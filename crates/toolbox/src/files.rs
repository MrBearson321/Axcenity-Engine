//! Moving a finished download into a project as one step.

use std::fs;
use std::path::Path;

use crate::SourceError;

/// Moves the folder `staging` to `dest`, replacing `dest` when it's already
/// there, and returns whether it replaced one.
///
/// `backup` holds the old folder until the new one is in place, and the
/// old one is put back if the move fails, so `dest` is never left half
/// written. When a rename can't cross drives, the folder is copied instead.
pub fn replace_dir(staging: &Path, dest: &Path, backup: &Path) -> Result<bool, SourceError> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let replaced = dest.exists();
    if replaced {
        let _ = fs::remove_dir_all(backup);
        fs::rename(dest, backup)?;
    }
    if fs::rename(staging, dest).is_err()
        && let Err(err) = copy_dir(staging, dest)
    {
        if replaced {
            let _ = fs::remove_dir_all(dest);
            let _ = fs::rename(backup, dest);
        }
        return Err(err);
    }
    if replaced {
        let _ = fs::remove_dir_all(backup);
    }
    Ok(replaced)
}

/// Copies the folder `from` and everything in it to `to`.
pub fn copy_dir(from: &Path, to: &Path) -> Result<(), SourceError> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let dest = to.join(entry.file_name());
        if entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            copy_dir(&entry.path(), &dest)?;
        } else {
            fs::copy(entry.path(), &dest)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_is_moved_in_and_then_replaced() {
        let root =
            std::env::temp_dir().join(format!("axcenity-toolbox-replace-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let dest = root
            .join("project")
            .join("assets")
            .join("textures")
            .join("Rock");
        let backup = root.join("previous");

        let first = root.join("first");
        fs::create_dir_all(&first).unwrap();
        fs::write(first.join("old.png"), b"old").unwrap();
        assert!(!replace_dir(&first, &dest, &backup).unwrap());
        assert!(dest.join("old.png").is_file());

        let second = root.join("second");
        fs::create_dir_all(&second).unwrap();
        fs::write(second.join("new.png"), b"new").unwrap();
        assert!(replace_dir(&second, &dest, &backup).unwrap());
        assert!(dest.join("new.png").is_file());
        assert!(!dest.join("old.png").exists());
        assert!(!backup.exists());
        let _ = fs::remove_dir_all(&root);
    }
}
