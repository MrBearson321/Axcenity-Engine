//! The designer's side panels: the palette of elements, the hierarchy, and
//! the inspector: name, text and font, then its sections beginner-first
//! (`sections`: Styles, Anchor, Text, Widget, Image, Look, Code &
//! animation, then States, Bevel and Slice, folded; `fold.rs`), and the
//! Style (CSS) section.

use std::sync::Arc;

use axcenity_core::{InstanceId, Value, World};
use axcenity_ui::{Stylesheet, UiState};
use eframe::egui;

use super::anchors::{Pin, Placement, anchor_of};
use super::fold::{self, Section, fold};
use super::text_toolbar;
use super::{
    CLICK_INSERT_AT, DesignerAction, DesignerScene, PALETTE, PaletteItem, UiDesigner,
    anchor_action, container_for, hierarchy, reset_anchor_action,
};
use crate::properties::Edit;
use crate::responsive;
use crate::style_panel;
use crate::theme;
use crate::widgets;

/// Hierarchy rows being dragged: the elements they move.
struct HierarchyRows(Vec<InstanceId>);

/// The narrowest a palette cell gets, in points.
const PALETTE_CELL: f32 = 88.0;
/// The side of an anchor picker cell, in points.
const ANCHOR_CELL: f32 = 24.0;

/// Where `id` sits in its parent on its ScreenGui's design-size screen,
/// in design pixels, and whether a flex layout places it.
pub fn placement(
    world: &World,
    stylesheets: &[Stylesheet],
    gui: InstanceId,
    id: InstanceId,
) -> Option<(Placement, bool)> {
    let screen = axcenity_ui::design_size(world, gui);
    let layout = axcenity_ui::layout_artboard(world, gui, stylesheets, screen, 1.0, [0.0, 0.0]);
    let element = layout.element(id)?;
    let per_unit = element.pixels_per_unit.max(1e-4);
    let parent = world
        .get(id)?
        .parent()
        .and_then(|parent| layout.element(parent))
        .map_or(
            axcenity_ui::UiRect {
                x: 0.0,
                y: 0.0,
                width: screen.0 as f32,
                height: screen.1 as f32,
            },
            |parent| parent.rect,
        );
    let rect = element.rect;
    Some((
        Placement {
            start: [
                (rect.x - parent.x) / per_unit,
                (rect.y - parent.y) / per_unit,
            ],
            size: [rect.width / per_unit, rect.height / per_unit],
            parent: [parent.width / per_unit, parent.height / per_unit],
        },
        element.in_flow,
    ))
}

/// The font `id`'s text is drawn in: its CSS `font-family`, with the
/// generic names as the fonts they stand for.
pub fn current_font(world: &World, id: InstanceId, stylesheets: &[Stylesheet]) -> String {
    let value = axcenity_ui::applied_declaration(world, id, stylesheets, "font-family")
        .map(|declaration| declaration.value)
        .unwrap_or_default();
    let first = value
        .split(',')
        .next()
        .unwrap_or_default()
        .trim()
        .trim_matches(['\'', '"']);
    match first.to_ascii_lowercase().as_str() {
        "" | "sans-serif" => "Montserrat".to_owned(),
        "monospace" => "JetBrains Mono".to_owned(),
        "serif" => "Lora".to_owned(),
        _ => first.to_owned(),
    }
}

impl UiDesigner {
    /// Elements to drag onto the artboard. A click puts one in the
    /// selected container instead.
    pub(super) fn palette(
        &mut self,
        ui: &mut egui::Ui,
        scene: &DesignerScene,
        gui: InstanceId,
        actions: &mut Vec<DesignerAction>,
    ) {
        widgets::section_label(ui, "Elements");
        let palette = theme::palette(ui.ctx());
        // As many equal cells to a row as fit.
        let gap = 4.0;
        let width = ui.available_width();
        let columns = ((width + gap) / (PALETTE_CELL + gap)).floor().max(1.0) as usize;
        let cell = egui::vec2(
            ((width - gap * (columns - 1) as f32) / columns as f32).max(1.0),
            ui.spacing().interact_size.y + 4.0,
        );
        let sense = if scene.editing {
            egui::Sense::click_and_drag()
        } else {
            egui::Sense::hover()
        };
        for row in PALETTE.chunks(columns) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for &(what, name) in row {
                    let (rect, response) = ui.allocate_exact_size(cell, sense);
                    widgets::paint_hover(ui, rect, &response, false, 6.0);
                    ui.painter().rect_stroke(
                        rect,
                        6.0,
                        egui::Stroke::new(1.0_f32, palette.border),
                        egui::StrokeKind::Inside,
                    );
                    let text = theme::with_icon(theme::class_icon(what.class()), name);
                    let color = if scene.editing {
                        palette.text
                    } else {
                        palette.text_muted
                    };
                    let galley = widgets::elided(
                        ui,
                        text.clone(),
                        egui::FontId::proportional(theme::type_scale::BODY),
                        color,
                        (rect.width() - 12.0).max(1.0),
                        1,
                    );
                    ui.painter().galley(
                        egui::pos2(rect.left() + 6.0, rect.center().y - galley.size().y * 0.5),
                        galley,
                        color,
                    );
                    response.dnd_set_drag_payload(PaletteItem(what));
                    if response.dragged()
                        && let Some(pointer) = ui.ctx().pointer_latest_pos()
                    {
                        // What's being carried, under the pointer.
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                        let layer = egui::LayerId::new(egui::Order::Tooltip, response.id);
                        ui.ctx().layer_painter(layer).text(
                            pointer + egui::vec2(12.0, 8.0),
                            egui::Align2::LEFT_TOP,
                            text,
                            egui::FontId::proportional(theme::type_scale::BODY),
                            palette.accent_text,
                        );
                    }
                    if response.clicked() {
                        let parent = container_for(scene.world, gui, scene.selected);
                        actions.push(DesignerAction::Insert {
                            what,
                            parent,
                            at: CLICK_INSERT_AT,
                        });
                    }
                    response.on_hover_text(format!(
                        "Drag a {name} onto the artboard, or click to add one to the selected frame"
                    ));
                }
            });
        }
    }

    /// The ScreenGui and everything in it. A click selects; a double-click
    /// on text types over it on the artboard. Dragging a row (or the rows
    /// selected with it) onto a container's row moves it in, keeping where
    /// it is on screen; onto any other row, into the container that row is
    /// in. The row it would go into is outlined while dragging.
    pub(super) fn hierarchy(
        &mut self,
        ui: &mut egui::Ui,
        scene: &DesignerScene,
        gui: InstanceId,
        actions: &mut Vec<DesignerAction>,
    ) {
        widgets::section_label(ui, "Hierarchy");
        let palette = theme::palette(ui.ctx());
        let row_height = ui.spacing().interact_size.y;
        let rows = hierarchy(scene.world, gui);
        let order: Vec<InstanceId> = rows.iter().map(|&(id, _)| id).collect();
        let chosen = self.chosen(scene, gui);
        // What a drag is carrying, and the row under the pointer.
        let carried = egui::DragAndDrop::payload::<HierarchyRows>(ui.ctx());
        let mut drop_onto = None;
        let mut row_rects = Vec::new();
        for (id, depth) in rows {
            let Some(instance) = scene.world.get(id) else {
                continue;
            };
            let sense = if scene.editing && id != gui {
                egui::Sense::click_and_drag()
            } else {
                egui::Sense::click()
            };
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(ui.available_width(), row_height), sense);
            response.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::WidgetType::SelectableLabel,
                    true,
                    scene.selected == Some(id),
                    instance.name(),
                )
            });
            row_rects.push((id, rect));
            let selected = scene.selected == Some(id) || chosen.contains(&id);
            widgets::paint_hover(ui, rect, &response, selected, 4.0);
            let indent = 6.0 + depth as f32 * 12.0;
            let text = theme::with_icon(theme::class_icon(instance.class()), instance.name());
            // Not drawn on the artboard: by its eye, or not the page shown.
            let off =
                self.hidden_on_artboard(scene.world, id) || !self.on_shown_page(scene.world, id);
            let galley = widgets::elided(
                ui,
                text,
                egui::FontId::proportional(theme::type_scale::BODY),
                if off {
                    palette.text_muted
                } else if selected {
                    palette.text_strong
                } else {
                    palette.text
                },
                // Room for the eye at the end.
                (rect.width() - indent - 28.0).max(1.0),
                1,
            );
            ui.painter().galley(
                egui::pos2(
                    rect.left() + indent,
                    rect.center().y - galley.size().y * 0.5,
                ),
                galley,
                palette.text,
            );
            if id != gui {
                self.eye(ui, scene.world, id, rect);
            }
            if response.clicked() {
                // Shift selects the rows between, Ctrl adds or takes one away.
                let modifiers = ui.input(|input| input.modifiers);
                if id != gui && (modifiers.shift || modifiers.command || modifiers.ctrl) {
                    let selection = self.extended(scene, gui, id, modifiers, Some(&order));
                    actions.push(DesignerAction::SelectMany(selection));
                } else {
                    actions.push(DesignerAction::Select(Some(id)));
                }
            }
            if response.double_clicked() && scene.editing {
                self.start_typing(scene, id);
            }
            if scene.editing && id != gui {
                // The rows selected with it go along.
                let moving = if chosen.contains(&id) && chosen.len() > 1 {
                    order
                        .iter()
                        .copied()
                        .filter(|row| chosen.contains(row))
                        .collect()
                } else {
                    vec![id]
                };
                response.dnd_set_drag_payload(HierarchyRows(moving));
            }
            if carried.is_some() && response.contains_pointer() {
                drop_onto = Some(id);
            }
            if scene.editing && id != gui {
                response.context_menu(|ui| {
                    super::order::menu_items(ui, scene.world, scene.stylesheets, id, actions);
                    self.selection_menu_items(ui, scene, gui, id, actions);
                });
            }
        }
        let (Some(carried), Some(onto)) = (carried, drop_onto) else {
            return;
        };
        let Some(target) = super::hierarchy_drop_target(scene.world, gui, &carried.0, onto) else {
            ui.ctx().set_cursor_icon(egui::CursorIcon::NoDrop);
            return;
        };
        if let Some(&(_, rect)) = row_rects.iter().find(|&&(id, _)| id == target) {
            ui.painter().rect_stroke(
                rect,
                4.0,
                egui::Stroke::new(1.5_f32, palette.accent),
                egui::StrokeKind::Inside,
            );
        }
        if ui.input(|input| input.pointer.any_released()) {
            egui::DragAndDrop::clear_payload(ui.ctx());
            actions.push(DesignerAction::MoveInto {
                ids: carried.0.clone(),
                parent: target,
            });
        }
    }

    /// The selected element's name, text, anchor and font, then its Style (CSS).
    pub(super) fn inspector(
        &mut self,
        ui: &mut egui::Ui,
        scene: &DesignerScene,
        gui: InstanceId,
        actions: &mut Vec<DesignerAction>,
    ) {
        let world = scene.world;
        let id = scene
            .selected
            .filter(|&id| id == gui || world.is_descendant_of(id, gui))
            .unwrap_or(gui);
        let Some(instance) = world.get(id) else {
            return;
        };
        widgets::section_label(ui, instance.class().as_str());
        ui.add_enabled_ui(scene.editing, |ui| {
            let has_text = instance.property("Text").and_then(Value::as_str).is_some();
            let fonts = has_text.then(|| self.font_choices(scene));
            responsive::form(ui, "ui designer inspector", |ui| {
                ui.label("Name");
                let mut name = instance.name().to_owned();
                let mut field = egui::TextEdit::singleline(&mut name)
                    .desired_width(f32::INFINITY)
                    .show(ui);
                if self.renaming == Some(id) {
                    // Rename, from a right-click menu: focused, all selected,
                    // so typing replaces it.
                    self.renaming = None;
                    field.response.request_focus();
                    let all = egui::text::CCursorRange::two(
                        egui::text::CCursor::new(0),
                        egui::text::CCursor::new(name.chars().count()),
                    );
                    field.state.cursor.set_char_range(Some(all));
                    field.state.store(ui.ctx(), field.response.id);
                }
                if field.response.changed() {
                    actions.push(DesignerAction::Edit(id, Edit::Name(name)));
                }
                ui.end_row();
                if let Some(text) = instance.property("Text").and_then(Value::as_str) {
                    ui.label("Text");
                    let mut text = text.to_owned();
                    if ui
                        .add(egui::TextEdit::singleline(&mut text).desired_width(f32::INFINITY))
                        .on_hover_text("Or double-click the text on the artboard")
                        .changed()
                    {
                        actions.push(DesignerAction::Edit(
                            id,
                            Edit::Property("Text", Value::String(text)),
                        ));
                    }
                    ui.end_row();
                }
                super::strings::text_key_row(ui, scene, id, actions);
                if let Some((built_in, project)) = &fonts {
                    ui.label("Font");
                    let current = current_font(world, id, scene.stylesheets);
                    let mut look_again = false;
                    if let Some(family) =
                        font_picker(ui, &current, built_in, project, &mut look_again)
                    {
                        actions.push(DesignerAction::Font { id, family });
                    }
                    if look_again {
                        scene.assets.forget_fonts();
                    }
                    ui.end_row();
                }
            });
            // Folded, States edits nothing: the text toolbar is Normal's.
            self.states_open = Section::States.is_open(ui.ctx());
            let state = self.state_for(scene, id);
            for section in sections(world, gui, id, scene.stylesheets) {
                self.section(ui, scene, gui, id, section, state, actions);
            }
            if scene.editing
                && let Some(edit) = style_panel::show(ui, world, Some(id), scene.stylesheets)
            {
                actions.push(DesignerAction::Style(id, edit));
            }
        });
    }

    /// Draws one of the inspector's sections, under its folding header.
    #[allow(clippy::too_many_arguments)]
    fn section(
        &mut self,
        ui: &mut egui::Ui,
        scene: &DesignerScene,
        gui: InstanceId,
        id: InstanceId,
        section: Section,
        state: UiState,
        actions: &mut Vec<DesignerAction>,
    ) {
        let title = match (section, state) {
            // The text toolbar edits the state picked under States.
            (Section::Text, UiState::Normal) => section.title().to_owned(),
            (Section::Text, state) => format!("{} · {}", section.title(), state.name()),
            _ => fold::title(scene, id, section),
        };
        fold(ui, section, &title, |ui| match section {
            Section::Styles => {
                super::style_sets::styles_section(ui, scene, id, &mut self.imported_styles, actions)
            }
            Section::Anchor => anchor_picker(ui, scene, gui, id, actions),
            Section::Text => text_toolbar::text_toolbar(ui, scene, id, state, actions),
            Section::Widget => super::widget_section::widget_section(ui, scene, id, actions),
            Section::Image => {
                let image = scene
                    .world
                    .get(id)
                    .and_then(|instance| instance.property("Image"))
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                responsive::form(ui, ("ui designer image", id), |ui| {
                    super::image::image_row(ui, scene, id, image, actions);
                });
            }
            Section::Look => look_section(ui, scene, id, actions),
            Section::Code => super::links::links_section(ui, scene, gui, id, actions),
            Section::States => self.states_editor(ui, scene, id, actions),
            Section::Bevel => super::bevel::bevel_editor(ui, scene, id, actions),
            Section::Slice => self.slice_editor(ui, scene, id, actions),
        });
    }

    /// The fonts to pick from: built in (family and kind), and the
    /// project's own families, read again when its font files change.
    fn font_choices(&mut self, scene: &DesignerScene) -> (Vec<(String, String)>, Vec<String>) {
        let built_in = axcenity_render::BUILT_IN_FONTS
            .iter()
            .map(|font| (font.family.to_owned(), font.kind.to_owned()))
            .collect();
        let files = scene.assets.fonts();
        let key = Arc::as_ptr(&files) as usize;
        if self
            .project_fonts
            .as_ref()
            .is_none_or(|(seen, _)| *seen != key)
        {
            let mut families: Vec<String> = files
                .iter()
                .flat_map(|file| axcenity_render::font_families(&file.data))
                .collect();
            families.sort();
            families.dedup();
            self.project_fonts = Some((key, families));
        }
        let project = self
            .project_fonts
            .as_ref()
            .map(|(_, families)| families.clone())
            .unwrap_or_default();
        (built_in, project)
    }
}

/// The selected element's turn and how see-through each of its parts is:
/// `Rotation` (degrees; the artboard's corner knobs turn it too) and its
/// background, text and image transparencies as 0–100% sliders.
fn look_section(
    ui: &mut egui::Ui,
    scene: &DesignerScene,
    id: InstanceId,
    actions: &mut Vec<DesignerAction>,
) {
    let Some(instance) = scene.world.get(id) else {
        return;
    };
    let number = |name| instance.property(name).and_then(Value::as_number);
    responsive::form(ui, "ui designer look", |ui| {
        if let Some(mut degrees) = number("Rotation") {
            ui.label("Rotation");
            if ui
                .add(
                    egui::DragValue::new(&mut degrees)
                        .speed(1.0)
                        .suffix("°")
                        .max_decimals(1),
                )
                .on_hover_text(
                    "Turns it and what's in it around its middle, clockwise. \
                     Or drag a knob just outside its corners (Shift snaps to 15°).",
                )
                .changed()
            {
                actions.push(DesignerAction::Edit(
                    id,
                    Edit::Property("Rotation", Value::Number(degrees)),
                ));
            }
            ui.end_row();
        }
    });
    let parts = [
        "BackgroundTransparency",
        "TextTransparency",
        "ImageTransparency",
    ];
    if parts.iter().all(|&name| number(name).is_none()) {
        return;
    }
    // Said once, so "Background 100%" isn't read as fully shown.
    ui.weak("Transparency: 0% is solid, 100% is invisible.");
    responsive::form(ui, "ui designer transparency", |ui| {
        for (name, label, tip) in [
            (
                "BackgroundTransparency",
                "Background",
                "How see-through the background is",
            ),
            ("TextTransparency", "Text", "How see-through the text is"),
            (
                "ImageTransparency",
                "Image",
                "How see-through the picture is",
            ),
        ] {
            let Some(value) = number(name) else {
                continue;
            };
            ui.label(label).on_hover_text(tip);
            if let Some(value) = crate::properties::transparency_slider(ui, value) {
                actions.push(DesignerAction::Edit(
                    id,
                    Edit::Property(name, Value::Number(value)),
                ));
            }
            ui.end_row();
        }
    });
}

/// A list of fonts, the built-in ones then the project's. Returns the
/// family picked. `look_again` is set when the project's fonts should be
/// read again.
fn font_picker(
    ui: &mut egui::Ui,
    current: &str,
    built_in: &[(String, String)],
    project: &[String],
    look_again: &mut bool,
) -> Option<String> {
    let mut picked = None;
    egui::ComboBox::from_id_salt("ui designer font")
        .width(ui.available_width().clamp(80.0, 200.0))
        .selected_text(current)
        .show_ui(ui, |ui| {
            widgets::section_label(ui, "Built in");
            for (family, kind) in built_in {
                let label = egui::RichText::new(format!("{family}  ·  {kind}"));
                if ui.selectable_label(family == current, label).clicked() {
                    picked = Some(family.clone());
                }
            }
            widgets::section_label(ui, "In this project");
            if project.is_empty() {
                ui.weak("Put .ttf or .otf files in the project folder to use them here.");
            }
            if ui
                .small_button("Look again")
                .on_hover_text("Read the project's font files again, after adding one")
                .clicked()
            {
                *look_again = true;
            }
            for family in project {
                if ui.selectable_label(family == current, family).clicked() {
                    picked = Some(family.clone());
                }
            }
        })
        .response
        .on_hover_text("The font this element's text is drawn in (CSS font-family)");
    picked
}

/// The inspector's sections for `id` in `gui`, in order, beginner-first:
/// the ready-made looks, where it's pinned, its text, value and picture,
/// how see-through it is and what it does; then the fine-tuning (states,
/// bevel, slice), which start folded. The ScreenGui has only Text, if any.
pub fn sections(
    world: &World,
    gui: InstanceId,
    id: InstanceId,
    stylesheets: &[Stylesheet],
) -> Vec<Section> {
    let Some(instance) = world.get(id) else {
        return Vec::new();
    };
    let has_text = instance.property("Text").and_then(Value::as_str).is_some();
    if id == gui {
        return if has_text {
            vec![Section::Text]
        } else {
            Vec::new()
        };
    }
    let class = instance.class();
    let has_image = instance.property("Image").and_then(Value::as_str).is_some();
    [
        (Section::Styles, true),
        (Section::Anchor, has_anchor(instance)),
        (Section::Text, has_text),
        (Section::Widget, class.is_value_widget()),
        (Section::Image, has_image),
        (Section::Look, true),
        (Section::Code, true),
        (
            Section::States,
            super::states::has_states(world, id, stylesheets),
        ),
        (Section::Bevel, true),
        (Section::Slice, super::slice::sliceable(class)),
    ]
    .into_iter()
    .filter_map(|(section, shown)| shown.then_some(section))
    .collect()
}

/// Whether `instance` can be anchored: it has a `Position` and a `Size`.
fn has_anchor(instance: &axcenity_core::Instance) -> bool {
    let udim2 = |name| instance.property(name).and_then(Value::as_udim2);
    udim2("Position").is_some() && udim2("Size").is_some()
}

/// Unity-style anchor presets: a 4×4 grid of where the element is pinned
/// across (left, centre, right, stretched) and down (top, middle, bottom,
/// stretched). Picking one keeps the element where it is.
fn anchor_picker(
    ui: &mut egui::Ui,
    scene: &DesignerScene,
    gui: InstanceId,
    id: InstanceId,
    actions: &mut Vec<DesignerAction>,
) {
    let world = scene.world;
    let Some(instance) = world.get(id) else {
        return;
    };
    let udim2 = |name| instance.property(name).and_then(Value::as_udim2);
    let (Some(position), Some(size)) = (udim2("Position"), udim2("Size")) else {
        return;
    };
    let Some((placed, in_flow)) = placement(world, scene.stylesheets, gui, id) else {
        return;
    };
    place_numbers(ui, scene, id, (position, size), placed, in_flow, actions);
    if in_flow {
        ui.weak("Its parent's flex layout places it, so it has no anchor.");
        return;
    }
    let current = (Pin::of(position.x, size.x), Pin::of(position.y, size.y));
    let palette = theme::palette(ui.ctx());
    ui.horizontal_top(|ui| {
        egui::Grid::new("ui designer anchors")
            .spacing([2.0, 2.0])
            .show(ui, |ui| {
                for y in Pin::ALL {
                    for x in Pin::ALL {
                        let chosen = current == (Some(x), Some(y));
                        let (rect, response) = ui.allocate_exact_size(
                            egui::vec2(ANCHOR_CELL, ANCHOR_CELL),
                            egui::Sense::click(),
                        );
                        widgets::paint_hover(ui, rect, &response, chosen, 3.0);
                        paint_pin_icon(ui.painter(), rect.shrink(5.0), x, y, &palette, chosen);
                        let response = response.on_hover_text(format!(
                            "Pin to the {} and the {}",
                            y.name(false),
                            x.name(true)
                        ));
                        if response.clicked()
                            && let Some(action) =
                                anchor_action(world, scene.stylesheets, gui, id, x, y)
                        {
                            actions.push(action);
                        }
                    }
                    ui.end_row();
                }
            });
        let said = match current {
            (Some(x), Some(y)) => format!(
                "Pinned to the {} and the {}: it keeps that distance on any screen.",
                y.name(false),
                x.name(true)
            ),
            _ => {
                let [x, y] = [anchor_of(position.x, size.x), anchor_of(position.y, size.y)];
                format!(
                    "Anchored at {} across and {} down: it keeps its distance from there.",
                    percent_range(x),
                    percent_range(y)
                )
            }
        };
        ui.add(egui::Label::new(egui::RichText::new(said).weak()).wrap());
    });
    if ui
        .button("Reset")
        .on_hover_text(
            "Anchor it to the top left, not stretched, without moving it. Or drag the diamonds on the artboard to anchor it anywhere.",
        )
        .clicked()
        && let Some(action) = reset_anchor_action(world, scene.stylesheets, gui, id)
    {
        actions.push(action);
    }
}

/// X, Y, W and H in design pixels: where the element's top left is from
/// its parent's, and its size, as on the artboard's rulers. Typing or
/// dragging a number moves or resizes it, keeping its anchor (the offsets
/// are worked out again, as pinning does). A flex child has only W and H;
/// a size its stylesheet sets is greyed, saying where it's set.
fn place_numbers(
    ui: &mut egui::Ui,
    scene: &DesignerScene,
    id: InstanceId,
    (position, size): (axcenity_core::UDim2, axcenity_core::UDim2),
    placed: Placement,
    in_flow: bool,
    actions: &mut Vec<DesignerAction>,
) {
    let tidy = |value: f32| (value * 10.0).round() / 10.0;
    let width_axis = crate::ui_handles::Axis::of(scene.world, id, scene.stylesheets, "width");
    let height_axis = crate::ui_handles::Axis::of(scene.world, id, scene.stylesheets, "height");
    // (label, value now, hover, whether it can be changed, axis 0 or 1, a size?)
    let fields = [
        (
            "X",
            placed.start[0],
            "Left edge, in design pixels from its parent's left",
            !in_flow,
            0,
            false,
        ),
        (
            "Y",
            placed.start[1],
            "Top edge, in design pixels from its parent's top",
            !in_flow,
            1,
            false,
        ),
        (
            "W",
            placed.size[0],
            "Width, in design pixels",
            true,
            0,
            true,
        ),
        (
            "H",
            placed.size[1],
            "Height, in design pixels",
            true,
            1,
            true,
        ),
    ];
    ui.horizontal_wrapped(|ui| {
        for (label, now, hover, movable, axis, resize) in fields {
            let css = match (resize, axis) {
                (true, 0) => &width_axis,
                (true, _) => &height_axis,
                _ => &crate::ui_handles::Axis::Size,
            };
            let set_by_css = match css {
                crate::ui_handles::Axis::Size => None,
                crate::ui_handles::Axis::Css(declaration)
                | crate::ui_handles::Axis::Locked(declaration) => Some(declaration),
            };
            let enabled = movable && set_by_css.is_none();
            let hover = match set_by_css {
                Some(declaration) => format!(
                    "Its stylesheet sets this ({} {{ {}: {} }}). Change it under Style, or \
                     drag its handle on the artboard.",
                    declaration.selector, declaration.property, declaration.value
                ),
                None if !movable => {
                    "Its parent's flex layout places it, so it can't be moved here.".to_owned()
                }
                None => hover.to_owned(),
            };
            let mut value = tidy(now);
            let response = ui
                .add_enabled(
                    enabled,
                    egui::DragValue::new(&mut value)
                        .prefix(format!("{label} "))
                        .speed(1.0)
                        .max_decimals(1)
                        .range(if resize {
                            0.0..=f32::MAX
                        } else {
                            f32::MIN..=f32::MAX
                        }),
                )
                .on_hover_text(&hover)
                .on_disabled_hover_text(&hover);
            // Named for screen readers (and tests) by its letter.
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::DragValue, enabled, label)
            });
            if response.changed() && value != tidy(now) {
                let anchor = anchor_of(
                    if axis == 0 { position.x } else { position.y },
                    if axis == 0 { size.x } else { size.y },
                );
                let (start, length) = if resize {
                    (placed.start[axis], value)
                } else {
                    (value, placed.size[axis])
                };
                let (along, across) =
                    super::anchors::anchor_axis(anchor, start, length, placed.parent[axis]);
                let (mut position, mut size) = (position, size);
                if axis == 0 {
                    (position.x, size.x) = (along, across);
                } else {
                    (position.y, size.y) = (along, across);
                }
                actions.push(DesignerAction::Place {
                    id,
                    position,
                    size,
                    resize,
                });
            }
        }
    });
}

/// An anchor on one axis in words: `40%`, or `30% to 70%` when stretched.
fn percent_range([min, max]: [f32; 2]) -> String {
    if min == max {
        format!("{:.0}%", min * 100.0)
    } else {
        format!("{:.0}% to {:.0}%", min * 100.0, max * 100.0)
    }
}

/// A cell's picture: the parent as a box, and where the element is pinned
/// in it (a dot, or a bar for a stretched axis).
fn paint_pin_icon(
    painter: &egui::Painter,
    rect: egui::Rect,
    x: Pin,
    y: Pin,
    palette: &theme::Palette,
    chosen: bool,
) {
    let color = if chosen {
        palette.accent_text
    } else {
        palette.text_weak
    };
    painter.rect_stroke(
        rect,
        1.0,
        egui::Stroke::new(1.0_f32, palette.border_strong),
        egui::StrokeKind::Inside,
    );
    let along = |pin: Pin, start: f32, end: f32| match pin {
        Pin::Start => (start + 2.0, start + 2.0),
        Pin::Center => ((start + end) * 0.5, (start + end) * 0.5),
        Pin::End => (end - 2.0, end - 2.0),
        Pin::Stretch => (start + 2.0, end - 2.0),
    };
    let (x0, x1) = along(x, rect.left(), rect.right());
    let (y0, y1) = along(y, rect.top(), rect.bottom());
    let mark = egui::Rect::from_min_max(egui::pos2(x0, y0), egui::pos2(x1, y1)).expand(1.5);
    painter.rect_filled(mark, 1.0, color);
}

#[cfg(test)]
mod tests {
    use axcenity_core::{ClassName, UDim2};

    use super::*;

    #[test]
    fn placement_is_in_design_pixels_from_the_parent_and_fonts_read_back() {
        let mut world = World::new();
        let gui = world.create(ClassName::ScreenGui).unwrap();
        world.set_parent(gui, Some(world.workspace())).unwrap();
        for (name, value) in [
            ("ScaleMode", Value::String("Scale".into())),
            ("DesignWidth", Value::Number(800.0)),
            ("DesignHeight", Value::Number(600.0)),
        ] {
            world.set_property(gui, name, value).unwrap();
        }
        let frame = world.create(ClassName::Frame).unwrap();
        world.set_parent(frame, Some(gui)).unwrap();
        world
            .set_property(
                frame,
                "Position",
                Value::UDim2(UDim2::new(0.0, 100.0, 0.0, 50.0)),
            )
            .unwrap();
        let label = world.create(ClassName::TextLabel).unwrap();
        world.set_parent(label, Some(frame)).unwrap();
        world
            .set_property(
                label,
                "Position",
                Value::UDim2(UDim2::new(0.0, 10.0, 0.0, 20.0)),
            )
            .unwrap();
        world
            .set_property(
                label,
                "Size",
                Value::UDim2(UDim2::new(0.0, 60.0, 0.0, 30.0)),
            )
            .unwrap();
        world
            .set_property(label, "Id", Value::String("Title".into()))
            .unwrap();

        let (frame_at, _) = placement(&world, &[], gui, frame).unwrap();
        assert_eq!(frame_at.start, [100.0, 50.0]);
        assert_eq!(frame_at.parent, [800.0, 600.0]);
        let (label_at, in_flow) = placement(&world, &[], gui, label).unwrap();
        assert!(!in_flow);
        assert_eq!(label_at.start, [10.0, 20.0]);
        assert_eq!(label_at.size, [60.0, 30.0]);
        assert_eq!(label_at.parent, [200.0, 100.0], "the Frame's default size");

        assert_eq!(current_font(&world, label, &[]), "Montserrat");
        let (sheet, _) = axcenity_ui::parse_stylesheet(
            "ui.css",
            "#Title { font-family: \"Bebas Neue\", serif }",
        );
        assert_eq!(current_font(&world, label, &[sheet]), "Bebas Neue");
        let (sheet, _) = axcenity_ui::parse_stylesheet("ui.css", "#Title { font-family: serif }");
        assert_eq!(current_font(&world, label, &[sheet]), "Lora");
    }
}
