# Visual effects editor (R17): design

Roadmap item R17, section 6.4 of [competitive-development-roadmap.md](competitive-development-roadmap.md). This is the design and the stage plan; each stage is its own PR.

## What exists

- **One particle system, two places to run it.** `ParticleEmitter` (`core/src/class.rs`) is moved on the CPU by `scene::Particles` (`scene/src/particles.rs`, at most 4,000 per emitter) or, from `Rate × Lifetime` of 3,000, on the GPU (`render/src/renderer/gpu_particles.rs`, `particles.wgsl`, up to 2¹⁸ per emitter, 100k in about 0.23 ms on an RX 9070 XT). Both read the same properties and must keep moving and looking alike (`docs/rendering/particles.md`).
- **Each look is a start and an end.** `Size`/`SizeEnd`, `Color`/`ColorEnd` and `Transparency`/`TransparencyEnd` blend linearly over a particle's life; `render::ParticleLook` carries them as two-entry arrays. Motion is `Speed`, `Direction`, `SpreadAngle`, `Acceleration`, `Drag` and `WindAffectsDrag`; turning is `Rotation` and `RotSpeed`.
- **Where particles come from:** inside the emitter's part, or a 0.1-stud box for a free emitter. `Emit(count)` bursts from Lua, blocks and the Emission tool.
- **Authoring** is Properties, the Emission tool's presets (`build_tools::particle_preset`) and Lua. The viewport shows emitters while editing (Phase 37). Prefabs (`*.prefab.json`) save and share any tree, and global assets carry them between projects. Weather and wind (`docs/rendering/weather.md`, `wind.md`) already move particles.
- **Missing:** curves and gradients, sub-emitters, collisions, an effect asset, a place to build and preview one effect, and a measured cost per effect.

## Decisions

- **D1. No second particle system.** An effect is a tree of the Instances that exist: ParticleEmitters (in a part or free), and Lights, Sounds and other emitters beside them. Everything R17 adds is a property the CPU and GPU paths both honour, so an effect made in the editor plays in the player, through Lua and on either path. The effect editor is a way to edit that tree, not a format of its own.
- **D2. Curves are values, with Roblox-familiar names.** Two new `Value` types, `NumberSequence` (keypoints of time 0–1, value and envelope, the envelope a random spread per particle) and `ColorSequence` (keypoints of time and Color3), with `NumberSequence.new(...)`, `ColorSequence.new(...)`, `NumberSequenceKeypoint` and `ColorSequenceKeypoint` in Lua and save files. They are Value types, so Properties, undo, Collaborate, prefabs, replication and `.axgame` packing carry them like any other value.
- **D3. The old properties keep working.** The emitter gains `SizeOverLife`, `TransparencyOverLife` (NumberSequence) and `ColorOverLife` (ColorSequence), empty by default. Empty means the start and end values blend as today, so no project, script or save changes. Set, a sequence is used instead of its pair, and Properties greys the pair out. (Roblox's `Size` *is* a sequence; making Axcenity's `Size` one would break every saved emitter and script that sets a number, against the development policy's "reliable over premature".) `SpeedOverLife` (a NumberSequence multiplying `Speed`) moved to R17.2 beside `VelocityInheritance`: it changes how both paths move particles, not how they look, so it belongs with the motion work.
- **D4. Curves reach the GPU as tables.** Each emitter's sequences are sampled into 32 evenly spaced entries (`ParticleLook` grows a curve table; the CPU path reads the same table), so the shader does one lookup and blend, and the CPU and GPU paths can't drift apart. 32 entries keep a sharp step within 3% of a particle's life; the table is uploaded only when a sequence changes.
- **D5. The effect asset is a prefab.** An effect saves as `*.effect.json`, the prefab format with `"kind": "effect"` and a preview setup (camera distance, loop length, background). It is listed under the Assets panel's Particles chip, can be a global asset, and the Toolbox can install one like any prefab. Placing it is placing a prefab, so overrides, apply and revert already work.
- **D6. A stack first, a node graph later.** Stage 3's editor shows each emitter as a stack of modules in the order a particle lives them (Spawn: rate, bursts, shape; Birth: speed, direction, size, color, rotation; Over life: the sequences, drag, acceleration, wind; Render: texture, emission, brightness), each a group of the emitter's properties with curve and gradient editors. The roadmap asks for node-based authoring; a free graph (`editor-node-graph`) is the right tool once modules need wiring (events between emitters, custom forces), and stage 5 decides it with the stack in hand. Until then a graph would only redraw the property list as boxes.
- **D7. Costs are measured per effect.** The effect editor's preview shows its particle count, the CPU step and the GPU particle pass time (`RenderStats` already times `FramePass::Particles`), and `bench_effects` (ignored, release) times a set of sample effects on the PC, so "runtime overhead is measurable" is a number per effect.

## Stages

| Stage | What | Tests | PC check |
|---|---|---|---|
| R17.1 Curves | `NumberSequence`, `ColorSequence` values (core, serialization, Lua, Properties' curve and gradient editors), `SizeOverLife`, `TransparencyOverLife`, `ColorOverLife`, the 32-entry table on both paths | Round trips (save, Lua, replication), CPU and GPU particles match at sampled ages, empty sequences draw as before | A GPU emitter with curves on the 9070 XT |
| R17.2 Spawn and motion | Bursts (`Bursts` sequence of time and count, looping with `Duration`), emission shapes (`Shape`: Box, Sphere, Cylinder, Disc, Surface), `VelocityInheritance`, `SpeedOverLife`, flipbooks (`FlipbookLayout`, `FlipbookFramerate`) | Shape bounds, burst counts per loop, CPU and GPU match | Visual check of flipbooks |
| R17.3 Effect editor | An Effect tab: the module stack, a preview viewport (play, pause, restart, loop, slow motion, a ground and background), counts and costs (D7), Save as `*.effect.json`, Assets and Toolbox listing | Saving and placing an effect, overrides, the stack edits the emitter's properties with undo | Preview cost readout against `RenderStats` |
| R17.4 Collisions and sub-emitters | `Collides` (CPU: physics rays, at most a budget per frame; GPU: against the depth prepass), `Bounce`, `LifetimeLoss`; `SubEmitters` (on birth, on death, on collision) naming child emitters | Particles stop on a floor on both paths; a death burst spawns where particles die | GPU collision cost at 100k particles |
| R17.5 Environment and graph | Weather and wind modules in the stack, Lighting-aware presets, and the D6 decision on a node graph | As each lands | `bench_effects` numbers into `docs/rendering/particles.md` |

Each stage updates `docs/rendering/particles.md` (and stage 3 `docs/editor/`), the wiki's rendering and editor pages, and the roadmap's 6.4 status.

## Acceptance (section 6.4)

- **Without scripts:** stages 1–3 make every look and spawn setting editable in the stack and Properties.
- **Existing systems reused:** D1 and D4.
- **Saved and shared between projects:** D5 (prefab format, global assets, Toolbox).
- **Runtime overhead measurable:** D7.
- **Toolbox and Assets:** D5.

## Risks

- **CPU and GPU drifting apart.** The shared table (D4) and tests that compare the two paths at set ages are the guard; every stage adds to them.
- **Shader limits.** The sprite and particle shaders don't share the scene shader's 16-texture budget, but the curve table adds a storage buffer per frame to `particles.wgsl`; WebGL 2 has no compute and stays on the CPU path as today.
- **Scope.** Stage 4's GPU collisions read the depth prepass and need it alive (as ambient occlusion does); if that costs too much at Low, collisions stay CPU-only there.
