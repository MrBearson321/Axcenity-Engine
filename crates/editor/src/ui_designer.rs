//! The UI Designer tab: one ScreenGui on an artboard at its design size
//! (`DesignWidth` × `DesignHeight`), like a design tool's frame, with zoom
//! and pan. Presets set the design size to common screens, or type your own.
//!
//! Around the artboard are a palette of UI elements to drag onto it, the
//! ScreenGui's hierarchy, and an inspector with the element's name and
//! text and the same Style (CSS) section as Properties. The selected
//! element gets the viewport's handles, with the same snapping
//! (`ui_handles.rs`).
//!
//! The designer only reads the world. Everything done in it comes back as
//! [`DesignerAction`]s, which the app applies through `EditorApp::change`,
//! the tree edits and the stylesheet tabs, so each is an undo step and the
//! Scene View and Play show the same UI.
//!
//! The artboard is laid out with `axcenity_ui::layout_artboard`: exactly
//! what a screen of the design size shows in Play, scaled by the zoom.

use axcenity_assets::Assets;
use axcenity_core::{ClassName, InstanceId, Value, World};
use axcenity_render::glam::Vec3;
use axcenity_render::{Camera, UiDrawItem, UiRectangle};
use axcenity_ui::{Stylesheet, UiLayout, UiRect};
use eframe::egui;
use eframe::egui_wgpu::RenderState;

use crate::animation::preview::Canvas;
use crate::properties::Edit;
use crate::responsive;
use crate::style_panel::{self, StyleEdit};
use crate::theme::{self, icons};
use crate::tree::Insertable;
use crate::ui_handles::{self, Grip, Handles, SnapGuide, UiDrag, UiDragEdit};
use crate::widgets;

/// Design sizes to pick from: name, width and height.
pub const PRESETS: [(&str, u32, u32); 4] = [
    ("Desktop 1920 × 1080", 1920, 1080),
    ("Desktop 1280 × 720", 1280, 720),
    ("Phone portrait 390 × 844", 390, 844),
    ("Phone landscape 844 × 390", 844, 390),
];

/// What the palette offers, with its name there.
pub const PALETTE: [(Insertable, &str); 8] = [
    (Insertable::Frame, "Frame"),
    (Insertable::TextLabel, "Text"),
    (Insertable::TextButton, "Button"),
    (Insertable::ImageLabel, "Image"),
    (Insertable::TextBox, "TextBox"),
    (Insertable::PageLayout, "Pages"),
    (Insertable::Page, "Page"),
    (Insertable::Table, "Table"),
];

const MIN_ZOOM: f32 = 0.05;
const MAX_ZOOM: f32 = 8.0;
/// The space left around the artboard when it's fitted into view, in points.
const FIT_MARGIN: f32 = 24.0;
/// Below this width the artboard goes on top and the panels under it.
const STACK_BELOW: f32 = 640.0;
/// The artboard's colour behind the UI (sRGB): a mid grey, so dark and
/// light UIs both stand out. In Play the 3D world is behind it.
const BACKDROP: [f32; 4] = [0.30, 0.32, 0.36, 1.0];
/// The narrowest a palette cell gets, in points.
const PALETTE_CELL: f32 = 88.0;
/// Where a palette item clicked (not dragged) goes in its parent, in design pixels.
const CLICK_INSERT_AT: [f32; 2] = [24.0, 24.0];

/// Something done in the designer, for the app to apply.
#[derive(Clone, Debug, PartialEq)]
pub enum DesignerAction {
    /// Selects this (or nothing).
    Select(Option<InstanceId>),
    /// Sets the ScreenGui's `DesignWidth` and `DesignHeight`.
    DesignSize {
        gui: InstanceId,
        width: u32,
        height: u32,
    },
    /// Sets the ScreenGui's `ScaleMode` to `Fit`.
    UseFit(InstanceId),
    /// Puts a new `what` into `parent`, its top left `at` design pixels
    /// from the parent's.
    Insert {
        what: Insertable,
        parent: InstanceId,
        at: [f32; 2],
    },
    /// A handle dragged on the artboard.
    Drag(UiDragEdit),
    /// The inspector's name or text.
    Edit(InstanceId, Edit),
    /// The inspector's Style section.
    Style(InstanceId, StyleEdit),
    /// Makes a ScreenGui, when there's none to design.
    NewScreenGui,
}

/// What's dragged out of the palette.
#[derive(Clone, Copy, Debug)]
pub struct PaletteItem(pub Insertable);

/// What the designer needs from the app to draw.
pub struct DesignerScene<'a> {
    pub world: &'a World,
    pub selected: Option<InstanceId>,
    pub stylesheets: &'a [Stylesheet],
    pub assets: &'a Assets,
    /// False during Play: the designer shows the game, but doesn't edit it.
    pub editing: bool,
}

/// The designer's view and the drag under way. Kept while the editor runs.
pub struct UiDesigner {
    /// The ScreenGui on the artboard.
    gui: Option<InstanceId>,
    /// The selection last followed, so a change is followed once.
    followed: Option<InstanceId>,
    /// Points per design pixel.
    pub zoom: f32,
    /// The artboard's top left, from the canvas's top left, in points.
    pan: egui::Vec2,
    /// What was fitted into view last: the ScreenGui and its design size.
    /// Another one (or a new size) is fitted again.
    fitted: Option<(InstanceId, (u32, u32))>,
    /// The canvas size it was fitted into. While the view hasn't been
    /// zoomed or panned by hand, a resized canvas fits it again.
    fitted_room: egui::Vec2,
    /// The view was zoomed or panned by hand since it was fitted.
    moved: bool,
    /// A zoom asked for by the toolbar, about the canvas's middle.
    zoom_by: Option<f32>,
    /// Set by Fit, to fit the artboard on the next frame.
    fit_next: bool,
    canvas: Canvas,
    drag: Option<UiDrag>,
    guides: Vec<SnapGuide>,
    /// Where the artboard was drawn last, in points.
    board: Option<egui::Rect>,
    /// Nothing but sky, for the renderer's 3D part (the artboard covers it).
    empty: World,
}

impl Default for UiDesigner {
    fn default() -> Self {
        Self {
            gui: None,
            followed: None,
            zoom: 0.5,
            pan: egui::vec2(FIT_MARGIN, FIT_MARGIN),
            fitted: None,
            fitted_room: egui::Vec2::ZERO,
            moved: false,
            zoom_by: None,
            fit_next: true,
            canvas: Canvas::default(),
            drag: None,
            guides: Vec::new(),
            board: None,
            empty: World::new(),
        }
    }
}

/// The ScreenGui `id` is or is inside, if it's in the Workspace.
pub fn screen_gui_of(world: &World, id: Option<InstanceId>) -> Option<InstanceId> {
    let mut current = id;
    while let Some(id) = current {
        let instance = world.get(id)?;
        if instance.class() == ClassName::ScreenGui {
            return world.is_descendant_of(id, world.workspace()).then_some(id);
        }
        current = instance.parent();
    }
    None
}

/// Every ScreenGui in the Workspace, in tree order.
fn screen_guis(world: &World) -> Vec<InstanceId> {
    world
        .descendants(world.workspace())
        .into_iter()
        .filter(|&id| {
            world
                .get(id)
                .is_some_and(|instance| instance.class() == ClassName::ScreenGui)
        })
        .collect()
}

/// True for the classes a dropped element can go inside.
fn is_container(class: ClassName) -> bool {
    matches!(
        class,
        ClassName::ScreenGui | ClassName::Frame | ClassName::PageLayout | ClassName::Table
    )
}

/// The container a new element goes in when `id` is under the pointer or
/// selected: `id` itself, or the nearest container around it, within `gui`.
pub fn container_for(world: &World, gui: InstanceId, id: Option<InstanceId>) -> InstanceId {
    let mut current = id;
    while let Some(id) = current {
        if id == gui {
            break;
        }
        if !world.is_descendant_of(id, gui) {
            return gui;
        }
        let Some(instance) = world.get(id) else {
            break;
        };
        if is_container(instance.class()) {
            return id;
        }
        current = instance.parent();
    }
    gui
}

/// Where a drop at `pixel` (artboard layout pixels) lands in `parent`, in
/// design pixels from its top left. The ScreenGui's own corner is the
/// artboard's `origin`.
pub fn drop_offset(
    layout: &UiLayout,
    parent: InstanceId,
    pixel: [f32; 2],
    origin: [f32; 2],
    scale: f32,
) -> [f32; 2] {
    let (corner, per_unit) = layout.element(parent).map_or((origin, scale), |element| {
        ([element.rect.x, element.rect.y], element.pixels_per_unit)
    });
    let per_unit = per_unit.max(1e-4);
    [
        ((pixel[0] - corner[0]) / per_unit).round(),
        ((pixel[1] - corner[1]) / per_unit).round(),
    ]
}

/// One row of the hierarchy: an element and how deep it is under the ScreenGui.
fn hierarchy(world: &World, gui: InstanceId) -> Vec<(InstanceId, usize)> {
    let mut rows = Vec::new();
    let mut stack = vec![(gui, 0)];
    while let Some((id, depth)) = stack.pop() {
        let Some(instance) = world.get(id) else {
            continue;
        };
        rows.push((id, depth));
        for &child in instance.children().iter().rev() {
            stack.push((child, depth + 1));
        }
    }
    rows
}

impl UiDesigner {
    /// The ScreenGui on the artboard.
    #[cfg(test)]
    pub fn gui(&self) -> Option<InstanceId> {
        self.gui
    }

    /// Where the artboard was drawn last, in points, and the zoom.
    #[cfg(test)]
    pub fn board(&self) -> Option<(egui::Rect, f32)> {
        Some((self.board?, self.zoom))
    }

    /// Shows the ScreenGui the selection is in, when the selection changes,
    /// and keeps showing it while something outside UI is selected. With
    /// none shown (or it's gone), shows the Workspace's first.
    pub fn follow(&mut self, world: &World, selected: Option<InstanceId>) {
        if selected != self.followed {
            self.followed = selected;
            if let Some(gui) = screen_gui_of(world, selected) {
                self.gui = Some(gui);
            }
        }
        if screen_gui_of(world, self.gui) != self.gui || self.gui.is_none() {
            self.gui = screen_guis(world).first().copied();
        }
    }

    /// Fits the artboard into view on the next frame.
    pub fn fit(&mut self) {
        self.fit_next = true;
    }

    /// True while a handle is being dragged.
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Draws the designer and returns what was done in it.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        scene: &DesignerScene,
        render_state: Option<&RenderState>,
    ) -> Vec<DesignerAction> {
        let mut actions = Vec::new();
        self.follow(scene.world, scene.selected);
        let Some(gui) = self.gui else {
            self.drag = None;
            ui.label("There's no ScreenGui to design yet.");
            if ui
                .add_enabled(
                    scene.editing,
                    widgets::button(
                        widgets::Role::Primary,
                        Some(icons::PLUS),
                        "New ScreenGui",
                        &theme::palette(ui.ctx()),
                    ),
                )
                .clicked()
            {
                actions.push(DesignerAction::NewScreenGui);
            }
            return actions;
        };
        self.toolbar(ui, scene, gui, &mut actions);
        self.notes(ui, scene, gui, &mut actions);
        ui.add_space(2.0);

        let width = ui.available_width();
        if width < STACK_BELOW {
            // Tall and narrow: the artboard on top, the panels under it.
            let height = (ui.available_height() * 0.45).clamp(140.0, 420.0);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
            self.artboard(ui, rect, scene, gui, render_state, &mut actions);
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt("ui designer panels")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    self.palette(ui, scene, gui, &mut actions);
                    ui.separator();
                    self.hierarchy(ui, scene, gui, &mut actions);
                    ui.separator();
                    self.inspector(ui, scene, gui, &mut actions);
                });
            return actions;
        }
        // Wide: palette and hierarchy, the artboard, then the inspector.
        let height = ui.available_height();
        let side = (width * 0.2).clamp(150.0, 220.0);
        let inspector = (width * 0.26).clamp(200.0, 300.0);
        ui.horizontal_top(|ui| {
            let top_down = egui::Layout::top_down(egui::Align::Min);
            ui.allocate_ui_with_layout(egui::vec2(side, height), top_down, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("ui designer left")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        self.palette(ui, scene, gui, &mut actions);
                        ui.separator();
                        self.hierarchy(ui, scene, gui, &mut actions);
                    });
            });
            ui.separator();
            let middle =
                (ui.available_width() - inspector - ui.spacing().item_spacing.x * 2.0).max(80.0);
            let (rect, _) =
                ui.allocate_exact_size(egui::vec2(middle, height), egui::Sense::hover());
            self.artboard(ui, rect, scene, gui, render_state, &mut actions);
            ui.separator();
            ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), height), top_down, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("ui designer right")
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.inspector(ui, scene, gui, &mut actions));
            });
        });
        actions
    }

    /// The ScreenGui picker, the design size and the zoom.
    fn toolbar(
        &mut self,
        ui: &mut egui::Ui,
        scene: &DesignerScene,
        gui: InstanceId,
        actions: &mut Vec<DesignerAction>,
    ) {
        let world = scene.world;
        let (width, height) = axcenity_ui::design_size(world, gui);
        responsive::toolbar(ui, |ui| {
            let guis = screen_guis(world);
            if guis.len() > 1 {
                responsive::room_for(ui, 130.0);
                let name = |id| world.get(id).map_or("", |instance| instance.name());
                egui::ComboBox::from_id_salt("ui designer screen gui")
                    .width(110.0)
                    .selected_text(name(gui))
                    .show_ui(ui, |ui| {
                        for id in guis {
                            if ui.selectable_label(id == gui, name(id)).clicked() {
                                self.gui = Some(id);
                                actions.push(DesignerAction::Select(Some(id)));
                            }
                        }
                    });
            }
            ui.add_enabled_ui(scene.editing, |ui| {
                let preset = PRESETS
                    .iter()
                    .find(|(_, w, h)| (*w, *h) == (width, height))
                    .map_or("Custom", |(name, _, _)| name);
                responsive::room_for(ui, 180.0);
                egui::ComboBox::from_id_salt("ui designer size")
                    .width(170.0)
                    .selected_text(preset)
                    .show_ui(ui, |ui| {
                        for &(name, w, h) in &PRESETS {
                            if ui
                                .selectable_label((w, h) == (width, height), name)
                                .clicked()
                            {
                                actions.push(DesignerAction::DesignSize {
                                    gui,
                                    width: w,
                                    height: h,
                                });
                            }
                        }
                    })
                    .response
                    .on_hover_text("The screen size this UI is designed at");
                // Custom sizes: type or drag either number.
                let (mut w, mut h) = (width, height);
                responsive::room_for(ui, 150.0);
                let changed_w = ui
                    .add(egui::DragValue::new(&mut w).range(1..=8192).speed(2.0))
                    .on_hover_text("Design width, in pixels")
                    .changed();
                ui.label("×");
                let changed_h = ui
                    .add(egui::DragValue::new(&mut h).range(1..=8192).speed(2.0))
                    .on_hover_text("Design height, in pixels")
                    .changed();
                if changed_w || changed_h {
                    actions.push(DesignerAction::DesignSize {
                        gui,
                        width: w,
                        height: h,
                    });
                }
            });
            responsive::room_for(ui, 130.0);
            if widgets::icon_button(ui, icons::MAGNIFYING_GLASS_MINUS, "Zoom out", false).clicked()
            {
                self.zoom_by = Some(1.0 / 1.25);
            }
            ui.label(format!("{:.0}%", self.zoom * 100.0));
            if widgets::icon_button(ui, icons::MAGNIFYING_GLASS_PLUS, "Zoom in", false).clicked() {
                self.zoom_by = Some(1.25);
            }
            if widgets::icon_button(ui, icons::CORNERS_OUT, "Fit the artboard into view", false)
                .clicked()
            {
                self.fit();
            }
        });
    }

    /// What will differ in Play: a hidden layer, or one that doesn't scale.
    fn notes(
        &mut self,
        ui: &mut egui::Ui,
        scene: &DesignerScene,
        gui: InstanceId,
        actions: &mut Vec<DesignerAction>,
    ) {
        let Some(instance) = scene.world.get(gui) else {
            return;
        };
        if instance.property("Enabled").and_then(Value::as_bool) == Some(false) {
            ui.weak("Enabled is off: this ScreenGui is hidden in Play.");
        }
        let fixed = instance
            .property("ScaleMode")
            .and_then(Value::as_str)
            .is_none_or(|mode| !mode.eq_ignore_ascii_case("Fit"));
        if fixed {
            ui.horizontal_wrapped(|ui| {
                ui.weak(
                    "Fixed: Play lays this out at the window's size. Fit scales this \
                     design to fit any window.",
                );
                if ui
                    .add_enabled(scene.editing, egui::Button::new("Use Fit"))
                    .clicked()
                {
                    actions.push(DesignerAction::UseFit(gui));
                }
            });
        }
    }

    /// The artboard in `rect`: the ScreenGui drawn at its design size, the
    /// selected element's handles, and drops from the palette.
    fn artboard(
        &mut self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        scene: &DesignerScene,
        gui: InstanceId,
        render_state: Option<&RenderState>,
        actions: &mut Vec<DesignerAction>,
    ) {
        let world = scene.world;
        let palette = theme::palette(ui.ctx());
        let ppp = ui.ctx().pixels_per_point();
        let design = axcenity_ui::design_size(world, gui);
        let response = ui.interact(
            rect,
            ui.id().with("ui designer artboard"),
            egui::Sense::click_and_drag(),
        );

        // Fitted when it's new, resized or asked for, and when the canvas
        // changes size if it hasn't been moved by hand.
        let resized = (rect.size() - self.fitted_room).length() > 0.5;
        if self.fit_next || self.fitted != Some((gui, design)) || (resized && !self.moved) {
            self.fit_next = false;
            self.moved = false;
            self.fitted = Some((gui, design));
            self.fitted_room = rect.size();
            let room =
                (rect.size() - egui::Vec2::splat(FIT_MARGIN * 2.0)).max(egui::vec2(1.0, 1.0));
            self.zoom = (room.x / design.0 as f32)
                .min(room.y / design.1 as f32)
                .clamp(MIN_ZOOM, MAX_ZOOM);
            let size = egui::vec2(design.0 as f32, design.1 as f32) * self.zoom;
            self.pan = (rect.size() - size) * 0.5;
        }
        // Right- or middle-drag and the wheel pan; Ctrl + wheel zooms.
        let mut pan = egui::Vec2::ZERO;
        let mut zoom_at = self.zoom_by.take().map(|by| (by, rect.center()));
        if response.dragged_by(egui::PointerButton::Secondary)
            || response.dragged_by(egui::PointerButton::Middle)
        {
            pan += response.drag_delta();
        }
        if response.hovered() {
            let (scroll, zoom, pointer) =
                ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.pointer.hover_pos()));
            pan += scroll;
            if zoom != 1.0
                && let Some(pointer) = pointer
            {
                zoom_at = Some((zoom, pointer));
            }
        }
        if pan != egui::Vec2::ZERO {
            self.pan += pan;
            self.moved = true;
        }
        if let Some((by, at)) = zoom_at {
            let old = self.zoom;
            self.zoom = (self.zoom * by).clamp(MIN_ZOOM, MAX_ZOOM);
            let from_corner = at - rect.min - self.pan;
            self.pan = at - rect.min - from_corner * (self.zoom / old);
            self.moved = true;
        }

        let board = egui::Rect::from_min_size(
            rect.min + self.pan,
            egui::vec2(design.0 as f32, design.1 as f32) * self.zoom,
        );
        let visible = board.intersect(rect);
        self.board = Some(board);
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, palette.inset);
        if !visible.is_positive() {
            self.drag = None;
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "The artboard is out of view: Fit brings it back.",
                egui::FontId::proportional(theme::type_scale::SMALL),
                palette.text_weak,
            );
            return;
        }
        // Laid out in the pixels of a texture the size of `visible`.
        let scale = self.zoom * ppp;
        let origin = (board.min - visible.min) * ppp;
        let layout = axcenity_ui::layout_artboard(
            world,
            gui,
            scene.stylesheets,
            scale,
            [origin.x, origin.y],
        );
        let screen = UiRect {
            x: origin.x,
            y: origin.y,
            width: design.0 as f32 * scale,
            height: design.1 as f32 * scale,
        };
        let pixel_of = |pointer: egui::Pos2| {
            let at = (pointer - visible.min) * ppp;
            [at.x, at.y]
        };

        let handles = scene
            .selected
            .filter(|_| scene.editing)
            .and_then(|id| layout.element(id))
            .map(|&element| Handles::new(element, world, scene.stylesheets, visible.min, ppp));
        let (pressed, down, pointer) = ui.input(|input| {
            (
                input.pointer.primary_pressed(),
                input.pointer.primary_down(),
                input.pointer.latest_pos(),
            )
        });
        let over =
            pointer.filter(|&pointer| response.contains_pointer() && visible.contains(pointer));
        let grip = match (over, &handles) {
            (Some(pointer), Some(handles)) => handles.grip_at(pointer),
            _ => None,
        };

        // A press on the selection's handles drags them. A press anywhere
        // else selects what's there, and can drag it straight away.
        if pressed
            && self.drag.is_none()
            && scene.editing
            && let Some(pointer) = over
        {
            if let (Some(grip), Some(handles)) = (grip, &handles) {
                self.drag = UiDrag::start(handles, grip, pointer, world);
            } else {
                let [x, y] = pixel_of(pointer);
                let hit = layout.hit(x, y);
                actions.push(DesignerAction::Select(Some(hit.unwrap_or(gui))));
                if let Some(element) = hit.and_then(|id| layout.element(id)) {
                    let handles =
                        Handles::new(*element, world, scene.stylesheets, visible.min, ppp);
                    self.drag = UiDrag::start(&handles, Grip::BODY, pointer, world);
                }
            }
        }
        if let Some(drag) = &self.drag {
            if !down || !scene.editing {
                self.drag = None;
                self.guides.clear();
            } else if let Some(pointer) = pointer {
                let edit = drag.update(pointer, ppp, world, &layout.elements, screen);
                self.guides = edit.guides.clone();
                // An unmoved click selects without a step.
                if !edit.values.is_empty() || !edit.css.is_empty() {
                    actions.push(DesignerAction::Drag(edit));
                }
            }
        }
        if scene.editing
            && let Some(item) = response.dnd_release_payload::<PaletteItem>()
            && let Some(pointer) = over
        {
            let pixel = pixel_of(pointer);
            let parent = container_for(world, gui, layout.hit(pixel[0], pixel[1]));
            let at = drop_offset(&layout, parent, pixel, [origin.x, origin.y], scale);
            actions.push(DesignerAction::Insert {
                what: item.0,
                parent,
                at,
            });
        }

        // Drawn: the backdrop, then the UI, by the game's renderer when
        // there is one.
        let backdrop = UiDrawItem::Rectangle(UiRectangle {
            position: [origin.x, origin.y],
            size: [screen.width, screen.height],
            color: BACKDROP,
            color_end: BACKDROP,
            border_color: [0.0; 4],
            border_width: 0.0,
            border_radius: 0.0,
        });
        match render_state {
            Some(render_state) => {
                let camera = Camera {
                    position: Vec3::new(0.0, 5.0, 10.0),
                    look_at: Vec3::ZERO,
                    fov_y_degrees: 70.0,
                };
                let mut list = axcenity_scene::build_draw_list(&self.empty, scene.assets, camera);
                list.ui = std::iter::once(backdrop)
                    .chain(layout.draw_items.iter().cloned())
                    .collect();
                axcenity_scene::attach_ui_textures(&mut list.ui, scene.assets);
                self.canvas.paint(ui, visible, render_state, &list);
            }
            None => sketch(
                &painter,
                visible.min,
                ppp,
                std::iter::once(&backdrop).chain(&layout.draw_items),
            ),
        }
        painter.rect_stroke(
            board,
            0.0,
            egui::Stroke::new(1.0_f32, palette.border_strong),
            egui::StrokeKind::Outside,
        );
        painter.text(
            board.left_top() + egui::vec2(0.0, -4.0),
            egui::Align2::LEFT_BOTTOM,
            format!("{} × {}", design.0, design.1),
            egui::FontId::proportional(theme::type_scale::CAPTION),
            palette.text_weak,
        );
        let painter = ui.painter_at(visible);
        if let Some(handles) = &handles {
            let highlighted = self.drag.as_ref().map(|drag| drag.grip).or(grip);
            handles.paint(&painter, palette.accent, highlighted);
        }
        if self.drag.is_some() {
            ui_handles::paint_guides(&self.guides, &painter, visible.min, ppp);
        }
        match (&self.drag, &handles) {
            (Some(drag), Some(handles)) => ui.ctx().set_cursor_icon(handles.cursor(drag.grip)),
            (None, Some(handles)) if grip.is_some_and(|g| g != Grip::BODY) => {
                ui.ctx()
                    .set_cursor_icon(handles.cursor(grip.expect("checked")));
            }
            _ => {}
        }
        if response.dnd_hover_payload::<PaletteItem>().is_some() {
            painter.rect_stroke(
                visible,
                0.0,
                egui::Stroke::new(2.0_f32, palette.accent),
                egui::StrokeKind::Inside,
            );
        }
    }

    /// Elements to drag onto the artboard. A click puts one in the
    /// selected container instead.
    fn palette(
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

    /// The ScreenGui and everything in it. A click selects.
    fn hierarchy(
        &mut self,
        ui: &mut egui::Ui,
        scene: &DesignerScene,
        gui: InstanceId,
        actions: &mut Vec<DesignerAction>,
    ) {
        widgets::section_label(ui, "Hierarchy");
        let palette = theme::palette(ui.ctx());
        let row_height = ui.spacing().interact_size.y;
        for (id, depth) in hierarchy(scene.world, gui) {
            let Some(instance) = scene.world.get(id) else {
                continue;
            };
            let (rect, response) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), row_height),
                egui::Sense::click(),
            );
            let selected = scene.selected == Some(id);
            widgets::paint_hover(ui, rect, &response, selected, 4.0);
            let indent = 6.0 + depth as f32 * 12.0;
            let text = theme::with_icon(theme::class_icon(instance.class()), instance.name());
            let galley = widgets::elided(
                ui,
                text,
                egui::FontId::proportional(theme::type_scale::BODY),
                if selected {
                    palette.text_strong
                } else {
                    palette.text
                },
                (rect.width() - indent - 4.0).max(1.0),
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
            if response.clicked() {
                actions.push(DesignerAction::Select(Some(id)));
            }
        }
    }

    /// The selected element's name and text, and its Style (CSS).
    fn inspector(
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
            responsive::form(ui, "ui designer inspector", |ui| {
                ui.label("Name");
                let mut name = instance.name().to_owned();
                if ui
                    .add(egui::TextEdit::singleline(&mut name).desired_width(f32::INFINITY))
                    .changed()
                {
                    actions.push(DesignerAction::Edit(id, Edit::Name(name)));
                }
                ui.end_row();
                if let Some(text) = instance.property("Text").and_then(Value::as_str) {
                    ui.label("Text");
                    let mut text = text.to_owned();
                    if ui
                        .add(egui::TextEdit::singleline(&mut text).desired_width(f32::INFINITY))
                        .changed()
                    {
                        actions.push(DesignerAction::Edit(
                            id,
                            Edit::Property("Text", Value::String(text)),
                        ));
                    }
                    ui.end_row();
                }
            });
            if scene.editing
                && let Some(edit) = style_panel::show(ui, world, Some(id), scene.stylesheets)
            {
                actions.push(DesignerAction::Style(id, edit));
            }
        });
    }
}

/// A rough drawing of UI draw items with egui's painter, for when there's
/// no GPU renderer (drawing tests). `origin` is where pixel 0, 0 is.
fn sketch<'a>(
    painter: &egui::Painter,
    origin: egui::Pos2,
    pixels_per_point: f32,
    items: impl Iterator<Item = &'a UiDrawItem>,
) {
    // UI colours are sRGB, 0 to 1.
    let color = |c: [f32; 4]| {
        let [r, g, b, a] = c.map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8);
        egui::Color32::from_rgba_unmultiplied(r, g, b, a)
    };
    let rect = |position: [f32; 2], size: [f32; 2]| {
        egui::Rect::from_min_size(
            origin + egui::vec2(position[0], position[1]) / pixels_per_point,
            egui::vec2(size[0], size[1]) / pixels_per_point,
        )
    };
    for item in items {
        match item {
            UiDrawItem::Rectangle(r) => {
                painter.rect_filled(
                    rect(r.position, r.size),
                    r.border_radius / pixels_per_point,
                    color(r.color),
                );
            }
            UiDrawItem::Image(image) => {
                painter.rect_filled(rect(image.position, image.size), 0.0, color(image.tint));
            }
            UiDrawItem::Text(text) => {
                painter.text(
                    rect(text.position, text.size).left_top(),
                    egui::Align2::LEFT_TOP,
                    &text.text,
                    egui::FontId::proportional((text.font_size / pixels_per_point).max(1.0)),
                    color(text.color),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use axcenity_core::UDim2;

    use super::*;

    /// A ScreenGui designed at 400×300 with a Frame at 40, 30 holding a
    /// TextLabel, and a Part.
    fn menu() -> (World, InstanceId, InstanceId, InstanceId, InstanceId) {
        let mut world = World::new();
        let gui = world.create(ClassName::ScreenGui).unwrap();
        world.set_parent(gui, Some(world.workspace())).unwrap();
        world
            .set_property(gui, "DesignWidth", Value::Number(400.0))
            .unwrap();
        world
            .set_property(gui, "DesignHeight", Value::Number(300.0))
            .unwrap();
        let frame = world.create(ClassName::Frame).unwrap();
        world.set_parent(frame, Some(gui)).unwrap();
        world
            .set_property(
                frame,
                "Position",
                Value::UDim2(UDim2::new(0.0, 40.0, 0.0, 30.0)),
            )
            .unwrap();
        let label = world.create(ClassName::TextLabel).unwrap();
        world.set_parent(label, Some(frame)).unwrap();
        let part = world.create(ClassName::Part).unwrap();
        world.set_parent(part, Some(world.workspace())).unwrap();
        (world, gui, frame, label, part)
    }

    #[test]
    fn the_designer_follows_the_selection_into_its_screen_gui() {
        let (mut world, gui, _, label, part) = menu();
        let other = world.create(ClassName::ScreenGui).unwrap();
        world.set_parent(other, Some(world.workspace())).unwrap();
        let mut designer = UiDesigner::default();
        designer.follow(&world, None);
        assert_eq!(designer.gui(), Some(gui), "the first ScreenGui");
        designer.follow(&world, Some(other));
        assert_eq!(designer.gui(), Some(other));
        designer.follow(&world, Some(label));
        assert_eq!(designer.gui(), Some(gui), "the one around the label");
        designer.follow(&world, Some(part));
        assert_eq!(designer.gui(), Some(gui), "kept while a Part is selected");
        world.set_parent(gui, None).unwrap();
        designer.follow(&world, Some(part));
        assert_eq!(designer.gui(), Some(other), "a removed one isn't shown");
    }

    #[test]
    fn a_drop_goes_into_the_container_under_it_at_design_pixels() {
        let (world, gui, frame, label, _) = menu();
        assert_eq!(container_for(&world, gui, Some(label)), frame);
        assert_eq!(container_for(&world, gui, Some(frame)), frame);
        assert_eq!(container_for(&world, gui, None), gui);

        // At half size, 10 pixels in from the artboard's corner.
        let layout = axcenity_ui::layout_artboard(&world, gui, &[], 0.5, [10.0, 10.0]);
        let element = layout.element(frame).unwrap();
        assert_eq!([element.rect.x, element.rect.y], [30.0, 25.0]);
        // 20 pixels into the frame on screen is 40 design pixels.
        assert_eq!(
            drop_offset(&layout, frame, [50.0, 35.0], [10.0, 10.0], 0.5),
            [40.0, 20.0]
        );
        assert_eq!(
            drop_offset(&layout, gui, [50.0, 35.0], [10.0, 10.0], 0.5),
            [80.0, 50.0]
        );
    }

    #[test]
    fn the_hierarchy_lists_the_screen_gui_and_its_elements_by_depth() {
        let (world, gui, frame, label, _) = menu();
        assert_eq!(
            hierarchy(&world, gui),
            vec![(gui, 0), (frame, 1), (label, 2)]
        );
    }
}
