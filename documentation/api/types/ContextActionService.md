---
description: "Functions bound to inputs, from game:GetService(\"ContextActionService\")."
---

# ContextActionService

Functions bound to inputs, from `game:GetService("ContextActionService")`. When a bound input goes down, changes or comes up, only the highest-priority action bound to it is called (the latest bound, of equal ones), so a menu can take a key from the game until it's unbound. Input the UI took doesn't reach actions. LocalScripts only.

### Functions

#### BindAction

```luau
ContextActionService:BindAction(name: string, handler: (string, EnumUserInputState, InputObject) -> (), createTouchButton: boolean, ...: EnumItem): ()
```

Calls `handler(actionName, inputState, inputObject)` for `inputs` (`Enum.KeyCode` or `Enum.UserInputType` items), at the default priority (2000). Binding a name again replaces it. `createTouchButton` is kept, but no touch button is drawn yet.

```lua
local ContextActionService = game:GetService("ContextActionService")
local function reload(actionName, inputState)
	if inputState == Enum.UserInputState.Begin then
		print("Reloading")
	end
end
ContextActionService:BindAction("Reload", reload, true, Enum.KeyCode.R)
```

#### BindActionAtPriority

```luau
ContextActionService:BindActionAtPriority(name: string, handler: (string, EnumUserInputState, InputObject) -> (), createTouchButton: boolean, priority: number, ...: EnumItem): ()
```

BindAction at `priority`: a higher number hides the same inputs from lower ones.

#### UnbindAction

```luau
ContextActionService:UnbindAction(name: string): ()
```

Unbinds action `name`, so the inputs go to the next action down.

#### UnbindAllActions

```luau
ContextActionService:UnbindAllActions(): ()
```

Unbinds every action.

#### GetBoundActionInfo

```luau
ContextActionService:GetBoundActionInfo(name: string): {[string]: any}
```

Action `name`'s `priorityLevel`, `inputTypes`, `createTouchButton` and `stackOrder`, or an empty table when it isn't bound.

