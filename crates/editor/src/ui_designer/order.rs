//! Layer order: Bring to Front, Bring Forward, Send Backward and Send to
//! Back, from the hierarchy's and the artboard's right-click menus.
//!
//! Siblings are drawn lowest `ZIndex` first (CSS `z-index` wins), and equal
//! ones in tree order (`axcenity_ui::z_index_of`). Moving one changes its
//! own `ZIndex` to a whole number that puts it in its new place; only when
//! no number fits between its new neighbours are the ones above it raised
//! too. The tree order isn't touched, so a flex layout doesn't move.

use axcenity_core::{ClassName, InstanceId, World};
use axcenity_ui::Stylesheet;
use eframe::egui;

use super::DesignerAction;

/// Where an element moves among its siblings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Restack {
    /// Over all of them.
    Front,
    /// Over the next one up.
    Forward,
    /// Under the next one down.
    Backward,
    /// Under all of them.
    Back,
}

impl Restack {
    pub const ALL: [Restack; 4] = [
        Restack::Front,
        Restack::Forward,
        Restack::Backward,
        Restack::Back,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Restack::Front => "Bring to Front",
            Restack::Forward => "Bring Forward",
            Restack::Backward => "Send Backward",
            Restack::Back => "Send to Back",
        }
    }
}

/// True for the classes that are drawn in order among their siblings.
fn is_layered(class: ClassName) -> bool {
    matches!(
        class,
        ClassName::Frame
            | ClassName::TextLabel
            | ClassName::TextButton
            | ClassName::ImageLabel
            | ClassName::TextBox
            | ClassName::PageLayout
            | ClassName::Table
            | ClassName::ScrollingFrame
    )
}

/// `id` and its layered siblings in drawing order, lowest first: each with
/// its place in the tree and its z.
fn drawing_order(
    world: &World,
    stylesheets: &[Stylesheet],
    id: InstanceId,
) -> Option<Vec<(InstanceId, usize, f64)>> {
    let parent = world.get(world.get(id)?.parent()?)?;
    let mut siblings: Vec<(InstanceId, usize, f64)> = parent
        .children()
        .iter()
        .copied()
        .enumerate()
        .filter(|&(_, child)| world.get(child).is_some_and(|c| is_layered(c.class())))
        .map(|(index, child)| {
            (
                child,
                index,
                axcenity_ui::z_index_of(world, child, stylesheets),
            )
        })
        .collect();
    siblings.sort_by(|a, b| a.2.total_cmp(&b.2).then(a.1.cmp(&b.1)));
    Some(siblings)
}

/// Whether `a` is drawn before (under) `b`: lower z, or equal z and earlier in the tree.
fn under(a: (usize, f64), b: (usize, f64)) -> bool {
    a.1 < b.1 || (a.1 == b.1 && a.0 < b.0)
}

/// The z each sibling of `id` that has to change gets for `id` to move as
/// `how` says, with `id` first. Empty when it's already there.
pub fn restack(
    world: &World,
    stylesheets: &[Stylesheet],
    id: InstanceId,
    how: Restack,
) -> Vec<(InstanceId, f64)> {
    let Some(mut order) = drawing_order(world, stylesheets, id) else {
        return Vec::new();
    };
    let Some(from) = order.iter().position(|entry| entry.0 == id) else {
        return Vec::new();
    };
    let last = order.len() - 1;
    let to = match how {
        Restack::Front => last,
        Restack::Back => 0,
        Restack::Forward => (from + 1).min(last),
        Restack::Backward => from.saturating_sub(1),
    };
    if to == from {
        return Vec::new();
    }
    let moved = order.remove(from);
    order.insert(to, moved);
    let (_, tree, _) = moved;

    // The lowest whole z that puts it over the one now under it, or under
    // the one over it when it goes to the back.
    let z = match (
        to.checked_sub(1).map(|below| order[below]),
        order.get(to + 1),
    ) {
        (Some((_, below_tree, below_z)), _) => {
            if tree > below_tree {
                below_z
            } else {
                below_z + 1.0
            }
        }
        (None, Some(&(_, above_tree, above_z))) => {
            if tree < above_tree {
                above_z
            } else {
                above_z - 1.0
            }
        }
        (None, None) => return Vec::new(),
    };
    let mut changes = vec![(id, z)];
    // Raise what's over it until each is over the one under it.
    let mut previous = (tree, z);
    for &(other, other_tree, other_z) in &order[to + 1..] {
        if under(previous, (other_tree, other_z)) {
            previous = (other_tree, other_z);
            continue;
        }
        let raised = if other_tree > previous.0 {
            previous.1
        } else {
            previous.1 + 1.0
        };
        changes.push((other, raised));
        previous = (other_tree, raised);
    }
    // What's already there needs no change (but the order did change).
    let current = |id| {
        order
            .iter()
            .find(|entry| entry.0 == id)
            .map(|entry| entry.2)
    };
    changes.retain(|&(id, z)| current(id) != Some(z));
    changes
}

/// The four order buttons for `id`, as a right-click menu's items.
pub fn menu_items(
    ui: &mut egui::Ui,
    world: &World,
    stylesheets: &[Stylesheet],
    id: InstanceId,
    actions: &mut Vec<DesignerAction>,
) {
    for how in Restack::ALL {
        let changes = restack(world, stylesheets, id, how);
        if ui
            .add_enabled(!changes.is_empty(), egui::Button::new(how.label()))
            .clicked()
        {
            actions.push(DesignerAction::Restack(changes));
            ui.close();
        }
    }
}

#[cfg(test)]
mod tests {
    use axcenity_core::Value;

    use super::*;

    /// A ScreenGui holding three Frames, A, B and C, in that tree order.
    fn three() -> (World, [InstanceId; 3]) {
        let mut world = World::new();
        let gui = world.create(ClassName::ScreenGui).unwrap();
        world.set_parent(gui, Some(world.workspace())).unwrap();
        let ids = ["A", "B", "C"].map(|name| {
            let id = world.create(ClassName::Frame).unwrap();
            world.set_name(id, name).unwrap();
            world.set_parent(id, Some(gui)).unwrap();
            id
        });
        (world, ids)
    }

    fn apply(world: &mut World, changes: &[(InstanceId, f64)]) {
        for &(id, z) in changes {
            world.set_property(id, "ZIndex", Value::Number(z)).unwrap();
        }
    }

    fn drawn(world: &World, id: InstanceId) -> Vec<InstanceId> {
        drawing_order(world, &[], id)
            .unwrap()
            .into_iter()
            .map(|entry| entry.0)
            .collect()
    }

    #[test]
    fn each_move_puts_the_element_where_it_says_changing_only_it_when_it_can() {
        let (mut world, [a, b, c]) = three();
        let front = restack(&world, &[], a, Restack::Front);
        assert_eq!(front, vec![(a, 2.0)], "one over the rest");
        apply(&mut world, &front);
        assert_eq!(drawn(&world, a), vec![b, c, a]);

        let back = restack(&world, &[], a, Restack::Back);
        assert_eq!(
            back,
            vec![(a, 1.0)],
            "back under B, which is later in the tree"
        );
        apply(&mut world, &back);
        assert_eq!(drawn(&world, a), vec![a, b, c]);

        let changes = restack(&world, &[], a, Restack::Forward);
        apply(&mut world, &changes);
        assert_eq!(drawn(&world, a), vec![b, a, c]);
        let changes = restack(&world, &[], c, Restack::Backward);
        apply(&mut world, &changes);
        assert_eq!(drawn(&world, a), vec![b, c, a]);
        assert!(
            restack(&world, &[], a, Restack::Front).is_empty(),
            "already on top"
        );
        assert!(
            restack(&world, &[], b, Restack::Backward).is_empty(),
            "already at the back"
        );
    }

    #[test]
    fn when_no_number_fits_the_ones_above_are_raised_in_order() {
        let (mut world, [a, b, c]) = three();
        // C under B, both at 1: B and C are tied but for tree order.
        let changes = restack(&world, &[], c, Restack::Backward);
        apply(&mut world, &changes);
        assert_eq!(drawn(&world, a), vec![a, c, b]);
        // A forward goes between C and B.
        let changes = restack(&world, &[], a, Restack::Forward);
        apply(&mut world, &changes);
        assert_eq!(drawn(&world, a), vec![c, a, b]);
    }

    #[test]
    fn css_z_index_decides_the_order_it_starts_from() {
        let (world, [a, b, c]) = three();
        let (sheet, _) = axcenity_ui::parse_stylesheet("ui.css", "Frame { z-index: 5 }");
        let mut world = world;
        world
            .set_property(a, "Id", Value::String("a".into()))
            .unwrap();
        let (low, _) = axcenity_ui::parse_stylesheet("low.css", "#a { z-index: 0 }");
        let sheets = [sheet, low];
        assert_eq!(
            drawing_order(&world, &sheets, a)
                .unwrap()
                .into_iter()
                .map(|e| e.0)
                .collect::<Vec<_>>(),
            vec![a, b, c]
        );
        assert_eq!(restack(&world, &sheets, a, Restack::Front), vec![(a, 6.0)]);
    }
}
