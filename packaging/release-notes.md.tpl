# Axcenity Engine __VERSION__

This is an **alpha** build of the editor and player. Expect bugs, missing polish and breaking changes. Games you make are yours; see the [README](https://github.com/MrBearson321/Axcenity-Engine#license) for licensing.

## Install

| You are on | Download | Then |
|---|---|---|
| Windows | `AxcenityEditor-__VERSION__-windows-x86_64.exe` | Run the setup wizard. If SmartScreen appears, **More info** → **Run anyway**. |
| macOS | `AxcenityEditor-__VERSION__-macos-universal.dmg` | Agree to the license, drag **Axcenity Editor** onto **Applications**. Control-click the app → **Open** (unsigned alpha). |
| Debian / Ubuntu | `AxcenityEditor-__VERSION__-linux-x86_64.deb` | `sudo apt install ./AxcenityEditor-__VERSION__-linux-x86_64.deb` |
| Fedora / RHEL / openSUSE | `AxcenityEditor-__VERSION__-linux-x86_64.rpm` | `sudo dnf install ./AxcenityEditor-__VERSION__-linux-x86_64.rpm` |
| Arch Linux | `AxcenityEditor-__VERSION__-linux-x86_64.pkg.tar.zst` | `sudo pacman -U AxcenityEditor-__VERSION__-linux-x86_64.pkg.tar.zst` |
| Any Linux (portable) | `AxcenityEditor-__VERSION__-linux-x86_64.tar.gz` | Extract and run `./axcenity-editor`. |
| Any Linux (no install) | `AxcenityEditor-__VERSION__-linux-x86_64.AppImage` | `chmod +x` that file, then run it. |

Checksums are in `SHA256SUMS.txt`. Full steps, including Gatekeeper and Vulkan notes: [docs/project/install.md](https://github.com/MrBearson321/Axcenity-Engine/blob/main/docs/project/install.md).

Each editor build includes players for Windows, macOS and Linux, so **File > Export Game** can target any of those operating systems.

## Versioning

Releases are SemVer with a channel suffix while the engine is in alpha:

- `v0.1.0-alpha.N` — alpha (this channel)
- `v0.1.0-beta.N` — beta, later
- `v0.1.0` — the first stable 0.1

The crate version in `Cargo.toml` stays on the `0.1.0` line until 0.1 is stable.
