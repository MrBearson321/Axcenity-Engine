//! Shaping UI text: the face for each run's weight and style (a run is a
//! rich text span, or the whole text), the size `TextScaled` text fits at,
//! and what's drawn for it besides its letters: copies for shadows, outlines
//! and faux bold, and lines for underline and line-through.

use std::borrow::Cow;

use glyphon::cosmic_text::{Align, CacheKeyFlags, Style, Weight, Wrap, fontdb};
use glyphon::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping};

use super::ui_pass::glyph_color;
use crate::draw_list::{TextAlign, TextSpan, TextStroke, UiRectangle, UiText, VerticalAlign};

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

/// UI text with the face each of its runs is drawn in. A run is one rich
/// text span, or the whole text when it has none.
pub(super) struct Styled<'a> {
    pub text: &'a UiText,
    pub runs: Cow<'a, [TextSpan]>,
    pub faces: Vec<FacePick>,
}

impl<'a> Styled<'a> {
    pub fn new(db: &fontdb::Database, text: &'a UiText) -> Self {
        let runs = if text.spans.is_empty() {
            Cow::Owned(vec![TextSpan {
                text: text.text.clone(),
                font_family: text.font_family.clone(),
                font_size: text.font_size,
                weight: text.weight,
                italic: text.italic,
                underline: text.underline,
                line_through: text.line_through,
                color: text.color,
                stroke: text.stroke,
            }])
        } else {
            Cow::Borrowed(text.spans.as_slice())
        };
        let faces = runs
            .iter()
            .map(|run| pick_face(db, &run.font_family, run.weight, run.italic))
            .collect();
        Self { text, runs, faces }
    }

    /// How much `font_size` (the size the text is drawn at, which
    /// `TextScaled` can change) scales the sizes it was given.
    fn ratio(&self, font_size: f32) -> f32 {
        font_size.max(1.0) / self.text.font_size.max(1.0)
    }
}

/// What each run's letters are painted in, in one shaped copy of the text:
/// its own colour, or `None` for the colour of the pass drawing the copy.
pub(super) type Paint = Vec<Option<[f32; 4]>>;

/// Hidden: a run left out of a copy, such as one without the outline the
/// copy draws.
const HIDDEN: Option<[f32; 4]> = Some([0.0; 4]);

/// The text shaped at `font_size`, its runs painted as `paint` says. Lines
/// wrap at the box's width (unless `nowrap`), and none are cut off at its
/// bottom: the text area's bounds clip them. Each glyph's metadata is the
/// index of its run.
pub(super) fn shape(
    font_system: &mut FontSystem,
    styled: &Styled,
    font_size: f32,
    paint: &[Option<[f32; 4]>],
) -> Buffer {
    let text = styled.text;
    let font_size = font_size.max(1.0);
    // A line height or letter spacing set in pixels shrinks with the text.
    let ratio = styled.ratio(font_size);
    let line_height = |size: f32| {
        text.line_height
            .filter(|height| height.is_finite() && *height > 0.0)
            .map_or(size * 1.2, |height| height * ratio)
    };
    let mut buffer = Buffer::new(font_system, Metrics::new(font_size, line_height(font_size)));
    let wrap = if !text.wrap {
        Wrap::None
    } else if text.scaled {
        // Fitting shrinks a long word instead of breaking it.
        Wrap::Word
    } else {
        Wrap::WordOrGlyph
    };
    buffer.set_wrap(wrap);
    buffer.set_size(Some(text.size[0].max(0.0)), None);
    let spacing = text.letter_spacing * ratio;
    let attrs: Vec<Attrs> = styled
        .runs
        .iter()
        .zip(&styled.faces)
        .enumerate()
        .map(|(index, (run, face))| {
            let size = (run.font_size * ratio).max(1.0);
            let mut attrs = Attrs::new()
                .family(Family::Name(&run.font_family))
                .weight(Weight(face.weight))
                .style(if face.italic {
                    Style::Italic
                } else {
                    Style::Normal
                })
                .metrics(Metrics::new(size, line_height(size)))
                .metadata(index);
            if face.fake_italic {
                attrs = attrs.cache_key_flags(CacheKeyFlags::FAKE_ITALIC);
            }
            if spacing.is_finite() && spacing != 0.0 {
                // The text system measures letter spacing in ems.
                attrs = attrs.letter_spacing(spacing / size);
            }
            if let Some(Some(color)) = paint.get(index) {
                attrs = attrs.color(glyph_color(*color));
            }
            attrs
        })
        .collect();
    let align = match text.align {
        TextAlign::Left => None,
        TextAlign::Center => Some(Align::Center),
        TextAlign::Right => Some(Align::Right),
    };
    let default = attrs.first().cloned().unwrap_or_else(Attrs::new);
    // Setters only record what to lay out; `shape_until_scroll` below does it.
    buffer.set_rich_text(
        styled
            .runs
            .iter()
            .zip(&attrs)
            .map(|(run, attrs)| (run.text.as_str(), attrs.clone())),
        &default,
        Shaping::Advanced,
        align,
    );
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
pub(super) fn fitted_size(font_system: &mut FontSystem, styled: &Styled) -> f32 {
    let text = styled.text;
    let mut fits = |size: f32| {
        let (width, height) = extent(&shape(font_system, styled, size, &[]));
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

/// One copy of the text's letters, `offset` pixels from where it's drawn,
/// shaped with the paint at index `layer`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Pass {
    pub offset: [f32; 2],
    pub color: [f32; 4],
    pub layer: usize,
}

/// Every copy of the text to draw, back to front, and the paints they're
/// shaped with: the text's shadows (the first one last, so it's on top),
/// each run's outline, then the letters in each run's colour. Faux bold
/// draws those again a little to the right.
pub(super) fn passes(styled: &Styled, font_size: f32) -> (Vec<Paint>, Vec<Pass>) {
    let text = styled.text;
    let runs = &styled.runs;
    // Layer 0 paints every run in the pass's colour.
    let mut layers: Vec<Paint> = vec![vec![None; runs.len()]];
    let mut copies = Vec::new();
    for shadow in text.shadows.iter().rev() {
        let [dx, dy] = shadow.offset;
        let blur = shadow.blur.max(0.0);
        if blur < 0.5 {
            copies.push(Pass {
                offset: [dx, dy],
                color: shadow.color,
                layer: 0,
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
            layer: 0,
        });
        for (x, y) in ring(8) {
            copies.push(Pass {
                offset: [dx + x * blur * 0.5, dy + y * blur * 0.5],
                color: [r, g, b, alpha],
                layer: 0,
            });
        }
    }
    // Each different outline, around the runs that have it.
    let ratio = styled.ratio(font_size);
    let mut strokes: Vec<TextStroke> = Vec::new();
    for stroke in runs.iter().filter_map(|run| run.stroke) {
        if stroke.width > 0.0 && !strokes.contains(&stroke) {
            strokes.push(stroke);
        }
    }
    for stroke in strokes {
        let paint: Paint = runs
            .iter()
            .map(|run| {
                if run.stroke == Some(stroke) {
                    None
                } else {
                    HIDDEN
                }
            })
            .collect();
        let layer = add_layer(&mut layers, paint);
        // A run's outline is part of its size, which `TextScaled` scales.
        let width = if text.spans.is_empty() {
            stroke.width
        } else {
            stroke.width * ratio
        };
        let around = if width <= 1.5 { 8 } else { 16 };
        for (x, y) in ring(around) {
            copies.push(Pass {
                offset: [x * width, y * width],
                color: stroke.color,
                layer,
            });
        }
        if width > 2.0 {
            for (x, y) in ring(around) {
                copies.push(Pass {
                    offset: [x * width * 0.5, y * width * 0.5],
                    color: stroke.color,
                    layer,
                });
            }
        }
    }
    let letters = add_layer(&mut layers, colours(runs, |_| true));
    copies.push(Pass {
        offset: [0.0, 0.0],
        color: text.color,
        layer: letters,
    });

    let fake_bold: Vec<bool> = styled.faces.iter().map(|face| face.fake_bold).collect();
    if !fake_bold.contains(&true) {
        return (layers, copies);
    }
    let largest = runs
        .iter()
        .zip(&fake_bold)
        .filter(|(_, bold)| **bold)
        .map(|(run, _)| run.font_size * ratio)
        .fold(0.0_f32, f32::max);
    let thicker = (largest / 24.0).max(0.5);
    if fake_bold.iter().all(|bold| *bold) {
        // Everything is thickened: each copy three times.
        let copies = copies
            .into_iter()
            .flat_map(|pass| {
                [0.0, 0.5, 1.0].map(|step| Pass {
                    offset: [pass.offset[0] + thicker * step, pass.offset[1]],
                    ..pass
                })
            })
            .collect();
        return (layers, copies);
    }
    // Only some runs are faux bold: their letters are drawn twice more.
    let bold = add_layer(&mut layers, colours(runs, |index| fake_bold[index]));
    for step in [0.5, 1.0] {
        copies.push(Pass {
            offset: [thicker * step, 0.0],
            color: text.color,
            layer: bold,
        });
    }
    (layers, copies)
}

/// Each run in its own colour where `shown`, else hidden. When every run is
/// shown in one colour, that's the colour of the pass (layer 0's paint).
fn colours(runs: &[TextSpan], shown: impl Fn(usize) -> bool) -> Paint {
    let first = runs.first().map(|run| run.color);
    if runs
        .iter()
        .enumerate()
        .all(|(index, run)| shown(index) && Some(run.color) == first)
    {
        return vec![None; runs.len()];
    }
    runs.iter()
        .enumerate()
        .map(|(index, run)| {
            if shown(index) {
                Some(run.color)
            } else {
                HIDDEN
            }
        })
        .collect()
}

/// The index of `paint` in `layers`, adding it if it's new.
fn add_layer(layers: &mut Vec<Paint>, paint: Paint) -> usize {
    if let Some(index) = layers.iter().position(|layer| *layer == paint) {
        return index;
    }
    layers.push(paint);
    layers.len() - 1
}

/// `count` points around a circle of radius 1.
fn ring(count: usize) -> impl Iterator<Item = (f32, f32)> {
    (0..count).map(move |i| {
        let angle = i as f32 / count as f32 * std::f32::consts::TAU;
        (angle.cos(), angle.sin())
    })
}

/// The underline and line-through under each run of each line of `buffer`
/// that has them, drawn at `[left, top]` on screen, in the run's colour.
pub(super) fn decorations(
    styled: &Styled,
    buffer: &Buffer,
    font_size: f32,
    [left, top]: [f32; 2],
) -> Vec<UiRectangle> {
    let runs = &styled.runs;
    if !runs.iter().any(|run| run.underline || run.line_through) {
        return Vec::new();
    }
    let ratio = styled.ratio(font_size);
    // Each letter's advance includes the spacing after it, which the line
    // shouldn't run on into past the last letter.
    let spacing = (styled.text.letter_spacing * ratio).max(0.0);
    let mut lines = Vec::new();
    for line in buffer.layout_runs() {
        // The glyphs in a row from each run, with where they start and end.
        let mut pieces: Vec<(usize, f32, f32)> = Vec::new();
        for glyph in line.glyphs {
            match pieces.last_mut() {
                Some((run, _, end)) if *run == glyph.metadata => {
                    *end = end.max(glyph.x + glyph.w);
                }
                _ => pieces.push((glyph.metadata, glyph.x, glyph.x + glyph.w)),
            }
        }
        let count = pieces.len();
        for (index, (run_index, start, end)) in pieces.into_iter().enumerate() {
            let Some(run) = runs.get(run_index) else {
                continue;
            };
            let end = if index + 1 == count {
                end - spacing
            } else {
                end
            };
            if !(start.is_finite() && end > start) {
                continue;
            }
            let size = run.font_size * ratio;
            let thickness = (size / 14.0).max(1.0);
            let mut add = |below_baseline: f32| {
                lines.push(UiRectangle {
                    position: [
                        left + start,
                        top + line.line_y + below_baseline - thickness * 0.5,
                    ],
                    size: [end - start, thickness],
                    color: run.color,
                    color_end: run.color,
                    border_color: [0.0; 4],
                    border_width: 0.0,
                    border_radius: 0.0,
                    ..Default::default()
                });
            };
            if run.underline {
                add(size * 0.12);
            }
            if run.line_through {
                add(-size * 0.3);
            }
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw_list::TextShadow;

    fn font_system() -> FontSystem {
        FontSystem::new_with_locale_and_db("en-US".to_owned(), crate::fonts::built_in_database())
    }

    /// The PostScript name of the face the first letter of `text` is drawn in.
    fn face_drawn(font_system: &mut FontSystem, text: &UiText) -> (String, FacePick) {
        let styled = Styled::new(font_system.db(), text);
        let face = styled.faces[0];
        let buffer = shape(font_system, &styled, text.font_size, &[]);
        let id = buffer.layout_runs().next().unwrap().glyphs[0].font_id;
        (
            font_system.db().face(id).unwrap().post_script_name.clone(),
            face,
        )
    }

    /// `text` shaped as it's drawn, at its own size.
    fn shaped(font_system: &mut FontSystem, text: &UiText, size: f32) -> Buffer {
        let styled = Styled::new(font_system.db(), text);
        shape(font_system, &styled, size, &[])
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
        let copies = passes(&Styled::new(fonts.db(), &text), 16.0).1;
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
        let long = UiText {
            size: [200.0, 40.0],
            font_size: 48.0,
            scaled: true,
            ..label("A much longer string than fits in this box at that size")
        };
        let styled = Styled::new(fonts.db(), &long);
        let size = fitted_size(&mut fonts, &styled);
        assert!(size < 48.0 && size > 4.0, "{size}");
        let (width, height) = extent(&shaped(&mut fonts, &long, size));
        assert!(width <= 200.5 && height <= 40.5, "{width} × {height}");
        // A little bigger wouldn't fit.
        let (width, height) = extent(&shaped(&mut fonts, &long, size + 1.0));
        assert!(width > 200.5 || height > 40.5);

        let short = UiText {
            size: [400.0, 200.0],
            font_size: 48.0,
            scaled: true,
            ..label("Hi")
        };
        let styled = Styled::new(fonts.db(), &short);
        assert_eq!(fitted_size(&mut fonts, &styled), 48.0);
    }

    #[test]
    fn spacing_line_height_nowrap_and_vertical_align_change_the_layout() {
        let mut fonts = font_system();
        let text = UiText {
            size: [120.0, 200.0],
            ..label("one two three four five six")
        };
        let plain = shaped(&mut fonts, &text, 16.0);
        let (plain_width, plain_height) = extent(&plain);
        assert!(plain.layout_runs().count() > 1);

        let one_line = shaped(
            &mut fonts,
            &UiText {
                wrap: false,
                ..text.clone()
            },
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
        let spaced_width = extent(&shaped(&mut fonts, &spaced, 16.0)).0;
        let unspaced = extent(&one_line).0;
        assert!(
            (spaced_width - unspaced - 4.0 * letters).abs() <= 6.0,
            "{spaced_width} vs {unspaced} + 4 × {letters}"
        );

        let tall = UiText {
            line_height: Some(40.0),
            ..text.clone()
        };
        let tall = shaped(&mut fonts, &tall, 16.0);
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
        let copies = passes(&Styled::new(&fontdb::Database::new(), &text), 16.0).1;
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
                layer: 0,
            }
        );
    }

    #[test]
    fn underline_and_line_through_run_under_and_through_each_line() {
        let mut fonts = font_system();
        let text = UiText {
            underline: true,
            line_through: true,
            ..label("Under")
        };
        let buffer = shaped(&mut fonts, &text, 16.0);
        let lines = decorations(&Styled::new(fonts.db(), &text), &buffer, 16.0, [10.0, 20.0]);
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
        assert!(
            decorations(
                &Styled::new(fonts.db(), &label("x")),
                &buffer,
                16.0,
                [0.0, 0.0]
            )
            .is_empty()
        );

        // With letter spacing, the line stops at the last letter.
        let spaced = UiText {
            letter_spacing: 6.0,
            ..text.clone()
        };
        let spaced_buffer = shaped(&mut fonts, &spaced, 16.0);
        let line = decorations(
            &Styled::new(fonts.db(), &spaced),
            &spaced_buffer,
            16.0,
            [0.0, 0.0],
        )[0];
        assert!((line.size[0] - (extent(&spaced_buffer).0 - 6.0)).abs() < 1.0);
    }

    /// A rich text span in the text's own style, saying `text`.
    fn span(text: &str) -> TextSpan {
        let base = UiText::default();
        TextSpan {
            text: text.to_owned(),
            font_family: base.font_family,
            font_size: base.font_size,
            weight: base.weight,
            italic: false,
            underline: false,
            line_through: false,
            color: base.color,
            stroke: None,
        }
    }

    /// `Press <b>E</b> to <font color="#ff0000">open</font>`, as `ui` hands
    /// it over.
    fn press_e_to_open() -> UiText {
        let spans = vec![
            span("Press "),
            TextSpan {
                weight: 700,
                ..span("E")
            },
            span(" to "),
            TextSpan {
                color: [1.0, 0.0, 0.0, 1.0],
                ..span("open")
            },
        ];
        UiText {
            text: spans.iter().map(|span| span.text.as_str()).collect(),
            spans,
            ..label("")
        }
    }

    #[test]
    fn rich_text_draws_one_bold_word_and_one_red_word_on_one_line() {
        let mut fonts = font_system();
        let text = press_e_to_open();
        let styled = Styled::new(fonts.db(), &text);
        let (layers, copies) = passes(&styled, 16.0);
        // Montserrat has a real bold, so nothing is thickened: one copy, of
        // the letters in their own colours.
        assert_eq!(copies.len(), 1);
        let letters = &layers[copies[0].layer];
        assert_eq!(letters[3], Some([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(letters[0], Some(text.color));

        let buffer = shape(&mut fonts, &styled, 16.0, letters);
        assert_eq!(buffer.layout_runs().count(), 1, "one line");
        let glyphs = buffer.layout_runs().next().unwrap().glyphs.to_vec();
        let face = |glyph: &glyphon::LayoutGlyph| {
            fonts
                .db()
                .face(glyph.font_id)
                .unwrap()
                .post_script_name
                .clone()
        };
        let run_of = |run: usize| glyphs.iter().filter(move |glyph| glyph.metadata == run);
        assert!(run_of(1).all(|glyph| face(glyph) == "Montserrat-Bold"));
        assert!(run_of(0).all(|glyph| face(glyph) == "Montserrat-Regular"));
        let red = glyphon::Color::rgba(255, 0, 0, 255);
        assert_eq!(run_of(3).count(), 4);
        assert!(run_of(3).all(|glyph| glyph.color_opt == Some(red)));
        assert!(run_of(2).all(|glyph| glyph.color_opt != Some(red)));

        // Laid out as the same words would be without the tags.
        let plain = shaped(&mut fonts, &label("Press E to open"), 16.0);
        let width = |buffer: &Buffer| extent(buffer).0;
        assert!((width(&buffer) - width(&plain)).abs() < 3.0);
    }

    #[test]
    fn rich_runs_keep_their_own_size_outline_underline_and_faux_bold() {
        let mut fonts = font_system();
        let spans = vec![
            TextSpan {
                font_size: 32.0,
                underline: true,
                ..span("Big")
            },
            TextSpan {
                font_family: "Bangers".to_owned(),
                weight: 700,
                stroke: Some(TextStroke {
                    width: 1.0,
                    color: [0.0, 0.0, 0.0, 1.0],
                }),
                ..span(" pow\nnext")
            },
        ];
        let text = UiText {
            text: "Big pow\nnext".to_owned(),
            spans,
            ..label("")
        };
        let styled = Styled::new(fonts.db(), &text);
        assert!(!styled.faces[0].fake_bold && styled.faces[1].fake_bold);
        let (layers, copies) = passes(&styled, 16.0);
        // The outline is drawn around the second run only, and its letters
        // are thickened, the first run's not.
        let outline = copies
            .iter()
            .find(|pass| pass.color == [0.0, 0.0, 0.0, 1.0])
            .unwrap();
        assert_eq!(layers[outline.layer], vec![HIDDEN, None]);
        let bold = copies.last().unwrap();
        assert!(bold.offset[0] > 0.0);
        assert_eq!(layers[bold.layer][0], HIDDEN);
        assert_eq!(layers[bold.layer][1], Some(text.color));

        let buffer = shape(&mut fonts, &styled, 16.0, &[]);
        assert_eq!(buffer.layout_runs().count(), 2, "a new line in a run");
        let first = buffer.layout_runs().next().unwrap();
        assert!(
            first.line_height >= 32.0 * 1.2,
            "the big run makes its line taller"
        );
        let big = first
            .glyphs
            .iter()
            .find(|glyph| glyph.metadata == 0)
            .unwrap();
        assert_eq!(big.font_size, 32.0);
        // Only "Big" is underlined, in a line as thick as its size asks.
        let lines = decorations(&styled, &buffer, 16.0, [0.0, 0.0]);
        assert_eq!(lines.len(), 1);
        let big_width = first
            .glyphs
            .iter()
            .filter(|glyph| glyph.metadata == 0)
            .map(|glyph| glyph.w)
            .sum::<f32>();
        assert!((lines[0].size[0] - big_width).abs() < 1.0);
        assert!(lines[0].size[1] > 2.0);

        // TextScaled shrinks every run together.
        let scaled = UiText {
            size: [60.0, 30.0],
            scaled: true,
            ..text.clone()
        };
        let styled = Styled::new(fonts.db(), &scaled);
        let size = fitted_size(&mut fonts, &styled);
        assert!(size < 16.0, "{size}");
        let (width, height) = extent(&shape(&mut fonts, &styled, size, &[]));
        assert!(width <= 60.5 && height <= 30.5, "{width} × {height}");
    }
}
