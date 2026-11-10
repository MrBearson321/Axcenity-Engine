# Model Studio usability pass (U20)

The dedicated Model Studio pass that [usability-audit.md](usability-audit.md) (A18, U20) left for later. It uses the same method and the same research: **[R§n]** is section n of [engine-conventions.md](engine-conventions.md). What each change was checked with is in [verification.md](verification.md).

## How this pass was done

1. **Looked at the real tab.** `editor_tour_snapshots` (`app/tour_snapshots.rs`; `TOUR_ONLY=studio` draws just these) now draws the whole editor with Model Studio in seven states: empty, a chair modeled from a New cube in Object and in Edit mode, and a humanoid in Paint, Sculpt, Rig and (rigged) Weights mode.
2. **Read the code behind each control**: `model_studio.rs` (toolbar, modes), `model_studio/tools.rs` (Mesh menu, adjust panel), `uv_editor.rs`, `modifiers.rs`, `paint.rs`, `sculpt.rs`, `retopo.rs`, `rig.rs` and `view.rs` (the hint line).
3. **Tried the first things a beginner does**: New cube, Edit, extrude, then Paint; and Rig on a body with no bones.

## Verdict

Model Studio is a capable Blender-style modeler, and much of it already follows the research. It has its own undo with the step's name in the tooltip. **Adjust Last Operation** (the panel at the bottom left after an extrude or bevel) is Blender's F9 **[R§4]**. A hint line in the view says what the mouse and keys do in each mode, as Blender's status bar does **[R§3]**. The Mesh menu names each operation's key, which teaches the keys as you use the menu **[R§3]**. Opening another model asks before throwing away unsaved changes. The empty state names the next step. The side panel already changes with the mode (Paint, Sculpt and Rig each show their own).

The problems are about **context** and **explanation**, not missing features:

1. The toolbar showed every modeling control in every mode, so Paint and Rig had a two-row toolbar of buttons that do nothing there.
2. Seven modes sat in one switch, and two of them (Weights, Pose) only work after a step the switch doesn't show. Clicking Pose without bones did nothing, and Weights without bones drew a stray cube in place of the model.
3. Names such as Bridge Edge Loops, Grid Fill, Dissolve, Smart UV Project or Solidify are the standard ones (every Blender tutorial uses them), but nothing in the Studio says what they do.
4. A specialist pipeline (Retopology and Bake) takes up most of the side panel while you model a chair.
5. Painting a New cube paints all six sides at once, because New shapes share one texture square per face, and nothing says so.

## Findings

| # | Problem | Severity | Frequency | Beginner impact | Advanced impact |
|---|---|---|---|---|---|
| MS1 | **The toolbar ignores the mode.** In Paint, Sculpt, Rig, Weights and Pose it still showed the four handle tools, Local axes, Show normals, the Mesh menu (every item greyed except shading), proportional editing and the pivot. At 1,600 points the toolbar wrapped to two rows. | High | Every non-modeling session | Tries buttons that do nothing | Noise |
| MS2 | **Seven modes in one switch.** Weights and Pose are steps of rigging, after Bones, but sat beside Paint and Sculpt with nothing showing the order. | Medium | Every rig | Doesn't know where to start | None |
| MS3 | **A mode that can't start says nothing.** Pose with no bones ignored the click. Weights with no bones went in, showed the brush, and the view drew a default cube in place of the model (`rig_model_key` took over the view without a model to give it). | High | First rig | Thinks it's broken | Confusing |
| MS4 | **Retopology and Bake always show.** Under Modifiers in Object and Edit mode: Reference, Triangles, Snap to surface, Poly Build, Make Low Poly, then Bake's size, cage, occlusion rays, four maps, a DirectX switch and Bake. That's most of the panel, for a sculpt → low poly → bake pipeline few models need. | Medium | Every modeling session | Overwhelmed | Fine |
| MS5 | **Menus name operations but don't explain them.** The Mesh, UV and Add Modifier menus have no descriptions on hover, unlike the ribbon's Insert menus. The Mesh menu's operations and Mark Seam are greyed in Object mode with no reason given. | Medium | Often | Guesses | None |
| MS6 | **Painting a New cube paints every side.** New shapes' faces each fill the whole uv square, so one texel is on all six faces. The docs say so; the Studio doesn't. | High | First paint | Thinks the brush is broken | Knows to unwrap |
| MS7 | **Edit mode's hint ends in a row of keys**: "E I Ctrl+B Ctrl+R K M X F: model", with no names. | Low | Edit mode | Unreadable | Fine |
| MS8 | **No ribbon button.** Terrain Studio has one on the World tab. Model Studio is reached from Panels, View, the palette, or Edit in Model Studio on a MeshPart. | Medium | Discovery | Doesn't find it | None |

## Keep, improve, simplify, consolidate, defer, remove

- **Keep**: the Studio's own undo with named steps, Adjust Last Operation, the per-mode hint line, keys in menu names, G/R/S with typed numbers and axis locks, the unsaved-changes bar, the Objects list, the modifier stack (show, apply, remove, reorder), and the side panel following the mode.
- **Simplify**: the toolbar, to what works in the mode (MS1).
- **Consolidate**: Rig, Weights and Pose into one Rig mode with three steps (MS2). The code keeps its three modes.
- **Improve**: say why a mode waits (MS3); descriptions and disabled reasons in the menus (MS5); the overlapping-uv warning in Paint (MS6); the Edit hint (MS7); a ribbon button (MS8).
- **Defer**: Retopology and Bake into a closed section that opens itself while a reference is set (MS4).
- **Remove**: nothing. Every mode and operation has a use, and the names are the ones tutorials teach.

## How the research applies

| Convention | Who does it | Model Studio before | Change | Why it helps here |
|---|---|---|---|---|
| Task-scoped screens that switch the mode | Blender workspaces are "geared towards a specific task" and can switch the object Mode; Roblox's ribbon changes per tab **[R§5]**. Context menus adjusted to the object type (Roblox Explorer) **[R§3]** | Same toolbar in all seven modes | MS1 | Paint and Rig keep only buttons that work there |
| Modes must be loud and ordered | Cross-cutting takeaway 2 **[R§9]** | Weights and Pose beside Paint, silent when they can't start | MS2, MS3 | The order bones → weights → pose is visible, and a step that can't start says what's missing |
| Tooltips that explain, and say why a control is disabled | Blender's tooltip shows the description, the shortcut and why a field is disabled; Godot and Unity take tooltips from one source **[R§2]**. The editor's own Insert menus explain each item | Names only, greyed without a reason | MS5 | Standard names stay (tutorials use them); one sentence explains each |
| Progressive disclosure | Unreal's Advanced section, Godot's collapsed groups **[R§2]**; this editor's Properties Advanced (U15) and Terrain Studio's Advanced | Always open | MS4 | Modeling shows Modifiers; the pipeline is one click away |
| One obvious action where a beginner is stuck | Takeaway 1: an empty state with one action and the wiring behind it **[R§9]** | Silent | MS6 | The first paint stroke does what a beginner expects after one click |
| Everything findable from where you look first | Roblox Quick Open Actions and Blender F3 find any action **[R§3]**; the editor's ribbon opens Terrain Studio | Panels menu only | MS8 | The Model tab is where someone looking to make a model looks |

## Plan

| PR | Findings | Size |
|---|---|---|
| 1. Modes and toolbar (#42) | MS1, MS2, MS3 (and the stray cube) | M |
| 2. Explain and fold | MS4, MS5, MS6, MS7, MS8 | M |

**Not planned:** renaming Blender's standard terms; captions on the toolbar's icons (the toolbar now fits one row, and each icon has a tooltip); putting the Studio's operations in the command palette (worth doing once the palette has a per-tab command list; medium work).
