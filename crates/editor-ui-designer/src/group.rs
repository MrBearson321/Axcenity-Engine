//! Handles around several selected elements at once: one box around them
//! all, with eight squares. Dragging inside it moves every element, and
//! dragging a square resizes the box from that edge or corner, each
//! element keeping its place and share of it.
//!
//! Moving snaps the box's edges and middle to other elements, the screen
//! and ruler guides, as one element's drag does; resizing doesn't snap.
//! Each element's `Position` and `Size` offsets change in design pixels, or
//! the CSS `width` or `height` that sets its size (`ui_handles.rs`). A flex
//! child is resized but not moved, as the layout places it.

use axcenity_core::{InstanceId, UDim2, Value, World};
use axcenity_ui::{Stylesheet, UiElement, UiLayout, UiRect, resized_css_length};
use eframe::egui;

use crate::ui_handles::{
    self, Axis, CssEdit, GRAB_DISTANCE, Grip, HANDLE_SIZE, SNAP_DISTANCE, SnapGuide, UiDragEdit,
};

/// The smallest the box can be resized to, in pixels.
const MIN_SIZE: f32 = 1.0;

/// The box around the selected elements, as on screen now.
#[derive(Clone, Copy, Debug)]
pub struct GroupBox {
    /// Around every element, in points.
    pub rect: egui::Rect,
}

/// The rectangle around `rects`.
fn union(rects: impl IntoIterator<Item = UiRect>) -> Option<UiRect> {
    rects.into_iter().reduce(|a, b| {
        let (x, y) = (a.x.min(b.x), a.y.min(b.y));
        UiRect {
            x,
            y,
            width: (a.x + a.width).max(b.x + b.width) - x,
            height: (a.y + a.height).max(b.y + b.height) - y,
        }
    })
}

/// The rectangle around `ids` in `layout`, in its pixels.
pub fn bounds(layout: &UiLayout, ids: &[InstanceId]) -> Option<UiRect> {
    union(
        ids.iter()
            .filter_map(|&id| layout.element(id).map(|element| element.rect)),
    )
}

impl GroupBox {
    /// The box around `ids` in `layout`, whose pixel 0, 0 is at `origin`
    /// (points), with `pixels_per_point`.
    pub fn around(
        layout: &UiLayout,
        ids: &[InstanceId],
        origin: egui::Pos2,
        pixels_per_point: f32,
    ) -> Option<GroupBox> {
        let rect = bounds(layout, ids)?;
        Some(GroupBox {
            rect: egui::Rect::from_min_size(
                origin + egui::vec2(rect.x, rect.y) / pixels_per_point,
                egui::vec2(rect.width, rect.height) / pixels_per_point,
            ),
        })
    }

    /// The eight handles and where their centers are.
    fn handles(&self) -> impl Iterator<Item = (Grip, egui::Pos2)> + '_ {
        let r = self.rect;
        let at = |side: i8, start: f32, middle: f32, end: f32| match side {
            -1 => start,
            0 => middle,
            _ => end,
        };
        [-1, 0, 1]
            .into_iter()
            .flat_map(|y| [-1, 0, 1].map(|x| Grip { x, y }))
            .filter(|&grip| grip != Grip::BODY)
            .map(move |grip| {
                let point = egui::pos2(
                    at(grip.x, r.left(), r.center().x, r.right()),
                    at(grip.y, r.top(), r.center().y, r.bottom()),
                );
                (grip, point)
            })
    }

    /// The grip under `pointer`: the nearest handle in reach, or the body
    /// inside the box.
    pub fn grip_at(&self, pointer: egui::Pos2) -> Option<Grip> {
        self.handles()
            .map(|(grip, at)| (grip, at.distance(pointer)))
            .filter(|&(_, distance)| distance <= GRAB_DISTANCE)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(grip, _)| grip)
            .or_else(|| self.rect.contains(pointer).then_some(Grip::BODY))
    }

    /// Draws the box, dashed so it reads apart from one element's outline,
    /// and its handles, `highlighted` one brighter.
    pub fn paint(&self, painter: &egui::Painter, color: egui::Color32, highlighted: Option<Grip>) {
        let r = self.rect;
        let corners = [
            r.left_top(),
            r.right_top(),
            r.right_bottom(),
            r.left_bottom(),
            r.left_top(),
        ];
        painter.extend(egui::Shape::dashed_line(
            &corners,
            egui::Stroke::new(1.5_f32, color),
            6.0,
            4.0,
        ));
        for (grip, at) in self.handles() {
            let square = egui::Rect::from_center_size(at, egui::vec2(HANDLE_SIZE, HANDLE_SIZE));
            let fill = if highlighted == Some(grip) {
                egui::Color32::WHITE
            } else {
                color
            };
            painter.rect_filled(square, 1.0, fill);
        }
    }
}

/// One element of a group being dragged, as it was when the drag started.
#[derive(Clone, Debug)]
struct Member {
    id: InstanceId,
    start: UiRect,
    position: UDim2,
    size: UDim2,
    in_flow: bool,
    width: Axis,
    height: Axis,
    pixels_per_unit: f32,
}

/// Several elements being moved or resized together by their box.
#[derive(Clone, Debug)]
pub struct GroupDrag {
    pub grip: Grip,
    /// Where the mouse went down, in points.
    start_pointer: egui::Pos2,
    /// The box when the drag started, in pixels.
    start: UiRect,
    members: Vec<Member>,
}

impl GroupDrag {
    /// Starts dragging `grip` of the box around `ids`, laid out in `layout`.
    pub fn start(
        world: &World,
        stylesheets: &[Stylesheet],
        layout: &UiLayout,
        ids: &[InstanceId],
        grip: Grip,
        pointer: egui::Pos2,
    ) -> Option<GroupDrag> {
        let members: Vec<Member> = ids
            .iter()
            .filter_map(|&id| {
                let element: &UiElement = layout.element(id)?;
                let instance = world.get(id)?;
                let udim2 = |name| instance.property(name).and_then(Value::as_udim2);
                Some(Member {
                    id,
                    start: element.rect,
                    position: udim2("Position")?,
                    size: udim2("Size")?,
                    in_flow: element.in_flow,
                    width: Axis::of(world, id, stylesheets, "width"),
                    height: Axis::of(world, id, stylesheets, "height"),
                    pixels_per_unit: element.pixels_per_unit.max(1e-4),
                })
            })
            .collect();
        if members.len() < 2 {
            return None;
        }
        Some(GroupDrag {
            grip,
            start_pointer: pointer,
            start: union(members.iter().map(|member| member.start))?,
            members,
        })
    }

    /// What every element becomes with the mouse at `pointer`, and the
    /// lines a move snapped to. `elements`, `viewport` and `guides` are
    /// what a move can snap to, in the layout's pixels (see
    /// [`ui_handles::UiDrag::update_with_guides`]).
    pub fn update(
        &self,
        pointer: egui::Pos2,
        pixels_per_point: f32,
        world: &World,
        elements: &[UiElement],
        viewport: UiRect,
        guides: &[UiRect],
    ) -> (Vec<UiDragEdit>, Vec<SnapGuide>) {
        let moved = (pointer - self.start_pointer) * pixels_per_point;
        let mut delta = egui::vec2(moved.x.round(), moved.y.round());
        if self.grip == Grip::BODY {
            let lines = self.snap(
                &mut delta,
                world,
                elements,
                viewport,
                guides,
                pixels_per_point,
            );
            let edits = self
                .members
                .iter()
                .map(|member| {
                    let mut position = member.position;
                    let mut values = Vec::new();
                    if !member.in_flow {
                        position.x.offset += (delta.x / member.pixels_per_unit).round();
                        position.y.offset += (delta.y / member.pixels_per_unit).round();
                        values.push(("Position", Value::UDim2(position)));
                    }
                    UiDragEdit {
                        id: member.id,
                        resize: false,
                        values,
                        css: Vec::new(),
                        guides: Vec::new(),
                    }
                })
                .collect();
            return (edits, lines);
        }

        // The box's new edges; each element keeps its share of it.
        let (x, width) = sized(self.grip.x, self.start.x, self.start.width, delta.x);
        let (y, height) = sized(self.grip.y, self.start.y, self.start.height, delta.y);
        let scale_x = width / self.start.width.max(1e-4);
        let scale_y = height / self.start.height.max(1e-4);
        let edits = self
            .members
            .iter()
            .map(|member| {
                let s = member.start;
                let placed = UiRect {
                    x: x + (s.x - self.start.x) * scale_x,
                    y: y + (s.y - self.start.y) * scale_y,
                    width: s.width * scale_x,
                    height: s.height * scale_y,
                };
                member.edit(placed)
            })
            .collect();
        (edits, Vec::new())
    }

    /// Pulls a move's `delta` onto the nearest lines within the snap
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
        let mut targets: Vec<UiRect> = Vec::new();
        if viewport.width > 0.0 && viewport.height > 0.0 {
            targets.push(viewport);
        }
        targets.extend(
            elements
                .iter()
                .filter(|element| {
                    !self.members.iter().any(|member| {
                        member.id == element.id
                            || ui_handles::descends_from(world, element.id, member.id)
                    })
                })
                .map(|element| element.rect),
        );
        targets.extend_from_slice(guides);
        let threshold = SNAP_DISTANCE * pixels_per_point;
        let mut lines = Vec::new();
        for along_x in [true, false] {
            let moved = if along_x { delta.x } else { delta.y };
            let start = ui_handles::axis_start(&self.start, along_x) + moved;
            let length = ui_handles::axis_length(&self.start, along_x);
            let best = ui_handles::axis_lines(start, length)
                .into_iter()
                .flat_map(|edge| {
                    targets.iter().flat_map(move |target| {
                        ui_handles::axis_lines(
                            ui_handles::axis_start(target, along_x),
                            ui_handles::axis_length(target, along_x),
                        )
                        .map(|line| (line - edge, line))
                    })
                })
                .filter(|(gap, _)| gap.abs() <= threshold)
                .min_by(|a, b| a.0.abs().total_cmp(&b.0.abs()));
            if let Some((gap, at)) = best {
                let adjustment = ui_handles::whole_adjustment(gap);
                if along_x {
                    delta.x += adjustment;
                } else {
                    delta.y += adjustment;
                }
                let mut rect = self.start;
                rect.x += delta.x;
                rect.y += delta.y;
                lines.push(ui_handles::guide_line(rect, along_x, at, &targets));
            }
        }
        lines
    }
}

/// One axis of the box with its `side` edge (-1 start, 1 end, 0 neither)
/// dragged `delta` pixels: its new start and length.
fn sized(side: i8, start: f32, length: f32, delta: f32) -> (f32, f32) {
    match side {
        0 => (start, length),
        _ => {
            let (new, shift) = ui_handles::resize(side, length, delta);
            if new < MIN_SIZE {
                let shift = if side < 0 { length - MIN_SIZE } else { 0.0 };
                (start + shift, MIN_SIZE)
            } else {
                (start + shift, new)
            }
        }
    }
}

impl Member {
    /// What this element becomes placed at `placed` (layout pixels).
    fn edit(&self, placed: UiRect) -> UiDragEdit {
        let per_unit = self.pixels_per_unit;
        let (mut position, mut size) = (self.position, self.size);
        let mut css = Vec::new();
        let mut resized = false;
        let axes = [
            (
                &self.width,
                self.start.width,
                placed.width,
                &mut size.x,
                &mut position.x,
                placed.x - self.start.x,
            ),
            (
                &self.height,
                self.start.height,
                placed.height,
                &mut size.y,
                &mut position.y,
                placed.y - self.start.y,
            ),
        ];
        for (axis, start, length, size, position, shift) in axes {
            let (start, length) = (start / per_unit, length / per_unit);
            match axis {
                Axis::Size => {
                    size.offset += (length - start).round();
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
                Axis::Locked(_) => {}
            }
            if !self.in_flow {
                position.offset += (shift / per_unit).round();
            }
        }
        let mut values = Vec::new();
        if resized {
            values.push(("Size", Value::UDim2(size)));
        }
        if !self.in_flow {
            values.push(("Position", Value::UDim2(position)));
        }
        UiDragEdit {
            id: self.id,
            resize: true,
            values,
            css,
            guides: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use axcenity_core::ClassName;

    use super::*;

    /// A 400×300 ScreenGui with two 100×50 Frames, at 0, 0 and 200, 100.
    fn two() -> (World, InstanceId, [InstanceId; 2], UiLayout) {
        let mut world = World::new();
        let gui = world.create(ClassName::ScreenGui).unwrap();
        world.set_parent(gui, Some(world.workspace())).unwrap();
        world
            .set_property(gui, "DesignWidth", Value::Number(400.0))
            .unwrap();
        world
            .set_property(gui, "DesignHeight", Value::Number(300.0))
            .unwrap();
        let ids = [[0.0, 0.0], [200.0, 100.0]].map(|[x, y]| {
            let id = world.create(ClassName::Frame).unwrap();
            world.set_parent(id, Some(gui)).unwrap();
            world
                .set_property(id, "Position", Value::UDim2(UDim2::new(0.0, x, 0.0, y)))
                .unwrap();
            world
                .set_property(id, "Size", Value::UDim2(UDim2::new(0.0, 100.0, 0.0, 50.0)))
                .unwrap();
            id
        });
        let layout = axcenity_ui::layout_artboard(&world, gui, &[], (400, 300), 1.0, [0.0, 0.0]);
        (world, gui, ids, layout)
    }

    fn value(edits: &[UiDragEdit], id: InstanceId, name: &str) -> Option<UDim2> {
        edits
            .iter()
            .find(|edit| edit.id == id)?
            .values
            .iter()
            .find(|(property, _)| *property == name)
            .and_then(|(_, value)| value.as_udim2())
    }

    #[test]
    fn the_box_goes_around_every_element_and_its_body_moves_them_all() {
        let (world, _, [a, b], layout) = two();
        let group = GroupBox::around(&layout, &[a, b], egui::Pos2::ZERO, 1.0).unwrap();
        assert_eq!(
            group.rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(300.0, 150.0))
        );
        assert_eq!(group.grip_at(egui::pos2(150.0, 75.0)), Some(Grip::BODY));
        assert_eq!(
            group.grip_at(egui::pos2(300.0, 150.0)),
            Some(Grip { x: 1, y: 1 })
        );

        let drag = GroupDrag::start(
            &world,
            &[],
            &layout,
            &[a, b],
            Grip::BODY,
            egui::pos2(150.0, 75.0),
        )
        .unwrap();
        // Far from anything to snap to.
        let (edits, _) = drag.update(
            egui::pos2(173.0, 118.0),
            1.0,
            &world,
            &[],
            UiRect::default(),
            &[],
        );
        assert_eq!(
            value(&edits, a, "Position"),
            Some(UDim2::new(0.0, 23.0, 0.0, 43.0))
        );
        assert_eq!(
            value(&edits, b, "Position"),
            Some(UDim2::new(0.0, 223.0, 0.0, 143.0))
        );
    }

    #[test]
    fn resizing_the_box_scales_each_element_and_where_it_sits_in_it() {
        let (world, _, [a, b], layout) = two();
        // The right edge from 300 to 600: twice as wide.
        let drag = GroupDrag::start(
            &world,
            &[],
            &layout,
            &[a, b],
            Grip { x: 1, y: 0 },
            egui::pos2(300.0, 75.0),
        )
        .unwrap();
        let (edits, _) = drag.update(
            egui::pos2(600.0, 75.0),
            1.0,
            &world,
            &[],
            UiRect::default(),
            &[],
        );
        assert_eq!(
            value(&edits, a, "Position"),
            Some(UDim2::new(0.0, 0.0, 0.0, 0.0))
        );
        assert_eq!(
            value(&edits, a, "Size"),
            Some(UDim2::new(0.0, 200.0, 0.0, 50.0))
        );
        assert_eq!(
            value(&edits, b, "Position"),
            Some(UDim2::new(0.0, 400.0, 0.0, 100.0))
        );
        assert_eq!(
            value(&edits, b, "Size"),
            Some(UDim2::new(0.0, 200.0, 0.0, 50.0))
        );
    }
}
