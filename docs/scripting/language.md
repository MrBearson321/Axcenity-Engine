# Language and API style

Lua 5.4, the definitions file, and how names are chosen.

- **Lua 5.4** via `mlua` (vendored). Not Luau, not LuaJIT.
- Types for tooling come from a generated **Lua Language Server definitions file** (`---@class`, `---@param` annotations), checked in at `definitions/axcenity.lua`. The generator is `crates/script/src/definitions.rs`: classes, properties and enums come from `core`, while functions, globals and value types are described there by hand. Every public API addition must update it, then run `cargo run -p axcenity-script --bin lua-definitions`. Tests fail (so CI fails) if a property or event has no description, if a global or library function is missing from the descriptions, or if the checked-in file is out of date.
- **In a project** (built in Phase 16, chosen by the developer): the editor's **File > Set Up VS Code Autocomplete** writes `.axcenity/axcenity.lua` (the definitions), `.luarc.json` (Lua 5.4, `.axcenity` as a library, `io`/`debug`/`package` turned off) and `.vscode/extensions.json` (suggests the `sumneko.lua` extension) into the project. The last two are only written if missing, since they may hold the creator's settings. Opening a project in the editor rewrites `.axcenity/axcenity.lua` when it's out of date (`crates/editor/src/autocomplete.rs`). Checked with the real Lua Language Server (`lua-language-server --check`): every example has no warnings, and mistakes such as `part.Size = 5`, `task.wiat()` and `io.write` are flagged.
- **Children by name** are typed as the union of every class (`Part|Model|Folder|...`), not plain `Instance`: with `Instance`, `workspace.Pad.Color = ...` was flagged as a field Instance doesn't have. For the same reason `require` takes an `Instance`.

- Reuse familiar names and patterns: `Instance.new("Part")`, `part.Parent = workspace`, `part.Touched:Connect(fn)`, `Vector3.new`, `task.wait` / `task.spawn` / `task.delay`.
- Classes are PascalCase, properties are PascalCase, methods are called with `:`, events are `RBXScriptSignal`-like objects with `:Connect` (but don't name anything with `RBX`).
- Clean up Roblox quirks when it helps beginners. Don't chase full compatibility.
- Never use Roblox's name, logos or assets.
