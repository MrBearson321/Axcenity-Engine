---
description: "The service that holds the project's DataLayers (roadmap R16)."
---

# DataLayerService

```luau
game.DataLayerService
```

The service that holds the project's DataLayers (roadmap R16). Every level shares them. In a game, a switched-off layer's Instances are set aside inside the layer: not simulated, drawn or sent to players. SetLayerActive (Scripts only) switches a layer, IsLayerActive reads it and LayerChanged reports it on every device. A loaded level's own members of a switched-off layer are set aside too, and save slots keep each layer's state.

It also has every member of [Instance](Instance.md).

### Events

#### LayerChanged

```luau
DataLayerService.LayerChanged: Signal
```

Fires with the layer's name and whether it's now on, after SetLayerActive switched it (or a save slot loaded with it the other way). On both sides: a player's device hears it once the change and the parts arrive.

### Functions

#### SetLayerActive

```luau
DataLayerService:SetLayerActive(name: string, active: boolean): ()
```

Scripts only (roadmap R16.4): switches the layer `name` on or off. Off, each topmost Instance tagged with it moves inside the layer, out of the game; on, each goes back under its old parent at its old place (one whose parent was destroyed meanwhile goes with it). An Instance in two layers stays out while either is off. Errors if there's no such layer.

#### IsLayerActive

```luau
DataLayerService:IsLayerActive(name: string): boolean
```

True if the layer `name` is on (its Active). Works on both sides. Errors if there's no such layer.

