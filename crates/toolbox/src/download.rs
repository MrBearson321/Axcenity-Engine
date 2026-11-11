//! Verified downloads into the asset directory.
//!
//! Each file goes to a `.part` file first, is checked against the size and
//! MD5 the source gave, and only then renamed into place, so a file in the
//! directory is always complete. A variant's files gather in a
//! `<folder>.partial` folder that becomes the real folder when all are in;
//! files already complete there aren't fetched again, so a retry resumes.
//! A variant already downloaded (every file still there at its size) isn't
//! fetched at all. Each download is recorded in the metadata database with
//! the asset's record.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use md5::{Digest, Md5};

use crate::db::{Database, Download, StoredFile, now};
use crate::directory::{AssetDirectory, folder_name};
use crate::http::Http;
use crate::record::{AssetRecord, Package, Variant};
use crate::source::SourceInfo;
use crate::{SourceError, guard};

/// The largest single file fetched (an 8K EXR map or a large model's zip).
pub const FILE_LIMIT: u64 = 2 * 1024 * 1024 * 1024;

/// A file's MD5 as lowercase hex.
pub fn md5_of(path: &Path) -> std::io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Md5::new();
    let mut buffer = vec![0; 1 << 16];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

/// Whether `path` is a file matching the size and MD5 given (each checked
/// only when known).
pub fn is_complete(path: &Path, size: Option<u64>, md5: Option<&str>) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    meta.is_file()
        && size.is_none_or(|size| meta.len() == size)
        && md5.is_none_or(|md5| md5_of(path).is_ok_and(|actual| actual.eq_ignore_ascii_case(md5)))
}

/// Downloads `url` to `dest` through a `.part` file, checking the size and
/// MD5 when given. Returns `false` when a matching file was already there.
pub fn fetch_file(
    http: &dyn Http,
    url: &str,
    dest: &Path,
    size: Option<u64>,
    md5: Option<&str>,
) -> Result<bool, SourceError> {
    if is_complete(dest, size, md5) {
        return Ok(false);
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut name = dest.file_name().unwrap_or_default().to_owned();
    name.push(".part");
    let part = dest.with_file_name(name);
    let fetched = (|| {
        let mut file = fs::File::create(&part)?;
        http.fetch(
            url,
            size.map_or(FILE_LIMIT, |size| size.min(FILE_LIMIT)),
            &mut file,
        )?;
        drop(file);
        if !is_complete(&part, size, md5) {
            return Err(SourceError::BadResponse(
                "the download didn't match the size or checksum the source gave",
            ));
        }
        fs::rename(&part, dest)?;
        Ok(true)
    })();
    if fetched.is_err() {
        let _ = fs::remove_file(&part);
    }
    fetched
}

/// Whether every file of a recorded download is still in the directory at
/// its recorded size.
pub fn is_present(dir: &AssetDirectory, download: &Download) -> bool {
    let folder = dir.path(&download.folder);
    !download.files.is_empty()
        && download
            .files
            .iter()
            .all(|file| is_complete(&join(&folder, &file.path), Some(file.size), None))
}

/// `folder` joined with a relative `/` path.
fn join(folder: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .fold(folder.to_owned(), |path, part| path.join(part))
}

/// Where downloads come from and go: the HTTP client, the asset directory
/// and its database.
pub struct Fetcher<'a> {
    pub http: &'a dyn Http,
    pub dir: &'a AssetDirectory,
    pub db: &'a Database,
}

impl Fetcher<'_> {
    /// Makes sure one variant of `record` is in the asset directory and
    /// recorded in the database, downloading only what's missing, and returns
    /// the download.
    ///
    /// Every file must come from one of `source`'s hosts. An `archive` is
    /// unpacked (only files whose extension is in `extract`, flat, nothing
    /// outside the folder) and the zip deleted; other packages keep each file
    /// at its path. `cancel` is checked between files; a cancelled variant
    /// stays partial and resumes next time.
    pub fn fetch_variant(
        &self,
        source: &SourceInfo,
        record: &AssetRecord,
        variant: &Variant,
        extract: &[&str],
        cancel: &AtomicBool,
    ) -> Result<Download, SourceError> {
        let (http, dir, db) = (self.http, self.dir, self.db);
        let key = record.key();
        if let Some(download) = db.download(&key, &variant.label)?
            && is_present(dir, &download)
        {
            db.put_asset(record)?;
            return Ok(download);
        }
        if variant.files.is_empty() {
            return Err(SourceError::BadResponse(
                "the source lists no files for that download",
            ));
        }
        for file in &variant.files {
            if !source.allows_download_from(&file.url) {
                return Err(SourceError::Refused(
                    "a file isn't on the source's own site",
                ));
            }
        }
        let relative =
            dir.variant_folder(&record.provider, record.kind, &record.id, &variant.label)?;
        let folder = dir.path(&relative);
        let mut partial_name = folder.file_name().unwrap_or_default().to_owned();
        partial_name.push(".partial");
        let partial = folder.with_file_name(partial_name);
        fs::create_dir_all(&partial)?;

        let mut files = Vec::new();
        if variant.package == Package::Archive {
            let [file] = variant.files.as_slice() else {
                return Err(SourceError::BadResponse(
                    "an archive download should be one file",
                ));
            };
            let cache = dir.cache_folder(&record.provider);
            let zip = cache.join(format!("{}-{}.zip", record.id, folder_name(&variant.label)));
            fetch_file(http, &file.url, &zip, file.size, file.md5.as_deref())?;
            if cancel.load(Ordering::Relaxed) {
                return Err(SourceError::Cancelled);
            }
            let names = guard::extract_files(&zip, &partial, extract)?;
            let _ = fs::remove_file(&zip);
            for name in names {
                let size = fs::metadata(partial.join(&name))?.len();
                files.push(StoredFile {
                    path: name,
                    size,
                    md5: None,
                });
            }
        } else {
            for file in &variant.files {
                if cancel.load(Ordering::Relaxed) {
                    return Err(SourceError::Cancelled);
                }
                let path = guard::safe_relative_path(&file.path)
                    .ok_or(SourceError::Refused("a file's name would leave its folder"))?;
                let dest = join(&partial, &path);
                fetch_file(http, &file.url, &dest, file.size, file.md5.as_deref())?;
                let size = fs::metadata(&dest)?.len();
                files.push(StoredFile {
                    path,
                    size,
                    md5: file.md5.clone(),
                });
            }
        }

        if folder.exists() {
            fs::remove_dir_all(&folder)?;
        }
        fs::rename(&partial, &folder)?;
        let download = Download {
            key,
            variant: variant.label.clone(),
            folder: relative,
            bytes: files.iter().map(|file| file.size).sum(),
            files,
            source_urls: variant.files.iter().map(|file| file.url.clone()).collect(),
            fetched: now(),
        };
        db.put_asset(record)?;
        db.put_download(&download)?;
        Ok(download)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;
    use crate::http::Fixture;
    use crate::record::{Kind, RemoteFile};
    use crate::{AssetSource, ambientcg, polyhaven};

    struct Temp(PathBuf);

    impl Temp {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("axcenity-toolbox-dl-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            Self(path)
        }
    }

    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn md5_hex(bytes: &[u8]) -> String {
        Md5::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    fn maps_record() -> (AssetRecord, Variant) {
        let file = |name: &str, body: &[u8]| RemoteFile {
            url: format!("https://dl.polyhaven.org/{name}"),
            path: name.to_owned(),
            size: Some(body.len() as u64),
            md5: Some(md5_hex(body)),
            map: None,
        };
        let variant = Variant {
            label: "2K JPG".into(),
            resolution: Some("2k".into()),
            format: "jpg".into(),
            package: Package::Maps,
            files: vec![
                file("brick_diff_2k.jpg", b"color"),
                file("brick_nor_gl_2k.jpg", b"normal"),
            ],
        };
        let record = AssetRecord {
            provider: "polyhaven".into(),
            id: "brick".into(),
            kind: Kind::Texture,
            license: "CC0".into(),
            variants: vec![variant.clone()],
            ..AssetRecord::default()
        };
        (record, variant)
    }

    #[test]
    fn a_file_is_checked_before_it_is_put_in_place() {
        let temp = Temp::new("file");
        let dest = temp.0.join("a.jpg");
        let http = Fixture::default().with("https://x.org/a.jpg", "hello");
        assert!(
            fetch_file(
                &http,
                "https://x.org/a.jpg",
                &dest,
                Some(5),
                Some(&md5_hex(b"hello"))
            )
            .unwrap()
        );
        assert_eq!(fs::read(&dest).unwrap(), b"hello");
        // Already complete: no request (an empty fixture would 404).
        assert!(
            !fetch_file(
                &Fixture::default(),
                "https://x.org/a.jpg",
                &dest,
                Some(5),
                None
            )
            .unwrap()
        );

        let bad = temp.0.join("b.jpg");
        let err = fetch_file(&http, "https://x.org/a.jpg", &bad, None, Some("0000")).unwrap_err();
        assert!(matches!(err, SourceError::BadResponse(_)));
        assert!(!bad.exists());
        assert!(!temp.0.join("b.jpg.part").exists());
    }

    #[test]
    fn loose_maps_land_in_the_directory_and_are_recorded_once() {
        let temp = Temp::new("maps");
        let dir = AssetDirectory::open(&temp.0).unwrap();
        let db = Database::in_memory().unwrap();
        let (record, variant) = maps_record();
        let http = Fixture::default()
            .with("https://dl.polyhaven.org/brick_diff_2k.jpg", "color")
            .with("https://dl.polyhaven.org/brick_nor_gl_2k.jpg", "normal");
        let cancel = AtomicBool::new(false);
        let info = polyhaven::POLYHAVEN.info();
        let download = Fetcher {
            http: &http,
            dir: &dir,
            db: &db,
        }
        .fetch_variant(info, &record, &variant, &[], &cancel)
        .unwrap();
        assert_eq!(download.folder, "textures/polyhaven/brick/2K-JPG");
        assert_eq!(download.bytes, 11);
        let folder = dir.path(&download.folder);
        assert_eq!(
            fs::read(folder.join("brick_diff_2k.jpg")).unwrap(),
            b"color"
        );
        assert!(!folder.with_file_name("2K-JPG.partial").exists());
        assert_eq!(
            db.download("polyhaven:brick", "2K JPG").unwrap(),
            Some(download.clone())
        );
        assert_eq!(db.asset("polyhaven:brick").unwrap().unwrap().license, "CC0");

        // A second project gets it without a request.
        let again = Fetcher {
            http: &Fixture::default(),
            dir: &dir,
            db: &db,
        }
        .fetch_variant(info, &record, &variant, &[], &cancel)
        .unwrap();
        assert_eq!(again, download);
    }

    #[test]
    fn a_file_from_another_host_or_a_bad_checksum_is_refused() {
        let temp = Temp::new("refused");
        let dir = AssetDirectory::open(&temp.0).unwrap();
        let db = Database::in_memory().unwrap();
        let (record, mut variant) = maps_record();
        let cancel = AtomicBool::new(false);
        let info = polyhaven::POLYHAVEN.info();
        let http = Fixture::default()
            // The right size, the wrong bytes: only the MD5 can tell.
            .with("https://dl.polyhaven.org/brick_diff_2k.jpg", "COLOR")
            .with("https://dl.polyhaven.org/brick_nor_gl_2k.jpg", "normal");
        let err = Fetcher {
            http: &http,
            dir: &dir,
            db: &db,
        }
        .fetch_variant(info, &record, &variant, &[], &cancel)
        .unwrap_err();
        assert!(matches!(err, SourceError::BadResponse(_)));
        assert_eq!(db.download("polyhaven:brick", "2K JPG").unwrap(), None);

        variant.files[0].url = "https://evil.example/brick_diff_2k.jpg".into();
        let err = Fetcher {
            http: &http,
            dir: &dir,
            db: &db,
        }
        .fetch_variant(info, &record, &variant, &[], &cancel)
        .unwrap_err();
        assert!(matches!(err, SourceError::Refused(_)));
    }

    #[test]
    fn an_archive_is_unpacked_and_its_zip_removed() {
        let temp = Temp::new("archive");
        let dir = AssetDirectory::open(&temp.0).unwrap();
        let db = Database::in_memory().unwrap();
        let mut zip = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut zip);
            let options = zip::write::SimpleFileOptions::default();
            writer.start_file("Rock_Color.jpg", options).unwrap();
            writer.write_all(b"rock").unwrap();
            writer.start_file("Rock.usdc", options).unwrap();
            writer.write_all(b"scene").unwrap();
            writer.finish().unwrap();
        }
        let url = "https://ambientcg.com/get?file=Rock_1K-JPG.zip";
        let variant = Variant {
            label: "1K JPG".into(),
            resolution: Some("1k".into()),
            format: "jpg".into(),
            package: Package::Archive,
            files: vec![RemoteFile {
                url: url.into(),
                path: "Rock_1K-JPG.zip".into(),
                ..RemoteFile::default()
            }],
        };
        let record = AssetRecord {
            provider: "ambientcg".into(),
            id: "Rock".into(),
            kind: Kind::Texture,
            ..AssetRecord::default()
        };
        let http = Fixture::default().with(url, zip.into_inner());
        let cancel = AtomicBool::new(false);
        let info = ambientcg::AMBIENTCG.info();
        let download = Fetcher {
            http: &http,
            dir: &dir,
            db: &db,
        }
        .fetch_variant(info, &record, &variant, &["jpg"], &cancel)
        .unwrap();
        let names: Vec<_> = download.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(names, ["Rock_Color.jpg"]);
        assert!(
            fs::read_dir(dir.cache_folder("ambientcg"))
                .unwrap()
                .next()
                .is_none()
        );
    }

    #[test]
    fn a_cancelled_download_is_not_recorded() {
        let temp = Temp::new("cancel");
        let dir = AssetDirectory::open(&temp.0).unwrap();
        let db = Database::in_memory().unwrap();
        let (record, variant) = maps_record();
        let cancel = AtomicBool::new(true);
        let info = polyhaven::POLYHAVEN.info();
        let err = Fetcher {
            http: &Fixture::default(),
            dir: &dir,
            db: &db,
        }
        .fetch_variant(info, &record, &variant, &[], &cancel)
        .unwrap_err();
        assert!(matches!(err, SourceError::Cancelled));
        assert_eq!(db.download("polyhaven:brick", "2K JPG").unwrap(), None);
    }
}
