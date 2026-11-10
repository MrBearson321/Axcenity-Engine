# Editor usability audit and plan

The first pass of the usability initiative: what a beginner meets in each editor system, what established tools do about the same problems, which features to keep, change or hide, and the order to fix them in. Research on other tools is in [engine-conventions.md](engine-conventions.md); this file cites it as **[R§n]** (its section n).

- **Date:** 2026-11-10, on `main` at `320b2c5` (after PR #29).
- **Policy:** [development-policy.md](../../development-policy.md). Stability and correctness come before performance, and performance before usability, so every change here is small, local to the editor, tested, and leaves file formats and the script API alone.
- **Guiding rule:** make the engine easier to use, not less capable. Nothing in this plan removes a capability.

## How this audit was done

1. **Looked at the real editor.** A test (`editor_tour_snapshots` in `app/tour_snapshots.rs`, ignored; run it with `AXCENITY_SNAPSHOTS=<folder> cargo test -p axcenity-editor editor_tour -- --ignored`) drew the whole editor window offscreen (`EditorApp::frame` through `snapshot::snapshot_rendered`, the same path the drawing tests use) in 14 states: the start screen, an open project, a selected part, each ribbon tab, the Assets panel, and the Animation tab empty, on a rigged model's state machine, on a keyed door clip, and right after Animate. The existing `every_panel_fits_a_narrow_column_and_a_short_strip` and `the_animation_tab_says_where_you_are_and_fits_any_shape` tests saved every dock panel at 260×900 and 900×260. Findings below quote what those pictures show.
2. **Read the code behind each control:** labels, tooltips (`on_hover_text`), empty states, undo (`History`), and confirmations.
3. **Counted** interactive controls and tooltips per system, as a rough measure of density:

| System | Source lines (no tests) | Controls | Tooltips |
|---|---:|---:|---:|
| Animation Studio (`animation/`) | 13,900 | 157 | 97 |
| Model Studio | 9,900 | 86 | 46 |
| UI Designer | 10,800 | 68 | 68 |
| Menus and ribbon (`app/chrome`) | 3,300 | 57 | 87 |
| Terrain Studio | 2,600 | 46 | 41 |
| Properties | 2,300 | 36 | 42 |
| Block editor | 7,900 | 34 | 14 |
| Material tab | 2,200 | 23 | 14 |
| Sequencer | 1,300 | 21 | 16 |
| Data Table | 1,100 | 20 | 8 |
| Assets panel | 2,800 | 18 | 17 |
| Explorer | 1,000 | 18 | 15 |

**Depth.** Systems marked *inspected* were looked at in pictures and code. Systems marked *reviewed* were read in code and docs only; their findings are lower-confidence and are listed as things to look at, not as defects.

## Overall verdict

Axcenity's editor is in better shape than "a collection of disconnected tools". It already has most of the conventions the research recommends: a Roblox-style ribbon with grouped, captioned buttons; a command palette and Quick Open that also find Instances (Ctrl+Shift+P, Ctrl+P) **[R§3]**; undo for nearly every edit, including file operations on clips, prefabs and imports **[R§4]**; templates and a Learn page on the start screen **[R§6]**; empty states that name the one next action (Properties, the Animation tab, Model Studio, Tests, Sequencer); Reset Layout **[R§5]**; and plain-language tooltips on most buttons.

The problems are concentrated, not everywhere:

1. **Properties, the most-used panel, explains nothing and can't be searched.** Property names have no description on hover, even though the engine already has a one-line description for almost every property (1,454 of the 1,720 members in `definitions/axcenity.d.luau` are documented; most of the rest are enum items). There's no filter box, no way to tell a changed value from a default, and no reset. A beginner selecting a Part meets `MaterialVariant`, `BlueprintTexture`, `CastShadow`, `CanTouch` with no explanation.
2. **The Animation Studio hides its modes.** Record is a red-tinted word in a crowded toolbar; nothing else changes while it's on. With Record off, a move in the preview silently disappears when the playhead moves. The empty dope sheet tells you to "press K" even when Record is on and K isn't needed. These three are exactly the confusions the research calls out **[R§1, §9.2]**.
3. **A few inconsistencies** between places that do the same thing: the ribbon's Insert menus explain each item on hover but the Explorer's Insert menu doesn't; the Material tab resets values with ↺ but Properties can't; Terrain Studio folds rare settings under **Advanced** but Properties shows everything at once.

Fixing those is cheap, low-risk and helps every user every day, so they come first.

## A. Audit by system

Severity: **High** blocks or misleads a beginner in a common task; **Medium** slows or confuses but has a workaround; **Low** polish. Frequency: how often a typical user meets it.

### A1. Properties and inspector (*inspected*)

| # | Problem | Severity | Frequency | Beginner impact | Advanced impact |
|---|---|---|---|---|---|
| P1 | Property names have no hover description. The descriptions exist (`axcenity_script::api_reference()`, already used by the start screen's API Reference) but Properties never shows them. | High | Constant | Must guess what `Transparency` 0–1 or `CanTouch` do, or leave the editor | Minor; still useful for rare properties |
| P2 | No filter box. A Lighting or ParticleEmitter has 20–40 rows across several groups; finding one means scrolling. Roblox Studio, Godot and Unity all filter **[R§2]**. | High | Constant | Slow, and hides what exists | Slow |
| P3 | No reset to default and no mark on changed values. The Material tab has ↺ per value; Properties has ↺ only for prefab overrides. | Medium | Often | Can't undo "what did I change?" except with Ctrl+Z | Has to remember defaults |
| P4 | Rare properties sit beside common ones (Appearance shows `MaterialVariant` and `BlueprintTexture` between Material and Metalness). No Advanced fold. | Medium | Constant | Noise | None |
| P5 | `ClassName` repeats in a Data row under a header that already shows it. | Low | Constant | Noise | None |

Keep: the groups (Data, Appearance, Behavior, Part, …) match Roblox Studio and the "Other" catch-all means new properties always show; the header with icon, name and class; **Animate** and **Edit in Model Studio** at the top; the Play-mode pins; read-only reasons.

### A2. Animation Studio and controllers (*inspected*)

The tab is capable and mostly well-explained: the breadcrumb (Hero ▸ State Machine ▸ Walk), the outline, the empty state ("Animate Floor to give it an Animator and a clip, then move it in the preview with Record on"), one-click **Animate** (Animator + clip + Animation as one undo step, the "one action, auto-wired" pattern Unity uses **[R§1]**), tooltips on nearly every control, and inspector text that explains state machines and layers in a sentence each.

| # | Problem | Severity | Frequency | Beginner impact | Advanced impact |
|---|---|---|---|---|---|
| AN1 | **Record is easy to miss.** The only sign is the word Record turning red. The preview, the timeline and the playhead look the same either way. Unity tints the timeline red; Spine colours each key button by state **[R§1, §9.2]**. | High | Every session | Moves that key (or don't) without them knowing | Occasional surprise |
| AN2 | **Unkeyed changes vanish silently.** With Record off, moving something shows only "until the playhead moves" (`pending` in `property_editor.rs` and `clip_editor.rs`); then it's gone with no warning. | High | Often | Loses work, doesn't know why | Knows the rule, still loses moves |
| AN3 | **The empty dope sheet gives the Record-off instruction always:** "No tracks yet: select something in the preview, move it, and press K", shown right after Animate, which turns Record on. | Medium | Every new clip | Thinks K is needed | None |
| AN4 | **State machine parameter chips break letter by letter** in the side column: "Wave" and "Hop" render as "Wav/e" and "H/o/p" stacked vertically (seen in `tour-11-anim-noodle`). The chip's label wraps to the space left on the row. | Medium | Every state machine with 3+ parameters | Unreadable | Unreadable |
| AN5 | Jargon in the keying toolbar: **Dope sheet**, **Key selected**, **New keys: Smooth**, **Add track**. Each already explains itself on hover, so no change now; revisit only if the terms keep confusing people. | Low | Often | Must hover to learn | None |
| AN6 | A linked (take) clip's settings (range, loop, speed, events) aren't undo steps; only Revert puts the file back. Keyed clips are fully undoable. | Medium | Sometimes | Inconsistent safety net | Same |
| AN7 | The outline repeats "Linked to noodle.fbx · …" under every state; the drawer lists the Animator and Animation Instances as "Objects" next to the parts that actually move. *(Done in a follow-up: the outline names the take only; the drawer leaves them out.)* | Low | Often | Noise | None |
| AN8 | Two different panels are both titled **Parameters** in the state-machine view: the definitions beside the graph and the live values under the preview. | Low | Every state machine | Which one do I edit? | None |

### A3. Scene hierarchy, Explorer and object management (*inspected*)

| # | Problem | Severity | Frequency | Notes |
|---|---|---|---|---|
| E1 | No filter in the Explorer. Quick Open (Ctrl+P) finds Instances, but few will discover it, and it jumps to one result instead of narrowing the tree. Roblox Studio filters the Explorer in place **[R§2, §3]**. | Medium | Often in big scenes | |
| E2 | The Explorer's **Insert ▸** items have no tooltips, while the same items in the ribbon's dropdowns explain themselves (`insert_hint`). | Low | Often | Pure consistency fix |

Keep: Ctrl/Shift multi-select, rename in place, Lock/Unlock, Animate and Open in the row menu, the class colours and icons, undoable delete (no confirmation needed **[R§4]**).

### A4. General navigation, ribbon, menus, workspace (*inspected*)

Good: seven ribbon tabs that each show a few captioned groups, a command palette with fuzzy search, custom ribbon tabs, dockable and floatable panels, Reset Layout, a start screen with templates, game starters and a Learn page.

| # | Problem | Severity | Notes |
|---|---|---|---|
| N1 | The Test tab's Check group has a button captioned **Assets** with a magnifier icon. It opens the Asset Check scan, but reads like "open the Assets panel". | Low | Rename to **Check Assets** |
| N2 | The ribbon is a different height on each tab (Home is ~18 points taller than World), so the whole layout jumps when you switch tabs. | Low | Cosmetic |
| N3 | Undo has no visible history list; the Edit menu names only the next step. Unity and Blender list steps **[R§4]**. | Low | Undo itself is thorough |

### A5. Asset Browser and importing (*inspected*)

Good: one Import… for every kind of file, drag-and-drop onto the window, category chips, search, "Unused", an import dialog only when it's worth asking (`worth_asking`), with object and material tabs and a preview. This already matches the Roblox 3D Importer pattern **[R§8]**. Low-priority: the default panel height cuts the card labels off. No change proposed now.

### A6. Material and texture editing (*inspected*)

Good: Values / Graph switch (simple first, node graph for those who want it), presets (Plastic, Metal, Glass…), per-value reset ↺, plain warnings ("A base is missing or leads back around…"). This is the pattern Properties should copy for P3. No change proposed now.

### A7. Terrain editing (*inspected*)

Good: Generate with presets and a seed, sliders with numbers, an **Advanced** fold for rare landscape settings (progressive disclosure already in use). Medium: the Sculpt/Materials/Terrain tool rows are icon-only; each icon has a tooltip, but a beginner must hover every one. Consider captions in a wide panel later.

### A8. Scene editing, transforms and gizmos (*reviewed*)

Move/Resize/Rotate/Transform tools, steps for move and rotate, surface and collision snapping, a view cube, a grid and a distance reading. Mature; matches Roblox Studio. No change proposed.

### A9. Lighting and rendering (*reviewed*)

Lighting properties are split into Time of Day, Shadows, Sky, Fog and Post-processing groups; the World tab inserts lights, skies and effects from captioned dropdowns with tooltips. Lighting has many properties, so it benefits most from P1, P2 and P4.

### A10. Physics configuration and debugging (*reviewed*)

Anchored, CanCollide, CanTouch, collision groups (a dialog from the ribbon), collider and raycast overlays under View > Overlays, constraints with a surface-picking tool. Physics benefits from P1 (what does CanTouch do?); no separate change.

### A11. Sound and spatial audio (*reviewed*)

Sound properties with a Listen button, a Mixer panel, SoundGroups, reverb zones, explained in the Insert tooltips. No change proposed.

### A12. UI Toolkit / UI Designer (*reviewed*)

Artboard, anchors, rulers, styles, rich text, an Event Graph and a Live Preview. Empty state: "There's no ScreenGui to design yet" with **New ScreenGui**. Large (10,800 lines, 68 controls) and the best candidate for a dedicated pass later.

### A13. Block Coding Kit and scripting (*reviewed*)

Block palette, a Graph view, scripts and LocalScripts with autocomplete (`.axcenity/axcenity.d.luau`), Find in Scripts, a searchable API reference on the start screen. The block editor has the lowest tooltip ratio of any big system (34 controls, 14 tooltips). Its own pass is [block-editor.md](block-editor.md).

### A14. Script editor and debugging (*reviewed*)

Script tabs, the Console with a Lua prompt, F9 Developer Console in Play, the Profiler. No change proposed.

### A15. Project management (*reviewed*)

Start screen with templates and genre starters, Version History checkpoints, Back Up Project, Build Game, Settings. A restore refuses unsaved changes (good recoverability). No change proposed.

### A16. Playtesting and simulation (*inspected*)

Play/Stop in the ribbon on several tabs, a notice in Properties during Play explaining that changes are undone at Stop unless pinned, Keep changes per value. This is clearer than most engines. No change proposed.

### A17. Multiplayer and networking (*reviewed*)

The Test tab's Clients group (client count, Network menu for simulated latency/loss). Per the development policy, dedicated hosting is deferred; local multiplayer is the focus. No change proposed.

### A18. Model Studio and Sequencer (*reviewed*)

Model Studio (modeling, UVs, paint, sculpt, rig) is a second large tool with its own undo; its empty state names the next step. The Sequencer reuses the Animation Studio's timeline, so AN1–AN3 fixes should be checked there too. Model Studio's own pass is [model-studio.md](model-studio.md).

## B. Research and how it applies

Summary of [engine-conventions.md](engine-conventions.md), mapped to Axcenity:

| Convention | Who does it | Axcenity today | Adopt? | Why it helps here |
|---|---|---|---|---|
| Property descriptions on hover, from the class reference | Godot (doc comments → tooltips), Unity, Roblox (docs links) **[R§2, §6]** | Descriptions exist in `api_reference()`; Properties doesn't show them | **Yes, now** | Zero new text to write; one source of truth with the script docs, so it can't drift |
| Property filter box | Roblox Studio (Ctrl+Shift+P), Godot, Unity **[R§2]** | None | **Yes, now** | Lighting and emitters have 20–40 rows |
| Mark changed values, reset in place | Unreal, Godot **[R§2]** | Material tab only | **Yes, next** | Needs per-class defaults (`Instance::new` gives them); medium work |
| Advanced fold driven by metadata | Unreal (`AdvancedDisplay`) **[R§2]** | Terrain's Advanced fold | **Later** | Needs a per-property "advanced" list and agreement on which; low cost but a judgement call per property |
| Loud record mode | Unity (red timeline), Spine (key colours) **[R§1]** | Red word only | **Yes, now** | Mode errors are the commonest animation confusion |
| Explicit Record toggle vs always-key | Unity/Blender/Unreal/Spine vs Roblox **[R§1]** | Explicit toggle; Animate turns it on | **Keep** | The toggle is the more common model and Animate already starts with it on |
| Warn about unkeyed changes | Spine (orange "changed, not keyed") **[R§1]** | Silent drop | **Yes, now** | Stops silent loss without changing behaviour |
| Separate clip editing from state machines | Unity, Godot, Unreal **[R§1]** | Already separate (breadcrumb, outline) | **Keep** | |
| One auto-wired action in empty states | Unity's Create **[R§1, §6]** | Animate does exactly this | **Keep** | |
| Filter the hierarchy in place | Roblox Studio **[R§3]** | Quick Open only | **Next** | Moderate work: the tree needs a "matches or has a matching descendant" pass |
| Visible undo history | Unity, Blender **[R§4]** | Edit menu label | **Later** | Nice; undo already reliable |
| Context help on F1 | Blender, Godot, Unity **[R§6]** | API reference on the start screen | **Later** | P1 covers most of the need first |
| Task workspaces | Blender, Godot **[R§5]** | Ribbon tabs + dock tabs | **Keep** | Ribbon tabs already scope the tools; a second workspace system would duplicate them |
| Guided tours | (avoided by all five) **[R§6]** | None | **Don't add** | Empty states and tooltips do the job without interrupting |

## C. Feature evaluation

| Feature | Decision | Reasoning |
|---|---|---|
| Ribbon tabs and captioned groups | **Keep** | Clear, Roblox-familiar, customizable |
| Command palette and Quick Open | **Keep** | Covers discoverability for experienced users |
| Custom ribbon tabs | **Keep** | Costs beginners nothing (hidden behind +) |
| Start screen templates, game starters, Learn | **Keep** | Matches research on onboarding |
| Properties groups and "Other" | **Keep** | Matches Roblox; new properties always show |
| Properties without descriptions or filter | **Improve** (P1, P2) | The data exists; the UI doesn't use it |
| Properties showing every property at once | **Defer** rare ones (P4, later) | Hide behind an Advanced fold, never remove |
| `ClassName` row under the header | **Simplify** (later) | Repeats the header; keep it searchable |
| Reset values | **Consolidate** (P3, next) | One ↺ convention across Material tab and Properties |
| Animate (one step to a playable clip) | **Keep** | The research's best pattern, already built |
| Record toggle | **Improve** (AN1) | Keep the model, make it visible |
| Pending (unkeyed) preview moves | **Improve** (AN2) | Keep the behaviour, say what will happen |
| Dope sheet + Curves | **Keep** | Standard pair; Curves is one click away, not in the way |
| New keys easing picker | **Keep** | Already explained on hover; default Smooth |
| IK chain, pins, snaps, world/local | **Keep** | Advanced, but tucked beside the posing tools and off the beginner path |
| Linked (take) clips vs keyed clips | **Keep**, **Improve** undo later (AN6) | Two real workflows (imported mocap vs hand-keyed); the badge names them |
| State machine, layers, blend trees, parameters | **Keep**, **Improve** chip layout (AN4) and naming (AN8) | Separate from clip editing already |
| Explorer Insert menu | **Consolidate** tooltips with the ribbon's (E2) | Same items, same explanations |
| Explorer filter | **Improve** (E1, next) | |
| Asset Check under Test ▸ Check | **Improve** label (N1) | |
| Floating panels in OS windows | **Keep** | Off the beginner path |
| Debug menu | **Keep** | Debug builds only |
| Anything to **Remove** | **None found** | Every feature looked at serves a real workflow; the complexity problems are presentation, not surplus features. Removal would cost capability for little gain. |

## D. Prioritized plan

Complexity: S (hours, one module), M (a day, a few modules), L (several days or cross-cutting). Risk is regression risk to existing behaviour.

### High priority (do now)

| ID | Change | Systems | Complexity | Risk | Depends on |
|---|---|---|---|---|---|
| U1 | **Record is loud**: while Record is on, a red frame around the preview and a red "● Recording: moves set keys at <time>" line above the timeline | Animation (keyed and linked clips) | S | Low: drawing only | — |
| U2 | **Unkeyed changes are named**: when the preview has pending moves and Record is off, show "Not keyed yet: moving the playhead drops this. Key it (K)" with a button that keys it | Animation | S | Low: reads existing `pending`; the button calls the existing K path | — |
| U3 | **Right empty-dope-sheet hint**: with Record on, "move it in the preview to set a key"; with it off, "…and press K" | Animation | S | Very low | — |
| U4 | **Parameter chips never break mid-word**: start a new row when a chip doesn't fit, and truncate a name wider than the column | Animation | S | Low | — |
| U6 | **Property descriptions on hover** from `api_reference()` | Properties | S | Low: read-only lookup, cached once | — |
| U7 | **Filter properties** box at the top of Properties: matches property names (and group titles), shows only matching rows, keeps groups open while filtering | Properties | M | Low–medium: must not change edit routing; test that edits still apply while filtered | — |
| U8 | **Explorer Insert tooltips** reuse the ribbon's `insert_hint` | Explorer | S | None | — |
| U9 | **Check Assets** caption | Ribbon | S | Very low: the palette test checks the command list | — |

### Medium priority (next)

| ID | Change | Complexity | Risk | Depends on |
|---|---|---|---|---|
| U10 | Mark properties that differ from the class default, with ↺ to reset (one undo step) | M | Medium: defaults per class and per value kind | U7 (same row code) |
| U11 | Explorer filter: narrow the tree to matching names and their ancestors | M | Medium: tree drawing, selection with hidden rows | — |
| U12 | Take-clip setting changes as undo steps (AN6) | M | Medium: touches clip save and history | — |
| U13 | Rename the state machine's two Parameters panels ("Parameters" and "Try parameters") (AN8) | S | Low | — |
| U14 | Same Record and pending feedback in the Sequencer, which reuses the dope sheet | S | Low | U1–U3 |

### Low priority (later, or in dedicated passes)

| ID | Change | Complexity | Risk |
|---|---|---|---|
| U15 | Advanced fold in Properties for rare properties (a short per-class list) | M | Low, but needs agreement on the list |
| U16 | Drop the duplicate `ClassName` row (keep it findable by the filter) | S | Low |
| U17 | Undo History list under Edit | M | Low |
| U18 | Captions under Terrain Studio's tool icons when the panel is wide | S | Low |
| U19 | Steady ribbon height across tabs | S | Low |
| U20 | Dedicated passes on the UI Designer, Block editor and Model Studio, with the same screenshot method | L | — |

**Not planned:** guided tours, a second workspace system, renaming properties away from their Roblox-familiar names (the script API uses them; descriptions explain them instead), removing any feature.

### Dependencies

U1–U4 are independent of U6–U9, so they ship as two PRs: **Animation Studio** first, then **Properties and Explorer**. U10 builds on U7's row code. U14 follows U1–U3. None of these touch the schedule (M1), the draw list (M4), physics sync (M3) or Lua hot paths, which other work is changing now.

## E and F

Implementation and verification are recorded in [verification.md](verification.md) as each PR lands.
