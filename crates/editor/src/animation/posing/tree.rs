//! The bone tree in the drawer: named bones, folding and their rows.

use std::collections::HashSet;

use super::*;

/// The rig's named bones in order, parents first, each with its depth
/// (unnamed bones are left out and don't count).
pub fn bone_rows(rig: &Rig) -> Vec<(usize, usize)> {
    let mut depths: Vec<Option<usize>> = Vec::with_capacity(rig.bones.len());
    let mut rows = Vec::new();
    for (index, bone) in rig.bones.iter().enumerate() {
        // The depth of the nearest named bone above, if any.
        let mut above = bone.parent;
        let mut parent_depth = None;
        while let Some(parent) = above {
            if let Some(depth) = depths[parent] {
                parent_depth = Some(depth);
                break;
            }
            above = rig.bones[parent].parent;
        }
        if bone.name.is_empty() {
            depths.push(None);
            continue;
        }
        let depth = parent_depth.map_or(0, |depth| depth + 1);
        depths.push(Some(depth));
        rows.push((index, depth));
    }
    rows
}

/// How many levels of a bone tree show when it's first seen.
pub(super) const OPEN_LEVELS: usize = 3;
/// The width of a bone row's ▸/▾, so leaves line up with branches.
const FOLD_WIDTH: f32 = 16.0;

/// Which bones are folded in each model's bone tree (by `MeshId`), kept
/// while the editor runs. A tree first opens with [`OPEN_LEVELS`] levels
/// showing.
#[derive(Default)]
pub struct Folds {
    models: HashMap<String, HashSet<usize>>,
}

/// One shown row of a bone tree.
#[derive(Clone, Debug, PartialEq)]
pub struct TreeRow {
    pub bone: usize,
    pub depth: usize,
    /// For a bone with bones under it, whether they're folded away.
    pub folded: Option<bool>,
    /// It's folded and a bone under it has keys.
    pub hidden_keys: bool,
}

/// The nearest named bone above each bone of `rig`.
pub(super) fn named_parents(rig: &Rig) -> Vec<Option<usize>> {
    rig.bones
        .iter()
        .map(|bone| {
            let mut above = bone.parent;
            while let Some(parent) = above {
                if !rig.bones[parent].name.is_empty() {
                    return Some(parent);
                }
                above = rig.bones[parent].parent;
            }
            None
        })
        .collect()
}

/// The named bones of `rig` that have named bones under them.
fn branches(rig: &Rig) -> HashSet<usize> {
    let parents = named_parents(rig);
    bone_rows(rig)
        .into_iter()
        .filter_map(|(bone, _)| parents[bone])
        .collect()
}

impl Folds {
    /// `model`'s folded bones, starting with everything below
    /// [`OPEN_LEVELS`] folded.
    fn folded(&mut self, model: &str, rig: &Rig) -> &mut HashSet<usize> {
        self.models.entry(model.to_owned()).or_insert_with(|| {
            let branches = branches(rig);
            bone_rows(rig)
                .into_iter()
                .filter(|(bone, depth)| *depth + 1 >= OPEN_LEVELS && branches.contains(bone))
                .map(|(bone, _)| bone)
                .collect()
        })
    }

    /// The rows of `model`'s tree that show, parents first. `keys` is each
    /// bone's key count, for the dot on a folded bone.
    pub fn rows(&mut self, model: &str, rig: &Rig, keys: &[usize]) -> Vec<TreeRow> {
        let folded = self.folded(model, rig).clone();
        let parents = named_parents(rig);
        let rows = bone_rows(rig);
        let branches = branches(rig);
        // Keys at or under each bone, children before parents.
        let mut under = vec![0; rig.bones.len()];
        for &(bone, _) in rows.iter().rev() {
            under[bone] += keys.get(bone).copied().unwrap_or(0);
            if let Some(parent) = parents[bone] {
                under[parent] += under[bone];
            }
        }
        let mut hidden = vec![false; rig.bones.len()];
        let mut shown = Vec::new();
        for (bone, depth) in rows {
            hidden[bone] = parents[bone].is_some_and(|p| hidden[p] || folded.contains(&p));
            if hidden[bone] {
                continue;
            }
            let fold = branches.contains(&bone).then(|| folded.contains(&bone));
            let own = keys.get(bone).copied().unwrap_or(0);
            shown.push(TreeRow {
                bone,
                depth,
                folded: fold,
                hidden_keys: fold == Some(true) && under[bone] > own,
            });
        }
        shown
    }

    /// Folds or unfolds `bone` in `model`'s tree.
    pub fn toggle(&mut self, model: &str, rig: &Rig, bone: usize) {
        let folded = self.folded(model, rig);
        if !folded.remove(&bone) {
            folded.insert(bone);
        }
    }

    /// Folds every bone, leaving only the roots showing.
    pub fn collapse_all(&mut self, model: &str, rig: &Rig) {
        *self.folded(model, rig) = branches(rig);
    }

    pub fn expand_all(&mut self, model: &str, rig: &Rig) {
        self.folded(model, rig).clear();
    }

    /// Collapse all and Expand all, for the drawer's header.
    pub fn buttons(&mut self, ui: &mut egui::Ui, model: &str, rig: &Rig) {
        if ui
            .small_button(icons::ARROWS_IN_LINE_VERTICAL)
            .on_hover_text("Collapse all")
            .clicked()
        {
            self.collapse_all(model, rig);
        }
        if ui
            .small_button(icons::ARROWS_OUT_LINE_VERTICAL)
            .on_hover_text("Expand all")
            .clicked()
        {
            self.expand_all(model, rig);
        }
    }
}

/// A ▸ (folded) or ▾ (open) that flips `folded` when clicked, or the room
/// for one when there's nothing to fold (`None`). Returns true if clicked.
pub fn fold_toggle(ui: &mut egui::Ui, folded: Option<bool>) -> bool {
    let Some(folded) = folded else {
        ui.add_space(FOLD_WIDTH);
        return false;
    };
    let icon = if folded {
        icons::CARET_RIGHT
    } else {
        icons::CARET_DOWN
    };
    ui.add_sized(
        [FOLD_WIDTH, ui.spacing().interact_size.y],
        egui::Button::new(icon).frame(false),
    )
    .on_hover_text(if folded { "Unfold" } else { "Fold" })
    .clicked()
}

/// One bone in the drawer: indented by its depth, with ▸/▾ if it has bones
/// under it, 🔒 if `locked`, `keys` (its key count) if it has any, and a
/// dot when it's folded over keys. Clicking an unlocked bone selects it.
/// Returns the row's response, and whether its ▸/▾ was clicked.
pub fn bone_row(
    ui: &mut egui::Ui,
    name: &str,
    row: &TreeRow,
    selected: bool,
    locked: bool,
    keys: usize,
) -> (egui::Response, bool) {
    ui.horizontal(|ui| {
        ui.add_space((row.depth as f32 * 8.0).min(64.0));
        let toggled = fold_toggle(ui, row.folded);
        let mut text = if locked {
            format!("{} {name}", icons::LOCK)
        } else {
            format!("{} {name}", icons::BONE)
        };
        if keys > 0 {
            text.push_str(&format!("  {} {keys}", icons::KEY));
        }
        if row.hidden_keys {
            text.push_str("  •");
        }
        let button = ui.add_enabled(!locked, egui::Button::selectable(selected, text).truncate());
        let button = if locked {
            button.on_disabled_hover_text(format!(
                "{name} is moved by the take, so it can't be keyed. Convert to keys (⋯) to edit it."
            ))
        } else if row.hidden_keys {
            button.on_hover_text(format!("{name}: a bone folded under it has keys"))
        } else {
            button.on_hover_text(format!("{name}: select it to turn it in the preview"))
        };
        (button, toggled)
    })
    .inner
}
