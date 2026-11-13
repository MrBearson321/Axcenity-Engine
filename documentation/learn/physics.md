---
description: Anchored and unanchored parts, collisions, touch events, materials' physical properties, welds, joints, movers, vehicles and destruction.
icon: atom
samples: [hello-physics, hello-touched, hello-joints, hello-movers, hello-vehicle, hello-destruction]
---

# Physics and collisions

During Play, every part that isn't **Anchored** is simulated: it falls, collides, slides and rolls. Physics steps 60 times a second whatever the frame rate, and parts are drawn smoothly between steps.

## Anchored or not

- **Anchored** parts stay exactly where they are: floors, walls, buildings. Other parts collide with them.
- Unanchored parts are moved by physics. New parts start unanchored, so tick **Anchored** (or use the **Anchor** tool, **M**) on anything that should stay put.
- **CanCollide** off lets other parts pass through a part (it still reports touches). **CanTouch** off stops touch events. **Massless** parts add no weight to what they are welded to.

Setting a part's **Position** or **Orientation** from a script teleports it and stops it. To push it, use `Part:ApplyImpulse()` or set `Part.AssemblyLinearVelocity`.

## Gravity and materials

`Workspace.Gravity` is 196.2 studs per second² to start. Each part's **Material** gives it a density, friction and bounciness: Metal is heavy, Ice slides and Rubber bounces. **CustomPhysicalProperties** overrides them for one part.

## Touches

A part's `Touched` event fires when another part starts touching it, and `TouchEnded` when they part. Parts resting on each other when the game starts don't count, and two anchored parts never touch.

```lua
local pad = workspace.Pad
pad.Touched:Connect(function(other)
	print(other.Name .. " landed on the pad")
end)
```

The **hello-touched** sample does this in Lua, and **hello-touched-blocks** with blocks.

## Collision groups

Put parts in named groups with their **CollisionGroup** property, then choose which groups collide in **Collision Groups…** (Home tab, Snapping). Up to 32 groups; they're saved in `collision-groups.json`.

## Welds, joints and movers

- A **WeldConstraint** holds two parts together as one rigid body. **Weld** (Model tab, Constraints) welds the selected parts to the largest of them; **Join Surfaces** welds parts as you place them.
- Joints connect two parts at **Attachments**: hinges, sliders, springs, ropes, rods, ball sockets and more. Place them with the **Constraint** tool: click a surface on one part, then on the other.
- Movers push or turn a part: **LinearVelocity**, **AngularVelocity**, **VectorForce**, **Torque**, **AlignPosition** and **AlignOrientation**.
- **Viewport > Overlays > Constraints** draws welds, joints and movers, and **Colliders** draws each part's collision shape.

## Vehicles, ragdolls and destruction

- **Vehicle** (World tab, Gameplay) inserts a ready-to-drive car with a **VehicleSeat** and **VehicleWheel**s. Sit in the seat during Play and drive with the keys.
- A **Ragdoll** in a character makes it go limp and blend back.
- A **Fracture** in a part lets it shatter into pieces when hit hard enough or when a script calls `Fracture:Shatter()`.

## Spatial queries

Scripts ask the world what's where without moving anything: `workspace:Raycast()` finds the first part along a line, `workspace:GetPartBoundsInBox()` and `workspace:GetPartBoundsInRadius()` find parts in an area. **Viewport > Overlays > Raycasts** shows recent rays during Play.

> **Known limits.** Imported models (MeshParts) collide as their convex hull by default, so hollows are filled in. To keep a doorway or an arch open, give the model its own collision pieces (objects named like it with `_COL` on the end, such as `Arch_COL1` and `Arch_COL2`, each a convex shape; they aren't drawn), or set the MeshPart's **CollisionFidelity** to `PreciseConvexDecomposition`. A ragdoll can briefly bend past its joint limits on a hard landing.
