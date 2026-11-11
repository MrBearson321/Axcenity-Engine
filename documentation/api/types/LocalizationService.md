---
description: "The language UI text shows, from game:GetService(\"LocalizationService\")."
---

# LocalizationService

The language UI text shows, from `game:GetService("LocalizationService")`. A TextLabel or TextButton with a `TextKey` shows that key's value from the project's string table (`strings.json`, edited in the UI Designer's Strings tab) in `Language`, or its own Text when there's none. Both sides share it.

### Properties

#### Language

```luau
LocalizationService.Language: string
```

The active language, such as `"English"`: one of `Languages`, the first when the game starts. Setting it changes every bound label and button; a language the table doesn't have is an error and changes nothing.

#### Languages

```luau
LocalizationService.Languages: { string }
```

The string table's languages, in order. Read only.

### Functions

#### GetString

```luau
LocalizationService:GetString(key: string): string?
```

`key`'s value in the active language, or nil when the string table has none.

