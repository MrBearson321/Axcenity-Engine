---
name: implement
description: Implements a requested Axcenity Engine feature or roadmap phase in Rust/Lua. Use when asked to build, continue, or complete a feature or phase (for example "implement physics", "start phase 4", or "do the next phase"). Reads only the relevant plan, stories, and crate guidance; makes focused changes; and hands off feature testing to the user unless they ask for verification.
---

# Implement an Axcenity Engine feature

Implement the user's requested scope end to end. Keep this skill's default workflow lightweight: inspect only relevant context, make the change, and leave feature testing to the user. Do not turn every feature request into a full phase audit.

## Workflow

1. **Resolve scope.** For a numbered, named, or "next" roadmap phase, use the plan helper as described in [phase-workflow.md](references/phase-workflow.md). For a standalone feature, inspect only the plan section and PRD stories that directly apply. If the feature is not planned, proceed when the user's request is clear; don't edit the roadmap or expand scope without reason.
2. **Read relevant context.** Consult the matching sections of `CLAUDE.md`, only the relevant `docs/*.md`, and `crates/<name>/CLAUDE.md` for affected crates. Follow repository architecture constraints and local patterns. Check nearby source and tests before adding or changing behavior.
3. **Clarify only blockers.** Ask before coding when an unresolved choice materially affects creator-facing behavior, compatibility, file formats, public APIs, or dependencies. Decide reversible implementation details using the simplest idiomatic approach, and mention notable decisions in the handoff.
4. **Implement narrowly.** Complete the requested behavior without pulling in later-phase work. Keep examples and directly related documentation consistent. For a new crate, add its short `CLAUDE.md`; pin new dependencies in `[workspace.dependencies]` and check license compatibility. Preserve cross-platform behavior.
5. **Hand off.** Do not run feature tests, builds, clippy, or runtime/visual checks by default; the user will test the feature. Do not claim behavior is verified or tick acceptance criteria based only on writing code. Give concise manual test steps and expected results for the changed behavior. If the user asks for verification, load [verification.md](references/verification.md) and run only the checks appropriate to the request.

## Final response

Summarize what changed and any important decisions. Include relevant acceptance-criterion status only for a roadmap phase, and distinguish implementation from verified behavior. Give the user the exact commands or editor steps needed to test the feature; avoid a long, fixed report template for standalone work. Do not commit or push unless asked.

## Load details only when needed

- Roadmap phase selection, dependencies on prior phases, and criteria handling: [phase-workflow.md](references/phase-workflow.md)
- Checks when the user explicitly requests verification: [verification.md](references/verification.md)
- Repository-specific implementation gotchas, only when relevant: [repo-gotchas.md](references/repo-gotchas.md)
