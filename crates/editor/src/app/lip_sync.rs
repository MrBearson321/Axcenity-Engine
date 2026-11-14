//! The Console's `lipsync` command (facial animation F5): a lip sync made
//! beside a sound file from how it sounds, or from a phoneme timings file,
//! as one undo step. Character Studio's lip-sync editor (F6) edits them.

use super::*;

use axcenity_assets::{LIP_SYNC_EXTENSION, LipSync};

/// Reads phoneme timings: one `start end phoneme` per line (seconds,
/// separated by spaces or tabs, as forced aligners write them); blank
/// lines and `#` comments are skipped.
pub(crate) fn read_phoneme_timings(text: &str) -> Result<Vec<(f64, f64, String)>, String> {
    let mut timings = Vec::new();
    for (number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        let [start, end, phoneme] = parts[..] else {
            return Err(format!(
                "line {} should be `start end phoneme`, such as `0.12 0.2 AA1`",
                number + 1
            ));
        };
        let (Ok(start), Ok(end)) = (start.parse::<f64>(), end.parse::<f64>()) else {
            return Err(format!(
                "line {} starts with times that aren't numbers",
                number + 1
            ));
        };
        timings.push((start, end, phoneme.to_owned()));
    }
    Ok(timings)
}

/// Where the lip sync made from `source` goes: beside it, its extension
/// swapped for `.lipsync.json`.
pub(crate) fn lip_sync_path(source: &str) -> String {
    let file_start = source.rfind(['/', '\\']).map_or(0, |at| at + 1);
    let stem_end = source[file_start..]
        .rfind('.')
        .map_or(source.len(), |at| file_start + at);
    format!("{}{LIP_SYNC_EXTENSION}", &source[..stem_end])
}

/// Makes the lip sync for `source` (a project path): from phoneme timings
/// for a `.txt`, `.tsv` or `.lab` file, else from the sound itself.
/// Returns it and the phonemes it didn't know.
pub(crate) fn make_lip_sync(
    folder: &std::path::Path,
    source: &str,
) -> Result<(LipSync, Vec<String>), String> {
    let relative = axcenity_assets::safe_path(source)
        .ok_or_else(|| format!("`{source}` isn't inside the project folder"))?;
    let bytes = std::fs::read(folder.join(relative)).map_err(|err| match err.kind() {
        std::io::ErrorKind::NotFound => format!("there's no file `{source}`"),
        _ => format!("`{source}` can't be read: {err}"),
    })?;
    let lower = source.to_ascii_lowercase();
    let mut made = if [".txt", ".tsv", ".lab"]
        .iter()
        .any(|end| lower.ends_with(end))
    {
        let text = String::from_utf8(bytes).map_err(|_| format!("`{source}` isn't text"))?;
        let timings = read_phoneme_timings(&text)?;
        axcenity_anim::lip_sync_from_phonemes(&timings, 0.0)
    } else {
        let (samples, rate) =
            axcenity_audio::decode_mono(source, bytes).map_err(|err| err.to_string())?;
        let mut lip = axcenity_anim::lip_sync_from_audio(&samples, rate);
        lip.sound = source.to_owned();
        (lip, Vec::new())
    };
    made.0.visemes.retain(|key| key.time.is_finite());
    Ok(made)
}

impl EditorApp {
    /// `lipsync <file>`: makes and saves the lip sync, one undo step.
    pub(super) fn lip_sync_command(&mut self, source: &str) {
        let source = source.trim().trim_matches(['"', '\'']).replace('\\', "/");
        if let Some((path, lip)) = self.make_lip_sync_file(&source) {
            self.output.info(format!(
                "lipsync: made `{path}` ({} visemes over {:.2} s). Play it with \
                 `face:PlayLipSync(\"{path}\", sound)`.",
                lip.visemes.len(),
                lip.duration
            ));
        }
    }

    /// Makes the lip sync for `source` and saves it beside it, one undo
    /// step (the Console's `lipsync` and Character Studio's Make from).
    /// Returns its path and itself.
    pub(super) fn make_lip_sync_file(&mut self, source: &str) -> Option<(String, LipSync)> {
        let Some(folder) = self.project.as_ref().map(|p| p.folder.clone()) else {
            self.output.warning("lipsync: open a project first.");
            return None;
        };
        let (lip, unknown) = match make_lip_sync(&folder, source) {
            Ok(made) => made,
            Err(why) => {
                self.output.warning(format!("lipsync: {why}."));
                return None;
            }
        };
        let path = lip_sync_path(source);
        if !self.save_face_file(&path, lip.to_json(), |folder| {
            axcenity_assets::write_lip_sync(folder, &path, &lip)
        }) {
            return None;
        }
        if !unknown.is_empty() {
            self.output.warning(format!(
                "lipsync: these phonemes weren't known and are Silence: {}.",
                unknown.join(", ")
            ));
        }
        Some((path, lip))
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    /// A 16-bit mono WAV: 0.2 s quiet, 0.3 s of a 140 Hz hum, 0.2 s quiet.
    pub(in crate::app) fn hum_wav() -> Vec<u8> {
        let rate = 16_000u32;
        let mut samples = vec![0i16; 3200];
        samples.extend((0..4800).map(|i| {
            (8000.0 * (i as f32 * 2.0 * std::f32::consts::PI * 140.0 / rate as f32).sin()) as i16
        }));
        samples.extend(vec![0i16; 3200]);
        let data: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let mut wav = Vec::new();
        wav.extend(b"RIFF");
        wav.extend((36 + data.len() as u32).to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16u32.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(rate.to_le_bytes());
        wav.extend((rate * 2).to_le_bytes());
        wav.extend(2u16.to_le_bytes());
        wav.extend(16u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend((data.len() as u32).to_le_bytes());
        wav.extend(data);
        wav
    }

    #[test]
    fn timings_read_and_paths_sit_beside_their_source() {
        let timings =
            read_phoneme_timings("# from an aligner\n0 0.1 sil\n0.1\t0.25\tHH\n\n").unwrap();
        assert_eq!(timings[1], (0.1, 0.25, "HH".to_owned()));
        assert!(
            read_phoneme_timings("0.1 HH")
                .unwrap_err()
                .contains("line 1")
        );
        assert_eq!(
            lip_sync_path("assets/voice/hello.ogg"),
            "assets/voice/hello.lipsync.json"
        );
        assert_eq!(lip_sync_path("hello"), "hello.lipsync.json");
        assert_eq!(lip_sync_path("a.b/hello"), "a.b/hello.lipsync.json");
    }

    #[test]
    fn a_wav_becomes_a_lip_sync_beside_it() {
        let folder =
            std::env::temp_dir().join(format!("axcenity-lipsync-cmd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(folder.join("voice")).unwrap();
        let wav = hum_wav();
        std::fs::write(folder.join("voice/hum.wav"), wav).unwrap();
        let (lip, unknown) = make_lip_sync(&folder, "voice/hum.wav").unwrap();
        assert!(unknown.is_empty());
        assert_eq!(lip.sound, "voice/hum.wav");
        let said: Vec<&str> = lip.visemes.iter().map(|key| key.viseme.as_str()).collect();
        assert_eq!(said.first(), Some(&"Silence"));
        assert_eq!(said.last(), Some(&"Silence"));
        assert!(said.len() >= 3, "{said:?}");
        assert!(
            make_lip_sync(&folder, "voice/none.wav")
                .unwrap_err()
                .contains("no file")
        );
        let _ = std::fs::remove_dir_all(&folder);
    }
}
