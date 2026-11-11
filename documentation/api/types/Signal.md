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

