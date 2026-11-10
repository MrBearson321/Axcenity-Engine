//! The ribbon's dropdowns: one button that opens a short menu of what it
//! inserts, in place of a button for every class. The Explorer's Insert
//! menu has every class too.

use super::*;

/// A ribbon button named `label` that opens a menu of `sections` (a line
/// between each). The one picked is noted in `chosen`. Off during Play.
pub(super) fn insert_menu(
    ui: &mut egui::Ui,
    (icon, label): (&str, &str),
    hover: &str,
    sections: &[&[Insertable]],
    playing: bool,
    chosen: &mut Option<Insertable>,
) {
    let button = ui
        .add_enabled(!playing, RibbonButton::new(icon, label).dropdown())
        .on_hover_text(hover)
        .on_disabled_hover_text("Press Stop to edit.");
    egui::Popup::menu(&button).show(|ui| {
        insert_items(ui, sections, chosen);
    });
}

/// The menu items for `sections`, with a line between each.
pub(super) fn insert_items(
    ui: &mut egui::Ui,
    sections: &[&[Insertable]],
    chosen: &mut Option<Insertable>,
) {
    for (index, section) in sections.iter().enumerate() {
        if index > 0 {
            ui.separator();
        }
        for &what in *section {
            if ui
                .button(explorer::insert_item(ui, what))
                .on_hover_text(insert_hint(what))
                .clicked()
            {
                *chosen = Some(what);
            }
        }
    }
}

pub(super) use crate::explorer::insert_hint;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_insertable_on_the_ribbon_says_what_it_does() {
        let ribbon = [
            &Insertable::PARTS[..],
            &Insertable::UI,
            &Insertable::AUDIO,
            &Insertable::LIGHTS,
            &Insertable::PROBES,
            &Insertable::SKY,
            &Insertable::LENS,
            &Insertable::VOLUMES,
            &Insertable::EFFECTS,
            &Insertable::CHARACTERS,
            &Insertable::AGENTS,
            &Insertable::SPLINES,
            &Insertable::CHECKPOINTS,
            &Insertable::HEALTH,
            &Insertable::SEATS,
            &Insertable::INTERACTION,
            &Insertable::HUMANOIDS,
            &Insertable::SPAWNING,
            &Insertable::TOOLS,
            &Insertable::VEHICLES,
            &Insertable::CAMERAS,
            &Insertable::CONSTRAINTS,
        ];
        for what in ribbon.into_iter().flatten() {
            let hint = insert_hint(*what);
            assert!(hint.len() > 25, "{what:?}: {hint}");
        }
    }
}
