use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;

use axcenity_core::{ClassName, InstanceId, Value, World};

use crate::interact::UiState;
use crate::transition::Transitions;

/// The CSS properties UI elements understand. Others are warned about.
pub const SUPPORTED_PROPERTIES: &[&str] = &[
    "-webkit-text-stroke",
    "align-items",
    "aspect-ratio",
    "background-color",
    "background-image",
    "border",
    "border-bottom",
    "border-bottom-color",
    "border-bottom-width",
    "border-color",
    "border-image",
    "border-left",
    "border-left-color",
    "border-left-width",
    "border-radius",
    "border-right",
    "border-right-color",
    "border-right-width",
    "border-style",
    "border-top",
    "border-top-color",
    "border-top-width",
    "border-width",
    "box-shadow",
    "color",
    "cursor",
    "display",
    "flex-direction",
    "flex-grow",
    "font-family",
    "font-size",
    "font-style",
    "font-weight",
    "gap",
    "height",
    "justify-content",
    "letter-spacing",
    "line-height",
    "margin",
    "margin-bottom",
    "margin-left",
    "margin-right",
    "margin-top",
    "opacity",
    "overflow",
    "padding",
    "padding-bottom",
    "padding-left",
    "padding-right",
    "padding-top",
    "text-align",
    "text-decoration",
    "text-shadow",
    "text-stroke",
    "text-transform",
    "transform",
    "transition",
    "vertical-align",
    "white-space",
    "width",
    "z-index",
];

type Specificity = (u32, u32, u32);

/// What the cascade needs besides the tree: the pointer's states, and the
/// running CSS transitions. The `'a` is how long the transitions are
/// borrowed for; the struct is `Copy`, so it's passed by value everywhere.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct PseudoClasses<'a> {
    pub(crate) hovered: Option<InstanceId>,
    pub(crate) active: Option<InstanceId>,
    pub(crate) focused: Option<InstanceId>,
    /// An element shown in a state whatever the pointer does, for the UI
    /// Designer's state preview.
    pub(crate) forced: Option<(InstanceId, UiState)>,
    /// Eases each element's styles toward their new values. `RefCell`
    /// lets the shared borrow update them as styles are resolved.
    pub(crate) transitions: Option<&'a RefCell<Transitions>>,
    /// ScrollingFrames shown scrolled to these canvas positions instead of
    /// their own `CanvasPosition`: the UI Designer scrolls them this way.
    pub(crate) scrolled: Option<&'a HashMap<InstanceId, [f32; 2]>>,
}

/// A part of an element that isn't an Instance, styled with a CSS
/// pseudo-element: a ScrollingFrame's bar (`::scrollbar`) and the thumb
/// dragged along it (`::thumb`, also a Slider's and a switch's knob), and
/// the widgets' parts (`::track`, `::fill`, `::check`, and a Dropdown's
/// `::list` and each `::option` in it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PseudoElement {
    Scrollbar,
    Thumb,
    Track,
    Fill,
    Check,
    List,
    Option,
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
    disabled: bool,
    /// `:checked`: a Toggle that's on.
    checked: bool,
    /// The type name `Tooltip`: the box a `Tooltip` property shows, which
    /// isn't an Instance. It's styled as a child of the element it's for.
    tooltip: bool,
    /// A part of the element (`::thumb`): only the last part of a selector
    /// has one.
    element: Option<PseudoElement>,
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
    pseudo: PseudoClasses<'_>,
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
    if let Some(transitions) = pseudo.transitions {
        transitions.borrow_mut().apply(world, id, &mut resolved);
    }
    resolved
}

/// The styles of the tooltip `owner` shows: rules whose last part is
/// `Tooltip`, with the rest matching `owner` or its ancestors (so
/// `#play Tooltip` styles only the Play button's), and the custom
/// properties `owner` inherits.
pub(crate) fn resolve_tooltip_styles(
    world: &World,
    owner: InstanceId,
    stylesheets: &[Stylesheet],
) -> ResolvedStyles {
    let mut resolved = resolve_styles(world, owner, stylesheets, PseudoClasses::default());
    resolved.values.clear();
    let winners = winning_declarations(stylesheets, |selector| {
        tooltip_selector_matches(world, owner, selector)
    });
    for (name, winner) in winners {
        let target = if name.starts_with("--") {
            &mut resolved.variables
        } else {
            &mut resolved.values
        };
        target.insert(name.to_owned(), winner.declaration.value.clone());
    }
    resolved
}

/// The styles of a part of `owner`, such as a ScrollingFrame's thumb: rules
/// whose last part names it (`#list::thumb`, `ScrollingFrame:hover::thumb`),
/// with the rest matching `owner` as usual, and the custom properties
/// `owner` inherits.
pub(crate) fn resolve_part_styles(
    world: &World,
    owner: InstanceId,
    stylesheets: &[Stylesheet],
    pseudo: PseudoClasses<'_>,
    part: PseudoElement,
) -> ResolvedStyles {
    let plain = PseudoClasses {
        transitions: None,
        ..pseudo
    };
    let mut resolved = resolve_styles(world, owner, stylesheets, plain);
    resolved.values.clear();
    let winners = winning_declarations(stylesheets, |selector| {
        selector_matches_part(world, owner, selector, plain, Some(part))
    });
    for (name, winner) in winners {
        let target = if name.starts_with("--") {
            &mut resolved.variables
        } else {
            &mut resolved.values
        };
        target.insert(name.to_owned(), winner.declaration.value.clone());
    }
    resolved
}

/// The declaration of `property` in the last rule written exactly as
/// `selector` (such as `#play:hover`), whether or not it wins the cascade.
pub fn rule_declaration(
    stylesheets: &[Stylesheet],
    selector: &str,
    property: &str,
) -> Option<CssDeclaration> {
    stylesheets.iter().rev().find_map(|stylesheet| {
        stylesheet
            .rules
            .iter()
            .rev()
            .filter(|rule| rule.selector_text == selector)
            .find_map(|rule| {
                let declaration = rule
                    .declarations
                    .iter()
                    .rev()
                    .find(|declaration| declaration.name == property)?;
                Some(CssDeclaration {
                    property: declaration.name.clone(),
                    value: declaration.value.clone(),
                    path: stylesheet.path.clone(),
                    selector: rule.selector_text.clone(),
                    span: declaration.span.clone(),
                    line: declaration.line,
                })
            })
    })
}

/// The rules written for the element with `Id` `element_id` alone: `#id`
/// itself and `#id` followed by states or parts (`#id:hover`,
/// `#id::thumb`), in the order they're first written. Each is what follows
/// `#id` in its selector (`""` for `#id`) and its declarations, a later one
/// of the same property replacing an earlier one in place.
pub fn own_rules(
    stylesheets: &[Stylesheet],
    element_id: &str,
) -> Vec<(String, Vec<(String, String)>)> {
    let own = format!("#{element_id}");
    let mut rules: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for rule in stylesheets.iter().flat_map(|sheet| &sheet.rules) {
        let Some(suffix) = rule.selector_text.strip_prefix(&own) else {
            continue;
        };
        if !(suffix.is_empty() || suffix.starts_with(':')) || suffix.contains([' ', ',']) {
            continue;
        }
        let index = match rules.iter().position(|(known, _)| known == suffix) {
            Some(index) => index,
            None => {
                rules.push((suffix.to_owned(), Vec::new()));
                rules.len() - 1
            }
        };
        let declarations = &mut rules[index].1;
        for declaration in &rule.declarations {
            match declarations
                .iter_mut()
                .find(|(name, _)| *name == declaration.name)
            {
                Some((_, value)) => value.clone_from(&declaration.value),
                None => declarations.push((declaration.name.clone(), declaration.value.clone())),
            }
        }
    }
    rules
}

/// Whether any stylesheet has a rule written exactly as `selector`.
pub fn has_rule(stylesheets: &[Stylesheet], selector: &str) -> bool {
    stylesheets
        .iter()
        .any(|stylesheet| stylesheet.rule_end(selector).is_some())
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
    pseudo: PseudoClasses<'_>,
) -> HashMap<&'a str, Winner<'a>> {
    winning_declarations(stylesheets, |selector| {
        selector_matches(world, id, selector, pseudo)
    })
}

/// The winning declaration of each property among the rules whose
/// selector `matches`.
fn winning_declarations<'a>(
    stylesheets: &'a [Stylesheet],
    matches: impl Fn(&Selector) -> bool,
) -> HashMap<&'a str, Winner<'a>> {
    let mut winners: HashMap<&str, Winner> = HashMap::new();
    let mut order = 0;
    for stylesheet in stylesheets {
        for rule in &stylesheet.rules {
            for declaration in &rule.declarations {
                for selector in &rule.selectors {
                    if !matches(selector) {
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
    pseudo: PseudoClasses<'_>,
) -> bool {
    selector_matches_part(world, id, selector, pseudo, None)
}

/// Whether `selector` styles `id` itself (`part` is `None`), or that part of it.
fn selector_matches_part(
    world: &World,
    id: InstanceId,
    selector: &Selector,
    pseudo: PseudoClasses<'_>,
    part: Option<PseudoElement>,
) -> bool {
    let Some(last) = selector.parts.last() else {
        return false;
    };
    if last.element != part || !simple_matches(world, id, last, pseudo) {
        return false;
    }
    let ancestor = world.get(id).and_then(|instance| instance.parent());
    ancestors_match(
        world,
        ancestor,
        &selector.parts[..selector.parts.len() - 1],
        pseudo,
    )
}

/// Whether a selector ending in `Tooltip` styles the tooltip of `owner`:
/// the parts before it match `owner` or the elements around it.
fn tooltip_selector_matches(world: &World, owner: InstanceId, selector: &Selector) -> bool {
    let Some(last) = selector.parts.last() else {
        return false;
    };
    if !last.tooltip || last.id.is_some() || !last.classes.is_empty() || last.element.is_some() {
        return false;
    }
    let parts = &selector.parts[..selector.parts.len() - 1];
    ancestors_match(world, Some(owner), parts, PseudoClasses::default())
}

/// Whether `parts`, a descendant selector's leading parts, each match an
/// element from `ancestor` up, in order, nearest last.
fn ancestors_match(
    world: &World,
    mut ancestor: Option<InstanceId>,
    parts: &[SimpleSelector],
    pseudo: PseudoClasses<'_>,
) -> bool {
    for part in parts.iter().rev() {
        loop {
            let Some(candidate) = ancestor else {
                return false;
            };
            ancestor = world.get(candidate).and_then(|instance| instance.parent());
            if simple_matches(world, candidate, part, pseudo) {
                break;
            }
        }
    }
    true
}

fn simple_matches(
    world: &World,
    id: InstanceId,
    selector: &SimpleSelector,
    pseudo: PseudoClasses<'_>,
) -> bool {
    let Some(instance) = world.get(id) else {
        return false;
    };
    if selector.tooltip
        || selector
            .class
            .is_some_and(|class| class != instance.class())
    {
        return false;
    }
    let interactable = instance.property("Interactable").and_then(Value::as_bool) != Some(false);
    let forced = pseudo
        .forced
        .filter(|(forced, _)| *forced == id)
        .map(|(_, state)| state);
    // A pressed button is hovered too, as in CSS.
    let (hovered, active, disabled) = match forced {
        Some(UiState::Normal) => (false, false, !interactable),
        Some(UiState::Hover) => (true, false, false),
        Some(UiState::Pressed) => (true, true, false),
        Some(UiState::Disabled) => (false, false, true),
        None => (
            pseudo.hovered == Some(id),
            pseudo.active == Some(id),
            !interactable,
        ),
    };
    let checked = instance.class() == ClassName::Toggle
        && instance.property("Value").and_then(Value::as_bool) == Some(true);
    if (selector.hovered && !hovered)
        || (selector.active && !active)
        || (selector.disabled && !disabled)
        || (selector.checked && !checked)
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
                b':' if bytes.get(cursor + 1) == Some(&b':') => {
                    // A pseudo-element ends the selector.
                    cursor += 2;
                    let start = cursor;
                    while cursor < raw.len() && is_identifier(bytes[cursor]) {
                        cursor += 1;
                    }
                    if cursor < raw.len() || part.element.is_some() {
                        return None;
                    }
                    part.element = Some(match &raw[start..cursor] {
                        "scrollbar" | "-webkit-scrollbar" => PseudoElement::Scrollbar,
                        "thumb" | "-webkit-scrollbar-thumb" => PseudoElement::Thumb,
                        "track" => PseudoElement::Track,
                        "fill" => PseudoElement::Fill,
                        "check" => PseudoElement::Check,
                        "list" => PseudoElement::List,
                        "option" => PseudoElement::Option,
                        _ => return None,
                    });
                    specificity.2 += 1;
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
                        "disabled" if start != cursor && !part.disabled => {
                            part.disabled = true;
                            specificity.1 += 1;
                        }
                        "checked" if start != cursor && !part.checked => {
                            part.checked = true;
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
                    if name == "Tooltip" {
                        if part.tooltip || part.class.is_some() {
                            return None;
                        }
                        part.tooltip = true;
                        specificity.2 += 1;
                    } else if name != "*" {
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
    // Only the last part can name a pseudo-element.
    if parts[..parts.len() - 1]
        .iter()
        .any(|part| part.element.is_some())
    {
        return None;
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
                    ..PseudoClasses::default()
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
                    ..PseudoClasses::default()
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
