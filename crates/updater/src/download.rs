//! Downloading an update payload: into `<name>.part` beside where it ends
//! up, resumed from where an earlier try stopped (an HTTP range request),
//! cancellable, and checked against the manifest's size and SHA-256 before
//! it's renamed into place. The network is behind [`Fetch`], so tests
//! answer from memory; with the `http` feature, [`HttpFetch`] is the real
//! one.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use axcenity_release::publish::Artifact;
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Opens `url` from byte `from` on. Returns the body and whether it really
/// starts at `from` (a server may ignore the range and send it all).
pub trait Fetch {
    fn open(&self, url: &str, from: u64) -> Result<(Box<dyn Read>, bool), String>;
}

#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("couldn't download the update: {0}")]
    Fetch(String),
    #[error("couldn't save the update: {0}")]
    Io(#[from] io::Error),
    #[error("the download was cancelled")]
    Cancelled,
    #[error("the download is {got} bytes, not the {expected} its manifest says")]
    WrongSize { expected: u64, got: u64 },
    #[error("the download doesn't match its manifest's SHA-256")]
    WrongHash,
}

/// Downloads `artifact` from `url` to `to`. `progress` gets (bytes so far,
/// total); setting `cancel` stops it and deletes the partial file. A file
/// already at `to` with the right hash counts as downloaded.
pub fn download(
    fetch: &dyn Fetch,
    url: &str,
    artifact: &Artifact,
    to: &Path,
    progress: &mut dyn FnMut(u64, u64),
    cancel: &AtomicBool,
) -> Result<PathBuf, DownloadError> {
    if to.is_file() && hash_of(to)? == artifact.sha256 {
        progress(artifact.size, artifact.size);
        return Ok(to.to_owned());
    }
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    let part = part_path(to);
    let mut have = fs::metadata(&part).map_or(0, |m| m.len());
    if have > artifact.size {
        fs::remove_file(&part)?;
        have = 0;
    }
    if have < artifact.size {
        let (mut body, resumed) = fetch.open(url, have).map_err(DownloadError::Fetch)?;
        if !resumed {
            have = 0;
        }
        let mut file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(resumed)
            .truncate(!resumed)
            .open(&part)?;
        let mut buffer = vec![0; 64 * 1024];
        loop {
            if cancel.load(Ordering::Relaxed) {
                drop(file);
                let _ = fs::remove_file(&part);
                return Err(DownloadError::Cancelled);
            }
            let read = body.read(&mut buffer).map_err(|e| DownloadError::Fetch(e.to_string()))?;
            if read == 0 {
                break;
            }
            have += read as u64;
            if have > artifact.size {
                drop(file);
                let _ = fs::remove_file(&part);
                return Err(DownloadError::WrongSize {
                    expected: artifact.size,
                    got: have,
                });
            }
            file.write_all(&buffer[..read])?;
            progress(have, artifact.size);
        }
        file.sync_all()?;
    }
    if have != artifact.size {
        // Cut short: keep the part to resume next time.
        return Err(DownloadError::WrongSize {
            expected: artifact.size,
            got: have,
        });
    }
    if hash_of(&part)? != artifact.sha256 {
        let _ = fs::remove_file(&part);
        return Err(DownloadError::WrongHash);
    }
    fs::rename(&part, to)?;
    Ok(to.to_owned())
}

/// `<to>.part`.
pub fn part_path(to: &Path) -> PathBuf {
    let mut name = to.file_name().unwrap_or_default().to_owned();
    name.push(".part");
    to.with_file_name(name)
}

/// A file's SHA-256 in lowercase hex, as manifests write it.
pub fn hash_of(path: &Path) -> io::Result<String> {
    let mut hasher = Sha256::new();
    let mut file = fs::File::open(path)?;
    let mut buffer = vec![0; 64 * 1024];
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

/// HTTPS with `ureq`: a range request when resuming.
#[cfg(feature = "http")]
pub struct HttpFetch {
    agent: ureq::Agent,
}

#[cfg(feature = "http")]
impl HttpFetch {
    pub fn new(user_agent: &str) -> HttpFetch {
        let agent = ureq::Agent::config_builder()
            .user_agent(user_agent)
            // Per read, not for the whole download: big payloads take minutes.
            .timeout_connect(Some(std::time::Duration::from_secs(20)))
            .timeout_recv_body(Some(std::time::Duration::from_secs(60)))
            .build()
            .into();
        HttpFetch { agent }
    }
}

#[cfg(feature = "http")]
impl Fetch for HttpFetch {
    fn open(&self, url: &str, from: u64) -> Result<(Box<dyn Read>, bool), String> {
        if !url.starts_with("https://") {
            return Err(format!("{url} isn't https"));
        }
        let mut request = self.agent.get(url);
        if from > 0 {
            request = request.header("Range", &format!("bytes={from}-"));
        }
        let response = request.call().map_err(|e| e.to_string())?;
        let resumed = from > 0 && response.status().as_u16() == 206;
        let reader = response.into_body().into_reader();
        Ok((Box::new(reader), resumed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// Serves `body`, honouring ranges if `ranges`, and stopping after
    /// `cut` bytes on the first request if given (a dropped connection).
    struct Memory {
        body: Vec<u8>,
        ranges: bool,
        cut: RefCell<Option<usize>>,
        asked: RefCell<Vec<u64>>,
    }

    impl Fetch for Memory {
        fn open(&self, _: &str, from: u64) -> Result<(Box<dyn Read>, bool), String> {
            self.asked.borrow_mut().push(from);
            let start = if self.ranges { from as usize } else { 0 };
            let mut bytes = self.body[start..].to_vec();
            if let Some(cut) = self.cut.borrow_mut().take() {
                bytes.truncate(cut);
            }
            Ok((Box::new(io::Cursor::new(bytes)), self.ranges && from > 0))
        }
    }

    fn memory(body: &[u8], ranges: bool, cut: Option<usize>) -> Memory {
        Memory {
            body: body.to_vec(),
            ranges,
            cut: RefCell::new(cut),
            asked: RefCell::new(Vec::new()),
        }
    }

    fn artifact_of(body: &[u8]) -> Artifact {
        let hash: String = Sha256::digest(body).iter().map(|b| format!("{b:02x}")).collect();
        Artifact {
            file: "update.zip".to_owned(),
            sha256: hash,
            size: body.len() as u64,
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("axcenity-download-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_dropped_download_resumes_where_it_stopped() {
        let body: Vec<u8> = (0..200_000u32).map(|i| i as u8).collect();
        let artifact = artifact_of(&body);
        let to = scratch("resume").join("update.zip");
        let server = memory(&body, true, Some(70_000));
        let cancel = AtomicBool::new(false);
        let mut seen = 0;
        let first = download(&server, "https://x", &artifact, &to, &mut |n, _| seen = n, &cancel);
        assert!(matches!(first, Err(DownloadError::WrongSize { got: 70_000, .. })));
        assert_eq!(fs::metadata(part_path(&to)).unwrap().len(), 70_000);
        download(&server, "https://x", &artifact, &to, &mut |n, _| seen = n, &cancel).unwrap();
        assert_eq!(*server.asked.borrow(), [0, 70_000]);
        assert_eq!(seen, body.len() as u64);
        assert_eq!(fs::read(&to).unwrap(), body);
        assert!(!part_path(&to).exists());
        // Already there: nothing is fetched again.
        download(&server, "https://x", &artifact, &to, &mut |_, _| {}, &cancel).unwrap();
        assert_eq!(server.asked.borrow().len(), 2);
    }

    #[test]
    fn a_server_without_ranges_starts_over() {
        let body = b"0123456789".repeat(1000);
        let artifact = artifact_of(&body);
        let to = scratch("norange").join("update.zip");
        fs::create_dir_all(to.parent().unwrap()).unwrap();
        fs::write(part_path(&to), &body[..3000]).unwrap();
        let server = memory(&body, false, None);
        download(&server, "https://x", &artifact, &to, &mut |_, _| {}, &AtomicBool::new(false))
            .unwrap();
        assert_eq!(fs::read(&to).unwrap(), body);
    }

    #[test]
    fn a_wrong_hash_or_size_is_never_kept() {
        let body = b"payload".repeat(100);
        let mut artifact = artifact_of(&body);
        artifact.sha256 = "00".repeat(32);
        let to = scratch("hash").join("update.zip");
        let server = memory(&body, true, None);
        let cancel = AtomicBool::new(false);
        assert!(matches!(
            download(&server, "https://x", &artifact, &to, &mut |_, _| {}, &cancel),
            Err(DownloadError::WrongHash)
        ));
        assert!(!to.exists() && !part_path(&to).exists());

        let mut short = artifact_of(&body);
        short.size -= 10;
        assert!(matches!(
            download(&server, "https://x", &short, &to, &mut |_, _| {}, &cancel),
            Err(DownloadError::WrongSize { .. })
        ));
        assert!(!to.exists() && !part_path(&to).exists());
    }

    #[test]
    fn cancel_deletes_the_partial_file() {
        let body = vec![7u8; 300_000];
        let artifact = artifact_of(&body);
        let to = scratch("cancel").join("update.zip");
        let server = memory(&body, true, None);
        let cancel = AtomicBool::new(false);
        let result = download(
            &server,
            "https://x",
            &artifact,
            &to,
            &mut |n, _| {
                if n > 100_000 {
                    cancel.store(true, Ordering::Relaxed);
                }
            },
            &cancel,
        );
        assert!(matches!(result, Err(DownloadError::Cancelled)));
        assert!(!to.exists() && !part_path(&to).exists());
    }
}
