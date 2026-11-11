---
description: "Lua code that scripts load with require, from a .module.lua file."
---

# ModuleScript

```luau
Instance.new("ModuleScript")
```

Lua code that scripts load with `require`, from a `.module.lua` file. It runs once per side and returns one value.

It also has every member of [Instance](Instance.md).

### Properties

#### SourceFile

```luau
ModuleScript.SourceFile: string
```

For a script inside an object: the project file its code is in, such as `objects/Door/Open.module.lua`, which every copy shares. Empty for a script in the `scripts` folder. Changing it while the game runs doesn't change the code that runs.

