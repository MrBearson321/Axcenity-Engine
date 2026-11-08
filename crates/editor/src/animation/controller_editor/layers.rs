//! A state machine's layers (Phase 78) in the editor. Opening a layer
//! puts its states in [`ControllerDoc::file`] as a state machine of their
//! own (with the file's parameters), so the graph, the inspector and the
//! preview edit it as they edit the base layer; the whole file is kept in
//! `base` meanwhile, and [`ControllerDoc::merged`] puts them back together.

use super::*;
use axcenity_anim::files::{LayerDef, LayerMode};

impl ControllerDoc {
    /// The whole file, with the open layer's edits in it.
    pub fn merged(&self) -> StateMachineFile {
        let Some(base) = &self.base else {
            return self.file.clone();
        };
        let mut full = base.clone();
        full.parameters.clone_from(&self.file.parameters);
        if let Some(layer) = full.layers.get_mut(self.layer) {
            layer.set_machine(&self.file);
        }
        full
    }

    /// The layer open in the graph, by its index in the file's `layers`, or
    /// `None` for the base layer.
    pub fn open_layer_index(&self) -> Option<usize> {
        self.base.as_ref().map(|_| self.layer)
    }

    /// The open layer's settings (`None` on the base layer).
    pub fn open_layer(&self) -> Option<&LayerDef> {
        self.base.as_ref()?.layers.get(self.layer)
    }

    fn open_layer_mut(&mut self) -> Option<&mut LayerDef> {
        self.base.as_mut()?.layers.get_mut(self.layer)
    }

    /// Shows the layer `layer` in the graph (`None`: the base layer).
    pub fn show_layer(&mut self, layer: Option<usize>) {
        let full = self.merged();
        self.selected = Selected::Controller;
        self.renaming = None;
        self.preview = None;
        match layer.filter(|&index| index < full.layers.len()) {
            Some(index) => {
                self.file = full.layers[index].machine(&full.parameters);
                self.layer = index;
                self.base = Some(full);
            }
            None => {
                self.file = full;
                self.layer = 0;
                self.base = None;
            }
        }
    }

    /// Adds an empty Override layer over the others and shows it. Returns
    /// its index.
    pub fn add_layer(&mut self) -> usize {
        let mut full = self.merged();
        let name = (1u32..)
            .map(|n| match n {
                1 => "Upper Body".to_owned(),
                n => format!("Layer {n}"),
            })
            .find(|name| full.layer(name).is_none())
            .expect("some number is always free");
        let mut layer = LayerDef::new(&name);
        layer.default_state = "Empty".to_owned();
        layer.states.push(StateDef::new(
            "Empty",
            Motion::Clip(String::new()),
            [0.0, 0.0],
        ));
        full.layers.push(layer);
        let index = full.layers.len() - 1;
        self.base = None;
        self.file = full;
        self.show_layer(Some(index));
        index
    }

    /// Removes the layer `index`, showing the base layer.
    pub fn remove_layer(&mut self, index: usize) {
        self.show_layer(None);
        if index < self.file.layers.len() {
            self.file.layers.remove(index);
        }
    }

    /// Renames the open layer, if no other layer has the name.
    pub fn rename_layer(&mut self, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("A layer needs a name.".to_owned());
        }
        let index = self.layer;
        let base = self.base.as_mut().ok_or("The base layer keeps its name.")?;
        if base
            .layers
            .iter()
            .enumerate()
            .any(|(i, layer)| i != index && layer.name == name)
        {
            return Err(format!("There's already a layer called `{name}`."));
        }
        base.layers[index].name = name.to_owned();
        Ok(())
    }

    /// Adds `bone` (a bone path) to the open layer's mask.
    pub fn mask_bone(&mut self, bone: &str) {
        if let Some(layer) = self.open_layer_mut()
            && !layer.mask.iter().any(|b| b == bone)
        {
            layer.mask.push(bone.to_owned());
        }
    }

    /// The Layers part of the controller's inspector: which layer the graph
    /// shows, and the open layer's weight, mode and mask.
    pub(super) fn layers_section(&mut self, ui: &mut egui::Ui) {
        ui.strong(format!("{} Layers", icons::STACK));
        ui.label("Layers: each plays its own states over the ones under it, on the bones its mask picks. An Upper Body layer can reload while the legs run.");
        let names: Vec<String> = match &self.base {
            Some(base) => base.layers.iter().map(|l| l.name.clone()).collect(),
            None => self.file.layers.iter().map(|l| l.name.clone()).collect(),
        };
        let open = self.open_layer_index();
        let mut show = None;
        let mut remove = None;
        ui.horizontal_wrapped(|ui| {
            if ui.selectable_label(open.is_none(), "Base Layer").clicked() && open.is_some() {
                show = Some(None);
            }
            for (index, name) in names.iter().enumerate() {
                let response = ui.selectable_label(open == Some(index), name);
                if response.clicked() && open != Some(index) {
                    show = Some(Some(index));
                }
                response.context_menu(|ui| {
                    if ui
                        .button(format!("{} Delete layer", icons::TRASH))
                        .clicked()
                    {
                        remove = Some(index);
                        ui.close();
                    }
                });
            }
            if ui
                .button(format!("{} Add layer", icons::PLUS))
                .on_hover_text(
                    "A layer over the others, starting in an empty state that moves nothing",
                )
                .clicked()
            {
                self.add_layer();
            }
        });
        if let Some(index) = remove {
            self.remove_layer(index);
        } else if let Some(layer) = show {
            self.show_layer(layer);
        }
        let bones = self.bones.clone();
        let Some(layer) = self.open_layer().cloned() else {
            return;
        };
        let mut name = layer.name.clone();
        let mut renamed = None;
        crate::responsive::form(ui, "layer settings", |ui| {
            ui.label("Name");
            if ui
                .add(egui::TextEdit::singleline(&mut name).desired_width(140.0))
                .lost_focus()
                && name != layer.name
            {
                renamed = Some(name.clone());
            }
            ui.end_row();
            ui.label("Weight");
            let mut weight = layer.weight;
            if ui
                .add(egui::Slider::new(&mut weight, 0.0..=1.0))
                .on_hover_text("How much it counts when the game starts; scripts change it with animator:SetLayerWeight")
                .changed()
                && let Some(open) = self.open_layer_mut()
            {
                open.weight = weight;
            }
            ui.end_row();
            ui.label("Mode");
            ui.horizontal(|ui| {
                for (mode, label, hint) in [
                    (LayerMode::Override, "Override", "Its pose replaces the pose under it"),
                    (LayerMode::Additive, "Additive", "How far its pose moves each bone from the rest pose is added (aim offsets)"),
                ] {
                    if ui
                        .selectable_label(layer.mode == mode, label)
                        .on_hover_text(hint)
                        .clicked()
                        && let Some(open) = self.open_layer_mut()
                    {
                        open.mode = mode;
                    }
                }
            });
            ui.end_row();
        });
        if let Some(name) = renamed
            && let Err(problem) = self.rename_layer(&name)
        {
            ui.colored_label(ui.visuals().error_fg_color, problem);
        }
        ui.label(format!("{} Mask", icons::BONE));
        if layer.mask.is_empty() {
            ui.weak("The whole skeleton. Pick a bone to limit it to that bone and everything under it, such as the spine for the upper body.");
        }
        let mut unmask = None;
        for (index, bone) in layer.mask.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("{bone} and below"));
                if ui.small_button(icons::X).on_hover_text("Remove").clicked() {
                    unmask = Some(index);
                }
            });
        }
        if let Some(index) = unmask
            && let Some(open) = self.open_layer_mut()
        {
            open.mask.remove(index);
        }
        if bones.is_empty() {
            ui.weak("Pick a model with a skeleton for the preview to choose its bones.");
        } else {
            let mut picked = None;
            egui::ComboBox::from_id_salt("mask bone")
                .selected_text(format!("{} Add bone…", icons::PLUS))
                .height(320.0)
                .show_ui(ui, |ui| {
                    // The rig tree: each bone under its parent.
                    for path in &bones {
                        let depth = path.matches('/').count();
                        let name = path.rsplit('/').next().unwrap_or(path);
                        let label = format!("{}{name}", "   ".repeat(depth));
                        if ui
                            .selectable_label(false, label)
                            .on_hover_text(path)
                            .clicked()
                        {
                            picked = Some(path.clone());
                        }
                    }
                });
            if let Some(path) = picked {
                self.mask_bone(&path);
            }
        }
    }
}
