# Patched dependencies

Copies of crates.io releases with small Axcenity fixes, used through
`[patch.crates-io]` in the workspace `Cargo.toml`. They are left out of the
workspace (`exclude = ["vendor"]`), so they aren't linted or formatted as ours.
Each keeps its own license (rapier: Apache-2.0, https://github.com/dimforge/rapier).
Every change is marked `AXCENITY PATCH` in the code.

## rapier3d 0.36.1

The release as published on crates.io (`.cargo_vcs_info.json` names its
upstream commit), with one change:

- `src/geometry/narrow_phase/pair_update.rs`: contact recycling (reusing a
  pair's contact points while the pair has barely moved) skipped every pair
  with an active hook. Pairs whose only hook is `MODIFY_SOLVER_CONTACTS` are
  now recycled too, keeping what the hook last set. Axcenity's hook
  (`physics/src/material.rs`) only sets friction and restitution, which
  depend on the two colliders and not on where they are. Without this, parts
  with friction or elasticity weights other than 1 (Ice, Rubber, custom
  `PhysicalProperties`) never kept their contacts, and a stack of ten of them
  toppled where Plastic stood (`physics` test
  `a_stack_of_ice_blocks_stands_like_plastic`).

### Upgrading rapier

Copy the new release from `~/.cargo/registry/src/*/rapier3d-<version>`,
carry the change over (search for `hooks_involved`), and run the physics
tests. Drop the copy once upstream recycles pairs like these.
