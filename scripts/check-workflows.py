#!/usr/bin/env python3
"""Checks the GitHub Actions workflows locally, before any of them runs on
GitHub (docs/project/releases.md, "Testing the release workflow").

- Triggers: no workflow runs on pushes to branches or on pull requests;
  release.yml runs only for `v*` tags and by hand.
- Every `needs.<job>.outputs.<name>` and `steps.<id>.outputs.<name>` names a
  job, step and output that exist, and every `needs.<job>` is in the job's
  `needs`.
- Every bash `run:` block parses (`bash -n`), with `${{ … }}` replaced.
- Jobs that can write to the repository or read the signing key are only the
  ones meant to.
- No job checks out a ref other than the one the run started on, so code from
  a tag never runs in the default branch's context (CodeQL's cache-poisoning
  rule).

Uses PyYAML. Exit code 1 on any problem.

    python3 scripts/check-workflows.py
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[1]
WORKFLOWS = ROOT / ".github" / "workflows"
EXPR = re.compile(r"\$\{\{(.*?)\}\}", re.S)
NEEDS_OUT = re.compile(r"needs\.([\w-]+)\.outputs\.([\w-]+)")
NEEDS_ANY = re.compile(r"needs\.([\w-]+)")
STEPS_OUT = re.compile(r"steps\.([\w-]+)\.outputs\.([\w-]+)")
# The only jobs that may write contents or see the signing key.
WRITERS = {"release.yml": {"draft"}}
SIGNERS = {"release.yml": {"draft", "signing-check"}}


def triggers(doc: dict) -> dict:
    # PyYAML reads the bare key `on` as True.
    on = doc.get("on", doc.get(True))
    if isinstance(on, str):
        return {on: None}
    if isinstance(on, list):
        return {name: None for name in on}
    return on or {}


def step_outputs(job: dict) -> dict[str, set[str]]:
    """Step id → output names it writes to $GITHUB_OUTPUT (best effort)."""
    found: dict[str, set[str]] = {}
    for step in job.get("steps", []):
        if "id" in step:
            names = set(re.findall(r"echo \"([\w-]+)=", step.get("run", "")))
            found[step["id"]] = names
    return found


def check_file(path: Path) -> list[str]:
    problems: list[str] = []
    doc = yaml.safe_load(path.read_text(encoding="utf-8"))
    on = triggers(doc)
    if "pull_request" in on or "pull_request_target" in on:
        problems.append("runs on pull requests")
    push = on.get("push")
    if "push" in on:
        if not isinstance(push, dict) or "branches" in push or set(push) != {"tags"}:
            problems.append("runs on pushes to branches (only tag pushes are allowed)")
        elif any(not str(t).startswith("v") for t in push["tags"]):
            problems.append(f"push tags {push['tags']} aren't release tags")
    if "schedule" in on:
        problems.append("runs on a schedule")

    jobs: dict = doc.get("jobs", {})
    outputs = {name: set((job.get("outputs") or {}).keys()) for name, job in jobs.items()}
    top_permissions = doc.get("permissions", {})
    if top_permissions not in ({"contents": "read"}, "read-all", {}):
        problems.append(f"workflow permissions {top_permissions} are wider than contents: read")

    for name, job in jobs.items():
        needs = job.get("needs", [])
        needs = [needs] if isinstance(needs, str) else needs
        text = yaml.safe_dump(job)
        for job_name, output in NEEDS_OUT.findall(text):
            if job_name not in jobs:
                problems.append(f"{name}: needs.{job_name} is not a job")
            elif output not in outputs[job_name]:
                problems.append(f"{name}: {job_name} has no output `{output}`")
        for job_name in set(NEEDS_ANY.findall(text)):
            if job_name not in needs:
                problems.append(f"{name}: uses needs.{job_name} without `needs: {job_name}`")
        steps = step_outputs(job)
        for step_id, output in STEPS_OUT.findall(text):
            if step_id not in steps:
                problems.append(f"{name}: no step with id `{step_id}`")
            elif output not in steps[step_id]:
                problems.append(f"{name}: step `{step_id}` never writes output `{output}`")
        for output_name, value in (job.get("outputs") or {}).items():
            for step_id, output in STEPS_OUT.findall(str(value)):
                if output not in steps.get(step_id, set()):
                    problems.append(f"{name}: output {output_name} reads a missing steps.{step_id}.outputs.{output}")

        perms = job.get("permissions", {})
        writes = isinstance(perms, dict) and "write" in perms.values() or perms == "write-all"
        if writes and name not in WRITERS.get(path.name, set()):
            problems.append(f"{name}: has write permissions")
        if "secrets." in text and name not in SIGNERS.get(path.name, set()):
            problems.append(f"{name}: reads a secret")
        if "secrets." in text and job.get("environment") != "release":
            problems.append(f"{name}: reads a secret outside the `release` environment")

        for step in job.get("steps", []):
            if str(step.get("uses", "")).startswith("actions/checkout") and "ref" in (step.get("with") or {}):
                problems.append(f"{name}: checks out another ref ({step['with']['ref']})")

        shell = (job.get("defaults") or {}).get("run", {}).get("shell", "bash")
        for index, step in enumerate(job.get("steps", [])):
            if "run" not in step or step.get("shell", shell) not in ("bash", None):
                continue
            script = EXPR.sub("EXPR", step["run"])
            result = subprocess.run(["bash", "-n"], input=script, capture_output=True, text=True)
            if result.returncode != 0:
                label = step.get("name", f"step {index + 1}")
                problems.append(f"{name}: `{label}` doesn't parse: {result.stderr.strip()}")
    return problems


def main() -> int:
    failed = False
    for path in sorted(WORKFLOWS.glob("*.yml")):
        problems = check_file(path)
        for problem in problems:
            print(f"FAIL {path.name}: {problem}")
        if not problems:
            print(f"ok   {path.name}")
        failed |= bool(problems)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
