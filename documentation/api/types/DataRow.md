---
description: "One row of a data table, read by column name: row.Damage."
---

# DataRow

One row of a data table, read by column name: `row.Damage`. Numbers, strings, booleans, Vector3s and Color3s are what they say; an enum is its option's name, an asset its path, and a reference the row it names (nil for none). `for column, value in row do` goes through its columns. Changing it is an error: tables are read-only in scripts.

