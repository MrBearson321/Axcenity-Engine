---
description: "Collision groups, from game:GetService(\"PhysicsService\")."
---

# PhysicsService

Collision groups, from `game:GetService("PhysicsService")`. Changes last until Stop; the Collision Groups window (Model tab) sets the ones a game starts with.

### Functions

#### RegisterCollisionGroup

```luau
PhysicsService:RegisterCollisionGroup(name: string): ()
```

Adds a collision group (at most 32, Default included). It collides with every group until set not to. Registering one that's there does nothing.

#### CollisionGroupSetCollidable

```luau
PhysicsService:CollisionGroupSetCollidable(group1: string, group2: string, collidable: boolean): ()
```

Sets whether parts in the two groups collide (and touch). Both must be registered.

#### CollisionGroupsAreCollidable

```luau
PhysicsService:CollisionGroupsAreCollidable(group1: string, group2: string): boolean
```

True if parts in the two groups collide. Both must be registered.

