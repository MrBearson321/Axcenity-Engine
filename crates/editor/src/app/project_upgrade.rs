//! Opening a project made with another Axcenity (release system Phase 7,
//! `docs/project/layout.md`): asks before upgrading an older one,
//! refuses one this version can't open and says what to use instead, and
//! checks an upgraded project's assets. Command-line tools upgrade without
//! asking (`axcenity_project::load`).

use axcenity_project::{Compatibility, UpgradeStep};

use super::*;

/// "Upgrade this project?", waiting for an answer.
pub(super) struct UpgradePrompt {
    folder: PathBuf,
    level: Option<String>,
    name: String,
    from: u32,
    steps: Vec<UpgradeStep>,
}

impl EditorApp {
    /// Whether `folder` can start loading now. An older project waits for
    /// "Upgrade"; one this version can't open is refused with the reason.
    pub(super) fn may_open(&mut self, folder: &Path, level: Option<&str>) -> bool {
        match axcenity_project::compatibility(folder) {
            // A damaged manifest opens as before, with its problem listed.
            Compatibility::Current | Compatibility::Damaged(_) => true,
            Compatibility::Upgrade { from, steps } => {
                self.upgrade_prompt = Some(UpgradePrompt {
                    folder: folder.to_owned(),
                    level: level.map(str::to_owned),
                    name: shown_name(folder),
                    from,
                    steps,
                });
                false
            }
            Compatibility::TooOld { found } => {
                self.refuse_open(folder, &too_old(&shown_name(folder), found));
                false
            }
            Compatibility::TooNew { found, saved_with } => {
                let needs = crate::version_explorer::built_in_history()
                    .minimum_engine_for("project", found)
                    .map(ToString::to_string);
                let text = too_new(
                    &shown_name(folder),
                    found,
                    saved_with.as_deref(),
                    needs.as_deref(),
                );
                self.refuse_open(folder, &text);
                false
            }
        }
    }

    fn refuse_open(&mut self, folder: &Path, text: &str) {
        self.open_failed(folder, text);
        self.toasts.error("This project can't be opened here");
        self.follow_pending_handoff(false);
        if self.project.is_none() {
            self.start.set_notice(text.to_owned());
        }
    }

    /// The "Upgrade this project?" box, while one is waiting.
    pub(super) fn upgrade_prompt(&mut self, ctx: &egui::Context) {
        let Some(prompt) = &self.upgrade_prompt else {
            return;
        };
        let message = upgrade_message(&prompt.name, prompt.from);
        let items = prompt
            .steps
            .iter()
            .map(|step| (icons::ARROW_FAT_LINES_UP, step.summary.to_owned()))
            .collect();
        let outcome = dialog::ask(
            ctx,
            &dialog::Question {
                id: "upgrade project",
                icon: icons::ARROW_FAT_LINES_UP,
                tone: dialog::Tone::Warning,
                title: "Upgrade this project?",
                message: &message,
                items,
            },
            &[
                dialog::Button::new(icons::X, "Cancel", dialog::Kind::Plain),
                dialog::Button::new(icons::ARROW_FAT_LINES_UP, "Upgrade", dialog::Kind::Main),
            ],
        );
        match outcome {
            Some(dialog::Outcome::Picked(1)) => {
                let prompt = self.upgrade_prompt.take().expect("checked above");
                self.start_loading(ctx, &prompt.folder, prompt.level);
            }
            Some(_) => {
                self.upgrade_prompt = None;
                self.follow_pending_handoff(false);
            }
            None => {}
        }
    }

    /// After an upgrade: says where the files from before are, and runs the
    /// Asset Check once so anything the upgrade broke shows now.
    pub(super) fn report_upgrade(&mut self, project: &axcenity_project::Project, from: u32) {
        let kept = project
            .migration_backup
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| axcenity_project::PREVIOUS_FORMAT_DIR.to_owned());
        self.output.info(format!(
            "Upgraded \"{}\" from project format {from} to {}. The previous files are kept in \
             `{kept}`.",
            project.manifest.name,
            axcenity_project::FORMAT_VERSION,
        ));
        let report = axcenity_project::check_assets(&project.folder, &project.world);
        if !report.is_ok() {
            let count = report.findings.len();
            self.output.warning(format!(
                "After the upgrade, the Asset Check found {count} {} (View > Panels > Asset \
                 Check). The files from before the upgrade are in `{kept}`.",
                if count == 1 { "problem" } else { "problems" },
            ));
            self.toasts
                .warning("The upgraded project has asset problems");
        }
    }
}

/// The project's name from its manifest, or its folder's.
fn shown_name(folder: &Path) -> String {
    axcenity_project::project_name(folder).unwrap_or_else(|| {
        folder.file_name().map_or_else(
            || folder.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        )
    })
}

fn upgrade_message(name: &str, from: u32) -> String {
    format!(
        "\"{name}\" was saved by an older Axcenity (project format {from}). To open it, this \
         version upgrades its files to format {}: an older Axcenity won't open it after that. \
         The files as they are now are kept first in {}/{from} inside the project.",
        axcenity_project::FORMAT_VERSION,
        axcenity_project::PREVIOUS_FORMAT_DIR,
    )
}

fn too_old(name: &str, found: u32) -> String {
    format!(
        "\"{name}\" uses project format {found}, which is older than this version of Axcenity \
         can upgrade. Open it with an older Axcenity and save it there first."
    )
}

fn too_new(name: &str, found: u32, saved_with: Option<&str>, needs: Option<&str>) -> String {
    let by = saved_with.map_or_else(String::new, |v| format!(" {v}"));
    let update = match needs {
        Some(version) => format!("Update to Axcenity {version} or later to open it."),
        None => "Update Axcenity to open it.".to_owned(),
    };
    format!(
        "\"{name}\" was saved by a newer Axcenity{by} (project format {found}); this is \
         Axcenity {}, which opens format {} and older. {update}",
        axcenity_release::Version::current(),
        axcenity_project::FORMAT_VERSION,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str, manifest: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("axcenity-upgrade-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join(axcenity_project::MANIFEST_FILE), manifest).unwrap();
        std::fs::write(
            folder.join(axcenity_project::WORLD_FILE),
            "{\"format_version\": 1}\n",
        )
        .unwrap();
        folder
    }

    #[test]
    fn an_older_project_waits_for_upgrade_and_cancel_leaves_it_alone() {
        let folder = temp("older", r#"{"name": "Old Farm", "format_version": 3}"#);
        let ctx = theme::test_context();
        let mut app = EditorApp::new(None);
        app.begin_open(&ctx, &folder);
        assert!(app.loading.is_none(), "nothing loads before Upgrade");
        let prompt = app.upgrade_prompt.as_ref().unwrap();
        assert_eq!(prompt.name, "Old Farm");
        assert_eq!(prompt.from, 3);
        assert_eq!(
            prompt.steps.len() as u32,
            axcenity_project::FORMAT_VERSION - 3
        );
        assert!(!folder.join(".axcenity").exists());

        // The box draws, and nothing is written while it's up.
        let mut draw = |ui: &mut egui::Ui| app.upgrade_prompt(ui.ctx());
        match std::env::var_os("AXCENITY_SNAPSHOTS") {
            Some(out) => crate::snapshot::snapshot(
                &PathBuf::from(out).join("upgrade-project.png"),
                egui::vec2(640.0, 480.0),
                vec![Vec::new(); 3],
                &mut draw,
            ),
            None => {
                for _ in 0..3 {
                    let _ = theme::run(&ctx, Default::default(), &mut draw);
                }
            }
        }
        assert!(app.upgrade_prompt.is_some() && app.loading.is_none());
        app.upgrade_prompt = None;
        assert!(!folder.join(".axcenity").exists());
        let manifest =
            std::fs::read_to_string(folder.join(axcenity_project::MANIFEST_FILE)).unwrap();
        assert!(manifest.contains("\"format_version\": 3"));
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_newer_or_ancient_project_is_refused_with_what_to_do() {
        let ctx = theme::test_context();
        let newer = temp(
            "newer",
            r#"{"name": "Later", "format_version": 99, "engine": {"saved_with": "9.0.0"}}"#,
        );
        let mut app = EditorApp::new(None);
        app.begin_open(&ctx, &newer);
        assert!(app.loading.is_none() && app.upgrade_prompt.is_none());
        let text = &app.output.lines().last().unwrap().text;
        assert!(text.contains("newer Axcenity 9.0.0"), "{text}");
        assert!(text.contains("Update"), "{text}");

        let ancient = temp("ancient", r#"{"name": "Ancient", "format_version": 0}"#);
        app.begin_open(&ctx, &ancient);
        let text = &app.output.lines().last().unwrap().text;
        assert!(text.contains("older Axcenity"), "{text}");
        for folder in [newer, ancient] {
            assert!(!folder.join(".axcenity").exists());
            std::fs::remove_dir_all(&folder).unwrap();
        }
    }
}
