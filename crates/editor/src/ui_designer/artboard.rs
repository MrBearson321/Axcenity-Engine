//! The artboard: the ScreenGui on the previewed screen, its rulers and
//! guides, the selected element's handles and distances, palette drops,
//! and text typed over on a double-click.

use axcenity_core::{ClassName, InstanceId, UDim2, Value};
use axcenity_render::glam::Vec3;
use axcenity_render::{Camera, UiDrawItem, UiRectangle};
use axcenity_ui::{UiLayout, UiRect};
use eframe::egui;
use eframe::egui_wgpu::RenderState;

use super::anchors::Pin;
use super::rulers::{self, RULER};
use super::{
    DesignerAction, DesignerScene, FIT_MARGIN, Guide, MAX_ZOOM, MIN_ZOOM, PaletteItem, TextEditing,
    UiDesigner, container_for, drop_offset, scale_mode,
};
use crate::properties::Edit;
use crate::theme;
use crate::ui_handles::{self, Grip, Handles, UiDrag};

/// The artboard's colour behind the UI (sRGB): a mid grey, so dark and
/// light UIs both stand out. In Play the 3D world is behind it.
const BACKDROP: [f32; 4] = [0.30, 0.32, 0.36, 1.0];
/// How far the pointer moves, in points, before a press becomes a drag.
const DRAG_SLOP: f32 = 3.0;
/// How close the pointer must be to a guide to grab it, in points.
const GUIDE_GRAB: f32 = 4.0;
/// Guide lines, in a cyan that reads apart from the magenta snap lines.
pub(super) const GUIDE_COLOR: egui::Color32 = egui::Color32::from_rgb(64, 200, 230);

/// Where the screen is drawn, and how design pixels map onto it.
#[derive(Clone, Copy, Debug)]
pub(super) struct View {
    /// The area inside the rulers.
    pub canvas: egui::Rect,
    /// The previewed screen, in points.
    pub board: egui::Rect,
    /// Points per screen pixel.
    pub zoom: f32,
    /// Screen pixels per design pixel (Fit and Scale scale the design).
    pub unit: f32,
    /// Screen pixels from the screen's corner to design pixel (0, 0).
    pub unit_origin: egui::Vec2,
    /// The area the ScreenGui is laid out on, in design pixels.
    pub area: egui::Vec2,
}

impl View {
    /// Points per design pixel.
    pub fn per_unit(&self) -> f32 {
        self.zoom * self.unit
    }

    /// Where design pixel `design` is, in points.
    pub fn point(&self, design: egui::Vec2) -> egui::Pos2 {
        self.board.min + (self.unit_origin + design * self.unit) * self.zoom
    }

    /// The design pixel at `point`.
    pub fn design(&self, point: egui::Pos2) -> egui::Vec2 {
        ((point - self.board.min) / self.zoom - self.unit_origin) / self.unit.max(1e-6)
    }

    /// The ScreenGui's own area, in points.
    pub fn area_rect(&self) -> egui::Rect {
        egui::Rect::from_min_size(self.point(egui::Vec2::ZERO), self.area * self.per_unit())
    }

    /// Where `guide` is drawn: its x (vertical) or y, in points.
    pub fn guide_at(&self, guide: Guide) -> f32 {
        let design = if guide.vertical {
            egui::vec2(guide.at, 0.0)
        } else {
            egui::vec2(0.0, guide.at)
        };
        let point = self.point(design);
        if guide.vertical { point.x } else { point.y }
    }
}

/// True for elements whose `Text` can be typed over on the artboard.
fn has_text(class: ClassName) -> bool {
    matches!(
        class,
        ClassName::TextLabel | ClassName::TextButton | ClassName::TextBox
    )
}

impl UiDesigner {
    /// The artboard in `rect` (rulers included).
    pub(super) fn artboard(
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
        let screen = self.screen(world, gui);
        let canvas = egui::Rect::from_min_max(rect.min + egui::vec2(RULER, RULER), rect.max);
        let response = ui.interact(
            canvas,
            ui.id().with("ui designer artboard"),
            egui::Sense::click_and_drag(),
        );
        self.place_view(ui, canvas, &response, gui, screen);

        let (unit, unit_origin) = axcenity_ui::screen_mapping(world, gui, screen.0, screen.1);
        let design = axcenity_ui::design_size(world, gui);
        let area = if scale_mode(world, gui) == "Fit" {
            egui::vec2(design.0 as f32, design.1 as f32)
        } else {
            egui::vec2(screen.0 as f32, screen.1 as f32) / unit
        };
        let board = egui::Rect::from_min_size(
            canvas.min + self.pan,
            egui::vec2(screen.0 as f32, screen.1 as f32) * self.zoom,
        );
        let view = View {
            canvas,
            board,
            zoom: self.zoom,
            unit,
            unit_origin: egui::vec2(unit_origin[0], unit_origin[1]),
            area,
        };
        let visible = board.intersect(canvas);
        self.board = Some(board);
        self.canvas_rect = Some(canvas);
        let painter = ui.painter_at(rect);
        painter.rect_filled(canvas, 0.0, palette.inset);

        let selected_rect = if visible.is_positive() {
            self.draw_board(
                ui,
                &view,
                visible,
                scene,
                gui,
                &response,
                render_state,
                actions,
            )
        } else {
            self.drag = None;
            painter.text(
                canvas.center(),
                egui::Align2::CENTER_CENTER,
                "The artboard is out of view: Fit brings it back.",
                egui::FontId::proportional(theme::type_scale::SMALL),
                palette.text_weak,
            );
            None
        };

        // Rulers, then guides being dragged out of them (or back in).
        let (top, left) = rulers::draw(ui, rect, &view, selected_rect, self.guides(gui));
        if scene.editing {
            if top.drag_started() {
                self.guide_drag = Some((None, false));
            } else if left.drag_started() {
                self.guide_drag = Some((None, true));
            }
        }
        self.drag_guide(ui, &view, gui);
    }

    /// Fits the screen into the canvas when it's new or asked for, then
    /// pans and zooms.
    fn place_view(
        &mut self,
        ui: &egui::Ui,
        canvas: egui::Rect,
        response: &egui::Response,
        gui: InstanceId,
        screen: (u32, u32),
    ) {
        let resized = (canvas.size() - self.fitted_room).length() > 0.5;
        if self.fit_next || self.fitted != Some((gui, screen)) || (resized && !self.moved) {
            self.fit_next = false;
            self.moved = false;
            self.fitted = Some((gui, screen));
            self.fitted_room = canvas.size();
            let room =
                (canvas.size() - egui::Vec2::splat(FIT_MARGIN * 2.0)).max(egui::vec2(1.0, 1.0));
            self.zoom = (room.x / screen.0 as f32)
                .min(room.y / screen.1 as f32)
                .clamp(MIN_ZOOM, MAX_ZOOM);
            let size = egui::vec2(screen.0 as f32, screen.1 as f32) * self.zoom;
            self.pan = (canvas.size() - size) * 0.5;
        }
        // Right- or middle-drag and the wheel pan; Ctrl + wheel zooms.
        let mut pan = egui::Vec2::ZERO;
        let mut zoom_at = self.zoom_by.take().map(|by| (by, canvas.center()));
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
            let from_corner = at - canvas.min - self.pan;
            self.pan = at - canvas.min - from_corner * (self.zoom / old);
            self.moved = true;
        }
    }

    /// The screen and everything on it: the UI, guides, handles, distances
    /// and anchors, and what's done there. Returns the selected element's
    /// rectangle, in points, for the rulers.
    #[allow(clippy::too_many_arguments)] // The artboard's pieces, worked out once.
    fn draw_board(
        &mut self,
        ui: &mut egui::Ui,
        view: &View,
        visible: egui::Rect,
        scene: &DesignerScene,
        gui: InstanceId,
        response: &egui::Response,
        render_state: Option<&RenderState>,
        actions: &mut Vec<DesignerAction>,
    ) -> Option<egui::Rect> {
        let world = scene.world;
        let palette = theme::palette(ui.ctx());
        let ppp = ui.ctx().pixels_per_point();
        let screen = self.screen(world, gui);
        // Laid out in the pixels of a texture the size of `visible`.
        let scale = self.zoom * ppp;
        let origin = (view.board.min - visible.min) * ppp;
        let layout = axcenity_ui::layout_artboard(
            world,
            gui,
            scene.stylesheets,
            screen,
            scale,
            [origin.x, origin.y],
        );
        let screen_rect = UiRect {
            x: origin.x,
            y: origin.y,
            width: screen.0 as f32 * scale,
            height: screen.1 as f32 * scale,
        };
        let pixel_of = |pointer: egui::Pos2| {
            let at = (pointer - visible.min) * ppp;
            [at.x, at.y]
        };
        let points_of = |rect: UiRect| {
            egui::Rect::from_min_size(
                visible.min + egui::vec2(rect.x, rect.y) / ppp,
                egui::vec2(rect.width, rect.height) / ppp,
            )
        };
        // Ruler guides, as lines across the screen in layout pixels.
        let guide_lines: Vec<UiRect> = self
            .guides(gui)
            .iter()
            .map(|&guide| {
                let at = view.guide_at(guide);
                if guide.vertical {
                    UiRect {
                        x: (at - visible.min.x) * ppp,
                        y: screen_rect.y,
                        width: 0.0,
                        height: screen_rect.height,
                    }
                } else {
                    UiRect {
                        x: screen_rect.x,
                        y: (at - visible.min.y) * ppp,
                        width: screen_rect.width,
                        height: 0.0,
                    }
                }
            })
            .collect();

        let handles = scene
            .selected
            .filter(|_| scene.editing && self.text_editing.is_none())
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
        let guide_under = over.and_then(|pointer| {
            self.guides(gui).iter().position(|&guide| {
                let at = view.guide_at(guide);
                let along = if guide.vertical { pointer.x } else { pointer.y };
                (along - at).abs() <= GUIDE_GRAB
            })
        });

        // A press on the selection's handles drags them, and one on a guide
        // moves it. A press anywhere else selects what's there, and can
        // drag it straight away.
        if pressed
            && self.drag.is_none()
            && self.guide_drag.is_none()
            && scene.editing
            && let Some(pointer) = over
        {
            self.drag_held = Some(pointer);
            if let (Some(grip), Some(handles)) = (grip, &handles) {
                self.drag = UiDrag::start(handles, grip, pointer, world);
            } else if let Some(index) = guide_under {
                let vertical = self.guides(gui)[index].vertical;
                self.guide_drag = Some((Some(index), vertical));
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
                self.snaps.clear();
            } else if let Some(pointer) = pointer
                && self
                    .drag_held
                    .is_none_or(|from| from.distance(pointer) >= DRAG_SLOP)
            {
                self.drag_held = None;
                let edit = drag.update_with_guides(
                    pointer,
                    ppp,
                    world,
                    &layout.elements,
                    screen_rect,
                    &guide_lines,
                );
                self.snaps = edit.guides.clone();
                // An unmoved click selects without a step.
                if !edit.values.is_empty() || !edit.css.is_empty() {
                    actions.push(DesignerAction::Drag(edit));
                }
            }
        }
        if scene.editing
            && response.double_clicked()
            && let Some(pointer) = over
        {
            let [x, y] = pixel_of(pointer);
            if let Some(id) = layout.hit(x, y) {
                self.start_typing(scene, id);
            }
        }
        if scene.editing
            && let Some(item) = response.dnd_release_payload::<PaletteItem>()
            && let Some(pointer) = over
        {
            let pixel = pixel_of(pointer);
            let parent = container_for(world, gui, layout.hit(pixel[0], pixel[1]));
            let corner = origin + view.unit_origin * scale;
            let at = drop_offset(
                &layout,
                parent,
                pixel,
                [corner.x, corner.y],
                view.unit * scale,
            );
            actions.push(DesignerAction::Insert {
                what: item.0,
                parent,
                at,
            });
        }

        self.paint_ui(ui, visible, screen_rect, &layout, scene, render_state);
        let painter = ui.painter_at(view.canvas);
        painter.rect_stroke(
            view.board,
            0.0,
            egui::Stroke::new(1.0_f32, palette.border_strong),
            egui::StrokeKind::Outside,
        );
        let painter = ui.painter_at(visible);
        for &guide in self.guides(gui) {
            paint_guide(&painter, view, guide);
        }

        // The selection: its handles, its distances in design pixels (as
        // the viewport shows them), and where it's anchored.
        let selected = scene
            .selected
            .and_then(|id| Some((id, *layout.element(id)?)));
        let selected_rect = selected.map(|(_, element)| points_of(element.rect));
        if let Some((id, element)) = selected {
            let parent = world
                .get(id)
                .and_then(|instance| instance.parent())
                .and_then(|parent| layout.element(parent))
                .map(|parent| points_of(parent.rect));
            if let Some(handles) = &handles {
                let highlighted = self.drag.as_ref().map(|drag| drag.grip).or(grip);
                handles.paint(&painter, palette.accent, highlighted);
                // Its size and distances, in design pixels, while it's being
                // dragged or pointed at: on a small artboard they'd cover it.
                let pointed_at =
                    over.is_some_and(|pointer| handles.rect.expand(8.0).contains(pointer));
                if self.drag.is_some() || pointed_at {
                    handles.paint_measures(&painter, view.board, parent, 1.0 / view.per_unit());
                }
            }
            if !element.in_flow {
                paint_anchor(
                    &painter,
                    world,
                    id,
                    parent.unwrap_or_else(|| view.area_rect()),
                    palette.accent,
                );
            }
        }
        if self.drag.is_some() {
            ui_handles::paint_guides(&self.snaps, &painter, visible.min, ppp);
        }
        match (&self.drag, &handles) {
            (Some(drag), Some(handles)) => ui.ctx().set_cursor_icon(handles.cursor(drag.grip)),
            (None, Some(handles)) if grip.is_some_and(|g| g != Grip::BODY) => {
                ui.ctx()
                    .set_cursor_icon(handles.cursor(grip.expect("checked")));
            }
            (None, _) if guide_under.is_some() && grip.is_none() => {
                let vertical = self.guides(gui)[guide_under.expect("checked")].vertical;
                ui.ctx().set_cursor_icon(if vertical {
                    egui::CursorIcon::ResizeHorizontal
                } else {
                    egui::CursorIcon::ResizeVertical
                });
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
        self.type_over(ui, scene, &layout, visible, view, actions);
        selected_rect
    }

    /// The backdrop and the UI, by the game's renderer when there is one.
    fn paint_ui(
        &mut self,
        ui: &egui::Ui,
        visible: egui::Rect,
        screen: UiRect,
        layout: &UiLayout,
        scene: &DesignerScene,
        render_state: Option<&RenderState>,
    ) {
        let backdrop = UiDrawItem::Rectangle(UiRectangle {
            position: [screen.x, screen.y],
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
                &ui.painter_at(visible),
                visible.min,
                ui.ctx().pixels_per_point(),
                std::iter::once(&backdrop).chain(&layout.draw_items),
            ),
        }
    }

    /// Starts typing over `id`'s text, if it has any.
    pub(super) fn start_typing(&mut self, scene: &DesignerScene, id: InstanceId) {
        let Some(instance) = scene.world.get(id) else {
            return;
        };
        if !has_text(instance.class()) {
            return;
        }
        let text = instance
            .property("Text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        self.drag = None;
        self.text_editing = Some(TextEditing {
            id,
            original: text.clone(),
            text,
            focused: false,
        });
    }

    /// The text being typed over, in a box over its element. Each change is
    /// an edit of its `Text`, so the whole typing is one undo step. Enter
    /// or clicking away finishes, and Escape puts the text back.
    fn type_over(
        &mut self,
        ui: &mut egui::Ui,
        scene: &DesignerScene,
        layout: &UiLayout,
        visible: egui::Rect,
        view: &View,
        actions: &mut Vec<DesignerAction>,
    ) {
        let Some(editing) = &mut self.text_editing else {
            return;
        };
        let ppp = ui.ctx().pixels_per_point();
        let (Some(element), Some(instance), true) = (
            layout.element(editing.id),
            scene.world.get(editing.id),
            scene.editing,
        ) else {
            self.text_editing = None;
            return;
        };
        let rect = egui::Rect::from_min_size(
            visible.min + egui::vec2(element.rect.x, element.rect.y) / ppp,
            egui::vec2(element.rect.width, element.rect.height) / ppp,
        )
        .expand(2.0);
        let size = instance
            .property("TextSize")
            .and_then(Value::as_number)
            .unwrap_or(16.0) as f32;
        let font = egui::FontId::proportional((size * view.per_unit()).clamp(8.0, 72.0));
        let multi_line = instance.property("MultiLine").and_then(Value::as_bool) == Some(true);
        let edit = if multi_line {
            egui::TextEdit::multiline(&mut editing.text)
        } else {
            egui::TextEdit::singleline(&mut editing.text)
        };
        let id = ui.id().with(("ui designer typing", editing.id));
        let response = ui.put(rect, edit.id(id).font(font).desired_width(rect.width()));
        if !editing.focused {
            response.request_focus();
            editing.focused = true;
        }
        if response.changed() {
            actions.push(DesignerAction::Edit(
                editing.id,
                Edit::Property("Text", Value::String(editing.text.clone())),
            ));
        }
        let escaped = ui.input(|input| input.key_pressed(egui::Key::Escape));
        if escaped {
            actions.push(DesignerAction::Edit(
                editing.id,
                Edit::Property("Text", Value::String(editing.original.clone())),
            ));
            self.text_editing = None;
        } else if response.lost_focus() {
            self.text_editing = None;
        }
    }

    /// A guide being dragged: it follows the pointer in whole design
    /// pixels. Let go over a ruler (or outside the canvas), it goes away.
    fn drag_guide(&mut self, ui: &egui::Ui, view: &View, gui: InstanceId) {
        let Some((index, vertical)) = self.guide_drag else {
            return;
        };
        let (down, pointer) =
            ui.input(|input| (input.pointer.primary_down(), input.pointer.latest_pos()));
        let Some(pointer) = pointer else {
            return;
        };
        let design = view.design(pointer);
        let at = if vertical { design.x } else { design.y }.round();
        let inside = view.canvas.contains(pointer);
        let guide = Guide { vertical, at };
        if down {
            match index {
                Some(index) => {
                    if let Some(existing) = self.guides.get_mut(&gui).and_then(|g| g.get_mut(index))
                    {
                        existing.at = at;
                    }
                }
                None if inside => paint_guide(&ui.painter_at(view.canvas), view, guide),
                None => {}
            }
            ui.ctx().set_cursor_icon(if vertical {
                egui::CursorIcon::ResizeHorizontal
            } else {
                egui::CursorIcon::ResizeVertical
            });
            return;
        }
        self.guide_drag = None;
        match (index, inside) {
            (None, true) => self.add_guide(gui, guide),
            (Some(index), false) => {
                if let Some(guides) = self.guides.get_mut(&gui)
                    && index < guides.len()
                {
                    guides.remove(index);
                }
            }
            _ => {}
        }
    }
}

/// A guide line across the canvas.
fn paint_guide(painter: &egui::Painter, view: &View, guide: Guide) {
    let at = view.guide_at(guide);
    let canvas = view.canvas;
    let (a, b) = if guide.vertical {
        (
            egui::pos2(at, canvas.top()),
            egui::pos2(at, canvas.bottom()),
        )
    } else {
        (
            egui::pos2(canvas.left(), at),
            egui::pos2(canvas.right(), at),
        )
    };
    painter.line_segment([a, b], egui::Stroke::new(1.0_f32, GUIDE_COLOR));
}

/// Where `id` is anchored in `parent` (in points): a small diamond at each
/// point it's pinned to.
fn paint_anchor(
    painter: &egui::Painter,
    world: &axcenity_core::World,
    id: InstanceId,
    parent: egui::Rect,
    color: egui::Color32,
) {
    let Some(instance) = world.get(id) else {
        return;
    };
    let udim2 = |name| instance.property(name).and_then(Value::as_udim2);
    let (Some(position), Some(size)) = (udim2("Position"), udim2("Size")) else {
        return;
    };
    let UDim2 { x, y } = position;
    let points = |pin: Option<Pin>, start: f32, end: f32| -> Vec<f32> {
        match pin {
            Some(Pin::Start) => vec![start],
            Some(Pin::Center) => vec![(start + end) * 0.5],
            Some(Pin::End) => vec![end],
            Some(Pin::Stretch) => vec![start, end],
            None => Vec::new(),
        }
    };
    let xs = points(Pin::of(x, size.x), parent.left(), parent.right());
    let ys = points(Pin::of(y, size.y), parent.top(), parent.bottom());
    for &px in &xs {
        for &py in &ys {
            let at = egui::pos2(px, py);
            let r = 4.0;
            let diamond = vec![
                at + egui::vec2(0.0, -r),
                at + egui::vec2(r, 0.0),
                at + egui::vec2(0.0, r),
                at + egui::vec2(-r, 0.0),
            ];
            painter.add(egui::Shape::convex_polygon(
                diamond,
                color,
                egui::Stroke::new(1.0_f32, egui::Color32::from_black_alpha(160)),
            ));
        }
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
