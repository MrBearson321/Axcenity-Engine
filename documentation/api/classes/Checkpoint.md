---
description: "A place in the world to come back to."
---

# Checkpoint

```luau
Instance.new("Checkpoint")
```

A place in the world to come back to. `SaveService:SetCheckpoint(checkpoint)` makes it the active one and captures the player data and world state there; `SaveService:RestoreCheckpoint()` puts them back and stands the player character at its Position. A save slot keeps the active checkpoint.

It also has every member of [Instance](Instance.md).

### Properties

#### Position

```luau
Checkpoint.Position: Vector3
```

Where SaveService:RestoreCheckpoint stands the player character: the middle of its feet goes here.

