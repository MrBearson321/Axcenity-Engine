//! HTTPS requests for the Toolbox's sources.
//!
//! Everything goes through [`Http`], so tests can answer from memory
//! ([`Fixture`]) and never reach the network. [`Client`] is the real one:
//! `ureq` with rustls, one connection pool per client (clone it to share
//! the pool between threads), and every answer read through a size limit
//! so a bad response can't fill memory or the disk.

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::time::Duration;

use crate::SourceError;

/// Sent with every request. Poly Haven's API terms ask for one that names
/// the software, and none of the sources should be sent a browser's.
pub const USER_AGENT: &str = concat!(
    "AxcenityEngine/",
    env!("CARGO_PKG_VERSION"),
    " (Axcenity Toolbox)"
);

/// Fetches an address into a writer, refusing more than a limit.
pub trait Http: Send + Sync {
    /// Writes the body of `url` to `out` and returns its length in bytes.
    /// `url` must be https. More than `limit` bytes is [`SourceError::TooLarge`].
    fn fetch(&self, url: &str, limit: u64, out: &mut dyn Write) -> Result<u64, SourceError>;
}

/// The body of `url` in memory, at most `limit` bytes.
pub fn get_bytes(http: &dyn Http, url: &str, limit: u64) -> Result<Vec<u8>, SourceError> {
    let mut bytes = Vec::new();
    http.fetch(url, limit, &mut bytes)?;
    Ok(bytes)
}

/// The body of `url` as text, at most `limit` bytes.
pub fn get_text(http: &dyn Http, url: &str, limit: u64) -> Result<String, SourceError> {
    String::from_utf8(get_bytes(http, url, limit)?)
        .map_err(|_| SourceError::BadResponse("the answer was not text"))
}

/// The real client: HTTPS through `ureq`.
#[derive(Clone)]
pub struct Client {
    agent: ureq::Agent,
}

impl Client {
    /// A client whose requests give up after `timeout`, start to finish.
    pub fn new(timeout: Duration) -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            .build();
        Self {
            agent: config.into(),
        }
    }
}

impl Http for Client {
    fn fetch(&self, url: &str, limit: u64, out: &mut dyn Write) -> Result<u64, SourceError> {
        if !url.starts_with("https://") {
            return Err(SourceError::NotHttps);
        }
        let mut response = self
            .agent
            .get(url)
            .header("User-Agent", USER_AGENT)
            .call()
            .inspect_err(|err| log::warn!("Toolbox request failed: GET {url}: {err}"))?;
        let mut reader = response.body_mut().as_reader();
        copy_limited(&mut reader, out, limit)
    }
}

/// Answers from memory, by exact address: for tests.
#[derive(Default)]
pub struct Fixture {
    bodies: HashMap<String, Vec<u8>>,
}

impl Fixture {
    /// Answers `url` with `body` from now on.
    pub fn with(mut self, url: &str, body: impl Into<Vec<u8>>) -> Self {
        self.bodies.insert(url.to_owned(), body.into());
        self
    }
}

impl Http for Fixture {
    fn fetch(&self, url: &str, limit: u64, out: &mut dyn Write) -> Result<u64, SourceError> {
        if !url.starts_with("https://") {
            return Err(SourceError::NotHttps);
        }
        let body = self.bodies.get(url).ok_or(SourceError::Status(404))?;
        copy_limited(&mut body.as_slice(), out, limit)
    }
}

/// Copies `reader` into `writer`, failing once more than `limit` bytes come.
pub fn copy_limited(
    reader: &mut dyn Read,
    writer: &mut dyn Write,
    limit: u64,
) -> Result<u64, SourceError> {
    let copied = io::copy(&mut reader.take(limit + 1), writer)?;
    if copied > limit {
        Err(SourceError::TooLarge)
    } else {
        Ok(copied)
    }
}

/// `text` percent-encoded for a query string: letters, digits and `-_.~`
/// stay, every other byte is `%XX`.
pub fn encode_query(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_answers_known_addresses_and_404s_the_rest() {
        let http = Fixture::default().with("https://example.com/a", "hello");
        assert_eq!(
            get_text(&http, "https://example.com/a", 100).unwrap(),
            "hello"
        );
        assert!(matches!(
            get_bytes(&http, "https://example.com/b", 100),
            Err(SourceError::Status(404))
        ));
    }

    #[test]
    fn only_https_is_fetched() {
        let http = Fixture::default().with("http://example.com/a", "hello");
        assert!(matches!(
            get_bytes(&http, "http://example.com/a", 100),
            Err(SourceError::NotHttps)
        ));
        let client = Client::new(Duration::from_secs(1));
        assert!(matches!(
            get_bytes(&client, "http://example.com/a", 100),
            Err(SourceError::NotHttps)
        ));
    }

    #[test]
    fn a_body_over_the_limit_is_refused() {
        let http = Fixture::default().with("https://example.com/a", "hello");
        assert!(matches!(
            get_bytes(&http, "https://example.com/a", 4),
            Err(SourceError::TooLarge)
        ));
        assert_eq!(
            get_bytes(&http, "https://example.com/a", 5).unwrap(),
            b"hello"
        );
    }

    #[test]
    fn queries_are_percent_encoded() {
        assert_eq!(encode_query("red brick/03"), "red%20brick%2F03");
        assert_eq!(encode_query("a-b_c.d~"), "a-b_c.d~");
    }

    #[test]
    fn the_user_agent_names_the_engine() {
        assert!(USER_AGENT.starts_with("AxcenityEngine/"));
    }
}
