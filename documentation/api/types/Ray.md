---
description: "A half-line from Origin along Direction, from Camera:ScreenPointToRay."
---

# Ray

A half-line from `Origin` along `Direction`, from `Camera:ScreenPointToRay`.

### Properties

#### Origin

```luau
Ray.Origin: Vector3
```

Where it starts.

#### Direction

```luau
Ray.Direction: Vector3
```

Which way it goes, one stud long. Multiply it for `workspace:Raycast`: `workspace:Raycast(ray.Origin, ray.Direction * 500)`.

#### Unit

```luau
Ray.Unit: Ray
```

The same ray: its Direction is already one stud long.

