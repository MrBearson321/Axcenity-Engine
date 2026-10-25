use super::*;

/// The panel under a script paused at a breakpoint: the line, Continue, and
/// each script function on the paused call stack with its locals, the
/// breakpoint's own open. Returns true when Continue was pressed.
pub(in crate::app) fn paused_panel(ui: &mut egui::Ui, pause: &axcenity_script::Pause) -> bool {
    let palette = theme::palette(ui.ctx());
    let continued = crate::responsive::toolbar(ui, |ui| {
        ui.label(
            egui::RichText::new(theme::with_icon(
                icons::PAUSE,
                &format!("Paused before line {}", pause.line),
            ))
            .color(palette.warning)
            .strong(),
        );
        ui.button(theme::with_icon(filled_icons::PLAY, "Continue"))
            .on_hover_text(format!(
                "Run on from here ({})",
                ui.ctx().format_shortcut(&PLAY_SHORTCUT)
            ))
            .clicked()
    });
    ui.separator();
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            for (index, frame) in pause.frames.iter().enumerate() {
                let file = frame.path.rsplit('/').next().unwrap_or(&frame.path);
                let title = match &frame.function {
                    Some(function) => format!("{function}  ·  {file}:{}", frame.line),
                    None => format!("{file}:{}", frame.line),
                };
                egui::CollapsingHeader::new(title)
                    .id_salt(("paused frame", index))
                    .default_open(index == 0)
                    .icon(widgets::chevron)
                    .show(ui, |ui| {
                        if frame.locals.is_empty() {
                            ui.weak("No local variables here yet.");
                            return;
                        }
                        crate::responsive::form(ui, ("paused locals", index), |ui| {
                            for local in &frame.locals {
                                ui.monospace(&local.name);
                                ui.add(
                                    egui::Label::new(egui::RichText::new(&local.value).monospace())
                                        .truncate(),
                                )
                                .on_hover_text(&local.value);
                                ui.end_row();
                            }
                        });
                    });
            }
        });
    continued
}

/// The green "Playing" badge in the header during Play, with a dot that
/// pulses gently (steady when motion is reduced). Paused at a breakpoint,
/// it's an amber "Paused" with a steady dot.
pub(in crate::app) fn playing_badge(ui: &mut egui::Ui, paused: bool) {
    let palette = theme::palette(ui.ctx());
    let (color, text, hover) = if paused {
        (
            palette.warning,
            "Paused",
            "Paused at a breakpoint. Continue (F5) carries on; Stop (Shift+F5) ends the game.",
        )
    } else {
        (
            palette.success,
            "Playing",
            "Only the Properties panel can change the game, until you press \
             Stop. Those changes are undone at Stop.",
        )
    };
    let pulse = if paused {
        1.0
    } else if ui.ctx().style().animation_time > 0.0 {
        let time = ui.input(|input| input.time);
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(50));
        0.6 + 0.4 * (time * 2.5).sin().abs() as f32
    } else {
        1.0
    };
    egui::Frame::new()
        .fill(color.gamma_multiply(0.14))
        .corner_radius(9)
        .inner_margin(egui::Margin::symmetric(8, 1))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            ui.label(
                egui::RichText::new(filled_icons::CIRCLE)
                    .family(theme::filled())
                    .size(7.0)
                    .color(color.gamma_multiply(pulse)),
            );
            ui.label(
                egui::RichText::new(text)
                    .family(theme::medium_family())
                    .size(theme::type_scale::SMALL)
                    .color(color),
            );
        })
        .response
        .on_hover_text(hover);
}

/// Every object in the Workspace, for the block editor's object menus: its
/// path of names below the Workspace (`Pad.Chime`) and its class.
/// Every object under the Workspace, by path, for the blocks' menus.
/// Animators also get their controller's names (from `names_of`, given the
/// `Controller` path).
pub(super) fn block_objects(
    world: &World,
    mut names_of: impl FnMut(&str) -> Vec<(block_editor::NameKind, String)>,
) -> Vec<block_editor::ObjectInfo> {
    let workspace = world.workspace();
    world
        .descendants(workspace)
        .into_iter()
        .filter_map(|id| {
            let instance = world.get(id)?;
            let controller_names = match instance.class() {
                ClassName::Animator => instance
                    .property("StateMachine")
                    .and_then(Value::as_str)
                    .filter(|path| !path.is_empty())
                    .map(&mut names_of)
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            let mut names = vec![instance.name().to_owned()];
            let mut parent = instance.parent();
            while let Some(above) = parent.filter(|&above| above != workspace) {
                let above = world.get(above)?;
                names.push(above.name().to_owned());
                parent = above.parent();
            }
            names.reverse();
            Some(block_editor::ObjectInfo {
                path: names.join("."),
                class: instance.class(),
                names: controller_names,
            })
        })
        .collect()
}

pub(super) fn overlay_button(
    ui: &mut egui::Ui,
    on: &mut bool,
    icon: &'static str,
    label: &'static str,
    hint: &'static str,
) {
    let button = RibbonButton::new(icon, label).selected(*on);
    if ui.add(button).on_hover_text(hint).clicked() {
        *on = !*on;
    }
}

pub(super) fn overlay_menu_item(
    ui: &mut egui::Ui,
    on: &mut bool,
    icon: &'static str,
    label: &'static str,
    hint: &'static str,
) {
    let item = egui::Button::selectable(*on, theme::with_icon(icon, label));
    if ui.add(item).on_hover_text(hint).clicked() {
        *on = !*on;
    }
}

impl EditorApp {
    /// The keyboard shortcuts for undo, redo, the tools, and renaming,
    /// duplicating and deleting the selection.
    pub(in crate::app) fn shortcuts(&mut self, ctx: &egui::Context) {
        // Stop first: egui counts Shift+F5 as F5 too.
        let (stop, play) = ctx.input_mut(|input| {
            let stop = input.consume_shortcut(&STOP_SHORTCUT);
            (stop, input.consume_shortcut(&PLAY_SHORTCUT))
        });
        if stop {
            self.stop();
        } else if play && self.paused() {
            self.continue_play();
        } else if play {
            self.play();
        }

        // A text box being typed in has its own undo, and uses Delete and
        // Backspace itself, so leave those keys to it then.
        let typing = ctx.wants_keyboard_input();
        let playing = self.playing();
        // Over a property clip, K, Delete and Backspace are the dope sheet's.
        let animating = self.animation.wants_keys(ctx);
        let studio_tool = |input: &mut egui::InputState| {
            Tool::ALL.into_iter().find(|tool| {
                tool.shortcut()
                    .is_some_and(|shortcut| input.consume_shortcut(&shortcut))
            })
        };
        // F3X's single keys only while editing, so a game gets every key.
        let f3x_tool = |input: &mut egui::InputState| {
            if playing || animating {
                return None;
            }
            Tool::ALL.into_iter().find(|tool| {
                tool.key()
                    .is_some_and(|key| input.consume_key(egui::Modifiers::NONE, key))
            })
        };
        let keys = ctx.input_mut(|input| {
            if typing {
                return Keys {
                    tool: studio_tool(input),
                    ..Keys::default()
                };
            }
            // Redo first: egui counts Ctrl+Shift+Z as Ctrl+Z too.
            let redo = input.consume_shortcut(&REDO_SHIFT_SHORTCUT)
                || input.consume_shortcut(&REDO_SHORTCUT);
            let none = egui::Modifiers::NONE;
            Keys {
                redo,
                undo: input.consume_shortcut(&UNDO_SHORTCUT),
                tool: studio_tool(input).or_else(|| f3x_tool(input)),
                toggle: !playing && input.consume_key(none, egui::Key::Enter),
                // Taken before the viewport sees them, so Ctrl+D doesn't
                // also fly the camera right.
                duplicate: input.consume_shortcut(&DUPLICATE_SHORTCUT),
                rename: input.consume_key(none, RENAME_KEY),
                delete: !animating
                    && (input.consume_key(none, egui::Key::Delete)
                        || (cfg!(target_os = "macos")
                            && input.consume_key(none, egui::Key::Backspace))),
            }
        });
        if keys.redo {
            self.redo();
        }
        if keys.undo {
            self.undo();
        }
        if let Some(tool) = keys.tool {
            self.tool = tool;
        }
        if keys.toggle && self.tool == Tool::Angular {
            self.extend_angular();
        } else if keys.toggle && matches!(self.tool, Tool::Anchor | Tool::Collision) {
            let property = if self.tool == Tool::Anchor {
                "Anchored"
            } else {
                "CanCollide"
            };
            let parts = self.tool_parts();
            let all_on = parts.iter().all(|&id| {
                self.world
                    .get(id)
                    .and_then(|i| i.property(property))
                    .and_then(Value::as_bool)
                    != Some(false)
            });
            self.set_on_parts(&parts, property, Value::Bool(!all_on));
            self.history.seal();
        }
        // Undo, duplicate and delete check for Play themselves; renaming
        // only opens a text box, so it's checked here.
        if let Some(id) = self.selected {
            if keys.rename && !self.playing() {
                self.explorer.start_rename(&self.world, id);
            }
            if keys.duplicate {
                self.duplicate_selected();
            }
            if keys.delete {
                self.delete_selected();
            }
        }
    }

    /// File > Version History: the saves kept for this project, and restoring one.
    pub(in crate::app) fn version_window(&mut self, ctx: &egui::Context) {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            self.versions_open = false;
            return;
        };
        let mut open = self.versions_open;
        let mut restore = None;
        let mut checkpoint = false;
        egui::Window::new("Version History")
            .open(&mut open)
            .resizable(true)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.weak(
                    "Each save keeps the world, scripts and styles. Restoring puts that \
                     version back. Assets and the project name stay as they are.",
                );
                ui.add_space(6.0);
                if ui.button("Save a version now").clicked() {
                    checkpoint = true;
                }
                ui.add_space(6.0);
                let versions = axcenity_project::list_versions(&folder);
                if versions.is_empty() {
                    ui.weak("No versions yet. Save the project to keep the first one.");
                }
                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .show(ui, |ui| {
                        for version in &versions {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(version.label.as_str());
                                    ui.weak(version.when());
                                });
                                if ui.button("Restore").clicked() {
                                    restore = Some(version.id.clone());
                                }
                            });
                            ui.separator();
                        }
                    });
            });
        self.versions_open = open;
        if checkpoint {
            if self.unsaved() {
                self.save();
            } else {
                match axcenity_project::record_version(&folder, "Checkpoint") {
                    Ok(true) => self
                        .output
                        .info("Kept a version of the world, scripts and styles."),
                    Ok(false) => self
                        .output
                        .info("The newest version already matches these files."),
                    Err(err) => self
                        .output
                        .warning(format!("Could not keep a version: {err}")),
                }
            }
        }
        if let Some(id) = restore {
            self.restore_version(&id);
        }
    }

    /// Puts version `id` back, keeping the current files as a version first.
    pub(in crate::app) fn restore_version(&mut self, id: &str) {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        if self.unsaved() && !self.save() {
            return;
        }
        if let Err(err) = axcenity_project::record_version(&folder, "Before restore") {
            self.output.warning(format!(
                "Could not keep the current files before restoring: {err}"
            ));
        }
        match axcenity_project::restore_version(&folder, id) {
            Ok(()) => {
                self.open(&folder);
                self.output.info("Restored that version. The files it replaced were kept as a version named \"Before restore\".");
                self.versions_open = true;
            }
            Err(err) => self
                .output
                .error(format!("Could not restore that version: {err}")),
        }
    }
}
