//! Handles for UI elements in the viewport, like Roblox Studio's: the
//! selected Frame or TextLabel gets an outline with eight squares. Dragging
//! the element's body moves it, and dragging a square resizes it from that
//! edge or corner.
//!
//! Moving changes the `Position` offset. Resizing changes the `Size`
//! offset, unless a stylesheet sets that axis's `width` or `height`: the
//! cascade would ignore `Size`, so the drag rewrites that CSS value instead
//! (in pixels or a percentage, as it was written). A value that can't be
//! rewritten, such as `var(--width)`, locks that axis, and its handles are
//! drawn hollow. An element in a flex container can't be moved, as the flex
//! layout places it.
//!
//! A drag also snaps, like Roblox Studio's UI editor. An edge or the center
//! of the element snaps to an edge or center of another element, or of the
//! screen, when it comes within a few points, and a guide line shows what
//! it lined up with. Children move with the element being dragged, so they
//! are not snap targets. Flex children are not snapped either: the layout
//! places them, and a guide would promise a position they can't take.
//!
//! A turned element (`Rotation`, its own or a parent's) has its outline
//! and handles turned with it, and a drag is measured along its own axes.
//! It doesn't snap: the lines it would line up with aren't straight to it.
//! Just outside each corner, a round knob turns it ([`Handles::rotate_at`]).
//! An element whose CSS keeps an `aspect-ratio` has its height worked out
//! from its width, so any handle resizes its width and the height follows.
//!
//! Everything here works in egui points, except the element rectangles
//! from `axcenity_ui`, which are in the viewport texture's physical pixels.

use std::ops::Range;

use axcenity_core::{InstanceId, UDim2, Value, World};
use axcenity_ui::{CssDeclaration, Stylesheet, UiElement, UiRect, resized_css_length};
use eframe::egui;

/// The side of a handle's square, in points.
pub(crate) const HANDLE_SIZE: f32 = 8.0;
/// How far from a handle's center the mouse still grabs it, in points.
pub(crate) const GRAB_DISTANCE: f32 = 7.0;
/// How close an edge or center must be to another's, in points, to snap.
pub(crate) const SNAP_DISTANCE: f32 = 8.0;
/// How far outside a corner the rotate knob is, along the diagonal, in points.
const ROTATE_KNOB: f32 = 12.0;
/// The rotate knob's radius, and how far from it the mouse still grabs it.
const ROTATE_KNOB_RADIUS: f32 = 3.5;
const ROTATE_REACH: f32 = 9.0;
/// How far a guide line runs past the elements it joins, in points.
const GUIDE_PAD: f32 = 4.0;
/// Studio draws alignment guides in magenta, so they read apart from the
/// blue selection outline.
const GUIDE_COLOR: egui::Color32 = egui::Color32::from_rgb(232, 64, 176);

/// Which part of an element is grabbed: -1, 0 or 1 on each axis for the
/// left (top) edge, neither, or the right (bottom) edge. [`Grip::BODY`],
/// neither on both, moves the element.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Grip {
    pub x: i8,
    pub y: i8,
}

impl Grip {
    pub const BODY: Grip = Grip { x: 0, y: 0 };

    /// The cursor that shows which way this grip drags.
    pub(crate) fn cursor(self) -> egui::CursorIcon {
        use egui::CursorIcon::*;
        match (self.x, self.y) {
            (0, 0) => Move,
            (0, _) => ResizeVertical,
            (_, 0) => ResizeHorizontal,
            (x, y) if x == y => ResizeNwSe,
            _ => ResizeNeSw,
        }
    }
}

/// What decides an element's size on one axis.
#[derive(Clone, Debug, PartialEq)]
pub enum Axis {
    /// Its `Size` property.
    Size,
    /// A CSS declaration with a plain length, which resizing rewrites.
    Css(CssDeclaration),
    /// A CSS declaration that can't be rewritten, such as `var(--width)`.
    Locked(CssDeclaration),
}

impl Axis {
    /// What decides `id`'s size along `property` (`width` or `height`).
    pub fn of(world: &World, id: InstanceId, stylesheets: &[Stylesheet], property: &str) -> Axis {
        match axcenity_ui::applied_declaration(world, id, stylesheets, property) {
            None => Axis::Size,
            Some(declaration) if resized_css_length(&declaration.value, 1.0, 1.0).is_some() => {
                Axis::Css(declaration)
            }
            Some(declaration) => Axis::Locked(declaration),
        }
    }

    fn locked(&self) -> bool {
        matches!(self, Axis::Locked(_))
    }
}

/// A new value for a CSS declaration, at its span in the stylesheet's text
/// as it was when the drag started.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CssEdit {
    pub path: String,
    pub span: Range<usize>,
    pub value: String,
}

/// What dragging a UI element changes this frame, from where it started.
#[derive(Clone, Debug, PartialEq)]
pub struct UiDragEdit {
    pub id: InstanceId,
    /// True for a resize, false for a move.
    pub resize: bool,
    pub values: Vec<(&'static str, Value)>,
    pub css: Vec<CssEdit>,
    /// Guide lines for the edges or centers this drag snapped to.
    pub guides: Vec<SnapGuide>,
}

/// A guide line drawn while a UI drag is snapped to another edge or center.
/// Coordinates are physical pixels from the viewport's top left, same as
/// [`UiRect`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapGuide {
    /// True when the line is horizontal (a shared Y). A vertical line shares X.
    pub horizontal: bool,
    /// The shared coordinate: Y when `horizontal`, otherwise X.
    pub at: f32,
    /// The line's extent on the other axis. Drawing extends it a few points past this.
    pub from: f32,
    pub to: f32,
}

/// The selected UI element's outline and handles, as on screen now.
pub struct Handles {
    pub element: UiElement,
    /// The element, in points.
    pub rect: egui::Rect,
    pub width: Axis,
    pub height: Axis,
    /// Screen pixels per authored pixel. See [`UiElement::pixels_per_unit`].
    pixels_per_unit: f32,
    /// Its CSS `aspect-ratio` (width over height), which its width decides.
    ratio: Option<f32>,
    /// Where the layout's pixel 0, 0 is, in points, and pixels per point.
    origin: egui::Pos2,
    pixels_per_point: f32,
}

impl Handles {
    /// Handles for `element`, in a viewport whose top left is at `origin`.
    pub fn new(
        element: UiElement,
        world: &World,
        stylesheets: &[Stylesheet],
        origin: egui::Pos2,
        pixels_per_point: f32,
    ) -> Handles {
        let UiRect {
            x,
            y,
            width,
            height,
        } = element.rect;
        let rect = egui::Rect::from_min_size(
            origin + egui::vec2(x, y) / pixels_per_point,
            egui::vec2(width, height) / pixels_per_point,
        );
        Handles {
            element,
            rect,
            width: Axis::of(world, element.id, stylesheets, "width"),
            height: Axis::of(world, element.id, stylesheets, "height"),
            pixels_per_unit: element.pixels_per_unit,
            ratio: axcenity_ui::aspect_ratio_of(world, element.id, stylesheets),
            origin,
            pixels_per_point: pixels_per_point.max(1e-4),
        }
    }

    /// Where a point of the upright element (in points, where `rect` is)
    /// is drawn: turned with the element.
    pub fn on_screen(&self, point: egui::Pos2) -> egui::Pos2 {
        let pixel = (point - self.origin) * self.pixels_per_point;
        let [x, y] = self.element.on_screen([pixel.x, pixel.y]);
        self.origin + egui::vec2(x, y) / self.pixels_per_point
    }

    /// A point on screen, in points, in the upright element's frame.
    pub fn upright(&self, point: egui::Pos2) -> egui::Pos2 {
        let pixel = (point - self.origin) * self.pixels_per_point;
        let [x, y] = self.element.local([pixel.x, pixel.y]);
        self.origin + egui::vec2(x, y) / self.pixels_per_point
    }

    /// The element's middle on screen, in points: what it turns around.
    pub fn centre(&self) -> egui::Pos2 {
        self.on_screen(self.rect.center())
    }

    /// The rotate knobs, just outside each corner, upright (where `rect` is).
    fn rotate_knobs(&self) -> [egui::Pos2; 4] {
        let r = self.rect;
        let out = ROTATE_KNOB / std::f32::consts::SQRT_2;
        [
            r.left_top() + egui::vec2(-out, -out),
            r.right_top() + egui::vec2(out, -out),
            r.left_bottom() + egui::vec2(-out, out),
            r.right_bottom() + egui::vec2(out, out),
        ]
    }

    /// Whether `pointer` is on a rotate knob, outside a corner.
    pub fn rotate_at(&self, pointer: egui::Pos2) -> bool {
        let upright = self.upright(pointer);
        !self.rect.contains(upright)
            && self
                .rotate_knobs()
                .iter()
                .any(|knob| knob.distance(upright) <= ROTATE_REACH)
    }

    /// The eight handles and where their centers are, upright.
    fn handles(&self) -> impl Iterator<Item = (Grip, egui::Pos2)> + '_ {
        let (left, right) = (self.rect.left(), self.rect.right());
        let (top, bottom) = (self.rect.top(), self.rect.bottom());
        let center = self.rect.center();
        let across = |side: i8, start: f32, middle: f32, end: f32| match side {
            -1 => start,
            0 => middle,
            _ => end,
        };
        [-1, 0, 1]
            .into_iter()
            .flat_map(|y| [-1, 0, 1].map(|x| Grip { x, y }))
            .filter(|&grip| grip != Grip::BODY)
            .map(move |grip| {
                let at = egui::pos2(
                    across(grip.x, left, center.x, right),
                    across(grip.y, top, center.y, bottom),
                );
                (grip, at)
            })
    }

    /// The grip under `pointer`: the nearest handle in reach, or else the
    /// body if the pointer is over the element.
    pub fn grip_at(&self, pointer: egui::Pos2) -> Option<Grip> {
        // Turning keeps distances, so the upright frame measures the same.
        let pointer = self.upright(pointer);
        let nearest = self
            .handles()
            .map(|(grip, at)| (grip, at.distance(pointer)))
            .filter(|&(_, distance)| distance <= GRAB_DISTANCE)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(grip, _)| grip);
        nearest.or_else(|| self.rect.contains(pointer).then_some(Grip::BODY))
    }

    /// Whether dragging `grip` would change anything. With an aspect
    /// ratio, every handle resizes the width.
    pub fn can_drag(&self, grip: Grip) -> bool {
        if grip == Grip::BODY {
            return !self.element.in_flow;
        }
        if self.ratio.is_some() {
            return !self.width.locked();
        }
        (grip.x != 0 && !self.width.locked()) || (grip.y != 0 && !self.height.locked())
    }

    /// The cursor to show over `grip`.
    pub fn cursor(&self, grip: Grip) -> egui::CursorIcon {
        if self.can_drag(grip) {
            grip.cursor()
        } else {
            egui::CursorIcon::NotAllowed
        }
    }

    /// Draws the outline and handles, `highlighted` one brighter. A handle
    /// that can't resize anything is hollow.
    pub fn paint(&self, painter: &egui::Painter, color: egui::Color32, highlighted: Option<Grip>) {
        let r = self.rect;
        let corners = [
            r.left_top(),
            r.right_top(),
            r.right_bottom(),
            r.left_bottom(),
        ]
        .map(|corner| self.on_screen(corner))
        .to_vec();
        painter.add(egui::Shape::closed_line(
            corners,
            egui::Stroke::new(1.5_f32, color),
        ));
        for (grip, at) in self.handles() {
            let at = self.on_screen(at);
            let square = egui::Rect::from_center_size(at, egui::vec2(HANDLE_SIZE, HANDLE_SIZE));
            let fill = if highlighted == Some(grip) {
                egui::Color32::WHITE
            } else {
                color
            };
            if self.can_drag(grip) {
                painter.rect_filled(square, 1.0, fill);
            } else {
                painter.rect_filled(square, 1.0, egui::Color32::from_black_alpha(140));
            }
            painter.rect_stroke(
                square,
                1.0,
                egui::Stroke::new(1.0_f32, fill),
                egui::StrokeKind::Middle,
            );
        }
    }

    /// Draws the rotate knobs outside the corners, `lit` brighter.
    pub fn paint_rotate_knobs(&self, painter: &egui::Painter, color: egui::Color32, lit: bool) {
        let fill = if lit { egui::Color32::WHITE } else { color };
        for knob in self.rotate_knobs() {
            let at = self.on_screen(knob);
            painter.circle_filled(at, ROTATE_KNOB_RADIUS, egui::Color32::from_black_alpha(140));
            painter.circle_stroke(at, ROTATE_KNOB_RADIUS, egui::Stroke::new(1.2_f32, fill));
        }
    }

    /// The element's size in screen pixels, how far it sits from the screen
    /// edges, and (when `parent` is its frame) how far it sits from that frame.
    pub fn paint_measures(
        &self,
        painter: &egui::Painter,
        viewport: egui::Rect,
        parent: Option<egui::Rect>,
        pixels_per_point: f32,
    ) {
        if pixels_per_point <= 0.0 {
            return;
        }
        let px = |points: f32| (points * pixels_per_point).round() as i32;
        let size = format!("{} × {} px", px(self.rect.width()), px(self.rect.height()));
        let screen = edge_text(
            "screen",
            px(self.rect.left() - viewport.left()),
            px(viewport.right() - self.rect.right()),
            px(self.rect.top() - viewport.top()),
            px(viewport.bottom() - self.rect.bottom()),
        );
        let frame = parent.map(|parent| {
            edge_text(
                "frame",
                px(self.rect.left() - parent.left()),
                px(parent.right() - self.rect.right()),
                px(self.rect.top() - parent.top()),
                px(parent.bottom() - self.rect.bottom()),
            )
        });
        let mut lines = vec![size, screen];
        if let Some(frame) = frame {
            lines.push(frame);
        }
        paint_badges(painter, self.rect, viewport, &lines);
        paint_edge_gaps(
            painter,
            viewport,
            self.rect,
            egui::Color32::from_rgb(120, 190, 255),
            pixels_per_point,
        );
        if let Some(parent) = parent {
            paint_edge_gaps(painter, parent, self.rect, GUIDE_COLOR, pixels_per_point);
        }
    }
}

/// `left right top bottom`, in pixels, from `name`'s edges.
fn edge_text(name: &str, left: i32, right: i32, top: i32, bottom: i32) -> String {
    format!("{name}   ← {left}   → {right}   ↑ {top}   ↓ {bottom}")
}

/// A dark label under `rect`, kept inside `viewport`.
fn paint_badges(painter: &egui::Painter, rect: egui::Rect, viewport: egui::Rect, lines: &[String]) {
    let font = egui::FontId::monospace(11.0);
    let width = lines
        .iter()
        .map(|line| line.chars().count() as f32 * 6.6)
        .fold(0.0_f32, f32::max)
        + 10.0;
    let height = lines.len() as f32 * 15.0 + 6.0;
    let mut origin = egui::pos2(rect.left(), rect.bottom() + 6.0);
    if origin.y + height > viewport.bottom() {
        origin.y = (rect.top() - height - 6.0).max(viewport.top());
    }
    origin.x = origin.x.clamp(
        viewport.left(),
        (viewport.right() - width).max(viewport.left()),
    );
    let background = egui::Rect::from_min_size(origin, egui::vec2(width, height));
    painter.rect_filled(background, 3.0, egui::Color32::from_black_alpha(180));
    for (index, line) in lines.iter().enumerate() {
        painter.text(
            origin + egui::vec2(5.0, 4.0 + index as f32 * 15.0),
            egui::Align2::LEFT_TOP,
            line,
            font.clone(),
            egui::Color32::WHITE,
        );
    }
}

/// Lines across the gap between `bounds` and `inner` on each side, with the
/// pixel distance when the gap is wide enough to hold it.
fn paint_edge_gaps(
    painter: &egui::Painter,
    bounds: egui::Rect,
    inner: egui::Rect,
    color: egui::Color32,
    pixels_per_point: f32,
) {
    let stroke = egui::Stroke::new(1.0_f32, color);
    let gaps = [
        (
            egui::pos2(bounds.left(), inner.center().y),
            egui::pos2(inner.left(), inner.center().y),
            inner.left() - bounds.left(),
        ),
        (
            egui::pos2(inner.right(), inner.center().y),
            egui::pos2(bounds.right(), inner.center().y),
            bounds.right() - inner.right(),
        ),
        (
            egui::pos2(inner.center().x, bounds.top()),
            egui::pos2(inner.center().x, inner.top()),
            inner.top() - bounds.top(),
        ),
        (
            egui::pos2(inner.center().x, inner.bottom()),
            egui::pos2(inner.center().x, bounds.bottom()),
            bounds.bottom() - inner.bottom(),
        ),
    ];
    for (from, to, gap) in gaps {
        if gap < 4.0 {
            continue;
        }
        painter.line_segment([from, to], stroke);
        if gap >= 28.0 {
            let pixels = (gap * pixels_per_point).round() as i32;
            painter.text(
                from + (to - from) * 0.5,
                egui::Align2::CENTER_CENTER,
                pixels.to_string(),
                egui::FontId::monospace(10.0),
                color,
            );
        }
    }
}

/// A UI element being dragged by its body or one of its handles.
#[derive(Clone, Debug)]
pub struct UiDrag {
    pub id: InstanceId,
    pub grip: Grip,
    /// Where the mouse went down, in points.
    start_pointer: egui::Pos2,
    /// The element on screen when the drag started, in pixels.
    start: UiRect,
    position: UDim2,
    size: UDim2,
    in_flow: bool,
    width: Axis,
    height: Axis,
    /// Screen pixels per authored pixel. Property offsets are in authored pixels.
    pixels_per_unit: f32,
    /// Its CSS `aspect-ratio`: a resize changes the width, and the height follows.
    ratio: Option<f32>,
    /// How far it's turned on screen, and how far its parent is (which
    /// its `Position` is measured in), in degrees.
    degrees: f32,
    parent_degrees: f32,
}

impl UiDrag {
    /// Starts dragging `grip` of the element `handles` are on, from
    /// `pointer`. `None` if the grip can't drag anything.
    pub fn start(
        handles: &Handles,
        grip: Grip,
        pointer: egui::Pos2,
        world: &World,
    ) -> Option<UiDrag> {
        if !handles.can_drag(grip) {
            return None;
        }
        let instance = world.get(handles.element.id)?;
        let udim2 = |name| instance.property(name).and_then(Value::as_udim2);
        let degrees = handles.element.degrees();
        let own = instance
            .property("Rotation")
            .and_then(Value::as_number)
            .filter(|n| n.is_finite())
            .unwrap_or(0.0) as f32;
        Some(UiDrag {
            id: handles.element.id,
            grip,
            start_pointer: pointer,
            start: handles.element.rect,
            position: udim2("Position")?,
            size: udim2("Size")?,
            in_flow: handles.element.in_flow,
            width: handles.width.clone(),
            height: handles.height.clone(),
            pixels_per_unit: handles.pixels_per_unit.max(1e-4),
            ratio: handles.ratio,
            degrees,
            parent_degrees: degrees - own,
        })
    }

    /// What the element becomes with the mouse at `pointer`. Moves and
    /// sizes are in whole pixels. `elements` are this frame's layout, in
    /// the same pixels, and `viewport` is the screen they sit on; both are
    /// what the drag can snap to.
    pub fn update(
        &self,
        pointer: egui::Pos2,
        pixels_per_point: f32,
        world: &World,
        elements: &[UiElement],
        viewport: UiRect,
    ) -> UiDragEdit {
        self.update_with_guides(pointer, pixels_per_point, world, elements, viewport, &[])
    }

    /// [`update`](Self::update), also snapping to `guides`: lines across
    /// the screen, each a rectangle with no width (a vertical line) or no
    /// height (a horizontal one), in the same pixels.
    pub fn update_with_guides(
        &self,
        pointer: egui::Pos2,
        pixels_per_point: f32,
        world: &World,
        elements: &[UiElement],
        viewport: UiRect,
        guides: &[UiRect],
    ) -> UiDragEdit {
        let pointer_delta = (pointer - self.start_pointer) * pixels_per_point;
        // A move is along the parent's axes and a resize along the
        // element's own, turned as they are on screen.
        let turned = if self.grip == Grip::BODY {
            self.parent_degrees
        } else {
            self.degrees
        };
        let [dx, dy] =
            axcenity_render::turn_point([pointer_delta.x, pointer_delta.y], [0.0, 0.0], -turned);
        let mut delta = egui::vec2(dx.round(), dy.round());
        // Flex layout ignores Position, so snapping its edges would draw a
        // guide the element can't actually land on. Turned edges don't
        // line up with straight ones.
        let guides = if self.in_flow || axcenity_render::is_turned(self.degrees) {
            Vec::new()
        } else {
            self.snap(
                &mut delta,
                world,
                elements,
                viewport,
                guides,
                pixels_per_point,
            )
        };
        // Fit mode shows a scaled copy. Offsets and CSS pixels stay in the
        // design size, so the snapped screen movement is converted back.
        let design = egui::vec2(
            (delta.x / self.pixels_per_unit).round(),
            (delta.y / self.pixels_per_unit).round(),
        );
        let start_width = self.start.width / self.pixels_per_unit;
        let start_height = self.start.height / self.pixels_per_unit;

        let (mut position, mut size) = (self.position, self.size);
        let mut values = Vec::new();
        let mut css = Vec::new();
        if self.grip == Grip::BODY {
            if !self.in_flow {
                position.x.offset += design.x;
                position.y.offset += design.y;
                values.push(("Position", Value::UDim2(position)));
            }
            return UiDragEdit {
                id: self.id,
                resize: false,
                values,
                css,
                guides,
            };
        }

        let (mut moved, mut resized) = (false, false);
        if let Some(ratio) = self.ratio {
            // The width decides the height: a side handle sizes it by its
            // width, a top or bottom one by the height it asks for.
            let (length, shift) = if self.grip.x != 0 {
                resize(self.grip.x, start_width, design.x)
            } else {
                let (height, _) = resize(self.grip.y, start_height, design.y);
                (height * ratio, 0.0)
            };
            let rise = if self.grip.y < 0 {
                start_height - length / ratio
            } else {
                0.0
            };
            match &self.width {
                Axis::Size => {
                    size.x.offset += length - start_width;
                    values.push(("Size", Value::UDim2(size)));
                }
                Axis::Css(declaration) => {
                    if let Some(value) = resized_css_length(&declaration.value, start_width, length)
                    {
                        css.push(CssEdit {
                            path: declaration.path.clone(),
                            span: declaration.span.clone(),
                            value,
                        });
                    }
                }
                Axis::Locked(_) => {}
            }
            if !self.in_flow && (shift != 0.0 || rise != 0.0) {
                position.x.offset += shift;
                position.y.offset += rise.round();
                values.push(("Position", Value::UDim2(position)));
            }
            return UiDragEdit {
                id: self.id,
                resize: true,
                values,
                css,
                guides,
            };
        }
        // `&mut size.x` and `&mut size.y` borrow different fields, so both
        // can be held at once.
        let axes = [
            (
                &self.width,
                self.grip.x,
                start_width,
                design.x,
                &mut size.x,
                &mut position.x,
            ),
            (
                &self.height,
                self.grip.y,
                start_height,
                design.y,
                &mut size.y,
                &mut position.y,
            ),
        ];
        for (axis, side, start, delta, size, position) in axes {
            if side == 0 || axis.locked() {
                continue;
            }
            let (length, shift) = resize(side, start, delta);
            match axis {
                Axis::Size => {
                    size.offset += length - start;
                    resized = true;
                }
                Axis::Css(declaration) => {
                    if let Some(value) = resized_css_length(&declaration.value, start, length) {
                        css.push(CssEdit {
                            path: declaration.path.clone(),
                            span: declaration.span.clone(),
                            value,
                        });
                    }
                }
                Axis::Locked(_) => unreachable!("skipped above"),
            }
            if !self.in_flow && shift != 0.0 {
                position.offset += shift;
                moved = true;
            }
        }
        if resized {
            values.push(("Size", Value::UDim2(size)));
        }
        if moved {
            values.push(("Position", Value::UDim2(position)));
        }
        UiDragEdit {
            id: self.id,
            resize: true,
            values,
            css,
            guides,
        }
    }

    /// Pulls `delta` onto the nearest edges and centers within the snap
    /// distance, and returns the guide lines for the ones it landed on.
    fn snap(
        &self,
        delta: &mut egui::Vec2,
        world: &World,
        elements: &[UiElement],
        viewport: UiRect,
        guides: &[UiRect],
        pixels_per_point: f32,
    ) -> Vec<SnapGuide> {
        let mut targets = snap_targets(world, self.id, elements, viewport);
        targets.extend_from_slice(guides);
        let threshold = SNAP_DISTANCE * pixels_per_point;
        let x = self.axis_snap(delta.x, true, &targets, threshold);
        let y = self.axis_snap(delta.y, false, &targets, threshold);
        if let Some(snap) = &x {
            delta.x += snap.adjustment;
        }
        if let Some(snap) = &y {
            delta.y += snap.adjustment;
        }
        let rect = self.placed_rect(*delta);
        let mut guides = Vec::new();
        if let Some(snap) = x {
            guides.push(guide_line(rect, true, snap.at, &targets));
        }
        if let Some(snap) = y {
            guides.push(guide_line(rect, false, snap.at, &targets));
        }
        guides
    }

    /// The snap on one axis, if any edge being dragged is close enough.
    /// `along_x` is the horizontal position (a vertical guide).
    fn axis_snap(
        &self,
        delta: f32,
        along_x: bool,
        targets: &[UiRect],
        threshold: f32,
    ) -> Option<AxisSnap> {
        if !self.snaps_axis(along_x) {
            return None;
        }
        let mut candidates = Vec::new();
        for edge in moving_edges(self, delta, along_x) {
            for target in targets {
                for line in axis_lines(axis_start(target, along_x), axis_length(target, along_x)) {
                    let gap = line - edge;
                    if gap.abs() <= threshold {
                        candidates.push((gap.abs(), whole_adjustment(gap), line));
                    }
                }
            }
        }
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
        candidates.into_iter().find_map(|(_, adjustment, at)| {
            size_stays_open(self, delta + adjustment, along_x)
                .then_some(AxisSnap { adjustment, at })
        })
    }

    /// Whether this drag moves the position on `along_x`. A locked CSS
    /// length can't, and a resize handle only moves its own axis.
    fn snaps_axis(&self, along_x: bool) -> bool {
        let (side, locked) = if along_x {
            (self.grip.x, self.width.locked())
        } else {
            (self.grip.y, self.height.locked())
        };
        self.grip == Grip::BODY || (side != 0 && !locked)
    }

    /// Where the element sits after a drag of `delta` pixels, with the same
    /// clamping as the property edit.
    fn placed_rect(&self, delta: egui::Vec2) -> UiRect {
        let mut rect = self.start;
        if self.grip == Grip::BODY {
            rect.x += delta.x;
            rect.y += delta.y;
            return rect;
        }
        if self.grip.x != 0 && !self.width.locked() {
            let (length, shift) = resize(self.grip.x, self.start.width, delta.x);
            rect.x += shift;
            rect.width = length;
        }
        if self.grip.y != 0 && !self.height.locked() {
            let (length, shift) = resize(self.grip.y, self.start.height, delta.y);
            rect.y += shift;
            rect.height = length;
        }
        rect
    }
}

/// Resizes one axis of length `length` by dragging an edge `delta` pixels.
/// `side` is -1 for the start (left or top) edge and 1 for the end edge.
/// Returns the new length, never below zero, and how far the start edge moved.
pub(crate) fn resize(side: i8, length: f32, delta: f32) -> (f32, f32) {
    if side < 0 {
        let new = (length - delta).max(0.0);
        (new, length - new)
    } else {
        ((length + delta).max(0.0), 0.0)
    }
}

/// One axis of a drag lining up with a target line.
struct AxisSnap {
    /// Whole pixels added to the pointer delta so the edge lands on the line.
    adjustment: f32,
    /// The target line, in pixels.
    at: f32,
}

/// The rectangles a drag can line up with: the screen, and every element
/// except itself and its descendants. A descendant's edges move with the
/// drag, so snapping to one would pin the element in place.
fn snap_targets(
    world: &World,
    id: InstanceId,
    elements: &[UiElement],
    viewport: UiRect,
) -> Vec<UiRect> {
    let mut targets = Vec::new();
    if viewport.width > 0.0 && viewport.height > 0.0 {
        targets.push(viewport);
    }
    for element in elements {
        if element.id == id || descends_from(world, element.id, id) {
            continue;
        }
        targets.push(element.rect);
    }
    targets
}

pub(crate) fn descends_from(world: &World, id: InstanceId, ancestor: InstanceId) -> bool {
    let mut next = world.get(id).and_then(|instance| instance.parent());
    while let Some(parent) = next {
        if parent == ancestor {
            return true;
        }
        next = world.get(parent).and_then(|instance| instance.parent());
    }
    false
}

/// Start, center and end of one axis.
pub(crate) fn axis_lines(start: f32, length: f32) -> [f32; 3] {
    [start, start + length * 0.5, start + length]
}

pub(crate) fn axis_start(rect: &UiRect, along_x: bool) -> f32 {
    if along_x { rect.x } else { rect.y }
}

pub(crate) fn axis_length(rect: &UiRect, along_x: bool) -> f32 {
    if along_x { rect.width } else { rect.height }
}

/// The edges of `drag` that move on this axis, after a pointer delta of
/// `delta` pixels and before snapping. A move uses the start, center and
/// end; a resize uses only the edge under the handle.
fn moving_edges(drag: &UiDrag, delta: f32, along_x: bool) -> Vec<f32> {
    let start = axis_start(&drag.start, along_x);
    let length = axis_length(&drag.start, along_x);
    let side = if along_x { drag.grip.x } else { drag.grip.y };
    if drag.grip == Grip::BODY {
        let moved = start + delta;
        vec![moved, moved + length * 0.5, moved + length]
    } else if side < 0 {
        vec![start + delta]
    } else {
        vec![start + length + delta]
    }
}

/// The whole-pixel nudge that lands closer to the target line than staying
/// put. Zero when the edge is already on the nearest pixel, so a guide can
/// show without the element jumping to the other side of the line.
pub(crate) fn whole_adjustment(gap: f32) -> f32 {
    let rounded = gap.round();
    if (gap - rounded).abs() < gap.abs() {
        rounded
    } else {
        0.0
    }
}

/// False when applying `delta` would push this axis's length below zero.
/// A move has no length to protect.
fn size_stays_open(drag: &UiDrag, delta: f32, along_x: bool) -> bool {
    let side = if along_x { drag.grip.x } else { drag.grip.y };
    if drag.grip == Grip::BODY || side == 0 {
        return true;
    }
    let length = axis_length(&drag.start, along_x);
    let new_length = if side < 0 {
        length - delta
    } else {
        length + delta
    };
    new_length >= 0.0
}

fn cross_span(rect: UiRect, along_x: bool) -> (f32, f32) {
    if along_x {
        (rect.y, rect.y + rect.height)
    } else {
        (rect.x, rect.x + rect.width)
    }
}

/// A guide through `at` covering the dragged rect and every target that
/// shares that line. `along_x` means the snapped axis is X, so the guide
/// itself is vertical.
pub(crate) fn guide_line(rect: UiRect, along_x: bool, at: f32, targets: &[UiRect]) -> SnapGuide {
    let (mut from, mut to) = cross_span(rect, along_x);
    for target in targets {
        let shares_line = axis_lines(axis_start(target, along_x), axis_length(target, along_x))
            .into_iter()
            .any(|line| (line - at).abs() <= 0.5);
        if shares_line {
            let (start, end) = cross_span(*target, along_x);
            from = from.min(start);
            to = to.max(end);
        }
    }
    SnapGuide {
        horizontal: !along_x,
        at,
        from,
        to,
    }
}

/// Draws `guides` over the viewport. `origin` is the viewport's top left in
/// points, and the guide coordinates are physical pixels.
pub fn paint_guides(
    guides: &[SnapGuide],
    painter: &egui::Painter,
    origin: egui::Pos2,
    pixels_per_point: f32,
) {
    if pixels_per_point <= 0.0 {
        return;
    }
    let stroke = egui::Stroke::new(1.0_f32, GUIDE_COLOR);
    for guide in guides {
        let at = guide.at / pixels_per_point;
        let from = guide.from.min(guide.to) / pixels_per_point - GUIDE_PAD;
        let to = guide.from.max(guide.to) / pixels_per_point + GUIDE_PAD;
        let (a, b) = if guide.horizontal {
            (origin + egui::vec2(from, at), origin + egui::vec2(to, at))
        } else {
            (origin + egui::vec2(at, from), origin + egui::vec2(at, to))
        };
        painter.line_segment([a, b], stroke);
    }
}

/// `text` with each edit's span replaced by its value. The spans are all in
/// `text`, so the edits go from last to first, leaving earlier spans in place.
pub fn apply_css_edits<'a>(text: &str, edits: impl Iterator<Item = &'a CssEdit>) -> String {
    let mut edits: Vec<&CssEdit> = edits.collect();
    edits.sort_by_key(|edit| std::cmp::Reverse(edit.span.start));
    let mut text = text.to_owned();
    for edit in edits {
        if text.get(edit.span.clone()).is_some() {
            text.replace_range(edit.span.clone(), &edit.value);
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use axcenity_core::ClassName;
    use axcenity_ui::UiLayout;

    use super::*;

    /// The 800×600 viewport `frame` lays out into.
    fn viewport() -> UiRect {
        UiRect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 600.0,
        }
    }

    /// A ScreenGui holding a Frame at (100, 50) sized 200×80 by its
    /// properties, laid out in an 800×600 viewport, with `css` applied.
    fn frame(css: &str) -> (World, Vec<Stylesheet>, Handles, UiLayout) {
        let mut world = World::new();
        let gui = world.create(ClassName::ScreenGui).unwrap();
        world.set_parent(gui, Some(world.workspace())).unwrap();
        let frame = world.create(ClassName::Frame).unwrap();
        world.set_parent(frame, Some(gui)).unwrap();
        world
            .set_property(frame, "Class", Value::String("hud".into()))
            .unwrap();
        world
            .set_property(
                frame,
                "Position",
                Value::UDim2(UDim2::new(0.0, 100.0, 0.0, 50.0)),
            )
            .unwrap();
        world
            .set_property(
                frame,
                "Size",
                Value::UDim2(UDim2::new(0.0, 200.0, 0.0, 80.0)),
            )
            .unwrap();
        let (stylesheet, _) = axcenity_ui::parse_stylesheet("ui.css", css);
        let stylesheets = vec![stylesheet];
        let layout = axcenity_ui::layout_ui(&world, 800, 600, &stylesheets);
        let element = *layout.element(frame).unwrap();
        let handles = Handles::new(element, &world, &stylesheets, egui::Pos2::ZERO, 1.0);
        (world, stylesheets, handles, layout)
    }

    fn moved(
        drag: &UiDrag,
        pointer: egui::Pos2,
        pixels_per_point: f32,
        world: &World,
        layout: &UiLayout,
    ) -> UiDragEdit {
        drag.update(
            pointer,
            pixels_per_point,
            world,
            &layout.elements,
            viewport(),
        )
    }

    fn udim2(edit: &UiDragEdit, name: &str) -> Option<UDim2> {
        edit.values
            .iter()
            .find(|(property, _)| *property == name)
            .and_then(|(_, value)| value.as_udim2())
    }

    #[test]
    fn with_an_aspect_ratio_every_handle_resizes_the_width_and_the_height_follows() {
        // 200 wide at 2:1 is 100 tall, whatever Size says.
        let (world, _, handles, layout) = frame(".hud { aspect-ratio: 2 }");
        assert_eq!(handles.rect.height(), 100.0);
        let size = |grip: Grip, from: egui::Pos2, by: egui::Vec2| {
            let drag = UiDrag::start(&handles, grip, from, &world).unwrap();
            let edit = moved(&drag, from + by, 1.0, &world, &layout);
            (udim2(&edit, "Size"), udim2(&edit, "Position"))
        };
        // The bottom handle asks for 110 tall: 220 wide.
        let (bottom, _) = size(
            Grip { x: 0, y: 1 },
            egui::pos2(200.0, 150.0),
            egui::vec2(0.0, 10.0),
        );
        assert_eq!(bottom.unwrap().x.offset, 220.0);
        let (right, _) = size(
            Grip { x: 1, y: 0 },
            egui::pos2(300.0, 100.0),
            egui::vec2(20.0, 0.0),
        );
        assert_eq!(right.unwrap().x.offset, 220.0);
        // The top left corner keeps the bottom right where it is.
        let (corner, position) = size(
            Grip { x: -1, y: -1 },
            egui::pos2(100.0, 50.0),
            egui::vec2(-20.0, -3.0),
        );
        assert_eq!(corner.unwrap().x.offset, 220.0);
        let position = position.unwrap();
        assert_eq!((position.x.offset, position.y.offset), (80.0, 40.0));
    }

    #[test]
    fn a_turned_elements_handles_turn_with_it_and_drag_along_its_own_axes() {
        let mut world = World::new();
        let gui = world.create(ClassName::ScreenGui).unwrap();
        world.set_parent(gui, Some(world.workspace())).unwrap();
        let frame = world.create(ClassName::Frame).unwrap();
        world.set_parent(frame, Some(gui)).unwrap();
        world
            .set_property(
                frame,
                "Position",
                Value::UDim2(UDim2::new(0.0, 100.0, 0.0, 50.0)),
            )
            .unwrap();
        world
            .set_property(
                frame,
                "Size",
                Value::UDim2(UDim2::new(0.0, 200.0, 0.0, 100.0)),
            )
            .unwrap();
        world
            .set_property(frame, "Rotation", Value::Number(90.0))
            .unwrap();
        let layout = axcenity_ui::layout_ui(&world, 800, 600, &[]);
        let element = *layout.element(frame).unwrap();
        let handles = Handles::new(element, &world, &[], egui::Pos2::ZERO, 1.0);
        // Turned a quarter around (200, 100), its right edge faces down.
        let right = Grip { x: 1, y: 0 };
        assert_eq!(handles.grip_at(egui::pos2(200.0, 200.0)), Some(right));
        assert_eq!(handles.grip_at(egui::pos2(300.0, 100.0)), None);
        assert!(handles.centre().distance(egui::pos2(200.0, 100.0)) < 1e-3);
        // Dragging it down makes it wider.
        let drag = UiDrag::start(&handles, right, egui::pos2(200.0, 200.0), &world).unwrap();
        let edit = moved(&drag, egui::pos2(200.0, 230.0), 1.0, &world, &layout);
        assert_eq!(udim2(&edit, "Size").unwrap().x.offset, 230.0);
        // Its knobs are just outside its turned corners.
        let corner = handles.on_screen(handles.rect.right_bottom());
        assert!(
            handles.rotate_at(corner + egui::vec2(-8.0, 8.0)),
            "{corner:?}"
        );
        assert!(!handles.rotate_at(egui::pos2(200.0, 100.0)));
    }

    #[test]
    fn handles_sit_on_the_corners_and_edges_and_the_body_is_inside() {
        let (_, _, handles, _) = frame("");
        assert_eq!(
            handles.rect,
            egui::Rect::from_min_size(egui::pos2(100.0, 50.0), egui::vec2(200.0, 80.0))
        );
        assert_eq!(
            handles.grip_at(egui::pos2(300.0, 130.0)),
            Some(Grip { x: 1, y: 1 })
        );
        assert_eq!(
            handles.grip_at(egui::pos2(102.0, 90.0)),
            Some(Grip { x: -1, y: 0 })
        );
        assert_eq!(handles.grip_at(egui::pos2(200.0, 90.0)), Some(Grip::BODY));
        assert_eq!(handles.grip_at(egui::pos2(500.0, 500.0)), None);
    }

    #[test]
    fn dragging_the_body_moves_and_the_left_edge_resizes_and_moves() {
        let (world, _, handles, layout) = frame("");
        let start = egui::pos2(200.0, 90.0);
        let drag = UiDrag::start(&handles, Grip::BODY, start, &world).unwrap();
        let edit = moved(&drag, start + egui::vec2(10.4, -5.0), 1.0, &world, &layout);
        assert!(!edit.resize);
        assert_eq!(
            udim2(&edit, "Position"),
            Some(UDim2::new(0.0, 110.0, 0.0, 45.0))
        );

        let start = egui::pos2(100.0, 90.0);
        let drag = UiDrag::start(&handles, Grip { x: -1, y: 0 }, start, &world).unwrap();
        let edit = moved(&drag, start + egui::vec2(-20.0, 30.0), 1.0, &world, &layout);
        assert!(edit.resize && edit.css.is_empty());
        assert_eq!(
            udim2(&edit, "Size"),
            Some(UDim2::new(0.0, 220.0, 0.0, 80.0))
        );
        assert_eq!(
            udim2(&edit, "Position"),
            Some(UDim2::new(0.0, 80.0, 0.0, 50.0))
        );
    }

    #[test]
    fn a_css_width_is_rewritten_and_a_variable_locks_its_axis() {
        let css = ".hud { width: 200px; height: var(--tall, 80px) }";
        let (world, _, handles, layout) = frame(css);
        assert!(matches!(handles.width, Axis::Css(_)));
        assert!(matches!(handles.height, Axis::Locked(_)));
        assert!(!handles.can_drag(Grip { x: 0, y: 1 }));
        assert!(handles.can_drag(Grip { x: 1, y: 1 }));

        let start = egui::pos2(300.0, 130.0);
        let drag = UiDrag::start(&handles, Grip { x: 1, y: 1 }, start, &world).unwrap();
        // Two pixels per point: 25 points is 50 pixels.
        let edit = moved(&drag, start + egui::vec2(25.0, 25.0), 2.0, &world, &layout);
        assert_eq!(
            udim2(&edit, "Size"),
            None,
            "CSS decides the width, and the height is locked"
        );
        assert_eq!(edit.css.len(), 1);
        assert_eq!(
            apply_css_edits(css, edit.css.iter()),
            ".hud { width: 250px; height: var(--tall, 80px) }"
        );
    }

    #[test]
    fn several_edits_to_one_text_keep_their_places() {
        let text = "a { width: 10px; height: 20px }";
        let edit = |span: Range<usize>, value: &str| CssEdit {
            path: "ui.css".into(),
            span,
            value: value.into(),
        };
        let edits = [edit(11..15, "100px"), edit(25..29, "5px")];
        assert_eq!(
            apply_css_edits(text, edits.iter()),
            "a { width: 100px; height: 5px }"
        );
    }

    fn add_frame(
        world: &mut World,
        parent: InstanceId,
        name: &str,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) -> InstanceId {
        let frame = world.create(ClassName::Frame).unwrap();
        world.set_parent(frame, Some(parent)).unwrap();
        world.set_name(frame, name).unwrap();
        world
            .set_property(frame, "Position", Value::UDim2(UDim2::new(0.0, x, 0.0, y)))
            .unwrap();
        world
            .set_property(
                frame,
                "Size",
                Value::UDim2(UDim2::new(0.0, width, 0.0, height)),
            )
            .unwrap();
        frame
    }

    /// A ScreenGui of absolutely positioned frames, laid out in the test viewport.
    fn screen_with(frames: &[(&str, f32, f32, f32, f32)]) -> (World, UiLayout) {
        let mut world = World::new();
        let gui = world.create(ClassName::ScreenGui).unwrap();
        world.set_parent(gui, Some(world.workspace())).unwrap();
        for &(name, x, y, width, height) in frames {
            add_frame(&mut world, gui, name, x, y, width, height);
        }
        let layout = axcenity_ui::layout_ui(&world, 800, 600, &[]);
        (world, layout)
    }

    fn drag_named(
        world: &World,
        layout: &UiLayout,
        name: &str,
        grip: Grip,
        by: egui::Vec2,
    ) -> UiDragEdit {
        let id = layout
            .elements
            .iter()
            .find(|element| world.get(element.id).unwrap().name() == name)
            .unwrap()
            .id;
        let element = *layout.element(id).unwrap();
        let handles = Handles::new(element, world, &[], egui::Pos2::ZERO, 1.0);
        let start = handles.rect.center();
        let drag = UiDrag::start(&handles, grip, start, world).unwrap();
        drag.update(start + by, 1.0, world, &layout.elements, viewport())
    }

    #[test]
    fn dragging_snaps_to_another_elements_edge_and_draws_a_guide() {
        // A is 100,50 200×80, so its right edge is x=300. B is 320,200 40×40.
        // Dragging B left 12px puts its left edge at 308, within 8px of A's right.
        let (world, layout) = screen_with(&[
            ("A", 100.0, 50.0, 200.0, 80.0),
            ("B", 320.0, 200.0, 40.0, 40.0),
        ]);
        let edit = drag_named(&world, &layout, "B", Grip::BODY, egui::vec2(-12.0, 0.0));
        assert_eq!(
            udim2(&edit, "Position"),
            Some(UDim2::new(0.0, 300.0, 0.0, 200.0))
        );
        assert_eq!(
            edit.guides,
            vec![SnapGuide {
                horizontal: false,
                at: 300.0,
                from: 50.0,
                to: 240.0,
            }]
        );

        // 16px away is outside the snap distance, so B follows the pointer.
        let edit = drag_named(&world, &layout, "B", Grip::BODY, egui::vec2(-4.0, 0.0));
        assert_eq!(
            udim2(&edit, "Position"),
            Some(UDim2::new(0.0, 316.0, 0.0, 200.0))
        );
        assert!(edit.guides.is_empty());
    }

    #[test]
    fn dragging_snaps_centers_and_resizing_snaps_the_moving_edge() {
        // A's center is x=200. B's center starts at 340. A drag of -136
        // leaves it at 204, four pixels off, so it snaps the rest of the way.
        let (world, layout) = screen_with(&[
            ("A", 100.0, 50.0, 200.0, 80.0),
            ("B", 320.0, 200.0, 40.0, 40.0),
        ]);
        let edit = drag_named(&world, &layout, "B", Grip::BODY, egui::vec2(-136.0, 0.0));
        assert_eq!(
            udim2(&edit, "Position"),
            Some(UDim2::new(0.0, 180.0, 0.0, 200.0))
        );

        // B's right edge is 290. Pulling it 6px toward A's right edge at 300 snaps.
        let (world, layout) = screen_with(&[
            ("A", 100.0, 50.0, 200.0, 80.0),
            ("B", 250.0, 200.0, 40.0, 40.0),
        ]);
        let edit = drag_named(
            &world,
            &layout,
            "B",
            Grip { x: 1, y: 0 },
            egui::vec2(6.0, 0.0),
        );
        assert_eq!(udim2(&edit, "Size"), Some(UDim2::new(0.0, 50.0, 0.0, 40.0)));
        assert_eq!(udim2(&edit, "Position"), None);
        assert_eq!(edit.guides.len(), 1);
        assert_eq!(edit.guides[0].at, 300.0);
        assert!(!edit.guides[0].horizontal);
    }

    #[test]
    fn a_drag_snaps_to_a_ruler_guide() {
        // A's left edge is 100. A guide at x=130 is in reach after a 26px
        // drag (126), and pulls it the rest of the way.
        let (world, layout) = screen_with(&[("A", 100.0, 50.0, 200.0, 80.0)]);
        let id = layout.elements[0].id;
        let element = *layout.element(id).unwrap();
        let handles = Handles::new(element, &world, &[], egui::Pos2::ZERO, 1.0);
        let start = handles.rect.center();
        let drag = UiDrag::start(&handles, Grip::BODY, start, &world).unwrap();
        let guide = UiRect {
            x: 130.0,
            y: 0.0,
            width: 0.0,
            height: 600.0,
        };
        let pointer = start + egui::vec2(26.0, 0.0);
        let edit =
            drag.update_with_guides(pointer, 1.0, &world, &layout.elements, viewport(), &[guide]);
        assert_eq!(
            udim2(&edit, "Position"),
            Some(UDim2::new(0.0, 130.0, 0.0, 50.0))
        );
        assert_eq!(edit.guides[0].at, 130.0);
        let unguided = drag.update(pointer, 1.0, &world, &layout.elements, viewport());
        assert_eq!(
            udim2(&unguided, "Position"),
            Some(UDim2::new(0.0, 126.0, 0.0, 50.0))
        );
    }

    #[test]
    fn a_drag_snaps_to_the_screen_but_not_to_its_own_children() {
        // Center starts at x=200. +196 puts it at 396, four pixels left of
        // the screen's center.
        let (world, layout) = screen_with(&[("A", 100.0, 50.0, 200.0, 80.0)]);
        let edit = drag_named(&world, &layout, "A", Grip::BODY, egui::vec2(196.0, 0.0));
        assert_eq!(
            udim2(&edit, "Position"),
            Some(UDim2::new(0.0, 300.0, 0.0, 50.0))
        );

        let mut world = World::new();
        let gui = world.create(ClassName::ScreenGui).unwrap();
        world.set_parent(gui, Some(world.workspace())).unwrap();
        let parent = add_frame(&mut world, gui, "Parent", 100.0, 100.0, 200.0, 200.0);
        add_frame(&mut world, parent, "Child", 0.0, 0.0, 20.0, 20.0);
        let layout = axcenity_ui::layout_ui(&world, 800, 600, &[]);
        let child = layout
            .elements
            .iter()
            .find(|element| world.get(element.id).unwrap().name() == "Child")
            .unwrap();
        assert_eq!(
            child.rect.x, 100.0,
            "the child shares the parent's left edge"
        );
        // The child's left edge sits on the parent's. Snapping to it would
        // hold the parent still; five pixels of drag should stick.
        let edit = drag_named(&world, &layout, "Parent", Grip::BODY, egui::vec2(5.0, 0.0));
        assert_eq!(
            udim2(&edit, "Position"),
            Some(UDim2::new(0.0, 105.0, 0.0, 100.0))
        );
    }
}
