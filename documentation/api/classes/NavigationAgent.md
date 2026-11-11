---
description: "Walks the Model or Part it is parented to during Play: along the navigation mesh to its Destination, round walls and gaps, or straight at its Velocity."
---

# NavigationAgent

```luau
Instance.new("NavigationAgent")
```

Walks the Model or Part it is parented to during Play: along the navigation mesh to its Destination, round walls and gaps, or straight at its Velocity. It sees and hears the player character within its SightRange, SightAngle and HearingRange, and fires PlayerSensed. Its BehaviorTree, if it has one, decides where it walks. It is not a physics body. View > Overlays > Agent Paths draws the path it is walking.

It also has every member of [Instance](Instance.md).

### Properties

#### Destination

```luau
NavigationAgent.Destination: Vector3
```

Where the agent walks to, along the navigation mesh, during Play. Setting a new one finds a new path. A point off the mesh, or with no walkable route to it, leaves the agent where it is: Status becomes "Unreachable" and Output says why. The editor starts it where the agent stands. Set it from a script or the "walk to" block.

#### WalkSpeed

```luau
NavigationAgent.WalkSpeed: number
```

How fast the agent walks its path, in studs a second. It steers round anything that moves into its path, and rejoins the path past it.

#### Velocity

```luau
NavigationAgent.Velocity: Vector3
```

Studs a second. Not zero, the agent moves straight that way along the ground, with no path, and stops at the edge of walkable ground. Destination follows it, so setting Velocity back to zero leaves it where it is. Y is ignored. Set it from a script or the "set velocity" block.

#### Status

```luau
NavigationAgent.Status: string
```

"Moving" while it walks a path or moves at Velocity, "Reached" at its Destination, "Unreachable" when no path leads to its Destination, "Blocked" when something in the way of its path has no way round (it stops, Output says so, and it carries on once the way clears), and "Idle" after Velocity stopped it. Set by the game during Play; read it from a script or the property block.

#### SightRange

```luau
NavigationAgent.SightRange: number
```

How far the agent sees the player character, in studs, from its eyes near the top of its body. Parts that collide block its sight. 0 sees nothing.

#### SightAngle

```luau
NavigationAgent.SightAngle: number
```

How wide the agent sees, in degrees across, centred on the way it faces: the way it last moved, or its body's front before it has moved. 360 sees all round; 0 sees nothing.

#### HearingRange

```luau
NavigationAgent.HearingRange: number
```

How far the agent hears the player character, in studs, through walls. It hears a Sound playing anywhere inside the player character's model. 0 hears nothing.

#### SeesPlayer

```luau
NavigationAgent.SeesPlayer: boolean
```

True while the agent sees the player character (the one whose controller owns the camera). Set by the game during Play; PlayerSensed fires when it turns true.

#### HearsPlayer

```luau
NavigationAgent.HearsPlayer: boolean
```

True while the agent hears a Sound playing in the player character. Set by the game during Play; PlayerSensed fires when it turns true.

#### BehaviorTree

```luau
NavigationAgent.BehaviorTree: string
```

The behavior tree the agent runs during Play: a .behaviortree.json file in the project, made in the Behavior Tree tab. Every frame, before the agent walks, the tree decides where it goes (it can test SeesPlayer and HearsPlayer, set Destination, chase the player character and wait), so it may overwrite a Destination a script set. Empty runs none, and scripts set Destination themselves. A file that isn't a behavior tree, or has problems, runs nothing and Output says why once.

#### PatrolSpline

```luau
NavigationAgent.PatrolSpline: Instance?
```

A Spline whose points the agent walks to in order during Play, with no script: round again from the last to the first on a Closed spline, back and forth on an open one. It walks the navigation mesh between points, so it goes round walls; a point it can't reach is skipped. nil patrols nothing. `agent:Patrol(spline)` sets it.

### Events

#### PlayerSensed

```luau
NavigationAgent.PlayerSensed: Signal
```

Fires when the agent starts seeing or hearing the player character. The handler gets the sense: "Sight" or "Hearing".

### Functions

#### Patrol

```luau
NavigationAgent:Patrol(spline: Spline?): ()
```

Sets PatrolSpline: the agent walks `spline`'s points in order from now on (nil stops).

