//! Getting a downloaded package from the central asset directory into a
//! project: a texture set into `assets/textures/<id>/`, a sky into
//! `assets/sky/`, a sound into `assets/audio/`, a model through the Import
//! dialog; and recording each install in the metadata database and the
//! project's `.axcenity/asset-sources.json`.

use axcenity_assets::{
    Channel, MapFile, MapSlot, MaterialFile, MaterialMaps, TextureSet, give_asset_id,
};
use axcenity_toolbox::provenance;

use super::*;

/// How a download ended.
#[derive(Debug)]
pub enum DownloadResult {
    /// It's in: `text` for the Console, and what's in the project now.
    Saved {
        text: String,
        installed: Installed,
    },
    Cancelled,
}

/// What a download put where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Installed {
    /// A texture set's `.texture.json`, in the project.
    TextureSet { path: String, name: String },
    /// A sky image, in the project.
    Sky(String),
    /// A sound, in the project.
    Sound(String),
    /// A model file in the asset directory, for the Import dialog.
    Model {
        file: PathBuf,
        key: String,
        variant: String,
    },
}

/// Gets one package of `record` into the project: a texture into
/// `assets/textures/<id>/` with its texture set file, a sky into
/// `assets/sky/`, a sound into `assets/audio/`; a model only into the asset
/// directory, for the Import dialog to take from there.
///
/// The package goes into the central asset directory at `assets` first
/// (verified, unpacked, recorded in its metadata database with the asset's
/// source and licence), unless it's already there, in which case nothing
/// is downloaded. Then its images are copied into the project, and the
/// database records the install. Does nothing to the project when `cancel`
/// is set before the files are moved into place.
pub(super) fn download(
    http: &dyn Http,
    source: &dyn AssetSource,
    assets: &Path,
    project: &Path,
    record: &AssetRecord,
    variant: &Variant,
    cancel: &AtomicBool,
) -> Result<DownloadResult, String> {
    let id = record.id.as_str();
    if !folder_id(id) {
        return Err("the asset id is not a file name".to_owned());
    }
    let dir = AssetDirectory::open(assets).map_err(|err| err.to_string())?;
    let db = Database::open(&dir.database_path()).map_err(|err| err.to_string())?;
    let fetcher = Fetcher {
        http,
        dir: &dir,
        db: &db,
    };
    let extract = rules(record.kind).extract;
    let fetched = match fetcher.fetch_variant(source.info(), record, variant, extract, cancel) {
        Ok(fetched) => fetched,
        Err(SourceError::Cancelled) => return Ok(DownloadResult::Cancelled),
        Err(err) => return Err(err.to_string()),
    };
    if cancel.load(Ordering::Relaxed) {
        return Ok(DownloadResult::Cancelled);
    }
    let outcome = match record.kind {
        Kind::Hdri | Kind::Sound => install_file(&dir, &fetched, project, record, variant)?,
        Kind::Model => {
            // The Import dialog copies it into the project; the install is
            // recorded once it has (`record_model_install`).
            let file = model_file(&dir, &fetched)?;
            return Ok(DownloadResult::Saved {
                text: format!(
                    "Got {} ({}); choose how to import it.",
                    record.name,
                    license::display(&record.license, &record.license_version)
                ),
                installed: Installed::Model {
                    file,
                    key: record.key(),
                    variant: variant.label.clone(),
                },
            });
        }
        Kind::Texture | Kind::Sprites => {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |time| time.as_millis());
            let root = std::env::temp_dir().join(format!("axcenity-texture-{id}-{stamp}"));
            let outcome = install(&root, &dir, &fetched, project, record, cancel);
            let _ = fs::remove_dir_all(&root);
            outcome?
        }
    };
    if let DownloadResult::Saved { installed, .. } = &outcome {
        let (path, id_files) = match installed {
            // Each map gets an ID; the set is recorded by its first.
            Installed::TextureSet { path, .. } => (
                format!("{PROJECT_TEXTURES_FOLDER}/{id}"),
                axcenity_assets::read_texture_set(&project.join(path))
                    .map(|set| set.maps.into_iter().map(|map| map.path).collect())
                    .unwrap_or_default(),
            ),
            Installed::Sky(path) | Installed::Sound(path) => (path.clone(), vec![path.clone()]),
            Installed::Model { .. } => unreachable!("returned above"),
        };
        record_install(
            &db,
            project,
            &path,
            &id_files,
            &record.key(),
            &variant.label,
        );
    }
    Ok(outcome)
}

/// Records that `path` in `project` came from the asset `key`: gives
/// of `id_files` its `ax://` ID first (keeping the first's), then writes the install to the database
/// and the project's `.axcenity/asset-sources.json`. A failure is logged;
/// the files are in either way.
pub fn record_install(
    db: &Database,
    project: &Path,
    path: &str,
    id_files: &[String],
    key: &str,
    variant: &str,
) {
    let mut uid = None;
    for id_file in id_files {
        match give_asset_id(project, id_file) {
            Ok(given) => uid = uid.or(given.map(|given| given.to_string())),
            Err(err) => log::warn!("Could not give {id_file} an ID: {err}"),
        }
    }
    let install = Install {
        project: provenance::project_key(project),
        path: path.to_owned(),
        key: key.to_owned(),
        variant: variant.to_owned(),
        uid,
        installed_at: db::now(),
    };
    if let Err(err) = db.record_install(&install) {
        log::warn!("Could not record installing {key}: {err}");
    }
    if let Err(err) = provenance::refresh(db, project) {
        log::warn!("Could not update the project's asset sources: {err}");
    }
}

/// Records a Toolbox model the Import dialog copied to `copy` (its file in
/// the project, such as `assets/models/chair/chair.gltf`).
pub fn record_model_install(project: &Path, copy: &str, key: &str, variant: &str) {
    let Ok((_, db)) = library() else {
        return;
    };
    let folder = match copy.rsplit_once('/') {
        Some((folder, _)) if folder != axcenity_assets::MODELS_FOLDER => folder.to_owned(),
        _ => copy.to_owned(),
    };
    record_install(&db, project, &folder, &[copy.to_owned()], key, variant);
}

/// The model file in a downloaded package: a glTF first, then a GLB, then
/// an FBX, the one nearest the top of the package. Checked against the
/// Toolbox's rules (a glTF's files stay beside it, a GLB is one).
pub(super) fn model_file(dir: &AssetDirectory, fetched: &db::Download) -> Result<PathBuf, String> {
    let rank = |path: &str| match extension(path).as_str() {
        "gltf" => Some(0),
        "glb" => Some(1),
        "fbx" => Some(2),
        _ => None,
    };
    let file = fetched
        .files
        .iter()
        .filter_map(|file| {
            Some((
                rank(&file.path)?,
                file.path.matches('/').count(),
                &file.path,
            ))
        })
        .min()
        .map(|(_, _, path)| path.clone())
        .ok_or_else(|| "the download has no glTF or FBX model the engine imports".to_owned())?;
    let path = file
        .split('/')
        .fold(dir.path(&fetched.folder), |path, part| path.join(part));
    axcenity_toolbox::guard::check_model(&path).map_err(|err| err.to_string())?;
    Ok(path)
}

/// A material file using a texture set's maps, for Make material.
pub fn material_from_set(set: &TextureSet) -> MaterialFile {
    let map = |slot: MapSlot, channelled: bool| {
        set.maps.iter().find(|map| map.slot == slot).map(|found| {
            let mut file = MapFile::of(found.path.clone());
            if channelled && found.channel != Channel::Red {
                file.channel = Some(found.channel);
            }
            if found.invert {
                file.invert = Some(true);
            }
            if slot == MapSlot::Normal && found.flip_green {
                file.direct_x = Some(true);
            }
            file
        })
    };
    let maps = MaterialMaps {
        color: map(MapSlot::BaseColor, false),
        normal: map(MapSlot::Normal, false),
        metalness: map(MapSlot::Metalness, true),
        roughness: map(MapSlot::Roughness, true),
        occlusion: map(MapSlot::Occlusion, true),
        emissive: map(MapSlot::Emissive, false),
        opacity: map(MapSlot::Opacity, true),
        specular: map(MapSlot::Specular, true),
    };
    MaterialFile {
        // The maps hold the colour and roughness, so the plain values don't
        // tint or dull them.
        color: maps.color.as_ref().map(|_| [1.0, 1.0, 1.0]),
        roughness: maps.roughness.as_ref().map(|_| 1.0),
        metalness: maps.metalness.as_ref().map(|_| 1.0),
        maps,
        ..MaterialFile::default()
    }
}

/// Copies a downloaded sky into `assets/sky/<id>_<resolution>.<hdr|exr>`
/// or a sound into `assets/audio/<name>-<id start>.<ext>`, and returns what
/// to say and the file's path in the project.
pub(super) fn install_file(
    dir: &AssetDirectory,
    fetched: &db::Download,
    project: &Path,
    record: &AssetRecord,
    variant: &Variant,
) -> Result<DownloadResult, String> {
    let wanted = rules(record.kind).extract;
    let file = fetched
        .files
        .iter()
        .find(|file| wanted.contains(&extension(&file.path).as_str()))
        .ok_or_else(|| "the download has no file the engine reads".to_owned())?;
    let ext = extension(&file.path);
    let relative = if record.kind == Kind::Hdri {
        let resolution = variant.resolution.as_deref().unwrap_or("sky");
        format!("{PROJECT_SKY_FOLDER}/{}_{resolution}.{ext}", record.id)
    } else {
        format!("{PROJECT_AUDIO_FOLDER}/{}.{ext}", sound_name(record))
    };
    let dest = relative
        .split('/')
        .fold(project.to_owned(), |path, part| path.join(part));
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let replaced = dest.exists();
    let source = file
        .path
        .split('/')
        .fold(dir.path(&fetched.folder), |path, part| path.join(part));
    fs::copy(&source, &dest).map_err(|err| err.to_string())?;
    let verb = if replaced { "Replaced" } else { "Saved" };
    let text = format!(
        "{verb} {} ({}) in {relative}.",
        record.name,
        license::display(&record.license, &record.license_version)
    );
    let installed = if record.kind == Kind::Hdri {
        Installed::Sky(relative)
    } else {
        Installed::Sound(relative)
    };
    Ok(DownloadResult::Saved { text, installed })
}

/// A file name's extension, lower case.
pub(super) fn extension(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default()
}

/// A sound's file name: its title as lower-case words joined by `-`, then
/// the start of its id so two sounds with one title don't collide.
pub(super) fn sound_name(record: &AssetRecord) -> String {
    let words: Vec<String> = record
        .name
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_ascii_lowercase)
        .collect();
    let mut title = words.join("-");
    title.truncate(40);
    let title = title.trim_end_matches('-');
    let id: String = record
        .id
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(8)
        .collect();
    match (title.is_empty(), id.is_empty()) {
        (true, _) => format!("sound-{id}"),
        (false, true) => title.to_owned(),
        (false, false) => format!("{title}-{id}"),
    }
}

/// Copies a downloaded package's images into a staging folder, writes the
/// texture set beside them, and swaps the folder into the project.
pub(super) fn install(
    root: &Path,
    dir: &AssetDirectory,
    fetched: &db::Download,
    project: &Path,
    record: &AssetRecord,
    cancel: &AtomicBool,
) -> Result<DownloadResult, String> {
    let id = record.id.as_str();
    let staging = root.join("files");
    fs::create_dir_all(&staging).map_err(|err| err.to_string())?;
    let source = dir.path(&fetched.folder);
    let mut names = Vec::new();
    for file in &fetched.files {
        // Unpacked archives are flat, so each path is a plain file name.
        if file.path.contains('/') {
            continue;
        }
        fs::copy(source.join(&file.path), staging.join(&file.path))
            .map_err(|err| err.to_string())?;
        names.push(file.path.clone());
    }
    let pairs: Vec<(String, String)> = names
        .iter()
        .map(|file| (file.clone(), format!("assets/textures/{id}/{file}")))
        .collect();
    let borrowed: Vec<(&str, &str)> = pairs
        .iter()
        .map(|(file, path)| (file.as_str(), path.as_str()))
        .collect();
    let set = texture_set_from_files(&record.provider, id, &record.url, &borrowed);
    if set.maps.is_empty() {
        return Err("none of the images were recognized as maps".to_owned());
    }
    write_texture_set(&staging.join(format!("{id}{TEXTURE_SET_SUFFIX}")), &set)
        .map_err(|err| err.to_string())?;
    if cancel.load(Ordering::Relaxed) {
        return Ok(DownloadResult::Cancelled);
    }
    let dest = project.join("assets").join("textures").join(id);
    let replaced = files::replace_dir(&staging, &dest, &root.join("previous"))
        .map_err(|err| err.to_string())?;
    let verb = if replaced { "Replaced" } else { "Saved" };
    Ok(DownloadResult::Saved {
        text: format!(
            "{verb} {} ({}) in assets/textures/{id}.",
            record.name,
            license::display(&record.license, &record.license_version)
        ),
        installed: Installed::TextureSet {
            path: drawer::texture_set_path(id),
            name: record.name.clone(),
        },
    })
}
