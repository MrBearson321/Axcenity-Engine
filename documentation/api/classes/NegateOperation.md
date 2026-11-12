---
description: "A Part made negative by Negate (Model ▸ Solid Modeling): shown pink and see-through, and when it's unioned with other parts it cuts itself out of them, as a doorway out of a wall."
---

# NegateOperation

```luau
Instance.new("NegateOperation")
```

A Part made negative by Negate (Model ▸ Solid Modeling): shown pink and see-through, and when it's unioned with other parts it cuts itself out of them, as a doorway out of a wall. Negate it again to turn it back into a Part. Everything a Part has.

It also has every member of [Instance](Instance.md).

### Properties

#### Anchored

```luau
NegateOperation.Anchored: boolean
```

If true, the part stays where it is. If false, it falls and collides with other parts.

#### Shape

```luau
NegateOperation.Shape: EnumPartType | "Block" | "Ball" | "Cylinder" | "Wedge" | "Capsule" | "CornerWedge"
```

Block, Ball, Cylinder (along X), Wedge, Capsule (along X, with round ends) or CornerWedge (rising to a point over its back right corner).

#### Position

```luau
NegateOperation.Position: Vector3
```

Where the part's center is, in studs. Setting it on an unanchored part teleports it and stops it moving.

#### Orientation

```luau
NegateOperation.Orientation: Vector3
```

Rotation in degrees around X, Y and Z, applied Z first, then X, then Y.

#### Size

```luau
NegateOperation.Size: Vector3
```

The part's size in studs along X, Y and Z.

#### Color

```luau
NegateOperation.Color: Color3
```

The part's color.

#### Material

```luau
NegateOperation.Material: EnumMaterial | "Plastic" | "SmoothPlastic" | "Wood" | "WoodPlanks" | "Marble" | "Granite" | "Concrete" | "Slate" | "Brick" | "Fabric" | "Sand" | "Metal" | "Glass" | "Neon" | "Ice" | "Rubber"
```

Plastic, Metal, Glass (see-through) or Neon (glows). Setting it resets Metalness and Roughness.

#### MaterialVariant

```luau
NegateOperation.MaterialVariant: string
```

A material file in the project, such as "assets/materials/brick.material.json", drawn in place of Material, Metalness, Roughness and Emission; Color tints it and Transparency fades it. A SurfaceAppearance inside the part wins over it. Empty for none.

#### BlueprintTexture

```luau
NegateOperation.BlueprintTexture: boolean
```

If true, a plain Plastic part is drawn in a prototype grid texture while editing, to build by. It never shows in Play or in a built game.

#### Metalness

```luau
NegateOperation.Metalness: number
```

From 0 (not metal) to 1 (metal).

#### Roughness

```luau
NegateOperation.Roughness: number
```

From 0 (mirror-smooth) to 1 (matte).

#### CastShadow

```luau
NegateOperation.CastShadow: boolean
```

If false, the part casts no sun shadow. It's still shaded by other parts' shadows.

#### Transparency

```luau
NegateOperation.Transparency: number
```

From 0 (as solid as its material) to 1 (invisible).

#### CanCollide

```luau
NegateOperation.CanCollide: boolean
```

If false, other parts pass through it. It still fires Touched. An unanchored part that can't collide falls through the floor.

#### WindAffected

```luau
NegateOperation.WindAffected: boolean
```

If true and the part isn't anchored, workspace.GlobalWind pushes it, harder for parts with bigger faces.

#### EmissiveColor

```luau
NegateOperation.EmissiveColor: Color3
```

The color the part glows in when Emission is above 0.

#### Emission

```luau
NegateOperation.Emission: number
```

How brightly the part glows in EmissiveColor: 0 is no glow, 1 as bright as a white surface in full light, higher values bloom.

#### TileSize

```luau
NegateOperation.TileSize: number
```

Studs one copy of a SurfaceAppearance's textures covers. 0 leaves the shape's own coordinates unscaled.

#### CollisionGroup

```luau
NegateOperation.CollisionGroup: string
```

The collision group it's in, `"Default"` at first. Parts in two groups set not to collide (the Collision Groups window, or `PhysicsService:CollisionGroupSetCollidable`) pass through each other and don't touch. A name that isn't registered counts as Default.

#### CanQuery

```luau
NegateOperation.CanQuery: boolean
```

If false, scripts' raycasts, shape casts and overlap queries (`workspace:Raycast`, `GetPartsInPart`, ...) pass it by. It still collides.

#### CanTouch

```luau
NegateOperation.CanTouch: boolean
```

If false, it fires no Touched or TouchEnded, and touching it fires neither on other parts. It still collides.

#### Massless

```luau
NegateOperation.Massless: boolean
```

If true, the part adds no mass to an assembly it shares with other parts. A part on its own (every part, until welds exist) keeps its own mass.

#### CustomPhysicalProperties

```luau
NegateOperation.CustomPhysicalProperties: PhysicalProperties?
```

Its own density, friction, elasticity and their weights (`PhysicalProperties.new(...)`), or nil for its Material's. Read `CurrentPhysicalProperties` for the ones in use.

#### AssemblyLinearVelocity

```luau
NegateOperation.AssemblyLinearVelocity: Vector3
```

How fast the part moves, in studs a second. Physics updates it as the part moves; setting it throws the part. Anchored parts don't move.

#### AssemblyAngularVelocity

```luau
NegateOperation.AssemblyAngularVelocity: Vector3
```

How fast the part spins, in radians a second around each world axis. Physics updates it; setting it spins the part.

#### LinearDrag

```luau
NegateOperation.LinearDrag: number
```

Air resistance on its speed: each second its assembly loses about that share of it (1 takes away about 63%). 0 at first. Welded parts use their root part's.

#### AngularDrag

```luau
NegateOperation.AngularDrag: number
```

Air resistance on its spin, like LinearDrag. 0.5 at first, so a spinning part slows by itself; 0 spins on.

#### RollingFriction

```luau
NegateOperation.RollingFriction: number
```

Extra spin slowing while its assembly touches anything, so a ball rolls to a stop on the ground but spins on in the air. 0 at first. Welded parts use their root part's.

#### Locked

```luau
NegateOperation.Locked: boolean
```

If true, the editor's viewport can't select or change it, and the building tools leave it alone. It has no effect in a running game.

#### CFrame

```luau
NegateOperation.CFrame: CFrame
```

Its Position and Orientation as one CFrame. Setting it sets both; it isn't saved itself.

#### AssemblyMass

```luau
NegateOperation.AssemblyMass: number
```

The mass of everything moving with it: the parts welded into its assembly, except Massless ones (a part alone keeps its own mass). Read-only.

#### AssemblyCenterOfMass

```luau
NegateOperation.AssemblyCenterOfMass: Vector3
```

Where its mass is centered, in the world: the middle of a block or ball. Read-only.

#### AssemblyLinearMomentum

```luau
NegateOperation.AssemblyLinearMomentum: Vector3
```

Its assembly's mass times its `AssemblyLinearVelocity`: how hard it is to stop. Read-only.

#### CurrentPhysicalProperties

```luau
NegateOperation.CurrentPhysicalProperties: PhysicalProperties
```

The physical properties in use: `CustomPhysicalProperties`, or its Material's when that's nil. Read-only.

### Events

#### Touched

```luau
NegateOperation.Touched: Signal
```

Fires when another part starts touching this one, once per new contact (not every frame while they touch). The handler gets the other part: `part.Touched:Connect(function(otherPart) ... end)`.

#### TouchEnded

```luau
NegateOperation.TouchEnded: Signal
```

Fires when another part stops touching this one: once they've been apart for a few physics steps, or at once when one of them can't touch the other any more (`CanTouch` off, or set not to collide). The handler gets the other part.

### Functions

#### UnionAsync

```luau
NegateOperation:UnionAsync(parts: {Instance}, options: {[string]: any}?): UnionOperation
```

Joins the part and `parts` into one solid and returns it as a new UnionOperation (its Parent is nil), with this part's look and physics. NegateOperations among `parts` are cut out. The work runs in the background and the script waits for it. Errors, saying why, if a part isn't a closed solid (a Capsule, or a model with holes). `options.CollisionFidelity` sets the union's.

#### SubtractAsync

```luau
NegateOperation:SubtractAsync(parts: {Instance}, options: {[string]: any}?): UnionOperation
```

Cuts every one of `parts` out of the part and returns what's left as a new UnionOperation (its Parent is nil), with this part's look and physics: a crater where a ball was. The work runs in the background and the script waits for it. Errors, saying why, if a part isn't a closed solid or nothing is left. `options.CollisionFidelity` sets the union's.

#### IntersectAsync

```luau
NegateOperation:IntersectAsync(parts: {Instance}, options: {[string]: any}?): UnionOperation
```

Keeps only where the part and all of `parts` overlap, as a new UnionOperation (its Parent is nil), with this part's look and physics. The work runs in the background and the script waits for it. Errors, saying why, if a part isn't a closed solid or they don't overlap. `options.CollisionFidelity` sets the union's.

#### GetConnectedParts

```luau
NegateOperation:GetConnectedParts(recursive: boolean?): {Instance}
```

The part, then the parts welded to it by an enabled WeldConstraint; with `recursive` true, every part in its assembly.

#### GetRootPart

```luau
NegateOperation:GetRootPart(): Instance
```

The root of its assembly: an anchored part if there is one, then one that isn't Massless, then the largest. A part alone is its own root.

#### GetPivot

```luau
NegateOperation:GetPivot(): CFrame
```

Where it turns and moves from: a part's own CFrame, or for a Model the centre of the box around its parts, unturned.

#### PivotTo

```luau
NegateOperation:PivotTo(target: CFrame): ()
```

Moves and turns it so its pivot is at `target`. A Model's parts all move together and keep their places relative to each other.

#### SetNetworkOwner

```luau
NegateOperation:SetNetworkOwner(player: Instance?): ()
```

Scripts only (Phase 66): `player`'s device simulates this part's whole assembly from now on, so it responds at once for them, and the server sees what it does; nil gives it back to the server. An anchored part has no owner (an error). Scripts only.

#### GetNetworkOwner

```luau
NegateOperation:GetNetworkOwner(): Instance?
```

Scripts only: the Player whose device simulates this part's assembly now, or nil for the server. An anchored part has no owner (an error). Scripts only.

#### SetNetworkOwnershipAuto

```luau
NegateOperation:SetNetworkOwnershipAuto(): ()
```

Scripts only: the engine chooses who simulates this part's assembly: the player driving a VehicleSeat in it, else the nearest player within 40 studs, else the server. Scripts only.

#### GetNetworkOwnershipAuto

```luau
NegateOperation:GetNetworkOwnershipAuto(): boolean
```

Scripts only: true after SetNetworkOwnershipAuto, until SetNetworkOwner. Scripts only.

#### ApplyImpulse

```luau
NegateOperation:ApplyImpulse(impulse: Vector3): ()
```

Pushes the part through its center of mass: its velocity changes by `impulse` divided by its assembly's mass (`AssemblyMass`, which is `GetMass()` for a part alone). Pushes in one frame add up. Anchored parts don't move.

#### ApplyImpulseAtPosition

```luau
NegateOperation:ApplyImpulseAtPosition(impulse: Vector3, position: Vector3): ()
```

Pushes the part at `position` in the world: it speeds up as `ApplyImpulse` would, and an impulse off its center of mass also spins it.

#### ApplyAngularImpulse

```luau
NegateOperation:ApplyAngularImpulse(impulse: Vector3): ()
```

Spins the part: its `AssemblyAngularVelocity` changes by `impulse` (radians a second times inertia) through its inertia.

#### ApplyForce

```luau
NegateOperation:ApplyForce(force: Vector3, seconds: number?): ()
```

Pushes the part's assembly with `force` for `seconds` (one physics step, 1/60 s, if left out): its `AssemblyLinearVelocity` changes by force times time over its mass. To push steadily, call it every Heartbeat with that frame's time, or use a VectorForce.

#### ApplyTorque

```luau
NegateOperation:ApplyTorque(torque: Vector3, seconds: number?): ()
```

Turns the part's assembly with `torque` for `seconds` (one physics step if left out): its `AssemblyAngularVelocity` changes by torque times time through its inertia. To turn steadily, call it every Heartbeat with that frame's time, or use a Torque.

#### GetMass

```luau
NegateOperation:GetMass(): number
```

Its density times its volume: a 1×1×1 Plastic block weighs 1. MeshParts weigh their collision shape.

