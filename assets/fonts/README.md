# Fonts

The editor's fonts, built into it with `include_bytes!` (see `crates/editor/src/theme.rs`), and the fonts game UI can use, built into the renderer (`crates/render/src/fonts.rs`). A project can also use its own `.ttf` and `.otf` files.

| File | Use | Licence |
|---|---|---|
| `Inter-Regular.ttf` | The editor's UI text | SIL Open Font License 1.1 (`Inter-OFL.txt`) |
| `Inter-Medium.ttf` | Tabs, buttons and labels that lead a group | SIL Open Font License 1.1 (`Inter-OFL.txt`) |
| `Inter-SemiBold.ttf` | Headings and titles | SIL Open Font License 1.1 (`Inter-OFL.txt`) |
| `Montserrat-Regular.ttf` | Game UI text (player and the editor's viewport) | SIL Open Font License 1.1 (`Montserrat-OFL.txt`) |
| `Montserrat-SemiBold.ttf` | Game UI headings | SIL Open Font License 1.1 (`Montserrat-OFL.txt`) |
| `JetBrainsMono-Regular.ttf` | Code, and the Output panel; game UI `monospace` | SIL Open Font License 1.1 (`JetBrainsMono-OFL.txt`) |
| `Roboto-Variable.ttf` | Game UI | SIL Open Font License 1.1 (`Roboto-OFL.txt`) |
| `Poppins-Regular.ttf` | Game UI | SIL Open Font License 1.1 (`Poppins-OFL.txt`) |
| `Nunito-Variable.ttf` | Game UI (rounded) | SIL Open Font License 1.1 (`Nunito-OFL.txt`) |
| `Oswald-Variable.ttf` | Game UI (condensed) | SIL Open Font License 1.1 (`Oswald-OFL.txt`) |
| `BebasNeue-Regular.ttf` | Game UI (display) | SIL Open Font License 1.1 (`BebasNeue-OFL.txt`) |
| `Bangers-Regular.ttf` | Game UI (comic) | SIL Open Font License 1.1 (`Bangers-OFL.txt`) |
| `Lobster-Regular.ttf` | Game UI (script) | SIL Open Font License 1.1 (`Lobster-OFL.txt`) |
| `PressStart2P-Regular.ttf` | Game UI (pixel) | SIL Open Font License 1.1 (`PressStart2P-OFL.txt`) |
| `Lora-Variable.ttf` | Game UI `serif` | SIL Open Font License 1.1 (`Lora-OFL.txt`) |
| `Montserrat-Bold.ttf`, `-Italic.ttf`, `-BoldItalic.ttf` | Game UI bold and italic | SIL Open Font License 1.1 (`Montserrat-OFL.txt`) |
| `Inter-Bold.ttf`, `-Italic.ttf`, `-BoldItalic.ttf` | Game UI bold and italic | SIL Open Font License 1.1 (`Inter-OFL.txt`) |
| `Poppins-Bold.ttf`, `-Italic.ttf`, `-BoldItalic.ttf` | Game UI bold and italic | SIL Open Font License 1.1 (`Poppins-OFL.txt`) |
| `JetBrainsMono-Bold.ttf`, `-Italic.ttf`, `-BoldItalic.ttf` | Game UI bold and italic | SIL Open Font License 1.1 (`JetBrainsMono-OFL.txt`) |
| `Roboto-Italic-Variable.ttf`, `Nunito-Italic-Variable.ttf`, `Lora-Italic-Variable.ttf` | Game UI italic | SIL Open Font License 1.1 (the family's `-OFL.txt`) |

The game UI fonts added for the UI Designer come from [Google Fonts](https://github.com/google/fonts) (`ofl/<name>/`). Game UI also uses `Inter-Regular.ttf` and `Montserrat-Regular.ttf`. The `-Variable` files are variable fonts; the renderer draws their default (regular) weight.

Sources: [Inter](https://github.com/rsms/inter) 4.1 (`extras/ttf` in the release zip), [Montserrat](https://github.com/JulietaUla/Montserrat) (`fonts/ttf`) and [JetBrains Mono](https://github.com/JetBrains/JetBrainsMono) (`fonts/ttf`). The game UI bold and italic files come from the same places, and Poppins's and the `-Italic-Variable` files from Google Fonts (`ofl/<name>/`, the variable italics renamed from `<Name>-Italic[wght].ttf`). The game UI's Inter files aren't subset: the editor doesn't load them.

The Inter files are subset to drop its private-use characters (U+E000–U+F8FF), because Phosphor's icons use the same code points and Inter comes first in the editor's font list: with them, some icons drew as Inter's glyphs. Everything else is kept. To update Inter, run `pyftsubset Inter-<Weight>.ttf --unicodes="U+0000-DFFF,U+F900-10FFFF" --layout-features='*' --name-IDs='*' --name-languages='*' --glyph-names --notdef-outline --hinting` (fontTools) on each weight; a test (`icons_come_from_phosphor_not_the_text_font`) catches a missed step.

These are the static (one weight per file) fonts, not the variable ones: egui only draws a variable font's default weight.

The OFL lets the fonts be bundled with Apache-2.0 software. Keep each licence file next to its font, don't sell the fonts on their own, and don't call a changed copy "Montserrat" or "JetBrains Mono" (Inter's licence reserves no font name).

The icons come from the `egui-phosphor` crate (Phosphor, MIT), not from this folder.
