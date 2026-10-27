# Engine docs

Open **one** file for the task. The pages in this folder that share a name with a topic (`architecture.md`, `scripting.md`, `rendering.md`, `project-and-export.md`) are maps. The detail is in the file they name.

Source and tests win when a doc is wrong. Update the topic file you used.

| If you are changing… | Open |
|---|---|
| A crate boundary or a new dependency | [architecture/crates.md](architecture/crates.md) |
| Instances and IDs | [architecture/data-model.md](architecture/data-model.md) |
| The frame loop | [architecture/frame-loop.md](architecture/frame-loop.md) |
| Physics | [architecture/physics.md](architecture/physics.md) |
| The navigation mesh, agents, or behavior trees | [architecture/navigation.md](architecture/navigation.md) |
| Play and Stop | [architecture/play-mode.md](architecture/play-mode.md) |
| Save slots, checkpoints, `SaveService` | [architecture/saves.md](architecture/saves.md) |
| Platforms or file access | [architecture/portability.md](architecture/portability.md) |
| The editor window, ribbon, or theme | [editor/shell.md](editor/shell.md) |
| Selection or Properties | [editor/selection.md](editor/selection.md) |
| Move, scale, rotate, or undo | [editor/transforms.md](editor/transforms.md) |
| The Explorer tree | [editor/tree.md](editor/tree.md) |
| Script tabs, file watching, or rebuilding the engine | [editor/scripts.md](editor/scripts.md) |
| Model import or the Animation panel | [editor/import.md](editor/import.md) |
| Paint, materials, lights, or the angular tool | [editor/building.md](editor/building.md) |
| The Assets panel | [editor/assets.md](editor/assets.md) |
| Scripts, LocalScripts, or RemoteEvents in the editor | [editor/sides.md](editor/sides.md) |
| Lua, the sandbox, blocks, or graphs | [scripting.md](scripting.md), then one file |
| Drawing, materials, lights, or animation playback | [rendering.md](rendering.md), then one file |
| Project files, export, licenses, or GitHub releases | [project-and-export.md](project-and-export.md), then one file |
| In-game UI, the string table and languages | [ui.md](ui.md) |
| Terrain algorithms | [terrain.md](terrain.md) |

Product scope is one story in `prd/`. The roadmap is one phase in `plans/`. Don't open either file in full.
