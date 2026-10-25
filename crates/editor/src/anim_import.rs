//! The Animation part of the Import Model dialog: what an animated FBX
//! holds (its skeleton and takes), and the clips to make from its takes.
//!
//! A take is one timeline in the file; often a file holds one take with
//! every move on it, one after another. Each clip row is a named part of a
//! take, by frame, so such a take can be split into Idle, Walk, Jump and so
//! on. Importing writes a clip file per row and, if asked, a state machine
//! with a state per clip, for an Animator in the new MeshPart.

use std::path::Path;

use axcenity_anim::files::{self, ClipFile};
use axcenity_anim::{ANIMATIONS_FOLDER, CLIP_EXTENSION};
use axcenity_assets::{Rig, Take};
use eframe::egui;

use crate::theme::icons;

/// One bone in the skeleton tree.
#[derive(Clone, Debug, PartialEq)]
pub struct BoneRow {
    pub name: String,
    pub depth: usize,
}

/// One take in the file.
#[derive(Clone, Debug, PartialEq)]
pub struct TakeRow {
    pub name: String,
    pub frames: u32,
    pub frame_rate: f32,
    /// Bones it moves.
    pub tracks: usize,
}

impl TakeRow {
    fn seconds(&self, frame: u32) -> f64 {
        f64::from(frame) / f64::from(self.frame_rate.max(1.0))
    }
}

/// A clip to make: a named part of one take, by frame.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipRow {
    pub name: String,
    pub take: usize,
    pub start: u32,
    pub end: u32,
    pub looping: bool,
}

/// What the dialog shows and changes about an animated file.
#[derive(Clone, Debug, PartialEq)]
pub struct AnimationImport {
    pub bones: Vec<BoneRow>,
    /// How many things the model's corners are weighted to.
    pub joints: usize,
    pub takes: Vec<TakeRow>,
    pub clips: Vec<ClipRow>,
    /// Give each new MeshPart an Animator, with a controller for the clips.
    pub add_animator: bool,
    /// The take the Split buttons split, and into how many.
    split_take: usize,
    split_parts: u32,
}

/// Take names exporters give a file's only take, which say nothing about it.
const GENERIC_TAKES: [&str; 6] = [
    "mixamo.com",
    "take 001",
    "take001",
    "scene",
    "default take",
    "animation",
];

impl AnimationImport {
    /// The import for a file with `rig` (if its model has one) and `takes`,
    /// with a clip for each whole take. `None` if the file isn't animated.
    /// `model_name` names a clip of the only take when the file calls it
    /// something like "Take 001".
    pub fn new(rig: Option<&Rig>, takes: &[Take], model_name: &str) -> Option<Self> {
        let takes: Vec<&Take> = takes
            .iter()
            .filter(|take| !take.tracks.is_empty())
            .collect();
        if rig.is_none() && takes.is_empty() {
            return None;
        }
        let bones = rig.map_or_else(Vec::new, |rig| {
            let mut depths: Vec<usize> = Vec::with_capacity(rig.bones.len());
            rig.bones
                .iter()
                .map(|bone| {
                    let depth = bone.parent.and_then(|p| depths.get(p)).map_or(0, |d| d + 1);
                    depths.push(depth);
                    BoneRow {
                        name: match bone.name.as_str() {
                            "" => "(root)".to_owned(),
                            name => name.to_owned(),
                        },
                        depth,
                    }
                })
                .collect()
        });
        let rows: Vec<TakeRow> = takes
            .iter()
            .map(|take| TakeRow {
                name: take.name.clone(),
                frames: take.frames().max(1),
                frame_rate: take.frame_rate,
                tracks: take.tracks.len(),
            })
            .collect();
        let mut import = Self {
            bones,
            joints: rig.map_or(0, |rig| rig.joints.len()),
            takes: rows,
            clips: Vec::new(),
            add_animator: !takes.is_empty(),
            split_take: 0,
            split_parts: 2,
        };
        for (index, take) in import.takes.clone().iter().enumerate() {
            let wanted = clip_name_for(&take.name, model_name, import.takes.len());
            let name = import.free_name(&wanted);
            import.clips.push(ClipRow {
                name,
                take: index,
                start: 0,
                end: take.frames,
                looping: true,
            });
        }
        Some(import)
    }

    /// `wanted`, or with 2, 3… after it, the first no clip row has.
    fn free_name(&self, wanted: &str) -> String {
        (1u32..)
            .map(|n| match n {
                1 => wanted.to_owned(),
                n => format!("{wanted}{n}"),
            })
            .find(|name| {
                !self
                    .clips
                    .iter()
                    .any(|clip| clip.name.eq_ignore_ascii_case(name))
            })
            .expect("some number is always free")
    }

    /// Replaces the clips of take `take` with `parts` equal ones, named
    /// after it with a number.
    pub fn split(&mut self, take: usize, parts: u32) {
        let Some(row) = self.takes.get(take).cloned() else {
            return;
        };
        let parts = parts.clamp(1, row.frames.max(1));
        let first = self
            .clips
            .iter()
            .position(|clip| clip.take == take)
            .unwrap_or(self.clips.len());
        let base = self
            .clips
            .get(first)
            .filter(|clip| clip.take == take)
            .map_or_else(|| files::file_safe(&row.name), |clip| clip.name.clone());
        self.clips.retain(|clip| clip.take != take);
        let first = first.min(self.clips.len());
        let mut made = Vec::new();
        for part in 0..parts {
            let start = row.frames * part / parts;
            let end = row.frames * (part + 1) / parts;
            made.push(ClipRow {
                name: String::new(),
                take,
                start,
                end,
                looping: true,
            });
        }
        for (index, clip) in made.iter_mut().enumerate() {
            clip.name = format!("{base} {}", index + 1);
        }
        self.clips.splice(first..first, made);
    }

    /// Why the clips can't be imported as they are, if they can't.
    pub fn problem(&self) -> Option<String> {
        for (index, clip) in self.clips.iter().enumerate() {
            let name = files::file_safe(&clip.name);
            if clip.name.trim().is_empty() {
                return Some("Every clip needs a name.".to_owned());
            }
            if clip.end <= clip.start {
                return Some(format!(
                    "The clip `{}` must end after it starts.",
                    clip.name
                ));
            }
            if self.clips[..index]
                .iter()
                .any(|other| files::file_safe(&other.name).eq_ignore_ascii_case(&name))
            {
                return Some(format!("Two clips are called `{name}`."));
            }
        }
        None
    }

    /// The clip files to write, each with its file name (without the
    /// extension), for a model file at `source` in the project.
    pub fn clip_files(&self, source: &str) -> Vec<(String, ClipFile)> {
        (0..self.clips.len())
            .filter_map(|index| {
                let file = self.clip_file(index, source)?;
                Some((files::file_safe(&self.clips[index].name), file))
            })
            .collect()
    }

    /// The file clip row `index` makes, for a model file at `source`.
    /// `None` if there's no such row or its take is gone.
    pub fn clip_file(&self, index: usize, source: &str) -> Option<ClipFile> {
        let clip = self.clips.get(index)?;
        let take = self.takes.get(clip.take)?;
        let mut file = ClipFile::linked(
            source,
            &take.name,
            take.seconds(clip.start),
            take.seconds(clip.end),
        );
        file.looping = clip.looping;
        Some(file)
    }

    /// The dialog's Animation step. `previewing` is the clip row the
    /// preview plays; each row's play button picks it.
    pub fn show(&mut self, ui: &mut egui::Ui, with_animator: bool, previewing: &mut usize) {
        if !self.bones.is_empty() {
            egui::CollapsingHeader::new(format!(
                "Skeleton · {} bones, {} skinned joints",
                self.bones.len(),
                self.joints
            ))
            .id_salt("skeleton")
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("bones")
                    .max_height(160.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for bone in &self.bones {
                            ui.horizontal(|ui| {
                                ui.add_space(bone.depth as f32 * 14.0);
                                ui.label(format!("{} {}", icons::BONE, bone.name));
                            });
                        }
                    });
            });
        }
        if self.takes.is_empty() {
            ui.weak("This file has a skeleton but no takes. Clips from another file for the same skeleton can play on it.");
            return;
        }
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Takes in the file").strong());
        ui.weak("The animation timelines the file was exported with.");
        for take in &self.takes {
            ui.weak(format!(
                "   {} {} · {} frames at {} fps ({:.2} s), moves {} {}",
                icons::FILM_STRIP,
                take.name,
                take.frames,
                take.frame_rate,
                take.seconds(take.frames),
                take.tracks,
                if take.tracks == 1 { "bone" } else { "bones" }
            ));
        }
        ui.add_space(6.0);
        ui.label(egui::RichText::new("Clips to make").strong());
        ui.weak(
            "Each clip is a named part of a take, saved as its own file. If one take holds \
             several moves, Split it and name the parts (Idle, Walk, Jump). Press ▶ to watch \
             a clip in the preview.",
        );
        ui.add_space(2.0);
        self.clips_table(ui, previewing);
        ui.horizontal(|ui| {
            if ui.button(format!("{} Add clip", icons::PLUS)).clicked() {
                let take = self.split_take.min(self.takes.len() - 1);
                let name = self.free_name("Clip");
                self.clips.push(ClipRow {
                    name,
                    take,
                    start: 0,
                    end: self.takes[take].frames,
                    looping: true,
                });
            }
            ui.separator();
            ui.label("Split");
            let selected = self
                .takes
                .get(self.split_take)
                .map_or_else(String::new, |take| take.name.clone());
            egui::ComboBox::from_id_salt("split take")
                .width(120.0)
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    for (index, take) in self.takes.iter().enumerate() {
                        ui.selectable_value(&mut self.split_take, index, &take.name);
                    }
                });
            ui.label("into");
            ui.add(egui::DragValue::new(&mut self.split_parts).range(1..=50));
            if ui
                .button("Split")
                .on_hover_text("Replace that take's clips with this many equal parts")
                .clicked()
            {
                self.split(self.split_take, self.split_parts);
            }
        });
        if let Some(problem) = self.problem() {
            ui.colored_label(ui.visuals().error_fg_color, problem);
        }
        if with_animator {
            ui.add_space(4.0);
            ui.checkbox(
                &mut self.add_animator,
                "Add an Animator so it can play these",
            )
            .on_hover_text(
                "Puts an Animator in the new MeshPart with an Animation per clip, and a \
                     state machine with a state per clip (Idle, if there is one, plays first).",
            );
        }
    }

    fn clips_table(&mut self, ui: &mut egui::Ui, previewing: &mut usize) {
        let mut remove = None;
        egui::ScrollArea::vertical()
            .id_salt("clips")
            .max_height(200.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                egui::Grid::new("clip rows")
                    .num_columns(8)
                    .spacing([8.0, 4.0])
                    .striped(true)
                    .show(ui, |ui| {
                        for heading in ["", "Name", "Take", "Start", "End", "Loop", "Length", ""] {
                            ui.weak(heading);
                        }
                        ui.end_row();
                        for (index, clip) in self.clips.iter_mut().enumerate() {
                            let shown = *previewing == index;
                            if ui
                                .selectable_label(shown, icons::PLAY)
                                .on_hover_text("Play this clip in the preview")
                                .clicked()
                            {
                                *previewing = index;
                            }
                            ui.add(egui::TextEdit::singleline(&mut clip.name).desired_width(110.0));
                            egui::ComboBox::from_id_salt(("clip take", index))
                                .width(100.0)
                                .selected_text(
                                    self.takes
                                        .get(clip.take)
                                        .map_or("", |take| take.name.as_str()),
                                )
                                .show_ui(ui, |ui| {
                                    for (t, take) in self.takes.iter().enumerate() {
                                        if ui
                                            .selectable_value(&mut clip.take, t, &take.name)
                                            .changed()
                                        {
                                            clip.start = 0;
                                            clip.end = take.frames;
                                        }
                                    }
                                });
                            let frames = self.takes.get(clip.take).map_or(1, |take| take.frames);
                            ui.add(
                                egui::DragValue::new(&mut clip.start)
                                    .range(0..=frames)
                                    .suffix(" f"),
                            );
                            ui.add(
                                egui::DragValue::new(&mut clip.end)
                                    .range(0..=frames)
                                    .suffix(" f"),
                            );
                            ui.checkbox(&mut clip.looping, "");
                            let seconds = self.takes.get(clip.take).map_or(0.0, |take| {
                                take.seconds(clip.end.saturating_sub(clip.start))
                            });
                            ui.weak(format!("{seconds:.2} s"));
                            if ui
                                .small_button(icons::X)
                                .on_hover_text("Don't make this clip")
                                .clicked()
                            {
                                remove = Some(index);
                            }
                            ui.end_row();
                        }
                    });
            });
        if let Some(index) = remove {
            self.clips.remove(index);
            if *previewing > index {
                *previewing -= 1;
            }
        }
    }
}

/// The name for a clip of the whole take `take`: its name after any
/// `Armature|` prefix (Blender's), or `model_name` when the file's only take
/// has a name that says nothing ("Take 001", "mixamo.com").
fn clip_name_for(take: &str, model_name: &str, takes: usize) -> String {
    let short = take.rsplit('|').next().unwrap_or(take).trim();
    let generic = GENERIC_TAKES.contains(&short.to_lowercase().as_str());
    if generic && takes == 1 {
        files::file_safe(model_name)
    } else {
        files::file_safe(short)
    }
}

/// Writes `clips` into `assets/animations/<model>/` in `project` (each
/// under a free name) and, if `controller` is true, a state machine with a
/// state per clip beside them, `assets/animations/<model>/<model>.statemachine.json`.
/// Returns the clip paths and the state machine's path (in the project).
pub fn write_animation(
    project: &Path,
    model: &str,
    clips: &[(String, ClipFile)],
    controller: bool,
) -> std::io::Result<(Vec<String>, Option<String>)> {
    let model = files::file_safe(model);
    let folder = format!("{ANIMATIONS_FOLDER}/{model}");
    let mut paths = Vec::new();
    for (name, clip) in clips {
        let name = free_file(project, &folder, name, CLIP_EXTENSION);
        let path = format!("{folder}/{name}{CLIP_EXTENSION}");
        files::write_file(&project.join(&path), &clip.to_json())?;
        paths.push(path);
    }
    if !controller || paths.is_empty() {
        return Ok((paths, None));
    }
    // An Idle clip (by name) is where it starts; otherwise the first.
    let path = crate::animation::store::new_controller(project, &model, &paths)?.path;
    Ok((paths, Some(path)))
}

/// `name`, or `name2`, `name3`… (`name 2` when it ends in a digit) the
/// first with no `name` + `extension` in `folder` (in `project`), ignoring
/// case so it's the same on every OS.
fn free_file(project: &Path, folder: &str, name: &str, extension: &str) -> String {
    let existing: Vec<String> = std::fs::read_dir(project.join(folder))
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().to_lowercase())
                .collect()
        })
        .unwrap_or_default();
    let gap = if name.ends_with(|c: char| c.is_ascii_digit()) {
        " "
    } else {
        ""
    };
    (1u32..)
        .map(|n| match n {
            1 => name.to_owned(),
            n => format!("{name}{gap}{n}"),
        })
        .find(|name| !existing.contains(&format!("{name}{extension}").to_lowercase()))
        .expect("some number is always free")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_anim::StateMachineFile;

    fn take(name: &str, seconds: f32) -> Take {
        Take {
            name: name.into(),
            duration: seconds,
            frame_rate: 30.0,
            tracks: vec![axcenity_assets::Track::default()],
        }
    }

    #[test]
    fn each_take_starts_as_a_whole_clip_named_sensibly() {
        let import = AnimationImport::new(
            None,
            &[take("Armature|Walk", 1.0), take("Run", 2.0)],
            "hero",
        )
        .unwrap();
        let names: Vec<&str> = import.clips.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Walk", "Run"]);
        assert_eq!((import.clips[1].start, import.clips[1].end), (0, 60));
        let only = AnimationImport::new(None, &[take("mixamo.com", 1.0)], "Hero Run").unwrap();
        assert_eq!(only.clips[0].name, "Hero Run");
        assert!(AnimationImport::new(None, &[], "x").is_none());
    }

    #[test]
    fn splitting_a_take_makes_equal_named_parts() {
        let mut import = AnimationImport::new(None, &[take("Moves", 2.0)], "noodle").unwrap();
        import.split(0, 3);
        let rows: Vec<(&str, u32, u32)> = import
            .clips
            .iter()
            .map(|c| (c.name.as_str(), c.start, c.end))
            .collect();
        assert_eq!(
            rows,
            [("Moves 1", 0, 20), ("Moves 2", 20, 40), ("Moves 3", 40, 60)]
        );
        assert_eq!(import.problem(), None);
        import.clips[1].name = "moves 1".into();
        assert!(import.problem().unwrap().contains("Two clips"));
        import.clips[1].name = "Wave".into();
        import.clips[1].end = 10;
        assert!(import.problem().unwrap().contains("end after"));
    }

    #[test]
    fn clips_become_files_in_seconds_and_a_controller_starts_at_idle() {
        let mut import = AnimationImport::new(None, &[take("Moves", 2.0)], "noodle").unwrap();
        import.split(0, 2);
        import.clips[1].name = "Idle".into();
        import.clips[1].looping = false;
        let clips = import.clip_files("assets/models/noodle.fbx");
        assert_eq!(clips[1].0, "Idle");
        let link = clips[1].1.link.as_ref().unwrap();
        assert_eq!((link.start, link.end), (1.0, 2.0));
        assert!(!clips[1].1.looping);

        let project =
            std::env::temp_dir().join(format!("axcenity-anim-import-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&project);
        let (paths, controller) = write_animation(&project, "noodle", &clips, true).unwrap();
        assert_eq!(
            paths,
            [
                "assets/animations/noodle/Moves 1.anim.json",
                "assets/animations/noodle/Idle.anim.json"
            ]
        );
        let controller = controller.unwrap();
        let text = std::fs::read_to_string(project.join(&controller)).unwrap();
        let file = StateMachineFile::from_json(&text).unwrap();
        assert_eq!(file.default_state, "Idle");
        assert_eq!(file.states[0].name, "Idle");
        // A second import doesn't overwrite the first.
        let (again, _) = write_animation(&project, "noodle", &clips, false).unwrap();
        assert_eq!(again[0], "assets/animations/noodle/Moves 1 2.anim.json");
        std::fs::remove_dir_all(&project).unwrap();
    }
}
