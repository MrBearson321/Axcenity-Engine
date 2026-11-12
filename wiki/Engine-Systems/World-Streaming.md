---
title: World streaming
type: system
status: current
implementation: partial
crates: [core, scene, net, physics, render]
source: [crates/core/src/streaming.rs, crates/scene/src/streaming.rs, crates/net/src/streaming.rs, crates/render/src/renderer/uploads.rs]
docs: [docs/architecture/streaming.md]
verified: 2026-11-12
verified_commit: 81874e76
tags: [system/streaming, large-worlds]
---

# World streaming

> [!summary]
> Phase 80: on a world too big to keep whole, each device has only what's near it. **Off by default** (`Workspace.StreamingEnabled`); with it off nothing here runs and projects play exactly as before. `core::streaming::Partition` splits the Workspace into **units** that load together (a loose part, a Model without nested Models, an `Atomic` Model); the server sends each joined player their share; a host's own screen leaves out of range content, frees its GPU copies, builds terrain chunks on a thread and lets far physics sleep. **Partial**: built and tested, one plan criterion still open, not on by default.

## Architecture

```mermaid
flowchart LR
  P["core::streaming::Partition<br/>(split again when the Workspace's<br/>structure, the World or the characters change)"] --> NS["net::streaming Streams<br/>per player: MinRadius now, out to TargetRadius<br/>nearest first, 4,000 Instances/s"]
  P --> SS["scene::Streaming (host's screen)<br/>in range of the camera"]
  SS --> DL[build_draw_list leaves the rest out]
  SS --> GPU["GPU frees after KEEP_UNDRAWN_FRAMES;<br/>background uploads when back in range"]
  SS --> TC[terrain chunks built on a worker]
  SS --> PH[Physics::set_far_sleep]
  P --> OV["editor Streaming overlay (R16)<br/>unit boxes by Reach, legend from<br/>Partition::summary"]
```

## Settings

`StreamingMinRadius` (64), `StreamingTargetRadius` (1024), unload 25% past the target; `Model.ModelStreamingMode` `Default`/`Atomic`/`Persistent`. Characters, LocalScript holders and Persistent Models always stay.

## Performance

**Measured** (`bench_50000_parts_streaming`, RX 9070 XT, release): 50,000 parts over 4 km: 6.2 ms physics, 0.7 ms streaming, against a 16.7 ms budget (`docs/architecture/streaming.md`).

**Measured** (streaming world, `axcenity-test --make-streaming`, Container, release, `--bench 600`): the server with one joined player went from 21.9 to 4.2 ms a frame, Network send 16.7 → 2.0 ms, after ownership, replication and the partition stopped reading every part every frame. A teleport's tick sends up to a second's budget at once (about 14 ms here). The desktop numbers that tick the plan's criterion are still to be taken ([Optimization log](../Performance/Optimization-Log.md)).

## Limitations

- A flat Model with thousands of loose parts is one unit; big maps must be nested in Models or Folders to stream. The editor's Streaming overlay names a unit of more than 1,000 Instances.
- A texture draws white until its background upload finishes.
- Editing always draws everything; Play streams around the camera. The Streaming overlay (R16) shows what would stream around the camera while editing: unit boxes by reach, always-there parts, radii and counts. There's still no region management or data layers (R16's other parts).
- A change to the Workspace's tree still splits the whole partition again (about 12 ms for 51,000 parts in the container); ownership and physics catch up from the moves log instead.

## Related documentation

`docs/architecture/streaming.md`; [Networking](Networking.md), [Scene bridge](Scene-Bridge.md); `examples/hello-streaming`
