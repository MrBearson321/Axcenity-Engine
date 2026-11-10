use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;

use axcenity_core::{ClassName, Instance, InstanceId, Value, World};
use rustc_hash::FxHashMap;

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
    /// The element keyboard or controller navigation has focused.
    pub(crate) selected: Option<InstanceId>,
    /// An element shown in a state whatever the pointer does, for the UI
    /// Designer's state preview.
    pub(crate) forced: Option<(InstanceId, UiState)>,
    /// Eases each element's styles toward their new values. `RefCell`
    /// lets the shared borrow update them as styles are resolved.
    pub(crate) transitions: Option<&'a RefCell<Transitions>>,
    /// ScrollingFrames shown scrolled to these canvas positions instead of
    /// their own `CanvasPosition`: the UI Designer scrolls them this way.
    pub(crate) scrolled: Option<&'a HashMap<InstanceId, [f32; 2]>>,
    /// Elements the UI Designer hides on its artboard (see
    /// [`crate::DesignerOverrides`]).
    pub(crate) hidden: Option<&'a HashSet<InstanceId>>,
    /// The page each PageLayout shows on the UI Designer's artboard.
    pub(crate) pages: Option<&'a HashMap<InstanceId, InstanceId>>,
    /// Styles already matched during this layout, shared by its passes
    /// (see [`StyleCache`]).
    pub(crate) styles: Option<&'a StyleCache>,
    /// Only where elements go is wanted (a pass for hit tests), so their
    /// boxes, text and scroll bars aren't painted.
    pub(crate) unpainted: bool,
}

/// The pointer and focus states selectors match against: what a
/// [`StyleCache`]'s entries were matched in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct MatchStates {
    hovered: Option<InstanceId>,
    active: Option<InstanceId>,
    focused: Option<InstanceId>,
    selected: Option<InstanceId>,
    forced: Option<(InstanceId, UiState)>,
}

impl MatchStates {
    fn of(pseudo: &PseudoClasses<'_>) -> Self {
        Self {
            hovered: pseudo.hovered,
            active: pseudo.active,
            focused: pseudo.focused,
            selected: pseudo.selected,
            forced: pseudo.forced,
        }
    }

    /// The elements whose own state differs between `self` and `other`.
    /// Only they and their descendants can match differently.
    fn changed(&self, other: &Self) -> Vec<InstanceId> {
        let mut changed = Vec::new();
        let mut differ = |a: Option<InstanceId>, b: Option<InstanceId>| {
            if a != b {
                changed.extend(a);
                changed.extend(b);
            }
        };
        differ(self.hovered, other.hovered);
        differ(self.active, other.active);
        differ(self.focused, other.focused);
        differ(self.selected, other.selected);
        if self.forced != other.forced {
            changed.extend(self.forced.map(|(id, _)| id));
            changed.extend(other.forced.map(|(id, _)| id));
        }
        changed
    }
}

/// The cascade's result for each element already resolved during one
/// layout (roadmap item M14), before transitions ease it. Without it each
/// element matched every rule against itself and every ancestor (for the
/// custom properties it inherits), once for each time its styles were
/// read. With it an element is matched once, and inherits its parent's
/// cached variables.
///
/// It's only valid while the world is unchanged, so a layout makes a new
/// one each frame. The entries hold for the states (hover, press, focus)
/// set by [`StyleCache::begin`]; the passes of one frame differ only in
/// those, so moving to the next pass keeps every entry outside the
/// subtrees whose state changed. A lookup in other states bypasses it.
#[derive(Debug, Default)]
pub(crate) struct StyleCache {
    inner: RefCell<StyleCacheInner>,
}

#[derive(Debug, Default)]
struct StyleCacheInner {
    states: Option<MatchStates>,
    entries: HashMap<InstanceId, Rc<ResolvedStyles>>,
    /// The entries once eased by the transitions at `eased_by`. Easing is
    /// the same however often it's asked in one frame, so it's done once.
    eased: HashMap<InstanceId, Rc<ResolvedStyles>>,
    eased_by: Option<*const RefCell<Transitions>>,
}

impl StyleCache {
    /// Makes the entries hold for `pseudo`'s states, forgetting those of
    /// elements whose state (or an ancestor's) is different from before.
    pub(crate) fn begin(&self, world: &World, pseudo: &PseudoClasses<'_>) {
        let states = MatchStates::of(pseudo);
        let mut inner = self.inner.borrow_mut();
        let Some(old) = inner.states.replace(states) else {
            return;
        };
        let changed = old.changed(&states);
        if changed.is_empty() {
            return;
        }
        let unchanged = |id: &InstanceId| {
            let mut current = Some(*id);
            while let Some(at) = current {
                if changed.contains(&at) {
                    return false;
                }
                current = world.get(at).and_then(Instance::parent);
            }
            true
        };
        inner.entries.retain(|id, _| unchanged(id));
        inner.eased.retain(|id, _| unchanged(id));
    }

    /// `id`'s styles as `transitions` eased them earlier in these states.
    fn eased(
        &self,
        id: InstanceId,
        transitions: &RefCell<Transitions>,
        pseudo: &PseudoClasses<'_>,
    ) -> Option<Rc<ResolvedStyles>> {
        let inner = self.inner.borrow();
        if inner.states != Some(MatchStates::of(pseudo))
            || inner.eased_by != Some(std::ptr::from_ref(transitions))
        {
            return None;
        }
        inner.eased.get(&id).cloned()
    }

    /// Keeps `styles`, `id`'s styles eased by `transitions` in `pseudo`'s states.
    fn keep_eased(
        &self,
        id: InstanceId,
        transitions: &RefCell<Transitions>,
        pseudo: &PseudoClasses<'_>,
        styles: &Rc<ResolvedStyles>,
    ) {
        let mut inner = self.inner.borrow_mut();
        if inner.states != Some(MatchStates::of(pseudo)) {
            return;
        }
        let by = std::ptr::from_ref(transitions);
        if inner.eased_by != Some(by) {
            inner.eased.clear();
            inner.eased_by = Some(by);
        }
        inner.eased.insert(id, Rc::clone(styles));
    }

    /// `id`'s cascaded styles in `pseudo`'s states, matched now if they
    /// aren't cached yet, or `None` when the cache is for other states.
    fn styles(
        &self,
        world: &World,
        id: InstanceId,
        stylesheets: &[Stylesheet],
        pseudo: &PseudoClasses<'_>,
    ) -> Option<Rc<ResolvedStyles>> {
        {
            let inner = self.inner.borrow();
            if inner.states != Some(MatchStates::of(pseudo)) {
                return None;
            }
            if let Some(styles) = inner.entries.get(&id) {
                return Some(Rc::clone(styles));
            }
        }
        let instance = world.get(id)?;
        let mut resolved = match instance.parent() {
            Some(parent) => {
                let inherited = self.styles(world, parent, stylesheets, pseudo);
                ResolvedStyles {
                    values: StyleMap::default(),
                    variables: inherited
                        .map(|styles| Rc::clone(&styles.variables))
                        .unwrap_or_default(),
                }
            }
            None => ResolvedStyles::default(),
        };
        add_own_declarations(world, id, stylesheets, *pseudo, &mut resolved);
        let resolved = Rc::new(resolved);
        self.inner
            .borrow_mut()
            .entries
            .insert(id, Rc::clone(&resolved));
        Some(resolved)
    }
}

/// A part of an element that isn't an Instance, styled with a CSS
/// pseudo-element: a ScrollingFrame's bar (`::scrollbar`) and the thumb
/// dragged along it (`::thumb`, also a Slider's and a switch's knob), and
/// the widgets' parts (`::track`, `::fill`, `::check`, and a Dropdown's
/// `::list` and each `::option` in it), and a ProximityPrompt's `::key`
/// cap and hold `::ring`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PseudoElement {
    Scrollbar,
    Thumb,
    Track,
    Fill,
    Check,
    List,
    Option,
    Key,
    Ring,
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

/// CSS property (or custom property) names to their values. Fx hashing,
/// as styles are looked up by short names many times per element (M14).
pub(crate) type StyleMap = FxHashMap<String, String>;

#[derive(Clone, Debug, Default)]
pub(crate) struct ResolvedStyles {
    pub(crate) values: StyleMap,
    /// Shared with the parent's until an element sets its own (M14).
    pub(crate) variables: Rc<StyleMap>,
}

/// So code can take a list of styles, shared or not.
impl AsRef<ResolvedStyles> for ResolvedStyles {
    fn as_ref(&self) -> &ResolvedStyles {
        self
    }
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
    /// `:checked`: a Toggle that's on, or a Tool in hand.
    checked: bool,
    /// `:focus`: the element navigation focused, or a TextBox being typed in.
    focus: bool,
    /// The type name `Tooltip`: the box a `Tooltip` property shows, which
    /// isn't an Instance. It's styled as a child of the element it's for.
    tooltip: bool,
    /// A part of the element (`::thumb`): only the last part of a selector
    /// has one.
    element: Option<PseudoElement>,
    /// A type name for one of the engine's own widgets that isn't an
    /// Instance, such as `DialogueBox` (Phase 77). Styled by itself: see
    /// [`resolve_widget_styles`].
    widget: Option<Widget>,
}

/// The engine's own widgets that CSS styles by a type name of their own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Widget {
    DialogueBox,
    QuestTracker,
}

impl Widget {
    fn from_name(name: &str) -> Option<Self> {
        match name {
            "DialogueBox" => Some(Widget::DialogueBox),
            "QuestTracker" => Some(Widget::QuestTracker),
            _ => None,
        }
    }
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
    Rc::unwrap_or_clone(shared_styles(world, id, stylesheets, pseudo))
}

/// [`resolve_styles`], shared with `pseudo`'s [`StyleCache`] rather than
/// copied out of it, for layout's hot path.
pub(crate) fn shared_styles(
    world: &World,
    id: InstanceId,
    stylesheets: &[Stylesheet],
    pseudo: PseudoClasses<'_>,
) -> Rc<ResolvedStyles> {
    if let (Some(cache), Some(transitions)) = (pseudo.styles, pseudo.transitions)
        && let Some(eased) = cache.eased(id, transitions, &pseudo)
    {
        return eased;
    }
    let cached = pseudo
        .styles
        .and_then(|cache| cache.styles(world, id, stylesheets, &pseudo));
    let Some(transitions) = pseudo.transitions else {
        return cached.unwrap_or_else(|| Rc::new(cascade(world, id, stylesheets, pseudo)));
    };
    let mut resolved = match cached {
        Some(styles) => Rc::unwrap_or_clone(styles),
        None => cascade(world, id, stylesheets, pseudo),
    };
    transitions.borrow_mut().apply(world, id, &mut resolved);
    let resolved = Rc::new(resolved);
    if let Some(cache) = pseudo.styles {
        cache.keep_eased(id, transitions, &pseudo, &resolved);
    }
    resolved
}

/// `id`'s cascaded styles, matched from scratch: its own declarations,
/// and the custom properties it inherits from its ancestors.
fn cascade(
    world: &World,
    id: InstanceId,
    stylesheets: &[Stylesheet],
    pseudo: PseudoClasses<'_>,
) -> ResolvedStyles {
    let mut ancestors = Vec::new();
    let mut current = world.get(id).and_then(Instance::parent);
    while let Some(instance_id) = current {
        let Some(instance) = world.get(instance_id) else {
            break;
        };
        ancestors.push(instance_id);
        current = instance.parent();
    }
    let mut resolved = ResolvedStyles::default();
    for &instance_id in ancestors.iter().rev() {
        let declarations = matching_declarations(world, instance_id, stylesheets, pseudo);
        for (name, winner) in declarations {
            if name.starts_with("--") {
                let value = winner.declaration.value.clone();
                Rc::make_mut(&mut resolved.variables).insert(name.to_owned(), value);
            }
        }
    }
    if world.get(id).is_some() {
        add_own_declarations(world, id, stylesheets, pseudo, &mut resolved);
    }
    resolved
}

/// Adds the declarations winning for `id` itself to `resolved`: its
/// values, and its custom properties over the inherited ones.
fn add_own_declarations(
    world: &World,
    id: InstanceId,
    stylesheets: &[Stylesheet],
    pseudo: PseudoClasses<'_>,
    resolved: &mut ResolvedStyles,
) {
    for (name, winner) in matching_declarations(world, id, stylesheets, pseudo) {
        let value = winner.declaration.value.clone();
        if name.starts_with("--") {
            Rc::make_mut(&mut resolved.variables).insert(name.to_owned(), value);
        } else {
            resolved.values.insert(name.to_owned(), value);
        }
    }
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
            Rc::make_mut(&mut resolved.variables)
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
            Rc::make_mut(&mut resolved.variables)
        } else {
            &mut resolved.values
        };
        target.insert(name.to_owned(), winner.declaration.value.clone());
    }
    resolved
}

/// The styles of one of the engine's own widgets, or a part of it: rules
/// that are only its type name (`DialogueBox`, `DialogueBox::option`),
/// with `:focus` matching when `focus` is (the chosen option). Classes,
/// ids and descendant selectors don't apply to it.
pub(crate) fn resolve_widget_styles(
    stylesheets: &[Stylesheet],
    widget: Widget,
    part: Option<PseudoElement>,
    focus: bool,
) -> ResolvedStyles {
    let winners = winning_declarations(stylesheets, |selector| {
        let [only] = selector.parts.as_slice() else {
            return false;
        };
        only.widget == Some(widget)
            && only.element == part
            && only.id.is_none()
            && only.classes.is_empty()
            && !only.hovered
            && !only.active
            && !only.disabled
            && !only.checked
            && (!only.focus || focus)
    });
    let mut resolved = ResolvedStyles::default();
    for (name, winner) in winners {
        let target = if name.starts_with("--") {
            Rc::make_mut(&mut resolved.variables)
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

/// One look in a stylesheet, to copy onto an element: a simple selector
/// (`.card`, `#play`, `TextButton`) and its states and parts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuleSet {
    /// The selector without states or parts, such as `.card`.
    pub selector: String,
    /// What follows `selector` in each of its rules (`""`, `:hover`,
    /// `::thumb`) with its declarations, in the order first written; a
    /// later declaration of a property replaces the earlier one in place.
    pub rules: Vec<(String, Vec<(String, String)>)>,
}

/// Every [`RuleSet`] in `stylesheet`, in the order first written. Rules
/// with descendant or several selectors (`.menu .title`, `a, b`) style
/// elements by where they are, so they aren't a look of one element and
/// are left out.
pub fn rule_sets(stylesheet: &Stylesheet) -> Vec<RuleSet> {
    let mut sets: Vec<RuleSet> = Vec::new();
    for rule in &stylesheet.rules {
        let selector = rule.selector_text.trim();
        if selector.is_empty() || selector.contains([' ', ',', '>', '+', '~']) {
            continue;
        }
        let (base, suffix) = selector.split_at(selector.find(':').unwrap_or(selector.len()));
        if base.is_empty() {
            continue;
        }
        let set = match sets.iter().position(|set| set.selector == base) {
            Some(index) => &mut sets[index],
            None => {
                sets.push(RuleSet {
                    selector: base.to_owned(),
                    rules: Vec::new(),
                });
                sets.last_mut().expect("just pushed")
            }
        };
        let index = match set.rules.iter().position(|(known, _)| known == suffix) {
            Some(index) => index,
            None => {
                set.rules.push((suffix.to_owned(), Vec::new()));
                set.rules.len() - 1
            }
        };
        let declarations = &mut set.rules[index].1;
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
    sets.retain(|set| {
        set.rules
            .iter()
            .any(|(_, declarations)| !declarations.is_empty())
    });
    sets
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
) -> FxHashMap<&'a str, Winner<'a>> {
    winning_declarations(stylesheets, |selector| {
        selector_matches(world, id, selector, pseudo)
    })
}

/// The winning declaration of each property among the rules whose
/// selector `matches`.
fn winning_declarations<'a>(
    stylesheets: &'a [Stylesheet],
    matches: impl Fn(&Selector) -> bool,
) -> FxHashMap<&'a str, Winner<'a>> {
    let mut winners: FxHashMap<&str, Winner> = FxHashMap::default();
    let mut order = 0;
    for stylesheet in stylesheets {
        for rule in &stylesheet.rules {
            // Each selector is matched once per rule, not once per
            // declaration (M14). Of a rule's selectors that match, the most
            // specific one decides how its declarations compete.
            let specificity = if rule.declarations.is_empty() {
                None
            } else {
                rule.selectors
                    .iter()
                    .filter(|selector| matches(selector))
                    .map(|selector| selector.specificity)
                    .max()
            };
            let Some(specificity) = specificity else {
                order += rule.declarations.len();
                continue;
            };
            for declaration in &rule.declarations {
                let replace = winners
                    .get(declaration.name.as_str())
                    .is_none_or(|old| (specificity, order) > (old.specificity, old.order));
                if replace {
                    winners.insert(
                        &declaration.name,
                        Winner {
                            specificity,
                            order,
                            path: &stylesheet.path,
                            selector: &rule.selector_text,
                            declaration,
                        },
                    );
                }
                order += 1;
            }
        }
    }
    winners
}

/// Whether a `:focus` rule styles `id` itself, in `pseudo`'s states. When
/// none does, a focused element gets the default focus ring.
pub(crate) fn focus_rule_applies(
    world: &World,
    id: InstanceId,
    stylesheets: &[Stylesheet],
    pseudo: PseudoClasses<'_>,
) -> bool {
    stylesheets
        .iter()
        .flat_map(|stylesheet| &stylesheet.rules)
        .filter(|rule| !rule.declarations.is_empty())
        .flat_map(|rule| &rule.selectors)
        .filter(|selector| selector.parts.last().is_some_and(|last| last.focus))
        .any(|selector| selector_matches(world, id, selector, pseudo))
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
        || selector.widget.is_some()
        || selector
            .class
            .is_some_and(|class| class != instance.class())
    {
        return false;
    }
    // The cheap tests first, and each state looked up only when the
    // selector asks for it (M14): most selectors fail on their class, id
    // or classes.
    if selector.id.as_deref().is_some_and(|id| {
        instance
            .property("Id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            != id
    }) {
        return false;
    }
    if !selector.classes.is_empty() {
        let classes = instance
            .property("Class")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let has = |class: &String| {
            classes
                .split_whitespace()
                .any(|candidate| candidate == class)
        };
        if !selector.classes.iter().all(has) {
            return false;
        }
    }
    if !(selector.hovered
        || selector.active
        || selector.disabled
        || selector.checked
        || selector.focus)
    {
        return true;
    }
    let interactable = || instance.property("Interactable").and_then(Value::as_bool) != Some(false);
    let forced = pseudo
        .forced
        .filter(|(forced, _)| *forced == id)
        .map(|(_, state)| state);
    // A pressed button is hovered too, as in CSS.
    let (hovered, active) = match forced {
        Some(UiState::Normal | UiState::Disabled) => (false, false),
        Some(UiState::Hover) => (true, false),
        Some(UiState::Pressed) => (true, true),
        None => (pseudo.hovered == Some(id), pseudo.active == Some(id)),
    };
    let disabled = || match forced {
        Some(UiState::Normal) | None => !interactable(),
        Some(UiState::Hover | UiState::Pressed) => false,
        Some(UiState::Disabled) => true,
    };
    // A Toggle that's on, or a Tool in its Inventory's hand (the hotbar's
    // slot, Phase 45).
    let checked = || match instance.class() {
        ClassName::Toggle => instance.property("Value").and_then(Value::as_bool) == Some(true),
        ClassName::Tool => axcenity_core::tool::inventory_holding(world, id)
            .is_some_and(|inventory| axcenity_core::tool::equipped(world, inventory) == Some(id)),
        _ => false,
    };
    let focused = pseudo.focused == Some(id) || pseudo.selected == Some(id);
    !((selector.hovered && !hovered)
        || (selector.active && !active)
        || (selector.focus && !focused)
        || (selector.disabled && !disabled())
        || (selector.checked && !checked()))
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
                    // It ends the selector, but a widget's part may have
                    // a pseudo-class after it (`DialogueBox::option:focus`).
                    let widget_state = part.widget.is_some() && bytes.get(cursor) == Some(&b':');
                    if (cursor < raw.len() && !widget_state) || part.element.is_some() {
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
                        "key" => PseudoElement::Key,
                        "ring" => PseudoElement::Ring,
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
                        "focus" | "focus-visible" if start != cursor && !part.focus => {
                            part.focus = true;
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
                    } else if let Some(widget) = Widget::from_name(name) {
                        if part.widget.replace(widget).is_some() || part.class.is_some() {
                            return None;
                        }
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

pub(crate) fn resolve_variables(value: &str, variables: &StyleMap) -> Option<String> {
    resolve_variables_inner(value, variables, &mut Vec::new())
}

fn resolve_variables_inner(
    value: &str,
    variables: &StyleMap,
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

    /// Every element's styles from a [`StyleCache`] kept across changing
    /// pointer, press, focus and forced states (as one frame's layout
    /// passes keep it) are the ones matched from scratch.
    #[test]
    fn cached_styles_match_the_cascade_as_states_change() {
        let mut world = World::new();
        let add = |world: &mut World, parent, class, props: &[(&str, &str)]| {
            let id = world.create(class).unwrap();
            world.set_parent(id, Some(parent)).unwrap();
            for (name, value) in props {
                world
                    .set_property(id, name, Value::String((*value).to_owned()))
                    .unwrap();
            }
            id
        };
        let workspace = world.workspace();
        let gui = add(&mut world, workspace, ClassName::ScreenGui, &[]);
        let root = add(
            &mut world,
            gui,
            ClassName::Frame,
            &[("Id", "root"), ("Class", "panel")],
        );
        let mut slots = Vec::new();
        let mut buttons = Vec::new();
        for index in 0..3 {
            let class = if index == 1 { "slot rare" } else { "slot" };
            let slot = add(&mut world, root, ClassName::Frame, &[("Class", class)]);
            add(&mut world, slot, ClassName::TextLabel, &[("Class", "name")]);
            buttons.push(add(
                &mut world,
                slot,
                ClassName::TextButton,
                &[("Class", "use")],
            ));
            slots.push(slot);
        }
        let toggle = add(&mut world, root, ClassName::Toggle, &[]);
        world
            .set_property(toggle, "Value", Value::Bool(true))
            .unwrap();
        world
            .set_property(buttons[2], "Interactable", Value::Bool(false))
            .unwrap();
        let (sheet, warnings) = parse_stylesheet(
            "test.css",
            "#root { --c: red; --gap: 4px; padding: var(--gap); }
             .panel .slot { --c: green; background-color: var(--c); }
             .slot:hover { --c: blue; border-color: var(--c); }
             .slot.rare:hover TextLabel { color: var(--c); }
             .slot:hover TextLabel.name, #root TextButton:active { font-size: 20px; }
             .use:focus { --c: yellow; color: var(--c); }
             TextButton:disabled { opacity: 0.5; }
             Toggle:checked { background-color: var(--c); }
             Frame, TextLabel { --depth: 1; }",
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        let sheets = [sheet];
        let mut ids = vec![gui, root, toggle];
        for &slot in &slots {
            ids.push(slot);
            ids.extend(world.get(slot).unwrap().children().iter().copied());
        }
        let states = [
            PseudoClasses::default(),
            PseudoClasses {
                hovered: Some(slots[0]),
                ..PseudoClasses::default()
            },
            PseudoClasses {
                hovered: Some(slots[1]),
                active: Some(buttons[1]),
                ..PseudoClasses::default()
            },
            PseudoClasses {
                hovered: Some(buttons[1]),
                active: Some(buttons[1]),
                focused: Some(buttons[0]),
                ..PseudoClasses::default()
            },
            PseudoClasses {
                selected: Some(buttons[2]),
                forced: Some((slots[2], UiState::Hover)),
                ..PseudoClasses::default()
            },
            PseudoClasses::default(),
        ];
        let cache = StyleCache::default();
        for (index, state) in states.into_iter().enumerate() {
            cache.begin(&world, &state);
            let cached = PseudoClasses {
                styles: Some(&cache),
                ..state
            };
            // Children before parents on odd passes, so an element's
            // ancestors are matched on its account first.
            let order: Vec<_> = if index % 2 == 1 {
                ids.iter().rev().copied().collect()
            } else {
                ids.clone()
            };
            for id in order {
                let expected = cascade(&world, id, &sheets, state);
                let got = resolve_styles(&world, id, &sheets, cached);
                assert_eq!(got.values, expected.values, "state {index}, {id:?}");
                assert_eq!(*got.variables, *expected.variables, "state {index}, {id:?}");
            }
        }
    }

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
        let variables = StyleMap::from_iter([
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
