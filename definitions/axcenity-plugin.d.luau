-- The Axcenity editor's plugin API, for luau-lsp: load it after
-- `axcenity.d.luau` when editing a plugin's Lua (a folder in `plugins/`).
-- File > Set Up VS Code Autocomplete writes it to
-- `.axcenity/axcenity-plugin.d.luau`; add it to
-- `luau-lsp.types.definitionFiles` as "@axcenity-plugin" in a workspace for
-- plugins (game scripts don't have these globals).
-- Kept by hand; `crates/plugins` tests that it names the whole API.

--- A plugin event. Handlers run as tasks of their own.
declare extern type PluginSignal with
	--- Runs `callback` every time the event fires. Returns a connection for stopping it.
	function Connect(self, callback: (...any) -> ()): PluginConnection
	--- Runs `callback` the next time the event fires, then disconnects it.
	function Once(self, callback: (...any) -> ()): PluginConnection
end

--- A function connected to a plugin event.
declare extern type PluginConnection with
	--- True until `Disconnect` is called.
	Connected: boolean
	--- Stops the function from running when the event fires again.
	function Disconnect(self): ()
end

--- A button in a plugin's ribbon group.
declare extern type PluginToolbarButton with
	--- Fires when the button is clicked.
	Click: PluginSignal
	--- Off: greyed out, and clicks do nothing.
	Enabled: boolean
	--- Draws the button pressed in (on) or not.
	function SetActive(self, active: boolean): ()
end

--- A plugin's ribbon group, on the ribbon's Plugins tab.
declare extern type PluginToolbar with
	--- Adds a button labelled `id`. `icon` is a Phosphor icon name such as "grid-four", or one symbol. Asking again for the same id gives the same button.
	function CreateButton(self, id: string, tooltip: string?, icon: string?): PluginToolbarButton
end

--- The mouse over the Scene View (`plugin:GetMouse()`).
declare extern type PluginMouse with
	--- Where the mouse points in the world: on the part under it, or far along the view.
	Hit: CFrame
	--- The part (or Terrain) under the mouse, or nil.
	Target: Instance?
	--- The left button went down over the Scene View.
	Button1Down: PluginSignal
	--- The left button went up after going down over the Scene View.
	Button1Up: PluginSignal
	--- The mouse moved over the Scene View.
	Move: PluginSignal
end

--- A shape a plugin drew in the Scene View. It stays until removed.
declare extern type PluginDrawing with
	--- Takes the drawing out of the Scene View.
	function Remove(self): ()
	--- Draws it in another colour.
	function SetColor(self, color: Color3): ()
end

--- How a drawing looks.
type DrawOptions = {
	--- Its colour (left out: orange).
	Color: Color3?,
	--- Line width in points, 0.5 to 32 (left out: 2). For `DrawText`, the text size (left out: 14).
	Thickness: number?,
}

--- A dot in the Scene View the user drags (`plugin:CreateHandle`).
declare extern type PluginHandle with
	--- Where it is. Setting it moves it (without firing Dragged).
	Position: Vector3
	--- Fires with the new position each time the user drags it.
	Dragged: PluginSignal
	--- Fires with the final position when the user lets go: a good place for `ChangeHistoryService:SetWaypoint`.
	DragEnded: PluginSignal
	--- Takes the handle out of the Scene View and disconnects its events.
	function Remove(self): ()
end

--- How a handle looks and moves.
type HandleOptions = {
	--- Its colour (left out: orange).
	Color: Color3?,
	--- Its radius in points, 3 to 24 (left out: 7).
	Size: number?,
	--- A direction to slide along (left out: it moves in the plane facing the camera).
	Axis: Vector3?,
}

--- A row a plugin added to the Properties panel.
declare extern type PluginPropertyRow with
	--- A button row: fires with the Instance when it's clicked.
	Activated: PluginSignal
	--- An attribute row: fires with the Instance and the new value after the user edits it there (the attribute is already set, as its own undo step).
	Changed: PluginSignal
	--- Takes the row out of the Properties panel and disconnects its events.
	function Remove(self): ()
end

--- An attribute shown as a row of its own in the Properties panel.
type PropertyRowInfo = {
	--- The attribute it edits.
	Attribute: string,
	--- What it shows (and its type) while the attribute isn't set: a boolean, number, string, Vector3, Vector2, Color3, UDim2, CFrame or EnumItem.
	Default: any,
	--- It shows for Instances of this class (IsA), such as "BasePart" (left out: every Instance).
	Class: string?,
	--- Its name in the panel (left out: the attribute's name).
	Label: string?,
	--- Shown when the mouse is over its name.
	Tooltip: string?,
	--- For numbers: the smallest value (with Max, a slider).
	Min: number?,
	--- For numbers: the largest value.
	Max: number?,
}

--- A button in the Properties panel.
type PropertyButtonInfo = {
	--- The button's text.
	Label: string,
	--- It shows for Instances of this class (IsA) (left out: every Instance).
	Class: string?,
	--- Shown when the mouse is over it.
	Tooltip: string?,
}

--- A plugin's importer for files the editor doesn't import itself.
declare extern type PluginImporter with
	--- Fires with the file's name (no folders) and its contents (a string of its bytes) when such a file is imported: with Import..., or dropped on the editor. What the plugin changes is an undo step.
	Imported: PluginSignal
	--- Stops importing those files.
	function Remove(self): ()
end

--- An item a plugin added to the Assets panel's right-click menu.
declare extern type PluginAssetAction with
	--- Fires with the asset's path in the project (such as "assets/models/tree.glb") when it's picked.
	Activated: PluginSignal
	--- Takes the item out of the menu.
	function Remove(self): ()
end

--- What an importer takes.
type ImporterInfo = {
	--- What the files are, for the Import... file picker.
	Name: string,
	--- File endings, such as { "csv", "txt" }. Endings the editor imports itself (models, images, sounds, skies, prefabs, packages) stay the editor's.
	Extensions: { string },
}

--- An Assets panel menu item.
type AssetActionInfo = {
	--- The item's text.
	Label: string,
	--- File endings it's offered for (left out: every project asset).
	Extensions: { string }?,
	--- Shown when the mouse is over it.
	Tooltip: string?,
}

--- A command a plugin made (API 3).
declare extern type PluginAction with
	--- Fires when it's run: picked in Ctrl+K, or its shortcut pressed while editing. What the plugin changes is an undo step.
	Triggered: PluginSignal
	--- Takes the command away.
	function Remove(self): ()
end

--- An item a plugin added to the Explorer's right-click menu (API 3).
declare extern type PluginExplorerAction with
	--- Fires with the selected Instances (the row right-clicked is one of them) when it's picked. What the plugin changes is an undo step.
	Activated: PluginSignal
	--- Takes the item out of the menu.
	function Remove(self): ()
end

--- A command.
type ActionInfo = {
	--- Names it: CreateAction with the same Id again changes this one.
	Id: string,
	--- What Ctrl+K lists it as (left out: its Id).
	Text: string?,
	--- A keyboard shortcut while editing, such as "Ctrl+Shift+A" or "F9": Ctrl (Cmd on macOS), Shift and Alt with a letter, a digit or F1 to F12. Letters and digits need Ctrl or Alt. A shortcut the editor already uses stays the editor's.
	Shortcut: string?,
	--- Shown under it in Ctrl+K.
	Tooltip: string?,
}

--- An Explorer menu item.
type ExplorerActionInfo = {
	--- The item's text.
	Label: string,
	--- It shows on rows of this class, or a class that is one (IsA), such as "Model" or "BasePart" (left out: every row).
	Class: string?,
	--- Shown when the mouse is over it.
	Tooltip: string?,
}

--- What a dock widget starts with.
type DockWidgetInfo = {
	--- The tab's title (left out: the widget's id).
	Title: string?,
}

--- The running plugin.
declare extern type Plugin with
	--- Fires when the plugin loses the Scene View's left button.
	Deactivation: PluginSignal
	--- The plugin API's version: 1 for toolbars, dock widgets, selection, undo, settings, files and the mouse; 2 adds drawing, handles, Properties rows, importers and Assets panel items; 3 adds commands, Explorer items, ModelImported, Unloading, OpenScript and Notify. Check it to work in older editors.
	ApiVersion: number
	--- Fires with the new file's path in the project after the editor imports a file itself (a model, image, sound, sky or prefab).
	AssetImported: PluginSignal
	--- API 3. Fires with what an imported model made (its Model, or each MeshPart) and the model file's path in the project, before it's put in the world: name parts, add attributes and tags, set collision groups. Changes to it are part of the import's undo step.
	ModelImported: PluginSignal
	--- API 3. Fires before the plugin is stopped or loaded again (its files saved, turned off, the project closed): keep what it needs with SetSetting.
	Unloading: PluginSignal
	--- A ribbon group named `name` (the same one when asked again).
	function CreateToolbar(self, name: string): PluginToolbar
	--- A dock tab: a ScreenGui outside the game for the plugin's UI (Frame, TextButton, Slider and the rest), laid out and styled by the plugin's `.css` files. Its `Enabled` shows or hides the tab; closing the tab turns it off.
	function CreateDockWidget(self, id: string, info: DockWidgetInfo?): ScreenGui
	--- A value kept for this plugin in the editor's data (nil, booleans, numbers, strings and tables of them).
	function GetSetting(self, key: string): any
	--- Keeps `value` for this plugin; nil removes it.
	function SetSetting(self, key: string, value: any): ()
	--- The text of a file in the open project, by its path in it. Needs "files" in plugin.json's permissions.
	function ReadProjectFile(self, path: string): string
	--- Writes a text file in the open project, making its folders. Needs "files" in plugin.json's permissions.
	function WriteProjectFile(self, path: string, text: string): ()
	--- The mouse over the Scene View.
	function GetMouse(self): PluginMouse
	--- With true, takes the Scene View's left button: clicks go to the mouse's events instead of selecting.
	function Activate(self, exclusiveMouse: boolean?): ()
	--- Gives the Scene View's left button back.
	function Deactivate(self): ()
	--- Whether the plugin has the Scene View's left button.
	function IsActivated(self): boolean
	--- Draws a line in the Scene View, over the scene, until it's removed. At most 10000 drawings at once.
	function DrawLine(self, from: Vector3, to: Vector3, options: DrawOptions?): PluginDrawing
	--- Draws the edges of a box `size` across, centred on `at` (a CFrame turns it; a Vector3 keeps it lined up with the world).
	function DrawBox(self, at: CFrame | Vector3, size: Vector3, options: DrawOptions?): PluginDrawing
	--- Draws a sphere as three rings round `center`.
	function DrawSphere(self, center: Vector3, radius: number, options: DrawOptions?): PluginDrawing
	--- Draws a label at `position`, the same size however far away (up to 200 characters).
	function DrawText(self, position: Vector3, text: string, options: DrawOptions?): PluginDrawing
	--- Removes everything the plugin drew.
	function ClearDrawings(self): ()
	--- Shows an attribute of the selected Instance as a row of its own in the Properties panel, under the plugin's name. Editing it there sets the attribute as an undo step. At most 64 rows.
	function AddPropertyRow(self, info: PropertyRowInfo): PluginPropertyRow
	--- A button in the Properties panel, under the plugin's name, for the selected Instance.
	function AddPropertyButton(self, info: PropertyButtonInfo): PluginPropertyRow
	--- Imports files of other kinds: their contents go to the importer's Imported event. At most 32 importers and asset actions together.
	function AddImporter(self, info: ImporterInfo): PluginImporter
	--- Adds an item to the Assets panel's right-click menu for the project's assets.
	function AddAssetAction(self, info: AssetActionInfo): PluginAssetAction
	--- API 3. A command, listed in Ctrl+K under the plugin's name, with an optional shortcut. At most 32 commands and Explorer items together.
	function CreateAction(self, info: ActionInfo): PluginAction
	--- API 3. Adds an item to the Explorer's right-click menu.
	function AddExplorerAction(self, info: ExplorerActionInfo): PluginExplorerAction
	--- API 3. Opens a script's code in a tab, with the cursor on `line` if given.
	function OpenScript(self, script: Instance, line: number?): ()
	--- API 3. Shows `text` in a short notice at the bottom of the editor (up to 200 characters).
	function Notify(self, text: string): ()
	--- A handle at `position` the user can drag in the Scene View while editing; the Move snap applies. At most 256 at once.
	function CreateHandle(self, position: Vector3, options: HandleOptions?): PluginHandle
end

--- What's selected in the editor.
declare extern type Selection with
	--- Fires when the editor's selection changes (not for the plugin's own `Set`).
	SelectionChanged: PluginSignal
	--- The selected Instances, the one shown in Properties last.
	function Get(self): { Instance }
	--- Selects these Instances (only ones in the game can be selected).
	function Set(self, instances: { Instance }): ()
end

--- Undo steps for a plugin's edits.
declare extern type ChangeHistoryService with
	--- Ends an undo step named `name`: every edit since the last waypoint undoes as one.
	function SetWaypoint(self, name: string): ()
	--- Undoes the last step, as Edit > Undo does.
	function Undo(self): ()
	--- Redoes the last undone step.
	function Redo(self): ()
end

--- The running plugin.
declare plugin: Plugin
--- What's selected in the editor (also `game:GetService("Selection")`).
declare Selection: Selection
--- Undo steps (also `game:GetService("ChangeHistoryService")`).
declare ChangeHistoryService: ChangeHistoryService
