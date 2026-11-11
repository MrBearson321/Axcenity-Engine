//! Unpacking an update archive into a staging folder, refusing anything
//! that could write outside it: absolute paths, `..`, links, devices, and
//! more files or bytes than an editor install could hold. The Windows
//! archive is a zip with the files at its root (`packaging/update-zip.py`);
//! the Linux one is a `.tar.gz` with one top folder
//! (`packaging/linux/build-packages.sh`), which is left out.

use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use thiserror::Error;

/// More than this many files is not an editor install.
pub const MAX_FILES: usize = 20_000;
/// Nor more than this many bytes unpacked (8 GiB).
pub const MAX_BYTES: u64 = 8 << 30;

#[derive(Debug, Error)]
pub enum ExtractError {
    #[error("couldn't read the archive: {0}")]
    Io(#[from] io::Error),
    #[error("the archive isn't a zip: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("the archive has an unsafe entry: {0}")]
    Unsafe(String),
    #[error("the archive is too large to be an update")]
    TooLarge,
    #[error("{0} is not empty; it must be a new folder")]
    NotEmpty(PathBuf),
}

/// The archive formats updates come in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Zip,
    TarGz,
}

impl Format {
    /// By the file name: `.zip` or `.tar.gz`.
    pub fn of(file: &str) -> Option<Format> {
        if file.ends_with(".zip") {
            Some(Format::Zip)
        } else if file.ends_with(".tar.gz") {
            Some(Format::TarGz)
        } else {
            None
        }
    }
}

/// Unpacks `archive` into `into`, which must not exist or be empty.
/// Returns the files written, relative to `into`.
pub fn extract(archive: &Path, format: Format, into: &Path) -> Result<Vec<PathBuf>, ExtractError> {
    if into.exists() && fs::read_dir(into)?.next().is_some() {
        return Err(ExtractError::NotEmpty(into.to_owned()));
    }
    fs::create_dir_all(into)?;
    let mut budget = Budget::default();
    let mut written = Vec::new();
    match format {
        Format::Zip => {
            let mut zip = zip::ZipArchive::new(fs::File::open(archive)?)?;
            for index in 0..zip.len() {
                let mut entry = zip.by_index(index)?;
                let name = entry.name().to_owned();
                if entry.is_dir() {
                    continue;
                }
                if entry.is_symlink() || !entry.is_file() {
                    return Err(ExtractError::Unsafe(format!("{name} is not a plain file")));
                }
                let relative = safe_relative(&name)?;
                let executable = entry.unix_mode().is_some_and(|mode| mode & 0o111 != 0);
                write_file(into, &relative, &mut entry, executable, &mut budget)?;
                written.push(relative);
            }
        }
        Format::TarGz => {
            let gz = flate2::read::GzDecoder::new(fs::File::open(archive)?);
            let mut tar = tar::Archive::new(gz);
            for entry in tar.entries()? {
                let mut entry = entry?;
                let kind = entry.header().entry_type();
                let name = String::from_utf8_lossy(&entry.path_bytes()).into_owned();
                if kind.is_dir() {
                    continue;
                }
                if !kind.is_file() {
                    return Err(ExtractError::Unsafe(format!("{name} is not a plain file")));
                }
                let relative = safe_relative(&name)?;
                let executable = entry.header().mode().is_ok_and(|mode| mode & 0o111 != 0);
                write_file(into, &relative, &mut entry, executable, &mut budget)?;
                written.push(relative);
            }
        }
    }
    if format == Format::TarGz {
        written = lift_single_folder(into, written)?;
    }
    written.sort();
    Ok(written)
}

#[derive(Default)]
struct Budget {
    files: usize,
    bytes: u64,
}

/// `name` as a path under the staging folder, or an error for anything
/// that could leave it.
fn safe_relative(name: &str) -> Result<PathBuf, ExtractError> {
    let unsafe_name = || ExtractError::Unsafe(name.to_owned());
    // Both separators, whatever this OS uses: a zip made on Windows may
    // say `a\b`.
    let normalized = name.replace('\\', "/");
    if normalized.starts_with('/') || normalized.contains(':') {
        return Err(unsafe_name());
    }
    let mut out = PathBuf::new();
    for part in normalized.split('/') {
        match part {
            "" | "." => {}
            ".." => return Err(unsafe_name()),
            part => {
                let path = Path::new(part);
                if !matches!(path.components().next(), Some(Component::Normal(_))) {
                    return Err(unsafe_name());
                }
                out.push(part);
            }
        }
    }
    if out.as_os_str().is_empty() {
        return Err(unsafe_name());
    }
    Ok(out)
}

fn write_file(
    into: &Path,
    relative: &Path,
    from: &mut dyn Read,
    executable: bool,
    budget: &mut Budget,
) -> Result<(), ExtractError> {
    budget.files += 1;
    if budget.files > MAX_FILES {
        return Err(ExtractError::TooLarge);
    }
    let path = into.join(relative);
    if path.exists() {
        return Err(ExtractError::Unsafe(format!(
            "{} is in the archive twice",
            relative.display()
        )));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::File::create(&path)?;
    let room = MAX_BYTES - budget.bytes;
    let copied = io::copy(&mut from.take(room + 1), &mut file)?;
    if copied > room {
        return Err(ExtractError::TooLarge);
    }
    budget.bytes += copied;
    set_executable(&path, executable)?;
    Ok(())
}

#[cfg(unix)]
fn set_executable(path: &Path, executable: bool) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = if executable { 0o755 } else { 0o644 };
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn set_executable(_: &Path, _: bool) -> io::Result<()> {
    Ok(())
}

/// A `.tar.gz` holds one top folder (`AxcenityEditor-<version>-linux-x86_64/`):
/// moves its contents up into `into`, so both formats stage the same way.
fn lift_single_folder(into: &Path, written: Vec<PathBuf>) -> Result<Vec<PathBuf>, ExtractError> {
    let mut tops = written.iter().filter_map(|p| p.components().next());
    let Some(first) = tops.next() else {
        return Ok(written);
    };
    let all_under_one =
        written.iter().all(|p| p.components().count() > 1) && tops.all(|top| top == first);
    if !all_under_one {
        return Ok(written);
    }
    let top = into.join(first.as_os_str());
    for entry in fs::read_dir(&top)? {
        let entry = entry?;
        fs::rename(entry.path(), into.join(entry.file_name()))?;
    }
    fs::remove_dir(&top)?;
    Ok(written
        .into_iter()
        .map(|p| p.components().skip(1).collect())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("axcenity-extract-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn zip_of(path: &Path, entries: &[(&str, &[u8])]) {
        let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
        for (name, body) in entries {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(body).unwrap();
        }
        zip.finish().unwrap();
    }

    fn tar_gz_of(path: &Path, entries: &[(&str, &[u8], u32)]) {
        let gz = flate2::write::GzEncoder::new(
            fs::File::create(path).unwrap(),
            flate2::Compression::fast(),
        );
        let mut tar = tar::Builder::new(gz);
        for (name, body, mode) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(body.len() as u64);
            header.set_mode(*mode);
            header.set_entry_type(tar::EntryType::Regular);
            // `append_data` would refuse the unsafe names these tests need.
            header.as_gnu_mut().unwrap().name[..name.len()].copy_from_slice(name.as_bytes());
            header.set_cksum();
            tar.append(&header, *body).unwrap();
        }
        tar.into_inner().unwrap().finish().unwrap();
    }

    #[test]
    fn a_zip_unpacks_at_its_root() {
        let dir = scratch("zip");
        let archive = dir.join("update.zip");
        zip_of(
            &archive,
            &[("install.json", b"{}"), ("bin/axcenity-editor.exe", b"MZ")],
        );
        let files = extract(&archive, Format::Zip, &dir.join("staged")).unwrap();
        assert_eq!(
            files,
            [
                PathBuf::from("bin/axcenity-editor.exe"),
                PathBuf::from("install.json")
            ]
        );
        assert_eq!(
            fs::read(dir.join("staged/bin/axcenity-editor.exe")).unwrap(),
            b"MZ"
        );
    }

    #[test]
    fn a_tar_gz_loses_its_top_folder_and_keeps_executables() {
        let dir = scratch("tar");
        let archive = dir.join("update.tar.gz");
        tar_gz_of(
            &archive,
            &[
                ("AxcenityEditor-0.2.0/axcenity-editor", b"ELF", 0o755),
                ("AxcenityEditor-0.2.0/install.json", b"{}", 0o644),
            ],
        );
        let staged = dir.join("staged");
        let files = extract(&archive, Format::TarGz, &staged).unwrap();
        assert_eq!(
            files,
            [
                PathBuf::from("axcenity-editor"),
                PathBuf::from("install.json")
            ]
        );
        assert_eq!(fs::read(staged.join("axcenity-editor")).unwrap(), b"ELF");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &str| fs::metadata(staged.join(p)).unwrap().permissions().mode();
            assert_eq!(mode("axcenity-editor") & 0o111, 0o111);
            assert_eq!(mode("install.json") & 0o111, 0);
        }
    }

    #[test]
    fn entries_that_could_leave_the_folder_are_refused() {
        for name in [
            "../evil",
            "a/../../evil",
            "/etc/evil",
            "C:/evil",
            "a\\..\\..\\evil",
        ] {
            assert!(
                matches!(safe_relative(name), Err(ExtractError::Unsafe(_))),
                "{name}"
            );
        }
        assert_eq!(safe_relative("./a//b").unwrap(), PathBuf::from("a/b"));

        let dir = scratch("unsafe");
        let archive = dir.join("bad.tar.gz");
        tar_gz_of(&archive, &[("../escaped", b"x", 0o644)]);
        assert!(matches!(
            extract(&archive, Format::TarGz, &dir.join("staged")),
            Err(ExtractError::Unsafe(_))
        ));
        assert!(!dir.join("escaped").exists());
    }

    #[test]
    fn links_and_a_used_folder_are_refused() {
        let dir = scratch("links");
        let archive = dir.join("link.tar.gz");
        let gz = flate2::write::GzEncoder::new(
            fs::File::create(&archive).unwrap(),
            flate2::Compression::fast(),
        );
        let mut tar = tar::Builder::new(gz);
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        tar.append_link(&mut header, "top/link", "/etc/passwd")
            .unwrap();
        tar.into_inner().unwrap().finish().unwrap();
        assert!(matches!(
            extract(&archive, Format::TarGz, &dir.join("staged")),
            Err(ExtractError::Unsafe(_))
        ));

        let used = dir.join("used");
        fs::create_dir_all(&used).unwrap();
        fs::write(used.join("keep.txt"), b"mine").unwrap();
        let zip = dir.join("ok.zip");
        zip_of(&zip, &[("a", b"1")]);
        assert!(matches!(
            extract(&zip, Format::Zip, &used),
            Err(ExtractError::NotEmpty(_))
        ));
        assert_eq!(fs::read(used.join("keep.txt")).unwrap(), b"mine");
    }

    #[test]
    fn formats_follow_the_file_name() {
        assert_eq!(Format::of("x-windows-x86_64-update.zip"), Some(Format::Zip));
        assert_eq!(Format::of("x-linux-x86_64.tar.gz"), Some(Format::TarGz));
        assert_eq!(Format::of("x.AppImage"), None);
    }
}
