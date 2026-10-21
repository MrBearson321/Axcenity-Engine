# Branding

The Axcenity Engine logo: a blue hexagon made of cube faces (the **mark**), with the name "Axcenity" beside it in grey and orange among yellow and orange dots (the **wordmark**).

Colours: blue `#004AAD`, orange `#F1892D`, yellows `#F9BF3B` and `#FFCF4B`, greys `#545454` and `#AAAAAA`. On dark backgrounds the blue becomes `#4D86E8` and the dark grey near-white.

| File | Size | Use |
|---|---|---|
| `wordmark-source.png` | 1536×1024 | The master, as supplied, on white. Every other file is made from it by `build.py`. |
| `wordmark.png` | 840 wide | The wordmark for light backgrounds, transparent. The editor's About box (light theme). |
| `wordmark-dark.png` | 840 wide | The wordmark for dark backgrounds, transparent: the "Axc" letters are light with a soft grey outline. The same place, dark theme. |
| `wordmark-small.png`, `wordmark-dark-small.png` | 336 wide | The same, smaller. Not used by the editor now: its header and start screen draw `mark.svg`'s lines in the theme's colours (`widgets::paint_mark`). |
| `mark.svg` | vector | The hexagon on its own, redrawn as strokes (in the source, the letters cover parts of it). |
| `mark.png` | 1024×1024 | The hexagon in brand blue, transparent. |
| `icon.png` | 1024×1024 | The app icon: the hexagon on a white rounded square, so it shows on light and dark taskbars. |
| `icon-256.png` | 256×256 | The editor's and the player's window icon (built in with `include_bytes!`). |
| `icon.ico` | 16 to 256 | The Windows app icon for exported games and installers, and a website favicon. |
| `favicon-32.png` | 32×32 | A PNG favicon for a website. |

To change the logo, replace `wordmark-source.png` (same layout, white background) and run `python assets/branding/build.py` (needs Pillow and NumPy). If the hexagon moves, update the stroke coordinates in `build.py`. Then run `python packaging/generate-assets.py` so the Windows installer bitmaps and the macOS disk-image background match.

Windows and Linux (X11) show the window icon in the title bar and taskbar. macOS and Wayland ignore window icons: there, the app bundle or desktop file sets the icon, which comes with exporting.

egui under wgpu has no mipmaps, so an image drawn at under half its size looks jagged. The editor's images are about twice the size they're drawn at, for high-DPI screens; `build.py` makes a smaller copy for each place that draws them smaller.
