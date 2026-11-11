//! Builds the repository's release files (`releases/`) into the editor, so
//! the Version Explorer (`src/version_explorer.rs`) works offline and on a
//! build that has never been online. Writes `release_files.rs` in OUT_DIR:
//! each file's path relative to `releases/` and its text, for
//! `axcenity_release::History::from_sources`. Reruns only when `releases/`
//! changes.
//!
//! Also builds in the documentation (`documentation/`, the Learn and
//! Documentation pages, `src/start/docs.rs`): `doc_files.rs` lists each
//! Markdown file's path relative to `documentation/` and its text. The
//! generated API pages (`documentation/api/`) are left out: the editor's
//! API Reference shows the same entries.

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
    for file in ["issues.toml", "formats.toml"] {
        let path = releases.join(file);
        if path.is_file() {
            files.push((file.to_owned(), path));
        }
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

    write_doc_files(&manifest_dir.join("../../documentation"));
}

/// Writes `doc_files.rs`: every `.md` under `documentation/` but `api/`.
fn write_doc_files(root: &Path) {
    println!("cargo:rerun-if-changed={}", root.display());
    let mut files = Vec::new();
    markdown_files(root, root, &mut files);
    files.sort();
    let mut out = String::from(
        "/// The documentation built in: path relative to `documentation/`, and text.\n\
         pub const DOC_FILES: &[(&str, &str)] = &[\n",
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
    let target = PathBuf::from(env::var("OUT_DIR").unwrap()).join("doc_files.rs");
    fs::write(target, out).unwrap();
}

/// The `.md` files under `dir`, named relative to `root` with `/`. Cargo
/// only watches a folder's own entries, so each folder is watched too.
fn markdown_files(root: &Path, dir: &Path, files: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for path in entries.filter_map(Result::ok).map(|e| e.path()) {
        let relative = path
            .strip_prefix(root)
            .unwrap()
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        if path.is_dir() {
            if relative != "api" && !relative.starts_with('.') {
                println!("cargo:rerun-if-changed={}", path.display());
                markdown_files(root, &path, files);
            }
        } else if path.extension().is_some_and(|e| e == "md") {
            println!("cargo:rerun-if-changed={}", path.display());
            files.push((relative, path));
        }
    }
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
