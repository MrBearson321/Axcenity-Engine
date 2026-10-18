# Roadmap phase workflow

Use this reference for an explicit phase request or when a requested feature maps directly to a plan phase. Run commands from the repository root:

```text
python .claude/skills/implement/scripts/plan.py status
python .claude/skills/implement/scripts/plan.py show <N|next>
```

`show` returns the selected phase and the full text of its linked PRD stories without loading either large document in full. `next` means the first phase with unticked criteria. For a feature name, use the matching phase if there is one.

Before implementation, check the previous phase's status and relevant code. Investigate an earlier gap only when it blocks the requested work or makes its assumptions unclear; don't stop to audit unrelated old criteria or CI. Tell the user if a blocker means the requested work depends on unresolved behavior.

Treat the phase's "What to build" and its linked stories as scope. Keep later-phase work out unless the user asks for it. Ask about unresolved choices only when they block implementation or have meaningful creator-facing, compatibility, or maintenance impact; otherwise choose a simple reversible option and report it.

Update plan checkboxes only when the criterion is directly evidenced in the current work. Since feature tests and runtime checks are opt-in, leave behavior criteria unticked until they have evidence; give the user exact steps to verify them. Code inspection may substantiate a structural criterion, but not a claim that runtime behavior works.

Update only documentation that is directly affected. Keep `CLAUDE.md` a short project index; update a crate's `CLAUDE.md` when its structure, key types, or gotchas change. Don't update README status mechanically for every implementation.
