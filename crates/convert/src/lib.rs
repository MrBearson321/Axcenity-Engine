//! Converts model formats the engine doesn't read at run time into `.glb`
//! files, when they're imported (ADR-0027). Editor only: the player and
//! server never link this crate, so they never compile a USD reader.
//!
//! [`convert`] reads a file and returns the `.glb` it becomes, which the
//! editor then imports as if the user had picked that `.glb`: the Import
//! dialog, cooking, asset IDs, levels of detail, collision pieces and
//! built games all work as for any glTF model. Behavior:
//! `docs/editor/import.md`; why: `docs/research/import-formats-and-plugins.md`.
//!
//! Only USD for now ([`usd`]): `.usd`, `.usda`, `.usdc` and `.usdz`.

mod glb;
mod textures;
pub mod usd;

use std::path::Path;

/// The extensions (lowercase) of the files [`convert`] reads.
pub const CONVERTED_EXTENSIONS: [&str; 4] = ["usd", "usda", "usdc", "usdz"];

/// Whether `path` is a file [`convert`] reads, by its extension (any case).
pub fn is_converted(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            CONVERTED_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
        })
}

/// A file converted to glTF.
#[derive(Debug)]
pub struct Converted {
    /// The `.glb` file's bytes, textures embedded.
    pub glb: Vec<u8>,
    /// What didn't come across, each finishing the sentence "Converting
    /// `kitchen.usdz`: …", such as a texture that isn't there.
    pub warnings: Vec<String>,
}

/// Why a file couldn't be converted. The messages finish the sentence
/// "Could not import `…`: ".
#[derive(Debug, thiserror::Error)]
pub enum ConvertError {
    #[error("it isn't a file Axcenity converts (.usd, .usda, .usdc or .usdz)")]
    NotConverted,
    #[error("it isn't a valid USD file: {0}")]
    Usd(String),
    #[error("it has no meshes to draw")]
    NoMeshes,
}

/// Converts the file at `path` into a `.glb`, by its extension.
pub fn convert(path: &Path) -> Result<Converted, ConvertError> {
    if !is_converted(path) {
        return Err(ConvertError::NotConverted);
    }
    // The reader is new to us and reads files from anywhere: a panic in it
    // is a bad file, never a crashed editor.
    std::panic::catch_unwind(|| usd::convert(path))
        .unwrap_or_else(|_| Err(ConvertError::Usd("the file is damaged".into())))
}
