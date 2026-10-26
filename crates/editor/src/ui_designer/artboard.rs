//! The artboard: the ScreenGui on the previewed screen, its rulers and
//! guides, the selected element's handles and distances, palette drops,
//! and text typed over on a double-click.

use axcenity_core::{ClassName, InstanceId, Value, World};
use axcenity_render::glam::Vec3;
use axcenity_render::{Camera, UiDrawItem, UiRectangle};
use axcenity_ui::{UiLayout, UiRect};
use eframe::egui;
use eframe::egui_wgpu::RenderState;

use super::anchor_handles::{AnchorDrag, AnchorHandles};
use super::rich_text::{self, Format};
use super::rotate::{self, RotateDrag};
use super::rulers::{self, RULER};
use super::{
    DesignerAction, DesignerScene, FIT_MARGIN, Guide, MAX_ZOOM, MIN_ZOOM, PaletteItem, TextEditing,
    Toggle, UiDesigner, container_for, drop_offset, panels, reset_anchor_action, safe_insets,
};
use crate::properties::Edit;
use crate::theme::{self, icons};
use crate::ui_handles::{self, Grip, Handles, UiDrag};
use crate::widgets;

/// The artboard's colour behind the UI (sRGB): a mid grey, so dark and
/// light UIs both stand out. In Play the 3D world is behind it.
const BACKDROP: [f32; 4] = [0.30, 0.32, 0.36, 1.0];
/// How far the pointer moves, in points, before a press becomes a drag.
const DRAG_SLOP: f32 = 3.0;
/// How close the pointer must be to a guide to grab it, in points.
const GUIDE_GRAB: f32 = 4.0;
/// The bar of formats over rich text being typed, in points.
const FORMAT_BAR_WIDTH: f32 = 300.0;
const FORMAT_BAR_HEIGHT: f32 = 28.0;
/// The states editor's eye, over the artboard's top left corner, in points.
const EYE_SIZE: egui::Vec2 = egui::vec2(160.0, 24.0);
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
        self.place_view(ui, canvas, &response, (world, gui), screen);

        // A phone's notch and home bar are left out with `DeviceSafeInsets`.
        let (unit, unit_origin, area) =
            axcenity_ui::safe_screen_mapping(world, gui, screen.0, screen.1, safe_insets(screen));
        let area = egui::vec2(area[0], area[1]);
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
            self.anchor_drag = None;
            self.rotate_drag = None;
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
        (world, gui): (&World, InstanceId),
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
            // Over a ScrollingFrame, the wheel scrolls it instead.
            let scrolled = zoom == 1.0
                && scroll != egui::Vec2::ZERO
                && pointer.is_some_and(|pointer| self.wheel_frame(ui, world, pointer, scroll));
            if !scrolled {
                pan += scroll;
            }
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

    /// Scrolls the ScrollingFrame under `pointer` by the wheel's `scroll`
    /// (points), on the artboard only. Returns whether one was there.
    fn wheel_frame(
        &mut self,
        ui: &egui::Ui,
        world: &World,
        pointer: egui::Pos2,
        scroll: egui::Vec2,
    ) -> bool {
        let Some(visible) = self.board_visible else {
            return false;
        };
        let ppp = ui.ctx().pixels_per_point();
        let at = (pointer - visible.min) * ppp;
        let delta = -scroll * ppp;
        let wheel = self.scroller.wheel(world, [at.x, at.y], [delta.x, delta.y]);
        if let Some((id, to)) = wheel.moved {
            self.scrolled.insert(id, [to.x, to.y]);
        }
        wheel.taken
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
        let shown = self.shown_state(scene);
        let layout = axcenity_ui::layout_artboard_view(
            world,
            gui,
            scene.stylesheets,
            axcenity_ui::ArtboardView {
                screen,
                scale,
                origin: [origin.x, origin.y],
                shown,
                scrolled: Some(&self.scrolled),
                safe_insets: safe_insets(screen),
            },
        );
        self.scroller.remember(&layout);
        self.board_visible = Some(visible);
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
        // The box being typed in takes its own clicks: they place the
        // caret, not select or drag what's under it.
        let typing = self.typing_rect(&layout, visible, ppp);
        let bar = self.format_bar_rect(world, typing, visible);
        // The states editor's eye, while a state other than Normal is picked.
        let eye = scene
            .selected
            .filter(|&id| self.state_for(scene, id) != axcenity_ui::UiState::Normal)
            .map(|_| egui::Rect::from_min_size(visible.min + egui::vec2(8.0, 8.0), EYE_SIZE));
        let over = pointer.filter(|&pointer| {
            response.contains_pointer()
                && visible.contains(pointer)
                && ![typing, bar, eye]
                    .iter()
                    .flatten()
                    .any(|rect| rect.contains(pointer))
        });
        // The selected element's anchor diamonds (a flex child has none).
        // They're over its handles, so they're grabbed first.
        let anchor_handles = scene
            .selected
            .filter(|&id| layout.element(id).is_some_and(|element| !element.in_flow))
            .and_then(|id| {
                let parent = world
                    .get(id)?
                    .parent()
                    .and_then(|parent| layout.element(parent))
                    .map_or_else(|| view.area_rect(), |parent| points_of(parent.rect));
                AnchorHandles::new(world, id, parent)
            });
        let anchor_grab = over
            .filter(|_| scene.editing && self.text_editing.is_none())
            .and_then(|pointer| anchor_handles.as_ref()?.grab_at(pointer));
        let grip = match (over, &handles) {
            (Some(pointer), Some(handles)) if anchor_grab.is_none() => handles.grip_at(pointer),
            _ => None,
        };
        // Just outside a corner, a knob turns it.
        let rotate_grab = match (over, &handles) {
            (Some(pointer), Some(handles)) if anchor_grab.is_none() && grip.is_none() => {
                handles.rotate_at(pointer)
            }
            _ => false,
        };
        let guide_under = over.and_then(|pointer| {
            self.guides(gui).iter().position(|&guide| {
                let at = view.guide_at(guide);
                let along = if guide.vertical { pointer.x } else { pointer.y };
                (along - at).abs() <= GUIDE_GRAB
            })
        });

        // A ScrollingFrame's bar being dragged scrolls it on the artboard.
        if self.scroller.scrolling() {
            match pointer.filter(|_| down) {
                Some(pointer) => {
                    let moved = self.scroller.drag(pixel_of(pointer)).moved;
                    if let Some((id, to)) = moved {
                        self.scrolled.insert(id, [to.x, to.y]);
                    }
                }
                None => self.scroller.release(),
            }
        }
        // A press on a scroll bar drags it, one on the selection's handles
        // drags them, and one on a guide moves it. A press anywhere else
        // selects what's there, and can drag it straight away.
        if pressed
            && self.drag.is_none()
            && self.anchor_drag.is_none()
            && self.rotate_drag.is_none()
            && self.guide_drag.is_none()
            && !self.scroller.scrolling()
            && scene.editing
            && let Some(pointer) = over
        {
            self.drag_held = Some(pointer);
            let bar = self.scroller.press(world, pixel_of(pointer));
            if !bar.taken {
                // The artboard's content drags move elements, not scroll.
                self.scroller.release();
            }
            if let Some((id, to)) = bar.moved {
                self.scrolled.insert(id, [to.x, to.y]);
            }
            if bar.taken {
                // The bar has it.
            } else if let (Some(grab), Some(anchor)) = (anchor_grab, &anchor_handles) {
                if let Some((placement, _)) =
                    panels::placement(world, scene.stylesheets, gui, anchor.id)
                {
                    self.anchor_drag = Some(AnchorDrag::start(anchor, grab, pointer, placement));
                }
            } else if let (Some(grip), Some(handles)) = (grip, &handles) {
                self.drag = UiDrag::start(handles, grip, pointer, world);
            } else if let (true, Some(handles)) = (rotate_grab, &handles) {
                self.rotate_drag = RotateDrag::start(handles, pointer, world);
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
        // The anchor follows the pointer, and the element stays put. Alt
        // turns snapping off.
        if let Some(drag) = &mut self.anchor_drag {
            if !down || !scene.editing {
                self.anchor_drag = None;
            } else if let (Some(pointer), Some(anchor)) = (pointer, &anchor_handles)
                && self
                    .drag_held
                    .is_none_or(|from| from.distance(pointer) >= DRAG_SLOP)
            {
                self.drag_held = None;
                let snap = !ui.input(|input| input.modifiers.alt);
                let (position, size) = drag.update(pointer, anchor.parent, snap);
                actions.push(DesignerAction::MoveAnchor {
                    id: drag.id,
                    position,
                    size,
                });
            }
        }
        // The rotate knob turns the selection around its middle. Shift
        // snaps it to 15° steps.
        let mut turning = None;
        if let Some(drag) = &mut self.rotate_drag {
            if !down || !scene.editing {
                self.rotate_drag = None;
            } else if let Some(pointer) = pointer
                && self
                    .drag_held
                    .is_none_or(|from| from.distance(pointer) >= DRAG_SLOP)
            {
                self.drag_held = None;
                let snap = ui.input(|input| input.modifiers.shift);
                let rotation = drag.update(pointer, snap);
                turning = Some((pointer, rotation));
                let now = world
                    .get(drag.id)
                    .and_then(|instance| instance.property("Rotation"))
                    .and_then(Value::as_number);
                if now != Some(rotation) {
                    actions.push(DesignerAction::Edit(
                        drag.id,
                        Edit::Property("Rotation", Value::Number(rotation)),
                    ));
                }
            }
        }
        // Right-click on a diamond: Reset anchor.
        if let Some(anchor) = &anchor_handles
            && ((anchor_grab.is_some() && self.anchor_drag.is_none()) || self.anchor_menu)
        {
            let menu = response.context_menu(|ui| {
                if ui
                    .button("Reset anchor")
                    .on_hover_text("Anchor it to the top left, not stretched, without moving it")
                    .clicked()
                {
                    actions.extend(reset_anchor_action(
                        world,
                        scene.stylesheets,
                        gui,
                        anchor.id,
                    ));
                    ui.close();
                }
            });
            self.anchor_menu = menu.is_some();
        } else {
            self.anchor_menu = false;
            // Right-click on an element: its layer order.
            if scene.editing
                && response.secondary_clicked()
                && let Some(pointer) = over
            {
                let [x, y] = pixel_of(pointer);
                self.order_target = layout.hit(x, y);
                if let Some(id) = self.order_target {
                    actions.push(DesignerAction::Select(Some(id)));
                }
            }
            if let Some(id) = self.order_target.filter(|_| scene.editing) {
                let menu = response.context_menu(|ui| {
                    super::order::menu_items(ui, world, scene.stylesheets, id, actions);
                });
                if menu.is_none() && !response.secondary_clicked() {
                    self.order_target = None;
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
        paint_unsafe_edges(&ui.painter_at(visible), view.board, screen, self.zoom);
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
        if let Some((id, _)) = selected {
            let parent = world
                .get(id)
                .and_then(|instance| instance.parent())
                .and_then(|parent| layout.element(parent))
                .map(|parent| points_of(parent.rect));
            if let Some(handles) = &handles {
                let highlighted = self.drag.as_ref().map(|drag| drag.grip).or(grip);
                handles.paint(&painter, palette.accent, highlighted);
                handles.paint_rotate_knobs(
                    &painter,
                    palette.accent,
                    rotate_grab || self.rotate_drag.is_some(),
                );
                // Its size and distances, in design pixels, while it's being
                // dragged or pointed at: on a small artboard they'd cover it.
                let pointed_at =
                    over.is_some_and(|pointer| handles.rect.expand(8.0).contains(pointer));
                if self.drag.is_some() || pointed_at {
                    handles.paint_measures(&painter, view.board, parent, 1.0 / view.per_unit());
                }
            }
        }
        if let Some(anchor) = &anchor_handles {
            let lit = self
                .anchor_drag
                .as_ref()
                .map(|drag| drag.grab)
                .or(anchor_grab);
            anchor.paint(&painter, palette.accent, lit);
        }
        if self.drag.is_some() {
            ui_handles::paint_guides(&self.snaps, &painter, visible.min, ppp);
        }
        if let (Some(drag), Some(anchor)) = (&self.anchor_drag, &anchor_handles) {
            // Its snap lines are in points.
            ui_handles::paint_guides(&drag.snaps, &painter, egui::Pos2::ZERO, 1.0);
            drag.paint_label(ui, &painter, anchor.parent);
        }
        if let Some((pointer, rotation)) = turning {
            let text = rotate::label(rotation);
            let at = pointer + egui::vec2(14.0, 14.0);
            let galley =
                painter.layout_no_wrap(text, egui::FontId::monospace(11.0), egui::Color32::WHITE);
            let back = egui::Rect::from_min_size(at, galley.size()).expand(3.0);
            painter.rect_filled(back, 3.0, egui::Color32::from_black_alpha(180));
            painter.galley(at, galley, egui::Color32::WHITE);
        }
        if self.anchor_drag.is_some() || self.rotate_drag.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        } else if anchor_grab.is_some() || (rotate_grab && self.drag.is_none()) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
        match (&self.drag, &handles) {
            (Some(drag), Some(handles)) => ui.ctx().set_cursor_icon(handles.cursor(drag.grip)),
            (None, Some(handles)) if grip.is_some_and(|g| g != Grip::BODY) => {
                ui.ctx()
                    .set_cursor_icon(handles.cursor(grip.expect("checked")));
            }
            (None, _) if guide_under.is_some() && grip.is_none() && anchor_grab.is_none() => {
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
        self.type_over(ui, scene, &layout, visible, actions);
        if let Some(rect) = eye {
            let state = self.state.name();
            let (icon, text, tip) = if self.show_state {
                (
                    icons::EYE,
                    format!("Showing {state}"),
                    "The selection is shown in this state. Click to show it as it is.",
                )
            } else {
                (
                    icons::EYE_SLASH,
                    format!("Show {state}"),
                    "Show the selection in this state",
                )
            };
            let button = egui::Button::new(format!("{icon} {text}")).selected(self.show_state);
            // A child, so the row the artboard is in doesn't move on past it.
            let mut over_board = ui.new_child(egui::UiBuilder::new().max_rect(rect));
            if over_board
                .add_sized(rect.size(), button)
                .on_hover_text(tip)
                .clicked()
            {
                self.show_state = !self.show_state;
            }
        }
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
            ..Default::default()
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
            selection: None,
        });
    }

    /// Where the box for the text being typed over goes, in points.
    fn typing_rect(&self, layout: &UiLayout, visible: egui::Rect, ppp: f32) -> Option<egui::Rect> {
        let element = layout.element(self.text_editing.as_ref()?.id)?;
        let rect = egui::Rect::from_min_size(
            visible.min + egui::vec2(element.rect.x, element.rect.y) / ppp,
            egui::vec2(element.rect.width, element.rect.height) / ppp,
        );
        Some(rect.expand(2.0))
    }

    /// Where the bar of formats goes while rich text is typed over: just
    /// above the box, or under it when there's no room above.
    fn format_bar_rect(
        &self,
        world: &World,
        typing: Option<egui::Rect>,
        visible: egui::Rect,
    ) -> Option<egui::Rect> {
        let editing = self.text_editing.as_ref()?;
        let rich = world
            .get(editing.id)?
            .property("RichText")
            .and_then(Value::as_bool);
        let typing = typing.filter(|_| rich == Some(true))?;
        let size = egui::vec2(FORMAT_BAR_WIDTH, FORMAT_BAR_HEIGHT);
        let above = typing.left_top() - egui::vec2(0.0, size.y + 4.0);
        let at = if above.y >= visible.top() {
            above
        } else {
            typing.left_bottom() + egui::vec2(0.0, 4.0)
        };
        Some(egui::Rect::from_min_size(at, size))
    }

    /// The text being typed over, in a box over its element. Each change is
    /// an edit of its `Text`, so the whole typing is one undo step. Enter
    /// or clicking away finishes, and Escape puts the text back.
    ///
    /// With `RichText` on, the box shows the tags' styles, and Ctrl+B,
    /// Ctrl+I, Ctrl+U or the bar over the box wraps the selection in tags.
    /// With nothing selected (or `RichText` off), Ctrl+B, Ctrl+I and Ctrl+U
    /// change the whole element's style, as they do outside the box.
    fn type_over(
        &mut self,
        ui: &mut egui::Ui,
        scene: &DesignerScene,
        layout: &UiLayout,
        visible: egui::Rect,
        actions: &mut Vec<DesignerAction>,
    ) {
        let ppp = ui.ctx().pixels_per_point();
        let rect = self.typing_rect(layout, visible, ppp);
        let bar = self.format_bar_rect(scene.world, rect, visible);
        let Some(editing) = &mut self.text_editing else {
            return;
        };
        let (Some(rect), Some(element), Some(instance), true) = (
            rect,
            layout.element(editing.id),
            scene.world.get(editing.id),
            scene.editing,
        ) else {
            self.text_editing = None;
            return;
        };
        // The size the artboard draws it at, so typing doesn't shrink it.
        let size = (element.font_size * element.pixels_per_unit / ppp).clamp(6.0, 256.0);
        let font = egui::FontId::proportional(size);
        let multi_line = instance.property("MultiLine").and_then(Value::as_bool) == Some(true);
        let rich = bar.is_some();
        let id = ui.id().with(("ui designer typing", editing.id));

        // Taken before the box sees them: it has its own Ctrl+U.
        let mut format = None;
        for (key, toggle) in rich_text::SHORTCUTS {
            let shortcut = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, key);
            if ui.input_mut(|input| input.consume_shortcut(&shortcut)) {
                format = Some(Format::Toggle(toggle));
            }
        }
        let pressed_on_bar = bar.is_some_and(|bar| {
            ui.input(|input| {
                input.pointer.any_pressed()
                    && input
                        .pointer
                        .interact_pos()
                        .is_some_and(|at| bar.contains(at))
            })
        });

        let base_size = element.font_size;
        let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, wrap_width: f32| {
            // A one-line box scrolls sideways instead of wrapping.
            let wrap_width = if multi_line {
                wrap_width
            } else {
                f32::INFINITY
            };
            let job =
                rich_text::layout_job(text.as_str(), size, base_size, ui.visuals(), wrap_width);
            ui.fonts(|fonts| fonts.layout_job(job))
        };
        let mut edit = if multi_line {
            egui::TextEdit::multiline(&mut editing.text)
        } else {
            egui::TextEdit::singleline(&mut editing.text)
        };
        if rich {
            edit = edit.layouter(&mut layouter);
        }
        // A child Ui, so the box doesn't move the cursor of the row the
        // artboard sits in: `ui.put` would, and the inspector after it
        // would be laid out over the artboard.
        let mut over = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(
            egui::Layout::centered_and_justified(egui::Direction::TopDown),
        ));
        let output = edit
            .id(id)
            .font(font)
            .desired_width(rect.width())
            .show(&mut over);
        let response = output.response;
        if !editing.focused {
            response.request_focus();
            editing.focused = true;
        }
        if let Some(range) = output.cursor_range {
            let [start, end] = range.sorted_cursors();
            editing.selection = Some(start.index..end.index);
        }
        if let Some(bar) = bar
            && let Some(picked) = format_bar(ui, bar)
        {
            format = Some(picked);
        }

        let mut refocus = pressed_on_bar;
        if let Some(format) = format {
            refocus = true;
            let wrapped = editing
                .selection
                .clone()
                .filter(|selection| rich && !selection.is_empty())
                .and_then(|selection| rich_text::apply(&editing.text, selection, format));
            match (wrapped, format) {
                (Some((text, selected)), _) => {
                    editing.text = text;
                    actions.push(DesignerAction::Edit(
                        editing.id,
                        Edit::Property("Text", Value::String(editing.text.clone())),
                    ));
                    let mut state = output.state;
                    state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::two(
                            egui::text::CCursor::new(selected.start),
                            egui::text::CCursor::new(selected.end),
                        )));
                    state.store(ui.ctx(), id);
                    editing.selection = Some(selected);
                }
                (None, Format::Toggle(toggle)) => {
                    if let Some(style) =
                        axcenity_ui::text_style(scene.world, editing.id, scene.stylesheets)
                    {
                        actions.push(toggle.action(editing.id, &style));
                    }
                }
                (None, Format::Color(_)) => {}
            }
        } else if response.changed() {
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
        } else if refocus {
            // The bar was used: typing carries on.
            response.request_focus();
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

/// The bar over rich text being typed: B, I, U and S, then colours. Returns
/// the one clicked.
fn format_bar(ui: &mut egui::Ui, rect: egui::Rect) -> Option<Format> {
    let mut picked = None;
    let mut bar = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    egui::Frame::popup(bar.style())
        .inner_margin(egui::Margin::symmetric(4, 2))
        .show(&mut bar, |ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            for (toggle, icon, tip) in [
                (Toggle::Bold, icons::TEXT_B, "Bold: <b> (Ctrl+B)"),
                (Toggle::Italic, icons::TEXT_ITALIC, "Italic: <i> (Ctrl+I)"),
                (
                    Toggle::Underline,
                    icons::TEXT_UNDERLINE,
                    "Underline: <u> (Ctrl+U)",
                ),
                (
                    Toggle::Strikethrough,
                    icons::TEXT_STRIKETHROUGH,
                    "Strikethrough: <s>",
                ),
            ] {
                if widgets::icon_button(ui, icon, tip, false).clicked() {
                    picked = Some(Format::Toggle(toggle));
                }
            }
            ui.separator();
            for color @ [r, g, b] in rich_text::SWATCHES {
                let swatch = egui::Button::new("")
                    .fill(egui::Color32::from_rgb(r, g, b))
                    .min_size(egui::vec2(16.0, 16.0));
                let hint = format!("Colour: <font color=\"#{r:02x}{g:02x}{b:02x}\">");
                let response = ui.add(swatch);
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &hint)
                });
                if response.on_hover_text(&hint).clicked() {
                    picked = Some(Format::Color(color));
                }
            }
        });
    picked
}

/// Hatched bars over the previewed screen's unsafe edges (a phone's notch
/// and home bar, [`safe_insets`]), which `DeviceSafeInsets` keeps clear of.
/// `board` is the screen in points, at `zoom` points per screen pixel.
fn paint_unsafe_edges(painter: &egui::Painter, board: egui::Rect, screen: (u32, u32), zoom: f32) {
    let [top, right, bottom, left] = safe_insets(screen).map(|inset| inset * zoom);
    let bars = [
        egui::Rect::from_min_max(board.min, egui::pos2(board.max.x, board.min.y + top)),
        egui::Rect::from_min_max(egui::pos2(board.max.x - right, board.min.y), board.max),
        egui::Rect::from_min_max(egui::pos2(board.min.x, board.max.y - bottom), board.max),
        egui::Rect::from_min_max(board.min, egui::pos2(board.min.x + left, board.max.y)),
    ];
    let stroke = egui::Stroke::new(
        1.0_f32,
        egui::Color32::from_rgba_unmultiplied(255, 190, 60, 150),
    );
    for bar in bars
        .into_iter()
        .filter(|bar| bar.width() > 0.5 && bar.height() > 0.5)
    {
        painter.rect_filled(
            bar,
            0.0,
            egui::Color32::from_rgba_unmultiplied(255, 170, 0, 36),
        );
        // Lines from bottom left to top right, every 8 points.
        let lines = painter.with_clip_rect(bar.intersect(painter.clip_rect()));
        let mut at = bar.left() - bar.height();
        while at < bar.right() {
            lines.line_segment(
                [
                    egui::pos2(at, bar.bottom()),
                    egui::pos2(at + bar.height(), bar.top()),
                ],
                stroke,
            );
            at += 8.0;
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
        // A clip cuts the item to its box (its rounding isn't sketched).
        let clip = match item {
            UiDrawItem::Rectangle(r) => r.clip,
            UiDrawItem::Image(image) => image.clip,
            UiDrawItem::Text(text) => text.clip,
        };
        let painter = match clip {
            Some(clip) => painter
                .with_clip_rect(rect(clip.position, clip.size).intersect(painter.clip_rect())),
            None => painter.clone(),
        };
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
