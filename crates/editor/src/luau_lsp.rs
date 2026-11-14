//! The Luau language server the script tabs type-check with
//! (`luau_check.rs`): the version the engine pins, where a package puts it,
//! and downloading that same version for a build that doesn't carry it.
//!
//! `packaging/luau-lsp.toml` pins one luau-lsp release and each OS's
//! archive (size and SHA-256). Every package ships it as `tools/luau-lsp`
//! beside the editor (`packaging/stage.py`). An editor built from source
//! has none there, so Settings > External tools and a bar above a script
//! tab offer to download it into the editor's data folder
//! (`<data>/Tools/luau-lsp-<version>/`), checked against the same hash
//! before it's unpacked.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};

use axcenity_release::publish::Artifact;
use axcenity_updater::download::{self, Fetch};
use axcenity_updater::extract::{self, Format};
use serde::Deserialize;

/// The pinned release, as `packaging/luau-lsp.toml` writes it.
const MANIFEST: &str = include_str!("../../../packaging/luau-lsp.toml");

#[derive(Clone, Debug, Deserialize)]
pub struct Pinned {
    pub version: String,
    /// With `{version}` and `{file}` to fill in.
    url: String,
    /// By `<os>-<arch>`, such as `windows-x86_64`.
    assets: BTreeMap<String, Asset>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Asset {
    pub file: String,
    pub size: u64,
    pub sha256: String,
}

impl Pinned {
    /// This computer's archive, if luau-lsp is built for it.
    pub fn asset(&self) -> Option<&Asset> {
        self.assets.get(&platform())
    }

    pub fn url(&self, asset: &Asset) -> String {
        self.url
            .replace("{version}", &self.version)
            .replace("{file}", &asset.file)
    }
}

/// The version every package ships.
pub fn pinned() -> Pinned {
    toml::from_str(MANIFEST).expect("packaging/luau-lsp.toml is checked by a test")
}

/// `<os>-<arch>` for this build, as the manifest keys its archives.
fn platform() -> String {
    let os = match std::env::consts::OS {
        "macos" => "macos",
        "windows" => "windows",
        _ => "linux",
    };
    format!("{os}-{}", std::env::consts::ARCH)
}

/// `luau-lsp.exe` on Windows, else `luau-lsp`.
pub fn program_name() -> &'static str {
    if cfg!(windows) {
        "luau-lsp.exe"
    } else {
        "luau-lsp"
    }
}

/// Where a package puts it: `tools/` beside the editor program (inside
/// `Contents/MacOS` in the macOS app).
pub fn bundled() -> Option<PathBuf> {
    let program = std::env::current_exe().ok()?;
    Some(program.parent()?.join("tools").join(program_name()))
}

/// The editor's data folder, where a download goes.
pub fn data_folder() -> Option<PathBuf> {
    eframe::storage_dir(crate::APP_ID)
}

/// Where a download of the pinned version is kept under `data`.
pub fn downloaded(data: &Path) -> PathBuf {
    data.join("Tools")
        .join(format!("luau-lsp-{}", pinned().version))
        .join(program_name())
}

/// How far a download has got, shared with the thread doing it.
#[derive(Default)]
pub struct Progress {
    pub done: AtomicU64,
    pub total: AtomicU64,
    pub cancel: AtomicBool,
}

impl Progress {
    /// From 0 to 1.
    pub fn fraction(&self) -> f32 {
        let total = self.total.load(Ordering::Relaxed);
        if total == 0 {
            0.0
        } else {
            self.done.load(Ordering::Relaxed) as f32 / total as f32
        }
    }
}

/// A download running on a thread of its own.
pub struct Download {
    pub progress: Arc<Progress>,
    finished: Receiver<Result<PathBuf, String>>,
}

impl Download {
    /// Starts downloading the pinned luau-lsp into `data` over HTTPS.
    pub fn start(data: PathBuf) -> Download {
        let fetch = axcenity_updater::download::HttpFetch::new(&format!(
            "Axcenity-Editor/{}",
            env!("CARGO_PKG_VERSION")
        ));
        Download::start_with(Box::new(fetch), data)
    }

    /// Starts it with `fetch` answering instead of the network (tests).
    pub fn start_with(fetch: Box<dyn Fetch + Send>, data: PathBuf) -> Download {
        let progress = Arc::new(Progress::default());
        let shared = progress.clone();
        let (send, finished) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("luau-lsp download".to_owned())
            .spawn(move || {
                let _ = send.send(install(fetch.as_ref(), &data, &shared));
            });
        if let Err(err) = spawned {
            let (send, failed) = mpsc::channel();
            let _ = send.send(Err(err.to_string()));
            return Download {
                progress,
                finished: failed,
            };
        }
        Download { progress, finished }
    }

    /// The program once the download has finished, or why it failed.
    pub fn finished(&self) -> Option<Result<PathBuf, String>> {
        self.finished.try_recv().ok()
    }

    /// Waits for it to finish (tests).
    #[cfg(test)]
    pub fn wait(&self) -> Result<PathBuf, String> {
        self.finished
            .recv_timeout(std::time::Duration::from_secs(30))
            .unwrap_or_else(|_| Err("timed out".to_owned()))
    }
}

/// Downloads the pinned archive for this computer into `data/Tools`,
/// checks it, and unpacks the program. One already there is used as it is.
pub fn install(fetch: &dyn Fetch, data: &Path, progress: &Progress) -> Result<PathBuf, String> {
    let pinned = pinned();
    let asset = pinned
        .asset()
        .ok_or_else(|| format!("luau-lsp {} isn't built for {}", pinned.version, platform()))?;
    let program = downloaded(data);
    if program.is_file() {
        return Ok(program);
    }
    let tools = data.join("Tools");
    let archive = tools.join(format!("luau-lsp-{}-{}", pinned.version, asset.file));
    let artifact = Artifact {
        file: asset.file.clone(),
        sha256: asset.sha256.clone(),
        size: asset.size,
    };
    progress.total.store(asset.size, Ordering::Relaxed);
    download::download(
        fetch,
        &pinned.url(asset),
        &artifact,
        &archive,
        &mut |done, _| progress.done.store(done, Ordering::Relaxed),
        &progress.cancel,
    )
    .map_err(|err| err.to_string().replace("the update", "luau-lsp"))?;
    // Unpacked beside where it ends up, then moved there whole, so a
    // half-unpacked folder is never taken for the program.
    let folder = program.parent().unwrap_or(&tools).to_owned();
    let unpacking = folder.with_extension("unpacking");
    let _ = fs::remove_dir_all(&unpacking);
    extract::extract(&archive, Format::Zip, &unpacking).map_err(|err| err.to_string())?;
    let unpacked = unpacking.join(program_name());
    if !unpacked.is_file() {
        let _ = fs::remove_dir_all(&unpacking);
        return Err(format!("{} has no {}", asset.file, program_name()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&unpacked)
            .map_err(|err| err.to_string())?
            .permissions();
        permissions.set_mode(permissions.mode() | 0o755);
        fs::set_permissions(&unpacked, permissions).map_err(|err| err.to_string())?;
    }
    let _ = fs::remove_dir_all(&folder);
    fs::rename(&unpacking, &folder).map_err(|err| err.to_string())?;
    let _ = fs::remove_file(&archive);
    Ok(program)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    #[test]
    fn the_manifest_pins_every_package_and_matches_the_license() {
        let pinned = pinned();
        assert!(pinned.version.split('.').count() == 3, "{}", pinned.version);
        // The release builds and every host a source build runs on.
        for key in [
            "windows-x86_64",
            "macos-aarch64",
            "macos-x86_64",
            "linux-x86_64",
            "linux-aarch64",
        ] {
            let asset = &pinned.assets[key];
            assert_eq!(asset.sha256.len(), 64, "{key}");
            assert!(asset.sha256.chars().all(|c| c.is_ascii_hexdigit()));
            assert!(asset.file.ends_with(".zip") && asset.size > 0);
            assert!(pinned.url(asset).starts_with("https://"));
        }
        let license = include_str!("../../../packaging/luau-lsp-LICENSE.txt");
        assert!(license.starts_with("MIT License"));
    }

    /// Serves `body` for every request.
    struct Memory(Vec<u8>);

    impl Fetch for Memory {
        fn open(&self, _: &str, _: u64) -> Result<(Box<dyn Read>, bool), String> {
            Ok((Box::new(std::io::Cursor::new(self.0.clone())), false))
        }
    }

    fn zip_with(name: &str, contents: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut bytes));
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(contents).unwrap();
            zip.finish().unwrap();
        }
        bytes
    }

    #[test]
    fn a_download_that_doesnt_match_the_pinned_hash_is_never_unpacked() {
        let data = std::env::temp_dir().join(format!("axcenity-lsp-dl-{}", std::process::id()));
        let _ = fs::remove_dir_all(&data);
        let mut fake = zip_with(program_name(), b"not luau-lsp");
        // The pinned size, so only the hash tells it apart.
        if let Some(asset) = pinned().asset() {
            fake.resize(asset.size as usize, 0);
        }
        let download = Download::start_with(Box::new(Memory(fake)), data.clone());
        let failed = download.wait();
        assert!(failed.is_err(), "{failed:?}");
        assert!(!downloaded(&data).exists());
        // Nothing half-done is left to be taken for the program.
        let tools = data.join("Tools");
        let left: Vec<_> = fs::read_dir(&tools)
            .map(|entries| entries.flatten().map(|entry| entry.file_name()).collect())
            .unwrap_or_default();
        assert!(left.is_empty(), "{left:?}");
        let _ = fs::remove_dir_all(&data);
    }

    #[test]
    fn a_program_already_downloaded_is_used_without_fetching() {
        let data = std::env::temp_dir().join(format!("axcenity-lsp-have-{}", std::process::id()));
        let _ = fs::remove_dir_all(&data);
        let program = downloaded(&data);
        fs::create_dir_all(program.parent().unwrap()).unwrap();
        fs::write(&program, b"x").unwrap();
        struct Never;
        impl Fetch for Never {
            fn open(&self, _: &str, _: u64) -> Result<(Box<dyn Read>, bool), String> {
                panic!("nothing should be fetched")
            }
        }
        if pinned().asset().is_some() {
            assert_eq!(
                install(&Never, &data, &Progress::default()).unwrap(),
                program
            );
        }
        let _ = fs::remove_dir_all(&data);
    }

    /// Downloads the real pinned archive and runs it. Network, so only
    /// with `AXCENITY_LUAU_LSP_DOWNLOAD=1`; or, with
    /// `AXCENITY_LUAU_LSP_ARCHIVE` naming the archive already downloaded
    /// (behind a proxy rustls doesn't trust), unpacks and runs that.
    #[test]
    fn the_pinned_luau_lsp_downloads_and_runs() {
        let archive = std::env::var_os("AXCENITY_LUAU_LSP_ARCHIVE").map(PathBuf::from);
        if std::env::var_os("AXCENITY_LUAU_LSP_DOWNLOAD").is_none() && archive.is_none() {
            return;
        }
        let data = std::env::temp_dir().join(format!("axcenity-lsp-real-{}", std::process::id()));
        let _ = fs::remove_dir_all(&data);
        let download = match archive {
            Some(archive) => {
                Download::start_with(Box::new(Memory(fs::read(archive).unwrap())), data.clone())
            }
            None => Download::start(data.clone()),
        };
        let program = download.wait().unwrap();
        let output = std::process::Command::new(&program)
            .arg("--version")
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            pinned().version
        );
        let _ = fs::remove_dir_all(&data);
    }
}
