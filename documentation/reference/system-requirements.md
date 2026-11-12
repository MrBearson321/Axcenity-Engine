---
description: The computers Axcenity runs on: supported operating systems, and minimum and recommended processor, memory, graphics, network and storage for making and playing games.
icon: gauge
---

# System requirements

What a computer needs to run the Axcenity editor, and the games you build with it. The numbers come from what the engine is built on (wgpu 30 for graphics, egui 0.36 for the editor, Luau for scripts) and from measurements on the machines listed at the end of this page.

> Axcenity is in **alpha**. These requirements can change between releases, most likely upward as rendering features such as ray tracing are added. Hardware below the minimum may still start; it just isn't something the engine is designed or tested for.

## At a glance

| | Minimum | Recommended | High end |
|---|---|---|---|
| **What you get** | Small and medium projects at the Low or Medium preset, 1080p | Most projects at the High preset, 1080p to 1440p | Big worlds and dense foliage at Ultra, 1440p and above; ray tracing when it lands |
| **Operating system** | Windows 10 64-bit (kept up to date), Ubuntu 22.04 (or another Linux with glibc 2.35), macOS 11 Big Sur | Windows 11, a current Linux distribution | Windows 11 or a current Linux distribution |
| **Processor** | 64-bit, 4 cores, with AVX (Intel Core or AMD Ryzen from the last decade) | 6 to 8 cores (AMD Ryzen 5 5600 or Intel Core i5-12400 class) | 8 or more fast cores (AMD Ryzen 7 7800X3D or Intel Core i7-13700K class) |
| **Memory** | 8 GB | 16 GB | 32 GB or more |
| **Graphics** | A DirectX 12, Vulkan or Metal GPU with 2 GB of video memory; integrated graphics works | A dedicated GPU with 8 GB (AMD Radeon RX 6600, NVIDIA GeForce RTX 3060 or Intel Arc A750 class) | 12 to 16 GB (AMD Radeon RX 9070 XT, NVIDIA GeForce RTX 4070 Ti Super or better) |
| **Network** | None to make and play single-player games | Broadband for the Toolbox, updates and Collaborate | Broadband, plus Tailscale for Collaborate across networks |
| **Storage** | 1 GB free for the editor plus your projects | An SSD with 20 GB free | An NVMe SSD with 50 GB free |
| **Display** | 1280 × 720 | 1920 × 1080 | 2560 × 1440 or above |

The editor and a game built with **File > Build Game** need the same kind of computer: a built game runs the same renderer and scripts, without the editor's panels. The figures above are for the editor, which uses more memory than a built game.

## Operating systems and platforms

| Platform | Support | Download |
|---|---|---|
| **Windows 10 and 11**, 64-bit (x86-64) | Main platform: checked on every change | Setup wizard (`.exe`) |
| **Linux**, 64-bit (x86-64), X11 or Wayland | Main platform: checked on every change | `.deb`, `.rpm`, `.pkg.tar.zst`, `.tar.gz` and AppImage |
| **macOS 11 Big Sur and later**, Apple silicon and Intel | Works and ships in every release, checked less often | One universal disk image (`.dmg`) |

- **Linux**: release builds are made on Ubuntu 22.04, so they need glibc 2.35 or newer: Ubuntu 22.04, Debian 12, Fedora 36, Linux Mint 21, current Arch, openSUSE Tumbleweed and their successors. The packages need ALSA, D-Bus and xkbcommon (on X11 also `libxkbcommon-x11`, which desktop installs already have), and a Vulkan driver (`mesa-vulkan-drivers` on Debian, Ubuntu and Fedora; `vulkan-icd-loader` plus `vulkan-radeon`, `vulkan-intel` or NVIDIA's driver on Arch).
- **Games you build** run on the same three systems. **File > Build Game** can build for any of them from any of them.
- **Servers**: the dedicated server (`axcenity-server`) and the Collaborate server (`axcenity-collab-server`) run on Windows or Linux with no display or GPU.
- **Not supported**: 32-bit systems, Windows on Arm and Linux on Arm (no builds are made for them), Windows 7 and 8, mobile (Android, iOS), web browsers and game consoles. Mobile and web are planned for later; see the engine's development policy.

## Processor

Any 64-bit x86-64 processor runs the engine. Apple silicon runs the native Arm build.

- **AVX**: scripts are compiled to native code where the processor has AVX (every Intel Core from the 2nd generation and every AMD Ryzen has it), and run in Luau's interpreter otherwise. Apple silicon always gets native code.
- **Cores**: physics, animation and drawing each run on several threads, so more cores help with many moving parts, many animated characters and big scenes. A game's scripts run on one thread, so a fast core matters as much as many cores.
- **Measured**: the standard 10,000-part benchmark scene (200 scripts, 50 sounds, 20 animated characters, 10 walking agents and 1,000 moving physics bodies) takes the player about 7 ms a frame on the development desktop (AMD Ryzen 9 9950X3D with an RX 9070 XT), about 140 fps. On the 4-core cloud test machine, the same scene's CPU work alone takes about 18 ms a frame, so a 4-core processor is the floor for scenes that busy, not a comfortable fit.

## Memory

| | Minimum | Recommended | High end |
|---|---|---|---|
| System memory (RAM) | 8 GB | 16 GB | 32 GB or more |

Measured on the cloud test machine (November 2026, release build): the editor uses about 750 MB with a small project open and about 850 MB with the 10,000-part benchmark scene, and the player about 900 MB running that scene. Those figures include the graphics memory, because that machine draws in software; with a real GPU, part of it moves to video memory. With your operating system, a browser and a code editor open as well, 8 GB is the least that's comfortable, and 16 GB leaves room for large projects.

Large projects take more: imported models and their textures are held in memory while the project is open, and terrain with tens of thousands of painted plants adds to that. Running a local AI model for the editor's AI assistant (an 8-billion-parameter model such as Qwen3 8B) needs about 8 GB more, in system or video memory.

## Graphics

Axcenity draws with **wgpu**, which uses **DirectX 12** on Windows, **Vulkan** on Linux and **Metal** on macOS. On Windows, DirectX 12 is the default and Vulkan can be chosen by setting the `WGPU_BACKEND` environment variable to `vulkan`.

**Minimum:**

- **Windows**: a DirectX 12 GPU (feature level 11_0 or higher) with a current driver, from about 2016 on: AMD Radeon RX 400 series, NVIDIA GeForce GTX 900 series, Intel HD Graphics 500 (6th generation Core) and newer.
- **Linux**: the same GPUs with a Vulkan driver: Mesa for AMD and Intel, or NVIDIA's own driver.
- **macOS**: any Mac that runs macOS 11 (they all have Metal).
- **2 GB of video memory** (or shared memory on integrated graphics).
- **BC texture compression**, which every desktop GPU has. Textures are kept compressed on the GPU (BC7, and BC5 for normal maps); without it they are decoded and take about four times the video memory.

The cards named on this page are examples of the class of GPU each tier needs, not a list of tested cards: the engine is tested on the machines at the end of this page.

Integrated graphics (such as Intel Iris Xe, AMD Radeon 780M or Apple silicon) runs the editor and small worlds: start at the Low or Medium preset and let the Graphics Benchmark (below) pick. A software renderer (Mesa's llvmpipe, Microsoft WARP) works, but is very slow, and the engine picks the Low preset on it.

**Recommended:** a dedicated GPU with 8 GB of video memory, such as the AMD Radeon RX 6600, NVIDIA GeForce RTX 3060 or Intel Arc A750, for the High preset (the look worlds are made with) at 1080p and 1440p.

**High end:** 12 to 16 GB of video memory, such as the AMD Radeon RX 9070 XT the engine is developed on, for Ultra at 1440p in big worlds. On that card, a dense foliage scene (a terrain with about 35,000 painted plants and two oak trees of about a million triangles each) draws at Ultra and 2560 × 1440 in about 11 ms, about 90 fps.

### What each preset needs

Each graphics preset has a texture memory budget; textures beyond it are shrunk to fit. The renderer's own frame targets and shadow maps come on top.

| Preset | Texture memory budget | Sun shadow maps | Suggested video memory |
|---|---|---|---|
| Low | 256 MB | 2 × 1024² | 2 GB |
| Medium | 512 MB | 2 × 2048² | 4 GB |
| High | 1 GB | 3 × 2048² | 6 to 8 GB |
| Ultra | 2 GB | 4 × 4096² | 8 GB or more |

The engine checks what the GPU can do when it starts and lowers any setting the GPU can't do, with a note in **Settings > Graphics**: a shadow map or reflection size larger than the GPU's textures, an MSAA level it lacks, or real-time global illumination on a GPU without compute shaders. It never lowers a setting you can run. To find the best settings for your computer, run **Debug > Graphics Benchmark…** in the editor: it measures each preset on your GPU and recommends one, and sets the texture budget to a quarter of your card's memory.

### Ray tracing (optional)

> **Known limits:** ray tracing isn't in this release yet. It's planned as an **optional** setting, off at every preset, and without it Axcenity draws exactly as it does today, so it will never raise the minimum.

When it lands, it will need:

- **Windows**: DirectX 12 Raytracing tier 1.1 and Shader Model 6.5: AMD Radeon RX 6000 series and newer, NVIDIA GeForce RTX 20 series and newer, Intel Arc.
- **Linux**: a Vulkan driver with ray queries (`VK_KHR_ray_query`) on the same GPUs.
- **macOS**: macOS 15 or later. Apple M3 and newer have ray tracing hardware.

For a sense of cost, on the RX 9070 XT one shadow or ambient occlusion ray per pixel at 1440p, over 20 million triangles, took about 0.3 ms.

## Network

Nothing in Axcenity needs an internet connection to make or play a single-player game. It's used for:

| Feature | What it needs |
|---|---|
| **Toolbox** (free textures, skies, models and sounds from ambientCG, Poly Haven and Openverse) | Internet. A high-resolution texture set is often tens of megabytes, so broadband helps. What you've downloaded works offline. |
| **Updates** | Internet, to check GitHub once a day and download new versions. You can turn this off in **Settings > Updates**. |
| **Multiplayer games** on a local network | UDP port **47800** for the game (any port can be chosen with `--host`) and UDP **47801** for finding games on the network. Each player gets up to about 24 KB/s of movement updates (about 0.2 Mbit/s), so a host with 8 players sends about 1.6 Mbit/s plus events. |
| **Collaborate** (editing a project together) | TCP port **47810** on the host. Across different networks, [Tailscale](https://tailscale.com) (free), which the editor sets up for you. A 10,000-part project is about 82 KB to join, and four busy members send about 19 KB/s. |
| **Collaborate projects on GitHub** | Internet, and Git 2.38 or newer installed (Git LFS for big files). |
| **AI assistant** | Nothing for a local model. A cloud assistant (Claude Code, Cursor, Codex) needs its own internet connection. |

Internet-hosted dedicated servers aren't supported yet: multiplayer is for local networks, a forwarded port or a VPN such as Tailscale.

## Storage

- **The editor** takes about 300 MB installed: the editor itself (about 110 MB), its player for your system, and the players for the other two systems that **File > Build Game** copies into a game (about 50 MB each). Windows builds are about 20 MB larger each because they carry Microsoft's DirectX shader compiler, and the macOS app about twice the size because it holds both Apple silicon and Intel code.
- **A built game** is the player (about 50 to 75 MB) plus one `game.axgame` file holding the game's files.
- **Projects** are folders of text files plus your assets. Small projects are a few megabytes; projects with many imported 4K textures run to gigabytes (one test project's models are 1.7 GB).
- **Cooked assets**: the editor keeps the GPU-ready copies of textures and models it has prepared in each project's `.axcenity` folder, which takes extra space beside your assets.
- **Toolbox downloads** are kept in a shared asset library, which grows with what you download.
- **An SSD is recommended.** Projects open, and big worlds stream in, faster from an SSD. A hard drive works.

## Building the engine from source

Only needed to change the engine itself, not to make games.

- **Rust 1.97.0**, installed with rustup (the repository picks the version).
- **Windows**: Visual Studio 2022 (17.11 or later) or its Build Tools, with **Desktop development with C++** and **C++ ATL for latest v143 build tools (x86 & x64)**.
- **Linux**: a C compiler, `pkg-config`, and the development packages for D-Bus, ALSA and udev (`libdbus-1-dev libasound2-dev libudev-dev` on Debian and Ubuntu).
- **macOS**: Xcode's command line tools.
- **Memory**: 16 GB recommended. The largest single compiler or linker process in a cold build peaks around 2.4 GB, so 8 GB works with fewer parallel jobs (`cargo build -j 4`).
- **Storage**: about 30 GB free. The build folder is about 9 GB for a debug build of everything and grows with each build; `cargo clean` empties it.
- **Time**: a cold build of everything with its tests takes about 6 minutes on a 6-core laptop (AMD Ryzen 5 5500U, 14 GB).

## Machines these numbers come from

| Machine | Hardware | What it measured |
|---|---|---|
| Development desktop | AMD Ryzen 9 9950X3D (16 cores), 64 GB, AMD Radeon RX 9070 XT (16 GB), Windows 11, DirectX 12 | Frame times, graphics presets, ray tracing |
| Laptop | AMD Ryzen 5 5500U (6 cores), 14 GB | Build times from source |
| MacBook Pro | Apple silicon, macOS 26 | macOS packages and Collaborate |
| Cloud test machine | 4 cores (Intel Xeon, 2.1 GHz), 15 GB, no GPU (software Vulkan) | Memory use and install size |

Your own computer: **Debug > Graphics Benchmark…** in the editor measures how it draws, and **Settings > Graphics** shows what the GPU can do and anything the engine lowered.
