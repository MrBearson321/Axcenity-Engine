//! The Prefab menu of a placed copy's Explorer row (Phase 49): apply its
//! overrides to the prefab (all, or one property), revert them, or unpack
//! it. The app carries out the [`PrefabAction`] (`app/prefabs.rs`).

use axcenity_core::prefab::{self, Overrides};
use axcenity_core::{InstanceId, World};
use eframe::egui;

use crate::theme::{self, icons};

/// Something done to a placed copy of a prefab.
#[derive(Clone, Debug, PartialEq)]
pub enum PrefabAction {
    /// Write every override of the copy rooted at this Instance into its
    /// prefab, and bring every copy up to date.
    ApplyAll(InstanceId),
    /// Write one overridden property of this Instance into its prefab.
    ApplyProperty { id: InstanceId, property: String },
    /// Make the copy match its prefab again, keeping where its parts are.
    RevertAll(InstanceId),
    /// Set one property of this Instance back to its prefab's.
    RevertProperty { id: InstanceId, property: String },
    /// Unlink the copy from its prefab, keeping what it is now.
    Unpack(InstanceId),
}

/// Every overridden property in a copy, as the Instance holding it and the
/// property's name.
pub fn overridden_properties(
    world: &World,
    root: InstanceId,
    overrides: &Overrides,
) -> Vec<(InstanceId, String)> {
    fn walk(
        world: &World,
        id: InstanceId,
        overrides: &Overrides,
        out: &mut Vec<(InstanceId, String)>,
    ) {
        out.extend(overrides.properties.iter().map(|name| (id, name.clone())));
        for (segment, inner) in &overrides.children {
            if let Some(child) = prefab::descend(world, id, std::slice::from_ref(segment)) {
                walk(world, child, inner, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(world, root, overrides, &mut out);
    out
}

/// The file name of a prefab path, without `.prefab.json`.
pub fn prefab_name(path: &str) -> &str {
    let file = path.rsplit('/').next().unwrap_or(path);
    file.strip_suffix(".prefab.json").unwrap_or(file)
}

/// How many children were added and removed, for the menu.
fn changed_children(overrides: &Overrides) -> (usize, usize) {
    overrides
        .children
        .values()
        .map(changed_children)
        .fold((overrides.added.len(), overrides.removed.len()), |a, b| {
            (a.0 + b.0, a.1 + b.1)
        })
}

/// The Prefab ▸ submenu for `id`'s row, when `id` is the root of a placed
/// copy (not one inside another copy).
pub fn row_menu(ui: &mut egui::Ui, world: &World, id: InstanceId) -> Option<PrefabAction> {
    let place = prefab::place(world, id).filter(|place| place.root == id)?;
    let path = world.get(id)?.prefab()?.path.clone();
    let mut chosen = None;
    let title = format!("Prefab · {}", prefab_name(&path));
    ui.menu_button(theme::with_icon(icons::PACKAGE, &title), |ui| {
        let count = place.overrides.count();
        let apply = ui
            .add_enabled(
                count > 0,
                egui::Button::new(theme::with_icon(
                    icons::UPLOAD_SIMPLE,
                    &format!("Apply overrides to prefab ({count})"),
                )),
            )
            .on_hover_text(format!(
                "Write this copy's changes into {path}, as one undo step. Every other copy \
                 takes them, except where it has its own."
            ))
            .on_disabled_hover_text("This copy is the same as its prefab.");
        if apply.clicked() {
            chosen = Some(PrefabAction::ApplyAll(id));
        }
        let properties = overridden_properties(world, id, &place.overrides);
        if !properties.is_empty() {
            ui.menu_button(theme::with_icon(icons::UPLOAD_SIMPLE, "Apply one"), |ui| {
                for (holder, property) in &properties {
                    let label = match world.get(*holder) {
                        Some(instance) if *holder != id => {
                            format!("{} · {property}", instance.name())
                        }
                        _ => property.clone(),
                    };
                    if ui.button(label).clicked() {
                        chosen = Some(PrefabAction::ApplyProperty {
                            id: *holder,
                            property: property.clone(),
                        });
                    }
                }
            });
        }
        let (added, removed) = changed_children(&place.overrides);
        if added + removed > 0 {
            ui.label(
                egui::RichText::new(format!("{added} added, {removed} removed"))
                    .size(theme::type_scale::SMALL)
                    .color(theme::palette(ui.ctx()).text_weak),
            );
        }
        ui.separator();
        if ui
            .add_enabled(
                count > 0,
                egui::Button::new(theme::with_icon(
                    icons::ARROW_COUNTER_CLOCKWISE,
                    "Revert all",
                )),
            )
            .on_hover_text(
                "Make this copy the same as its prefab again, keeping where its parts are.",
            )
            .clicked()
        {
            chosen = Some(PrefabAction::RevertAll(id));
        }
        if ui
            .button(theme::with_icon(icons::LINK_BREAK, "Unpack"))
            .on_hover_text(
                "Unlink it from the prefab. It stays as it is, and no longer follows the \
                 prefab's changes.",
            )
            .clicked()
        {
            chosen = Some(PrefabAction::Unpack(id));
        }
    });
    chosen
}
