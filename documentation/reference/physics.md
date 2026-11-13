---
description: How physics steps, assemblies and welds, collision shapes, materials and physical properties, touches, joints and movers, spatial queries and network ownership.
icon: atom
samples: [hello-physics, hello-joints, hello-movers, hello-vehicle, hello-destruction]
---

# Physics

Physics uses **rapier**, a Rust physics engine, on its own pool of threads. Learn the basics first in [Physics and collisions](../learn/physics.md); this page is the detail.

## Stepping

- Physics steps at a **fixed 60 Hz**, whatever the frame rate: a fast computer runs no step on some frames, a slow one runs several (at most 10 a frame). Parts are drawn smoothly between steps.
- Results don't depend on the frame rate. They aren't guaranteed to be identical across different computers.
- Bodies that stop moving go to sleep and cost almost nothing until something touches them.

## Bodies and assemblies

- An **Anchored** part is fixed. Everything else is a dynamic body.
- Parts joined by **WeldConstraint**s form one **assembly**: one rigid body with every part's collider. `Part:GetRootPart()` and `Part:GetConnectedParts()` find it; `Part.AssemblyMass` and `Part.AssemblyLinearVelocity` describe it.
- **Massless** parts add nothing to their assembly's mass.
- Characters with a controller aren't bodies: the controller moves them, and they push and stand on bodies.

## Collision shapes

| Object | Collides as |
|---|---|
| Part | Its exact shape: block, ball, cylinder, wedge or capsule. |
| MeshPart | Set by `MeshPart.CollisionFidelity`. `Default`: the model's own collision pieces (its `_COL` objects) when it has any, otherwise the convex hull of its mesh (hollows are filled), or its own triangles when it's small and already convex. Or a box, a hull, or `PreciseConvexDecomposition` (several hulls, keeps hollows). |
| UnionOperation | Set by `UnionOperation.CollisionFidelity`: a box, a hull (the default), or `PreciseConvexDecomposition` (several hulls, keeps hollows). |
| Terrain | A heightfield. |

**Viewport > Overlays > Colliders** draws the shapes.

## Materials and physical properties

Each **Material** sets a density, friction and elasticity. `Part.CustomPhysicalProperties` overrides them with `PhysicalProperties.new()`; `Part.CurrentPhysicalProperties` is what's in effect. `Workspace.Gravity` is 196.2 studs/s² to start, and `Workspace.GlobalWind` pushes parts with `Part.WindAffected` on.

## Collisions and touches

- **CanCollide** off: other parts pass through, but touches still fire. **CanTouch** off: no touch events. **CanQuery** off: rays and area queries ignore it.
- `Part.Touched` fires when a pair starts touching after a step, and `Part.TouchEnded` once they've been apart for three steps. Pairs already resting together when the game starts don't fire. Two anchored parts never touch.
- **Collision groups** (up to 32, set in **Collision Groups…** and with `PhysicsService:CollisionGroupSetCollidable()`) choose which parts collide.

## Forces and motion from scripts

| To | Use |
|---|---|
| Push once | `Part:ApplyImpulse()`, `Part:ApplyImpulseAtPosition()`, `Part:ApplyAngularImpulse()` |
| Set a speed | `Part.AssemblyLinearVelocity`, `Part.AssemblyAngularVelocity` |
| Push or turn steadily | Movers: `LinearVelocity`, `AngularVelocity`, `VectorForce`, `Torque`, `AlignPosition`, `AlignOrientation` |
| Teleport | Set `Part.Position` or `Part.CFrame` (this also stops it) |

## Joints

Joints connect two **Attachments**: `HingeConstraint` (with a motor or a servo), `BallSocketConstraint`, `PrismaticConstraint`, `CylindricalConstraint`, `SpringConstraint`, `RopeConstraint` and `RodConstraint`. Limits, motors and break forces are properties of each.

## Spatial queries

`Workspace:Raycast()`, `Workspace:Spherecast()`, `Workspace:Blockcast()` and `Workspace:Shapecast()` sweep along a direction and return a **RaycastResult**; `Workspace:GetPartBoundsInBox()`, `Workspace:GetPartBoundsInRadius()` and `Workspace:GetPartsInPart()` find parts in an area. Filter both with **RaycastParams** or **OverlapParams**.

```lua
local params = RaycastParams.new()
params.FilterDescendantsInstances = { workspace.Player1 }
local hit = workspace:Raycast(Vector3.new(0, 50, 0), Vector3.new(0, -100, 0), params)
if hit then
	print(hit.Instance.Name, hit.Position, hit.Distance)
end
```

## Multiplayer

In a multiplayer game the server simulates physics. `Part:SetNetworkOwner()` lets one player's device simulate an assembly (good for something they drive); the server checks how far it may move.

> **Known limits.** MeshParts without collision pieces collide as hulls unless set to `PreciseConvexDecomposition`. Setting `Position` stops a part. A ragdoll can bend past its joint limits for a moment on a hard landing.

More: [Physics in the engineering wiki](https://github.com/MrBearson321/axcenity-engine/blob/main/wiki/Engine-Systems/Physics.md).
