---
title: "ADR-0018: GitHub Actions runs for release tags only; CI by hand"
type: adr
adr: 18
adr_status: Accepted
date: 2026-11-11
supersedes: "ADR-0011's 'Actions disabled entirely' (the rest of ADR-0011 stands)"
affects: [release, all]
source: [.github/workflows/release.yml, .github/workflows/ci.yml]
docs: [docs/development-policy.md, docs/development.md, docs/project/releases.md]
tags: [adr, process]
---

# ADR-0018: GitHub Actions runs for release tags only; CI by hand

- **Status:** Accepted (development policy, 2026-11-11). **Amends** [ADR-0011](ADR-0011-Desktop-First-Local-Verification.md), which had Actions off entirely.
- **Affected systems:** releases, verification

## Context and problem

Building, signing and publishing packages for Windows, macOS and Linux needs machines of each kind; doing it by hand was slow and error-prone (release system audit, `docs/research/release-system/`). Running CI on every push still costs more than it's worth while the architecture changes.

## Decision

- The **Release** workflow runs on release tags (`v<version>`), building every OS, signing, and publishing a draft release. Paul is told before a tag is pushed.
- The **CI** workflow runs only when started by hand (Actions > CI > Run workflow), never on pushes or pull requests.
- Local verification remains the only routine check: a pull request still says what ran and on which OS.

## Consequences

- "CI will catch it" is still not verification.
- Release runs are deliberate, rare events; Actions minutes are spent on purpose.
- Revisit per-push CI when the architecture and core systems are stable (unchanged from ADR-0011).

## Implementation references

`.github/workflows/release.yml`, `crates/release`; [Release process](../../Development/Release-Process.md).
