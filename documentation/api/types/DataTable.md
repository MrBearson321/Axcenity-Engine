---
description: "A data table, read-only."
---

# DataTable

A data table, read-only. Each call reads the version in use now, so an edit made during Play is what the next call sees.

### Functions

#### Get

```luau
DataTable:Get(key: string): DataRow?
```

The row with this key, or nil if there's none.

#### GetAll

```luau
DataTable:GetAll(): { [string]: DataRow }
```

Every row, by its key: `for key, row in weapons:GetAll() do`. A new table each call.

#### Keys

```luau
DataTable:Keys(): { string }
```

Every row's key, in the table's order. A new list each call.

