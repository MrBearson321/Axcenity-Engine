//! Focus arrows: where keyboard and controller focus goes from the selected
//! element. A `Selectable` element has a small arrow just outside the
//! middle of each edge. One whose `NextSelectionUp`, `Down`, `Left` or
//! `Right` names another element by its `Id` is filled, with a line to that
//! element. Drag an arrow onto another selectable element to point it
//! there, or onto nothing to clear it. One drag is one undo step.

use axcenity_core::{InstanceId, Value, World};
use axcenity_ui::UiLayout;
use eframe::egui;

/// Each way focus can go: its property, its name, and which way it points
/// on screen.
pub const DIRECTIONS: [(&str, &str, egui::Vec2); 4] = [
    ("NextSelectionUp", "up", egui::vec2(0.0, -1.0)),
    ("NextSelectionDown", "down", egui::vec2(0.0, 1.0)),
    ("NextSelectionLeft", "left", egui::vec2(-1.0, 0.0)),
    ("NextSelectionRight", "right", egui::vec2(1.0, 0.0)),
];

/// How far outside the edge an arrow sits, and how near the pointer must
/// be to grab it, in points.
const OUT: f32 = 22.0;
const GRAB: f32 = 8.0;
/// How big an arrow is, in points.
const SIZE: f32 = 6.0;

/// The selected element's four arrows.
#[derive(Clone, Copy, Debug)]
pub struct FocusArrows {
    pub id: InstanceId,
    /// The element's box on the artboard, in points.
    rect: egui::Rect,
}

/// An arrow being dragged to another element.
#[derive(Clone, Copy, Debug)]
pub struct FocusDrag {
    pub id: InstanceId,
    /// Which of [`DIRECTIONS`].
    pub index: usize,
}

/// Whether keyboard and controller focus can land on `id`.
fn selectable(world: &World, id: InstanceId) -> bool {
    world
        .get(id)
        .and_then(|instance| instance.property("Selectable"))
        .and_then(Value::as_bool)
        == Some(true)
}

impl FocusArrows {
    /// The arrows of `id`, laid out at `rect` points, when focus can land on it.
    pub fn new(world: &World, id: InstanceId, rect: egui::Rect) -> Option<FocusArrows> {
        selectable(world, id).then_some(FocusArrows { id, rect })
    }

    /// Where arrow `index`'s middle is.
    pub fn at(&self, index: usize) -> egui::Pos2 {
        let way = DIRECTIONS[index].2;
        let half = self.rect.size() / 2.0;
        self.rect.center() + egui::vec2(way.x * (half.x + OUT), way.y * (half.y + OUT))
    }

    /// The arrow under `pointer`, if any.
    pub fn grab_at(&self, pointer: egui::Pos2) -> Option<usize> {
        (0..DIRECTIONS.len()).find(|&index| self.at(index).distance(pointer) <= GRAB)
    }

    /// Draws the arrows, and a line from each set one to the element it
    /// names (`target` finds where that is). `lit` is the one pointed at
    /// or dragged.
    pub fn paint(
        &self,
        painter: &egui::Painter,
        world: &World,
        target: impl Fn(&str) -> Option<egui::Rect>,
        color: egui::Color32,
        lit: Option<usize>,
    ) {
        for (index, (property, _, way)) in DIRECTIONS.iter().enumerate() {
            let named = world
                .get(self.id)
                .and_then(|instance| instance.property(property))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|named| !named.is_empty());
            let at = self.at(index);
            let to = named.and_then(&target);
            if let Some(to) = to {
                link(painter, at, to, color);
            }
            let stroke = egui::Stroke::new(if lit == Some(index) { 2.5_f32 } else { 1.5 }, color);
            let fill = if to.is_some() || lit == Some(index) {
                color
            } else {
                egui::Color32::TRANSPARENT
            };
            painter.add(egui::Shape::convex_polygon(arrow(at, *way), fill, stroke));
        }
    }
}

/// A triangle pointing `way` around `at`.
fn arrow(at: egui::Pos2, way: egui::Vec2) -> Vec<egui::Pos2> {
    let across = egui::vec2(-way.y, way.x);
    vec![
        at + way * SIZE,
        at - way * SIZE * 0.6 + across * SIZE,
        at - way * SIZE * 0.6 - across * SIZE,
    ]
}

/// A line with an arrowhead from `from` to the nearest point of `to`.
pub fn link(painter: &egui::Painter, from: egui::Pos2, to: egui::Rect, color: egui::Color32) {
    let end = if to.contains(from) {
        to.center()
    } else {
        to.clamp(from)
    };
    let along = end - from;
    if along.length() < 1.0 {
        return;
    }
    let stroke = egui::Stroke::new(1.5_f32, color);
    painter.line_segment([from, end], stroke);
    let way = along.normalized();
    let across = egui::vec2(-way.y, way.x);
    let head = vec![
        end,
        end - way * 8.0 + across * 4.0,
        end - way * 8.0 - across * 4.0,
    ];
    painter.add(egui::Shape::convex_polygon(head, color, egui::Stroke::NONE));
}

/// What an arrow dragged from `from` and let go at `pixel` (in the
/// layout's pixels) points at: the topmost element there that focus can
/// land on, or the nearest such element around it, but not `from` itself.
pub fn target_at(
    world: &World,
    layout: &UiLayout,
    [x, y]: [f32; 2],
    from: InstanceId,
) -> Option<InstanceId> {
    let mut current = layout.hit(x, y);
    while let Some(id) = current {
        if id != from && selectable(world, id) {
            return Some(id);
        }
        current = world.get(id)?.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_core::ClassName;

    #[test]
    fn a_selectable_element_has_an_arrow_outside_each_edge_and_others_none() {
        let mut world = World::new();
        let button = world.create(ClassName::TextButton).unwrap();
        let label = world.create(ClassName::TextLabel).unwrap();
        let rect = egui::Rect::from_min_size(egui::pos2(100.0, 100.0), egui::vec2(80.0, 40.0));
        assert!(FocusArrows::new(&world, label, rect).is_none());
        let arrows = FocusArrows::new(&world, button, rect).unwrap();
        assert_eq!(arrows.at(0), egui::pos2(140.0, 100.0 - OUT));
        assert_eq!(arrows.at(1), egui::pos2(140.0, 140.0 + OUT));
        assert_eq!(arrows.at(2), egui::pos2(100.0 - OUT, 120.0));
        assert_eq!(arrows.at(3), egui::pos2(180.0 + OUT, 120.0));
        assert_eq!(arrows.grab_at(egui::pos2(141.0, 163.0)), Some(1));
        assert_eq!(arrows.grab_at(egui::pos2(140.0, 120.0)), None);
    }
}
