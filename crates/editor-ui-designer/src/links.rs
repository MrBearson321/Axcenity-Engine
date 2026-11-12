//! **Code & animation** in the inspector: where the selected element meets
//! the script editor and the Animation tab.
//!
//! - Its Lua path (`workspace.Menu.Panel.Play`), to copy into a script.
//! - **Handlers**: one button per event it fires (a button's `Activated`,
//!   a text box's `FocusLost`, a widget's `ValueChanged`, `MouseEnter`
//!   and `MouseLeave`). Each writes `path.Event:Connect(function() end)`
//!   at the end of the ScreenGui's LocalScript (the first `.client.lua`
//!   that names the ScreenGui's path, else a new one named after it) and
//!   opens it with the cursor in the function (`handler_script`,
//!   `app/ui_links.rs`). A button's **Play on click** does the same with
//!   `animator:LoadAnimation(animation):Play()` in it, for an Animation in
//!   its ScreenGui.
//! - **In scripts**: each line of a Lua script that names the element
//!   (`.Play`, `"Play"`), which opens that script at that line.
//! - **Animations**: the element's Animations (its own, or its Animator's),
//!   each opening its clip in the Animation tab, and **Animate**, which
//!   makes one (an Animator, a keyed clip and an Animation) and opens it
//!   with Record on.

use axcenity_core::{ClassName, InstanceId, Value, World};
use eframe::egui;

use super::{DesignerAction, DesignerScene};
use crate::edit::Edit;
use crate::theme::{self, icons};
use crate::widgets;

/// How many script lines naming the element are listed.
const MENTIONS_SHOWN: usize = 8;

/// A script's path and its code as the editor has it now (its tab's text,
/// unsaved changes and all, when it's open).
#[derive(Clone, Copy, Debug)]
pub struct ScriptText<'a> {
    pub path: &'a str,
    pub text: &'a str,
}

/// Whether `name` can be written after a `.` in Lua.
fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `id`'s path from `workspace` in Lua, such as `workspace.Menu.Play`
/// (`["My Button"]` for a name that isn't a Lua name). `None` outside the
/// Workspace.
pub fn lua_path(world: &World, id: InstanceId) -> Option<String> {
    let workspace = world.workspace();
    let mut names = Vec::new();
    let mut current = id;
    while current != workspace {
        let instance = world.get(current)?;
        names.push(instance.name().to_owned());
        current = instance.parent()?;
    }
    let mut path = "workspace".to_owned();
    for name in names.iter().rev() {
        if is_identifier(name) {
            path.push('.');
            path.push_str(name);
        } else {
            path.push_str(&format!("[{name:?}]"));
        }
    }
    Some(path)
}

/// Whether `text` has `name` where Lua code names a child: after a `.`,
/// or alone in quotes (`FindFirstChild("Play")`, `{ "Play", "Quit" }`).
fn names_child(text: &str, name: &str) -> bool {
    let ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    text.match_indices(name).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + name.len()..].chars().next();
        match (before, after) {
            (Some('.'), after) => !after.is_some_and(ident),
            (Some(quote @ ('"' | '\'')), Some(close)) => close == quote,
            _ => false,
        }
    })
}

/// Each line (path, counting from 1) of `scripts` that names `name`, as
/// [`names_child`] reads it, comment lines left out.
pub fn mentions(scripts: &[ScriptText], name: &str) -> Vec<(String, usize)> {
    if name.is_empty() {
        return Vec::new();
    }
    scripts
        .iter()
        .flat_map(|script| {
            script
                .text
                .lines()
                .enumerate()
                .filter(|(_, line)| !line.trim_start().starts_with("--") && names_child(line, name))
                .map(|(index, _)| (script.path.to_owned(), index + 1))
        })
        .collect()
}

/// Whether a handler can be written into the script at `path`: a Lua
/// LocalScript (not blocks or an Event Graph).
pub fn is_handler_script(path: &str) -> bool {
    path.ends_with(".client.lua") && !axcenity_blocks::ui_graph::is_ui_graph(path)
}

/// The LocalScript a handler for an element of `gui` goes in: the first
/// Lua LocalScript that names the ScreenGui's path, or `None` to make one.
pub fn handler_script<'a>(
    world: &World,
    scripts: &[ScriptText<'a>],
    gui: InstanceId,
) -> Option<&'a str> {
    let path = lua_path(world, gui)?;
    scripts
        .iter()
        .find(|script| {
            is_handler_script(script.path)
                && script.text.match_indices(&path).any(|(at, _)| {
                    !script.text[at + path.len()..].starts_with(char::is_alphanumeric)
                })
        })
        .map(|script| script.path)
}

/// The arguments a handler for `event` takes.
fn parameters(event: &str) -> &'static str {
    match event {
        "ValueChanged" => "value",
        _ => "",
    }
}

/// A handler for `event` on the element at `path`: a blank line, then the
/// connection, with `body` (or an empty line) inside. Returns the text and
/// which of its lines (counting from 1) the cursor goes on.
pub fn handler(path: &str, event: &str, body: Option<&str>) -> (String, usize) {
    let body = body.unwrap_or("");
    (
        format!(
            "\n{path}.{event}:Connect(function({})\n\t{body}\nend)\n",
            parameters(event)
        ),
        3,
    )
}

/// The Animations that animate `id`: its own, and its Animator's.
pub fn animations_of(world: &World, id: InstanceId) -> Vec<InstanceId> {
    let children = |id| {
        world
            .get(id)
            .map_or(&[][..], |instance| instance.children())
    };
    let is = |child: InstanceId, class| world.get(child).is_some_and(|c| c.class() == class);
    let mut animations: Vec<InstanceId> = children(id)
        .iter()
        .copied()
        .filter(|&child| is(child, ClassName::Animation))
        .collect();
    if let Some(animator) = crate::animatable::animator_of(world, id) {
        animations.extend(
            children(animator)
                .iter()
                .copied()
                .filter(|&child| is(child, ClassName::Animation)),
        );
    }
    animations
}

/// Every Animation in `gui` (at any depth), with what it animates (its
/// parent, or its Animator's parent).
pub fn animations_in(world: &World, gui: InstanceId) -> Vec<(InstanceId, InstanceId)> {
    world
        .descendants(gui)
        .into_iter()
        .chain(std::iter::once(gui))
        .filter(|&id| {
            world
                .get(id)
                .is_some_and(|i| i.class() != ClassName::Animation)
        })
        .flat_map(|object| {
            animations_of(world, object)
                .into_iter()
                .map(move |animation| (animation, object))
        })
        .collect()
}

/// An Animation's clip file name (or its own name when it has none).
fn clip_name(world: &World, animation: InstanceId) -> String {
    let Some(instance) = world.get(animation) else {
        return String::new();
    };
    instance
        .property("Clip")
        .and_then(Value::as_str)
        .filter(|clip| !clip.is_empty())
        .map(|clip| clip.rsplit('/').next().unwrap_or(clip).to_owned())
        .unwrap_or_else(|| instance.name().to_owned())
}

/// The Code & animation section for `id` in `gui`.
pub(super) fn links_section(
    ui: &mut egui::Ui,
    scene: &DesignerScene,
    gui: InstanceId,
    id: InstanceId,
    actions: &mut Vec<DesignerAction>,
) {
    let world = scene.world;
    let Some(instance) = world.get(id) else {
        return;
    };
    let palette = theme::palette(ui.ctx());

    // The path, to copy into a script.
    if let Some(path) = lua_path(world, id) {
        ui.horizontal(|ui| {
            if widgets::icon_button(ui, icons::COPY, "Copy this Lua path", false).clicked() {
                ui.ctx().copy_text(path.clone());
            }
            ui.add(egui::Label::new(egui::RichText::new(&path).monospace().small()).truncate())
                .on_hover_text(format!("{path}\nHow a LocalScript finds this element"));
        });
    }

    // A handler per event.
    let events = instance.class().events();
    if !events.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("When").color(palette.text_weak));
            for &event in events {
                if ui
                    .add_enabled(
                        scene.editing,
                        egui::Button::new(theme::with_icon(icons::CODE, event)).small(),
                    )
                    .on_hover_text(format!(
                        "Write `{}:Connect(…)` into this ScreenGui's LocalScript and open it there",
                        event
                    ))
                    .clicked()
                {
                    actions.push(DesignerAction::AddHandler {
                        id,
                        event: event.to_owned(),
                        body: None,
                    });
                }
            }
        });
    }

    // Play an animation in this ScreenGui when the button is clicked.
    let in_gui = animations_in(world, gui);
    if instance.class() == ClassName::TextButton && !in_gui.is_empty() {
        ui.add_enabled_ui(scene.editing, |ui| {
            axcenity_editor_kit::widgets::menu_button(
                ui,
                Some(icons::PLAY),
                "Play on click",
                |ui| {
                    for &(animation, object) in &in_gui {
                        let label = format!(
                            "{} · {}",
                            world.get(object).map_or("", |i| i.name()),
                            clip_name(world, animation)
                        );
                        let animator = crate::animatable::animator_of(world, object)
                            .and_then(|animator| lua_path(world, animator));
                        if ui.button(label).clicked()
                            && let Some(path) = lua_path(world, animation)
                            && let Some(animator) = animator
                        {
                            actions.push(DesignerAction::AddHandler {
                                id,
                                event: "Activated".to_owned(),
                                body: Some(format!("{animator}:LoadAnimation({path}):Play()")),
                            });
                            ui.close();
                        }
                    }
                },
            )
            .response
            .on_hover_text("Play one of this ScreenGui's animations when the button is clicked");
        });
    }

    // Where scripts name it.
    let found = mentions(scene.scripts, instance.name());
    if !found.is_empty() {
        ui.label(egui::RichText::new("In scripts").color(palette.text_weak));
        for (path, line) in found.iter().take(MENTIONS_SHOWN) {
            let name = path.rsplit('/').next().unwrap_or(path);
            if ui
                .link(format!("{} {name}:{line}", icons::FILE_CODE))
                .on_hover_text(format!("Open {path} at line {line}"))
                .clicked()
            {
                actions.push(DesignerAction::OpenScript {
                    path: path.clone(),
                    line: *line,
                });
            }
        }
        if found.len() > MENTIONS_SHOWN {
            ui.weak(format!("and {} more", found.len() - MENTIONS_SHOWN));
        }
    }

    // Its animations.
    let own = animations_of(world, id);
    for &animation in &own {
        let clip = clip_name(world, animation);
        if ui
            .link(format!("{} {clip}", icons::FILM_STRIP))
            .on_hover_text("Open this clip in the Animation tab")
            .clicked()
        {
            actions.push(DesignerAction::Edit(animation, Edit::OpenClip));
        }
    }
    if crate::animatable::can_animate(instance.class()) {
        let label = if own.is_empty() {
            format!("Animate {}", instance.name())
        } else {
            "New animation".to_owned()
        };
        if ui
            .add_enabled(
                scene.editing,
                egui::Button::new(theme::with_icon(icons::FILM_SLATE, &label)),
            )
            .on_hover_text(
                "Give it an Animator, a new clip and an Animation playing it, and open the \
                 clip in the Animation tab with Record on",
            )
            .clicked()
        {
            actions.push(DesignerAction::Edit(id, Edit::Animate));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lua_paths_go_from_the_workspace_and_quote_odd_names() {
        let mut world = World::new();
        let gui = world.create(ClassName::ScreenGui).unwrap();
        world.set_name(gui, "Menu").unwrap();
        world.set_parent(gui, Some(world.workspace())).unwrap();
        let button = world.create(ClassName::TextButton).unwrap();
        world.set_name(button, "Play Button").unwrap();
        world.set_parent(button, Some(gui)).unwrap();
        assert_eq!(lua_path(&world, gui).as_deref(), Some("workspace.Menu"));
        assert_eq!(
            lua_path(&world, button).as_deref(),
            Some("workspace.Menu[\"Play Button\"]")
        );
    }

    #[test]
    fn mentions_are_lines_that_name_a_child_outside_comments() {
        let text = "local panel = workspace.Menu.Panel\n\
                    -- Play is the big button\n\
                    panel.Play.Activated:Connect(go)\n\
                    for _, name in { \"Play\", \"Quit\" } do end\n\
                    print(\"Play pressed\")\n\
                    panel.Player.Text = \"x\"\n";
        let scripts = [ScriptText {
            path: "scripts/menu.client.lua",
            text,
        }];
        assert_eq!(
            mentions(&scripts, "Play"),
            [
                ("scripts/menu.client.lua".to_owned(), 3),
                ("scripts/menu.client.lua".to_owned(), 4)
            ]
        );
    }

    #[test]
    fn a_handler_goes_in_the_lua_local_script_that_names_its_screen_gui() {
        let mut world = World::new();
        let gui = world.create(ClassName::ScreenGui).unwrap();
        world.set_name(gui, "Menu").unwrap();
        world.set_parent(gui, Some(world.workspace())).unwrap();
        let scripts = [
            ScriptText {
                path: "scripts/hud.client.lua",
                text: "local hud = workspace.MenuBar",
            },
            ScriptText {
                path: "scripts/menu.server.lua",
                text: "workspace.Menu.Visible = true",
            },
            ScriptText {
                path: "scripts/menu.client.lua",
                text: "local menu = workspace.Menu\n",
            },
        ];
        assert_eq!(
            handler_script(&world, &scripts, gui),
            Some("scripts/menu.client.lua")
        );
        assert_eq!(handler_script(&world, &scripts[..2], gui), None);
        let (text, line) = handler("workspace.Menu.Volume", "ValueChanged", None);
        assert_eq!(
            text,
            "\nworkspace.Menu.Volume.ValueChanged:Connect(function(value)\n\t\nend)\n"
        );
        assert_eq!(text.lines().nth(line - 1), Some("\t"));
    }
}
