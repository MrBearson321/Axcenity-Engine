//! The Character Studio tab (facial animation F6, decision D9;
//! `docs/research/facial-animation.md`): the selected character's face in
//! a preview of its own, and its Face workspace, in three sections:
//!
//! - **Rig** (`rig.rs`): the face profile the FaceControls uses, what
//!   doesn't fit the model, its 48 controls as sliders (edits of the
//!   FaceControls, one undo step each) and a profile editor (which blend
//!   shapes each control drives, the eye and jaw bones and their limits),
//!   saved as `assets/faces/<name>.face.json`.
//! - **Expressions** (`expressions.rs`): the built-in and the project's
//!   expressions, previewed at an intensity and blended with another, and
//!   edited and saved as `assets/faces/expressions/<name>.expression.json`.
//! - **Lip sync** (`lip_sync.rs`): a `*.lipsync.json` on a timeline over
//!   its sound's waveform, its visemes picked, moved, changed, added and
//!   deleted, played back on the face, made from a sound or phoneme
//!   timings, and saved.
//!
//! Keying the face over time is the Animation tab's (F4b): Key in
//! Animation selects the FaceControls there. The tab only draws and
//! returns a [`StudioAction`]; `app/character_studio.rs` does it.

mod expressions;
mod lip_sync;
mod rig;
#[cfg(test)]
mod tests;

use std::sync::Arc;
use std::time::Instant;

use axcenity_anim::{FaceProblem, FaceRig};
use axcenity_assets::{Assets, FaceExpression, FaceProfile, LipSync, Model};
use axcenity_core::face::FACE_CONTROLS;
use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};
use axcenity_editor_tools::preview::Canvas;
use axcenity_render::Camera;
use axcenity_render::glam::{Mat4, Vec3};
use eframe::egui;
use eframe::egui_wgpu::RenderState;

pub use expressions::ExpressionEditor;
pub use lip_sync::LipSyncEditor;

use crate::theme::{self, icons};
use crate::widgets;

/// The Face workspace's sections.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Section {
    #[default]
    Rig,
    Expressions,
    LipSync,
}

/// What the tab asks the editor to do.
#[derive(Clone, Debug, PartialEq)]
pub enum StudioAction {
    /// Sets a property of the FaceControls `face`; `done` ends the undo
    /// step (a slider let go).
    Set {
        face: InstanceId,
        property: &'static str,
        value: Value,
        done: bool,
    },
    /// Saves `profile` to `path` and, with `face`, makes it its Profile.
    SaveProfile {
        face: Option<InstanceId>,
        path: String,
        profile: FaceProfile,
    },
    SaveExpression {
        path: String,
        expression: FaceExpression,
    },
    SaveLipSync {
        path: String,
        lip_sync: LipSync,
    },
    /// Makes a lip sync beside `source` (a sound or phoneme timings).
    MakeLipSync {
        source: String,
    },
    /// Selects the FaceControls and opens the Animation tab to key it.
    KeyInAnimation(InstanceId),
}

/// The character shown: a MeshPart, and its FaceControls if it has one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    pub part: InstanceId,
    pub face: Option<InstanceId>,
}

/// The character `selected` stands for: a FaceControls in a MeshPart, a
/// MeshPart (with its FaceControls, if any), or a Model or folder with a
/// FaceControls in a MeshPart inside.
pub fn face_target(world: &World, selected: Option<InstanceId>) -> Option<Target> {
    let id = selected?;
    let instance = world.get(id)?;
    let is_mesh_part = |id: InstanceId| {
        world
            .get(id)
            .is_some_and(|i| i.class() == ClassName::MeshPart)
    };
    let face_in = |part: InstanceId| {
        world.get(part)?.children().iter().copied().find(|&child| {
            world
                .get(child)
                .is_some_and(|c| c.class() == ClassName::FaceControls)
        })
    };
    match instance.class() {
        ClassName::FaceControls => {
            let part = instance.parent().filter(|&p| is_mesh_part(p))?;
            Some(Target {
                part,
                face: Some(id),
            })
        }
        ClassName::MeshPart => Some(Target {
            part: id,
            face: face_in(id),
        }),
        _ => world.descendants(id).into_iter().find_map(|child| {
            let face = world.get(child)?;
            (face.class() == ClassName::FaceControls).then_some(())?;
            let part = face.parent().filter(|&p| is_mesh_part(p))?;
            Some(Target {
                part,
                face: Some(child),
            })
        }),
    }
}

/// The face profile matched to the shown model.
struct Bound {
    /// What it was bound for: the model, the profile wanted and the
    /// project's profiles then (or the draft's revision).
    key: (u64, String, usize),
    name: String,
    rig: Arc<FaceRig>,
    problems: Vec<FaceProblem>,
}

/// The tab: what it shows, its preview and each section's state.
pub struct CharacterStudio {
    pub section: Section,
    target: Option<Target>,
    /// The copy of the MeshPart the preview draws, and what it was copied
    /// from (part, MeshId, Size).
    shown: World,
    shown_part: Option<InstanceId>,
    copied: Option<(InstanceId, String, [u32; 3])>,
    canvas: Canvas,
    yaw: f32,
    pitch: f32,
    zoom: f32,
    model: Option<Arc<Model>>,
    bound: Option<Bound>,
    pub rig: rig::RigEditor,
    pub expressions: ExpressionEditor,
    pub lip_sync: LipSyncEditor,
    last_frame: Option<Instant>,
}

impl Default for CharacterStudio {
    fn default() -> Self {
        Self {
            section: Section::Rig,
            target: None,
            shown: World::new(),
            shown_part: None,
            copied: None,
            canvas: Canvas::default(),
            yaw: 0.0,
            pitch: 5.0,
            zoom: 1.0,
            model: None,
            bound: None,
            rig: rig::RigEditor::default(),
            expressions: ExpressionEditor::default(),
            lip_sync: LipSyncEditor::default(),
            last_frame: None,
        }
    }
}

impl CharacterStudio {
    /// The character shown.
    #[cfg(test)]
    pub fn target(&self) -> Option<Target> {
        self.target
    }

    /// The name of the face profile the shown model is matched to.
    #[cfg(test)]
    pub fn profile_name(&self) -> Option<&str> {
        self.bound.as_ref().map(|bound| bound.name.as_str())
    }

    /// Follows the selection (keeping the last character while something
    /// else is selected), copies it for the preview and matches its
    /// profile.
    pub fn follow(&mut self, world: &World, assets: &Assets, selected: Option<InstanceId>) {
        let target = face_target(world, selected).or_else(|| {
            self.target
                .filter(|target| world.get(target.part).is_some())
                .map(|target| Target {
                    // Its FaceControls may have come or gone.
                    face: face_target(world, Some(target.part)).and_then(|t| t.face),
                    ..target
                })
        });
        if target.map(|t| t.part) != self.target.map(|t| t.part) {
            self.zoom = 1.0;
            self.yaw = 0.0;
            self.rig.discard();
        }
        self.target = target;
        let Some(target) = target else {
            self.copied = None;
            self.shown = World::new();
            self.shown_part = None;
            self.model = None;
            self.bound = None;
            return;
        };
        let part = world.get(target.part).expect("followed");
        let mesh = part
            .property("MeshId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let size = part
            .property("Size")
            .and_then(Value::as_vector3)
            .unwrap_or(Vector3::new(1.0, 1.0, 1.0));
        let key = (
            target.part,
            mesh.clone(),
            [size.x.to_bits(), size.y.to_bits(), size.z.to_bits()],
        );
        if self.copied.as_ref() != Some(&key) {
            self.copy(world, target.part);
            self.copied = Some(key);
        }
        self.model = (!mesh.is_empty())
            .then(|| assets.model_blocking(&mesh))
            .flatten();
        self.bind(world, assets);
    }

    /// Copies the MeshPart alone, at the origin and unturned, into a
    /// world of its own.
    fn copy(&mut self, world: &World, part: InstanceId) {
        let mut shown = World::new();
        let workspace = shown.workspace();
        let Ok(copy) = shown.copy_from(world, part) else {
            return;
        };
        let _ = shown.set_parent(copy, Some(workspace));
        for (name, value) in [
            ("Position", Value::Vector3(Vector3::new(0.0, 0.0, 0.0))),
            ("Orientation", Value::Vector3(Vector3::new(0.0, 0.0, 0.0))),
            ("Transparency", Value::Number(0.0)),
        ] {
            let _ = shown.set_property(copy, name, value);
        }
        let lighting = shown.lighting();
        let _ = shown.set_property(lighting, "ClockTime", Value::Number(14.0));
        self.shown = shown;
        self.shown_part = Some(copy);
    }

    /// Matches the profile the FaceControls names (or the draft being
    /// edited) to the model, again only when one of them changed.
    fn bind(&mut self, world: &World, assets: &Assets) {
        let Some(model) = self.model.clone() else {
            self.bound = None;
            return;
        };
        let wanted = self
            .target
            .and_then(|t| t.face)
            .and_then(|face| world.get(face))
            .and_then(|face| face.property("Profile"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let profiles = assets.face_profiles();
        let key = match &self.rig.draft {
            Some(draft) => (model.key(), format!("draft:{}", draft.from), draft.revision),
            None => (model.key(), wanted.clone(), Arc::as_ptr(&profiles) as usize),
        };
        if self.bound.as_ref().is_some_and(|bound| bound.key == key) {
            return;
        }
        let (name, profile) = match &self.rig.draft {
            Some(draft) => (draft.from.clone(), draft.profile.clone()),
            None => match axcenity_anim::choose_face_profile(&model, &profiles, &wanted) {
                Ok(chosen) => {
                    self.rig.error = None;
                    chosen
                }
                Err(why) => {
                    self.rig.error = Some(why);
                    (
                        "axcenity".to_owned(),
                        axcenity_anim::built_in_face_profile("axcenity").expect("built in"),
                    )
                }
            },
        };
        let (rig, problems) = FaceRig::bind(&model, &name, &profile);
        self.rig.matched = Some((name.clone(), profile));
        self.bound = Some(Bound {
            key,
            name,
            rig: Arc::new(rig),
            problems,
        });
    }

    /// The face's control values to preview, by section: the
    /// FaceControls' own (Rig), the expression (Expressions), or the
    /// visemes at the play head (Lip sync).
    pub fn preview_controls(&self, world: &World, assets: &Assets) -> Vec<f32> {
        let own = || {
            let face = self.target.and_then(|t| t.face).and_then(|f| world.get(f));
            FACE_CONTROLS
                .iter()
                .map(|control| {
                    face.and_then(|face| face.property(control.name))
                        .and_then(Value::as_number)
                        .map_or(0.0, |value| value as f32)
                })
                .collect::<Vec<f32>>()
        };
        match self.section {
            Section::Rig => own(),
            Section::Expressions => self.expressions.controls(assets),
            Section::LipSync => {
                let face = self.target.and_then(|t| t.face).and_then(|f| world.get(f));
                self.lip_sync.controls(face, assets)
            }
        }
    }

    /// Draws the tab for the selection. Returns what it asks for.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        world: &World,
        assets: &Assets,
        selected: Option<InstanceId>,
        render_state: Option<&RenderState>,
    ) -> Option<StudioAction> {
        self.follow(world, assets, selected);
        let now = Instant::now();
        let seconds = self
            .last_frame
            .map_or(0.0, |last| now.duration_since(last).as_secs_f64());
        self.last_frame = Some(now);
        let Some(target) = self.target else {
            widgets::empty_state(
                ui,
                icons::USER_FOCUS,
                "No character selected",
                "Select a MeshPart whose model has blend shapes or eye and jaw bones, its \
                 FaceControls, or the model holding it, to edit its face here.",
                None,
            );
            return None;
        };
        if self.lip_sync.advance(seconds) {
            ui.ctx().request_repaint();
        }
        let mut action = None;
        let name = world
            .get(target.part)
            .map_or(String::new(), |i| i.name().to_owned());
        responsive_header(ui, &name, &mut self.section);
        let height = (ui.available_height() * 0.45).clamp(150.0, 420.0);
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width().max(1.0), height),
            egui::Sense::click_and_drag(),
        );
        self.orbit(ui, &response);
        self.draw_preview(ui, rect, world, assets, render_state);
        widgets::divider(ui);
        let folder = assets.folder();
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .id_salt("character-studio")
            .show(ui, |ui| {
                action = match self.section {
                    Section::Rig => {
                        let bound = self
                            .bound
                            .as_ref()
                            .map(|b| (b.name.as_str(), &*b.rig, &b.problems[..]));
                        let profiles = assets.face_profiles();
                        self.rig
                            .ui(ui, world, &profiles, target, self.model.as_deref(), bound)
                    }
                    Section::Expressions => self.expressions.ui(ui, assets),
                    Section::LipSync => self.lip_sync.ui(ui, folder.as_deref()),
                };
            });
        action
    }

    fn orbit(&mut self, ui: &egui::Ui, response: &egui::Response) {
        if response.dragged_by(egui::PointerButton::Primary) {
            let delta = response.drag_delta();
            self.yaw -= delta.x * 0.4;
            self.pitch = (self.pitch + delta.y * 0.3).clamp(-60.0, 75.0);
        }
        if response.hovered() {
            let scroll = ui.input(|input| input.smooth_scroll_delta.y);
            self.zoom = (self.zoom * (1.0 - scroll * 0.002)).clamp(0.3, 8.0);
        }
        if response.double_clicked() {
            (self.zoom, self.yaw, self.pitch) = (1.0, 0.0, 5.0);
        }
    }

    /// Where the camera looks: the face (its head bone) and how far off it
    /// stands, or for a model without bones its top.
    pub fn framing(&self) -> (Vec3, f32, Vec3) {
        let size = self
            .shown_part
            .and_then(|part| self.shown.get(part))
            .and_then(|part| part.property("Size"))
            .and_then(Value::as_vector3)
            .map_or(Vec3::ONE, |size| Vec3::new(size.x, size.y, size.z));
        let scale = Mat4::from_scale(size);
        let bound = self.bound.as_ref();
        let face = self
            .model
            .as_deref()
            .zip(bound)
            .and_then(|(model, bound)| axcenity_anim::face_point(model, &bound.rig));
        let forward = self
            .model
            .as_ref()
            .and_then(|model| model.rig.as_ref())
            .zip(bound)
            .map(|(rig, bound)| (scale * rig.fit).transform_vector3(bound.rig.forward()))
            .map(|f| Vec3::new(f.x, 0.0, f.z).normalize_or_zero())
            .filter(|f| *f != Vec3::ZERO)
            .unwrap_or(Vec3::Z);
        match face {
            Some(point) => {
                // The head bone sits low in the head: look a little above.
                let at = scale.transform_point3(point) + Vec3::Y * size.y * 0.03;
                (at, (size.y * 0.45).clamp(0.5, 12.0), forward)
            }
            None => (
                Vec3::new(0.0, size.y * 0.35, 0.0),
                (size.max_element() * 0.9).clamp(0.5, 40.0),
                forward,
            ),
        }
    }

    fn camera(&self) -> Camera {
        let (at, reach, forward) = self.framing();
        let base = forward.x.atan2(forward.z);
        let (yaw, pitch) = (base + self.yaw.to_radians(), self.pitch.to_radians());
        let direction = Vec3::new(
            yaw.sin() * pitch.cos(),
            pitch.sin(),
            yaw.cos() * pitch.cos(),
        );
        Camera {
            position: at + direction * reach * self.zoom,
            look_at: at,
            fov_y_degrees: 35.0,
        }
    }

    fn draw_preview(
        &mut self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        world: &World,
        assets: &Assets,
        render_state: Option<&RenderState>,
    ) {
        let palette = theme::palette(ui.ctx());
        let Some(render_state) = render_state else {
            ui.painter_at(rect)
                .rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
            return;
        };
        let mut poses = axcenity_scene::Poses::new();
        if let (Some(part), Some(model), Some(bound)) =
            (self.shown_part, self.model.as_deref(), self.bound.as_ref())
        {
            let controls = self.preview_controls(world, assets);
            poses.insert(
                part,
                Arc::new(axcenity_anim::pose_face(model, &bound.rig, &controls)),
            );
        }
        let list = axcenity_scene::build_draw_list_posed(
            &self.shown,
            assets,
            self.camera(),
            &poses,
            &mut axcenity_scene::TerrainCache::new(),
        );
        self.canvas.paint(ui, rect, render_state, &list);
        if self.model.is_none() {
            ui.painter_at(rect).text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "The model is loading or can't be read.",
                egui::FontId::proportional(13.0),
                palette.text_weak,
            );
        }
    }

    /// Opens the lip sync at `path` in the Lip sync section.
    pub fn open_lip_sync(&mut self, path: &str, lip_sync: LipSync) {
        self.section = Section::LipSync;
        self.lip_sync.open(path, lip_sync);
    }

    /// Drops the preview's renderer, for snapshots each made with a
    /// renderer of their own.
    #[cfg(test)]
    pub fn forget_canvas(&mut self) {
        self.canvas = Canvas::default();
    }

    /// Forgets what was read from the project (after a file changed).
    pub fn rescan(&mut self) {
        self.lip_sync.rescan();
    }
}

fn responsive_header(ui: &mut egui::Ui, name: &str, section: &mut Section) {
    crate::responsive::toolbar(ui, |ui| {
        ui.label(egui::RichText::new(name).strong());
        widgets::segmented_tips(
            ui,
            section,
            &[
                (Section::Rig, Some(icons::SLIDERS_HORIZONTAL), "Rig"),
                (Section::Expressions, Some(icons::SMILEY), "Expressions"),
                (Section::LipSync, Some(icons::WAVEFORM), "Lip sync"),
            ],
            &[
                "The face profile, its controls and what they move",
                "Make, preview and blend expressions",
                "Edit a line's mouth shapes over its sound",
            ],
        );
    });
}

/// `name` as a file name: letters, digits, spaces, `-` and `_`, or `None`
/// when nothing is left.
pub fn file_name(name: &str) -> Option<String> {
    let kept: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'))
        .collect();
    let kept = kept.trim().to_owned();
    (!kept.is_empty()).then_some(kept)
}

/// The project's files whose names end with one of `endings` (any case),
/// by path, sorted; hidden folders and build output are skipped.
pub fn project_files(folder: &std::path::Path, endings: &[&str]) -> Vec<String> {
    fn walk(at: &std::path::Path, relative: &str, endings: &[&str], found: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(at) else {
            return;
        };
        for entry in entries.flatten() {
            if found.len() >= 5_000 {
                return;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let path = if relative.is_empty() {
                name.clone()
            } else {
                format!("{relative}/{name}")
            };
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                if !matches!(name.as_str(), "target" | "build" | "node_modules") {
                    walk(&entry.path(), &path, endings, found);
                }
            } else {
                let lower = name.to_ascii_lowercase();
                if endings.iter().any(|end| lower.ends_with(end)) {
                    found.push(path);
                }
            }
        }
    }
    let mut found = Vec::new();
    walk(folder, "", endings, &mut found);
    found.sort();
    found
}

/// A folder for tests to make project files in.
#[cfg(test)]
pub fn test_folder(name: &str) -> std::path::PathBuf {
    let folder =
        std::env::temp_dir().join(format!("axcenity-studio-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    folder
}
