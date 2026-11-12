---
description: Build Game for Windows, macOS and Linux, what the build contains, the loading screen, credits and licences, testing a build, and multiplayer hosting.
icon: package
samples: [hello-part, hello-loading]
---

# Publishing your game

**File > Build Game** turns a project into a game players can run without the editor.

## Building

1. Save the project.
2. Choose **File > Build Game ▸ For this OS** (or **Windows**, **macOS** or **Linux**).
3. Pick an empty output folder.

The build writes two things: the **player** (`AxcenityPlayer.exe` on Windows, `AxcenityPlayer` on Linux, `AxcenityPlayer.app` on macOS) and **`game.axgame`**, an archive with your world, levels, scripts, styles and assets. Block scripts are translated to Lua, and textures and models go in already compressed. Editor-only files stay behind. Zip the folder and share it.

With an icon, the game's window and taskbar show it, and the build adds it for each system: `game.ico` beside the Windows player (for shortcuts and installers; the `.exe` file itself keeps Axcenity's icon), `game.icns` in the macOS app (Finder and the Dock show it), and `icon.png` beside the Linux player. The macOS app and the Linux launcher carry your game's name.

Every build also writes `LICENSES.txt` (the engine's notices) and, when you used Toolbox assets, `CREDITS.txt` with the credits their licences ask for.

## Before you ship

- **Loading screen** (File > Build Game > Loading screen): the Axcenity splash, none, or one of your own ScreenGuis.
- **Developer Console (F9)**: off by default in builds; tick it for a test build.
- **Your game's icon and banner** (Settings > This project > Branding): pick a square PNG for the icon (16×16 up to 1024×1024; 256×256 or more looks sharp) and a wide PNG or JPEG for the banner (at least 640×160, two to six times as wide as it is high). The settings show each one and say what's wrong with a picture that won't do; until it's fixed, the build uses Axcenity's. Show them in your own UI with an ImageLabel whose Image is `branding://icon` or `branding://banner`.
- Play the build on a computer without the editor, and check `logs/latest.log` in the game's data folder if something goes wrong.
- Run your game tests (`axcenity-test`) one last time.

## Building for another OS

Players for other systems are included in release downloads of the editor, so a Windows computer can build for macOS and Linux. Windows can't mark a file as a program for those systems, so the build also writes a `.zip` that keeps the mark: send that zip and unzip it on the Mac or Linux computer.

From a terminal: `axcenity-editor --build-game <project folder> <output folder> --target linux`.

## Multiplayer

Built games can host and join over the local network: start the player with `--host`, and others join with `--join <address>`. A dedicated server, `axcenity-server`, runs a game with no window. Multiplayer is **partly built**: it's for LAN play and trusted friends (connections aren't encrypted or authenticated yet), and internet servers aren't supported. See [Multiplayer and networking](../reference/networking.md).

> **Known limits.** macOS builds aren't signed with an Apple Developer ID or notarized: the first time, players Control-click the app and choose **Open**. There's no web, mobile or console export; Axcenity targets Windows, macOS and Linux desktops.
