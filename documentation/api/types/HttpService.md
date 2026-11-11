---
description: "JSON and GUIDs, from game:GetService(\"HttpService\")."
---

# HttpService

JSON and GUIDs, from `game:GetService("HttpService")`. It doesn't reach the network.

### Functions

#### JSONEncode

```luau
HttpService:JSONEncode(value: any): string
```

`value` as JSON text. A table with keys 1, 2, 3 (or none) becomes an array; one with text keys an object. Numbers, strings, booleans and nil can go in; anything else, or a table inside itself, is an error.

#### JSONDecode

```luau
HttpService:JSONDecode(json: string): any
```

The value JSON text holds: arrays and objects become tables, `null` nil. Text that isn't JSON is an error saying the line and column of the mistake.

#### GenerateGUID

```luau
HttpService:GenerateGUID(wrapInCurlyBraces: boolean?): string
```

A new random ID such as `"{0F8FAD5B-D9CB-469F-A165-70867728950E}"`; `false` leaves the braces off.

