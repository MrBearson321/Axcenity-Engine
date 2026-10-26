//! The anchor's diamonds on the artboard, and dragging them, like Unity's
//! anchor handles. There's a diamond for each corner of the anchor,
//! pointing at it, so a joined anchor shows four round one point. Grab the
//! middle to move them together, or one diamond to pull it away from its
//! pair, which stretches the element on that axis. Each drag rewrites
//! `Position` and `Size` with the element staying where it is
//! (`anchors.rs`).

use axcenity_core::{InstanceId, UDim, UDim2, Value, World};
use eframe::egui;

use super::anchors::{self, Ends, Placement};
use crate::theme;
use crate::ui_handles::SnapGuide;

/// How far a diamond reaches from the point it marks, in points.
const REACH: f32 = 9.0;
/// How close to the point itself a press grabs every diamond there.
const MIDDLE: f32 = 3.5;
/// How close a dragged anchor snaps to the parent's edges, its middle,
/// or its pair, in points.
const SNAP: f32 = 6.0;

/// What a press on the diamonds grabbed: the ends it moves across and down.
pub(super) type Grab = [Ends; 2];

/// The selected element's anchor, as drawn on the artboard.
#[derive(Clone, Copy, Debug)]
pub(super) struct AnchorHandles {
    pub id: InstanceId,
    /// The parent's box, in points.
    pub parent: egui::Rect,
    /// The anchor across and down (`anchors::anchor_of`).
    anchor: [[f32; 2]; 2],
}

impl AnchorHandles {
    pub fn new(world: &World, id: InstanceId, parent: egui::Rect) -> Option<Self> {
        let instance = world.get(id)?;
        let udim2 = |name| instance.property(name).and_then(Value::as_udim2);
        let (position, size) = (udim2("Position")?, udim2("Size")?);
        Some(Self {
            id,
            parent,
            anchor: [
                anchors::anchor_of(position.x, size.x),
                anchors::anchor_of(position.y, size.y),
            ],
        })
    }

    /// The point at fractions `at` of the parent, in points.
    fn point(&self, at: [f32; 2]) -> egui::Pos2 {
        self.parent.min + egui::vec2(at[0], at[1]) * self.parent.size()
    }

    /// The four diamonds: the ends each moves, the point it marks, and
    /// which way it points from there (-1 or 1 across and down).
    fn diamonds(&self) -> [(Grab, egui::Pos2, egui::Vec2); 4] {
        let corner = |x: usize, y: usize| {
            let ends = |end| if end == 0 { Ends::Min } else { Ends::Max };
            let way = |end| if end == 0 { -1.0 } else { 1.0 };
            (
                [ends(x), ends(y)],
                self.point([self.anchor[0][x], self.anchor[1][y]]),
                egui::vec2(way(x), way(y)),
            )
        };
        [corner(0, 0), corner(1, 0), corner(0, 1), corner(1, 1)]
    }

    /// What a press at `pointer` grabs. On the point itself it's every
    /// diamond there (both ends of a joined axis); on a diamond, its ends.
    pub fn grab_at(&self, pointer: egui::Pos2) -> Option<Grab> {
        let joined = self.anchor.map(|[min, max]| min == max);
        if let Some((grab, _, _)) = self
            .diamonds()
            .into_iter()
            .find(|(_, point, _)| point.distance(pointer) <= MIDDLE)
        {
            return Some([0, 1].map(|axis| if joined[axis] { Ends::Both } else { grab[axis] }));
        }
        let room = -1.0..=REACH + 1.0;
        self.diamonds()
            .into_iter()
            .filter_map(|(grab, point, way)| {
                let along = (pointer - point) * way;
                (room.contains(&along.x) && room.contains(&along.y))
                    .then(|| (grab, (along - egui::Vec2::splat(REACH * 0.5)).length()))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(grab, _)| grab)
    }

    /// The diamonds, outlined in white where `lit` grabs them.
    pub fn paint(&self, painter: &egui::Painter, color: egui::Color32, lit: Option<Grab>) {
        for (grab, point, way) in self.diamonds() {
            let mut shape = vec![
                point,
                point + way * egui::vec2(REACH, REACH * 0.4),
                point + way * REACH,
                point + way * egui::vec2(REACH * 0.4, REACH),
            ];
            // Clockwise on screen, whichever way it points.
            if way.x * way.y < 0.0 {
                shape.reverse();
            }
            let moved = lit.is_some_and(|lit| {
                (0..2).all(|axis| lit[axis] == Ends::Both || lit[axis] == grab[axis])
            });
            let outline = if moved {
                egui::Stroke::new(1.5_f32, egui::Color32::WHITE)
            } else {
                egui::Stroke::new(1.0_f32, egui::Color32::from_black_alpha(160))
            };
            painter.add(egui::Shape::convex_polygon(shape, color, outline));
        }
    }
}

/// A drag of the anchor's diamonds under way.
pub(super) struct AnchorDrag {
    pub id: InstanceId,
    pub grab: Grab,
    /// The anchor when it was grabbed.
    from: [[f32; 2]; 2],
    /// From the pointer to the point grabbed, in points.
    offset: egui::Vec2,
    /// Where the element is, which the drag keeps it.
    placement: Placement,
    /// The lines the anchor snapped to, in points.
    pub snaps: Vec<SnapGuide>,
    /// Where the grabbed point is, as fractions of the parent.
    pub at: [f32; 2],
}

impl AnchorDrag {
    pub fn start(
        handles: &AnchorHandles,
        grab: Grab,
        pointer: egui::Pos2,
        placement: Placement,
    ) -> Self {
        let at = [0, 1].map(|axis| {
            let [min, max] = handles.anchor[axis];
            if grab[axis] == Ends::Max { max } else { min }
        });
        Self {
            id: handles.id,
            grab,
            from: handles.anchor,
            offset: handles.point(at) - pointer,
            placement,
            snaps: Vec::new(),
            at,
        }
    }

    /// The element's `Position` and `Size` with the grabbed point under
    /// `pointer`, in a parent at `parent` (in points). It snaps to the
    /// parent's edges and middle, and to its pair, unless `snap` is off.
    pub fn update(
        &mut self,
        pointer: egui::Pos2,
        parent: egui::Rect,
        snap: bool,
    ) -> (UDim2, UDim2) {
        let target = pointer + self.offset;
        self.snaps.clear();
        let mut udims = [(UDim::default(), UDim::default()); 2];
        for axis in 0..2 {
            let length = parent.size()[axis].max(1e-3);
            let mut targets = vec![0.0, 0.5, 1.0];
            match self.grab[axis] {
                Ends::Min => targets.push(self.from[axis][1]),
                Ends::Max => targets.push(self.from[axis][0]),
                Ends::Both => {}
            }
            let within = if snap { SNAP / length } else { 0.0 };
            let value = (target[axis] - parent.min[axis]) / length;
            let (value, snapped) = anchors::snap_anchor(value, &targets, within);
            let anchor = anchors::move_ends(self.from[axis], self.grab[axis], value);
            self.at[axis] = value.clamp(0.0, 1.0);
            if let Some(snapped) = snapped {
                let other = 1 - axis;
                self.snaps.push(SnapGuide {
                    horizontal: axis == 1,
                    at: parent.min[axis] + snapped * length,
                    from: parent.min[other],
                    to: parent.max[other],
                });
            }
            udims[axis] = anchors::anchor_axis(
                anchor,
                self.placement.start[axis],
                self.placement.size[axis],
                self.placement.parent[axis],
            );
        }
        let [(px, sx), (py, sy)] = udims;
        (UDim2 { x: px, y: py }, UDim2 { x: sx, y: sy })
    }

    /// The anchor in percent of the parent, beside the point being dragged.
    pub fn paint_label(&self, ui: &egui::Ui, painter: &egui::Painter, parent: egui::Rect) {
        let palette = theme::palette(ui.ctx());
        let point = parent.min + egui::vec2(self.at[0], self.at[1]) * parent.size();
        let text = format!("{:.0}%, {:.0}%", self.at[0] * 100.0, self.at[1] * 100.0);
        let galley = painter.layout_no_wrap(
            text,
            egui::FontId::proportional(theme::type_scale::SMALL),
            palette.on_accent,
        );
        // Beside the point, on the side away from the diamonds dragged.
        let mut corner = point + egui::vec2(REACH + 6.0, REACH + 6.0);
        if self.grab[0] == Ends::Min {
            corner.x = point.x - REACH - 6.0 - galley.size().x;
        }
        if self.grab[1] == Ends::Min {
            corner.y = point.y - REACH - 6.0 - galley.size().y;
        }
        let rect = egui::Rect::from_min_size(corner, galley.size());
        painter.rect_filled(rect.expand(3.0), 3.0, palette.accent_solid);
        painter.galley(corner, galley, palette.on_accent);
    }
}
