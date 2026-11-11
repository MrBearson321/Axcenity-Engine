---
description: "Lua code that runs on the player's side (input, camera), from a .client.lua file."
---

# LocalScript

```luau
Instance.new("LocalScript")
```

Lua code that runs on the player's side (input, camera), from a `.client.lua` file.

It also has every member of [Instance](Instance.md).

### Properties

#### SourceFile

```luau
LocalScript.SourceFile: string
```

For a script inside an object: the project file its code is in, such as `objects/Door/Open.client.lua`, which every copy shares. Empty for a script in the `scripts` folder. Changing it while the game runs doesn't change the code that runs.

