# World streaming

Phase 80. On a world too big to keep whole, each device only has what's near it. Off by default: with `Workspace.StreamingEnabled` off nothing here runs, so a project plays exactly as before.

## Settings

- `Workspace.StreamingEnabled` (off), `StreamingMinRadius` (64 studs), `StreamingTargetRadius` (1024). The target is never less than the minimum, and what's loaded stays until it's 25% past the target (`UNLOAD_MARGIN`), so the edge doesn't flicker.
- `Model.ModelStreamingMode`: `Default`, `Atomic` or `Persistent`.

## What streams

`core::streaming::Partition` splits the Workspace into **units**, the pieces that load and unload together:

- A BasePart loose in the Workspace or a Folder is a unit alone.
- A Model with no Model inside it is one unit (`Default`). A Model with Models inside streams like a Folder: each piece on its own.
- An `Atomic` Model is one unit, nested Models and all.
- A `Persistent` Model, a Player's character, and anything holding a LocalScript or ModuleScript are in no unit: they're always there.

Everything outside a unit is always sent: services and what's in them, Folders, Models themselves (a Model keeps its place when its contents go, so `workspace.Castle` still works), Terrains. A flat Model with thousands of loose parts is one unit: nest big maps in Models or Folders so they stream.

A unit's box is measured in a share of the units each frame (`BOUNDS_ROUNDS`), and the split is rebuilt when the Workspace's or Players' structure version changes (`World::structure_version_of`).

## Joined players (`net::streaming`)

The server's `Replicator` still finds what changed once for everyone; `Streams` then passes each player only their share.

- `StreamingMinRadius` around the character (or a region, below) is sent at once and arrives with the snapshot. Out to `StreamingTargetRadius` arrives nearest first within `INSTANCES_PER_SECOND` (4000), a tick (`TICK_SECONDS`, 0.2) at a time, in messages of at most 256 Instances. Past the unload radius the server tells the device to drop it (`Update::streamed_out`).
- A device keeps a stub Model for a streamed-out Model, and removes loose parts. What's dropped is forgotten by motion. Reference properties (`ObjectValue.Value`, a Model's parts) to a streamed-out Instance read as nil there.
- A character always streams to everyone. What a player owns (a car they drive) stays in their stream.
- Protocol 9 carries `streamed_out`/`streamed_in` and `StreamAround`/`StreamedAround`.

To keep the server cheap, `Replicator::frame` skips Instances whose `Instance::revision` hasn't changed, and walks the tree again only when `World::structure_version` moved.

## The host's own screen (`scene::Streaming`, in `TerrainCache`)

A listen server or single-player device has the whole world but doesn't draw all of it. `Streaming::step` (each frame) works out what's in range of the camera, and `build_draw_list` leaves the rest out:

- **Meshes and textures**: what stops being drawn leaves the GPU after `KEEP_UNDRAWN_FRAMES` and `KEEP_UNDRAWN_SECONDS` (both must pass). Coming back in range, `Renderer::set_background_uploads` uploads them on a thread (`renderer/uploads.rs`): the mesh shows a few frames later, and a texture draws white until it's there. Built-in shapes share one GPU mesh, so they're just left out.
- **Terrain chunks** out of range are dropped; in range, `TerrainCache` builds them on a thread (`ChunkWorker`) and draws them when ready. The loading screen's `prepare` builds them right away, so the first frame has ground. Foliage keeps its own distance cull.
- **Physics** (`Physics::set_far_sleep`): dynamic bodies further than the unload radius from the camera and every joined player's character sleep, and wake when something comes near. Anchored parts and colliders stay.

## Lua

- `Player:RequestStreamAroundAsync(position)` loads what's around `position` and waits until it's there, on either side; then teleport (the region stays loaded for `REGION_SECONDS`, 10). A server Script's call waits for the update to go out; a device's call waits for the server's answer; on a host's own screen it waits for the background uploads and chunks (at most 5 seconds). With streaming off it carries on at once.
- `Instance.StreamingState`: `Enum.StreamingState.StreamedIn`, or `StreamedOut` for what's not on this device. It works on an Instance that's gone (a held reference reads StreamedOut rather than erroring, and so does one that was destroyed).
- `Instance.StreamedIn` fires on a device when that Instance comes back, or a Model's contents arrive. A handler on something streamed out is kept until it returns (`script::network::StreamedOut`).

Blocks: "load around *position* for *player*", "when *thing* streams in", and "*thing* is streamed in".

## Editor

During Play with streaming on, the viewport streams around the camera as the host does and shows both radii as rings at its feet (`paint_streaming_radius`). Editing always shows everything.

**Streaming overlay** (roadmap R16; Viewport menu ▸ Overlays ▸ Streaming, the command palette, or Settings ▸ Viewport ▸ Show streaming; kept between runs): with `StreamingEnabled` on, it shows what streams around the camera while editing as well as in Play, from the same `Partition` the server and the host's screen use (`editor/src/viewport/streaming_overlay.rs`, kept between frames and split again when the Workspace's tree or the World changes). Each unit's box within the unload radius is outlined by where it stands from the camera (`Settings::reach`, `Reach`): green within the minimum radius (there at once), blue within the target (loads), grey up to the unload radius (kept if it was loaded). At most 3,000 boxes are drawn, nearest first. Parts in no unit (a `Persistent` Model, code a device runs) are outlined in orange, and both radii lie at the camera's feet. A legend in the bottom-left corner counts what a player standing at the camera has (`Partition::summary`: units and Instances loaded of all of them, units kept, parts always there) and names the biggest unit when it holds more than `LARGE_UNIT` (1,000) Instances, since it loads and unloads all at once. With streaming off, the legend says so. `RequestStreamAroundAsync` in Play carries on at once. `Play` never hosts, so its joined-player paths run only with Play with several clients.

## What it costs

`Physics` reads only the parts whose `Instance::revision` changed (`sync_changed_parts`), and runs its full sync when the tree, a weld, ragdoll or inventory, or the held parts change. Set against a 60 frames a second budget of 16.7 ms, `bench_50000_parts_streaming` (50,000 parts over 4 km in Models of 8, target radius 512, release build, Radeon RX 9070 XT) measured 6.2 ms physics, 0.7 ms streaming, 3.9 ms network send for one joined player, 6.1 ms building the draw list and 1.2 ms drawing: about 14 ms on the host alone and 18 ms with a joined player, before scripts, characters and UI. Run it with `cargo test -p axcenity-player --release -- --ignored bench_50000 --nocapture --test-threads 1`.

The streaming world (`axcenity-test --make-streaming <folder>`, `docs/benchmarks.md`) is the same map as a project, so every host's `--bench` measures a whole frame with it, and the server's `--bench-clients` counts its joined players. The first runs found per-frame passes over every part in ownership, replication and the partition; they now read only what changed (revisions, the Workspace's moves log, Fx hash maps), and a change outside the Workspace no longer splits the partition again. Server with one joined player, cloud container: 21.9 → 4.2 ms a frame, Network send 16.7 → 2.0 ms ("The streaming world" in `docs/benchmarks.md`).

## Testing it by hand

`examples/hello-streaming` builds a town of about 8,000 parts across 4 km and teleports the player across it after a few seconds. Raise `PARTS` in its script to 50,000 to measure the frame budget: `cargo run --release -p axcenity-player -- examples/hello-streaming`, and F9 for frame times.
