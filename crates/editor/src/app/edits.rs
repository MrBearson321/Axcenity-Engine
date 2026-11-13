//! Selection, property edits, transform tools, stylesheets and undo.
//!
//! Every edit-mode change goes through [`EditorApp::change`] so one gesture is
//! one history step. Play-mode property edits go to the session world instead
//! and are not recorded. These are methods of [`super::EditorApp`].

use super::*;
use crate::prefab_menu::PrefabAction;

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
    /// True when anything has changes that aren't saved (see `documents.rs`).
    /// Undoing back to where it was saved counts as no changes.
    pub(super) fn unsaved(&self) -> bool {
        !self.modified_documents().is_empty()
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
            Edit::PickTexture(_)
            | Edit::PickGradingFile(_)
            | Edit::PickAudio
            | Edit::PickController
            | Edit::PickBehaviorTree
            | Edit::PickSourceFile
            | Edit::PickClip => {
                if let Some(Some((name, value))) = self.pick_file_for(&edit) {
                    self.apply(id, Edit::Property(name, value));
                    // Picking is one whole step, not more typing.
                    self.history.seal();
                }
                return;
            }
            &Edit::Reset(name) => {
                let default = instance
                    .class()
                    .default_properties()
                    .into_iter()
                    .find(|(property, _)| *property == name);
                if let Some((_, default)) = default {
                    self.apply(id, Edit::Property(name, default));
                }
                return;
            }
            Edit::OpenController => {
                self.open_animator_controller(id);
                return;
            }
            Edit::PickSkyImage => {
                if let Some(path) = self.pick_sky_image() {
                    self.ask_sky(path);
                }
                return;
            }
            Edit::AskSkyImage(path) => {
                let path = path.clone();
                self.ask_sky(path);
                return;
            }
            Edit::UseDefaultSky => {
                self.ask_default_sky();
                return;
            }
            Edit::UseTimeOfDaySky => {
                self.use_time_of_day_sky();
                return;
            }
            Edit::OpenSourceFile => {
                self.open_script_instance(id);
                return;
            }
            Edit::MoveSourceFile => {
                if let Some(path) = self.world.get(id).and_then(|i| i.source_file()) {
                    let path = path.to_owned();
                    self.layout.open(Pane::Assets);
                    self.asset_browser.start_move(path);
                }
                return;
            }
            Edit::OpenBehaviorTree => {
                if let Some(path) = self
                    .world
                    .get(id)
                    .and_then(|i| i.property("BehaviorTree"))
                    .and_then(Value::as_str)
                {
                    let path = path.to_owned();
                    self.open_behavior_tree(&path);
                }
                return;
            }
            Edit::Animate => {
                self.animate(id);
                return;
            }
            Edit::BuildRagdoll => {
                self.build_ragdoll(id);
                return;
            }
            Edit::RevertOverride(property) => {
                let property = property.clone();
                self.prefab_action(PrefabAction::RevertProperty { id, property });
                return;
            }
            Edit::ApplyOverride(property) => {
                let property = property.clone();
                self.prefab_action(PrefabAction::ApplyProperty { id, property });
                return;
            }
            Edit::GenerateFracture => {
                self.generate_fracture(id);
                return;
            }
            Edit::BakeSpline => {
                self.bake_spline(id);
                return;
            }
            Edit::FlattenTerrainAlong => {
                self.flatten_terrain_along(id);
                return;
            }
            Edit::OpenInModelStudio | Edit::OpenInBlender => {
                let mesh_id = self
                    .world
                    .get(id)
                    .and_then(|instance| instance.property("MeshId"))
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                if let Some(mesh_id) = mesh_id {
                    if matches!(edit, Edit::OpenInBlender) {
                        self.edit_in_blender(&mesh_id);
                    } else {
                        self.open_in_model_studio(&mesh_id);
                    }
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
            &Edit::Keep(field, kept) => {
                if let Some(session) = &mut self.play {
                    session.kept_mut().set_kept(id, field, kept);
                }
                return;
            }
            Edit::Attribute(name, value) => {
                let verb = if value.is_some() { "Set" } else { "Remove" };
                let label = format!("{verb} attribute {name} of {}", instance.name());
                let key = format!("attribute {name} of {id:?}");
                let (name, value) = (name.clone(), value.clone());
                self.annotate(&[id], key, label, |world, id| {
                    world.set_attribute(id, &name, value.clone())?;
                    Ok(())
                });
                return;
            }
            Edit::AddTag(tag) | Edit::RemoveTag(tag) => {
                let adding = matches!(edit, Edit::AddTag(_));
                let verb = if adding { "Tag" } else { "Untag" };
                let label = format!("{verb} {} {tag}", instance.name());
                let tag = tag.clone();
                self.annotate(&[id], label.clone(), label, |world, id| {
                    if adding {
                        world.add_tag(id, &tag)?;
                    } else {
                        world.remove_tag(id, &tag)?;
                    }
                    Ok(())
                });
                // A tag is added or removed with one click: its own step.
                self.history.seal();
                return;
            }
        };
        self.change(id, key, label, vec![edit]);
    }
    /// Asks for the file a picker edit (such as [`Edit::PickTexture`])
    /// wants: the property to set and the path picked. `None` for other
    /// edits, `Some(None)` when nothing was picked.
    fn pick_file_for(&mut self, edit: &Edit) -> Option<Option<(&'static str, Value)>> {
        let picked = match *edit {
            Edit::PickTexture(name) => {
                let folder = match name {
                    "Texture" => crate::asset_browser::PARTICLES_FOLDER,
                    // A UI element's picture, as the Assets panel's Images.
                    "Image" => crate::asset_browser::IMAGES_FOLDER,
                    _ => axcenity_assets::PROJECT_TEXTURES_FOLDER,
                };
                self.pick_texture(folder)
                    .map(|path| (name, self.reference_to(path)))
            }
            Edit::PickGradingFile(name) => self
                .pick_grading_file(name)
                .map(|path| (name, self.reference_to(path))),
            Edit::PickAudio => self
                .pick_audio()
                .map(|path| ("SoundId", self.reference_to(path))),
            Edit::PickController => self.pick_controller().map(|path| ("StateMachine", path)),
            Edit::PickBehaviorTree => self.pick_behavior_tree().map(|path| ("BehaviorTree", path)),
            Edit::PickSourceFile => self.pick_source_file().map(|path| ("SourceFile", path)),
            Edit::PickClip => self.pick_property_clip().map(|path| ("Clip", path)),
            _ => return None,
        };
        Some(picked.map(|(name, path)| (name, Value::String(path))))
    }
    /// Applies an edit made in Properties with several Instances selected
    /// (the last is the one clicked last) to every one of them that can
    /// take it, as one undo step: a name to those that can be renamed, a
    /// value to those that have the property, ↺ to each one's own default.
    /// Locked ones only take a change to Locked. Buttons (Open, Animate)
    /// act on the last one, as with one selected.
    pub(super) fn apply_many(&mut self, ids: &[InstanceId], edit: Edit) {
        if let Some(picked) = self.pick_file_for(&edit) {
            if let Some((name, value)) = picked {
                self.apply_many(ids, Edit::Property(name, value));
                self.history.seal();
            }
            return;
        }
        let world = shown(&self.world, &self.play);
        let playing = self.play.is_some();
        let can_change =
            |id: InstanceId, name: &str| playing || name == "Locked" || !world.is_locked(id);
        let (key, label, edits): (Field, String, Vec<(InstanceId, Vec<Edit>)>) = match &edit {
            Edit::Name(name) => {
                let edits: Vec<_> = ids
                    .iter()
                    .filter(|&&id| {
                        world
                            .get(id)
                            .is_some_and(crate::properties::can_rename_here)
                            && can_change(id, "Name")
                    })
                    .map(|&id| (id, vec![Edit::Name(name.clone())]))
                    .collect();
                let targets: Vec<_> = edits.iter().map(|(id, _)| *id).collect();
                let label = format!("Rename {}", crate::properties::count_name(world, &targets));
                (Field::Name, label, edits)
            }
            Edit::Property(name, value) => {
                let edits: Vec<_> = ids
                    .iter()
                    .filter(|&&id| {
                        world.get(id).is_some_and(|i| i.property(name).is_some())
                            && can_change(id, name)
                    })
                    .map(|&id| (id, vec![Edit::Property(name, value.clone())]))
                    .collect();
                let targets: Vec<_> = edits.iter().map(|(id, _)| *id).collect();
                let label = format!(
                    "Change {name} of {}",
                    crate::properties::count_name(world, &targets)
                );
                (Field::Property(name), label, edits)
            }
            &Edit::Reset(name) => {
                let edits: Vec<_> = ids
                    .iter()
                    .filter(|&&id| can_change(id, name))
                    .filter_map(|&id| {
                        let instance = world.get(id)?;
                        let (_, default) = instance
                            .class()
                            .default_properties()
                            .into_iter()
                            .find(|(property, _)| *property == name)?;
                        (instance.property(name)? != &default)
                            .then(|| (id, vec![Edit::Property(name, default)]))
                    })
                    .collect();
                let targets: Vec<_> = edits.iter().map(|(id, _)| *id).collect();
                let label = format!(
                    "Reset {name} of {}",
                    crate::properties::count_name(world, &targets)
                );
                (Field::Property(name), label, edits)
            }
            _ => {
                if let Some(&last) = ids.last() {
                    self.apply(last, edit);
                }
                return;
            }
        };
        if edits.is_empty() {
            return;
        }
        let reset = matches!(edit, Edit::Reset(_));
        self.change_many(key, label, edits);
        if reset {
            // ↺ is one click: its own step.
            self.history.seal();
        }
    }
    /// Changes the attributes or tags of `ids` with `change`, as one undo
    /// step named `label` (`key` merges a drag into one step). During Play
    /// it changes the game's world instead, with no undo step.
    pub(super) fn annotate(
        &mut self,
        ids: &[InstanceId],
        key: String,
        label: String,
        change: impl Fn(&mut World, InstanceId) -> anyhow::Result<()>,
    ) {
        if let Some(session) = &mut self.play {
            for &id in ids {
                if let Err(err) = change(session.world_mut(), id) {
                    self.output.error(format!("Could not change that: {err:#}"));
                }
            }
            return;
        }
        let before: Vec<_> = ids
            .iter()
            .map(|&id| (id, history::annotations(&self.world, id)))
            .collect();
        for &id in ids {
            if let Err(err) = change(&mut self.world, id) {
                self.output.error(format!("Could not change that: {err:#}"));
            }
        }
        self.history
            .record_annotations(&mut self.world, key, label, before);
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
            ToolAction::UseBuiltIn(material) => self.use_built_in_material(&parts, material),
            ToolAction::UsePrototype { palette, number } => {
                self.use_prototype(parts, palette, number);
            }
            ToolAction::UseMaterial { path, name } => self.use_material(&path, &name, parts),
            ToolAction::NewCustomMaterial => self.custom_material = Some(Default::default()),
            ToolAction::LoadMaterial => self.load_material_dialog(parts),
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
            ToolAction::MoveBy(by) => {
                let moved = parts
                    .iter()
                    .filter_map(|&id| {
                        let at = self.world.get(id)?.property("Position")?.as_vector3()?;
                        let at = axcenity_core::Vector3::new(at.x + by.x, at.y + by.y, at.z + by.z);
                        Some((id, vec![("Position", Value::Vector3(at))]))
                    })
                    .collect();
                self.apply_drag(DragEdit {
                    kind: DragKind::Move,
                    parts: moved,
                });
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
                .is_some_and(|i| i.class().is_base_part())
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
        let materials: Vec<String> = if self.tool == Tool::Material {
            self.asset_browser
                .entries()
                .into_iter()
                .filter(|entry| entry.kind == crate::asset_browser::AssetKind::Material)
                .map(|entry| entry.path)
                .collect()
        } else {
            Vec::new()
        };
        let scene = PanelScene {
            world: &self.world,
            parts: &parts,
            materials: &materials,
        };
        let (tool, settings, modes, snap) =
            (self.tool, &mut self.build, &mut self.modes, &mut self.snap);
        let actions = egui::Area::new(egui::Id::new("building tool panel"))
            .fixed_pos(viewport.min + egui::vec2(10.0, 10.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .corner_radius(0)
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
        let texts = self.drag_css_texts(edit.css);
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
    /// Rewrites stylesheets for a UI drag's `css` edits, each against the
    /// text the stylesheet had when the drag started (kept until it ends),
    /// and returns their text before the drag and now.
    pub(super) fn drag_css_texts(&mut self, css: Vec<CssEdit>) -> Vec<history::TextChange> {
        let mut texts = Vec::new();
        let mut edits_by_path: HashMap<String, Vec<CssEdit>> = HashMap::new();
        for css_edit in css {
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
        texts
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
        self.history.seal();
        self.add_style_unsealed(id, pseudo_class, property, value);
        self.history.seal();
    }
    /// [`Self::add_style_to`], as part of the gesture under way: a drag,
    /// or several edits that make one step.
    pub(super) fn add_style_unsealed(
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
        let text = self.add_style_text(&selector, property, value, true);
        self.record_style(id, before, text.into_iter().collect());
    }
    /// Adds `property: value` to `selector`'s rule (see [`Self::add_style`])
    /// and returns the stylesheet's text before and after. `before` is
    /// `None` when the stylesheet had to be made. `report` says what was
    /// added in Output (making a stylesheet always is).
    pub(super) fn add_style_text(
        &mut self,
        selector: &str,
        property: &str,
        value: &str,
        report: bool,
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
                if report {
                    self.output.info(format!(
                        "Added `{property}: {value}` to `{selector}` in `{path}`."
                    ));
                }
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
    /// undo step, and Stop throws them away with the rest of the game,
    /// unless they're kept (see `play::kept`).
    /// Builds the Ragdoll `id` from its MeshPart's skeleton (Phase 41), as
    /// one undo step, or says why it can't.
    pub(super) fn build_ragdoll(&mut self, id: InstanceId) {
        let before = self.world.clone();
        match axcenity_anim::ragdoll::build(&mut self.world, &self.assets, id) {
            Ok(bodies) => {
                let name = self.world.get(id).map_or("", |r| r.name()).to_owned();
                self.history
                    .record_world(&mut self.world, format!("Build {name}"), before);
                self.output.info(format!(
                    "Built {name}: {bodies} capsules and their joints. Select one to change its Size, or a joint to change its limits."
                ));
            }
            Err(problem) => self.output.warning(problem),
        }
    }

    /// Cuts the part the Fracture `id` is in into its pieces (Phase 42),
    /// saves them, and sets its `PiecesId` as one undo step, or says why
    /// it can't. The file stays when the step is undone.
    pub(super) fn generate_fracture(&mut self, id: InstanceId) {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            self.output
                .warning("Open or save a project first: the pieces are saved in it.");
            return;
        };
        match axcenity_mesh_edit::generate(&self.world, &self.assets, &folder, id) {
            Ok(path) => {
                let before = self.world.clone();
                let _ = self
                    .world
                    .set_property(id, "PiecesId", Value::String(path.clone()));
                let pieces = self.world.get(id).map_or("", |f| f.name()).to_owned();
                self.history
                    .record_world(&mut self.world, format!("Generate {pieces}"), before);
                self.output.info(format!("Saved the pieces in {path}."));
            }
            Err(problem) => self.output.warning(problem),
        }
    }

    pub(super) fn change(&mut self, id: InstanceId, key: Field, label: String, edits: Vec<Edit>) {
        if self.play.is_some() {
            self.change_in_play(id, edits);
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
    /// Makes `edits` to `id` in the game's world, and notes each value
    /// they changed, for its Keep pin.
    fn change_in_play(&mut self, id: InstanceId, edits: Vec<Edit>) {
        let Some(session) = &mut self.play else {
            return;
        };
        let before = history::snapshot(session.world(), id);
        for edit in edits {
            if let Err(err) = set(session.world_mut(), id, edit) {
                self.output.error(format!("Could not change that: {err:#}"));
            }
        }
        for (field, value) in history::snapshot(session.world(), id) {
            if !before.contains(&(field, value)) {
                session.kept_mut().edited(id, field);
            }
        }
    }
    /// Like [`change`](Self::change), for edits to several Instances that
    /// make one undo step, such as painting the whole selection.
    pub(super) fn change_many(
        &mut self,
        key: Field,
        label: String,
        edits: Vec<(InstanceId, Vec<Edit>)>,
    ) {
        if self.play.is_some() {
            for (id, edits) in edits {
                self.change_in_play(id, edits);
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
        // Over a block script, Ctrl+Z undoes in its blocks.
        if self.step_block_script(true) {
            return;
        }
        // Over the Event Graph, Ctrl+Z undoes in the graph.
        if self.ui_designer.graph_has_pointer() && !self.playing() && self.undo_graph() {
            return;
        }
        if self.behavior.has_pointer() && !self.playing() && self.behavior.undo() {
            return;
        }
        if self.dialogues.has_pointer() && !self.playing() && self.dialogues.undo() {
            return;
        }
        // Tables are edited during Play too, so their undo works then.
        if self.data_tables.has_pointer() && self.data_tables.undo() {
            self.hand_over_tables();
            return;
        }
        // So are materials: Play draws each edit.
        if self.materials.has_pointer() && self.materials.undo() {
            self.hand_over_materials();
            return;
        }
        // The Model Studio has its own history while the mouse is over it.
        if self.model_studio.has_pointer() && self.model_studio.undo() {
            return;
        }
        if !self.viewport.is_dragging() && !self.ui_designer.is_dragging() && !self.playing() {
            if self.team_refuses_world_swap(false) {
                return;
            }
            if let Some(restored) = self.history.undo(&mut self.world) {
                self.apply_history_texts(restored.texts);
                self.write_package_files(&restored.blobs);
                self.report_skipped(restored.skipped);
            }
            self.forget_selection_outside_tree();
            // A spline put back makes its road or scatter again at once.
            self.generate_frame();
        }
    }
    /// Undoes (`jump` below 0) or redoes that many steps, one at a time as
    /// Ctrl+Z and Ctrl+Y would, stopping early if one can't be done.
    pub(in crate::app) fn jump_in_history(&mut self, jump: isize) {
        for _ in 0..jump.unsigned_abs() {
            let before = (
                self.history.undo_labels().len(),
                self.history.redo_labels().len(),
            );
            if jump < 0 {
                self.undo();
            } else {
                self.redo();
            }
            let after = (
                self.history.undo_labels().len(),
                self.history.redo_labels().len(),
            );
            if after == before {
                break;
            }
        }
    }

    pub(super) fn redo(&mut self) {
        if self.step_block_script(false) {
            return;
        }
        if self.ui_designer.graph_has_pointer() && !self.playing() && self.redo_graph() {
            return;
        }
        if self.behavior.has_pointer() && !self.playing() && self.behavior.redo() {
            return;
        }
        if self.dialogues.has_pointer() && !self.playing() && self.dialogues.redo() {
            return;
        }
        if self.data_tables.has_pointer() && self.data_tables.redo() {
            self.hand_over_tables();
            return;
        }
        if self.materials.has_pointer() && self.materials.redo() {
            self.hand_over_materials();
            return;
        }
        if self.model_studio.has_pointer() && self.model_studio.redo() {
            return;
        }
        if !self.viewport.is_dragging() && !self.ui_designer.is_dragging() && !self.playing() {
            if self.team_refuses_world_swap(true) {
                return;
            }
            if let Some(restored) = self.history.redo(&mut self.world) {
                self.apply_history_texts(restored.texts);
                self.write_package_files(&restored.blobs);
                self.report_skipped(restored.skipped);
            }
            self.forget_selection_outside_tree();
            self.generate_frame();
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
        // A script attached to an object, or put back in its service.
        self.follow_service_script_files(&files);
        // Scripts inside objects' files (G2): their code as the file now is.
        for file in &files {
            if self.is_object_file(&file.path) {
                if axcenity_project::is_block_script(&file.path) {
                    // Its Lua is what its blocks translate to.
                    if let Some(text) = &file.after {
                        if let Some(project) = &mut self.project {
                            project.object_files.entry(file.path.clone()).or_default();
                        }
                        self.take_script_text(&file.path, text);
                    } else if let Some(project) = &mut self.project {
                        project.object_files.remove(&file.path);
                    }
                    continue;
                }
                if let Some(project) = &mut self.project {
                    match &file.after {
                        Some(text) => {
                            project.object_files.insert(file.path.clone(), text.clone());
                        }
                        None => {
                            project.object_files.remove(&file.path);
                        }
                    }
                }
                self.sync_tab(&file.path, file.after.as_deref());
            }
        }
        // The Input and Collision Groups windows' files, read again.
        self.reload_project_settings(&files);
        // A level's start or name changed back or again (Phase 48).
        if files
            .iter()
            .any(|file| file.path == axcenity_project::MANIFEST_FILE)
        {
            self.follow_start_level();
        }
        self.sync_sequence(&files);
        self.animation.sync_after_history(
            &project,
            &self.assets,
            &mut self.world,
            &files,
            &mut self.output,
        );
        self.asset_browser.refresh();
    }

    /// After undo or redo wrote `input.json` or `collision-groups.json`
    /// back, the project's input map or the world's collision groups as
    /// the file now says (no file is the default).
    fn reload_project_settings(&mut self, files: &[crate::animation::store::FileDiff]) {
        for file in files {
            if file.path == axcenity_project::INPUT_FILE
                && let Some(project) = &mut self.project
            {
                project.input_map = match file.after.as_deref() {
                    None => Default::default(),
                    Some(text) => match axcenity_project::parse_input_map(text) {
                        Ok(map) => map,
                        Err(err) => {
                            self.output.error(format!("Input: {err}."));
                            continue;
                        }
                    },
                };
            } else if file.path == axcenity_project::COLLISION_GROUPS_FILE {
                let groups = match file.after.as_deref() {
                    None => Default::default(),
                    Some(text) => match axcenity_project::parse_collision_groups(text) {
                        Ok(groups) => groups,
                        Err(err) => {
                            self.output.error(format!("Collision groups: {err}."));
                            continue;
                        }
                    },
                };
                self.world.set_collision_groups(groups);
            }
        }
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
pub(super) fn set(world: &mut World, id: InstanceId, edit: Edit) -> anyhow::Result<()> {
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
        | Edit::PickGradingFile(_)
        | Edit::PickAudio
        | Edit::PickController
        | Edit::OpenController
        | Edit::PickBehaviorTree
        | Edit::PickSourceFile
        | Edit::OpenSourceFile
        | Edit::MoveSourceFile
        | Edit::OpenBehaviorTree
        | Edit::PickSkyImage
        | Edit::AskSkyImage(_)
        | Edit::UseDefaultSky
        | Edit::UseTimeOfDaySky
        | Edit::PickClip
        | Edit::OpenClip
        | Edit::Animate
        | Edit::BuildRagdoll
        | Edit::RevertOverride(_)
        | Edit::ApplyOverride(_)
        | Edit::GenerateFracture
        | Edit::BakeSpline
        | Edit::FlattenTerrainAlong
        | Edit::OpenInModelStudio
        | Edit::OpenInBlender
        | Edit::PreviewSound(_)
        | Edit::Keep(..)
        | Edit::Attribute(..)
        | Edit::AddTag(_)
        | Edit::RemoveTag(_)
        | Edit::Reset(_) => {}
    }
    Ok(())
}
