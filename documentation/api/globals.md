---
description: "The variables and functions every script has."
---

# Globals

The variables and functions every script has.

## game

```luau
game: DataModel
```

The root of the game. Every service is inside it.

## workspace

```luau
workspace: Workspace
```

The service that holds the 3D world (same as `game.Workspace`).

## script

```luau
script: Script | LocalScript | ModuleScript
```

The Script, LocalScript or ModuleScript that is running this code.

## Enum

Every enum, such as `Enum.Material`, `Enum.PartType` and `Enum.KeyCode`.

## require

```luau
require(module: Instance): any
```

Runs a ModuleScript (a `.module.lua` file, such as `require(game.ReplicatedStorage.Utils)`) the first time, and returns what it returned. Later calls on the same side return the same value.

