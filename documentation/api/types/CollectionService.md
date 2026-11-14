---
description: "Tags on Instances, from game:GetService(\"CollectionService\")."
---

# CollectionService

Tags on Instances, from `game:GetService("CollectionService")`. An Instance counts as tagged while it is in the game. Tags are saved with the world, in prefabs and in save slots, and copied by `Clone`; the editor's Tags panel lists them.

### Functions

#### AddTag

```luau
CollectionService:AddTag(instance: Instance, tag: string): ()
```

Gives `instance` the tag `tag`. Adding a tag it has does nothing.

```lua
local CollectionService = game:GetService("CollectionService")
CollectionService:AddTag(workspace.Spikes, "Hazard")
for _, hazard in CollectionService:GetTagged("Hazard") do
	hazard.Color = Color3.fromRGB(255, 0, 0)
end
```

#### RemoveTag

```luau
CollectionService:RemoveTag(instance: Instance, tag: string): ()
```

Takes the tag `tag` off `instance`.

#### HasTag

```luau
CollectionService:HasTag(instance: Instance, tag: string): boolean
```

Whether `instance` has the tag `tag`.

#### GetTags

```luau
CollectionService:GetTags(instance: Instance): {string}
```

The tags of `instance`, in the order they were added.

#### GetTagged

```luau
CollectionService:GetTagged(tag: string): {Instance}
```

Every Instance in the game with the tag `tag`, parents before their children.

```lua
local CollectionService = game:GetService("CollectionService")
for _, lamp in CollectionService:GetTagged("Lamp") do
	lamp.Material = Enum.Material.Neon
end
```

#### GetAllTags

```luau
CollectionService:GetAllTags(): {string}
```

Every tag an Instance in the game has, sorted.

#### GetInstanceAddedSignal

```luau
CollectionService:GetInstanceAddedSignal(tag: string): Signal
```

An event that fires with an Instance when it gets the tag `tag` while in the game, or joins the game with it (such as a tagged clone put in the Workspace).

```lua
local CollectionService = game:GetService("CollectionService")
CollectionService:GetInstanceAddedSignal("Coin"):Connect(function(coin)
	coin.Transparency = 0
end)
```

#### GetInstanceRemovedSignal

```luau
CollectionService:GetInstanceRemovedSignal(tag: string): Signal
```

An event that fires with an Instance when it loses the tag `tag`, or leaves the game with it (taken out, or destroyed).

