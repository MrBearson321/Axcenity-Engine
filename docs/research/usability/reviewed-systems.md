# Usability pass: the systems the audit only reviewed

The [usability audit](usability-audit.md) *inspected* Properties, the Animation tab, the Explorer, the ribbon, the Asset Browser, materials and terrain, and gave the UI Designer, the Block editor and Model Studio passes of their own. The rest (A8–A11, A13–A17) were only *reviewed* from the code. This pass looks at them the same way as the others: pictures of the real editor, then the code behind what looked wrong. **[R§n]** is section n of [engine-conventions.md](engine-conventions.md). What each change was checked with is in [verification.md](verification.md).

Pictures: tour shots 60–70 (`TOUR_ONLY=- AXCENITY_SNAPSHOTS=<dir> xvfb-run -a cargo test -p axcenity-editor editor_tour -- --ignored`, `app/tour_snapshots.rs`): the script editor, Lighting and a Sound in Properties, the Mixer, Collision Groups, a part's physics, Settings, Version History, Input, Tests and the Profiler.

## Verdict

These systems are in good shape. Each window opens with a sentence that says what it's for and where its data goes (Collision Groups, Version History, Input). The panels that have nothing to show say what to do (Tests: "Add a file ending in .spec.lua under tests/"; Profiler and Mixer: "Press Play…"). The script tab has the Code, Blocks and Graph switch with "the script runs the same either way". The fixes needed were about **safety**, not about layout: one place where *looking* changed the project, and two settings windows whose edits couldn't be undone.

## Findings

| # | System | Problem | Severity | Change |
|---|---|---|---|---|
| RS1 | Lighting (A9), Properties | **Selecting Lighting changed it.** The title bar showed the project as unsaved and Edit ▸ Undo said "Change GeographicLatitude of Lighting" without anything being touched. The latitude slider clamps with `SliderClamping::Always`, which in egui writes the value back on every frame, rounded to the slider's one decimal (14.036° → 14.0°). | High: every project has Lighting; a beginner is asked to save changes they never made, and undo has a step they didn't do | Clamp edits only (`SliderClamping::Edits`), as the other sliders do. A new test shows one Instance of every class in every example and checks nothing changed, so no widget can do this again unnoticed |
| RS2 | Input window (A15) | **Restore defaults wipes every custom action in one click**, and none of the window's edits (remove a binding, delete an action) could be undone: they write `input.json` at once. Everything else in the editor undoes. | Medium: rare, but it destroys work silently | Each edit is an undo step; undo writes the file back and the editor reads it again |
| RS3 | Collision Groups (A10) | The same: removing a group (and the pairs set for it) couldn't be undone | Medium | Each edit is an undo step, as above |
| RS4 | Mixer (A11) | The whole output's meter was called **Master**, and most projects' top SoundGroup is called Master too (the `hello-mixer` example's is), so the panel listed Master twice | Low | The output meter is **Output**, with a hover saying it's everything the game plays |

**Seen, not changed**:
- The Input window offers an axis action **Add…** (a menu of sticks and key sets) but a button action **Press a key…** and **Add a button…**. They are different kinds of input, so the different controls are right.
- Properties in the default 340-point column cuts long names ("GeographicLa…"). The hover shows the whole name and its description (U6), and the column can be widened.
- The script tab doesn't say whether the script runs on the server or a client; the Explorer's icon and the file name (`.server.lua`) do. Fine for now.
- Multiplayer (A17): unchanged by policy (local multiplayer first). Gizmos (A8): mature, matches Roblox Studio; not re-shot.

## How the research applies

- **Undo, not confirmation dialogs.** Every engine in R§4 relies on undo for safety, and the research's takeaway is that undo, not dialogs, is the safety net **[R§4, §9.5]**. The editor already undoes file operations (clips, prefabs, imports), so the two settings windows now do too, without a "Are you sure?" in front of Restore defaults.
- **Looking must never edit.** Unity, Godot and Unreal mark a scene dirty only on a change **[R§4]**; a false unsaved mark teaches people to ignore the real one.
- **One name per thing.** Unity's mixer calls its root group Master and has no second Master; here the root is the output, so it's named for what it is.

## Left for later, as decided before

- **BL7** (palette blocks cut off on the right): the Block editor pass chose not to widen the palette; the hover shows the whole block's Lua.
- **Context help on F1**: the audit put it after hover descriptions (U6), which cover most of the need.
