# Branding

The Axcenity Engine logo: an orange ring around a blue gear, with a cube in the middle. All files have a transparent background.

| File | Size | Use |
|---|---|---|
| `icon.png` | 1024×1024 | The master copy. Make other sizes from this one. |
| `icon-256.png` | 256×256 | The editor's and the player's window icon (built in with `include_bytes!`). |
| `icon.ico` | 16 to 256 | The Windows app icon for exported games and installers, and a website favicon. |
| `favicon-32.png` | 32×32 | A PNG favicon for a website. |

Windows and Linux (X11) show the window icon in the title bar and taskbar. macOS and Wayland ignore window icons: there, the app bundle or desktop file sets the icon, which comes with exporting.

A wordmark (the logo with the name beside it) for splash and loading screens is still to come. The first version spelled the name "Axenity" and had dark grey text, which disappears on a dark background.
