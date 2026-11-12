//! Settings > This project > Memory budget (competitive roadmap G7): limits
//! the Profiler's Memory tab compares the playing game with, and warns in
//! Output the first time a Play goes over. Kept in the manifest
//! (`project::memory_budget`); each limit is optional.

use super::*;

use axcenity_project::memory_budget;

impl EditorApp {
    pub(super) fn memory_budget_settings(&mut self, ui: &mut egui::Ui) {
        let Some(project) = &self.project else {
            return;
        };
        let folder = project.folder.clone();
        let palette = theme::palette(ui.ctx());
        let about = ui.label(
            egui::RichText::new(
                "Limits for what the playing game holds. The Profiler's Memory tab shows \
                 the game against them, and Output warns the first time a Play goes over one.",
            )
            .size(type_scale::SMALL)
            .color(palette.text_secondary),
        );
        found(ui, &about, "Memory budget");
        ui.add_space(space::S);
        let mut budget = self.profiler.budget();
        let mut save = false;
        egui::Grid::new("memory budget")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                // Each limit, with what it starts at when it's turned on.
                for (name, slot, start) in [
                    ("Whole game", &mut budget.game_mb, 1024),
                    ("Lua", &mut budget.lua_mb, 256),
                    ("Textures", &mut budget.textures_mb, 512),
                ] {
                    ui.label(name);
                    ui.horizontal(|ui| {
                        let mut limited = slot.is_some();
                        if ui
                            .checkbox(&mut limited, "")
                            .on_hover_text("Off, there's no limit.")
                            .changed()
                        {
                            *slot = limited.then_some(start);
                            save = true;
                        }
                        match slot {
                            Some(mb) => {
                                let response = ui.add(
                                    egui::DragValue::new(mb)
                                        .range(1..=65_536)
                                        .speed(4.0)
                                        .suffix(" MB"),
                                );
                                // Saved once a drag ends, not every frame of it.
                                save |= response.drag_stopped()
                                    || (response.changed() && !response.dragged());
                            }
                            None => {
                                ui.label(
                                    egui::RichText::new("No limit").color(palette.text_secondary),
                                );
                            }
                        }
                    });
                    ui.end_row();
                }
            });
        self.profiler.set_budget(budget);
        if save && let Err(error) = memory_budget::set_memory_budget(&folder, &budget) {
            self.output.error(format!(
                "Couldn't save the project's memory budget: {error}"
            ));
        }
    }
}
