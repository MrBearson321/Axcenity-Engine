---
description: "Global tables of functions, such as task."
---

# Libraries

Global tables of functions, such as `task`.

## task

Pauses scripts and runs functions later, without stopping the game.

### Functions

#### wait

```luau
task.wait(seconds: number?): number
```

Pauses this code for `seconds` (left out: until the next frame), while the game and other scripts keep running. Returns the seconds that really passed.

```lua
while true do
	task.wait(1)
	print("Another second")
end
```

#### spawn

```luau
task.spawn(callback: (...any) -> (), ...: any): thread
```

Runs `callback` right away with the given arguments, until it finishes or waits, then carries on here. An error inside it is reported without stopping this code.

```lua
task.spawn(function()
	task.wait(2)
	print("This runs alongside")
end)
print("This runs first")
```

#### delay

```luau
task.delay(seconds: number, callback: (...any) -> (), ...: any): thread
```

Runs `callback` with the given arguments after `seconds`, without pausing this code.

```lua
task.delay(5, function()
	workspace.Door.CanCollide = false
end)
```

#### defer

```luau
task.defer(callback: (...any) -> (), ...: any): thread
```

Runs `callback` (or resumes a thread) with the given arguments at the next frame step, after this code has finished or paused.

#### cancel

```luau
task.cancel(thread: thread): ()
```

Stops `thread` (from `task.spawn`, `task.delay` or `task.defer`) for good: it doesn't start or resume again. It can't stop the code running now.

#### synchronize

```luau
task.synchronize(): ()
```

Does nothing: every script runs on one thread, so there's nothing to wait for. For code written for parallel Luau.

#### desynchronize

```luau
task.desynchronize(): ()
```

Does nothing: every script runs on one thread. For code written for parallel Luau.

