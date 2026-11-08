# Dialogue and quests

Phase 77. Code: `crates/script/src/dialogue.rs` (files, running, `DialogueService`), `crates/script/src/quest.rs` (`QuestService`), `crates/blocks-runtime/src/dialogue.rs` (the nodes the editor offers), `crates/ui/src/dialogue_box.rs` (the dialogue box and quest tracker), `crates/editor/src/dialogue_editor.rs` with `app/dialogues.rs` (the Dialogue tab). The example is `examples/hello-dialogue` (the Farmer gives an apple quest through his prompt; L switches English and Spanish), with game tests in its `tests/`.

## Dialogue files

A dialogue is `assets/dialogue/<Name>.dialogue.json`, a node graph in the same shape as a UI Event Graph or a behavior tree (`{ "format_version": 1, "nodes", "wires", "comments" }`), edited in the same graph editor. A new optional file, so `project::FORMAT_VERSION` didn't change, and built games carry it with the rest of `assets/`. White exec wires say what comes next, and may loop back (a Choice that goes back to ask again).

| Node | Does |
|---|---|
| **Start** | Where it starts. One per dialogue. |
| **Line** *Speaker, Text, Audio, Animation* | Shows Text said by Speaker (empty: the speaker Model's name) until the player goes on. Text and Speaker may be string table keys. Audio plays a sound file from the speaker (a `Sound` called `DialogueVoice`, stopped at the next line); Animation plays that state of the speaker's Animator. A Line wired straight to a Choice shows the choices under it. |
| **Choice** *Option 1–4* | Lets the player pick an option (empty ones aren't shown); each goes on from its pin. Options may be string table keys. |
| **Attribute?** *Of, Attribute, Equals* | True when the player's character (else the Player) or the speaker has the attribute and it isn't false, 0 or empty; with Equals, when it's that value (a number, true or false, or text). |
| **Has Tag?** *Of, Tag* | True when it has the tag. |
| **Data Table?** *Table, Key, Column, Equals* | The same test on a data table's cell. |
| **Quest?** *Quest, State* | True when the quest is `NotStarted`, `Active` or `Completed`. |
| **Lua Condition** *Code* | True when the expression is truthy. |
| **Set Attribute** *On, Attribute, Value* | Sets it (a number, true or false, or text). |
| **Give Item** *Item* | Puts a copy of the Tool of that name (searched in ServerStorage, then ReplicatedStorage) in the player character's Inventory. |
| **Start Quest**, **Complete Quest** *Quest* | `QuestService:Start` and `Complete`. |
| **Run Lua** *Code* | Runs it. |
| **End** *Ending* | Ends the dialogue with that name. |

Lua in a node runs in a Script's environment on the side running the dialogue, with `player`, `character` and `speaker` set, and can't wait. Blocks have no dialogue nodes yet; a ModuleScript made of blocks can be `require`d from a node's Lua.

`Dialogue::from_json` checks a file and gives every problem with its node: no Start (or a second), nothing wired to Start, an unknown kind, two wires out of one pin, a wire into or out of a pin a node hasn't got, an empty required field (Text, Attribute, Tag, Table, Key, Column, Quest, Code, Item), a Choice with no options, or an option not wired. A dialogue with problems doesn't start (`the dialogue "…" can't run: node 9: Option 2 isn't wired to what comes next`). An unwired output ends it with the ending `""`. More than 1000 nodes in a row with no Line or Choice to wait at stops it with an error (`MAX_STEPS`).

## Running one

- `DialogueService:Start(dialogue, speaker?)` runs the dialogue at that project path, said by `speaker` (a Model), waits for it to end and returns the ending. It's a Lua function (so the script can wait), and an error from a script's main thread, while another dialogue runs, or for a dialogue that can't run. One that ends before showing anything returns at once.
- A **ProximityPrompt** with `Dialogue` set starts it when triggered, with no script: the speaker is the nearest Model above the prompt (else its part). Its `Triggered` still fires first.
- `Advance()` goes on (or picks the highlighted choice), `Choose(n)` picks choice `n`, `Stop()` ends it with `""`, `Active` says whether one runs, and `GetLine()` is `{ Speaker, Text, Choices, Selected }` in the player's language, or nil.
- Talking to a speaker whose Name is a Talk objective's Target counts as the dialogue starts, before its first node, so its conditions see the result.
- One dialogue runs at a time, for the local player, on the side that runs Scripts (a replica's LocalScripts on a device following a server; it isn't sent between devices). Both sides share it.

## The player's keys and the box

`ScriptRuntime::step_prompts` (frame step 1) runs a dialogue's input instead of the prompts while one is active: Interact or UIConfirm (E, Enter, a controller's X or A) goes on or picks, UINavigate up and down (arrows, D-pad, stick) moves between choices round the ends, and 1 to 4 pick a choice. The press that triggered a prompt doesn't also go on. Meanwhile prompts don't show (`shown_prompts` is empty) and `ScriptRuntime::movement_locked` is true, so hosts give the character controller no movement (it's also true while a Sequence locks input).

Hosts draw `ui::dialogue_box_items(world, &scripts.dialogue_view(world), …)` over the game UI: the speaker's name, the wrapped line and the numbered choices, the highlighted one tinted. The view is worked out each frame, so changing `LocalizationService.Language` changes the box at once. CSS (any stylesheet the game loads):

| Rule | Styles |
|---|---|
| `DialogueBox` | The box: `background-color`, `color`, `font-*`, `padding`, `border`, `border-radius`, `opacity`, `width` (720px) and `margin-bottom` (24px). |
| `DialogueBox::key` | The speaker's name. |
| `DialogueBox::option` | Each choice. |
| `DialogueBox::option:focus` | The highlighted choice. |

`DialogueBox` and `QuestTracker` are type names for the engine's own widgets (`style::Widget`), like `Tooltip`: they take no classes, ids or descendant selectors, and their parts may be followed by `:focus`.

## Quests

`QuestService` reads two data tables ([../project/data-tables.md](../project/data-tables.md)):

- `quests`: one row per quest, by key, with `Title` and optionally `Description`.
- `quest_objectives`: one row per objective, in order, with `Quest` (the quest's key: a string or a reference to `quests`), `Kind` (`Count`, `Reach` or `Talk`), `Target`, `Count` (Count objectives; 1 when missing) and `Text` (what the tracker shows; the Target when empty).

Titles and texts may be string table keys. Objectives:

- **Count**: `QuestService:Progress(target, amount?)` adds `amount` (1) to every active quest's Count objectives with that Target, up to Count.
- **Reach**: done once the middle of the player character is inside the box of the part at the path `Target` from the Workspace (`Farm.Gate`, rotation ignored), checked every frame in `step_prompts`.
- **Talk**: done when a dialogue starts with a speaker of that Name.

A quest whose objectives are all done completes. `Start(quest)` (false if it's active or done), `Complete(quest)`, `GetState(quest)`, `GetActive()`, `GetObjectives(quest)` (`{ Key, Kind, Target, Text, Count, Progress, Done }`) and `GetTitle(quest)`; a quest that isn't in `quests` is an error listing the keys. Events, on both sides once the change has been made: `QuestStarted(quest)`, `QuestProgressed(quest, objective, progress)` and `QuestCompleted(quest)`.

**Saving**: a quest's state is player data in `save::Saves`: `Quest/<key>` is `"Active"` or `"Completed"` and `Quest/<key>/<objective>` its progress. So save slots and checkpoints keep it, `LoadSlot` puts it back, and on a server it would be the game's (not per player) data. `SaveService:GetPlayerData("Quest/apples")` reads it too.

**The quest tracker**: hosts draw `ui::quest_tracker_items(world, &scripts.quest_tracker(world), …)` at the top left: each active quest's title, then its objectives (`Apples 3/5`, `… (done)`). `QuestTracker` rules style the panel (`width` 280px, `margin-left`, `margin-top`, and the rest), `QuestTracker::key` the titles and `QuestTracker::check` the objectives that are done.

## In the editor

The **Dialogue** tab (View > Panels; starts closed) edits dialogues in the generic graph editor. It follows the selected ProximityPrompt (or the one in the selected part or Model) and shows its dialogue; its picker shows any dialogue in the project's `assets`. **New dialogue** makes `assets/dialogue/<speaker>.dialogue.json` (a number added when taken) with a Start, a Line and an End, and gives it to the selected prompt if it has none; **Use on <speaker>'s prompt** sets the prompt's `Dialogue` (one undo step). Problems show on their nodes, checked by the same code that runs it. Dialogues save with the project; each finished edit is a step of the dialogue's own undo while the pointer is over the graph. The prompt's `Dialogue` is a text box in Properties.

Tests: `script/src/dialogue_tests.rs`, `ui/src/dialogue_box.rs`, `blocks-runtime/src/dialogue.rs`, `editor/src/dialogue_editor.rs`, `editor/src/app/dialogue_tests.rs`, and `examples/hello-dialogue/tests/dialogue.spec.lua`.
