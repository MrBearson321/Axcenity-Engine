# Blocks

How each block becomes Lua. One-way: blocks translate to Lua, and there is one runtime.

- Block scripts are JSON files ending in `.server.blocks.json` or `.client.blocks.json` (format in `docs/project/world.md`). The editor shows them Scratch-style: a palette by category on the left, and a canvas of stacks, each under an event block ("hat"), or loose (an unattached stack, which doesn't run). Blocks snap together, C-shaped blocks hold stacks, and reporters (round for values, pointed for true or false) drop into slots; a pointed reporter fits any slot, a round one only round slots. Client-only blocks (keys, button clicks) are only offered in client block scripts, and the canvas shows a Server or Client badge.
- The same blocks can be shown as a Blueprint-style graph of nodes and wires instead (the Graph view): [graphs.md](graphs.md).
- There is no separate runtime: **blocks always translate to Lua**, using the same `ScriptRuntime` and API as handwritten scripts. Objects are named by their path under the Workspace (`Pad.Chime` is `workspace.Pad.Chime`, or `workspace["Red Ball"]` for names that aren't Lua names).
- Variables belong to the whole block script, as in Scratch: each is declared once at the top as `local name = 0`. Loops that may run long (forever, repeat until, wait until) wait a frame each time round (`task.wait()`), so the game keeps running.
- "View as Lua" shows the generated Lua beside the canvas, readable and with no markers in it. `Translation::block_lines` maps each block to the line its code starts on, so a runtime error's line highlights the block it came from (in red).
- Switching a script from Code to Blocks (`from_lua`, read by the Luau reader in `crates/blocks/src/lua.rs`) turns code shaped like the translation back into blocks; anything else is kept word for word in grey Lua blocks, so nothing is lost and it runs the same. All of Luau is read: `x += n` becomes change variable, move by or turn by (others such as `-=` stay Lua), and `continue`, if-expressions, backtick strings, `::` casts and anything with type annotations (typed locals and `for`s, `type` definitions, a handler with typed parameters) stay Lua, so the annotations aren't lost. Lua 5.4 syntax that Luau doesn't have (`goto`, labels, `<const>`, bitwise operators) is a syntax error, as it is when the script runs.
- Every block must correspond to a real Lua API. Don't add block-only features. The blocks and what they become:

| Category | Block | Lua |
|---|---|---|
| Events | when game starts | `task.spawn(function() … end)` |
| | when *part* is touched | `part.Touched:Connect(function(otherPart) … end)` |
| | when *key* key pressed (client) | `UserInputService.InputBegan:Connect(function(input) if input.KeyCode == Enum.KeyCode.Key then … end end)` |
| | when *button* is clicked (client) | `button.Activated:Connect(function() … end)` |
| | every frame | `RunService.Heartbeat:Connect(function(deltaTime) … end)` |
| | when *sound* ends | `sound.Ended:Connect(function() … end)` |
| Control | wait, wait until | `task.wait(n)`; `while not (c) do task.wait() end` |
| | repeat *n* times, forever, repeat until | `for _ = 1, n do … end`; `while true do … task.wait() end`; `while not (c) do … task.wait() end` |
| | if, if/else, stop this script | `if c then … else … end`; `do return end` |
| Motion | move to, move by, turn to, turn by, set anchored | `Position =`, `Position = Position + v`, `Orientation =`, `Orientation = Orientation + v`, `Anchored =` |
| Looks | set color, set text, show, hide, show page, set *property* | `Color =`, `Text = tostring(v)`, `Visible = true/false`, `layout.CurrentPage = page`, `object.Property = v` (a character controller's walk or run is `controller.Movement = "Run"`; a custom mode is any other name, and text `""` clears it) |
| Sound | play, stop, pause, resume, set volume | `:Play()`, `:Stop()`, `:Pause()`, `:Resume()`, `Volume =` |
| Variables | set, change by, *variable* | `name = v`, `name = name + n`, `name` |
| More | print, destroy | `print(v)`, `object:Destroy()` |
| Operators | + − × ÷ mod ^, pick random, join, round/abs/floor/ceiling/sqrt, < = > ≤ ≥ ≠, and, or, not | Lua operators; `math.random(math.floor(a), math.floor(b))`; `tostring(a) .. tostring(b)`; `math.floor(x + 0.5)`, `math.abs` …; `not` |
| Sensing | touching part, key pressed? (client), *property* of *object*, raycast from *origin* along *direction* | `otherPart` (inside "when touched" only), `UserInputService:IsKeyDown(Enum.KeyCode.Key)`, `object.Property`, `workspace:Raycast(origin, direction)` |
| Animation | set trigger *name* of *animator*, reset trigger, set *bool* of *animator* to *true/false*, set *number* of *animator* to *n*, play state *s* of *animator*, play animation *animation* on *animator* | `animator:SetTrigger("name")`, `:ResetTrigger("name")`, `:SetBool("name", v)`, `:SetNumber("name", n)`, `:Play("s")`, `animator:LoadAnimation(animation):Play()` |
| | when *animator* changes state, when *animator* reaches event *e* | `animator.StateChanged:Connect(function(state, previous) … end)`; `animator.AnimationEvent:Connect(function(name) if name == "e" then … end end)` |
| | *bool* of *animator*, *number* of *animator*, CurrentState of *animator* | `animator:GetBool("name")` (pointed), `animator:GetNumber("name")`, `animator.CurrentState` (the property reporter, set to CurrentState) |
| Tweens | tween *property* of *object* to *v* over *n* seconds *style* *direction* | `TweenService:Create(object, TweenInfo.new(n, Enum.EasingStyle.Style, Enum.EasingDirection.Direction), {Property = v}):Play()` |
| | slide *object* in from left / right / top / bottom over *n* seconds | `do local info = TweenInfo.new(n) local goal = object.Position object.Position = goal - UDim2.fromScale(1, 0) TweenService:Create(object, info, {Position = goal}):Play() end` (on its own lines; `+` for right and bottom, `fromScale(0, 1)` for top and bottom) |
| | fade *object* in / out over *n* seconds | `TweenService:Create(object, TweenInfo.new(n), {BackgroundTransparency = 0})` (1 for out) `:Play()` |

Tween blocks start the tween and carry on, like `tween:Play()` (see [api-tween.md](api-tween.md)); `local TweenService = game:GetService("TweenService")` goes at the top when one is used. Their style, direction and edge menus offer `EASING_STYLES`, `EASING_DIRECTIONS` and `SLIDE_EDGES` (`crates/blocks/src/lib.rs`). Switching Code to Blocks turns those shapes back into them; `TweenInfo.new(n)` alone is Quad Out, and a fade only when it's `BackgroundTransparency` to 0 or 1. A tween with several goals stays Lua.

The Animation blocks (Phase 42) name an Animator by its path (`Noodle.Animator`, picked from a menu of the Workspace's Animators); play animation names an Animation the same way (v3 Phase 7; stopping a track needs the track, so it stays Lua); their parameter, state and event menus offer the names in that Animator's state machine (its parameters by kind, its states, and its clips' events, read again every 2 s) and a box to type another. Switching Code to Blocks turns those calls back into the blocks (a `Play` with a blend, or a handler that uses `state`, `previous` or `name`, stays Lua).
