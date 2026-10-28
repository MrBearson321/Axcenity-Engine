//! The Constraint tool (Phase 36): pick a joint in the ribbon, click a
//! surface on one part, then on another. The two Attachments and the joint
//! are made as one undo step (`constraints::new_joint`).

use super::*;
use crate::viewport::SurfacePick;

/// The Constraint tool while it's on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ConstraintTool {
    /// The joint it makes.
    pub class: ClassName,
    /// The first surface clicked, waiting for the second.
    pub first: Option<SurfacePick>,
}

impl EditorApp {
    /// Turns the Constraint tool on for `class`, or off with `None`.
    pub(super) fn pick_constraint_tool(&mut self, class: Option<ClassName>) {
        self.constraint_tool = class.map(|class| ConstraintTool { class, first: None });
        if let Some(class) = class {
            self.output.info(format!(
                "{class}: click a surface on one part, then a surface on another. Esc stops."
            ));
        }
    }

    /// A surface clicked with the Constraint tool: the first is remembered,
    /// and the second makes the joint and selects it. The tool stays on for
    /// the next one.
    pub(super) fn constraint_surface(&mut self, pick: SurfacePick) {
        if self.playing() {
            return;
        }
        let Some(tool) = self.constraint_tool.as_mut() else {
            return;
        };
        let Some(first) = tool.first else {
            tool.first = Some(pick);
            return;
        };
        if first.part == pick.part {
            self.output
                .warning("Click a surface on a different part to join it to.");
            return;
        }
        let class = tool.class;
        tool.first = None;
        let ids = crate::constraints::new_joint(&mut self.world, class, first, pick);
        // Each Attachment goes in its part, and the joint in the first part.
        let parents = [first.part, pick.part, first.part];
        let mut moves = Vec::new();
        for (id, parent) in ids.into_iter().zip(parents) {
            let wanted = self.world.get(id).map(|i| i.class().as_str()).unwrap_or("");
            let name = tree::unique_name(&self.world, parent, wanted);
            self.world.set_name(id, name).expect("just made");
            let index = self.world.get(parent).map_or(0, |p| p.children().len());
            let place = Place { parent, index };
            if history::put(&mut self.world, id, Some(place)).is_ok() {
                moves.push(Move {
                    id,
                    before: None,
                    after: Some(place),
                });
            }
        }
        self.history
            .record_tree(&mut self.world, format!("Insert {class}"), moves);
        self.select_from_viewport(Some(ids[2]));
    }
}
