//! The Effect tab's actions (roadmap R17.3; the tab is `effect_editor.rs`):
//! its module stack edits emitters as Properties does (one undo step
//! each), and Save as Effect saves the effect as a prefab in
//! `assets/effects`, so placing, overrides, Apply and Revert work as for
//! any prefab (`docs/research/vfx-editor.md`, D5).

use eframe::egui;
use eframe::egui_wgpu::RenderState;

use super::*;
use crate::asset_browser::EFFECTS_FOLDER;
use crate::effect_editor::EffectAction;

impl EditorApp {
    /// Draws the Effect tab for the selection (or the last effect shown).
    pub(super) fn effect_pane(&mut self, ui: &mut egui::Ui, render_state: Option<&RenderState>) {
        let playing = self.playing();
        let world = shown(&self.world, &self.play);
        let revision = self.history.revision().0 ^ u64::from(playing);
        let action = self.effect_editor.ui(
            ui,
            world,
            &self.assets,
            self.selected,
            revision,
            render_state,
        );
        match action {
            Some(EffectAction::Edit(id, edit)) => self.apply(id, edit),
            Some(EffectAction::Select(id)) => self.select_from_viewport(Some(id)),
            Some(EffectAction::Save(root)) if !playing => {
                self.save_effect(root);
            }
            Some(EffectAction::Save(_)) => self.output.error("Press Stop to save an effect."),
            None => {}
        }
    }

    /// Saves the effect at `root` as an effect asset (a prefab in
    /// `assets/effects`) and makes it a copy of it. Returns its path.
    pub(super) fn save_effect(&mut self, root: InstanceId) -> Option<String> {
        if !tree::can_be_prefab(&self.world, root) {
            self.output.error(
                "Only an effect in the Workspace can be saved: a part, model or free \
                 ParticleEmitter.",
            );
            return None;
        }
        self.save_prefab_in(&[root], EFFECTS_FOLDER, "effect")
    }
}

#[cfg(test)]
mod tests {
    use axcenity_core::{ClassName, Value};

    use super::*;
    use crate::effect_editor::effect_root;

    #[test]
    fn saving_an_effect_makes_an_effect_asset_and_the_stack_edits_with_undo() {
        let folder = super::super::tests::copy_of_example("hello-part", "save-effect");
        let mut app = EditorApp::new(None);
        app.open(&folder);
        let workspace = app.world.workspace();
        let part = app.world.create(ClassName::Part).unwrap();
        app.world.set_name(part, "Fountain").unwrap();
        app.world.set_parent(part, Some(workspace)).unwrap();
        let emitter = app.world.create(ClassName::ParticleEmitter).unwrap();
        app.world.set_parent(emitter, Some(part)).unwrap();
        app.history.seal();
        assert_eq!(effect_root(&app.world, Some(emitter)), Some(part));

        // The stack's edit is an undo step like Properties'.
        app.apply(emitter, Edit::Property("Rate", Value::Number(60.0)));
        app.history.seal();
        let rate = |app: &EditorApp| app.world.get(emitter).unwrap().property("Rate").cloned();
        assert_eq!(rate(&app), Some(Value::Number(60.0)));
        app.undo();
        assert_eq!(rate(&app), Some(Value::Number(20.0)));

        let path = app.save_effect(part).expect("saved");
        assert_eq!(path, "assets/effects/Fountain.prefab.json");
        assert!(crate::asset_browser::is_effect(&path));
        let text = std::fs::read_to_string(folder.join(&path)).unwrap();
        assert!(text.contains("ParticleEmitter"), "{text}");
        // What was saved is now a copy of it, so Apply and Revert work.
        let link = app.world.get(part).and_then(|p| p.prefab()).cloned();
        assert_eq!(link.map(|link| link.path), Some(path.clone()));
        // A second save doesn't overwrite it.
        let again = app.save_effect(part).expect("saved again");
        assert_eq!(again, "assets/effects/Fountain2.prefab.json");
        let _ = std::fs::remove_dir_all(&folder);
    }

    /// The tab draws the selected effect's stack in a narrow column, with
    /// no GPU.
    #[test]
    fn the_tab_draws_the_selected_effects_stack() {
        let mut app = EditorApp::new(None);
        let workspace = app.world.workspace();
        let part = app.world.create(ClassName::Part).unwrap();
        app.world.set_parent(part, Some(workspace)).unwrap();
        let emitter = app.world.create(ClassName::ParticleEmitter).unwrap();
        app.world.set_name(emitter, "Sparks").unwrap();
        app.world.set_parent(emitter, Some(part)).unwrap();
        app.select_from_viewport(Some(emitter));
        let ctx = crate::theme::test_context();
        let input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(260.0, 3000.0),
            )),
            ..Default::default()
        };
        let output = crate::theme::run(&ctx, input(), |ui| app.effect_pane(ui, None));
        let shapes = format!("{:?}", output.shapes);
        for text in ["Sparks", "Spawn", "Birth", "Over life", "Render", "Rate"] {
            assert!(shapes.contains(text), "`{text}` is drawn");
        }
        assert_eq!(app.effect_editor.root(), Some(part));
    }
}
