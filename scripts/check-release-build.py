#!/usr/bin/env python3
"""Checks that release binaries say what they are: each one's
`--version --json` must name the version, the tag, the commit and an official
build. The release workflow runs it on every OS after building; run it locally
against a build made with the same environment:

    AXCENITY_OFFICIAL_BUILD=1 AXCENITY_BUILD_TAG=v0.2.0-alpha.1 \\
    AXCENITY_BUILD_COMMIT=$(git rev-parse HEAD) cargo build -p axcenity-player
    python3 scripts/check-release-build.py 0.2.0-alpha.1 $(git rev-parse HEAD) target/debug/axcenity-player

Exit code 1 if any binary disagrees.
"""

from __future__ import annotations

import json
import subprocess
import sys


def check(binary: str, version: str, commit: str, profile: str | None) -> list[str]:
    try:
        result = subprocess.run(
            [binary, "--version", "--json"], capture_output=True, text=True, timeout=60
        )
    except OSError as err:
        return [f"could not run: {err}"]
    if result.returncode != 0:
        return [f"--version --json exited with {result.returncode}: {result.stderr.strip()}"]
    try:
        info = json.loads(result.stdout)
    except json.JSONDecodeError:
        return [f"--version --json printed something that isn't JSON: {result.stdout[:200]!r}"]
    expected = {"version": version, "tag": f"v{version}", "commit": commit, "kind": "official"}
    if profile:
        expected["profile"] = profile
    return [
        f"{key} is {info.get(key)!r}, not {value!r}"
        for key, value in expected.items()
        if info.get(key) != value
    ]


def main(argv: list[str]) -> int:
    profile = None
    if len(argv) > 1 and argv[1].startswith("--profile="):
        profile = argv.pop(1).split("=", 1)[1]
    if len(argv) < 4:
        print(__doc__, file=sys.stderr)
        return 2
    version, commit, binaries = argv[1], argv[2], argv[3:]
    failed = False
    for binary in binaries:
        problems = check(binary, version, commit, profile)
        if problems:
            failed = True
            print(f"FAIL {binary}: " + "; ".join(problems))
        else:
            print(f"ok   {binary}: official {version} at {commit[:9]}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
