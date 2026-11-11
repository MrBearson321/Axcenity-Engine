---
description: "A side players can be on, in Teams."
---

# Team

```luau
Instance.new("Team")
```

A side players can be on, in Teams. Its players spawn on SpawnLocations of its TeamColor; players joining go on the AutoAssignable team with the fewest players. `GetPlayers` lists them.

It also has every member of [Instance](Instance.md).

### Properties

#### TeamColor

```luau
Team.TeamColor: Color3
```

The team's color: its players spawn on SpawnLocations of this TeamColor.

#### AutoAssignable

```luau
Team.AutoAssignable: boolean
```

If true, players joining are put on this team, or on the AutoAssignable team with the fewest players when there are several.

### Functions

#### GetPlayers

```luau
Team:GetPlayers(): {Player}
```

The Players on this team (their Team is it, and Neutral is off).

