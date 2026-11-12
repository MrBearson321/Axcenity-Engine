---
description: The Instance tree, services, properties, attributes, tags, events and connections, and how to find and change objects from scripts.
icon: tree
samples: [hello-events, hello-object-scripts]
---

# Instances, properties and events

Everything in a game is an **Instance**: parts, models, lights, sounds, scripts, UI and the services themselves. Instances form one tree, rooted at `game`.

## The tree

`game` holds the **services**, created by the engine:

| Service | Holds |
|---|---|
| Workspace (`workspace`) | The 3D world: everything that's drawn and simulated. |
| Players | A Player for everyone in the game, during Play. |
| Lighting | The sky, sun, fog and post effects. |
| ReplicatedStorage | Things both the server and players' scripts can reach: RemoteEvents, ModuleScripts, templates. |
| ServerScriptService | Scripts. Never sent to players. |
| ServerStorage | Things only the server can reach. |
| StarterPlayer, StarterPlayerScripts | Character settings, and LocalScripts each player runs. |
| SoundService | Sound groups and audio settings. |
| Teams | Teams, when the game has them. |

Other services aren't in the tree but come from `game:GetService()`, such as TweenService and UserInputService. See [Services and globals](services.md).

## Properties

Each class has its own properties, with defaults. Read and write them with a dot:

```lua
local part = workspace.Crate
part.Anchored = true
print(part.Name, part.ClassName)
```

Every Instance has `Instance.Name`, `Instance.ClassName` and `Instance.Parent`. Setting **Parent** moves the Instance in the tree; setting it to `nil` takes it out of the game. `Instance:Destroy()` removes it for good.

Names with spaces or dots need brackets: `workspace["Red Ball"]`. A name that might not be there yet is safer with `Instance:FindFirstChild()` (gives `nil`) or `Instance:WaitForChild()` (waits).

## Finding Instances

| To find | Use |
|---|---|
| A child by name | `Instance:FindFirstChild()`, `Instance:WaitForChild()` |
| A child by class | `Instance:FindFirstChildOfClass()`, `Instance:FindFirstChildWhichIsA()` |
| Everything inside | `Instance:GetChildren()`, `Instance:GetDescendants()` |
| What it's inside | `Instance:FindFirstAncestor()`, `Instance:IsDescendantOf()` |
| Its class | `Instance:IsA()` |
| Everything with a tag | `CollectionService:GetTagged()` |

## Attributes and tags

**Attributes** are your own named values on any Instance: `Instance:SetAttribute()` and `Instance:GetAttribute()`, shown in the Properties panel's Attributes section. **Tags** are labels: `Instance:AddTag()`, `Instance:HasTag()`, and `CollectionService:GetTagged()` to find every Instance with one. The editor's **Tags** panel adds and removes them.

```lua
local CollectionService = game:GetService("CollectionService")
for _, coin in CollectionService:GetTagged("Coin") do
	coin:SetAttribute("Value", 10)
end
```

## Events

Events are signals you connect a function to. `Signal:Connect()` returns a **Connection**; call `Connection:Disconnect()` to stop listening.

| Event | Fires when |
|---|---|
| `Instance.Changed` | Any property changes (with its name). |
| `Instance:GetPropertyChangedSignal()` | One property changes. |
| `Instance.ChildAdded`, `Instance.ChildRemoved` | A child is added or removed. |
| `Instance.DescendantAdded`, `Instance.DescendantRemoving` | Anything inside is added or is being removed. |
| `Instance.AttributeChanged` | An attribute changes. |
| `Instance.Destroying` | It's about to be destroyed. |

```lua
local door = workspace.Door
local connection = door:GetPropertyChangedSignal("Transparency"):Connect(function()
	print("The door is now", door.Transparency)
end)
task.wait(10)
connection:Disconnect()
```

Each connected function runs as its own coroutine, in the order they were connected, so it may `task.wait()` without holding up the others. Change events (`Changed`, `ChildAdded` and the rest) are delivered just after the change, in the order things changed.

`Signal:Once()` connects a function for the next time only. `Signal:Wait()` pauses the script until the event next fires and gives back what the event passed:

```lua
local pad = workspace.Pad
local otherPart = pad.Touched:Wait()
print(otherPart.Name, "stepped on the pad")

pad:GetAttributeChangedSignal("Locked"):Once(function()
	print("Locked changed for the first time:", pad:GetAttribute("Locked"))
end)
```

`game:GetService("Players")` gets a service; `game:FindService(name)` is the same but gives `nil` instead of an error when there's no such service.

Block scripts have the same tools: **when *property* of *object* changes** and **when attribute *name* of *object* changes** (Events), **wait until *object* *event* fires** (Control), and **set attribute** and **attribute *name* of *object*** (Variables).

## Copies and prefabs

`Instance:Clone()` copies an Instance with everything inside it; set the copy's **Parent** to place it. A **prefab** is a saved object you can place many times as linked copies (see [Assets: import and manage](../learn/assets.md)).
