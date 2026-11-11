use std::io;

/// Why a Toolbox request or download failed.
///
/// Every message is a short phrase that's safe to show a user ("Could not
/// get Fabric 086: the download is too large"): it never carries a response
/// body or a header. The underlying error, where there is one, is kept as
/// the source and logged where it happens.
#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    /// Only `https://` addresses are fetched.
    #[error("the address is not https")]
    NotHttps,
    /// The source has no asset with that id (or the id isn't one it uses).
    #[error("the source has no asset with that id")]
    NotFound,
    /// The source asked us to slow down (HTTP 429).
    #[error("the source is limiting requests; wait a minute and try again")]
    RateLimited,
    /// The server answered with an error status.
    #[error("the server answered HTTP {0}")]
    Status(u16),
    /// The request couldn't be made or finished (offline, timed out, TLS).
    #[error("the connection failed")]
    Network(#[source] ureq::Error),
    /// The answer was bigger than the limit asked for.
    #[error("the download is too large")]
    TooLarge,
    /// The answer wasn't what the source sends (not UTF-8, not JSON, no files).
    #[error("{0}")]
    BadResponse(&'static str),
    /// A guard rule refused it (a file name that isn't one, a host not allowed).
    #[error("{0}")]
    Refused(&'static str),
    /// Reading or writing files on this computer failed.
    #[error("{0}")]
    Io(#[from] io::Error),
    /// The metadata database couldn't be opened, read or written.
    #[error("the asset database couldn't be read or written")]
    Database(#[from] rusqlite::Error),
    /// Someone cancelled the download.
    #[error("cancelled")]
    Cancelled,
}

impl From<ureq::Error> for SourceError {
    fn from(err: ureq::Error) -> Self {
        match err {
            ureq::Error::StatusCode(404) => SourceError::NotFound,
            ureq::Error::StatusCode(429) => SourceError::RateLimited,
            ureq::Error::StatusCode(status) => SourceError::Status(status),
            other => SourceError::Network(other),
        }
    }
}
