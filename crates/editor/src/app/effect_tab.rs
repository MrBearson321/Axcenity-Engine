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
            Some(EffectAction::AddPreset(target, preset)) if !playing => {
                self.add_preset(target, preset);
            }
            Some(EffectAction::AddPreset(..)) => {
                self.output.error("Press Stop to add a preset.");
            }
            None => {}
        }
    }

    /// Adds `preset`'s emitter (suited to the place's Lighting and wind)
    /// to `target`, as one undo step, and selects it (roadmap R17.5): into
    /// a part or Attachment; beside a free emitter, where it stands; or
    /// into a model or folder, at the middle of its effect. Returns it.
    pub(super) fn add_preset(
        &mut self,
        target: InstanceId,
        preset: axcenity_scene::EffectPreset,
    ) -> Option<InstanceId> {
        let instance = self.world.get(target)?;
        let class = instance.class();
        let (parent, position) = if class.is_base_part() || class == ClassName::Attachment {
            (target, None)
        } else if class == ClassName::ParticleEmitter {
            let at = instance.property("Position").and_then(Value::as_vector3);
            (instance.parent()?, at)
        } else {
            let middle = self.effect_editor.middle();
            (target, Some(Vector3::new(middle.x, middle.y, middle.z)))
        };
        let spec = axcenity_scene::preset_emitter(&self.world, preset);
        let id = axcenity_scene::create_preset(&mut self.world, &spec, position);
        let name = tree::unique_name(&self.world, parent, spec.name);
        self.world.set_name(id, &name).expect("just made");
        let index = self.world.get(parent).map_or(0, |p| p.children().len());
        if self.place(id, Some(Place { parent, index }), format!("Add {name}")) {
            self.select_from_viewport(Some(id));
            return Some(id);
        }
        None
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
        for text in [
            "Sparks",
            "Spawn",
            "Birth",
            "Over life",
            "Render",
            "Environment",
            "Rate",
            "GlobalWind",
        ] {
            assert!(shapes.contains(text), "`{text}` is drawn");
        }
        assert_eq!(app.effect_editor.root(), Some(part));
    }

    /// R17.5: a preset goes into the selected part as one undo step,
    /// suited to the place: at night its fire glows less bright.
    #[test]
    fn a_preset_goes_into_the_part_as_one_undo_step() {
        let mut app = EditorApp::new(None);
        let workspace = app.world.workspace();
        let part = app.world.create(ClassName::Part).unwrap();
        app.world.set_parent(part, Some(workspace)).unwrap();
        app.history.seal();

        let fire = app
            .add_preset(part, axcenity_scene::EffectPreset::Fire)
            .expect("added");
        app.history.seal();
        assert_eq!(app.world.get(fire).unwrap().parent(), Some(part));
        assert_eq!(app.selected, Some(fire));
        assert_eq!(effect_root(&app.world, Some(fire)), Some(part));
        let brightness = |app: &EditorApp, id| {
            app.world
                .get(id)
                .and_then(|i| i.property("Brightness"))
                .and_then(Value::as_number)
                .unwrap()
        };
        let by_day = brightness(&app, fire);

        // A second one gets its own name; an explosion keeps its smoke.
        let lighting = app.world.lighting();
        app.world
            .set_property(lighting, "ClockTime", Value::Number(0.0))
            .unwrap();
        app.history.seal();
        let again = app
            .add_preset(part, axcenity_scene::EffectPreset::Fire)
            .expect("added");
        app.history.seal();
        assert_ne!(
            app.world.get(again).unwrap().name(),
            app.world.get(fire).unwrap().name()
        );
        assert!(brightness(&app, again) < by_day, "dimmer at night");
        let boom = app
            .add_preset(part, axcenity_scene::EffectPreset::Explosion)
            .expect("added");
        app.history.seal();
        assert_eq!(app.world.get(boom).unwrap().children().len(), 1);

        // One undo takes the explosion and its smoke out together (kept
        // detached, so redo can put them back).
        app.undo();
        let parent = |app: &EditorApp, id| app.world.get(id).and_then(|i| i.parent());
        assert_eq!(parent(&app, boom), None);
        assert_eq!(parent(&app, again), Some(part));
        assert_eq!(app.world.get(boom).unwrap().children().len(), 1);
    }
}
