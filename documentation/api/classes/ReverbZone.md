---
description: "A Part the camera walks into: while the camera is inside it, its ReverbType blends onto every spatial Sound over FadeTime seconds, and out again on leaving."
---

# ReverbZone

```luau
Instance.new("ReverbZone")
```

A Part the camera walks into: while the camera is inside it, its ReverbType blends onto every spatial Sound over FadeTime seconds, and out again on leaving. Where zones overlap, the smallest wins. Anchored, mostly see-through and walked through (CanCollide off) by default. Everything a Part has, and ReverbType and FadeTime.

It also has every member of [Instance](Instance.md).

### Properties

#### Anchored

```luau
ReverbZone.Anchored: boolean
```

If true, the part stays where it is. If false, it falls and collides with other parts.

#### Shape

```luau
ReverbZone.Shape: EnumPartType | "Block" | "Ball" | "Cylinder" | "Wedge" | "Capsule" | "CornerWedge"
```

Block, Ball, Cylinder (along X), Wedge, Capsule (along X, with round ends) or CornerWedge (rising to a point over its back right corner).

#### Position

```luau
ReverbZone.Position: Vector3
```

Where the part's center is, in studs. Setting it on an unanchored part teleports it and stops it moving.

#### Orientation

```luau
ReverbZone.Orientation: Vector3
```

Rotation in degrees around X, Y and Z, applied Z first, then X, then Y.

#### Size

```luau
ReverbZone.Size: Vector3
```

The part's size in studs along X, Y and Z.

#### Color

```luau
ReverbZone.Color: Color3
```

The part's color.

#### Material

```luau
ReverbZone.Material: EnumMaterial | "Plastic" | "SmoothPlastic" | "Wood" | "WoodPlanks" | "Marble" | "Granite" | "Concrete" | "Slate" | "Brick" | "Fabric" | "Sand" | "Metal" | "Glass" | "Neon" | "Ice" | "Rubber"
```

Plastic, Metal, Glass (see-through) or Neon (glows). Setting it resets Metalness and Roughness.

#### MaterialVariant

```luau
ReverbZone.MaterialVariant: string
```

A material file in the project, such as "assets/materials/brick.material.json", drawn in place of Material, Metalness, Roughness and Emission; Color tints it and Transparency fades it. A SurfaceAppearance inside the part wins over it. Empty for none.

#### BlueprintTexture

```luau
ReverbZone.BlueprintTexture: boolean
```

If true, a plain Plastic part is drawn in a prototype grid texture while editing, to build by. It never shows in Play or in a built game.

#### Metalness

```luau
ReverbZone.Metalness: number
```

From 0 (not metal) to 1 (metal).

#### Roughness

```luau
ReverbZone.Roughness: number
```

From 0 (mirror-smooth) to 1 (matte).

#### CastShadow

```luau
ReverbZone.CastShadow: boolean
```

If false, the part casts no sun shadow. It's still shaded by other parts' shadows.

#### Transparency

```luau
ReverbZone.Transparency: number
```

From 0 (as solid as its material) to 1 (invisible).

#### CanCollide

```luau
ReverbZone.CanCollide: boolean
```

If false, other parts pass through it. It still fires Touched. An unanchored part that can't collide falls through the floor.

#### WindAffected

```luau
ReverbZone.WindAffected: boolean
```

If true and the part isn't anchored, workspace.GlobalWind pushes it, harder for parts with bigger faces.

#### EmissiveColor

```luau
ReverbZone.EmissiveColor: Color3
```

The color the part glows in when Emission is above 0.

#### Emission

```luau
ReverbZone.Emission: number
```

How brightly the part glows in EmissiveColor: 0 is no glow, 1 as bright as a white surface in full light, higher values bloom.

#### TileSize

```luau
ReverbZone.TileSize: number
```

Studs one copy of a SurfaceAppearance's textures covers. 0 leaves the shape's own coordinates unscaled.

#### CollisionGroup

```luau
ReverbZone.CollisionGroup: string
```

The collision group it's in, `"Default"` at first. Parts in two groups set not to collide (the Collision Groups window, or `PhysicsService:CollisionGroupSetCollidable`) pass through each other and don't touch. A name that isn't registered counts as Default.

#### CanQuery

```luau
ReverbZone.CanQuery: boolean
```

If false, scripts' raycasts, shape casts and overlap queries (`workspace:Raycast`, `GetPartsInPart`, ...) pass it by. It still collides.

#### CanTouch

```luau
ReverbZone.CanTouch: boolean
```

If false, it fires no Touched or TouchEnded, and touching it fires neither on other parts. It still collides.

#### Massless

```luau
ReverbZone.Massless: boolean
```

If true, the part adds no mass to an assembly it shares with other parts. A part on its own (every part, until welds exist) keeps its own mass.

#### CustomPhysicalProperties

```luau
ReverbZone.CustomPhysicalProperties: PhysicalProperties?
```

Its own density, friction, elasticity and their weights (`PhysicalProperties.new(...)`), or nil for its Material's. Read `CurrentPhysicalProperties` for the ones in use.

#### AssemblyLinearVelocity

```luau
ReverbZone.AssemblyLinearVelocity: Vector3
```

How fast the part moves, in studs a second. Physics updates it as the part moves; setting it throws the part. Anchored parts don't move.

#### AssemblyAngularVelocity

```luau
ReverbZone.AssemblyAngularVelocity: Vector3
```

How fast the part spins, in radians a second around each world axis. Physics updates it; setting it spins the part.

#### ReverbType

```luau
ReverbZone.ReverbType: EnumReverbType | "Room" | "StoneRoom" | "Hallway" | "Cave" | "ConcertHall" | "Arena" | "SewerPipe"
```

The room it sounds like (`Enum.ReverbType`): Room (the default), StoneRoom, Hallway, Cave, ConcertHall, Arena or SewerPipe.

#### FadeTime

```luau
ReverbZone.FadeTime: number
```

Seconds the reverb takes to blend in when the camera comes in, and out when it leaves. 1 by default.

#### Locked

```luau
ReverbZone.Locked: boolean
```

If true, the editor's viewport can't select or change it, and the building tools leave it alone. It has no effect in a running game.

#### CFrame

```luau
ReverbZone.CFrame: CFrame
```

Its Position and Orientation as one CFrame. Setting it sets both; it isn't saved itself.

#### AssemblyMass

```luau
ReverbZone.AssemblyMass: number
```

The mass of everything moving with it: the parts welded into its assembly, except Massless ones (a part alone keeps its own mass). Read-only.

#### AssemblyCenterOfMass

```luau
ReverbZone.AssemblyCenterOfMass: Vector3
```

Where its mass is centered, in the world: the middle of a block or ball. Read-only.

#### CurrentPhysicalProperties

```luau
ReverbZone.CurrentPhysicalProperties: PhysicalProperties
```

The physical properties in use: `CustomPhysicalProperties`, or its Material's when that's nil. Read-only.

### Events

#### Touched

```luau
ReverbZone.Touched: Signal
```

Fires when another part starts touching this one, once per new contact (not every frame while they touch). The handler gets the other part: `part.Touched:Connect(function(otherPart) ... end)`.

#### TouchEnded

```luau
ReverbZone.TouchEnded: Signal
```

Fires when another part stops touching this one: once they've been apart for a few physics steps, or at once when one of them can't touch the other any more (`CanTouch` off, or set not to collide). The handler gets the other part.

### Functions

#### UnionAsync

```luau
ReverbZone:UnionAsync(parts: {Instance}, options: {[string]: any}?): UnionOperation
```

Joins the part and `parts` into one solid and returns it as a new UnionOperation (its Parent is nil), with this part's look and physics. NegateOperations among `parts` are cut out. The work runs in the background and the script waits for it. Errors, saying why, if a part isn't a closed solid (a Capsule, or a model with holes). `options.CollisionFidelity` sets the union's.

#### SubtractAsync

```luau
ReverbZone:SubtractAsync(parts: {Instance}, options: {[string]: any}?): UnionOperation
```

Cuts every one of `parts` out of the part and returns what's left as a new UnionOperation (its Parent is nil), with this part's look and physics: a crater where a ball was. The work runs in the background and the script waits for it. Errors, saying why, if a part isn't a closed solid or nothing is left. `options.CollisionFidelity` sets the union's.

#### IntersectAsync

```luau
ReverbZone:IntersectAsync(parts: {Instance}, options: {[string]: any}?): UnionOperation
```

Keeps only where the part and all of `parts` overlap, as a new UnionOperation (its Parent is nil), with this part's look and physics. The work runs in the background and the script waits for it. Errors, saying why, if a part isn't a closed solid or they don't overlap. `options.CollisionFidelity` sets the union's.

#### GetConnectedParts

```luau
ReverbZone:GetConnectedParts(recursive: boolean?): {Instance}
```

The part, then the parts welded to it by an enabled WeldConstraint; with `recursive` true, every part in its assembly.

#### GetRootPart

```luau
ReverbZone:GetRootPart(): Instance
```

The root of its assembly: an anchored part if there is one, then one that isn't Massless, then the largest. A part alone is its own root.

#### GetPivot

```luau
ReverbZone:GetPivot(): CFrame
```

Where it turns and moves from: a part's own CFrame, or for a Model the centre of the box around its parts, unturned.

#### PivotTo

```luau
ReverbZone:PivotTo(target: CFrame): ()
```

Moves and turns it so its pivot is at `target`. A Model's parts all move together and keep their places relative to each other.

#### SetNetworkOwner

```luau
ReverbZone:SetNetworkOwner(player: Instance?): ()
```

Scripts only (Phase 66): `player`'s device simulates this part's whole assembly from now on, so it responds at once for them, and the server sees what it does; nil gives it back to the server. An anchored part has no owner (an error). Scripts only.

#### GetNetworkOwner

```luau
ReverbZone:GetNetworkOwner(): Instance?
```

Scripts only: the Player whose device simulates this part's assembly now, or nil for the server. An anchored part has no owner (an error). Scripts only.

#### SetNetworkOwnershipAuto

```luau
ReverbZone:SetNetworkOwnershipAuto(): ()
```

Scripts only: the engine chooses who simulates this part's assembly: the player driving a VehicleSeat in it, else the nearest player within 40 studs, else the server. Scripts only.

#### GetNetworkOwnershipAuto

```luau
ReverbZone:GetNetworkOwnershipAuto(): boolean
```

Scripts only: true after SetNetworkOwnershipAuto, until SetNetworkOwner. Scripts only.

#### ApplyImpulse

```luau
ReverbZone:ApplyImpulse(impulse: Vector3): ()
```

Pushes the part through its center of mass: its velocity changes by `impulse` divided by its assembly's mass (`AssemblyMass`, which is `GetMass()` for a part alone). Pushes in one frame add up. Anchored parts don't move.

#### ApplyImpulseAtPosition

```luau
ReverbZone:ApplyImpulseAtPosition(impulse: Vector3, position: Vector3): ()
```

Pushes the part at `position` in the world: it speeds up as `ApplyImpulse` would, and an impulse off its center of mass also spins it.

#### ApplyAngularImpulse

```luau
ReverbZone:ApplyAngularImpulse(impulse: Vector3): ()
```

Spins the part: its `AssemblyAngularVelocity` changes by `impulse` (radians a second times inertia) through its inertia.

#### GetMass

```luau
ReverbZone:GetMass(): number
```

Its density times its volume: a 1×1×1 Plastic block weighs 1. MeshParts weigh their collision shape.

