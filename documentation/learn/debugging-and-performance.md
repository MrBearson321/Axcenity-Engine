---
description: Output and errors, changing scripts while the game runs, breakpoints, the Developer Console (F9), the Profiler's Game, Graphics and Memory tabs, visual overlays, game tests and keeping a game fast.
icon: bug
samples: [hello-script, hello-character]
---

# Debugging and performance

## Output and errors

The **Output** panel shows what scripts `print()` and every warning and error. An error starts with the script and line, such as `scripts/Door.server.lua:12:`; click it to open the script at that line. A block script's error highlights the block it came from. Errors are written in plain words: what went wrong, and often what to do about it.

Use `print()` for quick checks of what a script sees.

## Changing a script while the game runs

Save a script during Play and it runs its new code at once, without Stop. The game keeps its state: the script's top-level `local`s keep the values they had, so a score, a counter or a part the script made carries on. A local whose starting value you edited takes the new value, and ending its line with `-- @reset` always starts it fresh. A changed ModuleScript is picked up by every script that uses it. Output says which values were kept, such as `Reloaded Door.server.lua, kept count and part`. A script with a mistake in it isn't run: the last good version keeps going until you fix it. Models, textures, sounds and clips you change on disk swap in during Play too.

## Breakpoints

Click a line number in a script tab to set a breakpoint (a red dot). When a script reaches that line during Play, the whole game **pauses** there: the tab opens at the line, and a panel under the code lists the call stack with each function's local variables. **Continue** (or **F5**) carries on. Breakpoints work in Play in the editor only, never in a built game.

## The Developer Console

Press **F9** during Play for the Developer Console, drawn over the game:

- **Log**: everything printed, filtered by level (**L**) and side (**S**).
- **Stats**: the frame rate and how long each part of the frame took.
- **Commands**: type Lua to run it, or a built-in command: `help`, `clear`, and `fps` for a frame-time readout that says whether the game is waiting on the CPU or the GPU.

Games can add their own commands with `ConsoleService:RegisterCommand()`. Built games have the console only when **File > Build Game > Developer Console (F9)** is ticked.

## The Profiler

**View > Panels > Profiler** shows what the game costs, averaged over about two seconds:

- **Game**: frame rate, CPU and GPU time, memory, and the time spent in physics, audio, scripts, animation and the network.
- **Graphics**: what the GPU drew: draw calls, triangles, shadows, lights and passes.
- **Memory**: memory by kind (Lua, Instances, models, textures, sounds, physics) and the classes that hold the most.

## Seeing what the engine sees

**Viewport > Overlays** draws debug information over the Scene: **Colliders** (collision shapes), **Constraints** (welds, joints and movers), **Raycasts**, **Emitters** (where sounds play from), the **Navigation mesh** and **agent paths**, and what's **culled**. **Viewport > View Mode** shows the scene as wireframe, lighting only, normals and other debug views.

## Game tests

Write automated tests for your game in `tests/*.spec.lua`: describe what should happen, run the game forward, press keys and click buttons, and check the result.

```lua
describe("The door", function()
	it("opens when the button is pressed", function()
		local door = Test.find("Workspace.House.Door")
		Test.click(Test.find("Workspace.Menu.Open"))
		Test.step(1)
		expect(door.CanCollide).toBe(false)
	end)
end)
```

Run them from **View > Panels > Tests** (Run all, a file, or one test), or from a terminal with `axcenity-test <project folder>`.

## Keeping a game fast

- Anchor everything that doesn't need to move: anchored parts cost nothing in physics.
- Use **Models** with levels of detail for detailed meshes (Asset Check and the LOD Inspector help), and keep textures to the size they're seen at.
- Don't loop without `task.wait()`, and prefer events (`Touched`, `Changed`) to checking every frame.
- Turn on **StreamingEnabled** on the Workspace for big worlds, so only what's near the player is loaded. This is newer and off to start (see [Multiplayer and networking](../reference/networking.md)).
- Lower the **graphics quality** in Settings to see whether the GPU is the limit.
