---
name: organize
description: Tidies the Axcenity Engine repo in two passes. First it syncs the docs and steering files (CLAUDE.md, crate CLAUDE.md files, docs/ topic files, docs/README.md map) with the current code and recent history. Then it removes dead code and splits oversized source files without changing behavior. Use when asked to "organize", "clean up the repo", "tidy the docs and code", or after a stretch of feature work.
---

# Organize the Axcenity Engine repo

Two passes, in order: **docs** first, then **code**. Behavior must not change in either. Source and tests are authoritative when docs disagree.

## Pass 1: Docs and steering files

1. **Gather what changed.** Run `git status --short`, `git log --oneline -20`, and `git diff --stat`. Use the current conversation as extra context for what was built and decided. Do not read plans or PRDs in full.
2. **Map changes to docs.** For each touched crate, open its `crates/<name>/CLAUDE.md` and the one topic file `docs/README.md` points to for that area. Open only those.
3. **Update, don't expand.**
   - Fix statements the code no longer matches; add new behavior to the topic file, not to a crate `CLAUDE.md`.
   - Each crate `CLAUDE.md` stays under 10 lines: what the crate is, which topic file to open, local constraints.
   - Every crate under `crates/` has a `CLAUDE.md`; every topic file is listed in the `docs/README.md` table. Add or remove rows as files appear or go away.
   - Root `CLAUDE.md`: keep invariants and commands accurate (check commands against `Cargo.toml` and `README.md`). Keep it short.
   - Check `README.md` run instructions and the `.claude/skills/*` references still name files that exist.
4. **Memory.** If the session revealed a durable preference or project fact that is not derivable from the repo, save it to the memory directory per the memory rules. Drop memories that are now wrong.
5. **Verify links.** Grep the edited docs for relative links and confirm each target exists.

## Pass 2: Code cleanup

Work crate by crate. Keep each change behavior-preserving and small enough to review.

### Find dead and unneeded code

- Run `cargo clippy --workspace --all-targets -- -D warnings` and fix dead-code, unused import, and unused dependency warnings it reports. Note pre-existing noise (see memory) and leave untouched files alone.
- Grep for `pub` items with no users outside their definition (`Grep` the name across `crates/`, `examples/`, `definitions/`). Remove only when there are no callers, no Lua/definitions exposure, no serialized-format role, and no test use.
- Remove commented-out code, stale TODOs that are done, duplicate helpers (merge into the existing one), and unused workspace dependencies.
- Do not remove: public Lua API surfaced in `definitions/axcenity.lua`, file-format fields kept for compatibility, or code the user is mid-way through (uncommitted work in files you did not touch for cleanup). When unsure, list it in the report instead of deleting.

### Split long files

- Find candidates: `find crates -name '*.rs' | xargs wc -l | sort -rn | head -20`. Treat roughly 1500+ lines as a candidate; prefer files with several unrelated responsibilities over one cohesive long file.
- Split along existing seams (a `mod` per responsibility, tests into `tests.rs`, large `impl` blocks by feature). Follow how sibling modules are already laid out, for example `app.rs` with `app/*.rs`.
- Move code verbatim. Do not rename, reorder logic, or "improve" it in the same step. Keep public paths stable with `pub use` re-exports so callers don't change.
- Respect invariants in the root `CLAUDE.md` (`player` independent of `editor`/`egui`/`blocks`, `render` independent of Instances/scripts).
- Split at most a few files per run; say which remain.

### Verify

After code changes run `cargo fmt --all` only on files you edited, then `cargo build --workspace`, `cargo test --workspace`, and `cargo clippy --workspace --all-targets -- -D warnings`. Report failures with output; separate pre-existing failures (see memory) from ones you caused. If something you changed fails, fix or revert that change.

## Rules

- Do not commit or push unless asked.
- Do not touch plans, PRDs, or acceptance criteria.
- Do not change behavior, file formats, or public APIs. Anything beyond tidy-up goes in the report as a suggestion.
- The working tree is often dirty with the user's in-progress work. Never revert or reformat changes you did not make.

## Final response

Short report: docs updated (files), code removed (what and why), files split (before/after line counts), verification results, and items skipped or needing the user's decision.
