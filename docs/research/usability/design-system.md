# Editor design system audit and modernization plan

Paul's brief of 2026-11-10: audit the editor's whole interface for usability, visual consistency, component reuse, layout, interaction states, accessibility and performance, then propose a prioritized, phased plan. **Nothing in this file is implemented yet.** Each phase starts only after Paul approves it.

- **Date:** 2026-11-10, on `main` at `0461abe` (after PR #59).
- **Builds on:** the [usability audit](usability-audit.md) (U1–U20, all merged), its research [engine-conventions.md](engine-conventions.md) (cited **[R§n]**), and the passes on [Model Studio](model-studio.md), the [Block editor](block-editor.md), the [UI Designer](ui-designer.md) and the [reviewed systems](reviewed-systems.md). Those fixed *what each control says and does*. This pass is about *how the editor looks and behaves as one product*: the design language, the shared components, layout and workspaces, interaction states, feedback, accessibility, and UI cost. It doesn't repeat their findings.
- **Policy:** [development-policy.md](../../development-policy.md). No new UI framework or dependency (egui 0.32, eframe, egui_dock and Phosphor stay), nothing in file formats or the script API, every phase small, local tests for each, no CI.

## How this audit was done

1. **Pictures of the real editor.** The tour test (`app/tour_snapshots.rs`) draws the whole window offscreen at 1600×900 through `EditorApp::frame`. This pass added six states (80–85: a material graph, a behavior tree, a dialogue, the Sequencer, Terrain Studio and a block script) and two switches, `TOUR_THEME=light` and `TOUR_DENSITY=compact`, so every state can be drawn in the light theme and at Compact density too:
   `TOUR_THEME=light AXCENITY_SNAPSHOTS=<dir> xvfb-run -a cargo test -p axcenity-editor editor_tour -- --ignored`
2. **Measured the code** (240 non-test files, 134,000 lines in `crates/editor/src`): where colours, sizes, radii and fonts come from, which shared widgets are used and where screens draw their own (table in [B](#b-visual-consistency-assessment)).
3. **Read the code** behind each area in the brief: the frame (`app.rs`), header and menus (`window_frame.rs`, `app/chrome/`), ribbon (`ribbon.rs`, `ribbon_tabs.rs`), docking (`layout.rs`), Explorer, Properties, Console (`output.rs`), toasts, dialogs, Settings, the three graph renderers, the viewport overlays.
4. **Timed two panels** that grow with the project (Explorer and Console) at 100, 1,000 and 10,000 rows ([A9](#ui-cost-measured)).

## Verdict

**Axcenity already has a design system, and it's a good one.** `theme.rs` defines semantic colour tokens for a dark and a light theme, eight accents that all meet WCAG AA (a test checks), a type scale on three families (Inter, Plus Jakarta Sans for titles, JetBrains Mono for code and micro-labels), Phosphor icons for everything, a colour and icon per Instance class, two densities, one motion duration with Reduce motion, and UI scale from 90 to 150%. `widgets.rs`, `dialog.rs`, `toast.rs` and `responsive.rs` give the core screens shared buttons, segmented controls, search fields, question boxes, toasts and narrow-panel layouts. The shell is already the professional kind the brief asks for: one dark header with the logo mark, Roblox-style ribbon tabs, docking with floatable panels, square flat panels with hairline dividers, a near-monochrome palette with one accent, and a command palette.

That look **is** Axcenity's identity, and this plan keeps all of it: the mark and wordmark, the charcoal neutral palette, square panels, the accent underline, Inter with Plus Jakarta Sans titles and mono micro-labels, Phosphor icons, class colours, the dot grid on the start pages. Nothing here is a redesign.

The problems are where the system **stops**:

1. **The newer canvases don't use it.** The three graph renderers (the generic `node_graph`, the block Graph view and the Animation state machine) each carry the same copy-pasted node colours, ignore the light theme, and mark selection in orange while the rest of the editor uses the accent. The viewport, Model Studio, splines and the in-viewport UI handles each pick their own selection and guide colours (selection is blue `#19A0FF` in the viewport, orange in graphs, the accent in panels; UI guides are cyan in the designer and magenta in the viewport).
2. **Colour and type have tokens; shape and space don't.** There are eleven different corner radii (0, 2, 3, 4, 5, 6, 7, 8, 9, 10, 12), and the shell doc says panels, windows and menus are square while question boxes are 12-point round and toasts 8. Font sizes 10, 12 and 14 are written by hand in 60 places beside a scale of 10.5, 11.5, 13 and 15.
3. **Feedback is easy to miss.** Errors arrive as a toast that disappears after 2.4 seconds and can't be clicked; the Console tab doesn't show that anything went wrong while it's behind Assets or Profiler; there's no persistent status area.
4. **A few common tasks take too many steps.** Properties edits only the last-clicked Instance when several are selected. The Explorer has no keyboard navigation. Windows don't agree on Escape.
5. **Customization ends at appearance.** Shortcuts are fixed and the Settings list misses many of them; layouts can be reset but not saved or switched.
6. **Lists don't scale.** Explorer and Console lay out every row every frame, and Console keeps every line forever.

None of these is **Critical** in the brief's sense (nothing stops normal development; each has a workaround). They are mostly **High** and **Medium**, and every fix fits inside the existing architecture.

## Most significant problems

| # | Problem | Why it matters | Plan |
|---|---|---|---|
| 1 | Properties edits only the last-clicked of several selected Instances | The commonest bulk task (anchor 20 parts, recolour a wall) is one-by-one. Unity, Unreal, Godot and Roblox Studio all edit a multi-selection with mixed values shown | V5 |
| 2 | Error toasts vanish in 2.4 s and the Console tab gives no sign of new errors | A failed import or save can go unnoticed; the brief asks that users always know whether an operation failed | V2, V3 |
| 3 | The ribbon keeps showing world tools (Move, Rotate, snapping, Insert Parts) while a script, Model Studio or a graph fills the main area; Model Studio then shows two sets of transform tools | Two identical-looking tool rows acting on different things is the kind of confusion beginners can't diagnose | V4 |
| 4 | Graph editors and viewport overlays ignore the theme and disagree on selection colour | Three selection colours teach three meanings; the light theme shows dark graph nodes | V1, V12 |
| 5 | No keyboard navigation in the Explorer | Keyboard users and fast users can't walk the tree; every reference engine supports arrow keys | V6 |
| 6 | Eleven corner radii, hand-written font sizes, no spacing or size tokens | The source of most small visual inconsistencies; each new screen picks again | V11, V21 |
| 7 | Explorer and Console cost grows with every row, Console never forgets | Large scenes and chatty scripts slow every frame of the editor ([A9](#ui-cost-measured)) | V7, V8 |
| 8 | Tool windows behave differently (Escape, collapse, frame) and seven modals don't use the shared question box | Dialogs feel like different apps | V14, V15 |
| 9 | Shortcuts are fixed, scattered over 36 files, and only partly listed | Hard to learn; impossible to adapt | V16, V23 |
| 10 | `text_muted` (below AA, meant for disabled text) is used for readable labels in about 20 places; control outlines are 1.3:1 | Readability for low-vision users and on poor displays | V13, V18 |
| 11 | Layouts can only be reset, not saved or switched per task | Animation, UI and scripting work want different panel sets | V17 |

## A. Audit by area

Each area lists what to **keep** and the **findings** (DS1…). The eight questions of the brief are answered where there's something to say; "no issue" is left out to keep this readable.

### A1. Main layout and navigation

**Keep:** header → ribbon tabs → ribbon → dock, top to bottom; the Scene, Animation and UI Designer as tabs over the same space (the studios replace the viewport rather than squeezing beside it); Explorer over Properties on the right; Console, Assets and Profiler under the Scene; floating panels in OS windows; Reset Layout; panels that fit a 260-point column (`responsive.rs`).

| # | Finding | Criteria | Severity |
|---|---|---|---|
| DS1 | The layout can be reset but not saved, named or switched. Animation, UI design and scripting each want a different arrangement, and today every switch is by hand. Blender, Godot and Unity offer saved or task layouts with a guaranteed reset **[R§5, §9.7]**. The earlier audit kept "no second workspace system"; named layouts are not a second system, just saved copies of the one there is (`Layout::to_text`). | Efficiency, scalability | Medium |
| DS2 | The Console shares a tab group with Assets and Profiler, and its tab doesn't say when errors arrive while another tab is showing. | Discoverability, feedback | High |
| DS3 | No status area. Background work appears as a toast with a bar (fine); everything else that would live in a status bar (last message, error count) has nowhere to stay. Unity and Blender keep a status bar for exactly this [R§4 sources u-status, b-status]. | Feedback | Medium (see V22: a decision for Paul) |

### A2. Header, ribbon, menus and contextual controls

**Keep:** the single 34-point header with the mark, menus and window buttons (VS Code style); seven ribbon tabs with captioned groups and custom tabs; Compact density turning the ribbon icon-only; Play/Stop in the Game group on the tabs that need it; the short menu bar (File, Edit, View, Help, Debug in debug builds), one level of submenus at most; the command palette and Quick Open.

| # | Finding | Criteria | Severity |
|---|---|---|---|
| DS4 | **The ribbon doesn't follow what's in the main area.** With a script, Model Studio, a graph or the Sequencer filling the space where the Scene was, the ribbon still shows Home's Select/Move/Resize/Rotate, snapping and Insert Parts, which act on the hidden 3D scene (tour 22, 60, 80–85). In Model Studio this puts two sets of the same transform icons on screen, one row apart, acting on different things. | Clarity, visual hierarchy, efficiency | High |
| DS5 | Shortcuts are written as constants and `key_pressed` checks in 36 files. Settings ▸ Shortcuts lists 24 of them; the dope sheet's keys, graph editors' keys, Model Studio's G/R/S, Terrain brush keys and F2/Delete contexts aren't there. Menus show a shortcut only where someone added `shortcut_text`. | Discoverability, consistency, customization | Medium |

### A3. Panels, inspectors and property editors

**Keep:** Properties after U6–U16 (descriptions, filter, changed-value marks with reset, Advanced folds); `responsive::form`; the Material tab's Values/Graph switch; Terrain Studio's Advanced fold.

| # | Finding | Criteria | Severity |
|---|---|---|---|
| DS6 | **Multi-selection edits one Instance.** With several selected, Properties says "Properties are for the last one clicked". Mixed values aren't shown and an edit can't reach the others. | Efficiency | High |
| DS7 | **Two conventions for "changed from default".** Properties puts ↺ in front of the name, which pushes those names out of the name column (tour 02: "Color" and "Roughness" are indented, the rest aren't). The Material tab marks the same thing with an accent dot, an accent-coloured name and ↺ after the value (tour 80). | Consistency, visual hierarchy | Medium |
| DS8 | Collapsible sections are built 15 ways: egui's `CollapsingHeader` with and without the shared chevron, the UI Designer's `fold.rs` (remembers open state, shows a summary when folded), and custom headers. Open state is remembered in some (UI Designer, Properties) and not others (Terrain Studio, Model Studio tools). | Consistency | Medium |
| DS9 | Empty states are good but each is hand-built (11 places): different icon sizes, some with a button, some without. | Consistency | Low |

### A4. Viewport overlays and interaction controls

**Keep:** the view cube, the measuring grid, the distance reading while moving, Play badge, the gizmo axis colours (red/green/blue, the convention everywhere), the building tool panel.

| # | Finding | Criteria | Severity |
|---|---|---|---|
| DS10 | **Overlay colours are scattered.** Viewport selection `#19A0FF` (`viewport.rs`), gizmo hover gold `#FFD700`, spline curve orange, selected spline point light blue, UI handle guides magenta `#E840B0` (`ui_handles.rs`) while the UI Designer's guides are cyan `#40C8E6` (`ui_designer/artboard.rs`), Model Studio seams red and tool yellow. Each choice is reasonable alone; together the same meaning (selected, guide) has two or three colours. | Consistency | Medium |

Overlay colours are rightly *not* theme colours (they sit on 3D content, not on panels), so the fix is one shared set of overlay tokens, not the accent.

### A5. Asset browser and content management

**Keep:** category chips, search, Unused, list/grid by width, thumbnails loaded only when visible (`is_rect_visible`), Import with a dialog only when it's worth asking, Global Assets. Already reviewed in [usability-audit A5](usability-audit.md#a5-asset-browser-and-importing); nothing new here except the chip, which is drawn by its own function (`asset_browser::chip`) and would move to the shared widgets (V19).

### A6. Animation and other specialized editors

**Keep:** everything the U-passes settled (loud Record, pending-key notice, breadcrumb, Model Studio's per-mode toolbars, the UI Designer's folds).

| # | Finding | Criteria | Severity |
|---|---|---|---|
| DS11 | **Three graph renderers, three copies of one look.** `node_graph.rs` (Material, UI Event Graph, Behavior Tree, Dialogue), `block_editor/graph.rs` and `animation/graph/draw.rs` each define `NODE_BODY #1E2028`, `NODE_EDGE`, `PIN_TEXT` and `SELECTED #FFB02E` as their own constants. None reads the palette, so in the light theme the canvases stay dark-node-on-light-panel, and selection is orange in graphs but the accent everywhere else. Their add-node menus (`node_graph/menu.rs`, `block_editor/graph/menu.rs`) are two copies of the same 190-line search menu. | Consistency, maintenance, scalability | High |
| DS12 | **Graphs open too small to read.** The Behavior Tree opens fitted at 48% and the Dialogue at 60% (tour 81, 82), so node text is drawn at about 5–6 points: the user has to zoom before reading anything. The Animation state machine opens at a readable size (tour 11). | Usability, accessibility | Medium |
| DS13 | **The Sequencer uses two time units on one screen**: the timeline bar counts frames (0–600 at 30 fps) and the dope sheet under it counts seconds (0–20) (tour 83). The same `animation::Timeline` bar counts frames for linked take clips in the Animation tab, whose keyed clips count seconds. | Clarity | Low |
| DS14 | Canvas text sizes (10, 11, 12, 13, 14 written in the timeline, dope sheet, curves, graphs) are independent of the type scale and of Settings' code font size. Most are multiplied by zoom, which is right; the unzoomed ones should come from the scale. | Consistency | Low |

### A7. Dialogs, popups, dropdowns and context menus

**Keep:** `dialog::ask` (icon tile, title, message, Main/Danger/Plain buttons, Enter and Escape); undo instead of "Are you sure?" **[R§4, §9.5]**; context menus on every tree and list.

| # | Finding | Criteria | Severity |
|---|---|---|---|
| DS15 | Only four question boxes use `dialog::ask`. Seven other modals (asset dialog, About, the start screen's dialogs, remove global asset, Import, the Animation dialogs, Get texture) frame, title and lay out their buttons on their own. | Consistency | Medium |
| DS16 | Nine tool windows (`egui::Window`: Settings, Input, Collision Groups, Version History, Find in Scripts, New Custom Material, References, Learn, ribbon tab editor) disagree: only Settings and Input close on Escape; four have a collapse arrow, five don't. | Consistency, keyboard | Medium |
| DS17 | Corners: the shell doc and `theme::visuals` make windows and menus square, but question boxes are 12-point round, toasts and the offer card 8, buttons 6, segmented controls 7 outside and 5 inside, fields 4. | Visual consistency | Medium |

### A8. Settings, preferences and configuration

**Keep:** the Settings window's pages (Appearance with live theme previews, Text & density, Editor layout, Graphics, Shortcuts, Accessibility); changes apply at once and persist; Input and Collision Groups windows with undo (RS2, RS3).

| # | Finding | Criteria | Severity |
|---|---|---|---|
| DS18 | Text & density ▸ "Interface font" is a sentence, not a setting. Fine as is: UI scale covers text size without a second font-size system. Noted so nobody adds one. | Simplicity | — |
| DS19 | No higher-contrast option. The default palette deliberately has quiet borders (control and field outlines are about 1.3:1 against the panel, under WCAG's 3:1 for component boundaries), which is the identity and should stay the default, but some users need more. | Accessibility | Medium |

### A9. Status, notifications, progress and errors; performance of the UI

**Keep:** toasts for success ("Saved"), the Busy bar for background work, the Offer card (Relaunch), Console severity filters with counts, error lines that link to the script line, Record and pending-key banners in Animation.

| # | Finding | Criteria | Severity |
|---|---|---|---|
| DS20 | **Errors vanish.** `toast.rs` shows an error for 2.4 s like a success, it can't be clicked, and at most three show at once. The message is in the Console, but nothing says to look there. Toasts have only Success and Error kinds. | Feedback, accessibility (timing) | High |
| DS21 | **Explorer cost grows with every row.** `Explorer::rows` builds a `Vec<Row>` with a `String` per visible Instance every frame, then lays out every row, even the thousands scrolled out of view. | Performance | High (measured below) |
| DS22 | **Console keeps every line and lays out all of them.** A script that prints each frame adds 60 lines a second forever; each frame lays out all of them. | Performance | High (measured below) |

#### UI cost, measured

Measured with a scratch test (not committed) that draws the panel alone through egui into a 300×800 (Explorer: one Folder of Parts, open) or 900×260 (Console: one-line messages) window, after three warm-up frames, averaged over 20 (Explorer) or 10 (Console) frames, on the 4-vCPU cloud container. The container is about 2.5× slower than Paul's desktop ([benchmarks](../../benchmarks.md)), so divide by roughly 2.5 for his PC.

| Rows | Explorer, release | Explorer, debug | Console, release | Console, debug |
|---:|---:|---:|---:|---:|
| 100 | 0.86 ms | 1.75 ms | 0.35 ms | 0.46 ms |
| 1,000 | 7.1 ms | 12.3 ms | 3.7 ms | 5.0 ms |
| 10,000 | **61 ms** | 133 ms | **42 ms** | 72 ms |

Both grow in a straight line with the number of rows, although only about 30 (Explorer) or 12 (Console) are on screen. At 10,000 rows each panel alone takes longer than a 60 Hz frame (16.7 ms), even on Paul's PC (about 25 ms and 17 ms). An open folder of 10,000 parts is unusual (the S10k scene has one), but 10,000 Console lines are not: a script that prints once a frame gets there in under three minutes of Play, and the Console keeps them for the rest of the session. That makes DS21 and DS22 **High**.

Everything else the brief lists under performance is in good shape: the editor stops drawing shortly after the last input (`STEADY_FRAME_SECONDS`) unless Play is running or "Hold the refresh rate steady" is on, thumbnails load only when visible, Properties' descriptions are looked up once and cached, `theme::apply` runs only when a setting changes, animations never wake an idle window on a timer (`theme::arrival`), and there are no per-frame allocations in the shared widgets beyond egui's own.

### A10. Tooltips, icons, labels and indicators

**Keep:** 586 hover texts, Phosphor everywhere (no emoji, no second icon set; the few text glyphs like `×`, `→` and `⋯` are inside sentences), class icons and colours, icon plus colour plus text for severities (never colour alone), one icon per animation kind.

| # | Finding | Criteria | Severity |
|---|---|---|---|
| DS23 | Tooltips appear on mouse hover only; a control focused with Tab shows its focus ring but not its tooltip, so keyboard users don't get the description. | Accessibility | Low |
| DS24 | `text_muted` is documented as "disabled and decoration only: below AA" (2.8:1 dark, 2.6:1 light) but carries readable text in about 20 places: the Explorer's class labels, the command palette's menu paths, parts of the Console prompt and Assets, UI Designer panels. | Accessibility | Medium |
| DS25 | **Block labels are white on light category colours** (Scratch's palette): 1.7:1 on Events yellow, 1.9:1 on Control orange, 2.3–2.9:1 on Operators, Variables, Sensing and Motion (tour 85). Readable to most, but far under AA. The colours are part of what makes blocks recognizable, so the default stays; Increase contrast (V18) should darken the fills, as Scratch's own high-contrast theme does. | Accessibility | Medium |
| DS26 | Custom-drawn clickable things (39 `allocate_*` with a click or drag sense) give AccessKit a role and label in 23 places. Rows, chips and canvas nodes are mostly unlabeled for screen readers. Canvases can stay as they are (no reference engine labels graph nodes), but rows and chips should be labeled. | Accessibility | Low |

### A11. Interaction states

| State | Today | Verdict |
|---|---|---|
| Hover | Eased fill (`theme::ease`), no growth on hover, stronger border | Consistent in widgets; graphs and timeline have their own hover |
| Selected / active | `accent_soft` fill with `accent_text`; active tab joined to its panel with an accent underline | Consistent in panels; graphs orange, viewport light blue (DS10, DS11) |
| Focus | Accent ring on egui widgets and `paint_hover` rows | Rows drawn without `paint_hover` (Explorer) have no ring and no arrow keys (DS27) |
| Disabled | egui's dimming, `text_muted` | Fine |
| Drag and drop | Explorer rows, Assets cards, UI Designer hierarchy, Animation graph, viewport drops | Each shows its own drop highlight; acceptable, unify the colour with V12 |
| Loading / progress | Busy toast with bar; Model "draw nothing while loading" (Paul's choice) | Fine |
| Error / validation | Red text, red outline on bad block inputs, Console | Fine, except DS20 |
| Undo / redo | Everywhere, Undo History list (U17) | Fine |

| # | Finding | Criteria | Severity |
|---|---|---|---|
| DS27 | **The Explorer has no keyboard navigation**: no Up/Down to move the selection, no Left/Right to fold, no Shift+arrow to extend. Only F2, Delete, Ctrl+D act on it. Rows aren't focusable, so Tab skips them. | Efficiency, accessibility | High |

## B. Visual consistency assessment

| Dimension | Token today | Adherence | Verdict |
|---|---|---|---|
| Colour (chrome) | `Palette`: 25 semantic tokens, both themes, 8 accents | 229 uses of `theme::palette`; 326 hand-written colours: about 150 in canvases and overlays (graph nodes and categories, selection, guides, handles) and the rest content (block categories, syntax, gizmo axes, the Learn pictures) | **Strong in panels, absent in canvases** (DS10, DS11) |
| Type | `type_scale` (26, 15, 13, 11.5, 10.5) + four families | 165 uses of the scale; about 60 hand-written sizes, mostly 10, 12 and 14, which aren't on it | Good; close the gap (V21) |
| Spacing | egui's spacing per density; no named steps | `add_space` is 2/4/6/8/12/16 in 90% of 251 calls; 10, 14, 22 elsewhere | Already on a 2-point grid; name it (V11) |
| Corner radius | One value in `visuals` (4); squares for panels | 11 different values | **Inconsistent** (DS17, V11) |
| Borders | `border`, `border_strong`, 1 px; hairline dock dividers one physical pixel | Consistent | Keep |
| Elevation | Two shadows (window, popup) per theme | Consistent | Keep |
| Icons | Phosphor regular and fill, `class_icon`, `tool_icon`, `insert_icon` | Consistent; icon sizes 11–18 written by hand | Add icon sizes to V11 |
| Motion | `MOTION_SECONDS` 0.16, `curve::*`, Reduce motion | Consistent; nothing waits on an animation | Keep |
| Density | Comfortable / Compact | Applies to egui spacing and the ribbon; custom rows read `interact_size` | Keep |

**Light theme** (every tour state, `-light`). The shell, Explorer, Properties, Console, Settings, the Material tab and the block canvas switch cleanly; the theme is complete for everything that reads the palette. The graph editors don't: the Behavior Tree and Dialogue draw near-black nodes on the light canvas, and their comment boxes' titles (light grey on light grey) can't be read (tour 81-light, 82-light). Fields are white on white with a 1.3:1 outline, so in the light theme a text field is found mostly by its text (tour 02-light: the Name field) — the case for V18.

**Compact density** (`-compact`). Works as designed: the ribbon becomes one icon row with names in tooltips and rows tighten, giving the dock 27 more points (tour 02-compact). The dock's tab bars (30 points) and the header (34) don't change with density; that's fine and not worth a token.

## C. Component reuse and design system assessment

| Component | Shared today | Where it's duplicated or missing | Plan |
|---|---|---|---|
| Buttons, button roles | `widgets::button` (Primary, Secondary, Ghost, Danger), `icon_button`, `RibbonButton` | 109 plain `ui.button` calls: fine, they're styled by `visuals` | Keep |
| Button groups / segmented | `widgets::segmented*` | — | Keep |
| Text and numeric fields | egui `TextEdit`, `DragValue`, `Slider`; `search_field` | Consistent through `visuals` | Keep |
| Dropdowns | egui `ComboBox` (99) | — | Keep |
| Checkboxes and toggles | egui `checkbox` (91) | — | Keep |
| Tabs | dock tabs (`layout::style`), ribbon tabs, script tabs, `segmented` | Ribbon and script tabs share the accent underline | Keep |
| Toolbars | `responsive::toolbar`, ribbon groups | — | Keep |
| Panels and headers | dock + `page_title`, `section_label` | — | Keep |
| Property rows | `responsive::form`, `properties.rs` | Style panel, Terrain Studio and UI Designer reuse `form` | Keep; add multi-edit (V5) |
| Expandable sections | `widgets::chevron`, `ui_designer::fold` | 15 implementations (DS8) | `widgets::section` (V19) |
| Dialogs | `dialog::ask` | 7 modals of their own (DS15) | V15 |
| Tool windows | — | 9 `egui::Window`s set up separately (DS16) | `widgets::tool_window` (V14) |
| Context menus | egui `context_menu` | Consistent | Keep |
| Tooltips | `on_hover_text` | Mouse only (DS23) | V25 |
| Status indicators, badges, chips | `micro`, `asset_browser::chip`, Animation `Badge`, warning badges | Three looks for one idea | V19 |
| Notifications | `toast::Toasts` (Success, Error, Busy, Offer) | Errors vanish (DS20) | V2 |
| Progress | Busy toast, `Spinner` | — | Keep |
| Empty states | none | 11 hand-built (DS9) | V19 |
| Graph canvas | `node_graph` (used by 4 editors) | Block Graph and Animation graph are separate engines with the same look copied (DS11) | V1 now, V24 later |
| Overlay colours | none | Scattered (DS10) | V12 |

**Verdict.** The component set is the right size: it covers what most screens need without wrapping every egui widget, so egui's own controls stay usable and styled by `visuals`. Adding a component library on top of egui would be the "unnecessary abstraction" the brief warns against. What's missing is small: tokens for radius, space and sizes; a canvas and an overlay palette; a section fold, an empty state, a chip; a tool window; and one shortcut table.

## D. Prioritized improvements

Size: S (hours, one module), M (a day, a few modules), L (several days or cross-cutting). Risk is regression risk.

Each item gives the eight things the brief asks for: what happens **now**, the **problem**, the **change**, the **benefit**, what it **touches**, what it **depends on**, what it **could break**, and how it's **verified**.

### High priority

#### V1. Graph editors take the theme, and selection is the accent everywhere *(fixes DS11; M)*
- **Now:** `node_graph.rs`, `block_editor/graph.rs` and `animation/graph/draw.rs` each define the same node body, edge, pin text and orange selection constants, ignore the light theme, and draw selection orange.
- **Problem:** three copies to keep in step; light theme shows dark nodes on a light canvas; selection means orange in graphs and the accent in every panel next to them.
- **Change:** a `theme::Canvas` set of tokens built from the palette (canvas background, node body, node edge, pin text, wire, comment, selection = `accent`, error = `error`, live = `success`) for dark and light; the three renderers read it and drop their constants. Category header colours (Events, Actions, Data…) stay content colours, moved into the same module and given light-theme variants that keep header text at AA.
- **Benefit:** one look for every graph in both themes; selection reads the same in the Explorer, Properties and every graph; the next graph editor gets the look for free.
- **Touches:** `theme.rs`, `node_graph.rs`, `node_graph/draw.rs`, `block_editor/graph.rs`, `block_editor/graph/nodes.rs`, `animation/graph/draw.rs`.
- **Depends on:** nothing (V11's radius tokens help but aren't required).
- **Could break:** contrast of header text on category colours in the light theme; the Animation graph's live-state green and entry wires; existing graph drawing tests that compare colours.
- **Verified by:** tour shots 11, 44, 80–82, 85 in dark and light, before and after; the WCAG test extended to canvas text on node bodies and on every category header in both themes; the existing graph tests (`node_graph/tests.rs`, `behavior_tree_tests`, `state_machine_tests`, `block_undo_tests`).

#### V2. Errors stay until they're seen *(fixes DS20; S–M)*
- **Now:** `Toasts::error` shows for 2.4 s, can't be clicked, max three, Success and Error only.
- **Problem:** a failed save, import or export can disappear while the user looks elsewhere; the brief asks that users always know whether something failed.
- **Change:** toasts get four kinds (Info, Success, Warning, Error). Success and Info still go on their own. Warning and Error stay until dismissed (×) or clicked; clicking opens the Console with that line in view. The stack keeps at most three; a fourth error collapses into "+2 more errors in the Console". Toasts become clickable only where they're drawn, so they still never block the viewport.
- **Benefit:** no silent failures, without adding dialogs.
- **Touches:** `toast.rs`, the 16 `Toasts::error` callers (no change at call sites), `output.rs` (scroll to a line), `app.rs`.
- **Depends on:** nothing.
- **Could break:** tests that expect error toasts to go away (`toasts_go_away_on_their_own…` covers success only); clicks under a toast in the bottom-right corner of the viewport.
- **Verified by:** unit tests in `toast.rs` (an error stays past `SHOWN_SECONDS`, × removes it, a click returns the line to open, a fourth collapses); an app test that a failed import leaves an error toast and clicking it shows the Console; tour shot of a failed import.

#### V3. The Console tab says when something went wrong *(fixes DS2; S)*
- **Now:** errors from scripts during Play and from the editor go to the Console only; when the Console isn't the shown tab of its group, nothing changes on screen.
- **Problem:** a beginner presses Play, nothing happens, and the reason is behind the Assets tab.
- **Change:** the Console tab's title shows a count of errors (red, with the error icon) or warnings (amber) added since the Console was last shown, e.g. `Console  ⊗ 3`; it clears when the tab is shown. Same in a floating Console window's title.
- **Benefit:** the one place every failure goes is always one glance away.
- **Touches:** `layout.rs` (`Pane::title` gets the count), `output.rs` (an "unseen since" mark), `app/chrome/panes.rs`.
- **Depends on:** nothing.
- **Could break:** tab width (egui_dock measures the title each frame; a badge changes it), saved layouts (titles aren't saved, so no).
- **Verified by:** app test: an error while Assets is the shown tab puts a count on the Console tab; showing the Console clears it; snapshot of the tab bar with the badge in both themes.

#### V4. The ribbon follows the main area *(fixes DS4; M)*
- **Now:** the ribbon tab is whatever was clicked last. With a script, Model Studio, a graph or the Sequencer shown where the Scene was, Home's Tools, Snapping and Parts groups stay live and act on the hidden scene.
- **Problem:** two sets of transform tools on screen (Model Studio's and the ribbon's, also the Animation preview's); clicks on the ribbon's change the world the user can't see.
- **Change (recommended):** when no Scene View is visible (not the shown tab of any dock node or floating window), the ribbon groups that act only on the 3D scene (Home: Tools, Snapping, Parts; Model: Build, Solid, Constraints; World: Terrain brush) are drawn disabled, with the hover "These act on the Scene view: show it to use them" and a **Show Scene** link at the end of the ribbon. Play, Panels, Insert menus and everything that works without the viewport stay live. Nothing switches tabs by itself. *Alternative (Paul's call):* switch the ribbon tab to the related one once when the main tab changes (a script → Script, UI Designer → UI, Model Studio → Model, Terrain → World), as Roblox Studio's contextual tabs do.
- **Benefit:** one live set of tools at a time; the screen says why a tool isn't available instead of letting it act out of sight.
- **Touches:** `app/chrome/toolbar.rs` (`ribbon_group`), `ribbon_tabs.rs` (which groups are scene-only), `layout.rs` (is a pane visible), `app.rs`.
- **Depends on:** nothing.
- **Could break:** the tool keys (1–5, V, N, M, U, P, R) must follow the same rule or they'd change the world out of sight; a Scene View floated into its own window counts as visible; custom ribbon tabs mixing groups.
- **Verified by:** app tests: with a script shown, the Move button is disabled and the 2 key doesn't change the tool; with Scene View split beside the script, both work; floating Scene counts; tour shots 22, 60 and 80–85 before and after.

#### V5. Properties edits every selected Instance *(fixes DS6; L)*
- **Now:** with several selected, Properties shows the last one clicked and edits only it.
- **Problem:** bulk changes (Anchored, Color, Material, CanCollide on many parts) are one by one; the brief's "reduce the steps of common tasks" points straight at this.
- **Change:** Properties shows the properties the selected Instances have in common (same class first; across classes, the shared ones such as BasePart's). A value that differs shows as `—` (mixed) in its field. An edit sets that property on all of them as **one** undo step through `EditorApp::change`. The header says "12 Parts" (or "12 selected · 3 kinds"). Read-only reasons, descriptions, the filter, Advanced folds and changed-value marks work as now; reset applies to all.
- **Benefit:** the commonest bulk edit in every reference engine, and the biggest efficiency gain in this plan.
- **Touches:** `properties.rs` (rows gain a mixed state; widgets for vectors show `—` per axis), `app/chrome/panes.rs` (`properties_pane` passes the whole selection), `app/edits.rs` (apply to many, one history step), the style panel stays single-Instance at first.
- **Depends on:** nothing; easier after V11.
- **Could break:** RS1's guarantee that showing properties never edits (a mixed field must not write a value back); prefab overrides (each copy records its own); Play-mode Keep changes per value; undo grouping; performance with hundreds selected.
- **Verified by:** app tests: anchoring three parts is one undo step and undo restores each one's own value; a mixed field shows `—` and leaves every value alone when only looked at (extend `showing_the_properties_of_anything_in_the_examples_changes_nothing` to multi-selections); mixed classes show only shared rows; editing during Play changes every selected Instance in the session; a selection of 1,000 parts draws Properties in under a set time.

#### V6. Keyboard navigation in the Explorer *(fixes DS27; M)*
- **Now:** rows are mouse-only; Tab skips them.
- **Problem:** slow for experienced users, impossible for keyboard-only users.
- **Change:** when the Explorer has focus (click on it, or Tab into it): Up/Down move the selection, Shift+Up/Down extend it, Left folds (or moves to the parent), Right unfolds (or moves to the first child), Home/End, Enter or F2 renames, and the selected row scrolls into view. The focused row shows the accent focus ring (`widgets::paint_hover`). Keys go to the Explorer only while it has focus, so the viewport's WASD and tool keys are untouched.
- **Benefit:** the hierarchy works like Unity's, Godot's and Roblox Studio's.
- **Touches:** `explorer.rs` (focus, key handling), `explorer/tests.rs`.
- **Depends on:** easier after V7 (row list cached), but not required.
- **Could break:** the viewport's key handling (focus routing), rename-in-place, drag and drop, the filter field's own keys.
- **Verified by:** explorer tests for each key; an app test that W/A/S/D still fly the camera when the Explorer doesn't have focus and that arrow keys in the filter box move its text cursor.

#### V7. The Explorer draws only what's on screen *(fixes DS21; M)*
- **Now:** a `Vec<Row>` with a `String` per visible Instance every frame, and every row laid out.
- **Change:** keep the row list and rebuild it only when `World::structure_version`, a name, the open set or the filter changes; draw with `ScrollArea::show_rows` (rows have one height), so only on-screen rows are laid out. Drag-and-drop, scroll-to-selected and rename keep working.
- **Benefit:** the Explorer costs the same with 100 or 100,000 Instances (see [A9](#ui-cost-measured)).
- **Touches:** `explorer.rs`.
- **Depends on:** nothing.
- **Could break:** scroll-to-selected (must compute the offset from the row index), rename field focus, drop targets on rows that weren't laid out.
- **Verified by:** existing explorer tests; a new ignored timing test at 10,000 rows (committed this time) with the before and after numbers in the PR; S10k with Workspace open in the editor Play bench (`bench_project_play`) before and after.

#### V8. The Console keeps a bounded log and draws only what's on screen *(fixes DS22; M)*
- **Now:** every line kept forever and laid out every frame; counts recomputed by walking all lines.
- **Change:** keep the newest 10,000 lines (a line at the top says how many older ones were dropped), keep per-kind counts as lines arrive, and draw only the visible lines (lines wrap, so heights are cached per line and width).
- **Benefit:** a chatty script can't slow the editor down over a long session.
- **Touches:** `output.rs`.
- **Could break:** stick-to-bottom scrolling, clicking a script link, the stack trace under an error, filters.
- **Verified by:** `output.rs` tests; the timing test; a test that the 10,001st line drops the first and counts stay right.

### Medium priority

#### V9. One way to show "changed from default" *(fixes DS7; S)*
- **Now:** Properties puts ↺ before the name (names leave their column); the Material tab uses an accent dot, an accent name and ↺ after the value.
- **Change:** one row convention in `responsive::form` rows: a 2-point accent bar at the row's left edge (in the gutter, so names stay aligned), the name in normal text, ↺ at the row's right end on hover and when focused. Properties, the Material tab, the Style section and the UI Designer inspector use it.
- **Benefit:** names line up; the same mark means the same thing in every inspector; the accent is used once per row instead of twice.
- **Touches:** `responsive.rs` or `widgets.rs` (the row helper), `properties.rs`, `material_editor.rs`, `style_panel.rs`, `ui_designer/panels.rs`.
- **Depends on:** nothing; pairs well with V5 (mixed rows).
- **Could break:** the 260-point column layout (the ↺ needs room at the end; under 280 points it goes under the value as the control does); tests that click ↺ by position.
- **Verified by:** the existing reset tests (U10) and Material reset tests; `every_panel_fits_a_narrow_column_and_a_short_strip`; tour shots 02, 61, 80.

#### V10. Graphs open at a readable size *(fixes DS12; S)*
- **Now:** a graph opens fitted to the view, down to 48%.
- **Change:** on opening, fit the graph but never below 80%; if it doesn't fit, centre on the start/root node instead. Below about 50% zoom, draw node titles at a minimum readable size and drop pin labels (a simple level of detail), so a zoomed-out overview still shows what each node is.
- **Benefit:** the user reads the graph without zooming first.
- **Touches:** `node_graph.rs` (fit on open, drawing), `block_editor/graph.rs`, `animation/graph.rs` (same rule).
- **Depends on:** nothing (V1 makes it one change instead of three, if done first).
- **Could break:** tests that assert the view after opening.
- **Verified by:** a test per editor that opening a large example graph gives zoom ≥ 0.8; tour 44, 81, 82 before and after.

#### V11. Tokens for shape, space and size *(fixes DS17 and part of DS8; M)*
- **Now:** colour and type have tokens; radii (11 values), spacing (mostly a 2-point grid), row heights and icon sizes are written by hand.
- **Problem:** every screen picks its own; the shell's "square panels" rule and the rounded dialog and toast disagree.
- **Change:** in `theme.rs`: `radius` (`NONE` 0 for panels, tabs, windows and menus; `CONTROL` 4 for fields, buttons and chips; `CARD` 6 for cards, toasts, popovers and the question box), `space` (2, 4, 8, 12, 16, 24, named `XXS…XL`), `size` (row height per density, icon `SMALL` 13 / `MEDIUM` 15 / `LARGE` 18, ribbon heights already there). Shared widgets, dialogs, toasts, the command palette and dock style use them first; other screens move over as they're touched. Buttons go from 6 to 4 and the question box from 12 to 6, which is the only visible change: slightly crisper.
- **Benefit:** the "consistent corner radii, spacing and proportions" the brief asks for, enforced by names instead of memory.
- **Touches:** `theme.rs`, `widgets.rs`, `dialog.rs`, `toast.rs`, `command_palette.rs`, `layout.rs`, `ribbon.rs`, `app/settings.rs`.
- **Depends on:** nothing. Do it first: V14, V15 and V19 build on it.
- **Could break:** layouts that relied on a widget's exact height (snapshot tests that compare sizes, the 260-point column test).
- **Verified by:** `every_panel_fits_a_narrow_column_and_a_short_strip`; the whole tour in dark, light and compact before and after (expect only corner changes); a test that every `CornerRadius` the shared widgets use is one of the three tokens.

#### V12. One set of overlay colours *(fixes DS10; S)*
- **Now:** selection, hover, guide and handle colours are constants in `viewport.rs`, `gizmo.rs`, `splines.rs`, `ui_handles.rs`, `ui_designer/artboard.rs` and `model_studio/view.rs`.
- **Problem:** guides are cyan in one place and magenta in another; "selected" is light blue, gold or yellow depending on the tool.
- **Change:** `theme::overlay`: `SELECTED`, `HOVER`, `GUIDE`, `HANDLE`, `WARNING`, plus the gizmo axes, as fixed colours (not themed: they sit on 3D content and must read on any sky). Pick one guide colour (magenta reads on both sky and UI; cyan clashes with the default blue accent) and one selection blue. Each file uses the tokens.
- **Benefit:** the same meaning has the same colour in the viewport, Model Studio, splines and the UI Designer.
- **Touches:** `theme.rs` and the six files above.
- **Depends on:** nothing.
- **Could break:** visibility of a colour on a particular background (check snow, sky, grass examples).
- **Verified by:** tour shots 02, 21–27, 41–43, plus a splines and a UI-handles shot, before and after.

#### V13. Readable text is at least AA *(fixes DS24; S)*
- **Now:** `text_muted` (2.8:1 dark, 2.6:1 light) is used for readable labels in about 20 places.
- **Change:** those move to `text_weak` (5.1:1 dark, 5.3:1 light); `text_muted` stays for disabled text, placeholder hints and decoration.
- **Benefit:** the Explorer's class labels, the palette's menu paths and similar become legible without changing the look much (one step lighter).
- **Touches:** about 20 call sites listed by `grep text_muted`.
- **Could break:** hierarchy where the muted shade was carrying "less important" (check the Explorer and palette shots).
- **Verified by:** a test that lists the allowed `text_muted` uses is overkill; instead, a review of each site in the PR and the tour shots in both themes.

#### V14. One helper for tool windows *(fixes DS16; S)*
- **Now:** nine `egui::Window`s, each configured on its own.
- **Change:** `widgets::tool_window(title, icon, &mut open)` returns a configured window: title in the heading family, no collapse arrow, resizable, a default size that's clamped to the screen, Escape closes it (when no text field inside wants Escape). The nine use it.
- **Benefit:** every window opens, closes and looks the same way.
- **Touches:** `widgets.rs` and the nine files.
- **Depends on:** V11 for margins.
- **Could break:** Escape in a window that has its own use for it (Input's "Press a key…" capture must keep Escape).
- **Verified by:** an app test that opens each window and closes it with Escape; the Input capture test still passes.

#### V15. Modals share the question box's frame *(fixes DS15; M)*
- **Now:** seven modals lay themselves out.
- **Change:** split `dialog.rs` into the frame (title row with icon tile, body, button row with `widgets::button` roles, Enter = main, Escape = cancel) and `ask` on top of it. The seven move onto the frame, keeping their own bodies (the Import dialog's tabs and preview stay).
- **Benefit:** every dialog has the title, buttons and keys in the same place.
- **Touches:** `dialog.rs`, `asset_browser.rs`, `start.rs`, `start/dialogs.rs`, `start/global.rs`, `import_dialog.rs`, `animation/dialogs.rs`, `ambient_cg.rs`.
- **Depends on:** V11.
- **Could break:** Import dialog sizing; Enter in a dialog with a text field (must commit the field, not press the main button, unless the field is the dialog's only input).
- **Verified by:** existing tests of each dialog; tour shots of Import, New Project and About in both themes.

#### V16. One shortcut table *(fixes DS5; M)*
- **Now:** constants and `key_pressed` checks in 36 files; Settings ▸ Shortcuts lists 24.
- **Change:** `shortcuts.rs` holds every shortcut as data: action, default keys, context (Everywhere, Viewport, Explorer, Script, Animation, Graph, Model Studio, UI Designer) and label. Handlers ask the table (`shortcuts::pressed(ctx, Action::Save)`), Settings ▸ Shortcuts lists it by context, menus and the command palette show keys from it. Behaviour doesn't change.
- **Benefit:** every shortcut is findable in one list, and V23 (rebinding) becomes a small change.
- **Touches:** `app/chrome/windows.rs`, `app.rs`, `app/command_palette.rs`, `app/settings.rs`, and the context handlers (dope sheet, graphs, Model Studio, UI Designer), one context per commit.
- **Depends on:** nothing.
- **Could break:** a key consumed in the wrong order (egui's `consume_shortcut` order matters, as Ctrl+Shift+Z before Ctrl+Z does today).
- **Verified by:** a test that every action in the table appears in Settings ▸ Shortcuts and no two actions in overlapping contexts share keys; the existing palette test (`every_ribbon_and_menu_command_can_be_run_from_the_palette`); existing key tests per editor.

#### V17. Saved and preset layouts *(fixes DS1; M)*
- **Now:** one layout, saved between runs, and Reset Layout.
- **Change:** View ▸ Layout: **Default** (today's, always there, can't be overwritten), presets **Scripting** (script tabs wide, Console tall beside them, Explorer), **Animation** (Animation tab large, Scene preview beside it, Properties), **UI Design** (UI Designer with Explorer and Properties), **Modeling & Assets** (Model Studio, Assets wide), **Lighting & Environment** (Scene large, Properties tall, Terrain beside Properties); then **Save Current Layout…**, and saved ones listed with Delete. Each is a `Layout::to_text` snapshot in eframe storage. Picking one never closes an open script or changes what's selected. The ribbon tab doesn't switch layouts by itself.
- **Benefit:** task workspaces like Blender's, Godot's and Unity's **[R§5]**, without a second system: a layout preset is the existing layout, saved.
- **Touches:** `layout.rs`, `app/chrome/menus.rs`, `app/settings.rs` (Editor layout page lists them), command palette entries.
- **Depends on:** nothing.
- **Could break:** saved layouts from older versions (`from_text` already falls back to the default); floating windows in a preset on a smaller screen (clamp them).
- **Verified by:** tests that each preset loads, contains the Scene, and survives `to_text`/`from_text`; that Default can't be deleted; that a saved layout persists through a restart (fake storage, as `graphics_settings_survive_a_restart` does); tour shot per preset.

#### V18. Increase contrast *(fixes DS19, DS25; S–M)*
- **Now:** one contrast level per theme.
- **Change:** Settings ▸ Accessibility ▸ **Increase contrast**: control and field outlines at 3:1 or more, `border` one step stronger, `text_muted` raised to AA, focus ring 2 points, and block category fills darkened until their white labels reach 4.5:1 (DS25). It changes the palette and the block fills only, so every screen follows.
- **Benefit:** a real option for low vision and poor displays, while the default stays Axcenity's quiet look.
- **Touches:** `theme.rs` (`Palette::new` takes the option, `Appearance` gains a field with a serde default), `app/settings.rs`, `block_editor.rs` (`Category::fill`/`edge` take the option).
- **Depends on:** V13.
- **Could break:** nothing outside the option.
- **Verified by:** the WCAG test runs on both contrast levels and checks 3:1 for outlines when it's on; settings persistence test.

#### V19. Three shared pieces: section, empty state, chip *(fixes DS8, DS9 and the badge part of C; M, gradual)*
- **Now:** 15 collapsible-section implementations, 11 hand-built empty states, three chip and badge looks.
- **Change:** `widgets::section` (from `ui_designer/fold.rs`: chevron, remembered open state, optional summary when folded), `widgets::empty_state(icon, title, sentence, optional action)`, `widgets::chip`/`badge`. Move the Properties groups, Terrain Studio, Model Studio tools, Animation inspectors and Assets chips onto them over a few PRs.
- **Benefit:** the same fold, empty state and chip everywhere; less code.
- **Touches:** `widgets.rs` then each screen.
- **Depends on:** V11.
- **Could break:** remembered open state of sections whose ids change (use the existing ids).
- **Verified by:** existing tests per screen (several open sections with `open_section`); tour shots.

### Low priority

#### V20. One time unit in timelines *(fixes DS13; S)*
`animation::Timeline` labels seconds (with frame ticks between, as the dope sheet does) so the Sequencer and the Animation tab's take bar match the dope sheet. Frame numbers stay in the playhead's hover. Verified by tour 12, 13 and 83 and the timeline tests.

#### V21. Hand-written font sizes go through the scale *(fixes DS14; S)*
Unzoomed text sizes 10, 12 and 14 become `CAPTION`, `SMALL` and `HEADING`-adjacent sizes from `type_scale`; canvas text keeps zoom but starts from the scale (`type_scale::BODY * zoom`). Touches the timeline, dope sheet, curves, graphs, toasts and view cube. Could break: text that was sized to fit a fixed box. Verified by: tour shots 10–14, 80–85.

#### V22. A status bar *(DS3; M; Paul's call)*
A 22-point strip under the dock: the last Console message (click opens the Console), error and warning counts, background work (moving the Busy toast there), and the selection count. It costs 22 points of height on every screen, so V2 and V3 come first; if they're enough, skip this.

#### V23. Rebind shortcuts *(M; after V16)*
Settings ▸ Shortcuts: click a shortcut, press new keys, conflicts shown in the same context, Reset to defaults, saved in eframe storage.

#### V24. One graph canvas *(L; after V1)*
Move the block Graph view and the Animation state machine onto `node_graph`'s canvas (pan, zoom, box select, add-node menu), keeping their own node types. Large and risky for little user-visible gain once V1 has made them look alike; revisit only when one of them needs a canvas feature the others have.

#### V25. Tooltips for keyboard focus *(fixes DS23; S)*
A focused control shows its tooltip after the same delay as hover. One helper in `widgets.rs` that the shared widgets use.

#### V26. Screen-reader labels on rows and chips *(fixes DS26; S)*
`widget_info` with a role and label on Explorer rows, Assets cards and chips, Console lines and nav items that don't have one.

## E. Phased plan

Six phases, each one or two PRs that leaves the editor working and tested. Phases 1–3 are the core of the plan; 4 and 5 can swap; 6 is optional. Every phase:

- runs `cargo test -p axcenity-editor` (and `cargo clippy -p axcenity-editor --all-targets -- -D warnings`, `cargo fmt --all`) locally, with the result in the PR; the audio-device tutorial test and the translucent-leaf test fail in the cloud on `main` too and are noted, not fixed;
- re-runs the tour (`editor_tour`) in dark, light and compact and attaches before and after shots of the states it changed;
- keeps `every_panel_fits_a_narrow_column_and_a_short_strip` and `showing_the_properties_of_anything_in_the_examples_changes_nothing` green;
- appends a section to [verification.md](verification.md) (what changed, files, what was preserved, tests and results, what's left, next phase), which is the per-phase report the brief asks for;
- isn't checked on Windows or a real GPU in the cloud; those checks go on the PC-checks list as for U1–U20.

| Phase | Items | Priority | Size | What the user sees |
|---|---|---|---|---|
| **1. Finish the tokens** | V1, V9, V10, V11, V12, V13 | High (V1) + Medium | M–L | Graphs follow the light theme, select in the accent and open at a readable size; slightly crisper corners; one guide and selection colour per meaning; one changed-value mark; more legible secondary labels |
| **2. Clear state and feedback** | V2, V3, V4, V14, V15 | High | M–L | Errors stay until seen; the Console tab counts new errors; world tools go quiet when the Scene isn't shown; every window and dialog closes and confirms the same way |
| **3. Scale** | V7, V8 | High (measured) | M | Big scenes and chatty scripts don't slow the editor |
| **4. Faster everyday editing** | V5, V6 | High | L | Edit many selected Instances at once; walk the Explorer with the keyboard |
| **5. Workspaces and options** | V16, V17, V18, V19 | Medium | M–L | Layout presets and saved layouts; Increase contrast; every shortcut listed; one section, empty state and chip |
| **6. Optional polish** | V20–V26 | Low | S–L | As each item says |

**Why this order.** Tokens first because V14, V15 and V19 are built from them and because they touch many files: doing them before the behavioural phases keeps later PRs small and conflict-free. Feedback next because it's the cheapest High item. Then scale, because the timings show the Explorer and Console each taking a whole frame at 10,000 rows, and because V6 (Explorer keys) is simpler on V7's cached row list. Multi-edit and keyboard navigation are the largest changes and come last of the core, once the shared pieces under them are settled.

### Acceptance criteria per phase

**Phase 1.**
- No hand-written colour constants left in the three graph renderers; they read `theme::Canvas`. Selection in every graph is the current accent (change the accent in Settings and it follows).
- Graph text on node bodies and category headers meets AA in both themes (test).
- Shared widgets, dialogs, toasts, the palette and the dock use only the three radius tokens (test): buttons and fields share one corner, and cards, toasts, popovers and the question box share another.
- Overlay colours are defined once (`theme::overlay`); the UI guides in the viewport and the UI Designer are the same colour.
- No informative text uses `text_muted` (reviewed list in the PR).
- Changed-from-default looks the same in Properties, the Material tab, the Style section and the UI Designer inspector, and names stay in their column (tour 02, 80).
- Every graph editor opens an example graph at 80% zoom or more (tests).
- Tour in dark, light and compact: nothing else moved (corners, the colours above, the changed-value mark and graph zoom are the only differences).

**Phase 2.**
- An error toast is still there after 10 s, closes with ×, and clicking it shows the Console scrolled to the line (tests).
- An error while the Console is hidden puts a count on its tab, cleared when shown (test).
- With the Scene View hidden, scene-only ribbon groups and their keys do nothing and say why; with it visible (docked, split or floating) they work as before (tests).
- Every tool window closes on Escape (except where a field inside is capturing keys), has no collapse arrow, and opens inside the screen at 1280×800 and at 150% UI scale (tests).
- All modals have the title, icon tile and button row of `dialog::ask`; Enter and Escape behave the same in each (tests per dialog that exist today still pass).

**Phase 3.**
- Explorer and Console frame time at 10,000 rows within 2× of the time at 100 rows (timing test, release build, numbers in the PR).
- The editor Play bench with S10k shows no regression; with Workspace open, the Explorer's share drops.
- The Console holds at most 10,000 lines and says how many were dropped (test).

**Phase 4.**
- With N Instances selected, an edit is one undo step that undo reverts per Instance (test); mixed values show `—`; looking never edits (test across every example, multi-selections included).
- Explorer: every key in V6 does what it says (tests); the viewport's keys are unaffected when the Explorer doesn't have focus (test); focus ring visible.

**Phase 5.**
- Each layout preset loads, includes the Scene, survives a restart; Default can't be deleted; a saved layout comes back after a restart (tests).
- Increase contrast: outlines ≥ 3:1 and all text ≥ 4.5:1 in both themes (WCAG test), saved between runs.
- Settings ▸ Shortcuts lists every shortcut in the table, grouped by context; no clash within a context (test).
- Sections, empty states and chips in the moved screens come from `widgets.rs`; their open state is remembered as before (tests).

**Phase 6.** Per item, as written in D.

### Subjective changes

Several items can't be fully verified by a test: whether 4-point corners look better than 6, whether a magenta guide reads better than cyan, whether a preset layout suits a task. For those the PR shows before and after shots in both themes and says why; Paul's look at them is the check.

### Open questions for Paul

1. **Approve the plan, or which phases?** Nothing is implemented until you say so.
2. **Status bar (V22):** try V2 and V3 first and skip the status bar if they're enough (recommended), or add it in Phase 2?
3. **Corners (V11):** consolidate to 0 / 4 / 6 (recommended: keeps the square, flat identity), or keep buttons at 6 and only fix the outliers?
4. **Guide colour (V12):** one colour for every guide, magenta (recommended) or cyan?
5. **Ribbon and main area (V4):** disable scene-only tools while the Scene isn't shown (recommended), or switch the ribbon tab to match the main tab?
6. **Layout presets (V17):** are the six from the brief the right set, or start with fewer (Default, Scripting, Animation, UI Design)?

## F. What this plan keeps, and what it won't do

- **Kept as is:** the identity (mark, wordmark, palette, square panels, accent underline, fonts, Phosphor, class colours, dot grid), the ribbon, the menus, docking and floating panels, the command palette, Properties after U6–U16, the studios' layouts after their U20 passes, undo instead of confirmations, every feature.
- **Not planned:** a new UI framework, a component library over egui, a second font-size setting (UI scale covers it), automatic layout switching when the ribbon tab changes (surprising; layouts are picked, not imposed), theming beyond dark/light/system plus accent, rounded "web dashboard" cards in the editor chrome, new animations.
- **Open questions for Paul** are at the end of [E](#e-phased-plan).
