# `updater`

- `axcenity-updater` (`main.rs`) and the editor's update steps: download with resume and hash checks (`download.rs`, real HTTPS behind the `http` feature), safe unpacking (`extract.rs`), staging and hand-over (`stage.rs`), and the swap, check and roll back after the editor exits (`apply.rs`). Design: `docs/research/release-system/architecture.md` §7; user-facing: `docs/editor/shell.md`.
- No egui and no engine crates besides `release`: it must stay a small binary that each release ships beside the editor.
- Only touch the install folder, its `.new`/`.previous`/`.failed`/`.rollback` siblings and `Updates/`; `tests.rs` checks fake user data around the install is byte-for-byte unchanged. Keep every outside effect behind `apply::Host`.
