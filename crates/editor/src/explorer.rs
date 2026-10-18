//! The Explorer panel: the Instance tree, from `game` down, with branches
//! that open and close.
//!
//! [`Explorer::rows`] works out which rows are visible without any UI code,
//! so tests can check the tree; [`Explorer::show`] only draws those rows.

use std::collections::HashSet;

use axcenity_core::{ClassName, InstanceId, World};
use eframe::egui;

/// How far each level of the tree is indented, in points.
const INDENT: f32 = 14.0;
/// The size of the open/close arrow's clickable square, in points.
const ARROW_SIZE: f32 = 16.0;

/// Which branches are open. IDs of Instances that no longer exist do no harm:
/// they simply never match a row again.
pub struct Explorer {
    expanded: HashSet<InstanceId>,
}

/// One visible line in the Explorer.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub id: InstanceId,
    pub name: String,
    pub class: ClassName,
    /// How many levels below `game` it is (`game` itself is 0).
    pub depth: usize,
    pub has_children: bool,
    pub expanded: bool,
}

impl Explorer {
    /// Starts with the root (`game` in scripts, named `Game`) and `Workspace` open, so the parts are in view.
    pub fn new(world: &World) -> Self {
        Self {
            expanded: HashSet::from([world.root(), world.workspace()]),
        }
    }

    /// Opens a closed branch, or closes an open one.
    pub fn toggle(&mut self, id: InstanceId) {
        // `insert` returns false when the ID was already there, which means
        // the branch was open: close it instead.
        if !self.expanded.insert(id) {
            self.expanded.remove(&id);
        }
    }

    /// The visible rows, top to bottom: every Instance whose ancestors are
    /// all open, with children in their stored order.
    pub fn rows(&self, world: &World) -> Vec<Row> {
        let mut rows = Vec::new();
        self.add_rows(world, world.root(), 0, &mut rows);
        rows
    }

    /// Adds the row for `id`, then (if it's open) its children's rows.
    fn add_rows(&self, world: &World, id: InstanceId, depth: usize, rows: &mut Vec<Row>) {
        let Some(instance) = world.get(id) else {
            return;
        };
        let expanded = self.expanded.contains(&id);
        rows.push(Row {
            id,
            name: instance.name().to_string(),
            class: instance.class(),
            depth,
            has_children: !instance.children().is_empty(),
            expanded,
        });
        if expanded {
            for &child in instance.children() {
                self.add_rows(world, child, depth + 1, rows);
            }
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, world: &World) {
        ui.strong("Explorer");
        ui.separator();

        // Toggling while looping over the rows would change what's being
        // looped over, so remember the click and apply it afterward.
        let mut clicked = None;
        egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            for row in self.rows(world) {
                ui.horizontal(|ui| {
                    ui.add_space(row.depth as f32 * INDENT);
                    let arrow_sense = if row.has_children {
                        egui::Sense::click()
                    } else {
                        egui::Sense::hover()
                    };
                    let (_, arrow) =
                        ui.allocate_exact_size(egui::Vec2::splat(ARROW_SIZE), arrow_sense);
                    if row.has_children {
                        let openness = if row.expanded { 1.0 } else { 0.0 };
                        egui::collapsing_header::paint_default_icon(ui, openness, &arrow);
                    }

                    let label = ui
                        .add(egui::Label::new(&row.name).sense(egui::Sense::click()))
                        .on_hover_text(row.class.as_str());
                    // Show the class too when the name doesn't already say it.
                    if row.name != row.class.as_str() {
                        ui.weak(row.class.as_str());
                    }
                    if row.has_children && (arrow.clicked() || label.double_clicked()) {
                        clicked = Some(row.id);
                    }
                });
            }
        });
        if let Some(id) = clicked {
            self.toggle(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn demo_world() -> World {
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-touched");
        axcenity_project::load(&folder).unwrap().world
    }

    /// Each visible row as indentation plus name, for easy comparing.
    fn outline(explorer: &Explorer, world: &World) -> Vec<String> {
        explorer
            .rows(world)
            .iter()
            .map(|row| format!("{}{}", "  ".repeat(row.depth), row.name))
            .collect()
    }

    #[test]
    fn explorer_starts_with_game_and_workspace_open() {
        let world = demo_world();
        let explorer = Explorer::new(&world);
        let rows = explorer.rows(&world);

        assert_eq!(rows[0].name, "Game");
        assert_eq!(rows[0].class, ClassName::DataModel);
        assert!(rows[0].expanded);
        assert_eq!(rows[1].name, "Workspace");
        assert_eq!(rows[1].depth, 1);
        assert!(rows[1].expanded);
        // Every part in the Workspace is listed right under it.
        let workspace = world.get(world.workspace()).unwrap();
        for (i, &child) in workspace.children().iter().enumerate() {
            assert_eq!(rows[2 + i].id, child);
            assert_eq!(rows[2 + i].depth, 2);
        }
        // ServerScriptService is closed, so its Script isn't shown.
        let last = rows.last().unwrap();
        assert_eq!(last.name, "ServerScriptService");
        assert!(last.has_children && !last.expanded);
    }

    #[test]
    fn branches_expand_and_collapse() {
        let world = demo_world();
        let mut explorer = Explorer::new(&world);
        let before = outline(&explorer, &world);

        explorer.toggle(world.server_script_service());
        let opened = outline(&explorer, &world);
        assert_eq!(opened.len(), before.len() + 1);
        assert_eq!(opened.last().unwrap(), "    pad");

        explorer.toggle(world.workspace());
        let parts = world.get(world.workspace()).unwrap().children().len();
        assert_eq!(outline(&explorer, &world).len(), opened.len() - parts);

        explorer.toggle(world.workspace());
        explorer.toggle(world.server_script_service());
        assert_eq!(outline(&explorer, &world), before);
    }

    #[test]
    fn closing_a_branch_hides_everything_inside_it_and_reopening_restores_it() {
        // game → Workspace → Car → Wheel, three levels deep. (Model doesn't
        // exist yet, so a Part holds the other Part.)
        let mut world = World::new();
        let car = world.create(ClassName::Part).unwrap();
        world.set_name(car, "Car").unwrap();
        world.set_parent(car, Some(world.workspace())).unwrap();
        let wheel = world.create(ClassName::Part).unwrap();
        world.set_name(wheel, "Wheel").unwrap();
        world.set_parent(wheel, Some(car)).unwrap();
        let mut explorer = Explorer::new(&world);
        explorer.toggle(car);
        let open = [
            "Game",
            "  Workspace",
            "    Car",
            "      Wheel",
            "  ServerScriptService",
        ];
        assert_eq!(outline(&explorer, &world), open);

        explorer.toggle(world.root());
        assert_eq!(outline(&explorer, &world), ["Game"]);

        // The Model inside stays open, so it all comes back as it was.
        explorer.toggle(world.root());
        assert_eq!(outline(&explorer, &world), open);
    }
}
