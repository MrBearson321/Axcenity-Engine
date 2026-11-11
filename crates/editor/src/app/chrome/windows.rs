use super::*;
use axcenity_script::Resume;

use crate::shortcuts::{self, Action};

/// The panel under a paused script (roadmap G6): where and why it paused,
/// Continue and the step buttons, the watches, and each script function on
/// the paused call stack with its locals and the variables it uses from
/// outside, the innermost open. Returns how to carry on, if a button was
/// pressed.
pub(in crate::app) fn paused_panel(
    ui: &mut egui::Ui,
    pause: &axcenity_script::Pause,
    watches: &mut crate::watches::Watches,
) -> Option<Resume> {
    let palette = theme::palette(ui.ctx());
    let failed = pause.error.is_some();
    let resume = crate::responsive::toolbar(ui, |ui| {
        let title = if failed {
            format!("Stopped by an error on line {}", pause.line)
        } else {
            format!("Paused before line {}", pause.line)
        };
        let color = if failed {
            palette.error
        } else {
            palette.warning
        };
        ui.label(
            egui::RichText::new(theme::with_icon(icons::PAUSE, &title))
                .color(color)
                .strong(),
        );
        let mut resume = None;
        let buttons = [
            (
                Resume::Continue,
                filled_icons::PLAY,
                "Continue",
                Action::Play,
                "Run on from here",
            ),
            (
                Resume::StepOver,
                icons::ARROW_ARC_RIGHT,
                "Step Over",
                Action::StepOver,
                "Run this line, then pause at the next one here",
            ),
            (
                Resume::StepInto,
                icons::ARROW_LINE_DOWN,
                "Step Into",
                Action::StepInto,
                "Pause at the next line run, inside a function this line calls too",
            ),
            (
                Resume::StepOut,
                icons::ARROW_LINE_UP,
                "Step Out",
                Action::StepOut,
                "Run to the end of this function, then pause where it was called",
            ),
        ];
        for (how, icon, label, action, hover) in buttons {
            // A failed script can't run on, so it can't step either.
            let enabled = how == Resume::Continue || !failed;
            if ui
                .add_enabled(enabled, egui::Button::new(theme::with_icon(icon, label)))
                .on_hover_text(format!("{hover} ({})", action.menu_text(ui.ctx())))
                .on_disabled_hover_text(
                    "The script that failed can't run on. Continue runs the rest of the game.",
                )
                .clicked()
            {
                resume = Some(how);
            }
        }
        resume
    });
    if let Some(error) = &pause.error {
        ui.add(
            egui::Label::new(egui::RichText::new(error).color(palette.error).monospace()).wrap(),
        );
    }
    ui.separator();
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            watches_section(ui, watches);
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
                        if frame.locals.is_empty() && frame.upvalues.is_empty() {
                            ui.weak("No local variables here yet.");
                            return;
                        }
                        variables(ui, ("paused locals", index), &frame.locals);
                        if !frame.upvalues.is_empty() {
                            ui.weak("From outside this function");
                            variables(ui, ("paused upvalues", index), &frame.upvalues);
                        }
                    });
            }
        });
    resume
}

/// Names and values, one per row.
fn variables(ui: &mut egui::Ui, id: impl std::hash::Hash, list: &[axcenity_script::Local]) {
    crate::responsive::form(ui, id, |ui| {
        for local in list {
            ui.monospace(&local.name);
            ui.add(egui::Label::new(egui::RichText::new(&local.value).monospace()).truncate())
                .on_hover_text(&local.value);
            ui.end_row();
        }
    });
}

/// The watches: each expression with its value in the innermost frame, a
/// button to remove it, and a box to add one.
fn watches_section(ui: &mut egui::Ui, watches: &mut crate::watches::Watches) {
    let palette = theme::palette(ui.ctx());
    egui::CollapsingHeader::new(theme::with_icon(icons::EYE, "Watches"))
        .id_salt("paused watches")
        .default_open(true)
        .icon(widgets::chevron)
        .show(ui, |ui| {
            let mut removed = None;
            crate::responsive::form(ui, "paused watch list", |ui| {
                for (index, watch) in watches.list.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if ui
                            .small_button(icons::X)
                            .on_hover_text("Remove this watch")
                            .clicked()
                        {
                            removed = Some(index);
                        }
                        ui.monospace(&watch.expression);
                    });
                    let (text, color) = match &watch.value {
                        Some(Ok(value)) => (value.as_str(), palette.text),
                        Some(Err(error)) => (error.as_str(), palette.error),
                        None => ("…", palette.text_weak),
                    };
                    ui.add(
                        egui::Label::new(egui::RichText::new(text).monospace().color(color))
                            .truncate(),
                    )
                    .on_hover_text(text);
                    ui.end_row();
                }
            });
            if let Some(index) = removed {
                watches.remove(index);
            }
            let response = ui.add(
                egui::TextEdit::singleline(&mut watches.draft)
                    .hint_text("Add a watch: an expression, such as health * 2")
                    .desired_width(f32::INFINITY)
                    .font(egui::TextStyle::Monospace),
            );
            if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                watches.add_draft();
                response.request_focus();
            }
        });
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
            "Paused. Continue (F5) carries on, Step Over (F10), Step Into (F11) and Step Out \
             (Shift+F11) run one line at a time, and Stop (Shift+F5) ends the game.",
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
        .chain(std::iter::once(storage_info(world)))
        .collect()
}

/// ReplicatedStorage's entry (`block_editor::STORAGE_PATH`), whose names are
/// the paths of the BindableEvents in it, for the event blocks' menus.
fn storage_info(world: &World) -> block_editor::ObjectInfo {
    let storage = world.replicated_storage();
    let names = world
        .descendants(storage)
        .into_iter()
        .filter(|&id| {
            world
                .get(id)
                .is_some_and(|instance| instance.class() == ClassName::BindableEvent)
        })
        .filter_map(|id| {
            let mut names = Vec::new();
            let mut at = Some(id);
            while let Some(here) = at.filter(|&here| here != storage) {
                let instance = world.get(here)?;
                names.push(instance.name().to_owned());
                at = instance.parent();
            }
            names.reverse();
            Some((block_editor::NameKind::Bindable, names.join(".")))
        })
        .collect();
    block_editor::ObjectInfo {
        path: block_editor::STORAGE_PATH.to_owned(),
        class: ClassName::ReplicatedStorage,
        names,
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
        // Stop is taken first: egui counts Shift+F5 as F5 too.
        let pressed = ctx.input_mut(|input| {
            shortcuts::consume_first(input, &[shortcuts::Action::Play, shortcuts::Action::Stop])
        });
        let (stop, play) = (
            pressed == Some(shortcuts::Action::Stop),
            pressed == Some(shortcuts::Action::Play),
        );
        if stop {
            self.stop();
        } else if play && self.paused() {
            self.continue_play();
        } else if play {
            self.play();
        } else if self.paused() {
            // Shift+F11 is Step Out, taken before F11's Step Into.
            let step = ctx.input_mut(|input| {
                shortcuts::consume_first(
                    input,
                    &[Action::StepOut, Action::StepInto, Action::StepOver],
                )
            });
            let how = match step {
                Some(Action::StepOver) => Some(Resume::StepOver),
                Some(Action::StepInto) => Some(Resume::StepInto),
                Some(Action::StepOut) => Some(Resume::StepOut),
                _ => None,
            };
            if let Some(how) = how {
                self.resume_play(how);
            }
        }

        // A text box being typed in has its own undo, and uses Delete and
        // Backspace itself, so leave those keys to it then. The Explorer
        // tabbed into takes only its own keys.
        let typing = ctx.wants_keyboard_input() && !self.explorer.has_focus(ctx);
        // The arrow keys, Home and End are the Explorer's while it has them,
        // taken before the viewport or the UI Designer can see them.
        let explorer_keys =
            !typing && !egui::Popup::is_any_open(ctx) && self.explorer.has_keys(ctx);
        if explorer_keys {
            ctx.input_mut(|input| self.explorer.take_keys(input));
        }
        let playing = self.playing();
        // Over a property clip, K, Delete and Backspace are the dope sheet's;
        // over the Model Studio, its own single keys and Enter are.
        let studio = self.model_studio.has_pointer();
        // Over a block script, Delete, Ctrl+D, F2 and the tools' keys would
        // change the world out of sight, so they're left alone.
        let blocks = self.block_tab_pointer.is_some();
        // Over a node graph (a material, a UI Event Graph, a behavior tree,
        // a dialogue), Delete and Ctrl+D are the graph's.
        let graph = blocks
            || self.ui_designer.graph_has_pointer()
            || self.materials.has_pointer()
            || self.behavior.has_pointer()
            || self.dialogues.has_pointer();
        let animating = self.animation.wants_keys(ctx) || studio || blocks;
        // With no Scene View showing, a tool picked by its key would act on
        // a scene the user can't see, as its ribbon button would.
        let scene = self.scene_shown(ctx);
        // The tools' single keys (1 to 5, F3X's letters) only while
        // editing, so a game gets every key. Escape goes back to Select; it
        // isn't taken, so it still closes a menu or stops the Constraint tool.
        let tool_key = |input: &mut egui::InputState| {
            if playing || animating || !scene {
                return None;
            }
            let tools: Vec<Action> = Tool::ALL.into_iter().map(Action::Tool).collect();
            match shortcuts::consume_first(input, &tools) {
                Some(Action::Tool(tool)) => Some(tool),
                _ => shortcuts::pressed(input, Action::PutToolDown).then_some(Tool::Select),
            }
        };
        let keys = ctx.input_mut(|input| {
            if typing {
                return Keys::default();
            }
            // Redo's Ctrl+Shift+Z is taken first: egui counts it as Ctrl+Z too.
            let history = shortcuts::consume_first(input, &[Action::Undo, Action::Redo]);
            Keys {
                redo: history == Some(Action::Redo),
                undo: history == Some(Action::Undo),
                tool: tool_key(input),
                // The Anchor or Angular tool's Enter, else the Explorer's
                // rename (see below).
                toggle: !playing
                    && !studio
                    && shortcuts::consume_first(input, &[Action::UseTool, Action::ExplorerRename])
                        .is_some(),
                // Taken before the viewport sees them, so Ctrl+D doesn't
                // also fly the camera right.
                duplicate: !graph && shortcuts::consume(input, Action::Duplicate),
                rename: !blocks && shortcuts::consume(input, Action::Rename),
                delete: !animating && !graph && shortcuts::consume(input, Action::Delete),
            }
        });
        if keys.redo {
            self.redo();
        }
        if keys.undo {
            self.undo();
        }
        if let Some(tool) = keys.tool {
            self.toggle_tool(tool);
        }
        if keys.toggle && self.tool == Tool::Angular {
            self.extend_angular();
        } else if keys.toggle && self.tool == Tool::Anchor {
            let property = "Anchored";
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
        } else if keys.toggle
            && explorer_keys
            && let Some(id) = self.selected
        {
            // Enter in the Explorer renames, like F2.
            self.explorer.start_rename(&self.world, id);
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
}

#[cfg(test)]
mod tests {
    use axcenity_script::{Local, Pause, PausedFrame};

    use super::*;
    use crate::watches::Watches;

    fn pause(error: Option<&str>) -> Pause {
        let local = |name: &str, value: &str| Local {
            name: name.to_owned(),
            value: value.to_owned(),
        };
        Pause {
            path: "scripts/door.server.lua".to_owned(),
            line: 7,
            frames: vec![
                PausedFrame {
                    function: Some("open".to_owned()),
                    path: "scripts/door.server.lua".to_owned(),
                    line: 7,
                    locals: vec![local("angle", "90"), local("door", "Door")],
                    upvalues: vec![local("speed", "4")],
                },
                PausedFrame {
                    function: None,
                    path: "scripts/door.server.lua".to_owned(),
                    line: 12,
                    locals: vec![local("speed", "4")],
                    upvalues: Vec::new(),
                },
            ],
            error: error.map(str::to_owned),
            breakpoint: error.is_none(),
        }
    }

    fn watches() -> Watches {
        let mut watches = Watches::default();
        for expression in ["angle * 2", "door.Missing"] {
            watches.draft = expression.to_owned();
            watches.add_draft();
        }
        watches.evaluate(|expression| match expression {
            "angle * 2" => Ok("180".to_owned()),
            _ => Err("Missing is not a valid member of Part \"Door\"".to_owned()),
        });
        watches
    }

    #[test]
    fn the_paused_panel_draws_steps_watches_and_an_error() {
        let ctx = theme::test_context();
        let mut watches = watches();
        for error in [None, Some("scripts/door.server.lua:7: locked")] {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let resume = paused_panel(ui, &pause(error), &mut watches);
                    assert_eq!(resume, None, "nothing pressed");
                });
            });
        }
        let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") else {
            return;
        };
        for (name, error) in [
            ("paused-step.png", None),
            (
                "paused-error.png",
                Some("scripts/door.server.lua:7: locked"),
            ),
        ] {
            crate::snapshot::snapshot(
                &std::path::PathBuf::from(&folder).join(name),
                egui::vec2(520.0, 330.0),
                vec![Vec::new(); 3],
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let _ = paused_panel(ui, &pause(error), &mut watches);
                    });
                },
            );
        }
    }
}
