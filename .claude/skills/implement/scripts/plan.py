"""Reads the implementation plan so the implement skill doesn't load whole files.

Usage (from the repository root):
    python .claude/skills/implement/scripts/plan.py [--plan NAME] status
        Every phase with its ticked/total criteria, and which phase is next.
    python .claude/skills/implement/scripts/plan.py [--plan NAME] show <phase number | next>
        One phase's section, plus the full text of the PRD user stories it lists.

NAME is a file in `plans/`, with or without the `axcenity-engine-` prefix and
`.md` suffix (`unified`, `v2`, ...). It defaults to the unified plan.
"""

import re
import sys
from pathlib import Path

PLAN = Path("plans/axcenity-engine-unified.md")
PRD = Path("prd/axcenity-engine-v2.md")

# A phase is numbered, with a letter for one added between others ("8a").
PHASE_HEADING = re.compile(r"^## Phase (\d+[a-z]?): (.+)$")
# The PRD a plan cites in its header, such as "(PRD `prd/axcenity-engine-v2.md`)".
PRD_REFERENCE = re.compile(r"`(prd/[^`]+\.md)`")
# Stories are listed as "**User stories**: 60, 58" or "**From**: v2 Phase 13 (stories 16, 20)".
STORIES = re.compile(r"^\*\*User stories\*\*:(.*)$|\(stor(?:y|ies) ([\d, ]+)\)")
CHECKBOX = re.compile(r"^- \[( |x|X)\] ")


def read(path):
    if not path.exists():
        sys.exit(f"{path} not found; run this from the repository root")
    return path.read_text(encoding="utf-8").splitlines()


def phases():
    """(number, title, lines of the section) for each phase, in order. The
    number is text, as a phase like "8a" has a letter."""
    found = []
    current = None
    for line in read(PLAN):
        match = PHASE_HEADING.match(line)
        # A phase ends at the next phase, the next milestone (`# `), or a `---` rule.
        if match or line.startswith("# ") or line.strip() == "---":
            if current:
                found.append(current)
                current = None
        if match:
            current = (match.group(1), match.group(2), [line])
        elif current:
            current[2].append(line)
    if current:
        found.append(current)
    return found


def counts(lines):
    boxes = [CHECKBOX.match(line) for line in lines]
    boxes = [b for b in boxes if b]
    ticked = sum(1 for b in boxes if b.group(1) in "xX")
    return ticked, len(boxes)


def next_phase(all_phases):
    for phase in all_phases:
        ticked, total = counts(phase[2])
        if ticked < total:
            return phase
    return None


def status():
    all_phases = phases()
    upcoming = next_phase(all_phases)
    for number, title, lines in all_phases:
        ticked, total = counts(lines)
        marker = "  <- next" if upcoming and number == upcoming[0] else ""
        print(f"Phase {number:>3}: {ticked}/{total}  {title}{marker}")
    if not upcoming:
        print("Every phase is complete.")


def user_stories(numbers):
    """The PRD lines for the given story numbers (`12. As a creator, ...`)."""
    wanted = set(numbers)
    lines = []
    for line in read(PRD):
        match = re.match(r"^(\d+)\. ", line)
        if match and int(match.group(1)) in wanted:
            lines.append(line)
    return lines


def show(which):
    all_phases = phases()
    if which == "next":
        phase = next_phase(all_phases)
        if not phase:
            sys.exit("Every phase is complete.")
    else:
        matches = [p for p in all_phases if p[0] == which]
        if not matches:
            sys.exit(f"No phase {which}. Run `status` to list them.")
        phase = matches[0]

    _, _, lines = phase
    print("\n".join(lines).rstrip())

    at = all_phases.index(phase)
    if at > 0:
        previous, _, previous_lines = all_phases[at - 1]
        ticked, total = counts(previous_lines)
        print(f"\n(Previous phase {previous}: {ticked}/{total} criteria ticked)")

    for line in lines:
        match = STORIES.search(line)
        if match:
            numbers = [int(n) for n in re.findall(r"\d+", match.group(1) or match.group(2))]
            print(f"\nUser stories from {PRD}:")
            print("\n".join(user_stories(numbers)) or "(none found)")


def select_plan(name):
    """Point PLAN at `plans/<name>` and PRD at the PRD that plan cites."""
    global PLAN, PRD
    stem = name.removesuffix(".md")
    if not stem.startswith("axcenity-engine-"):
        stem = f"axcenity-engine-{stem}"
    PLAN = Path("plans") / f"{stem}.md"
    header = "\n".join(read(PLAN)[:5])
    cited = PRD_REFERENCE.search(header)
    PRD = Path(cited.group(1)) if cited else Path("prd") / f"{stem}.md"


if __name__ == "__main__":
    # Windows consoles default to a legacy encoding that can't print `→`.
    sys.stdout.reconfigure(encoding="utf-8")
    if len(sys.argv) >= 3 and sys.argv[1] == "--plan":
        select_plan(sys.argv[2])
        del sys.argv[1:3]
    if len(sys.argv) >= 2 and sys.argv[1] == "status":
        status()
    elif len(sys.argv) >= 3 and sys.argv[1] == "show":
        show(sys.argv[2])
    else:
        sys.exit(__doc__)
