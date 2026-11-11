## Install

| You are on | Download | Then |
|---|---|---|
| Windows | `AxcenityEditor-__VERSION__-windows-x86_64.exe` | Run the setup wizard. If SmartScreen appears, **More info** → **Run anyway**. |
| macOS | `AxcenityEditor-__VERSION__-macos-universal.dmg` | Agree to the license, drag **Axcenity Editor** onto **Applications**. Control-click the app → **Open** (unsigned build). |
| Debian / Ubuntu | `AxcenityEditor-__VERSION__-linux-x86_64.deb` | `sudo apt install ./AxcenityEditor-__VERSION__-linux-x86_64.deb` |
| Fedora / RHEL / openSUSE | `AxcenityEditor-__VERSION__-linux-x86_64.rpm` | `sudo dnf install ./AxcenityEditor-__VERSION__-linux-x86_64.rpm` |
| Arch Linux | `AxcenityEditor-__VERSION__-linux-x86_64.pkg.tar.zst` | `sudo pacman -U AxcenityEditor-__VERSION__-linux-x86_64.pkg.tar.zst` |
| Any Linux (portable) | `AxcenityEditor-__VERSION__-linux-x86_64.tar.gz` | Extract and run `./axcenity-editor`. |
| Any Linux (no install) | `AxcenityEditor-__VERSION__-linux-x86_64.AppImage` | `chmod +x` that file, then run it. |

Each editor build includes players for Windows, macOS and Linux, so **File > Build Game** can target any of them. The `-update.zip` file is what the editor's own updater installs; you don't need it. Full steps, including Gatekeeper and Vulkan notes: [docs/project/install.md](https://github.com/MrBearson321/Axcenity-Engine/blob/main/docs/project/install.md).

## Checking the download

`SHA256SUMS.txt` lists every file's SHA-256. `axcenity-release.json` lists them too and is signed with minisign: `minisign -Vm axcenity-release.json -p axcenity-release.pub`, with the key from [packaging/axcenity-release.pub](https://github.com/MrBearson321/Axcenity-Engine/blob/main/packaging/axcenity-release.pub). `axcenity-history.json` is the release history the editor's Version Explorer reads.
