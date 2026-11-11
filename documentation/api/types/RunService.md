---
description: "The game's frame loop, from game:GetService(\"RunService\")."
---

# RunService

The game's frame loop, from `game:GetService("RunService")`.

### Properties

#### Heartbeat

```luau
RunService.Heartbeat: Signal
```

Fires once every frame, after physics. The handler gets the frame's length in seconds: `function(deltaTime) ... end`.

```lua
local RunService = game:GetService("RunService")
local spinner = workspace.Spinner
RunService.Heartbeat:Connect(function(dt)
	spinner.CFrame = spinner.CFrame * CFrame.Angles(0, dt, 0)
end)
```

#### Stepped

```luau
RunService.Stepped: Signal
```

Fires once every frame, before physics. The handler gets the game time and the frame's length, in seconds: `function(time, deltaTime) ... end`.

#### RenderStepped

```luau
RunService.RenderStepped: Signal
```

Fires once every frame just before it's drawn, after the functions bound with `BindToRenderStep`. The handler gets the frame's length in seconds. LocalScripts only. LocalScripts only.

### Functions

#### BindToRenderStep

```luau
RunService:BindToRenderStep(name: string, priority: number, callback: (number) -> ()): ()
```

Runs `callback` with the frame's length every frame just before it's drawn, in `priority` order with the others bound (lowest first; `Enum.RenderPriority.Camera.Value` is 200). Binding a name again replaces it. LocalScripts only. LocalScripts only.

#### UnbindFromRenderStep

```luau
RunService:UnbindFromRenderStep(name: string): ()
```

Stops what was bound as `name`. LocalScripts only. LocalScripts only.

#### IsServer

```luau
RunService:IsServer(): boolean
```

Whether this is a Script, on the server.

#### IsClient

```luau
RunService:IsClient(): boolean
```

Whether this is a LocalScript, on the player's side.

#### IsStudio

```luau
RunService:IsStudio(): boolean
```

Whether the game runs in the editor (Play, or the Console), not the player.

#### IsRunning

```luau
RunService:IsRunning(): boolean
```

Whether the game runs: false only in the editor's Console outside Play.

