---
description: "A function scripts on the same side call for each other: Invoke(...) calls OnInvoke and returns what it returns."
---

# BindableFunction

```luau
Instance.new("BindableFunction")
```

A function scripts on the same side call for each other: `Invoke(...)` calls `OnInvoke` and returns what it returns. It doesn't cross between Scripts and LocalScripts.

It also has every member of [Instance](Instance.md).

### Properties

#### OnInvoke

```luau
BindableFunction.OnInvoke: (...any) -> ...any
```

The function `Invoke` calls: `func.OnInvoke = function(...) return ... end`. Setting it replaces the one before; it can be set, not read. Each side has its own.

### Functions

#### Invoke

```luau
BindableFunction:Invoke(...: any): ...any
```

Calls `OnInvoke` with the arguments and returns what it returns, waiting while it waits. Its errors are raised here.

