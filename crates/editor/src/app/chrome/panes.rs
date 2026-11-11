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
        if let Some(selection) = explorer.select {
            self.selected = selection.last().copied();
            self.selection = selection;
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
        let kept = match (&self.play, selected) {
            (Some(session), Some(id)) => session.kept().fields(id),
            _ => Vec::new(),
        };
        // Several selected: what they have in common, edited together.
        let many: Vec<InstanceId> = self
            .selection
            .iter()
            .copied()
            .filter(|id| world.get(*id).is_some())
            .collect();
        if many.len() > 1 {
            if let Some(edit) = properties::show_many(ui, world, &self.assets, &many, !editing) {
                self.apply_many(&many, edit);
                ui.ctx().request_repaint();
            }
            return;
        }
        let mut style_edit = None;
        let edit = properties::show(ui, world, &self.assets, selected, !editing, &kept, |ui| {
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
        // A selected SplineMesh or SplineScatter takes the models clicked.
        let spline_models = self.spline_models_picking();
        let action = {
            let picking = if picking_foliage {
                Some(Picking::Foliage(self.terrain.foliage_source.clone()))
            } else {
                spline_models.map(|(_, models)| Picking::SplineModels(models))
            };
            self.asset_browser
                .show(ui, !self.playing(), can_save, picking.as_ref())
        };
        match action {
            Some(BrowserAction::Place(entry)) => self.drop_asset(&entry, None),
            Some(BrowserAction::Pick(entry)) if picking_foliage => {
                if let Some(source) = entry.foliage_source()
                    && self.use_global_asset(&entry.path)
                {
                    self.terrain.foliage_source = source;
                }
            }
            Some(BrowserAction::Pick(entry)) => {
                if let Some((generator, _)) = self.spline_models_picking()
                    && self.use_global_asset(&entry.path)
                {
                    self.pick_spline_model(generator, &entry.path);
                }
            }
            Some(BrowserAction::Import { image_folder }) => self.import_with_dialog(image_folder),
            Some(BrowserAction::OpenToolbox) => self.open_toolbox(None),
            Some(BrowserAction::SavePrefab) => self.save_prefab(),
            Some(BrowserAction::RenamePrefab { path, name }) => self.rename_prefab(&path, &name),
            Some(BrowserAction::DuplicatePrefab { path }) => self.duplicate_prefab(&path),
            Some(BrowserAction::NewVariant { path }) => self.new_variant(&path),
            Some(BrowserAction::DeleteAssets { name, files }) => self.delete_assets(&name, &files),
            Some(BrowserAction::ExportPackage { name, paths }) => {
                self.export_package_with_dialog(&name, paths)
            }
            Some(BrowserAction::EditMaterial(path)) => self.open_material(&path),
            Some(BrowserAction::EditModel(path)) => self.open_in_model_studio(&path),
            Some(BrowserAction::FindReferences(path)) => self.find_asset_references(&path),
            Some(BrowserAction::RenameAsset { from, to }) => self.rename_asset(&from, &to),
            Some(BrowserAction::FindUnused(paths)) => self.find_unused_assets(&paths),
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
        // The shown ScreenGui's Event Graph, lent to the designer.
        self.ui_designer
            .follow(shown(&self.world, &self.play), self.selected);
        let gui = self.ui_designer.current_gui();
        let document = gui.and_then(|gui| self.graph_document(gui));
        let path = gui
            .and_then(|gui| self.world.get(gui))
            .map(|gui| axcenity_blocks::ui_graph::ui_graph_path(gui.name()))
            .unwrap_or_default();
        let editing = !self.playing();
        // The Image picker's pictures, only while an element with an
        // `Image` is selected (each is a thumbnail to read).
        let wants_images = editing
            && self
                .selected
                .and_then(|id| self.world.get(id))
                .is_some_and(|instance| instance.property("Image").is_some());
        let images = if wants_images {
            self.asset_browser.pictures(ui.ctx())
        } else {
            Vec::new()
        };
        // The scripts, only while an element is selected (the inspector's
        // Code & animation searches them).
        let texts = if self.selected.is_some() {
            self.script_texts()
        } else {
            Vec::new()
        };
        let scripts: Vec<crate::ui_designer::links::ScriptText> = texts
            .iter()
            .map(|(path, text)| crate::ui_designer::links::ScriptText { path, text })
            .collect();
        let (stylesheets, graph) = match self.project.as_mut() {
            Some(project) => {
                let graph = document.map(|index| GraphPane {
                    graph: &mut project.ui_graphs[index].graph,
                    path: &path,
                });
                (&project.stylesheets[..], graph)
            }
            None => (&[][..], None),
        };
        let scene = DesignerScene {
            world: shown(&self.world, &self.play),
            selected: self.selected,
            selection: &self.selection,
            stylesheets,
            assets: &self.assets,
            images: &images,
            scripts: &scripts,
            editing,
        };
        let actions = self
            .ui_designer
            .show_with_graph(ui, &scene, render_state, graph);
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
            match action {
                DesignerAction::GraphStep(before) => {
                    self.graph_step(before);
                    continue;
                }
                DesignerAction::PreviewOutput(lines) => {
                    for (error, line) in lines {
                        let line = format!("Live Preview: {line}");
                        if error {
                            self.output.error(line);
                        } else {
                            self.output.info(line);
                        }
                    }
                    continue;
                }
                DesignerAction::Language(language) => {
                    // Play shows the game's world; editing, the edit world.
                    let world = match &mut self.play {
                        Some(session) => session.world_mut(),
                        None => &mut self.world,
                    };
                    world.set_language(&language);
                    continue;
                }
                DesignerAction::UiSettings(settings) => {
                    let world = match &mut self.play {
                        Some(session) => session.world_mut(),
                        None => &mut self.world,
                    };
                    world.set_ui_settings(settings);
                    continue;
                }
                _ => {}
            }
            if self.playing()
                && !matches!(
                    action,
                    DesignerAction::Select(_) | DesignerAction::SelectMany(_)
                )
            {
                continue;
            }
            match action {
                DesignerAction::Select(id) => self.select_from_viewport(id),
                DesignerAction::SelectMany(ids) => self.select_many(ids),
                DesignerAction::GroupDrag { resize, edits } => self.apply_ui_drags(resize, edits),
                DesignerAction::Arrange { label, positions } => self.arrange(label, positions),
                DesignerAction::PasteStyle { ids, declarations } => {
                    self.paste_style(ids, declarations)
                }
                DesignerAction::MoveInto { ids, parent } => self.move_ui_into(ids, parent),
                DesignerAction::Place {
                    id,
                    position,
                    size,
                    resize,
                } => {
                    let Some(name) = self.world.get(id).map(|i| i.name().to_owned()) else {
                        continue;
                    };
                    let (key, label) = if resize {
                        ("Size", format!("Resize {name}"))
                    } else {
                        ("Position", format!("Move {name}"))
                    };
                    // Sealed when the drag or the typing ends.
                    self.change(
                        id,
                        history::Field::Property(key),
                        label,
                        vec![
                            Edit::Property("Position", Value::UDim2(position)),
                            Edit::Property("Size", Value::UDim2(size)),
                        ],
                    );
                }
                DesignerAction::Duplicate => self.duplicate_selected(),
                DesignerAction::Delete => self.delete_selected(),
                DesignerAction::Paste { parent, clipboard } => self.paste_ui(parent, clipboard),
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
                DesignerAction::NextSelection {
                    id,
                    property,
                    target,
                } => self.set_next_selection(id, property, target),
                DesignerAction::Font { id, family } => self.set_font(id, &family),
                DesignerAction::TextStyle {
                    id,
                    property,
                    value,
                    dragging,
                } => self.set_own_style(id, property, &value, dragging),
                DesignerAction::StateStyle {
                    id,
                    state,
                    property,
                    value,
                    dragging,
                } => self.set_state_style(id, state, property, &value, dragging),
                DesignerAction::Insert { what, parent, at } => {
                    self.designer_insert(what, parent, at);
                }
                DesignerAction::Drag(edit) => self.apply_ui_drag(edit),
                DesignerAction::Edit(id, edit) => self.apply(id, edit),
                DesignerAction::Image(id, path) => {
                    self.apply(id, crate::ui_designer::image::image_edit(path));
                    // A pick is a whole step, not more typing.
                    self.history.seal();
                }
                DesignerAction::UploadImage(id) => {
                    self.apply(id, Edit::PickTexture("Image"));
                    self.asset_browser.refresh();
                }
                DesignerAction::Style(id, edit) => self.apply_style(id, edit),
                DesignerAction::ApplyStyleSet { ids, name, rules } => {
                    let label = match ids.as_slice() {
                        [only] => format!(
                            "Style {} as {name}",
                            self.world.get(*only).map_or("", |i| i.name())
                        ),
                        _ => format!("Style {} elements as {name}", ids.len()),
                    };
                    self.apply_style_set(ids, label, rules);
                }
                DesignerAction::ImportStyleFile(id) => self.import_style_file(id),
                DesignerAction::AddHandler { id, event, body } => {
                    self.add_ui_handler(id, &event, body.as_deref())
                }
                DesignerAction::OpenScript { path, line } => self.open_script(&path, Some(line)),
                DesignerAction::Bevel {
                    id,
                    normal,
                    pressed,
                    dragging,
                } => {
                    // Both rules in one step: the edits below don't end it.
                    if !dragging {
                        self.history.seal();
                    }
                    self.set_own_style(id, "box-shadow", &normal, true);
                    if let Some(pressed) = pressed {
                        self.set_state_style(
                            id,
                            axcenity_ui::UiState::Pressed,
                            "box-shadow",
                            &pressed,
                            true,
                        );
                    }
                    if !dragging {
                        self.history.seal();
                    }
                }
                DesignerAction::Restack(changes) => self.restack(changes),
                DesignerAction::NewScreenGui => self.insert(Insertable::ScreenGui, None),
                DesignerAction::Strings { key, label, table } => {
                    if key.is_empty() {
                        self.history.seal();
                    }
                    let before = self.world.strings().clone();
                    self.world.set_strings(std::sync::Arc::new(table));
                    self.history
                        .record_strings(&mut self.world, key.clone(), label, before);
                    if key.is_empty() {
                        self.history.seal();
                    }
                }
                // Handled above.
                DesignerAction::GraphStep(_)
                | DesignerAction::PreviewOutput(_)
                | DesignerAction::Language(_)
                | DesignerAction::UiSettings(_) => {}
            }
        }
    }

    /// Asks for a `.css` file and reads its rule sets: one is applied to
    /// `id` at once, several are offered in the designer's Styles.
    fn import_style_file(&mut self, id: InstanceId) {
        let mut dialog = rfd::FileDialog::new()
            .set_title("Import styles from a stylesheet")
            .add_filter("Stylesheet", &["css"]);
        if let Some(project) = &self.project {
            dialog = dialog.set_directory(&project.folder);
        }
        let Some(file) = dialog.pick_file() else {
            return;
        };
        let name = file
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        let source = match std::fs::read_to_string(&file) {
            Ok(source) => source,
            Err(err) => {
                self.output
                    .error(format!("Could not read `{}`: {err}", file.display()));
                return;
            }
        };
        let (sheet, warnings) = axcenity_ui::parse_stylesheet(&name, &source);
        for warning in warnings {
            self.output.warning(format!("{name}: {}", warning.message));
        }
        let mut sets = axcenity_ui::rule_sets(&sheet);
        match sets.len() {
            0 => self.output.warning(format!(
                "`{name}` has no rules for one element to import (rules like `.card {{ … }}` and \
                 `.card:hover {{ … }}`)."
            )),
            1 => {
                let set = sets.remove(0);
                self.designer_actions(vec![DesignerAction::ApplyStyleSet {
                    ids: vec![id],
                    name: set.selector,
                    rules: set.rules,
                }]);
            }
            _ => self.ui_designer.offer_imported_styles(name, sets),
        }
    }

    /// Layer order from the designer's menus: each element's new z, in its
    /// own `#Id` rule's `z-index` where CSS places it, else in `ZIndex`.
    fn restack(&mut self, changes: Vec<(InstanceId, f64)>) {
        let stylesheets = self
            .project
            .as_ref()
            .map_or(&[][..], |project| &project.stylesheets[..]);
        let (css, plain): (Vec<_>, Vec<_>) = changes
            .into_iter()
            .partition(|&(id, _)| axcenity_ui::css_sets_z_index(&self.world, id, stylesheets));
        self.history.seal();
        if !plain.is_empty() {
            let name = self
                .world
                .get(plain[0].0)
                .map(|i| i.name().to_owned())
                .unwrap_or_default();
            self.change_many(
                history::Field::Property("ZIndex"),
                format!("Change layer order of {name}"),
                plain
                    .into_iter()
                    .map(|(id, z)| (id, vec![Edit::Property("ZIndex", Value::Number(z))]))
                    .collect(),
            );
        }
        for (id, z) in css {
            self.set_own_style(id, "z-index", &z.to_string(), true);
        }
        self.history.seal();
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
            // Added as part of the gesture, which goes on.
            _ if dragging => return self.add_style_unsealed(id, "", property, value),
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

    /// Sets `property` to `value` in `id`'s own rule for `state` (such as
    /// `#play:hover`): changed in place when that rule has it, else added to
    /// the rule, which is made if there isn't one. Normal is `#play` itself
    /// ([`Self::set_own_style`]). One undo step, or part of a drag's.
    pub(in crate::app) fn set_state_style(
        &mut self,
        id: InstanceId,
        state: axcenity_ui::UiState,
        property: &str,
        value: &str,
        dragging: bool,
    ) {
        if state == axcenity_ui::UiState::Normal {
            return self.set_own_style(id, property, value, dragging);
        }
        let own = self
            .world
            .get(id)
            .and_then(|instance| instance.property("Id"))
            .and_then(Value::as_str)
            .filter(|element_id| !element_id.is_empty())
            .map(|element_id| crate::ui_designer::state_selector(element_id, state));
        let existing = own.and_then(|selector| {
            let project = self.project.as_ref()?;
            axcenity_ui::rule_declaration(&project.stylesheets, &selector, property)
        });
        if !dragging {
            self.history.seal();
        }
        match existing {
            Some(declaration) => self.apply_style(
                id,
                StyleEdit::Set {
                    path: declaration.path,
                    span: declaration.span,
                    value: value.to_owned(),
                },
            ),
            None if dragging => self.add_style_unsealed(id, state.pseudo_class(), property, value),
            None => self.add_style_to(id, state.pseudo_class(), property, value),
        }
        if !dragging {
            self.history.seal();
        }
    }

    /// Puts a new `what` from the designer's palette into `parent`, its top
    /// left `at` design pixels from the parent's, and selects it. A Page
    /// goes into `parent` when it's a PageLayout, else into the PageLayout
    /// around it, else into a new PageLayout made in `parent` (never one
    /// in another ScreenGui).
    pub(in crate::app) fn designer_insert(
        &mut self,
        what: Insertable,
        parent: InstanceId,
        at: [f32; 2],
    ) {
        if what == Insertable::Page {
            let is_layout = |world: &World, id| {
                world
                    .get(id)
                    .is_some_and(|instance| instance.class() == ClassName::PageLayout)
            };
            let around = std::iter::successors(Some(parent), |&id| {
                self.world.get(id).and_then(|instance| instance.parent())
            })
            .take_while(|&id| self.world.get(id).map(|i| i.class()) != Some(ClassName::ScreenGui))
            .find(|&id| is_layout(&self.world, id));
            let layout = match around {
                Some(layout) => layout,
                None => {
                    self.insert_named(Insertable::PageLayout, Some(parent), "PageLayout", vec![]);
                    match self.selected.filter(|&id| is_layout(&self.world, id)) {
                        Some(layout) => layout,
                        None => return,
                    }
                }
            };
            self.insert_named(Insertable::Frame, Some(layout), "Page", Vec::new());
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

    /// Trades the shown Play device's particles and streaming with the
    /// viewport's (`Session::swap_effects`); a second call trades back.
    fn swap_play_effects(&mut self) {
        if let Some(play) = &mut self.play {
            let (particles, streaming) = self.viewport.effects_mut();
            play.swap_effects(particles, streaming);
        }
    }

    /// The Scene View panel: the viewport.
    pub(in crate::app) fn scene_pane(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&RenderState>,
    ) -> ViewportOutput {
        let ctx = ui.ctx().clone();
        self.scene_drawn_pass = Some(theme::frame_number(&ctx));
        self.refresh_nav_mesh(&ctx);
        self.update_sequence_preview();
        // The game's particles and streaming, which its frame moves on, are
        // drawn in the viewport while it shows Play; swapped back below.
        self.swap_play_effects();
        // The Sequencer's preview, outside Play (Phase 51): its own copy
        // of the world, posed and seen from its camera cut.
        let sequence = (!self.playing() && self.layout.is_open(Pane::Sequencer))
            .then(|| self.sequencer.shown())
            .flatten();
        let world = sequence.map_or_else(|| shown(&self.world, &self.play), |p| &p.world);
        let stylesheets = self
            .project
            .as_ref()
            .map_or(&[][..], |project| project.stylesheets.as_slice());
        // No tool during Play or the preview, so nothing can be dragged.
        let tool = (!self.playing() && sequence.is_none()).then_some(self.tool);
        let preview = self.angular_preview();
        let terrain_brush = self.terrain_brush_view(&ctx);
        let plugin_mouse = self
            .plugins
            .entries
            .iter()
            .any(|entry| entry.running.is_some())
            .then(|| self.plugin_has_mouse());
        let mut console = self.dev_console_items();
        let emits = std::mem::take(&mut self.pending_emits);
        let debug_rays = self
            .play
            .as_ref()
            .map(|play| play.debug_rays())
            .unwrap_or_default();
        let agent_paths = match &self.play {
            Some(session) if self.overlays.agent_paths => session.agent_paths(),
            _ => Vec::new(),
        };
        let spline_editing = self.spline_editing();
        let mut output = egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| match render_state {
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
                            loading_screen: self
                                .play
                                .as_ref()
                                .and_then(|play| play.loading_screen()),
                            hotbar: self.play.as_ref().and_then(|play| play.hotbar()),
                            ability_bar: self
                                .play
                                .as_ref()
                                .map(|play| play.ability_bar())
                                .unwrap_or_default(),
                            quests: self
                                .play
                                .as_ref()
                                .map(|play| play.quest_tracker())
                                .unwrap_or_default(),
                            dialogue: self.play.as_ref().and_then(|play| play.dialogue_view()),
                            leaderboard: self.play.as_ref().is_some_and(|play| play.leaderboard()),
                            chat: self
                                .play
                                .as_ref()
                                .map(|play| (play.chat_lines(), self.chat_typing.clone())),
                            prompts: self
                                .play
                                .as_ref()
                                .map(|play| play.prompts())
                                .unwrap_or_default(),
                            click_cursor: self.play.as_ref().and_then(|play| play.click_cursor()),
                            console: std::mem::take(&mut console),
                            light_cones: self.light_cones,
                            view_cube: self.view_cube,
                            grid: self.grid,
                            grid_strength: self.grid_strength,
                            overlays: self.overlays,
                            view_mode: self.view_mode,
                            playing: self.play.is_some(),
                            debug_rays: &debug_rays,
                            nav_mesh: self.nav_mesh.as_ref().map(|cache| &cache.mesh),
                            agent_paths: &agent_paths,
                            poses: sequence
                                .map(crate::sequencer::Preview::poses)
                                .or(self.play.as_ref().map(|play| play.poses())),
                            // A Sequencer preview draws its own poses, not
                            // the game's.
                            blends: self
                                .play
                                .as_ref()
                                .filter(|_| sequence.is_none())
                                .map(|play| play.blends()),
                            terrain_brush,
                            constraint_pick: self
                                .constraint_tool
                                .as_ref()
                                .map(|tool| tool.first.map(|first| first.point)),
                            spline_editing,
                            plugin_mouse,
                            play_camera: match sequence.and_then(|p| p.camera) {
                                Some(shot) => Some(crate::viewport::PlayCamera {
                                    shown: axcenity_script::CameraView {
                                        position: shot.position,
                                        orientation: shot.orientation,
                                        field_of_view: shot.field_of_view,
                                    },
                                    holds: true,
                                }),
                                None => self.play.as_ref().map(|session| {
                                    let (position, orientation) = self.viewport.camera_view();
                                    crate::viewport::PlayCamera {
                                        shown: session.shown_camera(axcenity_script::CameraState {
                                            position,
                                            orientation,
                                        }),
                                        holds: session.holds_camera(),
                                    }
                                }),
                            },
                            // A LocalScript's change lasts until Stop.
                            graphics: self
                                .play
                                .as_ref()
                                .and_then(|play| play.graphics())
                                .unwrap_or(&self.graphics),
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
        self.swap_play_effects();
        if let Some(render) = output.render {
            self.profiler.record_render(render);
            self.last_drawn = Some(super::mcp::LastDrawn {
                cost: render,
                at: std::time::Instant::now(),
                size: self.viewport.pixel_size(),
                playing: self.play.is_some(),
            });
        }
        self.profiler.record_shader_errors(&output.shader_errors);
        for error in output.shader_errors.drain(..) {
            self.output.error(error.to_string());
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
            let switch = egui::Panel::top(egui::Id::new(("script mode", id)))
                .frame(
                    egui::Frame::new()
                        .fill(palette.panel)
                        .inner_margin(egui::Margin::symmetric(10, 5)),
                )
                .show(ui, |ui| scripts::mode_bar(ui, view, !playing))
                .inner;
            if let Some(view) = switch {
                self.switch_script_view(&path, view);
                ctx.request_repaint();
                return;
            }
        }
        // Paused in this script: where, the locals, watches, Continue and
        // the steps.
        if let Some(pause) = self.tabs.paused.clone().filter(|pause| pause.path == path) {
            let watches = &mut self.watches;
            let resume = egui::Panel::bottom(egui::Id::new(("paused", id)))
                .resizable(true)
                .default_size(180.0)
                .min_size(80.0)
                .frame(
                    egui::Frame::new()
                        .fill(palette.panel)
                        .inner_margin(egui::Margin::symmetric(10, 6)),
                )
                .show(ui, |ui| paused_panel(ui, &pause, watches))
                .inner;
            if self.watches.is_stale() {
                self.evaluate_watches();
            }
            if let Some(how) = resume {
                self.resume_play(how);
            }
        }

        if axcenity_project::is_block_script(&path) {
            let mut changed = false;
            let folder = self.project.as_ref().map(|project| project.folder.clone());
            let animation = &mut self.animation;
            let mut objects = block_objects(&self.world, |controller| match &folder {
                Some(folder) => animation.controller_names(folder, controller),
                None => Vec::new(),
            });
            if let Some(folder) = &folder {
                self.add_table_names(folder, &mut objects);
            }
            if ui.rect_contains_pointer(ui.max_rect()) {
                self.block_tab_pointer = Some(path.clone());
            }
            // An edit is over once nothing is held or typed (`BlockUndo`).
            let busy =
                ctx.egui_wants_keyboard_input() || ctx.input(|input| input.pointer.any_down());
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ui, |ui| {
                    if let Some(document) = self.project.as_mut().and_then(|project| {
                        project
                            .block_scripts
                            .iter_mut()
                            .find(|document| document.path == path)
                    }) {
                        let before = document.script.clone();
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
                        document.undo.track(before, &document.script, busy);
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
        egui::CentralPanel::default().show(ui, |ui| self.tabs.show(ui, &path));
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
