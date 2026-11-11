#!/usr/bin/env python3
"""Check the Axcenity engineering wiki (an Obsidian vault of Markdown files).

Read-only: it reports problems and never edits a page.

Checks:
  * links between pages resolve, including #heading anchors (GitHub-style slugs);
  * backticked repository paths (crates/..., docs/..., Cargo.toml, ...) and the
    `source` / `docs` frontmatter lists name files that exist;
  * pages whose `source` files changed after their `verified_commit`
    ("may be stale");
  * ADR numbers are unique and every ADR is listed in the ADR index;
  * pages that no other page links to (orphans).

Usage (from the repository root):
  python scripts/check-wiki.py                 # wiki/ in this repository
  python scripts/check-wiki.py --wiki <folder> # a vault somewhere else
  python scripts/check-wiki.py --strict        # warnings fail too

Exit code 1 when there are errors (broken links, missing paths, duplicate
ADR numbers), or warnings with --strict.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
import urllib.parse
from pathlib import Path

LINK = re.compile(r"(?<!!)\[(?P<text>[^\]]*)\]\((?P<target>[^)\s]+)(?:\s+\"[^\"]*\")?\)")
CODE_SPAN = re.compile(r"`([^`\n]+)`")
HEADING = re.compile(r"^(#{1,6})\s+(.*?)\s*#*\s*$")
FENCE = re.compile(r"^\s*(```|~~~)")
REPO_ROOTS = (
    "crates/", "docs/", "plans/", "prd/", "scripts/", "benchmarks/", "examples/",
    "templates/", "definitions/", ".config/", ".github/", "packaging/", ".claude/",
)
REPO_FILES = {
    "Cargo.toml", "Cargo.lock", "CLAUDE.md", "AGENTS.md", "README.md", "NOTICE",
    "LICENSE", "rust-toolchain.toml", "selene.toml", "axcenity.yml",
}
SKIP_DIRS = {".obsidian", ".trash", "Templates"}


def slug(heading: str) -> str:
    """GitHub's anchor for a heading: lower case, punctuation dropped, spaces to -."""
    text = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", heading)  # a link keeps its text
    text = text.replace("`", "").strip().lower()
    text = re.sub(r"[^\w\- ]", "", text)
    return text.replace(" ", "-")


def strip_code(lines: list[str]) -> list[tuple[int, str]]:
    """Lines outside fenced code blocks, with their numbers."""
    out, fenced = [], False
    for number, line in enumerate(lines, 1):
        if FENCE.match(line):
            fenced = not fenced
            continue
        if not fenced:
            out.append((number, line))
    return out


def frontmatter(text: str) -> dict[str, object]:
    if not text.startswith("---"):
        return {}
    end = text.find("\n---", 3)
    if end < 0:
        return {}
    data: dict[str, object] = {}
    for line in text[3:end].splitlines():
        if ":" not in line or line.startswith((" ", "\t", "#")):
            continue
        key, _, value = line.partition(":")
        value = value.strip()
        if value.startswith("[") and value.endswith("]"):
            data[key.strip()] = [v.strip().strip("'\"") for v in value[1:-1].split(",") if v.strip()]
        else:
            data[key.strip()] = value.strip("'\"")
    return data


class Page:
    def __init__(self, path: Path, wiki: Path):
        self.path = path
        self.rel = path.relative_to(wiki).as_posix()
        self.text = path.read_text(encoding="utf-8")
        self.lines = self.text.splitlines()
        self.meta = frontmatter(self.text)
        self.anchors = set()
        for _, line in strip_code(self.lines):
            m = HEADING.match(line)
            if m:
                self.anchors.add(slug(m.group(2)))


def repo_path(span: str) -> str | None:
    """The repository path a code span names, or None if it isn't one."""
    span = span.strip()
    if any(c in span for c in "*<>{} ") or "…" in span or "..." in span:
        return None
    span = span.split("#", 1)[0]
    span = re.sub(r":\d+(-\d+)?$", "", span)
    if span in REPO_FILES or span.startswith(REPO_ROOTS):
        return span.rstrip("/")
    return None


def git(repo: Path, *args: str) -> str:
    try:
        return subprocess.run(
            ["git", "-C", str(repo), *args], capture_output=True, text=True, check=True
        ).stdout
    except (subprocess.CalledProcessError, FileNotFoundError):
        return ""


def main() -> int:
    here = Path(__file__).resolve().parent.parent
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--wiki", type=Path, default=here / "wiki", help="the vault folder (default: wiki/)")
    parser.add_argument("--repo", type=Path, default=here, help="the repository root (default: this script's)")
    parser.add_argument("--strict", action="store_true", help="fail on warnings too")
    parser.add_argument("--no-stale", action="store_true", help="skip the git staleness check")
    args = parser.parse_args()

    wiki, repo = args.wiki.resolve(), args.repo.resolve()
    if not wiki.is_dir():
        print(f"no wiki at {wiki} (pass --wiki)", file=sys.stderr)
        return 2

    pages: dict[str, Page] = {}
    for path in sorted(wiki.rglob("*.md")):
        if any(part in SKIP_DIRS for part in path.relative_to(wiki).parts):
            continue
        page = Page(path, wiki)
        pages[page.rel] = page

    errors: list[str] = []
    warnings: list[str] = []
    inbound: dict[str, set[str]] = {rel: set() for rel in pages}

    for page in pages.values():
        for number, line in strip_code(page.lines):
            where = f"{page.rel}:{number}"
            for m in LINK.finditer(line):
                target = m.group("target")
                if re.match(r"^[a-z][a-z0-9+.-]*:", target, re.I):
                    continue  # http:, https:, mailto:, obsidian:
                target = urllib.parse.unquote(target)
                file_part, _, anchor = target.partition("#")
                if file_part:
                    dest = (page.path.parent / file_part).resolve()
                    try:
                        dest_rel = dest.relative_to(wiki).as_posix()
                    except ValueError:
                        errors.append(f"{where}: link leaves the wiki: {target} (name repo files in backticks instead)")
                        continue
                    if not dest.exists():
                        errors.append(f"{where}: broken link: {target}")
                        continue
                    if dest_rel in pages:
                        inbound[dest_rel].add(page.rel)
                        if anchor and slug(anchor) not in pages[dest_rel].anchors and anchor not in pages[dest_rel].anchors:
                            errors.append(f"{where}: no heading #{anchor} in {dest_rel}")
                elif anchor and slug(anchor) not in page.anchors and anchor not in page.anchors:
                    errors.append(f"{where}: no heading #{anchor} on this page")
            for m in CODE_SPAN.finditer(line):
                path = repo_path(m.group(1))
                if path and not (repo / path).exists():
                    errors.append(f"{where}: repo path not found: {path}")

        for key in ("source", "docs"):
            listed = page.meta.get(key) or []
            if isinstance(listed, str):
                listed = [listed]
            for path in listed:
                if path and not (repo / path).exists():
                    errors.append(f"{page.rel}: frontmatter {key} not found: {path}")

    # Staleness: sources changed since the page was last checked against them.
    if not args.no_stale:
        for page in pages.values():
            commit = page.meta.get("verified_commit")
            sources = page.meta.get("source") or []
            if not commit or not sources or not isinstance(sources, list):
                continue
            existing = [s for s in sources if (repo / s).exists()]
            if not existing:
                continue
            if not git(repo, "cat-file", "-t", str(commit)).strip():
                warnings.append(f"{page.rel}: verified_commit {commit} isn't in this repository")
                continue
            changed = git(repo, "log", "--format=%h %s", f"{commit}..HEAD", "--", *existing).strip()
            if changed:
                count = len(changed.splitlines())
                warnings.append(f"{page.rel}: may be stale: {count} commit(s) touched its sources since {commit}")

    # ADRs: unique numbers, all listed in the index.
    adr_numbers: dict[str, str] = {}
    index = next((p for rel, p in pages.items() if rel.endswith("ADR-Index.md")), None)
    for rel, page in pages.items():
        if page.meta.get("type") != "adr":
            continue
        number = str(page.meta.get("adr", "")).zfill(4)
        if number in adr_numbers:
            errors.append(f"{rel}: ADR number {number} also used by {adr_numbers[number]}")
        adr_numbers[number] = rel
        if index and Path(rel).name not in index.text:
            errors.append(f"{rel}: not listed in {index.rel}")
        status = page.meta.get("adr_status", "")
        if status not in {"Proposed", "Accepted", "Superseded", "Rejected"}:
            warnings.append(f"{rel}: adr_status should be Proposed, Accepted, Superseded or Rejected (is {status!r})")

    for rel, sources in inbound.items():
        if not sources and Path(rel).name not in {"Home.md"}:
            warnings.append(f"{rel}: orphan (no page links to it)")

    for line in errors:
        print(f"error: {line}")
    for line in warnings:
        print(f"warning: {line}")
    print(f"{len(pages)} pages, {len(errors)} errors, {len(warnings)} warnings")
    return 1 if errors or (args.strict and warnings) else 0


if __name__ == "__main__":
    sys.exit(main())
