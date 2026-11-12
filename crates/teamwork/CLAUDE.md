# `teamwork`

- The project's shared board (`docs/research/teamwork.md`, editor side `docs/editor/team-board.md`): items (`item.rs`: kinds, statuses, assignees, links, checklist, comments, events, `merge`), the board as files in `team/board/` (`board.rs`: load, save, delete, filter, merging the copies two people's changes leave), dates without a date library (`time.rs`).
- No engine, egui, `collab` or `repo` dependency: everything is plain files, which sessions and Git already carry. Keep the JSON's field order stable (Git merges fields on separate lines).
- People are Collaborate names; compare them with `item::same_person`. Tests (`tests.rs`) use temp folders.
