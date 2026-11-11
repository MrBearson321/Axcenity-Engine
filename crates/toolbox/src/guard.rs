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

/// Checks a downloaded model before anything imports it: a `.glb` must
/// start with `glTF`; a `.gltf` must be JSON whose buffers and images are
/// embedded (`data:`) or files beside it, inside its folder, and present.
/// An `.fbx` must be a file. Anything else isn't a model.
pub fn check_model(path: &Path) -> Result<(), SourceError> {
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match extension.as_str() {
        "glb" => {
            let mut magic = [0u8; 4];
            let mut file = fs::File::open(path)?;
            io::Read::read_exact(&mut file, &mut magic)
                .map_err(|_| SourceError::Refused("the model isn't a glTF binary"))?;
            if &magic != b"glTF" {
                return Err(SourceError::Refused("the model isn't a glTF binary"));
            }
            Ok(())
        }
        "gltf" => {
            let text = fs::read_to_string(path)?;
            let json: serde_json::Value = serde_json::from_str(&text)
                .map_err(|_| SourceError::Refused("the model isn't valid glTF"))?;
            let folder = path.parent().unwrap_or(Path::new("."));
            for list in ["buffers", "images"] {
                let entries = json.get(list).and_then(|v| v.as_array());
                for entry in entries.into_iter().flatten() {
                    let Some(uri) = entry.get("uri").and_then(|uri| uri.as_str()) else {
                        continue;
                    };
                    if uri.starts_with("data:") {
                        continue;
                    }
                    let decoded = percent_decode(uri);
                    let relative = safe_relative_path(&decoded).ok_or(SourceError::Refused(
                        "the model refers to a file outside its folder",
                    ))?;
                    let file = relative
                        .split('/')
                        .fold(folder.to_owned(), |p, part| p.join(part));
                    if !file.is_file() {
                        return Err(SourceError::Refused("a file the model needs is missing"));
                    }
                }
            }
            Ok(())
        }
        "fbx" if path.is_file() => Ok(()),
        "fbx" => Err(SourceError::Refused("the model file is missing")),
        _ => Err(SourceError::Refused(
            "the file isn't a model the engine imports",
        )),
    }
}

/// `%20` and the like in a glTF URI, as bytes, read as UTF-8.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(byte) = u8::from_str_radix(&text[i + 1..i + 3], 16)
        {
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
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

    #[test]
    fn a_model_must_keep_its_files_beside_it() {
        let dir = temp_dir("model");
        fs::create_dir_all(dir.join("textures")).unwrap();
        fs::write(dir.join("chair.bin"), b"buf").unwrap();
        fs::write(dir.join("textures").join("chair diff.jpg"), b"img").unwrap();
        let gltf = |uris: &str| {
            let path = dir.join("chair.gltf");
            fs::write(
                &path,
                format!(r#"{{"asset": {{"version": "2.0"}}, {uris}}}"#),
            )
            .unwrap();
            check_model(&path)
        };
        assert!(gltf(r#""buffers": [{"uri": "chair.bin"}], "images": [{"uri": "textures/chair%20diff.jpg"}, {"uri": "data:image/png;base64,AA=="}]"#).is_ok());
        assert!(gltf(r#""buffers": [{"uri": "../secret.bin"}]"#).is_err());
        assert!(gltf(r#""images": [{"uri": "C:/Windows/x.png"}]"#).is_err());
        assert!(gltf(r#""buffers": [{"uri": "missing.bin"}]"#).is_err());
        fs::write(dir.join("not.gltf"), b"{oops").unwrap();
        assert!(check_model(&dir.join("not.gltf")).is_err());
        fs::write(dir.join("good.glb"), b"glTF\x02\x00\x00\x00").unwrap();
        fs::write(dir.join("bad.glb"), b"PK\x03\x04").unwrap();
        assert!(check_model(&dir.join("good.glb")).is_ok());
        assert!(check_model(&dir.join("bad.glb")).is_err());
        assert!(check_model(&dir.join("chair.blend")).is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
