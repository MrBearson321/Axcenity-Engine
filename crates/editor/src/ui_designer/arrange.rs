//! Arranging the selection: aligning and distributing it, nudging it with
//! the arrow keys, Copy style and Paste style, and the clipboard Ctrl+C
//! and Ctrl+V copy elements through.
//!
//! Each works out new values for the app to apply as one undo step: a
//! new `Position` for each element that moves (its offsets, in design
//! pixels), or the declarations and elements to write.

use axcenity_core::{InstanceId, UDim2, Value, World};
use axcenity_ui::{Stylesheet, UiLayout, UiRect};

use super::group;
use crate::theme::icons;

/// The rules an element has to itself (see [`axcenity_ui::own_rules`]):
/// what follows `#Id` in each selector, and its declarations.
pub type OwnRules = Vec<(String, Vec<(String, String)>)>;

/// Where the toolbar lines the selection up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Centre,
    Right,
    Top,
    Middle,
    Bottom,
}

impl Align {
    pub const ALL: [Align; 6] = [
        Align::Left,
        Align::Centre,
        Align::Right,
        Align::Top,
        Align::Middle,
        Align::Bottom,
    ];

    /// The toolbar button's tooltip, which is also its name.
    pub fn label(self) -> &'static str {
        match self {
            Align::Left => "Align left edges",
            Align::Centre => "Align centres across",
            Align::Right => "Align right edges",
            Align::Top => "Align top edges",
            Align::Middle => "Align middles up and down",
            Align::Bottom => "Align bottom edges",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Align::Left => icons::ALIGN_LEFT,
            Align::Centre => icons::ALIGN_CENTER_HORIZONTAL,
            Align::Right => icons::ALIGN_RIGHT,
            Align::Top => icons::ALIGN_TOP,
            Align::Middle => icons::ALIGN_CENTER_VERTICAL,
            Align::Bottom => icons::ALIGN_BOTTOM,
        }
    }

    /// True for left, centre and right, which move elements across.
    fn across(self) -> bool {
        matches!(self, Align::Left | Align::Centre | Align::Right)
    }

    /// 0 for the start edge, 0.5 for the middle and 1 for the end.
    fn at(self) -> f32 {
        match self {
            Align::Left | Align::Top => 0.0,
            Align::Centre | Align::Middle => 0.5,
            Align::Right | Align::Bottom => 1.0,
        }
    }
}

/// The selected elements that can be placed: laid out, not the ScreenGui,
/// not inside another selected one (it moves with that), and not placed
/// by a flex layout. In the selection's order.
pub fn movable(
    world: &World,
    layout: &UiLayout,
    gui: InstanceId,
    ids: &[InstanceId],
) -> Vec<InstanceId> {
    ids.iter()
        .copied()
        .filter(|&id| {
            id != gui
                && layout.element(id).is_some_and(|element| !element.in_flow)
                && world
                    .get(id)
                    .is_some_and(|instance| instance.property("Position").is_some())
                && !ids
                    .iter()
                    .any(|&other| other != id && other != gui && world.is_descendant_of(id, other))
        })
        .collect()
}

/// `id`'s `Position` moved `by` layout pixels, or `None` when that's less
/// than half a design pixel.
fn moved(
    world: &World,
    layout: &UiLayout,
    id: InstanceId,
    by: [f32; 2],
) -> Option<(InstanceId, UDim2)> {
    let per_unit = layout.element(id)?.pixels_per_unit.max(1e-4);
    let mut position = world
        .get(id)?
        .property("Position")
        .and_then(Value::as_udim2)?;
    let [x, y] = by.map(|pixels| (pixels / per_unit).round());
    if x == 0.0 && y == 0.0 {
        return None;
    }
    position.x.offset += x;
    position.y.offset += y;
    Some((id, position))
}

fn start(rect: &UiRect, across: bool) -> f32 {
    if across { rect.x } else { rect.y }
}

fn length(rect: &UiRect, across: bool) -> f32 {
    if across { rect.width } else { rect.height }
}

/// Lines up the selection's edges or middles. Several elements line up
/// with the box around them all; one lines up with its parent (`area` is
/// the ScreenGui's, for one straight inside it). In `layout`'s pixels.
pub fn align(
    world: &World,
    layout: &UiLayout,
    area: UiRect,
    gui: InstanceId,
    ids: &[InstanceId],
    how: Align,
) -> Vec<(InstanceId, UDim2)> {
    let ids = movable(world, layout, gui, ids);
    let target = match ids.as_slice() {
        [] => return Vec::new(),
        [only] => world
            .get(*only)
            .and_then(|instance| instance.parent())
            .and_then(|parent| layout.element(parent))
            .map_or(area, |parent| parent.rect),
        _ => match group::bounds(layout, &ids) {
            Some(rect) => rect,
            None => return Vec::new(),
        },
    };
    let across = how.across();
    let line = start(&target, across) + length(&target, across) * how.at();
    ids.iter()
        .filter_map(|&id| {
            let rect = layout.element(id)?.rect;
            let gap = line - (start(&rect, across) + length(&rect, across) * how.at());
            let by = if across { [gap, 0.0] } else { [0.0, gap] };
            moved(world, layout, id, by)
        })
        .collect()
}

/// Spaces three or more elements evenly across (or up and down): the
/// first and last stay, and the gaps between neighbours become equal.
pub fn distribute(
    world: &World,
    layout: &UiLayout,
    gui: InstanceId,
    ids: &[InstanceId],
    across: bool,
) -> Vec<(InstanceId, UDim2)> {
    let mut placed: Vec<(InstanceId, UiRect)> = movable(world, layout, gui, ids)
        .into_iter()
        .filter_map(|id| Some((id, layout.element(id)?.rect)))
        .collect();
    if placed.len() < 3 {
        return Vec::new();
    }
    placed.sort_by(|a, b| start(&a.1, across).total_cmp(&start(&b.1, across)));
    let first = placed[0].1;
    let last = placed[placed.len() - 1].1;
    let span = start(&last, across) + length(&last, across) - start(&first, across);
    let filled: f32 = placed.iter().map(|(_, rect)| length(rect, across)).sum();
    let gap = (span - filled) / (placed.len() - 1) as f32;
    let mut next = start(&first, across) + length(&first, across) + gap;
    let mut changes = Vec::new();
    for (id, rect) in &placed[1..placed.len() - 1] {
        let shift = next - start(rect, across);
        let by = if across { [shift, 0.0] } else { [0.0, shift] };
        changes.extend(moved(world, layout, *id, by));
        next += length(rect, across) + gap;
    }
    changes
}

/// The selection moved `by` design pixels, as the arrow keys do.
pub fn nudge(
    world: &World,
    layout: &UiLayout,
    gui: InstanceId,
    ids: &[InstanceId],
    by: [f32; 2],
) -> Vec<(InstanceId, UDim2)> {
    movable(world, layout, gui, ids)
        .into_iter()
        .filter_map(|id| {
            let mut position = world
                .get(id)?
                .property("Position")
                .and_then(Value::as_udim2)?;
            position.x.offset += by[0];
            position.y.offset += by[1];
            Some((id, position))
        })
        .collect()
}

/// `id`'s `Id`, when it has one.
pub fn element_id(world: &World, id: InstanceId) -> Option<&str> {
    world
        .get(id)?
        .property("Id")
        .and_then(Value::as_str)
        .filter(|element_id| !element_id.is_empty())
}

/// What Copy style takes: the declarations in `id`'s own `#Id` rule.
/// `None` when it has no `Id` or that rule says nothing.
pub fn own_style(
    world: &World,
    stylesheets: &[Stylesheet],
    id: InstanceId,
) -> Option<Vec<(String, String)>> {
    let element_id = element_id(world, id)?;
    axcenity_ui::own_rules(stylesheets, element_id)
        .into_iter()
        .find(|(suffix, _)| suffix.is_empty())
        .map(|(_, declarations)| declarations)
        .filter(|declarations| !declarations.is_empty())
}

/// Elements copied with Ctrl+C, to paste into any ScreenGui.
#[derive(Clone, Debug, PartialEq)]
pub struct UiClipboard {
    /// The elements and everything inside them, written as a prefab file
    /// is. Also put on the system clipboard, so a paste can tell it's ours.
    pub prefab: String,
    /// The own rules of every copied element with an `Id`, by that `Id`,
    /// so the pasted copies (which get fresh ones) look the same.
    pub rules: Vec<(String, OwnRules)>,
}

/// Copies `ids` (those not inside another of them) and their own rules.
pub fn copy(world: &World, stylesheets: &[Stylesheet], ids: &[InstanceId]) -> Option<UiClipboard> {
    let roots: Vec<InstanceId> = ids
        .iter()
        .copied()
        .filter(|&id| {
            !ids.iter()
                .any(|&other| other != id && world.is_descendant_of(id, other))
        })
        .collect();
    if roots.is_empty() {
        return None;
    }
    let mut rules = Vec::new();
    for &root in &roots {
        for id in std::iter::once(root).chain(world.descendants(root)) {
            if let Some(element_id) = element_id(world, id)
                && !rules.iter().any(|(known, _)| known == element_id)
            {
                let own = axcenity_ui::own_rules(stylesheets, element_id);
                if !own.is_empty() {
                    rules.push((element_id.to_owned(), own));
                }
            }
        }
    }
    Some(UiClipboard {
        prefab: axcenity_project::write_prefab(world, &roots),
        rules,
    })
}

#[cfg(test)]
mod tests {
    use axcenity_core::ClassName;

    use super::*;

    /// A 400×300 ScreenGui with Frames 20 wide at x 0, 50 and 200, and a
    /// Frame holding a flex child.
    fn row() -> (World, InstanceId, Vec<InstanceId>) {
        let mut world = World::new();
        let gui = world.create(ClassName::ScreenGui).unwrap();
        world.set_parent(gui, Some(world.workspace())).unwrap();
        world
            .set_property(gui, "DesignWidth", Value::Number(400.0))
            .unwrap();
        world
            .set_property(gui, "DesignHeight", Value::Number(300.0))
            .unwrap();
        let ids = [(0.0, 10.0), (50.0, 40.0), (200.0, 20.0)]
            .map(|(x, y)| {
                let id = world.create(ClassName::Frame).unwrap();
                world.set_parent(id, Some(gui)).unwrap();
                world
                    .set_property(id, "Position", Value::UDim2(UDim2::new(0.0, x, 0.0, y)))
                    .unwrap();
                world
                    .set_property(id, "Size", Value::UDim2(UDim2::new(0.0, 20.0, 0.0, 20.0)))
                    .unwrap();
                id
            })
            .to_vec();
        (world, gui, ids)
    }

    fn laid_out(world: &World, gui: InstanceId) -> UiLayout {
        axcenity_ui::layout_artboard(world, gui, &[], (400, 300), 1.0, [0.0, 0.0])
    }

    fn x_of(changes: &[(InstanceId, UDim2)], id: InstanceId) -> Option<f32> {
        changes
            .iter()
            .find(|(changed, _)| *changed == id)
            .map(|(_, position)| position.x.offset)
    }

    #[test]
    fn distributing_three_spaces_them_evenly_and_keeps_the_ends() {
        let (world, gui, ids) = row();
        let layout = laid_out(&world, gui);
        let changes = distribute(&world, &layout, gui, &ids, true);
        // From 0 to 220 with 60 of frames: gaps of 80.
        assert_eq!(changes.len(), 1, "only the middle one moves");
        assert_eq!(x_of(&changes, ids[1]), Some(100.0));
        assert!(
            distribute(&world, &layout, gui, &ids[..2], true).is_empty(),
            "two have nothing between them"
        );
    }

    #[test]
    fn aligning_lines_up_edges_and_one_element_aligns_to_its_parent() {
        let (world, gui, ids) = row();
        let layout = laid_out(&world, gui);
        let area = UiRect {
            x: 0.0,
            y: 0.0,
            width: 400.0,
            height: 300.0,
        };
        let right = align(&world, &layout, area, gui, &ids, Align::Right);
        assert_eq!(x_of(&right, ids[0]), Some(200.0));
        assert_eq!(x_of(&right, ids[1]), Some(200.0));
        assert_eq!(x_of(&right, ids[2]), None, "already there");
        let tops = align(&world, &layout, area, gui, &ids, Align::Top);
        assert!(tops.iter().all(|(_, position)| position.y.offset == 10.0));

        let centred = align(&world, &layout, area, gui, &ids[..1], Align::Centre);
        assert_eq!(x_of(&centred, ids[0]), Some(190.0), "the middle of 400");
    }

    #[test]
    fn nudging_moves_what_can_move_and_not_the_screen_gui() {
        let (world, gui, ids) = row();
        let layout = laid_out(&world, gui);
        let mut selection = vec![gui];
        selection.extend(&ids);
        let changes = nudge(&world, &layout, gui, &selection, [10.0, -1.0]);
        assert_eq!(changes.len(), 3);
        assert_eq!(changes[0].1, UDim2::new(0.0, 10.0, 0.0, 9.0));
    }
}
