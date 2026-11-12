//! Project templates people make themselves (`.axtemplate` files): a saved
//! project turned into a starting point for new ones, beside New Project's
//! built-in templates (`templates.rs`).
//!
//! A template file is a zip holding `axtemplate.json` ([`TemplateInfo`]),
//! an optional `thumbnail.png`, and the project's files under `project/`,
//! with `/` between folders. `.axcenity/` (this computer's cache, version
//! history, AutoRecover), `.git/` and `collaborate.json` (a Collaborate
//! project's repository, remote and project ID) are left out.
//! [`create_project_from_template_file`] unpacks those files into a new
//! folder and gives the manifest the new project's name, so the new project
//! starts with its own history.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{MANIFEST_FILE, SaveError, THUMBNAIL_FILE};

/// A template file's extension.
pub const TEMPLATE_EXTENSION: &str = "axtemplate";
/// What it holds, inside the zip.
const INFO_FILE: &str = "axtemplate.json";
/// The project's files, inside the zip.
const FILES_PREFIX: &str = "project/";
/// The newest template format this reads, and the one it writes.
const TEMPLATE_FORMAT: u32 = 1;
/// What's left out of a template, at the top of the project: this
/// computer's cache, history and AutoRecover, and a Collaborate project's
/// Git repository and `collaborate.json` (its project ID and remote belong
/// to that project, not to the ones made from it).
const LEFT_OUT: [&str; 3] = [".axcenity", ".git", "collaborate.json"];

/// What a template is, as `axtemplate.json` holds it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateInfo {
    pub format: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// The engine version that made it.
    #[serde(default)]
    pub created_with: String,
}

/// A template file found on disk, read for New Project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserTemplate {
    pub file: PathBuf,
    pub info: TemplateInfo,
    /// Its picture (PNG), if it has one.
    pub thumbnail: Option<Vec<u8>>,
    /// How many project files it holds.
    pub files: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    #[error("could not read or write `{}`", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("`{}` isn't an Axcenity template: {message}", path.display())]
    Invalid { path: PathBuf, message: String },
    #[error("`{}` was made by a newer Axcenity (template format {found}); update Axcenity to use it", path.display())]
    TooNew { path: PathBuf, found: u32 },
    #[error(transparent)]
    Save(#[from] SaveError),
}

impl TemplateError {
    fn io(path: &Path) -> impl FnOnce(io::Error) -> TemplateError + '_ {
        move |source| TemplateError::Io {
            path: path.to_owned(),
            source,
        }
    }

    fn invalid(path: &Path, message: impl Into<String>) -> TemplateError {
        TemplateError::Invalid {
            path: path.to_owned(),
            message: message.into(),
        }
    }

    /// True if creating failed because the new project's folder is there.
    pub fn already_exists(&self) -> bool {
        matches!(self, TemplateError::Save(err) if err.already_exists())
    }
}

/// A file name for a template called `name`, such as `My Shooter.axtemplate`
/// (characters file systems don't allow become `-`).
pub fn template_file_name(name: &str) -> String {
    let safe: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '-'
            } else {
                c
            }
        })
        .collect();
    let safe = safe.trim().trim_end_matches('.');
    let safe = if safe.is_empty() { "Template" } else { safe };
    format!("{safe}.{TEMPLATE_EXTENSION}")
}

/// Saves the project in `project` (as it is on disk) as the template file
/// `file`, called `name`, replacing a file already there. The project's
/// `thumbnail.png`, if it has one, becomes the template's picture.
pub fn save_template(
    project: &Path,
    file: &Path,
    name: &str,
    description: &str,
) -> Result<UserTemplate, TemplateError> {
    if name.trim().is_empty() {
        return Err(TemplateError::invalid(file, "the template needs a name"));
    }
    if !project.join(MANIFEST_FILE).is_file() {
        return Err(TemplateError::invalid(
            project,
            format!("there's no `{MANIFEST_FILE}` in it, so it isn't a project"),
        ));
    }
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent).map_err(TemplateError::io(parent))?;
    }
    let skip = file
        .parent()
        .and_then(|parent| fs::canonicalize(parent).ok());
    let mut files = Vec::new();
    collect(project, "", skip.as_deref(), &mut files).map_err(TemplateError::io(project))?;
    files.sort();
    let info = TemplateInfo {
        format: TEMPLATE_FORMAT,
        name: name.trim().to_owned(),
        description: description.trim().to_owned(),
        created_with: axcenity_release::Version::current().to_string(),
    };
    let thumbnail = fs::read(project.join(THUMBNAIL_FILE)).ok();

    // Written next to the file first, then moved over it, so a failed save
    // never leaves half a template in the user's list.
    let mut temporary = file.as_os_str().to_owned();
    temporary.push(".saving");
    let temporary = PathBuf::from(temporary);
    let zip_error = |err: zip::result::ZipError| io::Error::other(err.to_string());
    let written = (|| -> io::Result<()> {
        let mut writer = zip::ZipWriter::new(fs::File::create(&temporary)?);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        writer.start_file(INFO_FILE, options).map_err(zip_error)?;
        let text = serde_json::to_string_pretty(&info).map_err(io::Error::other)?;
        writer.write_all(text.as_bytes())?;
        if let Some(picture) = &thumbnail {
            writer
                .start_file(THUMBNAIL_FILE, options)
                .map_err(zip_error)?;
            writer.write_all(picture)?;
        }
        for relative in &files {
            writer
                .start_file(format!("{FILES_PREFIX}{relative}"), options)
                .map_err(zip_error)?;
            writer.write_all(&fs::read(project.join(relative))?)?;
        }
        writer.finish().map_err(zip_error)?;
        Ok(())
    })();
    if let Err(err) = written.and_then(|()| fs::rename(&temporary, file)) {
        let _ = fs::remove_file(&temporary);
        return Err(TemplateError::Io {
            path: file.to_owned(),
            source: err,
        });
    }
    Ok(UserTemplate {
        file: file.to_owned(),
        info,
        thumbnail,
        files: files.len(),
    })
}

/// Reads the template file `file`: what it is and its picture, without
/// unpacking the project.
pub fn read_template(file: &Path) -> Result<UserTemplate, TemplateError> {
    let mut archive = open(file)?;
    let info = read_info(&mut archive, file)?;
    let thumbnail = archive.by_name(THUMBNAIL_FILE).ok().and_then(|mut entry| {
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).ok().map(|_| bytes)
    });
    let files = archive
        .file_names()
        .filter(|name| name.starts_with(FILES_PREFIX) && !name.ends_with('/'))
        .count();
    Ok(UserTemplate {
        file: file.to_owned(),
        info,
        thumbnail,
        files,
    })
}

/// Every template file in `folder`, by name. Files that aren't templates
/// are left out, and listed with why in the second list.
pub fn list_templates(folder: &Path) -> (Vec<UserTemplate>, Vec<TemplateError>) {
    let mut templates = Vec::new();
    let mut problems = Vec::new();
    let Ok(entries) = fs::read_dir(folder) else {
        return (templates, problems);
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_template = path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case(TEMPLATE_EXTENSION));
        if !is_template || !path.is_file() {
            continue;
        }
        match read_template(&path) {
            Ok(template) => templates.push(template),
            Err(err) => problems.push(err),
        }
    }
    templates.sort_by(|a, b| {
        a.info
            .name
            .to_lowercase()
            .cmp(&b.info.name.to_lowercase())
            .then_with(|| a.file.cmp(&b.file))
    });
    (templates, problems)
}

/// Makes a new project called `name` in a new folder inside `parent` from
/// the template file `file`, and returns the folder. Like
/// [`crate::create_project_from`], it fails rather than touch a folder
/// that exists, and leaves nothing behind when it fails.
pub fn create_project_from_template_file(
    parent: &Path,
    name: &str,
    file: &Path,
) -> Result<PathBuf, TemplateError> {
    let mut archive = open(file)?;
    read_info(&mut archive, file)?;
    // The new folder, named the way built-in templates name theirs.
    let folder = crate::templates::make_project_folder(parent, name)?;
    let unpacked = unpack(&mut archive, file, &folder).and_then(|()| {
        // The new project's own name and engine record; the rest of the
        // template's manifest (start level, branding, settings) stays.
        let path = folder.join(MANIFEST_FILE);
        let text = fs::read_to_string(&path).map_err(TemplateError::io(&path))?;
        let mut manifest: serde_json::Value = serde_json::from_str(&text)
            .map_err(|err| TemplateError::invalid(file, format!("its manifest: {err}")))?;
        let engine = axcenity_release::Version::current().to_string();
        if let Some(object) = manifest.as_object_mut() {
            object.insert("name".into(), serde_json::Value::String(name.trim().into()));
            object.insert(
                "engine".into(),
                serde_json::json!({ "created_with": engine, "saved_with": engine }),
            );
        }
        let text = serde_json::to_string_pretty(&manifest).unwrap_or_default();
        crate::write_through_temporary(&path, &text)?;
        Ok(())
    });
    if let Err(err) = unpacked {
        let _ = fs::remove_dir_all(&folder);
        return Err(err);
    }
    Ok(folder)
}

fn open(file: &Path) -> Result<zip::ZipArchive<fs::File>, TemplateError> {
    let handle = fs::File::open(file).map_err(TemplateError::io(file))?;
    zip::ZipArchive::new(handle).map_err(|err| TemplateError::invalid(file, err.to_string()))
}

fn read_info(
    archive: &mut zip::ZipArchive<fs::File>,
    file: &Path,
) -> Result<TemplateInfo, TemplateError> {
    let mut text = String::new();
    archive
        .by_name(INFO_FILE)
        .map_err(|_| TemplateError::invalid(file, format!("it has no `{INFO_FILE}`")))?
        .read_to_string(&mut text)
        .map_err(TemplateError::io(file))?;
    // Read the format first, so a newer template is refused by name even
    // if the rest has changed shape.
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|err| TemplateError::invalid(file, format!("`{INFO_FILE}`: {err}")))?;
    let found = value.get("format").and_then(|f| f.as_u64()).unwrap_or(0) as u32;
    if found > TEMPLATE_FORMAT {
        return Err(TemplateError::TooNew {
            path: file.to_owned(),
            found,
        });
    }
    serde_json::from_value(value)
        .map_err(|err| TemplateError::invalid(file, format!("`{INFO_FILE}`: {err}")))
}

/// Writes the template's project files into the new, empty `folder`. Paths that would leave the folder are refused.
fn unpack(
    archive: &mut zip::ZipArchive<fs::File>,
    file: &Path,
    folder: &Path,
) -> Result<(), TemplateError> {
    let mut found_manifest = false;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|err| TemplateError::invalid(file, err.to_string()))?;
        let Some(relative) = entry.name().strip_prefix(FILES_PREFIX) else {
            continue;
        };
        if relative.is_empty() || entry.is_dir() {
            continue;
        }
        let safe = Path::new(relative)
            .components()
            .all(|part| matches!(part, Component::Normal(_)));
        if !safe
            || relative
                .split('/')
                .next()
                .is_some_and(|top| LEFT_OUT.contains(&top))
        {
            return Err(TemplateError::invalid(
                file,
                format!("it holds `{relative}`, which isn't a project file"),
            ));
        }
        found_manifest |= relative == MANIFEST_FILE;
        let path = folder.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(TemplateError::io(parent))?;
        }
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(TemplateError::io(file))?;
        fs::write(&path, bytes).map_err(TemplateError::io(&path))?;
    }
    if !found_manifest {
        return Err(TemplateError::invalid(
            file,
            format!("it has no `{MANIFEST_FILE}`"),
        ));
    }
    Ok(())
}

/// Every file under `dir`, relative to the project with `/` between
/// folders, leaving out [`LEFT_OUT`] and the folder `skip` (where the
/// template is being written, if that's inside the project).
fn collect(
    dir: &Path,
    prefix: &str,
    skip: Option<&Path>,
    files: &mut Vec<String>,
) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if prefix.is_empty() && LEFT_OUT.contains(&name.as_str()) {
            continue;
        }
        let relative = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        let path = entry.path();
        let kind = entry.file_type()?;
        if kind.is_dir() {
            if skip.is_some() && fs::canonicalize(&path).ok().as_deref() == skip {
                continue;
            }
            collect(&path, &relative, skip, files)?;
        } else if kind.is_file() {
            // A template being written here, or a half-written one.
            if relative.ends_with(&format!(".{TEMPLATE_EXTENSION}"))
                || relative.ends_with(".saving")
            {
                continue;
            }
            files.push(relative);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Template, create_project_from, load};

    fn temp_folder(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("axcenity-{name}-templates-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    /// A project saved as a template makes new projects with its world,
    /// scripts and assets, under their own names, without its `.axcenity`
    /// folder; the template lists with its name, description and picture.
    #[test]
    fn a_saved_template_makes_new_projects_like_its_project() {
        let parent = temp_folder("round-trip");
        let source = create_project_from(&parent, "Shooter", Template::FirstPerson).unwrap();
        fs::create_dir_all(source.join(".axcenity/versions")).unwrap();
        fs::write(source.join(".axcenity/versions/old.json"), "{}").unwrap();
        fs::create_dir_all(source.join(".git")).unwrap();
        fs::write(source.join(".git/config"), "[remote]").unwrap();
        fs::write(source.join("collaborate.json"), "{}").unwrap();
        crate::write_script(&source, "scripts/Extra.server.lua", "print('mine')").unwrap();

        let library = parent.join("My Templates");
        let file = library.join(template_file_name("My: Shooter"));
        assert_eq!(file.file_name().unwrap(), "My- Shooter.axtemplate");
        let saved = save_template(&source, &file, "My Shooter", "A blaster game").unwrap();
        assert!(saved.thumbnail.is_some(), "the project's picture");

        let (listed, problems) = list_templates(&library);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].info.name, "My Shooter");
        assert_eq!(listed[0].info.description, "A blaster game");
        assert_eq!(listed[0].thumbnail, saved.thumbnail);
        assert_eq!(listed[0].files, saved.files);

        let made = create_project_from_template_file(&parent, "Second Game", &file).unwrap();
        assert_eq!(made.file_name().unwrap(), "Second Game");
        let project = load(&made).unwrap();
        assert!(project.problems.is_empty(), "{:?}", project.problems);
        assert_eq!(project.manifest.name, "Second Game");
        let original = load(&source).unwrap();
        assert_eq!(
            crate::write_world(&project.world),
            crate::write_world(&original.world),
            "the same world"
        );
        assert_eq!(
            fs::read_to_string(made.join("scripts/Extra.server.lua")).unwrap(),
            "print('mine')"
        );
        assert!(!made.join(".axcenity/versions/old.json").exists());
        assert!(!made.join(".git").exists(), "not the project's repository");
        assert!(
            !made.join("collaborate.json").exists(),
            "nor its project ID"
        );

        // A second project of the same name is refused, and nothing is touched.
        let again = create_project_from_template_file(&parent, "Second Game", &file).unwrap_err();
        assert!(again.already_exists(), "{again}");
        fs::remove_dir_all(parent).unwrap();
    }

    /// Saving a template into the project itself doesn't pack the template
    /// into itself.
    #[test]
    fn a_template_saved_inside_its_project_leaves_itself_out() {
        let parent = temp_folder("inside");
        let source = create_project_from(&parent, "Plate", Template::Baseplate).unwrap();
        let file = source.join("Plate.axtemplate");
        save_template(&source, &file, "Plate", "").unwrap();
        let again = save_template(&source, &file, "Plate", "").unwrap();
        let names: Vec<String> = {
            let mut archive = open(&file).unwrap();
            (0..archive.len())
                .map(|i| archive.by_index(i).unwrap().name().to_owned())
                .collect()
        };
        assert!(
            !names.iter().any(|name| name.ends_with(".axtemplate")),
            "{names:?}"
        );
        assert_eq!(
            again.files,
            names.iter().filter(|n| n.starts_with(FILES_PREFIX)).count()
        );
        fs::remove_dir_all(parent).unwrap();
    }

    /// Files that aren't templates, newer templates and ones holding paths
    /// that climb out of the project are refused, leaving no folder.
    #[test]
    fn broken_newer_and_unsafe_templates_are_refused() {
        let parent = temp_folder("refused");
        let library = parent.join("library");
        fs::create_dir_all(&library).unwrap();
        fs::write(library.join("Junk.axtemplate"), "not a zip").unwrap();

        let write_zip = |file: &Path, info: &str, entries: &[(&str, &str)]| {
            let mut writer = zip::ZipWriter::new(fs::File::create(file).unwrap());
            let options = zip::write::SimpleFileOptions::default();
            writer.start_file(INFO_FILE, options).unwrap();
            writer.write_all(info.as_bytes()).unwrap();
            for (name, text) in entries {
                writer.start_file(*name, options).unwrap();
                writer.write_all(text.as_bytes()).unwrap();
            }
            writer.finish().unwrap();
        };
        let newer = library.join("Newer.axtemplate");
        write_zip(&newer, r#"{"format": 99, "name": "Newer"}"#, &[]);
        let unsafe_file = library.join("Unsafe.axtemplate");
        write_zip(
            &unsafe_file,
            r#"{"format": 1, "name": "Unsafe"}"#,
            &[
                (
                    "project/axcenity.json",
                    r#"{"name": "x", "format_version": 1}"#,
                ),
                ("project/../escape.txt", "no"),
            ],
        );

        let (listed, problems) = list_templates(&library);
        assert_eq!(listed.len(), 1, "only Unsafe reads as a template");
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(matches!(
            create_project_from_template_file(&parent, "From Newer", &newer),
            Err(TemplateError::TooNew { found: 99, .. })
        ));
        assert!(matches!(
            create_project_from_template_file(&parent, "From Unsafe", &unsafe_file),
            Err(TemplateError::Invalid { .. })
        ));
        assert!(!parent.join("From Unsafe").exists(), "nothing left behind");
        assert!(!parent.join("escape.txt").exists());
        fs::remove_dir_all(parent).unwrap();
    }
}
