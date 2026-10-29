# Engine docs

Open **one** file for the task. The pages in this folder that share a name with a topic (`architecture.md`, `scripting.md`, `rendering.md`, `project-and-export.md`) are maps. The detail is in the file they name.

Source and tests win when a doc is wrong. Update the topic file you used.

| If you are changing… | Open |
|---|---|
| A crate boundary or a new dependency | [architecture/crates.md](architecture/crates.md) |
| Instances and IDs | [architecture/data-model.md](architecture/data-model.md) |
| The frame loop | [architecture/frame-loop.md](architecture/frame-loop.md) |
| Physics | [architecture/physics.md](architecture/physics.md) |
| Fractures, the `mesh_edit` crate, or plane cutting | [architecture/fracture.md](architecture/fracture.md) |
| Union, Negate, Intersect, Separate, `UnionOperation`, `CollisionFidelity` | [architecture/solid-modeling.md](architecture/solid-modeling.md) |
| The navigation mesh, agents, or behavior trees | [architecture/navigation.md](architecture/navigation.md) |
| Play and Stop | [architecture/play-mode.md](architecture/play-mode.md) |
| Save slots, checkpoints, `SaveService` | [architecture/saves.md](architecture/saves.md) |
| Multiplayer: the `net` crate, `axcenity-server`, `NetworkService`, hosting, joining, LAN discovery | [architecture/networking.md](architecture/networking.md) |
| Levels, `LevelService`, the Levels panel | [project/levels.md](project/levels.md) |
| Cutscene sequences, the Sequence Instance, the Sequencer tab | [project/sequences.md](project/sequences.md) |
| Platforms or file access | [architecture/portability.md](architecture/portability.md) |
| Build speed, test commands, or nextest settings | [development.md](development.md) |
| The editor window, ribbon, or theme | [editor/shell.md](editor/shell.md) |
| Selection or Properties | [editor/selection.md](editor/selection.md) |
| Move, scale, rotate, or undo | [editor/transforms.md](editor/transforms.md) |
| The Explorer tree | [editor/tree.md](editor/tree.md) |
| Script tabs, file watching, or rebuilding or hot-patching the engine | [editor/scripts.md](editor/scripts.md) |
| Model import or the Animation panel | [editor/import.md](editor/import.md) |
| The Model Studio tab, editable meshes (`EditMesh`, `ModelDoc`), modeling operations (`MeshOp`), modifiers and the `.meshedit.json` sidecar, Generate LODs, saving `.glb` | [editor/model-studio.md](editor/model-studio.md) |
| Paint, materials, lights, or the angular tool | [editor/building.md](editor/building.md) |
| The Assets panel | [editor/assets.md](editor/assets.md) |
| Scripts, LocalScripts, or RemoteEvents in the editor | [editor/sides.md](editor/sides.md) |
| Lua, the sandbox, blocks, or graphs | [scripting.md](scripting.md), then one file |
| Drawing, materials, lights, or animation playback | [rendering.md](rendering.md), then one file |
| Project files, export, licenses, or GitHub releases | [project-and-export.md](project-and-export.md), then one file |
| Data tables, the Data Table tab, or `DataService` | [project/data-tables.md](project/data-tables.md) |
| Game tests (`tests/*.spec.lua`), the Tests panel, `axcenity-test` | [project/game-tests.md](project/game-tests.md) |
| Editor plugins (`plugins/`, the plugin API, the Plugins panel) | [editor/plugins.md](editor/plugins.md) |
| In-game UI, the string table and languages | [ui.md](ui.md) |
| Terrain algorithms | [terrain.md](terrain.md) |

Product scope is one story in `prd/`. The roadmap is one phase in `plans/`. Don't open either file in full.
