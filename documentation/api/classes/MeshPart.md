---
description: "A solid 3D model imported from a glTF or FBX file, drawn with the file's own materials (or its SurfaceAppearances')."
---

# MeshPart

```luau
Instance.new("MeshPart")
```

A solid 3D model imported from a glTF or FBX file, drawn with the file's own materials (or its SurfaceAppearances'). It collides as the model's convex hull.

It also has every member of [Instance](Instance.md).

### Properties

#### Anchored

```luau
MeshPart.Anchored: boolean
```

If true, the part stays where it is. If false, it falls and collides with other parts.

#### MeshId

```luau
MeshPart.MeshId: string
```

The model's glTF or FBX file, inside the project folder, such as "assets/models/chair.glb"; `#` after it picks objects in the file ("assets/models/props.fbx#Bench"). The editor's Import Model sets it.

#### MaterialVariant

```luau
MeshPart.MaterialVariant: string
```

A material file in the project, such as "assets/materials/brick.material.json", drawn in place of every material of the model, except one a SurfaceAppearance replaces. Empty for none.

#### Position

```luau
MeshPart.Position: Vector3
```

Where the center of the model's box is, in studs. Setting it on an unanchored part teleports it and stops it moving.

#### Orientation

```luau
MeshPart.Orientation: Vector3
```

Rotation in degrees around X, Y and Z, applied Z first, then X, then Y.

#### Size

```luau
MeshPart.Size: Vector3
```

The size of the model's box in studs along X, Y and Z. The model stretches to fill it.

#### CastShadow

```luau
MeshPart.CastShadow: boolean
```

If false, the part casts no sun shadow. It's still shaded by other parts' shadows.

#### Transparency

```luau
MeshPart.Transparency: number
```

From 0 (as solid as its model) to 1 (invisible).

#### CanCollide

```luau
MeshPart.CanCollide: boolean
```

If false, other parts pass through it. It still fires Touched.

#### WindAffected

```luau
MeshPart.WindAffected: boolean
```

If true and the part isn't anchored, workspace.GlobalWind pushes it, harder for parts with bigger faces.

#### CollisionGroup

```luau
MeshPart.CollisionGroup: string
```

The collision group it's in, `"Default"` at first. Parts in two groups set not to collide (the Collision Groups window, or `PhysicsService:CollisionGroupSetCollidable`) pass through each other and don't touch. A name that isn't registered counts as Default.

#### CanQuery

```luau
MeshPart.CanQuery: boolean
```

If false, scripts' raycasts, shape casts and overlap queries (`workspace:Raycast`, `GetPartsInPart`, ...) pass it by. It still collides.

#### CanTouch

```luau
MeshPart.CanTouch: boolean
```

If false, it fires no Touched or TouchEnded, and touching it fires neither on other parts. It still collides.

#### Massless

```luau
MeshPart.Massless: boolean
```

If true, the part adds no mass to an assembly it shares with other parts. A part on its own (every part, until welds exist) keeps its own mass.

#### CustomPhysicalProperties

```luau
MeshPart.CustomPhysicalProperties: PhysicalProperties?
```

Its own density, friction, elasticity and their weights (`PhysicalProperties.new(...)`), or nil for Plastic's. Read `CurrentPhysicalProperties` for the ones in use.

#### AssemblyLinearVelocity

```luau
MeshPart.AssemblyLinearVelocity: Vector3
```

How fast the part moves, in studs a second. Physics updates it as the part moves; setting it throws the part. Anchored parts don't move.

#### AssemblyAngularVelocity

```luau
MeshPart.AssemblyAngularVelocity: Vector3
```

How fast the part spins, in radians a second around each world axis. Physics updates it; setting it spins the part.

#### LinearDrag

```luau
MeshPart.LinearDrag: number
```

Air resistance on its speed: each second its assembly loses about that share of it (1 takes away about 63%). 0 at first. Welded parts use their root part's.

#### AngularDrag

```luau
MeshPart.AngularDrag: number
```

Air resistance on its spin, like LinearDrag. 0.5 at first, so a spinning part slows by itself; 0 spins on.

#### RollingFriction

```luau
MeshPart.RollingFriction: number
```

Extra spin slowing while its assembly touches anything, so a ball rolls to a stop on the ground but spins on in the air. 0 at first. Welded parts use their root part's.

#### Locked

```luau
MeshPart.Locked: boolean
```

If true, the editor's viewport can't select or change it, and the building tools leave it alone. It has no effect in a running game.

#### CFrame

```luau
MeshPart.CFrame: CFrame
```

Its Position and Orientation as one CFrame. Setting it sets both; it isn't saved itself.

#### AssemblyMass

```luau
MeshPart.AssemblyMass: number
```

The mass of everything moving with it: the parts welded into its assembly, except Massless ones (a part alone keeps its own mass). Read-only.

#### AssemblyCenterOfMass

```luau
MeshPart.AssemblyCenterOfMass: Vector3
```

Where its mass is centered, in the world: the middle of a block or ball. Read-only.

#### AssemblyLinearMomentum

```luau
MeshPart.AssemblyLinearMomentum: Vector3
```

Its assembly's mass times its `AssemblyLinearVelocity`: how hard it is to stop. Read-only.

#### CurrentPhysicalProperties

```luau
MeshPart.CurrentPhysicalProperties: PhysicalProperties
```

The physical properties in use: `CustomPhysicalProperties`, or its Material's when that's nil. Read-only.

### Events

#### Touched

```luau
MeshPart.Touched: Signal
```

Fires when another part starts touching this one, once per new contact (not every frame while they touch). The handler gets the other part: `part.Touched:Connect(function(otherPart) ... end)`.

#### TouchEnded

```luau
MeshPart.TouchEnded: Signal
```

Fires when another part stops touching this one: once they've been apart for a few physics steps, or at once when one of them can't touch the other any more (`CanTouch` off, or set not to collide). The handler gets the other part.

### Functions

#### UnionAsync

```luau
MeshPart:UnionAsync(parts: {Instance}, options: {[string]: any}?): UnionOperation
```

Joins the part and `parts` into one solid and returns it as a new UnionOperation (its Parent is nil), with this part's look and physics. NegateOperations among `parts` are cut out. The work runs in the background and the script waits for it. Errors, saying why, if a part isn't a closed solid (a Capsule, or a model with holes). `options.CollisionFidelity` sets the union's.

#### SubtractAsync

```luau
MeshPart:SubtractAsync(parts: {Instance}, options: {[string]: any}?): UnionOperation
```

Cuts every one of `parts` out of the part and returns what's left as a new UnionOperation (its Parent is nil), with this part's look and physics: a crater where a ball was. The work runs in the background and the script waits for it. Errors, saying why, if a part isn't a closed solid or nothing is left. `options.CollisionFidelity` sets the union's.

#### IntersectAsync

```luau
MeshPart:IntersectAsync(parts: {Instance}, options: {[string]: any}?): UnionOperation
```

Keeps only where the part and all of `parts` overlap, as a new UnionOperation (its Parent is nil), with this part's look and physics. The work runs in the background and the script waits for it. Errors, saying why, if a part isn't a closed solid or they don't overlap. `options.CollisionFidelity` sets the union's.

#### GetConnectedParts

```luau
MeshPart:GetConnectedParts(recursive: boolean?): {Instance}
```

The part, then the parts welded to it by an enabled WeldConstraint; with `recursive` true, every part in its assembly.

#### GetRootPart

```luau
MeshPart:GetRootPart(): Instance
```

The root of its assembly: an anchored part if there is one, then one that isn't Massless, then the largest. A part alone is its own root.

#### GetPivot

```luau
MeshPart:GetPivot(): CFrame
```

Where it turns and moves from: a part's own CFrame, or for a Model the centre of the box around its parts, unturned.

#### PivotTo

```luau
MeshPart:PivotTo(target: CFrame): ()
```

Moves and turns it so its pivot is at `target`. A Model's parts all move together and keep their places relative to each other.

#### SetNetworkOwner

```luau
MeshPart:SetNetworkOwner(player: Instance?): ()
```

Scripts only (Phase 66): `player`'s device simulates this part's whole assembly from now on, so it responds at once for them, and the server sees what it does; nil gives it back to the server. An anchored part has no owner (an error). Scripts only.

#### GetNetworkOwner

```luau
MeshPart:GetNetworkOwner(): Instance?
```

Scripts only: the Player whose device simulates this part's assembly now, or nil for the server. An anchored part has no owner (an error). Scripts only.

#### SetNetworkOwnershipAuto

```luau
MeshPart:SetNetworkOwnershipAuto(): ()
```

Scripts only: the engine chooses who simulates this part's assembly: the player driving a VehicleSeat in it, else the nearest player within 40 studs, else the server. Scripts only.

#### GetNetworkOwnershipAuto

```luau
MeshPart:GetNetworkOwnershipAuto(): boolean
```

Scripts only: true after SetNetworkOwnershipAuto, until SetNetworkOwner. Scripts only.

#### ApplyImpulse

```luau
MeshPart:ApplyImpulse(impulse: Vector3): ()
```

Pushes the part through its center of mass: its velocity changes by `impulse` divided by its assembly's mass (`AssemblyMass`, which is `GetMass()` for a part alone). Pushes in one frame add up. Anchored parts don't move.

#### ApplyImpulseAtPosition

```luau
MeshPart:ApplyImpulseAtPosition(impulse: Vector3, position: Vector3): ()
```

Pushes the part at `position` in the world: it speeds up as `ApplyImpulse` would, and an impulse off its center of mass also spins it.

#### ApplyAngularImpulse

```luau
MeshPart:ApplyAngularImpulse(impulse: Vector3): ()
```

Spins the part: its `AssemblyAngularVelocity` changes by `impulse` (radians a second times inertia) through its inertia.

#### ApplyForce

```luau
MeshPart:ApplyForce(force: Vector3, seconds: number?): ()
```

Pushes the part's assembly with `force` for `seconds` (one physics step, 1/60 s, if left out): its `AssemblyLinearVelocity` changes by force times time over its mass. To push steadily, call it every Heartbeat with that frame's time, or use a VectorForce.

#### ApplyTorque

```luau
MeshPart:ApplyTorque(torque: Vector3, seconds: number?): ()
```

Turns the part's assembly with `torque` for `seconds` (one physics step if left out): its `AssemblyAngularVelocity` changes by torque times time through its inertia. To turn steadily, call it every Heartbeat with that frame's time, or use a Torque.

#### GetMass

```luau
MeshPart:GetMass(): number
```

Its density times its volume: a 1×1×1 Plastic block weighs 1. MeshParts weigh their collision shape.

