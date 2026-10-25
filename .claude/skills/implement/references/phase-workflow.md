# Roadmap phase workflow

Use this reference for an explicit phase request or when a requested feature maps directly to a plan phase. Run commands from the repository root:

```text
python .claude/skills/implement/scripts/plan.py status
python .claude/skills/implement/scripts/plan.py show <N|next>
python .claude/skills/implement/scripts/plan.py --plan v2 show <N>
```

The helper reads `plans/axcenity-engine-unified.md` by default; `--plan <name>` picks another file in `plans/` (`v1`, `v2`, `v3`, `unified`). `show` returns the selected phase and the full text of its linked PRD stories without loading either large document in full. `next` means the first phase with unticked criteria. For a feature name, use the matching phase if there is one.

Before implementation, check the previous phase's status and relevant code. Investigate an earlier gap only when it blocks the requested work or makes its assumptions unclear; don't stop to audit unrelated old criteria or CI. Tell the user if a blocker means the requested work depends on unresolved behavior.

Treat the phase's "What to build" and its linked stories as scope. Keep later-phase work out unless the user asks for it. Ask about unresolved choices only when they block implementation or have meaningful creator-facing, compatibility, or maintenance impact; otherwise choose a simple reversible option and report it.

## Ticking criteria

Tick a criterion (`- [ ]` to `- [x]`) in the plan the phase came from, once its evidence exists in this run:

- a passing automated test that exercises the behavior the criterion names (name the test in the handoff), or
- an inspected screenshot that shows it, for a visual criterion, or
- code inspection, for a purely structural criterion (a dependency rule, a file's location).

A criterion that needs a person (feel, a real controller, a macOS or Linux run the CI hasn't covered) stays unticked; list the manual step for it. When the unified plan's phase names a source phase ("**From**: v2 Phase 13"), tick the unified plan; leave the source plan alone unless that phase's own criteria are the ones proven.

Update only documentation that is directly affected. Keep `CLAUDE.md` a short project index; update a crate's `CLAUDE.md` when its structure, key types, or gotchas change. Don't update README status mechanically for every implementation.
