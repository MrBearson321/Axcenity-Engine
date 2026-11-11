---
description: "The Tools a character carries, put in the character's Model: up to Capacity Tools as its children, in hotbar order, and Equipped, the one in hand."
---

# Inventory

```luau
Instance.new("Inventory")
```

The Tools a character carries, put in the character's Model: up to Capacity Tools as its children, in hotbar order, and Equipped, the one in hand. The default hotbar shows them; number keys, the mouse wheel (while one is in hand) and the controller bumpers pick one. Styled in CSS as `Inventory` (the bar), `Tool` (each slot, `Tool:checked` in hand) and `Tool::key` (its number). Save slots keep it with the world.

It also has every member of [Instance](Instance.md).

### Properties

#### Capacity

```luau
Inventory.Capacity: number
```

How many Tools it holds. A full Inventory leaves a touched Tool on the ground, and Add returns false.

#### Equipped

```luau
Inventory.Equipped: Instance?
```

The Tool in hand, or nil. Read-only: change it with Equip and UnequipAll.

#### HotbarEnabled

```luau
Inventory.HotbarEnabled: boolean
```

If false, the default hotbar doesn't show and its number keys, wheel and bumpers do nothing, for a game that makes its own. Drop and Primary still work.

### Functions

#### GetItems

```luau
Inventory:GetItems(): {Tool}
```

The Tools in it, in hotbar order, the one in hand included.

#### Add

```luau
Inventory:Add(tool: Tool): boolean
```

Puts `tool` in it, taking it out of any other Inventory first. Returns false, leaving the Tool where it is, when it already holds Capacity Tools; true if it's in it.

#### Remove

```luau
Inventory:Remove(tool: Tool): boolean
```

Takes `tool` out of it (out of the hand first), leaving its Parent nil. Returns false if it wasn't in it.

#### Equip

```luau
Inventory:Equip(tool: Tool): ()
```

Puts `tool`, which must be in it, in the character's right hand, putting away the one there. Fires Unequipped and Equipped, and sets the character's Animator's `Equipped` Bool parameter if it has one.

#### UnequipAll

```luau
Inventory:UnequipAll(): ()
```

Puts away the Tool in hand, if any, firing its Unequipped.

