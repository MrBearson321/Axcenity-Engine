//! Rebuilding the engine in the background and relaunching into it with the
//! session (unified Phase 25). Only a debug editor run from the source tree
//! has any of this: `Rebuilds::source` is `None` otherwise, so Settings has
//! no Engine page, nothing is watched and nothing is built. The build itself
//! is `rebuild.rs`; the handoff to the new editor is `relaunch.rs`. These
//! are methods of [`super::EditorApp`].

use super::*;

use crate::hot_patch::{HotPatch, Verdict};
use crate::rebuild::BuildMessage;
use crate::relaunch::{self, Handoff, TabHandoff, View};

/// The keys Settings > Engine is saved under.
pub(super) const REBUILD_KEY: &str = "engine rebuild on change";
pub(super) const RELAUNCH_KEY: &str = "engine relaunch automatically";
pub(super) const BUILD_UNITS_KEY: &str = "engine build units";

impl EditorApp {
    /// Each frame: notices changed engine sources, starts and follows the
    /// background build, and relaunches when asked.
    pub(super) fn engine_frame(&mut self, ctx: &egui::Context) {
        let Some(source) = self.rebuilds.source.clone() else {
            return;
        };
        if !self.rebuilds.hot_patch_checked {
            self.rebuilds.hot_patch_checked = true;
            self.rebuilds.hot_patch = HotPatch::connect(&source, ctx);
            if self.rebuilds.hot_patch.is_some() {
                self.output.info(
                    "Hot-patching: dx is serving this editor, so a change to function bodies \
                     in the engine runs from the next frame. Other engine changes rebuild and \
                     relaunch.",
                );
            }
        }
        let rebuilds = &mut self.rebuilds;
        // Hot-patching always watches: it decides between a patch and a rebuild.
        let watch = rebuilds.on_change || rebuilds.hot_patch.is_some();
        if watch && rebuilds.watcher.is_none() {
            let watcher = source.watch();
            let ctx = ctx.clone();
            watcher.on_change(move || ctx.request_repaint());
            rebuilds.watcher = Some(watcher);
        } else if !watch {
            rebuilds.watcher = None;
        }
        let changed = rebuilds
            .watcher
            .as_ref()
            .map(axcenity_project::Watcher::changes)
            .unwrap_or_default();
        if !changed.is_empty() {
            if self.rebuilds.hot_patch.is_some() {
                self.hot_patch_or_rebuild(ctx, &changed);
            } else {
                self.rebuild_engine(ctx, &changed);
            }
        }
        self.follow_hot_patch(ctx);
        self.follow_engine_build(ctx);
    }

    /// Engine code changed: rebuilds, or builds again after the build running
    /// now. Output names the `changed` files, unless the caller already said why.
    fn rebuild_engine(&mut self, ctx: &egui::Context, changed: &[String]) {
        if self.rebuilds.build.is_some() {
            self.rebuilds.again = true;
            return;
        }
        if !changed.is_empty() {
            self.output.info(format!(
                "Engine code changed ({}). Rebuilding the editor in the background.",
                quoted_list(&changed.iter().map(String::as_str).collect::<Vec<_>>())
            ));
        }
        self.start_engine_build(ctx);
    }

    /// Engine code changed while dx serves the editor: a change to function
    /// bodies waits for dx's patch, and anything else rebuilds and relaunches.
    fn hot_patch_or_rebuild(&mut self, ctx: &egui::Context, changed: &[String]) {
        let Some(hot) = &mut self.rebuilds.hot_patch else {
            return;
        };
        match hot.verdict() {
            Verdict::Unchanged => {}
            Verdict::Unpatchable(why) => {
                hot.waiting = None;
                if hot.held {
                    self.rebuild_engine(ctx, changed);
                } else {
                    hot.held = true;
                    self.output.warning(format!(
                        "Can't hot-patch this change: {why}. The editor keeps running the code \
                         it has, and rebuilds and relaunches instead."
                    ));
                    self.rebuild_engine(ctx, &[]);
                }
            }
            Verdict::Patchable(_) if hot.held => self.rebuild_engine(ctx, changed),
            Verdict::Patchable(_) => {
                // dx starts its patch again on each change, so the wait does too.
                hot.waiting = Some(Instant::now());
                self.output.info(format!(
                    "Engine code changed ({}): only function bodies, so dx patches them into \
                     the running editor.",
                    quoted_list(&changed.iter().map(String::as_str).collect::<Vec<_>>())
                ));
            }
        }
    }

    /// Acts on a patch dx applied: the next frame runs it if the sources it
    /// came from could be patched. A change that waited too long for its
    /// patch rebuilds and relaunches instead.
    fn follow_hot_patch(&mut self, ctx: &egui::Context) {
        let playing = self.playing();
        let Some(hot) = &mut self.rebuilds.hot_patch else {
            return;
        };
        if hot.take_patch() {
            hot.waiting = None;
            match hot.verdict() {
                Verdict::Unpatchable(why) => {
                    if !hot.held {
                        hot.held = true;
                        self.output.warning(format!(
                            "dx sent a patch the running editor can't take: {why}. It keeps \
                             running the code it has, and rebuilds and relaunches instead."
                        ));
                        self.rebuild_engine(ctx, &[]);
                    }
                }
                verdict => {
                    hot.held = false;
                    hot.accept();
                    let files = match verdict {
                        Verdict::Patchable(files) => format!(
                            " ({})",
                            quoted_list(&files.iter().map(String::as_str).collect::<Vec<_>>())
                        ),
                        _ => String::new(),
                    };
                    let play = if playing { ", and Play carries on" } else { "" };
                    self.output.info(format!(
                        "Hot-patched the running editor{files}: the next frame runs the new \
                         code{play}."
                    ));
                }
            }
        } else if hot.waiting.is_some_and(|since| since.elapsed() >= hot.wait) {
            hot.waiting = None;
            let seconds = hot.wait.as_secs();
            self.output.warning(format!(
                "dx sent no patch within {seconds} s, so the editor rebuilds and relaunches \
                 instead. dx's terminal says why (a patch that didn't build, or code it doesn't \
                 patch)."
            ));
            self.rebuild_engine(ctx, &[]);
        }
    }

    /// Starts `cargo build` for the editor, in its own target folder.
    pub(super) fn start_engine_build(&mut self, ctx: &egui::Context) {
        let Some(source) = &self.rebuilds.source else {
            return;
        };
        let wake = {
            let ctx = ctx.clone();
            move || ctx.request_repaint()
        };
        match source.build(wake) {
            Ok(build) => {
                let rebuilds = &mut self.rebuilds;
                rebuilds.build = Some(build);
                rebuilds.again = false;
                rebuilds.compiled = 0;
                rebuilds.errors = 0;
                rebuilds.ready = false;
                self.toasts.offer = None;
            }
            Err(err) => self.output.error(format!(
                "Could not start `cargo build` for the engine: {err}"
            )),
        }
    }

    /// Reads what the running build said: progress, errors, and how it ended.
    pub(super) fn follow_engine_build(&mut self, ctx: &egui::Context) {
        let Some(build) = &mut self.rebuilds.build else {
            return;
        };
        for message in build.poll() {
            match message {
                BuildMessage::Compiled => self.rebuilds.compiled += 1,
                BuildMessage::Error(text) => {
                    self.rebuilds.errors += 1;
                    self.output.error(text);
                }
                BuildMessage::Finished { success, why } => {
                    for line in why {
                        self.output.error(line);
                    }
                    self.engine_build_finished(ctx, success);
                }
            }
        }
        if self
            .rebuilds
            .build
            .as_ref()
            .is_some_and(crate::rebuild::Build::finished)
        {
            self.rebuilds.build = None;
        }
    }

    fn engine_build_finished(&mut self, ctx: &egui::Context, success: bool) {
        let rebuilds = &mut self.rebuilds;
        if !success {
            let errors = rebuilds.errors;
            self.output.error(format!(
                "The engine didn't build{}. The editor carries on as it was; click an \
                 error to open its file.",
                match errors {
                    0 => String::new(),
                    1 => ": 1 error".to_owned(),
                    n => format!(": {n} errors"),
                }
            ));
            self.toasts
                .error("The engine didn't build. The errors are in Output.");
            if rebuilds.again {
                self.start_engine_build(ctx);
            }
            return;
        }
        rebuilds.expected = rebuilds.compiled;
        if rebuilds.again {
            // The sources changed while it built: build the newest instead.
            self.start_engine_build(ctx);
            return;
        }
        rebuilds.ready = true;
        if rebuilds.relaunch_automatically {
            self.output
                .info("Rebuilt the engine. Relaunching with this session.");
            self.relaunch();
        } else {
            self.output.info(
                "Rebuilt the engine. Relaunch (in the bottom-right corner) to use it; the \
                 project, its unsaved edits and Play carry over.",
            );
            self.toasts.offer = Some(toast::Offer {
                text: "The engine was rebuilt.".to_owned(),
                button: "Relaunch".to_owned(),
            });
        }
    }

    /// The bottom-right note while the engine builds.
    pub(super) fn engine_busy(&self) -> Option<toast::Busy> {
        let fraction = self.rebuilds.progress()?;
        let errors = match self.rebuilds.errors {
            0 => String::new(),
            1 => ", 1 error".to_owned(),
            n => format!(", {n} errors"),
        };
        Some(toast::Busy {
            text: format!(
                "Rebuilding the engine: {} crates{errors}",
                self.rebuilds.compiled
            ),
            fraction,
        })
    }

    /// Acts on the Relaunch offer's buttons.
    pub(super) fn follow_offer(&mut self, click: toast::OfferClick) {
        if click.accepted {
            self.relaunch();
        } else if click.dismissed {
            self.toasts.offer = None;
        }
    }

    /// Writes the handoff for the rebuilt editor; `update` then starts it
    /// and closes this one. Refused (and said why) when there's no rebuilt
    /// editor, or when there are unsaved edits it can't carry.
    pub(super) fn relaunch(&mut self) {
        if !self.rebuilds.ready || self.launching.is_some() {
            return;
        }
        if self.ui_graphs_unsaved()
            || self.behavior.unsaved()
            || self.data_tables.unsaved()
            || self.materials.unsaved()
        {
            self.output.warning(
                "Relaunch carries the world and script tabs, not Event Graphs, behavior \
                 trees, data tables or materials. Save those first, then Relaunch.",
            );
            self.toasts
                .error("Save the Event Graph or behavior tree first.");
            return;
        }
        let written = relaunch::handoff_root()
            .ok_or_else(|| std::io::Error::other("this system has no folder for app data"))
            .and_then(|root| relaunch::new_handoff_folder(&root))
            .and_then(|folder| {
                self.handoff(&folder)?.write(&folder)?;
                Ok(folder)
            });
        match written {
            Ok(folder) => {
                self.toasts.offer = None;
                self.launching = Some(folder);
            }
            Err(err) => self.output.error(format!(
                "Could not relaunch: writing the handoff failed: {err}"
            )),
        }
    }

    /// Starts the rebuilt editor with the handoff `relaunch` wrote, and
    /// closes this one. The settings are saved first, so the new editor
    /// reads the same ones.
    pub(super) fn launch(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let Some(folder) = self.launching.take() else {
            return;
        };
        let Some(source) = self.rebuilds.source.clone() else {
            return;
        };
        if let Some(storage) = frame.storage_mut() {
            eframe::App::save(self, storage);
            storage.flush();
        }
        match relaunch::launch(&source, &folder) {
            Ok(()) => {
                self.closing = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Err(err) => {
                let _ = std::fs::remove_dir_all(&folder);
                self.output
                    .error(format!("Could not start the rebuilt editor: {err}"));
            }
        }
    }

    /// What the new editor needs to carry on, with the worlds' landscapes
    /// written into `folder`.
    pub(super) fn handoff(&self, folder: &Path) -> std::io::Result<Handoff> {
        let world = if self.history.is_saved() {
            None
        } else {
            Some(relaunch::write_world(&self.world, folder, "edit")?)
        };
        let tabs = self
            .tabs
            .ids()
            .into_iter()
            .filter_map(|id| self.tabs.path_of(id))
            .filter_map(|path| self.tabs.get(path))
            .map(|tab| TabHandoff {
                path: tab.path.clone(),
                unsaved: tab.unsaved().then(|| tab.text.clone()),
            })
            .collect();
        let block_scripts = self.project.as_ref().map_or_else(Vec::new, |project| {
            project
                .block_scripts
                .iter()
                .filter(|document| document.script != document.saved)
                .map(|document| (document.path.clone(), document.script.clone()))
                .collect()
        });
        let shown = shown(&self.world, &self.play);
        let selection = self
            .selection
            .iter()
            .filter_map(|&id| relaunch::locate(shown, id))
            .collect();
        let (position, orientation) = self.viewport.camera_view();
        let play = match &self.play {
            Some(session) => Some(relaunch::write_world(session.world(), folder, "play")?),
            None => None,
        };
        Ok(Handoff {
            version: relaunch::HANDOFF_VERSION,
            project: self.project.as_ref().map(|project| project.folder.clone()),
            world,
            tabs,
            block_scripts,
            layout: self.layout.to_text(),
            camera: View::new(position, orientation),
            selection,
            play,
        })
    }

    /// Carries on from the handoff in `folder`, written by the editor this
    /// one was relaunched from. The project opens in the background and the
    /// handoff applies once it has; the folder is deleted then.
    pub fn resume(&mut self, ctx: &egui::Context, folder: &Path) {
        match Handoff::read(folder) {
            Ok(handoff) => match handoff.project.clone() {
                Some(project) => {
                    self.begin_open(ctx, &project);
                    self.pending_handoff = Some((folder.to_owned(), handoff));
                }
                None => self.apply_handoff(handoff, folder),
            },
            Err(why) => {
                let _ = std::fs::remove_dir_all(folder);
                self.output
                    .error(format!("Could not carry on from the relaunch: {why}"));
            }
        }
    }

    /// After a project opened: applies a handoff waiting for it, if any.
    pub(super) fn follow_pending_handoff(&mut self, opened: bool) {
        let Some((folder, handoff)) = self.pending_handoff.take() else {
            return;
        };
        if opened {
            self.apply_handoff(handoff, &folder);
        } else {
            let _ = std::fs::remove_dir_all(&folder);
            self.output.error(
                "Relaunched, but the project couldn't open, so its unsaved edits weren't \
                 carried over.",
            );
        }
    }

    /// Puts the session in `handoff` back, over the project just opened from
    /// its files: the unsaved world, script tabs and block scripts, the
    /// layout, the camera and the selection, then Play from the saved game
    /// world. Deletes `folder` after.
    pub(super) fn apply_handoff(&mut self, handoff: Handoff, folder: &Path) {
        let mut problems = Vec::new();
        self.layout = Layout::from_text(&handoff.layout);
        if let Some(world) = &handoff.world {
            problems.extend(relaunch::read_world(&mut self.world, world, folder));
            self.explorer = Explorer::new(&self.world);
            // The edits came from the other editor: unsaved until saved here.
            self.history.mark_unsaved();
        }
        for (path, script) in handoff.block_scripts {
            if let Some(document) = self.project.as_mut().and_then(|project| {
                project
                    .block_scripts
                    .iter_mut()
                    .find(|document| document.path == path)
            }) {
                document.script = script;
                self.update_block_lua(&path);
            }
        }
        for tab in &handoff.tabs {
            let Some(project_folder) = self.project.as_ref().map(|p| p.folder.clone()) else {
                break;
            };
            if axcenity_project::is_stylesheet(&tab.path) {
                let disk = std::fs::read_to_string(project_folder.join(&tab.path));
                self.tabs.open(&tab.path, &disk.unwrap_or_default(), None);
            } else {
                self.open_script(&tab.path, None);
            }
            if let (Some(text), Some(open)) = (&tab.unsaved, self.tabs.get_mut(&tab.path)) {
                open.text.clone_from(text);
            }
        }
        // The tabs reopen behind the Scene View, which stays in front.
        // (The dock makes a tab it gains active, so it gains them now.)
        self.tabs.show_world();
        let _ = self.tabs.take_focus();
        self.layout.sync_scripts(&self.tabs.ids(), None, None);
        self.layout.open(Pane::Scene);
        self.viewport
            .set_camera_view(handoff.camera.position(), handoff.camera.orientation());
        self.output.info(
            "Relaunched with the rebuilt engine. The project and its unsaved edits carried \
             over; the undo history starts fresh.",
        );
        if let Some(play) = &handoff.play {
            let mut game = self.world.clone();
            problems.extend(relaunch::read_world(&mut game, play, folder));
            self.start_session(Some(game));
            if self.playing() {
                // Starting Play put the camera where the game wants it.
                self.viewport
                    .set_camera_view(handoff.camera.position(), handoff.camera.orientation());
            }
        }
        let shown = shown(&self.world, &self.play);
        let selection: Vec<InstanceId> = handoff
            .selection
            .iter()
            .filter_map(|locator| relaunch::find(shown, locator))
            .collect();
        self.selected = selection.last().copied();
        self.selection = selection;
        for problem in problems {
            self.output.error(format!(
                "Relaunch couldn't carry everything over: {problem}"
            ));
        }
        let _ = std::fs::remove_dir_all(folder);
    }

    /// Opens an engine source file a compile error names, in the code editor.
    pub(super) fn open_engine_source(&mut self, path: &str, line: usize) {
        let Some(source) = &self.rebuilds.source else {
            return;
        };
        if let Err(err) = crate::rebuild::open_source(&source.workspace, path, line) {
            self.output.error(format!("Could not open `{path}`: {err}"));
        }
    }

    /// Settings > Engine: rebuilding when the engine's code changes.
    pub(super) fn engine_page(&mut self, ui: &mut egui::Ui) {
        settings::page_title(
            ui,
            "Engine",
            "Rebuild the editor from its source while you work on it.",
        );
        let palette = theme::palette(ui.ctx());
        let Some(source) = self.rebuilds.source.clone() else {
            return;
        };
        widgets::section_label(ui, "Rebuild");
        ui.checkbox(
            &mut self.rebuilds.on_change,
            theme::with_icon(icons::HAMMER, "Rebuild when engine code changes"),
        )
        .on_hover_text(
            "Watches crates/ (.rs, Cargo.toml and shaders) and runs cargo build for the \
             editor in the background, into its own target folder.",
        );
        ui.checkbox(
            &mut self.rebuilds.relaunch_automatically,
            theme::with_icon(
                icons::ARROWS_CLOCKWISE,
                "Relaunch automatically after a build",
            ),
        )
        .on_hover_text(
            "Off, a note in the bottom-right corner offers Relaunch. The project, its unsaved \
             edits and Play carry over.",
        );
        ui.add_space(6.0);
        let building = self.rebuilds.build.is_some();
        if ui
            .add_enabled(
                !building,
                widgets::button(
                    widgets::Role::Secondary,
                    Some(icons::HAMMER),
                    "Rebuild Now",
                    &palette,
                ),
            )
            .clicked()
        {
            self.output.info("Rebuilding the editor in the background.");
            let ctx = ui.ctx().clone();
            self.start_engine_build(&ctx);
        }
        ui.add_space(16.0);
        widgets::section_label(ui, "Hot-patching");
        let hot_patching = match &self.rebuilds.hot_patch {
            Some(hot) if hot.held => {
                "On, waiting for a rebuild: the last change can't be patched, so the editor \
                 runs the code it has until it relaunches."
            }
            Some(_) => {
                "On: dx is serving this editor. A change to function bodies runs from the next \
                 frame, also during Play. Other engine changes rebuild and relaunch, whatever \
                 the setting above."
            }
            None if cfg!(feature = "hot-patch") => {
                "Off: start the editor with `dx serve --hot-patch` (see the README) to patch \
                 function bodies in without relaunching."
            }
            None => {
                "Off: this editor was built without the `hot-patch` feature. The README says \
                 how to start it with dx."
            }
        };
        ui.add(
            egui::Label::new(egui::RichText::new(hot_patching).color(palette.text_secondary))
                .wrap(),
        );
        ui.add_space(16.0);
        widgets::section_label(ui, "Source");
        for (label, path) in [
            ("Workspace", source.workspace.clone()),
            ("Builds go to", source.target_dir()),
        ] {
            ui.label(
                egui::RichText::new(format!("{label}: {}", path.display()))
                    .color(palette.text_secondary),
            );
        }
    }
}
