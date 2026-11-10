# Developer Console and logs

Phase 71. A console over the game (F9), a game's own commands (`ConsoleService`), and the player's log files.

## Who has it

- **Editor Play**: always. F9 over the viewport (not while an editor text field has the keyboard).
- **Player, project folder** (`cargo run -p axcenity-player -- examples/x`): a development run, always.
- **Player, built game** (`.axgame`, or one bundled next to the player): only when the project setting is on (`developer_console` in `axcenity.json`; File ▸ Build Game ▸ Developer Console (F9), also in the command palette), or the player is started with `--dev`. Otherwise F9 is an ordinary key for the game. The rule is `player::dev_console::console_allowed`.

## The console

`axcenity_ui::DevConsole` (`ui/src/console.rs`) keeps the state and draws itself as draw items (`console_items`) over everything, in the top 55% of the view, in JetBrains Mono. While it's open it takes every key; opening it lets go of the game's held keys.

- **Log**: what scripts and the engine printed. `L` cycles the level filter (all, warnings and errors, errors), `S` the side (both, server, client). Lines from the engine have no side and always show. The last 500 lines are kept.
- **Stats**: frame rate (averaged over 60 frames), the last frame's time and each step's (player: Network, Characters, Agents, Input, Scripts, Physics, Heartbeat, Animation, Draw list, UI, Render, the same timer as `--bench` ([benchmarks.md](benchmarks.md)); editor: Scripts, Physics, Audio, Animation), process memory, both Lua heaps, the Instance count, and, hosting or joined, players, the worst ping and the bytes sent and received a second.
- **Commands**: the log with a line to type under it. `Up`/`Down` go through earlier lines, `PageUp`/`PageDown` scroll, `Tab` changes tab, `Escape` or F9 closes.

A typed line is, in order:

1. A built-in (`axcenity_ui::built_in`): `help`, `clear`, `fps` (readout at the top right), `wireframe` (the renderer's wireframe debug view; the player builds `render` with `debug-views` for it), `collisions` (collider outlines from `physics::debug_colliders`, green solid and amber sensors; in the editor, the collider overlay), `noclip` (the character flies through everything: Jump up, Crouch down, no gravity; `Controllers::set_noclip`), `timescale <0–100>` (game seconds per real second), `teleport <x> <y> <z>` (the character's feet, else the fly camera), and `crash` (a panic, to try the crash log; in the editor it stops Play as a crash).
2. A game's command registered with `ConsoleService` (LocalScripts' first, then Scripts').
3. On a device joined to a server elsewhere, a line that reads as a command (`looks_like_command`: a name, then only words, numbers and `_ - . + : , / @`) whose first word isn't a Lua global goes to the server (`Message::Console`, protocol 6).
4. Otherwise Lua, as the editor's Console runs it (server side; client side on a joined device). A command-looking line whose word means nothing to Lua says "Unknown command".

## ConsoleService

`game:GetService("ConsoleService")` (not an Instance; `script/src/console_service.rs`):

- `RegisterCommand(name, help, callback)`: `name` is one word. Typed as `name a b`, it calls `callback(player, "a", "b")` in a task of its own (so it can `task.wait`). A LocalScript's runs on that device; a Script's on the server. A stopped script's commands go with it.
- `AllowedUserIds` (server): the `UserId`s that may run the server's commands from another device. The host's own device, and single player, always may. Refusals and unknown commands are in the server's log.

Block: **when console command *name* is run, help *text*** (Events), which becomes `ConsoleService:RegisterCommand(name, help, function() … end)` and comes back from Lua.

Tests: `script` `console_service_tests`, `net` `a_joined_players_console_command_runs_on_the_server_only_when_their_user_id_is_allowed`, `blocks` `a_when_command_run_block_runs_its_blocks_when_the_command_is_typed`, `player` `console_tests`, `editor` `dev_console_tests`.

## Logs

Everything the player prints goes through `player::logs::line`: to the terminal, to the log file, and to the console. `log` records go there too (`logs::Logger` wraps `env_logger`).

- The file is `logs/latest.log` in the player's data folder for the game (beside its saves: `axcenity_save::game_folder`). Each start moves it to `previous-1.log`, and those along to `previous-4.log`, so five runs are kept.
- A panic (`logs::install_panic_hook`) writes `panic: message (at file:line)` to the log, then prints "The game crashed: … The log is at …" on stderr before the player exits. There is no dialog box; on Windows the player keeps its console window, which shows it.

Test: `logs::tests::a_forced_panic_leaves_a_log_with_the_panic_and_says_where_the_log_is`.
