---
name: implement
description: Implements a requested Axcenity Engine feature or roadmap phase in Rust/Lua. Use when asked to build, continue, or complete a feature or phase (for example "implement physics", "start phase 4", or "do the next phase"). Reads only the relevant plan, stories, and crate guidance; makes focused changes; tests what it can automatically; ticks the plan criteria the tests prove; then commits and pushes to GitHub.
---

# Implement an Axcenity Engine feature

Implement the user's requested scope end to end: inspect only relevant context, make the change, test it automatically where a test can prove it, tick what's proven, and commit and push. Do not turn every feature request into a full phase audit.

## Workflow

1. **Resolve scope.** For a numbered, named, or "next" roadmap phase, use the plan helper as described in [phase-workflow.md](references/phase-workflow.md). The current roadmap is `plans/axcenity-engine-unified.md`. For a standalone feature, inspect only the plan section and PRD stories that directly apply. If the feature is not planned, proceed when the user's request is clear; don't edit the roadmap or expand scope without reason.
2. **Read relevant context.** Consult the matching sections of `CLAUDE.md`, only the relevant `docs/*.md`, and `crates/<name>/CLAUDE.md` for affected crates. Read the affected systems' pages in the engineering wiki (`wiki/Architecture/System-Inventory.md` maps them) and the ADRs they link; if the work conflicts with an accepted ADR, say so before coding. Follow repository architecture constraints and local patterns. Check nearby source and tests before adding or changing behavior.
3. **Clarify only blockers.** Ask before coding when an unresolved choice materially affects creator-facing behavior, compatibility, file formats, public APIs, or dependencies. Decide reversible implementation details using the simplest idiomatic approach, and mention notable decisions in the handoff.
4. **Implement narrowly.** Complete the requested behavior without pulling in later-phase work. Keep examples and directly related documentation consistent, including the wiki when the change alters architecture, a decision, performance or a limitation (the table in `AGENTS.md`; then `python scripts/check-wiki.py`). For a new crate, add its short `CLAUDE.md`; pin new dependencies in `[workspace.dependencies]` and check license compatibility. Preserve cross-platform behavior.
5. **Test automatically.** Write tests for each acceptance criterion a test can express (headless engine, physics, scripting, file round trips, egui layout with a test harness), then run the checks in [verification.md](references/verification.md). Fix what fails. When a criterion is visual and a screenshot can show it, take and inspect one.
6. **Tick what's proven.** Tick a plan checkbox when a passing test, or an inspected screenshot, directly demonstrates that criterion. Leave the rest unticked and say what manual check each needs. Never tick from having written the code alone.
7. **Commit and push.** Once the checks pass, commit the work (see below) and push the current branch to `origin`. If checks still fail after a reasonable attempt, commit nothing, and report the failure with its output.

## Commit and push

- Stage only files this task changed. Leave unrelated working-tree changes alone and mention them.
- Message: a short title in the repo's style (for example "Jump, Slopes and Steps"), a blank line, a one-paragraph body if useful, and the attribution line the session asks for.
- `git push origin HEAD`. If the push is rejected, report it; don't force-push or rebase someone else's work. GitHub Actions is off on purpose, so nothing runs after a push; the local checks are the verification.

## Final response

Summarize what changed and any important decisions. For a roadmap phase, list each acceptance criterion as ticked (with the test or screenshot that proves it) or open (with the manual step that would). Report which checks ran and their result, and the commit hash that was pushed. Give exact commands or editor steps for anything the user still needs to check by hand.

## Load details only when needed

- Roadmap phase selection, dependencies on prior phases, and criteria handling: [phase-workflow.md](references/phase-workflow.md)
- Which checks to run and how: [verification.md](references/verification.md)
- Repository-specific implementation gotchas, only when relevant: [repo-gotchas.md](references/repo-gotchas.md)
