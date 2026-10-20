use std::collections::HashSet;
use std::fs;
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};

use axcenity_blocks::translate;
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
/// Editor settings and unrelated documents are excluded. Block JSON is
/// translated into Lua in the archive so an exported player only needs the
/// normal script loader.
pub fn pack_game(folder: &Path) -> Result<Vec<u8>, PackError> {
    let mut files = Vec::new();
    collect_runtime_files(folder, folder, &mut files)?;
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
                    Some(".git" | ".vscode" | ".axcenity" | "target")
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

fn include_runtime_file(root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    let components: Vec<_> = relative.components().collect();
    let Some(Component::Normal(first)) = components.first() else {
        return false;
    };
    if matches!(first.to_str(), Some("scripts" | "assets")) {
        return true;
    }
    relative == Path::new(crate::MANIFEST_FILE)
        || relative == Path::new(crate::WORLD_FILE)
        || relative
            .extension()
            .is_some_and(|extension| extension == "css")
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
    use axcenity_blocks::{Block, BlockKind, BlockScript};

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
            r#"{"name":"Game","format_version":1}"#,
        )
        .unwrap();
        fs::write(source.join("world.json"), r#"{"format_version":1}"#).unwrap();
        fs::create_dir_all(source.join("scripts")).unwrap();
        fs::create_dir_all(source.join("assets/audio")).unwrap();
        fs::write(source.join("scripts/start.server.lua"), "print('hello')").unwrap();
        fs::write(source.join("assets/audio/chime.wav"), [1, 2, 3]).unwrap();
        fs::write(source.join("ui.css"), "Frame { opacity: 1 }").unwrap();
        fs::write(source.join(".luarc.json"), "{}").unwrap();
        let blocks = BlockScript {
            blocks: vec![Block::new(BlockKind::WhenStarted {
                body: vec![Block::new(BlockKind::Wait {
                    seconds: axcenity_blocks::Expression::Number { value: 1.0 },
                })],
            })],
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
        fs::remove_dir_all(source).unwrap();
        fs::remove_dir_all(output).unwrap();
    }

    #[test]
    fn hello_touched_blocks_exports_translated_lua() {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-touched-blocks");
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
