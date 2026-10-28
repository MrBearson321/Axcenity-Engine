//! The Mixer panel's place in the app (Phase 52; the panel is `mixer.rs`):
//! it shows the SoundGroups of the world on screen, the Play world while
//! the game runs.

use eframe::egui;

use super::{EditorApp, shown};

impl EditorApp {
    pub(in crate::app) fn mixer_pane(&mut self, ui: &mut egui::Ui) {
        let groups = axcenity_audio::mix::groups(shown(&self.world, &self.play));
        let playing = self.play.is_some();
        self.mixer.show(ui, &groups, playing);
    }
}
