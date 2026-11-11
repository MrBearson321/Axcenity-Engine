---
description: "Dialogues (assets/dialogue/*.dialogue.json, made in the Dialogue tab), from game:GetService(\"DialogueService\")."
---

# DialogueService

Dialogues (`assets/dialogue/*.dialogue.json`, made in the Dialogue tab), from `game:GetService("DialogueService")`. One runs at a time, for the local player: the dialogue box shows its lines and choices, and the character stands still. A ProximityPrompt with a `Dialogue` starts one with no script. Both sides share it.

### Properties

#### Active

```luau
DialogueService.Active: boolean
```

True while a dialogue runs.

### Functions

#### Start

```luau
DialogueService:Start(dialogue: string, speaker: Instance?): string
```

Runs the dialogue at the project path `dialogue`, said by `speaker` (a Model; its Name is the speaker's when a Line has none), waits until it ends, and returns the name of the End it reached (`""` for none). An error if another dialogue is running or this one can't run.

#### Advance

```luau
DialogueService:Advance(): ()
```

Goes on from the line shown, as the player pressing Interact does; with choices showing, picks the highlighted one.

#### Choose

```luau
DialogueService:Choose(index: number): ()
```

Picks the choice shown `index` (from 1). An error with no choices showing.

#### Stop

```luau
DialogueService:Stop(): ()
```

Ends the running dialogue at once; Start returns `""`.

#### GetLine

```luau
DialogueService:GetLine(): { Speaker: string, Text: string, Choices: { string }, Selected: number }?
```

What the dialogue box shows, in the player's language: `{ Speaker, Text, Choices, Selected }`, or nil with no dialogue running.

