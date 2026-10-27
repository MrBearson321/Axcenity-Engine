//! The ribbon's UI ▸ Controls: a ready-made controls menu. It inserts a
//! ScreenGui named `Controls` listing the input map's actions, each with a
//! button showing what it's bound to, and a LocalScript
//! (`scripts/Controls.client.lua`) that shows the menu on the `Menu`
//! action and rebinds an action (`UserInputService:RebindAction`) to the
//! next key or button pressed after its button is clicked. The rebinds are
//! settings, so they go into save slots.

use axcenity_core::input::InputMap;
use axcenity_core::{Color3, UDim2};

use super::*;
use crate::history::Place;

/// The menu's LocalScript.
pub(super) const CONTROLS_SCRIPT: &str = r#"-- The Controls menu: Menu (Escape, or Start on a controller) shows and
-- hides it. Click an action's button, then press the key or button to use
-- for it. Rebinds are SaveService settings, so they're saved in slots.
local UserInputService = game:GetService("UserInputService")
local gui = workspace:WaitForChild("Controls")
local panel = gui:WaitForChild("Panel")
local waiting = nil

local function show(row)
	row.Binding.Text = table.concat(UserInputService:GetActionInputs(row.Name), ", ")
end

for _, row in panel:GetChildren() do
	if row:FindFirstChild("Binding") then
		show(row)
		row.Binding.Activated:Connect(function()
			waiting = row
			row.Binding.Text = "Press a key or button..."
		end)
	end
end

local MOUSE_BUTTONS = {
	[Enum.UserInputType.MouseButton1] = true,
	[Enum.UserInputType.MouseButton2] = true,
	[Enum.UserInputType.MouseButton3] = true,
}

UserInputService.InputBegan:Connect(function(input, gameProcessedEvent)
	if waiting then
		local new = nil
		if input.KeyCode ~= Enum.KeyCode.Unknown then
			new = input.KeyCode
		elseif MOUSE_BUTTONS[input.UserInputType] and not gameProcessedEvent then
			new = input.UserInputType
		end
		if new then
			UserInputService:RebindAction(waiting.Name, { new })
			show(waiting)
			waiting = nil
		end
	elseif not gameProcessedEvent and UserInputService:IsActionPressed("Menu") then
		gui.Enabled = not gui.Enabled
	end
end)
"#;

/// The actions a player rebinds in the menu: the game's own, except the
/// axes (`Move` and `Look` take four keys or a stick, not one key).
fn rebindable(map: &InputMap) -> impl Iterator<Item = &axcenity_core::input::Action> {
    map.actions
        .iter()
        .filter(|action| !action.is_ui() && !action.axis)
}

/// Makes the menu's ScreenGui, out of the tree: a panel in the middle with
/// a title and a row for each of `map`'s rebindable actions (named after
/// it, with a `Label` and a `Binding` button). It starts hidden.
pub(super) fn controls_gui(world: &mut World, map: &InputMap) -> InstanceId {
    const ROW: f32 = 40.0;
    let make = |world: &mut World, class, name: &str, parent| {
        let id = world.create(class).expect("UI classes can be made");
        world.set_name(id, name).expect("just made");
        world.set_parent(id, parent).expect("both exist");
        id
    };
    let set = |world: &mut World, id, property: &str, value: Value| {
        world
            .set_property(id, property, value)
            .expect("UI classes have these properties");
    };
    let gui = make(world, ClassName::ScreenGui, "Controls", None);
    set(world, gui, "Enabled", Value::Bool(false));
    set(world, gui, "DisplayOrder", Value::Number(10.0));
    let rows = rebindable(map).count() as f32;
    let height = 64.0 + rows * ROW + 16.0;
    let panel = make(world, ClassName::Frame, "Panel", Some(gui));
    set(
        world,
        panel,
        "Position",
        Value::UDim2(UDim2::new(0.5, -210.0, 0.5, -height / 2.0)),
    );
    set(
        world,
        panel,
        "Size",
        Value::UDim2(UDim2::new(0.0, 420.0, 0.0, height)),
    );
    set(
        world,
        panel,
        "BackgroundColor3",
        Value::Color3(Color3::new(0.11, 0.12, 0.15)),
    );
    let title = make(world, ClassName::TextLabel, "Title", Some(panel));
    set(world, title, "Text", Value::String("Controls".to_owned()));
    set(world, title, "TextSize", Value::Number(24.0));
    set(
        world,
        title,
        "Position",
        Value::UDim2(UDim2::new(0.0, 16.0, 0.0, 12.0)),
    );
    set(
        world,
        title,
        "Size",
        Value::UDim2(UDim2::new(1.0, -32.0, 0.0, 36.0)),
    );
    let names: Vec<String> = rebindable(map).map(|action| action.name.clone()).collect();
    for (index, name) in names.iter().enumerate() {
        let top = 64.0 + index as f32 * ROW;
        let row = make(world, ClassName::Frame, name, Some(panel));
        set(
            world,
            row,
            "Position",
            Value::UDim2(UDim2::new(0.0, 16.0, 0.0, top)),
        );
        set(
            world,
            row,
            "Size",
            Value::UDim2(UDim2::new(1.0, -32.0, 0.0, ROW - 6.0)),
        );
        set(world, row, "BackgroundTransparency", Value::Number(1.0));
        let label = make(world, ClassName::TextLabel, "Label", Some(row));
        set(world, label, "Text", Value::String(name.clone()));
        set(
            world,
            label,
            "Size",
            Value::UDim2(UDim2::new(0.4, 0.0, 1.0, 0.0)),
        );
        let button = make(world, ClassName::TextButton, "Binding", Some(row));
        set(
            world,
            button,
            "Position",
            Value::UDim2(UDim2::new(0.4, 0.0, 0.0, 0.0)),
        );
        set(
            world,
            button,
            "Size",
            Value::UDim2(UDim2::new(0.6, 0.0, 1.0, 0.0)),
        );
        set(
            world,
            button,
            "BackgroundColor3",
            Value::Color3(Color3::new(0.22, 0.25, 0.32)),
        );
        let inputs = map
            .action(name)
            .map(|action| {
                action
                    .inputs
                    .iter()
                    .map(axcenity_core::input::Input::name)
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        set(world, button, "Text", Value::String(inputs));
    }
    gui
}

impl EditorApp {
    /// Inserts the Controls menu into the Workspace (one undo step) and
    /// makes its LocalScript, unless the project has one already.
    pub(super) fn insert_controls_ui(&mut self) {
        let Some(project) = &self.project else {
            return;
        };
        let (folder, map) = (project.folder.clone(), project.input_map.clone());
        let gui = controls_gui(&mut self.world, &map);
        let workspace = self.world.workspace();
        let name = tree::unique_name(&self.world, workspace, "Controls");
        self.world.set_name(gui, &name).expect("just made");
        let index = self.world.get(workspace).map_or(0, |w| w.children().len());
        let place = Place {
            parent: workspace,
            index,
        };
        if !self.place(gui, Some(place), "Insert Controls".to_owned()) {
            return;
        }
        self.select_from_viewport(Some(gui));
        let service = axcenity_project::script_service(&self.world, ClassName::LocalScript);
        if self.world.find_first_child(service, "Controls").is_some() {
            self.output.info(
                "Inserted the Controls menu. The project already has a Controls LocalScript, \
                 so it was left as it is.",
            );
            return;
        }
        match axcenity_project::create_script(
            &folder,
            "Controls",
            ClassName::LocalScript,
            CONTROLS_SCRIPT,
        ) {
            Ok(path) => {
                self.add_script(&path, CONTROLS_SCRIPT.to_owned());
                self.output.info(format!(
                    "Inserted the Controls menu and its LocalScript, `{path}`. In the game, \
                     Menu (Escape, or Start on a controller) shows it."
                ));
            }
            Err(err) => self.output.error(format!(
                "Inserted the Controls menu, but could not make its LocalScript: {}",
                files::chain(&err)
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use axcenity_core::KeyCode;
    use axcenity_core::input::{Input, InputEvent, InputKind};

    use super::super::tests::{copy_of_example, play_for};
    use super::*;

    #[test]
    fn the_controls_menu_shows_on_menu_and_rebinds_an_action_to_the_next_key() {
        let folder = copy_of_example("hello-character", "controls-menu");
        let mut app = EditorApp::new(Some(&folder));
        app.insert_controls_ui();
        let gui = app
            .world
            .find_first_child(app.world.workspace(), "Controls")
            .unwrap();
        assert!(folder.join("scripts/Controls.client.lua").is_file());
        // One undo step takes the menu out again (its script stays).
        assert_eq!(app.history.undo_label(), Some("Insert Controls"));

        app.play();
        play_for(&mut app, 0.2);
        let enabled = |app: &EditorApp| {
            app.play
                .as_ref()
                .unwrap()
                .world()
                .get(gui)
                .unwrap()
                .property("Enabled")
                .cloned()
        };
        assert_eq!(enabled(&app), Some(Value::Bool(false)));
        let tap = |app: &mut EditorApp, key| {
            for pressed in [true, false] {
                let event = InputEvent::new(InputKind::Key { key, pressed });
                app.play_frame(1.0 / 60.0, &[event], None, None);
            }
        };
        tap(&mut app, KeyCode::Escape);
        assert_eq!(enabled(&app), Some(Value::Bool(true)));

        let panel = app.world.find_first_child(gui, "Panel").unwrap();
        let jump = app.world.find_first_child(panel, "Jump").unwrap();
        let button = app.world.find_first_child(jump, "Binding").unwrap();
        let text = |app: &EditorApp| {
            app.play
                .as_ref()
                .unwrap()
                .world()
                .get(button)
                .unwrap()
                .property("Text")
                .cloned()
        };
        assert_eq!(text(&app), Some(Value::String("Space, ButtonA".into())));
        app.play_frame(1.0 / 60.0, &[], Some(button), None);
        tap(&mut app, KeyCode::K);
        assert_eq!(text(&app), Some(Value::String("K".into())));
        let map = app.play.as_ref().unwrap().input_map();
        assert_eq!(map.action("Jump").unwrap().inputs, [Input::Key(KeyCode::K)]);
        let errors: Vec<_> = app
            .output
            .lines()
            .iter()
            .filter(|line| line.kind == crate::output::OutputKind::Error)
            .map(|line| line.text.clone())
            .collect();
        assert!(errors.is_empty(), "{errors:?}");
    }
}
