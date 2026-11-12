//! The Model Studio's modifier stack (Phase 57): the Modifiers panel under
//! the object list, and the edits it makes. Adding, removing, reordering,
//! showing or hiding a modifier, or changing a setting, is an undo step
//! (a drag is one step); Apply bakes one into the mesh as one step.
//! Generate LODs adds `_LOD1` and `_LOD2` copies with a Decimate. The
//! modifiers themselves are `mesh_edit`'s.

use axcenity_mesh_edit::{Axis, MAX_COPIES, MAX_LEVELS, Modifier, ModifierKind, Purpose};
use eframe::egui;

use super::{Mode, StudioDoc};
use crate::responsive;
use crate::theme::{self, icons};
use crate::widgets::{self, Role};

impl StudioDoc {
    /// Puts `stack` on the active object. The first change is an undo step
    /// named `label`; the changes after it join that step until
    /// [`end_modifier_edit`](Self::end_modifier_edit), so a drag is one.
    pub fn set_modifiers(&mut self, label: &str, stack: Vec<Modifier>) {
        let object = self.active;
        if self.model.objects.get(object).is_none() {
            return;
        }
        if self.modifier_edit == Some((object, self.undo.len())) {
            self.revision += 1;
        } else {
            let before = self.snapshot();
            self.record(label, before);
            self.modifier_edit = Some((object, self.undo.len()));
        }
        self.model.objects[object].modifiers = stack;
    }

    /// The next stack change is a new undo step.
    pub fn end_modifier_edit(&mut self) {
        self.modifier_edit = None;
    }

    /// Apply: modifier `index` of the active object baked into its mesh
    /// (with its viewport settings) and taken off the stack, as one undo
    /// step. As in Blender, it runs on the mesh itself, without the
    /// modifiers before it.
    pub fn apply_modifier(&mut self, index: usize) -> Result<(), String> {
        let object = self
            .model
            .objects
            .get(self.active)
            .ok_or("No object to apply it to")?;
        let mesh = object.mesh.as_ref().ok_or("A group has no mesh")?;
        let modifier = object.modifiers.get(index).ok_or("No such modifier")?;
        let baked = modifier.kind.run(mesh, Purpose::Viewport)?;
        let label = format!("Apply {}", modifier.kind.label());
        let before = self.snapshot();
        let object = &mut self.model.objects[self.active];
        object.mesh = Some(baked);
        object.modifiers.remove(index);
        // The mesh's numbers changed, so what was picked means nothing.
        self.selection.clear();
        self.record(&label, before);
        Ok(())
    }

    /// Generate LODs for the active object, as one undo step.
    pub fn generate_lods(&mut self) -> Result<(), String> {
        let before = self.snapshot();
        let index = self
            .model
            .generate_lods(self.active)
            .ok_or("Generate LODs needs a full object with a mesh, not a group or a copy")?;
        self.active = index;
        self.objects = [index].into();
        self.mode = Mode::Object;
        self.selection.clear();
        self.record("Generate LODs", before);
        Ok(())
    }
}

/// The Modifiers panel for the active object. `errors` are the modifiers
/// that couldn't run last time, by place, and why.
pub(super) fn panel(
    ui: &mut egui::Ui,
    doc: &mut StudioDoc,
    errors: &[(usize, String)],
    editable: bool,
    notices: &mut Vec<String>,
) {
    widgets::section_label(ui, "Modifiers");
    let Some(object) = doc.model.objects.get(doc.active) else {
        return;
    };
    if object.mesh.is_none() {
        ui.weak("Pick an object with a mesh.");
        return;
    }
    let palette = theme::palette(ui.ctx());
    let mut stack = object.modifiers.clone();
    let mut change: Option<String> = None;
    let mut apply = None;
    ui.add_enabled_ui(editable, |ui| {
        axcenity_editor_kit::widgets::menu_button(ui, Some(icons::PLUS), "Add Modifier", |ui| {
            for kind in ModifierKind::defaults() {
                if ui.button(kind.label()).on_hover_text(tip(&kind)).clicked() {
                    change = Some(format!("Add {}", kind.label()));
                    stack.push(Modifier::new(kind));
                    ui.close();
                }
            }
        });
        let count = stack.len();
        let mut swap = None;
        let mut remove = None;
        for (index, modifier) in stack.iter_mut().enumerate() {
            ui.push_id(("studio modifier", index), |ui| {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    responsive::toolbar(ui, |ui| {
                        let (icon, tip) = if modifier.show {
                            (icons::EYE, "Shown in the view and the saved model")
                        } else {
                            (icons::EYE_SLASH, "Hidden from the view and the saved model")
                        };
                        if widgets::icon_button(ui, icon, tip, modifier.show).clicked() {
                            modifier.show = !modifier.show;
                            let verb = if modifier.show { "Show" } else { "Hide" };
                            change = Some(format!("{verb} {}", modifier.kind.label()));
                        }
                        ui.strong(modifier.kind.label());
                        if ui
                            .add_enabled(index > 0, egui::Button::new(icons::ARROW_UP))
                            .on_hover_text("Move up")
                            .clicked()
                        {
                            swap = Some(index - 1);
                        }
                        if ui
                            .add_enabled(index + 1 < count, egui::Button::new(icons::ARROW_DOWN))
                            .on_hover_text("Move down")
                            .clicked()
                        {
                            swap = Some(index);
                        }
                        if ui
                            .button(icons::CHECK)
                            .on_hover_text("Apply: bake it into the mesh")
                            .clicked()
                        {
                            apply = Some(index);
                        }
                        if ui.button(icons::X).on_hover_text("Remove").clicked() {
                            remove = Some(index);
                        }
                    });
                    if let Some((_, why)) = errors.iter().find(|(k, _)| *k == index) {
                        ui.colored_label(palette.warning, why);
                    }
                    if settings_ui(ui, &mut modifier.kind) {
                        change = Some(format!("Change {}", modifier.kind.label()));
                    }
                });
            });
        }
        if let Some(upper) = swap {
            stack.swap(upper, upper + 1);
            change = Some("Reorder Modifiers".to_owned());
        }
        if let Some(index) = remove {
            change = Some(format!("Remove {}", stack[index].kind.label()));
            stack.remove(index);
        }
        ui.add_space(4.0);
        if ui
            .add(widgets::button(
                Role::Secondary,
                Some(icons::STACK),
                "Generate LODs",
                &palette,
            ))
            .on_hover_text(
                "Add _LOD1 and _LOD2 copies with a Decimate, \
                 which the game draws far away",
            )
            .clicked()
            && let Err(why) = doc.generate_lods()
        {
            notices.push(why);
        }
    });
    if let Some(label) = change {
        doc.set_modifiers(&label, stack);
    } else if let Some(index) = apply
        && let Err(why) = doc.apply_modifier(index)
    {
        notices.push(format!("Could not apply it: {why}"));
    }
    // A drag or a typed number goes on changing the same step.
    let busy = ui.ctx().dragged_id().is_some() || ui.ctx().memory(|m| m.focused().is_some());
    if !busy {
        doc.end_modifier_edit();
    }
}

fn length(ui: &mut egui::Ui, value: &mut f32, min: f32) -> bool {
    ui.add(
        egui::DragValue::new(value)
            .speed(0.01)
            .range(min..=10_000.0)
            .suffix(" studs"),
    )
    .changed()
}

/// A modifier's settings. True if one changed.
pub(super) fn settings_ui(ui: &mut egui::Ui, kind: &mut ModifierKind) -> bool {
    let mut changed = false;
    responsive::form(ui, "studio modifier settings", |ui| match kind {
        ModifierKind::Mirror {
            axis,
            clip,
            merge,
            merge_distance,
        } => {
            ui.label("Axis");
            ui.horizontal(|ui| {
                for option in Axis::ALL {
                    changed |= ui.radio_value(axis, option, option.label()).changed();
                }
            });
            ui.end_row();
            ui.label("Clipping");
            changed |= ui
                .checkbox(clip, "")
                .on_hover_text("Keep vertices from crossing the mirror in Edit mode")
                .changed();
            ui.end_row();
            ui.label("Merge");
            changed |= ui.checkbox(merge, "").changed();
            ui.end_row();
            ui.label("Distance");
            changed |= length(ui, merge_distance, 0.0);
            ui.end_row();
        }
        ModifierKind::Subdivision { viewport, export } => {
            ui.label("Viewport levels");
            changed |= ui
                .add(egui::DragValue::new(viewport).range(0..=MAX_LEVELS))
                .changed();
            ui.end_row();
            ui.label("Export levels");
            changed |= ui
                .add(egui::DragValue::new(export).range(0..=MAX_LEVELS))
                .on_hover_text("The levels the saved model has")
                .changed();
            ui.end_row();
        }
        ModifierKind::Array {
            count,
            offset,
            merge,
            merge_distance,
        } => {
            ui.label("Count");
            changed |= ui
                .add(egui::DragValue::new(count).range(1..=MAX_COPIES))
                .changed();
            ui.end_row();
            ui.label("Offset");
            ui.horizontal(|ui| {
                for value in offset.iter_mut() {
                    changed |= ui
                        .add(egui::DragValue::new(value).speed(0.01))
                        .on_hover_text("In the mesh's own sizes: 1 puts copies side by side")
                        .changed();
                }
            });
            ui.end_row();
            ui.label("Merge");
            changed |= ui.checkbox(merge, "").changed();
            ui.end_row();
            ui.label("Distance");
            changed |= length(ui, merge_distance, 0.0);
            ui.end_row();
        }
        ModifierKind::Solidify { thickness, offset } => {
            ui.label("Thickness");
            changed |= length(ui, thickness, -10_000.0);
            ui.end_row();
            ui.label("Offset");
            changed |= ui.add(egui::Slider::new(offset, -1.0..=1.0)).changed();
            ui.end_row();
        }
        ModifierKind::Bevel {
            width,
            segments,
            angle,
        } => {
            ui.label("Width");
            changed |= length(ui, width, 0.0);
            ui.end_row();
            ui.label("Segments");
            changed |= ui
                .add(egui::DragValue::new(segments).range(1..=64))
                .changed();
            ui.end_row();
            ui.label("Angle");
            changed |= ui
                .add(egui::Slider::new(angle, 0.0..=180.0).suffix("°"))
                .on_hover_text("Edges sharper than this are beveled")
                .changed();
            ui.end_row();
        }
        ModifierKind::Decimate { ratio } => {
            ui.label("Ratio");
            changed |= ui
                .add(egui::Slider::new(ratio, 0.01..=1.0))
                .on_hover_text("The share of triangles kept")
                .changed();
            ui.end_row();
        }
        ModifierKind::Triangulate => {
            ui.weak("Every face cut into triangles.");
            ui.end_row();
        }
    });
    changed
}

/// What a modifier does, for the Add Modifier menu. A modifier changes
/// what's drawn and saved, never the mesh itself, until it's applied.
fn tip(kind: &ModifierKind) -> &'static str {
    match kind {
        ModifierKind::Mirror { .. } => {
            "A reflected copy across the object's middle: model one half, get both"
        }
        ModifierKind::Subdivision { .. } => {
            "Smooths the shape by splitting every face, so a few faces make a round surface"
        }
        ModifierKind::Array { .. } => "Copies in a row: fences, stairs, chains",
        ModifierKind::Solidify { .. } => "Gives a flat surface thickness",
        ModifierKind::Bevel { .. } => "Rounds the sharp edges a little, so they catch the light",
        ModifierKind::Decimate { .. } => "Fewer triangles, for a lighter model",
        ModifierKind::Triangulate => "Cuts every face into triangles, as the game draws them",
    }
}
