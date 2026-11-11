---
description: "The project's data tables (assets/data/*.data.json, made in the Data Table tab), from game:GetService(\"DataService\")."
---

# DataService

The project's data tables (`assets/data/*.data.json`, made in the Data Table tab), from `game:GetService("DataService")`. Scripts read them; they can't change them.

### Functions

#### GetTable

```luau
DataService:GetTable(name: string): DataTable
```

The table named `name`: `"weapons"` is `assets/data/weapons.data.json`. A table that isn't there, or can't be read, is an error saying why.

#### CreateAbility

```luau
DataService:CreateAbility(name: string, key: string): Ability
```

A new Ability (with no Parent) made from the row `key` of the table `name`, named `key`: each column named after an Ability property (Cooldown, CostAttribute, CostAmount, ActivationKey, BlockedByTags, GrantsTags, Duration, Icon) sets it, and the other columns become its attributes.

#### CreateEffect

```luau
DataService:CreateEffect(name: string, key: string): Effect
```

A new Effect (with no Parent) made from the row `key` of the table `name`, named `key`: each column named after an Effect property (Attribute, Operation, Amount, Duration, Period, MaxStacks, GrantsTags) sets it, and the other columns become its attributes.

