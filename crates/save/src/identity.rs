//! This install's player identity (Phase 64): its Players' `UserId`, the
//! same each time the engine runs on this computer, so a server can tell a
//! returning player apart. There are no accounts: it's a random number
//! kept in a file the first time it's needed.

use std::collections::hash_map::RandomState;
use std::fs;
use std::hash::{BuildHasher, Hasher};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// The file in the [saves folder](crate::saves_folder) that keeps it.
const FILE: &str = "user-id.txt";

/// Lua numbers hold whole numbers exactly up to 2^53.
const LARGEST: u64 = (1 << 53) - 1;

/// This install's `UserId`, kept in the [saves folder](crate::saves_folder).
pub fn user_id() -> u64 {
    user_id_in(crate::saves_folder().as_deref())
}

/// This install's `UserId`, from `folder`'s file, made and kept there the
/// first time. With no folder, or one that can't be written, it's a new
/// number for this run.
pub fn user_id_in(folder: Option<&Path>) -> u64 {
    let Some(folder) = folder else {
        return random_id();
    };
    let path = folder.join(FILE);
    if let Some(id) = fs::read_to_string(&path)
        .ok()
        .and_then(|text| text.trim().parse::<u64>().ok())
        .filter(|&id| (1..=LARGEST).contains(&id))
    {
        return id;
    }
    let id = random_id();
    // Not kept, it's still a fine UserId for this run.
    let _ = fs::create_dir_all(folder).and_then(|_| fs::write(&path, format!("{id}\n")));
    id
}

/// A random number from 1 to [`LARGEST`]. `RandomState` is seeded from the
/// system's randomness; the time and process mix in more.
fn random_id() -> u64 {
    let mut hasher = RandomState::new().build_hasher();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    hasher.write_u128(now.as_nanos());
    hasher.write_u32(std::process::id());
    (hasher.finish() % LARGEST) + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_install_keeps_its_user_id() {
        let folder = std::env::temp_dir().join(format!("axcenity-user-id-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        let first = user_id_in(Some(&folder));
        assert!((1..=LARGEST).contains(&first));
        assert_eq!(user_id_in(Some(&folder)), first, "the same next time");
        fs::write(folder.join(FILE), "not a number").unwrap();
        assert_ne!(
            user_id_in(Some(&folder)),
            0,
            "a damaged file gets a new one"
        );
        let _ = fs::remove_dir_all(&folder);
    }
}
