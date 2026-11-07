//! Editing Splines (Phase 73): Insert > Spline, and what the viewport's
//! point editing does to them (`viewport::SplineAction`). Every edit is one
//! undo step: a point's drag, an added point, a deleted one (Delete goes
//! through `delete_selected`, as anything selected does).

use super::*;
use crate::viewport::SplineAction;
use axcenity_render::glam::Vec3;

impl EditorApp {
    /// The Spline whose points are edited in the Scene view: the selected
    /// one, or the one a selected SplinePoint is in. None during Play.
    pub(super) fn spline_editing(&self) -> Option<InstanceId> {
        if self.playing() {
            return None;
        }
        let selected = self.selected?;
        match self.world.get(selected)?.class() {
            ClassName::Spline => Some(selected),
            ClassName::SplinePoint => crate::splines::spline_of(&self.world, selected),
            _ => None,
        }
    }

    /// Does what the mouse did to a Spline in the viewport.
    pub(super) fn spline_action(&mut self, action: SplineAction) {
        if self.playing() {
            return;
        }
        match action {
            SplineAction::Edit(spline) | SplineAction::Pick(spline) => {
                self.select_from_viewport(Some(spline));
            }
            SplineAction::Move { point, to } => self.move_spline_point(point, to),
            SplineAction::Add { after, at } => self.add_spline_point(after, at),
        }
    }

    /// Puts `point` at `to`, merged into one step while the drag lasts.
    pub(super) fn move_spline_point(&mut self, point: InstanceId, to: Vec3) {
        let Some(name) = self.world.get(point).map(|i| i.name().to_owned()) else {
            return;
        };
        self.change(
            point,
            Field::Property("Position"),
            format!("Move {name}"),
            vec![Edit::Property(
                "Position",
                Value::Vector3(Vector3::new(tidy(to.x), tidy(to.y), tidy(to.z))),
            )],
        );
    }

    /// Adds a SplinePoint at `at` just after `after` in its Spline, as one
    /// undo step, and selects it.
    pub(super) fn add_spline_point(&mut self, after: InstanceId, at: Vec3) {
        let Some(spline) = crate::splines::spline_of(&self.world, after) else {
            return;
        };
        let Some(index) = self
            .world
            .get(spline)
            .and_then(|s| s.children().iter().position(|&id| id == after))
        else {
            return;
        };
        let mode = self
            .world
            .get(after)
            .and_then(|i| i.property("TangentMode"))
            .cloned();
        let position = Vector3::new(tidy(at.x), tidy(at.y), tidy(at.z));
        let id = new_point(&mut self.world, spline, position);
        if let Some(mode) = mode {
            let _ = self.world.set_property(id, "TangentMode", mode);
        }
        let place = Place {
            parent: spline,
            index: index + 1,
        };
        if history::put(&mut self.world, id, Some(place)).is_ok() {
            let moves = vec![Move {
                id,
                before: None,
                after: Some(place),
            }];
            self.history
                .record_tree(&mut self.world, "Add Spline Point".to_owned(), moves);
            self.select_from_viewport(Some(id));
        }
    }

    /// Insert > Spline: a Spline in the Workspace with two points either
    /// side of the camera's focus point, as one undo step. It's selected,
    /// so its points can be dragged at once.
    pub(super) fn insert_spline(&mut self) {
        let focus = self.insert_position();
        let workspace = self.world.workspace();
        let spline = self
            .world
            .create(ClassName::Spline)
            .expect("a Spline can be made");
        let name = tree::unique_name(&self.world, workspace, "Spline");
        self.world.set_name(spline, name).expect("just made");
        for dx in [-8.0, 8.0] {
            let point = new_point(
                &mut self.world,
                spline,
                Vector3::new(focus.x + dx, focus.y, focus.z),
            );
            let _ = self.world.set_parent(point, Some(spline));
        }
        let index = self.world.get(workspace).map_or(0, |w| w.children().len());
        let place = Place {
            parent: workspace,
            index,
        };
        if history::put(&mut self.world, spline, Some(place)).is_ok() {
            let moves = vec![Move {
                id: spline,
                before: None,
                after: Some(place),
            }];
            self.history
                .record_tree(&mut self.world, "Insert Spline".to_owned(), moves);
            self.select_from_viewport(Some(spline));
        }
    }
}

/// A new SplinePoint at `position`, named for its place in `spline`, with
/// no parent yet.
fn new_point(world: &mut World, spline: InstanceId, position: Vector3) -> InstanceId {
    let id = world
        .create(ClassName::SplinePoint)
        .expect("a SplinePoint can be made");
    let name = tree::unique_name(world, spline, "Point");
    world.set_name(id, name).expect("just made");
    let _ = world.set_property(id, "Position", Value::Vector3(position));
    id
}

/// Rounded to 3 decimals so the numbers stay tidy; `+ 0.0` turns -0 into 0.
fn tidy(value: f32) -> f32 {
    (value * 1000.0).round() / 1000.0 + 0.0
}
