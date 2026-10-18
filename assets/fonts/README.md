# Fonts

The editor's fonts, built into it with `include_bytes!` (see `crates/editor/src/theme.rs`).

| File | Use | Licence |
|---|---|---|
| `Montserrat-Regular.ttf` | All UI text | SIL Open Font License 1.1 (`Montserrat-OFL.txt`) |
| `Montserrat-SemiBold.ttf` | Headings and panel titles | SIL Open Font License 1.1 (`Montserrat-OFL.txt`) |
| `JetBrainsMono-Regular.ttf` | Code, and the Output panel | SIL Open Font License 1.1 (`JetBrainsMono-OFL.txt`) |

Sources: [Montserrat](https://github.com/JulietaUla/Montserrat) (`fonts/ttf`) and [JetBrains Mono](https://github.com/JetBrains/JetBrainsMono) (`fonts/ttf`).

These are the static (one weight per file) fonts, not the variable ones: egui only draws a variable font's default weight.

The OFL lets the fonts be bundled with Apache-2.0 software. Keep each licence file next to its font, don't sell the fonts on their own, and don't call a changed copy "Montserrat" or "JetBrains Mono".

The icons come from the `egui-phosphor` crate (Phosphor, MIT), not from this folder.
