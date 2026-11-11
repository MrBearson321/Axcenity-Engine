use std::collections::HashSet;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use axcenity_blocks_runtime::translate;
use thiserror::Error;

use crate::{is_block_script, read_block_script};

const MAGIC: &[u8; 8] = b"AXCGAME\0";
/// Version 2 (roadmap M9): each entry may be zstd-compressed and carries
/// its file's modified time. Version 1 files (no compression, no times)
/// still unpack.
const VERSION: u32 = 2;
/// zstd's level for entries. Measured on the examples' built games, level
/// 9 shrank them no more than 3 (cooked games 0.83 vs 0.81 of their size)
/// at twice the time.
const ZSTD_LEVEL: i32 = 3;
/// An entry is stored compressed only if that saves at least this share,
/// so PNG, JPEG and audio files (already compressed) cost nothing to read.
const MIN_SAVING: f64 = 0.05;
/// Entry flag: the data is a zstd frame.
const COMPRESSED: u8 = 1;
/// The largest file an entry may unpack to.
const MAX_FILE_BYTES: u64 = 16 << 30;
const MAX_FILES: usize = 100_000;
const MAX_PATH_BYTES: usize = 4096;

#[derive(Debug, Error)]
pub enum PackError {
    #[error("could not read `{}`", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("could not write `{}`", path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("could not translate block script `{path}`: {message}")]
    Blocks { path: String, message: String },
    #[error("game file contains an invalid or unsafe entry")]
    InvalidArchive,
    #[error("game file format version {0} is not supported")]
    UnsupportedVersion(u32),
    #[error("game file has too many entries")]
    TooManyFiles,
}

/// One file going into a game file.
struct PackFile {
    name: String,
    contents: Vec<u8>,
    /// Its modified time in nanoseconds since 1970, or 0 for none (a
    /// translated block script). Unpacking gives the file this time back,
    /// so a cooked copy's check against its source's length and time
    /// passes without hashing the source (`assets::cooked`).
    modified: u64,
}

impl PackFile {
    fn read(name: String, path: &Path) -> Result<Self, PackError> {
        let read_error = |source| PackError::Read {
            path: path.to_owned(),
            source,
        };
        let contents = fs::read(path).map_err(read_error)?;
        let modified = fs::metadata(path)
            .ok()
            .and_then(|meta| meta.modified().ok())
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |since| u64::try_from(since.as_nanos()).unwrap_or(0));
        Ok(Self {
            name,
            contents,
            modified,
        })
    }
}

/// Packs the runtime files of a project into one portable game file.
///
/// Editor settings and unrelated documents are excluded. Textures go with
/// their current cooked copies (`.axcenity/imported/textures/`, roadmap L2),
/// so the game uploads them as they are instead of decoding them, and
/// models with their cached simpler copies (`.axcenity/imported/meshes/`)
/// and cooked whole models (`.axcenity/imported/models/`), so the game
/// doesn't parse or simplify them again. Block JSON is
/// translated into Lua in the archive so an exported player only needs the
/// normal script loader.
pub fn pack_game(folder: &Path) -> Result<Vec<u8>, PackError> {
    let mut files = Vec::new();
    collect_runtime_files(folder, folder, &mut files)?;
    add_cooked_textures(folder, &mut files)?;
    add_asset_index(folder, &mut files);
    files.sort_by(|a, b| a.name.cmp(&b.name));
    if files.len() > MAX_FILES {
        return Err(PackError::TooManyFiles);
    }

    let mut names = HashSet::new();
    let mut output = Vec::new();
    output.extend_from_slice(MAGIC);
    output.extend_from_slice(&VERSION.to_le_bytes());
    output.extend_from_slice(&(files.len() as u32).to_le_bytes());
    for file in files {
        let name = file.name;
        if is_block_script(&name) {
            let script = read_block_script(folder, &name).map_err(|error| PackError::Blocks {
                path: name.clone(),
                message: error.to_string(),
            })?;
            let source = translate(&script)
                .map_err(|error| PackError::Blocks {
                    path: name.clone(),
                    message: error.to_string(),
                })?
                .source;
            let translated_name = block_lua_path(&name).ok_or(PackError::InvalidArchive)?;
            if !names.insert(translated_name.clone()) {
                return Err(PackError::InvalidArchive);
            }
            append_entry(&mut output, &translated_name, source.as_bytes(), 0)?;
            continue;
        }
        if !names.insert(name.clone()) {
            return Err(PackError::InvalidArchive);
        }
        append_entry(&mut output, &name, &file.contents, file.modified)?;
    }
    Ok(output)
}

/// Extracts a packed game into an empty folder, refusing paths that could
/// escape it and duplicate entries that could overwrite one another.
///
/// The whole index is read and checked first; then the files are written
/// on a few threads, since unpacking a compressed entry is mostly zstd's
/// time (on a 4-core machine, 3x faster for a 750 MB game).
pub fn unpack_game(bytes: &[u8], folder: &Path) -> Result<(), PackError> {
    let entries = read_entries(bytes)?;
    fs::create_dir_all(folder).map_err(|source| PackError::Write {
        path: folder.to_owned(),
        source,
    })?;
    let mut made = HashSet::new();
    for entry in &entries {
        let parent = folder
            .join(entry.name)
            .parent()
            .ok_or(PackError::InvalidArchive)?
            .to_owned();
        if made.insert(parent.clone()) {
            fs::create_dir_all(&parent).map_err(|source| PackError::Write {
                path: parent,
                source,
            })?;
        }
    }
    let next = std::sync::atomic::AtomicUsize::new(0);
    let threads = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(8)
        .min(entries.len().max(1));
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    loop {
                        let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(entry) = entries.get(index) else {
                            return Ok(());
                        };
                        if let Err(error) = write_entry(entry, folder) {
                            // Stops the others at their next entry.
                            next.store(entries.len(), std::sync::atomic::Ordering::Relaxed);
                            return Err(error);
                        }
                    }
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().expect("unpacking threads don't panic"))
            .collect::<Result<Vec<()>, PackError>>()
    })?;
    Ok(())
}

/// One entry of a game file, checked but not yet written.
struct Entry<'a> {
    name: &'a str,
    flags: u8,
    /// Nanoseconds since 1970; 0 for none.
    modified: u64,
    data: &'a [u8],
    file_len: u64,
}

/// Every entry of `bytes`, with its path checked as safe and unique.
fn read_entries(bytes: &[u8]) -> Result<Vec<Entry<'_>>, PackError> {
    let mut cursor = 0;
    if take(bytes, &mut cursor, MAGIC.len())? != MAGIC {
        return Err(PackError::InvalidArchive);
    }
    let version = read_u32(bytes, &mut cursor)?;
    if version != 1 && version != VERSION {
        return Err(PackError::UnsupportedVersion(version));
    }
    let count = read_u32(bytes, &mut cursor)? as usize;
    if count > MAX_FILES {
        return Err(PackError::TooManyFiles);
    }
    let mut names = HashSet::new();
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let path_len = read_u32(bytes, &mut cursor)? as usize;
        let (flags, modified, data_len, file_len) = if version == 1 {
            let data_len = read_u64(bytes, &mut cursor)?;
            (0, 0, data_len, data_len)
        } else {
            let flags = take(bytes, &mut cursor, 1)?[0];
            let modified = read_u64(bytes, &mut cursor)?;
            let data_len = read_u64(bytes, &mut cursor)?;
            let file_len = read_u64(bytes, &mut cursor)?;
            (flags, modified, data_len, file_len)
        };
        if path_len == 0
            || path_len > MAX_PATH_BYTES
            || flags & !COMPRESSED != 0
            || file_len > MAX_FILE_BYTES
            || (flags & COMPRESSED == 0 && file_len != data_len)
        {
            return Err(PackError::InvalidArchive);
        }
        let data_len = usize::try_from(data_len).map_err(|_| PackError::InvalidArchive)?;
        let name = std::str::from_utf8(take(bytes, &mut cursor, path_len)?)
            .map_err(|_| PackError::InvalidArchive)?;
        if !safe_entry_path(name) || !names.insert(name) {
            return Err(PackError::InvalidArchive);
        }
        let data = take(bytes, &mut cursor, data_len)?;
        entries.push(Entry {
            name,
            flags,
            modified,
            data,
            file_len,
        });
    }
    if cursor != bytes.len() {
        return Err(PackError::InvalidArchive);
    }
    Ok(entries)
}

/// Writes `entry` as a new file under `folder` (its folder already made).
fn write_entry(entry: &Entry<'_>, folder: &Path) -> Result<(), PackError> {
    let path = folder.join(entry.name);
    let write_error = |source| PackError::Write {
        path: path.clone(),
        source,
    };
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(write_error)?;
    if entry.flags & COMPRESSED != 0 {
        // Streamed into the file, never more than the entry says: a damaged
        // frame can't make it allocate or write without end, and zstd's
        // checksum refuses one that decodes to other bytes.
        let decoder = zstd::stream::read::Decoder::with_buffer(entry.data)
            .map_err(|_| PackError::InvalidArchive)?;
        let written = io::copy(&mut decoder.take(entry.file_len + 1), &mut file)
            .map_err(|_| PackError::InvalidArchive)?;
        if written != entry.file_len {
            return Err(PackError::InvalidArchive);
        }
    } else {
        file.write_all(entry.data).map_err(write_error)?;
    }
    if entry.modified != 0 {
        // Best effort: on a file system without times, the cooked copies'
        // check falls back to hashing their sources.
        let _ = file.set_modified(UNIX_EPOCH + Duration::from_nanos(entry.modified));
    }
    Ok(())
}

fn collect_runtime_files(
    root: &Path,
    directory: &Path,
    files: &mut Vec<PackFile>,
) -> Result<(), PackError> {
    let entries = fs::read_dir(directory).map_err(|source| PackError::Read {
        path: directory.to_owned(),
        source,
    })?;
    let mut entries: Vec<_> =
        entries
            .collect::<Result<_, _>>()
            .map_err(|source| PackError::Read {
                path: directory.to_owned(),
                source,
            })?;
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let file_type = entry.file_type().map_err(|source| PackError::Read {
            path: entry.path(),
            source,
        })?;
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        if file_type.is_dir() {
            let name = entry.file_name();
            if directory == root
                && matches!(
                    name.to_str(),
                    Some(".git" | ".vscode" | ".axcenity" | "target" | crate::PLUGINS_FOLDER)
                )
            {
                continue;
            }
            collect_runtime_files(root, &path, files)?;
            continue;
        }
        if !file_type.is_file() || !include_runtime_file(root, &path) {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .ok()
            .and_then(Path::to_str)
            .map(|path| path.replace('\\', "/"))
            .ok_or(PackError::InvalidArchive)?;
        if !safe_entry_path(&relative) {
            return Err(PackError::InvalidArchive);
        }
        files.push(PackFile::read(relative, &path)?);
        if files.len() > MAX_FILES {
            return Err(PackError::TooManyFiles);
        }
    }
    Ok(())
}

/// Adds [`axcenity_assets::ASSET_INDEX_FILE`], each asset ID's path, so the
/// player finds a file a reference names by its ID without the sidecars.
/// A project with no IDs gets none.
fn add_asset_index(root: &Path, files: &mut Vec<PackFile>) {
    files.retain(|file| file.name != axcenity_assets::ASSET_INDEX_FILE);
    let ids = axcenity_assets::asset_ids(root);
    if ids.is_empty() {
        return;
    }
    files.push(PackFile {
        name: axcenity_assets::ASSET_INDEX_FILE.to_owned(),
        contents: axcenity_assets::encode_asset_index(&ids),
        modified: 0,
    });
}

/// Adds the current cooked copy of each texture in `files`, at the same
/// place in the project, so the player finds it (it checks the copy against
/// the texture's length and modified time, which unpacking gives back).
fn add_cooked_textures(root: &Path, files: &mut Vec<PackFile>) -> Result<(), PackError> {
    let mut cooked: Vec<String> = Vec::new();
    for name in files
        .iter()
        .map(|file| &file.name)
        .filter(|name| name.starts_with("assets/"))
    {
        if is_texture_file(name) && axcenity_assets::is_cooked(root, name) {
            cooked.push(axcenity_assets::cooked_relative_path(name));
        } else if axcenity_assets::ModelFormat::of(Path::new(name)).is_some() {
            cooked.extend(axcenity_assets::cached_meshes(root, name));
            cooked.extend(axcenity_assets::cooked_models(root, name));
            cooked.extend(axcenity_assets::embedded_cooked_files(root, name));
        }
    }
    // Models that embed the same image share its cooked copy.
    cooked.sort();
    cooked.dedup();
    for name in cooked {
        let path = root.join(&name);
        files.push(PackFile::read(name, &path)?);
        if files.len() > MAX_FILES {
            return Err(PackError::TooManyFiles);
        }
    }
    Ok(())
}

fn is_texture_file(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, extension)| {
        axcenity_assets::TEXTURE_EXTENSIONS
            .iter()
            .any(|known| known.eq_ignore_ascii_case(extension))
    })
}

fn include_runtime_file(root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    let components: Vec<_> = relative.components().collect();
    let Some(Component::Normal(first)) = components.first() else {
        return false;
    };
    // The Model Studio's sidecars (and sidecar folders of paint layers)
    // are editing state; games load the `.glb`. Asset IDs' sidecars are
    // left out too: the game gets them all in one index instead
    // (`add_asset_index`).
    let lower = relative.to_string_lossy().to_ascii_lowercase();
    if lower.ends_with(axcenity_assets::MESH_EDIT_SUFFIX)
        || axcenity_assets::is_import_file(&lower)
        || components.iter().any(|component| {
            component
                .as_os_str()
                .to_string_lossy()
                .ends_with(axcenity_assets::MESH_EDIT_FOLDER_SUFFIX)
        })
    {
        return false;
    }
    // UI Event Graphs stay graphs: loading compiles each to the
    // LocalScript under its ScreenGui, as it does for a project folder.
    if matches!(
        first.to_str(),
        Some(
            "scripts"
                | "assets"
                | crate::LEVELS_FOLDER
                | axcenity_blocks_runtime::ui_graph::UI_GRAPHS_FOLDER
        )
    ) {
        return true;
    }
    // Stylesheets and fonts can be anywhere: UI text finds a font by its
    // family, wherever its file is.
    relative == Path::new(crate::MANIFEST_FILE)
        || relative == Path::new(crate::WORLD_FILE)
        || relative == Path::new(crate::STRINGS_FILE)
        || relative == Path::new(crate::INPUT_FILE)
        || relative == Path::new(crate::COLLISION_GROUPS_FILE)
        || relative.extension().is_some_and(|extension| {
            let extension = extension.to_ascii_lowercase();
            extension == "css" || extension == "ttf" || extension == "otf"
        })
}

fn block_lua_path(path: &str) -> Option<String> {
    for suffix in [".server.blocks.json", ".client.blocks.json"] {
        if let Some(base) = path.strip_suffix(suffix) {
            let script_side = suffix
                .strip_suffix(".blocks.json")
                .expect("block suffix ends in `.blocks.json`");
            return Some(format!("{base}{script_side}.lua"));
        }
    }
    None
}

/// Writes one version-2 entry: the path's length, flags, modified time,
/// the stored data's length and the file's length, the path, the data.
fn append_entry(
    output: &mut Vec<u8>,
    name: &str,
    contents: &[u8],
    modified: u64,
) -> Result<(), PackError> {
    if !safe_entry_path(name) || name.len() > MAX_PATH_BYTES {
        return Err(PackError::InvalidArchive);
    }
    // With zstd's checksum, so a damaged entry fails to unpack instead of
    // giving the game other bytes.
    let compressed = zstd::bulk::Compressor::new(ZSTD_LEVEL)
        .and_then(|mut compressor| {
            compressor.include_checksum(true)?;
            compressor.compress(contents)
        })
        .ok()
        .filter(|packed| (packed.len() as f64) <= contents.len() as f64 * (1.0 - MIN_SAVING));
    let (flags, data) = match &compressed {
        Some(packed) => (COMPRESSED, packed.as_slice()),
        None => (0, contents),
    };
    let path_len = u32::try_from(name.len()).map_err(|_| PackError::InvalidArchive)?;
    let data_len = u64::try_from(data.len()).map_err(|_| PackError::InvalidArchive)?;
    let file_len = u64::try_from(contents.len()).map_err(|_| PackError::InvalidArchive)?;
    if file_len > MAX_FILE_BYTES {
        return Err(PackError::InvalidArchive);
    }
    output.extend_from_slice(&path_len.to_le_bytes());
    output.push(flags);
    output.extend_from_slice(&modified.to_le_bytes());
    output.extend_from_slice(&data_len.to_le_bytes());
    output.extend_from_slice(&file_len.to_le_bytes());
    output.extend_from_slice(name.as_bytes());
    output.extend_from_slice(data);
    Ok(())
}

fn read_u32(bytes: &[u8], cursor: &mut usize) -> Result<u32, PackError> {
    Ok(u32::from_le_bytes(
        take(bytes, cursor, 4)?.try_into().unwrap(),
    ))
}

fn read_u64(bytes: &[u8], cursor: &mut usize) -> Result<u64, PackError> {
    Ok(u64::from_le_bytes(
        take(bytes, cursor, 8)?.try_into().unwrap(),
    ))
}

fn safe_entry_path(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('\\')
        && !name.contains(':')
        && !name.starts_with('/')
        && name
            .split('/')
            .all(|component| !component.is_empty() && component != "." && component != "..")
}

fn take<'a>(bytes: &'a [u8], cursor: &mut usize, length: usize) -> Result<&'a [u8], PackError> {
    let end = cursor
        .checked_add(length)
        .filter(|&end| end <= bytes.len())
        .ok_or(PackError::InvalidArchive)?;
    let result = &bytes[*cursor..end];
    *cursor = end;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_blocks_runtime::{Block, BlockKind, BlockScript};

    #[test]
    fn a_built_game_has_models_but_not_their_sidecars() {
        let root = Path::new("game");
        assert!(include_runtime_file(
            root,
            &root.join("assets/models/Head.glb")
        ));
        assert!(!include_runtime_file(
            root,
            &root.join("assets/models/Head.meshedit.json")
        ));
        // Its paint layers aren't either; the saved maps are.
        assert!(!include_runtime_file(
            root,
            &root.join("assets/models/Head.meshedit/0-0-BaseColor-1.png")
        ));
        assert!(include_runtime_file(
            root,
            &root.join("assets/models/textures/Head_Head_Skin_BaseColor.png")
        ));
        // Asset IDs' sidecars stay behind too.
        assert!(!include_runtime_file(
            root,
            &root.join("assets/models/Head.glb.import.json")
        ));
    }

    fn temp_folder(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("axcenity-pack-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn a_game_round_trips_assets_styles_scripts_and_translated_blocks() {
        let source = temp_folder("source");
        let output = temp_folder("output");
        fs::write(
            source.join("axcenity.json"),
            format!(
                r#"{{"name":"Game","format_version":{}}}"#,
                crate::FORMAT_VERSION
            ),
        )
        .unwrap();
        fs::write(source.join("world.json"), r#"{"format_version":1}"#).unwrap();
        fs::create_dir_all(source.join("scripts")).unwrap();
        fs::create_dir_all(source.join("assets/audio")).unwrap();
        fs::write(source.join("scripts/start.server.lua"), "print('hello')").unwrap();
        fs::write(source.join("assets/audio/chime.wav"), [1, 2, 3]).unwrap();
        fs::write(source.join("ui.css"), "Frame { opacity: 1 }").unwrap();
        fs::create_dir_all(source.join("ui")).unwrap();
        fs::write(source.join("ui/Title.ttf"), [4, 5]).unwrap();
        fs::write(source.join(".luarc.json"), "{}").unwrap();
        let blocks = BlockScript {
            blocks: vec![Block::new(BlockKind::WhenStarted {
                body: vec![Block::new(BlockKind::Wait {
                    seconds: axcenity_blocks_runtime::Expression::Number { value: 1.0 },
                })],
            })],
            ..Default::default()
        };
        fs::write(
            source.join("scripts/logic.server.blocks.json"),
            serde_json::to_vec(&blocks).unwrap(),
        )
        .unwrap();

        let packed = pack_game(&source).unwrap();
        unpack_game(&packed, &output).unwrap();

        assert_eq!(
            fs::read_to_string(output.join("scripts/start.server.lua")).unwrap(),
            "print('hello')"
        );
        assert!(
            fs::read_to_string(output.join("scripts/logic.server.lua"))
                .unwrap()
                .contains("task.wait(1)")
        );
        assert!(!output.join("scripts/logic.server.blocks.json").exists());
        assert!(!output.join(".luarc.json").exists());
        assert_eq!(
            fs::read(output.join("assets/audio/chime.wav")).unwrap(),
            [1, 2, 3]
        );
        assert_eq!(
            fs::read_to_string(output.join("ui.css")).unwrap(),
            "Frame { opacity: 1 }"
        );
        assert_eq!(fs::read(output.join("ui/Title.ttf")).unwrap(), [4, 5]);
        fs::remove_dir_all(source).unwrap();
        fs::remove_dir_all(output).unwrap();
    }

    #[test]
    fn a_built_game_carries_a_models_maps_cooked_in_place_and_the_player_reads_them() {
        let project = temp_folder("embedded");
        fs::create_dir_all(project.join("assets/models")).unwrap();
        fs::write(project.join(crate::MANIFEST_FILE), "{}").unwrap();
        let model = "assets/models/Helmet.glb";
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/hello-pbr/assets/models/DamagedHelmet/DamagedHelmet.glb"),
            project.join(model),
        )
        .unwrap();
        let bytes = fs::read(project.join(model)).unwrap();
        let images =
            axcenity_assets::embedded_files(&bytes, axcenity_assets::ModelFormat::Gltf).unwrap();
        let (_, image) = &images[0];
        let cooked = axcenity_assets::CookedTexture {
            format: axcenity_assets::CookedFormat::Bc7,
            width: 4,
            height: 4,
            levels: vec![vec![1; 16], vec![2; 16], vec![3; 16]],
            has_alpha: false,
            average: [0; 4],
        };
        axcenity_assets::write_embedded_cooked(&project, image, &cooked).unwrap();
        let stamp = axcenity_assets::SourceStamp::of(&project.join(model), &bytes);
        let hash = axcenity_assets::content_hash(image);
        axcenity_assets::mark_embedded_cooked(&project, model, stamp, true, &[hash]).unwrap();
        let copies = axcenity_assets::embedded_cooked_files(&project, model);
        assert_eq!(copies.len(), 2, "the copy and the model's note: {copies:?}");

        let unpacked = temp_folder("embedded-unpacked");
        let _ = fs::remove_dir_all(&unpacked);
        unpack_game(&pack_game(&project).unwrap(), &unpacked).unwrap();
        assert_eq!(
            axcenity_assets::embedded_cooked_files(&unpacked, model),
            copies
        );
        // Matched by the image's bytes, so unpacking's new times don't matter.
        assert_eq!(
            axcenity_assets::embedded_cooked_format(&unpacked, image),
            Some(axcenity_assets::CookedFormat::Bc7)
        );
        let _ = fs::remove_dir_all(project);
        let _ = fs::remove_dir_all(unpacked);
    }

    #[test]
    fn a_built_game_finds_a_file_by_its_asset_id_without_the_sidecars() {
        let project = temp_folder("asset-ids");
        fs::create_dir_all(project.join("assets/audio")).unwrap();
        fs::write(project.join(crate::MANIFEST_FILE), "{}").unwrap();
        let sound = "assets/audio/chime.wav";
        fs::write(project.join(sound), b"RIFF").unwrap();
        let id = axcenity_assets::AssetId::new();
        fs::write(
            project.join(axcenity_assets::import_path(sound)),
            axcenity_assets::encode_import_file(id),
        )
        .unwrap();

        let unpacked = temp_folder("asset-ids-unpacked");
        unpack_game(&pack_game(&project).unwrap(), &unpacked).unwrap();
        assert!(!unpacked.join(axcenity_assets::import_path(sound)).exists());
        let assets = axcenity_assets::Assets::new(&unpacked);
        assert_eq!(&*assets.resolve(&id.to_string()), sound);
        assert_eq!(assets.read_file(&id.to_string()).unwrap(), b"RIFF");

        // No IDs, no index.
        fs::remove_file(project.join(axcenity_assets::import_path(sound))).unwrap();
        let plain = temp_folder("asset-ids-none");
        unpack_game(&pack_game(&project).unwrap(), &plain).unwrap();
        assert!(!plain.join(axcenity_assets::ASSET_INDEX_FILE).exists());
        for folder in [project, unpacked, plain] {
            let _ = fs::remove_dir_all(folder);
        }
    }

    #[test]
    fn a_built_game_carries_current_cooked_textures_and_the_player_reads_them() {
        let project = temp_folder("cooked");
        fs::create_dir_all(project.join("assets/textures")).unwrap();
        fs::write(project.join(crate::MANIFEST_FILE), "{}").unwrap();
        let fresh = "assets/textures/fresh.png";
        let stale = "assets/textures/stale.png";
        for name in [fresh, stale] {
            fs::write(project.join(name), name.as_bytes()).unwrap();
        }
        let cooked = axcenity_assets::CookedTexture {
            format: axcenity_assets::CookedFormat::Bc7,
            width: 4,
            height: 4,
            levels: vec![vec![1; 16], vec![2; 16], vec![3; 16]],
            has_alpha: false,
            average: [0; 4],
        };
        let stamp =
            |name: &str| axcenity_assets::SourceStamp::of(&project.join(name), name.as_bytes());
        axcenity_assets::write_cooked(&project, fresh, stamp(fresh), &cooked).unwrap();
        // Cooked from bytes the file no longer has.
        let mut old = stamp(stale);
        old.hash ^= 1;
        old.modified ^= 1;
        axcenity_assets::write_cooked(&project, stale, old, &cooked).unwrap();

        let game = pack_game(&project).unwrap();
        let unpacked = temp_folder("cooked-unpacked");
        let _ = fs::remove_dir_all(&unpacked);
        unpack_game(&game, &unpacked).unwrap();
        // Unpacked files are newer: the copy is matched by the texture's bytes.
        let read = axcenity_assets::read_cooked(&unpacked, fresh).expect("packed and current");
        assert_eq!(read.levels, cooked.levels);
        assert!(!axcenity_assets::cooked_path(&unpacked, stale).exists());
        let _ = fs::remove_dir_all(project);
        let _ = fs::remove_dir_all(unpacked);
    }

    #[test]
    fn a_built_game_carries_its_models_cooked_copies_and_simpler_copies() {
        let model = "assets/models/CityPropsCollection.fbx";
        let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-mesh");
        let project = temp_folder("mesh-cache");
        let _ = fs::remove_dir_all(&project);
        fs::create_dir_all(project.join("assets/models")).unwrap();
        fs::write(project.join(crate::MANIFEST_FILE), "{}").unwrap();
        fs::copy(example.join(model), project.join(model)).unwrap();
        // Its textures, so it loads without warnings and is kept whole.
        let textures = project.join("assets/textures");
        fs::create_dir_all(&textures).unwrap();
        for entry in fs::read_dir(example.join("assets/textures"))
            .unwrap()
            .flatten()
        {
            fs::copy(entry.path(), textures.join(entry.file_name())).unwrap();
        }
        let cached = |folder: &Path| -> Vec<String> {
            let mut names = Vec::new();
            for kind in [
                axcenity_assets::MESH_CACHE_FOLDER,
                axcenity_assets::MODEL_CACHE_FOLDER,
            ] {
                for entry in fs::read_dir(folder.join(kind))
                    .into_iter()
                    .flatten()
                    .flatten()
                {
                    names.push(format!("{kind}/{}", entry.file_name().to_string_lossy()));
                }
            }
            names.sort();
            names
        };
        // Loading it in the editor simplifies it and keeps the copies, and
        // keeps the whole model.
        let assets = axcenity_assets::Assets::new(&project);
        let made = assets.model_blocking(model).expect("loads");
        assert!(made.warnings.is_empty(), "{:?}", made.warnings);
        assert!(
            made.primitives
                .iter()
                .any(|primitive| !primitive.lods.is_empty())
        );
        assert_eq!(cached(&project).len(), 2, "{:?}", cached(&project));

        let unpacked = temp_folder("mesh-cache-unpacked");
        let _ = fs::remove_dir_all(&unpacked);
        unpack_game(&pack_game(&project).unwrap(), &unpacked).unwrap();
        assert_eq!(cached(&unpacked), cached(&project));
        // The game reads them: it writes nothing new, and gets the same model.
        let times = || -> Vec<std::time::SystemTime> {
            let files = cached(&unpacked).into_iter();
            files
                .map(|file| {
                    fs::metadata(unpacked.join(file))
                        .unwrap()
                        .modified()
                        .unwrap()
                })
                .collect()
        };
        let shipped = times();
        let read = axcenity_assets::Assets::new(&unpacked)
            .model_blocking(model)
            .expect("loads");
        assert_eq!(times(), shipped);
        let triangles = |model: &axcenity_assets::Model| -> Vec<usize> {
            let primitives = model.primitives.iter();
            primitives
                .flat_map(|primitive| {
                    std::iter::once(primitive.indices.len())
                        .chain(primitive.lods.iter().map(|copy| copy.indices.len()))
                })
                .collect()
        };
        assert_eq!(triangles(&read), triangles(&made));
        let _ = fs::remove_dir_all(project);
        let _ = fs::remove_dir_all(unpacked);
    }

    #[test]
    fn a_built_game_has_every_data_table() {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-data");
        let output = temp_folder("data-tables");
        unpack_game(&pack_game(&source).unwrap(), &output).unwrap();
        for name in ["weapons", "ammo"] {
            let path = axcenity_assets::data_table_path(name);
            assert_eq!(
                fs::read(output.join(&path)).unwrap(),
                fs::read(source.join(&path)).unwrap(),
                "{path}"
            );
        }
        // Scripts read them from the unpacked game as they do from the project.
        let assets = axcenity_assets::Assets::new(&output);
        let weapons = assets.data_table("weapons").unwrap();
        assert_eq!(weapons.rows.len(), 3);
        let _ = fs::remove_dir_all(&output);
    }

    #[test]
    fn hello_touched_blocks_exports_translated_lua() {
        let source =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-touched-blocks");
        let packed = pack_game(&source).unwrap();
        let output = temp_folder("hello-touched-blocks-export");
        unpack_game(&packed, &output).unwrap();
        let lua = fs::read_to_string(output.join("scripts/pad.server.lua")).unwrap();
        assert!(
            lua.contains("Touched:Connect") && lua.contains("Neon") && lua.contains("listening"),
            "{lua}"
        );
        assert!(!output.join("scripts/pad.server.blocks.json").exists());
        if let Ok(dest) = std::env::var("AXCENITY_EXPORT_SMOKE") {
            let dest = PathBuf::from(dest);
            fs::create_dir_all(&dest).unwrap();
            fs::write(dest.join("game.axgame"), packed).unwrap();
        }
        fs::remove_dir_all(output).unwrap();
    }

    #[test]
    fn unpacking_rejects_traversal_and_trailing_bytes() {
        let folder = temp_folder("unsafe");
        // Version 1's entry layout, then version 2's.
        let mut archive = Vec::new();
        archive.extend_from_slice(MAGIC);
        archive.extend_from_slice(&1_u32.to_le_bytes());
        archive.extend_from_slice(&1_u32.to_le_bytes());
        archive.extend_from_slice(&9_u32.to_le_bytes());
        archive.extend_from_slice(&3_u64.to_le_bytes());
        archive.extend_from_slice(b"../escape");
        archive.extend_from_slice(b"bad");
        assert!(matches!(
            unpack_game(&archive, &folder),
            Err(PackError::InvalidArchive)
        ));
        let mut archive = Vec::new();
        archive.extend_from_slice(MAGIC);
        archive.extend_from_slice(&VERSION.to_le_bytes());
        archive.extend_from_slice(&1_u32.to_le_bytes());
        archive.extend_from_slice(&9_u32.to_le_bytes());
        archive.push(0);
        archive.extend_from_slice(&0_u64.to_le_bytes());
        archive.extend_from_slice(&3_u64.to_le_bytes());
        archive.extend_from_slice(&3_u64.to_le_bytes());
        archive.extend_from_slice(b"../escape");
        archive.extend_from_slice(b"bad");
        assert!(matches!(
            unpack_game(&archive, &folder),
            Err(PackError::InvalidArchive)
        ));
        assert!(!folder.parent().unwrap().join("escape").exists());

        let mut trailing = Vec::new();
        trailing.extend_from_slice(MAGIC);
        trailing.extend_from_slice(&VERSION.to_le_bytes());
        trailing.extend_from_slice(&0_u32.to_le_bytes());
        trailing.push(0);
        assert!(matches!(
            unpack_game(&trailing, &folder),
            Err(PackError::InvalidArchive)
        ));
        fs::remove_dir_all(folder).unwrap();
    }

    /// A version-2 archive of `entries` (name, contents, modified).
    fn archive_of(entries: &[(&str, &[u8], u64)]) -> Vec<u8> {
        let mut archive = Vec::new();
        archive.extend_from_slice(MAGIC);
        archive.extend_from_slice(&VERSION.to_le_bytes());
        archive.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        for (name, contents, modified) in entries {
            append_entry(&mut archive, name, contents, *modified).unwrap();
        }
        archive
    }

    #[test]
    fn text_is_stored_compressed_and_already_compressed_data_as_it_is() {
        let text = r#"{"Name":"Part","Anchored":true},"#.repeat(2000);
        // Bytes no compressor can shrink, as in a PNG or JPEG.
        let mut noise = Vec::new();
        let mut state = 0x2545_f491_4f6c_dd1d_u64;
        for _ in 0..20_000 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            noise.push(state as u8);
        }
        let archive = archive_of(&[
            ("world.json", text.as_bytes(), 0),
            ("assets/noise.png", &noise, 0),
        ]);
        assert!(
            archive.len() < noise.len() + text.len() / 10,
            "{} bytes",
            archive.len()
        );
        let folder = temp_folder("compressed");
        unpack_game(&archive, &folder).unwrap();
        assert_eq!(fs::read_to_string(folder.join("world.json")).unwrap(), text);
        assert_eq!(fs::read(folder.join("assets/noise.png")).unwrap(), noise);
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn unpacked_files_get_their_modified_times_back() {
        let source = temp_folder("times-source");
        fs::write(
            source.join("axcenity.json"),
            format!(
                r#"{{"name":"Game","format_version":{}}}"#,
                crate::FORMAT_VERSION
            ),
        )
        .unwrap();
        fs::write(source.join("world.json"), r#"{"format_version":1}"#).unwrap();
        let when = UNIX_EPOCH + Duration::from_secs(1_700_000_000) + Duration::from_micros(123_456);
        fs::File::options()
            .write(true)
            .open(source.join("world.json"))
            .unwrap()
            .set_modified(when)
            .unwrap();
        let output = temp_folder("times-output");
        unpack_game(&pack_game(&source).unwrap(), &output).unwrap();
        let modified = fs::metadata(output.join("world.json"))
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(modified, when);
        fs::remove_dir_all(source).unwrap();
        fs::remove_dir_all(output).unwrap();
    }

    #[test]
    fn a_version_1_game_file_still_unpacks() {
        let mut archive = Vec::new();
        archive.extend_from_slice(MAGIC);
        archive.extend_from_slice(&1_u32.to_le_bytes());
        archive.extend_from_slice(&1_u32.to_le_bytes());
        archive.extend_from_slice(&10_u32.to_le_bytes());
        archive.extend_from_slice(&5_u64.to_le_bytes());
        archive.extend_from_slice(b"world.json");
        archive.extend_from_slice(b"{\"a\"}");
        let folder = temp_folder("version-1");
        unpack_game(&archive, &folder).unwrap();
        assert_eq!(fs::read(folder.join("world.json")).unwrap(), b"{\"a\"}");
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_damaged_or_mislabelled_compressed_entry_is_refused() {
        let text = "print('hello')\n".repeat(500);
        let good = archive_of(&[("scripts/a.server.lua", text.as_bytes(), 0)]);
        // The file length sits after the path length, flags, time and data
        // length: 12 + 4 + 1 + 8 + 8 bytes in.
        let file_len_at = MAGIC.len() + 8 + 4 + 1 + 8 + 8;
        assert_eq!(good[MAGIC.len() + 8 + 4], COMPRESSED);
        for (what, archive) in [
            ("says shorter", {
                let mut bad = good.clone();
                bad[file_len_at..file_len_at + 8].copy_from_slice(&10_u64.to_le_bytes());
                bad
            }),
            ("says longer", {
                let mut bad = good.clone();
                let longer = text.len() as u64 + 1;
                bad[file_len_at..file_len_at + 8].copy_from_slice(&longer.to_le_bytes());
                bad
            }),
            ("damaged frame", {
                let mut bad = good.clone();
                let last = bad.len() - 3;
                bad[last] ^= 0xff;
                bad
            }),
            ("unknown flag", {
                let mut bad = good.clone();
                bad[MAGIC.len() + 8 + 4] = 2;
                bad
            }),
        ] {
            let folder = temp_folder("damaged");
            assert!(
                matches!(
                    unpack_game(&archive, &folder),
                    Err(PackError::InvalidArchive)
                ),
                "{what}"
            );
            fs::remove_dir_all(folder).unwrap();
        }
    }
}
