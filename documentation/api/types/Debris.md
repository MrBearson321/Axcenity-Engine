---
description: "Destroys Instances later without waiting, from game:GetService(\"Debris\")."
---

# Debris

Destroys Instances later without waiting, from `game:GetService("Debris")`.

### Functions

#### AddItem

```luau
Debris:AddItem(instance: Instance, seconds: number?): ()
```

Destroys `instance` once `seconds` (left out: 10) have passed. Nothing happens if it's destroyed before then. The script carries on at once.

```lua
local Debris = game:GetService("Debris")
local spark = Instance.new("Part")
spark.Parent = workspace
Debris:AddItem(spark, 2)
```

