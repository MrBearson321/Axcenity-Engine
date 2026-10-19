//! The Style section of the Properties panel: the CSS that styles the
//! selected UI element, editable in place.
//!
//! Roblox styles UI with properties only. Here the element's properties
//! (Position, Size, colours) work the same, and its handles in the viewport
//! drag and resize it, but the CSS rules that apply to it are listed under
//! them too, like a browser's style inspector: change a value and the
//! stylesheet is rewritten and the viewport restyled at once, follow a rule
//! to its line in the stylesheet, or add a style. A style added goes in the
//! element's own `#Id` rule (the element gets an `Id` if it has none), so it
//! changes only this element. Stylesheets are saved with the rest (Ctrl+S).

use std::ops::Range;

use axcenity_core::{ClassName, InstanceId, World};
use axcenity_ui::{CssDeclaration, Stylesheet};
use eframe::egui;

use crate::theme::{self, icons};

/// Something done in the Style section.
#[derive(Clone, Debug, PartialEq)]
pub enum StyleEdit {
    /// A declaration's value, at `span` in the stylesheet at `path`, is now `value`.
    Set {
        path: String,
        span: Range<usize>,
        value: String,
    },
    /// Adds `property: value` to the element's own rule.
    Add { property: String, value: String },
    /// Opens the stylesheet at `path` on `line`.
    Open { path: String, line: usize },
}

/// True for the classes CSS styles.
pub fn is_styled(class: ClassName) -> bool {
    matches!(
        class,
        ClassName::ScreenGui
            | ClassName::Frame
            | ClassName::TextLabel
            | ClassName::TextButton
            | ClassName::ImageLabel
    )
}

/// The properties that take colours, which get a colour swatch.
fn is_color(property: &str) -> bool {
    matches!(property, "background-color" | "color" | "border-color")
}

/// What's typed into a value box, made safe to put in a stylesheet: a
/// `;`, brace or line break would end the declaration or the rule early.
pub fn clean_value(value: &str) -> String {
    value
        .chars()
        .filter(|c| !matches!(c, ';' | '{' | '}' | '\n' | '\r'))
        .collect()
}

/// A colour as CSS writes it: `#rrggbb`, or `rgba()` when it's see-through.
fn css_color(color: egui::Color32) -> String {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("rgba({r}, {g}, {b}, {:.2})", f32::from(a) / 255.0)
    }
}

/// The selector of `id`'s own rule, such as `#Score`, from its `Id`.
pub fn own_selector(id: &str) -> String {
    format!("#{id}")
}

/// An `Id` made from an element's name: letters, digits, `-` and `_`.
pub fn id_from_name(name: &str) -> String {
    let id: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    match id.chars().next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => id,
        _ => format!("ui-{id}"),
    }
}

/// `source` with `property: value;` added to the rule ending at `rule_end`
/// (the offset of its `}`), or, with no such rule, a new `selector` rule
/// at the end.
pub fn add_declaration(
    source: &str,
    rule_end: Option<usize>,
    selector: &str,
    property: &str,
    value: &str,
) -> String {
    match rule_end.filter(|&end| source.get(end..end + 1) == Some("}")) {
        Some(end) => {
            let before = source[..end].trim_end();
            // The declaration before it may not end with `;`.
            let separator = if before.ends_with(';') || before.ends_with('{') {
                ""
            } else {
                ";"
            };
            format!(
                "{before}{separator}\n  {property}: {value};\n{}",
                &source[end..]
            )
        }
        None => {
            let mut text = source.trim_end().to_owned();
            if !text.is_empty() {
                text.push_str("\n\n");
            }
            text.push_str(&format!("{selector} {{\n  {property}: {value};\n}}\n"));
            text
        }
    }
}

/// The Style section for `selected`, if it's a UI element. Returns what was
/// done in it this frame.
pub fn show(
    ui: &mut egui::Ui,
    world: &World,
    selected: Option<InstanceId>,
    stylesheets: &[Stylesheet],
) -> Option<StyleEdit> {
    let id = selected?;
    let instance = world.get(id)?;
    if !is_styled(instance.class()) {
        return None;
    }
    let mut edit = None;
    let header = egui::RichText::new(theme::with_icon(icons::PAINT_BRUSH, "Style (CSS)"))
        .family(theme::heading_family());
    egui::CollapsingHeader::new(header)
        .id_salt("style section")
        .default_open(true)
        .show(ui, |ui| {
            let declarations = axcenity_ui::applied_declarations(world, id, stylesheets);
            if declarations.is_empty() {
                ui.weak(
                    "No CSS styles this element yet. Add a style below: it goes in a rule for \
                     this element only. Drag its handles in the viewport to move and size it.",
                );
            }
            // Grouped by rule, in the order the rules first appear.
            let mut rules: Vec<(&str, &str, usize)> = Vec::new();
            for declaration in &declarations {
                let rule = (
                    declaration.path.as_str(),
                    declaration.selector.as_str(),
                    declaration.line,
                );
                if !rules
                    .iter()
                    .any(|(path, selector, _)| *path == rule.0 && *selector == rule.1)
                {
                    rules.push(rule);
                }
            }
            for (path, selector, line) in rules {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(selector).monospace().strong());
                    let file = path.rsplit('/').next().unwrap_or(path);
                    if ui
                        .link(format!("{file}:{line}"))
                        .on_hover_text("Open the stylesheet at this rule")
                        .clicked()
                    {
                        edit = Some(StyleEdit::Open {
                            path: path.to_owned(),
                            line,
                        });
                    }
                });
                egui::Grid::new(("style rule", path, selector))
                    .num_columns(2)
                    .striped(true)
                    .spacing([12.0, 4.0])
                    .show(ui, |ui| {
                        for declaration in declarations
                            .iter()
                            .filter(|d| d.path == path && d.selector == selector)
                        {
                            if let Some(made) = declaration_row(ui, declaration) {
                                edit = Some(made);
                            }
                            ui.end_row();
                        }
                    });
                ui.add_space(4.0);
            }
            let sized_by_css: Vec<&str> = ["width", "height"]
                .into_iter()
                .filter(|property| declarations.iter().any(|d| d.property == *property))
                .collect();
            if !sized_by_css.is_empty() {
                ui.weak(theme::with_icon(
                    icons::INFO,
                    &format!(
                        "CSS sets the {}, so Size doesn't. Dragging a handle in the viewport \
                         rewrites the CSS value instead.",
                        sized_by_css.join(" and ")
                    ),
                ));
            }
            ui.separator();
            if let Some(made) = add_row(ui, id, &declarations) {
                edit = Some(made);
            }
        });
    edit
}

/// One declaration: its property, and its value to change.
fn declaration_row(ui: &mut egui::Ui, declaration: &CssDeclaration) -> Option<StyleEdit> {
    ui.label(egui::RichText::new(&declaration.property).monospace());
    let mut made = None;
    ui.horizontal(|ui| {
        if is_color(&declaration.property)
            && let Some((color, alpha)) = axcenity_ui::parse_css_color(&declaration.value)
        {
            let byte = |channel: f32| (channel.clamp(0.0, 1.0) * 255.0).round() as u8;
            let mut picked = egui::Color32::from_rgba_unmultiplied(
                byte(color.r),
                byte(color.g),
                byte(color.b),
                byte(alpha),
            );
            if ui.color_edit_button_srgba(&mut picked).changed() {
                made = Some(css_color(picked));
            }
        }
        let mut value = declaration.value.clone();
        let response = ui.add(
            egui::TextEdit::singleline(&mut value)
                .id_salt((
                    &declaration.path,
                    declaration.span.start,
                    &declaration.property,
                ))
                .font(egui::TextStyle::Monospace)
                .desired_width(f32::INFINITY),
        );
        if response.changed() {
            made = Some(clean_value(&value));
        }
    });
    made.map(|value| StyleEdit::Set {
        path: declaration.path.clone(),
        span: declaration.span.clone(),
        value,
    })
}

/// The property and value being typed into the "Add a style" row, kept
/// between frames.
#[derive(Clone, Default)]
struct Adding {
    property: String,
    value: String,
}

/// The row for adding a style to the element's own rule.
fn add_row(ui: &mut egui::Ui, id: InstanceId, applied: &[CssDeclaration]) -> Option<StyleEdit> {
    let memory = egui::Id::new(("adding a style", id));
    let mut adding: Adding = ui
        .data_mut(|data| data.get_temp(memory))
        .unwrap_or_default();
    let mut made = None;
    ui.label(egui::RichText::new("Add a style").strong());
    ui.horizontal(|ui| {
        let shown = if adding.property.is_empty() {
            "property"
        } else {
            adding.property.as_str()
        };
        egui::ComboBox::from_id_salt(("style property", id))
            .selected_text(egui::RichText::new(shown).monospace())
            .width(150.0)
            .height(320.0)
            .show_ui(ui, |ui| {
                for &property in axcenity_ui::SUPPORTED_PROPERTIES {
                    let set = applied.iter().any(|d| d.property == property);
                    let text = if set {
                        format!("{property} ✓")
                    } else {
                        property.to_owned()
                    };
                    ui.selectable_value(&mut adding.property, property.to_owned(), text);
                }
            });
        let hint = example_value(&adding.property);
        let typed = ui.add(
            egui::TextEdit::singleline(&mut adding.value)
                .hint_text(hint)
                .font(egui::TextStyle::Monospace)
                .desired_width(110.0),
        );
        let entered = typed.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let ready = !adding.property.is_empty() && !adding.value.trim().is_empty();
        if (ui
            .add_enabled(ready, egui::Button::new(icons::PLUS))
            .on_hover_text("Add it to this element's own rule")
            .clicked()
            || entered)
            && ready
        {
            made = Some(StyleEdit::Add {
                property: std::mem::take(&mut adding.property),
                value: clean_value(adding.value.trim()),
            });
            adding.value.clear();
        }
    });
    ui.data_mut(|data| data.insert_temp(memory, adding));
    made
}

/// What a value of `property` looks like, as the value box's hint.
fn example_value(property: &str) -> &'static str {
    match property {
        "background-color" | "color" | "border-color" => "#3478f6",
        "width" | "height" => "50%",
        "border-radius" | "border-width" | "gap" | "font-size" => "8px",
        "padding" | "margin" => "8px 12px",
        "border" => "2px solid #ffffff",
        "opacity" => "0.8",
        "display" => "flex",
        "flex-direction" => "row",
        "justify-content" => "center",
        "align-items" => "center",
        "flex-grow" => "1",
        "font-family" => "Montserrat",
        _ => "value",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn styles_are_added_to_the_elements_own_rule_or_a_new_one() {
        let source = "#Score {\n  color: red\n}\n";
        let (stylesheet, _) = axcenity_ui::parse_stylesheet("ui.css", source);
        let added = add_declaration(
            source,
            stylesheet.rule_end("#Score"),
            "#Score",
            "font-size",
            "24px",
        );
        assert_eq!(added, "#Score {\n  color: red;\n  font-size: 24px;\n}\n");
        let (reparsed, warnings) = axcenity_ui::parse_stylesheet("ui.css", &added);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(reparsed.rule_end("#Score").is_some());

        let fresh = add_declaration("", None, "#Health", "width", "50%");
        assert_eq!(fresh, "#Health {\n  width: 50%;\n}\n");
        let after = add_declaration(".hud { gap: 4px; }", None, "#Bar", "gap", "2px");
        assert_eq!(after, ".hud { gap: 4px; }\n\n#Bar {\n  gap: 2px;\n}\n");
    }

    #[test]
    fn the_style_section_draws_under_the_properties() {
        use axcenity_core::Value;
        let mut world = World::new();
        let gui = world.create(ClassName::ScreenGui).unwrap();
        world.set_parent(gui, Some(world.workspace())).unwrap();
        let frame = world.create(ClassName::Frame).unwrap();
        world.set_name(frame, "Panel").unwrap();
        world
            .set_property(frame, "Class", Value::String("hud".to_owned()))
            .unwrap();
        world.set_parent(frame, Some(gui)).unwrap();
        let css =
            ".hud {\n  width: 280px;\n  background-color: #142238;\n  border-radius: 8px;\n}\n";
        let (stylesheet, _) = axcenity_ui::parse_stylesheet("ui.css", css);
        let stylesheets = [stylesheet];
        let mut edits = Vec::new();
        let mut draw = |ctx: &egui::Context| {
            egui::SidePanel::right("properties")
                .default_width(320.0)
                .show(ctx, |ui| {
                    crate::properties::show(ui, &world, Some(frame), false, |ui| {
                        edits.extend(show(ui, &world, Some(frame), &stylesheets));
                    });
                });
        };
        match std::env::var_os("AXCENITY_SNAPSHOTS") {
            Some(folder) => crate::snapshot::snapshot(
                &std::path::PathBuf::from(folder).join("style-section.png"),
                egui::vec2(340.0, 900.0),
                vec![Vec::new(); 3],
                &mut draw,
            ),
            None => {
                let ctx = theme::test_context();
                for _ in 0..3 {
                    let _ = ctx.run(Default::default(), &mut draw);
                }
            }
        }
        assert!(edits.is_empty(), "drawing alone edits nothing: {edits:?}");
    }

    #[test]
    fn ids_and_values_are_made_safe_for_css() {
        assert_eq!(id_from_name("Health Bar"), "Health-Bar");
        assert_eq!(id_from_name("2nd"), "ui-2nd");
        assert_eq!(clean_value("red; } .x {"), "red  .x ");
        assert_eq!(css_color(egui::Color32::from_rgb(52, 120, 246)), "#3478f6");
        assert_eq!(
            css_color(egui::Color32::from_rgba_unmultiplied(0, 0, 0, 128)),
            "rgba(0, 0, 0, 0.50)"
        );
    }
}
