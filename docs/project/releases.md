# GitHub releases

How a tagged build becomes the installers on [GitHub Releases](https://github.com/MrBearson321/Axcenity-Engine/releases). Installing those files is [install.md](install.md).

## Version numbers

Public releases use **SemVer** with a channel suffix while the engine is in alpha:

| Tag | Meaning |
|---|---|
| `v0.1.0-alpha.1` | First alpha. Bump the last number for each alpha (`-alpha.2`, …). |
| `v0.1.0-beta.1` | Beta, later. |
| `v0.1.0-rc.1` | Release candidate, later. |
| `v0.1.0` | First stable 0.1. GitHub marks it as a full release, not a pre-release. |

The first packaged alpha is **`v0.1.0-alpha.1`**; bump the last number when a tag is retried (`-alpha.2`, `-alpha.3`, …). Anything with `-alpha`, `-beta` or `-rc` is uploaded as a GitHub pre-release.

The crate version in `Cargo.toml` stays `0.1.0` until 0.1 is stable. The tag is the product version shown in the installer and on the release.

## What a release contains

| File | Platform |
|---|---|
| `AxcenityEditor-<ver>-windows-x86_64.exe` | NSIS setup wizard (current-user install) |
| `AxcenityEditor-<ver>-macos-universal.dmg` | License agreement, then drag the app onto Applications |
| `AxcenityEditor-<ver>-linux-x86_64.deb` | Debian / Ubuntu |
| `AxcenityEditor-<ver>-linux-x86_64.rpm` | Fedora / RHEL / openSUSE |
| `AxcenityEditor-<ver>-linux-x86_64.pkg.tar.zst` | Arch Linux |
| `AxcenityEditor-<ver>-linux-x86_64.tar.gz` | Portable folder |
| `AxcenityEditor-<ver>-linux-x86_64.AppImage` | Run in place |
| `SHA256SUMS.txt` | Checksums |

Each editor payload includes `axcenity-player` for the host OS beside the editor, and `players/windows`, `players/macos`, `players/linux` for **File > Build Game**. macOS editor and player binaries are universal (Apple Silicon + Intel). Linux builds use Ubuntu 22.04 so the glibc requirement stays at 2.35. Windows and Linux packages are x86_64.

Installer pictures (NSIS header/sidebar, DMG background) come from `python packaging/generate-assets.py`, which reads `assets/branding` and Inter.

## Cutting a release

GitHub Actions is intentionally turned off ([development policy](../development-policy.md#continuous-integration)), and that includes the Release workflow. Turn **Release** back on in the repository's Actions settings for the release, and off again after.

1. Main passes the local platform checks on Windows and Linux (`scripts/check-platform.ps1`, `scripts/check-platform.sh`; [development.md](../development.md)), and on macOS too if the release ships a macOS build that should be checked.
2. Push a tag, or run **Actions > Release** and type the version without `v`:

   ```bash
   git tag v0.1.0-alpha.1
   git push origin v0.1.0-alpha.1
   ```

3. [.github/workflows/release.yml](../../.github/workflows/release.yml) builds the editor and player on Windows, macOS and Linux, packs them, and publishes the GitHub Release. Re-running the workflow on the same tag replaces the files.

Unsigned: Windows SmartScreen and macOS Gatekeeper will warn. Signing and notarizing need certificates that this workflow does not have yet.
