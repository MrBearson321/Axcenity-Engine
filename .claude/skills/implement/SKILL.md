---
name: implement
description: Implements a phase or feature of the Axcenity Engine from plans/axcenity-engine-v1.md, with the matching user stories from prd/. Checks the previous phase first, asks about decisions the plan leaves open, builds and verifies the work, ticks the acceptance-criteria boxes it can prove, and tells the user exactly how to test the rest. Use this whenever the user wants to start, continue, finish or tick off a phase ("start phase 4", "do the next phase", "implement physics", "tick phase 3 and move on", "/implement 5"), or asks to build any feature that the plan describes, even if they don't mention the plan.
---

# Implement a plan phase

The plan (`plans/axcenity-engine-v1.md`) splits the engine into numbered phases. Each has "What to build" and checkbox acceptance criteria, and links to user stories in the PRD (`prd/axcenity-engine-v1.md`). This skill takes one phase from unticked to built, verified and documented. The developer is new to Rust, so they rely on the boxes being honest and on clear testing instructions.

## 1. Find the phase

Run from the repository root:

```bash
python .claude/skills/implement/scripts/plan.py status       # every phase, ticked/total, and the next one
python .claude/skills/implement/scripts/plan.py show <N|next>  # one phase plus the full text of its user stories
```

Use `show` rather than reading the plan or PRD in full. Both are large, and the phase section plus its stories is what matters. Grep the PRD for a specific section only when the phase leaves something unclear (for example "Out of scope" or the architecture notes).

Interpret the argument:
- A number means that phase. "next" or nothing means the first phase with unticked boxes.
- A feature name ("physics", "glTF import") means the phase whose title matches.
- If the request isn't in the plan at all, don't quietly build it. Ask whether to add it as a new phase or fold it into an existing one, because the plan is the shared record of what gets built when.

## 2. Close out the previous phase first

Later phases build on earlier ones, so an unverified earlier phase is a shaky foundation. If the previous phase has unticked boxes:

1. Verify each one using step 5 (tests, commands, screenshots) and tick what you can prove.
2. Check its CI with `gh run list --limit 3` (and `gh run view <id> --json jobs --jq '.jobs[] | "\(.name): \(.conclusion)"'` for per-OS results). The plan says a phase isn't done until CI is green on Windows, macOS and Linux.
3. If boxes remain that only a person can check, list them with how to check, and ask whether to continue anyway.

Also run `git status`. Uncommitted work that isn't yours might be the user's work in progress, so mention it rather than mixing your changes into it.

## 3. Load context, then find the open decisions

Read what `CLAUDE.md`'s table says the work touches: the relevant `docs/*.md`, plus `crates/<name>/CLAUDE.md` for each crate involved (and its source files). The hard rules in `CLAUDE.md` (dependency directions, the arena, one scripting runtime, pinned and Apache-compatible dependencies, no single-OS assumptions) are architecture constraints, not style preferences. A design that breaks one is the wrong design.

Then list the decisions the phase leaves open: names, file formats, controls, how two features interact, which library to use. Sort each one:

- **Ask the user** when the choice is visible to creators (property names, file format, controls, Lua API shape); when the plan, PRD and docs disagree or are silent on something that matters; or when it's expensive to undo (on-disk formats, public APIs, new dependencies). Use `AskUserQuestion` before writing the affected code. Batch up to four questions in one call, give 2–4 options each with trade-offs, and put your recommendation first, labeled "(Recommended)". `CLAUDE.md` asks for exactly this.
- **Decide yourself** when it's internal and easy to change later. Pick the simplest idiomatic option, then record it in the right doc and in your final report so the user can overrule it.

If you're unsure which group a decision belongs in, ask. A question costs the user a few seconds, while rebuilding a file format costs hours.

## 4. Build it

- Build what the phase says, and only that. Later phases have their own criteria and tests, and work that jumps ahead goes unreviewed. When a reasonable design needs a hook for later, add the hook and leave the feature for its phase.
- New crate: create `crates/<name>/CLAUDE.md` (under 20 lines: purpose, key types, gotchas), and add the crate to the workspace dependencies.
- New dependency: pin it in `[workspace.dependencies]` and check its license is Apache-2.0-compatible (MIT, Apache-2.0, BSD, Zlib; no GPL).
- Write simple, idiomatic Rust with comments that explain *why*, and match the surrounding code. When you use something a Rust beginner may not know (lifetimes, a trait bound, a borrow-checker workaround), explain it in a one-line comment.
- Give each acceptance criterion a test where one is possible, named after the behavior (for example `moves_at_the_same_speed_at_any_frame_rate`). These tests are the evidence you'll cite when ticking boxes.
- Keep examples in `examples/` working, and add one when the criteria mention an example scene or project.

## 5. Verify

Run all of these, and fix anything that fails before moving on:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Unit tests can't catch everything, so for runtime and visual criteria, run the program too:
- Run the player (or editor) on the relevant example with a timeout, and check the log for errors and GPU validation messages: `RUST_LOG=info timeout 8 ./target/debug/axcenity-player examples/<name>` (exit code 124 means it ran until the timeout, which is what you want).
- For anything visual on Windows, take a screenshot and look at it:
  `powershell -ExecutionPolicy Bypass -File .claude/skills/implement/scripts/screenshot.ps1 -Exe target/debug/axcenity-player.exe -Arguments examples/<name> -Out <scratchpad>/shot.png`, then Read the PNG. On other OSes, leave visual checks to the user.

## 6. Tick boxes honestly

Tick a box (`- [ ]` → `- [x]`) only when you have evidence from this session:
- a passing test that checks that criterion,
- command output that shows it, or
- a screenshot you looked at that shows it.

Leave a box unticked, and explain how to check it, when it needs:
- human judgment ("moves smoothly", "a beginner can…", "matches a reference screenshot" without a reference),
- other machines or OSes ("on all three OSes", "tested on real machines"), or
- CI, which needs a push.

An unticked box with clear instructions is useful. A ticked box that's wrong is worse than useless, because the next phase builds on it.

## 7. Update the docs

- `docs/*.md`: record each decision you made in step 3, where the next person working in that area will look.
- `crates/<name>/CLAUDE.md`: add new modules, key types and gotchas you hit. Keep each under about 20 lines.
- `CLAUDE.md`: update the **Status** line (and the crate list if it changed). Keep it a short index.
- `README.md`: update the status line and how to run or try the new feature.

## 8. Report

Finish with this structure, written for a developer who is new to Rust:

```markdown
## Phase N: <title>

**Built:** what now works, in plain terms (a few bullets).

**Decisions I made:** each one with the options and why you chose that one, so the user can overrule it.

**Acceptance criteria**
- [x] <criterion>: <evidence: test name, command or screenshot>
- [ ] <criterion>: not ticked because <reason>

**How to test:** numbered steps with exact commands (`cargo run -p axcenity-player -- examples/...`) and what the user should see. Cover every unticked box.

**Questions:** anything still open (or "None").
```

Don't commit or push unless asked; it's the user's history. Offer a commit message instead. After a push, CI is the last gate: check it with `gh run list` and report each OS.

## Gotchas in this repo

- Git checks text files out with Windows line endings (`core.autocrlf=true`). Tests that compare file text exactly must use `.replace("\r\n", "\n")`.
- On Windows, Git Bash heredocs can break on Rust source (`'static` and similar). Write files with the Write tool instead.
- Minimized windows report a 0×0 size. Never configure a wgpu surface or texture at zero size.
- The workspace forbids `unsafe` code (`unsafe_code = "forbid"`).
