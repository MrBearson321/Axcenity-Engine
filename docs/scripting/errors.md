# Errors

How script errors are reported.

- Every message starts with `path:line:`, using the path from the project folder (`scripts/build.server.lua:3: Part.Size must be a Vector3, not a number`). Engine errors take the line from the Lua call stack.
- Scripts see the same plain strings through `pcall` and `xpcall`, which are thin Lua wrappers around the real ones, so pausing inside a `pcall` will still work.
- An error raised while a script runs also has a stack trace (`OutputLine::trace`): the script calls that led to it, innermost first, each `path:line: in <function>` as Lua words it (`scripts/Fuse.module.lua:3: in field 'light'`). Calls through other scripts and modules are included. Rust (`[C]`), the engine's own Lua (`[engine]`) and mlua's generated code are left out. For an engine error, the trace comes from the deepest Lua call, so it covers the whole coroutine. Syntax errors, `print` and engine messages have no trace.
- Output is a list of `OutputLine`s (`Print` or `Error`) that the host takes with `take_output()`: the player prints them (each trace call indented under its error, on stderr), and the editor's Console shows them during Play (`Print` as info, `Error` in red, with the trace muted underneath; the error and each trace call link to that script and line). Console commands run with `ScriptRuntime::run_command` during Play and `run_edit_command` outside it (see `docs/editor/shell.md`); their errors read `console:<line>:`.
