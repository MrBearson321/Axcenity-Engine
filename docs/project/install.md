# Installing a release

Alpha builds are on [GitHub Releases](https://github.com/MrBearson321/Axcenity-Engine/releases). Filenames look like `AxcenityEditor-0.1.0-alpha.1-<platform>.<ext>`. Check `SHA256SUMS.txt` on the same release if you want to confirm the download.

Each editor package includes the player for that OS beside the editor, and players for the other two OSes under `players/windows`, `players/macos` and `players/linux`, so **File > Build Game** can target Windows, macOS or Linux.

This is an alpha: expect bugs and breaking changes. Builds are not code-signed.

What a computer needs (supported systems, processor, memory, graphics, network and storage): [System requirements](../../documentation/reference/system-requirements.md).

## Windows (`.exe`)

1. Download `AxcenityEditor-*-windows-x86_64.exe`.
2. Open it. The setup wizard shows the license, then the install folder (default: `%LOCALAPPDATA%\Programs\Axcenity Engine`), Start menu name, and an optional desktop shortcut.
3. If **Windows protected your PC** (SmartScreen) appears, choose **More info**, then **Run anyway**, after you have checked you trust the file.
4. Finish, and optionally launch the editor.

Uninstall from **Settings > Apps > Axcenity Editor**, or the Uninstall shortcut in the Start menu folder. Project folders are left as they are.

## macOS (`.dmg`)

1. Download `AxcenityEditor-*-macos-universal.dmg` (Apple Silicon and Intel).
2. Open it. Agree to the Apache License 2.0.
3. Drag **Axcenity Editor** onto **Applications**.
4. The first open is blocked because the build is not notarized: **Control-click** the app, choose **Open**, then confirm. If it is still blocked, **System Settings > Privacy & Security > Open Anyway**.

## Debian, Ubuntu and similar (`.deb`)

Double-click the `.deb` to open your software centre, or:

```bash
sudo apt install ./AxcenityEditor-*-linux-x86_64.deb
```

That puts the editor in `/opt/axcenity`, a menu entry, and `axcenity-editor` / `axcenity-player` on your `PATH`. You also want a Vulkan driver, for example `mesa-vulkan-drivers` (already a recommended package).

Remove with `sudo apt remove axcenity-editor`.

## Fedora, RHEL, openSUSE (`.rpm`)

Double-click the `.rpm`, or:

```bash
sudo dnf install ./AxcenityEditor-*-linux-x86_64.rpm
# openSUSE:
sudo zypper install ./AxcenityEditor-*-linux-x86_64.rpm
```

Same layout as the `.deb`: `/opt/axcenity` and commands on your `PATH`.

## Arch Linux (`.pkg.tar.zst`)

```bash
sudo pacman -U AxcenityEditor-*-linux-x86_64.pkg.tar.zst
```

Needs `vulkan-icd-loader` and a vendor ICD (`vulkan-radeon`, `vulkan-intel`, or NVIDIA's).

## Portable tarball (`.tar.gz`)

```bash
tar -xzf AxcenityEditor-*-linux-x86_64.tar.gz
cd AxcenityEditor-*-linux-x86_64
./axcenity-editor
```

Install a Vulkan driver for your GPU (`mesa-vulkan-drivers` on Debian/Fedora; `vulkan-icd-loader` plus a vendor package on Arch). On an X11 desktop you also need `libxkbcommon-x11` (`libxkbcommon-x11-0` on Debian and Ubuntu), which desktop installs normally have; without it the editor and player stop at start with "Library libxkbcommon-x11.so could not be loaded". Nothing is copied into system folders.

## AppImage

```bash
chmod +x AxcenityEditor-*-linux-x86_64.AppImage
./AxcenityEditor-*-linux-x86_64.AppImage
```

You can also double-click it after marking it executable. Optional: [AppImageLauncher](https://github.com/TheAssassin/AppImageLauncher) to add a menu entry. Same Vulkan driver requirement as the tarball.

## Updating

The editor looks for new releases on GitHub once a day (Settings > Updates: the channel to follow, how often, or Check now). It only says an update is there after checking the release's signed manifest, and it never installs anything without asking.

- **Windows setup and the Linux `.tar.gz`** update themselves: Settings > Updates > **Download and install**, then **Restart and install**. The editor asks you to save, closes, swaps its folder for the new version, checks the new version starts, and opens your project again. If the new version doesn't start, the old one is put back. **Roll back** in the same place returns to the version it replaced. Your projects, settings and saves are never touched.
- **`.deb`, `.rpm`, Arch, AppImage and macOS**: Settings > Updates says a new version is out and links it; install it the same way as the first time.

Opening a project from an older Axcenity asks before upgrading it, and keeps a copy of its files from before in the project's `.axcenity/previous/<format>/` folder. After the upgrade, an older Axcenity can't open it; open that copy instead.
