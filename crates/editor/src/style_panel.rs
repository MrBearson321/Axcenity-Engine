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
            | ClassName::TextBox
            | ClassName::PageLayout
            | ClassName::Table
            | ClassName::ScrollingFrame
            | ClassName::BillboardGui
    )
}

/// The properties that take colours, which get a colour swatch.
fn is_color(property: &str) -> bool {
    matches!(
        property,
        "background-color"
            | "color"
            | "border-color"
            | "border-top-color"
            | "border-right-color"
            | "border-bottom-color"
            | "border-left-color"
    )
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
    let palette = theme::palette(ui.ctx());
    ui.add_space(4.0);
    crate::widgets::divider(ui);
    let header = egui::RichText::new(theme::with_icon(icons::PAINT_BRUSH, "Style (CSS)"))
        .family(theme::medium_family())
        .color(palette.text_secondary);
    egui::CollapsingHeader::new(header)
        .id_salt("style section")
        .default_open(true)
        .icon(crate::widgets::chevron)
        .show_unindented(ui, |ui| {
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
                ui.horizontal_wrapped(|ui| {
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
                crate::responsive::form(ui, ("style rule", path, selector), |ui| {
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
        if let Some(picked) =
            accept_tab(ui, &response, value_choices(&declaration.property), &value)
        {
            made = Some(clean_value(&picked));
        } else if response.changed() {
            made = Some(clean_value(&value));
        }
        let open = sticky_focus(
            ui,
            egui::Id::new(("style value", &declaration.path, declaration.span.start)),
            response.has_focus(),
        );
        if made.is_none()
            && open
            && let Some(picked) = choice_row(ui, value_choices(&declaration.property), &value, None)
        {
            made = Some(clean_value(&picked));
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

/// The row for adding a style to the element's own rule. The property and
/// value boxes offer the supported names and values while they are focused.
fn add_row(ui: &mut egui::Ui, id: InstanceId, applied: &[CssDeclaration]) -> Option<StyleEdit> {
    let memory = egui::Id::new(("adding a style", id));
    let mut adding: Adding = ui
        .data_mut(|data| data.get_temp(memory))
        .unwrap_or_default();
    let mut made = None;
    ui.label(egui::RichText::new("Add a style").strong());
    let mut property_focused = false;
    let mut value_focused = false;
    ui.horizontal_wrapped(|ui| {
        let property = ui.add(
            egui::TextEdit::singleline(&mut adding.property)
                .hint_text("property")
                .font(egui::TextStyle::Monospace)
                .desired_width(130.0),
        );
        property_focused = property.has_focus();
        if let Some(picked) = accept_tab(
            ui,
            &property,
            axcenity_ui::SUPPORTED_PROPERTIES,
            &adding.property,
        ) {
            adding.property = picked;
        }
        let hint = example_value(&adding.property);
        let typed = ui.add(
            egui::TextEdit::singleline(&mut adding.value)
                .hint_text(hint)
                .font(egui::TextStyle::Monospace)
                .desired_width(110.0),
        );
        value_focused = typed.has_focus();
        if let Some(picked) = accept_tab(ui, &typed, value_choices(&adding.property), &adding.value)
        {
            adding.value = picked;
        }
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
    if sticky_focus(ui, memory.with("property"), property_focused) {
        let set: Vec<&str> = applied
            .iter()
            .map(|declaration| declaration.property.as_str())
            .collect();
        if let Some(picked) = choice_row(
            ui,
            axcenity_ui::SUPPORTED_PROPERTIES,
            &adding.property,
            Some(&set),
        ) {
            adding.property = picked;
        }
    } else if sticky_focus(ui, memory.with("value"), value_focused)
        && let Some(picked) = choice_row(ui, value_choices(&adding.property), &adding.value, None)
    {
        adding.value = picked;
    }
    ui.data_mut(|data| data.insert_temp(memory, adding));
    made
}

/// True while `focused`, and also while the mouse is still down after it
/// was, so a click on a suggestion lands on a button that is still there.
fn sticky_focus(ui: &mut egui::Ui, key: egui::Id, focused: bool) -> bool {
    let held = ui.input(|input| input.pointer.primary_down());
    let was = ui.data(|data| data.get_temp::<bool>(key).unwrap_or(false));
    let open = focused || (was && held);
    ui.data_mut(|data| {
        if open {
            data.insert_temp(key, true);
        } else {
            data.remove::<bool>(key);
        }
    });
    open
}

/// Fills `typed` with the first match when Tab is pressed in `response`.
fn accept_tab(
    ui: &egui::Ui,
    response: &egui::Response,
    options: &[&str],
    typed: &str,
) -> Option<String> {
    let matches = matching(options, typed);
    if response.has_focus()
        && ui.input(|input| input.key_pressed(egui::Key::Tab))
        && let Some(first) = matches.first()
        && !first.eq_ignore_ascii_case(typed)
    {
        Some((*first).to_owned())
    } else {
        None
    }
}

/// Buttons for the options that match `typed`. `used` options get a check,
/// meaning a declaration already sets them. `None` when nothing is offered.
fn choice_row(
    ui: &mut egui::Ui,
    options: &[&str],
    typed: &str,
    used: Option<&[&str]>,
) -> Option<String> {
    let matches = matching(options, typed);
    if matches.is_empty() || (matches.len() == 1 && matches[0].eq_ignore_ascii_case(typed.trim())) {
        return None;
    }
    let mut picked = None;
    ui.horizontal_wrapped(|ui| {
        for option in matches {
            let label = match used {
                Some(used) if used.contains(&option) => format!("{option} ✓"),
                _ => option.to_owned(),
            };
            if ui
                .small_button(egui::RichText::new(label).monospace())
                .clicked()
            {
                picked = Some(option.to_owned());
            }
        }
    });
    picked
}

/// Options that start with `typed`, or contain it when none start with it.
fn matching<'a>(options: &'a [&'a str], typed: &str) -> Vec<&'a str> {
    let typed = typed.trim().to_ascii_lowercase();
    if typed.is_empty() {
        return options.iter().copied().take(8).collect();
    }
    let mut found: Vec<&str> = options
        .iter()
        .copied()
        .filter(|option| option.to_ascii_lowercase().starts_with(&typed))
        .collect();
    if found.is_empty() {
        found = options
            .iter()
            .copied()
            .filter(|option| option.to_ascii_lowercase().contains(&typed))
            .collect();
    }
    found.truncate(8);
    found
}

/// Values the engine understands for `property`, offered while typing.
fn value_choices(property: &str) -> &'static [&'static str] {
    match property {
        "display" => &["flex", "none"],
        "flex-direction" => &["row", "column", "row-reverse", "column-reverse"],
        "justify-content" => &[
            "flex-start",
            "center",
            "flex-end",
            "space-between",
            "space-around",
            "space-evenly",
        ],
        "align-items" => &["flex-start", "center", "flex-end", "stretch"],
        "text-align" => &["left", "center", "right"],
        "font-weight" => &["normal", "bold", "300", "500", "600", "800"],
        "font-style" => &["normal", "italic"],
        "text-decoration" => &[
            "none",
            "underline",
            "line-through",
            "underline line-through",
        ],
        "text-transform" => &["none", "uppercase", "lowercase", "capitalize"],
        "-webkit-text-stroke" | "text-stroke" => &["1px #000000", "2px #000000"],
        "text-shadow" => &["2px 2px 0 #000000", "0 2px 4px rgba(0, 0, 0, 0.5)", "none"],
        "letter-spacing" => &["normal", "1px", "0.1em"],
        "line-height" => &["normal", "1.2", "1.5", "24px"],
        "vertical-align" => &["top", "middle", "bottom"],
        "white-space" => &["normal", "nowrap"],
        "aspect-ratio" => &["1", "16 / 9", "4 / 3", "auto"],
        "border-style" => &["solid", "inset", "outset", "ridge", "groove", "none"],
        "box-shadow" => &[
            "0 2px 6px rgba(0, 0, 0, 0.4)",
            "inset 0 2px 4px rgba(0, 0, 0, 0.4)",
            "none",
        ],
        "border-image" => &["url(\"frame.png\") 16 fill", "none"],
        "font-family" => &[
            "Montserrat",
            "Inter",
            "Roboto",
            "Poppins",
            "Nunito",
            "Oswald",
            "\"Bebas Neue\"",
            "Bangers",
            "Lobster",
            "\"Press Start 2P\"",
            "Lora",
            "\"JetBrains Mono\"",
            "sans-serif",
            "serif",
            "monospace",
        ],
        "background-color" | "color" | "border-color" => {
            &["#ffffff", "#000000", "#3478f6", "transparent"]
        }
        "width" | "height" => &["100%", "50%", "100px", "auto"],
        "opacity" => &["1", "0.8", "0.5", "0"],
        "flex-grow" => &["0", "1"],
        "background-image" => &["linear-gradient(#3478f6, #142238)"],
        _ => &[],
    }
}

/// What a value of `property` looks like, as the value box's hint.
fn example_value(property: &str) -> &'static str {
    match property {
        "background-color" | "color" | "border-color" => "#3478f6",
        "width" | "height" => "50%",
        "border-radius" | "border-width" | "gap" | "font-size" => "8px",
        "padding" | "margin" => "8px 12px",
        "border" => "2px solid #ffffff",
        "border-style" => "inset",
        "box-shadow" => "inset 0 2px 4px #000000",
        "border-image" => "url(\"frame.png\") 16 fill",
        "opacity" => "0.8",
        "display" => "flex",
        "flex-direction" => "row",
        "justify-content" => "center",
        "align-items" => "center",
        "flex-grow" => "1",
        "font-family" => "Montserrat",
        "font-weight" => "bold",
        "font-style" => "italic",
        "text-decoration" => "underline",
        "text-transform" => "uppercase",
        "-webkit-text-stroke" | "text-stroke" => "2px #000000",
        "text-shadow" => "2px 2px 4px #000000",
        "letter-spacing" => "1px",
        "line-height" => "1.5",
        "vertical-align" => "middle",
        "white-space" => "nowrap",
        "aspect-ratio" => "16 / 9",
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
                    crate::properties::show(ui, &world, Some(frame), false, 1, |ui| {
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
