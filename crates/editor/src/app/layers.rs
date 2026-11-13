//! The Layers panel's actions (roadmap R16.2, `docs/editor/layers.md`):
//! making, renaming and deleting a `DataLayer` (one undo step each, with
//! its tag on every member), putting the selection in or out of one, and
//! this person's view of which layers are hidden or locked (kept in
//! `.axcenity/layers.json`, never an undo step).

use super::*;
use crate::layers::{self, LayerAction, LayerView};

impl EditorApp {
    pub(in crate::app) fn layers_pane(&mut self, ui: &mut egui::Ui) {
        let editable = !self.playing();
        let rows = layers::rows(&self.world, &self.layer_view);
        let selected = if editable { self.selection.len() } else { 0 };
        if let Some(action) = self.layers.show(ui, &rows, selected, editable) {
            self.layer_action(action);
            ui.ctx().request_repaint();
        }
    }

    /// Does what the Layers panel asked.
    pub(in crate::app) fn layer_action(&mut self, action: LayerAction) {
        match action {
            LayerAction::New => self.new_layer(),
            LayerAction::ToggleHidden(name) => {
                let mut view = self.layer_view.clone();
                if !view.hidden.remove(&name) {
                    view.hidden.insert(name);
                }
                self.set_layer_view(view);
            }
            LayerAction::ToggleLocked(name) => {
                let mut view = self.layer_view.clone();
                if !view.locked.remove(&name) {
                    view.locked.insert(name);
                }
                self.set_layer_view(view);
            }
            LayerAction::SelectMembers(name) => self.select_tagged(&name),
            LayerAction::Edit(id) => self.select_many(vec![id]),
            LayerAction::AddSelection(name) => self.tag_selection(&name, true),
            LayerAction::RemoveSelection(name) => self.tag_selection(&name, false),
            LayerAction::Rename(id, new) => self.rename_layer(id, &new),
            LayerAction::Delete(id) => self.delete_layer(id),
        }
    }

    /// Puts `view` in place and keeps it in the project folder.
    pub(in crate::app) fn set_layer_view(&mut self, view: LayerView) {
        self.layer_view = view;
        if let Some(project) = &self.project
            && let Err(err) = self.layer_view.save(&project.folder)
        {
            self.output.warning(format!(
                "Couldn't keep which layers are hidden ({}): {err}",
                layers::VIEW_FILE
            ));
        }
    }

    /// A new layer holding the selection, as one undo step; its name box
    /// opens.
    fn new_layer(&mut self) {
        if self.playing() {
            return;
        }
        let name = layers::unused_name(&self.world);
        let before = self.world.clone();
        let made = (|| -> anyhow::Result<InstanceId> {
            let layer = self.world.create(ClassName::DataLayer)?;
            self.world.set_name(layer, name.clone())?;
            self.world
                .set_parent(layer, Some(self.world.data_layer_service()))?;
            for &id in &self.selection {
                if self.world.is_descendant_of(id, self.world.workspace()) {
                    self.world.add_tag(id, &name)?;
                }
            }
            Ok(layer)
        })();
        match made {
            Ok(layer) => {
                self.history
                    .record_world(&mut self.world, format!("New layer {name}"), before);
                self.layers.start_renaming(layer, &name);
            }
            Err(err) => {
                self.world = before;
                self.output.error(format!("Couldn't make a layer: {err:#}"));
            }
        }
    }

    /// Puts the selection in the layer `name` (`add`) or takes it out, as
    /// one undo step. Only what's in the Workspace can be in a layer.
    fn tag_selection(&mut self, name: &str, add: bool) {
        if self.playing() {
            return;
        }
        let ids: Vec<InstanceId> = self
            .selection
            .iter()
            .copied()
            .filter(|&id| self.world.is_descendant_of(id, self.world.workspace()))
            .collect();
        if ids.is_empty() {
            return;
        }
        let label = if add {
            format!("Add to layer {name}")
        } else {
            format!("Remove from layer {name}")
        };
        self.history.seal();
        self.annotate(&ids, label.clone(), label, |world, id| {
            if add {
                world.add_tag(id, name)?;
            } else {
                world.remove_tag(id, name)?;
            }
            Ok(())
        });
        self.history.seal();
    }

    /// Renames the layer `id` to `new`, and its tag on every member, as one
    /// undo step. Refused if another layer has that name.
    fn rename_layer(&mut self, id: InstanceId, new: &str) {
        let new = new.trim();
        let Some(old) = self.world.get(id).map(|layer| layer.name().to_owned()) else {
            return;
        };
        if self.playing() || new.is_empty() || new == old {
            return;
        }
        if layers::layers(&self.world)
            .iter()
            .any(|layer| layer.id != id && layer.name == new)
        {
            self.output
                .warning(format!("There's already a layer called {new}."));
            return;
        }
        let before = self.world.clone();
        let members = self.world.tagged(&old);
        let renamed = (|| -> anyhow::Result<()> {
            self.world.set_name(id, new)?;
            for member in members {
                let Some(instance) = self.world.get(member) else {
                    continue;
                };
                // The tag keeps its place among the member's tags.
                let mut tags: Vec<String> = Vec::new();
                for tag in instance.tags() {
                    let tag = if *tag == old { new } else { tag.as_str() };
                    if !tags.iter().any(|t| t == tag) {
                        tags.push(tag.to_owned());
                    }
                }
                let annotations = history::Annotations {
                    attributes: instance.attributes().clone(),
                    tags,
                };
                history::put_annotations(&mut self.world, member, &annotations);
            }
            Ok(())
        })();
        match renamed {
            Ok(()) => {
                self.history.record_world(
                    &mut self.world,
                    format!("Rename layer {old} to {new}"),
                    before,
                );
                let mut view = self.layer_view.clone();
                view.rename(&old, new);
                self.set_layer_view(view);
            }
            Err(err) => {
                self.world = before;
                self.output
                    .error(format!("Couldn't rename the layer: {err:#}"));
            }
        }
    }

    /// Deletes the layer `id` and takes its tag off every member, as one
    /// undo step. The members stay.
    fn delete_layer(&mut self, id: InstanceId) {
        let Some(name) = self.world.get(id).map(|layer| layer.name().to_owned()) else {
            return;
        };
        if self.playing() {
            return;
        }
        let before = self.world.clone();
        let deleted = (|| -> anyhow::Result<()> {
            for member in self.world.tagged(&name) {
                self.world.remove_tag(member, &name)?;
            }
            self.world.destroy(id)?;
            Ok(())
        })();
        match deleted {
            Ok(()) => {
                self.history
                    .record_world(&mut self.world, format!("Delete layer {name}"), before);
                self.selection.retain(|&selected| selected != id);
                if self.selected == Some(id) {
                    self.selected = self.selection.last().copied();
                }
            }
            Err(err) => {
                self.world = before;
                self.output
                    .error(format!("Couldn't delete the layer: {err:#}"));
            }
        }
    }

    /// Brings the hidden and locked sets up to date with the world being
    /// edited and hands them to the viewport and the Explorer. During Play
    /// nothing is hidden: the game shows every layer.
    pub(in crate::app) fn update_layer_sets(&mut self) {
        if self.playing() {
            if !self.layer_sets.unpickable.is_empty() {
                self.explorer.hidden.clear();
                self.viewport
                    .set_layer_sets(Default::default(), Default::default());
                // Worked out again after Stop.
                self.layer_sets = Default::default();
            }
            return;
        }
        if self.layer_sets.update(&self.world, &self.layer_view) {
            self.explorer.hidden = self.layer_sets.hidden.clone();
            self.viewport.set_layer_sets(
                self.layer_sets.hidden.clone(),
                self.layer_sets.unpickable.clone(),
            );
        }
    }
}
