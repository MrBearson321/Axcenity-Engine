---
description: The classes you'll use most, grouped by what they're for, with the properties and events that matter. The API Reference lists every member.
icon: cube
samples: [hello-part, hello-character, hello-agent, hello-splines]
---

# Common classes

Click a class name to open its API Reference entry, with every property, method and event.

## Parts and models

| Class | What it is |
|---|---|
| `Part` | The basic building block: Block, Ball, Cylinder, Wedge or Capsule (`Part.Shape`). Key properties: `Part.Position`, `Part.Size`, `Part.Color`, `Part.Material`, `Part.Anchored`, `Part.CanCollide`, `Part.Transparency`. Event: `Part.Touched`. |
| `MeshPart` | An imported model's mesh, with its own materials or a `SurfaceAppearance`. |
| `UnionOperation`, `NegateOperation` | Parts combined (Union) or cut away (Negate). |
| `Model` | A group of parts that moves as one; `Model:PivotTo()` moves it. |
| `Folder` | A group for tidiness only. |
| `SpawnLocation` | Where players appear. |
| `Seat`, `VehicleSeat` | Places to sit; a VehicleSeat drives a vehicle. |
| `Terrain`, `TerrainLayer`, `ScatterRule` | The landscape, its painted layers and the rules that scatter plants and props over it. |
| `Decal`, `ProjectedDecal` | Pictures on a surface, or projected onto whatever is in a box. |

## Lights and effects

| Class | What it is |
|---|---|
| `PointLight`, `SpotLight`, `RectLight` | Lights inside a part. |
| `Lighting` | The sun, sky, fog and exposure; holds the effects below. |
| `Atmosphere`, `Clouds`, `Weather` | Haze, clouds, rain and snow. |
| `BloomEffect`, `ColorCorrectionEffect`, `DepthOfFieldEffect`, `VignetteEffect`, `ExposureEffect`, `FogEffect` and others | Post-processing. |
| `GraphicsVolume` | Changes lighting and effects inside a box. |
| `ReflectionProbe`, `LightProbeVolume` | Baked reflections and indirect light. |
| `ParticleEmitter` | Smoke, fire, sparks and dust. |
| `OutlineEffect` | An outline around an object. |

## Physics

| Class | What it is |
|---|---|
| `Attachment` | A point on a part that joints connect to. |
| `WeldConstraint` | Holds two parts together rigidly. |
| `HingeConstraint`, `BallSocketConstraint`, `PrismaticConstraint`, `CylindricalConstraint`, `SpringConstraint`, `RopeConstraint`, `RodConstraint` | Joints. |
| `LinearVelocity`, `AngularVelocity`, `VectorForce`, `Torque`, `AlignPosition`, `AlignOrientation` | Movers. |
| `VehicleWheel` | A driven or steered wheel. |
| `Ragdoll` | Makes a character go limp. |
| `Fracture` | Lets a part shatter. |
| `DamageZone` | A part that hurts what touches it. |

## Characters and gameplay

| Class | What it is |
|---|---|
| `ThirdPersonController`, `FirstPersonController` | Walk, run, jump and the camera, with no script. |
| `Humanoid` | A character's movement and states; `Humanoid:MoveTo()`. |
| `Health` | Hit points, with damage and death events. |
| `Tool`, `Inventory` | Things a player holds, and the hotbar. |
| `Ability`, `Effect` | Actions with cooldowns, and timed changes such as speed boosts. |
| `ProximityPrompt`, `ClickDetector` | "Press E to open" prompts and clickable parts. |
| `Checkpoint` | A place to respawn. |
| `Camera` | The view; `Workspace.CurrentCamera` in LocalScripts. |
| `Player`, `Team` | A person in the game and their team. |

## AI and paths

| Class | What it is |
|---|---|
| `NavigationAgent` | Walks to a destination around obstacles, sees and hears players; `NavigationAgent.PlayerSensed`. |
| `Spline`, `SplinePoint` | A smooth path. |
| `SplineFollower` | Moves something along a spline. |
| `SplineMesh`, `SplineScatter` | A mesh bent along a spline (roads, rails), or objects scattered along it (fences, trees). |

## Animation and sound

| Class | What it is |
|---|---|
| `Animator`, `Animation`, `AnimationTrack` | State machines and clips. See [Animation](animation.md). |
| `IKControl` | Points a limb at a target. |
| `Sequence` | A cutscene. |
| `Sound`, `SoundGroup`, `ReverbZone` | Sounds, buses and echoing places. See [Audio](audio.md). |

## UI

`ScreenGui`, `Frame`, `TextLabel`, `TextButton`, `ImageLabel`, `TextBox`, `ScrollingFrame`, `PageLayout`, `Table`, `Slider`, `Toggle`, `Dropdown`, `ProgressBar` and `BillboardGui`. See [UI](ui.md).

## Scripts and messages

| Class | What it is |
|---|---|
| `Script`, `LocalScript`, `ModuleScript` | Code. See [Scripting reference](scripting.md). |
| `RemoteEvent`, `RemoteFunction` | Messages between the server and players. |
| `BindableEvent`, `BindableFunction` | Messages between scripts on the same side. |
| `IntValue`, `NumberValue`, `StringValue`, `BoolValue`, `ObjectValue` and the other value objects | One value stored in the tree, with a `Changed` event. |
