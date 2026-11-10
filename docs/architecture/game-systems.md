# Game systems

Roadmap L7 moves gameplay systems out of the `script` crate, so `ScriptRuntime` can shrink toward the Luau core: the interpreter, the sandbox, the scheduler, Instances and events. A system that moves lives in the `gameplay` crate and plugs back in through one trait.

## `GameSystem`

`script::GameSystem` (`crates/script/src/system.rs`) is what a system gives the runtime:

- `services()`: the names it adds to `game:GetService(name)` and to `game.<Name>`, such as `"Debris"`. A wrong service name's error lists them after the engine's own.
- `service(lua, name)`: the service's value, as the scripts on that side see it.
- `step(lua)`: runs once per frame on each side inside `Context::step`, after `task` resumes, CSG results and content loading, and before decals fade (Debris's place before it moved). The World is lent, as during any script call. Systems step in the order they were added.

One value serves both sides. Per-side state goes in the interpreter's app data (`Lua::set_app_data`), so the server's and client's items stay apart.

A system uses only `script`'s public API: `axcenity_script::system` (`now`, `TOLERANCE_SECONDS`, `destroy`, `report_error`, `type_name`), `LuaInstance`, and `lent_world`/`lent_world_mut`. When a system needs more, make that one thing public on purpose rather than reaching into `script`'s modules.

## Adding the systems

`axcenity_gameplay::install(&mut runtime)` adds every system; `axcenity_gameplay::script_runtime()` is `ScriptRuntime::new()` with them, and `axcenity_sim::script_runtime` re-exports it. Every host (the player, editor Play and its UI graph preview, the dedicated server, the game test runner) and their tests build runtimes with `axcenity_sim::script_runtime()`. A bare `ScriptRuntime::new()` (`script`'s own tests, the edit-time Console and plugins' `EditRuntime`) has no Debris.

## Definitions

The luau-lsp definitions and the editor's API Reference still describe every service in `script/src/definitions/tables.rs`. `system::DESCRIBED_SYSTEM_SERVICES` lists the described services that systems add: `script`'s definition test skips them, because its bare runtime lacks them, and `gameplay`'s `described_system_services_are_the_installed_ones_and_their_members_exist` checks that the list matches the installed systems and that every described member exists on both sides.

## Systems moved

| System | Services | Step | Tests |
|---|---|---|---|
| Debris (`gameplay/src/debris.rs`) | `Debris` (`AddItem`) | destroys due items, with `Destroy`'s events | `gameplay/src/debris_tests.rs`; the player's `hello-messages` sparks test |

## What's next

The larger systems (Humanoids, Players, Tools, Abilities, Dialogue and Quests, Health) are stepped by the hosts through `ScriptRuntime` methods and share its fields, and `net` calls about 30 of those methods. Moving one needs the trait to grow host-facing steps through the `sim` Schedule, not only `Context::step`; do that with the first such system rather than ahead of it.
