# Engine Editor Usability Conventions

How Unity, Unreal Engine, Godot, Roblox Studio and Blender (plus Spine and Figma where they help) make their editors usable for beginners without losing power, and the reasons their docs give. This is input for the Axcenity editor usability audit.

- **Date:** 2026-11-10
- **Method:** official manuals and reference pages were fetched and read. Each claim links to the page it came from. Nothing was installed or tried in the tools themselves.
- **Versions:** current Unity 6 / Unreal 5 / Godot 4 / Roblox Creator Hub pages, except where a page did not render for the fetcher. For those, an older version of the same official page was read (Blender 3.6/4.0, some Unity 2018–2022 pages, some Unreal 4.26 pages). Older pages are marked with their version in the link, and the UI may have changed since.
- **Scope:** this file reports what the tools do and why. It does not map anything to Axcenity code. Each "Translates to Axcenity?" note stays general on purpose.

**Evidence markers**

| Marker | Meaning |
|---|---|
| **[F]** | Read on the official page linked |
| **[M]** | Official doc text, read through an unofficial mirror of the same docs because the official page did not render. Check before quoting |
| **[U]** | Not verified from a primary source (general knowledge, or the page did not say). Confirm before you rely on it |

---

## 1. Animation editing for beginners

### What each tool does

| Tool | Entry point and empty state | Record / auto-key labeling | "Create a clip and play it" | Clip authoring vs. state machines |
|---|---|---|---|---|
| **Roblox Animation Editor** | Opened from the **Avatar** tab or **Window › Avatar** [F][r-ae]. The tutorial opens it with the Avatar tab's **Clip Editor** button [F][r-aetut]. "You can only create animations using **rigs**", and if there is no rig you insert one with the Rig Generator [F][r-ae]. You select the rig in the viewport or Explorer [F][r-ae]. The exact empty-state prompt text is not documented [U]. | No documented auto-key toggle. Moving or rotating a part **always** adds a keyframe at the scrubber for that part [F][r-ae]. The editor "automatically detects and removes unnecessary keyframes" [F][r-ae]. | Select rig, pose, move scrubber, pose again, then press Play or Space. Studio "automatically creates keyframes in between those poses" [F][r-aetut]. The default length is 1 s at 30 fps, shown as `seconds:frames` [F][r-ae]. | Clips are authored in the editor. Runtime playback is a separate `Animator` instance under a Humanoid or AnimationController, and `LoadAnimation` returns an AnimationTrack [F][r-animator]. Roblox has no built-in visual state machine [U]. |
| **Unity Animation window** | When the selected GameObject has no clips, a **Create** button appears in the center of the window [F][u-newclip, 2018.4]. Saving the new clip makes Unity create an Animator Controller, add the clip as the default state, add an Animator component and assign the controller [F][u-newclip]. Stated reason: "All the required elements of the animation system are now set up" [F][u-newclip]. | A red-circle button turns on **record mode**. "The Animation window time line is tinted red when in record mode", and animated properties get a red background in the Inspector [F][u-rec]. A separate **Preview** mode tints properties blue and does *not* auto-create keys [F][u-rec]. | Create, then Record, then change values. Alternatively, use **Add Property** ("a pop up list of the GameObject's animatable properties"), then K (key all animated) or Shift-K (key all modified) [F][u-rec]. | Clips live in the Animation window. The **Animator Controller** is a separate asset, a state machine described as "a flow-chart of Animation Clips and Transitions" [F][u-ac]. "Even if you just have a single animation clip, you still need to place it into an Animator Controller" [F][u-ac]. |
| **Godot AnimationPlayer** | Clicking an AnimationPlayer node opens the Animation panel at the bottom [F][g-anim]. You create a clip from the **Animation** button, then **New** [F][g-3danim]. | No record/auto-key button appears in these tutorials. Instead, "a small keyframe button" shows next to every property in the Inspector, and clicking it adds a track and a key at the time cursor [F][g-anim][g-3danim]. Whether Godot 4 has an auto-key toggle was not verified [U]. | New animation, set the length, click the key icons, then turn on **Autoplay on Load** and the looping arrows [F][g-3danim]. | A separate **AnimationTree** node does blending. It "does not contain its own animations" and plays clips from an AnimationPlayer [F][g-tree]. Stated reason: blending through AnimationPlayer "is relatively limited" [F][g-tree]. |
| **Blender** | Animation is a workspace tab (see section 5). | The Timeline has an **Auto Keying** record button that adds or replaces keys "when you transform it in the 3D Viewport". It is limited to transform properties, and its modes are *Add & Replace* and *Replace* [F][b-timeline, 3.6]. | Press **I** in the viewport to get a menu of what to key, or hover any property and press **I**. Alt-I deletes keys and Shift-Alt-I clears them [F][b-kf, 3.6]. | The NLA and Actions are separate editors [U]. *Layered Recording* adds an NLA strip per pass "to allow non-destructive tweaking" [F][b-timeline]. |
| **Unreal Sequencer / Animation Blueprint** | Not researched for its empty state [U]. | The Sequencer toolbar has an **Auto Key** button, with modes Key All / Key Group / Key Changed [F][ue-keys]. Caveat: "The track you are auto keying must already have keyframes present" [F][ue-keys]. | Add Key button on a track, **Enter** to key the selected tracks, or **S** to key the transform of the selected actor [F][ue-keys]. | Animation Blueprints split the **EventGraph** (computes variables on the game thread, "use the EventGraph as little as possible") from the **AnimGraph** (evaluated every frame, possibly on a worker thread, ending in **Output Pose**) [F][ue-abp]. State machines live inside the AnimGraph. Stated reason: "This makes it easier to create complex animation blending" [F][ue-sm]. |
| **Spine** (2D) | **Setup mode** builds the skeleton and **Animate mode** designs animations. You switch with an icon at the top left of the viewport [F][spine-ui]. | Auto Key "on the main toolbar"; "care must be taken not to accidentally create unwanted keys" [F][spine-keys]. Each transform field has a **key button colored by state**: green means no key at this frame, orange means changed but not keyed, red means keyed [F][spine-keys]. | Change a value, then click its key button, or press K to "key all changed properties" [F][spine-keys]. | Not covered. |

Other related behavior:
- Godot's **RESET** animation holds the default pose. Stated reason: "to ensure that the default pose is restored when you save the scene and open it again" [F][g-anim]. RESET tracks are the default choice when keying [F][g-3danim].
- Unity separates the **Dopesheet** view ("a more compact view ... each property's keyframe sequence in an individual horizontal track") from the **Curves** view ("great control over viewing and editing the values"), with a toggle at the bottom of the property list [F][u-animwin, 2022.1 via docs.unity.cn].
- Unity's Animation window follows the selection. A **Lock** button pins it to one GameObject [F][u-animwin].
- Unity separates **Transitions** (state to state over time) from **Blend Trees** ("smoothly blend multiple animations ... at varying degrees", for motions "of similar nature and timing") [F][u-blend].
- Roblox loop caveat: looping "doesn't interpolate between the final and first keyframes", so you copy the first key to the end [F][r-ae].

### Advantages and tradeoffs

- **Always-key (Roblox)** gives the shortest path: posing *is* keying. The tradeoff is that you cannot try a pose without creating a key. Roblox offsets this with automatic redundant-key cleanup [F][r-ae].
- **Explicit record toggle (Unity, Blender, Unreal, Spine)** makes it obvious when edits become keys. The cost is a mode. Every tool that has one adds a strong visual tint (Unity's red timeline and red properties) or warns about stray keys (Spine) [F][u-rec][spine-keys]. Unity's separate **Preview** state lets users scrub without keying [F][u-rec].
- **Per-property key buttons (Godot, Spine, Blender's I-over-property)** make "what is animatable" visible where the value lives. Spine's three-color button also shows unkeyed changes, which is the best-documented safeguard against lost work [F][spine-keys].
- **Auto-setup (Unity)** removes three wiring steps for beginners. The cost is a hidden coupling: even one clip needs a controller [F][u-ac]. The docs still recommend learning how the pieces connect [F][u-newclip].
- **Separating clips from state machines/blending** is universal (Unity controller asset, Godot AnimationTree, Unreal AnimGraph). The stated reasons are blending power (Godot), managing complexity (Unreal) and performance/threading (Unreal) [F][g-tree][ue-sm][ue-abp].

**Translates to Axcenity?** Yes, largely. (a) The empty state should offer one primary action (Unity's centered **Create**) and, if no rig/animator exists, create or wire it automatically. (b) Pick one record model and label it loudly: a red timeline tint plus tinted properties when recording, or Roblox-style always-key with redundant-key cleanup. Don't mix the two silently. (c) Spine-style per-property key state (none / changed-unkeyed / keyed) is a cheap, high-value cue. (d) Keep controllers, state machines and blend trees in a separate view or asset from clip editing, but make "one clip, play it" work without opening it, through Godot-style Autoplay/Loop toggles on the clip.

---

## 2. Inspector / property organization

| Feature | Unity | Unreal | Godot | Roblox | Blender |
|---|---|---|---|---|---|
| **Grouping** | One foldout per component. Context/kebab menu per component [F][u-comp]. | Categories, nestable with `|` in code [F][ue-spec]. Some categories are custom widgets (Transform, Materials) [F][ue-details]. | Grouped by class, with sections inside each class [F][g-insp]. | Sections (e.g. Appearance, Transform). Collapse state "remains ... for other objects of the same class" [F][r-props]. | Panels (not researched in depth) [U]. |
| **Progressive disclosure** | Foldouts [U for details]. | `AdvancedDisplay` puts a property in "the advanced (dropdown) section", and `SimpleDisplay` shows it "without opening the 'Advanced' section" [F][ue-spec]. A Visibility Filter can "show or hide modified or advanced properties" [F][ue-bpdetails, 4.26]. The down-arrow at the bottom of a category is not described on the pages read [U]. | **Expand All / Collapse All / Expand Non-Default** [F][g-insp]. **Favorite Property** pins a property to the top for that class [F][g-insp]. | "Some properties are collapsed by default" behind a small arrow [F][r-props]. | n/a |
| **Search** | Add Component browser has search [F][u-comp]. Inspector property search [U]. | "Search Details" filters as you type [F][ue-details]. | Filter box, case-insensitive, live [F][g-insp]. | **Filter Properties**, shortcut Ctrl+Shift+P [F][r-props][r-short]. | Menu Search (section 3). |
| **Reset to default** | Component **Reset** (context menu). The manual defines it as restoring values "before the most recent editing session" [F][u-comp], and the Presets page says Reset applies default presets [F][u-preset]. Per-property reset [U]. | When a value differs from default, "an indicator is displayed", and clicking it resets [F][ue-details]. | A **revert icon** on every non-default value [F][g-insp]. | Only class-level **Reset Default [Instance]** for configurable defaults [F][r-props]. | Hover a property and press Backspace. The 3.6 manual says this "clears the value to zero" [F][b-keys, 3.6]. A context-menu "Reset to Default Value" exists [U]. |
| **Tooltips** | `[Tooltip("…")]` on fields [F][u-tooltip]. | `ToolTip` metadata [U]. | Hovering shows the description and how to use the property from script [F][g-insp]. `##` doc comments on exported vars become tooltips [M][g-doccomments]. | Not documented on the page read [U]. | Tooltip shows description, shortcut, value, and why a field is disabled (red) [F][b-help, 4.0]. |
| **Docs link** | **?** icon on each component header opens its reference page [F][u-comp]. | n/a [U] | Right-click a class, then **Open Documentation** [F][g-insp]. | Explorer class names link to reference pages [F][r-expl]. No property-level link found [U]. | F1 or right-click **Online Manual** on any control [F][b-help]. |
| **Other** | Edits made in Play Mode revert on exit, which allows fast experiments [F][u-comp]. | **EditCondition** greys out a property, often with an inline checkbox [F][ue-spec][ue-details]. Lock, Property Matrix [F][ue-details]. | Object history arrows (`<` / `>`) [F][g-insp]. | Attributes and Tags edited inline [F][r-props]. | Ctrl-Wheel steps a value, Ctrl/Shift while dragging [F][b-keys]. |

**Advantages and tradeoffs.**
- Showing reset per property, *only when the value is non-default* (Unreal, Godot), doubles as a "what did I change?" signal.
- Godot's **Expand Non-Default** and Unreal's "show modified" filter answer the same question at panel scale.
- Unreal's Advanced split depends on authors tagging properties well. Mis-tagged properties hide things beginners need [U].
- Remembering collapse state per class (Roblox) cuts repeated clicking. The cost is surprise when a section stays closed on a new object.
- Linking tooltips to docs (Godot doc comments, Unity `[Tooltip]`) keeps help text in one place.

**Translates to Axcenity?** Yes. In priority order: a filter box at the top of Properties with a focus shortcut (Roblox uses Ctrl+Shift+P); a reset glyph shown only on non-default values; tooltips taken from the same metadata as the scripting API docs; a per-category "Advanced" fold driven by property metadata; collapse state remembered per class; and a docs link on the class header.

---

## 3. Discoverability

| Tool | Command search | What it covers | Notes |
|---|---|---|---|
| **Blender** | **Menu Search**, F3 [F][b-search, 3.6] | Operators reachable from menus. The results show the menu path [F][b-search]. | A separate Operator Search (Developer Extras) finds operators "not exposed in a menu" and is meant for Python developers [F][b-search]. |
| **Unity** | **Search**, Ctrl+K, or **Edit › Search › Search All**, or the toolbar magnifier [F][u-search] | Assets, packages, Asset Store, logs and more. Has a visual query builder, Tab autocomplete and saved queries [F][u-search]. | Its own Ctrl+Z/Y undoes query edits [F][u-search]. |
| **Godot** | **Command Palette**, Ctrl+Shift+P [M][g-keys], or **Editor › Command Palette** [F][g-palette] | Holds "all the available Commands and their shortcuts text" [F/M][g-palette]. Plugins can add commands [M][g-palette]. | Also Quick Open (Shift+Alt+O) and Search Help (F1) [M][g-keys]. |
| **Roblox** | **Quick Open** (Ctrl+P) finds instances. **Quick Open Actions** (Alt+Ctrl+P) finds "any Studio action" [F][r-short] | Instances and actions | The Explorer filter has a query syntax (`is:Class`, `tag:`, `Parent.Child`, property `=`) [F][r-expl]. Every shortcut is customizable [F][r-short]. |
| **Figma** | **Actions**, Ctrl/Cmd+K [F][figma] | Commands, plugins and widgets, asset search | Stated purpose: "a fast and easy way to access features" [F][figma]. |

**Tooltips that show shortcuts.** Blender's tooltip includes "a keyboard or mouse shortcut associated to the tool" [F][b-help]. Godot's palette shows shortcut text next to each command [F/M][g-palette]. Unity, Unreal and Roblox tooltip shortcut display was not verified [U].

**Context menus.** Roblox's Explorer menu is "contextually adjusted for the object type" [F][r-expl]. Unity's component menu holds Reset, Remove, Move Up/Down and Copy/Paste values [F][u-comp]. Godot's inspector menu holds Copy Property Path, Favorite and Open Documentation [F][g-insp]. Blender's property context menu holds Insert Keyframe and Online Manual [F][b-kf][b-help]. Unreal places actors from a right-click **Place Actor** menu in the viewport [F][ue-place].

**Status-bar hints.** Blender's status bar shows mouse-button and tool keymap hints on the left, and tapping Alt reveals tool hotkeys [F][b-status].

**Advantages and tradeoffs.** A palette that lists commands with their shortcut and menu path teaches the menus and keys as you use it (Blender shows paths, Godot shows shortcut text). Blender keeps operators that are not in any menu out of beginner search and reserves them for developers. Unity's Search is powerful but is closer to a database query tool than a command palette.

**Translates to Axcenity?** Yes, and it is cheap if the editor has a central action registry. Build one palette (Ctrl+Shift+P is used by Godot, and Roblox uses Alt+Ctrl+P / Ctrl+P) that lists each action with its shortcut and its ribbon/menu location. Show the shortcut in every button tooltip. Keep debug/internal actions behind a developer toggle, as Blender does.

---

## 4. Feedback and recoverability

| Tool | Undo history UI | Notes |
|---|---|---|
| **Unity** | **Window › General › Undo History**, or the toolbar Undo icon. Selecting an entry reverts to that point. Redoable entries are marked with a red line [F][u-undo]. | Sort order (newest first or last) is configurable [F][u-undo]. |
| **Blender** | **Edit › Undo History**. The top of the list is the most recent action, and "once you do make a new change, the Undo History is truncated at that point" [F][b-undo, 3.6]. | **Adjust Last Operation** panel (or F9) lets you "tweak the parameters of an operator after running it" [F][b-undo]. Repeat Last is Shift-R [F][b-undo]. |
| **Unreal** | **Edit › Undo History** "Open a dialog with prior Undo operations" [F][ue-menu, 4.26]. | n/a |
| **Godot** | Each scene has its own undo history, managed by `EditorUndoRedoManager` [F][g-undomgr]. A History dock exists [U, not confirmed from a doc page]. | Pitfall: in the Import dock, selecting another file before **Reimport** discards your changes [F][g-import]. |
| **Roblox** | Ctrl+Z / Shift+Ctrl+Z [F][r-short]. No undo history panel found [U]. | n/a |

**Confirmation vs. undo.** None of the docs read state a policy. What they show: Unity *warns* when you remove a component that others depend on [F][u-comp]. Blender documents Delete as X or Delete with no confirmation mentioned, and Delete Globally as a separate, stronger command [F][b-delete]. Whether Blender's X shows a confirm popup was not confirmed [U]. The pattern across tools is to rely on undo for everyday deletes and keep warnings for edits that break dependencies [U, inferred].

**Notifications and status.**
- **Unity status bar:** the last Console message (click to open the Console), a global progress bar for background tasks (click for details), and a compile spinner [F][u-status].
- **Blender status bar:** running tasks with a time estimate and a cancel X, and report messages that "disappear after a short time". Clicking a message opens it in the Info editor [F][b-status].
- **Godot:** `EditorToaster` pushes toasts with INFO / WARNING / ERROR severities [M][g-toast].
- **Unreal:** toast conventions not verified [U].

**Play-mode safety.** Unity reverts Inspector edits made during Play when you exit, by design, to speed up tuning [F][u-comp].

**Advantages and tradeoffs.** A clickable history list (Unity, Blender) makes multi-step undo safe to explore. Blender's truncation rule is the standard linear model. Adjust Last Operation removes many "undo and redo with different parameters" loops. Transient toasts and status messages risk being missed, so both Unity and Blender make the message clickable to open a persistent log.

**Translates to Axcenity?** Yes. Add an Undo History panel with click-to-jump and a visible marker for redoable entries. Make deletes undoable with no modal. Keep confirmations for operations that cannot be undone or that affect other assets. Give toasts and status messages INFO/WARN/ERROR severity, have them fade, and make them open the Output/log when clicked. Decide explicitly whether Play-mode edits revert (Unity's choice) and show that state clearly.

---

## 5. Workspaces and layout

| Tool | Mechanism | Reset |
|---|---|---|
| **Blender** | Workspace tabs: "essentially predefined window layouts ... geared towards a specific task such as modeling, animating, or scripting". The defaults are Layout, Modeling, Sculpting, UV Editing, Texture Paint, Shading, Animation, Rendering, Compositing, Geometry Nodes and Scripting. A workspace can switch the object **Mode** when activated [F][b-ws, 3.6]. | Not described on the page [U]. Workspaces are saved in the .blend file and can be saved as defaults [F][b-ws]. |
| **Godot** | Main-screen buttons: **2D, 3D, Script, Game, Asset Library** [F][g-first]. Ctrl+F1/F2/F3 switch between them [M][g-keys]. Bottom panels "are folded by default" because "they can take precious space" [F][g-first]. Saved layouts exist because "you may want to save and load a dock configuration depending on the kind of task" [F][g-layout]. | A hardcoded **Default** layout "can't" be deleted [F][g-layout]. |
| **Unity** | Built-in layouts plus Save / Save to file / Load / Delete [F][u-layout]. | **Reset all Layouts**: "Return all layouts to their built-in configurations" [F][u-layout]. |
| **Unreal** | Window › Load Layout / Save Layout [F][ue-menu, 4.26]. | "Load the default layout" [F][ue-menu]. |
| **Roblox** | Ribbon ("mezzanine") tabs **Home, Model, Avatar, UI, Script, Plugins**, with test controls on the left [F][r-ui]. The ribbon can be customized: Manage tabs, Compact, Show labels, custom tabs [F][r-ui]. | Layout reset not described on the page [U]. |

**Advantages and tradeoffs.** Task tabs (Blender, Roblox ribbon, Godot main screens) give beginners a mental map and keep each screen sparse. The cost is hidden features: you must know which tab holds a tool. Godot's always-available, undeletable Default layout and Unity's Reset all Layouts are cheap safety nets for users who wreck their layout.

**Translates to Axcenity?** Yes. The ribbon tabs already match Roblox. Two additions are worth making: (1) an always-available **Reset Layout** (Unity/Godot) that cannot be deleted or overwritten by accident; (2) let a studio/tab bring its own panel set, as Blender ties Mode to a workspace, so opening Animation Studio arranges the timeline and properties without manual docking. Fold rarely used bottom panels by default (Godot's reason: space).

---

## 6. Onboarding and empty states

| Tool | First-run / templates | In-editor help |
|---|---|---|
| **Roblox** | Templates from Studio's landing page: minimal ones (Baseplate, Classic Baseplate, Flat Terrain, **Starting Place** "for learning Studio") and genre kits (Obby, Racing, Combat, etc.) [F][r-tmpl]. | Class names link to the reference [F][r-expl]. |
| **Unity** | Hub templates: "a packaged version of a project that you can use as a starting point" [F][u-hub]. | The **?** icon on each component [F][u-comp]. |
| **Godot** | Project Manager: name, folder, **renderer** ("can also be changed later"), then **Create & Edit**. Templates and demos come from the Asset Library tab, which is **offline by default "for privacy reasons"** [F][g-pm]. | Built-in class reference via F1 / Search Help, Ctrl-click in the script editor, and Open Documentation on nodes [F][g-first]. |
| **Blender** | The Help menu and splash screen link to the manual, tutorials and support [F][b-help]. | Context-sensitive **Online Manual** (F1) on any control [F][b-help]. |

**Empty states seen in the docs.** Unity's centered **Create** button in an empty Animation window [F][u-newclip]. Godot's Animation panel appears only when an AnimationPlayer is selected [F][g-anim]. Roblox requires a rig and points to the Rig Generator [F][r-ae].

**Why heavy tutorials are avoided.** No primary source read here gives a rationale [U]. What the docs show instead is a mix of *starter content* (templates, Roblox's learning "Starting Place"), *contextual help* (F1, ?, Open Documentation) and *defaults that need no tutorial* (Unity's auto-wired Animator). Tutorials live in external curricula, such as Roblox's Animator curriculum [F][r-aetut].

**Translates to Axcenity?** Yes. Ship 2–4 templates (empty Baseplate, a "learn the editor" place, one genre kit). Give every studio a one-action empty state (e.g. "Select a rig to animate" with an Insert Rig button). Add one context-help key (F1) that opens the docs page for whatever is hovered or selected, including the class in Properties. Prefer this over a guided tour.

---

## 7. Terminology

| Concept | Unity | Unreal | Godot | Roblox | Blender / Spine |
|---|---|---|---|---|---|
| Time-based data | Animation **Clip** [F][u-ac] | Animation Sequence / Level Sequence [U] | **Animation** (in an AnimationPlayer) [F][g-anim] | Animation (**KeyframeSequence** when saved) [F][r-ae] | Action [U] / Animation (Spine) |
| Single time sample | **Keyframe** [F][u-rec] | Keyframe / key [F][ue-keys] | **Keyframe** ("a time and value pair") [F][g-3danim] | **Keyframe**. The doc frames authoring as "**poses**" [F][r-ae] | Keyframe [F][b-kf]; key [F][spine-keys] |
| Recording toggle | **Record mode** [F][u-rec] | **Auto Key** [F][ue-keys] | n/a (key icons) [F][g-anim] | none (always keys) [F][r-ae] | **Auto Keying** (record button) [F][b-timeline]; **Auto Key** [F][spine-keys] |
| Runtime player component | **Animator** component [F][u-newclip] | Skeletal Mesh + Anim BP [U] | AnimationPlayer [F][g-anim] | **Animator** (under Humanoid/AnimationController) [F][r-animator] | n/a |
| Logic graph asset | **Animator Controller** (state machine) [F][u-ac] | **Animation Blueprint** → AnimGraph → **State Machine** [F][ue-abp][ue-sm] | **AnimationTree** with StateMachine / BlendTree / BlendSpace1D/2D roots [F][g-tree] | none built-in [U] | NLA (not equivalent) |
| Parametric blend | **Blend Tree** [F][u-blend] | Blend Space [U] | BlendSpace1D/2D, BlendTree [F][g-tree] | n/a | n/a |
| Rest/default pose | n/a [U] | n/a [U] | **RESET** animation [F][g-anim] | n/a | Rest pose [U]; **Setup mode** [F][spine-ui] |

**Observations.**
- "Keyframe" is universal. "Pose" is the beginner word Roblox uses for what you author [F][r-ae].
- "Record" (Unity) and "Auto Key" (Unreal, Spine, Blender's "Auto Keying") are the two labels for the same toggle.
- "Animator" is overloaded. In Unity it is the component and "Animator Controller" is the asset. In Roblox it is the runtime playback object [F][u-newclip][r-animator].
- "State Machine" and "Blend Tree/Blend Space" are shared across Unity, Unreal and Godot.

**Translates to Axcenity?** Yes. Since the scripting model is Roblox-like, keep **Animator** for the runtime player (Roblox meaning) and use a different noun for the graph asset, such as "Animation Controller" with "State Machine" and "Blend Tree" inside it. That avoids Unity's Animator/Animator Controller confusion. Use "Keyframe" in the UI, "Pose" in beginner copy, and pick one of "Record" or "Auto Key" everywhere.

---

## 8. Drag-and-drop and import defaults

| Tool | Asset → scene | Import dialog and defaults |
|---|---|---|
| **Unity** | Drag a prefab asset into the Hierarchy or Scene view. Dropping on the Hierarchy uses the **3D Placement Mode** setting [F][u-prefab]. Dragging a GameObject into Assets creates a prefab [F][u-prefab]. | **Default Presets** in the Preset Manager apply to new components and importers, filterable by glob such as `glob:"example-folder/*.fbx"`, and ordered with the last match winning [F][u-preset]. Presets are *not* re-applied on Reimport. You use **Reset** for that [F][u-preset]. |
| **Unreal** | "Left-click the Asset, then drag it into the Level Viewport" [F][ue-place]. Right-click **Place Actor** puts the actor where you clicked [F][ue-place]. | **Interchange**: a configuration window with **Basic Layout** ("filters ... down to the basic pipeline properties") and **Filter on Contents**. A **Preview** "shows a list of assets that will be created". Default pipelines contain "the most common options". Reimport reuses the saved options [F][ue-interchange]. |
| **Godot** | Drag supported resources from FileSystem into inspector slots [F][g-insp]. Viewport drag behavior not verified [U]. | **Automatic import** on file add, with settings in the **Import** dock and an explicit **Reimport** button. **Preset…** menu, saved defaults, and project-wide **Import Defaults**. A sidecar `.import` file should be committed [F][g-import]. |
| **Roblox** | Toolbox / Asset Manager drag [U]. | **3D Importer** (File › Import 3D) has a 3D preview, an object hierarchy and per-object settings. Warnings appear as icons in the hierarchy/settings with tooltip text. Rig type is auto-detected. Defaults are mostly on for safety and convenience (Insert In Workspace, Add To Inventory), and the doc warns that "Double-sided is more performance intensive" [F][r-imp]. |

**Advantages and tradeoffs.**
- Silent automatic import (Godot) is the fastest path. The cost is that wrong defaults spread until someone opens the Import dock, which is why Godot and Unity both offer project-level default presets.
- A preview dialog (Roblox, Unreal) catches problems before assets are created, at the cost of one extra click.
- Basic vs. full settings (Unreal Basic Layout) applies progressive disclosure to import.
- Showing warnings *on the offending child node* (Roblox) is more actionable than a log line.

**Translates to Axcenity?** Yes. Drag from the Asset Browser to the viewport should place the asset at the cursor, and drag to the Explorer should parent it. Import should have sensible defaults, a preview with per-node warnings on first import of complex files (rigs, scenes), and a "Basic / All settings" toggle. Store import settings next to the asset so they diff and reimport the same way. Add a project-level defaults/presets list later.

---

## 9. Cross-cutting takeaways

1. **One obvious action in every empty state**, with automatic wiring behind it (Unity Create → Controller + Animator) [u-newclip].
2. **Modes must be loud.** Recording tints the timeline and the properties (Unity), and per-property key state is shown in color (Spine) [u-rec][spine-keys].
3. **Show non-default values**, with reset in place (Unreal, Godot) [ue-details][g-insp].
4. **Search everywhere**: the property filter, the Explorer query, and a command palette that shows shortcuts and menu paths [r-short][b-search][g-palette].
5. **Undo is the safety net, not dialogs.** Back it with a visible history list (Unity, Blender) [u-undo][b-undo].
6. **Context help on F1 / ?** beats tours (Blender, Godot, Unity) [b-help][g-first][u-comp].
7. **Task-scoped layouts with a guaranteed reset** (Blender tabs, Godot Default, Unity Reset all Layouts) [b-ws][g-layout][u-layout].

---

## Sources

[r-ae]: https://create.roblox.com/docs/animation/editor
[r-aetut]: https://create.roblox.com/docs/tutorials/curriculums/animator/work-with-the-animation-editor
[r-animator]: https://create.roblox.com/docs/reference/engine/classes/Animator
[r-props]: https://create.roblox.com/docs/studio/properties
[r-ui]: https://create.roblox.com/docs/studio/ui-overview
[r-short]: https://create.roblox.com/docs/studio/shortcuts
[r-expl]: https://create.roblox.com/docs/studio/explorer
[r-imp]: https://create.roblox.com/docs/art/modeling/3d-importer
[r-tmpl]: https://create.roblox.com/docs/resources/templates
[u-rec]: https://docs.unity3d.com/2022.1/Documentation/Manual/animeditor-AnimatingAGameObject.html
[u-newclip]: https://docs.unity3d.com/2018.4/Documentation/Manual/animeditor-CreatingANewAnimationClip.html
[u-animwin]: https://docs.unity.cn/2022.1/Documentation/Manual/animeditor-UsingAnimationEditor.html
[u-ac]: https://docs.unity.com/en-us/engine/6000.5/manual/animation-section/animation-mecanim/animation-animator-controller/class-animator-controller
[u-blend]: https://docs.unity3d.com/6000.0/Documentation/Manual/class-BlendTree.html
[u-undo]: https://docs.unity.com/en-us/engine/6000.6/manual/unity-editor/editor-navigating-managing/undo-window
[u-search]: https://docs.unity.com/en-us/engine/6000.5/manual/unity-editor/editor-navigating-managing/editor-searching/search-overview/search-window-reference
[u-layout]: https://docs.unity.com/en-us/engine/6000.5/manual/extending-the-editor/customizing-your-workspace
[u-status]: https://docs.unity.com/en-us/engine/6000.6/manual/unity-editor/editor-navigating-managing/status-bar
[u-comp]: https://docs.unity.com/en-us/engine/6000.6/manual/working-with-gameobjects/unity-components/using-components
[u-preset]: https://docs.unity.com/en-us/engine/6000.0/manual/assets-and-media/presets/default
[u-tooltip]: https://docs.unity.com/en-us/engine/6000.0/script-reference/unityengine/tooltipattribute
[u-hub]: https://docs.unity.com/en-us/hub/templates
[u-prefab]: https://docs.unity.com/en-us/engine/6000.6/manual/working-with-gameobjects/prefabs/creating
[g-anim]: https://docs.godotengine.org/en/stable/tutorials/animation/introduction.html
[g-3danim]: https://docs.godotengine.org/en/stable/getting_started/first_3d_game/09.adding_animations.html
[g-tree]: https://docs.godotengine.org/en/stable/tutorials/animation/animation_tree.html
[g-first]: https://docs.godotengine.org/en/stable/getting_started/introduction/first_look_at_the_editor.html
[g-insp]: https://docs.godotengine.org/en/4.5/tutorials/editor/inspector_dock.html
[g-import]: https://docs.godotengine.org/en/stable/tutorials/assets_pipeline/import_process.html
[g-layout]: https://docs.godotengine.org/en/stable/tutorials/editor/customizing_editor.html
[g-pm]: https://docs.godotengine.org/en/stable/tutorials/editor/project_manager.html
[g-keys]: https://docs.godotengine.org/en/4.5/tutorials/editor/default_key_mapping.html
[g-palette]: https://docs.godotengine.org/en/stable/classes/class_editorcommandpalette.html
[g-toast]: https://docs.godotengine.org/en/4.5/classes/class_editortoaster.html
[g-doccomments]: https://docs.godotengine.org/en/4.5/tutorials/scripting/gdscript/gdscript_documentation_comments.html
[g-undomgr]: https://docs.godotengine.org/en/4.2/classes/class_editorundoredomanager.html
[b-timeline]: https://docs.blender.org/manual/en/3.6/editors/timeline.html
[b-kf]: https://docs.blender.org/manual/en/3.6/animation/keyframes/editing.html
[b-undo]: https://docs.blender.org/manual/en/3.6/interface/undo_redo.html
[b-ws]: https://docs.blender.org/manual/en/3.6/interface/window_system/workspaces.html
[b-search]: https://docs.blender.org/manual/en/3.6/interface/controls/templates/operator_search.html
[b-help]: https://docs.blender.org/manual/en/4.0/getting_started/help.html
[b-keys]: https://docs.blender.org/manual/en/3.6/interface/keymap/introduction.html
[b-status]: https://docs.blender.org/manual/en/3.3/interface/window_system/status_bar.html
[b-delete]: https://docs.blender.org/manual/it/3.6/scene_layout/object/editing/delete.html
[ue-keys]: https://dev.epicgames.com/documentation/en-us/unreal-engine/creating-animation-keyframes-in-unreal-engine
[ue-abp]: https://dev.epicgames.com/documentation/en-us/unreal-engine/graphing-in-animation-blueprints-in-unreal-engine
[ue-sm]: https://dev.epicgames.com/documentation/en-us/unreal-engine/state-machines-in-unreal-engine
[ue-details]: https://dev.epicgames.com/documentation/en-us/unreal-engine/level-editor-details-panel-in-unreal-engine
[ue-spec]: https://docs.unrealengine.com/4.26/ProgrammingAndScripting/GameplayArchitecture/Properties/Specifiers
[ue-bpdetails]: https://docs.unrealengine.com/4.26/ProgrammingAndScripting/Blueprints/Editor/UIComponents/Details
[ue-menu]: https://docs.unrealengine.com/4.26/BuildingWorlds/LevelEditor/MenuBar
[ue-place]: https://dev.epicgames.com/documentation/en-us/unreal-engine/placing-actors-in-unreal-engine
[ue-interchange]: https://dev.epicgames.com/documentation/en-us/unreal-engine/importing-assets-using-interchange-in-unreal-engine
[spine-keys]: https://esotericsoftware.com/spine-keys
[spine-ui]: https://eu.esotericsoftware.com/spine-ui
[figma]: https://help.figma.com/hc/en-us/articles/23570416033943-Use-quick-actions

**Reading notes**
- Several Godot pages were read from the official `_sources/*.rst.txt` of the same docs, which is the same text. Pages marked [M] (`g-keys`, `g-doccomments`, `g-toast`, part of `g-palette`) were read through a third-party mirror of the Godot docs because the official HTML would not render. Recheck them before quoting.
- `b-delete` was read from the Italian build of the Blender 3.6 manual. Its shortcut and description text was untranslated English.
- `u-animwin` is on docs.unity.cn, Unity's China-hosted copy of the manual.
- The Roblox Animation Editor page documents no auto-key toggle. If current Studio has one, the "always keys" reading in section 1 needs rechecking.
