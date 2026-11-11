# GitHub releases

How a tagged build becomes the installers on [GitHub Releases](https://github.com/MrBearson321/Axcenity-Engine/releases). Installing those files is [install.md](install.md).

## Version numbers

**The one version is `[workspace.package] version` in the root `Cargo.toml`**, including its pre-release suffix (`0.2.0-alpha.1`). Every crate inherits it, so the editor's About box, the start page, `--version` on the editor, player and server, and the release tag all show the same number. The tag must be `v` plus that version; `axcenity-release check --tag v…` refuses anything else. Between releases, `main` carries the version of the next release, and a local build says so with its commit ("0.2.0-alpha.1 · Development · local build of 1a2b3c4d5").

SemVer 2.0, with one of three pre-release tags. The channel follows from the version and is never written separately:

| Version | Channel | GitHub release |
|---|---|---|
| `0.2.0` | Stable | full release |
| `0.2.0-rc.1`, `0.2.0-beta.1` | Beta | pre-release |
| `0.2.0-alpha.1` | Development | pre-release |

While the engine is 0.x, a breaking change (an old engine can't open new projects, a script API removed) bumps the minor number, new features bump the minor number, and fixes alone bump the patch. Bump the last number of a pre-release (`-alpha.2`, …) for each new build of the same version, including a retried tag.

History so far: `v0.1.0-alpha.1` and `v0.1.0-alpha.2` are tags that never became releases, and `v0.1.0-alpha.3` (2026-10-21) is the one published release. Its editor reported itself as `0.1.0` (known issue AX-0002). The next release is `0.2.0-alpha.1`.

## Release notes and history

Every pull request that changes something a user would notice adds one change entry to `releases/unreleased/`. How to write one, and the feature and issue files beside it: [releases/README.md](../../releases/README.md). `axcenity-release` (`crates/release`) checks them and answers questions about the history:

```bash
cargo run -p axcenity-release -- check                       # releases/ and Cargo.toml agree
cargo run -p axcenity-release -- prepare 0.2.0-alpha.1       # start the release record from releases/unreleased/
cargo run -p axcenity-release -- query compare 0.1.0-alpha.3 unreleased
cargo run -p axcenity-release -- query introduced terrain.studio
cargo run -p axcenity-release -- query search physics --json
```

The design (the manifest, the update flow, the Version Explorer) is in [research/release-system](../research/release-system/README.md).

## What a release contains

| File | What it is |
|---|---|
| `AxcenityEditor-<ver>-windows-x86_64.exe` | NSIS setup wizard (current-user install) |
| `AxcenityEditor-<ver>-windows-x86_64-update.zip` | What the editor's updater installs on Windows: the setup's files, at the archive root (`packaging/update-zip.py`) |
| `AxcenityEditor-<ver>-macos-universal.dmg` | License agreement, then drag the app onto Applications |
| `AxcenityEditor-<ver>-linux-x86_64.deb` | Debian / Ubuntu |
| `AxcenityEditor-<ver>-linux-x86_64.rpm` | Fedora / RHEL / openSUSE |
| `AxcenityEditor-<ver>-linux-x86_64.pkg.tar.zst` | Arch Linux |
| `AxcenityEditor-<ver>-linux-x86_64.tar.gz` | Portable folder; also what the updater installs on Linux |
| `AxcenityEditor-<ver>-linux-x86_64.AppImage` | Run in place |
| `axcenity-release.json` | The manifest: version, channel, notes, known issues, formats, and every package's name, size and SHA-256 |
| `axcenity-release.json.minisig` | Its signature. The updater trusts nothing else directly |
| `axcenity-history.json` | The public release history, for the Version Explorer |
| `SHA256SUMS.txt` | Checksums |

Each editor payload includes `axcenity-player` for the host OS beside the editor, and `players/windows`, `players/macos`, `players/linux` for **File > Build Game**, and an `install.json` that says how it was installed (`windows-setup`, `linux-tarball`, `appimage`, `deb`, `rpm`, `pacman` or `macos-dmg`; `crates/release/src/install.rs`). Only the Windows setup, the tar.gz and the AppImage will update themselves. macOS editor and player binaries are universal (Apple Silicon + Intel). Linux builds use Ubuntu 22.04 so the glibc requirement stays at 2.35. Windows and Linux packages are x86_64.

Installer pictures (NSIS header/sidebar, DMG background) come from `python packaging/generate-assets.py`, which reads `assets/branding` and Inter.

## Cutting a release

GitHub Actions runs only for release tags, so each release run is deliberate ([development policy](../development-policy.md#continuous-integration)). **Tell Paul before pushing a tag**: the version, the platforms and roughly how long it takes.

1. **Prepare** on a branch: `cargo run -p axcenity-release -- prepare 0.2.0-alpha.1`. It moves `releases/unreleased/` into `releases/history/0.2.0-alpha.1.toml`, sets the version in `Cargo.toml` and `Cargo.lock`, and marks the issues those entries fix as fixed. Write the record's `title` (the release's name on GitHub, after "Axcenity Engine 0.2.0-alpha.1:"), `summary` and `[formats]` (the project, world, save and network format versions it reads and writes). Read the notes it will publish: `cargo run -p axcenity-release -- notes 0.2.0-alpha.1`.
2. **Check locally** on Windows and Linux: the platform checks (`scripts/check-platform.ps1`, `scripts/check-platform.sh`; [development.md](../development.md)), the package check ([below](#checking-packages)), and `cargo run -p axcenity-release -- check --tag v0.2.0-alpha.1`. Record what ran in the record's `[verification]`. Merge the branch.
3. **Tag** the merge commit and push the tag:

   ```bash
   git tag v0.2.0-alpha.1
   git push origin v0.2.0-alpha.1
   ```

4. [.github/workflows/release.yml](../../.github/workflows/release.yml) runs:
   - **Validate** (Linux, about two minutes): `check --tag`, the release tool's tests, the plan (channel, profile, platforms), that the tag isn't already a published release, and that the signing key's public half is committed. Nothing else starts if it fails.
   - **Build** on Windows, macOS and Linux, from scratch, with the profile for the channel: `release` for Development, the fat-LTO `dist` for Beta and Stable. Each build is official (`AXCENITY_OFFICIAL_BUILD=1`, the tag and the commit), and `scripts/check-release-build.py` checks that every binary's `--version --json` says so. One failing build cancels the others.
   - **Package** each OS as before, plus the Windows update zip.
   - **Sign and draft**, in the `release` environment: writes `axcenity-history.json` and `axcenity-release.json`, signs the manifest with minisign, verifies the signature with `packaging/axcenity-release.pub`, writes `SHA256SUMS.txt`, and creates a **draft** release with the notes from `axcenity-release notes`. With a required reviewer on the environment, this job waits for that person's approval first.
5. **Publish**: open the draft on GitHub, read it, and press **Publish release**. Nothing reaches users before this. A published release never changes; a mistake is fixed by the next release (and a `[[corrections]]` entry in its record).

To build an existing tag again (for example after a runner failure), run **Actions > Release > Run workflow** with dry run off and the tag filled in. It replaces the files of a draft, and refuses a tag that is already published.

### Costs

The repository is public, so GitHub's standard runners cost nothing. Runs are still kept rare and short: nothing runs on pushes or pull requests, the cheap Linux checks run before any other machine starts, and a failure stops the rest. Every release builds from scratch without a cache: caches are per tag, so one tag's cache wouldn't help the next, and skipping them keeps a run started by hand from writing a cache that later runs trust. A full run takes roughly an hour of runner time across the three OSes.

### Signing key

The manifest is signed with [minisign](https://jedisct1.github.io/minisign/) (Ed25519). The editor and the updater will check that signature with the public key built into them, and every other file by its SHA-256 in the manifest. One-time setup, by Paul:

1. Make the key pair without a password (GitHub keeps the secret encrypted; a password stored beside it adds nothing): `minisign -G -W -p axcenity-release.pub -s axcenity-release.key`.
2. Commit `axcenity-release.pub` as `packaging/axcenity-release.pub`.
3. In the repository's **Settings > Environments**, create `release`, add yourself as a required reviewer, and add an environment secret `MINISIGN_SECRET_KEY` with the whole contents of `axcenity-release.key`. Then keep the `.key` file offline (a password manager), not in the repository.
4. Run **Actions > Release > Run workflow** with dry run on. Its **Signing key check** signs a scratch file and verifies it with the committed public key.

Rotating the key means shipping the new public key in a release signed with the old one ([research/release-system/architecture.md](../research/release-system/architecture.md#9-security-and-integrity-d7)).

Windows SmartScreen and macOS Gatekeeper still warn on first install: OS code signing and notarization need paid certificates and are deferred (known issue AX-0001).

### Testing the release workflow

Before pushing a change to a workflow:

- `python3 scripts/check-workflows.py`: no workflow runs on pushes or pull requests; every job and step output a workflow reads exists; every bash step parses; only the draft job can write, and only the `release` environment sees the secret.
- `cargo test -p axcenity-release`: the plan, notes, history file and manifest.
- The scripts the steps call, run locally: `packaging/stage.py`, `packaging/update-zip.py`, `packaging/linux/build-packages.sh`, `scripts/check-release-build.py` (its docstring shows how to make an official build locally).

Then, once merged, one dry run (**Run workflow** with dry run on) proves the trigger, permissions and signing key in about two Linux minutes, without building anything.

## Checking packages

Packages are checked on each computer before a tag, with one script (Python 3, from the repository root):

```bash
python3 scripts/check-packages.py                    # dist profile, as released (fat LTO: slow)
python3 scripts/check-packages.py --profile release  # quicker, for trying packaging changes
python3 scripts/check-packages.py --players DIR      # add other computers' players: DIR/windows/axcenity-player.exe, DIR/macos/axcenity-player, DIR/linux/axcenity-player
python3 scripts/check-packages.py --install          # Windows: also install silently into a scratch folder, check, uninstall
```

(`py scripts\check-packages.py` on Windows.) It runs the release workflow's steps for this OS, then checks what they made. Every step runs even after a failure; the summary is saved to `target/package-check-<os>.txt` and the packages to `target/package-check/out/`.

1. Builds `axcenity-editor` and `axcenity-player`, then stages the payload (`packaging/stage.py --allow-missing-players`: a local build has only this OS's player, so the others come only from `--players`). Checks every file the packages ship, and that each program is the right kind for its OS (ELF, PE or Mach-O, read from its header) and marked executable.
2. Packages with the tools that are installed. A missing tool is a SKIP, not a FAIL.
   - **Linux**: the `.tar.gz` always; `.deb`, `.rpm`, `.pkg.tar.zst` with [nfpm](https://nfpm.goreleaser.com); the AppImage with [appimagetool](https://github.com/AppImage/appimagetool). Each is opened again and checked (`dpkg-deb -x`, `rpm -qlp`, `--appimage-extract`), with `desktop-file-validate` on the menu entry when it's installed.
   - **Windows**: the setup `.exe` with [NSIS](https://nsis.sourceforge.io) (`choco install nsis`). With `--install`, it installs silently into a scratch folder, checks the files and uninstalls; it refuses when Axcenity Editor is already installed on the account, since the uninstall would remove that one's registration.
   - **macOS**: the `.dmg` (`build-dmg.sh`; dmgbuild is fetched into a throwaway venv, so it needs the network). It's mounted read-only and checked: the app's `Info.plist` and icon, `codesign --verify --deep --strict`, the payload.
3. **Build Game with the packaged editor** (the extracted tarball, the mounted app, the installed copy, or else the staged files the installer copies as they are): `axcenity-editor --build-game` on a copy of `examples/hello-part` for each OS the package has a player for, checking each build's files (and on Windows, that the macOS and Linux zips mark the player executable).
4. **The built game for this OS starts**: it runs `AxcenityPlayer --bench 30` from another folder and expects its first frame (on Linux with no display, under `xvfb-run`).

What one computer can't check: a package for another OS, and the games built for it actually running. Copy `target/package-check/work` (kept with `--keep`) builds across to try them, or run the script there.

Cross-checks from Linux, without a Windows computer (checked 2026-11-10 on Ubuntu 24.04):

- The player builds for Windows with `rustup target add x86_64-pc-windows-gnu` and MinGW (`apt install g++-mingw-w64-x86-64`). Use the posix-thread compilers; with the default win32 ones, Luau's C++ fails to link (`__mingwthr_key_dtor`, `__gthr_win32_mutex_*`):

  ```bash
  export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc-posix \
    CC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-gcc-posix \
    CXX_x86_64_pc_windows_gnu=x86_64-w64-mingw32-g++-posix
  cargo build --release --locked -p axcenity-player --target x86_64-pc-windows-gnu
  mkdir -p target/cross-players/windows
  cp target/x86_64-pc-windows-gnu/release/axcenity-player.exe target/cross-players/windows/
  python3 scripts/check-packages.py --profile release --players target/cross-players
  ```

  The Linux package then has a Windows player, and Build Game for Windows is checked too. Releases use the MSVC build from a Windows computer; this shows only that the Windows code compiles and links, not that it runs.
- `makensis` from the distribution's `nsis` package compiles `installer.nsi` (NSIS takes `-DNAME=value` there instead of `/D`), so a change to the installer script can be checked before a Windows computer is at hand.
- macOS can't be cross-built from Linux or Windows: it needs Apple's SDK. The macOS package and macOS games are checked only on a Mac.
