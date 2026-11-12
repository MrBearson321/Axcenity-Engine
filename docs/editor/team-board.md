# Team Board

The project's shared tasks, bugs and goals and who they're assigned to (T1–T3 of [../research/teamwork.md](../research/teamwork.md)). Data: the `teamwork` crate. Panel: `crates/editor/src/team_board.rs` (draws, returns `BoardAction`s). App side: `crates/editor/src/app/team_board.rs` (reads the board, writes items, notices new assignments). Tests: `crates/teamwork/src/tests.rs`, `crates/editor/src/app/team_board_tests.rs`.

## Files

- One item per file: `team/board/<id>.json`, pretty JSON with a fixed field order. The board is only these files; delete the folder and the board is empty.
- Written through `<id>.json.tmp` and renamed; an unchanged item isn't rewritten.
- `<id>.<anything>.json` is another version of item `<id>`: a session's losing save (`.conflict-<who>`) or the repository's Keep both (`.remote-<commit>`). `Board::load(.., Copies::Merge)` merges it into the item and removes it. The editor passes `Copies::Leave` on a member's copy of a session and while the repository is in Conflict.

## Sharing

- In a Collaborate session the mirror shares board files like any other save; files that arrive go through `disk_changed` (`editor_reads` includes `board::is_board_path`), which marks the board stale; it's read again next frame.
- The watcher (`project::watch`, `Watched::Everything`) covers `team/`, so a Git pull or another program's change is read again too.
- Board files are assets to the session: a Viewer (`!Capability::EditAssets`) gets a read-only board.

## Panel

- Tab beside the Scene; Window > Team Board, Ctrl+K, and the Collaborate panel's Team Board line open it.
- List (grouped by status) or Board (columns To do, In progress, In review, Done; Won't do shows under Done). Below 520 points wide, Board shows the list. Cards drag between columns (`dnd_drag_source` / `dnd_drop_zone`).
- Goals (T3): the Goals view (each goal, its progress bar and open items; `BoardPanel::goals`), My work (`Board::my_work`, under Overdue / This week / Later / No date), the Goal row in a task's or bug's window (`goal_picker`) and a goal's Progress section with **Add a task to this goal** (`BoardAction::AddToGoal`). List and Board leave goals out unless the Goals chip is on. Deleting a goal saves its items without it first.
- Filters: Everyone, Mine, Unassigned, a person, a label, Tasks and Bugs, words, and Done (finished items).
- Bugs (T2): the add field's kind menu makes a Task or a Bug. A bug's window adds Severity, Found in (This build fills `found_in()`), Fixed in, Steps to reproduce, Expected and What happens; statuses read Open … Won't fix. **Report the last error** (`BoardAction::ReportLastError`, shown when Bug is chosen and the Output has an error) and the Explorer's **Report a Bug…** (`explorer::Action::ReportBug`, `report_bug_about`) make a bug with its link and open it.
- Clicking a row or card opens the item's window (one at a time): fields commit when a text field loses focus; status, priority, assignees and the checklist commit at once.
- People to assign: `Board::people` (everyone the board names) plus the session's roster and this computer's Collaborate name (`EditorApp::board_me`; "Tester" in tests unless `team_board.me` is set).
- A new assignment by someone else shows once as a toast and an Output line (`notice_new_assignments`).
