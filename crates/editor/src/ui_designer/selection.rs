//! The designer's selection of several elements: Shift- and Ctrl-clicks,
//! the box dragged on empty artboard, the arrange bar (align, distribute,
//! Copy style, Paste style, Duplicate), the right-click menu's copies of
//! those, and the keys: the arrows nudge, Ctrl+C and Ctrl+V copy and paste
//! elements. Nothing here fires while a text field is being typed in.

use axcenity_core::{InstanceId, World};
use axcenity_ui::{UiLayout, UiRect};
use eframe::egui;

use super::arrange::{self, Align};
use super::{DesignerAction, DesignerScene, UiDesigner, container_for};
use crate::explorer;
use crate::responsive;
use crate::theme::{self, icons};
use crate::widgets;

/// A box being dragged on empty artboard to select what's inside it.
#[derive(Clone, Debug)]
pub(super) struct Marquee {
    /// Where the pointer went down, in points.
    pub from: egui::Pos2,
    /// What stays selected besides what's inside: the selection as it was
    /// for a Shift- or Ctrl-drag, else nothing.
    pub base: Vec<InstanceId>,
    /// What it selected last, so a change is sent once.
    pub last: Option<Vec<InstanceId>>,
}

/// The elements inside `rect` (layout pixels) whose own parent isn't
/// inside it too: a Frame and the buttons in it select just the Frame.
pub(super) fn inside(
    world: &World,
    layout: &UiLayout,
    gui: InstanceId,
    rect: UiRect,
) -> Vec<InstanceId> {
    let contains = |inner: &UiRect| {
        inner.x >= rect.x
            && inner.y >= rect.y
            && inner.x + inner.width <= rect.x + rect.width
            && inner.y + inner.height <= rect.y + rect.height
    };
    let enclosed: Vec<InstanceId> = layout
        .elements
        .iter()
        .filter(|element| element.id != gui && contains(&element.rect))
        .map(|element| element.id)
        .collect();
    enclosed
        .iter()
        .copied()
        .filter(|&id| {
            !enclosed
                .iter()
                .any(|&other| other != id && world.is_descendant_of(id, other))
        })
        .collect()
}

/// "Button", or "3 elements".
fn named(world: &World, ids: &[InstanceId]) -> String {
    match ids {
        [only] => world
            .get(*only)
            .map_or_else(String::new, |instance| instance.name().to_owned()),
        _ => format!("{} elements", ids.len()),
    }
}

/// The ScreenGui laid out at its design size, one pixel to a design
/// pixel, and the area it's laid out on there.
fn design_layout(scene: &DesignerScene, gui: InstanceId) -> (UiLayout, UiRect) {
    let (width, height) = axcenity_ui::design_size(scene.world, gui);
    let layout = axcenity_ui::layout_artboard(
        scene.world,
        gui,
        scene.stylesheets,
        (width, height),
        1.0,
        [0.0, 0.0],
    );
    let (unit, origin, area) =
        axcenity_ui::safe_screen_mapping(scene.world, gui, width, height, [0.0; 4]);
    let area = UiRect {
        x: origin[0],
        y: origin[1],
        width: area[0] * unit,
        height: area[1] * unit,
    };
    (layout, area)
}

impl UiDesigner {
    /// The selected elements on the artboard: the selection inside the
    /// ScreenGui, which itself is left out. The last is the one the
    /// inspector shows.
    pub(super) fn chosen(&self, scene: &DesignerScene, gui: InstanceId) -> Vec<InstanceId> {
        let selection = if scene.selection.is_empty() {
            scene.selected.as_slice()
        } else {
            scene.selection
        };
        selection
            .iter()
            .copied()
            .filter(|&id| id != gui && scene.world.is_descendant_of(id, gui))
            .collect()
    }

    /// The selection after a Shift- or Ctrl-click on `id`: Ctrl adds or
    /// takes it away, Shift adds it, or with `order` (the hierarchy's
    /// rows) adds the rows from the last selected to it.
    pub(super) fn extended(
        &self,
        scene: &DesignerScene,
        gui: InstanceId,
        id: InstanceId,
        modifiers: egui::Modifiers,
        order: Option<&[InstanceId]>,
    ) -> Vec<InstanceId> {
        let mut selection = self.chosen(scene, gui);
        explorer::update_selection(&mut selection, Some(id), modifiers, order, false);
        selection.retain(|&chosen| chosen != gui);
        selection
    }

    /// Align, distribute, Copy style, Paste style and Duplicate, for the
    /// selection, then the game UI settings menu.
    pub(super) fn arrange_bar(
        &mut self,
        ui: &mut egui::Ui,
        scene: &DesignerScene,
        gui: InstanceId,
        actions: &mut Vec<DesignerAction>,
    ) {
        let chosen = self.chosen(scene, gui);
        let editing = scene.editing && !chosen.is_empty();
        responsive::toolbar(ui, |ui| {
            ui.add_enabled_ui(editing, |ui| {
                responsive::room_for(ui, 160.0);
                for how in Align::ALL {
                    let tip = if chosen.len() == 1 {
                        format!("{} (to its parent)", how.label())
                    } else {
                        how.label().to_owned()
                    };
                    if widgets::icon_button(ui, how.icon(), &tip, false).clicked() {
                        let (layout, area) = design_layout(scene, gui);
                        let positions =
                            arrange::align(scene.world, &layout, area, gui, &chosen, how);
                        actions.push(DesignerAction::Arrange {
                            label: format!("Align {}", named(scene.world, &chosen)),
                            positions,
                        });
                    }
                }
                responsive::room_for(ui, 60.0);
                for (across, icon, tip) in [
                    (
                        true,
                        icons::ARROWS_OUT_LINE_HORIZONTAL,
                        "Distribute horizontally",
                    ),
                    (
                        false,
                        icons::ARROWS_OUT_LINE_VERTICAL,
                        "Distribute vertically",
                    ),
                ] {
                    let response = ui
                        .add_enabled_ui(chosen.len() >= 3, |ui| {
                            widgets::icon_button(ui, icon, tip, false)
                        })
                        .inner
                        .on_disabled_hover_text("Select three or more to space them evenly");
                    if response.clicked() {
                        let (layout, _) = design_layout(scene, gui);
                        let positions =
                            arrange::distribute(scene.world, &layout, gui, &chosen, across);
                        actions.push(DesignerAction::Arrange {
                            label: format!("Distribute {}", named(scene.world, &chosen)),
                            positions,
                        });
                    }
                }
                ui.separator();
                responsive::room_for(ui, 90.0);
                self.style_buttons(ui, scene, &chosen, actions);
            });
            // Not about the selection: always there, Play too.
            ui.separator();
            super::accessibility::settings_menu(ui, scene, actions);
        });
    }

    /// Copy style, Paste style and Duplicate, as icon buttons.
    fn style_buttons(
        &mut self,
        ui: &mut egui::Ui,
        scene: &DesignerScene,
        chosen: &[InstanceId],
        actions: &mut Vec<DesignerAction>,
    ) {
        let primary = chosen.last().copied();
        let own = primary.and_then(|id| arrange::own_style(scene.world, scene.stylesheets, id));
        let copy = ui
            .add_enabled_ui(own.is_some(), |ui| {
                widgets::icon_button(ui, icons::COPY, "Copy style", false)
            })
            .inner
            .on_disabled_hover_text("Its own #Id rule has nothing to copy");
        if copy.clicked() {
            self.copied_style = own;
        }
        let paste = ui
            .add_enabled_ui(self.copied_style.is_some(), |ui| {
                widgets::icon_button(ui, icons::PAINT_BRUSH, "Paste style", false)
            })
            .inner
            .on_disabled_hover_text("Copy style from an element first");
        if paste.clicked() {
            actions.extend(self.paste_style(chosen));
        }
        if widgets::icon_button(ui, icons::COPY_SIMPLE, "Duplicate (Ctrl+D)", false).clicked() {
            actions.push(DesignerAction::Duplicate);
        }
    }

    /// Paste style onto `ids`, when a style was copied.
    fn paste_style(&self, ids: &[InstanceId]) -> Option<DesignerAction> {
        let declarations = self.copied_style.clone()?;
        (!ids.is_empty()).then(|| DesignerAction::PasteStyle {
            ids: ids.to_vec(),
            declarations,
        })
    }

    /// The right-click menu's Copy style, Paste style, Duplicate, Rename
    /// and Delete, for `id` (or the selection, when it's in it; Rename is
    /// for `id` alone).
    pub(super) fn selection_menu_items(
        &mut self,
        ui: &mut egui::Ui,
        scene: &DesignerScene,
        gui: InstanceId,
        id: InstanceId,
        actions: &mut Vec<DesignerAction>,
    ) {
        let chosen = self.chosen(scene, gui);
        let ids = if chosen.contains(&id) {
            chosen
        } else {
            vec![id]
        };
        ui.separator();
        let own = arrange::own_style(scene.world, scene.stylesheets, id);
        if ui
            .add_enabled(own.is_some(), egui::Button::new("Copy style"))
            .clicked()
        {
            self.copied_style = own;
            ui.close();
        }
        if ui
            .add_enabled(
                self.copied_style.is_some(),
                egui::Button::new("Paste style"),
            )
            .clicked()
        {
            actions.extend(self.paste_style(&ids));
            ui.close();
        }
        if ui
            .add(
                egui::Button::new(theme::with_icon(icons::COPY, "Duplicate"))
                    .shortcut_text(crate::shortcuts::Action::Duplicate.menu_text(ui.ctx())),
            )
            .clicked()
        {
            if !scene.selection.contains(&id) {
                actions.push(DesignerAction::Select(Some(id)));
            }
            actions.push(DesignerAction::Duplicate);
            ui.close();
        }
        ui.separator();
        if ui
            .button(theme::with_icon(icons::PENCIL_SIMPLE, "Rename"))
            .on_hover_text("Type its new name in the inspector")
            .clicked()
        {
            actions.push(DesignerAction::Select(Some(id)));
            self.renaming = Some(id);
            ui.close();
        }
        if ui
            .add(
                egui::Button::new(theme::with_icon(icons::TRASH, "Delete"))
                    .shortcut_text(crate::shortcuts::Action::Delete.menu_text(ui.ctx())),
            )
            .on_hover_text("Ctrl+Z brings it back")
            .clicked()
        {
            if !scene.selection.contains(&id) {
                actions.push(DesignerAction::Select(Some(id)));
            }
            actions.push(DesignerAction::Delete);
            ui.close();
        }
    }

    /// The arrow keys nudge the selection 1 design pixel (Shift: 10),
    /// Ctrl+C copies it and Ctrl+V pastes what was copied. Not while text
    /// is being typed anywhere.
    pub(super) fn selection_keys(
        &mut self,
        ctx: &egui::Context,
        scene: &DesignerScene,
        gui: InstanceId,
        actions: &mut Vec<DesignerAction>,
    ) {
        if !scene.editing || self.text_editing.is_some() || ctx.egui_wants_keyboard_input() {
            return;
        }
        let chosen = self.chosen(scene, gui);
        let mut by = [0.0_f32; 2];
        let mut ten = false;
        ctx.input_mut(|input| {
            use crate::shortcuts::{Action, count};
            // Shift first: a plain arrow's pattern matches it too.
            for (action, pixels) in [(Action::NudgeTen, 10.0), (Action::Nudge, 1.0)] {
                for (key, times) in count(input, action) {
                    let step = match key {
                        egui::Key::ArrowLeft => [-1.0, 0.0],
                        egui::Key::ArrowRight => [1.0, 0.0],
                        egui::Key::ArrowUp => [0.0, -1.0],
                        egui::Key::ArrowDown => [0.0, 1.0],
                        _ => [0.0, 0.0],
                    };
                    ten |= pixels > 1.0 && times > 0;
                    for axis in 0..2 {
                        by[axis] += step[axis] * pixels * times as f32;
                    }
                }
            }
        });
        if by != [0.0, 0.0] && !chosen.is_empty() {
            let (layout, _) = design_layout(scene, gui);
            let positions = arrange::nudge(scene.world, &layout, gui, &chosen, by);
            if !positions.is_empty() {
                let ids: Vec<InstanceId> = positions.iter().map(|(id, _)| *id).collect();
                let label = format!(
                    "Nudge {}{}",
                    named(scene.world, &ids),
                    if ten { " by 10" } else { "" }
                );
                actions.push(DesignerAction::Arrange { label, positions });
            }
        }

        // Ctrl+C and Ctrl+V come as Copy and Paste events from the window;
        // the keys themselves are taken too.
        let ours = self
            .clipboard
            .as_ref()
            .map(|clipboard| clipboard.prefab.clone());
        let (copy, paste) = ctx.input_mut(|input| {
            let copy = crate::shortcuts::consume(input, crate::shortcuts::Action::CopyElements)
                || input
                    .events
                    .iter()
                    .any(|event| matches!(event, egui::Event::Copy));
            let paste = crate::shortcuts::consume(input, crate::shortcuts::Action::PasteElements)
                || input.events.iter().any(|event| match event {
                    egui::Event::Paste(text) => ours
                        .as_deref()
                        .is_some_and(|ours| text.trim() == ours.trim()),
                    _ => false,
                });
            (copy, paste)
        });
        if copy && let Some(clipboard) = arrange::copy(scene.world, scene.stylesheets, &chosen) {
            ctx.copy_text(clipboard.prefab.clone());
            self.clipboard = Some(clipboard);
        }
        if paste && let Some(clipboard) = self.clipboard.clone() {
            let parent = container_for(scene.world, gui, chosen.last().copied());
            actions.push(DesignerAction::Paste { parent, clipboard });
        }
    }
}
