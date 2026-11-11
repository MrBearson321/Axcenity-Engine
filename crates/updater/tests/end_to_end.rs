//! The whole update path, as a real release would take it, with only the
//! network and the editor's window left out:
//!
//! 1. the release tools: `prepare` writes this version's record from the
//!    repository's `releases/`, and `Manifest::build` describes a package
//!    named as the release workflow names it;
//! 2. the manifest is signed with minisign, with a key made for this test
//!    (the real key never leaves CI);
//! 3. the update check reads a GitHub-shaped release list, picks this
//!    release, and verifies the manifest's signature;
//! 4. the download checks size and SHA-256, the archive is staged, and the
//!    real `axcenity-updater` installs it and checks it.
//!
//! The "editor" in the package is the updater binary, which answers
//! `--version --json` the same way. Runs on Linux (tar.gz) and Windows
//! (zip); everything happens in a temporary folder.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;

use axcenity_release::publish::{
    BuildFacts, HISTORY_FILE, HistoryFile, MANIFEST_FILE, Manifest, SIGNATURE_FILE,
};
use axcenity_release::updates::{self, UpdateError, asset_url, releases_url};
use axcenity_release::{History, InstallKind, Version};
use axcenity_updater::apply::{Action, EDITOR_EXE, Expected, Instruction, Outcome, UPDATER_EXE};
use axcenity_updater::download::{self, DownloadError, Fetch};
use axcenity_updater::stage;

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            copy_dir(&path, &to.join(entry.file_name()));
        } else {
            fs::copy(&path, to.join(entry.file_name())).unwrap();
        }
    }
}

/// The version and commit the built binary reports.
fn built() -> (Version, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_axcenity-updater"))
        .args(["--version", "--json"])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    (
        Version::parse(json["version"].as_str().unwrap()).unwrap(),
        json["commit"].as_str().unwrap().to_owned(),
    )
}

/// This platform's install kind, its marker and its update package's name.
fn platform(version: &Version) -> (InstallKind, &'static str, String) {
    if cfg!(windows) {
        (
            InstallKind::WindowsSetup,
            r#"{"schema":1,"kind":"windows-setup"}"#,
            format!("AxcenityEditor-{version}-windows-x86_64-update.zip"),
        )
    } else {
        (
            InstallKind::LinuxTarball,
            r#"{"schema":1,"kind":"linux-tarball"}"#,
            format!("AxcenityEditor-{version}-linux-x86_64.tar.gz"),
        )
    }
}

/// The update package, laid out as the release makes it: a zip with the
/// files at its root on Windows, a tar.gz with one top folder elsewhere.
fn package(path: &Path, files: &[(&str, &[u8])]) {
    let out = fs::File::create(path).unwrap();
    if cfg!(windows) {
        let mut zip = zip::ZipWriter::new(out);
        for (name, bytes) in files {
            let options = zip::write::SimpleFileOptions::default().unix_permissions(0o755);
            zip.start_file(*name, options).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    } else {
        let gz = flate2::write::GzEncoder::new(out, flate2::Compression::fast());
        let mut tar = tar::Builder::new(gz);
        for (name, bytes) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            tar.append_data(&mut header, format!("AxcenityEditor/{name}"), *bytes)
                .unwrap();
        }
        tar.into_inner().unwrap().finish().unwrap();
    }
}

/// A published release, as GitHub would serve it.
struct Release {
    tag: String,
    prerelease: bool,
    files: BTreeMap<String, Vec<u8>>,
}

impl Release {
    fn list(&self) -> Vec<u8> {
        let assets: Vec<_> = self
            .files
            .iter()
            .map(|(name, bytes)| serde_json::json!({ "name": name, "size": bytes.len() }))
            .collect();
        serde_json::to_vec(&serde_json::json!([{
            "tag_name": self.tag,
            "draft": false,
            "prerelease": self.prerelease,
            "html_url": format!("https://github.com/{}/releases/tag/{}", updates::REPOSITORY, self.tag),
            "assets": assets,
        }]))
        .unwrap()
    }

    /// GET `url`: the release list, or one of the release's files.
    fn get(&self, url: &str) -> Result<Vec<u8>, String> {
        if url == releases_url() {
            return Ok(self.list());
        }
        self.files
            .iter()
            .find(|(name, _)| url == asset_url(&self.tag, name))
            .map(|(_, bytes)| bytes.clone())
            .ok_or_else(|| format!("404 {url}"))
    }
}

impl Fetch for Release {
    fn open(&self, url: &str, from: u64) -> Result<(Box<dyn Read>, bool), String> {
        let bytes = self.get(url)?;
        let from = from as usize;
        let resumed = from > 0 && from <= bytes.len();
        let start = if resumed { from } else { 0 };
        Ok((
            Box::new(std::io::Cursor::new(bytes[start..].to_vec())),
            resumed,
        ))
    }
}

/// Makes the release with the real release tools and signs it. Returns it
/// and the public key to trust.
fn publish(
    root: &Path,
    version: &Version,
    commit: &str,
    payload: &[(&str, &[u8])],
) -> (Release, String) {
    // `prepare` on a copy of the repository's release files, as a person
    // cutting the release would run it.
    let repo = root.join("repo");
    copy_dir(&repository().join("releases"), &repo.join("releases"));
    fs::copy(repository().join("Cargo.toml"), repo.join("Cargo.toml")).unwrap();
    // Once this version is released its record is already there.
    let recorded = History::load(&repo.join("releases"))
        .unwrap()
        .release(version)
        .is_some();
    if !recorded {
        let record = axcenity_release::prepare::prepare(&repo, version, "2026-11-11").unwrap();
        let text = fs::read_to_string(&record).unwrap();
        let text = text
            .replacen("title = \"\"", "title = \"End-to-end test\"", 1)
            .replacen(
                "summary = \"\"",
                "summary = \"A release made by a test.\"",
                1,
            );
        fs::write(&record, text).unwrap();
    }
    let history = History::load(&repo.join("releases")).unwrap();
    assert!(history.validate().is_empty(), "{:?}", history.validate());

    // The files the workflow uploads.
    let (_, _, package_name) = platform(version);
    let dist = root.join("dist");
    fs::create_dir_all(&dist).unwrap();
    package(&dist.join(&package_name), payload);
    let history_file =
        serde_json::to_vec_pretty(&HistoryFile::from_history(&history.public())).unwrap();
    fs::write(dist.join(HISTORY_FILE), &history_file).unwrap();
    let manifest = Manifest::build(
        &history,
        version,
        &dist,
        BuildFacts {
            commit: commit.to_owned(),
            published: "2026-11-11T00:00:00Z".to_owned(),
            ..Default::default()
        },
    )
    .unwrap();
    let json = serde_json::to_vec_pretty(&manifest).unwrap();

    // Signed as the workflow signs it: the trusted comment names the tag.
    let keys = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
    let signature = minisign::sign(
        Some(&keys.pk),
        &keys.sk,
        json.as_slice(),
        Some(&format!("axcenity-engine {}", version.tag())),
        None,
    )
    .unwrap()
    .to_string();
    let public_key = keys.pk.to_box().unwrap().to_string();

    let mut files = BTreeMap::new();
    files.insert(
        package_name.clone(),
        fs::read(dist.join(&package_name)).unwrap(),
    );
    files.insert(HISTORY_FILE.to_owned(), history_file);
    files.insert(MANIFEST_FILE.to_owned(), json);
    files.insert(SIGNATURE_FILE.to_owned(), signature.into_bytes());
    (
        Release {
            tag: version.tag(),
            prerelease: version.is_prerelease(),
            files,
        },
        public_key,
    )
}

fn scratch(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("axcenity-e2e-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    root
}

/// A process that has already exited, standing in for the closed editor.
fn exited_pid() -> u32 {
    let mut done = Command::new(env!("CARGO_BIN_EXE_axcenity-updater"))
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let pid = done.id();
    done.wait().unwrap();
    pid
}

#[test]
fn a_release_made_by_the_release_tools_is_found_verified_downloaded_and_installed() {
    let root = scratch("release");
    let (version, commit) = built();
    let (kind, marker, _) = platform(&version);
    let program = fs::read(env!("CARGO_BIN_EXE_axcenity-updater")).unwrap();
    let (release, public_key) = publish(
        &root,
        &version,
        &commit,
        &[
            ("install.json", marker.as_bytes()),
            (EDITOR_EXE, &program),
            (UPDATER_EXE, &program),
        ],
    );

    // An install of an older version.
    let install = root.join("apps").join("Axcenity");
    fs::create_dir_all(&install).unwrap();
    fs::write(install.join("install.json"), marker).unwrap();
    fs::write(install.join(EDITOR_EXE), "the old editor").unwrap();
    let installed = Version::parse("0.1.0-alpha.3").unwrap();

    // The update check, as the editor runs it.
    let update = updates::check(
        &mut |url| release.get(url),
        &installed,
        version.channel(),
        &public_key,
    )
    .unwrap()
    .expect("the new release is offered");
    assert_eq!(update.candidate.version, version);
    assert_eq!(update.manifest.commit, commit);
    assert!(
        !update
            .manifest
            .notes
            .sections()
            .iter()
            .all(|(_, notes)| notes.is_empty())
    );

    // Download, stage, hand over, install: the editor's steps, then the
    // updater's.
    let artifact = stage::payload(&update.manifest, kind).expect("this platform's update");
    let updates_dir = root.join("Updates");
    let version_dir = updates_dir.join(version.to_string());
    let mut seen = 0;
    let archive = download::download(
        &release,
        &asset_url(&update.candidate.tag, &artifact.file),
        artifact,
        &version_dir.join(&artifact.file),
        &mut |done, _| seen = done,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(seen, artifact.size);
    let staged = version_dir.join("staged");
    stage::stage(&archive, &artifact.file, kind, &staged).unwrap();
    let instruction = Instruction {
        schema: 1,
        protocol: axcenity_release::publish::UPDATE_PROTOCOL,
        action: Action::Install,
        install: install.clone(),
        staged: Some(staged),
        expect: Some(Expected {
            version: update.manifest.version.to_string(),
            commit: update.manifest.commit.clone(),
        }),
        editor_pid: exited_pid(),
        relaunch: vec!["--version".to_owned()],
        log: updates_dir.join("update.log"),
        result: updates_dir.join("result.json"),
    };
    let (updater, path) = stage::hand_over(&version_dir, &instruction).unwrap();
    let status = Command::new(&updater)
        .arg("apply")
        .arg(&path)
        .status()
        .unwrap();
    let outcome: Outcome =
        serde_json::from_str(&fs::read_to_string(&instruction.result).unwrap()).unwrap();
    assert!(status.success() && outcome.ok, "{outcome:?}");
    assert_eq!(outcome.version, version.to_string());
    let out = Command::new(install.join(EDITOR_EXE))
        .args(["--version", "--json"])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["version"], version.to_string());
    assert_eq!(
        fs::read(root.join("apps").join("Axcenity.previous").join(EDITOR_EXE)).unwrap(),
        b"the old editor"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_release_signed_by_another_key_or_with_a_changed_package_is_refused() {
    let root = scratch("refused");
    let (version, commit) = built();
    let (kind, marker, _) = platform(&version);
    let (mut release, public_key) = publish(
        &root,
        &version,
        &commit,
        &[
            ("install.json", marker.as_bytes()),
            (EDITOR_EXE, b"x"),
            (UPDATER_EXE, b"x"),
        ],
    );
    let installed = Version::parse("0.1.0-alpha.3").unwrap();

    // Another key: nothing is offered.
    let other = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
    let other_key = other.pk.to_box().unwrap().to_string();
    let result = updates::check(
        &mut |url| release.get(url),
        &installed,
        version.channel(),
        &other_key,
    );
    assert!(
        matches!(result, Err(UpdateError::BadSignature(_))),
        "{result:?}"
    );

    // The right key, but the package on the server isn't the signed one:
    // the download is thrown away.
    let update = updates::check(
        &mut |url| release.get(url),
        &installed,
        version.channel(),
        &public_key,
    )
    .unwrap()
    .unwrap();
    let artifact = stage::payload(&update.manifest, kind).unwrap().clone();
    release
        .files
        .insert(artifact.file.clone(), vec![0; artifact.size as usize]);
    let to = root.join("Updates").join(&artifact.file);
    let result = download::download(
        &release,
        &asset_url(&update.candidate.tag, &artifact.file),
        &artifact,
        &to,
        &mut |_, _| {},
        &AtomicBool::new(false),
    );
    assert!(
        matches!(result, Err(DownloadError::WrongHash)),
        "{result:?}"
    );
    assert!(!to.exists() && !download::part_path(&to).exists());
    let _ = fs::remove_dir_all(&root);
}
