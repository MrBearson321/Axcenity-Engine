# Cohesion pass: audit and phased plan

Paul's brief of 2026-11-12, "Editor UX, Visual Identity, Workflow Improvements & Advanced Systems" (thread "Make Axcenity editor cohesive and professional"). This file is **Phase 1** of it: what exists, what is partial or missing, the shared problems underneath the requests, and the order of the work. IDs: findings **CF1…**, work items **CW<phase>.<n>**, phases **CP1–CP9** (named so they don't clash with the design system's DS/V, Collaborate's C/P or the unified plan's numbered phases).

Audited at `main` `7a50a6d` (after PR #225). Sources are named by path; claims about behavior were read in the code, and the ones marked *suspected* get a failing test first in their phase.

## Verdict

Most of what the brief asks for has a foundation already, often a good one. The work is mainly to **finish and connect** systems, not to build parallel ones:

| Brief foundation | What exists | What is missing |
|---|---|---|
| A. Shared UI design system | `editor-kit` (`theme.rs` tokens: type scale, 3 radii, spacing, sizes, palette, overlay colours; `widgets.rs`: role buttons, segmented, section, empty state, chip, nav rail, tool window; `responsive.rs`: toolbar, form, split). Design-system Phases 1–5 merged (#66 #71 #73 #74 #79) | A menu/dropdown button and toolbar row on the same height token (CF1); the block category colours locked inside `block_editor.rs` (CF6); menu icon alignment (CF3); ~65 hand-written font sizes and ~135 hard-coded colours outside the theme (CF5) |
| B. Unified visual scripting IR | **Already two shared IRs**: `BlockScript` (`blocks-runtime/src/lib.rs`) is the one IR behind Blocks, Graph and Lua (both directions, lossless through grey Lua blocks); `NodeGraph` (`blocks-runtime/src/node_graph.rs`) is the one IR behind UI Event Graphs, behavior trees and dialogues, compiling to Lua. One canvas for all graph views (`editor-node-graph/src/canvas.rs`, Phase 91) | One catalogue that **describes** nodes (name, description, purpose, pins, Lua, example) for tooltips in all views (CF12); navigation for scripts inside objects (CF11, suspected bug) |
| C. Physics abstraction | `rapier3d` is used **only** inside `crates/physics` (no other crate names it); the engine-facing API is Instance properties (`CustomPhysicalProperties`, `AssemblyLinearVelocity`, movers, constraints) plus `Physics`'s methods and `query.rs`; Luau reaches it through properties and methods (`ApplyImpulse`, `GetMass`, `AssemblyMass`…) | An explicit split inside the crate between backend-neutral types and rapier code (CF14); several properties the brief lists don't exist or don't act on the simulation (CF15) |
| D. Collaboration layers | Built and largely separated: real-time (`collab` host/client/ops/live `yrs` docs, presence, roles), transport (`socket.rs` WebSocket over TLS with a pinned certificate, `tailscale.rs` through the CLI), sync (`files.rs`, `mirror.rs`, journal, history, handoff recovery), version control (`repo` crate: git CLI, GitHub device flow, checkpoints, conflicts) — ADR-0020/21/22 | A home-page **Team Projects** entry point (CF17); trust preferences, connection approval, firewall detection and narrower listening (CF18–CF20); a single transparency page that says what needs a host (CF21) |
| E. Shared preview | `editor-tools/src/preview.rs` (`Canvas` over `render::TextureRenderer`, own camera, own draw list) already serves the Animation, Import and Material previews; `TextureRenderer` is also used by Model Studio, the UI Designer and the viewport debug views | An **Object Viewer** on top of it (orbit, pan, zoom, framing, reset, lighting, environment) and its hooks in Explorer, Assets, Toolbox and Model Studio (CF16) |

The order of the brief holds, with one change: **saving (CP3) moves to the front of the editor-workflow phase**, because it is a data-loss bug (policy: fix correctness before features).

## Findings

### UI, design system and responsive layout

- **CF1. Buttons of different heights come from two code paths.** `widgets::button` sets `min_size(…, size::BUTTON_HEIGHT)` (28 pt) and the theme's corner radius; egui's own `ui.menu_button` (60 call sites) and plain `ui.button` (about 90) use egui's interact size. Model Studio's New and Open are `menu_button`s and Save is `widgets::button`, which is exactly the mismatch Paul saw (`crates/editor-model-studio/src/lib.rs` `toolbar`). Same pattern in Animation, Sequencer, Terrain layers, UV editor, Data Table and the Explorer.
- **CF2. Ribbon captions sit at different heights.** `ribbon::group` paints its caption right under its own row, whose height is the tallest control in that group (`crates/editor/src/ribbon.rs`). Groups whose controls are taller or shorter than a ribbon button (Snapping's fields and switches, Game's play controls) put "Game", "Tools" and "Snapping" at different heights. Fix: one ribbon row height, captions pinned to the bottom of it.
- **CF3. Menu icons are glued into labels as text.** `theme::with_icon(icon, label)` makes `"<glyph> Label"`, so an icon's column depends on the label's font and spacing, and items without icons don't line up with items that have one. There is no setting for left versus centred icons.
- **CF4. Panels overflow when small.** (a) The Console's command line is an `egui::Panel::bottom` whose completion list can be 30 names wrapped over many lines with no height limit, so in a short Console it pushes over the log (`app/commands.rs`, `console.rs`). (b) The Blocks top bar lays its hint "Drag blocks onto the canvas. Drop them back on the palette to delete." left to right with no width limit, and the zoom and View-as-Lua buttons right to left in the same row, so a narrow tab overlaps them (`block_editor.rs` `show_toolbar`). (c) Long labels in fixed rows elsewhere; `responsive.rs` exists but isn't used by these.
- **CF5. Tokens aren't used everywhere.** About 65 `FontId::proportional(<number>)`, 22 `corner_radius(<number>)` and 135 `Color32::from_rgb` outside `theme.rs` in the editor crates (design system V21, never done).
- **CF6. The block category colours are private.** `block_editor.rs` `Category::base_fill`/`icon` hold the vibrant palette Paul likes; nothing else can use it. `theme::class_color` (Explorer icons) and the ribbon have their own colours.
- **CF7. The start screen's reading glass is on every page and its edge fights scrolling.** `start.rs` paints `backdrop::reading_glass` behind the content column on all five pages (Projects, Global, Learn, Documentation, API Reference). The glass starts `GLASS_MARGIN` (24 pt) below the page top, but the scroll area is clipped to the whole page, so text scrolls up off the glass onto the moving mountains for the last 24 pt, and the glass's fixed border cuts through lines while the scene behind slides sideways: the "text moves independently of the background" effect. The backdrop itself is fixed (painted under the navigation, no parallax on scroll), so the mountains don't need to stop moving. "Website" in the brief is this start screen (its pages are the editor's own Documentation, API Reference and Learn, from `documentation/`; there is no website in the repo).
- **CF8. No empty workspace.** The Scene tab can't be closed (`layout.rs` `Pane::Scene`), so the editor never shows an empty document area, and there is no Visual Studio-style welcome page. Recent projects are tracked for the start screen (`ProjectLink`s); recently opened script tabs aren't remembered (script tabs aren't saved, `layout.rs`).

### Editor workflow

- **CF9. Save, Don't Save, Cancel exists, but changes are written anyway.** `app/session.rs` `request`/`answer`/`go_ahead` ask correctly, but:
  - `finish_closing_project` calls `animation.write_all`, so **open clips and state machines are written to disk after Don't Save** (and on opening another clip, `animation/documents.rs`). Animation edits are not counted by `EditorApp::unsaved`, so they never trigger the question either.
  - Model Studio's `StudioDoc::unsaved` and the Sequencer's open file are not in `unsaved()`; closing loses or silently keeps them depending on path.
  - Eight separate dirty checks (`history.is_saved`, script tabs, UI graphs, behavior trees, dialogues, data tables, materials, block scripts) are or-ed by hand in `unsaved()`, again in `save()`, and again in the prompt's item list. Adding an editor means editing three places; three editors were missed.
  - Externally modified files: the watcher reloads scripts (I10), but a tab with unsaved changes and a changed file has no "changed on disk" state.
  - Play saves the project first by design (ADR-0017); that stays, and is stated in the prompt docs.
- **CF10. Double-click in the Explorer only opens scripts.** `explorer.rs` sets `output.open` for scripts and toggles rows with children; Models, MeshParts, Animators/Animations, UI and other objects do nothing. The ways to open things exist but are spread out: `open_script`, `open_in_model_studio`, `open_material`, `open_block`, `EditorApp::animate`, the UI Designer's follow, Find References. There's no one "open this" action, and no "focus in viewport" command (the viewport has `focus()` but nothing frames a selection from outside).
- **CF11. Scripts inside objects can't switch views reliably** (*suspected*). `switch_script_mode` finds the **first** `ScriptFile` with the path and renames the file (`.server.lua` ↔ `.server.blocks.json`); for an object script (G2, `SourceFile` property, possibly shared by several copies) the Instances' `SourceFile` isn't rewritten, so the Instance points at a file that no longer exists. Phase 3 starts with a failing test.

### Visual scripting

- **CF12. Tooltips show Lua, not meaning.** Blocks and graph nodes hover with their Lua (`block_editor/menus.rs`, `graph/menu.rs`); NodeGraph specs (`NodeSpec`) have titles and pins but no description. There is no catalogue to say what a node does, its inputs and outputs, and an example.
- **CF13. Two IRs on purpose.** `BlockScript` is structured statements with a lossless Lua round trip; `NodeGraph` is free-form wiring for domain graphs. Merging the formats would break saved files and gain nothing the shared catalogue (CF12) and shared error type don't give. Decision in ADR-0024: keep both, share node descriptions, validation messages and the canvas.

### Physics

- **CF14. Backend and API are separated by crate, not inside it.** Rapier types appear in 12 of the crate's modules (lib 49 uses, sync 38, joints 24, vehicles 17). Engine-facing names (`PhysicalProperties`, collision groups) already live in `core`. A second backend isn't planned (policy: no infrastructure before needed), so the split worth doing is a module boundary (`physics::api` types, rapier only in backend modules), not a trait object.
- **CF15. Properties the brief lists, against what acts on the simulation today:**

  | Brief | Today |
  |---|---|
  | Mass, density, weight | Density in `CustomPhysicalProperties`; `GetMass`, `AssemblyMass`, `AssemblyCenterOfMass`, `Massless` |
  | Velocity, angular velocity, momentum | `AssemblyLinearVelocity`, `AssemblyAngularVelocity`; momentum not exposed |
  | Acceleration, deceleration | Not readable |
  | Force, torque, impulse | `VectorForce`, `Torque`, `LinearVelocity` movers; `ApplyImpulse`, `ApplyImpulseAtPosition`, `ApplyAngularImpulse` |
  | Gravity | `Workspace.Gravity` (196.2) |
  | Static and dynamic friction | One `Friction` (rapier has one coefficient) + `FrictionWeight` |
  | Rolling friction | A global `ANGULAR_DAMPING` 0.5 only |
  | Air resistance, drag | Wind drag (`WIND_DRAG`) only; no per-part drag |
  | Restitution | `Elasticity` + `ElasticityWeight` |
  | Materials | Material presets give `PhysicalProperties` (Ice, Rubber…) |
  | Constraints and joints, CCD, sleeping, stacking, filtering | Joints and movers; collision groups; far-sleep; stacking fixed by the rapier patch (#121); CCD not exposed |

### Collaboration and security

- **CF16. Previews exist, an object viewer doesn't.** See foundation E. Previews redraw every frame while shown; an Object Viewer must draw only when its camera, object or size changes.
- **CF17. No Team Projects page.** Collaborate is reached from File > Set Up Collaborate, the Collaborate panel and the start screen's Join. Persistent projects keep records (`<data>/records/<id>.json`, `app/collab_project.rs`), which a Team Projects page can list.
- **CF18. No trust preference or connection approval.** Invites are one-time secrets with expiry and revocation, members are keyed devices with roles (`access.rs`), there are budgets and an audit log (`limits.rs`, `audit.rs`). But anyone holding a valid invite gets in without the host seeing a prompt, and nothing records "always allow this member" versus "ask".
- **CF19. Listening on every interface.** Editor host, handoff host and `axcenity-collab` bind `0.0.0.0` (`app/team.rs`, `app/collab_handoff.rs`, `collab-server/src/lib.rs`). Over Tailscale only the Tailscale address is needed.
- **CF20. Firewall handling is advice only.** The setup guide tells Windows users to press Allow on Defender's prompt; on Paul's PC Defender instead added **Block** rules for a worktree's `axcenity-collab.exe`, which only an admin PowerShell could fix. Nothing detects existing rules, explains a block, or offers a scoped allow.
- **CF21. What needs a host isn't stated in one place.** ADR-0022 and `collaborate-persistent.md` know; the UI and the user guide only partly say it.

### Branding and packaging

- **CF22. The game icon is the engine's, compiled in.** `player/src/icon.rs` `include_bytes!`s `assets/branding/icon-256.png`; Build Game copies the player unchanged; the manifest has no branding fields. There is no banner. Built games have no engine-provided main menu (the loading screen is the splash or a ScreenGui), so a banner reaches a game's menu through a content path a ScreenGui's `ImageLabel` can use.

## Dependencies between the requests

```
CP2 kit pieces (menu button, toolbar, category palette, icon layout)
  ├─> CP3 Blocks bar, Explorer, prompts use them
  ├─> CP4 tooltip card is a kit piece
  ├─> CP5 Object Viewer toolbar
  ├─> CP7 Team Projects page and trust dialogs
  └─> CP9 cohesion review
CP3 document tracker ──> CP3 Explorer open action (avoids duplicate tabs) ──> CP5 viewer opens from the same action
CP6 physics module split ──> new properties and Luau members
CP7 trust store ──> approval prompt ──> firewall check (asks only when hosting)
CP8 branding is independent (project manifest + Build Game + player)
```

## Plan

One PR per phase (bigger phases may split), each from `main`, each with RED/GREEN tests, docs, wiki and the hand checks for Paul's PC or Mac listed in the PR.

### CP2. Shared UI foundation

- **CW2.1** Kit: `widgets::menu_button` and `widgets::dropdown` on `BUTTON_HEIGHT` and the role look; `widgets::toolbar_row` (one height, wraps through `responsive::toolbar`). Replace `ui.menu_button` in toolbars (Model Studio first, then Animation, Sequencer, Terrain, UV editor, Data Table). Test: every toolbar control in Model Studio has the same height.
- **CW2.2** `theme::category` palette and icons moved out of `block_editor.rs` into the kit (Events, Control, Motion, Looks, Sound, Operators, Variables, Sensing, Animation, Tweens, More); blocks keep using them unchanged; ribbon groups and graph node headers take a category colour as an accent strip (controlled, not full fills).
- **CW2.3** Menu icon layout: `Appearance::menu_icons` (Left, the default, or Centered) in Settings > Appearance; menu items draw icon and label in two columns (`widgets::menu_item`) so labels line up whether or not an item has an icon.
- **CW2.4** Ribbon: one row height for every group, captions pinned to the bottom (CF2). Test: all captions share one baseline.
- **CW2.5** Responsive fixes: Console completions capped and scrolled, the command line never taller than half the panel; Blocks bar hint truncates with a tooltip and moves to its own row when narrow; checks with drawing tests at 260 pt and a short height that no two controls overlap.
- **CW2.6** Start screen: one `PageStyle` per page (`Reading` for Learn, Documentation, API Reference; `Scenic` for Projects and Global assets) chooses the glass; on reading pages the scroll area is clipped to the glass, so text never leaves it (CF7). Test: glass on reading pages only; content clip inside glass.
- **CW2.7** Welcome page: the Scene tab can be closed; when no document tab (Scene, script, Model Studio, Material, graph tabs…) is open the dock shows a **Welcome** tab: watermark, recent projects, recently closed tabs (remembered per project), shortcuts (from the shortcut table), and actions; opening any document replaces it in place. View > Scene and Show Scene bring the Scene back.

### CP3. Editor workflow

- **CW3.1** `documents.rs`: one `DocumentTracker` the editors register with (`Document` trait: path, state Saved/Modified/ChangedOnDisk, `save`, `revert`). `unsaved()`, `save()`, Save All and the prompt's list read it. Animation clips stop writing on close and on switching; Model Studio and Sequencer count. Failed saves keep the prompt open. RED: tests that Don't Save leaves a clip file untouched, that Model Studio changes ask, that a failed save cancels the close.
- **CW3.2** `open.rs`: `OpenTarget` (Script, Model, Animation, Material, UiGui, Graph, Instance) and `EditorApp::open_target`, the one place that picks the editor and reuses an open tab. Explorer double-click, Assets, Find References and search results route through it. Scene objects are selected and framed (`Command::Focus`, F in the viewport). RED: double-click tests per class.
- **CW3.3** Object scripts switch Code/Blocks/Graph with every referencing Instance's `SourceFile` rewritten, in one undo step; the view bar shows for every Script, LocalScript (and ModuleScript as Code only, with a note). RED first (CF11).
- **CW3.4** Blocks bar from CW2.5; selection states from the kit.

### CP4. Visual scripting

- **CW4.1** `blocks-runtime/src/catalogue.rs`: `NodeInfo { name, description, purpose, inputs, outputs, lua, example }` for every `BlockKind`, reporter and `NodeSpec`, with tests that none is missing.
- **CW4.2** One tooltip card (kit) used by the Blocks palette, block canvas, Graph menu and nodes, and NodeGraph views.
- **CW4.3** Shared `Diagnostic` type for validation errors across BlockScript translation and NodeGraph compilation, shown the same way on blocks and nodes.
- *Status (CP4 PR):* CW4.1, CW4.2 and CW4.4 done. CW4.3 is deferred: most `TranslateError`s carry no block, so a shared `Diagnostic` would only rename `GraphError` today; it's worth doing once translation reports the block at fault.
- **CW4.4** Conversion limits stated where they apply (Code → Blocks keeps unknown Lua as grey blocks; NodeGraph ↔ blocks is not offered).

### CP5. Object Viewer

- **CW5.1** `editor-tools/src/object_viewer.rs` on `preview::Canvas`: a world of its own built from an Instance subtree (copied) or an asset file (model, prefab, material), orbit/pan/zoom, frame, reset, lighting presets, floor and sky toggles; draws only on change.
- **CW5.2** Opened from Explorer (context menu and the open action), Assets, Toolbox (downloaded items), Model Studio; one dock tab reused.
- *Status (CP5 PR):* CW5.1 done for Instances, models, prefabs and materials (on a ball). CW5.2: Explorer's **View** item and Assets' **View** item open the one tab; the Explorer's double-click keeps framing in the Scene (CP3), since a part is edited there. Toolbox items reach it once installed, through their Assets tile; Model Studio already has its own orbit view, so it isn't hooked.

### CP6. Physics

- **CW6.1** `physics/src/api.rs` backend-neutral types; rapier stays in backend modules; doc page with the five layers.
- **CW6.2** Properties that act on the simulation: per-part `LinearDrag`/`AngularDrag` (air resistance; rapier damping), `RollingFriction` in `PhysicalProperties` (angular damping while touching), static friction via the contact hook only if measured cheap; readable `AssemblyAcceleration`, momentum helpers; `ApplyForce` for one step; CCD switch (`ContinuousCollision`). Every one with a test that it changes motion.
- **CW6.3** Properties panel groups them; Luau definitions and API pages regenerated; benchmark at 1k/5k/10k bodies before and after.

- *Status (CP6 PR):* CW6.1 is the module boundary: `physics/src/api.rs` holds the engine-facing constants, `SyncStats` and `gravity` (a test keeps rapier out of it), and `docs/architecture/physics.md` names the five layers; rapier stays in the other modules, with no backend trait. CW6.2: `LinearDrag`, `AngularDrag` (0.5, the old global spin slowing, now a property) and `RollingFriction` (a part property rather than a `PhysicalProperties` field, so the Lua constructor and saved values don't change), `ApplyForce`/`ApplyTorque(…, seconds?)` and `AssemblyLinearMomentum`, each tested against motion. Not done, with reasons: a `ContinuousCollision` switch (rapier's speculative contacts stop fast parts with CCD off too, so no test could show it doing anything; CCD stays on), static friction (one rapier coefficient; the contact hook would run for every pair), readable acceleration (one more write per moving part per frame). CW6.3: the Physics group in Properties lists them (under Advanced for parts), the Luau definitions and API pages are regenerated, and `step_cost` measured 1k/5k/10k bodies before and after (`docs/benchmarks.md`).

### CP7. Team Projects and security

- **CW7.1** Start screen **Team Projects** page: new, convert local, open (records), join, recent, settings; the existing wizard behind it, with a GitHub step (connect or create, branch, sync) on the `repo` crate.
- **CW7.2** Trust preferences (Always trust known members, Trust on join, Ask every time) in Settings > Collaborate, persisted; host-side approval prompt with who, device, role and address; revocation of remembered trust.
- **CW7.3** Listen on the Tailscale address (or the LAN address picked) instead of every interface.
- **CW7.4** `firewall.rs`: read-only detection per platform (Windows `netsh advfirewall firewall show rule`, Linux `ufw`/`firewall-cmd` if present, macOS `socketfilterfw --getappblocked`), explained in the wizard; an elevated, program-and-port-scoped rule only on the user's click (Windows), never at startup.
- **CW7.5** One "How Collaborate works" page (host required or not, disconnects, files, GitHub, Tailscale, what's shared, roles, conflicts, offline, external services) in the user docs and the wizard.

### CP8. Game branding

- **CW8.1** Manifest `branding` (banner, icon, icon pack folder), validated (PNG/JPEG, sizes 16–1024 square for icons, banner aspect and minimum size); Project > Branding page with previews.
- **CW8.2** Build Game copies them; the player uses the game's icon (falls back to Axcenity's); Windows `.ico` and Linux `.desktop` icon written with the build; `branding://banner` content path for ImageLabels.

- *Status (CP8 PR):* CW8.1 done: `branding.icon` and `branding.banner` in the manifest, checked for format, size and shape, edited in Settings > This project > Branding with previews (the "Project > Branding page" is that section, beside the project's other settings). An "icon pack folder" was left out: every size is made from the one icon at build time, so a pack would only add files to keep in step. CW8.2 done: the player's window uses the game's icon (Axcenity's when unset or bad), Build Game writes `.ico`, `.icns` and a PNG with the game's name in the macOS app and Linux launcher, and `branding://banner` (and `branding://icon`) resolve in `Assets`. Not done, with reasons: the Windows `.exe`'s own file icon (a PE resource; changing it means rewriting the player's resources) and an `Icon=` in the Linux desktop entry (only absolute paths or theme names work there). Details: `docs/project/export.md`.

### CP9. Final validation

- UI review with snapshots in light/dark/compact at 1280 and 260-pt panels; performance check (editor idle frame, physics bench); full test run; final report.

## Known limits of this pass

- Two-machine Collaborate checks, Windows firewall behaviour and GPU timings need Paul's PC or Mac; each PR lists them.
- No hosted service (Django) work: the identity seam from C12 stays the extension point.
- A second physics backend isn't built; the split makes it possible later.
