//! CSS `transition`: when an element's styles change (it's hovered,
//! pressed, disabled, or a script changes its class), the properties its
//! `transition` names ease from the old value to the new one instead of
//! snapping.
//!
//! Values are interpolated as text: colours are written as `rgba(…)` and
//! then every number in the old value is blended with the one in the same
//! place in the new value. Two values that aren't the same shape (`none` to
//! a gradient, `auto` to `40px`) snap. `none` for `transform` blends from
//! `scale(1)` and `translate(0)`, and for shadows from a clear shadow of
//! zero size. The timing functions are CSS's, drawn with the same Bezier
//! curve the animation keys use (`anim::cubic_bezier`).

use std::collections::HashMap;

use axcenity_core::{InstanceId, UDim2, Value, World};

use crate::css::{parse_color, split_css_args};
use crate::style::{ResolvedStyles, resolve_variables};

/// The properties a transition can animate.
const ANIMATED: [&str; 9] = [
    "background-color",
    "color",
    "border-color",
    "opacity",
    "width",
    "height",
    "transform",
    "box-shadow",
    "text-shadow",
];

/// The running transitions of every element, keyed by element and property.
/// Hosts keep one (inside `UiInteraction`) for as long as the game runs.
#[derive(Debug, Default)]
pub(crate) struct Transitions {
    /// Seconds since the first frame.
    now: f64,
    tracks: HashMap<(InstanceId, &'static str), Track>,
}

/// One property of one element: the value its styles ask for, and the
/// transition toward it, if one is running.
#[derive(Debug)]
struct Track {
    target: String,
    /// Whether `target` came from CSS. A change between two values that
    /// both came from the element's own properties (a script setting
    /// `BackgroundColor3`) isn't a style change, and doesn't transition.
    from_css: bool,
    run: Option<Run>,
}

#[derive(Debug)]
struct Run {
    from: String,
    to: String,
    start: f64,
    delay: f64,
    duration: f64,
    timing: Timing,
}

impl Run {
    fn value(&self, now: f64) -> String {
        let t = ((now - self.start - self.delay) / self.duration).clamp(0.0, 1.0);
        interpolate(&self.from, &self.to, self.timing.ease(t as f32))
            .unwrap_or_else(|| self.to.clone())
    }

    fn done(&self, now: f64) -> bool {
        now >= self.start + self.delay + self.duration
    }
}

/// A CSS timing function.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Timing {
    Linear,
    /// `cubic-bezier(x1, y1, x2, y2)`; `ease` and the rest are named ones.
    Bezier([f32; 4]),
}

impl Timing {
    fn parse(name: &str) -> Option<Timing> {
        Some(match name {
            "linear" => Timing::Linear,
            "ease" => Timing::Bezier([0.25, 0.1, 0.25, 1.0]),
            "ease-in" => Timing::Bezier([0.42, 0.0, 1.0, 1.0]),
            "ease-out" => Timing::Bezier([0.0, 0.0, 0.58, 1.0]),
            "ease-in-out" => Timing::Bezier([0.42, 0.0, 0.58, 1.0]),
            _ => {
                let inner = name.strip_prefix("cubic-bezier(")?.strip_suffix(')')?;
                let numbers: Vec<f32> = inner
                    .split(',')
                    .map(|n| n.trim().parse::<f32>().ok().filter(|n| n.is_finite()))
                    .collect::<Option<_>>()?;
                let [x1, y1, x2, y2] = numbers.try_into().ok()?;
                Timing::Bezier([x1, y1, x2, y2])
            }
        })
    }

    pub(crate) fn ease(self, t: f32) -> f32 {
        match self {
            Timing::Linear => t.clamp(0.0, 1.0),
            Timing::Bezier(handles) => axcenity_anim::cubic_bezier(handles, t),
        }
    }
}

/// One item of a `transition` list: which property (or `all`), for how
/// long, eased how, and after how long a wait.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TransitionSpec {
    pub(crate) property: String,
    pub(crate) duration: f64,
    pub(crate) timing: Timing,
    pub(crate) delay: f64,
}

/// `transition: <property> <duration> [<timing>] [<delay>], …`. The first
/// time is the duration and the second the delay; a missing property is
/// `all` and a missing timing `ease`. `none` is no transitions.
pub(crate) fn parse_transition(value: &str) -> Vec<TransitionSpec> {
    split_css_args(value)
        .into_iter()
        .filter_map(|item| {
            let mut spec = TransitionSpec {
                property: "all".to_owned(),
                duration: 0.0,
                timing: Timing::parse("ease").expect("ease is a timing function"),
                delay: 0.0,
            };
            let mut times = 0;
            for word in words(item) {
                let word = word.to_ascii_lowercase();
                if let Some(seconds) = seconds(&word) {
                    match times {
                        0 => spec.duration = seconds.max(0.0),
                        _ => spec.delay = seconds,
                    }
                    times += 1;
                } else if let Some(timing) = Timing::parse(&word) {
                    spec.timing = timing;
                } else if word == "none" {
                    return None;
                } else {
                    spec.property = word;
                }
            }
            Some(spec)
        })
        .collect()
}

/// `0.15s` or `150ms`, in seconds.
fn seconds(word: &str) -> Option<f64> {
    let number = |text: &str| text.parse::<f64>().ok().filter(|n| n.is_finite());
    if let Some(ms) = word.strip_suffix("ms") {
        number(ms).map(|ms| ms / 1000.0)
    } else {
        word.strip_suffix('s').and_then(number)
    }
}

/// `value` split at spaces outside brackets, so `cubic-bezier(0, 0, 1, 1)`
/// is one word.
fn words(value: &str) -> Vec<&str> {
    let mut words = Vec::new();
    let mut depth = 0_i32;
    let mut start = None;
    for (index, c) in value.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        if c.is_whitespace() && depth == 0 {
            if let Some(from) = start.take() {
                words.push(&value[from..index]);
            }
        } else if start.is_none() {
            start = Some(index);
        }
    }
    if let Some(from) = start {
        words.push(&value[from..]);
    }
    words
}

impl Transitions {
    /// Moves the clock on by `seconds` and forgets elements that are gone.
    pub(crate) fn advance(&mut self, world: &World, seconds: f64) {
        self.now += seconds.max(0.0);
        self.tracks.retain(|(id, _), _| world.contains(*id));
    }

    /// True while any transition is still under way, so the host keeps
    /// drawing frames.
    pub(crate) fn running(&self) -> bool {
        self.tracks
            .values()
            .any(|track| track.run.as_ref().is_some_and(|run| !run.done(self.now)))
    }

    /// Eases `id`'s `resolved` styles toward what they've just become.
    /// Calling it again with the same styles at the same time changes
    /// nothing, so layout can resolve an element's styles more than once.
    pub(crate) fn apply(&mut self, world: &World, id: InstanceId, resolved: &mut ResolvedStyles) {
        let specs = resolved
            .values
            .get("transition")
            .and_then(|value| resolve_variables(value, &resolved.variables))
            .map(|value| parse_transition(&value))
            .unwrap_or_default();
        let now = self.now;
        for property in ANIMATED {
            let css = resolved
                .values
                .get(property)
                .and_then(|value| resolve_variables(value, &resolved.variables));
            let from_css = css.is_some();
            let Some(target) = css.or_else(|| default_value(world, id, property)) else {
                self.tracks.remove(&(id, property));
                continue;
            };
            let Some(track) = self.tracks.get_mut(&(id, property)) else {
                self.tracks.insert(
                    (id, property),
                    Track {
                        target,
                        from_css,
                        run: None,
                    },
                );
                continue;
            };
            if track.target != target {
                let current = match &track.run {
                    Some(run) if !run.done(now) => run.value(now),
                    _ => track.target.clone(),
                };
                let spec = specs
                    .iter()
                    .rev()
                    .find(|spec| spec.property == property || spec.property == "all")
                    .filter(|spec| spec.duration > 0.0 && (from_css || track.from_css));
                track.run = spec
                    .filter(|_| interpolate(&current, &target, 0.5).is_some())
                    .map(|spec| Run {
                        from: current,
                        to: target.clone(),
                        start: now,
                        delay: spec.delay,
                        duration: spec.duration,
                        timing: spec.timing,
                    });
                track.target = target;
                track.from_css = from_css;
            }
            if let Some(run) = &track.run {
                if run.done(now) {
                    track.run = None;
                } else {
                    resolved.values.insert(property.to_owned(), run.value(now));
                }
            }
        }
    }
}

/// What `property` is when no CSS sets it, as CSS text: the element's own
/// colours, its pixel size, or the property's initial value. `None` when
/// it can't be written as one (a size that's a share of its parent).
fn default_value(world: &World, id: InstanceId, property: &str) -> Option<String> {
    let instance = world.get(id)?;
    let color = |name: &str, alpha: f64| {
        let color = instance.property(name).and_then(Value::as_color3)?;
        Some(rgba([color.r, color.g, color.b], alpha as f32))
    };
    let pixels = |pick: fn(&UDim2) -> axcenity_core::UDim| {
        let size = instance.property("Size").and_then(Value::as_udim2)?;
        let udim = pick(&size);
        (udim.scale == 0.0).then(|| format!("{}px", number(udim.offset.max(0.0))))
    };
    match property {
        "background-color" => {
            let transparency = instance
                .property("BackgroundTransparency")
                .and_then(Value::as_number)
                .unwrap_or(0.0)
                .clamp(0.0, 1.0);
            color("BackgroundColor3", 1.0 - transparency)
        }
        "color" => color("TextColor3", 1.0),
        "opacity" => Some("1".to_owned()),
        "width" => pixels(|size| size.x),
        "height" => pixels(|size| size.y),
        "transform" | "box-shadow" | "text-shadow" => Some("none".to_owned()),
        _ => None,
    }
}

fn rgba([r, g, b]: [f32; 3], alpha: f32) -> String {
    format!(
        "rgba({}, {}, {}, {})",
        number(r * 255.0),
        number(g * 255.0),
        number(b * 255.0),
        number(alpha)
    )
}

/// A number with up to four decimals, without trailing zeros.
fn number(value: f32) -> String {
    let text = format!("{value:.4}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" { "0" } else { text }.to_owned()
}

/// `from` blended `t` of the way to `to`, or `None` when they aren't the
/// same shape. `t` may go past 0 or 1 (a timing function that overshoots).
pub(crate) fn interpolate(from: &str, to: &str, t: f32) -> Option<String> {
    let from = with_rgba_colors(from);
    let to = with_rgba_colors(to);
    let (from, to) = match (is_none(&from), is_none(&to)) {
        (true, true) => return Some(to),
        (true, false) => (neutral(&to), to),
        (false, true) => {
            let neutral = neutral(&from);
            (from, neutral)
        }
        (false, false) => (from, to),
    };
    let from = tokens(&from);
    let to = tokens(&to);
    if from.len() != to.len() {
        return None;
    }
    let mut out = String::new();
    for (a, b) in from.iter().zip(&to) {
        match (a, b) {
            (Token::Number(a), Token::Number(b)) => out.push_str(&number(a + (b - a) * t)),
            (Token::Text(a), Token::Text(b)) if a == b => out.push_str(a),
            _ => return None,
        }
    }
    Some(out)
}

fn is_none(value: &str) -> bool {
    value.trim().eq_ignore_ascii_case("none")
}

/// What `value` blends from when the other end is `none`: each scale is 1
/// and every other number (moves, shadow offsets and blurs, colour
/// channels and alpha) is 0, so a shadow fades in from nothing.
fn neutral(value: &str) -> String {
    let mut out = String::new();
    let mut functions: Vec<String> = Vec::new();
    let mut name = String::new();
    for token in tokens(value) {
        match token {
            Token::Number(_) => {
                let scale = functions.last().is_some_and(|f| f.starts_with("scale"));
                out.push_str(if scale { "1" } else { "0" });
            }
            Token::Text(text) => {
                for c in text.chars() {
                    match c {
                        '(' => functions.push(std::mem::take(&mut name)),
                        ')' => {
                            functions.pop();
                        }
                        c if c.is_ascii_alphabetic() || c == '-' => name.push(c),
                        _ => name.clear(),
                    }
                }
                out.push_str(&text);
            }
        }
    }
    out
}

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Number(f32),
    Text(String),
}

/// `value` as numbers and the text between them. A number is only one
/// where a word can't go on: the `2` of `h2` stays text.
fn tokens(value: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut text = String::new();
    let chars: Vec<char> = value.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        let c = chars[index];
        let previous = index.checked_sub(1).map(|i| chars[i]);
        let after_word = previous.is_some_and(|p| p.is_ascii_alphanumeric() || p == '_');
        let digit_at = |i: usize| chars.get(i).is_some_and(|c| c.is_ascii_digit());
        let starts = !after_word
            && (c.is_ascii_digit()
                || (c == '.' && digit_at(index + 1))
                || (c == '-' && (digit_at(index + 1) || (chars.get(index + 1) == Some(&'.')))));
        if !starts {
            text.push(c);
            index += 1;
            continue;
        }
        let start = index;
        index += 1;
        while index < chars.len() && (chars[index].is_ascii_digit() || chars[index] == '.') {
            index += 1;
        }
        let number: String = chars[start..index].iter().collect();
        match number.parse::<f32>() {
            Ok(number) if number.is_finite() => {
                if !text.is_empty() {
                    tokens.push(Token::Text(std::mem::take(&mut text)));
                }
                tokens.push(Token::Number(number));
            }
            _ => text.push_str(&number),
        }
    }
    if !text.is_empty() {
        tokens.push(Token::Text(text));
    }
    tokens
}

/// `value` with each colour in it (`#fff`, `rgb(…)`, `red`) written as
/// `rgba(r, g, b, a)`, so colours written differently still blend.
fn with_rgba_colors(value: &str) -> String {
    let mut out = String::new();
    let mut rest = value.trim();
    while let Some(c) = rest.chars().next() {
        let length = if c == '#' {
            1 + rest[1..]
                .chars()
                .take_while(char::is_ascii_hexdigit)
                .count()
        } else if rest.starts_with("rgb") {
            rest.find(')').map_or(rest.len(), |close| close + 1)
        } else if c.is_ascii_alphabetic() {
            rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
                .unwrap_or(rest.len())
        } else {
            c.len_utf8()
        };
        let word = &rest[..length];
        let call = rest[length..].starts_with('(');
        match parse_color(word).filter(|_| !call && (c == '#' || c.is_ascii_alphabetic())) {
            Some((color, alpha)) => out.push_str(&rgba([color.r, color.g, color.b], alpha)),
            None => out.push_str(word),
        }
        rest = &rest[length..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_lists_read_property_duration_timing_and_delay() {
        let specs = parse_transition("all 0.15s ease, background-color 300ms linear 0.1s");
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0].property, "all");
        assert_eq!(specs[0].duration, 0.15);
        assert_eq!(specs[0].timing, Timing::Bezier([0.25, 0.1, 0.25, 1.0]));
        assert_eq!(specs[1].property, "background-color");
        assert!((specs[1].duration - 0.3).abs() < 1e-9);
        assert_eq!(specs[1].timing, Timing::Linear);
        assert_eq!(specs[1].delay, 0.1);
        let bezier = parse_transition("transform .2s cubic-bezier(0.3, 0, 0.2, 1)");
        assert_eq!(bezier[0].timing, Timing::Bezier([0.3, 0.0, 0.2, 1.0]));
        assert!(parse_transition("none").is_empty());
    }

    #[test]
    fn colours_numbers_transforms_and_shadows_blend_and_other_shapes_snap() {
        assert_eq!(
            interpolate("#000000", "rgb(255, 255, 255)", 0.5).as_deref(),
            Some("rgba(127.5, 127.5, 127.5, 1)")
        );
        assert_eq!(interpolate("0.2", "1", 0.5).as_deref(), Some("0.6"));
        assert_eq!(
            interpolate("none", "scale(0.9) translate(10px, -4px)", 0.5).as_deref(),
            Some("scale(0.95) translate(5px, -2px)")
        );
        assert_eq!(
            interpolate("0 2px 8px black", "none", 0.5).as_deref(),
            Some("0 1px 4px rgba(0, 0, 0, 0.5)")
        );
        assert_eq!(interpolate("auto", "40px", 0.5), None);
        assert_eq!(
            interpolate("ease-in 2s", "ease-in 4s", 0.5).as_deref(),
            Some("ease-in 3s")
        );
    }
}
