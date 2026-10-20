# Errors

How script errors are reported.

- Every message starts with `path:line:`, using the path from the project folder (`scripts/build.server.lua:3: Part.Size must be a Vector3, not a number`). Engine errors take the line from the Lua call stack.
- Scripts see the same plain strings through `pcall` and `xpcall`, which are thin Lua wrappers around the real ones, so pausing inside a `pcall` will still work.
- Output is a list of `OutputLine`s (`Print` or `Error`) that the host takes with `take_output()`: the player prints them, and the editor's Output panel shows them during Play (`Print` as info, `Error` in red).
