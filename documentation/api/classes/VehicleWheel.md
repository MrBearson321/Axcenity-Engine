---
description: "A wheel of the car whose part it's parented to: a spring and a ray to the ground, so no wheel collider touches it."
---

# VehicleWheel

```luau
Instance.new("VehicleWheel")
```

A wheel of the car whose part it's parented to: a spring and a ray to the ground, so no wheel collider touches it. Position and Orientation are relative to the part, as an Attachment's. Not drawn; its WheelPart is moved to it.

It also has every member of [Instance](Instance.md).

### Properties

#### Position

```luau
VehicleWheel.Position: Vector3
```

The wheel's middle with its spring at rest length, in studs from the middle of its part, along the part's own axes.

#### Orientation

```luau
VehicleWheel.Orientation: Vector3
```

How the wheel is turned relative to its part, in degrees like a Part's Orientation. The wheel rolls toward its -Z, turns on its X axle, and its spring is along its Y.

#### Radius

```luau
VehicleWheel.Radius: number
```

The wheel's radius in studs. Its bottom is this far below its middle.

#### SuspensionRestLength

```luau
VehicleWheel.SuspensionRestLength: number
```

Studs of spring above the wheel's middle. The car's weight squashes it; a wheel off the ground hangs at this length.

#### SuspensionStiffness

```luau
VehicleWheel.SuspensionStiffness: number
```

How hard the spring pushes back for each stud it's squashed, whatever the car weighs. With four wheels, the default 100 squashes each half a stud at rest.

#### SuspensionDamping

```luau
VehicleWheel.SuspensionDamping: number
```

How much the spring holds back bouncing. Higher settles the car sooner; the engine lowers it where more would make the car rock.

#### Friction

```luau
VehicleWheel.Friction: number
```

How much grip the tyre has before it slides, braking or pushing.

#### Steers

```luau
VehicleWheel.Steers: boolean
```

If true, the wheel turns with the seat's Steer.

#### Driven

```luau
VehicleWheel.Driven: boolean
```

If true, the wheel pushes with the seat's Throttle.

#### Brakes

```luau
VehicleWheel.Brakes: boolean
```

If true, the wheel holds the car on the HandBrake, and when nobody drives.

#### WheelPart

```luau
VehicleWheel.WheelPart: Instance?
```

A part moved to the wheel every frame during Play, spinning as it rolls and turning as it steers: its middle at the wheel's and its X along the axle, as a Cylinder's round faces are. Make it Anchored with CanCollide off; nil shows nothing.

