//! The Character Studio tab's actions (facial animation F6; the tab is
//! `character_studio.rs`): control sliders edit the FaceControls as
//! Properties does, and profiles, expressions and lip syncs save as files,
//! one undo step each (`History::record_asset`).

use std::path::Path;

use eframe::egui;
use eframe::egui_wgpu::RenderState;

use super::*;
use crate::character_studio::StudioAction;

impl EditorApp {
    /// Draws the Character Studio tab for the selection.
    pub(super) fn character_studio_pane(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&RenderState>,
    ) {
        let world = shown(&self.world, &self.play);
        let action = self
            .character_studio
            .ui(ui, world, &self.assets, self.selected, render_state);
        if let Some(action) = action {
            self.character_action(action);
        }
    }

    /// Does what the Character Studio tab asked.
    pub(super) fn character_action(&mut self, action: StudioAction) {
        match action {
            StudioAction::Set {
                face,
                property,
                value,
                done,
            } => {
                self.apply(face, Edit::Property(property, value));
                if done {
                    self.history.seal();
                }
            }
            StudioAction::SaveProfile {
                face,
                path,
                profile,
            } => {
                let written = self.save_face_file(&path, profile.to_json(), |folder| {
                    axcenity_assets::write_face_profile(folder, &path, &profile)
                });
                if written {
                    self.character_studio.rig.discard();
                    if let Some(face) = face {
                        self.apply(face, Edit::Property("Profile", Value::String(path)));
                        self.history.seal();
                    }
                }
            }
            StudioAction::SaveExpression { path, expression } => {
                self.save_face_file(&path, expression.to_json(), |folder| {
                    axcenity_assets::write_face_expression(folder, &path, &expression)
                });
            }
            StudioAction::SaveLipSync { path, lip_sync } => {
                if self.save_face_file(&path, lip_sync.to_json(), |folder| {
                    axcenity_assets::write_lip_sync(folder, &path, &lip_sync)
                }) {
                    self.character_studio.lip_sync.dirty = false;
                }
            }
            StudioAction::MakeLipSync { source } => {
                if let Some((path, lip)) = self.make_lip_sync_file(&source) {
                    self.character_studio.open_lip_sync(&path, lip);
                    self.character_studio.rescan();
                }
            }
            StudioAction::KeyInAnimation(face) => {
                self.select_from_viewport(Some(face));
                self.layout.open(Pane::Animation);
            }
        }
    }

    /// Writes `text` to the project file `path` with `write` (given the
    /// project folder), as one undo step. Returns whether it was written.
    pub(super) fn save_face_file(
        &mut self,
        path: &str,
        text: String,
        write: impl FnOnce(&Path) -> std::io::Result<()>,
    ) -> bool {
        let Some(folder) = self.project.as_ref().map(|p| p.folder.clone()) else {
            self.output.warning("Open a project first.");
            return false;
        };
        let before = std::fs::read_to_string(folder.join(path)).ok();
        if let Err(err) = write(&folder) {
            self.output
                .error(format!("`{path}` can't be written: {err}."));
            return false;
        }
        self.assets.forget(path);
        let verb = if before.is_some() { "Save" } else { "Make" };
        self.history.record_asset(
            &mut self.world,
            format!("{verb} {path}"),
            vec![history::TextChange {
                path: path.to_owned(),
                before,
                after: Some(text),
            }],
            Vec::new(),
        );
        self.asset_browser.refresh();
        self.output.info(format!("Saved `{path}`."));
        true
    }
}

#[cfg(test)]
mod tests {
    use axcenity_assets::FaceExpression;
    use axcenity_core::{ClassName, Value, Vector3};

    use super::*;
    use crate::character_studio::{Section, Target};

    fn draw(app: &mut EditorApp) -> String {
        let ctx = crate::theme::test_context();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(260.0, 4000.0),
            )),
            ..Default::default()
        };
        let output = crate::theme::run(&ctx, input, |ui| app.character_studio_pane(ui, None));
        format!("{:?}", output.shapes)
    }

    #[test]
    fn the_studio_edits_a_face_and_saves_its_files_as_undo_steps() {
        let folder = super::super::tests::copy_of_example("hello-character", "character-studio");
        let mut app = EditorApp::new(None);
        app.open(&folder);
        let workspace = app.world.workspace();
        let part = app.world.create(ClassName::MeshPart).unwrap();
        app.world.set_name(part, "Remy").unwrap();
        for (name, value) in [
            (
                "MeshId",
                Value::String("assets/models/Walking/Walking.fbx".into()),
            ),
            ("Size", Value::Vector3(Vector3::new(2.0, 5.0, 1.0))),
        ] {
            app.world.set_property(part, name, value).unwrap();
        }
        app.world.set_parent(part, Some(workspace)).unwrap();
        let face = app.world.create(ClassName::FaceControls).unwrap();
        app.world.set_parent(face, Some(part)).unwrap();
        app.history.seal();

        // Selecting the part shows it and its face.
        app.select_from_viewport(Some(part));
        let shapes = draw(&mut app);
        for text in [
            "Remy",
            "Rig",
            "Expressions",
            "Lip sync",
            "Blink_Left",
            "Matched to axcenity",
        ] {
            assert!(shapes.contains(text), "`{text}` is drawn");
        }
        let studio = &app.character_studio;
        assert_eq!(
            studio.target(),
            Some(Target {
                part,
                face: Some(face)
            })
        );
        assert_eq!(studio.profile_name(), Some("axcenity"));
        // Remy's eyes turn; the camera looks at his head, near the top.
        let (at, _, _) = studio.framing();
        assert!(at.y > 1.0, "{at}");
        // Something else selected keeps him shown.
        app.select_from_viewport(None);
        draw(&mut app);
        assert_eq!(app.character_studio.target().map(|t| t.part), Some(part));

        // A slider is an undo step like Properties'.
        let look = |app: &EditorApp| {
            app.world
                .get(face)
                .unwrap()
                .property("Eye_Look_Left")
                .cloned()
        };
        app.character_action(StudioAction::Set {
            face,
            property: "Eye_Look_Left",
            value: Value::Number(0.7),
            done: true,
        });
        assert_eq!(look(&app), Some(Value::Number(0.7)));
        app.undo();
        assert_eq!(look(&app), Some(Value::Number(0.0)));

        // An expression saves as a file the face can play, one undo step.
        let mut grin = FaceExpression::default();
        grin.controls.insert("Mouth_Smile_Left".into(), 0.8);
        let path = "assets/faces/expressions/Grin.expression.json";
        app.character_action(StudioAction::SaveExpression {
            path: path.into(),
            expression: grin,
        });
        assert!(folder.join(path).exists());
        assert!(app.assets.face_expressions().find("grin").is_some());
        app.undo();
        assert!(!folder.join(path).exists(), "undo takes the new file away");

        // A profile saves and becomes the face's.
        let model = app
            .assets
            .model_blocking("assets/models/Walking/Walking.fbx");
        app.character_studio.rig.edit(model.as_deref());
        let draft = app.character_studio.rig.draft.clone().expect("editing");
        let path = "assets/faces/Remy.face.json";
        app.character_action(StudioAction::SaveProfile {
            face: Some(face),
            path: path.into(),
            profile: draft.profile,
        });
        assert!(folder.join(path).exists());
        assert!(app.character_studio.rig.draft.is_none());
        let profile = app.world.get(face).unwrap().property("Profile").cloned();
        assert_eq!(profile, Some(Value::String(path.into())));
        draw(&mut app);
        assert_eq!(app.character_studio.profile_name(), Some(path));

        // A lip sync is made from a sound and opens in the Lip sync section.
        std::fs::create_dir_all(folder.join("voice")).unwrap();
        std::fs::write(
            folder.join("voice/hum.wav"),
            super::super::lip_sync::tests::hum_wav(),
        )
        .unwrap();
        app.character_action(StudioAction::MakeLipSync {
            source: "voice/hum.wav".into(),
        });
        let studio = &app.character_studio;
        assert_eq!(studio.section, Section::LipSync);
        assert_eq!(
            studio.lip_sync.path.as_deref(),
            Some("voice/hum.lipsync.json")
        );
        assert!(!studio.lip_sync.dirty);
        let shapes = draw(&mut app);
        for text in [
            "Make from",
            "voice/hum.lipsync.json",
            "Silence",
            "Add at play head",
        ] {
            assert!(shapes.contains(text), "`{text}` is drawn");
        }
        // Changed and saved.
        app.character_studio.lip_sync.lip.visemes[1].viseme = "O".into();
        app.character_studio.lip_sync.dirty = true;
        let lip_sync = app.character_studio.lip_sync.lip.clone();
        app.character_action(StudioAction::SaveLipSync {
            path: "voice/hum.lipsync.json".into(),
            lip_sync,
        });
        assert!(!app.character_studio.lip_sync.dirty);
        let text = std::fs::read_to_string(folder.join("voice/hum.lipsync.json")).unwrap();
        assert!(text.contains("\"O\""), "{text}");

        // The Expressions section draws the built-in ones.
        app.character_studio.section = Section::Expressions;
        let shapes = draw(&mut app);
        for text in ["Happy", "Sleepy", "Preview at", "Save expression"] {
            assert!(shapes.contains(text), "`{text}` is drawn");
        }

        // `AXCENITY_SNAPSHOTS=<folder>` saves each section, preview drawn.
        if let Some(shots) = std::env::var_os("AXCENITY_SNAPSHOTS") {
            let shots = std::path::PathBuf::from(shots);
            app.select_from_viewport(Some(part));
            for (section, name) in [
                (Section::Rig, "rig"),
                (Section::Expressions, "expressions"),
                (Section::LipSync, "lip-sync"),
            ] {
                app.character_studio.section = section;
                app.character_studio.forget_canvas();
                if section == Section::Expressions {
                    app.character_studio.expressions.pick(&app.assets, "Happy");
                }
                crate::snapshot::snapshot_rendered(
                    &shots.join(format!("character-studio-{name}.png")),
                    egui::vec2(460.0, 900.0),
                    vec![Vec::new(); 3],
                    |ui, render_state| app.character_studio_pane(ui, Some(render_state)),
                );
            }
            app.character_studio.section = Section::Expressions;
        }

        // Key in Animation selects the face and opens the Animation tab.
        app.character_action(StudioAction::KeyInAnimation(face));
        assert_eq!(app.selected, Some(face));
        assert!(app.layout.is_open(Pane::Animation));
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn with_nothing_to_show_the_tab_says_what_to_select() {
        let mut app = EditorApp::new(None);
        let shapes = draw(&mut app);
        assert!(shapes.contains("No character selected"));
    }
}
