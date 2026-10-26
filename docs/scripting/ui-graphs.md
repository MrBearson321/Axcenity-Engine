# UI Event Graphs

A ScreenGui's **Event Graph**: Unreal Blueprint-style nodes and wires that say what its buttons and text boxes do. It compiles to an ordinary LocalScript, so there is still one Lua runtime, and the player needs nothing new. `examples/hello-ui-graph` is a worked example (Settings and Back switch a PageLayout's pages).

## File and LocalScript

- Each ScreenGui has at most one graph, `ui/<ScreenGui Name>.uigraph.json`: `{ "format_version": 1, "nodes": [...], "wires": [...], "comments": [...], "variables": [...] }`. A node is `{ "id", "kind", "position": [x, y], "values": { field or pin name: text } }`; a wire is `{ "from": { "node", "pin" }, "to": { "node", "pin" } }` (an output to an input; `""` is a node's plain exec pin). `comments` and `variables` are left out when empty. It's a new optional file, so `project::FORMAT_VERSION` didn't change.
- Loading (`project::load`, `crates/project/src/ui_graphs.rs`) compiles each graph against the objects there are now and puts a **LocalScript called `EventGraph`** under the Workspace's ScreenGui of that name (`script.Parent` is the ScreenGui), after the project's other scripts. Its `ScriptFile::path` is the graph's path, so errors name `ui/Menu.uigraph.json:<line>`. The Lua is never saved: like block scripts, the graph file is the source. A graph that doesn't compile, or has no ScreenGui, is a load problem and has no LocalScript.
- Built games carry `ui/` as it is (`pack_game`), and the player's loader compiles it the same way.
- In the editor the LocalScript can't be edited, renamed, deleted or duplicated by itself: opening it (or an Output link to it) shows the graph, and the Explorer's rename, delete and duplicate say why and show the graph. Renaming the ScreenGui renames the graph's file at the next save.

## Nodes (`crates/blocks/src/ui_graph.rs`, `UI_NODES`)

Objects are picked from menus: elements by their path under the ScreenGui (empty is the ScreenGui itself), Sounds by their path under the Workspace, RemoteEvents under ReplicatedStorage. The Lua names them the way blocks do (`gui.Pages.Home.Play`, `gui["Name Box"]`).

| Category | Node | Lua |
|---|---|---|
| Events | Button Activated *button* | `gui.Path.Activated:Connect(function() … end)` |
| | Hovered *element* | `….MouseEnter:Connect(function() … end)` |
| | TextBox FocusLost *box* (its **Text** out) | `….FocusLost:Connect(function() … end)`; Text is `box.Text` |
| | Screen Shown | `screenShown()` run at start when `gui.Enabled`, and each time `Enabled` turns on (`GetPropertyChangedSignal("Enabled")`) |
| | Every Frame (its **Delta Time** out) | `RunService.Heartbeat:Connect(function(deltaTime) … end)` |
| Actions | Set Property *object*, *property*, Value | `object.Property = value` |
| | Show Page *PageLayout*, Page | `pages.CurrentPage = page` |
| | Set Visible *element*, Visible | `element.Visible = v` (`Enabled` for the ScreenGui) |
| | Tween *object*, *property*, *style*, *direction*, Goal, Seconds | `TweenService:Create(object, TweenInfo.new(s, Enum.EasingStyle.…, Enum.EasingDirection.…), {Property = goal}):Play()` |
| | Play Sound *sound* | `workspace.Path:Play()` |
| | Fire RemoteEvent *remote*, Value | `ReplicatedStorage.Path:FireServer(value)` (no argument when Value is empty and unwired) |
| | Print Value | `print(value)` |
| Flow | Branch Condition → True, False | `if c then … else … end` |
| | Sequence → Then 0, Then 1, Then 2 | each in turn |
| | Delay Seconds → Completed | `task.wait(s)` then Completed |
| Data | Get Property, Get Text | `object.Property`, `element.Text` |
| | Math (+ − × ÷ % ^), Compare (< ≤ = ≠ > ≥), Join Text | `(a op b)`, `(a .. b)` |
| Variables | Get Variable, Set Variable | `name`, `name = value`; each variable is `local name = start` at the top |

- Pin types: exec (white), number (green), text (pink), true or false (red), any (grey). A wire must fit: any fits anything, and numbers and true-or-false fit text (as `tostring(…)`). An exec output and a value input take one wire; a wire that would loop is refused.
- An unwired input uses the value typed in its box. A number box must hold a number; an **any** box reads a number, `true` or `false`, else text. Inputs marked required (Set Property's and Set Variable's Value, Tween's Goal, Show Page's Page) must be wired or typed.
- Only chains wired to an event run, in node id order, each handler as its own coroutine (so Delay waits only its chain). An event with nothing after it makes no Lua. A value from an event (Delta Time, a FocusLost's Text) is only known in that event's chain.
- Services (`RunService`, `TweenService`, `ReplicatedStorage`) are declared at the top only when used. Variable names must be Lua names and not ones the graph's Lua uses.

## Errors

`compile_ui_graph` returns every problem with its node (or none, for the graph as a whole): an object not picked, gone, or of the wrong class; an unwired required input; a value that isn't a number or true or false; a value from another event; an unknown node kind or variable; a loop. A graph with problems never runs: the editor shows each on its node (red outline and the message under its rows), lists them beside the graph, says so in Output when it's saved, and at Play says `ui/Menu.uigraph.json won't run: its Event Graph has problems…` instead of starting it, so there are no Lua errors from it.

## In the editor

- The UI Designer's **Design · Event Graph** tabs. Event Graph edits the shown ScreenGui's graph in the generic graph editor (`crates/editor/src/node_graph.rs`; behavior trees use it too, `docs/architecture/navigation.md`): right-click (or **Add node**) for the searchable node menu, drop a wire on the canvas for nodes that fit it (wired to it), drag between pins to wire (Alt-click or right-click a pin breaks), click or box-select nodes, drag to move, Delete, Ctrl+D, Ctrl+C / Ctrl+X / Ctrl+V (as text, so between ScreenGuis too), **Comment** around the selection (drag its title to move it with what's inside, its corner to resize, double-click to rename), right- or middle-drag and the wheel to pan, Ctrl + wheel to zoom, Frame all.
- Beside it: the graph's **Variables** (name and starting value), whether it's ready to run or its problems, and **Lua · View**, the LocalScript it becomes (read-only).
- Each finished edit (a drag, a typed value, a wire) is one step of the graph's own undo: Ctrl+Z and Ctrl+Y act on the graph while the pointer is over it, and on the world otherwise. Graphs save with Ctrl+S (a ScreenGui's first graph is only written once it has a node), which makes their LocalScripts again; a new graph's LocalScript is added under its ScreenGui then.
- **Live Preview** (beside the tabs) copies the world and runs only this graph's Lua there (`ui_designer/event_graph.rs`, `LivePreview`), shown on the artboard: clicking a button fires `Activated`, hovering fires `MouseEnter` and `MouseLeave`, and time moves on for Delay, tweens and Every Frame. What it prints goes to Output as `Live Preview: …`. Nothing it does reaches the world being edited; it starts again when the graph changes and stops for Play or another ScreenGui. A graph with problems doesn't start, and the Event Graph tab shows why. Typing into TextBoxes and sounds aren't previewed.

## Code

- Data and edits (generic): `crates/blocks/src/node_graph.rs` (`NodeGraph`, `NodeSpec`, `NodeLibrary`, `connect`, `copy`, `paste`).
- UI nodes and the compiler: `crates/blocks/src/ui_graph.rs`; tests in `ui_graph_tests.rs`.
- Loading and objects: `crates/project/src/ui_graphs.rs` (`graph_objects`, `compile_graph_for`).
- Editor: `crates/editor/src/node_graph.rs` (with `draw.rs`, `menu.rs`, `variables.rs`), `ui_designer/event_graph.rs`, `app/ui_graphs.rs` (documents, saving, undo); Play reads graphs in `play::read_scripts`.
