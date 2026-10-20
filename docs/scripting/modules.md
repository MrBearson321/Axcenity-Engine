# Modules

How require works.

- Roblox's way: a `scripts/**/*.module.lua` file becomes a **ModuleScript** in **ReplicatedStorage** (named after the file), and `require(game.ReplicatedStorage.Utils)` runs it. Only ModuleScripts can be required: a string (`require("utils")`), a path, a library name or another Instance is an error that explains what to pass. So nothing outside the project can be loaded.
- A module runs **once per side**, the first time a script there requires it, in its own environment (`script` is the ModuleScript); every later `require` on that side gets the same value. The server and the client each run their own copy.
- It must return exactly one value (usually a table), or `require` errors. An error while loading reaches the requiring script (with the module's path and line), and the module is tried again at the next `require`. A syntax error names both files: `scripts/main.server.lua:1: scripts/m.module.lua:3: ...`.
- The module's code runs in the requiring coroutine, so it may `task.wait`; a second script requiring it meanwhile waits until it's loaded. Modules that require each other (in the same coroutine) error instead of hanging.
- The host passes ModuleScripts to `ScriptRuntime::start_script` like any script; they only register their code (`modules.rs`), so pass them first (`project::script_order` sorts them first). A ModuleScript made with `Instance.new` has no code and can't be required.
