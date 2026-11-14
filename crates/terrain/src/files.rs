//! A terrain's saved files, as bytes (`project` reads and writes them).
//!
//! Each Terrain saves to its own folder (its `DataFolder`):
//!
//! - `terrain.json`: the format version, the grid's size, and the generator
//!   settings that made it, if any.
//! - `heights.png`: a 16-bit greyscale heightmap, black at the bottom of
//!   the height range and white at the top. Any image editor or landscape
//!   tool can read and write it.
//! - `paint-1-4.png` and `paint-5-8.png`: the hand-painted weights of
//!   layers 1 to 4 and 5 to 8, one per colour channel (red, green, blue,
//!   alpha). Missing files mean nothing is painted.
//! - `foliage.json`: painted plants, props, and models, with whether each
//!   collides. Missing means none. An extra file, so older engines ignore
//!   it and this engine does not bump the terrain format.

use std::io::Cursor;

use axcenity_core::{
    FoliageInstance, FoliagePlant, FoliageProp, FoliageSource, MAX_FOLIAGE_INSTANCES,
    MAX_TERRAIN_LAYERS, PaintWeights, TerrainData, valid_terrain_size,
};
use serde::{Deserialize, Serialize};

pub const TERRAIN_FORMAT_VERSION: u32 = 1;
pub const MANIFEST_FILE: &str = "terrain.json";
pub const HEIGHTS_FILE: &str = "heights.png";
pub const PAINT_FILES: [&str; 2] = ["paint-1-4.png", "paint-5-8.png"];
pub const FOLIAGE_FILE: &str = "foliage.json";

#[derive(Debug, thiserror::Error)]
pub enum FileError {
    #[error("`{file}` is missing")]
    Missing { file: &'static str },
    #[error("`{file}` isn't a valid terrain file: {message}")]
    Invalid { file: &'static str, message: String },
    #[error(
        "`{MANIFEST_FILE}` is from terrain format {0}, but this engine only understands up to {TERRAIN_FORMAT_VERSION}; update Axcenity to open it"
    )]
    TooNew(u32),
}

fn invalid(file: &'static str) -> impl Fn(String) -> FileError {
    move |message| FileError::Invalid { file, message }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format_version: u32,
    size_x: usize,
    size_z: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    generation: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize)]
struct FoliageFile {
    instances: Vec<FoliageRecord>,
}

#[derive(Serialize, Deserialize)]
struct FoliageRecord {
    kind: String,
    name: String,
    x: f32,
    z: f32,
    yaw: f32,
    scale: f32,
    #[serde(default)]
    collide: bool,
}

/// Every file of `data`, by name, ready to write. The same terrain always
/// gives the same bytes.
pub fn encode(data: &TerrainData) -> Vec<(&'static str, Vec<u8>)> {
    let (size_x, size_z) = data.size();
    let manifest = Manifest {
        format_version: TERRAIN_FORMAT_VERSION,
        size_x,
        size_z,
        generation: data
            .generation
            .as_deref()
            .and_then(|text| serde_json::from_str(text).ok()),
    };
    let mut text = serde_json::to_string_pretty(&manifest).expect("plain numbers");
    text.push('\n');
    let mut files = vec![
        (MANIFEST_FILE, text.into_bytes()),
        (HEIGHTS_FILE, encode_heights(size_x, size_z, data.heights())),
    ];
    for (half, name) in PAINT_FILES.into_iter().enumerate() {
        let pixels: Vec<u8> = data
            .paint()
            .iter()
            .flat_map(|weights| weights[half * 4..half * 4 + 4].to_vec())
            .collect();
        files.push((
            name,
            encode_png(
                size_x,
                size_z,
                &pixels,
                png::ColorType::Rgba,
                png::BitDepth::Eight,
            ),
        ));
    }
    if !data.foliage().is_empty() {
        files.push((FOLIAGE_FILE, encode_foliage(data.foliage())));
    }
    files
}

fn encode_foliage(foliage: &[FoliageInstance]) -> Vec<u8> {
    let file = FoliageFile {
        instances: foliage.iter().map(foliage_record).collect(),
    };
    let mut text = serde_json::to_string_pretty(&file).expect("plain numbers and strings");
    text.push('\n');
    text.into_bytes()
}

fn foliage_record(instance: &FoliageInstance) -> FoliageRecord {
    FoliageRecord {
        kind: instance.source.kind_key().to_owned(),
        name: match &instance.source {
            FoliageSource::Model(path) => path.clone(),
            other => other.name().to_owned(),
        },
        x: instance.x,
        z: instance.z,
        yaw: instance.yaw,
        scale: instance.scale,
        collide: instance.collide,
    }
}

fn decode_foliage(bytes: &[u8]) -> Result<Vec<FoliageInstance>, String> {
    let file: FoliageFile = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    let mut instances = Vec::new();
    for record in file.instances {
        if instances.len() >= MAX_FOLIAGE_INSTANCES {
            break;
        }
        let source = match record.kind.as_str() {
            "plant" => FoliagePlant::from_name(&record.name).map(FoliageSource::Plant),
            "prop" => FoliageProp::from_name(&record.name).map(FoliageSource::Prop),
            "model" if !record.name.is_empty() => Some(FoliageSource::Model(record.name)),
            _ => None,
        };
        let Some(source) = source else {
            continue;
        };
        instances.push(FoliageInstance {
            source,
            x: record.x,
            z: record.z,
            yaw: record.yaw,
            scale: record.scale.max(0.01),
            collide: record.collide,
            lean: 0.0,
            sink: 0.0,
        });
    }
    Ok(instances)
}

/// A terrain from its files, read by `read` (which gives `None` for a
/// missing file).
pub fn decode(mut read: impl FnMut(&str) -> Option<Vec<u8>>) -> Result<TerrainData, FileError> {
    let manifest = read(MANIFEST_FILE).ok_or(FileError::Missing {
        file: MANIFEST_FILE,
    })?;
    let manifest: Manifest = serde_json::from_slice(&manifest)
        .map_err(|error| invalid(MANIFEST_FILE)(error.to_string()))?;
    if manifest.format_version > TERRAIN_FORMAT_VERSION {
        return Err(FileError::TooNew(manifest.format_version));
    }
    let heights = read(HEIGHTS_FILE).ok_or(FileError::Missing { file: HEIGHTS_FILE })?;
    let image = decode_image(&heights).map_err(invalid(HEIGHTS_FILE))?;
    if (image.width, image.height) != (manifest.size_x, manifest.size_z) {
        return Err(invalid(HEIGHTS_FILE)(format!(
            "it's {} × {}, but `{MANIFEST_FILE}` says {} × {}",
            image.width, image.height, manifest.size_x, manifest.size_z
        )));
    }
    let count = manifest.size_x * manifest.size_z;
    let mut paint = vec![[0u8; MAX_TERRAIN_LAYERS]; count];
    for (half, name) in PAINT_FILES.into_iter().enumerate() {
        let Some(bytes) = read(name) else {
            continue;
        };
        let image = decode_image(&bytes).map_err(invalid(name))?;
        if (image.width, image.height) != (manifest.size_x, manifest.size_z) {
            return Err(invalid(name)(
                "its size doesn't match the heightmap".to_owned(),
            ));
        }
        for (sample, channels) in paint.iter_mut().zip(image.rgba8.as_chunks::<4>().0) {
            sample[half * 4..half * 4 + 4].copy_from_slice(channels);
        }
    }
    for sample in &mut paint {
        keep_paint_in_range(sample);
    }
    let mut data = TerrainData::from_parts(manifest.size_x, manifest.size_z, image.values, paint)
        .map_err(|error| invalid(MANIFEST_FILE)(error.to_string()))?;
    data.generation = manifest.generation.map(|value| value.to_string());
    if let Some(bytes) = read(FOLIAGE_FILE)
        && let Ok(foliage) = decode_foliage(&bytes)
    {
        data.load_foliage(foliage);
    }
    Ok(data)
}

/// Paint edited elsewhere may add up to more than 255: scale it back.
fn keep_paint_in_range(sample: &mut PaintWeights) {
    let total: u32 = sample.iter().map(|&w| u32::from(w)).sum();
    if total > 255 {
        for weight in sample.iter_mut() {
            *weight = (u32::from(*weight) * 255 / total) as u8;
        }
    }
}

/// Heights from 0 to 1 as a 16-bit greyscale PNG.
pub fn encode_heights(size_x: usize, size_z: usize, heights: &[f32]) -> Vec<u8> {
    let bytes: Vec<u8> = heights
        .iter()
        .flat_map(|h| ((h.clamp(0.0, 1.0) * 65535.0).round() as u16).to_be_bytes())
        .collect();
    encode_png(
        size_x,
        size_z,
        &bytes,
        png::ColorType::Grayscale,
        png::BitDepth::Sixteen,
    )
}

fn encode_png(
    width: usize,
    height: usize,
    data: &[u8],
    color: png::ColorType,
    depth: png::BitDepth,
) -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width as u32, height as u32);
    encoder.set_color(color);
    encoder.set_depth(depth);
    let mut writer = encoder
        .write_header()
        .expect("writing into memory can't fail");
    writer
        .write_image_data(data)
        .expect("the data matches the size");
    drop(writer);
    out
}

/// An image read for its first channel (as heights) and as 8-bit RGBA.
pub struct DecodedImage {
    pub width: usize,
    pub height: usize,
    /// The first channel, 0 to 1 (16-bit images keep their precision).
    pub values: Vec<f32>,
    pub rgba8: Vec<u8>,
}

/// Reads a PNG of any kind: greyscale or colour, 8 or 16 bits. Heightmaps
/// from other tools come in as their first channel.
pub fn decode_image(bytes: &[u8]) -> Result<DecodedImage, String> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().map_err(|error| error.to_string())?;
    let size = reader
        .output_buffer_size()
        .ok_or("the image is too large")?;
    let mut buffer = vec![0; size];
    let info = reader
        .next_frame(&mut buffer)
        .map_err(|error| error.to_string())?;
    let (width, height) = (info.width as usize, info.height as usize);
    if !valid_terrain_size(width, height) {
        return Err(format!(
            "a {width} × {height} image is too small or too large for a terrain"
        ));
    }
    let channels = match info.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return Err("palette images aren't supported".to_owned()),
    };
    let wide = info.bit_depth == png::BitDepth::Sixteen;
    let bytes_per = if wide { 2 } else { 1 };
    let stride = info.line_size;
    let mut values = Vec::with_capacity(width * height);
    let mut rgba8 = Vec::with_capacity(width * height * 4);
    for row in 0..height {
        let line = &buffer[row * stride..row * stride + width * channels * bytes_per];
        for pixel in line.chunks_exact(channels * bytes_per) {
            let channel = |c: usize| -> (f32, u8) {
                if wide {
                    let v = u16::from_be_bytes([pixel[c * 2], pixel[c * 2 + 1]]);
                    (v as f32 / 65535.0, (v >> 8) as u8)
                } else {
                    (pixel[c] as f32 / 255.0, pixel[c])
                }
            };
            values.push(channel(0).0);
            let rgba = match channels {
                1 => [channel(0).1, channel(0).1, channel(0).1, 255],
                2 => [channel(0).1, channel(0).1, channel(0).1, channel(1).1],
                3 => [channel(0).1, channel(1).1, channel(2).1, 255],
                _ => [channel(0).1, channel(1).1, channel(2).1, channel(3).1],
            };
            rgba8.extend_from_slice(&rgba);
        }
    }
    Ok(DecodedImage {
        width,
        height,
        values,
        rgba8,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn sample_terrain() -> TerrainData {
        let mut data = TerrainData::flat(17, 9, 0.0).unwrap();
        let all = data.bounds();
        let (heights, paint) = data.both_mut_in(all);
        for (i, h) in heights.iter_mut().enumerate() {
            *h = i as f32 / 152.0;
        }
        paint[5] = [10, 20, 30, 40, 50, 60, 0, 45];
        data.generation = Some(r#"{"seed":7}"#.to_owned());
        data.foliage_mut().push(axcenity_core::FoliageInstance {
            source: axcenity_core::FoliageSource::Plant(axcenity_core::FoliagePlant::Grass),
            x: 3.5,
            z: 1.25,
            yaw: 0.4,
            scale: 1.1,
            collide: true,
            lean: 0.0,
            sink: 0.0,
        });
        data
    }

    #[test]
    fn a_terrain_saves_and_loads_the_same() {
        let data = sample_terrain();
        let files: HashMap<&str, Vec<u8>> = encode(&data).into_iter().collect();
        let loaded = decode(|name| files.get(name).cloned()).unwrap();
        assert_eq!(loaded.size(), data.size());
        for (a, b) in loaded.heights().iter().zip(data.heights()) {
            assert!((a - b).abs() <= 0.5 / 65535.0 + 1e-7);
        }
        assert_eq!(loaded.paint(), data.paint());
        assert_eq!(loaded.foliage(), data.foliage());
        assert_eq!(loaded.generation.as_deref(), Some(r#"{"seed":7}"#));
        // Saving the loaded one gives the same files.
        let again: HashMap<&str, Vec<u8>> = encode(&loaded).into_iter().collect();
        assert_eq!(again, files);
    }

    #[test]
    fn missing_paint_is_no_paint_and_missing_heights_are_an_error() {
        let data = sample_terrain();
        let files: HashMap<&str, Vec<u8>> = encode(&data)
            .into_iter()
            .filter(|(name, _)| !name.starts_with("paint"))
            .collect();
        let loaded = decode(|name| files.get(name).cloned()).unwrap();
        assert!(loaded.paint().iter().all(|w| *w == [0; 8]));
        assert_eq!(loaded.foliage().len(), 1);

        let without_foliage: HashMap<&str, Vec<u8>> = encode(&data)
            .into_iter()
            .filter(|(name, _)| *name != FOLIAGE_FILE)
            .collect();
        let loaded = decode(|name| without_foliage.get(name).cloned()).unwrap();
        assert!(loaded.foliage().is_empty());
        let only_manifest = files.get(MANIFEST_FILE).cloned();
        let error = decode(|name| {
            (name == MANIFEST_FILE)
                .then(|| only_manifest.clone())
                .flatten()
        })
        .unwrap_err();
        assert!(error.to_string().contains("heights.png"), "{error}");
    }

    #[test]
    fn foliage_keeps_collide_and_a_model_path() {
        let mut data = TerrainData::flat(9, 9, 0.0).unwrap();
        data.foliage_mut().push(axcenity_core::FoliageInstance {
            source: axcenity_core::FoliageSource::Model("assets/models/Bush.glb".into()),
            x: 1.0,
            z: 2.0,
            yaw: 0.0,
            scale: 1.0,
            collide: true,
            lean: 0.0,
            sink: 0.0,
        });
        let files: HashMap<&str, Vec<u8>> = encode(&data).into_iter().collect();
        let loaded = decode(|name| files.get(name).cloned()).unwrap();
        assert_eq!(loaded.foliage(), data.foliage());
        assert!(loaded.foliage()[0].collide);

        let text = br#"{
  "instances": [
    { "kind": "prop", "name": "Crate", "x": 0.0, "z": 0.0, "yaw": 0.0, "scale": 1.0 }
  ]
}"#;
        let old = decode_foliage(text).unwrap();
        assert_eq!(old.len(), 1);
        assert!(!old[0].collide);
        assert_eq!(
            old[0].source,
            axcenity_core::FoliageSource::Prop(axcenity_core::FoliageProp::Crate)
        );
    }

    #[test]
    fn damaged_or_mismatched_files_are_reported() {
        let data = sample_terrain();
        let mut files: HashMap<&str, Vec<u8>> = encode(&data).into_iter().collect();
        files.insert(HEIGHTS_FILE, b"not a png".to_vec());
        let error = decode(|name| files.get(name).cloned()).unwrap_err();
        assert!(error.to_string().contains("heights.png"), "{error}");

        let other = TerrainData::flat(9, 9, 0.0).unwrap();
        files.insert(HEIGHTS_FILE, encode_heights(9, 9, other.heights()));
        let error = decode(|name| files.get(name).cloned()).unwrap_err();
        assert!(error.to_string().contains("says 17 × 9"), "{error}");

        files.insert(
            MANIFEST_FILE,
            br#"{"format_version": 99, "size_x": 9, "size_z": 9}"#.to_vec(),
        );
        assert!(matches!(
            decode(|name| files.get(name).cloned()),
            Err(FileError::TooNew(99))
        ));
    }

    #[test]
    fn an_8_bit_colour_heightmap_reads_its_first_channel() {
        let pixels: Vec<u8> = (0..81).flat_map(|i| [i as u8 * 3, 0, 0]).collect();
        let png = encode_png(9, 9, &pixels, png::ColorType::Rgb, png::BitDepth::Eight);
        let image = decode_image(&png).unwrap();
        assert_eq!(image.values[2], 6.0 / 255.0);
        assert_eq!(&image.rgba8[8..12], &[6, 0, 0, 255]);
        let tiny = encode_png(
            2,
            2,
            &[0; 4],
            png::ColorType::Grayscale,
            png::BitDepth::Eight,
        );
        assert!(decode_image(&tiny).is_err());
    }
}
