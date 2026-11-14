//! The Retargeting section of a linked clip's settings (AP3): how its take
//! plays on the preview model when that model's skeleton isn't the one
//! the take was made for.
//!
//! - **Status and checks**: whether the take plays by bone names or
//!   through humanoid roles, and what `check_rig` (each skeleton) and
//!   `check_pair` (the two together) found, worst first.
//! - **Bone maps**: either skeleton's role → bone map in a window, each
//!   role picked from the skeleton's bones, saved as a rig definition
//!   (`assets/rigs/<name>.rig.json`) that then applies to every skeleton
//!   with those bones.
//! - **Profile**: the clip's retarget profile (`*.retarget.json`), picked,
//!   made, and its settings changed; each change is written at once, so
//!   the preview (and every clip sharing the profile) follows.

use std::path::Path;

use axcenity_anim::{
    HipsTravel, HumanBone, HumanoidMap, RETARGET_EXTENSION, RetargetProfile, RigProblem, Severity,
    Side, check_pair, check_rig, is_retarget_profile,
};
use axcenity_assets::{Assets, Bone, RIGS_FOLDER, RigDefinition, RigDefinitions};
use eframe::egui;

use super::clip_editor::ClipDoc;
use crate::output::Output;
use crate::theme::{self, icons};

/// Which skeleton a bone map is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Which {
    /// The preview model's.
    Model,
    /// The one the take was made for.
    Take,
}

/// One skeleton's bone map, being changed in its window.
pub struct BoneMap {
    pub which: Which,
    /// The file the skeleton is from, for the window's title.
    pub file: String,
    pub bones: Vec<Bone>,
    pub map: HumanoidMap,
    /// Where Save writes its rig definition: the definition that fits the
    /// skeleton now, else a new one named after the file.
    pub path: String,
    /// Changed since it was opened or saved.
    pub changed: bool,
}

impl BoneMap {
    /// The map `bones` (from the file `file`) has now: the project's
    /// fitting rig definition's, else by names.
    pub fn open(which: Which, file: &str, bones: Vec<Bone>, rigs: &RigDefinitions) -> Self {
        let (map, found) = HumanoidMap::for_skeleton(&bones, rigs);
        let path = found.unwrap_or_else(|| {
            let name = file.rsplit('/').next().unwrap_or(file);
            let stem = name.split('.').next().unwrap_or(name);
            let stem = if stem.is_empty() { "Skeleton" } else { stem };
            format!("{RIGS_FOLDER}/{stem}.rig.json")
        });
        Self {
            which,
            file: file.to_owned(),
            bones,
            map,
            path,
            changed: false,
        }
    }

    /// Gives `role` the bone `bone` (`None`: no bone plays it). A bone
    /// playing another role leaves that one.
    pub fn set(&mut self, role: HumanBone, bone: Option<usize>) {
        if let Some(bone) = bone {
            let others: Vec<HumanBone> = self
                .map
                .roles()
                .filter(|&(other, b)| b == bone && other != role)
                .map(|(other, _)| other)
                .collect();
            for other in others {
                self.map.set(other, None);
            }
        }
        if self.map.get(role) != bone {
            self.map.set(role, bone);
            self.changed = true;
        }
    }

    /// The map as a rig definition: every role's bone, and `""` for a role
    /// names alone would give a bone but the map gives none.
    pub fn definition(&self) -> RigDefinition {
        let mut definition = self.map.to_definition(&self.bones);
        let by_name = HumanoidMap::detect(&self.bones);
        for role in HumanBone::all() {
            if self.map.get(role).is_none() && by_name.get(role).is_some() {
                definition
                    .bones
                    .insert(role.name().to_owned(), String::new());
            }
        }
        definition
    }

    /// What's wrong with the map as it is now.
    pub fn problems(&self) -> Vec<RigProblem> {
        check_rig(
            &self.bones,
            Some(&self.definition()),
            &RigDefinitions::default(),
        )
        .problems
    }

    /// Writes its rig definition and makes `assets` read the project's
    /// definitions again.
    pub fn save(&mut self, project: &Path, assets: &Assets) -> std::io::Result<()> {
        axcenity_assets::write_rig_definition(project, &self.path, &self.definition())?;
        assets.forget(&self.path);
        self.changed = false;
        Ok(())
    }
}

/// What the section shows about the clip on its preview model, worked out
/// again only when something it depends on changes.
struct Summary {
    made_from: String,
    status: String,
    problems: Vec<(&'static str, RigProblem)>,
}

/// The section's own state, kept while the clip is open.
#[derive(Default)]
pub struct RetargetPanel {
    pub bone_map: Option<BoneMap>,
    /// The clip's profile as last read or written, by its path.
    profile: Option<(String, Result<RetargetProfile, String>)>,
    summary: Option<Summary>,
}

/// Every retarget profile in the project (under `assets/rigs/`), in order.
pub fn profile_paths(project: &Path) -> Vec<String> {
    fn walk(folder: &Path, relative: &str, found: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(folder) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = format!("{relative}/{name}");
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                walk(&entry.path(), &path, found);
            } else if is_retarget_profile(&name) {
                found.push(path);
            }
        }
    }
    let mut found = Vec::new();
    walk(&project.join(RIGS_FOLDER), RIGS_FOLDER, &mut found);
    found.sort();
    found
}

/// A path for a new profile named after the clip at `clip_path`, not
/// taken yet.
pub fn new_profile_path(project: &Path, clip_path: &str) -> String {
    let name = axcenity_anim::files::clip_name(clip_path);
    let name = if name.is_empty() { "Retarget" } else { name };
    let free = |path: &str| !project.join(path).exists();
    let first = format!("{RIGS_FOLDER}/{name}{RETARGET_EXTENSION}");
    if free(&first) {
        return first;
    }
    (2..)
        .map(|n| format!("{RIGS_FOLDER}/{name} {n}{RETARGET_EXTENSION}"))
        .find(|path| free(path))
        .expect("some name is free")
}

/// Writes `profile` to `path` in the project.
pub fn write_profile(project: &Path, path: &str, profile: &RetargetProfile) -> std::io::Result<()> {
    axcenity_anim::files::write_file(&project.join(path), &profile.to_json())
}

impl RetargetPanel {
    /// Picks `path` (or none) as the clip's profile, making the file with
    /// the default settings first if it isn't there.
    pub fn use_profile(
        &mut self,
        doc: &mut ClipDoc,
        project: &Path,
        path: Option<String>,
    ) -> std::io::Result<()> {
        if let Some(path) = &path
            && !project.join(path).exists()
        {
            write_profile(project, path, &RetargetProfile::default())?;
        }
        doc.file.retarget = path;
        self.profile = None;
        self.summary = None;
        doc.rebind();
        Ok(())
    }

    /// Changes the clip's profile to `profile`, writing it at once.
    pub fn change_profile(
        &mut self,
        doc: &mut ClipDoc,
        project: &Path,
        profile: RetargetProfile,
    ) -> std::io::Result<()> {
        let Some(path) = doc.file.retarget.clone() else {
            return Ok(());
        };
        write_profile(project, &path, &profile)?;
        self.profile = Some((path, Ok(profile)));
        self.summary = None;
        doc.rebind();
        Ok(())
    }

    /// The clip's profile, read when the clip names another one.
    fn profile(
        &mut self,
        doc: &ClipDoc,
        assets: &Assets,
    ) -> Option<Result<RetargetProfile, String>> {
        let path = doc.file.retarget.clone()?;
        if self.profile.as_ref().is_none_or(|(read, _)| *read != path) {
            self.profile = Some((
                path.clone(),
                doc.profile(assets).map(Option::unwrap_or_default),
            ));
        }
        self.profile.as_ref().map(|(_, profile)| profile.clone())
    }

    /// The status line and problems for the clip on its preview model.
    fn summary(&mut self, doc: &ClipDoc, assets: &Assets) -> &Summary {
        let model = doc
            .preview_model
            .as_deref()
            .and_then(|mesh_id| assets.model_blocking(mesh_id));
        let rigs = assets.rig_definitions();
        let made_from = format!(
            "{:?} {:?} {:p} {:?}",
            model.as_ref().map(|m| m.key()),
            doc.file,
            std::sync::Arc::as_ptr(&rigs),
            self.profile
                .as_ref()
                .map(|(_, p)| p.as_ref().ok().map(RetargetProfile::to_json)),
        );
        if self
            .summary
            .as_ref()
            .is_none_or(|s| s.made_from != made_from)
        {
            let mut problems = Vec::new();
            let skeleton = doc.take_skeleton();
            let status = match model.as_ref().and_then(|model| model.rig.as_deref()) {
                None => {
                    "Pick a preview model with a skeleton to see how the take fits it.".to_owned()
                }
                Some(rig) => {
                    let has_profile = doc.file.retarget.is_some();
                    let (status, check) = match doc.bind_on(assets, rig) {
                        Ok(clip) if clip.retargeted() => (
                            if has_profile {
                                "Retargeted through humanoid roles, with its profile.".to_owned()
                            } else {
                                "Retargeted through humanoid roles (the take was made for \
                                 another skeleton)."
                                    .to_owned()
                            },
                            true,
                        ),
                        Ok(_) if has_profile => (
                            "Plays by bone names: the skeletons can't be retargeted (see \
                             below), so its profile isn't used."
                                .to_owned(),
                            true,
                        ),
                        Ok(_) => (
                            "Plays by bone names: the take was made for this skeleton.".to_owned(),
                            false,
                        ),
                        Err(why) => (format!("Can't play on this model: {why}."), true),
                    };
                    // Checks matter only when the take goes through roles.
                    if check {
                        for problem in check_rig(&rig.bones, None, &rigs).problems {
                            problems.push(("Model", problem));
                        }
                        if !skeleton.is_empty() {
                            for problem in check_rig(&skeleton, None, &rigs).problems {
                                problems.push(("Take", problem));
                            }
                            for problem in check_pair(&skeleton, &rig.bones, &rigs) {
                                problems.push(("Both", problem));
                            }
                        }
                    }
                    problems.sort_by_key(|(_, problem)| problem.severity);
                    status
                }
            };
            self.summary = Some(Summary {
                made_from,
                status,
                problems,
            });
        }
        self.summary.as_ref().expect("just made")
    }

    /// Draws the section under the clip's settings.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        doc: &mut ClipDoc,
        assets: &Assets,
        project: &Path,
        output: &mut Output,
    ) {
        ui.separator();
        ui.strong("Retargeting");
        let profile = self.profile(doc, assets);
        let summary = self.summary(doc, assets);
        ui.label(&summary.status);
        let palette = theme::palette(ui.ctx());
        for (who, problem) in &summary.problems {
            let (icon, color) = match problem.severity {
                Severity::Error => (icons::X_CIRCLE, palette.error),
                Severity::Warning => (icons::WARNING, palette.warning),
                Severity::Note => (icons::INFO, ui.visuals().weak_text_color()),
            };
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(color, icon);
                ui.weak(format!("{who}:"));
                ui.label(&problem.message);
            });
        }

        ui.horizontal_wrapped(|ui| {
            let model = doc
                .preview_model
                .clone()
                .and_then(|mesh_id| Some((assets.model_blocking(&mesh_id)?, mesh_id)));
            if let Some((model, mesh_id)) = &model
                && let Some(rig) = &model.rig
                && ui
                    .button(format!("{} Model's bone map…", icons::PERSON_SIMPLE))
                    .on_hover_text("Which of the model's bones plays each humanoid role")
                    .clicked()
            {
                self.bone_map = Some(BoneMap::open(
                    Which::Model,
                    mesh_id,
                    rig.bones.clone(),
                    &assets.rig_definitions(),
                ));
            }
            let skeleton = doc.take_skeleton();
            if !skeleton.is_empty()
                && ui
                    .button(format!("{} Take's bone map…", icons::PERSON_SIMPLE))
                    .on_hover_text("Which bone of the take's own skeleton plays each humanoid role")
                    .clicked()
            {
                self.bone_map = Some(BoneMap::open(
                    Which::Take,
                    &doc.file.source,
                    skeleton,
                    &assets.rig_definitions(),
                ));
            }
        });

        // The profile: picked, made, and its settings.
        let paths = profile_paths(project);
        let current = doc.file.retarget.clone();
        let mut pick: Option<Option<String>> = None;
        crate::responsive::form(ui, "retarget profile", |ui| {
            ui.label("Profile");
            egui::ComboBox::from_id_salt("retarget profile pick")
                .selected_text(current.as_deref().map_or("None (defaults)", |path| {
                    path.rsplit('/').next().unwrap_or(path)
                }))
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(current.is_none(), "None (defaults)")
                        .clicked()
                    {
                        pick = Some(None);
                    }
                    for path in &paths {
                        let name = path.rsplit('/').next().unwrap_or(path);
                        if ui
                            .selectable_label(current.as_deref() == Some(path), name)
                            .on_hover_text(path)
                            .clicked()
                        {
                            pick = Some(Some(path.clone()));
                        }
                    }
                    ui.separator();
                    if ui.button(format!("{} New profile", icons::PLUS)).clicked() {
                        pick = Some(Some(new_profile_path(project, &doc.path)));
                    }
                });
            ui.end_row();
        });
        if let Some(path) = pick
            && let Err(err) = self.use_profile(doc, project, path)
        {
            output.error(format!("Could not make the retarget profile: {err}"));
        }

        match profile {
            Some(Err(why)) => {
                ui.colored_label(palette.error, why);
            }
            Some(Ok(profile)) => {
                if let Some(changed) = profile_settings(ui, &profile)
                    && let Err(err) = self.change_profile(doc, project, changed)
                {
                    output.error(format!("Could not save the retarget profile: {err}"));
                }
            }
            None => {
                ui.weak(
                    "Without a profile, rest poses are matched, the hips travel scaled to the \
                     legs, and every bone plays fully. Make one to plant feet, place hands, \
                     play in place or tune single bones; clips can share it.",
                );
            }
        }

        if let Some(saved) = self.bone_map_window(ui.ctx(), project, assets, output) {
            output.info(format!("Saved the bone map as `{saved}`."));
            self.summary = None;
            doc.rebind();
        }
    }

    /// The bone map window, while one is open. Returns the rig definition
    /// saved, if Save was pressed.
    fn bone_map_window(
        &mut self,
        ctx: &egui::Context,
        project: &Path,
        assets: &Assets,
        output: &mut Output,
    ) -> Option<String> {
        let map = self.bone_map.as_mut()?;
        let mut open = true;
        let mut saved = None;
        let title = match map.which {
            Which::Model => format!("Bone map: {}", map.file.rsplit('/').next().unwrap_or("")),
            Which::Take => format!(
                "Take's bone map: {}",
                map.file.rsplit('/').next().unwrap_or("")
            ),
        };
        crate::widgets::tool_window(icons::BONE, &title)
            .id("retarget bone map")
            .size(egui::vec2(360.0, 480.0))
            .show(ctx, &mut open, |ui| {
                ui.weak(format!(
                    "Saved as `{}`, it applies to every skeleton that has these bones.",
                    map.path
                ));
                let problems = map.problems();
                let palette = theme::palette(ui.ctx());
                for problem in problems.iter().filter(|p| p.severity != Severity::Note) {
                    let color = match problem.severity {
                        Severity::Error => palette.error,
                        _ => palette.warning,
                    };
                    ui.colored_label(color, &problem.message);
                }
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            map.changed,
                            egui::Button::new(format!("{} Save", icons::FLOPPY_DISK)),
                        )
                        .clicked()
                    {
                        match map.save(project, assets) {
                            Ok(()) => saved = Some(map.path.clone()),
                            Err(err) => output.error(format!("Could not save the bone map: {err}")),
                        }
                    }
                    if ui
                        .button("Match by names")
                        .on_hover_text("Start again from what the bone names say")
                        .clicked()
                    {
                        map.map = HumanoidMap::detect(&map.bones);
                        map.changed = true;
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let groups: [(&str, Vec<HumanBone>); 3] = [
                        (
                            "Body",
                            HumanBone::all().filter(|r| !r.is_finger()).collect(),
                        ),
                        ("Left fingers", HumanBone::fingers(Side::Left).collect()),
                        ("Right fingers", HumanBone::fingers(Side::Right).collect()),
                    ];
                    for (group, roles) in groups {
                        egui::CollapsingHeader::new(group)
                            .default_open(group == "Body")
                            .show(ui, |ui| {
                                egui::Grid::new(("bone map", group))
                                    .num_columns(2)
                                    .striped(true)
                                    .show(ui, |ui| {
                                        for role in roles {
                                            role_row(ui, map, role);
                                            ui.end_row();
                                        }
                                    });
                            });
                    }
                });
            });
        if !open {
            self.bone_map = None;
        }
        saved
    }
}

/// One role's row in the bone map: its name and a list of the bones.
fn role_row(ui: &mut egui::Ui, map: &mut BoneMap, role: HumanBone) {
    let required = HumanBone::REQUIRED.contains(&role);
    if required {
        ui.label(role.name())
            .on_hover_text("Needed for retargeting");
    } else {
        ui.weak(role.name());
    }
    let current = map.map.get(role);
    let shown = current
        .and_then(|bone| map.bones.get(bone))
        .map_or("(none)", |bone| bone.name.as_str());
    let mut picked = None;
    egui::ComboBox::from_id_salt(("bone map role", role.index()))
        .selected_text(shown)
        .width(180.0)
        .show_ui(ui, |ui| {
            if ui.selectable_label(current.is_none(), "(none)").clicked() {
                picked = Some(None);
            }
            for (index, bone) in map.bones.iter().enumerate() {
                if bone.name.is_empty() {
                    continue;
                }
                if ui
                    .selectable_label(current == Some(index), &bone.name)
                    .clicked()
                {
                    picked = Some(Some(index));
                }
            }
        });
    if let Some(bone) = picked {
        map.set(role, bone);
    }
}

/// The profile's settings. Returns the profile changed, if it was.
fn profile_settings(ui: &mut egui::Ui, profile: &RetargetProfile) -> Option<RetargetProfile> {
    let mut edited = profile.clone();
    crate::responsive::form(ui, "retarget profile settings", |ui| {
        ui.label("Match rest poses");
        ui.checkbox(&mut edited.match_rest_pose, "").on_hover_text(
            "Turn each bone to rest as the take's skeleton rests first, so a T-pose take plays right on an A-pose model",
        );
        ui.end_row();

        ui.label("Hips");
        egui::ComboBox::from_id_salt("retarget hips")
            .selected_text(edited.hips.label())
            .show_ui(ui, |ui| {
                for travel in HipsTravel::ALL {
                    ui.selectable_value(&mut edited.hips, travel, travel.label());
                }
            });
        ui.end_row();

        ui.label("Plant feet");
        ui.checkbox(&mut edited.plant_feet, "").on_hover_text(
            "Put each foot where the take's is with leg IK, so feet on the ground stay there when the legs are another shape",
        );
        ui.end_row();

        ui.label("Match hands");
        ui.checkbox(&mut edited.match_hands, "").on_hover_text(
            "Put each hand where the take's is from the hips, with arm IK, so hands that touch keep touching",
        );
        ui.end_row();

        ui.label("Fingers");
        ui.add(egui::Slider::new(&mut edited.fingers, 0.0..=1.0))
            .on_hover_text("How much of the take's finger motion plays");
        ui.end_row();
    });
    egui::CollapsingHeader::new("Single bones")
        .id_salt("retarget roles")
        .show(ui, |ui| {
            ui.weak("Weight: how much of the take's turn a bone gets. Offset: an extra turn about its own axes, in degrees.");
            egui::Grid::new("retarget role settings")
                .num_columns(5)
                .striped(true)
                .show(ui, |ui| {
                    for role in HumanBone::all().filter(|role| !role.is_finger()) {
                        let mut setting = edited
                            .roles
                            .iter()
                            .find(|(name, _)| HumanBone::from_name(name) == Some(role))
                            .map(|(_, setting)| *setting)
                            .unwrap_or_default();
                        ui.label(role.name());
                        let mut changed = ui
                            .add(egui::DragValue::new(&mut setting.weight).range(0.0..=1.0).speed(0.01))
                            .changed();
                        for (axis, value) in ["X ", "Y ", "Z "].into_iter().zip(&mut setting.offset) {
                            changed |= ui
                                .add(egui::DragValue::new(value).range(-180.0..=180.0).prefix(axis).suffix("°"))
                                .changed();
                        }
                        if changed {
                            edited.set_role(role, setting);
                        }
                        ui.end_row();
                    }
                });
            if !edited.roles.is_empty() && ui.button("Reset single bones").clicked() {
                edited.roles.clear();
            }
        });
    (edited != *profile).then_some(edited)
}

#[cfg(test)]
#[path = "retarget_panel_tests.rs"]
mod tests;
