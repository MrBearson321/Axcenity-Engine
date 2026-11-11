//! Records where this build came from, for `BuildInfo` (`src/build_info.rs`).
//!
//! An official build is made by the release workflow, which sets
//! `AXCENITY_OFFICIAL_BUILD=1`, `AXCENITY_BUILD_TAG` and
//! `AXCENITY_BUILD_COMMIT`. Anything else is a local build, whose commit is
//! read from git here. The script reruns only when those variables change
//! or the checked-out commit does (git's `HEAD` and the branch it names), so
//! it doesn't make ordinary edit-and-build loops slower.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    for var in [
        "AXCENITY_OFFICIAL_BUILD",
        "AXCENITY_BUILD_TAG",
        "AXCENITY_BUILD_COMMIT",
        "SOURCE_DATE_EPOCH",
    ] {
        println!("cargo:rerun-if-env-changed={var}");
    }
    println!("cargo:rerun-if-changed=build.rs");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let official = env::var("AXCENITY_OFFICIAL_BUILD").is_ok_and(|v| v == "1");
    let version = env::var("CARGO_PKG_VERSION").unwrap();

    let tag = env::var("AXCENITY_BUILD_TAG").unwrap_or_default();
    if official && tag != format!("v{version}") {
        panic!(
            "an official build must be made from the tag v{version} (Cargo.toml's version), \
             but AXCENITY_BUILD_TAG is `{tag}`"
        );
    }

    let commit = match env::var("AXCENITY_BUILD_COMMIT") {
        Ok(commit) if !commit.is_empty() => commit,
        _ => {
            watch_git_head(&manifest_dir);
            git(&manifest_dir, &["rev-parse", "HEAD"]).unwrap_or_default()
        }
    };
    if official && commit.len() != 40 {
        panic!("an official build needs the full commit hash in AXCENITY_BUILD_COMMIT");
    }

    // The source's date: the variable reproducible builds use, else the
    // commit's time. Never the clock, so rebuilding the same commit gives
    // the same program.
    let source_date = env::var("SOURCE_DATE_EPOCH")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            (!commit.is_empty())
                .then(|| git(&manifest_dir, &["show", "-s", "--format=%ct", &commit]))
                .flatten()
        })
        .unwrap_or_default();

    let rustc = env::var("RUSTC").unwrap_or_else(|_| "rustc".to_owned());
    let rustc_version = Command::new(rustc)
        .arg("--version")
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|s| s.trim().to_owned())
        .unwrap_or_default();

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let profile = profile_name(&out_dir).unwrap_or_else(|| env::var("PROFILE").unwrap());

    let set = |name: &str, value: &str| println!("cargo:rustc-env={name}={value}");
    set(
        "AXCENITY_BUILD_KIND",
        if official { "official" } else { "local" },
    );
    set("AXCENITY_BUILD_COMMIT", &commit);
    set("AXCENITY_BUILD_TAG", if official { &tag } else { "" });
    set("AXCENITY_BUILD_SOURCE_DATE", &source_date);
    set("AXCENITY_BUILD_RUSTC", &rustc_version);
    set("AXCENITY_BUILD_TARGET", &env::var("TARGET").unwrap());
    set("AXCENITY_BUILD_PROFILE", &profile);

    // The release signing key's public half, built in for update checks
    // (`src/updates.rs`); empty until the key exists. The folder is watched,
    // not the file: Cargo reruns a script on every build while a watched
    // file is missing.
    let packaging = manifest_dir.join("../../packaging");
    println!("cargo:rerun-if-changed={}", packaging.display());
    let key = std::fs::read_to_string(packaging.join("axcenity-release.pub")).unwrap_or_default();
    std::fs::write(out_dir.join("release-key.pub"), key).expect("OUT_DIR is writable");
}

/// Runs git in `dir` and returns its trimmed output, or `None` if git isn't
/// there, this isn't a repository, or it failed.
fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8(out.stdout).ok()?.trim().to_owned();
    (!text.is_empty()).then_some(text)
}

/// Asks Cargo to rerun this script when the checked-out commit changes:
/// `HEAD`, the branch file it points to, and `packed-refs`. In a worktree,
/// `HEAD` is in the worktree's git folder and branches in the shared one.
fn watch_git_head(dir: &Path) {
    let Some(git_dir) = git(dir, &["rev-parse", "--absolute-git-dir"]) else {
        return;
    };
    let git_dir = PathBuf::from(git_dir);
    let common = git(dir, &["rev-parse", "--git-common-dir"])
        .map(|common| {
            let common = PathBuf::from(common);
            if common.is_absolute() {
                common
            } else {
                dir.join(common)
            }
        })
        .unwrap_or_else(|| git_dir.clone());
    println!("cargo:rerun-if-changed={}", git_dir.join("HEAD").display());
    if let Some(branch) = git(dir, &["symbolic-ref", "-q", "HEAD"]) {
        let file = common.join(&branch);
        if file.exists() {
            println!("cargo:rerun-if-changed={}", file.display());
        }
    }
    let packed = common.join("packed-refs");
    if packed.exists() {
        println!("cargo:rerun-if-changed={}", packed.display());
    }
}

/// The Cargo profile's folder name (`debug`, `release`, `dist`, …), read from
/// `target/[<triple>/]<profile>/build/<crate>-<hash>/out`, since `PROFILE`
/// only says `debug` or `release` even for custom profiles such as `dist`.
fn profile_name(out_dir: &Path) -> Option<String> {
    let parts: Vec<_> = out_dir.components().collect();
    let build = parts.iter().rposition(|c| c.as_os_str() == "build")?;
    let profile = parts.get(build.checked_sub(1)?)?;
    Some(profile.as_os_str().to_string_lossy().into_owned())
}
