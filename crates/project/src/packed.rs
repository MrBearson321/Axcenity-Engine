use std::collections::HashSet;
use std::fs;
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};

use axcenity_blocks_runtime::translate;
use thiserror::Error;

use crate::{is_block_script, read_block_script};

const MAGIC: &[u8; 8] = b"AXCGAME\0";
const VERSION: u32 = 1;
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
    files.sort_by(|a, b| a.0.cmp(&b.0));
    if files.len() > MAX_FILES {
        return Err(PackError::TooManyFiles);
    }

    let mut names = HashSet::new();
    let mut output = Vec::new();
    output.extend_from_slice(MAGIC);
    output.extend_from_slice(&VERSION.to_le_bytes());
    output.extend_from_slice(&(files.len() as u32).to_le_bytes());
    for (name, contents) in files {
        let name = if is_block_script(&name) {
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
            append_entry(&mut output, &translated_name, source.as_bytes())?;
            continue;
        } else {
            name
        };
        if !names.insert(name.clone()) {
            return Err(PackError::InvalidArchive);
        }
        append_entry(&mut output, &name, &contents)?;
    }
    Ok(output)
}

/// Extracts a packed game into an empty folder, refusing paths that could
/// escape it and duplicate entries that could overwrite one another.
pub fn unpack_game(bytes: &[u8], folder: &Path) -> Result<(), PackError> {
    let mut cursor = 0;
    if take(bytes, &mut cursor, MAGIC.len())? != MAGIC {
        return Err(PackError::InvalidArchive);
    }
    let version = u32::from_le_bytes(take(bytes, &mut cursor, 4)?.try_into().unwrap());
    if version != VERSION {
        return Err(PackError::UnsupportedVersion(version));
    }
    let count = u32::from_le_bytes(take(bytes, &mut cursor, 4)?.try_into().unwrap()) as usize;
    if count > MAX_FILES {
        return Err(PackError::TooManyFiles);
    }
    fs::create_dir_all(folder).map_err(|source| PackError::Write {
        path: folder.to_owned(),
        source,
    })?;
    let mut names = HashSet::new();
    for _ in 0..count {
        let path_len =
            u32::from_le_bytes(take(bytes, &mut cursor, 4)?.try_into().unwrap()) as usize;
        let data_len = usize::try_from(u64::from_le_bytes(
            take(bytes, &mut cursor, 8)?.try_into().unwrap(),
        ))
        .map_err(|_| PackError::InvalidArchive)?;
        if path_len == 0 || path_len > MAX_PATH_BYTES {
            return Err(PackError::InvalidArchive);
        }
        let name = std::str::from_utf8(take(bytes, &mut cursor, path_len)?)
            .map_err(|_| PackError::InvalidArchive)?;
        if !safe_entry_path(name) || !names.insert(name.to_owned()) {
            return Err(PackError::InvalidArchive);
        }
        let contents = take(bytes, &mut cursor, data_len)?;
        let path = folder.join(name);
        let parent = path.parent().ok_or(PackError::InvalidArchive)?;
        fs::create_dir_all(parent).map_err(|source| PackError::Write {
            path: parent.to_owned(),
            source,
        })?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|source| PackError::Write {
                path: path.clone(),
                source,
            })?;
        file.write_all(contents)
            .map_err(|source| PackError::Write { path, source })?;
    }
    if cursor != bytes.len() {
        return Err(PackError::InvalidArchive);
    }
    Ok(())
}

fn collect_runtime_files(
    root: &Path,
    directory: &Path,
    files: &mut Vec<(String, Vec<u8>)>,
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
        let contents = fs::read(&path).map_err(|source| PackError::Read {
            path: path.clone(),
            source,
        })?;
        files.push((relative, contents));
        if files.len() > MAX_FILES {
            return Err(PackError::TooManyFiles);
        }
    }
    Ok(())
}

/// Adds the current cooked copy of each texture in `files`, at the same
/// place in the project, so the player finds it (it checks the copy against
/// the texture's bytes, since unpacking gives every file a new time).
fn add_cooked_textures(root: &Path, files: &mut Vec<(String, Vec<u8>)>) -> Result<(), PackError> {
    let mut cooked: Vec<String> = Vec::new();
    for (name, _) in files.iter().filter(|(name, _)| name.starts_with("assets/")) {
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
        let contents = fs::read(&path).map_err(|source| PackError::Read { path, source })?;
        files.push((name, contents));
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
    // are editing state; games load the `.glb`. So are asset IDs' sidecars
    // for now: games find files by path.
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

fn append_entry(output: &mut Vec<u8>, name: &str, contents: &[u8]) -> Result<(), PackError> {
    if !safe_entry_path(name) || name.len() > MAX_PATH_BYTES {
        return Err(PackError::InvalidArchive);
    }
    let path_len = u32::try_from(name.len()).map_err(|_| PackError::InvalidArchive)?;
    let data_len = u64::try_from(contents.len()).map_err(|_| PackError::InvalidArchive)?;
    output.extend_from_slice(&path_len.to_le_bytes());
    output.extend_from_slice(&data_len.to_le_bytes());
    output.extend_from_slice(name.as_bytes());
    output.extend_from_slice(contents);
    Ok(())
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
        let mut archive = Vec::new();
        archive.extend_from_slice(MAGIC);
        archive.extend_from_slice(&VERSION.to_le_bytes());
        archive.extend_from_slice(&1_u32.to_le_bytes());
        archive.extend_from_slice(&9_u32.to_le_bytes());
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
}
