//! The editor kit's design system ([`axcenity_editor_kit::theme`]), plus
//! the icons of editor-only things (toolbar tools, things to insert). Every
//! `crate::theme` path in the editor goes through here.

pub use axcenity_editor_kit::theme::*;

use crate::gizmo::Tool;
use crate::tree::Insertable;

/// The icon of a tool in the toolbar.
pub fn tool_icon(tool: Tool) -> &'static str {
    match tool {
        Tool::Select => icons::CURSOR,
        Tool::Move => icons::ARROWS_OUT_CARDINAL,
        Tool::Scale => icons::ARROWS_OUT,
        Tool::Rotate => icons::ARROWS_CLOCKWISE,
        Tool::Transform => icons::BOUNDING_BOX,
        Tool::Paint => icons::PAINT_BUCKET,
        Tool::Material => icons::DROP,
        Tool::Anchor => icons::ANCHOR,
        Tool::Lighting => icons::LIGHTBULB,
        Tool::Emission => icons::SUN_DIM,
        Tool::Angular => icons::SPIRAL,
    }
}

/// The icon of something to insert: parts by their shape.
pub fn insert_icon(what: Insertable) -> &'static str {
    match what {
        Insertable::Part(shape) => part_icon(shape),
        Insertable::Vehicle => icons::CAR,
        Insertable::Character => icons::PERSON_ARMS_SPREAD,
        Insertable::HealthBar => icons::HEARTBEAT,
        _ => class_icon(what.class()),
    }
}

// These read the editor's own sources and graph colours, so they stay with
// the editor rather than the kit.
#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::CornerRadius;

    fn call_arguments<'a>(source: &'a str, function: &str) -> Vec<Vec<&'a str>> {
        let mut calls = Vec::new();
        for (start, _) in source.match_indices(function) {
            let open = start + function.len();
            let (mut depth, mut from, mut arguments) = (1, open, Vec::new());
            for (offset, c) in source[open..].char_indices() {
                let at = open + offset;
                match c {
                    '(' | '[' | '{' => depth += 1,
                    ')' | ']' | '}' => {
                        depth -= 1;
                        if depth == 0 {
                            arguments.push(source[from..at].trim());
                            break;
                        }
                    }
                    ',' if depth == 1 => {
                        arguments.push(source[from..at].trim());
                        from = at + 1;
                    }
                    _ => {}
                }
            }
            calls.push(arguments);
        }
        calls
    }

    #[test]
    fn shared_pieces_use_only_the_three_corner_radii() {
        let sources = [
            (
                "widgets.rs",
                include_str!("../../editor-kit/src/widgets.rs"),
            ),
            ("dialog.rs", include_str!("dialog.rs")),
            ("toast.rs", include_str!("toast.rs")),
            ("command_palette.rs", include_str!("command_palette.rs")),
            ("layout.rs", include_str!("layout.rs")),
            ("ribbon.rs", include_str!("ribbon.rs")),
        ];
        // Which argument is the radius, for each way of giving one.
        let calls = [
            (".corner_radius(", 0),
            ("CornerRadius::same(", 0),
            ("rect_filled(", 1),
            ("rect_stroke(", 1),
            ("paint_hover(", 4),
        ];
        let mut checked = 0;
        for (file, source) in sources {
            for (function, index) in calls {
                for arguments in call_arguments(source, function) {
                    let Some(argument) = arguments.get(index) else {
                        continue;
                    };
                    checked += 1;
                    // A number written in place of a token must at least be
                    // one of the tokens' values.
                    let number = argument
                        .trim_end_matches("_f32")
                        .trim_end_matches("_u8")
                        .parse::<f32>();
                    if let Ok(number) = number {
                        assert!(
                            radius::ALL.iter().any(|&r| f32::from(r) == number),
                            "{file}: {function}...{argument}) isn't a radius token"
                        );
                    }
                }
            }
        }
        assert!(checked > 20, "only {checked} radii found");
        for dark in [true, false] {
            let v = visuals(&Palette::new(dark, Accent::Blue));
            assert_eq!(
                v.widgets.inactive.corner_radius,
                CornerRadius::same(radius::CONTROL)
            );
            assert_eq!(v.window_corner_radius, CornerRadius::same(radius::NONE));
        }
    }

    #[test]
    fn overlay_colours_are_written_only_in_the_theme() {
        let sources = [
            ("viewport.rs", include_str!("viewport.rs")),
            (
                "viewport/move_overlay.rs",
                include_str!("viewport/move_overlay.rs"),
            ),
            (
                "viewport/view_cube.rs",
                include_str!("viewport/view_cube.rs"),
            ),
            ("gizmo.rs", include_str!("../../editor-tools/src/gizmo.rs")),
            ("splines.rs", include_str!("splines.rs")),
            ("constraints.rs", include_str!("constraints.rs")),
            ("ui_handles.rs", include_str!("ui_handles.rs")),
            (
                "ui_designer/artboard.rs",
                include_str!("ui_designer/artboard.rs"),
            ),
            (
                "ui_designer/rulers.rs",
                include_str!("ui_designer/rulers.rs"),
            ),
            ("model_studio/view.rs", include_str!("model_studio/view.rs")),
        ];
        let tokens = [
            overlay::SELECTED,
            overlay::HOVER,
            overlay::SNAP,
            overlay::GUIDE,
            overlay::PREVIEW,
            overlay::ATTACHMENT,
            overlay::HANDLE,
            overlay::WARNING,
            overlay::MEASURE,
            overlay::LABEL_BACK,
            overlay::AXIS_X,
            overlay::AXIS_Y,
            overlay::AXIS_Z,
        ];
        for (file, source) in sources {
            for color in tokens {
                let written = format!("from_rgb({}, {}, {})", color.r(), color.g(), color.b());
                assert!(
                    !source.contains(&written),
                    "{file} writes {written}: use theme::overlay"
                );
            }
        }
        // The ruler guides and the snap lines that land on them differ.
        assert!(contrast(overlay::GUIDE, overlay::SNAP) > 1.5);
    }

    #[test]
    fn graph_headers_meet_wcag_aa_for_every_category() {
        for dark in [true, false] {
            for accent in Accent::ALL {
                let c = Canvas::new(&Palette::new(dark, accent));
                let label = format!("{accent:?}, dark {dark}");
                // Every graph category, and any other colour a header
                // could be (block categories, state kinds).
                for name in [
                    "Events",
                    "Actions",
                    "Flow",
                    "Conditions",
                    "Data",
                    "Variables",
                    "Output",
                    "Inputs",
                    "Textures",
                    "Color",
                    "Noise",
                    "Other",
                ] {
                    let fill = c.header(crate::node_graph::category_color(name));
                    assert!(contrast(c.header_text, fill) >= 4.5, "{label} {name}");
                }
            }
        }
    }
}
