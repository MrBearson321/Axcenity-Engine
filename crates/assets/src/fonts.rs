//! A project's own font files (`.ttf` and `.otf`, anywhere in the project
//! folder), which UI text can use by their family name.

use std::path::Path;
use std::sync::Arc;

/// A font file in the project.
#[derive(Clone, Debug)]
pub struct FontFile {
    /// Relative to the project folder, with `/` separators.
    pub path: String,
    pub data: Arc<Vec<u8>>,
}

/// True for the file names fonts are read from.
pub fn is_font_file(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".ttf") || lower.ends_with(".otf")
}

/// Every font file under `folder`, by path. Hidden folders (such as `.git`)
/// and the build folder `target` are skipped.
pub(crate) fn read_fonts(folder: &Path) -> Vec<FontFile> {
    let mut fonts = Vec::new();
    let mut stack = vec![folder.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if path.is_dir() {
                if !name.starts_with('.') && name != "target" {
                    stack.push(path);
                }
                continue;
            }
            if !is_font_file(&name) {
                continue;
            }
            let Ok(relative) = path.strip_prefix(folder) else {
                continue;
            };
            let relative = relative
                .components()
                .map(|part| part.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            if let Ok(data) = std::fs::read(&path) {
                fonts.push(FontFile {
                    path: relative,
                    data: Arc::new(data),
                });
            }
        }
    }
    fonts.sort_by(|a, b| a.path.cmp(&b.path));
    fonts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_files_are_found_anywhere_but_hidden_folders() {
        let folder =
            std::env::temp_dir().join(format!("axcenity-assets-fonts-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        for (path, data) in [
            ("assets/fonts/Title.TTF", "a"),
            ("ui/Body.otf", "b"),
            ("ui/readme.txt", "c"),
            (".git/Hidden.ttf", "d"),
        ] {
            let path = folder.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, data).unwrap();
        }
        let fonts = read_fonts(&folder);
        let paths: Vec<&str> = fonts.iter().map(|font| font.path.as_str()).collect();
        assert_eq!(paths, ["assets/fonts/Title.TTF", "ui/Body.otf"]);
        assert_eq!(fonts[1].data.as_slice(), b"b");
        let _ = std::fs::remove_dir_all(&folder);
    }
}
