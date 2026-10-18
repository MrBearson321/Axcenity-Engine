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
//! Everything here works in egui points, except the element rectangles
//! from `axcenity_ui`, which are in the viewport texture's physical pixels.

use std::ops::Range;

use axcenity_core::{InstanceId, UDim2, Value, World};
use axcenity_ui::{CssDeclaration, Stylesheet, UiElement, UiRect, resized_css_length};
use eframe::egui;

/// The side of a handle's square, in points.
const HANDLE_SIZE: f32 = 8.0;
/// How far from a handle's center the mouse still grabs it, in points.
const GRAB_DISTANCE: f32 = 7.0;

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
    fn cursor(self) -> egui::CursorIcon {
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
}

/// The selected UI element's outline and handles, as on screen now.
pub struct Handles {
    pub element: UiElement,
    /// The element, in points.
    pub rect: egui::Rect,
    pub width: Axis,
    pub height: Axis,
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
        }
    }

    /// The eight handles and where their centers are.
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
        let nearest = self
            .handles()
            .map(|(grip, at)| (grip, at.distance(pointer)))
            .filter(|&(_, distance)| distance <= GRAB_DISTANCE)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(grip, _)| grip);
        nearest.or_else(|| self.rect.contains(pointer).then_some(Grip::BODY))
    }

    /// Whether dragging `grip` would change anything.
    pub fn can_drag(&self, grip: Grip) -> bool {
        if grip == Grip::BODY {
            return !self.element.in_flow;
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
        painter.rect_stroke(
            self.rect,
            0.0,
            egui::Stroke::new(1.5_f32, color),
            egui::StrokeKind::Middle,
        );
        for (grip, at) in self.handles() {
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
        })
    }

    /// What the element becomes with the mouse at `pointer`. Moves and
    /// sizes are in whole pixels.
    pub fn update(&self, pointer: egui::Pos2, pixels_per_point: f32) -> UiDragEdit {
        let delta = (pointer - self.start_pointer) * pixels_per_point;
        let (mut position, mut size) = (self.position, self.size);
        let mut values = Vec::new();
        let mut css = Vec::new();
        if self.grip == Grip::BODY {
            if !self.in_flow {
                position.x.offset += delta.x.round();
                position.y.offset += delta.y.round();
                values.push(("Position", Value::UDim2(position)));
            }
            return UiDragEdit {
                id: self.id,
                resize: false,
                values,
                css,
            };
        }

        let (mut moved, mut resized) = (false, false);
        // `&mut size.x` and `&mut size.y` borrow different fields, so both
        // can be held at once.
        let axes = [
            (
                &self.width,
                self.grip.x,
                self.start.width,
                delta.x.round(),
                &mut size.x,
                &mut position.x,
            ),
            (
                &self.height,
                self.grip.y,
                self.start.height,
                delta.y.round(),
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
        }
    }
}

/// Resizes one axis of length `length` by dragging an edge `delta` pixels.
/// `side` is -1 for the start (left or top) edge and 1 for the end edge.
/// Returns the new length, never below zero, and how far the start edge moved.
fn resize(side: i8, length: f32, delta: f32) -> (f32, f32) {
    if side < 0 {
        let new = (length - delta).max(0.0);
        (new, length - new)
    } else {
        ((length + delta).max(0.0), 0.0)
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

    use super::*;

    /// A ScreenGui holding a Frame at (100, 50) sized 200×80 by its
    /// properties, laid out in an 800×600 viewport, with `css` applied.
    fn frame(css: &str) -> (World, Vec<Stylesheet>, Handles) {
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
        (world, stylesheets, handles)
    }

    fn udim2(edit: &UiDragEdit, name: &str) -> Option<UDim2> {
        edit.values
            .iter()
            .find(|(property, _)| *property == name)
            .and_then(|(_, value)| value.as_udim2())
    }

    #[test]
    fn handles_sit_on_the_corners_and_edges_and_the_body_is_inside() {
        let (_, _, handles) = frame("");
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
        let (world, _, handles) = frame("");
        let start = egui::pos2(200.0, 90.0);
        let drag = UiDrag::start(&handles, Grip::BODY, start, &world).unwrap();
        let edit = drag.update(start + egui::vec2(10.4, -5.0), 1.0);
        assert!(!edit.resize);
        assert_eq!(
            udim2(&edit, "Position"),
            Some(UDim2::new(0.0, 110.0, 0.0, 45.0))
        );

        let start = egui::pos2(100.0, 90.0);
        let drag = UiDrag::start(&handles, Grip { x: -1, y: 0 }, start, &world).unwrap();
        let edit = drag.update(start + egui::vec2(-20.0, 30.0), 1.0);
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
        let (world, _, handles) = frame(css);
        assert!(matches!(handles.width, Axis::Css(_)));
        assert!(matches!(handles.height, Axis::Locked(_)));
        assert!(!handles.can_drag(Grip { x: 0, y: 1 }));
        assert!(handles.can_drag(Grip { x: 1, y: 1 }));

        let start = egui::pos2(300.0, 130.0);
        let drag = UiDrag::start(&handles, Grip { x: 1, y: 1 }, start, &world).unwrap();
        // Two pixels per point: 25 points is 50 pixels.
        let edit = drag.update(start + egui::vec2(25.0, 25.0), 2.0);
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
}
