---
description: How a game is one tree of objects, what the services are for, and how to organize, find and lock things.
icon: tree
samples: [hello-part, hello-object-scripts]
---

# Explorer and hierarchy

Everything in an Axcenity game is an **Instance**: a part, a light, a script, a sound, a UI button. Instances live in one tree. The **Explorer** panel shows that tree, starting at `Game`.

## The services

The top of the tree is a fixed set of **services**, each with its own job:

| Service | Holds |
|---|---|
| **Workspace** (`workspace` in scripts) | The 3D world: parts, models, lights, terrain, cameras |
| **Lighting** | The sun, sky, fog, clouds, weather and post effects |
| **ReplicatedStorage** | Things both sides use: ModuleScripts, RemoteEvents, templates to clone |
| **ServerScriptService** | Scripts that run on the server |
| **ServerStorage** | Things only the server sees, such as templates scripts clone |
| **StarterPlayer** | Each player's LocalScripts (StarterPlayerScripts) and character settings |
| **Players** | The players in the game, during Play |
| **SoundService** | Sound groups, effects and reverb |
| **Teams** | Teams, if the game has them |

Services can't be renamed, moved or deleted. Anything else can be moved by dragging its row onto another row; rows you can't drop on show why.

## Organize with Models and Folders

- A **Model** groups the parts of one thing, such as a car or a house. Scripts move a whole Model with `Model:PivotTo()` and read where it is with `Model:GetPivot()`.
- A **Folder** groups anything for tidiness and has no effect on the game.

Insert either from the Explorer's right-click **Insert** menu, then drag rows onto it. New objects go into the selected Model or Folder. Clicking a part in the Scene selects the part itself, not its Model.

## Find things

- The **Filter** box at the top of the Explorer shows only what matches a name or class, with the branches above it opened.
- Once the Explorer has the keyboard, the arrow keys move the selection and open and close branches, **Enter** renames, and **Home** and **End** jump to the ends.
- **Ctrl+K** finds any object by name or path from anywhere in the editor.
- In the Console, `find Door` selects every object whose name contains "Door", and `select Workspace.House.Door` selects one by its path.

## Rename, duplicate, delete, lock

Right-click a row for Insert, Rename (**F2**), Duplicate (**Ctrl+D**), Delete, **Lock** and Unlock, Export Package and, on a placed prefab, the Prefab menu. A **locked** object can't be clicked in the Scene, moved, duplicated or deleted until it's unlocked, which keeps a floor or a finished building safe. New names never clash with a sibling's: copies of `Pad` are `Pad2`, `Pad3` and so on.

## Prefabs

A **prefab** is a saved object (with everything inside it) you can place many times. Every placed copy follows the prefab; a property changed on one copy is an **override**, shown in bold in Properties with a ↺ to revert it. A copy's **Prefab** menu applies its overrides to the prefab, reverts them, or unpacks it into ordinary objects. Prefabs are files in `assets/prefabs/` and show in the Assets panel.

## Scripts inside objects

A Script, LocalScript or ModuleScript can live inside the object it controls, such as a door's script inside the door. Its code is a file in `objects/`, shared by every copy, so each copy of a prefab runs the same file with its own `script.Parent`. The **hello-object-scripts** sample has two doors from one prefab.
