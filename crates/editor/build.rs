//! Builds the repository's release files (`releases/`) into the editor, so
//! the Version Explorer (`src/version_explorer.rs`) works offline and on a
//! build that has never been online. Writes `release_files.rs` in OUT_DIR:
//! each file's path relative to `releases/` and its text, for
//! `axcenity_release::History::from_sources`. Reruns only when `releases/`
//! changes.

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let releases = manifest_dir.join("../../releases");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", releases.display());

    let mut files = Vec::new();
    for sub in ["history", "unreleased", "features"] {
        files.extend(toml_files(&releases.join(sub), sub));
    }
    let issues = releases.join("issues.toml");
    if issues.is_file() {
        files.push(("issues.toml".to_owned(), issues));
    }
    files.sort();

    let mut out = String::from(
        "/// The release files built in: path relative to `releases/`, and text.\n\
         pub const RELEASE_FILES: &[(&str, &str)] = &[\n",
    );
    for (name, path) in &files {
        let path = path.canonicalize().unwrap();
        writeln!(
            out,
            "    ({name:?}, include_str!({:?})),",
            path.display().to_string()
        )
        .unwrap();
    }
    out.push_str("];\n");
    let target = PathBuf::from(env::var("OUT_DIR").unwrap()).join("release_files.rs");
    fs::write(target, out).unwrap();
}

/// The `.toml` files in `dir`, named `<sub>/<file>`.
fn toml_files(dir: &Path, sub: &str) -> Vec<(String, PathBuf)> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "toml"))
        .map(|p| {
            let name = format!("{sub}/{}", p.file_name().unwrap().to_string_lossy());
            (name, p)
        })
        .collect()
}
