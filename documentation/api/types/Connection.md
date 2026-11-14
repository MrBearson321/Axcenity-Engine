---
description: "A function connected to an event, returned by Connect."
---

# Connection

A function connected to an event, returned by `Connect`.

### Properties

#### Connected

```luau
Connection.Connected: boolean
```

True until `Disconnect` is called (or the Instance is destroyed).

### Functions

#### Disconnect

```luau
Connection:Disconnect(): ()
```

Stops the function from running when the event fires again. Calling it twice is harmless.

```lua
local connection
connection = workspace.Lava.Touched:Connect(function(hit)
	print(hit.Name, "touched the lava")
	connection:Disconnect() -- only the first touch
end)
```

