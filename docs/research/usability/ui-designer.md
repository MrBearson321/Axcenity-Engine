# UI Designer usability pass (U20)

The dedicated pass on the UI Designer that the [audit](usability-audit.md) planned as U20 (finding A12 there was *reviewed* only). Same method as the audit: look at the real editor, read the code behind each control, then fix what a beginner trips on, in small PRs with tests. The research it leans on is [engine-conventions.md](engine-conventions.md) (**R§n** below).

## How it was looked at

- **Pictures.** `editor_tour_snapshots` (`app/tour_snapshots.rs`) now also draws the UI Designer: empty (no ScreenGui), `hello-menu` with the ScreenGui, a button and a label selected, the Event Graph (`hello-ui-graph`), Strings and Pages, plus the designer alone 2,400 points tall with a button, a label and a slider selected, so the whole inspector shows (at 1,100 and 1,600 points wide: a stacked and a two-column inspector). Run it with `TOUR_ONLY=ui-`:
  `AXCENITY_SNAPSHOTS=<folder> TOUR_ONLY=ui- xvfb-run -a cargo test -p axcenity-editor editor_tour -- --ignored`
- **Code.** Every inspector section, the toolbar, the arrange bar, the right-click menus and the empty state (`ui_designer.rs`, `ui_designer/`).

## Findings

Severity as in the audit: **High** blocks or misleads a beginner in a common task; **Medium** slows or confuses but has a workaround; **Low** polish.

| # | Problem | Severity | Fix |
|---|---|---|---|
| UD1 | The inspector is one flat list of up to 14 always-open sections, in the order they were built. For a TextButton the ready-made looks (Styles), the anchor and **When Activated** come last, about 1,400 points down, under States, Bevel, Slice and the transparency sliders. A beginner's first three jobs (make it look right, keep it in place on other screens, make it do something) are the furthest away. | High | D1 |
| UD2 | In a two-column inspector the Text section's **Shadow** row wraps inside its grid cell and the blur field is drawn over **Letter spacing** (a layout bug, seen in the 1,600-point shot). | Medium | D2 |
| UD3 | Look's transparency sliders read "Background 100%" for a background you can't see; only the hover text says it's transparency. | Medium | D3 |
| UD4 | The anchor's **Reset** hover text has a run of spaces mid-sentence (a missing line continuation). | Low | D3 |
| UD5 | Right-click on the artboard or in the Hierarchy offers layer order, Copy/Paste style and Duplicate, but not **Rename** or **Delete**. Delete works only by the Delete key, which the designer never mentions. | Medium | D4 |
| UD6 | With no ScreenGui the tab says "There's no ScreenGui to design yet." A beginner isn't told what a ScreenGui is (a screen of 2D interface over the game). | Medium | D5 |
| UD7 | The toolbar's **Fixed · Fit · Scale** has no label or hover text; what each does is said only after picking Fixed or Fit (the note under the toolbar). | Low | D6 |
| UD8 | Style (CSS)'s empty text reads "No CSS styles this element yet" (a word missing). Shared with Properties. | Low | D7 |
| UD9 | An element can't be moved into another Frame from the designer: an artboard drag moves it inside its parent and the Hierarchy has no drag and drop, so it takes the Explorer. | Medium | Later (D8) |
| UD10 | No X, Y, width and height in design pixels in the designer's inspector (Properties has `Position` and `Size` as UDim2). Handles, arrow-key nudges and the size label cover most needs. | Low | Later |

**Keep** (no change needed): the Elements palette (drag, or click to add to the selected frame, with hover text saying so); rulers, guides and snapping with magenta lines; the anchor grid and diamonds, which say in words what the element is pinned to; Styles' tiles; Code & animation's **When** buttons, which write and open the handler; the Event Graph with Live Preview and its "Ready to run" status; Strings and Pages, whose empty states say what they're for and offer the next step; the Preview screen picker; the Fixed and Fit notes with **Use Scale**; undo on every edit. **Remove:** nothing. Every section serves a real workflow; the problem is order and density, not surplus.

## How the research applies

- **Fold sections, open what's common, keep the user's choice.** Unity folds each component, Unreal hides rare properties under Advanced, and Roblox keeps a section's collapse state "for other objects of the same class" **[R§2]**. The designer's sections were flat labels, so nothing could be put away. Folding them, with the everyday ones open, lets a beginner see Styles, Anchor and Code without scrolling, and an expert open States once and have it stay open for every element.
- **Never hide what's set without a word.** Unreal's Advanced split "depends on authors tagging properties well" and can hide what beginners need **[R§2]**; Godot answers with *Expand Non-Default*. So a folded section's header names what the element has there (**Bevel · Raised**), the same idea as Properties' "Advanced · 2 changed" (U15).
- **Order by task, not by when a feature was built.** Every engine in R§2 puts the controls for the common job first (Unity's Transform, Roblox's Appearance); here that's look, place, text and behaviour.

## Plan

| ID | Change | Fixes | Complexity | Risk | PR |
|---|---|---|---|---|---|
| D1 | Inspector sections fold; beginner-first order (Styles, Anchor, Text, Widget, Image, Look, Code & animation, then States, Bevel, Slice folded); folded headers name what's set; open state kept per section; while States is folded the text toolbar and Ctrl+B/I/U edit the Normal look | UD1 | M | Low–medium: tests that reached Bevel, Slice and States now open them first | 1 |
| D2 | Shadow's four fields on two fixed lines, not a wrapped row | UD2 | S | Very low | 1 |
| D3 | "Transparency: 0% is solid, 100% is invisible." over the sliders; Reset's hover text | UD3, UD4 | S | Very low | 1 |
| D4 | **Rename** and **Delete** in the artboard's and the Hierarchy's right-click menus | UD5 | S | Low: Delete is the editor's own, one undo step | 2 |
| D5 | The empty state says what a ScreenGui is | UD6 | S | Very low | 2 |
| D6 | Hover text on Fixed, Fit and Scale | UD7 | S | Very low | 2 |
| D7 | Style (CSS)'s empty text | UD8 | S | Very low | 2 |
| D8 | Drag a Hierarchy row onto a container to move it in, keeping where it is on screen | UD9 | M | Medium: reparenting rewrites `Position` | later |

## Verification

### PR 1: beginner-first inspector (D1–D3)

| Workflow | Before | After |
|---|---|---|
| Give a new button a ready-made look | Scroll past Text, States, Bevel, Image, Slice and Look to **Styles** | **Styles** is the first section under Name, Text and Font |
| Make it do something when clicked | **When Activated** at the very end, above the CSS | **Code & animation** comes right after the everyday sections; States, Bevel and Slice are folded below it |
| Find a bevel set earlier | Always open | Folded, its header says **Bevel · Raised**; one click opens it, and it stays open for the next element |
| Bold the Normal text after trying the Hover tab | The text toolbar kept editing the Hover rule, with States far below | Folded, States edits nothing: the toolbar and Ctrl+B edit the Normal look (the Text header says **Text · Hover** only while another state is being edited) |
| Read a background's transparency | "Background 100%" | The same slider under "Transparency: 0% is solid, 100% is invisible." |

Tests (`app/ui_inspector_tests.rs`): `the_inspector_lists_everyday_sections_first_and_the_fine_tuning_last` (the order for a button, a label, a frame and the ScreenGui, and which sections start folded); `a_folded_section_opens_on_its_header_stays_open_and_names_what_it_has` (Bevel's header clicked open, **Bevel · Raised** after Raised, still open on the next element); `with_states_folded_the_text_toolbar_edits_the_normal_look` (Ctrl+B with the Hover tab picked writes `#play`, not `#play:hover`, until States is opened). The States, Bevel and Slice tests in `ui_text_style_tests.rs` open their section first, as a user would, and draw a taller designer so the section is on screen. D2 and D3 were checked in the snapshots (tour shots 47–50).
