# Axcenity vs Unity, Unreal, Roblox Studio, Godot and s&box

Snapshot of 11 Nov 2026 (late evening). The living copy is the [Claude Doc](https://claude.ai/artifact/Avjn49acgsh72rxsZARD4D); update this file when the doc changes. The measured cross-engine numbers and how to rerun them are in [benchmarks/cross-engine/results.md](../../benchmarks/cross-engine/results.md); the follow-up roadmap IDs (G1–G16) are in [engine-gap-analysis.md](engine-gap-analysis.md).

## Summary

Axcenity is now a broad, Roblox-Studio-style desktop engine whose feature list overlaps Godot 4 and Roblox far more than its age suggests, but it trails every peer on rendering scale, ecosystem and proof at scale. It has 447k lines of Rust in 28 crates, 67 runnable examples and 3,106 passing tests, in a GitHub repository created on 18 October 2026 (442 commits on main).

- **Against Roblox Studio** (its model): same building, Instance tree and Luau API style, plus things Roblox lacks: standalone exports you own, no revenue share, text project files in git, blocks ↔ Lua, a CSS UI system, Linux. Behind on everything a platform gives: audience, avatars, cloud data stores, marketplace, Team Create, and years of scale testing.
- **Against Godot 4.6** (its closest technical peer): similar breadth (Forward+-class renderer, built-in multiplayer, import pipeline, open source). Godot is ahead on 2D, platforms (mobile, web, consoles via partners), native extension (GDExtension), docs and community.
- **Against s&box**: both are creation platforms with royalty-free standalone export. s&box has Source 2 rendering, C# with hotload and a Steam audience; Axcenity runs on Linux and macOS too and is fully open (s&box keeps Valve's Source 2 core closed).
- **Against Unity 6.3 and Unreal 5.7**: not a like-for-like contest. Both lead on rendering (Nanite, MegaLights, GPU Resident Drawer), platforms, tooling depth and ecosystem by a wide margin. Axcenity's edge is simplicity, ownership and a Roblox-familiar workflow.

Measured on the same 10,000-part scene on Paul's PC (XB10k, see Performance), Axcenity's frame is 5.1 ms (6.7 ms before the 11 Nov physics work) against Unity's 3.1 ms and Godot's 10.3 ms; Unreal and Roblox were left out of this round. The top gaps, in order: per-frame CPU cost against Unity (render planning and the draw list now, physics next), no GPU-driven rendering path for big worlds, a thin ecosystem (docs, assets, community), and no native extension point. Recommended next items are at the end.

## How to read this

- **Axcenity facts** come from the repository at main `dc2d802` (PR #75, 10 Nov 2026): source, `docs/`, `docs/benchmarks.md` and the engine modernization report. Performance figures are **measured** on Paul's desktop (Ryzen 9 9950X3D, RX 9070 XT) or the cloud container, and say which.
- **Peer facts** are **claimed**: vendor pages and press, fetched in November 2026 and listed under Sources. No peer engine was run for this doc, so no peer performance number here is measured.
- **Versions compared**: Unity 6.3 LTS (Dec 2025), Unreal Engine 5.7 (Nov 2025), Godot 4.6.3 (May 2026), Roblox Studio as of the Aug 2026 release notes, s&box as launched on 28 Apr 2026. Newer releases (Godot 4.7/4.8, Unity 6.4+) may have shipped; they were not checked.
- **In flight on Axcenity** and likely to change this picture: vegetation rendering and LOD, universal search, editor design system Phase 5, L7 gameplay systems, the engineering wiki and packaging checks. This doc gets a refresh once those land.
- The older architecture scorecard (report section 3, Unreal/Godot/Unity/Roblox) predates most of the roadmap; this doc supersedes it at feature level.

## At a glance

|  | Axcenity | Roblox Studio | s&box | Godot 4.6 | Unity 6.3 LTS | Unreal 5.7 |
| --- | --- | --- | --- | --- | --- | --- |
| Core language | Rust | C++ (closed) | C# over Source 2 (C++, closed) | C++ | C++ core, C# API | C++ |
| Game scripting | Luau (Roblox-style API), blocks → Lua | Luau | C# (Roslyn, hotload), ActionGraph | GDScript, C#, GDExtension | C# | C++, Blueprints |
| World model | Instance tree (generational arena) | Instance tree | GameObjects + components | Node tree | GameObjects + components; ECS | Actors + components; Mass |
| Renderer | wgpu (DX12, Vulkan, Metal), tiled forward PBR | Proprietary | Source 2, Vulkan 1.2 | Forward+, Mobile, Compatibility | URP, HDRP (shared render graph) | Deferred/forward, Nanite, Lumen, MegaLights (beta) |
| Physics | rapier (parallel) | Proprietary | Box3D | Jolt (default for new projects) | PhysX; Havok separate | Chaos |
| Multiplayer | Built in: listen + dedicated server, LAN, renet/UDP | Built in, hosted | Built in | High-level multiplayer API | Netcode for GameObjects (package) | Built-in replication |
| Editor platforms | Windows, Linux, macOS (secondary) | Windows, macOS | Windows | Windows, Linux, macOS | Windows, macOS, Ubuntu | Windows, Linux, macOS |
| Game targets | Windows, Linux, macOS | Roblox apps (PC, mobile, console, VR) | s&box platform; standalone Steam (Windows) | Desktop, mobile, web; consoles via partners | Desktop, mobile, web, consoles, XR | Desktop, mobile, consoles, XR |
| Source | Open, Apache-2.0 | Closed (Luau is open) | s&box code MIT; Source 2 closed | Open, MIT | Closed | Source-available (EULA) |
| Cost to ship | Free, no royalty | Free; earn through Robux and DevEx | Royalty-free standalone on Steam | Free | Free under $200K/yr; Pro $2,310/seat/yr | 5% royalty above $1M lifetime gross |

Platform rows for peers are their vendors' claims; Axcenity's macOS builds ship but aren't verified per change ([development policy](https://github.com/MrBearson321/Axcenity-Engine/blob/main/docs/development-policy.md)).

## Architecture and core runtime

Since the 10 Nov roadmap merges, Axcenity's core matches Roblox's model and borrows Godot's and Unity's best structural ideas; threading is the remaining structural gap against Unity and Unreal.

| Area | Axcenity now | Peers | Verdict |
| --- | --- | --- | --- |
| Object model | One Instance tree, `ClassName` enum, generational IDs; deliberately no ECS | Roblox: same. Godot: node tree. Unity/Unreal: components plus an optional ECS (Entities, Mass) | At parity with Roblox and Godot. ECS rejected for heterogeneous Roblox-style content (report section 3.1) |
| Finding things | Class and tag index (M2, #17) | Roblox `CollectionService`, Godot groups | At parity |
| Frame order | One data-driven schedule shared by player, editor Play, server and test runner, with a golden-order test (M1) | Unity PlayerLoop, Unreal tick groups, Roblox RunService stages | At parity; the golden test is stricter than most |
| Sim → render | Retained draw list keyed by revisions (M4); `render` sees only draw data | Godot servers, Unreal scene proxies, Unity GPU Resident Drawer | Same pattern, CPU-side only (no GPU-driven path yet) |
| Threading | Main-thread frame; rapier `parallel`, rayon in render, navmesh and asset loading on workers | Unreal game/render/RHI threads and Tasks; Unity jobs + Burst; Roblox Parallel Luau | Behind. L4 found no clear 4-core target; untested on 8+ cores |
| Property storage | Per-Instance map with interned names; L1 slots measured and deferred (name lookups ~9% of S10k instructions) | Typed reflected fields everywhere | Behind, but measured as not the bottleneck today |
| Gameplay systems | Moving out of `script` into a `gameplay` crate behind `GameSystem` (L7, parts 1–3) | Unreal Gameplay Framework, Unity packages | In progress |
| Crash isolation | Engine panics in Play return to editing; script watchdog stops 3 s loops | Roblox script timeouts; Godot/Unity editors can crash on game code | Ahead of Unity and Godot for a hobbyist audience |

## Rendering

Axcenity's renderer is the broadest part of the engine: feature-for-feature it is close to Godot's Forward+ and ahead of Roblox, but it has none of the scale technology (virtualized geometry, GPU-driven drawing, upscaling, ray tracing) that Unreal and Unity lead with.

| Feature | Axcenity | Where peers stand |
| --- | --- | --- |
| Shading | PBR with presets; material files, surface shaders and material graphs; Toon, ramp, rim and outline models | Unreal Substrate is production-ready in 5.7; Godot, Unity have node shader graphs; Roblox fixed materials |
| Lights | Tiled forward, up to 1,024 lights (64 per 32 px tile); point, spot and rect lights with shadows | Unreal MegaLights (beta) targets many shadowed lights; Godot clustered Forward+ |
| Shadows | Sun cascades (up to 4 at 4096²), spot and point shadows, contact shadows | At parity with Godot and Unity URP; Unreal virtual shadow maps ahead |
| Global illumination | Baked light probe volumes and reflection probes; optional real-time probe GI (2 probes/frame, off by default) | Unreal Lumen (dynamic GI), Godot SDFGI/VoxelGI, Unity APV and baked lightmaps |
| Sky and atmosphere | Physically based atmosphere (Hillaire 2020), time of day, stars, clouds, weather, volumetric fog and sun shafts | Comparable to Godot; Unreal sky atmosphere and volumetric clouds ahead |
| Post and temporal | Bloom, tone mapping, LUT grading, DoF, motion blur, TAA, graphics volumes | Peers add upscalers (FSR, DLSS, TSR, STP). Axcenity has none |
| Geometry scale | Offline LODs with dithered cross-fade; frustum and distance culling; CPU-planned instancing | Unreal Nanite and 5.7's Nanite Foliage; Unity GPU Resident Drawer; Godot auto LOD and occlusion culling |
| Ray tracing | None | Unreal and Unity HDRP hardware RT |
| Water, decals, particles, terrain | All present (water material, projected decals, GPU-skinned meshes, sprite particles, sculpted and painted terrain with foliage) | Parity in kind; peers have deeper tools (Niagara, VFX Graph) |

Vegetation rendering and LOD is being reworked now on Paul's PC, so the geometry row will change. The quality check is a pixel reference per example and per graphics preset, which is stricter than most hobby engines but has no cross-engine visual benchmark.

## Asset pipeline and packaging

The L2 work closed the biggest gap the November audit found: Axcenity now has a Godot-style import pipeline. What it still lacks is breadth of formats and a place to get assets.

- **Import and cook (measured, merged):** glTF, GLB and FBX import with an Import dialog; textures cooked to BC7 with offline mips; models cooked whole, so built games never parse FBX or glTF; offline LODs; images inside models cooked once per project (#65). On Abandoned Farmhouse this took GPU texture memory from 947 to 311 MB and the first frame from 3.0 s to 0.23 s after #65, #67 and #69.
- **Identity:** stable asset IDs in sidecars, like Godot's `uid://` and Unity's `.meta` GUIDs, but references in worlds still name paths.
- **Async loading:** `ContentProvider:PreloadAsync` and load tickets (M5); models draw nothing until loaded.
- **Packaging:** one `.axgame` (zstd per entry since M9) plus a player executable; installers for Windows (.exe), macOS (.dmg) and Linux (.deb, .rpm, AppImage and more). Cross-OS builds need that OS's player beside the editor.
- **Sharing:** `.axpkg` asset packages and prefabs between projects.

| Gap | Who has it |
| --- | --- |
| Asset store or marketplace | Unity Asset Store, Fab (Unreal), Roblox Creator Store, Godot Asset Library, s&box Workshop |
| Formats beyond glTF/FBX (USD, Alembic, .blend) | Unreal and Unity (USD); Godot imports .blend |
| Mobile and console texture formats (ASTC, ETC2) | All peers; deliberately deferred by policy |
| Patch or delta updates | Godot 4.6 delta-encoded patch PCKs; Unreal and Unity patching |
| Cooked games still ship source textures beside cooked ones (about 2× size) | Peers ship cooked only; M9's next step |

## Scripting and gameplay systems

Scripting is Axcenity's strongest differentiator for beginners: one Luau runtime with a Roblox-familiar API, reachable from Lua, Scratch-style blocks and node graphs. Its weak spot is the built-in code editor, which Roblox Studio has invested heavily in.

**What Axcenity has (from the repo):**

- Luau in a sandbox with `safeenv` and read-only libraries, a 3 s watchdog, and opt-in `--!native` code generation (L3, #58). Measured on the numeric bench: 3.1× faster interpreted and 6.2× with `--!native`.
- Roblox-style API: Script/LocalScript/ModuleScript, RemoteEvents and RemoteFunctions, `task.*`, `RunService`, `TweenService`, `CollectionService`, `ContentProvider:PreloadAsync`, saves, data tables. API-compatible in spirit, not exact (policy).
- Visual authoring on top of the same runtime: blocks that round-trip to Lua, script graphs, UI event graphs, behavior trees, dialogue and quests, a cutscene sequencer, animation state machines and blend trees.
- Live script reload during Play, breakpoints, Find in Scripts, and luau-lsp definitions so VS Code autocompletes and type-checks the API.
- Ready-made systems: third- and first-person controllers, Humanoids and health, tools, abilities, seats and vehicles, navigation agents with perception, destruction, splines.

| Area | Axcenity | Peers |
| --- | --- | --- |
| In-editor code editing | egui text box, hand-written highlighter; no autocomplete or type errors in the editor itself | Roblox Studio: autocomplete and the new Luau type solver (general release 20 Nov 2025). Unity/Unreal/Godot: IDE integration or a full built-in editor (Godot) |
| Script speed | Interpreted Luau plus opt-in native code; Vector3 is userdata; native `vector` on hold by policy | Roblox: native code and native `vector`. Unity: C# + Burst. Unreal: C++. Godot: typed GDScript, C# |
| Script parallelism | None | Roblox Parallel Luau actors; Unity jobs; Unreal Tasks |
| Native extension | None (Rust changes mean an engine fork) | Godot GDExtension, Unreal modules, Unity packages, s&box C# libraries |
| Visual scripting | Blocks, graphs, behavior trees, UI graphs | Unreal Blueprints (far deeper), s&box ActionGraph, Unity Visual Scripting; none first-party in Roblox or Godot 4 |
| Iteration | Live Lua reload; engine hot-patch in debug builds | s&box C# hotload, Unreal Live Coding, Godot script reload, Unity domain reload |

## Editor and ease of use

The editor already covers more tools than Roblox Studio or Godot ship in the box, laid out the way Roblox users expect; what it lacks is collaboration, an AI assistant and the polish that comes from many users.

**What it has:** a Studio-style layout with dockable and floating panels (real OS windows), custom ribbon tabs, a command palette, a Lua console, undo for every edit, version history and checkpoints, and Play/Stop with several clients over an emulated network. Built-in studios cover models (Model Studio, CSG unions), terrain (sculpt, erosion, painted foliage), UI (UI Designer with CSS), animation (clips, state machines, blend trees), materials (graphs), AI behavior trees, dialogue and cutscenes, plus a Profiler, audio Mixer and Asset Check. Editor plugins are written in Lua.

**Usability work done:** the U1–U20 usability plan is merged, and the design-system phases 1–4 are merged (Phase 5 in progress). Measured: Explorer with 10k rows went from 75 to 0.15 ms a frame, Console with 10k lines from 40 to 0.09 ms (Phase 3, cloud container).

| Area | Axcenity | Peers |
| --- | --- | --- |
| Live collaboration | None; git-friendly text files and local version history | Roblox Team Create and comments; Unreal multi-user editing and One File Per Actor; Unity Version Control |
| AI assistance | None in the editor | Unreal 5.7 AI Assistant (experimental), Unity AI (beta in 6.3), Roblox Assistant |
| Learning material | 67 runnable examples, a Learn page, a searchable API reference; wiki in progress | Large official docs, courses and community answers for every peer |
| Editor iteration | Editor is one large crate (about 162k lines in Nov); incremental rebuild 5–6 s (L6 would split it) | Not comparable: peers ship binary editors |
| UI toolkit | egui immediate mode; tied to the wgpu version (M12) | Native retained toolkits (Slate, UI Toolkit, Godot Control) |

Against Roblox in particular, Axcenity's Studio-familiar workflow is the selling point, and its plain project folders and Linux editor are things Roblox users can't get.

## Physics, navigation, animation, audio and UI

Each of these exists and is usable for Roblox-scale games; none is as deep as Unreal's or Unity's equivalent, and two have small, specific gaps worth fixing.

| System | Axcenity | Peers | Note |
| --- | --- | --- | --- |
| Physics | rapier at fixed 60 Hz, `parallel` on; joints and springs, collision groups, wind, fracture, vehicles, seats; kinematic characters | Jolt (Godot default), PhysX (Unity), Chaos (Unreal), Box3D (s&box), Roblox's own solver | **No render interpolation:** above 60 fps moving parts step at 60 Hz. Every peer interpolates or extrapolates |
| Navigation and AI | Navmesh built on a worker (M13), agents with sight and hearing, behavior trees with an editor | Unreal Recast, StateTree and Behavior Trees; Unity AI Navigation; Godot NavigationServer; Roblox PathfindingService | At parity with Godot and Roblox |
| Animation | Skeletal FBX, GPU skinning, state machines, blend trees, root motion, events, IK posing in Model Studio | Unreal Control Rig, motion matching; Unity Mecanim; Godot 4.6 IK nodes | Behind on runtime IK and retargeting tools |
| Audio | Sounds in parts, effects, a Mixer panel, spatial example | Wwise/FMOD integrations, MetaSounds (Unreal) | Adequate; no middleware integration |
| Game UI | Instances styled with CSS (flex layout, transitions, `:hover`/`:focus`), rich text, controller navigation, a UI Designer and UI event graphs | Unity UI Toolkit (USS), Unreal UMG, Godot Control, Roblox UI Styling (beta) | Ahead of Roblox and Godot for web-style authoring; M14 measured a 500-element inventory at 3.6 ms/frame |
| Saves and data | Save slots on the player's machine, data tables | Roblox DataStores (cloud), Unity Cloud Save, platform saves | No cloud storage, by design (no hosting yet) |

## Networking and multiplayer

Multiplayer is built in from the first frame, as in Roblox and s&box, and ahead of Godot and Unity's defaults for a beginner; it is not ready for the public internet, by policy.

- **Model:** every game is server and clients, Roblox style. Single-player and editor Play are a listen server; `--host`/`--join` and a dedicated `axcenity-server` binary cover the rest. LAN discovery finds games.
- **Wire:** renet over UDP with reliable and unreliable channels; postcard encoding (M7, protocol 10). Parts move by quantized motion within a per-player budget and are smoothed on clients; each player moves their own character with no delay and the server checks it. World streaming by radius (Roblox's `StreamingEnabled` semantics).
- **Tooling:** editor Play with several clients and an emulated network (latency, jitter, loss), a Network section in the Profiler, chat, `Player:Kick`.
- **Measured (cloud container, M7):** S10k with 8 loopback players: 1.02 MB snapshot, all joined in 19 frames, 5,464 bytes a frame to all 8 afterwards, server frame 14.9 ms median.

| Gap | Peers |
| --- | --- |
| Connections are unsecured (no encryption or connect tokens); M11 deferred until internet hosting | Roblox and s&box host for you; Unreal and Unity transports support encryption |
| No hosting, matchmaking or relay | Roblox servers; s&box platform; Unity Multiplayer Services; Epic Online Services |
| No general rollback or prediction for arbitrary physics | Roblox Server Authority (Studio beta since Dec 2025, rollback and resimulation); Unreal Network Prediction plugin |
| Not tested beyond 8 loopback players | Roblox servers run large lobbies; its own post says 20+ players with Server Authority still needs work |

## Performance

Axcenity has now been timed against four peers on the same scene and machine. On XB10k its frame now takes 5.1 ms, against Unity's 3.1 ms and Godot's 10.3 ms. It was 6.7 ms before the 11 Nov physics and thread-pool work. Unreal and Roblox were not timed in this round: Unreal's editor build needed a .NET Framework SDK that wasn't installed, and Roblox Studio's Play stayed at its 60 fps cap. Both projects are ready on PR #80 for a later run.

**Measured, XB10k** (11 Nov 2026, Paul's desktop: Ryzen 9 9950X3D pinned to CPUs 0–15, RX 9070 XT, 1920 × 1080, vsync off, no frame cap; median of three runs' median frame). XB10k is S10k's core rebuilt from the same formulas in each engine: 10,000 parts (8,899 static, 100 spinning, 1,000 falling), 198 small scripts, a sun with shadows reaching 700 units. Each engine prints a checksum of what it built, and all matched. Spec, projects and logs: `benchmarks/cross-engine/` on PR #80.

| Engine | Median frame (ms) | FPS | p95 (ms) | Fixed 1/60 s per frame |
| --- | --- | --- | --- | --- |
| Unity 6000.6.3f1 (URP, D3D12, Mono) | 3.11 | 322 | 3.43 | yes |
| Axcenity (wgpu, Vulkan) | 5.14 (was 6.70) | 194 | 5.73 | yes |
| Godot 4.7.2 (Forward+, Vulkan, Jolt) | 10.27 | 97 | 11.33 | yes |
| s&box 26.11.10 (Source 2) | 2.05 | 488 | 4.84 | no |

- Axcenity's 5.14 ms (re-timed on main b3297ca, PR #94) splits into render 1.76, physics 1.46 (was 2.43), draw list 1.34 and Heartbeat scripts 0.30 ms (was 0.74). The GPU frame is about 2 ms, so it's CPU-bound; render planning and the draw list are now the largest costs.
- s&box runs in real time. At about 490 fps its 60 Hz physics steps roughly once every eighth frame, while the other three step once per frame, so its number isn't the same work and isn't ranked.
- Unity ran on Mono, because the IL2CPP module isn't installed; IL2CPP would usually make its scripts faster. Physics engines, sleeping rules and each renderer's default work differ; the spec lists every known difference.

**Measured, Axcenity against itself**

Paul's desktop (Ryzen 9 9950X3D pinned to the V-cache CCD, RX 9070 XT, Vulkan; median frame in ms, before vs after the 10 Nov roadmap merges, `docs/benchmarks.md`):

| Host and scene | Before | After |
| --- | --- | --- |
| Player, S10k (10k parts, 200 scripts, 20 animated characters, 10 agents) | 14.93 | 8.73 |
| Server, S10k | 5.13 | 3.82 |
| Test runner, S10k | 7.71 | 5.03 |
| Player, Temperate Wet Patch (1.9M-vertex trees) | 47.80 | 5.52 |
| Player, General Store Roadside | 6.99 | 4.40 |
| Player, Abandoned Farmhouse | 4.77 | 5.50 (first frame later cut to 212 ms) |

**What this does and doesn't show:**

- S10k plays at about 115 fps in the player on a high-end desktop, with physics as the largest CPU cost (58% of S10k's instructions in the L1 measurement). That is respectable for 10k dynamic-capable parts, but there is no matching scene in any peer to compare it with.
- Rendering cost on a GPU is small on these scenes (S10k Render 2.7 ms in the Nov 9 run); the engine has not been tested on large open worlds, where peers' GPU-driven and virtualized geometry matter most.
- Peer claims (Nanite Foliage's "millions of leaves", MegaLights) are vendor marketing for different hardware and content, not comparable numbers.

XB10k turns recommendation C1 into numbers. What they point at: the physics pass (C10) has already cut Axcenity's frame by a quarter; what's left is CPU-side rendering (planning 1.8 ms, draw list 1.3 ms), which is the case for C6 (M12 then L5) more than for GPU features. Unreal (default and a simpler Lumen-off profile) and Roblox Studio can be added later with the same projects; Roblox already builds the scene correctly but needs Studio's frame cap raised to 240 first.

## Platforms, licensing and ecosystem

Axcenity's licence is the most generous of the six, and its ecosystem is the smallest by far; ecosystem, not technology, is the gap a user will feel first.

|  | Axcenity | Roblox | s&box | Godot | Unity | Unreal |
| --- | --- | --- | --- | --- | --- | --- |
| Licence | Apache-2.0, games fully yours | Closed; games live on Roblox | s&box code MIT, Source 2 closed; standalone Steam export royalty-free (Valve licence signed Mar 2026) | MIT | Proprietary | Source-available EULA |
| Cost | Free | Free; Robux revenue via DevEx | Free | Free | Personal free under $200K revenue and funding a year; Pro $2,310/seat/yr | Free under $1M; 5% royalty above $1M lifetime gross (Epic Games Store sales exempt) |
| Shipping targets | Windows, Linux, macOS | Roblox's apps only | s&box platform, Steam (Windows) | Desktop, mobile, web; consoles via third parties | Desktop, mobile, web, consoles, XR | Desktop, mobile, consoles, XR |
| Audience built in | None | Yes, the Roblox platform | Yes, s&box on Steam | No | No | No (Fortnite UEFN is separate) |
| Asset marketplace | `.axpkg` sharing only | Creator Store | Workshop | Asset Library | Asset Store | Fab |
| Community and docs | One maintainer; repo docs and examples; wiki in progress | Very large | Growing since Apr 2026 launch | Large, open | Very large | Very large |

By policy, mobile, web, consoles and internet hosting are deferred (development policy, 10 Nov 2026), so the platform row is a choice, not an oversight.

## Gaps and recommended next roadmap items

IDs are new (C for comparison) and map to existing roadmap items where one exists. The 2026-11 gap analysis (`docs/research/engine-gap-analysis.md`) has since folded most of them into its G list, so each row's owner and status is below the table.

**Status on 11 Nov 2026 (late evening):**

- **C1 done**: XB10k in Axcenity, Godot, Unity and s&box, timed on Paul's desktop (PR #80, merged; see Performance). Unreal and Roblox were left out at Paul's call; their projects are ready.
- **C2 in progress** as G3 (physics render interpolation, on by default with a Workspace switch).
- **C3 waits for M12**, then G4 (luau-lsp as a child process).
- **C4 next** as G12 (cooked-only games, read the pack in place), after M9 part 1 (PR #61).
- **C5 waits for M12**, then G9.
- **C6**: M12 (wgpu 30 / egui 0.36) is in progress; L5 (GPU-driven) stays deferred until a big-world scene is CPU-bound in drawing.
- **C7** is G10, added piece by piece when a real plugin or Toolbox provider needs it.
- **C8** belongs to the wiki thread; six genre templates already ship in `templates/`.
- **C9 next** as G13, once Paul's machines are free.
- **C10 done**: physics step work (PR #84), physics' own thread pool of at most 12 (PR #86, deadlock fix #93) and render planning's own pool (PR #88). Rapier solver tuning continues in the physics thread.

| # | Item | Why (gap it closes) | Effort | Existing ID |
| --- | --- | --- | --- | --- |
| C1 | Build S10k in Godot 4.6 (then Unity 6.3) and run it on Paul's desktop with the same frame steps | Turns "claimed" into "measured"; Godot is open source and its node tree maps closely | M (1–2 wk) | new; extends I1 |
| C2 | Interpolate rendered transforms between 60 Hz physics steps | Every peer does it; visible stutter on 120–240 Hz monitors | S (days) | new |
| C3 | Luau type checking and autocomplete inside the editor's code tab (embed Luau's analysis or luau-lsp) | Roblox Studio's biggest scripting advantage; beginners don't use VS Code | M (2–3 wk) | new |
| C4 | Ship cooked-only games and read the pack in place | Games are about 2× bigger than needed; peers ship cooked only | S–M | M9 next part |
| C5 | Upscaling on top of the existing render scale and TAA | Every peer offers FSR/DLSS/TSR-class upscaling; big win on low-end GPUs | M | new |
| C6 | wgpu/egui upgrade, then the GPU-driven path for large worlds | The scale gap against Nanite and the GPU Resident Drawer; builds on the vegetation work now in flight | L (months) | M12 then L5 |
| C7 | Extend the Lua editor plugin API to importers, property editors and gizmos | Closest practical answer to GDExtension/Unreal modules without a native ABI | M | report 3.4 |
| C8 | Publish the wiki as a docs site with 3–5 complete starter games | Ecosystem is the gap users hit first | M, ongoing | wiki thread |
| C9 | Multiplayer test with 16–32 real clients on a LAN | Only 8 loopback players measured so far | S–M | extends M7 |
| C10 | Measure L4 parallel stages on the 16-core desktop | Threading is the largest architecture gap against Unity and Unreal | M | L4 |

**Keep deferred** (policy, still right after this comparison): mobile and web targets, consoles, internet hosting and secure netcode (M11), CI (M10), hardware ray tracing, an ECS. Revisit at the next milestone review.

## Sources

Axcenity: [repository](https://github.com/MrBearson321/Axcenity-Engine) at `dc2d802`: `README.md`, `docs/benchmarks.md`, `docs/research/engine-modernization-report.md`, `docs/development-policy.md`, `docs/rendering/`, `docs/architecture/`, `docs/editor/`.

Peers (fetched 11 Nov 2026):

- Unity: [Unity 6 release page](https://unity.com/unity-6-release), [pricing updates](https://unity.com/en/products/pricing-updates), [CG Channel on Unity 6.3 LTS](https://www.cgchannel.com/2025/12/unity-6-3-lts-is-out-see-5-key-features-for-cg-artists/)
- Unreal: [Unreal Engine licence](https://www.unrealengine.com/en-US/license), [GameFromScratch on UE 5.7](https://gamefromscratch.com/unreal-engine-5-7-released/)
- Godot: [licence](https://godotengine.org/license/), [Digital Production on 4.6](https://digitalproduction.com/2026/01/28/godot-4-6-arrives-with-major-cg-friendly-updates/), [Cinevva on 4.6](https://app.cinevva.com/news/2026-02-27-godot-4-6-released), [gamedev.net on 4.6.3](https://gamedev.net/news/3473-maintenance-release-godot-463/)
- Roblox: [new Luau type solver general release](https://devforum.roblox.com/t/general-release-luau%E2%80%99s-new-type-solver/4084991), [Server Authority Studio beta](https://devforum.roblox.com/t/server-authority-studio-beta-insights-from-engineers/4139321), [release notes 10 Aug 2026](https://create.roblox.com/docs/updates/2026-08-10.md), [Creator Roadmap summer 2025](https://devforum.roblox.com/t/creator-roadmap-2025-summer-update/3880746)
- s&box: [Valve Developer Community wiki](https://developer.valvesoftware.com/wiki/S%26box), [GameGrin on the Steam licence](https://www.gamegrin.com/news/facepunch-studios-announces-new-license-with-steam-to-join-the-engine-market/), [igor'sLAB on the Steam launch](https://www.igorslab.de/en/sandbox-launches-on-steam-facepunch-is-turning-garrys-mod-into-an-open-gaming-platform-rather-than-a-sequel/)

Not verified from a fetched page: Roblox's current DevEx rate (left out), s&box revenue share on its own platform, and peer platform lists (from general knowledge of each vendor's supported targets).
