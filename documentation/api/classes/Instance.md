---
description: "Anything in the game tree."
---

# Instance

Anything in the game tree. Every class below is a kind of Instance.

### Properties

#### Name

```luau
Instance.Name: string
```

The Instance's name. Children can be found by name: `workspace.Block`.

#### ClassName

```luau
Instance.ClassName: string
```

The Instance's class, such as `"Part"`. Read-only.

#### Parent

```luau
Instance.Parent: Instance?
```

The Instance this one is inside, or nil. Setting it moves the Instance.

```lua
local coin = workspace.Coin:Clone()
coin.Position = Vector3.new(10, 3, 0)
coin.Parent = workspace -- it appears once it has a parent
```

#### StreamingState

```luau
Instance.StreamingState: EnumItem
```

Whether this Instance is on this device: Enum.StreamingState.StreamedIn, or StreamedOut for one world streaming took away (or a Model whose contents are away). Works on an Instance that isn't there, so it never errors. Read-only.

### Events

#### Changed

```luau
Instance.Changed: Signal
```

Fires when a property (or `Name` or `Parent`) gets a different value, whether a script, physics or the editor changed it. The handler gets the property's name: `function(property) ... end`.

```lua
workspace.Door.Changed:Connect(function(property)
	print("Door changed:", property)
end)
```

#### ChildAdded

```luau
Instance.ChildAdded: Signal
```

Fires when an Instance is put inside this one. The handler gets the child.

```lua
workspace.ChildAdded:Connect(function(child)
	print(child.Name, "was added to the Workspace")
end)
```

#### ChildRemoved

```luau
Instance.ChildRemoved: Signal
```

Fires when an Instance is taken out of this one. The handler gets the child.

#### DescendantAdded

```luau
Instance.DescendantAdded: Signal
```

Fires when an Instance is put anywhere inside this one, once for it and once for each Instance inside it. The handler gets that descendant.

#### DescendantRemoving

```luau
Instance.DescendantRemoving: Signal
```

Fires when an Instance is taken out from anywhere inside this one, once for it and once for each Instance inside it. The handler gets that descendant.

#### AncestryChanged

```luau
Instance.AncestryChanged: Signal
```

Fires when this Instance or one of its ancestors gets a new Parent. The handler gets the Instance that moved and its new Parent (nil when it was taken out or destroyed).

#### Destroying

```luau
Instance.Destroying: Signal
```

Fires once when `Destroy` is called on this Instance or one of its ancestors, while it can still be read.

```lua
workspace.Bomb.Destroying:Connect(function()
	print("The bomb is gone")
end)
```

#### StreamedIn

```luau
Instance.StreamedIn: Signal
```

Fires on a device when world streaming brings this Instance back, or a Model's contents arrive. Connect to it on a Model that's still away, or a part kept from before it went.

#### AttributeChanged

```luau
Instance.AttributeChanged: Signal
```

Fires when an attribute gets a different value or is removed. The handler gets the attribute's name.

### Functions

#### FindFirstChild

```luau
Instance:FindFirstChild(name: string, recursive: boolean?): Instance?
```

The first child with this name, or nil. With `recursive`, searches every descendant.

```lua
local door = workspace:FindFirstChild("Door")
if door then
	door.Transparency = 0.5
end
```

#### GetPropertyChangedSignal

```luau
Instance:GetPropertyChangedSignal(property: string): Signal
```

An event that fires when this one property (or `Name` or `Parent`) gets a different value. The handler gets nothing: read the property to see its new value.

```lua
local lamp = workspace.Lamp
lamp:GetPropertyChangedSignal("Color"):Connect(function()
	print("The lamp is now", lamp.Color)
end)
```

#### FindFirstChildOfClass

```luau
Instance:FindFirstChildOfClass(className: string): Instance?
```

The first child whose class is exactly `className`, or nil.

```lua
local character = workspace:FindFirstChild("Hero")
local humanoid = character and character:FindFirstChildOfClass("Humanoid")
if humanoid then
	humanoid.WalkSpeed = 24
end
```

#### FindFirstChildWhichIsA

```luau
Instance:FindFirstChildWhichIsA(className: string, recursive: boolean?): Instance?
```

The first child that `IsA(className)`, or nil. With `recursive`, searches every descendant.

#### FindFirstAncestor

```luau
Instance:FindFirstAncestor(name: string): Instance?
```

The nearest Instance above this one with this name, or nil.

#### FindFirstAncestorOfClass

```luau
Instance:FindFirstAncestorOfClass(className: string): Instance?
```

The nearest Instance above this one whose class is exactly `className`, or nil.

```lua
workspace.Lava.Touched:Connect(function(hit)
	local model = hit:FindFirstAncestorOfClass("Model")
	if model then
		print(model.Name, "touched the lava")
	end
end)
```

#### FindFirstAncestorWhichIsA

```luau
Instance:FindFirstAncestorWhichIsA(className: string): Instance?
```

The nearest Instance above this one that `IsA(className)`, or nil.

#### WaitForChild

```luau
Instance:WaitForChild(name: string, timeout: number?): Instance?
```

The child with this name, waiting (as `task.wait` does) until there is one. With `timeout`, gives nil after that many seconds. Without one, warns once after 5 seconds and keeps waiting.

```lua
local gui = workspace:WaitForChild("MainMenu", 10)
if gui == nil then
	print("The menu never arrived")
end
```

#### GetChildren

```luau
Instance:GetChildren(): {Instance}
```

The Instances directly inside this one, in order.

```lua
for _, child in workspace.Coins:GetChildren() do
	child.Transparency = 0
end
```

#### GetDescendants

```luau
Instance:GetDescendants(): {Instance}
```

Every Instance inside this one, parents before their children.

```lua
for _, item in workspace.House:GetDescendants() do
	if item:IsA("BasePart") then
		item.Anchored = true
	end
end
```

#### IsA

```luau
Instance:IsA(className: string): boolean
```

Whether this Instance's class is `className` or inherits from it: a Part and a MeshPart are both a `"BasePart"`, and everything is an `"Instance"`.

```lua
for _, thing in workspace:GetDescendants() do
	if thing:IsA("Part") then
		thing.Anchored = true
	end
end
```

#### IsDescendantOf

```luau
Instance:IsDescendantOf(ancestor: Instance): boolean
```

Whether this Instance is somewhere inside `ancestor`.

#### IsAncestorOf

```luau
Instance:IsAncestorOf(descendant: Instance): boolean
```

Whether `descendant` is somewhere inside this Instance.

#### GetFullName

```luau
Instance:GetFullName(): string
```

The names from the service down to this Instance, joined by dots, such as `"Workspace.Model.Part"`.

#### Clone

```luau
Instance:Clone(): Instance
```

A copy of this Instance and everything inside it, with no Parent. A copied Script starts once it's put where scripts run (the Workspace or ServerScriptService; a LocalScript also in StarterPlayerScripts or Players). Services can't be cloned.

```lua
local copy = workspace.Crate:Clone()
copy.Position = workspace.Crate.Position + Vector3.new(0, 5, 0)
copy.Parent = workspace
```

#### Destroy

```luau
Instance:Destroy(): ()
```

Removes this Instance and everything inside it for good, firing `Destroying` first. Its Parent can't be set again, and a script inside it stops. Services can't be destroyed; destroying twice is harmless.

```lua
local coin = workspace.Coin
coin.Touched:Connect(function()
	coin:Destroy()
end)
```

#### ClearAllChildren

```luau
Instance:ClearAllChildren(): ()
```

Destroys every child of this Instance (except services).

```lua
-- Empty the folder of last round's coins.
workspace.Coins:ClearAllChildren()
```

#### SetAttribute

```luau
Instance:SetAttribute(name: string, value: any): ()
```

Sets the attribute `name` to `value`: a boolean, number, string, Vector3, Vector2, Color3, UDim2, CFrame or EnumItem; nil removes it. Names are 1 to 100 letters, digits and underscores. Attributes are saved with the world, in prefabs and in save slots, and copied by `Clone`.

```lua
local chest = workspace.Chest
chest:SetAttribute("Gold", 50)
print(chest:GetAttribute("Gold"))
```

#### GetAttribute

```luau
Instance:GetAttribute(name: string): any
```

The value of the attribute `name`, or nil if it has none.

```lua
local door = workspace.Door
local key = door:GetAttribute("KeyColor") or "Red"
print("This door needs the", key, "key")
```

#### GetAttributes

```luau
Instance:GetAttributes(): { [string]: any }
```

Every attribute, as a table from name to value.

#### GetAttributeChangedSignal

```luau
Instance:GetAttributeChangedSignal(name: string): Signal
```

An event that fires when the attribute `name` gets a different value or is removed. The handler gets nothing: read it with `GetAttribute`.

```lua
local door = workspace.Door
door:GetAttributeChangedSignal("Open"):Connect(function()
	door.CanCollide = not door:GetAttribute("Open")
end)
```

#### AddTag

```luau
Instance:AddTag(tag: string): ()
```

Gives this Instance the tag `tag` (same as `CollectionService:AddTag`). Adding a tag it has does nothing.

#### RemoveTag

```luau
Instance:RemoveTag(tag: string): ()
```

Takes the tag `tag` off this Instance (same as `CollectionService:RemoveTag`).

#### HasTag

```luau
Instance:HasTag(tag: string): boolean
```

Whether this Instance has the tag `tag`.

#### GetTags

```luau
Instance:GetTags(): {string}
```

This Instance's tags, in the order they were added.

#### new

```luau
Instance.new(className: string): Instance
```

Creates an Instance of this class with default properties and no parent. Set `Parent` to put it in the world.

```lua
local part = Instance.new("Part")
part.Size = Vector3.new(4, 1, 4)
part.Position = Vector3.new(0, 10, 0)
part.Parent = workspace
```

