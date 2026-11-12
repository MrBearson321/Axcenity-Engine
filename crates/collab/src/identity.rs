//! Who someone is in Collaborate (TC3; design §5): a key pair made on their
//! computer. The private key never leaves it; the host knows members by
//! their public key, and a member proves who they are by signing the host's
//! challenge. No account or service is needed. Axcenity accounts can come
//! later as a second kind of identity.

use std::fmt;
use std::path::Path;

use ring::rand::{SecureRandom, SystemRandom};
use ring::signature::{ED25519, Ed25519KeyPair, KeyPair, UnparsedPublicKey};
use serde::{Deserialize, Serialize};

/// A member's public key (Ed25519).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PublicKey(pub [u8; 32]);

impl PublicKey {
    /// True if `signature` is this key's signature of `message`.
    pub fn verifies(&self, message: &[u8], signature: &[u8]) -> bool {
        UnparsedPublicKey::new(&ED25519, self.0)
            .verify(message, signature)
            .is_ok()
    }

    /// The key's fingerprint, which people compare.
    pub fn fingerprint(&self) -> Fingerprint {
        Fingerprint::of(&self.0)
    }
}

impl fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PublicKey({})", self.fingerprint().short())
    }
}

/// SHA-256 of a key or certificate.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Fingerprint(pub [u8; 32]);

impl Fingerprint {
    pub fn of(bytes: &[u8]) -> Fingerprint {
        let digest = ring::digest::digest(&ring::digest::SHA256, bytes);
        let mut out = [0; 32];
        out.copy_from_slice(digest.as_ref());
        Fingerprint(out)
    }

    /// The first four bytes as "A3F9-12BC", as the panel shows a member.
    pub fn short(&self) -> String {
        let hex = to_hex(&self.0[..4]).to_uppercase();
        format!("{}-{}", &hex[..4], &hex[4..])
    }

    pub fn to_hex(&self) -> String {
        to_hex(&self.0)
    }

    pub fn from_hex(text: &str) -> Option<Fingerprint> {
        Some(Fingerprint(from_hex(text)?.try_into().ok()?))
    }
}

impl fmt::Debug for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Fingerprint({})", self.short())
    }
}

/// This computer's Collaborate identity: a display name, a colour for
/// presence, and the key pair.
pub struct Identity {
    pub name: String,
    pub color: [u8; 3],
    key: Ed25519KeyPair,
    /// The key pair as saved (PKCS#8).
    pkcs8: Vec<u8>,
}

/// The identity file's contents.
#[derive(Serialize, Deserialize)]
struct Saved {
    name: String,
    color: [u8; 3],
    /// The PKCS#8 key pair, in hex.
    key: String,
}

impl Identity {
    /// A new identity with a fresh key pair ("Sign in").
    pub fn new(name: impl Into<String>, color: [u8; 3]) -> Identity {
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
            .expect("the system's random numbers")
            .as_ref()
            .to_vec();
        Identity::from_pkcs8(name.into(), color, pkcs8).expect("a key pair just made")
    }

    fn from_pkcs8(name: String, color: [u8; 3], pkcs8: Vec<u8>) -> Option<Identity> {
        let key = Ed25519KeyPair::from_pkcs8(&pkcs8).ok()?;
        Some(Identity {
            name,
            color,
            key,
            pkcs8,
        })
    }

    pub fn public_key(&self) -> PublicKey {
        let mut out = [0; 32];
        out.copy_from_slice(self.key.public_key().as_ref());
        PublicKey(out)
    }

    pub fn sign(&self, message: &[u8]) -> Vec<u8> {
        self.key.sign(message).as_ref().to_vec()
    }

    /// Saves the identity to `path`, readable only by this user where the
    /// OS allows.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let saved = Saved {
            name: self.name.clone(),
            color: self.color,
            key: to_hex(&self.pkcs8),
        };
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let text = serde_json::to_string_pretty(&saved).map_err(std::io::Error::other)?;
        write_private(path, text.as_bytes())
    }

    /// The identity saved at `path`, if there is a readable one.
    pub fn load(path: &Path) -> std::io::Result<Identity> {
        let text = std::fs::read_to_string(path)?;
        let saved: Saved = serde_json::from_str(&text).map_err(std::io::Error::other)?;
        let bad = || std::io::Error::other("the Collaborate identity file is damaged");
        let pkcs8 = from_hex(&saved.key).ok_or_else(bad)?;
        Identity::from_pkcs8(saved.name, saved.color, pkcs8).ok_or_else(bad)
    }
}

impl fmt::Debug for Identity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Identity({}, {:?})", self.name, self.public_key())
    }
}

/// Writes `bytes` to `path` with owner-only permissions on Unix (Windows
/// keeps files in the user's profile private already).
pub(crate) fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(bytes)
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, bytes)
    }
}

/// `N` random bytes.
pub(crate) fn random<const N: usize>() -> [u8; N] {
    let mut out = [0; N];
    SystemRandom::new()
        .fill(&mut out)
        .expect("the system's random numbers");
    out
}

pub(crate) fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub(crate) fn from_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}
