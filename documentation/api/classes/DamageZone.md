---
description: "A Part that hurts: every Health whose Model has a part inside it loses DamagePerSecond a second, with the zone as the source."
---

# DamageZone

```luau
Instance.new("DamageZone")
```

A Part that hurts: every Health whose Model has a part inside it loses DamagePerSecond a second, with the zone as the source. Anchored, half see-through and walked through (CanCollide off) by default. Everything a Part has, and DamagePerSecond.

It also has every member of [Instance](Instance.md).

### Properties

#### Anchored

```luau
DamageZone.Anchored: boolean
```

If true, the part stays where it is. If false, it falls and collides with other parts.

#### Shape

```luau
DamageZone.Shape: EnumPartType | "Block" | "Ball" | "Cylinder" | "Wedge" | "Capsule" | "CornerWedge"
```

Block, Ball, Cylinder (along X), Wedge, Capsule (along X, with round ends) or CornerWedge (rising to a point over its back right corner).

#### Position

```luau
DamageZone.Position: Vector3
```

Where the part's center is, in studs. Setting it on an unanchored part teleports it and stops it moving.

#### Orientation

```luau
DamageZone.Orientation: Vector3
```

Rotation in degrees around X, Y and Z, applied Z first, then X, then Y.

#### Size

```luau
DamageZone.Size: Vector3
```

The part's size in studs along X, Y and Z.

#### Color

```luau
DamageZone.Color: Color3
```

The part's color.

#### Material

```luau
DamageZone.Material: EnumMaterial | "Plastic" | "SmoothPlastic" | "Wood" | "WoodPlanks" | "Marble" | "Granite" | "Concrete" | "Slate" | "Brick" | "Fabric" | "Sand" | "Metal" | "Glass" | "Neon" | "Ice" | "Rubber"
```

Plastic, Metal, Glass (see-through) or Neon (glows). Setting it resets Metalness and Roughness.

#### MaterialVariant

```luau
DamageZone.MaterialVariant: string
```

A material file in the project, such as "assets/materials/brick.material.json", drawn in place of Material, Metalness, Roughness and Emission; Color tints it and Transparency fades it. A SurfaceAppearance inside the part wins over it. Empty for none.

#### BlueprintTexture

```luau
DamageZone.BlueprintTexture: boolean
```

If true, a plain Plastic part is drawn in a prototype grid texture while editing, to build by. It never shows in Play or in a built game.

#### Metalness

```luau
DamageZone.Metalness: number
```

From 0 (not metal) to 1 (metal).

#### Roughness

```luau
DamageZone.Roughness: number
```

From 0 (mirror-smooth) to 1 (matte).

#### CastShadow

```luau
DamageZone.CastShadow: boolean
```

If false, the part casts no sun shadow. It's still shaded by other parts' shadows.

#### Transparency

```luau
DamageZone.Transparency: number
```

From 0 (as solid as its material) to 1 (invisible).

#### CanCollide

```luau
DamageZone.CanCollide: boolean
```

If false, other parts pass through it. It still fires Touched. An unanchored part that can't collide falls through the floor.

#### WindAffected

```luau
DamageZone.WindAffected: boolean
```

If true and the part isn't anchored, workspace.GlobalWind pushes it, harder for parts with bigger faces.

#### EmissiveColor

```luau
DamageZone.EmissiveColor: Color3
```

The color the part glows in when Emission is above 0.

#### Emission

```luau
DamageZone.Emission: number
```

How brightly the part glows in EmissiveColor: 0 is no glow, 1 as bright as a white surface in full light, higher values bloom.

#### TileSize

```luau
DamageZone.TileSize: number
```

Studs one copy of a SurfaceAppearance's textures covers. 0 leaves the shape's own coordinates unscaled.

#### CollisionGroup

```luau
DamageZone.CollisionGroup: string
```

The collision group it's in, `"Default"` at first. Parts in two groups set not to collide (the Collision Groups window, or `PhysicsService:CollisionGroupSetCollidable`) pass through each other and don't touch. A name that isn't registered counts as Default.

#### CanQuery

```luau
DamageZone.CanQuery: boolean
```

If false, scripts' raycasts, shape casts and overlap queries (`workspace:Raycast`, `GetPartsInPart`, ...) pass it by. It still collides.

#### CanTouch

```luau
DamageZone.CanTouch: boolean
```

If false, it fires no Touched or TouchEnded, and touching it fires neither on other parts. It still collides.

#### Massless

```luau
DamageZone.Massless: boolean
```

If true, the part adds no mass to an assembly it shares with other parts. A part on its own (every part, until welds exist) keeps its own mass.

#### CustomPhysicalProperties

```luau
DamageZone.CustomPhysicalProperties: PhysicalProperties?
```

Its own density, friction, elasticity and their weights (`PhysicalProperties.new(...)`), or nil for its Material's. Read `CurrentPhysicalProperties` for the ones in use.

#### AssemblyLinearVelocity

```luau
DamageZone.AssemblyLinearVelocity: Vector3
```

How fast the part moves, in studs a second. Physics updates it as the part moves; setting it throws the part. Anchored parts don't move.

#### AssemblyAngularVelocity

```luau
DamageZone.AssemblyAngularVelocity: Vector3
```

How fast the part spins, in radians a second around each world axis. Physics updates it; setting it spins the part.

#### DamagePerSecond

```luau
DamageZone.DamagePerSecond: number
```

Health a second taken from every Health whose Model has a part inside the zone (once each, however many of its parts are in). The damage's source is the zone. 0 or less does nothing.

#### Locked

```luau
DamageZone.Locked: boolean
```

If true, the editor's viewport can't select or change it, and the building tools leave it alone. It has no effect in a running game.

#### CFrame

```luau
DamageZone.CFrame: CFrame
```

Its Position and Orientation as one CFrame. Setting it sets both; it isn't saved itself.

#### AssemblyMass

```luau
DamageZone.AssemblyMass: number
```

The mass of everything moving with it: the parts welded into its assembly, except Massless ones (a part alone keeps its own mass). Read-only.

#### AssemblyCenterOfMass

```luau
DamageZone.AssemblyCenterOfMass: Vector3
```

Where its mass is centered, in the world: the middle of a block or ball. Read-only.

#### CurrentPhysicalProperties

```luau
DamageZone.CurrentPhysicalProperties: PhysicalProperties
```

The physical properties in use: `CustomPhysicalProperties`, or its Material's when that's nil. Read-only.

### Events

#### Touched

```luau
DamageZone.Touched: Signal
```

Fires when another part starts touching this one, once per new contact (not every frame while they touch). The handler gets the other part: `part.Touched:Connect(function(otherPart) ... end)`.

#### TouchEnded

```luau
DamageZone.TouchEnded: Signal
```

Fires when another part stops touching this one: once they've been apart for a few physics steps, or at once when one of them can't touch the other any more (`CanTouch` off, or set not to collide). The handler gets the other part.

### Functions

#### UnionAsync

```luau
DamageZone:UnionAsync(parts: {Instance}, options: {[string]: any}?): UnionOperation
```

Joins the part and `parts` into one solid and returns it as a new UnionOperation (its Parent is nil), with this part's look and physics. NegateOperations among `parts` are cut out. The work runs in the background and the script waits for it. Errors, saying why, if a part isn't a closed solid (a Capsule, or a model with holes). `options.CollisionFidelity` sets the union's.

#### SubtractAsync

```luau
DamageZone:SubtractAsync(parts: {Instance}, options: {[string]: any}?): UnionOperation
```

Cuts every one of `parts` out of the part and returns what's left as a new UnionOperation (its Parent is nil), with this part's look and physics: a crater where a ball was. The work runs in the background and the script waits for it. Errors, saying why, if a part isn't a closed solid or nothing is left. `options.CollisionFidelity` sets the union's.

#### IntersectAsync

```luau
DamageZone:IntersectAsync(parts: {Instance}, options: {[string]: any}?): UnionOperation
```

Keeps only where the part and all of `parts` overlap, as a new UnionOperation (its Parent is nil), with this part's look and physics. The work runs in the background and the script waits for it. Errors, saying why, if a part isn't a closed solid or they don't overlap. `options.CollisionFidelity` sets the union's.

#### GetConnectedParts

```luau
DamageZone:GetConnectedParts(recursive: boolean?): {Instance}
```

The part, then the parts welded to it by an enabled WeldConstraint; with `recursive` true, every part in its assembly.

#### GetRootPart

```luau
DamageZone:GetRootPart(): Instance
```

The root of its assembly: an anchored part if there is one, then one that isn't Massless, then the largest. A part alone is its own root.

#### GetPivot

```luau
DamageZone:GetPivot(): CFrame
```

Where it turns and moves from: a part's own CFrame, or for a Model the centre of the box around its parts, unturned.

#### PivotTo

```luau
DamageZone:PivotTo(target: CFrame): ()
```

Moves and turns it so its pivot is at `target`. A Model's parts all move together and keep their places relative to each other.

#### SetNetworkOwner

```luau
DamageZone:SetNetworkOwner(player: Instance?): ()
```

Scripts only (Phase 66): `player`'s device simulates this part's whole assembly from now on, so it responds at once for them, and the server sees what it does; nil gives it back to the server. An anchored part has no owner (an error). Scripts only.

#### GetNetworkOwner

```luau
DamageZone:GetNetworkOwner(): Instance?
```

Scripts only: the Player whose device simulates this part's assembly now, or nil for the server. An anchored part has no owner (an error). Scripts only.

#### SetNetworkOwnershipAuto

```luau
DamageZone:SetNetworkOwnershipAuto(): ()
```

Scripts only: the engine chooses who simulates this part's assembly: the player driving a VehicleSeat in it, else the nearest player within 40 studs, else the server. Scripts only.

#### GetNetworkOwnershipAuto

```luau
DamageZone:GetNetworkOwnershipAuto(): boolean
```

Scripts only: true after SetNetworkOwnershipAuto, until SetNetworkOwner. Scripts only.

#### ApplyImpulse

```luau
DamageZone:ApplyImpulse(impulse: Vector3): ()
```

Pushes the part through its center of mass: its velocity changes by `impulse` divided by its assembly's mass (`AssemblyMass`, which is `GetMass()` for a part alone). Pushes in one frame add up. Anchored parts don't move.

#### ApplyImpulseAtPosition

```luau
DamageZone:ApplyImpulseAtPosition(impulse: Vector3, position: Vector3): ()
```

Pushes the part at `position` in the world: it speeds up as `ApplyImpulse` would, and an impulse off its center of mass also spins it.

#### ApplyAngularImpulse

```luau
DamageZone:ApplyAngularImpulse(impulse: Vector3): ()
```

Spins the part: its `AssemblyAngularVelocity` changes by `impulse` (radians a second times inertia) through its inertia.

#### GetMass

```luau
DamageZone:GetMass(): number
```

Its density times its volume: a 1×1×1 Plastic block weighs 1. MeshParts weigh their collision shape.

