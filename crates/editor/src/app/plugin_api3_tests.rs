//! Plugin API 3 (P7.4) in the editor: commands in Ctrl+K and on their
//! shortcuts, Explorer items, `ModelImported` inside the import's undo
//! step, `Unloading` on reload, `OpenScript` and `Notify`.

use axcenity_core::Value;

use super::tests::{copy_of_example, example};
use super::*;

const PLUGIN: &str = r#"
local tidy = plugin:CreateAction({ Id = "tidy", Text = "Tidy up", Shortcut = "Ctrl+Alt+J", Tooltip = "Prints tidy" })
tidy.Triggered:Connect(function()
    print("tidy")
    plugin:Notify("Tidied")
end)
local open = plugin:CreateAction({ Id = "open-main", Text = "Open the first script" })
open.Triggered:Connect(function()
    for _, item in game:GetDescendants() do
        if item.ClassName == "Script" then
            plugin:OpenScript(item, 2)
            return
        end
    end
end)
local tag = plugin:AddExplorerAction({ Label = "Tag as prop", Class = "BasePart" })
tag.Activated:Connect(function(instances)
    for _, part in instances do
        part:SetAttribute("Prop", true)
    end
end)
plugin.ModelImported:Connect(function(root, path)
    root.Name = "Imported"
    root:SetAttribute("From", path)
end)
plugin.Unloading:Connect(function()
    plugin:SetSetting("unloaded", true)
end)
"#;

fn opened(test: &str) -> (EditorApp, std::path::PathBuf) {
    let folder = copy_of_example("hello-plugins", test);
    let tidy = folder.join("plugins").join("tidy");
    std::fs::create_dir_all(&tidy).unwrap();
    std::fs::write(
        tidy.join("plugin.json"),
        r#"{ "name": "Tidy", "version": "1.0.0" }"#,
    )
    .unwrap();
    std::fs::write(tidy.join("init.lua"), PLUGIN).unwrap();
    let mut app = EditorApp::new(None);
    app.open(&folder);
    app.plugins_frame(1.0 / 60.0);
    (app, folder)
}

fn index(app: &EditorApp) -> usize {
    app.plugins
        .entries
        .iter()
        .position(|entry| entry.found.key == "project/tidy")
        .unwrap()
}

fn printed(app: &EditorApp) -> Vec<String> {
    app.output
        .lines()
        .iter()
        .map(|line| line.text.clone())
        .collect()
}

fn command(app: &EditorApp, ctx: &egui::Context, title: &str) -> command_palette::CommandEntry {
    app.palette_commands(ctx)
        .into_iter()
        .find(|entry| entry.entry.title == title)
        .unwrap_or_else(|| panic!("no command {title}"))
}

#[test]
fn a_plugin_command_is_in_ctrl_k_and_runs_from_there_and_its_shortcut() {
    let (mut app, folder) = opened("plugin-api3-commands");
    let ctx = theme::test_context();
    let tidy = command(&app, &ctx, "Tidy up");
    assert_eq!(tidy.entry.detail, "Plugin › Tidy");
    assert_eq!(tidy.entry.shortcut.as_deref(), Some("Ctrl+Alt+J"));
    assert!(tidy.entry.aliases.contains(&"tidy".to_owned()));
    assert!(tidy.entry.enabled);

    app.run_command(&ctx, tidy.command);
    assert_eq!(printed(&app).last().map(String::as_str), Some("tidy"));
    assert!(app.toasts.texts().contains(&"Tidy: Tidied"));

    // Its shortcut runs it while editing.
    let before = printed(&app).len();
    let modifiers = egui::Modifiers::COMMAND | egui::Modifiers::ALT;
    let input = egui::RawInput {
        events: vec![egui::Event::Key {
            key: egui::Key::J,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }],
        ..Default::default()
    };
    let _ = crate::theme::run(&ctx, input, |ui| app.plugin_shortcuts(ui.ctx()));
    assert_eq!(printed(&app).len(), before + 1);

    // A command that opens a script at a line.
    app.insert_script(ClassName::Script);
    let path = app.tabs.active_path().unwrap().to_owned();
    app.tabs.close(&path);
    let open = command(&app, &ctx, "Open the first script");
    app.run_command(&ctx, open.command);
    assert_eq!(app.tabs.active_path(), Some(path.as_str()));
    assert_eq!(app.tabs.get(&path).unwrap().go_to_line(), Some(2));
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_plugin_explorer_item_gets_the_selection_and_undoes() {
    let (mut app, folder) = opened("plugin-api3-explorer");
    let plugin = index(&app);
    let items = app.plugins.explorer_actions();
    assert_eq!(items.len(), 1);
    let (at, name, item) = &items[0];
    assert_eq!((*at, name.as_str()), (plugin, "Tidy"));
    assert!(item.offered_for(ClassName::Part) && !item.offered_for(ClassName::Model));

    let parts: Vec<InstanceId> = app
        .world
        .descendants(app.world.workspace())
        .into_iter()
        .filter(|&id| app.world.get(id).unwrap().class() == ClassName::Part)
        .take(2)
        .collect();
    assert!(!parts.is_empty());
    app.selection = parts.clone();
    app.plugin_explorer_action(plugin, item.id);
    let prop = |app: &EditorApp, id| app.world.get(id).unwrap().attribute("Prop").cloned();
    for &id in &parts {
        assert_eq!(prop(&app, id), Some(Value::Bool(true)));
    }
    app.undo();
    for &id in &parts {
        assert_eq!(prop(&app, id), None);
    }
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn model_imported_changes_are_part_of_the_import_undo_step() {
    let (mut app, folder) = opened("plugin-api3-import");
    let before = app.history.undo_label().map(str::to_owned);
    let count = |app: &EditorApp| app.world.descendants(app.world.workspace()).len();
    let children = count(&app);
    app.import_model(&example("hello-mesh").join("assets/models/pyramid.gltf"));
    if let Some(dialog) = app.importing.take() {
        app.import_chosen(&dialog);
    }
    let imported = app
        .world
        .find_first_child(app.world.workspace(), "Imported")
        .expect("the plugin renamed it");
    let from = app.world.get(imported).unwrap().attribute("From").cloned();
    assert!(
        matches!(&from, Some(Value::String(path)) if path.ends_with("pyramid.gltf")),
        "{from:?}"
    );
    // One undo takes the import and the plugin's changes away together.
    app.undo();
    assert_eq!(count(&app), children);
    assert_eq!(app.history.undo_label().map(str::to_owned), before);
    app.redo();
    let again = app
        .world
        .find_first_child(app.world.workspace(), "Imported")
        .expect("redo brings the plugin's name back");
    assert!(app.world.get(again).unwrap().attribute("From").is_some());
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn unloading_saves_a_setting_when_the_plugin_reloads() {
    let (mut app, folder) = opened("plugin-api3-unloading");
    let plugin = index(&app);
    assert!(
        !app.plugins
            .settings("project/tidy")
            .contains_key("unloaded")
    );
    app.reload_plugin(plugin);
    assert_eq!(
        app.plugins.settings("project/tidy").get("unloaded"),
        Some(&serde_json::Value::Bool(true))
    );
    // Reloading made its commands again, once.
    let titles: Vec<String> = app
        .plugins
        .actions()
        .into_iter()
        .map(|(_, action)| action.text)
        .collect();
    assert_eq!(titles, ["Tidy up", "Open the first script"]);

    // Closing the project stops it too, and it hears that first.
    app.plugins.set_settings("project/tidy", Default::default());
    app.finish_closing_project();
    assert_eq!(
        app.plugins.settings("project/tidy").get("unloaded"),
        Some(&serde_json::Value::Bool(true))
    );
    assert!(app.plugins.actions().is_empty());
    let _ = std::fs::remove_dir_all(&folder);
}
