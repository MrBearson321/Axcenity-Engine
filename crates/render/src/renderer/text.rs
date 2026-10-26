//! Shaping UI text: the face for its weight and style, the size `TextScaled`
//! text fits at, and what's drawn for it besides its letters: copies for
//! shadows, the outline and faux bold, and lines for underline and
//! line-through.

use glyphon::cosmic_text::{Align, CacheKeyFlags, Style, Weight, Wrap, fontdb};
use glyphon::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping};

use crate::draw_list::{TextAlign, UiRectangle, UiText, VerticalAlign};

/// The face a family draws a weight and style with, and what has to be
/// made up because the family hasn't got it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FacePick {
    /// The weight of the face to use.
    pub weight: u16,
    /// True to use an italic face.
    pub italic: bool,
    /// Italic was asked for and the family has no italic face: the upright
    /// one is slanted.
    pub fake_italic: bool,
    /// Bold was asked for and the family's nearest face isn't bold: it's
    /// thickened.
    pub fake_bold: bool,
}

/// Weights from here up are bold, as in CSS.
const BOLD: u16 = 600;

/// Picks `family`'s face for `weight` and `italic`, as CSS font matching
/// does: its italic faces when it has any, then the nearest weight. A
/// family the database doesn't have is asked for as it is, and the text
/// system falls back to another font.
pub(crate) fn pick_face(
    db: &fontdb::Database,
    family: &str,
    weight: u16,
    italic: bool,
) -> FacePick {
    let faces: Vec<&fontdb::FaceInfo> = db
        .faces()
        .filter(|face| {
            face.families
                .iter()
                .any(|(name, _)| name.eq_ignore_ascii_case(family))
        })
        .collect();
    if faces.is_empty() {
        return FacePick {
            weight,
            italic,
            fake_italic: false,
            fake_bold: false,
        };
    }
    let slanted = |face: &&fontdb::FaceInfo| face.style != fontdb::Style::Normal;
    let has_italic = faces.iter().any(slanted);
    let use_italic = italic && has_italic;
    let nearest = faces
        .iter()
        .filter(|face| slanted(face) == use_italic)
        .map(|face| face.weight.0)
        .min_by_key(|face_weight| (face_weight.abs_diff(weight), *face_weight))
        .unwrap_or(weight);
    FacePick {
        weight: nearest,
        italic: use_italic,
        fake_italic: italic && !has_italic,
        fake_bold: weight >= BOLD && nearest < BOLD,
    }
}

/// `text` shaped at `font_size` in `face`. Lines wrap at the box's width
/// (unless `nowrap`), and none are cut off at its bottom: the text area's
/// bounds clip them.
pub(super) fn shape(
    font_system: &mut FontSystem,
    text: &UiText,
    face: FacePick,
    font_size: f32,
) -> Buffer {
    let font_size = font_size.max(1.0);
    // A line height or letter spacing set in pixels shrinks with the text.
    let ratio = font_size / text.font_size.max(1.0);
    let line_height = text
        .line_height
        .filter(|height| height.is_finite() && *height > 0.0)
        .map_or(font_size * 1.2, |height| height * ratio);
    let mut buffer = Buffer::new(font_system, Metrics::new(font_size, line_height));
    let wrap = if !text.wrap {
        Wrap::None
    } else if text.scaled {
        // Fitting shrinks a long word instead of breaking it.
        Wrap::Word
    } else {
        Wrap::WordOrGlyph
    };
    buffer.set_wrap(font_system, wrap);
    buffer.set_size(font_system, Some(text.size[0].max(0.0)), None);
    let mut attrs = Attrs::new()
        .family(Family::Name(&text.font_family))
        .weight(Weight(face.weight))
        .style(if face.italic {
            Style::Italic
        } else {
            Style::Normal
        });
    if face.fake_italic {
        attrs = attrs.cache_key_flags(CacheKeyFlags::FAKE_ITALIC);
    }
    let spacing = text.letter_spacing * ratio;
    if spacing.is_finite() && spacing != 0.0 {
        // The text system measures letter spacing in ems.
        attrs = attrs.letter_spacing(spacing / font_size);
    }
    buffer.set_text(font_system, &text.text, &attrs, Shaping::Advanced);
    let align = match text.align {
        TextAlign::Left => None,
        TextAlign::Center => Some(Align::Center),
        TextAlign::Right => Some(Align::Right),
    };
    for line in &mut buffer.lines {
        line.set_align(align);
    }
    buffer.shape_until_scroll(font_system, false);
    buffer
}

/// The widest line and the height of every line, in pixels.
pub(super) fn extent(buffer: &Buffer) -> (f32, f32) {
    buffer
        .layout_runs()
        .fold((0.0_f32, 0.0_f32), |(w, h), run| {
            (w.max(run.line_w), h + run.line_height)
        })
}

/// The size `TextScaled` text is drawn at: the largest up to its font size
/// whose lines fit its box. Half a pixel is allowed for rounding.
pub(super) fn fitted_size(font_system: &mut FontSystem, text: &UiText, face: FacePick) -> f32 {
    let mut fits = |size: f32| {
        let (width, height) = extent(&shape(font_system, text, face, size));
        width <= text.size[0] + 0.5 && height <= text.size[1] + 0.5
    };
    let largest = text.font_size.max(1.0);
    if fits(largest) {
        return largest;
    }
    let (mut low, mut high) = (1.0_f32, largest);
    while high - low > 0.25 {
        let middle = (low + high) * 0.5;
        if fits(middle) {
            low = middle;
        } else {
            high = middle;
        }
    }
    low
}

/// How far down the lines start in the box, for `vertical-align`. Text
/// taller than its box starts at the top.
pub(super) fn vertical_offset(text: &UiText, buffer: &Buffer) -> f32 {
    let room = text.size[1] - extent(buffer).1;
    match text.vertical_align {
        VerticalAlign::Top => 0.0,
        VerticalAlign::Middle => (room * 0.5).max(0.0),
        VerticalAlign::Bottom => room.max(0.0),
    }
}

/// An underline or line-through, drawn after the UI item at its index.
pub(super) type TextLine = (usize, UiRectangle);

/// One copy of the text's letters, `offset` pixels from where it's drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Pass {
    pub offset: [f32; 2],
    pub color: [f32; 4],
}

/// Every copy of `text` to draw, back to front: its shadows (the first one
/// last, so it's on top), its outline, then the letters. Faux bold draws
/// each of those again a little to the right.
pub(super) fn passes(text: &UiText, font_size: f32, fake_bold: bool) -> Vec<Pass> {
    let mut copies = Vec::new();
    for shadow in text.shadows.iter().rev() {
        let [dx, dy] = shadow.offset;
        let blur = shadow.blur.max(0.0);
        if blur < 0.5 {
            copies.push(Pass {
                offset: [dx, dy],
                color: shadow.color,
            });
            continue;
        }
        // A soft shadow: nine copies spread over the blur, each faint
        // enough that where they all overlap it's as dark as the colour.
        let alpha = 1.0 - (1.0 - shadow.color[3].clamp(0.0, 1.0)).powf(1.0 / 9.0);
        let [r, g, b, _] = shadow.color;
        copies.push(Pass {
            offset: [dx, dy],
            color: [r, g, b, alpha],
        });
        for (x, y) in ring(8) {
            copies.push(Pass {
                offset: [dx + x * blur * 0.5, dy + y * blur * 0.5],
                color: [r, g, b, alpha],
            });
        }
    }
    if let Some(stroke) = text.stroke.filter(|stroke| stroke.width > 0.0) {
        let width = stroke.width;
        let around = if width <= 1.5 { 8 } else { 16 };
        for (x, y) in ring(around) {
            copies.push(Pass {
                offset: [x * width, y * width],
                color: stroke.color,
            });
        }
        if width > 2.0 {
            for (x, y) in ring(around) {
                copies.push(Pass {
                    offset: [x * width * 0.5, y * width * 0.5],
                    color: stroke.color,
                });
            }
        }
    }
    copies.push(Pass {
        offset: [0.0, 0.0],
        color: text.color,
    });
    if !fake_bold {
        return copies;
    }
    let thicker = (font_size / 24.0).max(0.5);
    copies
        .into_iter()
        .flat_map(|pass| {
            [0.0, 0.5, 1.0].map(|step| Pass {
                offset: [pass.offset[0] + thicker * step, pass.offset[1]],
                color: pass.color,
            })
        })
        .collect()
}

/// `count` points around a circle of radius 1.
fn ring(count: usize) -> impl Iterator<Item = (f32, f32)> {
    (0..count).map(move |i| {
        let angle = i as f32 / count as f32 * std::f32::consts::TAU;
        (angle.cos(), angle.sin())
    })
}

/// The underline and line-through under each line of `buffer`, drawn at
/// `[left, top]` on screen, in the text's colour.
pub(super) fn decorations(
    text: &UiText,
    buffer: &Buffer,
    font_size: f32,
    [left, top]: [f32; 2],
) -> Vec<UiRectangle> {
    if !text.underline && !text.line_through {
        return Vec::new();
    }
    let thickness = (font_size / 14.0).max(1.0);
    // Each letter's advance includes the spacing after it, which the line
    // shouldn't run on into past the last letter.
    let spacing = (text.letter_spacing * font_size / text.font_size.max(1.0)).max(0.0);
    let mut lines = Vec::new();
    for run in buffer.layout_runs() {
        let start = run
            .glyphs
            .iter()
            .map(|glyph| glyph.x)
            .fold(f32::INFINITY, f32::min);
        let end = run
            .glyphs
            .iter()
            .map(|glyph| glyph.x + glyph.w)
            .fold(f32::NEG_INFINITY, f32::max)
            - spacing;
        if !(start.is_finite() && end > start) {
            continue;
        }
        let mut line = |below_baseline: f32| {
            lines.push(UiRectangle {
                position: [
                    left + start,
                    top + run.line_y + below_baseline - thickness * 0.5,
                ],
                size: [end - start, thickness],
                color: text.color,
                color_end: text.color,
                border_color: [0.0; 4],
                border_width: 0.0,
                border_radius: 0.0,
            });
        };
        if text.underline {
            line(font_size * 0.12);
        }
        if text.line_through {
            line(-font_size * 0.3);
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw_list::{TextShadow, TextStroke};

    fn font_system() -> FontSystem {
        FontSystem::new_with_locale_and_db("en-US".to_owned(), crate::fonts::built_in_database())
    }

    /// The PostScript name of the face the first letter of `text` is drawn in.
    fn face_drawn(font_system: &mut FontSystem, text: &UiText) -> (String, FacePick) {
        let face = pick_face(
            font_system.db(),
            &text.font_family,
            text.weight,
            text.italic,
        );
        let buffer = shape(font_system, text, face, text.font_size);
        let id = buffer.layout_runs().next().unwrap().glyphs[0].font_id;
        (
            font_system.db().face(id).unwrap().post_script_name.clone(),
            face,
        )
    }

    fn label(text: &str) -> UiText {
        UiText {
            text: text.to_owned(),
            size: [400.0, 100.0],
            ..UiText::default()
        }
    }

    #[test]
    fn bold_italic_montserrat_draws_with_its_real_bold_italic_face() {
        let mut fonts = font_system();
        let text = UiText {
            weight: 700,
            italic: true,
            underline: true,
            ..label("Play")
        };
        let (name, face) = face_drawn(&mut fonts, &text);
        assert_eq!(name, "Montserrat-BoldItalic");
        assert!(!face.fake_bold && !face.fake_italic, "{face:?}");

        let (name, _) = face_drawn(
            &mut fonts,
            &UiText {
                italic: false,
                ..text.clone()
            },
        );
        assert_eq!(name, "Montserrat-Bold");
        let (name, _) = face_drawn(&mut fonts, &label("Play"));
        assert_eq!(name, "Montserrat-Regular");
    }

    #[test]
    fn a_family_without_bold_or_italic_files_is_thickened_and_slanted() {
        let mut fonts = font_system();
        let text = UiText {
            font_family: "Bangers".to_owned(),
            weight: 700,
            italic: true,
            ..label("Pow")
        };
        let (name, face) = face_drawn(&mut fonts, &text);
        assert_eq!(name, "Bangers-Regular");
        assert!(face.fake_bold && face.fake_italic, "{face:?}");
        // Faux bold draws the letters three times, a little apart.
        let copies = passes(&text, 16.0, face.fake_bold);
        assert_eq!(copies.len(), 3);
        assert!(copies[2].offset[0] > copies[0].offset[0]);

        // A medium weight isn't bold, so it isn't thickened.
        let medium = pick_face(fonts.db(), "Bangers", 500, false);
        assert!(!medium.fake_bold);
    }

    #[test]
    fn a_project_font_with_its_own_bold_file_uses_that_file() {
        // Like a project's `MyFont-Regular.ttf` and `MyFont-Bold.ttf`: the
        // files join one family by the name inside them.
        let mut db = fontdb::Database::new();
        db.load_font_data(include_bytes!("../../../../assets/fonts/Poppins-Regular.ttf").to_vec());
        db.load_font_data(include_bytes!("../../../../assets/fonts/Poppins-Bold.ttf").to_vec());
        let mut fonts = FontSystem::new_with_locale_and_db("en-US".to_owned(), db);
        let text = UiText {
            font_family: "Poppins".to_owned(),
            weight: 700,
            ..label("Go")
        };
        let (name, face) = face_drawn(&mut fonts, &text);
        assert_eq!(name, "Poppins-Bold");
        assert!(!face.fake_bold);
        // With no italic file, italic is slanted.
        assert!(pick_face(fonts.db(), "poppins", 400, true).fake_italic);
    }

    #[test]
    fn text_scaled_shrinks_a_long_string_and_grows_a_short_one_to_its_size() {
        let mut fonts = font_system();
        let face = pick_face(fonts.db(), "Montserrat", 400, false);
        let long = UiText {
            size: [200.0, 40.0],
            font_size: 48.0,
            scaled: true,
            ..label("A much longer string than fits in this box at that size")
        };
        let size = fitted_size(&mut fonts, &long, face);
        assert!(size < 48.0 && size > 4.0, "{size}");
        let (width, height) = extent(&shape(&mut fonts, &long, face, size));
        assert!(width <= 200.5 && height <= 40.5, "{width} × {height}");
        // A little bigger wouldn't fit.
        let (width, height) = extent(&shape(&mut fonts, &long, face, size + 1.0));
        assert!(width > 200.5 || height > 40.5);

        let short = UiText {
            size: [400.0, 200.0],
            font_size: 48.0,
            scaled: true,
            ..label("Hi")
        };
        assert_eq!(fitted_size(&mut fonts, &short, face), 48.0);
    }

    #[test]
    fn spacing_line_height_nowrap_and_vertical_align_change_the_layout() {
        let mut fonts = font_system();
        let face = pick_face(fonts.db(), "Montserrat", 400, false);
        let text = UiText {
            size: [120.0, 200.0],
            ..label("one two three four five six")
        };
        let plain = shape(&mut fonts, &text, face, 16.0);
        let (plain_width, plain_height) = extent(&plain);
        assert!(plain.layout_runs().count() > 1);

        let one_line = shape(
            &mut fonts,
            &UiText {
                wrap: false,
                ..text.clone()
            },
            face,
            16.0,
        );
        assert_eq!(one_line.layout_runs().count(), 1);
        assert!(extent(&one_line).0 > 120.0);

        let spaced = UiText {
            letter_spacing: 4.0,
            wrap: false,
            ..text.clone()
        };
        let letters = text.text.chars().count() as f32;
        let spaced_width = extent(&shape(&mut fonts, &spaced, face, 16.0)).0;
        let unspaced = extent(&one_line).0;
        assert!(
            (spaced_width - unspaced - 4.0 * letters).abs() <= 6.0,
            "{spaced_width} vs {unspaced} + 4 × {letters}"
        );

        let tall = UiText {
            line_height: Some(40.0),
            ..text.clone()
        };
        let tall = shape(&mut fonts, &tall, face, 16.0);
        assert_eq!(extent(&tall).1, 40.0 * tall.layout_runs().count() as f32);
        assert!(plain_width <= 120.0);

        assert_eq!(vertical_offset(&text, &plain), 0.0);
        let middle = UiText {
            vertical_align: VerticalAlign::Middle,
            ..text.clone()
        };
        assert_eq!(
            vertical_offset(&middle, &plain),
            (200.0 - plain_height) * 0.5
        );
        let bottom = UiText {
            vertical_align: VerticalAlign::Bottom,
            ..text
        };
        assert_eq!(vertical_offset(&bottom, &plain), 200.0 - plain_height);
    }

    #[test]
    fn shadows_and_the_outline_are_drawn_behind_the_letters() {
        let text = UiText {
            color: [1.0, 1.0, 1.0, 1.0],
            stroke: Some(TextStroke {
                width: 1.0,
                color: [0.0, 0.0, 0.0, 1.0],
            }),
            shadows: vec![
                TextShadow {
                    offset: [2.0, 2.0],
                    blur: 0.0,
                    color: [1.0, 0.0, 0.0, 1.0],
                },
                TextShadow {
                    offset: [4.0, 4.0],
                    blur: 4.0,
                    color: [0.0, 0.0, 1.0, 0.5],
                },
            ],
            ..label("Hi")
        };
        let copies = passes(&text, 16.0, false);
        // The soft blue shadow (nine copies), the red one, eight outline
        // copies, then the letters.
        assert_eq!(copies.len(), 9 + 1 + 8 + 1);
        assert_eq!(copies[0].offset, [4.0, 4.0]);
        assert!(copies[0].color[3] < 0.5);
        assert_eq!(copies[9].offset, [2.0, 2.0]);
        assert_eq!(copies[10].color, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(copies[10].offset, [1.0, 0.0]);
        assert_eq!(
            *copies.last().unwrap(),
            Pass {
                offset: [0.0, 0.0],
                color: text.color,
            }
        );
    }

    #[test]
    fn underline_and_line_through_run_under_and_through_each_line() {
        let mut fonts = font_system();
        let face = pick_face(fonts.db(), "Montserrat", 400, false);
        let text = UiText {
            underline: true,
            line_through: true,
            ..label("Under")
        };
        let buffer = shape(&mut fonts, &text, face, 16.0);
        let lines = decorations(&text, &buffer, 16.0, [10.0, 20.0]);
        assert_eq!(lines.len(), 2);
        let baseline = 20.0 + buffer.layout_runs().next().unwrap().line_y;
        assert!(
            lines[0].position[1] > baseline - 1.0,
            "the underline is below"
        );
        assert!(
            lines[1].position[1] < baseline - 3.0,
            "the line-through is above"
        );
        assert!((lines[0].size[0] - extent(&buffer).0).abs() < 2.0);
        assert!(lines[0].position[0] >= 10.0);
        assert!(decorations(&label("x"), &buffer, 16.0, [0.0, 0.0]).is_empty());

        // With letter spacing, the line stops at the last letter.
        let spaced = UiText {
            letter_spacing: 6.0,
            ..text.clone()
        };
        let spaced_buffer = shape(&mut fonts, &spaced, face, 16.0);
        let line = decorations(&spaced, &spaced_buffer, 16.0, [0.0, 0.0])[0];
        assert!((line.size[0] - (extent(&spaced_buffer).0 - 6.0)).abs() < 1.0);
    }
}
