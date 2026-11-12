# Teamwork: a shared board, bug tracking, goals and each person's history

Paul's request of 2026-11-12 ("Expand developer cooperative interactions"): goal trackers, assignments, bug tracking and more, based lightly on ClickUp and other productivity tools, and per-developer version tracking and history, to see every change a person has made on a project. The phases here are **T1–T4**, named T so they don't clash with C1–C12 ([collaborate.md](collaborate.md)), P0–P7 ([collaborate-persistent.md](collaborate-persistent.md)) or the roadmap's G and R items.

## 1. What exists, and what's missing

Checked on `main` at `1003619` against the roadmap ([competitive-development-roadmap.md](competitive-development-roadmap.md) §7.2 and §11.1) and the gap analysis (G1–G16):

- **Built:** live sessions with chat and cursors (C8), session checkpoints that name who changed things (C9), the Owner's audit log (C10), Git-backed checkpoints whose message lists the participants (`Axcenity-Participants: Paul, Sam`, P2), conflicts kept both ways (P5), Version History on one computer (Phase 72).
- **Missing:** anything to plan or divide work (tasks, assignments, due dates), a place for bugs, goals with progress, and any view of what *one person* changed. No G or R item covers these; roadmap §7.1 lists "team management" under accounts (R19), which this doesn't need.

## 2. Principles

- **Plain files in the project.** The board is `team/board/<id>.json`, one item per file, pretty JSON with a fixed field order. Nothing new travels between computers: a Collaborate session's mirror shares a changed file like any save (`collab::Mirror`, TC5), and the project's repository keeps its history (P2). A project with no session and no repository still has a board on its one computer.
- **No server, no account.** People are their Collaborate names (the identity's name, the same names checkpoints list as participants). Two people with the same name are one person on the board; that's the trade for needing no account (§6).
- **Small files merge.** One file per item means two people rarely touch the same file. When they do, the session keeps the losing save as `<id>.conflict-<who>.json` and the repository's **Keep both** leaves `<id>.remote-<commit>.json`; the board merges such a copy into its item and removes it (§3.2).
- **Roles still hold.** Board files are assets to the host (`Capability::EditAssets`), so a Viewer sees the board and can't change it, and the host refuses a Viewer's file anyway.
- **Editor only.** `player` and games never read `team/`.

## 3. T1: the Team Board, tasks and assignments

### 3.1 Data (`crates/teamwork`)

An item (`item.rs`) has an ID (10 letters from 32, from the time and a per-process random seed, so IDs made on two computers between syncs don't clash), kind (Task; T2 adds Bug and T3 Goal), title, description, status (To do, In progress, In review, Done, Won't do), priority (Urgent, High, Normal, Low), assignees, labels, due date (`YYYY-MM-DD`), links (an Instance by its path, a file and line), a checklist, comments, and events (created, moved, assigned, unassigned, priority, due date, renamed), with who made it and who changed it last and when. Text is tidied (control characters out, length capped) as chat is.

`Board` (`board.rs`) reads `team/board/`, writes an item through a temporary file (`*.tmp`, which a session never shares), leaves an unchanged item's file alone, deletes, filters (whose, kind, label, words, finished) and sorts (status, priority, due date, newest). Damaged files are listed in `problems` and the rest still load.

### 3.2 Two people, one item

`item::merge` puts a copy into its item: comments, checklist lines, links and events from both are kept (by ID); the other fields come from whichever side changed last (`updated_at`); who made it stays. `Board::load` with `Copies::Merge` merges every `<id>.<something>.json` into `<id>.json`, writes it and removes the copy. The editor merges only where one computer should: not on a member's copy of a session (the host merges and the result comes back), and not while the repository is waiting for someone to resolve a conflict (P5's copies must stay until then).

### 3.3 In the editor

- **Team Board tab** (`crates/editor/src/team_board.rs`, beside the Scene; Window > Team Board, Ctrl+K, and the Collaborate panel's Team Board line): a field to add a task; List (grouped by status) or Board (a column per status, cards dragged between them; a narrow panel shows the list); filters Everyone / Mine / Unassigned / a person / a label, words, and Done; "N assigned to you, M overdue".
- **Item window**: title, status, priority, assignees (from the board's people, the session's roster and this computer's name, or typed), due date (typed, Today, +1 week), labels, description, checklist, links (the selected Instance or the open script; clicking one selects it or opens it), comments, activity, and Delete with a confirmation.
- **Saving**: each change writes the item's file at once (`app/team_board.rs`); there's no undo for board changes (the activity list says what happened, and the repository keeps every version).
- **Arrivals**: the watcher now also watches `team/`, and the session's `editor_reads` includes board files, so a changed item is read again. When someone else assigns this computer's person an item, a toast and an Output line say so once.

### 3.4 Tests

`crates/teamwork/src/tests.rs`: round trip and the readable file, a damaged file, two people's changes to one task (session copy and repository copy, merged; left alone when asked), filters and order, tidying. Editor `app/team_board_tests.rs`: a task made, assigned, moved, linked and deleted through the panel's actions and read back by another editor (snapshots `team-board-list`, `team-board-columns`, `team-board-task`); a teammate's task reaching the host over real sockets with "Sam assigned you …", and the host's change coming back; a Viewer seeing the board but changing nothing.

## 4. T2: bug tracking

- **Data:** `Kind::Bug` with `BugInfo` (`item.bug`, absent on tasks so a task's file is unchanged): severity (Blocker, Major, Minor, Trivial), steps to reproduce, expected, what happens, where it was found (`found_in`, "0.2.0-alpha.1 on linux": the build's version and the OS) and what fixed it (`fixed_in`: a checkpoint, commit or version, typed). A severity change is an event like a priority change. Bugs share the five statuses, named for bugs: Open, In progress, Fix in review, Fixed, Won't fix. `Item::bug_from_error` makes a bug from an error line: the first line (cut to 90 characters) is the title, the whole message is "what happens".
- **Board:** the Tasks and Bugs chips filter by kind; within a status, bugs sort by severity after priority. `Board::open_bugs` gives "N open bugs, M blocking" over the list.
- **Reporting:** the add field has a kind menu (Task or Bug). With Bug chosen and an error in the Output, **Report the last error** files it, linked to the script and line the message names. The Explorer's row menu has **Report a Bug…**, which links the Instance. Each opens the new bug's window.
- **Tests:** `a_bug_reports_what_happened_and_sorts_by_severity` (crate), `bugs_are_reported_from_the_last_error_and_from_the_explorer` (editor; snapshots `team-board-bug`, `team-board-bugs`).

## 5. T3: goals and milestones

- **Data:** `Kind::Goal`; its due date is the target. A task or bug names the goal it counts toward in `item.goal` (the goal's ID; absent otherwise, so other files are unchanged). `Item::set_goal` refuses a goal under a goal and anything under a non-goal, and logs "put it under “Alpha build”" / "took it out of its goal". Goal statuses read Planned, Under way, Wrapping up, Reached, Dropped. `merge` takes `goal` from the newer side like the other fields.
- **Board:** `Board::goals` (unfinished first, by target), `in_goal`, `progress` (`Progress { done, total, overdue }`, `fraction`), and `my_work(person)`: their unfinished tasks and bugs, soonest due first, no date last. `open_for` counts the same items.
- **Panel:** two more views. **Goals** shows each goal with a progress bar ("1 of 3 done · 1 overdue · Nov 22", amber when anything is late, green when all is done) and its open items under it, and how many open items are in no goal. **My work** groups this computer's person's items under Overdue, This week, Later and No date. List and Board leave goals out unless the Goals chip is on. A narrow panel shows the four views as icons. A task's or bug's window has a Goal row; a goal's window has Target for Due, its progress, its items (click to open) and **Add a task to this goal** (`BoardAction::AddToGoal`, which keeps the goal's window open). Deleting a goal takes its items out of it first.
- **Tests:** `a_goals_progress_is_its_items_and_my_work_comes_soonest_first` (crate), `a_goal_counts_its_tasks_and_my_work_lists_mine_soonest_first` (editor; snapshots `team-board-goal`, `team-board-goals`, `team-board-my-work`, `team-board-list-without-goals`).

## 6. T4: each person's history

Every change a person made, kept in the project so the team can see it.

- **Files:** `team/history/<person>/<YYYY-MM>.<computer>.jsonl` (`history.rs`), one JSON line per change: when, who, what kind (World edit, file, save, board), a sentence, links to the Instances and files it touched, the board item, a save's sentences, and a count. `<computer>` is this computer's Collaborate key's short fingerprint, so **each computer appends only to its own files**: two computers never change the same file, and Git never has to merge history. (We planned Git's `union` driver for one shared file per month, but sync checks merges with `git merge-tree`, and own files make that question go away.) `History::load` reads them all, skips a damaged line (listed in `problems`), keeps a copied line once, and keeps lines of the same second in the order they were written.
- **Who records what:** in a Collaborate session the hosting editor reads the host's log every 2 s (`harvest_host_log`): each transaction becomes a line by its author's Collaborate name, named by its undo label ("Rename to Gate") and linking the Instances it touched (by path when it's read) and the files it changed; a board file's change becomes a board line. Members record nothing. `team/history` doesn't sync in a session (`collab::files::is_synced`); the host's lines reach the others through the project's repository. Outside a session the editor records its own person: each Save as the version's change sentences from Version History (`project::version_changes`; "Saved the first version" when there's nothing before it) and each board change as it's made (`history::board_entry`, from the item's newest event).
- **Folding:** `Recorder` folds the same change by the same person within 60 s into one line with a count (a drag is dozens of transactions). Lines still folding are written when eframe saves, which it does when the window closes.
- **People window** (`people_history.rs`, `app/people_history.rs`; the People button in the Team Board toolbar and on the Collaborate panel's Team Board line): a chip per person (the most recently active first), "N changes in the last 7 days · last 2 min ago · 3 open on the board", filters World / Files / Saves / Board, and the lines by day (Today, Yesterday, Nov 10): a count for folded lines, links that select an Instance or open a file, a board line that opens its item, and a save's changes in a fold-out.
- **Tests:** `each_persons_history_is_kept_per_computer_and_read_back_by_day` (crate), `each_persons_saves_board_changes_and_session_edits_are_kept_and_shown` (editor: saves and board changes outside a session, then a member's renames recorded by the host over real sockets; snapshots `team-board-people`, `team-board-people-session`), and `is_synced` now refuses `team/history`.
- **Limits:** a line's Instance path is the one it had when recorded (a later rename doesn't change old lines). An editor that's killed loses lines still folding (at most a minute). Checkpoints and commits aren't listed per person yet; Collaborate's History window and the repository still name participants.

## 7. Not doing (now)

Custom statuses and fields, time tracking, recurring tasks, notifications outside the editor (email, push), and per-person permissions on the board beyond roles. Names are people (§2): a person who renames themselves shows as two until an alias file is needed.

## 8. Sources

- ClickUp's task model (statuses, assignees, checklists, comments, activity), taken lightly as Paul asked: <https://clickup.com>
- Git's `union` merge driver for append-only files: <https://git-scm.com/docs/gitattributes#_built_in_merge_drivers>
