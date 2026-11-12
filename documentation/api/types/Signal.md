---
description: "An event, such as part.Touched."
---

# Signal

An event, such as `part.Touched`. Connect a function to run it every time the event fires.

### Functions

#### Connect

```luau
Signal:Connect(callback: (...any) -> ()): Connection
```

Runs `callback` every time the event fires, each time as its own task (so it may `task.wait`). Returns a Connection for stopping it.

```lua
local connection = workspace.Button.Touched:Connect(function()
	print("Pressed")
end)
task.wait(10)
connection:Disconnect()
```

#### Once

```luau
Signal:Once(callback: (...any) -> ()): Connection
```

Runs `callback` the next time the event fires, then disconnects it, as its own task. Returns a Connection, so it can be stopped before then.

#### Wait

```luau
Signal:Wait(): ...any
```

Pauses the script until the event next fires, and returns the values the event gave its handlers: `local otherPart = part.Touched:Wait()`. A script that stops while waiting never carries on.

