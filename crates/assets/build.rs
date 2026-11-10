//! Hashes this crate's source and the workspace's lockfile into
//! `AXCENITY_ASSETS_SOURCE_HASH`, so a cooked model (`model/cooked_model.rs`)
//! made by other loading code, or other versions of the glTF and FBX
//! readers, is never taken for a current one.

use std::path::{Path, PathBuf};

fn main() {
    let crate_folder = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("set by cargo"));
    let lockfile = crate_folder.join("../../Cargo.lock");
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed={}", lockfile.display());

    let mut files = Vec::new();
    collect(&crate_folder.join("src"), &mut files);
    files.sort();
    files.push(lockfile);
    // FNV-1a over each file's path (within the crate) and bytes.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut mix = |bytes: &[u8]| {
        for &byte in bytes {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
    };
    for file in &files {
        let name = file.strip_prefix(&crate_folder).unwrap_or(file);
        mix(name.to_string_lossy().replace('\\', "/").as_bytes());
        // Line endings differ between checkouts; the code doesn't.
        let bytes = std::fs::read(file).unwrap_or_default();
        let text: Vec<u8> = bytes.into_iter().filter(|&byte| byte != b'\r').collect();
        mix(&text);
    }
    println!("cargo:rustc-env=AXCENITY_ASSETS_SOURCE_HASH={hash:016x}");
}

fn collect(folder: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, files);
        } else {
            files.push(path);
        }
    }
}
