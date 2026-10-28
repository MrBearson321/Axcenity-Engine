//! Model ▸ Build ▸ Collision Groups: the project's collision groups
//! (`collision-groups.json`). Groups are added by name and removed with
//! their ×; a matrix of checkboxes says which pairs collide. Changes are
//! written at once, like the Input window, and the next Play uses them.
//! A part joins a group through its `CollisionGroup` property.

use axcenity_core::{CollisionGroups, DEFAULT_GROUP};

use super::*;

/// Whether the Collision Groups window is open, and the name being typed.
#[derive(Default)]
pub(super) struct CollisionGroupsWindow {
    pub open: bool,
    new_group: String,
}

/// One change to the collision groups.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum GroupEdit {
    Add(String),
    Remove(String),
    SetCollidable(String, String, bool),
}

/// `groups` with `edit` made, or what's wrong with it.
pub(super) fn edited(groups: &CollisionGroups, edit: GroupEdit) -> Result<CollisionGroups, String> {
    let mut groups = groups.clone();
    match edit {
        GroupEdit::Add(name) => {
            let name = name.trim();
            if groups.contains(name) {
                return Err(format!(
                    "there's a collision group named \"{name}\" already"
                ));
            }
            groups.register(name)?;
        }
        GroupEdit::Remove(name) => groups.remove(&name)?,
        GroupEdit::SetCollidable(a, b, collidable) => groups.set_collidable(&a, &b, collidable)?,
    }
    Ok(groups)
}

impl EditorApp {
    /// Makes `edit` to the world's collision groups and writes
    /// `collision-groups.json`.
    pub(super) fn edit_collision_groups(&mut self, edit: GroupEdit) {
        let Some(project) = &self.project else {
            return;
        };
        let groups = match edited(self.world.collision_groups(), edit) {
            Ok(groups) => groups,
            Err(err) => {
                self.output.error(format!("Collision groups: {err}."));
                return;
            }
        };
        match axcenity_project::save_collision_groups(&project.folder, &groups) {
            Ok(()) => self.world.set_collision_groups(groups),
            Err(err) => self.output.error(format!(
                "Could not save the collision groups: {}",
                files::chain(&err)
            )),
        }
    }

    /// Model ▸ Build ▸ Collision Groups.
    pub(super) fn collision_groups_window(&mut self, ctx: &egui::Context) {
        if !self.collision_groups.open {
            return;
        }
        if self.project.is_none() {
            self.collision_groups.open = false;
            return;
        }
        let groups = self.world.collision_groups().clone();
        let editing = !self.playing();
        let mut edits = Vec::new();
        let mut open = self.collision_groups.open;
        let palette = theme::palette(ctx);
        egui::Window::new("Collision Groups")
            .open(&mut open)
            .resizable(true)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(
                        "Parts in two groups whose box is unticked pass through each other \
                         and don't touch. A part joins a group with its CollisionGroup \
                         property. Saved in collision-groups.json; scripts can change them \
                         with PhysicsService.",
                    )
                    .color(palette.text_weak),
                );
                if !editing {
                    ui.colored_label(
                        palette.warning,
                        "Press Stop to change the collision groups.",
                    );
                }
                ui.add_space(6.0);
                ui.add_enabled_ui(editing, |ui| {
                    egui::ScrollArea::both().max_height(380.0).show(ui, |ui| {
                        matrix(ui, &groups, &mut edits);
                    });
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        let field = ui.add(
                            egui::TextEdit::singleline(&mut self.collision_groups.new_group)
                                .hint_text("New group")
                                .desired_width(160.0),
                        );
                        let entered = field.lost_focus()
                            && ui.input(|input| input.key_pressed(egui::Key::Enter));
                        if ui.button("Add group").clicked() || entered {
                            edits.push(GroupEdit::Add(std::mem::take(
                                &mut self.collision_groups.new_group,
                            )));
                        }
                    });
                });
            });
        self.collision_groups.open = open;
        for edit in edits {
            self.edit_collision_groups(edit);
        }
    }
}

/// A row per group (its name, a × to remove it unless it's Default, and a
/// box for each group up to itself) under a row of group names.
fn matrix(ui: &mut egui::Ui, groups: &CollisionGroups, edits: &mut Vec<GroupEdit>) {
    let names = groups.names();
    egui::Grid::new("collision groups")
        .num_columns(names.len() + 2)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            ui.label("");
            ui.label("");
            for name in names {
                ui.strong(name);
            }
            ui.end_row();
            for (row, a) in names.iter().enumerate() {
                ui.strong(a);
                if a == DEFAULT_GROUP {
                    ui.label("");
                } else if ui
                    .small_button(icons::X)
                    .on_hover_text(format!(
                        "Remove {a}. Its parts collide as Default parts until they're moved \
                         to another group."
                    ))
                    .clicked()
                {
                    edits.push(GroupEdit::Remove(a.clone()));
                }
                for b in &names[..=row] {
                    let mut collidable = groups.collidable(a, b);
                    if ui
                        .checkbox(&mut collidable, "")
                        .on_hover_text(format!("{a} collides with {b}"))
                        .changed()
                    {
                        edits.push(GroupEdit::SetCollidable(a.clone(), b.clone(), collidable));
                    }
                }
                ui.end_row();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_add_and_remove_groups_and_set_pairs() {
        let groups = CollisionGroups::default();
        let groups = edited(&groups, GroupEdit::Add(" Players ".to_owned())).unwrap();
        let groups = edited(&groups, GroupEdit::Add("NPCs".to_owned())).unwrap();
        assert!(edited(&groups, GroupEdit::Add("NPCs".to_owned())).is_err());
        let groups = edited(
            &groups,
            GroupEdit::SetCollidable("Players".to_owned(), "NPCs".to_owned(), false),
        )
        .unwrap();
        assert!(!groups.collidable("NPCs", "Players"));
        let groups = edited(&groups, GroupEdit::Remove("NPCs".to_owned())).unwrap();
        assert_eq!(groups.names(), ["Default", "Players"]);
        assert!(edited(&groups, GroupEdit::Remove(DEFAULT_GROUP.to_owned())).is_err());
    }

    #[test]
    fn the_window_saves_the_groups_with_the_project_and_play_uses_them() {
        let folder = super::super::tests::copy_of_example("hello-part", "collision-groups");
        let mut app = EditorApp::new(Some(&folder));
        app.edit_collision_groups(GroupEdit::Add("Players".to_owned()));
        app.edit_collision_groups(GroupEdit::Add("NPCs".to_owned()));
        app.edit_collision_groups(GroupEdit::SetCollidable(
            "Players".to_owned(),
            "NPCs".to_owned(),
            false,
        ));
        let file = folder.join(axcenity_project::COLLISION_GROUPS_FILE);
        assert!(file.is_file());

        let reopened = EditorApp::new(Some(&folder));
        assert_eq!(
            reopened.world.collision_groups(),
            app.world.collision_groups()
        );
        assert!(
            !reopened
                .world
                .collision_groups()
                .collidable("NPCs", "Players")
        );

        app.play();
        let session = app.play.as_ref().expect("playing");
        assert!(
            !session
                .world()
                .collision_groups()
                .collidable("NPCs", "Players")
        );
        let _ = std::fs::remove_dir_all(&folder);
    }
}
