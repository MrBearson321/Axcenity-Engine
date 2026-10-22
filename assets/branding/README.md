# Branding

The Axcenity Engine logo: a geometric **A** (the **mark**) with the name "Axcenity" beside it (the **wordmark**). The identity is black, white and gray only.

The sources are black ink on white. `build.py` keys the white out by brightness, so every logo file is ink on a transparent background, with no plate or haze. Only the app icon has a plate: a rounded charcoal tile.

| File | Size | Use |
|---|---|---|
| `mark-source.png` | as supplied | The A on white. |
| `wordmark-source.png` | as supplied | The mark and name on white. |
| `mark.png` | 1024×1024 | The black A, transparent. The NSIS header (on white). |
| `mark-white.png` | 1024×1024 | The white A, transparent. The NSIS sidebar and the icon tile. |
| `mark-white-small.png` | 48×48 | The white A for the editor's header and start screen, tinted with the theme's text colour. |
| `wordmark.png` | 960 wide | The black wordmark, transparent, for light backgrounds. |
| `wordmark-white.png` | 960 wide | The white wordmark, transparent. The editor's About box (tinted) and both splashes; the disk-image background. |
| `icon.png` | 1024×1024 | The app icon: the white A on a rounded charcoal tile, see-through corners. |
| `icon-256.png` | 256×256 | The editor's and the player's window icon (built in with `include_bytes!`). |
| `icon.ico` | 16 to 256 | The Windows installer icon. |
| `favicon-32.png` | 32×32 | A PNG favicon for a website. |

To change the logo, replace `mark-source.png` and/or `wordmark-source.png` (black on white) and run `python assets/branding/build.py` (needs Pillow and NumPy). If faint background texture survives, lower `PAPER` in `build.py`; if edges look chewed, raise it. Then run `python packaging/generate-assets.py` so the Windows installer bitmaps and the macOS disk-image background match.

Windows and Linux (X11) show the window icon in the title bar and taskbar. macOS and Wayland ignore window icons: there, the app bundle or desktop file sets the icon, which comes with exporting.

egui under wgpu has no mipmaps, so an image drawn at under half its size looks jagged. That's why the header's mark has its own 48-pixel copy.
