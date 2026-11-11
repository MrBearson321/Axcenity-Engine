# Game systems

Roadmap L7 moves gameplay systems out of the `script` crate, so `ScriptRuntime` can shrink toward the Luau core: the interpreter, the sandbox, the scheduler, Instances and events. A system that moves lives in the `gameplay` crate and plugs back in through one trait.

## `GameSystem`

`script::GameSystem` (`crates/script/src/system.rs`) is what a system gives the runtime. Every method has a default that does nothing:

- `services()` and `service(lua, name)`: the names it adds to `game:GetService(name)` and `game.<Name>`, and their values as each side's scripts see them. A wrong service name's error lists them after the engine's own.
- `type_name(data)`: scripts' name for its userdata (`typeof(QuestService)` is `"QuestService"`).
- `added(lua)`: called for each side's interpreter when the system is added, before scripts run, to set up its app data.
- `methods(lua)`: Instance methods it adds, as `(class, name, function)` (`ability:Activate()`, `model:ApplyEffect(effect)`). A name the engine already has for that class keeps the engine's.
- `read_only(class, key)`: why scripts can't set a property the system keeps (`Ability.CooldownLeft`), or `None`.
- `step(lua)`: once per frame on each side inside `Context::step`, after `task` resumes, CSG results and content loading, and before decals fade. The World is lent.
- `holds_input()`: while true (a dialogue runs), ProximityPrompts, ClickDetectors and Tools don't step, and `ScriptRuntime::movement_locked` keeps the character still.
- `step_input(runtime, world, input)`: in `ScriptRuntime::step_prompts` (frame step 1), after prompts and ClickDetectors step and before their events fire, or instead of them while input is held. `fire_changes` follows each system's call.
- `prompt_triggered(runtime, world, prompt, input)`: a ProximityPrompt fired `Triggered`, after its handlers ran.
- `fire_events(runtime, world)`: fires the system's waiting events from `ScriptRuntime::fire_changes`, after the engine's (Abilities, Health, Tools, Players, levels) and before stopped scripts are dropped. It returns whether anything fired, and the runtime asks again up to its change-round limit. Inside it, use `fire_change_rounds`, not `fire_changes`.
- `fires_first()`: true to fire before the engine's events (Players, levels) instead of after them. Abilities and Health do (in that order, so the Health events Effects queue fire in the same round, and a Humanoid's death reaches Players in the same call). Tools fire after, so a `Died` or `CharacterAdded` handler's equips fire in the same call.
- `field_names()`, `get_field(lua, id, class, key)`, `sets_field(class, key)` and `set_field(lua, id, class, key, value)`: properties the system keeps instead of storing them. A Humanoid's `WalkSpeed` reads and writes its controller's, and `Health.Health` fires `HealthChanged`. Property reads check `field_names` first (one map lookup), so other names cost nothing more.
- `activation_requested(target, player)`: a player asked the server to activate `target` (`MessageKind::Activate`), from this device's LocalScripts or, through `receive_remote_messages`, from another device. Send one with `system::send_activation`.

One value serves both sides. State both sides share lives in the system (`Story` holds the running dialogue and the quests' waiting events); per-side state goes in each interpreter's app data.

## What a system may use

- **Interpreter side**, `axcenity_script::system`: `now`, `TOLERANCE_SECONDS`, `destroy`, `report_error` and `report`, `error_message`, `type_name`, `side`, `engine_environment` (a sandboxed environment for engine Lua), `running_script`, `waiting` and `resume_as` (pause a script on engine work and resume it later, as the debugger expects), `signal` and `fire` (a service's events, `events::Owner::Service`), `fire_instance_event`, `saves`, `player_middle`, `LuaCFrame`, `LuaVector3` and `to_vec3`, `get_property`/`set_property` (as a script's `id.key`), `camera_yaw`, `HUMANOID_STATES`, `humanoid_state_item` and `humanoid_state_name`, `local_player`, `player_of`, `send_activation`, `destroyed`. Also `LuaInstance` and `lent_world`/`lent_world_mut`.
- **Runtime side**, the `ScriptRuntime` methods documented "for game systems": `system::<T>()` (hosts reach a system's state through it, as `gameplay::dialogue_view` does), `lua(side)`, `run_on(side, world, f)` (lends the world, runs `f`, gathers output), `acting_side()`, `report_error`, `assets()`, `fire_change_rounds`, `prompt_input()`, `humanoid_died` (Players respawn the character), `MAX_CHANGE_ROUNDS`, plus the public `fire_changes`, `saves`, `local_player`, `input_locked` and `is_client_only`.

When a system needs more, make that one thing public on purpose rather than reaching into `script`'s modules.

## Adding the systems

`axcenity_gameplay::install(&mut runtime)` adds every system; `axcenity_gameplay::script_runtime()` is `ScriptRuntime::new()` with them, and `axcenity_sim::script_runtime` re-exports it. Every host (the player, editor Play and its UI graph preview, the dedicated server, the game test runner) and their tests build runtimes with `axcenity_sim::script_runtime()`. A bare `ScriptRuntime::new()` (`script`'s own tests, the edit-time Console and plugins' `EditRuntime`) has none of them: no Debris, DialogueService or QuestService, and Abilities have no methods.

## Definitions

The luau-lsp definitions and the editor's API Reference still describe every service in `script/src/definitions/tables.rs`. `system::DESCRIBED_SYSTEM_SERVICES`, `DESCRIBED_SYSTEM_METHODS` and `DESCRIBED_SYSTEM_FIELDS` list the described services, Instance methods and kept properties (a Humanoid's fields) that systems add: `script`'s definition test skips them, because its bare runtime lacks them, and `gameplay`'s `system_tests` check that the lists match the installed systems and that every described member exists on both sides.

## Systems moved

| System | Services | Hooks | Tests |
|---|---|---|---|
| Debris (`gameplay/src/debris.rs`) | `Debris` (`AddItem`) | destroys due items, with `Destroy`'s events | `gameplay/src/debris_tests.rs`; the player's `hello-messages` sparks test |
| Abilities and Effects (`ability_system.rs`, `ability.rs`; [../scripting/api-abilities.md](../scripting/api-abilities.md)) | none (methods on Ability and Model) | `methods`, `read_only`; fires first; `gameplay::step_health` counts cooldowns and Effects; `activation_requested` queues players' requests. Hosts call `gameplay::step_abilities` (frame step 1) and draw `gameplay::ability_bar` | `gameplay/src/ability_tests.rs`, `net/src/ability_tests.rs` |
| Tools and Inventories (`tool_system.rs`, `tool.rs`; [../scripting/api-character.md](../scripting/api-character.md)) | none (methods on Inventory, Tool, and Humanoid's `EquipTool`/`UnequipTools`) | `methods`, `read_only` (`Inventory.Equipped`); `fire_events` fires `Equipped`/`Unequipped` for what scripts (both sides share one `ToolChanges`) and the step changed. Hosts call `gameplay::step_tools` (frame step 1), `place_tools` (after the Animators) and draw the hotbar for `player_inventory` | `gameplay/src/tool_tests.rs`, `player/src/tool_tests.rs` |
| Health and Humanoids (`character_system.rs`, `health.rs`, `humanoid.rs`; [../scripting/api-character.md](../scripting/api-character.md)) | none (methods on Health and Humanoid) | `methods`; the field hooks for a Humanoid's fields and `Health.Health`; fires first, after Abilities: Health events, then the Humanoids of Healths that died. Hosts call `gameplay::fire_landed` and `step_humanoids` (frame step 1), `step_health` (step 4, server only, which also steps Abilities' cooldowns and Effects) and `share_nav_mesh` | `gameplay/src/health_tests.rs`, `humanoid_tests.rs`; `player/src/health_tests.rs`, `humanoid_tests.rs`, `spawn_tests.rs` |
| Story: dialogues and quests (`story.rs`, `dialogue.rs`, `quest.rs`; [../scripting/api-dialogue.md](../scripting/api-dialogue.md)) | `DialogueService`, `QuestService` | holds input while a dialogue runs; `step_input` runs its keys and Reach objectives; `prompt_triggered` starts a prompt's `Dialogue`; `fire_events` fires quest events and resumes scripts waiting in `Start` | `gameplay/src/dialogue_tests.rs`, `editor/src/app/dialogue_tests.rs` |

## What stays in `script`

Players (`players.rs`, Phase 47) stays: it is wired into joining, levels, kicks, `LocalPlayer` and networking, so it is part of the multiplayer core rather than a gameplay add-on (Paul's call, 2026-11-11). The character system tells it a Humanoid died with `ScriptRuntime::humanoid_died`. L7 ends with Health and Humanoids.
