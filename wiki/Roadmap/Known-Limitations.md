---
title: Known limitations
type: roadmap
status: current
verified: 2026-11-11
verified_commit: 19c7179d
docs: [docs/scripting/sandbox.md, docs/rendering/pipeline.md, docs/architecture/physics.md, docs/architecture/networking.md, docs/architecture/play-mode.md, docs/scripting/client-server.md, docs/rendering/lights.md]
tags: [roadmap, limitations]
---

# Known limitations

What the engine doesn't do, or does with a known flaw, today. Each entry names where it's documented. Performance issues are in [Known bottlenecks](../Performance/Known-Bottlenecks.md); structural problems in [Technical debt](Technical-Debt.md).

## Scripting

- No per-script **memory limit**; long C functions (`string.rep`, pathological patterns) aren't interrupted by the watchdog (`docs/scripting/sandbox.md`).
- `task.defer` runs at the next step (Roblox: later in the same frame); a RemoteEvent message with no handler is dropped (Roblox queues it); `ContextActionResult.Pass` and touch buttons not implemented (`docs/scripting/client-server.md`).
- Vector3 math allocates a userdata per result; `--!native` doesn't speed up engine calls.
- A breakpoint inside a script-made coroutine or a C-called Lua function can't pause; a step goes on to the next line where it can. The same goes for a conditional breakpoint, a logpoint and Pause (Pause waits for the next script line that can pause, so a game whose scripts are all waiting pauses only when one wakes). Logpoints and conditions make their line yield on every pass, which slows a hot loop.

## Rendering

- Overlapping see-through items can blend in the wrong order (sorted by center).
- AMD/Vulkan/Windows: the sky clear isn't sRGB-encoded through an sRGB view (`docs/rendering/pipeline.md`).
- Lights are tiled in 2D, not clustered; rect lights use a representative point.
- No vertex colors or `KHR_materials_emissive_strength` from glTF; FBX uses the first UV set; glTF animation isn't read.
- Humanoid retargeting finds roles by bone names only: a skeleton with unusual names can't be mapped by hand yet; fingers aren't retargeted (`docs/rendering/animation.md`).

## Physics

- MeshParts collide as convex hulls (hollows filled) unless already convex and small, their model has `_COL` collision pieces, or they're set to `PreciseConvexDecomposition` (R15.2). The Model Studio's Convex Pieces (R15.2b) runs on the UI thread and can take a few seconds on a big model.
- Setting `Position` stops a part (Roblox keeps its speed).
- Ragdolls can exceed joint limits briefly on hard landings.

## Networking

- **Unsecured netcode**: no encryption or connect tokens; LAN and direct-address play only.
- No relay, matchmaking, lag compensation or replays. Multiplayer has been measured with 8 loopback players; a 16–32 client LAN run (G13) is planned.
- Editor Play can't host or join a real network game.

## Editor

- Script completion and Go to Definition (F12) follow chains and declarations but doesn't type-check: a value it can't follow (a function's result, a table field, a module's return) offers nothing, Error underlines need luau-lsp installed (it isn't shipped), check one script at a time without the others (a `require`d module's types are unknown), and check only open tabs.
- Panics while drawing the Play world aren't caught.
- Model files aren't watched for changes while the editor runs (read again on open).
- Bakes (reflection probes, light probe volumes) go stale and must be rebaked by hand (flagged). A bake runs a little each frame with progress and Cancel, but each probe still reads its six faces back one at a time, so thousands of probes take minutes.

## Content and platforms

- References may name an asset by path or by ID (PR #78); older projects still use paths.
- Built games ship cooked textures without their sources (G12); what the CPU reads of such a texture (terrain layers, color grading lookups, UI images) is decoded from BC7 or BC5, so it's close to the source, not equal. Model files (FBX, glTF) still ship beside their cooked copies.
- macOS is not verified on changes; mobile and web are not supported.
- No binary world format (S10k's 10 MB `world.json` loads in 0.19 s since C11).
