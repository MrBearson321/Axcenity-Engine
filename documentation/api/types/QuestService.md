---
description: "Quests, from game:GetService(\"QuestService\"): rows of the data table quests (Title, Description), with objectives from quest_objectives (Quest, Kind: Count, Reach or Talk, Target, Count, Text)."
---

# QuestService

Quests, from `game:GetService("QuestService")`: rows of the data table `quests` (Title, Description), with objectives from `quest_objectives` (Quest, Kind: Count, Reach or Talk, Target, Count, Text). Their state is the player's data, so save slots keep it. The quest tracker shows the active ones. Both sides share it.

### Properties

#### QuestStarted

```luau
QuestService.QuestStarted: Signal
```

Fires with the quest's key when it starts: `function(quest) ... end`.

#### QuestProgressed

```luau
QuestService.QuestProgressed: Signal
```

Fires as an objective moves on: `function(quest, objective, progress) ... end`.

#### QuestCompleted

```luau
QuestService.QuestCompleted: Signal
```

Fires with the quest's key when it completes: `function(quest) ... end`.

### Functions

#### Start

```luau
QuestService:Start(quest: string): boolean
```

Starts the quest `quest` (its key in `quests`). Returns false when it's active or done already.

#### Progress

```luau
QuestService:Progress(target: string, amount: number?): ()
```

Adds `amount` (1 if left out) to every active quest's Count objectives whose Target is `target`, up to their Count. A quest whose objectives are all done completes.

#### Complete

```luau
QuestService:Complete(quest: string): boolean
```

Completes the quest, whatever its objectives say. Returns false when it was done already.

#### GetState

```luau
QuestService:GetState(quest: string): string
```

`"NotStarted"`, `"Active"` or `"Completed"`.

#### GetActive

```luau
QuestService:GetActive(): { string }
```

The keys of the active quests.

#### GetObjectives

```luau
QuestService:GetObjectives(quest: string): { { Key: string, Kind: string, Target: string, Text: string, Count: number, Progress: number, Done: boolean } }
```

The quest's objectives in order, each `{ Key, Kind, Target, Text, Count, Progress, Done }` (Text in the player's language).

#### GetTitle

```luau
QuestService:GetTitle(quest: string): string
```

The quest's Title, in the player's language.

