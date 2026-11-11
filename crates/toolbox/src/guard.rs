//! Rules for untrusted downloads. A source's answer and its files come from
//! the internet, so names are checked before they become paths, and
//! archives are unpacked only into the folder they're meant for.

use std::fs;
use std::io;
use std::path::Path;

use crate::SourceError;

/// Whether `id` is safe as a folder and file name: ASCII letters and digits
/// only, as ambientCG's ids are (`Fabric086`).
pub fn safe_asset_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric())
}

/// A relative path a source gave (a file inside a bundle) with `/`
/// separators, or `None` if it could leave its folder: empty, absolute, a
/// drive or URL, or any `.` or `..` part.
pub fn safe_relative_path(path: &str) -> Option<String> {
    let path = path.replace('\\', "/");
    if path.starts_with('/') || path.contains(':') {
        return None;
    }
    let parts: Vec<&str> = path.split('/').collect();
    if parts
        .iter()
        .any(|part| part.is_empty() || *part == "." || *part == "..")
    {
        return None;
    }
    Some(parts.join("/"))
}

/// Extracts the files of the zip at `zip_path` whose extension (any case)
/// is in `extensions` straight into `dest`, by file name only, and returns
/// their names in archive order.
///
/// Folders inside the archive are dropped, so nothing can land outside
/// `dest`; entries whose path would escape it, hidden files, and a second
/// file with a name already taken are skipped. An archive with none of
/// those files is an error.
pub fn extract_files(
    zip_path: &Path,
    dest: &Path,
    extensions: &[&str],
) -> Result<Vec<String>, SourceError> {
    let file = fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|_| SourceError::BadResponse("the package is not a zip archive"))?;
    let mut names = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|_| SourceError::BadResponse("the package is damaged"))?;
        if entry.is_dir() {
            continue;
        }
        let Some(enclosed) = entry.enclosed_name() else {
            continue;
        };
        let Some(file_name) = enclosed.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if file_name.is_empty() || file_name.starts_with('.') || file_name.contains("..") {
            continue;
        }
        let extension = file_name
            .rsplit_once('.')
            .map(|(_, extension)| extension.to_ascii_lowercase())
            .unwrap_or_default();
        if !extensions.contains(&extension.as_str()) {
            continue;
        }
        if names.iter().any(|known| known == file_name) {
            continue;
        }
        let mut output = fs::File::create(dest.join(file_name))?;
        io::copy(&mut entry, &mut output)?;
        names.push(file_name.to_owned());
    }
    if names.is_empty() {
        return Err(SourceError::BadResponse("the package has no images"));
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("axcenity-toolbox-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let mut cursor = io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut cursor);
            let options = zip::write::SimpleFileOptions::default();
            for (name, bytes) in entries {
                writer.start_file(*name, options).unwrap();
                writer.write_all(bytes).unwrap();
            }
            writer.finish().unwrap();
        }
        fs::write(path, cursor.into_inner()).unwrap();
    }

    #[test]
    fn extraction_keeps_images_and_drops_paths_that_escape() {
        let root = temp_dir("zip-slip");
        let zip_path = root.join("package.zip");
        write_zip(
            &zip_path,
            &[
                ("maps/Fabric086_Color.jpg", b"color"),
                ("../evil.png", b"no"),
                ("readme.txt", b"skip"),
            ],
        );
        let dest = root.join("out");
        fs::create_dir_all(&dest).unwrap();
        let names = extract_files(&zip_path, &dest, &["jpg", "png"]).unwrap();
        assert_eq!(names, vec!["Fabric086_Color.jpg".to_owned()]);
        assert!(dest.join("Fabric086_Color.jpg").is_file());
        assert!(!root.join("evil.png").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn hidden_and_repeated_names_are_skipped() {
        let root = temp_dir("repeats");
        let zip_path = root.join("package.zip");
        write_zip(
            &zip_path,
            &[
                ("a/Rock_Color.PNG", b"first"),
                ("b/Rock_Color.PNG", b"second"),
                (".Rock_Normal.png", b"hidden"),
            ],
        );
        let dest = root.join("out");
        fs::create_dir_all(&dest).unwrap();
        let names = extract_files(&zip_path, &dest, &["png"]).unwrap();
        assert_eq!(names, vec!["Rock_Color.PNG".to_owned()]);
        assert_eq!(fs::read(dest.join("Rock_Color.PNG")).unwrap(), b"first");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn an_archive_without_images_or_not_a_zip_is_an_error() {
        let root = temp_dir("empty");
        let zip_path = root.join("package.zip");
        write_zip(&zip_path, &[("readme.txt", b"nothing")]);
        assert!(matches!(
            extract_files(&zip_path, &root, &["png"]),
            Err(SourceError::BadResponse(_))
        ));
        fs::write(&zip_path, b"not a zip").unwrap();
        assert!(matches!(
            extract_files(&zip_path, &root, &["png"]),
            Err(SourceError::BadResponse(_))
        ));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn relative_paths_must_stay_inside_their_folder() {
        assert_eq!(
            safe_relative_path("textures/a_2k.jpg").as_deref(),
            Some("textures/a_2k.jpg")
        );
        assert_eq!(
            safe_relative_path("textures\\a.jpg").as_deref(),
            Some("textures/a.jpg")
        );
        for bad in [
            "",
            "../a.jpg",
            "a/../../b",
            "/etc/a",
            "C:/a.jpg",
            "https://x/a",
            "a//b",
            "./a",
        ] {
            assert_eq!(safe_relative_path(bad), None, "{bad}");
        }
    }

    #[test]
    fn asset_ids_are_plain_names() {
        assert!(safe_asset_id("Fabric086"));
        assert!(!safe_asset_id(""));
        assert!(!safe_asset_id("../nope"));
        assert!(!safe_asset_id("a b"));
    }
}
