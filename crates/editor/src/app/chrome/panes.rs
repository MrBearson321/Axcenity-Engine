use super::*;

impl EditorApp {
    /// The Explorer panel, then whatever was clicked or done in it.
    pub(in crate::app) fn explorer_pane(&mut self, ui: &mut egui::Ui) {
        let editing = !self.playing();
        // The game's world during Play, otherwise the one being edited.
        let world = shown(&self.world, &self.play);
        let explorer = self.explorer.show(ui, world, &self.selection, editing);
        let rows: Vec<InstanceId> = if explorer.clicked.is_some() {
            self.explorer
                .rows(world)
                .into_iter()
                .map(|row| row.id)
                .collect()
        } else {
            Vec::new()
        };
        if explorer.clicked.is_some() || explorer.keep_group {
            self.select_clicked(
                explorer.clicked,
                explorer.modifiers,
                Some(&rows),
                explorer.keep_group,
            );
            ui.ctx().request_repaint();
        }
        if let Some(action) = explorer.action {
            self.act(action);
            // The Explorer was drawn before this change, so draw it again.
            ui.ctx().request_repaint();
        }
        if let Some(id) = explorer.open {
            self.open_script_instance(id);
        }
    }

    /// The Properties panel (with the Style section for UI elements), then
    /// the edit made in it.
    pub(in crate::app) fn properties_pane(&mut self, ui: &mut egui::Ui) {
        let editing = !self.playing();
        let world = shown(&self.world, &self.play);
        let stylesheets = self
            .project
            .as_ref()
            .map_or(&[][..], |project| project.stylesheets.as_slice());
        let selected = self.selected;
        let mut style_edit = None;
        let edit = properties::show(ui, world, selected, !editing, self.selection.len(), |ui| {
            // Stylesheets are edited, not the game, so not during Play.
            if editing {
                style_edit = style_panel::show(ui, world, selected, stylesheets);
            }
        });
        // Other panels may already be drawn this frame, so draw again to
        // show the change everywhere.
        if let (Some(id), Some(edit)) = (selected, edit) {
            self.apply(id, edit);
            ui.ctx().request_repaint();
        }
        if let (Some(id), Some(edit)) = (selected, style_edit) {
            self.apply_style(id, edit);
            ui.ctx().request_repaint();
        }
    }

    /// The Assets panel, then what was asked for in it.
    pub(in crate::app) fn assets_pane(&mut self, ui: &mut egui::Ui) {
        let can_save = assets::any_prefab(&self.world, &self.selection_roots());
        let picking_foliage =
            self.terrain.tool == Some(crate::terrain_studio::TerrainTool::Foliage);
        let action = {
            let foliage = picking_foliage.then_some(&self.terrain.foliage_source);
            self.asset_browser
                .show(ui, &self.assets, !self.playing(), can_save, foliage)
        };
        match action {
            Some(BrowserAction::Place(entry)) => self.drop_asset(&entry, None),
            Some(BrowserAction::Foliage(entry)) => {
                if let Some(source) = entry.foliage_source() {
                    self.terrain.foliage_source = source;
                }
            }
            Some(BrowserAction::Import { image_folder }) => self.import_assets(image_folder),
            Some(BrowserAction::SavePrefab) => self.save_prefab(),
            Some(BrowserAction::RenamePrefab { path, name }) => self.rename_prefab(&path, &name),
            Some(BrowserAction::DuplicatePrefab { path }) => self.duplicate_prefab(&path),
            Some(BrowserAction::DeletePrefab { path }) => self.delete_prefab(&path),
            Some(BrowserAction::Notice { error, text }) => {
                if error {
                    self.output.error(text);
                } else {
                    self.output.info(text);
                }
            }
            None => {}
        }
    }

    /// The Animation tab (see `animation.rs`), following the selection.
    pub(in crate::app) fn animation_pane(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&RenderState>,
    ) {
        let folder = self.project.as_ref().map(|project| project.folder.clone());
        let stylesheets = self
            .project
            .as_ref()
            .map_or(&[][..], |project| &project.stylesheets[..]);
        let actions = self.animation.show(
            ui,
            folder.as_deref(),
            &self.assets,
            render_state,
            self.selected,
            &mut self.world,
            stylesheets,
            &mut self.output,
        );
        self.studio_actions(actions);
    }

    /// Does what the Animation tab asked for.
    pub(super) fn studio_actions(&mut self, actions: Vec<StudioAction>) {
        for action in actions {
            match action {
                StudioAction::ImportModel => self.import_model_with_dialog(),
                StudioAction::Animate(object) => self.animate(object),
                StudioAction::Show => self.layout.open(Pane::Animation),
                StudioAction::SetController {
                    part,
                    animator,
                    path,
                } => self.set_controller(part, animator, path),
                StudioAction::RecordFile {
                    path,
                    before,
                    after,
                    label,
                } => {
                    self.history.record_files(
                        &mut self.world,
                        label,
                        vec![crate::history::TextChange {
                            path,
                            before: Some(before),
                            after: Some(after),
                        }],
                    );
                }
                StudioAction::RecordFiles {
                    label,
                    files,
                    rebind_controller,
                } => self.record_asset_files(label, files, rebind_controller),
            }
        }
    }

    /// Animates `object`, as one undo step: gives it an Animator if it has
    /// none, writes a new keyed clip to `assets/animations/<name>/<name>.anim.json`
    /// (a number added if taken), and puts an Animation named after the
    /// clip, playing it, in the Animator. Then the Animation tab opens the
    /// clip with Record on.
    pub(in crate::app) fn animate(&mut self, object: InstanceId) {
        if self.playing() {
            return;
        }
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        let Some(instance) = self.world.get(object) else {
            return;
        };
        if !crate::animation::can_animate(instance.class()) {
            return;
        }
        let object_name = instance.name().to_owned();
        let name = Some(axcenity_anim::files::file_safe(&object_name))
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "Animation".to_owned());
        let clip = crate::animation::new_keyed_clip();
        let written = match crate::animation::store::create_property_clip(&folder, &name, &clip) {
            Ok(written) => written,
            Err(err) => {
                self.output
                    .error(format!("Could not animate {object_name}: {err}"));
                return;
            }
        };
        let mut moves = Vec::new();
        let mut place = |world: &mut World, id: InstanceId, parent: InstanceId| {
            let wanted = world
                .get(id)
                .map(|i| i.name().to_owned())
                .unwrap_or_default();
            let unique = tree::unique_name(world, parent, &wanted);
            world.set_name(id, unique).expect("just made");
            let index = world.get(parent).map_or(0, |p| p.children().len());
            let place = Place { parent, index };
            history::put(world, id, Some(place)).map(|()| {
                moves.push(Move {
                    id,
                    before: None,
                    after: Some(place),
                });
            })
        };
        let animator = match crate::animation::animator_of(&self.world, object) {
            Some(animator) => animator,
            None => {
                let id = self
                    .world
                    .create(ClassName::Animator)
                    .expect("not a service");
                if let Err(err) = place(&mut self.world, id, object) {
                    self.output
                        .error(format!("Could not animate {object_name}: {err}"));
                    return;
                }
                id
            }
        };
        let animation = self
            .world
            .create(ClassName::Animation)
            .expect("not a service");
        let clip_name = axcenity_anim::files::clip_name(&written.path).to_owned();
        self.world
            .set_name(animation, clip_name)
            .expect("just made");
        self.world
            .set_property(animation, "Clip", Value::String(written.path.clone()))
            .expect("every Animation has a Clip");
        if let Err(err) = place(&mut self.world, animation, animator) {
            self.output
                .error(format!("Could not animate {object_name}: {err}"));
            return;
        }
        let texts = written
            .files
            .into_iter()
            .map(|file| history::TextChange {
                path: file.path,
                before: file.before,
                after: file.after,
            })
            .collect();
        self.history.record_files_and_tree(
            &mut self.world,
            format!("Animate {object_name}"),
            texts,
            moves,
        );
        self.asset_browser.refresh();
        self.output.info(format!(
            "Animating {object_name}: made the clip `{}`. Move it in the Animation tab with Record on; tick AutoPlay on its Animation to play it when Play starts.",
            written.path
        ));
        self.animation.animated(
            &folder,
            &self.assets,
            object,
            &written.path,
            &mut self.world,
            &mut self.output,
        );
        self.layout.open(Pane::Animation);
    }

    /// The Asset Check panel: scan results, and a button to run the scan.
    pub(in crate::app) fn asset_check_pane(&mut self, ui: &mut egui::Ui) {
        match self.asset_check.show(ui, self.project.is_some()) {
            crate::asset_check::CheckAction::Run => self.run_asset_check(),
            crate::asset_check::CheckAction::Select(owner) => self.select_finding_owner(&owner),
            crate::asset_check::CheckAction::None => {}
        }
    }

    /// File > Check Assets, the Test ribbon, and the panel's button.
    pub(in crate::app) fn run_asset_check(&mut self) {
        let Some(project) = &self.project else {
            self.output.error("Open a project first.");
            return;
        };
        let report = axcenity_project::check_assets(&project.folder, &self.world);
        let count = report.findings.len();
        if report.is_ok() {
            self.output.info(
                "No missing files, invalid references, import failures, or broken dependencies.",
            );
            self.toasts.success("No asset problems");
        } else {
            let noun = if count == 1 { "problem" } else { "problems" };
            self.output.info(format!(
                "Asset check found {count} {noun}. See the Asset Check panel."
            ));
            self.toasts.error(format!("{count} {noun}"));
        }
        self.asset_check.set(report);
        self.layout.open(Pane::AssetCheck);
    }

    pub(super) fn select_finding_owner(&mut self, owner: &str) {
        let world = shown(&self.world, &self.play);
        let found = world
            .descendants(world.root())
            .into_iter()
            .find(|&id| world.full_name(id).as_deref() == Some(owner));
        if let Some(id) = found {
            self.select_from_viewport(Some(id));
        }
    }

    /// Gives `part`'s Animator (making one if `animator` is `None`) the
    /// controller at `path`, as one undo step.
    pub(in crate::app) fn set_controller(
        &mut self,
        part: InstanceId,
        animator: Option<InstanceId>,
        path: String,
    ) {
        if self.playing() {
            return;
        }
        match animator {
            Some(animator) => {
                let name = self
                    .world
                    .get(animator)
                    .map_or("Animator".to_owned(), |a| a.name().to_owned());
                self.change(
                    animator,
                    Field::Property("StateMachine"),
                    format!("Set State Machine of {name}"),
                    vec![Edit::Property("StateMachine", Value::String(path))],
                );
                self.history.seal();
            }
            None => {
                let id = self
                    .world
                    .create(ClassName::Animator)
                    .expect("not a service");
                self.world
                    .set_property(id, "StateMachine", Value::String(path))
                    .expect("every Animator has a StateMachine");
                self.place_new(part, vec![id], "Insert Animator".to_owned());
            }
        }
    }

    /// The Scene View panel: the viewport.
    pub(in crate::app) fn scene_pane(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&RenderState>,
    ) -> ViewportOutput {
        let ctx = ui.ctx().clone();
        let world = shown(&self.world, &self.play);
        let stylesheets = self
            .project
            .as_ref()
            .map_or(&[][..], |project| project.stylesheets.as_slice());
        // No tool during Play, so nothing can be dragged.
        let tool = (!self.playing()).then_some(self.tool);
        let preview = self.angular_preview();
        let terrain_brush = self.terrain_brush_view(&ctx);
        let emits = std::mem::take(&mut self.pending_emits);
        let debug_rays = self
            .play
            .as_ref()
            .map(Session::debug_rays)
            .unwrap_or_default();
        let output = egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show_inside(ui, |ui| match render_state {
                Some(render_state) => {
                    self.viewport_rect = Some(ui.available_rect_before_wrap());
                    self.viewport.show(
                        ui,
                        render_state,
                        ViewportScene {
                            world,
                            assets: &self.assets,
                            selected: self.selected,
                            selection: &self.selection,
                            tool,
                            snap: self.snap,
                            modes: self.modes,
                            preview: &preview,
                            emits: &emits,
                            stylesheets,
                            focused_text: self.focused_text,
                            light_cones: self.light_cones,
                            overlays: self.overlays,
                            debug_rays: &debug_rays,
                            poses: self.play.as_ref().map(Session::poses),
                            terrain_brush,
                            character_view: self.play.as_ref().and_then(Session::character_camera),
                        },
                    )
                }
                // Can't happen: main.rs always picks the wgpu renderer.
                None => {
                    ui.label("The viewport needs the wgpu renderer.");
                    Default::default()
                }
            })
            .inner;
        if let Some(render) = output.render {
            self.profiler.record_render(render);
        }
        output
    }

    /// A script's dock tab (see `scripts.rs`): Code/Blocks for a Script or
    /// LocalScript, then its code or blocks. `id` is its tab's id.
    pub(in crate::app) fn script_pane(&mut self, ui: &mut egui::Ui, id: u64) {
        let Some(path) = self.tabs.path_of(id).map(str::to_owned) else {
            return;
        };
        let ctx = ui.ctx().clone();
        let palette = theme::palette(&ctx);
        // Clicking or typing in a tab makes it the script used last.
        if ui.ui_contains_pointer() && ctx.input(|input| input.pointer.any_pressed()) {
            self.tabs.activate(id);
        }
        // Scripts and LocalScripts switch between Code and Blocks here.
        if axcenity_project::switched_script_path(&path).is_some() {
            let blocks = axcenity_project::is_block_script(&path);
            let playing = self.playing();
            let switch = egui::TopBottomPanel::top(egui::Id::new(("script mode", id)))
                .frame(
                    egui::Frame::new()
                        .fill(palette.panel)
                        .inner_margin(egui::Margin::symmetric(10, 5)),
                )
                .show_inside(ui, |ui| scripts::mode_bar(ui, blocks, !playing))
                .inner;
            if switch {
                self.switch_script_mode(&path);
                ctx.request_repaint();
                return;
            }
        }
        // Paused at a breakpoint in this script: where, the locals, Continue.
        if let Some(pause) = self.tabs.paused.clone().filter(|pause| pause.path == path) {
            let continued = egui::TopBottomPanel::bottom(egui::Id::new(("paused", id)))
                .resizable(true)
                .default_height(180.0)
                .min_height(80.0)
                .frame(
                    egui::Frame::new()
                        .fill(palette.panel)
                        .inner_margin(egui::Margin::symmetric(10, 6)),
                )
                .show_inside(ui, |ui| paused_panel(ui, &pause))
                .inner;
            if continued {
                self.continue_play();
            }
        }

        if axcenity_project::is_block_script(&path) {
            let mut changed = false;
            let folder = self.project.as_ref().map(|project| project.folder.clone());
            let animation = &mut self.animation;
            let objects = block_objects(&self.world, |controller| match &folder {
                Some(folder) => animation.controller_names(folder, controller),
                None => Vec::new(),
            });
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show_inside(ui, |ui| {
                    if let Some(document) = self.project.as_mut().and_then(|project| {
                        project
                            .block_scripts
                            .iter_mut()
                            .find(|document| document.path == path)
                    }) {
                        changed = block_editor::show(
                            ui,
                            &mut document.script,
                            block_editor::BlockContext {
                                key: &path,
                                client: axcenity_project::script_class(&path)
                                    == ClassName::LocalScript,
                                highlighted: document.highlighted,
                                objects: &objects,
                            },
                        );
                        if changed {
                            document.highlighted = None;
                        }
                    }
                });
            if changed {
                self.update_block_lua(&path);
            }
            return;
        }

        let stylesheet = axcenity_project::is_stylesheet(&path);
        let previous_source = stylesheet
            .then(|| self.tabs.get(&path).map(|tab| tab.text.clone()))
            .flatten();
        egui::CentralPanel::default().show_inside(ui, |ui| self.tabs.show(ui, &path));
        if let Some(previous_source) = previous_source {
            let current_source = self
                .tabs
                .get(&path)
                .map(|tab| tab.text.clone())
                .unwrap_or_default();
            if current_source != previous_source {
                self.refresh_stylesheet(&path, &current_source);
                ctx.request_repaint();
            }
        }
    }
}
