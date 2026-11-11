---
description: "Plays a cutscene: a *.sequence.json file made in the editor's Sequencer tab, with camera cuts, keyed properties, clips on Animators, Sounds, events, fades, letterbox bars, subtitles from the string table and levels added for a while."
---

# Sequence

```luau
Instance.new("Sequence")
```

Plays a cutscene: a `*.sequence.json` file made in the editor's Sequencer tab, with camera cuts, keyed properties, clips on Animators, Sounds, events, fades, letterbox bars, subtitles from the string table and levels added for a while. Play, Pause and Stop it; EventReached fires as it passes each event and Completed once at its end. With Skippable, the Menu action jumps it to its end; with LockPlayerInput, the player can't move or use the game's input while it plays.

It also has every member of [Instance](Instance.md).

### Properties

#### SequenceId

```luau
Sequence.SequenceId: string
```

The sequence file inside the project folder, such as "assets/sequences/Intro.sequence.json".

#### AutoPlay

```luau
Sequence.AutoPlay: boolean
```

If true, it plays when the game starts.

#### Skippable

```luau
Sequence.Skippable: boolean
```

If true, the Menu action (Escape, or Start on a controller) jumps it to its end while it plays: what it keys is left at its last values and Completed fires once.

#### LockPlayerInput

```luau
Sequence.LockPlayerInput: boolean
```

If true, the character doesn't move and scripts and prompts get no presses from the player while it plays or is paused. Control comes back when it ends or stops.

#### TimePosition

```luau
Sequence.TimePosition: number
```

Seconds from its start. Set it to jump there (the events between aren't fired).

#### IsPlaying

```luau
Sequence.IsPlaying: boolean
```

True from Play until it's paused, stopped or ends (read-only).

#### Length

```luau
Sequence.Length: number
```

Its file's duration in seconds (read-only; 0 if the file can't be read).

### Events

#### Completed

```luau
Sequence.Completed: Signal
```

Fires once when it reaches its end, or is skipped there. The handler gets no arguments.

#### EventReached

```luau
Sequence.EventReached: Signal
```

Fires with an event's name as it passes the event on its Events track. Events skipped over aren't fired.

### Functions

#### Play

```luau
Sequence:Play(): ()
```

Plays it: from its start if it hasn't played, has ended or was stopped; from where it is if it's paused.

#### Pause

```luau
Sequence:Pause(): ()
```

Holds it where it is, with the camera and everything it keys still there. Play carries on.

#### Stop

```luau
Sequence:Stop(): ()
```

Stops it and goes back to its start: its clips, Sounds, fades, subtitles and levels go, and the camera comes back. Keyed properties stay where they are. Completed doesn't fire.

