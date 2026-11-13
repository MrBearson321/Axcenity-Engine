# Data layers

Roadmap R16.2, decisions D1 and D2 of [../research/world-regions-r16.md](../research/world-regions-r16.md). Code: `crates/editor/src/layers.rs` (the view, the sets, the panel) and `app/layers.rs` (its edits); the classes are `core`'s `DataLayerService` and `DataLayer`.

## What a layer is

- A **`DataLayer`** inside the **`DataLayerService`** (the last service of `game`, made after every other one so no other service's ID moved). Its `Name` is a tag: the layer's **members** are the Instances tagged with it (CollectionService), wherever they are in the tree, and everything inside them, so tagging a Model puts all of it in the layer. An Instance can be in several layers.
- `Color` (how the Layers panel marks it) and `ActiveOnStart` (on; off, a game will start with the layer set aside, R16.4) are edited in Properties like any Instance's.
- The service is saved in `world.json` as `data_layer_service`, only when it holds something (`project/src/world_file.rs`). Levels share it, as they share the other services. Layers being Instances, undo, Collaborate and scripts work on them as on anything else.

## The Layers panel

View ▸ Panels ▸ **Layers** (starts closed, beside Properties; also in Ctrl+K). **New Layer** makes `Layer` (or `Layer 2` …) holding what's selected in the Workspace and opens its name box. Each row: the eye, the lock, the colour, the name (click it to select the members), how many Instances carry the tag, and **⋯**: Add Selection, Remove Selection, Layer Settings (selects the layer itself, for Properties), Rename and Delete.

- New Layer, Rename and Delete are one undo step each (`History::record_world`). Rename renames the tag on every member, keeping its place among their tags, and is refused when another layer has the name. Delete takes the tag off every member; the members stay. Add and Remove Selection are one step through `EditorApp::annotate`.
- The panel's edits are off during Play.

## Hidden and locked

Which layers you hide and lock is **your view**, not an edit (decision D2): it's kept in `.axcenity/layers.json` (`LayerView`, this computer's, as `.axcenity/backup.json` is; the file goes once nothing is hidden or locked), never makes an undo step, never marks the project unsaved, and isn't sent to others in Collaborate. Renaming a layer carries its settings over.

- **Hidden**: the members aren't drawn in the viewport (`scene::Streaming::set_also_hidden`, so their meshes leave the GPU as streamed-out ones do), a click goes through them, and their Explorer rows are dimmed with an eye-slash.
- **Locked**: drawn, but a click or drag in the viewport goes through them, as through `Locked` parts (`Viewport::unpickable`, `scene::pick_hit_where`). They can still be picked in the Explorer.
- Play shows every layer: the sets are emptied while playing and worked out again after Stop.
- `EditorApp::update_layer_sets` turns the view into Instances (`LayerSets`) when the tree, a hidden or locked layer's tagged Instances or the view change, before the Explorer and the viewport draw.
- Terrains in a hidden layer are still drawn, but a click goes through them.

## Regions

Roadmap R16.3b, decision D3. A **region** is a Folder or Model at the top of the Workspace: the Layers panel lists them under **Regions**, with an icon (a cube for a Model), the name (click to select it), how many parts are inside, and **Unload in Editor** (the eject button).

- An unloaded region isn't drawn, a click goes through it and its Explorer rows are dimmed, as for a hidden layer: its Instances join `LayerSets::hidden`. It stays in the world and is saved unchanged, so unloading isn't an edit: no undo step, nothing to save.
- Which regions are unloaded is your view, by name, beside the hidden and locked layers in `.axcenity/layers.json` (`LayerView::unloaded`). Two top-level Folders or Models with the same name unload together; renaming one loads it again.
- Play shows every region, as it shows every layer.
- A world with nothing but loose parts at the top of the Workspace has no regions: the panel says to put them in Folders or Models.

## In a game

Roadmap R16.4, decision D5. A layer can be switched off while a game runs: its members are **parked** and come back when it's switched on. Code: `core/src/data_layers.rs` (parking), `script/src/data_layers.rs` (the API).

- `DataLayerService:SetLayerActive(name, active)` (Scripts only: a LocalScript gets an error) switches a layer; `IsLayerActive(name)` reads it on either side; `LayerChanged(name, active)` fires on the server and on every player's device. Each layer's `Active` property is the state; scripts can't set it directly. Blocks: set layer *name* active, layer *name* is active, when layer *name* changes ([../scripting/blocks.md](../scripting/blocks.md)).
- **Parking** moves the topmost tagged Instance of each subtree inside the `DataLayer` itself, remembering its parent and its place among that parent's children (`World`'s parked map). Switching the layer on puts each back there, in order. An Instance tagged with two layers stays parked while either is off. One whose old parent was destroyed meanwhile is destroyed with it.
- Parked Instances are out of the game: not in the Workspace, so not simulated or drawn, and `World::is_server_only` is true for them, so they're never sent to players. The `DataLayer`s themselves are sent, so a player's device has `Active` and hears `LayerChanged`. A device sees parked parts leave (destroyed there) and come back (made again with the same IDs) as normal replication. Scripts already running inside a parked part keep running, as they would in ServerStorage.
- **When a game starts** (Play, the player, `axcenity-server`, game tests), each layer is set to its `ActiveOnStart` before any script runs (`data_layers::start`), so a layer that starts off starts parked.
- **Levels**: a level that replaces the Workspace drops what the old one had parked, and its own members of a switched-off layer are parked as it comes in; adding and removing a level does the same for its Model.
- **Save slots and checkpoints** keep each layer's state in the world state (`WorldState::layers`). Loading switches the layers first, then matches the Workspace with the save, so a part a slot saved as parked isn't made twice.

## Tests

In a game: `core/src/data_layers.rs` (parking keeps parents and order, an Instance in two layers, the start, levels dropping what they parked, a lost parent), `net/src/data_layer_tests.rs` (`a_layer_switched_on_the_server_comes_and_goes_on_a_joined_device`: `LayerChanged` on both sides, the parts leaving and coming back, a LocalScript refused), `script/src/levels_tests.rs` (`a_switched_off_layer_sets_aside_what_a_level_brings_in`, `Active` read-only), `save/src/tests.rs` (`a_slot_keeps_each_data_layers_state`) and the blocks' rows in `blocks/src/tests.rs`.

In the editor: `app/layers_tests.rs` (a new layer holding the selection, rename, delete, add and remove as undo steps, a name already taken, saving and reopening; a hidden layer kept out of the world, the undo history and unsaved changes, kept in `.axcenity/layers.json`, hidden from the viewport and dimmed in the Explorer, locked ones still drawn, a removed tag showing at once, Play showing every layer; `an_unloaded_region_is_set_aside_but_saved_unchanged`), `layers.rs` (`an_unloaded_region_is_hidden_with_everything_inside`) and `viewport/tests.rs` (`a_click_goes_through_hidden_and_locked_layers`; a click going through what's streamed out while editing, in `streaming_while_editing_hides_what_is_far_but_never_the_selection`).
