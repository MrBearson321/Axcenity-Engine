---
description: "Points at another Instance in Value, or nil."
---

# ObjectValue

```luau
Instance.new("ObjectValue")
```

Points at another Instance in Value, or nil. Its `Changed` fires with the new value. `Clone` points a copy at the same place inside the copy; saving keeps the path of names to it.

It also has every member of [Instance](Instance.md).

### Properties

#### Value

```luau
ObjectValue.Value: Instance?
```

The Instance it points at, or nil (the default).

