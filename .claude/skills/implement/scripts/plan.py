"""Reads the implementation plan so the implement skill doesn't load whole files.

Usage (from the repository root):
    python .claude/skills/implement/scripts/plan.py status
        Every phase with its ticked/total criteria, and which phase is next.
    python .claude/skills/implement/scripts/plan.py show <phase number | next>
        One phase's section, plus the full text of the PRD user stories it lists.
"""

import re
import sys
from pathlib import Path

PLAN = Path("plans/axcenity-engine-v1.md")
PRD = Path("prd/axcenity-engine-v1.md")

PHASE_HEADING = re.compile(r"^## Phase (\d+): (.+)$")
CHECKBOX = re.compile(r"^- \[( |x|X)\] ")


def read(path):
    if not path.exists():
        sys.exit(f"{path} not found; run this from the repository root")
    return path.read_text(encoding="utf-8").splitlines()


def phases():
    """(number, title, lines of the section) for each phase, in order."""
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
            current = (int(match.group(1)), match.group(2), [line])
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
        print(f"Phase {number:>2}: {ticked}/{total}  {title}{marker}")
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
        matches = [p for p in all_phases if str(p[0]) == which]
        if not matches:
            sys.exit(f"No phase {which}. Run `status` to list them.")
        phase = matches[0]

    number, _, lines = phase
    print("\n".join(lines).rstrip())

    previous = [p for p in all_phases if p[0] == number - 1]
    if previous:
        ticked, total = counts(previous[0][2])
        print(f"\n(Previous phase {number - 1}: {ticked}/{total} criteria ticked)")

    for line in lines:
        if line.startswith("**User stories**"):
            numbers = [int(n) for n in re.findall(r"\d+", line)]
            print("\nUser stories from the PRD:")
            print("\n".join(user_stories(numbers)) or "(none found)")


if __name__ == "__main__":
    # Windows consoles default to a legacy encoding that can't print `→`.
    sys.stdout.reconfigure(encoding="utf-8")
    if len(sys.argv) >= 2 and sys.argv[1] == "status":
        status()
    elif len(sys.argv) >= 3 and sys.argv[1] == "show":
        show(sys.argv[2])
    else:
        sys.exit(__doc__)
