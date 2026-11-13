//! Light probe bakes: the light a `LightProbeVolume`'s grid of probes saw
//! around them, baked in the editor and named by its `Bake` property.
//!
//! A bake is a JSON file in `assets/probes`, such as
//! `Hall.lightprobes.json`:
//!
//! ```json
//! { "counts": [4, 3, 4], "fingerprint": "00c0ffee12345678",
//!   "probes": [[[0.4, 0.1, 0.1], [0.0, 0.0, 0.0], [0.0, 0.0, 0.0], [0.1, 0.0, 0.0]], ...] }
//! ```
//!
//! `counts` is how many probes stand along the box's own X, Y and Z, each
//! in the middle of its cell of the grid. `probes` lists them X first, then
//! Y, then Z: each is the light reaching it as four spherical harmonics
//! coefficients (the constant one, then along Y, Z and X), each red, green
//! and blue, already blurred over a hemisphere and divided by π, so a white
//! matte surface facing `n` returns `c0 · 0.282095 + 0.488603 · (c1 n.y +
//! c2 n.z + c3 n.x)`. `fingerprint` is what the world looked like when it
//! was baked (16 hexadecimal digits), so the editor can tell when a bake is
//! out of date. `invalid`, if there, lists the probes (by their place in
//! `probes`) that stood inside a solid part or under the ground when baked;
//! surfaces leave them out of their blend (roadmap R18.2). Bakes from before
//! it have none. `sky`, if there, keeps apart the share of each probe's
//! light that came from the sky (`probes`, as above) and how bright the sky
//! was when baked (`band0`, the average of its light, red, green and blue),
//! so surfaces can follow a sky that dims or changes color without baking
//! again (roadmap R18.3). Bakes from before it keep all their light fixed.

use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::model::new_key;
use crate::safe_path;

/// The end of every light probe bake's file name. Bakes are saved in
/// [`PROBES_FOLDER`](crate::PROBES_FOLDER) beside reflection probe bakes.
pub const LIGHT_PROBES_SUFFIX: &str = ".lightprobes.json";

/// The most probes along any side of a volume.
pub const MAX_LIGHT_PROBES_PER_SIDE: u32 = 16;

/// Whether `path` names a light probe bake, by its name.
pub fn is_light_probes_file(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(LIGHT_PROBES_SUFFIX)
}

/// The project path a light probe bake named `name` is saved at, such as
/// `assets/probes/Hall.lightprobes.json`.
pub fn light_probes_path(name: &str) -> String {
    format!("{}/{name}{LIGHT_PROBES_SUFFIX}", crate::PROBES_FOLDER)
}

/// One probe's light: four spherical harmonics coefficients, each red,
/// green and blue (see the module's documentation).
pub type ProbeLight = [[f32; 3]; 4];

/// A light probe volume's bake, as the renderer reads it.
#[derive(Clone)]
pub struct LightProbes {
    key: u64,
    /// How many probes stand along the box's own X, Y and Z.
    pub counts: [u32; 3],
    /// What the world looked like when it was baked.
    pub fingerprint: u64,
    /// Each probe's light, X first, then Y, then Z. Shared, so handing it
    /// to the renderer each frame copies nothing.
    pub probes: Arc<Vec<ProbeLight>>,
    /// The probes inside a solid when baked, by their place in `probes`, in
    /// order (roadmap R18.2).
    pub invalid: Arc<Vec<u32>>,
    /// The sky's share of each probe's light, kept apart (roadmap R18.3).
    pub sky: Option<SkyLight>,
}

/// The share of a bake's light that came from the sky, and the sky it came
/// from (see the module's documentation).
#[derive(Clone, Debug, PartialEq)]
pub struct SkyLight {
    /// The average of the sky's light when baked, red, green and blue.
    pub band0: [f32; 3],
    /// Each probe's light from the sky, in the same order as the bake's.
    pub probes: Arc<Vec<ProbeLight>>,
}

impl LightProbes {
    /// A bake of `counts` probes, or `None` if `probes` isn't that many, a
    /// count is 0 or more than [`MAX_LIGHT_PROBES_PER_SIDE`], or a value
    /// isn't a number.
    pub fn new(counts: [u32; 3], fingerprint: u64, probes: Vec<ProbeLight>) -> Option<Self> {
        let total: u32 = counts.iter().product();
        let fits = counts
            .iter()
            .all(|&count| (1..=MAX_LIGHT_PROBES_PER_SIDE).contains(&count));
        let finite = probes.iter().flatten().flatten().all(|c| c.is_finite());
        (fits && finite && probes.len() == total as usize).then(|| Self {
            key: new_key(),
            counts,
            fingerprint,
            probes: Arc::new(probes),
            invalid: Arc::new(Vec::new()),
            sky: None,
        })
    }

    /// The bake with `sky` as its sky's share of the light, or `None` if
    /// that has a different number of probes or a value isn't a number.
    pub fn with_sky(mut self, sky: SkyLight) -> Option<Self> {
        let finite = sky
            .probes
            .iter()
            .flatten()
            .flatten()
            .chain(&sky.band0)
            .all(|c| c.is_finite());
        if !finite || sky.probes.len() != self.probes.len() {
            return None;
        }
        self.sky = Some(sky);
        Some(self)
    }

    /// The bake with the probes at `invalid` (places in `probes`, in any
    /// order) marked invalid, or `None` if one is past the last probe.
    pub fn with_invalid(mut self, mut invalid: Vec<u32>) -> Option<Self> {
        invalid.sort_unstable();
        invalid.dedup();
        if invalid
            .last()
            .is_some_and(|&last| last as usize >= self.probes.len())
        {
            return None;
        }
        self.invalid = Arc::new(invalid);
        Some(self)
    }

    /// Different for every bake read or made.
    pub fn key(&self) -> u64 {
        self.key
    }
}

impl std::fmt::Debug for LightProbes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let [x, y, z] = self.counts;
        write!(f, "LightProbes #{} ({x}×{y}×{z})", self.key)
    }
}

/// A bake file's contents.
#[derive(Serialize, Deserialize)]
struct File {
    counts: [u32; 3],
    fingerprint: String,
    probes: Vec<ProbeLight>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    invalid: Vec<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sky: Option<SkyFile>,
}

/// A bake file's `sky`.
#[derive(Serialize, Deserialize)]
struct SkyFile {
    band0: [f32; 3],
    probes: Vec<ProbeLight>,
}

/// Reads the bake at `path` inside `folder`, or explains why it can't. Not
/// cached: [`Assets::light_probes`](crate::Assets::light_probes) is the
/// cached way.
pub fn read_light_probes(folder: &Path, path: &str) -> Result<LightProbes, String> {
    let relative = safe_path(path).ok_or(
        "a light probe volume's bake must be a file inside the project folder, \
         such as \"assets/probes/Hall.lightprobes.json\"",
    )?;
    let text = std::fs::read_to_string(folder.join(&relative)).map_err(|err| match err.kind() {
        std::io::ErrorKind::NotFound => "the file isn't there".to_owned(),
        _ => err.to_string(),
    })?;
    decode_light_probes(&text)
}

/// Decodes a bake file's `text`.
pub fn decode_light_probes(text: &str) -> Result<LightProbes, String> {
    let file: File =
        serde_json::from_str(text).map_err(|err| format!("it isn't a light probe bake ({err})"))?;
    let fingerprint = u64::from_str_radix(&file.fingerprint, 16)
        .map_err(|_| "its fingerprint isn't 16 hexadecimal digits".to_owned())?;
    let [x, y, z] = file.counts;
    let count = file.probes.len();
    let probes = LightProbes::new(file.counts, fingerprint, file.probes).ok_or_else(|| {
        format!(
            "it has {count} probes for a {x}×{y}×{z} grid (each side 1 to \
             {MAX_LIGHT_PROBES_PER_SIDE}), or a value that isn't a number"
        )
    })?;
    let probes = probes
        .with_invalid(file.invalid)
        .ok_or_else(|| format!("it lists an invalid probe past its {count} probes"))?;
    match file.sky {
        Some(sky) => probes
            .with_sky(SkyLight {
                band0: sky.band0,
                probes: Arc::new(sky.probes),
            })
            .ok_or_else(|| {
                format!("its sky light isn't one for each of its {count} probes, or isn't a number")
            }),
        None => Ok(probes),
    }
}

/// `probes` as a bake file's text.
pub fn encode_light_probes(probes: &LightProbes) -> String {
    let file = File {
        counts: probes.counts,
        fingerprint: format!("{:016x}", probes.fingerprint),
        probes: probes.probes.as_ref().clone(),
        invalid: probes.invalid.as_ref().clone(),
        sky: probes.sky.as_ref().map(|sky| SkyFile {
            band0: sky.band0,
            probes: sky.probes.as_ref().clone(),
        }),
    };
    serde_json::to_string(&file).expect("plain numbers always encode")
}

/// Saves `probes` at `path` inside `folder`, making its folder if needed.
pub fn write_light_probes(folder: &Path, path: &str, probes: &LightProbes) -> Result<(), String> {
    let relative = safe_path(path).ok_or("a bake must be saved inside the project folder")?;
    let file = folder.join(relative);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    std::fs::write(file, encode_light_probes(probes)).map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(i: usize) -> ProbeLight {
        let i = i as f32;
        [
            [i, 0.5, 0.25],
            [0.1, -0.2, 0.0],
            [0.0, 0.0, 0.3],
            [-0.1, 0.0, 0.0],
        ]
    }

    #[test]
    fn a_bake_round_trips_through_its_file() {
        let folder =
            std::env::temp_dir().join(format!("axcenity-light-probes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let path = light_probes_path("Hall");
        assert_eq!(path, "assets/probes/Hall.lightprobes.json");
        assert!(
            is_light_probes_file(&path) && !is_light_probes_file("assets/probes/Hall.probe.hdr")
        );
        let baked = LightProbes::new([2, 1, 3], u64::MAX - 7, (0..6).map(probe).collect())
            .unwrap()
            .with_invalid(vec![4, 1, 4])
            .unwrap();
        write_light_probes(&folder, &path, &baked).unwrap();
        let read = read_light_probes(&folder, &path).unwrap();
        assert_eq!(read.counts, [2, 1, 3]);
        assert_eq!(read.fingerprint, u64::MAX - 7, "every bit of it");
        assert_eq!(read.probes, baked.probes);
        assert_eq!(*read.invalid, [1, 4], "in order, each once");
        assert_ne!(read.key(), baked.key());
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_missing_or_misshapen_bake_says_why() {
        let folder = std::env::temp_dir();
        assert_eq!(
            read_light_probes(&folder, "assets/probes/nowhere.lightprobes.json").unwrap_err(),
            "the file isn't there"
        );
        let short = r#"{"counts":[2,2,2],"fingerprint":"0","probes":[]}"#;
        assert!(
            decode_light_probes(short)
                .unwrap_err()
                .contains("0 probes for a 2×2×2")
        );
        let fingerprint = r#"{"counts":[1,1,1],"fingerprint":"nope","probes":[[[0,0,0],[0,0,0],[0,0,0],[0,0,0]]]}"#;
        assert!(
            decode_light_probes(fingerprint)
                .unwrap_err()
                .contains("fingerprint")
        );
        assert!(
            decode_light_probes("[]")
                .unwrap_err()
                .contains("isn't a light probe bake")
        );
        assert!(
            LightProbes::new([17, 1, 1], 0, vec![probe(0); 17]).is_none(),
            "too many"
        );
        assert!(LightProbes::new([1, 1, 1], 0, vec![[[f32::NAN; 3]; 4]]).is_none());
        let past = r#"{"counts":[1,1,1],"fingerprint":"0","probes":[[[0,0,0],[0,0,0],[0,0,0],[0,0,0]]],"invalid":[1]}"#;
        assert!(
            decode_light_probes(past)
                .unwrap_err()
                .contains("past its 1")
        );
    }

    /// R18.2: a bake from before invalid probes reads with none, and one
    /// without any is written as before.
    #[test]
    fn a_bake_without_invalid_probes_reads_and_writes_as_before() {
        let old =
            r#"{"counts":[1,1,1],"fingerprint":"0","probes":[[[0,0,0],[0,0,0],[0,0,0],[0,0,0]]]}"#;
        let read = decode_light_probes(old).unwrap();
        assert!(read.invalid.is_empty());
        assert!(read.sky.is_none(), "all its light stays fixed");
        let text = encode_light_probes(&read);
        assert!(!text.contains("invalid") && !text.contains("sky"));
    }

    /// R18.3: the sky's share of the light and the sky it came from round
    /// trip, and must be one for each probe.
    #[test]
    fn a_bakes_sky_light_round_trips_and_must_match_its_probes() {
        let sky = SkyLight {
            band0: [0.5, 0.75, 1.25],
            probes: Arc::new((0..2).map(|i| probe(i + 3)).collect()),
        };
        let baked = LightProbes::new([2, 1, 1], 0, (0..2).map(probe).collect())
            .unwrap()
            .with_sky(sky.clone())
            .unwrap();
        let read = decode_light_probes(&encode_light_probes(&baked)).unwrap();
        assert_eq!(read.sky, Some(sky.clone()));
        let one = LightProbes::new([1, 1, 1], 0, vec![probe(0)]).unwrap();
        assert!(one.clone().with_sky(sky).is_none(), "two for one probe");
        let broken = SkyLight {
            band0: [f32::NAN, 0.0, 0.0],
            probes: Arc::new(vec![probe(0)]),
        };
        assert!(one.with_sky(broken).is_none());
        let short = r#"{"counts":[1,1,1],"fingerprint":"0","probes":[[[0,0,0],[0,0,0],[0,0,0],[0,0,0]]],"sky":{"band0":[1,1,1],"probes":[]}}"#;
        assert!(
            decode_light_probes(short)
                .unwrap_err()
                .contains("sky light")
        );
    }
}
