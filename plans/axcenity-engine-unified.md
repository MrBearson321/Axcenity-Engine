# Plan: Axcenity Engine, unified remainder of v2 and v3

> Sources: `plans/axcenity-engine-v2.md` (PRD `prd/axcenity-engine-v2.md`) and `plans/axcenity-engine-v3.md` (the developer's requests of 2026-10-22 and 2026-10-23), and the developer's requests of 2026-10-25 (Phases 8a to 8d, and the UI requests in Phases 9a to 9i and the additions to Phases 10 and 19, and the asset-fix and live-reload requests in Phases 22 to 26). This plan holds only what has not been built, as of 2026-10-25 (commit `a9b6638`). The phases run in order. Each phase names the v2 or v3 phase it came from. Where the text changed from the source plan, the change is marked **Changed**.
>
> Built and committed, but not yet ticked: v2 Phase 12 and v3 Phases 4, 6, 7 and 8. Their open criteria are under **Open checks** below. They need a hands-on check, not new work.
>
> Milestones 9 to 14 (Phases 27 to 67) come from the developer's interview of 2026-10-27 (commit `bcbc2a7`). They cover the features found missing in a survey of the engine: scripting essentials, physics, a gameplay kit, levels and editor tools, a mesh-authoring Model Studio, and networking. Graphics stays in `plans/axcenity-engine-graphics.md`. Their decisions are under **Decisions for Milestones 9 to 14**.

## Architectural decisions

v1, v2 and v3 rules still hold. These are the ones the remaining phases depend on.

- **Crates**: `character` already exists. `nav` arrives in Phase 12. It may query `physics` and `terrain`, and `physics` and `character` must not depend on it. `save` arrives in Phase 17. It depends on `core` only and never reads or writes the editor project.
- **Dependency rules**: `player` never depends on `editor`, `egui` or `blocks`. `render` receives only a draw list. The graph-to-Lua compiler lives next to the blocks translator, so generated Lua is ordinary Lua for the player.
- **Character movement**: controllers move a character kinematically and own its camera during Play. The host steps them after input and before scripts, then holds them kinematic just before the physics step. Walk, run and crouch are built-in modes. Jump, slopes and step-up are movement on those modes. A custom mode is a name: while it's active, scripts and blocks supply the motion.
- **Frame order**: input, then character controllers and navigation agents, then scripts, physics, touch events, UI layout, render. Visual-debug overlays draw in the editor viewport only.
- **Graphs**: there is one generic node-graph editor (Phase 11). The UI Event Graph and behavior trees (Phase 16) both use it. **Changed**: v2 had behavior trees with their own graph editor. Animator state machines stay the animation system and are not node graphs.
- **AI**: behavior trees are their own asset. Scripts and blocks can set an agent's destination without a tree.
- **Saves**: a slot holds player data, world state, the active checkpoint and settings. Save files live in the player's data for that game, never in the project folder. Each file has a version and a migration. Editor Play and Stop still restore the editing world.
- **Project files**: new optional files (`*.uigraph.json`, `*.axpkg`, the string table, behavior-tree assets) don't bump `project::FORMAT_VERSION`. A changed shape of an existing file does, with a migration that keeps a copy of the previous files.
- **Scripting**: one Lua runtime. Any creator-facing Lua added in a phase has matching blocks. Output keeps `script:line` errors.
- **Dependencies**: **Changed**: `.axpkg` needs no new dependency. `zip` is already pinned in the workspace and used to read and write archives (`editor/src/ambient_cg.rs`). Controller (gamepad) input in Phase 19 needs one new pinned dependency, with its licence checked as Apache-2.0-compatible.
- **Live reload** (Phases 23 to 26): watching files, reloading scripts during Play, rebuilding and hot-patching are for development only. Exported games never watch or patch anything, and the player only watches with `--watch`. Engine rebuild and hot-patching are editor features, behind a dev-only `hot-patch` feature where they need a dependency, so `player` gains no new dependency. A file that fails to reload keeps the last good version.
- **Platforms**: Windows, macOS and Linux. **Changed** (2026-11-10, [development policy](../docs/development-policy.md#platforms)): desktop first, with Windows and Linux as the primary platforms and macOS secondary. Mobile and web are not immediate priorities.

## Decisions for Milestones 9 to 14

These were decided in the developer's interview of 2026-10-27.

- **Order**: scripting essentials, physics, gameplay kit, levels and editor tools, Model Studio, networking. Each milestone may use the ones before it.
- **New crates**:
  - `mesh_edit` arrives in Phase 42. It holds the editable-mesh math (cutting, booleans, modeling, UVs, sculpting, skin weights) and has no egui, GPU or `core` dependency. `physics` (fracture), the editor (CSG, Model Studio) and `project` may use it.
  - `testing` arrives in Phase 53 and builds the `axcenity-test` binary.
  - `net` arrives in Phase 63, with an `axcenity-server` binary. `net` depends on `core` only. `player` may depend on `net`. `render` and `ui` never do.
  - Exported games never include `testing`, and include `net` only as the player already does.
- **Frame order** (extends the one above): network receive, input and actions, character controllers, navigation agents and vehicles, scripts, physics (constraints and movers included), touch events, UI layout, network send, render.
- **CFrame**: CFrame is a Lua value type. `Part.CFrame` and `Model:PivotTo` read and write the existing `Position` and `Orientation` (degrees, YXZ). Nothing stores a CFrame, so no file changes.
- **Gameplay kit**: it's built from small Instances (`Health`, `Inventory`, `Tool`, `ProximityPrompt` and others) that work on any Model. A `Humanoid` bundles them for Roblox-familiar scripts. It forwards its properties to the pieces and stores none of its own, so there's one source of truth.
- **Data**: Attributes, tags and Value objects are optional fields in `world.json`. Data tables (`*.data.json`) are new optional files. Neither bumps `project::FORMAT_VERSION`.
- **Input**: a project input map (`input.json`) names actions (Jump, Interact and others). Controllers, vehicles, prompts, tools and UI navigation read actions, not fixed keys. Rebinds are saved in the player's settings (Phase 17).
- **Levels**: `world.json` stays the start level. Other levels live in `levels/<name>.world.json`, a new optional folder, so there's no format bump. Services outside the Workspace (ReplicatedStorage, Players, Lighting, StarterPlayer) persist across level loads.
- **Prefabs**: nesting and variants extend the existing prefab files with optional fields.
- **Editor plugins**: these are Lua, editor-only, and sandboxed, with their own interpreter. They change the edit world only through `EditorApp::change`, so every plugin edit undoes. The player never loads plugins.
- **Model Studio**: a dock tab in the editor. A model's `.glb` stays the asset the game loads. Editing state that `.glb` can't hold (the modifier stack, paint layers, sculpt levels) goes in a sidecar `<model>.meshedit.json` and folder. Opening an FBX saves a `.glb` beside it and leaves the FBX untouched. `render` gains only overlay draw items (edges, vertices, weight and mask colours) through the draw list. The graphics plan is unchanged.
- **Networking**: Roblox-style. The server is authoritative, and Workspace, ReplicatedStorage, Lighting, Players and Teams replicate automatically. ServerStorage and ServerScriptService never leave the server. A client owns its own character's movement, and the server checks it. Single-player and editor Play run the same code over an in-process loopback, so there's one path. Hosting is direct address or LAN, with a headless dedicated server. There's no relay or matchmaking.
- **Dependencies to choose and licence-check when their phase starts**: a UDP transport (Phase 63, for example `renet` or `quinn`), mesh booleans (Phase 50: in-house first), and UV unwrapping (Phase 58: in-house LSCM first, `xatlas` only if quality falls short). Writing `.glb` uses `gltf-json`, which comes with the pinned `gltf`; pin it in the workspace. Physics joints, vehicles and convex decomposition use the pinned `rapier3d`. Audio effects use the pinned `kira`.
- **Blocks parity** still holds: every new creator-facing Lua API gets matching blocks and API reference entries.
- **Not in this plan**: web, Android and iOS export, Steam integration, relay servers and matchmaking, runtime IK (feet on slopes, look-at), and graphics features.

## Audit of 2026-11-12

> **From**: the developer's request of 2026-11-12 to finish the unticked items and fit them to the code as it is now (main at `aa666a3`). Since this plan was last edited (2026-11-10), the modernization roadmap (I, M and L items), the gap analysis (G items), the editor crate split, Team Create and Collaborate, the MCP toolkit, the Toolbox and the release system have all landed. Each unticked criterion was compared with main. Where a criterion moved, it says so beside it.

**What's left, in order.** One PR per step, each opened after the previous one merges.

| Step | Work | Status on main | Fit to today's code |
| --- | --- | --- | --- |
| 1 | Phase 89: engine hot-patching out, game files hot reload like s&box (**Changed** 2026-11-12) | Done in this phase's PR. Was: not started. Script reload (Phase 24, `ScriptRuntime::reload_script`) reruns a script from scratch. `hot_patch.rs`, `hot_patch/` and the `hot-patch` feature (`subsecond`, `dioxus-*`) are still in `crates/editor`. | `rebuild.rs`, `relaunch.rs` and `app/engine.rs` stayed in `crates/editor` through the crate split. `blocks` already has a full Luau reader with source spans (`blocks/src/lua.rs`), which wraps scripts' top-level locals. |
| 2 | Phase 90: a narrower Model Studio | Done in this phase's PR. Was: not started. | Model Studio moved to `crates/editor-model-studio` (G11, #189): delete `sculpt*` and `retopo*` there, not in `editor/src/model_studio/`. The usability audit (MS4) already folded Retopology and Bake away, which agrees with removing them. Rigging still needs `sculpt::remesh::field_mesh` (`rig/template.rs`), `retopo::Reference` (`rig/weights.rs`) and `sculpt::subdivide` (`rig_tests.rs`). Terrain sculpting is a different tool and stays. |
| 3 | Phase 91: one graph canvas | Done in this phase's PR, except the line-count goal (see the phase). | `node_graph` is now the `editor-node-graph` crate (#193). `block_editor/graph.rs` is 1,436 lines and `animation/graph.rs` 930. The material graph's own view is `editor/src/material_editor/graph.rs`. |
| 4 | Phase 92: legacy paths, old templates and dead weight | Done in this phase's PR (format 6; see the phase). | `project::FORMAT_VERSION` is already 5 (the release system's engine record, #98 era), so this phase bumps it to **6**, not 5. There are four alpha releases now (`releases/history`: 0.1.0-alpha.1 to .3 and 0.2.0-alpha.1). |
| 5 | Phase 93: a multithreaded frame | About half done by the roadmap. Physics runs on rapier's parallel feature in its own pool of at most 12 threads with the same results at any thread count (M3 #25, `physics/src/threads.rs`). Frame planning and culling run on the renderer's pool (`render/src/renderer/parallel.rs`). The nav mesh builds on a worker (M13 #32). The retained draw list (M4 #23) removed most of the draw-list build. | Left: animator posing across threads, mesh decoding on a shared pool, per-thread time in the Profiler, the 200-character and 2,000-body benchmark, and test runs with 1 and 8 threads. The modernization report (§ `rayon` usage) asks for the same: posing, audio diff and nav fingerprint over `&World`, applied in index order. "Limited on mobile" is deferred with Milestone 20. |
| 6 | Phase 80's open criterion: the 4 × 4 km benchmark | No benchmark scene exists for it. | Build a generated 50,000-part, 4 × 4 km scene as an ignored bench test. Measuring it needs the reference machine (the developer's PC). |
| 7 | Phase 81: lag compensation and replays | Not started. | Builds on `net` replication (Phases 63 to 66), not on Collaborate, which is a separate editor-sync system with its own thread. Replays record the replication stream, so they don't need cross-machine determinism, which the development policy rules out. |

**Hand checks.** These need a person, the developer's PC or Mac, or two computers. They stay unticked until checked, and the PC and Mac are booked through the project's coordinator.

- Open checks, Animation window: the old panel's features in the one Animation tab.
- Phase 25: an editor label change rebuilds and relaunches (re-checked with `--features engine-dev` in Phase 89).
- Phase 30: `MouseBehavior = LockCenter` with `GetMouseDelta` in a real window (the Lua side is covered by `script` `input_tests.rs`).
- Phase 53: `axcenity-test examples/hello-character` on Windows and macOS (Linux checked in the cloud).
- Phase 55: a Blender model opened and saved unchanged in Model Studio reopens in Blender the same.
- Phase 63: `FindLanGames` across two computers (the developer's PC and Mac).

**Checked in this audit.**

- Phase 63, headless server: `the_server_runs_headless_and_a_player_joins_it` passes in the cloud container, which is Linux with no display.
- Phase 72, backup on another computer: `a_backup_zip_opens_as_the_project_somewhere_else` unzips a backup into another folder and opens it. `back_up_project` writes every entry with `/` between folders whatever the OS, so a zip made on Windows unpacks the same on macOS and Linux.

**Not done, on purpose.** Milestone 20 (mobile) is **Deferred** by the development policy, and its criteria are marked so. Phase 26's two criteria stay **Removed** (Phase 89).

---

# Open checks and Phases 1 to 67

Built, and moved to [axcenity-engine-unified-done.md](axcenity-engine-unified-done.md) by Phase 92. 6 hand checks there are still open (unticked, not marked Removed).

---

# Milestones 15 to 19: design review of 2026-10-30

> **From**: a design review of 2026-10-30 (commit `bb7f2fe`). Milestones 1 to 14 and the graphics plan are built, apart from the open checks. This review compared the engine with Unreal and kept only what fits Axcenity's aim: easy first, with depth you can opt into. Each feature below is something Unreal creators expect that Axcenity doesn't have yet. Phase 68 comes first, then Milestone 21 (trimming), so new work starts from a smaller engine. The milestones after that can run in any order.

## Decisions for Milestones 15 to 19

- **Easy first**: every new feature works with no script from a template or a property, and has Lua and blocks for more control. Blocks parity and API reference entries still hold.
- **No new crates.** Spline and blend-space math has no egui, GPU or `core` dependency, so `physics`, `nav`, `anim` and the editor can share it. The developer console runs in the player, so it's built with the `ui` crate, never egui.
- **Files**: new optional files (`*.dialogue.json`, `*.replay`, `logs/`) and new optional fields (splines, decals, layers, IK) don't bump `project::FORMAT_VERSION`.
- **Roblox-familiar names** where Roblox has the idea (`Decal`, `IKControl`, `Workspace.StreamingEnabled`, the F9 Developer Console). Unreal-style names where it doesn't (`Spline`, `Ability`, `Effect`), and never either brand.
- **Dependencies**: Milestones 15 to 19 need none beyond what's pinned, apart from Phase 93's thread pool. **Changed**: version history (Phase 72) builds on the existing `project::versions` snapshots instead of Git.
- **Still not in this plan**: web export (Android and iOS are Milestone 20), Steam and platform services, relays and matchmaking, cloth and hair, large-world coordinates beyond about 10 km, and branching or merging in version history.

---

# Milestone 15: Finish and start easy

## Phase 68: Close the open checks

**From**: the design review of 2026-10-30. **Changed**: this phase replaces the **Open checks** section's "check by hand later". It must finish before new work starts.

### What to build

**Hand checks.** Walk every unticked criterion in this plan, including the **Open checks** section. Tick each one that passes. Where it fails, fix it or write a test for it. Where a criterion can be automated (gamepad input through a fake device, LAN discovery on two loopback addresses, a headless `axcenity-server` in CI), automate it.

**Not checked.** Phase 26 (hot-patching) is removed in Phase 89, so its two open criteria are marked **Removed**, not checked. Phase 25's open criterion is still checked: rebuild and relaunch stays, as a dev-only feature (Phase 89).

### Acceptance criteria

- [ ] No criterion in Phases 1 to 67 or in **Open checks** is unticked, unless it's marked **Deferred** or **Removed** with a reason.
- [x] Phase 26's open criteria are marked **Removed**, pointing to Phase 89.
- [x] `cargo nextest run --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` pass. (On macOS through `scripts/check-platform.sh`, which runs `cargo test --workspace` where nextest isn't installed.)

---

## Phase 69: Genre templates

**From**: the design review of 2026-10-30. Unreal opens with First Person, Third Person, Vehicle and Top Down templates. Today Axcenity has Baseplate, Empty and Starter.

### What to build

**Templates.** New Project gains five playable templates, built only from features that already exist:
- **First Person**: the first-person controller, a Tool that fires and damages targets, ammo on the HUD.
- **Third Person**: the Humanoid, a ProximityPrompt door, a checkpoint, coins saved to a slot.
- **Vehicle**: a drivable car, a lap timer from a data table, a seat prompt.
- **Top Down**: click to move through a NavigationAgent, the camera set top-down, enemies run by a behavior tree.
- **Multiplayer Arena**: teams, spawns, the leaderboard, chat, health and respawn.

Each has a pause menu, a settings menu with rebindable input, and a working save slot. Each uses a mix of Lua and blocks, with comments that say where to start changing things.

**Picking one.** New Project shows a picture and a one-line description for each template.

**Keeping them working.** Every template ships with `tests/*.spec.lua` game tests. The workspace test suite creates a project from each template and runs those tests.

### Acceptance criteria

- [x] Each template creates a project that plays with no errors in editor Play and as an exported game.
- [x] Each template's game tests pass in `axcenity-test` and in the Tests panel.
- [x] The Multiplayer Arena template works with `--host` and two `--join` clients.

---

# Milestone 16: Editor and shipping tools

## Phase 70: Command palette, search and asset references

**From**: the design review of 2026-10-30.

### What to build

**Command palette.** Ctrl+Shift+P (Cmd on macOS) lists every menu command with its shortcut. It's searchable, fuzzy, and shows the most recent commands first. Plugins' commands (Phase 54) appear too.

**Quick open.** Ctrl+P finds scripts, assets, levels and Instances by name, and opens or selects the one picked.

**Find in scripts.** Ctrl+Shift+F searches every script and block script, with match case, whole word and regex options. Results go to their line or block, and Replace All is one undo step.

**Asset references.** In the Assets panel, right-clicking an asset and choosing **Find References** lists every Instance, script (by literal asset path), material, prefab, level and data table that uses it. An **Unused** filter shows assets that nothing references. Renaming or moving an asset updates every reference as one undo step. Check first what Phase 22 already covers.

### Acceptance criteria

- [x] Every ribbon and menu command can be run from the palette.
- [x] Find in scripts finds a name used in both a Lua script and a block script, and Replace All changes both and undoes in one step.
- [x] Renaming a texture used by a material, a MeshPart and a script keeps all three working in Play.

---

## Phase 71: Developer console and logs

**From**: the design review of 2026-10-30. This is like Unreal's `~` console and stat commands.

### What to build

**Developer Console.** F9 opens it in editor Play and in development builds. Exported games turn it on only when the project setting `DeveloperConsole` is set, or with `--dev`. It's built with the `ui` crate. It has three tabs:
- **Log**: output and errors, filtered by level and side (server or client).
- **Stats**: FPS, frame time by stage, memory, Instance count, and, in multiplayer, ping and bandwidth.
- **Commands**: built-in commands (`fps`, `wireframe`, `collisions`, `noclip`, `timescale`, `teleport`) and Lua.

**Custom commands.** `ConsoleService:RegisterCommand(name, help, callback)` adds commands, with matching blocks. In multiplayer, a command runs on the server only when the player is allowed (`ConsoleService.AllowedUserIds`).

**Logs.** Exported games write `logs/latest.log` to the player's data folder for that game, and keep the last five logs. If the game crashes, it shows a message with the log's path before it exits.

### Acceptance criteria

- [x] In an exported development build, F9 opens the console, `timescale 0.5` slows the game, and a registered Lua command runs.
- [x] A shipped build with `DeveloperConsole` off doesn't open the console on F9.
- [x] A forced panic in the player leaves a log with the panic message, and the message names the log's path.
- [x] `player` still doesn't depend on `egui`.

---

## Phase 72: Better version history

**From**: the design review of 2026-10-30. **Changed**: this was Git-based. The editor already keeps snapshots on every save (`project::versions`, File > Version History, the newest 40). This phase makes those snapshots easier to use instead of adding a second history, so it needs no new dependency and no branches.

### What to build

**Checkpoints.** **Save Checkpoint** saves with a message and pins the snapshot, so it's never among the oldest that get dropped. Ordinary saves keep rolling as now.

**Timeline.** The Version History window lists snapshots with their date, message (or "Save"), and a pin.

**What changed.** Selecting a snapshot shows what changed in plain words: Instances added, removed or changed (matched by ID), and scripts and assets changed, with a line diff for scripts.

**Restore.** **Restore this file** or **Restore project to here** saves a snapshot of the current state first, so a restore can always be undone. Restore asks first, and refuses if there are unsaved changes.

**Backup.** **Back Up Project** writes a `.zip` of the project (no `.axcenity/` cache) to a chosen folder, such as a cloud-synced one, by hand or on every checkpoint. It uses the pinned `zip`.

### Acceptance criteria

- [x] Three pinned checkpoints survive 50 later saves, and restoring the first keeps a snapshot of what was replaced.
- [x] The change view names a moved Part and an edited script correctly.
- [x] A backup `.zip` opens as a project on another computer. Test: `a_backup_zip_opens_as_the_project_somewhere_else`; entries always use `/` (Audit of 2026-11-12).

---

# Milestone 17: World building

## Phase 73: Splines

**From**: the design review of 2026-10-30. This is like Unreal's Spline component.

### What to build

**Instances.** A `Spline` Instance has `SplinePoint` children: `Position`, and a tangent mode of Auto, Linear or Free with handles. It has `Closed`, and it shows in the viewport in edit mode only.

**Editing.** Clicking a spline enters point editing.
- Alt-drag on a point adds one.
- Delete removes one.
- Points snap to surfaces.
- Every edit is one undo step.

**Lua.** `Length`, `GetPositionAt(t)`, `GetCFrameAt(distance)` (it faces along the curve) and `GetClosestDistance(position)`. Matching blocks.

**Uses.**
- A `SplineFollower` mover moves an assembly along a spline at `Speed`, with `Loop`, `PingPong` or `Once` and a `Reached` event. It suits moving platforms, trains and gondolas.
- `NavigationAgent:Patrol(spline)` walks the spline's points in order.
- The Sequencer's camera track can ride a spline.

### Acceptance criteria

- [x] A platform on a `SplineFollower` with `PingPong` carries a standing character smoothly, in Play and in multiplayer.
- [x] `GetCFrameAt(Length / 2)` lands on the curve's midpoint, checked by a test on a known curve.
- [x] An NPC patrols a closed spline with no script, and a Sequencer camera rides one.

---

## Phase 74: Spline meshes and scatter

**From**: the design review of 2026-10-30. It's a small version of Unreal's procedural content tools.

### What to build

**Spline mesh.** `SplineMesh` (a child of a Spline) bends a MeshPart along the spline, repeating it to fill the length, for roads, pipes, fences and rails. Collision follows the bent mesh.

**Scatter.** `SplineScatter` places models along a spline (`Spacing`) or inside a closed one (`Density`).
- It uses a seed and ranges for rotation, scale and offset.
- It aligns to the surface below, and skips slopes steeper than a set angle.
- It uses the Assets panel's models, like foliage painting.

**Generated, not saved.** Results are rebuilt from the spline and seed when the project loads or the spline changes. `world.json` stores only the settings. **Bake** turns the results into ordinary Instances.

**Terrain.** A **Flatten Terrain Along** action flattens a road-width strip under the spline (one undo step).

### Acceptance criteria

- [x] A road built from one straight segment mesh follows a curving spline with no gaps, and a car drives on it.
- [x] Moving a spline point regenerates its scatter, and the same seed gives the same layout after reopening.
- [x] Scatter adds nothing to `world.json` but its settings, until Bake.

---

## Phase 75: Decals

**From**: the design review of 2026-10-30. It follows the graphics plan's rules: `render` receives decals in the draw list only.

### What to build

**Instances.**
- `Decal` (a Part's child) puts a texture on one `Face`, with `Color3`, `Transparency` and tiling.
- `ProjectedDecal` is a box with `Size` that projects its texture onto whatever it overlaps, with a fade by angle. It suits bullet holes, puddles, road markings and dirt. `IgnoreCharacters` keeps it off characters.

**From scripts.** `Workspace:SpawnDecal(texture, cframe, size, lifetime)` places a projected decal that fades out. A project-wide cap removes the oldest first. Matching blocks.

**Quality.** Projected decals follow the graphics settings' quality level, and turn off at the lowest one.

### Acceptance criteria

- [x] A road-marking ProjectedDecal sits on curved terrain with no gaps or z-fighting.
- [x] 500 bullet-hole decals spawned from a Tool stay under the cap and within the frame budget in the graphics reference scene.
- [x] Decals save, reload, and show the same in editor Play and an exported game.

---

# Milestone 18: Gameplay and animation depth

## Phase 76: Abilities and effects

**From**: the design review of 2026-10-30. This is a simple version of Unreal's Gameplay Ability System. It uses the Attributes and tags that exist since Phase 29.

### What to build

**Ability.** An `Ability` Instance (under a Model or Player) has:
- `Cooldown`, a `Cost` (an Attribute name and amount, such as Mana 20), and `ActivationKey` (an input-map action).
- `BlockedByTags` and `GrantsTags`.
- `CanActivate()`, `Activate()` and `Cancel()`, and `Activated`, `Ended` and `CooldownChanged` events.

**Effect.** An `Effect` Instance changes an Attribute, or a Health or Humanoid property:
- It can be instant, last for a time, or repeat every tick (a heal over time).
- It adds or multiplies.
- It stacks up to `MaxStacks`.
- It adds tags while active.

Removing an Effect undoes its change. `Model:ApplyEffect(effect)` clones one onto a target.

**Easy start.**
- The ready-made widgets (Phase 9h) gain an ability bar with cooldown sweeps.
- Abilities and effects can be made from data tables.

**Network.** Abilities activate on the server, which checks cost, cooldown and tags. Cooldowns and Effects replicate, and the client shows the cooldown sweep at once.

### Acceptance criteria

- [x] A Fireball ability costs 20 Mana, has a 3-second cooldown, and can't activate while the player has the `Stunned` tag, with no script beyond the damage.
- [x] A 10-second Poison effect does damage every second, stacks to 3, and leaves Health unaffected once removed.
- [x] In multiplayer, a modified client can't skip a cooldown.

---

## Phase 77: Dialogue and quests

**From**: the design review of 2026-10-30.

### What to build

**Dialogue asset.** A `*.dialogue.json` asset is edited in the generic node-graph editor (Phase 11):
- **Line** nodes have a speaker, text from the string table (Phase 20), and optional audio and animation.
- **Choice** nodes have options.
- **Condition** nodes check Attributes, tags, data tables, or Lua and blocks.
- **Action** nodes set Attributes, give items, start quests, or run Lua and blocks.

**Running it.**
- `DialogueService:Start(dialogue, speakerModel)` runs one, yields until it ends, and returns the ending's name.
- A ready-made dialogue box widget shows lines and choices. It's CSS-styleable, and works with keyboard and controller navigation.
- A ProximityPrompt can start a dialogue with no script.

**Quests.** `QuestService` holds quests from a data table: objectives (count, reach a place, talk to someone), `Start`, `Progress`, `Complete`, and events. A quest tracker widget shows them. Quest state saves in the player's slot (Phase 17).

### Acceptance criteria

- [x] An NPC with a ProximityPrompt runs a branching dialogue that gives a quest, with no script.
- [x] Collecting 5 items completes the quest, and the progress survives saving and loading.
- [x] Switching language changes the dialogue text.

---

## Phase 78: Animation layers and blend spaces

**From**: the design review of 2026-10-30. Today blend trees are 1D, and a state machine animates the whole body.

### What to build

**Layers.** An Animator has layers. Each layer has its own state machine, a weight, a mode (Override or Additive), and a bone mask. The mask is picked in the rig tree, for example "Spine and below". So a character can run with its legs while it aims or reloads with its upper body.

**2D blend spaces.** A state's motion can be a 2D blend space: two Number parameters (for example, forward speed and sideways speed), with clips placed on a grid. It blends across triangles. An **Aim offset** is an additive 2D blend space driven by pitch and yaw.

**Lua.** `Animator:SetLayerWeight(name, weight)` (it can fade over time) and `GetLayerWeight`. Matching blocks.

**Network.** Layer weights replicate with the other Animator parameters.

### Acceptance criteria

- [x] A character strafes in 8 directions from one 2D blend space, with no foot sliding at the sample points.
- [x] An upper-body reload layer plays while running, and the legs keep running.
- [x] Projects with only 1D blend trees play the same as before.

---

## Phase 79: Runtime IK

**From**: the design review of 2026-10-30. **Changed**: Milestones 9 to 14 left runtime IK out. It now uses the IK solver from Phase 8c.

### What to build

**IKControl.** An `IKControl` Instance has:
- a `Type` of Transform, Position, Rotation or LookAt;
- `ChainRoot`, `EndEffector`, `Target` (a Part or Attachment) and `Pole`;
- `Weight` and `SmoothTime`.

It runs after the Animator and before drawing. It suits hands on a steering wheel, a head that looks at the player, and a hand on a door.

**Feet.** A character controller's `FootPlacement` option casts down from each foot, plants feet on slopes and stairs, and lowers the hips to match. It's on by default in the Third Person template.

**Network.** `Weight` and `Target` replicate. Foot placement runs on each device.

### Acceptance criteria

- [x] An NPC with a LookAt IKControl turns its head to follow the player within its limits.
- [x] A character standing on stairs has both feet on a step, with no knee bending backwards.
- [x] Turning IK off returns to the plain animation within `SmoothTime`.

---

# Milestone 19: Large worlds and competitive multiplayer

## Phase 80: World streaming

**From**: the design review of 2026-10-30. This is like Unreal's World Partition, with Roblox-familiar names.

### What to build

**Settings.** `Workspace.StreamingEnabled`, `StreamingMinRadius` and `StreamingTargetRadius`. `Model.ModelStreamingMode` is Default, Atomic or Persistent.

**What streams.**
- **For joined clients**, the server replicates only what is within each client's radius, as a model at a time, and removes what leaves it.
- **For the host viewer** (who is on the server World, from Phase 64), meshes, textures and terrain chunks outside the radius are unloaded from the GPU. They load again on a background thread when they come back in range, and far physics sleeps.

**Lua.** `Player:RequestStreamAroundAsync(position)` loads a region before a teleport. `Instance.StreamingState` and a `StreamedIn` event let scripts handle parts that aren't there. Matching blocks. The editor shows the radius in Play.

### Acceptance criteria

- [ ] A 4 × 4 km world with 50,000 parts plays on the reference machine within the frame budget, with streaming on.
- [x] Teleporting after `RequestStreamAroundAsync` lands on loaded ground.
- [x] A Persistent model is always there on every client, and scripts that check `StreamingState` don't error.
- [x] A project with streaming off plays exactly as before.

---

## Phase 81: Lag compensation and replays

**From**: the design review of 2026-10-30.

### What to build

**Lag compensation.**
- The server keeps one second of history of character and part positions.
- With `RaycastParams.CompensateFor = player`, a server raycast checks hits against where things were when that player fired (their ping, up to a limit).
- Tools (Phase 45) can turn this on with `Tool.LagCompensated`.

**Replays.**
- `ReplayService:StartRecording()` and `StopRecording(name)` save the replication stream to a `.replay` file in the player's data folder.
- `axcenity-player --replay file` plays one back, with a free camera, a follow camera, pause, speed and a timeline.
- `ReplayService.BufferSeconds` keeps the last N seconds in memory, so `PlayBuffer()` gives a kill cam or an instant replay.

Replays work in single-player too, because it runs over the same replication.

### Acceptance criteria

- [ ] With 150 ms of emulated latency, a shot that hits a moving target on the shooter's screen counts on the server.
- [ ] A recorded two-minute multiplayer match plays back, and the same frame looks the same each time.
- [ ] A kill cam shows the last 5 seconds from the killer's view.

---

# Milestone 20: Android and iOS

> **Deferred** (2026-11-10, [development policy](../docs/development-policy.md#platforms)). Mobile isn't an immediate priority: desktop comes first, with Windows and Linux as the primary platforms. Don't start these phases, and don't shape desktop code around them beyond keeping it reasonably portable. Revisit once the desktop engine is reliable. The CI criteria below (for example Phase 84's mobile builds) would also need CI, which is off on purpose.

**From**: the developer's request of 2026-10-30. **Changed**: Milestones 9 to 14 and the review of 2026-10-30 left mobile export out. Phase 68 comes first. Phases 82 and 83 need no device, so they can start at once. Phases 84 to 88 run in order.

## Decisions for Milestone 20

- **Only the player goes to mobile.** The editor, `axcenity-server`, `axcenity-test`, editor plugins, `--watch`, hot reload and hot-patching stay on the desktop. Creators build mobile games from the desktop editor.
- **One player, three shapes.** `player` becomes a library with one entry point:
  - The desktop binary is a thin wrapper around it.
  - Android loads it as a `cdylib` through winit's NativeActivity support, so there's no Java code and no Gradle.
  - iOS links it as a `staticlib`.

  `player` still never depends on `editor`, `egui` or `blocks`.
- **Targets**: `aarch64-linux-android` (plus `x86_64-linux-android` for the emulator), and `aarch64-apple-ios` (plus `aarch64-apple-ios-sim` for the simulator). Graphics run on wgpu's Vulkan backend on Android and Metal on iOS.
- **Minimum devices**, to confirm at the start of Phase 84: Android 10 (API 29) on arm64 with Vulkan 1.1, and iOS 15. Two reference devices are used for the frame budget: a mid-range Android phone from about 2021, and an iPhone 11 or newer.
- **Creators don't need Rust.**
  - GitHub release packages ship prebuilt mobile players under `players/android` and `players/ios`, next to `players/windows`, `players/macos` and `players/linux`.
  - Android export needs nothing else installed.
  - iOS export needs a Mac with Xcode, because Apple only signs apps through Xcode. It writes an Xcode project the creator opens, runs and archives.
- **Scripting**: Luau runs as the interpreter only, with no native code generation, because iOS forbids JIT. The workspace already builds `mlua` with only the `luau` feature, so this changes nothing on the desktop.
- **Input**: touch is the main input. `gilrs` has no Android or iOS backend, so it's left out of mobile builds, and controllers on mobile are not in this milestone. Every input-map action can have an on-screen touch button.
- **Files**: `game.axgame` goes inside the APK or the app bundle. The player reads it there, as it unpacks the archive on the desktop now. `save::game_folder` uses the app's private data folder on each platform. Exported mobile games never write outside it.
- **Networking**: joining by address works everywhere, and Android can host on the LAN. On iOS, `FindLanGames` needs Apple's multicast entitlement, so it's off unless the creator turns it on in export settings and has the entitlement. Mobile and desktop players can play together.
- **Dependencies to choose and licence-check when their phase starts** (pin each in the workspace and run `cargo hakari generate`):
  - JNI access for the Android soft keyboard and haptics (Phase 84; for example `jni`).
  - Android audio for `kira`'s output, through `cpal`'s AAudio support (Phase 84).
  - Texture compression to ASTC and ETC2 (Phase 85; for example Basis Universal, which is Apache-2.0).
  - Building and signing an APK in pure Rust (Phase 86; for example the `rust-mobile` packaging crates). Otherwise, as a fallback, Android's own build tools found on the machine.
- **Files on disk**: mobile export settings are new optional fields in `axcenity.json`, so `project::FORMAT_VERSION` doesn't change.

---

## Phase 82: Mobile preview in the editor

**From**: the developer's request of 2026-10-30. It needs no device.

### What to build

**Device preview.** Editor Play gains a **Device** menu: Desktop, Phone portrait, Phone landscape and Tablet, plus custom sizes.
- A device preview shows the game at that screen's aspect ratio and density, with its notch and home bar. It uses the same safe insets the UI Designer artboard uses (Phase 9g).
- The mouse acts as one finger. Holding Alt adds a second finger, mirrored around the centre, to test pinch and two-finger drag.

**Input flags.** `UserInputService` gains `TouchEnabled`, `KeyboardEnabled`, `MouseEnabled` and `GamepadEnabled`, and `PreferredInput` (Touch, KeyboardAndMouse or Gamepad, with a `PreferredInputChanged` event). The device preview reports a phone's flags. Matching blocks.

**Mobile graphics.** The preview can use the **Mobile** graphics preset (Phase 85 fills it in). Until then, it uses Low.

### Acceptance criteria

- [ ] **Deferred** (Milestone 20, development policy): In the Phone portrait preview, a `DeviceSafeInsets` ScreenGui stays clear of the notch and home bar, as on the UI Designer artboard.
- [ ] **Deferred** (Milestone 20, development policy): A LocalScript that checks `TouchEnabled` sees true in the phone preview and false on Desktop.
- [ ] **Deferred** (Milestone 20, development policy): `TouchStarted`, `TouchMoved` and `TouchEnded` fire from the mouse in the preview, and the second finger gives a pinch.

---

## Phase 83: On-screen touch controls

**From**: the developer's request of 2026-10-30. **Changed**: `ContextActionService`'s `createTouchButton` is accepted today but draws nothing (`docs/scripting/client-server.md`). This phase draws it.

### What to build

**Default controls.** When `TouchEnabled` is on:
- A character gets a movement thumbstick on the left. Dragging anywhere else turns the camera, and pinching zooms it.
- A jump button sits on the right.
- These replace the keyboard keys of the input map's Move, Jump and Crouch actions, with no script.

**Action buttons.** Each input-map action can be marked **Show touch button**, with an icon, a position and a size. `BindAction(..., true, ...)` makes a button. `ContextActionService` gains `SetTitle`, `SetImage`, `SetPosition` and `GetButton`, as Roblox-familiar scripts expect. Buttons are CSS-styleable and keep inside the safe area.

**Built-in features.** A ProximityPrompt shows a tappable button and supports holding. A Tool's activate button appears when a Tool is equipped. Vehicles get a steering stick with throttle and brake buttons. Seats get an exit button.

**Haptics.** `HapticService:IsVibrationSupported()`, `SetMotor(strength, duration)` and `Vibrate(pattern)`. They do nothing on a device without vibration. Matching blocks.

**Settings.** `StarterPlayer.TouchMovementMode` (Thumbstick, DynamicThumbstick or ClickToMove), and `TouchControlsEnabled`, so a game can turn the defaults off and draw its own.

**Templates.** All five genre templates (Phase 69) work with the default touch controls in the phone preview.

### Acceptance criteria

- [ ] **Deferred** (Milestone 20, development policy): In the phone preview, with no script, the Third Person template's character walks with the thumbstick, turns the camera by dragging, jumps with the button, and opens a door with the prompt's button.
- [ ] **Deferred** (Milestone 20, development policy): An action bound with `createTouchButton` true shows a button that fires the handler with Begin and End, and `SetPosition` moves it.
- [ ] **Deferred** (Milestone 20, development policy): Turning `TouchControlsEnabled` off hides every default control.
- [ ] **Deferred** (Milestone 20, development policy): Desktop Play with a mouse shows no touch controls.

---

## Phase 84: The mobile player

**From**: the developer's request of 2026-10-30. **The `player` crate becomes a library here.**

### What to build

**Library player.** The player's game loop moves from `main.rs` into the library, behind one entry point that takes a game and a window. The desktop binary, the Android `cdylib` and the iOS `staticlib` all call it. Desktop behaviour doesn't change.

**Life cycle.** The game handles each state:
- **Going to the background**: scripts, physics and audio pause, the slot autosaves when SaveService has autosave on, and `GameService.Suspended` fires.
- **Coming back**: the drawing surface is rebuilt (Android destroys it), audio resumes, and `GameService.Resumed` fires.
- **Rotating or resizing**: the game resizes, and safe insets update.
- **Low memory**: the asset cache is released, and `GameService.LowMemory` fires.

**Text input.** A focused TextBox shows the soft keyboard, and the game layout moves so the field stays visible. It's hidden again when focus leaves. On Android this goes through JNI. On iOS it goes through winit's IME.

**Audio.** `kira` plays through AAudio on Android and AVAudioSession on iOS. Other apps' audio is ducked while the game plays. Muting by the silent switch follows `SoundService.RespectSilentSwitch`.

**A development path for the engine.**
- A `scripts/mobile` script builds the player for the emulator and simulator, packs a game folder into it, and installs and runs it.
- CI builds `axcenity-player` for every mobile target, so the cross-platform invariant covers them.

### Acceptance criteria

- [ ] **Deferred** (Milestone 20, development policy): `examples/hello-part` and `examples/hello-character` run on the Android emulator and the iOS simulator, with touch controls.
- [ ] **Deferred** (Milestone 20, development policy): Sending the game to the background and back keeps its state, sound and picture, on a real Android phone and a real iPhone.
- [ ] **Deferred** (Milestone 20, development policy): A TextBox shows the soft keyboard, and typed text arrives in Lua.
- [ ] **Deferred** (Milestone 20, development policy): A save slot written on the phone is still there after the app is closed and opened again.
- [ ] **Deferred** (Milestone 20, development policy): `cargo build --workspace` on Windows, macOS and Linux is unchanged, and CI builds all four mobile targets.

---

## Phase 85: Mobile graphics and performance

**From**: the developer's request of 2026-10-30. It follows the graphics plan's rules: `render` receives only the draw list. The Mobile preset changes settings, not features.

### What to build

**Mobile preset.** Graphics settings (graphics Phase 2) gain a **Mobile** preset, chosen by default on phones and tablets. It sets:
- render scale below 1;
- FXAA instead of TAA;
- shorter shadow distance and one cascade;
- no screen-space reflections, real-time GI or ray tracing;
- fewer particles;
- lower LOD distances.

Features that a phone's GPU can't run fall back quietly, and the Graphics profiler names what fell back.

**Frame rate and battery.**
- `GameSettings.FrameRateCap` is 30, 60 or the display's maximum, and is 60 by default on mobile.
- When the device reports it's getting hot, the render scale steps down until it cools, and `GameService.ThermalStateChanged` fires.

**Texture compression.**
- Build Game for mobile compresses textures to ASTC, with ETC2 as the Android fallback.
- Each texture's import settings can turn this off (for UI that must stay sharp) or pick a quality.
- Desktop builds don't change.

**Budgets.** Build Game warns when a mobile build is over a size or memory budget, and lists the largest assets.

### Acceptance criteria

- [ ] **Deferred** (Milestone 20, development policy): The graphics reference scene holds 30 FPS on both reference devices with the Mobile preset, and the Third Person template holds 60.
- [ ] **Deferred** (Milestone 20, development policy): A mobile build of the graphics reference scene uses at most a third of the texture memory of the uncompressed build.
- [ ] **Deferred** (Milestone 20, development policy): Forcing a thermal warning lowers the render scale, and the game keeps running.
- [ ] **Deferred** (Milestone 20, development policy): Every graphics feature either runs or falls back on both reference devices, with no crash or blank screen.

---

## Phase 86: Android export

**From**: the developer's request of 2026-10-30.

### What to build

**Build Game > Android.** It writes a signed `.apk` from the prebuilt player in `players/android`, the game's `game.axgame`, and the generated manifest. No Android SDK, Java or Gradle is needed.

**Export settings** (in File > Build Game, saved in `axcenity.json`):
- the package ID (for example `com.studio.game`);
- the app name, the version name, and a version code that goes up by itself;
- orientation (portrait, landscape or both);
- an icon (one square image, made into every size and an adaptive icon);
- a launch background colour;
- permissions: none by default, network added when the game uses NetworkService;
- **Keep the screen on**.

**Signing.** A debug key is made the first time and kept in the user's editor settings, never in the project. A release keystore can be chosen for builds you publish. Its passwords are never saved in the project.

**Install and run.** If a phone with USB debugging, or an emulator, is found through `adb`, **Build and Run** installs the game and starts it. The game's log streams into the editor's Output. Without `adb`, the editor says where the `.apk` is and how to install it.

### Acceptance criteria

- [ ] **Deferred** (Milestone 20, development policy): On a computer with no Android tools, Build Game > Android makes an `.apk` that installs and runs the Third Person template on the reference Android phone.
- [ ] **Deferred** (Milestone 20, development policy): The installed app shows the chosen name, icon and orientation.
- [ ] **Deferred** (Milestone 20, development policy): Building a new version over an installed one keeps its save slots.
- [ ] **Deferred** (Milestone 20, development policy): Build and Run with `adb` shows the game's `print` output in the editor.

---

## Phase 87: iOS export

**From**: the developer's request of 2026-10-30. Apple signs apps only through Xcode, so this phase writes an Xcode project instead of a finished app.

### What to build

**Build Game > iOS.** On macOS, this writes a folder with an Xcode project, the prebuilt player library from `players/ios`, `game.axgame`, and the generated files:
- `Info.plist`;
- an asset catalog with every icon size;
- a launch screen;
- the privacy manifest Apple requires (`PrivacyInfo.xcprivacy`), filled in from what the game uses.

The settings are the same as Android's: bundle ID, name, version, build number, orientation, icon and launch colour. A **Team ID** field is added. On Windows and Linux, the iOS target is unavailable and says why.

**Signing.** The project uses Xcode's automatic signing with the Team ID, so a creator only signs in to Xcode once.

**Run.**
- **Open in Xcode** opens the project.
- **Build and Run** builds and runs it in the simulator through `xcrun`, and on a connected iPhone when signing is set up.
- The game's log streams into the editor's Output.

**Rebuilding.** Exporting again over an existing folder replaces the game and generated files, and keeps any changes the creator made in Xcode outside them.

### Acceptance criteria

- [ ] **Deferred** (Milestone 20, development policy): On a Mac with Xcode, Build Game > iOS and then Build and Run runs the Third Person template in the iOS simulator.
- [ ] **Deferred** (Milestone 20, development policy): With a Team ID, the same project runs on the reference iPhone with no changes made by hand in Xcode.
- [ ] **Deferred** (Milestone 20, development policy): The installed app shows the chosen name, icon, launch screen and orientation.
- [ ] **Deferred** (Milestone 20, development policy): Exporting again keeps a capability added by hand in Xcode.

---

## Phase 88: Store-ready builds

**From**: the developer's request of 2026-10-30. This phase covers what Google Play and the App Store require beyond a working app.

### What to build

**Google Play.**
- Build Game > Android gains **App Bundle (`.aab`)** for Play uploads, signed with the release keystore.
- The target API level follows Play's current minimum and is set in one place, so updating it is one change.

**App Store.**
- The Xcode project archives with no changes, so **Product > Archive** and upload to TestFlight work.
- Encryption export compliance is declared in `Info.plist`: standard HTTPS and the transport's encryption only.

**Checks before building.** A **Store check** lists what would be rejected before upload:
- a missing icon size or a default name or ID;
- a version that isn't higher than the last one built;
- an asset over the size limit;
- a permission the game doesn't use;
- the developer console turned on in a release build.

**Docs.** A new topic file, `docs/project/mobile.md`, covers the steps from export to store listing for both stores. `docs/architecture/portability.md` lists Android and iOS as targets.

### Acceptance criteria

- [ ] **Deferred** (Milestone 20, development policy): An `.aab` built from the Third Person template uploads to a Google Play internal testing track with no errors.
- [ ] **Deferred** (Milestone 20, development policy): An archive of the same template uploads to TestFlight and installs on the reference iPhone.
- [ ] **Deferred** (Milestone 20, development policy): The Store check catches each problem in its list when it's set up on purpose.

---

# Milestone 21: Trim and tighten

**From**: the developer's request of 2026-10-30 to cut what the engine doesn't need. It runs right after Phase 68 and before new feature work, so later phases build on a smaller engine. Phase 91 must land before Phase 77, which uses the shared graph editor. Phase 93 must land before Phases 80 and 85, which need its threads.

## Decisions for Milestone 21

- **Engine-developer tools leave creators' editors; game files hot reload like s&box.**
  - Hot-patching engine code (Phase 26) is removed, with its `subsecond` and `dioxus-*` dependencies.
  - Rebuild and relaunch (Phase 25) stays, behind a new `engine-dev` cargo feature. It's off in release builds, and on for whoever works on the engine.
  - **Changed** (2026-11-12): scripts keep their state when they reload during Play, as s&box's hotload does (Phase 89).
- **Model Studio is a low-poly modeling, UV, texture-painting and rigging tool.** **Changed** from Milestone 13: sculpting (Phase 60), retopology and baking (Phase 61) are removed. Creators who need them use Blender. They import the result as `.glb` or FBX, which already works, with normal maps through materials.
- **Old projects still open.** Alpha releases are public (`v0.1.0-alpha.3`), so every format migration stays. Legacy *runtime* paths go: files are upgraded on load, so each reader has only one shape.
- **Removing never loses work.** A removed feature's saved data is ignored when loaded, with a one-time note in Output. Saving doesn't touch files it doesn't understand, except as the migration says.
- **Measured.** Each phase records lines of code, editor build time (clean and incremental), and editor binary size, before and after, in its commit message.

---

## Phase 89: Game files hot reload like s&box, engine hot-patching out

**From**: the developer's request of 2026-10-30, **Changed** by the requests of 2026-11-12: "keep hot reloading but make it more similar to that of s&box", and "hotpatch game files, not engine files". In s&box, saving a code file recompiles it and swaps it into the running game within milliseconds, and the game keeps its state: objects' field values are copied into the new code, and runtime values win over changed defaults ([hotloading](https://sbox.game/dev/doc/code/advanced-topics/hotloading.md), [wiki](https://wiki.facepunch.com/sbox/Hotload)). Axcenity's game code is Luau, so scripts get the same behaviour. Assets already reload in place (Phase 23). The engine's own Rust is not hot-patched any more.

### What to build

**Remove engine hot-patching.** Delete `editor/src/hot_patch.rs`, `editor/src/hot_patch/`, `app/hot_patch_tests.rs` and their calls in `app.rs`, `main.rs`, `app/session.rs` and `app/engine.rs`. Remove the `hot-patch` feature and its dependencies (`subsecond`, `dioxus-devtools`, `dioxus-cli-config`) from the editor and the workspace, then run `cargo hakari generate`. Remove `dx serve --hot-patch` from the README.

**Rebuild and relaunch behind `engine-dev`.** `rebuild.rs`, `app/engine.rs`, and the menu items and file watching for engine sources compile only with the `engine-dev` feature. Without it, the editor never watches `crates/` and shows no engine menu items. The README's engine-development section says to run the editor with `--features engine-dev`.

**Scripts keep their state.** Saving a Script, LocalScript or ModuleScript during Play swaps the new code in and keeps what the game has built up, instead of Phase 24's rerun from scratch.
- The new code runs as Phase 24 does: the old code's connections are disconnected and its threads dropped, and the new code's top level runs, so it connects its handlers again.
- **State carries over by name**, as s&box copies fields. Each top-level `local` that the new code also declares keeps its old value, and its initializer doesn't run, so `local part = Instance.new("Part")` doesn't make a second part. Values that are code are never kept: functions, threads and connections take the new initializer.
- **Edited values win.** A local whose initializer text changed takes the new value, so changing `local speed = 10` to `20` takes effect. This differs from s&box on purpose: there, changed defaults wait for a restart. `-- @reset` at the end of a local's line always takes the new initializer.
- **Modules keep their identity.** A ModuleScript runs again at once with its state kept. When it returns a table, scripts that already required it keep that same table, which now holds the new functions, so they aren't rerun. A module that returns something else reruns the scripts that required it, as before.
- **How.** In the editor's Play only, each script's top-level locals are wrapped as it loads (the Luau reader in `blocks`, through a hook the editor gives `ScriptRuntime`, so `script` and the player gain nothing). Lines stay where they were, so errors and breakpoints point at the same lines. A script the reader can't read loads as it is and reloads as in Phase 24.
- **Saying what happened.** Output says "Reloaded Door.server.lua, kept count and part" (up to four names, then how many more). A syntax error keeps the old code running, as now. Block and graph scripts hot reload the same way, because their Lua is generated.

**Docs and wiki.** `docs/editor/scripts.md` (Live scripts; Hot-patching removed), the editor's `CLAUDE.md`, `docs/development.md`, the README, the wiki's live-reload pages and dependency list, and an ADR for keeping script state across reloads.

### Acceptance criteria

- [x] `cargo build -p axcenity-editor` doesn't compile `subsecond` or any `dioxus` crate, and `Cargo.lock` no longer lists them.
- [x] A release editor shows no Rebuild or Relaunch items, and editing a file under `crates/` does nothing.
- [x] `cargo run -p axcenity-editor --features engine-dev` still rebuilds and relaunches after a Rust change, keeping project, camera, selection and unsaved edits. (Relaunch tests pass in the cloud; a hand check on the PC is queued.)
- [x] During Play, changing the text a `Touched` handler prints keeps the script's counter: the next touch prints the new text with the old count, and no second part is made.
- [x] Changing a local's initializer takes the new value; `-- @reset` takes it too.
- [x] A script that required a ModuleScript uses the module's new function after a save, without rerunning, and the module's data is kept.
- [x] A syntax error keeps the old code running, and Phase 24's tests still pass. Live asset reload (Phase 23) works as before.
- [x] Exported games and the player run scripts exactly as before: nothing is wrapped.

---

## Phase 90: A narrower Model Studio

**From**: the developer's request of 2026-10-30. **Changed**: Phases 60 and 61 are removed.

### What to build

**Keep what rigging needs.** Rigging uses some sculpt and retopology code:
- the voxel remesh field in `rig/template.rs`;
- `retopo::Reference` in `rig/weights.rs`;
- `sculpt::subdivide` in rig tests.

First move these into a neutral `mesh_edit` module (for example `remesh.rs` and `reference.rs`), with their tests.

**Remove sculpting.** Delete `mesh_edit/src/sculpt*` (brushes, masks and sculpt levels), `editor/src/model_studio/sculpt*`, Sculpt mode in the Studio's toolbar, its shortcuts, and the mask-colour overlay items it drew.

**Remove retopology and baking.** Delete `mesh_edit/src/retopo*`, `mesh_edit/src/bake*`, `editor/src/model_studio/retopo*`, and their modes, menus and shortcuts.

**Old sidecars.** A `.meshedit.json` with sculpt levels, masks or retopo references still opens. The `.glb` is the model as last saved, so its shape doesn't change. The unused fields are dropped the next time it's saved, with a one-time note in Output.

**Docs.**
- `docs/editor/model-studio.md` loses its sculpt, retopology and baking sections. It gains a short "Sculpting and baking in Blender" note: export `.glb` or FBX, then Import, with the normal map added to the material.
- `docs/architecture/crates.md` updates the `mesh_edit` description.
- Phases 60 and 61 here are marked **Removed**.

### Acceptance criteria

- [x] Rigging a Mixamo-shaped model and painting weights works exactly as before. Phase 62's tests pass.
- [x] A model saved with sculpt data before this phase opens with the same shape and saves without that data. (Sidecars never held sculpt or retopology data, only the mesh they made, so there's nothing to drop and no Output note; `a_sidecar_with_fields_no_longer_read_still_opens_and_saves_without_them` checks that unknown fields are ignored and left out on save.)
- [ ] A normal map baked in Blender and imported shows the same detail on a low-poly model in Play. (Hand check on the PC, queued; imported normal maps already draw through materials.)
- [x] `mesh_edit` and the editor together are at least 5,000 lines smaller (about 5,800 today, less what moves for rigging). Measured: 6,184 lines removed, 551 added, 5,633 net (`mesh_edit` 22.8k → 19.3k).

---

## Phase 91: One graph canvas

**From**: the developer's request of 2026-10-30. The shared `node_graph` editor (Phase 11) already runs the UI Event Graph, behavior trees and the material graph. The block **Graph** view (`block_editor/graph.rs`) and the Animation tab's state machines (`animation/graph.rs`) each have their own canvas. **Changed**: state machines are still not node graphs as data. Only their canvas is shared.

### What to build

**One canvas.** `node_graph` gives every graph the same canvas:
- pan, zoom and fit;
- box selection, copy and paste;
- comment boxes;
- wire drawing and rerouting;
- a minimap;
- the same shortcuts.

The block Graph view and the state machine view keep their own node rules and draw on that canvas, and their own copies of these parts are deleted.

**Same everywhere.** F frames the selection, Space opens search, and Delete, Ctrl+D and Ctrl+C or V work the same in all five graph views.

### Acceptance criteria

- [x] Every existing graph test passes unchanged: blocks, state machines, UI Event Graph, behavior trees and material graphs. (One state machine test reads the pan as `view.pan` instead of `view.offset`; its checks are the same.)
- [x] A script switches between Code, Blocks and Graph and runs the same, as before.
- [x] The same shortcuts work in all five graph views, checked by one shared test (`editor/src/graph_keys_tests.rs`).
- [ ] `block_editor/graph.rs` and `animation/graph.rs` together are at least 800 lines smaller. (Not met: 2,366 to 2,346. About 850 lines of their own canvas code went into `editor-node-graph/src/canvas.rs` (641 lines), but both views gained copy and paste, the shared keys and the state machine a minimap. The 800 assumed more of them was shared than was.)

**Done (Phase 91)**: `editor-node-graph/src/canvas.rs` holds pan, zoom and fit, box select, the shared keys, the searchable menu, pins, wires, the minimap and notices. Every view uses it. Comment boxes and reroute points stay in the `node_graph` views only: block scripts and state machines have nowhere in their files to keep them.

---

## Phase 92: Legacy paths, old templates and dead weight

**From**: the developer's request of 2026-10-30.

### What to build

**One shape per file.** Migration upgrades old file shapes on load, so readers lose their legacy branches. This bumps `project::FORMAT_VERSION` to 5, with a migration that keeps a copy of the previous files, as the rules require. The legacy branches are:
- `anim`'s `legacy_clip` and `LEGACY_CONTROLLER_EXTENSION` (`.controller.json`);
- `blocks-runtime`'s `Slot::Legacy` condition shape;
- any other `legacy` or compatibility readers found by search.

**Old Lua.** The `animation:Play()`, `Stop()`, `Pause()` and `Resume()` methods on Animation (`note_legacy_animation`) become an error that names the fix: `animator:LoadAnimation(animation)`. Block scripts that used them are rewritten by the migration.

**Templates.** Once Phase 69 is built, the Starter template is removed, because the genre templates replace it. Baseplate and Empty stay. Existing projects aren't affected.

**Dead weight.**
- Run the `organize` skill's dead-code pass.
- Remove workspace dependencies that no crate uses, then run `cargo hakari generate`.
- Remove the `#[allow(dead_code)]` items that are really dead.

**Plan.** Move the built Phases 1 to 67 and **Open checks** to `plans/axcenity-engine-unified-done.md`. Leave a one-line pointer here.

### Acceptance criteria

- [x] Every `examples/` project and one project from each alpha release open, migrate once, and play the same. Tests: `every_example_saved_by_0_2_0_alpha_1_upgrades_once_and_opens_the_same` (each example at format 5, as 0.2.0-alpha.1 wrote it, opens with the same world and scripts and upgrades once) and `an_older_project_plays_the_same_once_migrated_as_one_made_in_the_new_format` (format 2, as the 0.1.0 alphas wrote, now through to 6). The examples are at format 6 in the repository.
- [x] A search for `legacy` in `crates/` finds only migrations and their tests. (`PhysicalProperties::LEGACY` is now `CLASSIC`, and the MCP test's older-protocol client is `older_protocol_client`.)
- [x] `animation:Play()` gives a clear error naming `LoadAnimation`. Test: `animation_play_and_the_rest_are_errors_that_name_load_animation`.
- [ ] Clippy with `-D warnings`, nextest and doc tests pass, with no unused dependencies left. (In the cloud: clippy with `-D warnings` is clean across the workspace, doc tests pass, and every crate's `cargo test` passes (3,700+ tests) apart from two this phase doesn't touch: `every_tutorial_sample_opens_and_plays_in_the_editor` needs a sound device, and render's `a_translucent_leaf_glows_green_with_the_sun_behind_it` fails on the container's software GPU. nextest and `cargo hakari generate` aren't installed there, so both are left for the PC.)

**Done (Phase 92)**: project format 6 (`project/src/migrate_shapes.rs`) rewrites any clip or state machine still in animation format 1, and block-script conditions in the first `{ left, operator, right }` shape, once; `anim` and `blocks-runtime` refuse those shapes now, and the 2 → 3 step reads them through the same functions. `LEGACY_CONTROLLER_EXTENSION` lives only in `migrate_animation.rs`. `animation:Play()`, `Stop`, `Pause` and `Resume` are errors naming the fix (and the unused `pause`, `resume` and `paused` playback fields went); the UI Designer's Play on click writes `animator:LoadAnimation(animation):Play()`. Block scripts needed no rewrite: the Play animation block has used `LoadAnimation` since v3. The Starter scene template is gone (Baseplate, Empty and the genre templates stay). Unused dependencies removed: `axcenity-nav` from `script`, `log` from `server`, `egui-phosphor` from `editor`. The `#[allow(dead_code)]` items left are all gated (tests, a feature or debug builds) or keep a handle alive, so none were dead (`hot_patch.rs`'s go with Phase 89). The `organize` skill's full dead-code pass wasn't run; the compiler's own dead-code lint is clean under `-D warnings`. The Open checks and Phases 1 to 67 moved to `plans/axcenity-engine-unified-done.md`.

---

## Phase 93: A multithreaded frame

**From**: the review of 2026-10-30. Background jobs already use threads (texture reads in `assets`, terrain jobs, CSG, weight solving), but the frame itself runs on one thread. Streaming (Phase 80) and mobile (Phase 85) need the headroom. This phase is the only addition in this milestone.

### What to build

**Thread pool.** One pinned thread pool, chosen and licence-checked at the start of the phase (for example `rayon`, which is MIT or Apache-2.0). The number of threads follows the CPU, and is limited on mobile.

**Parallel work.** Work that doesn't touch Lua runs across threads:
- physics, through `rapier3d`'s parallel feature;
- building the draw list and culling;
- animation poses for many Animators;
- navigation agents' path and avoidance steps;
- mesh decoding when assets load, joining the existing background texture reader (`assets`) and terrain jobs on the same pool.

**Lua stays single-threaded.** The one scripting runtime and the frame order don't change. Parallel steps finish before the next step that reads their results.

**Same results.** `axcenity-test` and the networking tests run with a fixed thread count, so their results don't depend on the machine. Physics determinism settings are chosen so that replays (Phase 81) still look the same.

**Measured.** The Profiler shows time per stage and per thread. A benchmark of the graphics reference scene with 200 animated characters and 2,000 physics bodies runs before and after.

### Acceptance criteria

- [ ] The benchmark's frame time drops by at least 30% on a 6-core desktop, with the same picture.
- [ ] Every test passes with 1 thread and with 8.
- [ ] Every example plays the same as before.
