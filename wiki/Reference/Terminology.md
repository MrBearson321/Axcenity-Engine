---
title: Terminology
type: reference
status: current
verified: 2026-11-10
tags: [reference, glossary]
---

# Terminology

| Term | Meaning |
|---|---|
| **Instance** | Any object in a game: a class, a name, a parent, children, properties, attributes, tags, events. Stored in the `World`'s arena |
| **`InstanceId`** | Generational ID (index + generation) of an Instance; stale IDs resolve to nothing. Same number on every device in multiplayer |
| **World** | The owned tree of Instances (`core::World`); `game` is its root (DataModel) |
| **Service** | A child of `game` such as Workspace, Lighting, Players, ReplicatedStorage, ServerScriptService |
| **Workspace** | The 3D world service; what's drawn and simulated |
| **Stud** | The world unit (Roblox-familiar); gravity 196.2 studs/s² |
| **Part / MeshPart / BasePart** | Built-in shape / imported model / either |
| **Assembly** | Welded parts simulated as one rapier body |
| **Host** | A program that runs the frame: player, editor Play, dedicated server, test runner |
| **`Sim`, `Schedule`, `HostIo`** | The simulation state, the ordered frame steps, and what differs per host (`crates/sim`) |
| **Step / Stage** | One named frame function / its RunService-style stage (Input, PreSimulation, Simulation, …) |
| **Side** | Server side (Scripts) or client side (LocalScripts): two Luau VMs in one `ScriptRuntime` |
| **Listen server** | A device that is both server and a player (single-player, `--host`, editor Play) |
| **Replica** | A joined device's copy of the server's World at the same IDs |
| **Draw list** | The plain data `scene` builds and `render` draws |
| **Retained draw items** | Draw items kept per Instance between frames and rebuilt by revision (M4) |
| **Revision / structure version / motion revision** | Per-Instance change counter / per-tree and per-service reparent counters / counter of motion-only changes |
| **Cooked copy** | An offline-processed asset in `.axcenity/imported/` (`.axtex`, `.axlod`, `.axmodel`) |
| **Asset ID** | A stable `ax://…` ID in an asset's `.import.json` sidecar |
| **Look vs cost** | World-owned appearance settings vs machine-owned graphics settings |
| **S10k** | The standard benchmark scene: 10,000 parts, 200 scripts, 50 sounds, 20 characters, 10 agents |
| **Pinning** | Running a benchmark on the 9950X3D's V-cache CCD (CPUs 0–15) |
| **Topic file** | One short `docs/` file per kind of change |
| **Roadmap IDs** | I1–I11 (immediate), M1–M14 (medium), L1–L8 (long) from the modernization report; U1–U20 usability items; DS1–DS27 design-system problems and V1–V26 improvements |
| **Phase / Milestone** | Units of the plans in `plans/` (unified Phases 1–93; graphics Phases 1–38) |
| **ADR** | Architecture decision record in this wiki |
| **Block script / Graph view / UI Event Graph** | Visual scripting formats; all compile to Lua |
| **Fracture, UnionOperation** | Pre-generated breakable pieces; a boolean-combined solid |
| **Streaming unit** | The Instances that load and unload together under `StreamingEnabled` |
