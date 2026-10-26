//! Selection, property edits, transform tools, stylesheets and undo.
//!
//! Every edit-mode change goes through [`EditorApp::change`] so one gesture is
//! one history step. Play-mode property edits go to the session world instead
//! and are not recorded. These are methods of [`super::EditorApp`].

use super::*;

impl EditorApp {
    /// Selects `id` (or nothing), from a click in the viewport. The Explorer
    /// opens the branches above it and scrolls to it, so it's highlighted there too.
    pub(super) fn select_from_viewport(&mut self, id: Option<InstanceId>) {
        self.selection = id.into_iter().collect();
        self.selected = id;
        if let Some(id) = id {
            self.explorer.reveal(shown(&self.world, &self.play), id);
        }
    }
    /// A click in the Explorer or the viewport. `order` is the Explorer's
    /// visible rows, so Shift can select the range; the viewport passes `None`.
    pub(super) fn select_clicked(
        &mut self,
        id: Option<InstanceId>,
        modifiers: egui::Modifiers,
        order: Option<&[InstanceId]>,
        keep_group: bool,
    ) {
        explorer::update_selection(&mut self.selection, id, modifiers, order, keep_group);
        self.selected = self.selection.last().copied();
        if let Some(id) = self.selected {
            self.explorer.reveal(shown(&self.world, &self.play), id);
        }
    }
    /// The selected Instances that aren't inside another selected one, so
    /// deleting or duplicating a parent doesn't also do it to its children.
    pub(super) fn selection_roots(&self) -> Vec<InstanceId> {
        let ids = if self.selection.is_empty() {
            self.selected.into_iter().collect::<Vec<_>>()
        } else {
            self.selection.clone()
        };
        ids.iter()
            .copied()
            .filter(|id| {
                !ids.iter()
                    .any(|other| self.world.is_descendant_of(*id, *other))
            })
            .collect()
    }
    /// True when the world or a script tab has changes that aren't saved.
    /// Undoing back to where it was saved counts as no changes.
    pub(super) fn unsaved(&self) -> bool {
        !self.history.is_saved()
            || self.tabs.any_unsaved()
            || self.project.as_ref().is_some_and(|project| {
                project
                    .block_scripts
                    .iter()
                    .any(|document| document.script != document.saved)
            })
    }
    /// Applies a change from the Properties panel, as an undo step such as
    /// "Change Color of Pad". During Play it changes the game instead.
    pub(super) fn apply(&mut self, id: InstanceId, edit: Edit) {
        let Some(instance) = shown(&self.world, &self.play).get(id) else {
            return;
        };
        let (key, label) = match &edit {
            Edit::Name(_) => (Field::Name, format!("Rename {}", instance.name())),
            Edit::Property(name, _) => (
                Field::Property(name),
                format!("Change {name} of {}", instance.name()),
            ),
            &Edit::PickTexture(name) => {
                let folder = if name == "Texture" {
                    crate::asset_browser::PARTICLES_FOLDER
                } else {
                    axcenity_assets::PROJECT_TEXTURES_FOLDER
                };
                if let Some(path) = self.pick_texture(folder) {
                    self.apply(id, Edit::Property(name, Value::String(path)));
                    // Picking is one whole step, not more typing.
                    self.history.seal();
                }
                return;
            }
            Edit::PickAudio => {
                if let Some(path) = self.pick_audio() {
                    self.apply(id, Edit::Property("SoundId", Value::String(path)));
                    self.history.seal();
                }
                return;
            }
            Edit::PickController => {
                if let Some(path) = self.pick_controller() {
                    self.apply(id, Edit::Property("StateMachine", Value::String(path)));
                    self.history.seal();
                }
                return;
            }
            Edit::OpenController => {
                self.open_animator_controller(id);
                return;
            }
            Edit::Animate => {
                self.animate(id);
                return;
            }
            Edit::PickClip => {
                if let Some(path) = self.pick_property_clip() {
                    self.apply(id, Edit::Property("Clip", Value::String(path)));
                    self.history.seal();
                }
                return;
            }
            Edit::OpenClip => {
                if let Some(path) = self
                    .world
                    .get(id)
                    .and_then(|i| i.property("Clip"))
                    .and_then(Value::as_str)
                {
                    let path = path.to_owned();
                    self.open_animation_clip(&path, false);
                }
                return;
            }
            &Edit::PreviewSound(play) => {
                self.preview_sound(id, play);
                return;
            }
        };
        self.change(id, key, label, vec![edit]);
    }
    /// Applies a drag from the viewport (a part or one of its handles), as
    /// an undo step such as "Move Pad".
    pub(super) fn apply_drag(&mut self, edit: DragEdit) {
        let Some(instance) = edit.parts.last().and_then(|(id, _)| self.world.get(*id)) else {
            return;
        };
        let (property, verb) = match edit.kind {
            DragKind::Move => ("Position", "Move"),
            DragKind::Resize => ("Size", "Resize"),
            DragKind::Rotate => ("Orientation", "Rotate"),
        };
        let label = match edit.parts.len() {
            1 => format!("{verb} {}", instance.name()),
            n => format!("{verb} {n} parts"),
        };
        let edits = edit
            .parts
            .into_iter()
            .map(|(id, values)| {
                let edits = values
                    .into_iter()
                    .map(|(name, value)| Edit::Property(name, value))
                    .collect();
                (id, edits)
            })
            .collect();
        self.change_many(Field::Property(property), label, edits);
    }
    /// The parts the building tools act on: the selected parts and those
    /// inside selected Models, the last selected last.
    pub(super) fn tool_parts(&self) -> Vec<InstanceId> {
        let world = shown(&self.world, &self.play);
        viewport::selected_parts(world, &self.selection, self.selected)
            .into_iter()
            .map(|(id, _)| id)
            .collect()
    }
    /// Does what a building tool's panel asked, to the parts it acts on,
    /// as one undo step.
    pub(super) fn apply_tool_action(&mut self, action: ToolAction) {
        if self.playing() || self.viewport.is_dragging() {
            return;
        }
        let parts = self.tool_parts();
        match action {
            ToolAction::Set { property, value } => self.set_on_parts(&parts, property, value),
            ToolAction::AddLight(class) => self.add_lights(&parts, class),
            ToolAction::RemoveLights(class) => {
                let lights = build_tools::lights_in(&self.world, &parts, class);
                let moves: Vec<Move> = lights
                    .iter()
                    .filter_map(|&id| {
                        let before = history::place_of(&self.world, id)?;
                        history::put(&mut self.world, id, None).ok()?;
                        Some(Move {
                            id,
                            before: Some(before),
                            after: None,
                        })
                    })
                    .collect();
                let label = format!("Remove {} {class}s", moves.len());
                self.history.record_tree(&mut self.world, label, moves);
                self.forget_selection_outside_tree();
            }
            ToolAction::Extend => self.extend_angular(),
            ToolAction::AddEmitter(kind) => {
                let properties = build_tools::particle_preset(kind);
                self.add_children(
                    &parts,
                    ClassName::ParticleEmitter,
                    &properties,
                    format!("{} particles", kind.as_str()),
                );
            }
            ToolAction::Burst => {
                let emitters =
                    build_tools::lights_in(&self.world, &parts, ClassName::ParticleEmitter);
                self.pending_emits
                    .extend(emitters.into_iter().map(|id| (id, 20)));
            }
            ToolAction::SetLight {
                class,
                property,
                value,
            } => {
                let lights = build_tools::lights_in(&self.world, &parts, class);
                let edits = lights
                    .into_iter()
                    .map(|id| (id, vec![Edit::Property(property, value.clone())]))
                    .collect();
                self.change_many(
                    Field::Property(property),
                    format!("Change {property} of {class}s"),
                    edits,
                );
            }
        }
    }
    /// Sets `property` to `value` on each of `parts` that has it, as one step.
    pub(super) fn set_on_parts(
        &mut self,
        parts: &[InstanceId],
        property: &'static str,
        value: Value,
    ) {
        let world = shown(&self.world, &self.play);
        let targets: Vec<InstanceId> = parts
            .iter()
            .copied()
            .filter(|&id| {
                world
                    .get(id)
                    .is_some_and(|i| i.property(property).is_some())
            })
            .collect();
        let Some(name) = targets
            .last()
            .and_then(|&id| world.get(id))
            .map(|i| i.name().to_owned())
        else {
            return;
        };
        let label = build_tools::step_label(property, &value, targets.len(), &name);
        let edits = targets
            .into_iter()
            .map(|id| (id, vec![Edit::Property(property, value.clone())]))
            .collect();
        self.change_many(Field::Property(property), label, edits);
    }
    /// Puts a new `class` light in each of `parts` that hasn't one, as one step.
    pub(super) fn add_lights(&mut self, parts: &[InstanceId], class: ClassName) {
        let parts: Vec<InstanceId> = parts
            .iter()
            .copied()
            .filter(|&part| build_tools::lights_in(&self.world, &[part], class).is_empty())
            .collect();
        let label = match parts.len() {
            1 => format!("Add {class}"),
            n => format!("Add {n} {class}s"),
        };
        self.add_children(&parts, class, &[], label);
    }
    /// Puts a new `class` with `properties` in each of `parts` (if it can
    /// go there), named after the class, as one undo step named `label`.
    pub(super) fn add_children(
        &mut self,
        parts: &[InstanceId],
        class: ClassName,
        properties: &[(&'static str, Value)],
        label: String,
    ) {
        let mut moves = Vec::new();
        for &part in parts {
            if !self
                .world
                .get(part)
                .is_some_and(|i| matches!(i.class(), ClassName::Part | ClassName::MeshPart))
            {
                continue;
            }
            let Ok(id) = self.world.create(class) else {
                continue;
            };
            for (property, value) in properties {
                let _ = self.world.set_property(id, property, value.clone());
            }
            let name = tree::unique_name(&self.world, part, class.as_str());
            let _ = self.world.set_name(id, name);
            let index = self.world.get(part).map_or(0, |p| p.children().len());
            let place = Place {
                parent: part,
                index,
            };
            if history::put(&mut self.world, id, Some(place)).is_ok() {
                moves.push(Move {
                    id,
                    before: None,
                    after: Some(place),
                });
            }
        }
        self.history.record_tree(&mut self.world, label, moves);
    }
    /// Where the Angular tool's copies of the last selected part would go.
    pub(super) fn angular_preview(&self) -> Vec<Pose> {
        if self.tool != Tool::Angular || self.playing() {
            return Vec::new();
        }
        self.selected
            .and_then(|id| self.world.get(id))
            .and_then(Pose::of)
            .map_or_else(Vec::new, |pose| angular::chain(pose, &self.build.angular))
    }
    /// The Angular tool: copies the last selected part (with what's inside
    /// it) to each place in its chain, right after it in the tree, as one
    /// undo step, and selects the last copy so the next press carries on.
    pub(super) fn extend_angular(&mut self) {
        let Some(original) = self.selected else {
            return;
        };
        let Some(pose) = self.world.get(original).and_then(Pose::of) else {
            self.output
                .info("The Angular tool extends a part: select one first.");
            return;
        };
        let Some(name) = self.editable_name(original, "extend") else {
            return;
        };
        let Some(start) = history::place_of(&self.world, original) else {
            return;
        };
        let mut moves = Vec::new();
        for (i, next) in angular::chain(pose, &self.build.angular)
            .into_iter()
            .enumerate()
        {
            let Ok(copy) = self.world.duplicate(original) else {
                break;
            };
            let orientation = orientation_from_rotation(next.rotation);
            let tidy = |value: f32| (value * 1000.0).round() / 1000.0 + 0.0;
            let position = Vector3::new(
                tidy(next.position.x),
                tidy(next.position.y),
                tidy(next.position.z),
            );
            let orientation = Vector3::new(
                tidy(orientation.x),
                tidy(orientation.y),
                tidy(orientation.z),
            );
            let _ = self
                .world
                .set_property(copy, "Position", Value::Vector3(position));
            let _ = self
                .world
                .set_property(copy, "Orientation", Value::Vector3(orientation));
            let copy_name = tree::unique_name(&self.world, start.parent, &name);
            let _ = self.world.set_name(copy, copy_name);
            let place = Place {
                parent: start.parent,
                index: start.index + 1 + i,
            };
            if history::put(&mut self.world, copy, Some(place)).is_err() {
                break;
            }
            moves.push(Move {
                id: copy,
                before: None,
                after: Some(place),
            });
        }
        let last = moves.last().map(|step| step.id);
        let label = match moves.len() {
            1 => format!("Extend {name}"),
            n => format!("Extend {name} {n} times"),
        };
        self.history.record_tree(&mut self.world, label, moves);
        if last.is_some() {
            self.select_from_viewport(last);
        }
    }
    /// The Paint tool: a click on a part paints it, and the rest of the
    /// selection too if it's selected.
    pub(super) fn paint_clicked(&mut self, part: InstanceId) {
        let parts = if self.selection.contains(&part) {
            self.tool_parts()
        } else {
            vec![part]
        };
        let color = Value::Color3(self.build.paint_color);
        self.set_on_parts(&parts, "Color", color);
        self.history.seal();
    }
    /// The selected building tool's options panel, at the top left of the viewport.
    pub(super) fn tool_panel(&mut self, ctx: &egui::Context, viewport: egui::Rect) {
        if self.tool == Tool::Select || self.playing() {
            return;
        }
        let parts = self.tool_parts();
        let scene = PanelScene {
            world: &self.world,
            parts: &parts,
        };
        let (tool, settings, modes, snap) =
            (self.tool, &mut self.build, &mut self.modes, &mut self.snap);
        let actions = egui::Area::new(egui::Id::new("building tool panel"))
            .fixed_pos(viewport.min + egui::vec2(10.0, 10.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .corner_radius(8)
                    .inner_margin(egui::Margin::same(10))
                    .show(ui, |ui| {
                        ui.set_max_width(260.0);
                        build_tools::panel(ui, tool, &scene, settings, modes, snap)
                    })
                    .inner
            })
            .inner;
        for action in actions {
            self.apply_tool_action(action);
            ctx.request_repaint();
        }
    }
    /// A UI handle dragged in the viewport or the UI Designer: its new
    /// `Position` and `Size`, and any CSS lengths it rewrote, as one undo
    /// step for the whole drag.
    pub(super) fn apply_ui_drag(&mut self, edit: UiDragEdit) {
        if self.playing() {
            return;
        }
        let Some(name) = self.world.get(edit.id).map(|i| i.name().to_owned()) else {
            return;
        };
        let before = history::snapshot(&self.world, edit.id);
        for (property, value) in edit.values {
            if let Err(err) = set(&mut self.world, edit.id, Edit::Property(property, value)) {
                self.output.error(format!("Could not change that: {err:#}"));
            }
        }
        let after = history::snapshot(&self.world, edit.id);

        let mut texts = Vec::new();
        let mut edits_by_path: HashMap<String, Vec<CssEdit>> = HashMap::new();
        for css_edit in edit.css {
            edits_by_path
                .entry(css_edit.path.clone())
                .or_default()
                .push(css_edit);
        }
        for (path, edits) in edits_by_path {
            if !self.ui_drag_baselines.contains_key(&path) {
                let source = self
                    .tabs
                    .get(&path)
                    .map(|tab| tab.text.clone())
                    .or_else(|| {
                        self.project.as_ref().and_then(|project| {
                            std::fs::read_to_string(project.folder.join(&path)).ok()
                        })
                    });
                let Some(source) = source else {
                    self.output.error(format!(
                        "Could not edit stylesheet `{path}`: its source file could not be read."
                    ));
                    continue;
                };
                self.ui_drag_baselines.insert(path.clone(), source.clone());
                self.tabs.open_in_background(&path, &source);
            }
            let Some(source) = self.ui_drag_baselines.get(&path) else {
                continue;
            };
            let updated = apply_css_edits(source, edits.iter());
            texts.push(history::TextChange {
                path: path.clone(),
                before: Some(source.clone()),
                after: Some(updated.clone()),
            });
            if let Some(tab) = self.tabs.get_mut(&path) {
                tab.text = updated.clone();
            }
            self.refresh_stylesheet(&path, &updated);
        }
        // One key for the whole drag, so a left-edge resize (which changes
        // Size and Position) stays one step.
        let (key, label) = if edit.resize {
            (Field::Property("Size"), format!("Resize {name}"))
        } else {
            (Field::Property("Position"), format!("Move {name}"))
        };
        self.history
            .record_with_texts(&mut self.world, edit.id, key, label, before, after, texts);
    }
    /// The text of the stylesheet at `path`: its tab's, if it's open (it may
    /// have unsaved changes), otherwise the file's.
    pub(super) fn stylesheet_text(&self, path: &str) -> Option<String> {
        self.tabs.get(path).map(|tab| tab.text.clone()).or_else(|| {
            let project = self.project.as_ref()?;
            std::fs::read_to_string(project.folder.join(path)).ok()
        })
    }
    /// Replaces the text of the stylesheet at `path` with `updated`. It goes
    /// in the stylesheet's tab (opened in the background), unsaved like any
    /// typed change, and the viewport is restyled with it at once.
    pub(super) fn set_stylesheet_text(&mut self, path: &str, updated: &str) {
        if self.tabs.get(path).is_none() {
            let saved = self.stylesheet_text(path).unwrap_or_default();
            self.tabs.open_in_background(path, &saved);
        }
        if let Some(tab) = self.tabs.get_mut(path) {
            tab.text = updated.to_owned();
        }
        self.refresh_stylesheet(path, updated);
    }
    /// Does what was done in the Style section for the UI element `id`.
    pub(super) fn apply_style(&mut self, id: InstanceId, edit: StyleEdit) {
        if self.playing() {
            return;
        }
        match edit {
            StyleEdit::Set { path, span, value } => {
                let Some(source) = self.stylesheet_text(&path) else {
                    self.output.error(format!(
                        "Could not edit stylesheet `{path}`: its file could not be read."
                    ));
                    return;
                };
                let edits = [CssEdit {
                    path: path.clone(),
                    span,
                    value,
                }];
                let updated = apply_css_edits(&source, edits.iter());
                self.set_stylesheet_text(&path, &updated);
                self.record_style(
                    id,
                    history::snapshot(&self.world, id),
                    vec![history::TextChange {
                        path,
                        before: Some(source),
                        after: Some(updated),
                    }],
                );
            }
            StyleEdit::Open { path, line } => {
                let source = self.stylesheet_text(&path).unwrap_or_default();
                self.tabs.open(&path, &source, Some(line));
            }
            StyleEdit::Add { property, value } => self.add_style(id, &property, &value),
        }
    }
    /// Adds `property: value` to the rule for `id`'s own `Id`, giving it an
    /// `Id` (from its name) if it has none. The rule is the first one with
    /// that selector, or a new one at the end of the first stylesheet; a
    /// project with no stylesheet gets `ui.css`. The new `Id` and the
    /// stylesheet's text are one undo step.
    pub(super) fn add_style(&mut self, id: InstanceId, property: &str, value: &str) {
        self.add_style_to(id, "", property, value);
    }
    /// [`Self::add_style`], into the rule for `#Id` followed by
    /// `pseudo_class` (such as `:hover`, for a state's rule).
    pub(super) fn add_style_to(
        &mut self,
        id: InstanceId,
        pseudo_class: &str,
        property: &str,
        value: &str,
    ) {
        let Some(instance) = self.world.get(id) else {
            return;
        };
        if self.project.is_none() {
            return;
        }
        let before = history::snapshot(&self.world, id);
        let mut element_id = instance
            .property("Id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if element_id.is_empty() {
            element_id = style_panel::id_from_name(instance.name());
            let edit = Edit::Property("Id", Value::String(element_id.clone()));
            if let Err(err) = set(&mut self.world, id, edit) {
                self.output.error(format!("Could not change that: {err:#}"));
            }
        }
        let selector = format!("{}{pseudo_class}", style_panel::own_selector(&element_id));
        let text = self.add_style_text(&selector, property, value);
        self.history.seal();
        self.record_style(id, before, text.into_iter().collect());
        self.history.seal();
    }
    /// Adds `property: value` to `selector`'s rule (see [`Self::add_style`])
    /// and returns the stylesheet's text before and after. `before` is
    /// `None` when the stylesheet had to be made.
    fn add_style_text(
        &mut self,
        selector: &str,
        property: &str,
        value: &str,
    ) -> Option<history::TextChange> {
        let project = self.project.as_ref()?;
        let target = project
            .stylesheets
            .iter()
            .find_map(|sheet| Some((sheet.path().to_owned(), sheet.rule_end(selector)?)))
            .map(|(path, end)| (path, Some(end)))
            .or_else(|| {
                project
                    .stylesheets
                    .first()
                    .map(|sheet| (sheet.path().to_owned(), None))
            });
        match target {
            Some((path, end)) => {
                let source = self.stylesheet_text(&path)?;
                let updated = style_panel::add_declaration(&source, end, selector, property, value);
                self.set_stylesheet_text(&path, &updated);
                self.output.info(format!(
                    "Added `{property}: {value}` to `{selector}` in `{path}`."
                ));
                Some(history::TextChange {
                    path,
                    before: Some(source),
                    after: Some(updated),
                })
            }
            None => {
                let folder = project.folder.clone();
                let source = style_panel::add_declaration("", None, selector, property, value);
                match axcenity_project::create_stylesheet(&folder, "ui", &source) {
                    Ok(path) => {
                        if let Some(project) = &mut self.project {
                            project
                                .stylesheet_texts
                                .insert(path.clone(), source.clone());
                        }
                        self.refresh_stylesheet(&path, &source);
                        self.tabs.open_in_background(&path, &source);
                        self.output.info(format!(
                            "Made `{path}` for this project's UI styles, with `{selector}` in it."
                        ));
                        Some(history::TextChange {
                            path,
                            before: None,
                            after: Some(source),
                        })
                    }
                    Err(err) => {
                        self.output
                            .error(format!("Could not make a stylesheet: {err}"));
                        None
                    }
                }
            }
        }
    }
    /// Records a change to `id`'s styles: its fields since `before` and the
    /// stylesheets in `texts`. Changes made one after another while the
    /// same value is being typed merge into one step.
    fn record_style(
        &mut self,
        id: InstanceId,
        before: history::Snapshot,
        texts: Vec<history::TextChange>,
    ) {
        let after = history::snapshot(&self.world, id);
        let name = self
            .world
            .get(id)
            .map_or_else(String::new, |instance| instance.name().to_owned());
        self.history.record_with_texts(
            &mut self.world,
            id,
            Field::Property("Style"),
            format!("Change style of {name}"),
            before,
            after,
            texts,
        );
    }
    /// Takes away the stylesheet at `path`, which undo found was made by the
    /// step it undid: its file, its tab and its styles.
    fn remove_stylesheet(&mut self, folder: &Path, path: &str) {
        if let Err(err) = std::fs::remove_file(folder.join(path))
            && err.kind() != std::io::ErrorKind::NotFound
        {
            self.output
                .error(format!("Could not remove `{path}`: {err}"));
        }
        self.tabs.close(path);
        if let Some(project) = &mut self.project {
            project.stylesheet_texts.remove(path);
            project.stylesheets.retain(|sheet| sheet.path() != path);
        }
    }
    pub(super) fn refresh_stylesheet(&mut self, path: &str, source: &str) {
        let (stylesheet, warnings) = axcenity_ui::parse_stylesheet(path, source);
        let Some(project) = &mut self.project else {
            return;
        };
        match project
            .stylesheets
            .iter_mut()
            .find(|existing| existing.path() == path)
        {
            Some(existing) => *existing = stylesheet,
            None => {
                let index = project
                    .stylesheets
                    .partition_point(|existing| existing.path() < path);
                project.stylesheets.insert(index, stylesheet);
            }
        }
        for warning in warnings {
            self.output.warning(format!(
                "Stylesheet `{}`: {}",
                warning.path, warning.message
            ));
        }
    }
    /// Makes `edits` to `id` and records them in the history. `key` is the
    /// field being edited, so a drag or typing keeps adding to one undo step.
    /// Only real changes are recorded: typing the same value again isn't one.
    ///
    /// During Play the panels show the game's world, so the edits go there
    /// instead: the game's scripts see them (`Changed` fires), there's no
    /// undo step, and Stop throws them away with the rest of the game.
    pub(super) fn change(&mut self, id: InstanceId, key: Field, label: String, edits: Vec<Edit>) {
        if let Some(session) = &mut self.play {
            for edit in edits {
                if let Err(err) = set(session.world_mut(), id, edit) {
                    self.output.error(format!("Could not change that: {err:#}"));
                }
            }
            return;
        }
        let before = history::snapshot(&self.world, id);
        for edit in edits {
            if let Err(err) = set(&mut self.world, id, edit) {
                // Can't happen from the panel or the gizmos, which only make valid values.
                self.output.error(format!("Could not change that: {err:#}"));
            }
        }
        let after = history::snapshot(&self.world, id);
        self.history
            .record(&mut self.world, id, key, label, before, after);
    }
    /// Like [`change`](Self::change), for edits to several Instances that
    /// make one undo step, such as painting the whole selection.
    pub(super) fn change_many(
        &mut self,
        key: Field,
        label: String,
        edits: Vec<(InstanceId, Vec<Edit>)>,
    ) {
        if let Some(session) = &mut self.play {
            for (id, edits) in edits {
                for edit in edits {
                    if let Err(err) = set(session.world_mut(), id, edit) {
                        self.output.error(format!("Could not change that: {err:#}"));
                    }
                }
            }
            return;
        }
        let mut changed = Vec::new();
        for (id, edits) in edits {
            let before = history::snapshot(&self.world, id);
            for edit in edits {
                if let Err(err) = set(&mut self.world, id, edit) {
                    self.output.error(format!("Could not change that: {err:#}"));
                }
            }
            let after = history::snapshot(&self.world, id);
            changed.push(history::Changed { id, before, after });
        }
        self.history
            .record_many(&mut self.world, key, label, changed);
    }
    /// Undoes the last step, unless a handle is being dragged or the game
    /// is playing (undo would change a world that isn't on screen).
    pub(super) fn undo(&mut self) {
        if !self.viewport.is_dragging() && !self.ui_designer.is_dragging() && !self.playing() {
            if let Some(restored) = self.history.undo(&mut self.world) {
                self.apply_history_texts(restored.texts);
                self.write_package_files(&restored.blobs);
            }
            self.forget_selection_outside_tree();
        }
    }
    pub(super) fn redo(&mut self) {
        if !self.viewport.is_dragging() && !self.ui_designer.is_dragging() && !self.playing() {
            if let Some(restored) = self.history.redo(&mut self.world) {
                self.apply_history_texts(restored.texts);
                self.write_package_files(&restored.blobs);
            }
            self.forget_selection_outside_tree();
        }
    }

    /// Writes restored file text from undo/redo (property clips, stylesheets,
    /// and clip, controller and prefab files).
    fn apply_history_texts(&mut self, texts: Vec<crate::history::TextChange>) {
        let Some(project) = self.project.as_ref().map(|p| p.folder.clone()) else {
            return;
        };
        let mut files = Vec::new();
        for text in texts {
            if axcenity_project::is_stylesheet(&text.path) {
                match text.after {
                    Some(updated) => self.set_stylesheet_text(&text.path, &updated),
                    None => self.remove_stylesheet(&project, &text.path),
                }
            } else {
                files.push(crate::animation::store::FileDiff {
                    path: text.path,
                    before: text.before,
                    after: text.after,
                });
            }
        }
        if files.is_empty() {
            return;
        }
        if let Err(err) = crate::animation::store::apply_file_diffs(&project, &files) {
            self.output
                .error(format!("Could not restore a project file: {err}"));
        }
        self.animation.sync_after_history(
            &project,
            &self.assets,
            &mut self.world,
            &files,
            &mut self.output,
        );
        self.asset_browser.refresh();
    }

    /// Records clip, controller or prefab file changes as one undo step.
    /// `rebind_controller` updates every Animator's `Controller` from the old
    /// path to the new one in that same step.
    pub(super) fn record_asset_files(
        &mut self,
        label: String,
        files: Vec<crate::animation::store::FileDiff>,
        rebind_controller: Option<(String, String)>,
    ) {
        let mut properties = Vec::new();
        if let Some((from, to)) = rebind_controller {
            for id in self.world.descendants(self.world.root()) {
                let Some(instance) = self.world.get(id) else {
                    continue;
                };
                if instance.class() != ClassName::Animator {
                    continue;
                }
                let Some(current) = instance.property("StateMachine").and_then(Value::as_str)
                else {
                    continue;
                };
                if current != from {
                    continue;
                }
                let before = Value::String(from.clone());
                let after = Value::String(to.clone());
                if self
                    .world
                    .set_property(id, "StateMachine", after.clone())
                    .is_err()
                {
                    continue;
                }
                properties.push(history::PropertyEdit {
                    id,
                    field: Field::Property("StateMachine"),
                    before,
                    after,
                });
            }
        }
        let texts = files
            .into_iter()
            .map(|file| history::TextChange {
                path: file.path,
                before: file.before,
                after: file.after,
            })
            .collect();
        self.history
            .record_asset(&mut self.world, label, texts, properties);
        self.asset_browser.refresh();
        self.animation.refresh();
    }
    /// Clears the selection if it was taken out of the tree (deleted, or an
    /// insert undone), so nothing edits an Instance that isn't in the world.
    /// After Stop, that includes anything the game made.
    pub(super) fn forget_selection_outside_tree(&mut self) {
        let world = shown(&self.world, &self.play);
        let root = world.root();
        let in_tree = |id: InstanceId| id == root || world.is_descendant_of(id, root);
        self.selection.retain(|id| in_tree(*id));
        match self.selected {
            Some(id) if in_tree(id) => {
                // Keep the primary pick even when `selection` was left empty
                // (some tests and call sites set `selected` alone).
                if !self.selection.contains(&id) {
                    self.selection.push(id);
                }
            }
            _ => {
                self.selected = self.selection.last().copied();
            }
        }
    }
}

/// Makes one edit to `world`, without recording it.
fn set(world: &mut World, id: InstanceId, edit: Edit) -> anyhow::Result<()> {
    let Some(instance) = world.get(id) else {
        return Ok(());
    };
    match edit {
        Edit::Name(name) => {
            if properties::can_rename_here(instance) {
                world.set_name(id, name)?;
            }
        }
        Edit::Property(name, value) => {
            // Setting a Material resets Metalness and Roughness, so
            // picking the material it already has must do nothing.
            if instance.property(name) != Some(&value) {
                world.set_property(id, name, value)?;
            }
        }
        // Turned into a property edit, or done, by `apply`.
        Edit::PickTexture(_)
        | Edit::PickAudio
        | Edit::PickController
        | Edit::OpenController
        | Edit::PickClip
        | Edit::OpenClip
        | Edit::Animate
        | Edit::PreviewSound(_) => {}
    }
    Ok(())
}
