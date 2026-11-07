# Cutscene sequences

Phase 51. A sequence is a timeline of tracks in an `assets/sequences/<name>.sequence.json` file, made in the editor's **Sequencer** tab and played by a **Sequence** Instance. Like Unreal's Sequencer or Unity's Timeline.

## The file

`anim::sequence::SequenceFile` (`crates/anim/src/sequence.rs`), format 1, written with sorted keys and empty lists left out (`to_json`; `examples/hello-cutscene` is canonical). `from_json` checks every section starts before it ends, inside `0..duration`.

| Field | What it is |
|---|---|
| `duration` | Seconds from start to end. |
| `camera_cuts` | `{ id, time, camera }`: from `time` to the next cut, the view is the bound Camera's `Position`, `Orientation` and `FieldOfView`. Bound to a Spline (Phase 73), the camera rides it from its start at `time` to its end at the next cut (or the end), facing along it ([../architecture/splines.md](../architecture/splines.md)). An empty `camera` uses the sequence's own camera keys. Before the first cut the camera is the game's. |
| `tracks` | Property tracks, the same shape as a clip's (keys, easings, Bezier handles, `sample_track`). `target` is a binding; the target `@Camera` holds the sequence's own camera keys (`Position`, `Orientation`, `FieldOfView`). A Model's `Position` and `Orientation` are its pivot, as in clips. |
| `animations` | `{ id, start, end, target, clip, blend_in, blend_out }`: the clip (`*.anim.json`) plays from its start on the bound Animator, or the Animator inside the bound Instance, its weight rising over `blend_in` and falling over `blend_out` seconds. |
| `audio` | `{ id, start, end, sound }`: the bound Sound plays from its start at `start` and stops at `end`. |
| `events` | `{ time, name }`: fired as `EventReached(name)` when passed. |
| `fades` | `{ id, start, end, from, to, color }`: the screen covered in `color` from `from` (0 clear, 1 covered) to `to`; after it, held at `to` until the next fade. |
| `letterbox` | `{ id, start, end, size, ramp }`: bars at the top and bottom, each `size` of the screen's height (0.12), sliding in and out over `ramp` seconds. |
| `subtitles` | `{ id, start, end, key, text }`: a line at the bottom showing the string table's `key` in the player's language (a TextLabel's `TextKey`), or `text` when the table hasn't got it. |
| `levels` | `{ id, start, end, level }`: the level is added beside the current one for the section (`LevelService:AddLevel`); one that reaches the end stays added. |

**Bindings** find what a track moves: a path of names from `game` (`Workspace/Door`), or `tag:` and a tag (`tag:Hero`), the first Instance in tree order with that tag, so placed prefab copies can be bound. A binding that finds nothing is warned about once.

## Playing

`SequenceRun::apply(world, time)` puts a world where the sequence has it at `time`, whatever came before: keyed properties (through `apply_evaluated`), each animation section's clip loaded on its Animator through an Animation outside the tree and its track held at the section's time (speed 0, the section's weight), and the overlay, a ScreenGui named `Cutscene` in the Workspace (`DisplayOrder` 1000) with `Fade`, `TopBar`, `BottomBar` and `Subtitle`. It returns the camera shot, the Sounds to start and stop, and the levels that should be added; `finish` lets go of the clips, the overlay and the Sounds (keyed properties stay). The editor's preview and the game call the same code, so they show the same at the same time.

The **Sequence** Instance (`script/src/sequence.rs`): `SequenceId` (the file), `AutoPlay` (plays when the game starts), `Skippable` (on) and `LockPlayerInput` (on).

- `Play()` starts it from the start, or carries on after `Pause()`. `Stop()` lets go of it and goes back to the start: its clips, Sounds, overlay and levels go and the camera comes back; keyed properties stay and `Completed` doesn't fire. `TimePosition` reads where it is and jumps when set (applied at the next step, events between not fired); `IsPlaying` and `Length` read.
- `EventReached(name)` fires as it passes each event, and `Completed` once at its end; then the camera comes back.
- Both sides share the playbacks. `ScriptRuntime::step` moves them on after both sides' scripts (frame step 2), so the Animators sample this frame's time. While a cut is in charge the client's `workspace.CurrentCamera` is held Scriptable at the shot, and given back as it was after.
- **Skipping**: with `Skippable`, a press of the Menu action (Escape, ButtonStart) while it plays jumps it to its end: applied at `duration`, so everything it keys is at its last values, then let go of and `Completed` fired once. The events skipped over aren't fired. The press goes no further.
- **Locked input**: while a Sequence with `LockPlayerInput` plays or is paused, `ScriptRuntime::input_locked` is true. Hosts give the character controllers no movement (the player's `controller_input`, the editor's `Session::frame`), and `input_events` passes only releases to scripts and prompts, so nothing stays held. Control comes back when it ends, is skipped or stops.

## The Sequencer tab

`crates/editor/src/sequencer.rs`, its actions in `app/sequencer.rs`. Pick a sequence or type a name for a new one (one undo step, `History::record_asset`); the Assets panel's Sequence tiles open there too, and **Insert** puts a Sequence that plays it in the Workspace.

- The Animation tab's pieces: its timeline (playhead, events, frame ticks) and the property editor's dope sheet and Curves view (`PropertyDoc`) over the sequence's keys. **Add track** and **Key selected** (K) key the Explorer's selection as the preview shows it; **Key camera** keys `@Camera` where the Scene view's camera is.
- Under the timeline, a row per track kind; **Add section** adds one at the playhead, bound to the Explorer's selection (or by its first tag, with **Bind by tag**). Drag a section to move it or its ends to resize it, and set the picked one below.
- **Preview** (on when a sequence opens) shows the sequence at the playhead in the Scene view without Play: a copy of the world, rebuilt when the edit history or the sequence changes, applied, posed by its own Animators and seen from the camera cut, with no tools. Its Sounds play while the preview plays, not while scrubbing. The tab closing, or Play, hides it.
- Each change is written to the file once the mouse is let go, as one undo step ("Edit Intro"); undo and redo show the file as it is.

## Blocks

"play cutscene *sequence*" is `sequence:Play()` and "when cutscene *sequence* finishes" is `sequence.Completed:Connect(function() … end)` ([../scripting/blocks.md](../scripting/blocks.md)). `:Play()` switched from Code back to Blocks reads as "play sound", which is the same Lua.

## Not yet

A built game reads sequences from `assets/`, so Build Game packs them; Asset Check doesn't check `SequenceId` or bindings yet. Sounds start from their beginning wherever the playhead is. One Instance per tag binding.
