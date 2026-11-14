---
description: "An event scripts on the same side fire for each other: Fire(...) fires Event with the same values."
---

# BindableEvent

```luau
Instance.new("BindableEvent")
```

An event scripts on the same side fire for each other: `Fire(...)` fires `Event` with the same values. It doesn't cross between Scripts and LocalScripts.

It also has every member of [Instance](Instance.md).

### Events

#### Event

```luau
BindableEvent.Event: Signal
```

Fires when a script on the same side calls `Fire`. The handler gets the same arguments (tables aren't copied).

### Functions

#### Fire

```luau
BindableEvent:Fire(...: any): ()
```

Fires `Event` with the arguments, for scripts on this side.

```lua
local roundEnded = game.ServerStorage.RoundEnded
roundEnded.Event:Connect(function(winner)
	print(winner, "won the round")
end)
roundEnded:Fire("Red team")
```

