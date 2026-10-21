# Installing a release

Alpha builds are on [GitHub Releases](https://github.com/MrBearson321/Axcenity-Engine/releases). Filenames look like `AxcenityEditor-0.1.0-alpha.1-<platform>.<ext>`. Check `SHA256SUMS.txt` on the same release if you want to confirm the download.

Each editor package includes the player for that OS beside the editor, and players for the other two OSes under `players/windows`, `players/macos` and `players/linux`, so **File > Export Game** can target Windows, macOS or Linux.

This is an alpha: expect bugs and breaking changes. Builds are not code-signed.

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

Install a Vulkan driver for your GPU (`mesa-vulkan-drivers` on Debian/Fedora; `vulkan-icd-loader` plus a vendor package on Arch). Nothing is copied into system folders.

## AppImage

```bash
chmod +x AxcenityEditor-*-linux-x86_64.AppImage
./AxcenityEditor-*-linux-x86_64.AppImage
```

You can also double-click it after marking it executable. Optional: [AppImageLauncher](https://github.com/TheAssassin/AppImageLauncher) to add a menu entry. Same Vulkan driver requirement as the tarball.
