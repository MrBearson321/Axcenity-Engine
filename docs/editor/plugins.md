# Editor plugins

Lua plugins that extend the editor (Phase 54). Editor-only: built games and the player never have plugin code. The plugin API is the `plugins` crate (`axcenity-plugins`); the editor runs plugins in `app/plugins.rs`, and keeps their state and the Plugins panel in `plugins.rs`. `examples/hello-plugins` has two worked examples.

## Where plugins come from

- A plugin is a folder with a `plugin.json` and an `init.lua`, in the project's `plugins/` folder or in the editor's own `Plugins` folder (in its data folder, beside the settings), which every project gets. Project plugins are listed first, each group by folder name. A folder without `plugin.json` isn't a plugin.
- `plugin.json`: `{ "name": "Align to Grid", "version": "1.0.0", "permissions": ["files"] }`. `name` and `version` are needed; `permissions` is optional, and `files` is the only one. Unknown keys are an error, shown in the Plugins panel.
- `require("name")` in a plugin loads `name.lua` from the plugin's folder (once). Any `.css` in the folder styles the plugin's dock tabs, never the game.
- `plugins/` is never packed into a built game (`project::pack_game`) and its stylesheets aren't the project's (`project::find_stylesheets` skips it). CI checks that `axcenity-player` doesn't depend on `axcenity-plugins`.
- Plugins start when a project opens (and again when the world is replaced, such as the world file reloading). A plugin reloads when its `plugin.json`, `.lua` or `.css` files change on disk (two watchers, one per folder); a new or removed folder makes the editor look for plugins again. A plugin whose dock UI vanished (a Console command's undo swaps the whole world) starts again.
- **Plugins panel** (View ▸ Panels, or the Plugins ribbon tab when no plugin has a toolbar): each plugin's name, version, where it is, whether it may use project files, an on/off box, Reload, and its errors (the last 20). Which plugins are off, and every plugin's settings, are kept in `Plugins.json` in the editor's data folder, by the plugin's key (`project/<folder>` or `user/<folder>`).

## The plugin API

Each plugin runs in its own sandboxed interpreter (`script::EditRuntime`): the scripts' sandbox and Instance API on the edit world, server side, `RunService:IsStudio()` true and `IsRunning()` false. Plugins only run while editing; during Play their buttons are off and nothing calls them.

- `plugin:CreateToolbar(name)` and `toolbar:CreateButton(id, tooltip, icon)`: a ribbon group on the ribbon's **Plugins** tab. `icon` is a Phosphor icon name from a short list (`grid-four`, `magnet`, `ruler`, `paint-brush`, `wrench`, …), or one symbol; anything else is a puzzle piece. A button has `Click`, `Enabled` and `SetActive(bool)`.
- `plugin:CreateDockWidget(id, { Title = "…" })` returns a ScreenGui outside the tree for the plugin's UI: Frame, TextButton, TextLabel, Slider and the other UI Instances, parented to it with the normal API. It's laid out alone by `ui::UiInteraction::only`, styled by the plugin's `.css`, and drawn in a dock tab (`Pane::Plugin`, opened beside Properties, never saved in the layout). Its `Enabled` shows or hides the tab; closing the tab turns it off. Buttons fire `Activated`, `MouseEnter` and `MouseLeave`; widgets' `ValueChanged` and other change events fire for changes the pointer makes. Being outside the tree, it's never saved, shown in the Explorer, or undone.
- `Selection:Get()`, `Selection:Set(list)` (only Instances in the game) and `Selection.SelectionChanged` (the editor's changes, not the plugin's own `Set`). Also `game:GetService("Selection")`.
- `ChangeHistoryService:SetWaypoint(name)`, `Undo()` and `Redo()`. Also `game:GetService("ChangeHistoryService")`.
- `plugin:GetSetting(key)` / `SetSetting(key, value)`: nil, booleans, numbers, strings and tables of them, kept per plugin.
- `plugin:ReadProjectFile(path)` / `WriteProjectFile(path, text)`: only with the `files` permission, and only inside the project. A path is relative with `/`; absolute paths, `..`, `\`, `:` and links that lead out of the project are refused (`plugins::files`).
- `plugin:GetMouse()`: `Hit` (a CFrame on the part under the mouse, or 1000 studs along the view), `Target`, `Button1Down`, `Button1Up` and `Move`, over the Scene View. `plugin:Activate(true)` takes the left button from selecting and dragging; `Deactivate()` and `IsActivated()`; `plugin.Deactivation` fires when it's taken back.
- `Destroy` on an Instance in the game only takes it out of the tree (as the editor's Delete does), so it can be undone. Change events fire for the dock tabs' own changes, not for the plugin's edits or the editor's.

## Undo

Every call into a plugin runs on the edit world with its change log on, after a copy of the world is taken. Afterwards the Instances the log names (in the game, before or after) become an undo step (`History::record_edits`, `history/edits.rs`): each one's place in the tree, fields, attributes and tags before and after. Later calls merge into that step until the plugin calls `SetWaypoint(name)`, which ends it and names it; several waypoints in one call make several steps (each waypoint keeps a copy of the world). A step with no waypoint is named after the plugin.

## Errors

A plugin's errors read `plugins/<folder>/init.lua:12: message` (`Plugins/<folder>/…` for the user's), go to the Console with their trace, and are kept in the Plugins panel. A plugin that doesn't compile, or has a bad `plugin.json`, doesn't start; one that errors while running keeps running. An engine crash inside a plugin call is caught (`play::guard`) and reported the same way.

## Autocomplete

`definitions/axcenity-plugin.d.luau` declares the plugin API for luau-lsp, kept by hand (a `plugins` test checks it names the whole API). File ▸ Set Up VS Code Autocomplete writes it to `.axcenity/axcenity-plugin.d.luau`; it isn't added to the project's luau-lsp settings, because game scripts don't have these globals, so add `"@axcenity-plugin": ".axcenity/axcenity-plugin.d.luau"` to `luau-lsp.types.definitionFiles` in a workspace for plugins.
