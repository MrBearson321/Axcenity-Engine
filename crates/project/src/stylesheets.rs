use super::*;

pub(super) fn load_stylesheets(
    folder: &Path,
    problems: &mut Vec<LoadError>,
) -> Vec<StylesheetFile> {
    find_stylesheets(folder, problems)
        .into_iter()
        .filter_map(|path| match read(&folder.join(&path)) {
            Ok(source) => Some(StylesheetFile { path, source }),
            Err(problem) => {
                problems.push(problem);
                None
            }
        })
        .collect()
}

/// The paths of the `.css` files in `folder` and its subfolders, sorted,
/// which is the order they cascade in. Folders that can't be read are added
/// to `problems`.
pub fn find_stylesheets(folder: &Path, problems: &mut Vec<LoadError>) -> Vec<String> {
    let mut paths = Vec::new();
    find_stylesheet_files(folder, "", &mut paths, problems);
    paths.sort();
    paths
}

/// True for a stylesheet's path (one ending in `.css`, in any capitals).
pub fn is_stylesheet(path: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("css"))
}

/// What a new stylesheet starts with: a rule for every ScreenGui, to show
/// how one looks.
pub const NEW_STYLESHEET_SOURCE: &str = "ScreenGui {
    --text: #ffffff;
}
";

/// Makes a new stylesheet, `name.css`, at the top of the project folder with
/// `source` in it, and returns its path. Fails (see
/// [`SaveError::already_exists`]) rather than replace a file.
pub fn create_stylesheet(folder: &Path, name: &str, source: &str) -> Result<String, SaveError> {
    let path = format!("{name}.css");
    let disk_path = folder.join(&path);
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&disk_path)
        .map_err(SaveError::new(&disk_path))?;
    file.write_all(source.as_bytes())
        .map_err(SaveError::new(&disk_path))?;
    Ok(path)
}

/// Writes `source` to the stylesheet at `path`, the same way as [`write_script`].
pub fn write_stylesheet(folder: &Path, path: &str, source: &str) -> Result<(), SaveError> {
    write_script(folder, path, source)
}

pub(super) fn find_stylesheet_files(
    directory: &Path,
    prefix: &str,
    found: &mut Vec<String>,
    problems: &mut Vec<LoadError>,
) {
    let mut problem = |source| {
        problems.push(LoadError::Read {
            path: directory.to_owned(),
            source,
        })
    };
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(err) if err.kind() == ErrorKind::NotFound && prefix.is_empty() => return,
        Err(err) => return problem(err),
    };

    let mut folders = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                problem(err);
                continue;
            }
        };
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let shown = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(err) => {
                problem(err);
                continue;
            }
        };
        if file_type.is_dir() {
            if !name.starts_with('.') && name != "target" && name != "node_modules" {
                folders.push((path, shown));
            }
        } else if file_type.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("css"))
        {
            found.push(shown);
        }
    }
    for (path, shown) in folders {
        find_stylesheet_files(&path, &shown, found, problems);
    }
}
