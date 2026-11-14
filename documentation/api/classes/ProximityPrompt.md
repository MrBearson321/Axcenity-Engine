---
description: "A prompt to do something, under a part or an Attachment."
---

# ProximityPrompt

```luau
Instance.new("ProximityPrompt")
```

A prompt to do something, under a part or an Attachment. When the player character is within MaxActivationDistance (and, with RequiresLineOfSight, nothing solid is between them), the nearest prompt for its Action shows ActionText · ObjectText over it with the bound key or controller button, and holding that action for HoldDuration seconds fires Triggered. Styled in CSS as `ProximityPrompt`, with `::key` and `::ring`.

It also has every member of [Instance](Instance.md).

### Properties

#### ActionText

```luau
ProximityPrompt.ActionText: string
```

What it does, such as "Open", shown in the prompt before ObjectText.

#### ObjectText

```luau
ProximityPrompt.ObjectText: string
```

What it does it to, such as "Door": the prompt shows "Open · Door". Empty shows ActionText alone.

#### Action

```luau
ProximityPrompt.Action: string
```

The input map action that uses it: Interact (E, or X on a controller) by default. The prompt shows the key, or the controller button once a controller was used last, with the player's rebinds.

#### HoldDuration

```luau
ProximityPrompt.HoldDuration: number
```

Seconds the action is held before Triggered fires; the prompt's ring fills meanwhile. 0 triggers on the press.

```lua
local prompt = workspace.Chest.ProximityPrompt
prompt.ActionText = "Open"
prompt.HoldDuration = 1.5 -- hold the key for a second and a half
prompt.Triggered:Connect(function(player)
	print(player.Name, "opened the chest")
end)
```

#### MaxActivationDistance

```luau
ProximityPrompt.MaxActivationDistance: number
```

Studs from the middle of the player character (or the camera, with no character) it shows within.

#### RequiresLineOfSight

```luau
ProximityPrompt.RequiresLineOfSight: boolean
```

If true, it doesn't show while a part that can be hit by queries is between it and the character. Its own part and the character don't count.

#### Enabled

```luau
ProximityPrompt.Enabled: boolean
```

If false, it never shows, and one showing hides.

#### Exclusivity

```luau
ProximityPrompt.Exclusivity: EnumProximityPromptExclusivity | "OnePerButton" | "OneGlobally" | "AlwaysShow"
```

`Enum.ProximityPromptExclusivity`: OnePerButton shows the nearest prompt for each action, OneGlobally the nearest of all, and AlwaysShow shows this one whenever it's in reach.

#### Dialogue

```luau
ProximityPrompt.Dialogue: string
```

A dialogue file (`assets/dialogue/Farmer.dialogue.json`) it starts when triggered, with no script, said by the Model it's in. Empty starts none.

### Events

#### Triggered

```luau
ProximityPrompt.Triggered: Signal
```

Fires with the Player when the action is pressed on a showing prompt (HoldDuration 0) or held for HoldDuration. Once per press: hold on and it doesn't fire again.

```lua
local prompt = workspace.Door.ProximityPrompt
prompt.Triggered:Connect(function(player)
	workspace.Door.CanCollide = false
end)
```

#### TriggerEnded

```luau
ProximityPrompt.TriggerEnded: Signal
```

Fires with the Player when the action that triggered it is let go, or the prompt hides.

#### PromptShown

```luau
ProximityPrompt.PromptShown: Signal
```

Fires when the prompt starts showing.

#### PromptHidden

```luau
ProximityPrompt.PromptHidden: Signal
```

Fires when the prompt stops showing: out of reach, out of sight, Enabled off, or another is nearer.

#### PromptButtonHoldBegan

```luau
ProximityPrompt.PromptButtonHoldBegan: Signal
```

Fires with the Player when the action is pressed on a showing prompt whose HoldDuration is above 0.

#### PromptButtonHoldEnded

```luau
ProximityPrompt.PromptButtonHoldEnded: Signal
```

Fires with the Player when a hold ends: let go early, the prompt hid, or HoldDuration was reached (just before Triggered).

