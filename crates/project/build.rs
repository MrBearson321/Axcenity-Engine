//! Embeds the genre templates (the repository's `templates/` folder) in the
//! crate, so New Project can write them anywhere. Each file becomes
//! `(template folder, path inside it, its bytes)` in `TEMPLATE_FILES`.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn main() {
    let root = Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../templates");
    // Cargo looks through a folder named here for changed files.
    println!("cargo:rerun-if-changed={}", root.display());
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&root) {
        let mut folders: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect();
        folders.sort();
        for folder in folders {
            let name = folder.file_name().unwrap().to_string_lossy().into_owned();
            let mut inside = Vec::new();
            walk(&folder, &folder, &mut inside);
            inside.sort();
            files.extend(
                inside
                    .into_iter()
                    .map(|(path, full)| (name.clone(), path, full)),
            );
        }
    }
    let mut code = String::from("pub(crate) static TEMPLATE_FILES: &[(&str, &str, &[u8])] = &[\n");
    for (template, path, full) in files {
        // `{:?}` writes each as a Rust string literal, backslashes escaped.
        // The path is absolute already (from `CARGO_MANIFEST_DIR`).
        writeln!(
            code,
            "    ({template:?}, {path:?}, include_bytes!({:?})),",
            full.display().to_string()
        )
        .unwrap();
    }
    code.push_str("];\n");
    let out = Path::new(&std::env::var("OUT_DIR").unwrap()).join("template_files.rs");
    std::fs::write(out, code).unwrap();
}

/// Every file under `folder`, as its path from `root` with `/` between
/// names, and its full path. Hidden files (`.DS_Store`) are left out.
fn walk(root: &Path, folder: &Path, out: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        if path.is_dir() {
            walk(root, &path, out);
        } else {
            let relative = path.strip_prefix(root).unwrap();
            let names: Vec<_> = relative
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect();
            out.push((names.join("/"), path));
        }
    }
}
