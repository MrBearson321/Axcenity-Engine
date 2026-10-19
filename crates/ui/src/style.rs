use std::collections::HashMap;
use std::ops::Range;

use axcenity_core::{ClassName, InstanceId, Value, World};

/// The CSS properties UI elements understand. Others are warned about.
pub const SUPPORTED_PROPERTIES: &[&str] = &[
    "align-items",
    "background-color",
    "background-image",
    "border",
    "box-shadow",
    "border-color",
    "border-radius",
    "border-width",
    "color",
    "display",
    "flex-direction",
    "flex-grow",
    "font-family",
    "font-size",
    "gap",
    "height",
    "justify-content",
    "margin",
    "margin-bottom",
    "margin-left",
    "margin-right",
    "margin-top",
    "opacity",
    "padding",
    "padding-bottom",
    "padding-left",
    "padding-right",
    "padding-top",
    "text-align",
    "width",
];

type Specificity = (u32, u32, u32);

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct PseudoClasses {
    pub(crate) hovered: Option<InstanceId>,
    pub(crate) active: Option<InstanceId>,
    pub(crate) focused: Option<InstanceId>,
}

#[derive(Clone, Debug, Default)]
pub struct Stylesheet {
    /// The file it came from, relative to the project folder.
    path: String,
    rules: Vec<Rule>,
}

impl Stylesheet {
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Where the last rule with exactly `selector` (such as `#score`) ends:
    /// the byte offset of its closing `}`, to add a declaration before it.
    pub fn rule_end(&self, selector: &str) -> Option<usize> {
        self.rules
            .iter()
            .rev()
            .find(|rule| rule.selector_text == selector)
            .map(|rule| rule.block.end)
    }
}
/// A declaration that applies to a UI element, and where it's written, so
/// the editor can show it and rewrite it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CssDeclaration {
    pub property: String,
    /// As written, before `var()` is resolved.
    pub value: String,
    /// The stylesheet's path.
    pub path: String,
    /// The rule's selector list, such as `.hud, #panel`.
    pub selector: String,
    /// Where `value` is in the stylesheet's source, in bytes.
    pub span: Range<usize>,
    /// The line `value` is on, counting from 1.
    pub line: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StyleWarning {
    pub path: String,
    pub message: String,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ResolvedStyles {
    pub(crate) values: HashMap<String, String>,
    pub(crate) variables: HashMap<String, String>,
}

#[derive(Clone, Debug)]
struct Rule {
    selector_text: String,
    selectors: Vec<Selector>,
    declarations: Vec<Declaration>,
    /// Where the text between the braces is in the source, in bytes.
    block: Range<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Declaration {
    name: String,
    value: String,
    /// Where `value` is in the source, in bytes.
    span: Range<usize>,
    line: usize,
}

#[derive(Clone, Debug)]
struct Selector {
    parts: Vec<SimpleSelector>,
    specificity: Specificity,
}

#[derive(Clone, Debug, Default)]
struct SimpleSelector {
    class: Option<ClassName>,
    id: Option<String>,
    classes: Vec<String>,
    hovered: bool,
    active: bool,
}

/// Parses the supported stylesheet subset. Unsupported declarations are
/// warned about and skipped without discarding other declarations in a rule.
pub fn parse_stylesheet(path: &str, source: &str) -> (Stylesheet, Vec<StyleWarning>) {
    // Comments become spaces of the same length, so byte offsets into the
    // stripped text are offsets into `source` too.
    let source = strip_comments(source);
    let mut stylesheet = Stylesheet {
        path: path.to_owned(),
        rules: Vec::new(),
    };
    let mut warnings = Vec::new();
    let mut rest = source.as_str();

    while !rest.trim().is_empty() {
        let Some(open) = find_delimiter(rest, '{') else {
            warnings.push(warning(path, "ignored CSS text without a rule block"));
            break;
        };
        let selector_text = rest[..open].trim();
        let after_open = &rest[open + 1..];
        let Some(close) = find_delimiter(after_open, '}') else {
            warnings.push(warning(path, "ignored a rule with no closing `}`"));
            break;
        };
        rest = &after_open[close + 1..];

        let mut selectors = Vec::new();
        for part in split_top_level(selector_text, ',') {
            match parse_selector(part.trim()) {
                Some(selector) => selectors.push(selector),
                None => warnings.push(warning(
                    path,
                    format!("ignored unsupported selector `{}`", part.trim()),
                )),
            }
        }
        if selectors.is_empty() {
            continue;
        }

        let block = &after_open[..close];
        let mut declarations = parse_declarations(block, path, &mut warnings);
        let block_start = offset_in(&source, block);
        for declaration in &mut declarations {
            declaration.span.start += block_start;
            declaration.span.end += block_start;
            declaration.line = source[..declaration.span.start].matches('\n').count() + 1;
        }
        stylesheet.rules.push(Rule {
            selector_text: selector_text
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
            selectors,
            declarations,
            block: block_start..block_start + block.len(),
        });
    }

    (stylesheet, warnings)
}

/// Resolves the cascade for an Instance, including custom properties inherited
/// from its UI ancestors.
pub(crate) fn resolve_styles(
    world: &World,
    id: InstanceId,
    stylesheets: &[Stylesheet],
    pseudo: PseudoClasses,
) -> ResolvedStyles {
    let mut ancestors = Vec::new();
    let mut current = Some(id);
    while let Some(instance_id) = current {
        let Some(instance) = world.get(instance_id) else {
            break;
        };
        ancestors.push(instance_id);
        current = instance.parent();
    }
    ancestors.reverse();

    let mut resolved = ResolvedStyles::default();
    for instance_id in ancestors {
        let declarations = matching_declarations(world, instance_id, stylesheets, pseudo);
        for (name, winner) in declarations {
            let value = winner.declaration.value.clone();
            if name.starts_with("--") {
                resolved.variables.insert(name.to_owned(), value);
            } else if instance_id == id {
                resolved.values.insert(name.to_owned(), value);
            }
        }
    }
    resolved
}

/// The declarations that apply to `id`, with where each is written, sorted
/// by property name. Custom properties inherited from ancestors aren't included.
pub fn applied_declarations(
    world: &World,
    id: InstanceId,
    stylesheets: &[Stylesheet],
) -> Vec<CssDeclaration> {
    let mut applied: Vec<_> =
        matching_declarations(world, id, stylesheets, PseudoClasses::default())
            .into_values()
            .map(|winner| CssDeclaration {
                property: winner.declaration.name.clone(),
                value: winner.declaration.value.clone(),
                path: winner.path.to_owned(),
                selector: winner.selector.to_owned(),
                span: winner.declaration.span.clone(),
                line: winner.declaration.line,
            })
            .collect();
    applied.sort_by(|a, b| a.property.cmp(&b.property));
    applied
}

/// The declaration of `property` that wins the cascade for `id`, if any.
pub fn applied_declaration(
    world: &World,
    id: InstanceId,
    stylesheets: &[Stylesheet],
    property: &str,
) -> Option<CssDeclaration> {
    applied_declarations(world, id, stylesheets)
        .into_iter()
        .find(|declaration| declaration.property == property)
}

/// The declaration winning the cascade for one property, and its rule.
struct Winner<'a> {
    specificity: Specificity,
    order: usize,
    path: &'a str,
    selector: &'a str,
    declaration: &'a Declaration,
}

/// The winning declaration of each property for `id`. The `'a` says the
/// winners borrow from `stylesheets`, so nothing is copied.
fn matching_declarations<'a>(
    world: &World,
    id: InstanceId,
    stylesheets: &'a [Stylesheet],
    pseudo: PseudoClasses,
) -> HashMap<&'a str, Winner<'a>> {
    let mut winners: HashMap<&str, Winner> = HashMap::new();
    let mut order = 0;
    for stylesheet in stylesheets {
        for rule in &stylesheet.rules {
            for declaration in &rule.declarations {
                for selector in &rule.selectors {
                    if !selector_matches(world, id, selector, pseudo) {
                        continue;
                    }
                    let replace = winners.get(declaration.name.as_str()).is_none_or(|old| {
                        (selector.specificity, order) > (old.specificity, old.order)
                    });
                    if replace {
                        winners.insert(
                            &declaration.name,
                            Winner {
                                specificity: selector.specificity,
                                order,
                                path: &stylesheet.path,
                                selector: &rule.selector_text,
                                declaration,
                            },
                        );
                    }
                }
                order += 1;
            }
        }
    }
    winners
}

fn selector_matches(
    world: &World,
    id: InstanceId,
    selector: &Selector,
    pseudo: PseudoClasses,
) -> bool {
    let Some(last) = selector.parts.last() else {
        return false;
    };
    if !simple_matches(world, id, last, pseudo) {
        return false;
    }

    let mut ancestor = world.get(id).and_then(|instance| instance.parent());
    for part in selector.parts[..selector.parts.len() - 1].iter().rev() {
        let mut found = None;
        while let Some(candidate) = ancestor {
            if simple_matches(world, candidate, part, pseudo) {
                found = world.get(candidate).and_then(|instance| instance.parent());
                break;
            }
            ancestor = world.get(candidate).and_then(|instance| instance.parent());
        }
        let Some(next_ancestor) = found else {
            return false;
        };
        ancestor = Some(next_ancestor);
    }
    true
}

fn simple_matches(
    world: &World,
    id: InstanceId,
    selector: &SimpleSelector,
    pseudo: PseudoClasses,
) -> bool {
    let Some(instance) = world.get(id) else {
        return false;
    };
    if selector
        .class
        .is_some_and(|class| class != instance.class())
    {
        return false;
    }
    if (selector.hovered && pseudo.hovered != Some(id))
        || (selector.active && pseudo.active != Some(id))
    {
        return false;
    }
    if selector.id.as_deref().is_some_and(|id| {
        instance
            .property("Id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            != id
    }) {
        return false;
    }
    let classes = instance
        .property("Class")
        .and_then(Value::as_str)
        .unwrap_or_default();
    selector.classes.iter().all(|class| {
        classes
            .split_whitespace()
            .any(|candidate| candidate == class)
    })
}

fn parse_selector(selector: &str) -> Option<Selector> {
    if selector.is_empty() {
        return None;
    }
    let mut specificity = (0, 0, 0);
    let mut parts = Vec::new();
    for raw in selector.split_whitespace() {
        let mut part = SimpleSelector::default();
        let mut cursor = 0;
        while cursor < raw.len() {
            let bytes = raw.as_bytes();
            match bytes[cursor] {
                b'.' | b'#' => {
                    let marker = bytes[cursor];
                    cursor += 1;
                    let start = cursor;
                    while cursor < raw.len() && is_identifier(bytes[cursor]) {
                        cursor += 1;
                    }
                    if start == cursor {
                        return None;
                    }
                    let name = raw[start..cursor].to_owned();
                    if marker == b'.' {
                        part.classes.push(name);
                        specificity.1 += 1;
                    } else {
                        if part.id.replace(name).is_some() {
                            return None;
                        }
                        specificity.0 += 1;
                    }
                }
                b':' => {
                    cursor += 1;
                    let start = cursor;
                    while cursor < raw.len() && is_identifier(bytes[cursor]) {
                        cursor += 1;
                    }
                    match &raw[start..cursor] {
                        "hover" if start != cursor && !part.hovered => {
                            part.hovered = true;
                            specificity.1 += 1;
                        }
                        "active" if start != cursor && !part.active => {
                            part.active = true;
                            specificity.1 += 1;
                        }
                        _ => return None,
                    }
                }
                byte => {
                    let start = cursor;
                    while cursor < raw.len()
                        && (is_identifier(bytes[cursor]) || bytes[cursor] == b'*')
                    {
                        cursor += 1;
                    }
                    if start == cursor {
                        return None;
                    }
                    let name = &raw[start..cursor];
                    if name != "*" {
                        let class = ClassName::from_name(name)?;
                        if part.class.replace(class).is_some() {
                            return None;
                        }
                        specificity.2 += 1;
                    } else if byte != b'*' {
                        return None;
                    }
                }
            }
        }
        parts.push(part);
    }
    Some(Selector { parts, specificity })
}

/// The declarations in a rule's `block`. Their spans are relative to the
/// block; the caller makes them relative to the whole source.
fn parse_declarations(
    block: &str,
    path: &str,
    warnings: &mut Vec<StyleWarning>,
) -> Vec<Declaration> {
    let mut declarations = Vec::new();
    for declaration in split_top_level(block, ';') {
        let declaration = declaration.trim();
        if declaration.is_empty() {
            continue;
        }
        let Some(colon) = find_delimiter(declaration, ':') else {
            warnings.push(warning(
                path,
                format!("ignored invalid declaration `{declaration}`"),
            ));
            continue;
        };
        let raw_name = declaration[..colon].trim();
        let name = if raw_name.starts_with("--") {
            raw_name.to_owned()
        } else {
            raw_name.to_ascii_lowercase()
        };
        let value = declaration[colon + 1..].trim();
        if name.is_empty() || value.is_empty() {
            warnings.push(warning(
                path,
                format!("ignored invalid declaration `{declaration}`"),
            ));
        } else if !name.starts_with("--") && !SUPPORTED_PROPERTIES.contains(&name.as_str()) {
            warnings.push(warning(path, format!("unsupported CSS property `{name}`")));
        } else {
            let start = offset_in(block, value);
            declarations.push(Declaration {
                name,
                value: value.to_owned(),
                span: start..start + value.len(),
                line: 0,
            });
        }
    }
    declarations
}

/// Where `inner`, a slice of `outer`, starts in it, in bytes.
fn offset_in(outer: &str, inner: &str) -> usize {
    inner.as_ptr() as usize - outer.as_ptr() as usize
}

fn warning(path: &str, message: impl Into<String>) -> StyleWarning {
    StyleWarning {
        path: path.to_owned(),
        message: message.into(),
    }
}

/// `source` with each comment blanked out by spaces (keeping its line
/// breaks), so everything else stays at the same byte offset.
fn strip_comments(source: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    let mut quote = None;
    while let Some(ch) = chars.next() {
        if let Some(end_quote) = quote {
            output.push(ch);
            if ch == '\\' {
                if let Some(escaped) = chars.next() {
                    output.push(escaped);
                }
            } else if ch == end_quote {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            output.push(ch);
        } else if ch == '/' && chars.peek() == Some(&'*') {
            chars.next();
            output.push_str("  ");
            while let Some(comment_char) = chars.next() {
                if comment_char == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    output.push_str("  ");
                    break;
                }
                match comment_char {
                    '\n' => output.push('\n'),
                    other => output.extend(std::iter::repeat_n(' ', other.len_utf8())),
                }
            }
        } else {
            output.push(ch);
        }
    }
    output
}

fn find_delimiter(source: &str, delimiter: char) -> Option<usize> {
    let mut quote = None;
    let mut depth = 0_u32;
    let mut escaped = false;
    for (index, ch) in source.char_indices() {
        if escaped {
            escaped = false;
        } else if ch == '\\' && quote.is_some() {
            escaped = true;
        } else if let Some(end_quote) = quote {
            if ch == end_quote {
                quote = None;
            }
        } else {
            match ch {
                '\'' | '"' => quote = Some(ch),
                '(' => depth += 1,
                ')' => depth = depth.saturating_sub(1),
                _ if ch == delimiter && depth == 0 => return Some(index),
                _ => {}
            }
        }
    }
    None
}

fn split_top_level(source: &str, delimiter: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut rest = source;
    while let Some(index) = find_delimiter(rest, delimiter) {
        parts.push(&rest[..index]);
        rest = &rest[index + delimiter.len_utf8()..];
        start = source.len() - rest.len();
    }
    parts.push(&source[start..]);
    parts
}

fn is_identifier(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')
}

pub(crate) fn resolve_variables(
    value: &str,
    variables: &HashMap<String, String>,
) -> Option<String> {
    resolve_variables_inner(value, variables, &mut Vec::new())
}

fn resolve_variables_inner(
    value: &str,
    variables: &HashMap<String, String>,
    resolving: &mut Vec<String>,
) -> Option<String> {
    let mut output = String::new();
    let mut rest = value;
    while let Some(start) = rest.find("var(") {
        output.push_str(&rest[..start]);
        let expression = &rest[start + 4..];
        let end = matching_parenthesis(expression)?;
        let arguments = &expression[..end];
        let parts = split_top_level(arguments, ',');
        let name = parts.first()?.trim();
        if !name.starts_with("--") || name.len() < 3 {
            return None;
        }
        let replacement = if resolving.contains(&name.to_owned()) {
            None
        } else if let Some(value) = variables.get(name) {
            resolving.push(name.to_owned());
            let resolved = resolve_variables_inner(value, variables, resolving);
            resolving.pop();
            resolved
        } else {
            None
        };
        let replacement = match replacement {
            Some(value) => value,
            None if parts.len() > 1 => {
                resolve_variables_inner(parts[1].trim(), variables, resolving)?
            }
            None => return None,
        };
        output.push_str(&replacement);
        rest = &expression[end + 1..];
    }
    output.push_str(rest);
    Some(output)
}

fn matching_parenthesis(source: &str) -> Option<usize> {
    let mut depth = 1_u32;
    for (index, ch) in source.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_core::ClassName;

    #[test]
    fn selectors_use_specificity_cascade_and_descendants() {
        let mut world = World::new();
        let gui = world.create(ClassName::ScreenGui).unwrap();
        world.set_parent(gui, Some(world.workspace())).unwrap();
        let frame = world.create(ClassName::Frame).unwrap();
        world.set_parent(frame, Some(gui)).unwrap();
        world
            .set_property(frame, "Class", Value::String("hud panel".into()))
            .unwrap();
        world
            .set_property(frame, "Id", Value::String("status".into()))
            .unwrap();

        let (sheet, warnings) = parse_stylesheet(
            "hud.css",
            "Frame { opacity: 0.1 } .panel { opacity: 0.2 } #status { opacity: 0.3 } ScreenGui #status { opacity: 0.4 }",
        );
        assert!(warnings.is_empty());
        assert_eq!(
            resolve_styles(&world, frame, &[sheet], PseudoClasses::default()).values["opacity"],
            "0.4"
        );
    }

    #[test]
    fn hover_and_active_pseudo_classes_use_class_specificity() {
        let mut world = World::new();
        let gui = world.create(ClassName::ScreenGui).unwrap();
        world.set_parent(gui, Some(world.workspace())).unwrap();
        let button = world.create(ClassName::TextButton).unwrap();
        world.set_parent(button, Some(gui)).unwrap();
        world
            .set_property(button, "Class", Value::String("start".into()))
            .unwrap();
        let (sheet, warnings) = parse_stylesheet(
            "ui.css",
            ".start { opacity: 0.2 } TextButton:hover { opacity: 0.5 } .start:active { opacity: 0.9 }",
        );
        assert!(warnings.is_empty());
        assert_eq!(
            resolve_styles(
                &world,
                button,
                std::slice::from_ref(&sheet),
                PseudoClasses {
                    hovered: Some(button),
                    active: None,
                    focused: None,
                },
            )
            .values["opacity"],
            "0.5"
        );
        assert_eq!(
            resolve_styles(
                &world,
                button,
                &[sheet],
                PseudoClasses {
                    hovered: Some(button),
                    active: Some(button),
                    focused: None,
                },
            )
            .values["opacity"],
            "0.9"
        );
    }

    #[test]
    fn unknown_properties_warn_but_other_rules_still_parse() {
        let (sheet, warnings) =
            parse_stylesheet("hud.css", ".hud { unsupported-prop: x; opacity: .5 }");
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].path, "hud.css");
        assert_eq!(sheet.rules.len(), 1);
        let declarations = &sheet.rules[0].declarations;
        assert_eq!(declarations.len(), 1);
        assert_eq!(
            (
                declarations[0].name.as_str(),
                declarations[0].value.as_str()
            ),
            ("opacity", ".5")
        );
    }

    #[test]
    fn applied_declarations_say_where_they_are_written() {
        let mut world = World::new();
        let gui = world.create(ClassName::ScreenGui).unwrap();
        world.set_parent(gui, Some(world.workspace())).unwrap();
        let frame = world.create(ClassName::Frame).unwrap();
        world.set_parent(frame, Some(gui)).unwrap();
        world
            .set_property(frame, "Class", Value::String("hud".into()))
            .unwrap();
        let source = "/* a comment\n with ü */ .hud {\n  width: 280px;\n  opacity: .5 }\nFrame { width: 10px }";
        let (sheet, warnings) = parse_stylesheet("ui.css", source);
        assert!(warnings.is_empty());

        let width = applied_declaration(&world, frame, &[sheet], "width").unwrap();
        assert_eq!(&source[width.span.clone()], "280px");
        assert_eq!(width.value, "280px");
        assert_eq!(width.line, 3);
        assert_eq!(width.selector, ".hud");
        assert_eq!(width.path, "ui.css");
    }

    #[test]
    fn a_rule_is_found_by_its_selector_to_add_to() {
        let source = "#score { color: red; }\n.hud { gap: 4px; }\n";
        let (stylesheet, _) = parse_stylesheet("ui.css", source);
        let end = stylesheet.rule_end("#score").unwrap();
        assert_eq!(&source[end..end + 1], "}");
        assert_eq!(stylesheet.rule_end("#missing"), None);
        assert_eq!(
            &source[..stylesheet.rule_end(".hud").unwrap()],
            "#score { color: red; }\n.hud { gap: 4px; "
        );
    }

    #[test]
    fn variables_resolve_with_fallbacks_and_detect_cycles() {
        let variables = HashMap::from([
            ("--primary".to_owned(), "#1a2b3c".to_owned()),
            ("--alias".to_owned(), "var(--primary)".to_owned()),
            ("--cycle".to_owned(), "var(--cycle)".to_owned()),
        ]);
        assert_eq!(
            resolve_variables("var(--alias, red)", &variables),
            Some("#1a2b3c".to_owned())
        );
        assert_eq!(
            resolve_variables("var(--missing, var(--primary))", &variables),
            Some("#1a2b3c".to_owned())
        );
        assert_eq!(
            resolve_variables("var(--cycle, #fff)", &variables),
            Some("#fff".to_owned())
        );
    }
}
