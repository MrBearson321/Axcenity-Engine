# Building and testing fast

How to keep builds and test runs short, down to a laptop (the numbers below are from a 6-core Ryzen 5 5500U with 14 GB of RAM).

**Testing is local.** GitHub Actions runs only for releases, and CI only by hand, while the architecture changes ([development policy](development-policy.md#continuous-integration)), so the checks on this page, run on your own machine, are the only verification. Significant engine changes need local tests, and a pull request says what ran and on which OS.

## Tools

Install once:

```bash
cargo install cargo-nextest --locked
cargo install cargo-hakari --locked
```

## Run the tests with nextest

| Task | Command |
|---|---|
| Every test | `cargo nextest run --workspace` |
| One crate's tests | `cargo nextest run -p axcenity-script` |
| A crate and every crate that uses it | `cargo nextest run --workspace -E 'rdeps(axcenity-core)'` |
| Tests whose name contains a word | `cargo nextest run --workspace jump` |
| Doc tests (nextest skips them) | `cargo test --workspace --doc` |

nextest runs each test in its own process, all crates at once, and keeps going after a failure. `cargo test --workspace` runs the crates one after another and stops at the first crate with a failure, so the crates after it never run. The whole suite takes about 25 s with nextest and 52 s with `cargo test`.

Settings are in `.config/nextest.toml`:

- A test still running after 30 s is marked SLOW. After 2 minutes it's stopped and fails, so a hung test can't stall the run.
- The slowest tests start first. Add a test there when it starts finishing last.
- `--profile ci` (used by the CI workflow, which is off) allows more time.
- The tests of `render` and `player` draw on the GPU, so they're in the `gpu` test group: at most 8 run at once. With dozens of processes sharing the GPU, a test's frames waited so long behind the others' that wgpu gave up on them and panicked (`We timed out while waiting on the last successful submission to complete!`).

nextest prints a test's output only when it fails, unless you pass `--no-capture`. `cargo test` still works too (add `--no-fail-fast` to run every crate).

Keep single tests short. `axcenity-player`'s `every_example_renders` draws every example on up to four threads sharing one GPU device (15 s one at a time, 5 s now).

## Checking another platform

GitHub Actions runs only for releases. The CI workflow (`.github/workflows/ci.yml`: format, the workspace-hack check, and build, the player having no editor code, clippy and every test on Windows, macOS and Linux) runs only when started by hand (**Actions > CI > Run workflow**), never on pushes or pull requests. The Release workflow runs for release tags ([releases.md](project/releases.md#cutting-a-release)). The priority is stability and development speed over keeping CI infrastructure running, and minutes are spent only on purpose ([development policy](development-policy.md#continuous-integration)). Don't add jobs to `ci.yml`; check any workflow change locally with `python3 scripts/check-workflows.py` first.

Check on the computers themselves instead. Windows and Linux are the primary platforms; macOS is secondary. The scripts also cover what CI never could (the game on a real GPU). From the repository root:

| Platform | Command |
|---|---|
| macOS, Linux | `scripts/check-platform.sh` |
| Windows | `powershell -ExecutionPolicy Bypass -File scripts\check-platform.ps1` |

It runs what CI runs (build, the player having no editor code, clippy, every test, doc tests), then `axcenity-test examples/hello-character` and the dedicated server started with no display (`crates/server/tests/headless.rs`; on Linux, run it from a terminal with no desktop, or over SSH, to be sure). Every step runs even after a failure. The summary is saved to `target/platform-check-<os>.txt`.

Packages (installers, `.dmg`, `.deb` and the rest) and File > Build Game for each OS have their own check, `python3 scripts/check-packages.py` ([project/releases.md](project/releases.md#checking-packages)).

LAN discovery between two computers is checked by hand: host `examples/hello-part` on one (`cargo run -p axcenity-player -- --host examples/hello-part`), then on another on the same network run `cargo test -p axcenity-net -- --ignored --nocapture a_game_hosted_on_another_computer`. A firewall must let the player take UDP on ports 47800 and 47801.

## The workspace-hack crate

Cargo turns on a dependency's features from the packages a command builds. Without help, `cargo test -p axcenity-script` or `cargo run -p axcenity-player` builds libraries like `serde`, `libc` and `winit` with fewer features than `--workspace` does. Cargo then compiles a second copy of them and of everything built on them. Measured after a full workspace build:

| Command | Without the hack | With it |
|---|---|---|
| `cargo test -p axcenity-script --no-run` | 79 crates, 2 min 20 s | 10 engine crates, 2 s |
| `cargo run -p axcenity-player` | over 5 minutes | 7 engine crates, 5 s |

Each copy was also kept, so the target folder grew to 23 GB instead of 9 GB.

`crates/workspace-hack` (generated by [cargo-hakari](https://docs.rs/cargo-hakari)) fixes this. It depends on those shared libraries with every feature any crate turns on, for Linux, Windows and both Macs, and every engine crate depends on it. So every build gets the same copy.

- **After adding, removing or changing a dependency**, run `cargo hakari generate`. For a new crate, also run `cargo hakari manage-deps`. To check, run `cargo hakari generate --diff` and `cargo hakari manage-deps --dry-run` (what CI's "Workspace hack" job ran); both print nothing when it's up to date.
- Settings are in `.config/hakari.toml`. `http` and `httparse` are left out (`final-excludes`) so the player doesn't depend on the editor's HTTP crates, and `winit` so the dedicated server builds no window crates (roadmap M8; check with `cargo tree -p axcenity-server -e normal -i winit`).
- The player still gets the union of features (for example `winit`'s default features, which the editor alone wouldn't turn on). Release builds already did this, since they build the editor and player in one command.

## Patched dependencies

`vendor/` holds copies of crates.io releases with small fixes of ours, used through `[patch.crates-io]` in the workspace `Cargo.toml` and left out of the workspace. `vendor/README.md` lists each change and how to carry it over when upgrading. Today that's rapier3d.

## Build settings

- **Optimized libraries, line tables only.** `[profile.dev.package."*"]` in the workspace `Cargo.toml` optimizes third-party crates (and `render` and `nav`) in debug builds. It keeps only file-and-line debug info for third-party crates, so panics and backtraces still name the line. The engine's own crates keep full debug info for a debugger. With full debug info everywhere, the editor binary was 1 GB instead of 550 MB, a cold build peaked at 3.9 GB of RAM instead of 2.4 GB, and edits rebuilt 15–30% slower.
- **Pinned toolchain.** `rust-toolchain.toml` names one Rust release (1.97.0), and the CI and release workflows (off for now) install the same one (`dtolnay/rust-toolchain@1.97.0`). Clippy runs with `-D warnings`, so on `stable` a new compiler's new lints could fail the checks with no code change. Raise all three together, and fix the new warnings in the same change.
- **Release and dist profiles.** `--release` uses thin LTO and keeps line tables, so panics still name the line. `release.yml` builds with `--profile dist` (into `target/dist/`): fat LTO, one codegen unit and stripped debug info, for the fastest shipped binaries at the cost of a longer build. Every profile keeps `panic = "unwind"`: asset loaders and editor Play recover from panics with `catch_unwind`.
- **Linker.** Rust links with LLD on x86-64 Linux by default, which is already fast. A faster linker such as mold saves little here.
- **rust-analyzer** gets its own target folder (`rust-analyzer.cargo.targetDir` in `.vscode/settings.json`). Its checks on save then don't hold the build lock, which would otherwise keep `cargo` waiting with "Blocking waiting for file lock". Its first check builds once into `target/rust-analyzer`.

Rebuild times after an edit: about 9 s to rebuild every test after a change to `core`'s API, 5–6 s for the editor binary or its tests after an editor change, and 2 s for one crate's tests.

## Low memory

If the machine starts swapping during a build, use fewer parallel jobs: `cargo build -j 4`, or `CARGO_BUILD_JOBS=4` in your shell. The biggest single compiler or linker process in a cold build peaks around 2.4 GB.

The target folder only grows (each `Cargo.lock` change keeps old builds). `cargo clean` empties it, and the next build is a cold one: about 6 minutes for every test on the laptop above.
