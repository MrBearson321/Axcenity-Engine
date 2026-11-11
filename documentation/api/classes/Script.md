---
description: "Lua code that runs on the server, from a .server.lua file."
---

# Script

```luau
Instance.new("Script")
```

Lua code that runs on the server, from a `.server.lua` file.

It also has every member of [Instance](Instance.md).

### Properties

#### SourceFile

```luau
Script.SourceFile: string
```

For a script inside an object: the project file its code is in, such as `objects/Door/Open.server.lua`, which every copy shares. Empty for a script in the `scripts` folder. Changing it while the game runs doesn't change the code that runs.

