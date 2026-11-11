---
description: "A Seat that drives the car it's welded into: the body made of every part welded to it, the chassis."
---

# VehicleSeat

```luau
Instance.new("VehicleSeat")
```

A Seat that drives the car it's welded into: the body made of every part welded to it, the chassis. While someone sits on it, the Move action sets Throttle and Steer, and the camera follows the car until they get out with Jump. The car rides on the VehicleWheels parented to its parts. Everything a Seat has, and MaxSpeed, Torque, TurnSpeed, SteerAngle, Throttle, Steer and HandBrake.

It also has every member of [Instance](Instance.md).

### Properties

#### Anchored

```luau
VehicleSeat.Anchored: boolean
```

If true, the part stays where it is. If false, it falls and collides with other parts.

#### Shape

```luau
VehicleSeat.Shape: EnumPartType | "Block" | "Ball" | "Cylinder" | "Wedge" | "Capsule"
```

Block, Ball, Cylinder (along X) or Wedge.

#### Position

```luau
VehicleSeat.Position: Vector3
```

Where the part's center is, in studs. Setting it on an unanchored part teleports it and stops it moving.

#### Orientation

```luau
VehicleSeat.Orientation: Vector3
```

Rotation in degrees around X, Y and Z, applied Z first, then X, then Y.

#### Size

```luau
VehicleSeat.Size: Vector3
```

The part's size in studs along X, Y and Z.

#### Color

```luau
VehicleSeat.Color: Color3
```

The part's color.

#### Material

```luau
VehicleSeat.Material: EnumMaterial | "Plastic" | "SmoothPlastic" | "Wood" | "WoodPlanks" | "Marble" | "Granite" | "Concrete" | "Slate" | "Brick" | "Fabric" | "Sand" | "Metal" | "Glass" | "Neon" | "Ice" | "Rubber"
```

Plastic, Metal, Glass (see-through) or Neon (glows). Setting it resets Metalness and Roughness.

#### MaterialVariant

```luau
VehicleSeat.MaterialVariant: string
```

A material file in the project, such as "assets/materials/brick.material.json", drawn in place of Material, Metalness, Roughness and Emission; Color tints it and Transparency fades it. A SurfaceAppearance inside the part wins over it. Empty for none.

#### BlueprintTexture

```luau
VehicleSeat.BlueprintTexture: boolean
```

If true, a plain Plastic part is drawn in a prototype grid texture while editing, to build by. It never shows in Play or in a built game.

#### Metalness

```luau
VehicleSeat.Metalness: number
```

From 0 (not metal) to 1 (metal).

#### Roughness

```luau
VehicleSeat.Roughness: number
```

From 0 (mirror-smooth) to 1 (matte).

#### CastShadow

```luau
VehicleSeat.CastShadow: boolean
```

If false, the part casts no sun shadow. It's still shaded by other parts' shadows.

#### Transparency

```luau
VehicleSeat.Transparency: number
```

From 0 (as solid as its material) to 1 (invisible).

#### CanCollide

```luau
VehicleSeat.CanCollide: boolean
```

If false, other parts pass through it. It still fires Touched. An unanchored part that can't collide falls through the floor.

#### WindAffected

```luau
VehicleSeat.WindAffected: boolean
```

If true and the part isn't anchored, workspace.GlobalWind pushes it, harder for parts with bigger faces.

#### EmissiveColor

```luau
VehicleSeat.EmissiveColor: Color3
```

The color the part glows in when Emission is above 0.

#### Emission

```luau
VehicleSeat.Emission: number
```

How brightly the part glows in EmissiveColor: 0 is no glow, 1 as bright as a white surface in full light, higher values bloom.

#### TileSize

```luau
VehicleSeat.TileSize: number
```

Studs one copy of a SurfaceAppearance's textures covers. 0 leaves the shape's own coordinates unscaled.

#### CollisionGroup

```luau
VehicleSeat.CollisionGroup: string
```

The collision group it's in, `"Default"` at first. Parts in two groups set not to collide (the Collision Groups window, or `PhysicsService:CollisionGroupSetCollidable`) pass through each other and don't touch. A name that isn't registered counts as Default.

#### CanQuery

```luau
VehicleSeat.CanQuery: boolean
```

If false, scripts' raycasts, shape casts and overlap queries (`workspace:Raycast`, `GetPartsInPart`, ...) pass it by. It still collides.

#### CanTouch

```luau
VehicleSeat.CanTouch: boolean
```

If false, it fires no Touched or TouchEnded, and touching it fires neither on other parts. It still collides.

#### Massless

```luau
VehicleSeat.Massless: boolean
```

If true, the part adds no mass to an assembly it shares with other parts. A part on its own (every part, until welds exist) keeps its own mass.

#### CustomPhysicalProperties

```luau
VehicleSeat.CustomPhysicalProperties: PhysicalProperties?
```

Its own density, friction, elasticity and their weights (`PhysicalProperties.new(...)`), or nil for its Material's. Read `CurrentPhysicalProperties` for the ones in use.

#### AssemblyLinearVelocity

```luau
VehicleSeat.AssemblyLinearVelocity: Vector3
```

How fast the part moves, in studs a second. Physics updates it as the part moves; setting it throws the part. Anchored parts don't move.

#### AssemblyAngularVelocity

```luau
VehicleSeat.AssemblyAngularVelocity: Vector3
```

How fast the part spins, in radians a second around each world axis. Physics updates it; setting it spins the part.

#### Occupant

```luau
VehicleSeat.Occupant: Instance?
```

The character controller sitting on the seat, or nil. Read-only: `seat:Sit(controller)` sits one, and `controller.Sit = false` stands it up.

#### Disabled

```luau
VehicleSeat.Disabled: boolean
```

If true, characters that touch the seat don't sit on it. `seat:Sit(controller)` still works.

#### SeatOffset

```luau
VehicleSeat.SeatOffset: Instance?
```

An Attachment where a sitting character's feet go, or nil for the middle of the seat's top face.

#### MaxSpeed

```luau
VehicleSeat.MaxSpeed: number
```

Studs a second the driven wheels stop pushing at. They push less the closer the car gets.

#### Torque

```luau
VehicleSeat.Torque: number
```

How hard the driven wheels push at full Throttle: studs a second squared on flat ground, whatever the car weighs. At the default 80 a car climbs slopes up to about 24°.

#### TurnSpeed

```luau
VehicleSeat.TurnSpeed: number
```

Degrees a second the steering wheels turn toward where Steer points them.

#### SteerAngle

```luau
VehicleSeat.SteerAngle: number
```

Degrees the wheels with Steers on turn at full Steer.

#### Throttle

```luau
VehicleSeat.Throttle: number
```

-1 (full reverse) to 1 (full forward). While someone sits on the seat, the Move action sets it every frame (W and S, or a stick); scripts set it to drive an empty car. Getting out sets it to 0.

#### Steer

```luau
VehicleSeat.Steer: number
```

-1 (left) to 1 (right), set like Throttle (A and D, or a stick).

#### HandBrake

```luau
VehicleSeat.HandBrake: boolean
```

If true, the wheels with Brakes on hold the car still. An empty seat with no Throttle holds the car the same way; otherwise a car with no Throttle coasts, slowing a little.

#### Locked

```luau
VehicleSeat.Locked: boolean
```

If true, the editor's viewport can't select or change it, and the building tools leave it alone. It has no effect in a running game.

#### CFrame

```luau
VehicleSeat.CFrame: CFrame
```

Its Position and Orientation as one CFrame. Setting it sets both; it isn't saved itself.

#### AssemblyMass

```luau
VehicleSeat.AssemblyMass: number
```

The mass of everything moving with it: the parts welded into its assembly, except Massless ones (a part alone keeps its own mass). Read-only.

#### AssemblyCenterOfMass

```luau
VehicleSeat.AssemblyCenterOfMass: Vector3
```

Where its mass is centered, in the world: the middle of a block or ball. Read-only.

#### CurrentPhysicalProperties

```luau
VehicleSeat.CurrentPhysicalProperties: PhysicalProperties
```

The physical properties in use: `CustomPhysicalProperties`, or its Material's when that's nil. Read-only.

### Events

#### Touched

```luau
VehicleSeat.Touched: Signal
```

Fires when another part starts touching this one, once per new contact (not every frame while they touch). The handler gets the other part: `part.Touched:Connect(function(otherPart) ... end)`.

#### TouchEnded

```luau
VehicleSeat.TouchEnded: Signal
```

Fires when another part stops touching this one: once they've been apart for a few physics steps, or at once when one of them can't touch the other any more (`CanTouch` off, or set not to collide). The handler gets the other part.

### Functions

#### UnionAsync

```luau
VehicleSeat:UnionAsync(parts: {Instance}, options: {[string]: any}?): UnionOperation
```

Joins the part and `parts` into one solid and returns it as a new UnionOperation (its Parent is nil), with this part's look and physics. NegateOperations among `parts` are cut out. The work runs in the background and the script waits for it. Errors, saying why, if a part isn't a closed solid (a Capsule, or a model with holes). `options.CollisionFidelity` sets the union's.

#### SubtractAsync

```luau
VehicleSeat:SubtractAsync(parts: {Instance}, options: {[string]: any}?): UnionOperation
```

Cuts every one of `parts` out of the part and returns what's left as a new UnionOperation (its Parent is nil), with this part's look and physics: a crater where a ball was. The work runs in the background and the script waits for it. Errors, saying why, if a part isn't a closed solid or nothing is left. `options.CollisionFidelity` sets the union's.

#### IntersectAsync

```luau
VehicleSeat:IntersectAsync(parts: {Instance}, options: {[string]: any}?): UnionOperation
```

Keeps only where the part and all of `parts` overlap, as a new UnionOperation (its Parent is nil), with this part's look and physics. The work runs in the background and the script waits for it. Errors, saying why, if a part isn't a closed solid or they don't overlap. `options.CollisionFidelity` sets the union's.

#### Sit

```luau
VehicleSeat:Sit(controller: Instance): ()
```

Sits `controller`'s character on the seat at its next step, even if the seat is Disabled. Whoever sat there stands up, and the character leaves any other seat.

#### GetConnectedParts

```luau
VehicleSeat:GetConnectedParts(recursive: boolean?): {Instance}
```

The part, then the parts welded to it by an enabled WeldConstraint; with `recursive` true, every part in its assembly.

#### GetRootPart

```luau
VehicleSeat:GetRootPart(): Instance
```

The root of its assembly: an anchored part if there is one, then one that isn't Massless, then the largest. A part alone is its own root.

#### GetPivot

```luau
VehicleSeat:GetPivot(): CFrame
```

Where it turns and moves from: a part's own CFrame, or for a Model the centre of the box around its parts, unturned.

#### PivotTo

```luau
VehicleSeat:PivotTo(target: CFrame): ()
```

Moves and turns it so its pivot is at `target`. A Model's parts all move together and keep their places relative to each other.

#### SetNetworkOwner

```luau
VehicleSeat:SetNetworkOwner(player: Instance?): ()
```

Scripts only (Phase 66): `player`'s device simulates this part's whole assembly from now on, so it responds at once for them, and the server sees what it does; nil gives it back to the server. An anchored part has no owner (an error). Scripts only.

#### GetNetworkOwner

```luau
VehicleSeat:GetNetworkOwner(): Instance?
```

Scripts only: the Player whose device simulates this part's assembly now, or nil for the server. An anchored part has no owner (an error). Scripts only.

#### SetNetworkOwnershipAuto

```luau
VehicleSeat:SetNetworkOwnershipAuto(): ()
```

Scripts only: the engine chooses who simulates this part's assembly: the player driving a VehicleSeat in it, else the nearest player within 40 studs, else the server. Scripts only.

#### GetNetworkOwnershipAuto

```luau
VehicleSeat:GetNetworkOwnershipAuto(): boolean
```

Scripts only: true after SetNetworkOwnershipAuto, until SetNetworkOwner. Scripts only.

#### ApplyImpulse

```luau
VehicleSeat:ApplyImpulse(impulse: Vector3): ()
```

Pushes the part through its center of mass: its velocity changes by `impulse` divided by its assembly's mass (`AssemblyMass`, which is `GetMass()` for a part alone). Pushes in one frame add up. Anchored parts don't move.

#### ApplyImpulseAtPosition

```luau
VehicleSeat:ApplyImpulseAtPosition(impulse: Vector3, position: Vector3): ()
```

Pushes the part at `position` in the world: it speeds up as `ApplyImpulse` would, and an impulse off its center of mass also spins it.

#### ApplyAngularImpulse

```luau
VehicleSeat:ApplyAngularImpulse(impulse: Vector3): ()
```

Spins the part: its `AssemblyAngularVelocity` changes by `impulse` (radians a second times inertia) through its inertia.

#### GetMass

```luau
VehicleSeat:GetMass(): number
```

Its density times its volume: a 1×1×1 Plastic block weighs 1. MeshParts weigh their collision shape.

