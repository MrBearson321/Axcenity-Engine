# Game systems

Roadmap L7 moves gameplay systems out of the `script` crate, so `ScriptRuntime` can shrink toward the Luau core: the interpreter, the sandbox, the scheduler, Instances and events. A system that moves lives in the `gameplay` crate and plugs back in through one trait.

## `GameSystem`

`script::GameSystem` (`crates/script/src/system.rs`) is what a system gives the runtime. Every method has a default that does nothing:

- `services()` and `service(lua, name)`: the names it adds to `game:GetService(name)` and `game.<Name>`, and their values as each side's scripts see them. A wrong service name's error lists them after the engine's own.
- `type_name(data)`: scripts' name for its userdata (`typeof(QuestService)` is `"QuestService"`).
- `added(lua)`: called for each side's interpreter when the system is added, before scripts run, to set up its app data.
- `step(lua)`: once per frame on each side inside `Context::step`, after `task` resumes, CSG results and content loading, and before decals fade. The World is lent.
- `holds_input()`: while true (a dialogue runs), ProximityPrompts, ClickDetectors and Tools don't step, and `ScriptRuntime::movement_locked` keeps the character still.
- `step_input(runtime, world, input)`: in `ScriptRuntime::step_prompts` (frame step 1), after prompts and ClickDetectors step and before their events fire, or instead of them while input is held. `fire_changes` follows each system's call.
- `prompt_triggered(runtime, world, prompt, input)`: a ProximityPrompt fired `Triggered`, after its handlers ran.
- `fire_events(runtime, world)`: fires the system's waiting events from `ScriptRuntime::fire_changes`, after the engine's (Abilities, Health, Tools, Players, levels) and before stopped scripts are dropped. It returns whether anything fired, and the runtime asks again up to its change-round limit. Inside it, use `fire_change_rounds`, not `fire_changes`.

One value serves both sides. State both sides share lives in the system (`Story` holds the running dialogue and the quests' waiting events); per-side state goes in each interpreter's app data.

## What a system may use

- **Interpreter side**, `axcenity_script::system`: `now`, `TOLERANCE_SECONDS`, `destroy`, `report_error` and `report`, `error_message`, `type_name`, `side`, `engine_environment` (a sandboxed environment for engine Lua), `running_script`, `waiting` and `resume_as` (pause a script on engine work and resume it later, as the debugger expects), `signal` and `fire` (a service's events, `events::Owner::Service`), `saves`, and `player_middle`. Also `LuaInstance` and `lent_world`/`lent_world_mut`.
- **Runtime side**, the `ScriptRuntime` methods documented "for game systems": `system::<T>()` (hosts reach a system's state through it, as `gameplay::dialogue_view` does), `lua(side)`, `run_on(side, world, f)` (lends the world, runs `f`, gathers output), `acting_side()`, `report_error`, `assets()`, `fire_change_rounds`, plus the public `fire_changes` and `saves`.

When a system needs more, make that one thing public on purpose rather than reaching into `script`'s modules.

## Adding the systems

`axcenity_gameplay::install(&mut runtime)` adds every system; `axcenity_gameplay::script_runtime()` is `ScriptRuntime::new()` with them, and `axcenity_sim::script_runtime` re-exports it. Every host (the player, editor Play and its UI graph preview, the dedicated server, the game test runner) and their tests build runtimes with `axcenity_sim::script_runtime()`. A bare `ScriptRuntime::new()` (`script`'s own tests, the edit-time Console and plugins' `EditRuntime`) has none of them: no Debris, DialogueService or QuestService.

## Definitions

The luau-lsp definitions and the editor's API Reference still describe every service in `script/src/definitions/tables.rs`. `system::DESCRIBED_SYSTEM_SERVICES` lists the described services that systems add: `script`'s definition test skips them, because its bare runtime lacks them, and `gameplay`'s `system_tests::described_system_services_are_the_installed_ones_and_their_members_exist` checks that the list matches the installed systems and that every described member exists on both sides.

## Systems moved

| System | Services | Hooks | Tests |
|---|---|---|---|
| Debris (`gameplay/src/debris.rs`) | `Debris` (`AddItem`) | destroys due items, with `Destroy`'s events | `gameplay/src/debris_tests.rs`; the player's `hello-messages` sparks test |
| Story: dialogues and quests (`story.rs`, `dialogue.rs`, `quest.rs`; [../scripting/api-dialogue.md](../scripting/api-dialogue.md)) | `DialogueService`, `QuestService` | holds input while a dialogue runs; `step_input` runs its keys and Reach objectives; `prompt_triggered` starts a prompt's `Dialogue`; `fire_events` fires quest events and resumes scripts waiting in `Start` | `gameplay/src/dialogue_tests.rs`, `editor/src/app/dialogue_tests.rs` |

## What's next

Abilities, Tools, Humanoids and Health, and Players are stepped by the hosts through `ScriptRuntime` methods (`step_abilities`, `step_tools`, `step_humanoids`, `step_health`, `step_players`), keep their state in its fields, and `net` calls them. Each moves in its own PR, growing the trait or the runtime's "for game systems" methods only as far as that system needs.
