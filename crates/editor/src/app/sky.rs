//! Choosing the world's sky image (Lighting's `SkyImage`): picked from the
//! computer in Properties, dragged onto the viewport or the Properties row,
//! double-clicked in the Assets panel, or dropped on the window as a file.
//! Every way asks "Use this sky?" first; yes is one undo step.

use super::*;

impl EditorApp {
    /// Asks whether to use the sky image at `path` (in the project) as the
    /// world's sky. Nothing changes until it's answered.
    pub(super) fn ask_sky(&mut self, path: String) {
        if self.project.is_some() {
            self.confirm_sky = Some(path);
        }
    }

    /// Asks for a sky image with the system's file picker, and returns its
    /// path in the project: one from outside is copied into `assets/sky`
    /// first. `None` if cancelled or it can't be used (said in Output).
    pub(super) fn pick_sky_image(&mut self) -> Option<String> {
        let project = self.project.as_ref()?;
        let skies = project.folder.join(axcenity_assets::PROJECT_SKY_FOLDER);
        let start = if skies.is_dir() {
            skies
        } else {
            project.folder.clone()
        };
        let file = rfd::FileDialog::new()
            .set_title("Choose a sky image")
            .add_filter("Sky image", &axcenity_assets::SKY_IMAGE_EXTENSIONS)
            .add_filter("HDR sky (EXR or HDR)", &axcenity_assets::HDR_SKY_EXTENSIONS)
            .set_directory(&start)
            .pick_file()?;
        match axcenity_assets::import_sky_image(&project.folder, &file) {
            Ok(path) => {
                self.asset_browser.refresh();
                Some(path)
            }
            Err(err) => {
                let err = files::chain(&err);
                self.output
                    .error(format!("Could not use `{}`: {err}", file.display()));
                None
            }
        }
    }

    /// Shows "Use this sky?" while a sky image is waiting for an answer.
    pub(super) fn sky_prompt(&mut self, ctx: &egui::Context) {
        let Some(path) = self.confirm_sky.clone() else {
            return;
        };
        let name = crate::asset_browser::display_name(&path);
        let title = format!("Use {name} as the sky?");
        let mut message = "It replaces the sky colors: it's drawn behind the world, and parts \
                           reflect it and are lit by it. Undo puts the old sky back."
            .to_owned();
        if self.graphics.reflection_quality() == axcenity_render::ReflectionQuality::Low {
            message.push_str(
                " Your Reflection quality is Low, so the editor keeps showing the sky colors: \
                 raise it in Settings > Graphics to see the image.",
            );
        }
        let outcome = dialog::ask(
            ctx,
            &dialog::Question {
                id: "use sky image",
                icon: icons::SUN_HORIZON,
                tone: dialog::Tone::Question,
                title: &title,
                message: &message,
                items: vec![(icons::IMAGE, path.clone())],
            },
            &[
                dialog::Button {
                    icon: icons::X,
                    label: "Cancel",
                    kind: dialog::Kind::Plain,
                },
                dialog::Button {
                    icon: icons::CHECK,
                    label: "Use as Sky",
                    kind: dialog::Kind::Main,
                },
            ],
        );
        // Escape or a click outside the box means Cancel.
        if let Some(outcome) = outcome {
            self.answer_sky(outcome == dialog::Outcome::Picked(1));
        }
    }

    /// Answers "Use this sky?": yes sets Lighting's `SkyImage` as one undo
    /// step (during Play, the game's) and says so.
    pub(super) fn answer_sky(&mut self, yes: bool) {
        let Some(path) = self.confirm_sky.take() else {
            return;
        };
        if !yes {
            return;
        }
        let lighting = shown(&self.world, &self.play).lighting();
        self.apply(
            lighting,
            Edit::Property("SkyImage", Value::String(path.clone())),
        );
        self.history.seal();
        let name = crate::asset_browser::display_name(&path);
        self.toasts.success(format!("Sky set to {name}"));
        self.output
            .info(format!("Lighting.SkyImage is now `{path}`."));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset_browser::{AssetEntry, AssetKind};

    fn sky_image(app: &EditorApp) -> String {
        app.world
            .get(app.world.lighting())
            .and_then(|lighting| lighting.property("SkyImage"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    }

    fn sky_entry(path: &str) -> AssetEntry {
        AssetEntry {
            kind: AssetKind::Sky,
            path: path.to_owned(),
            name: crate::asset_browser::display_name(path),
        }
    }

    #[test]
    fn double_clicking_a_sky_asks_first_and_yes_is_one_undo_step() {
        let mut app = EditorApp::new(Some(&super::super::tests::example("hello-graphics")));
        app.drop_asset(&sky_entry("assets/sky/noon.hdr"), None);
        assert_eq!(app.confirm_sky.as_deref(), Some("assets/sky/noon.hdr"));
        assert_eq!(sky_image(&app), "", "nothing changes before the answer");

        app.answer_sky(true);
        assert_eq!(app.confirm_sky, None);
        assert_eq!(sky_image(&app), "assets/sky/noon.hdr");
        app.undo();
        assert_eq!(sky_image(&app), "", "undo puts the old sky back");
    }

    #[test]
    fn cancelling_leaves_the_sky_as_it_was() {
        let mut app = EditorApp::new(Some(&super::super::tests::example("hello-graphics")));
        app.apply(
            app.world.lighting(),
            Edit::AskSkyImage("assets/sky/dusk.hdr".into()),
        );
        assert_eq!(app.confirm_sky.as_deref(), Some("assets/sky/dusk.hdr"));
        app.answer_sky(false);
        assert_eq!(app.confirm_sky, None);
        assert_eq!(sky_image(&app), "");
    }

    #[test]
    fn the_question_box_draws_and_enter_uses_the_sky() {
        let mut app = EditorApp::new(Some(&super::super::tests::example("hello-graphics")));
        app.ask_sky("assets/sky/noon.hdr".into());
        if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
            // Drawn from a copy of the question, so the app's stays asked.
            let mut shown = EditorApp::new(None);
            crate::snapshot::snapshot(
                &std::path::PathBuf::from(folder).join("sky-prompt.png"),
                egui::vec2(700.0, 420.0),
                vec![Vec::new(); 3],
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |_| {});
                    shown.confirm_sky = Some("assets/sky/noon.hdr".into());
                    shown.sky_prompt(ctx);
                },
            );
        }
        let ctx = crate::theme::test_context();
        let enter = egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        };
        for events in [vec![], vec![enter]] {
            let input = egui::RawInput {
                events,
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| app.sky_prompt(ctx));
        }
        assert_eq!(app.confirm_sky, None);
        assert_eq!(sky_image(&app), "assets/sky/noon.hdr");
    }
}
