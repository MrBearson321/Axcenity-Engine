//! How a click changes a selection, the same in the Explorer, the viewport
//! and the UI Designer.

use axcenity_core::InstanceId;
use eframe::egui;

/// Changes `selection` for a click. The last id is the primary one (Properties
/// and handles). Ctrl or Cmd toggles. Shift, with `order` (the visible rows),
/// selects the range from the primary to the clicked row; without `order` it
/// adds. A plain click replaces the selection. `None` clears it, unless Ctrl
/// or Shift is held. `keep_group` is a right-click on a row already selected:
/// the others stay, and that row becomes primary.
pub fn update_selection(
    selection: &mut Vec<InstanceId>,
    clicked: Option<InstanceId>,
    modifiers: egui::Modifiers,
    order: Option<&[InstanceId]>,
    keep_group: bool,
) {
    let toggle = modifiers.command || modifiers.ctrl || modifiers.mac_cmd;
    match clicked {
        None if toggle || modifiers.shift => {}
        None => selection.clear(),
        Some(id) if keep_group && selection.contains(&id) => {
            selection.retain(|other| *other != id);
            selection.push(id);
        }
        Some(id) if toggle => {
            if let Some(index) = selection.iter().position(|other| *other == id) {
                selection.remove(index);
            } else {
                selection.push(id);
            }
        }
        Some(id) if modifiers.shift => {
            if let Some(order) = order
                && let Some(anchor) = selection.last().copied().or(Some(id))
                && let (Some(from), Some(to)) = (
                    order.iter().position(|row| *row == anchor),
                    order.iter().position(|row| *row == id),
                )
            {
                let (lo, hi) = (from.min(to), from.max(to));
                *selection = order[lo..=hi].to_vec();
            } else if !selection.contains(&id) {
                selection.push(id);
            }
            if selection.last() != Some(&id) && selection.contains(&id) {
                selection.retain(|other| *other != id);
                selection.push(id);
            }
        }
        Some(id) => *selection = vec![id],
    }
}
