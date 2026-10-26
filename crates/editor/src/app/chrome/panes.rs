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
                .show(ui, !self.playing(), can_save, foliage)
        };
        match action {
            Some(BrowserAction::Place(entry)) => self.drop_asset(&entry, None),
            Some(BrowserAction::Foliage(entry)) => {
                if let Some(source) = entry.foliage_source()
                    && self.use_global_asset(&entry.path)
                {
                    self.terrain.foliage_source = source;
                }
            }
            Some(BrowserAction::Import { image_folder }) => self.import_with_dialog(image_folder),
            Some(BrowserAction::SavePrefab) => self.save_prefab(),
            Some(BrowserAction::RenamePrefab { path, name }) => self.rename_prefab(&path, &name),
            Some(BrowserAction::DuplicatePrefab { path }) => self.duplicate_prefab(&path),
            Some(BrowserAction::DeleteAssets { name, files }) => self.delete_assets(&name, &files),
            Some(BrowserAction::ExportPackage { name, paths }) => {
                self.export_package_with_dialog(&name, paths)
            }
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

    /// The UI Designer tab (see `ui_designer.rs`), then what was done in it.
    pub(in crate::app) fn ui_designer_pane(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&RenderState>,
    ) {
        let scene = DesignerScene {
            world: shown(&self.world, &self.play),
            selected: self.selected,
            stylesheets: self
                .project
                .as_ref()
                .map_or(&[][..], |project| &project.stylesheets[..]),
            assets: &self.assets,
            editing: !self.playing(),
        };
        let actions = self.ui_designer.show(ui, &scene, render_state);
        if !actions.is_empty() {
            self.designer_actions(actions);
            // Other panels may already be drawn this frame.
            ui.ctx().request_repaint();
        }
    }

    /// Does what was done in the UI Designer. Each edit goes through the
    /// same paths as the Properties panel, the Explorer and the viewport's
    /// handles, so it's an undo step and shows in the Scene View and Play.
    pub(in crate::app) fn designer_actions(&mut self, actions: Vec<DesignerAction>) {
        for action in actions {
            if self.playing() && !matches!(action, DesignerAction::Select(_)) {
                continue;
            }
            match action {
                DesignerAction::Select(id) => self.select_from_viewport(id),
                DesignerAction::DesignSize { gui, width, height } => {
                    let Some(name) = self.world.get(gui).map(|i| i.name().to_owned()) else {
                        continue;
                    };
                    self.change(
                        gui,
                        history::Field::Property("DesignWidth"),
                        format!("Change design size of {name}"),
                        vec![
                            Edit::Property("DesignWidth", Value::Number(f64::from(width))),
                            Edit::Property("DesignHeight", Value::Number(f64::from(height))),
                        ],
                    );
                }
                DesignerAction::ScaleMode(gui, mode) => {
                    self.apply(
                        gui,
                        Edit::Property("ScaleMode", Value::String(mode.to_owned())),
                    );
                    self.history.seal();
                }
                DesignerAction::Anchor { id, position, size } => {
                    self.anchor(id, position, size);
                    self.history.seal();
                }
                // Sealed when the drag ends, as handle drags are.
                DesignerAction::MoveAnchor { id, position, size } => {
                    self.anchor(id, position, size);
                }
                DesignerAction::Font { id, family } => self.set_font(id, &family),
                DesignerAction::TextStyle {
                    id,
                    property,
                    value,
                    dragging,
                } => self.set_own_style(id, property, &value, dragging),
                DesignerAction::Insert { what, parent, at } => {
                    self.designer_insert(what, parent, at);
                }
                DesignerAction::Drag(edit) => self.apply_ui_drag(edit),
                DesignerAction::Edit(id, edit) => self.apply(id, edit),
                DesignerAction::Style(id, edit) => self.apply_style(id, edit),
                DesignerAction::NewScreenGui => self.insert(Insertable::ScreenGui, None),
            }
        }
    }

    /// Sets `id`'s `Position` and `Size` together, as an anchor change.
    fn anchor(
        &mut self,
        id: InstanceId,
        position: axcenity_core::UDim2,
        size: axcenity_core::UDim2,
    ) {
        let Some(name) = self.world.get(id).map(|i| i.name().to_owned()) else {
            return;
        };
        self.change(
            id,
            history::Field::Property("Position"),
            format!("Anchor {name}"),
            vec![
                Edit::Property("Position", Value::UDim2(position)),
                Edit::Property("Size", Value::UDim2(size)),
            ],
        );
    }

    /// Draws `id`'s text in `family` (CSS `font-family`), as one undo step.
    pub(in crate::app) fn set_font(&mut self, id: InstanceId, family: &str) {
        // CSS needs a family name with spaces in quotes.
        let value = if family.contains(' ') {
            format!("\"{family}\"")
        } else {
            family.to_owned()
        };
        self.set_own_style(id, "font-family", &value, false);
    }

    /// Sets `property` to `value` for `id`: its own `#Id` rule's declaration
    /// is changed in place, or added to that rule (which wins over class
    /// rules) when it has none. One undo step, or, while `dragging`, part of
    /// the step the drag is making.
    pub(in crate::app) fn set_own_style(
        &mut self,
        id: InstanceId,
        property: &str,
        value: &str,
        dragging: bool,
    ) {
        let own = self
            .world
            .get(id)
            .and_then(|instance| instance.property("Id"))
            .and_then(Value::as_str)
            .filter(|element_id| !element_id.is_empty())
            .map(style_panel::own_selector);
        let applied = self.project.as_ref().and_then(|project| {
            axcenity_ui::applied_declaration(&self.world, id, &project.stylesheets, property)
        });
        let edit = match applied {
            Some(declaration) if Some(&declaration.selector) == own.as_ref() => StyleEdit::Set {
                path: declaration.path,
                span: declaration.span,
                value: value.to_owned(),
            },
            _ => StyleEdit::Add {
                property: property.to_owned(),
                value: value.to_owned(),
            },
        };
        if !dragging {
            self.history.seal();
        }
        self.apply_style(id, edit);
        if !dragging {
            self.history.seal();
        }
    }

    /// Puts a new `what` from the designer's palette into `parent`, its top
    /// left `at` design pixels from the parent's, and selects it. A Page
    /// goes into the PageLayout there (made if there isn't one).
    pub(in crate::app) fn designer_insert(
        &mut self,
        what: Insertable,
        parent: InstanceId,
        at: [f32; 2],
    ) {
        if what == Insertable::Page {
            self.insert(Insertable::Page, Some(parent));
            return;
        }
        let mut properties = vec![(
            "Position",
            Value::UDim2(axcenity_core::UDim2::new(0.0, at[0], 0.0, at[1])),
        )];
        // A TextLabel fills its parent by default, which a drop at a point
        // doesn't mean.
        if what == Insertable::TextLabel {
            properties.push((
                "Size",
                Value::UDim2(axcenity_core::UDim2::new(0.0, 200.0, 0.0, 40.0)),
            ));
        }
        self.insert_named(what, Some(parent), what.class().as_str(), properties);
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
                StudioAction::Import => {
                    self.import_with_dialog(axcenity_assets::PROJECT_TEXTURES_FOLDER)
                }
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
        // Scripts and LocalScripts switch between Code, Blocks and Graph here.
        let graph = self.project.as_ref().is_some_and(|project| {
            project
                .block_scripts
                .iter()
                .any(|document| document.path == path && document.script.graph.shown)
        });
        if axcenity_project::switched_script_path(&path).is_some() {
            let view = if !axcenity_project::is_block_script(&path) {
                scripts::ScriptView::Code
            } else if graph {
                scripts::ScriptView::Graph
            } else {
                scripts::ScriptView::Blocks
            };
            let playing = self.playing();
            let switch = egui::TopBottomPanel::top(egui::Id::new(("script mode", id)))
                .frame(
                    egui::Frame::new()
                        .fill(palette.panel)
                        .inner_margin(egui::Margin::symmetric(10, 5)),
                )
                .show_inside(ui, |ui| scripts::mode_bar(ui, view, !playing))
                .inner;
            if let Some(view) = switch {
                self.switch_script_view(&path, view);
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
                        let context = block_editor::BlockContext {
                            key: &path,
                            client: axcenity_project::script_class(&path) == ClassName::LocalScript,
                            highlighted: document.highlighted,
                            objects: &objects,
                        };
                        changed = if graph {
                            block_editor::show_graph(ui, &mut document.script, context)
                        } else {
                            block_editor::show(ui, &mut document.script, context)
                        };
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
