---
description: "The service with what each player starts with: StarterPlayerScripts, and the StarterCharacter each character is a copy of."
---

# StarterPlayer

```luau
game.StarterPlayer
```

The service with what each player starts with: StarterPlayerScripts, and the StarterCharacter each character is a copy of.

It also has every member of [Instance](Instance.md).

### Properties

#### StarterCharacter

```luau
StarterPlayer.StarterCharacter: string
```

The prefab (`assets/prefabs/*.prefab.json`) each player's character is a copy of. Empty uses the character Insert > Character makes: six welded parts with a Humanoid, a ThirdPersonController, a Health, an Inventory and an Animator.

#### CharacterFootPlacement

```luau
StarterPlayer.CharacterFootPlacement: boolean
```

If true, each player's character spawns with its controller's FootPlacement on. If false, a StarterCharacter prefab keeps its own setting.

