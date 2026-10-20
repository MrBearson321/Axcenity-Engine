# Fonts

The editor's fonts, built into it with `include_bytes!` (see `crates/editor/src/theme.rs`).

| File | Use | Licence |
|---|---|---|
| `Inter-Regular.ttf` | The editor's UI text | SIL Open Font License 1.1 (`Inter-OFL.txt`) |
| `Inter-Medium.ttf` | Tabs, buttons and labels that lead a group | SIL Open Font License 1.1 (`Inter-OFL.txt`) |
| `Inter-SemiBold.ttf` | Headings and titles | SIL Open Font License 1.1 (`Inter-OFL.txt`) |
| `Montserrat-Regular.ttf` | Game UI text (player and the editor's viewport) | SIL Open Font License 1.1 (`Montserrat-OFL.txt`) |
| `Montserrat-SemiBold.ttf` | Game UI headings | SIL Open Font License 1.1 (`Montserrat-OFL.txt`) |
| `JetBrainsMono-Regular.ttf` | Code, and the Output panel | SIL Open Font License 1.1 (`JetBrainsMono-OFL.txt`) |

Sources: [Inter](https://github.com/rsms/inter) 4.1 (`extras/ttf` in the release zip), [Montserrat](https://github.com/JulietaUla/Montserrat) (`fonts/ttf`) and [JetBrains Mono](https://github.com/JetBrains/JetBrainsMono) (`fonts/ttf`).

The Inter files are subset to drop its private-use characters (U+E000–U+F8FF), because Phosphor's icons use the same code points and Inter comes first in the editor's font list: with them, some icons drew as Inter's glyphs. Everything else is kept. To update Inter, run `pyftsubset Inter-<Weight>.ttf --unicodes="U+0000-DFFF,U+F900-10FFFF" --layout-features='*' --name-IDs='*' --name-languages='*' --glyph-names --notdef-outline --hinting` (fontTools) on each weight; a test (`icons_come_from_phosphor_not_the_text_font`) catches a missed step.

These are the static (one weight per file) fonts, not the variable ones: egui only draws a variable font's default weight.

The OFL lets the fonts be bundled with Apache-2.0 software. Keep each licence file next to its font, don't sell the fonts on their own, and don't call a changed copy "Montserrat" or "JetBrains Mono" (Inter's licence reserves no font name).

The icons come from the `egui-phosphor` crate (Phosphor, MIT), not from this folder.
