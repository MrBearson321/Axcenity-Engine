//! Data layers in the editor (roadmap R16.2, `docs/editor/layers.md`).
//!
//! A layer is a `DataLayer` in the `DataLayerService`; its members are the
//! Instances tagged with its name, and everything inside them. Whether a
//! layer is shown and whether a click can pick it is this person's view
//! ([`LayerView`], kept in `.axcenity/layers.json`): it never changes the
//! world or the undo history, and Play shows every layer. The app turns the
//! view into sets of Instances ([`LayerSets`]) that the viewport leaves out
//! of drawing and picking and the Explorer dims. The Layers panel
//! ([`LayersPanel`]) lists the layers; its edits are the app's
//! (`app/layers.rs`).
//!
//! Regions (roadmap R16.3) are the Workspace's top-level Folders and
//! Models. This person can unload one in the editor ([`LayerView::unloaded`],
//! by name): it isn't drawn or picked while editing, but it stays in the
//! world and is saved unchanged. The same panel lists them ([`regions`]).

use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use axcenity_core::{ClassName, Color3, InstanceId, Value, World};
use eframe::egui::{self, RichText};
use serde::{Deserialize, Serialize};

use crate::theme::{self, icons};
use crate::widgets;

/// Where a project keeps this computer's layer view.
pub const VIEW_FILE: &str = ".axcenity/layers.json";

/// Which layers this person hides and locks, and which regions they
/// unloaded, by name.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayerView {
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub hidden: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub locked: BTreeSet<String>,
    /// Regions (top-level Folders and Models) unloaded in the editor.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub unloaded: BTreeSet<String>,
}

impl LayerView {
    /// The view kept in `project`, or none hidden or locked when there's
    /// no file or it can't be read.
    pub fn load(project: &Path) -> LayerView {
        std::fs::read(project.join(VIEW_FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Keeps the view in `project` (removing the file once nothing is
    /// hidden or locked).
    pub fn save(&self, project: &Path) -> std::io::Result<()> {
        let path = project.join(VIEW_FILE);
        if self.hidden.is_empty() && self.locked.is_empty() && self.unloaded.is_empty() {
            return match std::fs::remove_file(&path) {
                Err(err) if err.kind() != std::io::ErrorKind::NotFound => Err(err),
                _ => Ok(()),
            };
        }
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let json = serde_json::to_vec_pretty(self).expect("plain data serializes");
        std::fs::write(path, json)
    }

    /// Takes `old`'s settings over to `new`, after a rename.
    pub fn rename(&mut self, old: &str, new: &str) {
        for names in [&mut self.hidden, &mut self.locked] {
            if names.remove(old) {
                names.insert(new.to_owned());
            }
        }
    }
}

/// One layer: its object, name and colour.
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub id: InstanceId,
    pub name: String,
    pub color: Color3,
}

/// The `DataLayer`s in `world`'s DataLayerService, in order.
pub fn layers(world: &World) -> Vec<Layer> {
    let Some(service) = world.get(world.data_layer_service()) else {
        return Vec::new();
    };
    service
        .children()
        .iter()
        .filter_map(|&id| {
            let layer = world.get(id)?;
            (layer.class() == ClassName::DataLayer).then(|| Layer {
                id,
                name: layer.name().to_owned(),
                color: match layer.property("Color") {
                    Some(Value::Color3(color)) => *color,
                    _ => Color3::new(1.0, 1.0, 1.0),
                },
            })
        })
        .collect()
}

/// A layer name no layer in `world` has: `Layer`, then `Layer 2`, …
pub fn unused_name(world: &World) -> String {
    let taken: Vec<String> = layers(world).into_iter().map(|layer| layer.name).collect();
    (1..)
        .map(|n| match n {
            1 => "Layer".to_owned(),
            n => format!("Layer {n}"),
        })
        .find(|name| !taken.contains(name))
        .expect("an unused name")
}

/// The Instances tagged with one of `names`, and everything inside them.
pub fn covered<'a>(world: &World, names: impl IntoIterator<Item = &'a str>) -> HashSet<InstanceId> {
    let mut ids = HashSet::new();
    for name in names {
        for tagged in world.tagged_iter(name) {
            if ids.insert(tagged) {
                ids.extend(world.descendants_iter(tagged));
            }
        }
    }
    ids
}

/// One region: a top-level Folder or Model of the Workspace.
#[derive(Clone, Debug, PartialEq)]
pub struct Region {
    pub id: InstanceId,
    pub name: String,
    pub class: ClassName,
    /// How many parts are inside it.
    pub parts: usize,
    pub unloaded: bool,
}

/// The Workspace's regions, in order, as `view` sees them.
pub fn regions(world: &World, view: &LayerView) -> Vec<Region> {
    region_ids(world)
        .filter_map(|id| {
            let instance = world.get(id)?;
            Some(Region {
                id,
                name: instance.name().to_owned(),
                class: instance.class(),
                parts: world
                    .descendants_iter(id)
                    .filter(|&inside| world.get(inside).is_some_and(|i| i.class().is_part()))
                    .count(),
                unloaded: view.unloaded.contains(instance.name()),
            })
        })
        .collect()
}

/// The Workspace's top-level Folders and Models.
fn region_ids(world: &World) -> impl Iterator<Item = InstanceId> + '_ {
    world
        .get(world.workspace())
        .map(|workspace| workspace.children().to_vec())
        .unwrap_or_default()
        .into_iter()
        .filter(|&id| {
            world
                .get(id)
                .is_some_and(|i| matches!(i.class(), ClassName::Folder | ClassName::Model))
        })
}

/// The regions named in `names`, and everything inside them. Two regions
/// with the same name go together.
fn unloaded_regions(world: &World, names: &BTreeSet<String>) -> Vec<InstanceId> {
    if names.is_empty() {
        return Vec::new();
    }
    region_ids(world)
        .filter(|&id| world.get(id).is_some_and(|i| names.contains(i.name())))
        .collect()
}

/// What the view hides and locks, as Instances, worked out again only when
/// the tree, a layer's tagged Instances or the view change.
#[derive(Default)]
pub struct LayerSets {
    pub hidden: HashSet<InstanceId>,
    /// Locked or hidden: what a click goes through.
    pub unpickable: HashSet<InstanceId>,
    /// What the sets were made from: the tree, the view, each hidden or
    /// locked layer's tagged Instances and the unloaded regions.
    key: Option<(u64, LayerView, Vec<Vec<InstanceId>>, Vec<InstanceId>)>,
}

impl LayerSets {
    /// Catches up with `world` and `view`. True if the sets changed.
    pub fn update(&mut self, world: &World, view: &LayerView) -> bool {
        // Only layers that exist count: a name left in the view from a
        // deleted layer, or a tag no layer has, hides nothing.
        let names: Vec<String> = layers(world).into_iter().map(|layer| layer.name).collect();
        let view = LayerView {
            hidden: view
                .hidden
                .iter()
                .filter(|n| names.contains(n))
                .cloned()
                .collect(),
            locked: view
                .locked
                .iter()
                .filter(|n| names.contains(n))
                .cloned()
                .collect(),
            unloaded: view.unloaded.clone(),
        };
        let tagged: Vec<Vec<InstanceId>> = view
            .hidden
            .iter()
            .chain(&view.locked)
            .map(|name| world.tagged_iter(name).collect())
            .collect();
        // By id too, as a rename doesn't change the tree.
        let unloaded = unloaded_regions(world, &view.unloaded);
        let key = (world.structure_version(), view, tagged, unloaded);
        if self.key.as_ref() == Some(&key) {
            return false;
        }
        let view = &key.1;
        self.hidden = covered(world, view.hidden.iter().map(String::as_str));
        for &region in &key.3 {
            self.hidden.insert(region);
            self.hidden.extend(world.descendants_iter(region));
        }
        self.unpickable = covered(world, view.locked.iter().map(String::as_str));
        self.unpickable.extend(self.hidden.iter().copied());
        self.key = Some(key);
        true
    }
}

/// What the panel asked the app to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayerAction {
    /// A new layer, holding the selection.
    New,
    /// Show or hide the layer of this name.
    ToggleHidden(String),
    /// Lock or unlock it.
    ToggleLocked(String),
    /// Select its members.
    SelectMembers(String),
    /// Select the layer itself, for its Color and ActiveOnStart.
    Edit(InstanceId),
    /// Put the selection in the layer, or take it out.
    AddSelection(String),
    RemoveSelection(String),
    /// Rename the layer (and its tag on every member).
    Rename(InstanceId, String),
    /// Delete the layer (and its tag from every member).
    Delete(InstanceId),
    /// Unload the region of this name in the editor, or load it again.
    ToggleUnloaded(String),
}

/// One row as the panel shows it.
pub struct LayerRow {
    pub layer: Layer,
    pub members: usize,
    pub hidden: bool,
    pub locked: bool,
}

/// The rows for `world`'s layers in `view`.
pub fn rows(world: &World, view: &LayerView) -> Vec<LayerRow> {
    layers(world)
        .into_iter()
        .map(|layer| LayerRow {
            members: world.tagged_iter(&layer.name).count(),
            hidden: view.hidden.contains(&layer.name),
            locked: view.locked.contains(&layer.name),
            layer,
        })
        .collect()
}

/// The panel's own state: the layer being renamed and the name so far.
#[derive(Default)]
pub struct LayersPanel {
    renaming: Option<(InstanceId, String)>,
}

impl LayersPanel {
    /// Opens the rename box on the layer `id`.
    pub fn start_renaming(&mut self, id: InstanceId, name: &str) {
        self.renaming = Some((id, name.to_owned()));
    }

    /// Draws the panel: the layers, then the regions. `selected` is how
    /// many Instances are selected, for the buttons that use them;
    /// `editable` is false during Play.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        rows: &[LayerRow],
        regions: &[Region],
        selected: usize,
        editable: bool,
    ) -> Option<LayerAction> {
        let palette = theme::palette(ui.ctx());
        let mut action = None;
        ui.add_enabled_ui(editable, |ui| {
            let new = ui
                .button(theme::with_icon(icons::PLUS, "New Layer"))
                .on_hover_text(if selected > 0 {
                    "A new layer holding what's selected"
                } else {
                    "A new, empty layer"
                });
            if new.clicked() {
                action = Some(LayerAction::New);
            }
        });
        ui.add_space(4.0);
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                if rows.is_empty() {
                    widgets::empty_state(
                        ui,
                        icons::STACK_SIMPLE,
                        "No layers",
                        "A layer gathers what's tagged with its name, wherever it is, to hide or lock it while you work.",
                        None,
                    );
                }
                for row in rows {
                    let layer = &row.layer;
                    if let Some((_, name)) = self.renaming.as_mut().filter(|(id, _)| *id == layer.id)
                    {
                        let mut finished = None;
                        ui.horizontal(|ui| {
                            let response = ui.add(
                                egui::TextEdit::singleline(name)
                                    .desired_width(ui.available_width() - 60.0),
                            );
                            response.request_focus();
                            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                            let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
                            if widgets::icon_button(ui, icons::CHECK, "Rename", false).clicked()
                                || enter
                            {
                                finished = Some(Some(name.trim().to_owned()));
                            } else if widgets::icon_button(ui, icons::X, "Cancel", false).clicked()
                                || escape
                            {
                                finished = Some(None);
                            }
                        });
                        match finished {
                            Some(Some(new)) => {
                                if !new.is_empty() && new != layer.name {
                                    action = Some(LayerAction::Rename(layer.id, new));
                                }
                                self.renaming = None;
                            }
                            Some(None) => self.renaming = None,
                            None => {}
                        }
                        continue;
                    }
                    let row_layout = egui::Layout::right_to_left(egui::Align::Center);
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
                        row_layout,
                        |ui| {
                            let more = ui.add_enabled_ui(editable, |ui| {
                                ui.menu_button(icons::DOTS_THREE, |ui| {
                                    let add = format!("Put the selection in {}", layer.name);
                                    if ui
                                        .add_enabled(
                                            selected > 0,
                                            egui::Button::new(theme::menu_item(
                                                ui.ctx(),
                                                icons::PLUS,
                                                "Add Selection",
                                            )),
                                        )
                                        .on_hover_text(add)
                                        .clicked()
                                    {
                                        action = Some(LayerAction::AddSelection(layer.name.clone()));
                                        ui.close();
                                    }
                                    if ui
                                        .add_enabled(
                                            selected > 0,
                                            egui::Button::new(theme::menu_item(
                                                ui.ctx(),
                                                icons::MINUS,
                                                "Remove Selection",
                                            )),
                                        )
                                        .clicked()
                                    {
                                        action =
                                            Some(LayerAction::RemoveSelection(layer.name.clone()));
                                        ui.close();
                                    }
                                    if ui
                                        .button(theme::menu_item(
                                            ui.ctx(),
                                            icons::SLIDERS_HORIZONTAL,
                                            "Layer Settings",
                                        ))
                                        .on_hover_text("Select the layer: its Color and ActiveOnStart are in Properties")
                                        .clicked()
                                    {
                                        action = Some(LayerAction::Edit(layer.id));
                                        ui.close();
                                    }
                                    if ui
                                        .button(theme::menu_item(
                                            ui.ctx(),
                                            icons::PENCIL_SIMPLE,
                                            "Rename",
                                        ))
                                        .clicked()
                                    {
                                        self.start_renaming(layer.id, &layer.name);
                                        ui.close();
                                    }
                                    if ui
                                        .button(theme::menu_item(ui.ctx(), icons::TRASH, "Delete"))
                                        .on_hover_text(
                                            "Delete the layer; its members stay, without its tag",
                                        )
                                        .clicked()
                                    {
                                        action = Some(LayerAction::Delete(layer.id));
                                        ui.close();
                                    }
                                })
                            });
                            more.response.on_hover_text("More");
                            ui.label(RichText::new(row.members.to_string()).color(palette.text_weak))
                                .on_hover_text("Instances tagged with the layer's name");
                            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                let (eye, eye_tip) = if row.hidden {
                                    (icons::EYE_SLASH, "Hidden in the editor: click to show it")
                                } else {
                                    (icons::EYE, "Shown: click to hide it while editing")
                                };
                                if widgets::icon_button(ui, eye, eye_tip, row.hidden).clicked() {
                                    action = Some(LayerAction::ToggleHidden(layer.name.clone()));
                                }
                                let (lock, lock_tip) = if row.locked {
                                    (icons::LOCK, "Locked: a click goes through it. Click to unlock")
                                } else {
                                    (icons::LOCK_OPEN, "Click to lock it, so a click goes through it")
                                };
                                if widgets::icon_button(ui, lock, lock_tip, row.locked).clicked() {
                                    action = Some(LayerAction::ToggleLocked(layer.name.clone()));
                                }
                                // Color3 is linear light; Color32 is sRGB.
                                let color = egui::Color32::from(egui::Rgba::from_rgb(
                                    layer.color.r,
                                    layer.color.g,
                                    layer.color.b,
                                ));
                                ui.label(
                                    RichText::new(theme::filled_icons::SQUARE)
                                        .family(theme::filled())
                                        .color(color),
                                );
                                let name = if row.hidden {
                                    RichText::new(&layer.name).color(palette.text_weak)
                                } else {
                                    RichText::new(&layer.name)
                                };
                                let label = ui
                                    .add(
                                        egui::Label::new(name)
                                            .truncate()
                                            .sense(egui::Sense::click()),
                                    )
                                    .on_hover_text("Click to select its members");
                                if label.clicked() {
                                    action = Some(LayerAction::SelectMembers(layer.name.clone()));
                                }
                            });
                        },
                    );
                }
                regions_section(ui, regions, editable, &mut action);
            });
        action
    }
}

/// The Regions list under the layers: each top-level Folder and Model,
/// its part count, and Unload in Editor.
fn regions_section(
    ui: &mut egui::Ui,
    regions: &[Region],
    editable: bool,
    action: &mut Option<LayerAction>,
) {
    let palette = theme::palette(ui.ctx());
    ui.add_space(8.0);
    ui.separator();
    ui.label(RichText::new("Regions").strong()).on_hover_text(
        "The Workspace's top-level Folders and Models. Unload one to set it aside while you \
             work on another: it isn't drawn or picked, but it stays in the world and saves \
             unchanged. Play shows everything.",
    );
    if regions.is_empty() {
        ui.label(
            RichText::new("Put the Workspace's parts in top-level Folders or Models to unload them one region at a time.")
                .color(palette.text_weak),
        );
        return;
    }
    for region in regions {
        let row_layout = egui::Layout::right_to_left(egui::Align::Center);
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
            row_layout,
            |ui| {
                ui.label(RichText::new(region.parts.to_string()).color(palette.text_weak))
                    .on_hover_text("Parts inside it");
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    let tip = if region.unloaded {
                        "Unloaded in the editor: click to load it"
                    } else {
                        "Click to unload it in the editor: not drawn or picked, still saved"
                    };
                    let toggle = ui.add_enabled_ui(editable, |ui| {
                        widgets::icon_button(ui, icons::EJECT, tip, region.unloaded)
                    });
                    if toggle.inner.clicked() {
                        *action = Some(LayerAction::ToggleUnloaded(region.name.clone()));
                    }
                    let icon = match (region.class, region.unloaded) {
                        (_, true) => icons::FOLDER_SIMPLE_DASHED,
                        (ClassName::Model, false) => icons::CUBE,
                        _ => icons::FOLDER_SIMPLE,
                    };
                    let text = format!("{icon} {}", region.name);
                    let name = if region.unloaded {
                        RichText::new(text).color(palette.text_weak)
                    } else {
                        RichText::new(text)
                    };
                    let label = ui
                        .add(
                            egui::Label::new(name)
                                .truncate()
                                .sense(egui::Sense::click()),
                        )
                        .on_hover_text("Click to select it");
                    if label.clicked() {
                        *action = Some(LayerAction::Edit(region.id));
                    }
                });
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A world with two layers, one with a tagged Model of two parts.
    fn world_with_layers() -> (World, InstanceId, InstanceId) {
        let mut world = World::new();
        for name in ["Night lights", "Props"] {
            let layer = world.create(ClassName::DataLayer).unwrap();
            world.set_name(layer, name).unwrap();
            world
                .set_parent(layer, Some(world.data_layer_service()))
                .unwrap();
        }
        let model = world.create(ClassName::Model).unwrap();
        world.set_parent(model, Some(world.workspace())).unwrap();
        let part = world.create(ClassName::Part).unwrap();
        world.set_parent(part, Some(model)).unwrap();
        world.add_tag(model, "Props").unwrap();
        (world, model, part)
    }

    #[test]
    fn sets_cover_what_is_inside_tagged_instances_and_follow_changes() {
        let (mut world, model, part) = world_with_layers();
        let mut sets = LayerSets::default();
        let mut view = LayerView::default();
        assert!(sets.update(&world, &view));
        assert!(sets.hidden.is_empty() && sets.unpickable.is_empty());
        assert!(!sets.update(&world, &view), "nothing changed");

        view.locked.insert("Props".into());
        assert!(sets.update(&world, &view));
        assert_eq!(sets.unpickable, HashSet::from([model, part]));
        assert!(sets.hidden.is_empty());

        // A tag no layer has hides nothing.
        view.hidden.insert("Ghost".into());
        world.add_tag(part, "Ghost").unwrap();
        sets.update(&world, &view);
        assert!(sets.hidden.is_empty());

        // A new member counts at once.
        let other = world.create(ClassName::Part).unwrap();
        world.set_parent(other, Some(world.workspace())).unwrap();
        world.add_tag(other, "Props").unwrap();
        assert!(sets.update(&world, &view));
        assert!(sets.unpickable.contains(&other));
        assert_eq!(unused_name(&world), "Layer");
    }

    /// R16.3b: an unloaded region isn't drawn, with everything inside it,
    /// and follows a rename by its name.
    #[test]
    fn an_unloaded_region_is_hidden_with_everything_inside() {
        let (mut world, model, part) = world_with_layers();
        world.set_name(model, "Town").unwrap();
        let folder = world.create(ClassName::Folder).unwrap();
        world.set_name(folder, "Forest").unwrap();
        world.set_parent(folder, Some(world.workspace())).unwrap();
        let tree = world.create(ClassName::Part).unwrap();
        world.set_parent(tree, Some(folder)).unwrap();
        let loose = world.create(ClassName::Part).unwrap();
        world.set_parent(loose, Some(world.workspace())).unwrap();

        let mut view = LayerView::default();
        let listed = regions(&world, &view);
        let names: Vec<_> = listed.iter().map(|r| (r.name.as_str(), r.parts)).collect();
        assert_eq!(
            names,
            [("Town", 1), ("Forest", 1)],
            "a loose part isn't a region"
        );

        let mut sets = LayerSets::default();
        view.unloaded.insert("Forest".into());
        assert!(sets.update(&world, &view));
        assert_eq!(sets.hidden, HashSet::from([folder, tree]));
        assert!(sets.unpickable.contains(&tree));
        assert!(regions(&world, &view)[1].unloaded);

        // Renamed, it's another region as far as the view goes.
        world.set_name(folder, "Woods").unwrap();
        assert!(sets.update(&world, &view));
        assert!(sets.hidden.is_empty());
        world.set_name(model, "Forest").unwrap();
        assert!(sets.update(&world, &view));
        assert_eq!(sets.hidden, HashSet::from([model, part]));

        // Kept in the project folder with the layers.
        let project = std::env::temp_dir().join(format!(
            "axcenity-editor-regions-test-{}",
            std::process::id()
        ));
        view.save(&project).unwrap();
        assert_eq!(LayerView::load(&project), view);
        view.unloaded.clear();
        view.save(&project).unwrap();
        assert!(!project.join(VIEW_FILE).exists());
        let _ = std::fs::remove_dir_all(&project);
    }

    #[test]
    fn the_panel_lists_each_layer_and_snapshot() {
        let (world, _, _) = world_with_layers();
        let view = LayerView {
            hidden: BTreeSet::from(["Night lights".to_owned()]),
            ..Default::default()
        };
        let rows = rows(&world, &view);
        let regions = regions(&world, &view);
        assert_eq!(rows.len(), 2);
        assert_eq!((rows[1].layer.name.as_str(), rows[1].members), ("Props", 1));
        assert!(rows[0].hidden && !rows[1].hidden);
        let mut panel = LayersPanel::default();
        let mut draw = |ui: &mut egui::Ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let _ = panel.show(ui, &rows, &regions, 1, true);
            });
        };
        // Drawn without a GPU too, so the panel's code runs in every test run.
        let ctx = theme::test_context();
        for _ in 0..2 {
            theme::run(&ctx, egui::RawInput::default(), &mut draw);
        }
        if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
            crate::snapshot::snapshot(
                &std::path::PathBuf::from(folder).join("layers-panel.png"),
                egui::vec2(260.0, 220.0),
                vec![Vec::new(); 3],
                &mut draw,
            );
        }
    }
}
