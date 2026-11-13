# Axcenity Engine — Competitive Development Roadmap

**Document type:** Engineering roadmap and competitive feature-gap plan
**Repository:** `MrBearson321/Axcenity-Engine`
**Location:** `docs/research/competitive-development-roadmap.md`
**Reference engines:** Unreal Engine 5, Godot 4, s&box, Unity 6
**Project philosophy:** Free, open-source, accessible, capable and developer-owned.

Paul's roadmap of 12 Nov 2026, committed as written apart from what the **Status** notes, section 2's status column, section 11's matrix and section 12's status column add. Statuses were checked against main at `5143a5a` (PR #236, 12 Nov 2026): the source, its tests and the topic docs, not module names. The earlier plan this reconciles with is [engine-gap-analysis.md](engine-gap-analysis.md) (G1–G16, PR #89); its IDs are kept, and nothing here duplicates a G item. Peer columns are the vendors' claims as gathered for [engine-comparison.md](engine-comparison.md) (Unreal 5.7, Godot 4.6, s&box as launched in April 2026, Unity 6.3 LTS); no peer engine was run for this document.

---

# 1. Purpose

This document establishes a coordinated development roadmap for Axcenity Engine based on a competitive analysis of Unreal Engine, Godot, s&box and Unity.

The objective is not to blindly replicate every feature offered by competing engines. Instead, Axcenity should identify important functional gaps, improve existing systems, develop missing production tools and establish a competitive ecosystem.

Axcenity should aim to combine:

- The accessibility and approachable workflows of Roblox Studio.
- The technical capabilities and rendering quality of Unreal Engine.
- The flexibility and open-source philosophy of Godot.
- The integrated development and publishing experience of s&box.
- The extensibility and production workflows of Unity.

The ultimate goal is to make Axcenity a viable professional game development engine while retaining its approachable design philosophy.

## 1.1 Core principles

All development must follow these principles:

1. **Open source:** The engine remains entirely free and open source under its existing Apache-2.0 licensing.
2. **Developer ownership:** Developers own their projects, assets and games.
3. **No mandatory online services:** Accounts, hosting, publishing and other online services must remain optional.
4. **Accessible development:** Advanced functionality should not require unnecessarily complicated workflows.
5. **Performance:** New features must be benchmarked and must not introduce unnecessary runtime overhead.
6. **Integrated experience:** Prefer coherent built-in tools over requiring developers to install numerous external applications.
7. **Maintainability:** Avoid unnecessary dependencies, duplicate implementations and architectural complexity.
8. **Measured development:** Feature parity is not a sufficient reason to implement something. Features should solve real development problems.

# 2. Existing development documentation

Before starting implementation, review and reconcile this roadmap with existing development work.

Important existing references include:

- `docs/research/engine-gap-analysis.md`
- `docs/research/` existing engine comparisons and research
- Existing graphics, physics, animation and networking documentation
- Toolbox and asset-source documentation
- Collaboration and version-control documentation
- Existing implementation plans and milestone tracking

Several recommendations in this document already exist in the engineering roadmap.

In particular (status checked on main `5143a5a`):

| Existing item | Work | Status |
|---|---|---|
| G1 | Toolbox foundation | **Implemented** (#90, #92, #96, #107, #139, #142, #147, #152, #179). Dragging a result card into the scene: #238, and onto the Assets panel. Gaps: sound preview playback, Freesound |
| G2 | Scripts inside objects | **Implemented** (#91, #111, #114, #115, #117, #120; Code/Blocks switch #230) |
| G3 | Physics interpolation | **Implemented** (#110) |
| G4 | Script completion and diagnostics | **Implemented** (#145 via #153; go to definition #239) |
| G5 | UI prefabs | **Implemented** (#122) |
| G6 | Debugger stepping and watches | **Implemented** (#131). Limit: a coroutine the script made itself can't pause |
| G7 | Memory profiling | **Implemented** (#125, #126; history, peaks and budget warnings #241) |
| G8 | Audio occlusion | **Implemented** (#128) |
| G9 | Upscaling | **Partial**: stage 1 spatial FSR-class upscale (#155, timings #171); stage 2 (temporal, inside TAA) not started |
| G10 | Plugin API | **Implemented**: viewport drawing #265, drag handles #267, Properties rows #270, importers, Assets panel items and `plugin.ApiVersion` #274, on Phase 54's toolbars, dock widgets, selection, settings, files and mouse |
| G11 | Editor modularization | **Implemented** for every planned move (#162, #164, #166, #189, #193, #194); further moves as they pay off |
| G12 | Cooked-only games | **Implemented** (#243): Build Game leaves texture sources out; reading the pack in place not planned |
| G13 | Multiplayer scaling tests | **Planned**: needs Paul's machines |
| G14 | Run without a player | **Implemented** (#245): Run, F8 |
| G15 | Physics debug views | **Implemented** (#253): Contacts and Bodies overlays (centers of mass, sleeping) beside collider wireframes, rays and Size boxes |
| G16 | Humanoid retargeting | **Implemented** (stage 1, #244: roles by bone name and rest-pose correction; hand-edited maps and profiles later) |

**Important:** These existing tasks must not be recreated as duplicate implementations or competing PRs. Extend their existing plans and update their implementation status as necessary.

## 2.1 Reconciliation notes

- **One ID per piece of work.** Roadmap items that are G items keep their G ID; the new items (sequence 12, 13, 15–20) are named here as **R12, R13, R15, R16, R17, R18, R19, R20** by their place in section 12.
- **Model Studio (6.6) and Phase 90.** On 12 Nov 2026 the unified plan's Phase 90 took sculpting, retopology and baking out of Model Studio (about 5,600 lines). Asked the same day, **Paul chose to bring sculpting and baking back** (12 Nov 2026, 20:42Z), as section 6.6 asks: R15 restores them from Phase 90's removal and builds on them. Section 9's "no full Blender replacement" still holds: Blender stays the target for advanced work.
- **Collaboration (7.2) is largely built.** Collaborate 2.0 (#208–#219) gives persistent projects without a fixed host, Git-backed sync and checkpoints, host handoff, per-file conflicts and the guide; C1–C12 add Tailscale, live scripts, chat, permissions and the account seam (C12, a document). What's left is Paul's hand checks, so 7.2 needs no new work.
- **Accounts (7.1)** have a design already: [collaborate-accounts.md](collaborate-accounts.md) (C12). The online-services architecture (R19) builds on it.
- **Phases 4 and 5 are documents only.** The [development policy](../development-policy.md) says no infrastructure before it's needed and desktop first; so R19 and R20 are research and design, and nothing hosted is built unless Paul says so.
- **G13, G14 and G15** aren't in section 12's sequence. G14 and G15 are small and go after G10; G13 runs when Paul's PC and Mac are free.
- **Mobile** stays deferred by policy (unified plan Phases 82–88); R20 records what a mobile, web or XR port would need.

# 3. Development priorities

The roadmap is divided into five major development phases.

| Phase | Focus | Priority |
|---|---|---|
| 1 | Core engine improvements | Critical |
| 2 | Professional production workflows | High |
| 3 | Graphics and world creation | High |
| 4 | Online ecosystem and services | Strategic |
| 5 | Platform expansion and advanced capabilities | Long-term |

Phases are logical groupings rather than strict release deadlines. Independent tasks may proceed in parallel where dependencies and available development resources permit.

---

# 4. Phase 1 — Core engine improvements

**Objective:** Close the most important functional gaps and improve the reliability and usability of existing systems.

These improvements should be prioritized before introducing additional large subsystems.

## 4.1 Scripts inside objects

**Priority:** Critical
**Existing reference:** G2
**Status:** Implemented. `SourceFile` references in `world.json`, levels and prefabs, files in `objects/`, Insert Script on a selected object, Asset Check, packages, Build Game, rename, replicas and streaming rules (`docs/project/world.md`, "Scripts inside objects"; `docs/editor/scripts.md`).

Allow Scripts, LocalScripts and ModuleScripts to exist underneath arbitrary Instances.

### Requirements

- Support scripts attached to Parts, Models and other appropriate Instances.
- Preserve existing Script, LocalScript and ModuleScript behavior.
- Store script references in world and prefab files.
- Keep actual source code in separate project files.
- Support project-relative paths and `ax://` asset references.
- Ensure cloned objects execute their associated scripts correctly.
- Preserve `script.Parent` behavior.
- Support nested models and reusable objects.
- Prevent replicated client objects from executing server-only scripts.
- Ensure streaming does not prematurely execute scripts.
- Update the Explorer context menu to support inserting scripts.
- Integrate script references into asset validation and project packaging.

### Acceptance criteria

- Existing projects continue loading without modification.
- Objects containing scripts save and reload correctly.
- Prefab instances execute their own scripts.
- Scripts survive renaming and project relocation.
- Server and client execution rules remain correct.
- Script files are included in packaged projects.

## 4.2 Physics interpolation

**Priority:** Critical
**Existing reference:** G3
**Status:** Implemented. `physics/src/blend.rs`, Workspace `PhysicsInterpolation` (on by default); tests cover blends between steps, several steps in a frame, assemblies the camera follows, scripts moving a part (no blend), and identical results with it off. A look at 120, 144 and 240 fps is a hand check on Paul's PC.

Improve visual smoothness for physics-driven objects when rendering above the physics simulation frequency.

### Requirements

- Preserve fixed-step physics simulation.
- Retain previous and current simulation poses.
- Interpolate render poses using the remaining simulation fraction.
- Avoid changing authoritative physics or script-visible positions.
- Support moving assemblies and welded objects.
- Handle teleports and explicit position changes without unwanted interpolation.
- Avoid double interpolation for network-replicated objects.
- Provide a Workspace-level configuration option.

### Acceptance criteria

- Physics-driven objects render smoothly at 120, 144 and 240 FPS.
- Game logic continues using authoritative simulation values.
- Teleports remain immediate.
- Network interpolation remains correct.
- Existing physics benchmarks do not regress.

## 4.3 Script editor improvements

**Priority:** Critical
**Existing reference:** G4
**Status:** Implemented. Completion (Instance paths, members, `GetService`, Enums, signatures), luau-lsp errors and lints in code tabs and Output, and Go to Definition (F12 or Ctrl+click: a local's line, an Instance, or an API Reference entry; #239) are built (`editor/src/script_completion.rs`, `luau_check.rs`). luau-lsp isn't shipped with the editor.

Develop a more complete built-in scripting experience.

### Requirements

Implement:

- Luau autocomplete.
- Instance hierarchy completion.
- Property and method completion.
- Enum completion.
- `game:GetService()` completion.
- Function signatures and parameter hints.
- Inline error diagnostics.
- Warnings and diagnostics in Output.
- Navigation to script definitions where supported.
- Improved syntax highlighting.
- Efficient completion for large scripts.

Use the existing Luau definitions and language-server infrastructure.

Use `luau-lsp` as a child process for diagnostics, following the existing architectural decision.

### Acceptance criteria

- Completion works without requiring VS Code.
- Diagnostics appear directly in the editor.
- Large scripts do not cause significant typing latency.
- Language-server failures do not crash the editor.
- Existing scripting functionality remains compatible.

## 4.4 Toolbox and asset pipeline

**Priority:** Critical
**Existing reference:** G1 and Toolbox integration documentation
**Status:** Partial. The provider trait, ambientCG, Poly Haven and Openverse, the central asset directory, `metadata.db` (SQLite), MD5-verified downloads, previews, search and filters, favourites, collections, history, licences, credits, the project snapshot `.axcenity/asset-sources.json`, offline library views, the CLI and the Blender drop-in are built (`crates/toolbox`, `crates/editor-toolbox`, `docs/editor/assets.md`). Dragging a result card into the scene is built (#238: it gets the asset and places it where it's dropped, a sound inside the part under the pointer). Dragging one onto the Assets panel gets it into the project and shows it there. Gaps: sound preview playback, and Freesound (later, with a user's own key, as decided in G1).

Complete the centralized Toolbox architecture.

### Requirements

Support:

- ambientCG.
- Poly Haven.
- Openverse.
- Future optional providers such as Freesound.

Implement:

- Unified provider abstraction.
- Centralized asset storage.
- Persistent SQLite metadata database.
- Download verification.
- Asset previews.
- Search and filtering.
- Favorites and collections.
- Download history.
- License and attribution tracking.
- Drag-and-drop asset insertion.
- Blender integration through the shared Rust core.
- Project-level asset attribution snapshots.
- Offline access to previously downloaded assets.

### Acceptance criteria

- All approved providers work through the same provider architecture.
- Existing ambientCG functionality remains intact.
- Blender and Axcenity use the same asset metadata.
- Asset source and license information survive project relocation.
- Downloads are verified before installation.
- The runtime does not link unnecessary provider or HTTP functionality.
- The editor remains usable offline.

## 4.5 Audio improvements

**Priority:** High
**Existing reference:** G8
**Status:** Implemented. `RollOffMode` Inverse, Linear, InverseTapered (and the tapered Linear), occlusion from physics rays (8 Sounds × up to 6 rays a frame, `sim/src/occlusion.rs`) applied as volume and a low-pass filter (`audio` `mix::muffle`).

Improve spatial audio realism.

### Requirements

- Add inverse roll-off.
- Add linear roll-off.
- Add inverse-tapered roll-off.
- Implement configurable distance attenuation.
- Add physics-based sound occlusion.
- Support wall and environmental obstruction.
- Add low-pass filtering for obstructed sounds.
- Avoid excessive raycasts through throttling and caching.
- Preserve existing Sound and SoundGroup workflows.

### Acceptance criteria

- Different roll-off modes produce predictable attenuation.
- Walls and other obstructions muffle spatial sounds.
- Performance remains within a defined audio-raycast budget.
- Existing audio projects remain compatible.

---

# 5. Phase 2 — Professional production workflows

**Objective:** Make Axcenity more suitable for larger and more complex game projects.

## 5.1 Advanced debugging

**Priority:** High
**Existing reference:** G6
**Status:** Implemented. Step Over, Into and Out, watches, pause on error, call stack with locals across modules (`script/src/debugger.rs`). Known limit: a coroutine the script made itself (`coroutine.wrap`) can't pause; a step goes on to the next line where it can (`wiki/Roadmap/Known-Limitations.md`).

Extend the existing debugger.

### Requirements

- Step Into.
- Step Over.
- Step Out.
- Watch expressions.
- Pause on error.
- Improved coroutine debugging.
- Better stack inspection.
- Module navigation.
- Improved variable inspection.
- Clear debugger state indicators.

### Acceptance criteria

- Stepping works across function calls and module boundaries.
- Coroutine and `task.wait()` behavior remains correct.
- Watch expressions update while execution is paused.
- Debugging does not destabilize the runtime.

## 5.2 Memory profiler

**Priority:** High
**Existing reference:** G7
**Status:** Implemented. The Profiler's Memory tab and F9 Stats break memory down by textures, models, lighting data, GPU buffers, audio, physics colliders, Instances by class and both Lua heaps (`core::memory::MemoryBreakdown`), and since #241 keep its history, the peak this Play, the change since Play started, and per-project budgets (Settings > This project > Memory budget) that warn in Output once per Play.

Expand memory monitoring beyond aggregate process and Lua heap measurements.

### Requirements

Add breakdowns for:

- Textures.
- Meshes.
- GPU buffers.
- Render targets.
- Audio resources.
- Physics colliders.
- Instances by class.
- Asset caches.
- Script memory.
- Other major engine allocations.

Provide:

- Memory usage over time.
- Peak memory.
- Allocation changes.
- Resource identification.
- Warnings when configured budgets are exceeded.

### Acceptance criteria

- Reported totals are consistent with known scene workloads.
- Major memory consumers can be identified.
- Memory monitoring has a configurable performance overhead.
- Profiling is available in both the editor and development builds.

## 5.3 Reusable UI prefabs

**Priority:** High
**Existing reference:** G5
**Status:** Implemented (#122): ScreenGui and element prefabs with their LocalScripts, nesting, overrides, apply and revert (`docs/editor/assets.md`, "Prefabs").

Extend the prefab system to support reusable UI components.

### Requirements

- ScreenGui prefabs.
- Nested UI prefabs.
- UI component overrides.
- UI event graphs.
- Associated LocalScripts.
- Reusable HUD elements.
- Reusable menu components.
- Prefab updates and instance overrides.

### Acceptance criteria

- UI prefabs save and load correctly.
- Instances preserve overrides.
- Nested prefabs work.
- Associated scripts remain functional.
- Existing world prefabs remain compatible.

## 5.4 Advanced asset management

**Priority:** High
**Status:** Implemented (R12: #260, #262). Built: Asset Check (missing files, broken references and dependencies, with fixes), Find References, the Unused chip (never deletes), Rename or Move with references, `ax://` IDs, packages (`.axpkg`) and global assets for moving assets between projects. Duplicate detection with Merge is in (Asset Check's Duplicate file rows, #260). Import presets are in (#262): a texture's preset, kept in its `.import.json` sidecar or set for a folder in `import-presets.json`, picks its compression (auto, color, normal map or none) and largest size, and a change cooks it again. Left for later: mesh presets (LOD settings at import), if a project needs them.

Expand asset management beyond discovery and downloading.

### Requirements

- Asset dependency tracking.
- Missing asset detection.
- Duplicate asset identification.
- Asset validation.
- Automatic import presets.
- Texture compression profiles.
- Mesh optimization profiles.
- Asset usage tracking.
- Unused asset detection.
- Dependency-aware project packaging.
- Asset migration between projects.

### Acceptance criteria

- Developers can identify missing dependencies before building.
- Asset validation reports actionable errors.
- Packaging includes required dependencies.
- Unused asset reporting does not delete anything automatically.
- Import settings remain reproducible.

## 5.5 Improved packaging and builds

**Priority:** High
**Existing reference:** G12
**Status:** Implemented (R13: #268). `.axgame` v2 with zstd, file times and threaded unpacking (M9, #61), cooked textures and models, installers for three OSes, `scripts/check-packages.py`. Cooked-only packs are built (G12, #243: Build Game leaves texture sources out). Build profiles (Release, Development and the project's own in `build-profiles.json`, `project/src/build_profiles.rs`), Asset Check before each build (a profile can stop on problems) and a build report beside the game (`build-report.json` and `.txt`) are built (`docs/project/export.md`).

Improve the game build pipeline.

### Requirements

- Cooked-only asset packaging.
- Configurable compression.
- Build profiles.
- Incremental builds.
- Dependency validation.
- Build reports.
- Platform-specific packaging settings.
- Reproducible build metadata.
- Better asset packaging diagnostics.

### Acceptance criteria

- Packaged games do not unnecessarily include source assets.
- Build size is reduced where possible.
- Builds can be reproduced using saved configuration.
- Missing dependencies are reported before packaging.
- Existing `.axgame` functionality remains compatible.

## 5.6 Plugin API improvements

**Priority:** Medium-High
**Existing reference:** G10
**Status:** Implemented. Lua plugins have toolbars, dock widgets made of UI Instances, selection, undo waypoints, settings, project files behind a permission, the viewport mouse and `.axcenity-plugin` sharing (Phase 54), and since G10: retained viewport drawing (#265), drag handles (#267), declared Properties rows and buttons (#270), and (#274) importers for file kinds the editor doesn't import, `plugin.AssetImported`, Assets panel menu items and `plugin.ApiVersion` (2). All in `docs/editor/plugins.md`. Not done: a native extension ABI (deliberately, as below) and permissions beyond `files`.

Expand the existing Lua plugin architecture.

### Requirements

- Viewport drawing APIs.
- Custom editor handles.
- Custom property rows.
- Importer hooks.
- Asset browser integration.
- Custom dockable panels.
- Additional editor extension points.
- Plugin permissions and capability boundaries.

Avoid introducing a native extension ABI until actual use cases justify the additional maintenance and security complexity.

### Acceptance criteria

- Plugins can extend useful editor workflows without modifying engine source.
- Plugin failures are isolated.
- APIs are documented and versioned.
- Existing plugins remain compatible where possible.

## 5.7 Editor modularization

**Priority:** Medium
**Existing reference:** G11
**Status:** Implemented for the planned moves: `editor-kit`, `editor-toolbox`, `editor-benchmark`, `editor-tools`, `editor-model-studio`, `editor-node-graph`, `editor-ui-designer`; the editor's rebuild after an edit went from 41 s to 30.7 s in the cloud container. `editor` is still about 168k lines; further moves happen when they pay off.

Continue splitting the large editor implementation into manageable modules.

### Requirements

- Establish clear editor subsystem boundaries.
- Extract panel families.
- Reduce unnecessary coupling.
- Define shared editor context interfaces.
- Improve compile times where measurable.
- Preserve existing editor behavior.

### Acceptance criteria

- Existing tests continue passing.
- No unnecessary runtime dependencies are introduced.
- Module boundaries are documented.
- Build and compile improvements are measured.

---

# 6. Phase 3 — Graphics and world creation

**Objective:** Improve rendering quality, scalability and built-in content creation capabilities.

This phase should emphasize measurable improvements over feature-count competition.

## 6.1 Temporal upscaling

**Priority:** High
**Existing reference:** G9
**Status:** Partial. Stage 1: EASU + RCAS (FSR 1-class, MIT) after FXAA below 100% render scale, sharpening, about 0.13 ms at 1080p and 0.2–0.25 ms at 1440p/4K on an RX 9070 XT (#155, #171). Stage 2, upscaling inside TAA's resolve, not started.

Improve the current rendering-scale and temporal anti-aliasing pipeline.

### Requirements

- Implement an FSR-class spatial upscaling option.
- Investigate temporal upscaling integration.
- Add configurable sharpening.
- Integrate upscaling with the existing rendering pipeline.
- Provide render-scale presets.
- Support quality and performance profiles.
- Avoid unnecessary ghosting and temporal artifacts.

### Acceptance criteria

- Upscaling produces measurable performance improvements at reduced render resolutions.
- Image quality is evaluated against native-resolution output.
- Existing TAA functionality remains available.
- GPU timings are recorded for each quality preset.

## 6.2 Advanced rendering scalability

**Priority:** High
**Status:** Partial (R16). Offline LODs with dithered fades, vegetation LOD and cull distances, retained draw items (M4), parallel frame planning, the 4 × 4 km streaming bench (Phase 80) and per-pass GPU timers exist. GPU-driven drawing (L5) and occlusion culling stay deferred on their measured triggers (`docs/rendering/performance.md`).

Continue improving rendering performance for large and complex scenes.

### Requirements

- Investigate GPU-driven rendering.
- Improve large-scale instance rendering.
- Optimize vegetation rendering.
- Improve LOD transitions.
- Evaluate leaf-card LOD.
- Improve terrain rendering scalability.
- Improve GPU resource management.
- Add advanced rendering diagnostics where necessary.

### Important constraints

Do not implement GPU-driven rendering, occlusion culling or a Nanite-equivalent system merely for feature parity.

Require representative benchmark evidence demonstrating a meaningful bottleneck before adopting major architectural changes.

### Acceptance criteria

- Large-scene benchmarks are maintained.
- Improvements are measured against the existing renderer.
- CPU and GPU bottlenecks are distinguished.
- Performance regressions are documented.

## 6.3 Global illumination improvements

**Priority:** Medium
**Status:** Partial (R18). Baked light probe volumes with stale-bake warnings, baked reflection probes, and optional real-time probe GI (off by default). R18.1 adds the Light Probes overlay, R18.2 marks probes baked inside solids invalid, so walls stop leaking, and R18.3 keeps the sky's share of baked light apart, so volumes follow a sky that dims or changes color. R18.4 adds Fill with Probes (a volume sized to the selection, spaced to a 512-probe budget) and real-time GI tiers by preset, with `bench_light_probes` for the costs. Options and stages R18.1–R18.4 (probe overlay, invalid probes, sky light kept apart, placement and tiers): [global-illumination.md](global-illumination.md).

**Ray-traced GI:** RT2 in [hardware-ray-tracing.md](hardware-ray-tracing.md) (ray-traced probe GI, DDGI-style) writes into the same light probe volumes and replaces `realtime_gi.rs`'s raster cube faces when ray tracing is on. It is R18's additional real-time GI technique on RT-capable GPUs. The rest of R18 (probe placement, diagnostics, quality settings, the non-RT path) applies to every GPU. G9 stage 2 is independent, because RT denoisers run before TAA.

Improve indirect lighting while preserving the existing baked-probe and optional real-time probe architecture.

### Requirements

- Improve probe placement and management.
- Improve indirect-lighting quality.
- Improve dynamic environment lighting.
- Evaluate additional real-time GI techniques.
- Investigate scalable quality settings.
- Improve lighting diagnostics.

### Acceptance criteria

- Existing lighting projects remain functional.
- New techniques can be enabled selectively.
- Performance costs are measurable.
- Low-end hardware remains supported by existing rendering paths.

Do not immediately attempt a complete Lumen or SDFGI equivalent.

## 6.4 Visual effects editor

**Priority:** Medium
**Status:** Done (R17). CPU and GPU particles (100k in 0.23 ms), weather and wind exist and are set through Properties and Lua (`docs/rendering/particles.md`). R17.1 adds curves and gradients: `NumberSequence` and `ColorSequence` values and size, color and transparency over life on both paths, edited in Properties. R17.2 adds spawn shapes (`Shape`, `ShapeStyle`) and `SpeedOverLife` on both paths, and R17.2b bursts (`Bursts`, `Duration`), `VelocityInheritance` and `ShapeInOut` headings, and R17.2c flipbooks (`FlipbookLayout`, `FlipbookMode`). R17.4 adds collisions (`Collides`, `Bounce`, `LifetimeLoss`; physics on the CPU, the depth prepass on the GPU since R17.4b) and sub-emitters (child emitters with `EmitOn`, `EmitCount`). R17.3 adds the Effect tab (a preview with play, loop and slow motion, each emitter's module stack, counts and costs) and effect assets (prefabs in `assets/effects`). R17.5 adds an Environment module, a Place backdrop and six Lighting-aware presets, and decides to keep the module stack instead of a node graph: sub-emitters already wire emitters together, so a graph waits for custom forces or per-particle logic. Design and stages R17.1–R17.5: [vfx-editor.md](vfx-editor.md).

Develop a unified authoring experience around existing CPU and GPU particle systems.

### Requirements

- Node-based effect authoring.
- Particle emitters.
- Particle modifiers.
- Curves and gradients.
- Color over lifetime.
- Size over lifetime.
- Velocity and acceleration controls.
- Collision interactions.
- Sub-emitters.
- Environmental effect integration.
- Wind and weather interaction.
- Effect previewing.
- Reusable effect assets.

### Acceptance criteria

- Effects can be authored without writing scripts.
- Existing particle systems can be reused.
- Effects can be saved and shared between projects.
- Runtime overhead is measurable.
- Visual effect assets integrate with the Toolbox and asset browser.

## 6.5 Advanced world creation

**Priority:** Medium-High
**Status:** Partial (R16). Streaming (`StreamingEnabled`, streaming radii as rings in Play, `RequestStreamAroundAsync`), levels, terrain sculpt, paint and erosion, painted foliage and water exist. Streaming diagnostics in the editor are built: the Streaming overlay (R16) shows each unit by where it stands from the camera, what's always there, both radii, what a player standing there has and units too big to stream well, while editing too. Missing: region management and data layers; their plan is [world-regions-r16.md](world-regions-r16.md) (data layers as tags listed as layers, regions set aside in the editor, streaming while editing, `DataLayerService`; stages R16.2–R16.4).

Improve large-world authoring and environmental systems.

### Requirements

- Better world streaming visualization.
- Improved terrain streaming workflows.
- Streaming diagnostics.
- World partition or equivalent region management where justified.
- Data layers or equivalent environment organization.
- Improved vegetation authoring.
- Better water and environmental effects.
- Improved world-scale lighting workflows.

### Acceptance criteria

- Developers can organize large environments into manageable regions.
- Streaming behavior can be inspected in the editor.
- Large-world performance is benchmarked.
- Existing terrain and world formats remain supported.

## 6.6 Model Studio improvements

**Priority:** Medium-High
**Status:** Partial (R15). Modeling operations, modifiers, UVs, paint, rigging, weights and pose, LOD generation and `.glb` round trips exist (`docs/editor/model-studio.md`). Sculpting, retopology and baking were removed by Phase 90 on 12 Nov 2026; Paul then chose to bring sculpting and baking back (section 2.1), and #271 restored them: Sculpt mode with its brushes, symmetry, masks, Voxel Remesh and Subdivide, and a Bake fold with Make Low Poly and maps baked from the sculpt. Retopology by hand (Poly Build, snapping) stays out. Improving the sculpting tools is still open.

Develop Model Studio into a stronger integrated asset-authoring environment.

### Requirements

- Improve mesh editing workflows.
- Improve UV editing.
- Expand texture painting brushes.
- Improve sculpting tools.
- Improve rigging workflows.
- Improve weight painting.
- Add collision generation and editing.
- Improve model LOD management.
- Add better baking workflows.
- Improve material previews.
- Improve Blender round-trip integration.

### Acceptance criteria

- Common game-asset adjustments can be performed without leaving Axcenity.
- Assets can be previewed using the actual engine renderer.
- Model import and export preserve supported data.
- Blender remains available for advanced external workflows.

## 6.7 Humanoid retargeting

**Priority:** High
**Existing reference:** G16
**Status:** Implemented, stage 1 (#244). A take made for another humanoid skeleton plays through 22 roles found by bone name (Mixamo, Unreal, Blender/Rigify, Biped and similar), with rest-pose correction between the skeletons' worlds and hip travel scaled by leg length (`anim/src/retarget.rs`). Later: a hand-edited role map, retargeting profiles and fingers.

Implement a general-purpose animation retargeting system.

### Requirements

- Bone mapping.
- Rest-pose correction.
- Rig compatibility validation.
- Animation preview.
- Retargeting profiles.
- Reusable rig definitions.
- Support for different humanoid proportions.

### Acceptance criteria

- Animations can be transferred between supported humanoid rigs.
- Retargeting does not require manually recreating animation clips.
- Existing IK and animation systems remain compatible.
- Imported animations can be previewed before applying changes.

---

# 7. Phase 4 — Axcenity Online ecosystem

**Objective:** Establish an optional online ecosystem that provides value to developers without compromising engine ownership or privacy.

This phase represents a potential long-term business opportunity.

The engine must remain fully usable without these services.

**Status:** Research and design only (R19), per the development policy. Collaboration (7.2) is built (section 2.1); accounts have a design (C12). The architecture for all of section 7 (principles, each service, the one `online` client crate, threats, and an order O1–O9 where O1 game update manifests and O2 a server container need no website): [online-services.md](online-services.md).

## 7.1 Account management

Develop account infrastructure through the Axcenity website.

### Requirements

- Optional developer accounts.
- Secure authentication.
- Account settings.
- Developer profiles.
- Team management.
- Project associations.
- API authentication.
- Account recovery.
- Privacy controls.

### Architectural requirements

- The engine must not require account authentication to function.
- Offline development must remain possible.
- Accounts must not become a prerequisite for exporting games.
- Local project ownership must remain independent of cloud accounts.

## 7.2 Collaboration services

Expand existing collaboration functionality.

### Requirements

- Persistent project synchronization.
- Git-backed version history.
- Branch and merge workflows.
- Team permissions.
- Conflict resolution.
- Project recovery.
- Optional remote collaboration infrastructure.
- Clear collaboration setup guidance.

### Acceptance criteria

- Collaboration does not depend on a continuously connected original host.
- Project changes can be recovered.
- Conflicts are reported clearly.
- Local development remains possible.
- Existing self-hosted and peer-to-peer workflows remain supported where practical.

## 7.3 Game publishing and distribution

Develop an optional publishing platform.

### Requirements

- Game publishing from the editor.
- Developer-owned game pages.
- Game version management.
- Update distribution.
- Game descriptions and screenshots.
- Public and private releases.
- Download statistics.
- Optional player discovery.
- Developer-controlled distribution settings.

### Acceptance criteria

- Developers can publish games without surrendering project ownership.
- Games can still be distributed independently.
- Updates do not require rebuilding the entire distribution unnecessarily.
- Developers control what information is collected.

## 7.4 Edge hosting

Develop managed hosting for Axcenity dedicated servers.

### Requirements

- Regional server deployment.
- Dedicated server orchestration.
- Automatic server provisioning.
- Server health monitoring.
- Resource allocation.
- Scaling policies.
- Restart and recovery.
- Server version management.
- Deployment configuration.
- Usage and billing visibility.

### Infrastructure research

Before implementation, evaluate:

- Bare-metal servers versus cloud instances.
- CPU performance and core density.
- RAM requirements.
- NVMe storage.
- Network bandwidth.
- Network transit costs.
- DDoS protection.
- Regional availability.
- Container orchestration.
- Server isolation.
- Monitoring and incident response.

### Acceptance criteria

- A developer can deploy a dedicated game server through a straightforward workflow.
- Server deployments can be monitored.
- Resources and costs are visible.
- Self-hosting remains supported.
- Developers retain control over their games and player data.

## 7.5 Analytics

Develop optional privacy-respecting game analytics.

### Requirements

- Player session counts.
- Concurrent player counts.
- Session duration.
- Server performance.
- Crash reporting.
- Network latency.
- Server resource utilization.
- Developer-defined events.
- Configurable retention.
- Data export.

### Privacy requirements

- No advertising-based monetization.
- No selling player data.
- No mandatory behavioral tracking.
- Clear opt-in and opt-out controls.
- Transparent documentation.
- Minimal data collection by default.

## 7.6 Marketplace and distribution of assets

Build on the existing Toolbox architecture.

### Requirements

- Asset publishing.
- Asset discovery.
- Search and categorization.
- License information.
- Attribution.
- Version management.
- Ratings and reviews.
- Developer asset profiles.
- Optional paid asset distribution in the future.
- Download statistics.

### Acceptance criteria

- Asset licensing is visible before installation.
- Attribution is preserved.
- Developers retain rights to their original assets.
- Marketplace participation is optional.
- The local Toolbox continues functioning without online marketplace access.

## 7.7 Developer dashboard

Provide a centralized website for online services.

### Requirements

- Account management.
- Team management.
- Published games.
- Hosting instances.
- Server analytics.
- Asset publishing.
- Project settings.
- API keys.
- Billing and usage information where applicable.

The website should complement the editor rather than duplicate all of its functionality.

---

# 8. Phase 5 — Platform expansion and advanced capabilities

**Objective:** Expand Axcenity's audience after the desktop engine and its development workflows reach sufficient maturity.

These projects should not delay the core engine roadmap.

**Status:** Research only (R20). Desktop is Windows and Linux first, macOS secondary; mobile is deferred by policy. What each target would need, its blockers and the first spike (Android, then PC VR through OpenXR; web only WebGPU and later; consoles only through platform programs): [platform-expansion.md](platform-expansion.md).

## 8.1 Mobile exports

Investigate:

- Android.
- iOS.
- Mobile input.
- Touch UI.
- Mobile performance profiles.
- GPU capability tiers.
- Mobile asset compression.
- Mobile packaging and signing.

Start with Android as the initial target if the rendering backend and dependencies support a practical implementation.

## 8.2 Web exports

Investigate:

- WebAssembly.
- WebGPU.
- Browser input.
- Browser audio.
- Asset streaming.
- Browser memory constraints.
- Web packaging.
- Multiplayer connectivity.

Do not commit to a web export until the runtime and graphics architecture can support it without excessive duplication.

## 8.3 VR and AR

Investigate:

- OpenXR.
- VR rendering.
- Stereoscopic rendering.
- Motion controller input.
- VR interaction tools.
- Spatial audio.
- AR capabilities where supported.

## 8.4 Console support

Investigate platform-specific requirements, licensing, certification and development hardware.

Console support should remain conditional on technical feasibility and access to the relevant platform programs.

## 8.5 Advanced simulation

Potential future features:

- Cloth simulation.
- Soft-body physics.
- Advanced hair simulation.
- Motion matching.
- Active ragdolls.
- More advanced procedural animation.

Only prioritize these when real game-development use cases justify the work.

---

# 9. Features deliberately deferred or excluded

The following should not become immediate development priorities.

| Feature | Decision | Reason |
|---|---|---|
| Complete Nanite equivalent | Deferred | Requires demonstrated large-world need |
| Full Lumen equivalent | Deferred | Existing GI architecture is sufficient for current scope |
| Hardware ray tracing | Planned, optional (RT0–RT5) | Paul overrode the deferral on 2026-11-12: [hardware-ray-tracing.md](hardware-ray-tracing.md) (PR #240), off by default and only on RT-capable GPUs |
| Occlusion culling overhaul | Deferred | Existing measurements do not justify it |
| Parallel Luau | Deferred | Current script performance does not justify added complexity |
| ECS replacement | Do not pursue currently | Preserve the Instance architecture |
| Native plugin ABI | Deferred | Existing Lua plugin system |
| Separate scripting language | Do not pursue currently | Avoid fragmentation |
| Motion matching | Deferred | Limited immediate benefit |
| General-purpose AI editor assistant | Deferred | Requires service, privacy and cost decisions |
| Full Blender replacement | Do not pursue | Blender remains a valuable integration target |
| Mandatory cloud accounts | Excluded | Conflicts with project philosophy |
| Mandatory hosted game distribution | Excluded | Developers retain independent distribution |

# 10. Testing and quality requirements

All development must use the project's existing testing and TDD conventions.

## 10.1 Before implementation

For every feature:

1. Inspect existing implementation.
2. Identify existing related systems.
3. Review existing documentation and open PRs.
4. Identify architectural dependencies.
5. Identify compatibility requirements.
6. Establish performance baselines where applicable.
7. Write tests before implementation.

## 10.2 During implementation

- Follow existing Rust and Lua conventions.
- Preserve modular architecture.
- Avoid unnecessary dependencies.
- Maintain cross-platform compatibility.
- Add regression tests.
- Update relevant documentation.
- Keep PRs focused.
- Benchmark performance-sensitive changes.

## 10.3 Before merging

Every PR should include:

- Tests passing.
- Formatting and linting passing.
- Documentation updated.
- Compatibility verified.
- Performance measured where relevant.
- No unintended changes to existing functionality.
- Updated implementation status in the roadmap.

CI should follow the project's existing development policy rather than introducing an unrelated CI infrastructure project.

# 11. Competitive feature tracking

Maintain a living competitive feature matrix.

Compare Axcenity against:

- Unreal Engine.
- Godot.
- s&box.
- Unity.

For every major feature, record:

| Field | Description |
|---|---|
| Feature | Name of the capability |
| Axcenity status | Implemented, partial, missing or deferred |
| Unreal status | Available capability |
| Godot status | Available capability |
| s&box status | Available capability |
| Unity status | Available capability |
| Implementation reference | Relevant Axcenity source, crate or documentation |
| Priority | Critical, high, medium or low |
| Dependencies | Required systems |
| Decision | Implement, improve, defer or exclude |

### Status definitions

- **Implemented:** The feature is functional and has supporting tests.
- **Partial:** Some functionality exists, but important capabilities remain incomplete.
- **Missing:** No meaningful implementation exists.
- **Planned:** Accepted for future development.
- **Deferred:** Deliberately postponed.
- **Not applicable:** The feature does not fit Axcenity's architecture or philosophy.

Do not mark a feature as implemented solely because a similarly named module or API exists. Verify actual user-facing functionality.

Revisit the comparison whenever a major release of a competing engine introduces relevant functionality.

## 11.1 The matrix

Checked on main `5143a5a` (12 Nov 2026). Peer columns are claimed capabilities (Unreal 5.7, Godot 4.6, s&box April 2026, Unity 6.3 LTS), not measured. A row's Axcenity status names what's missing when it isn't Implemented; the item ID links it to section 12.

### Core engine and scripting

| Feature | Axcenity | Unreal | Godot | s&box | Unity | Reference | Priority | Dependencies | Decision |
|---|---|---|---|---|---|---|---|---|---|
| Scripts attached to objects | Implemented | Components, Blueprints on actors | Script per node | C# components | MonoBehaviours | `project` `object_scripts`, `docs/project/world.md` (G2) | Critical | project, script, editor | Keep |
| Physics render interpolation | Implemented | Physics smoothing, async physics | Physics interpolation (4.3+) | Rigidbody interpolation | `Rigidbody.interpolation` | `physics/src/blend.rs` (G3) | Critical | physics, scene | Keep |
| Code completion in the built-in editor | Implemented | n/a (IDE, Blueprints) | Built-in editor with completion | IDE (C#) | IDE (C#) | `editor/src/script_completion.rs` (G4) | Critical | script definitions | Keep |
| Inline diagnostics | Implemented (needs luau-lsp installed) | IDE | Built-in | IDE | IDE | `editor/src/luau_check.rs` (G4) | Critical | luau-lsp | Keep |
| Go to definition | Implemented (#239) | IDE | Built-in | IDE | IDE | `script_completion::definition_at` (G4) | High | completion | Keep |
| Debugger: breakpoints, stepping, watches | Implemented (script-made coroutines can't pause) | Blueprint and C++ debuggers | Built-in debugger | IDE debugger | IDE debugger | `script/src/debugger.rs` (G6) | High | script | Keep; improve coroutines when a game needs it |
| Hot reload of game code | Implemented (scripts keep state) | Live Coding | Script reload | C# hotload | Domain reload | Phase 89, ADR-0023 | High | script | Keep |
| Parallel scripting | Deferred | n/a | Threads | Async tasks | Jobs | gap analysis P4 | Low | script | Defer (scripts 0.74 ms of XB10k) |
| Native extension ABI | Deferred | C++ modules | GDExtension | n/a (C#) | Native plugins | gap analysis §6 | Low | — | Defer |
| ECS | Not applicable | Mass | n/a | n/a | Entities | gap analysis §6 | — | — | Exclude (Instance tree) |

### Assets and builds

| Feature | Axcenity | Unreal | Godot | s&box | Unity | Reference | Priority | Dependencies | Decision |
|---|---|---|---|---|---|---|---|---|---|
| Built-in asset discovery | Partial: card drag into the scene (#238) and the Assets panel; no sound preview | Fab | Asset Library | sbox.game assets | Asset Store | `crates/toolbox`, `crates/editor-toolbox` (G1) | Critical | toolbox | Improve |
| Licence and attribution tracking | Implemented | Fab licences | Per asset | Platform | Asset Store EULA | `toolbox` `license.rs`, `provenance.rs`, CREDITS.txt | Critical | toolbox | Keep |
| Missing and broken references | Implemented | Reference viewer, map check | Dependency editor | Editor | Missing reference warnings | `project::check_assets` | High | project | Keep |
| Unused asset report | Implemented (never deletes) | Size map, audit | Orphan explorer | — | Third-party | `project::unused_assets` | High | project | Keep |
| Duplicate detection | Done (Asset Check Merge) | Audit tools | — | — | Third-party | R12 | Medium | project | Implement |
| Import presets and compression profiles | Done (textures, #262) | Texture groups, LOD settings | `.import` presets | Per asset | Presets, platform overrides | R12 | Medium | assets, cook | Implement |
| Cooked textures and models | Implemented | Cooking | Import cache | Compiled assets | Import pipeline | L2, `crates/cook` | Critical | cook | Keep |
| Cooked-only game packages | Implemented (#243) | Cooked pak | `.pck` | Compiled | Built player | G12 | High | cook, project | Keep |
| Build profiles and reports | Implemented (#268) | Build profiles, cook reports | Export presets | Publish wizard | Build profiles, build report | R13 | High | project | Implement |

### Editor and production

| Feature | Axcenity | Unreal | Godot | s&box | Unity | Reference | Priority | Dependencies | Decision |
|---|---|---|---|---|---|---|---|---|---|
| Prefabs with overrides, nesting, variants | Implemented | Blueprints, Level Instances | Scenes | Prefabs | Prefabs | Phase 49, `docs/editor/assets.md` | High | project | Keep |
| UI prefabs | Implemented | Widget Blueprints | Scenes | Razor panels | UI prefabs | G5 | High | ui, project | Keep |
| Memory breakdown | Implemented, with history, peaks and budgets (#241) | Memory Insights | Monitors | — | Memory Profiler | `editor/src/profiler.rs`, `core::memory` (G7) | High | sim, render | Keep |
| CPU and GPU profiling | Implemented | Unreal Insights | Profiler | Profiler | Profiler | Profiler panel, Tracy, `--bench` | High | — | Keep |
| Editor plugins | Implemented: drawing, handles, Properties rows, importers, Assets panel items (G10) | Editor modules, Utility Widgets | EditorPlugin | Editor tools (C#) | Editor scripting | `crates/plugins` (G10) | Medium-High | plugins | Improve |
| Simulate without a player | Implemented (Run, F8, #245) | Simulate | Run scene | — | Play without player | G14 | Medium | editor Play | Keep |
| Physics debug views | Implemented: colliders, rays, contacts, centers of mass, sleeping (#253) | Chaos Visual Debugger | Visible collision shapes | Physics debug | Physics Debugger | G15 | Medium | physics | Keep |
| Live collaboration | Implemented (hand checks open) | Multi-User Editing | — | — | Unity Version Control | Collaborate 2.0, ADR-0022 | Strategic | collab, repo | Keep |
| Editor modularization | Implemented for planned moves | Modules | Modules | — | Packages | G11 | Medium | — | Continue when it pays off |
| AI assistant in the editor | Deferred (MCP server for outside clients exists) | AI Assistant | — | — | Unity AI | `docs/editor/mcp.md` | Low | — | Defer |

### Rendering and world

| Feature | Axcenity | Unreal | Godot | s&box | Unity | Reference | Priority | Dependencies | Decision |
|---|---|---|---|---|---|---|---|---|---|
| Spatial upscaling | Implemented | TSR | FSR 1 | Source 2 upscaling | STP, FSR | `render` `upscale.wgsl` (G9 stage 1) | High | render | Keep |
| Temporal upscaling | Missing | TSR, DLSS/FSR plugins | FSR 2 | DLSS | STP, DLSS | G9 stage 2 | High | TAA | Implement |
| GPU-driven drawing | Deferred | Nanite | — | — | GPU Resident Drawer | L5, gap analysis P4 | Medium | M12 | Defer until a benchmark needs it |
| Occlusion culling | Deferred (measured ≤ 1.9 ms) | HZB | Occluders | PVS | Umbra | `docs/rendering/performance.md` | Low | — | Defer |
| LOD and vegetation | Implemented (leaf-card LOD open) | HLOD, Nanite foliage | Mesh LOD | LODs | LOD Groups, SpeedTree | `docs/rendering/models.md`, vegetation phases | High | render | Improve on measurement |
| Global illumination | Partial: baked probes, optional real-time probes, Light Probes overlay (R18.1), invalid probes (R18.2), sky light that follows the sky (R18.3), Fill with Probes and real-time GI tiers (R18.4) | Lumen | SDFGI, VoxelGI | Baked, probes | APV, lightmaps | `docs/rendering/lights.md` (R18) | Medium | render | Improve selectively |
| Ray tracing | Planned, optional (RT0–RT5) | Yes | — | — | HDRP | [hardware-ray-tracing.md](hardware-ray-tracing.md) (PR #240) | Medium | RT-capable GPU; L5 only for RT5 | Implement as an option, sun shadows first (RT1) |
| Particle authoring (VFX editor) | Done (curves over life, spawn shapes, bursts, flipbooks, the Effect tab and effect assets, collisions, sub-emitters, environment and presets; a module stack rather than a node graph) | Niagara | Particle editors | Particle editor | VFX Graph | `docs/rendering/particles.md` (R17) | Medium | particles | Implement in stages |
| World streaming | Implemented at runtime; Streaming overlay in the editor (R16) | World Partition | — | — | Additive scenes | `docs/architecture/streaming.md` (R16) | Medium-High | core streaming | Improve |
| Region management and data layers | Missing | World Partition, Data Layers | — | — | Scenes | R16 | Medium | levels | Implement where justified |
| In-editor modeling | Partial: sculpting and baking back (#271), retopology by hand left to Blender | Modeling Mode | — | Mapping (Hammer) | ProBuilder | `crates/editor-model-studio` (R15) | Medium-High | mesh_edit | Restore sculpt and bake, then improve |
| Humanoid retargeting | Implemented (stage 1) | IK Retargeter | Retarget on import | Citizen | Humanoid Avatars | G16 | High | anim | Hand-edited maps, profiles |
| Runtime IK | Implemented | Control Rig | SkeletonIK3D | IK | Animation Rigging | `anim/src/ik_control.rs` | — | anim | Keep |

### Multiplayer, online and platforms

| Feature | Axcenity | Unreal | Godot | s&box | Unity | Reference | Priority | Dependencies | Decision |
|---|---|---|---|---|---|---|---|---|---|
| Client/server, replication, dedicated server | Implemented | Yes | High-level multiplayer | Yes | Netcode for GameObjects | `docs/architecture/networking.md` | Critical | net | Keep |
| Lag compensation and replays | Implemented (hand check open) | Yes | — | — | Partial | Phase 81 | High | net | Keep |
| Scale test (16–32 clients on a LAN) | Planned | — | — | — | — | G13 | High | Paul's machines | Run when the PC is free |
| Accounts | Missing (design C12) | Epic accounts | — | Steam | Unity ID | `docs/research/collaborate-accounts.md` (R19) | Strategic | — | Design only |
| Game publishing platform | Missing (Build Game, installers) | Epic Store (separate) | — | sbox.game | Unity Gaming Services | R19 | Strategic | — | Design only |
| Managed dedicated hosting | Missing (self-hosted server exists) | — | — | Valve servers | Multiplay | M11, R19 | Strategic | — | Design only (policy) |
| Analytics | Missing | — | — | Platform stats | Unity Analytics | R19 | Strategic | — | Design only |
| Asset marketplace | Partial: Toolbox packs and the website companion | Fab | Asset Library | sbox.game | Asset Store | R19 | Strategic | toolbox | Design only |
| Desktop: Windows, Linux, macOS | Implemented | Yes | Yes | Windows | Yes | `docs/architecture/portability.md` | Critical | — | Keep |
| Mobile | Deferred | Yes | Yes | No | Yes | unified Phases 82–88 (R20) | Long-term | wgpu | Research only |
| Web | Missing | No | Yes | No | Yes | R20 | Long-term | wgpu WebGPU | Research only |
| VR/AR | Missing | OpenXR | OpenXR | VR | XR Toolkit | R20 | Long-term | render | Research only |
| Consoles | Not applicable now | Yes | Via partners | No | Yes | R20 | — | platform programs | Research only |
| Cloth and soft bodies | Missing | Chaos Cloth | SoftBody3D | Cloth | Cloth | R20 | Low | physics | Defer until a game needs it |
| Ragdolls | Implemented (active ragdolls deferred) | Physical animation | Ragdolls | Ragdolls | Ragdolls | `docs/architecture/physics.md` | — | physics | Keep |

# 12. Recommended implementation sequence

The following is the recommended order for execution. **Status** is on main `5143a5a`; **Next** is this roadmap's work for the item.

| Order | Work item | Reference | Status | Next |
|---|---|---|---|---|
| 1 | Physics interpolation | G3 | Implemented (#110) | Nothing; hand check at 120–240 fps on the PC |
| 2 | Scripts inside objects | G2 | Implemented (#111–#120, #230) | Nothing |
| 3 | Complete Toolbox integration | G1 | Partial (card drag into the scene #238 and the Assets panel) | Sound preview |
| 4 | Script completion and diagnostics | G4 | Implemented (#153, #239) | Nothing |
| 5 | Memory profiler | G7 | Implemented (#241) | Nothing |
| 6 | Audio occlusion and roll-off | G8 | Implemented (#128) | Nothing |
| 7 | Debugger stepping and watches | G6 | Implemented (#131) | Nothing now (coroutine limit recorded) |
| 8 | UI prefabs | G5 | Implemented (#122) | Nothing |
| 9 | Cooked-only games | G12 | Implemented (#243) | Hand check a real game's size on the PC |
| 10 | Humanoid retargeting | G16 | Implemented (stage 1, #244) | Hand-edited maps, profiles, fingers |
| 11 | Plugin API improvements | G10 | Implemented | #265, #267, #270, #274 (scripting thread) |
| 12 | Advanced asset management | New (R12) | Implemented (#260, #262) | Mesh presets later, if needed |
| 13 | Build pipeline improvements | New (R13) | Implemented (#268) | Build profiles and reports done |
| 14 | Temporal upscaling | G9 | Partial | Stage 2 inside TAA |
| 15 | Advanced Model Studio workflows | New (R15) | Partial: sculpting and baking back (#271) | Improve the sculpting tools |
| 16 | Large-world rendering and authoring | New (R16) | Partial: Streaming overlay built | Region management and data layers where justified |
| 17 | Visual effects editor | New (R17) | Implemented: curves over life (R17.1), spawn shapes and speed over life (R17.2), bursts, inherited velocity and headings (R17.2b), flipbooks (R17.2c), the Effect tab and effect assets (R17.3), collisions and sub-emitters (R17.4, R17.4b), environment and presets (R17.5; the stack kept, no node graph); design in [vfx-editor.md](vfx-editor.md) | None |
| 18 | Global illumination improvements | New (R18) | Stages R18.1–R18.4 done; options and stages in [global-illumination.md](global-illumination.md) | PC numbers from `bench_light_probes`; probe visibility only if leaks remain; RT2 in the ray tracing thread |
| 19 | Online services architecture | New (R19) | Designed in [online-services.md](online-services.md) | Build only when Paul says; O1 and O2 need no website |
| 20 | Platform expansion | New (R20) | Deferred; researched in [platform-expansion.md](platform-expansion.md) | Spikes (Android, OpenXR) only when Paul lifts the deferral |

Also done: G14 (Run without a player, #245) and G15 (physics debug views, #253). Still open: G13 (LAN scale test) when Paul's machines are free.

This sequence is a default dependency-aware recommendation, not a requirement to block all independent work.

# 13. Implementation management

Each roadmap item should be implemented as a separate engineering task or focused group of PRs.

Every task must include:

1. Problem statement.
2. Existing implementation assessment.
3. Proposed architecture.
4. User-facing functionality.
5. Dependencies.
6. Compatibility considerations.
7. Security considerations where applicable.
8. TDD test plan.
9. Performance expectations.
10. Acceptance criteria.
11. Documentation updates.

Large features should be divided into independently reviewable milestones.

Do not combine unrelated features into oversized PRs.

## 13.1 Existing work takes precedence

Before opening a new task:

- Check existing GitHub issues.
- Check open and merged pull requests.
- Check the existing roadmap.
- Check related feature documentation.
- Identify whether the feature is already partially implemented.
- Reuse existing architecture wherever practical.

If existing work already satisfies the requirement, update the status instead of implementing it again.

## 13.2 Release management

Major roadmap milestones should be tied to meaningful engine releases.

Do not make a release solely because a certain number of tasks have been completed.

A release should represent a stable, documented improvement to the development experience.

# 14. Definition of success

Axcenity should be considered successful in this roadmap when it achieves the following:

### Development experience
- Developers can create and script reusable objects.
- The built-in code editor provides a professional development experience.
- Debugging is practical without external tools.
- Assets and prefabs are reusable and portable.

### Production capabilities
- Large projects can be organized and packaged reliably.
- Developers can identify performance and memory bottlenecks.
- Model, animation and UI workflows are integrated.
- The editor remains responsive as projects grow.

### Graphics and performance
- High-quality rendering is available across supported hardware.
- Performance improvements are benchmarked.
- Large environments are practical to build and run.
- Rendering tools are accessible without sacrificing advanced functionality.

### Multiplayer and online services
- Multiplayer development remains accessible.
- Dedicated servers are reliable.
- Collaboration is not dependent on one continuously connected host.
- Optional hosting and analytics provide genuine developer value.

### Ecosystem
- Developers can discover and manage assets.
- Publishing and distribution are straightforward.
- External services are optional.
- Developers retain ownership and control.

### Platform support
- Desktop remains stable and well supported.
- Mobile, web and other targets are introduced when technically mature.

# 15. Final strategic direction

Axcenity should not attempt to become a direct clone of Unreal, Godot, s&box or Unity.

Its competitive advantage should come from combining capabilities that are often separated across multiple products:

- An approachable editor.
- Advanced built-in development tools.
- Integrated modeling and animation workflows.
- Powerful rendering and physics.
- Built-in multiplayer development.
- Open-source architecture.
- Optional online infrastructure.
- Developer privacy and ownership.

The engine should be capable enough for professional development while remaining approachable to someone learning game development for the first time.

**The long-term objective is simple: make Axcenity an engine where developers can build, test, optimize, collaborate on and publish complete games without being forced into a proprietary ecosystem.**

---

**Document maintenance:** Update this roadmap as features are implemented, deferred or reprioritized. Existing implementation plans remain authoritative for their respective engineering tasks. This document serves as the unified strategic development roadmap.
