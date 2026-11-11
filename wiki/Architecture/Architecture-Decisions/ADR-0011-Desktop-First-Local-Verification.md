---
title: "ADR-0011: Desktop first, verified locally, no CI for now"
type: adr
adr: 11
adr_status: Accepted
date: 2026-11-10
supersedes: "v1 plan: CI builds and tests all three OSes from Phase 1; a phase isn't done unless CI is green"
affects: [all]
source: [.github/workflows/ci.yml, scripts/check-platform.sh, scripts/check-platform.ps1, scripts/check-packages.py]
docs: [docs/development-policy.md, docs/development.md]
tags: [adr, process, platforms]
---

# ADR-0011: Desktop first, verified locally, no CI for now

- **Status:** Accepted (Paul's development policy, 2026-11-10). **Supersedes** the v1 plan's "CI builds and tests all three from Phase 1". **Amended 2026-11-11 by [ADR-0018](ADR-0018-Actions-For-Release-Tags-Only.md)**: Actions now runs for release tags, and CI by hand; everything else here stands.
- **Affected systems:** everything; how changes are verified; platform targets

## Context and problem

The modernization audit asked whether CI was really off, whether mobile and web were planned, and how much to invest in infrastructure while the architecture was changing fast.

## Decision

- **Platforms**: desktop first; Windows and Linux primary, macOS secondary (built and shipped, not required to verify). Mobile and web deferred (Milestone 20).
- **CI**: GitHub Actions intentionally disabled; the workflow files stay. Verification is local: the commands in `docs/development.md` and `scripts/check-platform.*`. A pull request says what ran and on which OS.
- **Infrastructure**: none before it's needed; hand-run scripts are fine.
- **Correctness before features** in a subsystem; **measure before optimizing**; keep cheap extension points.

## Consequences

- Every significant change carries local tests ([Testing strategy](../../Development/Testing-Strategy.md)); "CI will catch it" is not verification.
- M10 (CI half) and M11 (internet hosting) deferred; M12 gated on local GPU tests on Windows and Linux.
- Graphics formats and scripting favor desktop capability (BC7, Luau native code).
- Revisit when the architecture is stable (CI), the desktop engine is reliable (mobile, web).

## Implementation references

`docs/development-policy.md` (the full text); `scripts/check-platform.*`; [Contribution workflow](../../Development/Contribution-Workflow.md).
