//! A glTF file's animations as takes: each animation's translation,
//! rotation and scale channels, one track per node it moves, named by the
//! node, so they bind to a rig by bone name like an FBX file's takes.
//! Morph target weight channels aren't read here.

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use glam::{Quat, Vec3};
use gltf::animation::Interpolation;
use gltf::animation::util::ReadOutputs;

use super::rig::{Keys, Take, Track};
use super::{ModelError, data_uri, gltf_bones, node_name};

/// The frame rate a take shows its times in when its keys don't say.
const DEFAULT_FRAME_RATE: f32 = 30.0;

/// Every animation in a glTF or GLB file's `bytes`. Only the file's own
/// buffers (a GLB's binary chunk, data URIs) are read; an animation whose
/// keys are in another file is left out.
pub(super) fn load_takes(bytes: &[u8]) -> Result<Vec<Take>, ModelError> {
    let gltf::Gltf { document, blob } = gltf::Gltf::from_slice(bytes)?;
    if document.animations().len() == 0 {
        return Ok(Vec::new());
    }
    let buffers: Vec<Option<Vec<u8>>> = document
        .buffers()
        .map(|buffer| match buffer.source() {
            gltf::buffer::Source::Bin => blob.clone(),
            gltf::buffer::Source::Uri(uri) => data_uri(uri),
        })
        .collect();
    let read = |buffer: gltf::Buffer| buffers.get(buffer.index())?.as_deref();
    let mut takes = Vec::new();
    let mut moved = HashSet::new();
    for (index, animation) in document.animations().enumerate() {
        // Tracks by node, in the order the nodes are in the file.
        let mut tracks: BTreeMap<usize, Track> = BTreeMap::new();
        let (mut start, mut end) = (f32::INFINITY, f32::NEG_INFINITY);
        let mut step = f32::INFINITY;
        for channel in animation.channels() {
            let node = channel.target().node();
            let reader = channel.reader(read);
            let (Some(inputs), Some(outputs)) = (reader.read_inputs(), reader.read_outputs())
            else {
                continue;
            };
            let times: Vec<f32> = inputs.collect();
            if times.is_empty() || times.iter().any(|t| !t.is_finite()) {
                continue;
            }
            let interpolation = channel.sampler().interpolation();
            let track = tracks.entry(node.index()).or_insert_with(|| Track {
                bone: node_name(&node),
                ..Default::default()
            });
            match outputs {
                ReadOutputs::Translations(values) => {
                    let values: Vec<Vec3> = values.map(Vec3::from).collect();
                    track.translation = keys(&times, values, interpolation);
                }
                ReadOutputs::Rotations(values) => {
                    let values: Vec<Quat> = values
                        .into_f32()
                        .map(|q| Quat::from_array(q).normalize())
                        .collect();
                    track.rotation = keys(&times, values, interpolation);
                }
                ReadOutputs::Scales(values) => {
                    let values: Vec<Vec3> = values.map(Vec3::from).collect();
                    track.scale = keys(&times, values, interpolation);
                }
                ReadOutputs::MorphTargetWeights(_) => continue,
            }
            moved.insert(node.index());
            start = start.min(times[0]);
            end = end.max(*times.last().unwrap_or(&0.0));
            for pair in times.windows(2) {
                let gap = pair[1] - pair[0];
                if gap > 1e-4 {
                    step = step.min(gap);
                }
            }
        }
        if tracks.is_empty() {
            continue;
        }
        // Keys from 0 seconds, wherever the animation starts.
        let mut tracks: Vec<Track> = tracks.into_values().collect();
        for track in &mut tracks {
            for time in track
                .translation
                .times
                .iter_mut()
                .chain(&mut track.rotation.times)
                .chain(&mut track.scale.times)
            {
                *time -= start;
            }
        }
        // Exporters sample at the scene's frame rate: the closest gap
        // between keys gives it back (Blender's 24, 30 or 60).
        let frame_rate = if step.is_finite() {
            (1.0 / step).round().clamp(1.0, 240.0)
        } else {
            DEFAULT_FRAME_RATE
        };
        takes.push(Take {
            name: animation
                .name()
                .map_or_else(|| format!("Animation {index}"), str::to_owned),
            duration: (end - start).max(0.0),
            frame_rate,
            tracks,
            skeleton: Default::default(),
        });
    }
    // The skeleton the takes share: the nodes they move and the skins'
    // joints (bones they leave still still shape the body), with every
    // node above them.
    let joints = document
        .skins()
        .flat_map(|skin| skin.joints().map(|joint| joint.index()).collect::<Vec<_>>());
    let (skeleton, _) = gltf_bones(&document, moved.iter().copied().chain(joints));
    let skeleton = Arc::new(skeleton);
    for take in &mut takes {
        take.skeleton = skeleton.clone();
    }
    Ok(takes)
}

/// Keys for `values` at `times`, as the engine blends them (linearly):
/// a cubic spline keeps each key's value (its tangents are left out), and
/// a step holds each value until just before the next key.
fn keys<T: Copy>(times: &[f32], values: Vec<T>, interpolation: Interpolation) -> Keys<T> {
    match interpolation {
        Interpolation::Linear => Keys {
            times: times.to_vec(),
            values,
        },
        Interpolation::CubicSpline => Keys {
            times: times.to_vec(),
            // In-tangent, value, out-tangent for each key.
            values: values.chunks_exact(3).map(|key| key[1]).collect(),
        },
        Interpolation::Step => {
            let mut keys = Keys {
                times: Vec::new(),
                values: Vec::new(),
            };
            for (i, (&time, &value)) in times.iter().zip(&values).enumerate() {
                if i > 0 {
                    let before = (time - 1e-4).max(keys.times[keys.times.len() - 1]);
                    keys.times.push(before);
                    keys.values.push(keys.values[keys.values.len() - 1]);
                }
                keys.times.push(time);
                keys.values.push(value);
            }
            keys
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ModelFormat, load_takes};

    /// A GLB file of `json` and the binary chunk `bin`.
    fn glb(json: &str, bin: &[u8]) -> Vec<u8> {
        let mut json = json.as_bytes().to_vec();
        while !json.len().is_multiple_of(4) {
            json.push(b' ');
        }
        let mut bin = bin.to_vec();
        while !bin.len().is_multiple_of(4) {
            bin.push(0);
        }
        let total = 12 + 8 + json.len() + 8 + bin.len();
        let mut out = Vec::new();
        out.extend(b"glTF");
        out.extend(2u32.to_le_bytes());
        out.extend((total as u32).to_le_bytes());
        out.extend((json.len() as u32).to_le_bytes());
        out.extend(b"JSON");
        out.extend(json);
        out.extend((bin.len() as u32).to_le_bytes());
        out.extend(b"BIN\0");
        out.extend(bin);
        out
    }

    fn floats(values: &[f32]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    /// Two nodes, `Root` above `Arm`; one animation, `Wave`, starting at
    /// half a second: Arm turns (linear, 30 keys a second) and Root steps.
    fn waving() -> Vec<u8> {
        let half = std::f32::consts::FRAC_1_SQRT_2;
        let mut bin = Vec::new();
        bin.extend(floats(&[0.5, 0.5 + 1.0 / 30.0, 0.5 + 2.0 / 30.0])); // 0: 12 bytes
        bin.extend(floats(&[
            0.0, 0.0, 0.0, 1.0, half, 0.0, 0.0, half, 1.0, 0.0, 0.0, 0.0,
        ])); // 12: 48
        bin.extend(floats(&[0.5, 0.6])); // 60: 8
        bin.extend(floats(&[0.0, 0.0, 0.0, 0.0, 2.0, 0.0])); // 68: 24
        let json = format!(
            r#"{{
            "asset": {{ "version": "2.0" }},
            "scene": 0,
            "scenes": [{{ "nodes": [0] }}],
            "nodes": [
                {{ "name": "Root", "children": [1] }},
                {{ "name": "Arm", "translation": [0, 1, 0] }}
            ],
            "buffers": [{{ "byteLength": {len} }}],
            "bufferViews": [
                {{ "buffer": 0, "byteOffset": 0, "byteLength": 12 }},
                {{ "buffer": 0, "byteOffset": 12, "byteLength": 48 }},
                {{ "buffer": 0, "byteOffset": 60, "byteLength": 8 }},
                {{ "buffer": 0, "byteOffset": 68, "byteLength": 24 }}
            ],
            "accessors": [
                {{ "bufferView": 0, "componentType": 5126, "count": 3, "type": "SCALAR", "min": [0.5], "max": [0.5667] }},
                {{ "bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC4" }},
                {{ "bufferView": 2, "componentType": 5126, "count": 2, "type": "SCALAR", "min": [0.5], "max": [0.6] }},
                {{ "bufferView": 3, "componentType": 5126, "count": 2, "type": "VEC3" }}
            ],
            "animations": [{{
                "name": "Wave",
                "samplers": [
                    {{ "input": 0, "output": 1, "interpolation": "LINEAR" }},
                    {{ "input": 2, "output": 3, "interpolation": "STEP" }}
                ],
                "channels": [
                    {{ "sampler": 0, "target": {{ "node": 1, "path": "rotation" }} }},
                    {{ "sampler": 1, "target": {{ "node": 0, "path": "translation" }} }}
                ]
            }}]
        }}"#,
            len = bin.len()
        );
        glb(&json, &bin)
    }

    #[test]
    fn a_glb_animation_is_a_take_of_its_node_channels_from_zero_seconds() {
        let takes = load_takes(&waving(), ModelFormat::Gltf).unwrap();
        assert_eq!(takes.len(), 1);
        let take = &takes[0];
        assert_eq!(take.name, "Wave");
        // From the first key of any channel (0.5 s) to the last (0.6 s).
        assert!((take.duration - 0.1).abs() < 1e-4, "{}", take.duration);
        assert_eq!(take.frame_rate, 30.0);
        let arm = take.tracks.iter().find(|t| t.bone == "Arm").unwrap();
        assert_eq!(arm.rotation.times[0], 0.0);
        let turned = arm.rotation.sample(1.0 / 30.0).unwrap();
        assert!(turned.angle_between(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)) < 1e-3);
        let halfway = arm.rotation.sample(0.5 / 30.0).unwrap();
        assert!(halfway.angle_between(Quat::from_rotation_x(std::f32::consts::FRAC_PI_4)) < 1e-3);
        // A step holds its value until the next key.
        let root = take.tracks.iter().find(|t| t.bone == "Root").unwrap();
        assert_eq!(root.translation.sample(0.0995).unwrap(), Vec3::ZERO);
        assert_eq!(
            root.translation.sample(0.11).unwrap(),
            Vec3::new(0.0, 2.0, 0.0)
        );
        // The skeleton has both nodes, parents first, at rest.
        let names: Vec<&str> = take.skeleton.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["Root", "Arm"]);
        assert_eq!(take.skeleton[1].parent, Some(0));
        assert_eq!(take.skeleton[1].rest.translation, Vec3::new(0.0, 1.0, 0.0));
    }

    #[test]
    fn a_glb_with_no_animations_has_no_takes() {
        let json = r#"{ "asset": { "version": "2.0" }, "nodes": [ { "name": "A" } ] }"#;
        assert!(
            load_takes(json.as_bytes(), ModelFormat::Gltf)
                .unwrap()
                .is_empty()
        );
    }
}
