---
description: "Something a character carries: a Model of parts with a Handle."
---

# Tool

```luau
Instance.new("Tool")
```

Something a character carries: a Model of parts with a Handle. Touching a dropped Tool puts it in the character's Inventory, if there's room; equipping it holds its Grip in the right hand (a skinned character's right hand bone, a part named RightHand, or beside the body). Activated fires on the Primary action while it's in hand. Drop with the Drop action (Backspace, or Y) or tool:Drop().

It also has every member of [Instance](Instance.md).

### Properties

#### Handle

```luau
Tool.Handle: Instance?
```

The part held in the hand and touched to pick the Tool up. nil uses its child part named "Handle".

#### Grip

```luau
Tool.Grip: Instance?
```

The Attachment in the Handle that's put in the right hand, turned as the hand is. nil uses the Handle's Attachment named "Grip", else the Handle's middle.

#### RequiresHandle

```luau
Tool.RequiresHandle: boolean
```

If false, a Tool with no Handle can be carried and equipped (by scripts and the hotbar); it's never picked up by touch.

#### CanBeDropped

```luau
Tool.CanBeDropped: boolean
```

If false, the Drop action and tool:Drop() leave it carried.

#### ToolTip

```luau
Tool.ToolTip: string
```

Text the hotbar shows over the bar while it's in hand.

#### TextureId

```luau
Tool.TextureId: string
```

The image in its hotbar slot, such as "assets/textures/sword.png". Empty shows its Name.

#### Item

```luau
Tool.Item: string
```

The data-table row it is, as "table:key" (such as "weapons:sword"), for scripts to read with DataService. Save slots keep it. Empty for none.

### Events

#### Equipped

```luau
Tool.Equipped: Signal
```

Fires when it goes into the hand: Equip, the hotbar, or a save slot loading with it in hand.

#### Unequipped

```luau
Tool.Unequipped: Signal
```

Fires when it leaves the hand: put away, another equipped, dropped or removed.

#### Activated

```luau
Tool.Activated: Signal
```

Fires when the Primary action (the left mouse button, or RT) is pressed while it's in the player character's hand, such as to swing a sword.

#### Deactivated

```luau
Tool.Deactivated: Signal
```

Fires when the Primary action is let go after Activated, or the Tool leaves the hand first.

### Functions

#### ApplyEffect

```luau
Tool:ApplyEffect(effect: Effect): Effect?
```

Scripts only: puts a copy of the Effect `effect` on this Model and returns it, or adds a stack to the copy already here (up to its MaxStacks) and starts its Duration over. An instant Effect changes at once and returns nil.

#### Drop

```luau
Tool:Drop(): boolean
```

Takes it out of its Inventory (and the hand) and puts its Handle on the ground in front of the character, in the Workspace. The character picks it up again once it has stepped off it and touches it. Returns false if it's not carried or CanBeDropped is off.

#### GetPivot

```luau
Tool:GetPivot(): CFrame
```

Where it turns and moves from: a part's own CFrame, or for a Model the centre of the box around its parts, unturned.

#### PivotTo

```luau
Tool:PivotTo(target: CFrame): ()
```

Moves and turns it so its pivot is at `target`. A Model's parts all move together and keep their places relative to each other.

