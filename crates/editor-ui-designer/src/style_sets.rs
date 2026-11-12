//! **Styles** in the inspector: ready-made looks for the selected element,
//! and looks imported from CSS, each copied into the element's own rules in
//! one step.
//!
//! A style set is a simple selector's rules: its own (`.set`) and its
//! states' and parts' (`.set:hover`, `.set::thumb`, `.set:checked::track`).
//! Applying one writes each declaration into the element's own rule for
//! that state or part (`#play`, `#play:hover`), changed in place where it
//! has one, so the element stays as editable as before: the Style (CSS)
//! section, the states editor and the text toolbar show and change the
//! copied values. The built-in sets are CSS below; each kind's sets name
//! the same properties, so picking another one replaces all of the last.
//! **Import** lists the project's stylesheets' rule sets (`.card` with its
//! `:hover`), or reads any `.css` file on this computer.

use axcenity_core::{ClassName, InstanceId, World};
use axcenity_ui::{RuleSet, Stylesheet};
use eframe::egui;

use super::{DesignerAction, DesignerScene};
use crate::theme::{self, icons};
use crate::widgets;

/// What a built-in set is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Button,
    Panel,
    Text,
    TextBox,
    Slider,
    Toggle,
    Progress,
    Dropdown,
}

impl Kind {
    /// The kind of built-in sets an element of `class` gets.
    pub fn of(class: ClassName) -> Option<Kind> {
        Some(match class {
            ClassName::TextButton => Kind::Button,
            ClassName::Frame
            | ClassName::ScrollingFrame
            | ClassName::Table
            | ClassName::ImageLabel => Kind::Panel,
            ClassName::TextLabel => Kind::Text,
            ClassName::TextBox => Kind::TextBox,
            ClassName::Slider => Kind::Slider,
            ClassName::Toggle => Kind::Toggle,
            ClassName::ProgressBar => Kind::Progress,
            ClassName::Dropdown => Kind::Dropdown,
            _ => return None,
        })
    }
}

/// A built-in style set: its name, what it's like, and its CSS, written
/// for the selector `.set`.
pub struct BuiltIn {
    pub kind: Kind,
    pub name: &'static str,
    pub tip: &'static str,
    pub css: &'static str,
}

/// Every built-in set, by kind. Within a kind each names the same
/// properties in the same states, so one replaces another entirely.
pub const BUILT_IN: &[BuiltIn] = &[
    // Buttons.
    BuiltIn {
        kind: Kind::Button,
        name: "Primary",
        tip: "A solid blue call to action that lifts on hover",
        css: ".set { background-color: #3b82f6; background-image: none; color: #ffffff; border: none; border-radius: 8px; box-shadow: 0px 2px 6px rgba(0, 0, 0, 0.25); font-weight: 600; text-shadow: none; transition: all 0.15s ease; }
.set:hover { background-color: #2563eb; background-image: none; box-shadow: 0px 4px 14px rgba(37, 99, 235, 0.45); transform: scale(1.03); }
.set:active { background-color: #1d4ed8; background-image: none; box-shadow: 0px 1px 3px rgba(0, 0, 0, 0.3); transform: scale(0.97); }
.set:disabled { opacity: 0.5; }",
    },
    BuiltIn {
        kind: Kind::Button,
        name: "Secondary",
        tip: "A quiet grey button for the less important choice",
        css: ".set { background-color: #374151; background-image: none; color: #f9fafb; border: 1px solid #4b5563; border-radius: 8px; box-shadow: none; font-weight: 500; text-shadow: none; transition: all 0.15s ease; }
.set:hover { background-color: #4b5563; background-image: none; box-shadow: none; transform: scale(1.02); }
.set:active { background-color: #1f2937; background-image: none; box-shadow: none; transform: scale(0.97); }
.set:disabled { opacity: 0.5; }",
    },
    BuiltIn {
        kind: Kind::Button,
        name: "Outline",
        tip: "See-through with a coloured border, filled faintly on hover",
        css: ".set { background-color: rgba(96, 165, 250, 0); background-image: none; color: #60a5fa; border: 2px solid #60a5fa; border-radius: 8px; box-shadow: none; font-weight: 600; text-shadow: none; transition: all 0.15s ease; }
.set:hover { background-color: rgba(96, 165, 250, 0.15); background-image: none; box-shadow: none; transform: scale(1.02); }
.set:active { background-color: rgba(96, 165, 250, 0.3); background-image: none; box-shadow: none; transform: scale(0.97); }
.set:disabled { opacity: 0.4; }",
    },
    BuiltIn {
        kind: Kind::Button,
        name: "Pill",
        tip: "A round-ended green button",
        css: ".set { background-color: #10b981; background-image: none; color: #ffffff; border: none; border-radius: 999px; box-shadow: 0px 2px 8px rgba(16, 185, 129, 0.35); font-weight: 700; text-shadow: none; transition: all 0.15s ease; }
.set:hover { background-color: #059669; background-image: none; box-shadow: 0px 4px 14px rgba(16, 185, 129, 0.5); transform: scale(1.04); }
.set:active { background-color: #047857; background-image: none; box-shadow: none; transform: scale(0.96); }
.set:disabled { opacity: 0.5; }",
    },
    BuiltIn {
        kind: Kind::Button,
        name: "Danger",
        tip: "Red, for Quit, Delete and other choices that can't be taken back",
        css: ".set { background-color: #ef4444; background-image: none; color: #ffffff; border: none; border-radius: 8px; box-shadow: 0px 2px 6px rgba(0, 0, 0, 0.25); font-weight: 600; text-shadow: none; transition: all 0.15s ease; }
.set:hover { background-color: #dc2626; background-image: none; box-shadow: 0px 4px 14px rgba(220, 38, 38, 0.45); transform: scale(1.03); }
.set:active { background-color: #b91c1c; background-image: none; box-shadow: none; transform: scale(0.97); }
.set:disabled { opacity: 0.5; }",
    },
    BuiltIn {
        kind: Kind::Button,
        name: "Sunset",
        tip: "An orange to pink gradient with a glow",
        css: ".set { background-color: #f97316; background-image: linear-gradient(to bottom, #fb923c, #db2777); color: #ffffff; border: none; border-radius: 12px; box-shadow: 0px 4px 14px rgba(219, 39, 119, 0.4); font-weight: 700; text-shadow: 0px 1px 2px rgba(0, 0, 0, 0.35); transition: all 0.15s ease; }
.set:hover { background-color: #fb923c; background-image: linear-gradient(to bottom, #fdba74, #ec4899); box-shadow: 0px 6px 20px rgba(236, 72, 153, 0.55); transform: scale(1.04); }
.set:active { background-color: #ea580c; background-image: linear-gradient(to bottom, #ea580c, #be185d); box-shadow: 0px 2px 6px rgba(190, 24, 93, 0.4); transform: scale(0.97); }
.set:disabled { opacity: 0.5; }",
    },
    BuiltIn {
        kind: Kind::Button,
        name: "Glass",
        tip: "Frosted and see-through, for buttons over the game",
        css: ".set { background-color: rgba(255, 255, 255, 0.14); background-image: none; color: #ffffff; border: 1px solid rgba(255, 255, 255, 0.4); border-radius: 12px; box-shadow: 0px 8px 24px rgba(0, 0, 0, 0.25); font-weight: 600; text-shadow: 0px 1px 3px rgba(0, 0, 0, 0.4); transition: all 0.15s ease; }
.set:hover { background-color: rgba(255, 255, 255, 0.24); background-image: none; box-shadow: 0px 10px 28px rgba(0, 0, 0, 0.3); transform: scale(1.02); }
.set:active { background-color: rgba(255, 255, 255, 0.08); background-image: none; box-shadow: 0px 4px 12px rgba(0, 0, 0, 0.2); transform: scale(0.98); }
.set:disabled { opacity: 0.4; }",
    },
    BuiltIn {
        kind: Kind::Button,
        name: "Arcade",
        tip: "Yellow with a hard black outline and shadow, pressed flat",
        css: ".set { background-color: #facc15; background-image: none; color: #1f2937; border: 3px solid #1f2937; border-radius: 0px; box-shadow: 4px 4px 0px #1f2937; font-weight: 800; text-shadow: none; transition: all 0.08s linear; }
.set:hover { background-color: #fde047; background-image: none; box-shadow: 5px 5px 0px #1f2937; transform: translate(-1px, -1px); }
.set:active { background-color: #eab308; background-image: none; box-shadow: 0px 0px 0px #1f2937; transform: translate(4px, 4px); }
.set:disabled { opacity: 0.5; }",
    },
    // Frames, scrolling frames, tables and images.
    BuiltIn {
        kind: Kind::Panel,
        name: "Card",
        tip: "A dark rounded card with a soft shadow",
        css: ".set { background-color: #1f2937; background-image: none; border: 1px solid #374151; border-radius: 12px; box-shadow: 0px 8px 24px rgba(0, 0, 0, 0.35); }",
    },
    BuiltIn {
        kind: Kind::Panel,
        name: "Light card",
        tip: "A white rounded card",
        css: ".set { background-color: #ffffff; background-image: none; border: 1px solid #e5e7eb; border-radius: 12px; box-shadow: 0px 4px 12px rgba(0, 0, 0, 0.12); }",
    },
    BuiltIn {
        kind: Kind::Panel,
        name: "Glass",
        tip: "Frosted and see-through, for HUDs over the game",
        css: ".set { background-color: rgba(255, 255, 255, 0.1); background-image: none; border: 1px solid rgba(255, 255, 255, 0.25); border-radius: 16px; box-shadow: 0px 8px 32px rgba(0, 0, 0, 0.3); }",
    },
    BuiltIn {
        kind: Kind::Panel,
        name: "Night",
        tip: "A deep blue gradient",
        css: ".set { background-color: #111827; background-image: linear-gradient(to bottom, #1e3a8a, #111827); border: none; border-radius: 16px; box-shadow: 0px 10px 30px rgba(0, 0, 0, 0.4); }",
    },
    BuiltIn {
        kind: Kind::Panel,
        name: "Outline",
        tip: "See-through with a blue border",
        css: ".set { background-color: rgba(0, 0, 0, 0); background-image: none; border: 2px solid #60a5fa; border-radius: 8px; box-shadow: none; }",
    },
    BuiltIn {
        kind: Kind::Panel,
        name: "Parchment",
        tip: "Cream paper with a hard dark edge, for quests and dialogue",
        css: ".set { background-color: #fef3c7; background-image: none; border: 3px solid #78350f; border-radius: 4px; box-shadow: 6px 6px 0px rgba(120, 53, 15, 0.6); }",
    },
    BuiltIn {
        kind: Kind::Panel,
        name: "Clear",
        tip: "No background, border or shadow: just holds what's in it",
        css: ".set { background-color: rgba(0, 0, 0, 0); background-image: none; border: none; border-radius: 0px; box-shadow: none; }",
    },
    // Text labels.
    BuiltIn {
        kind: Kind::Text,
        name: "Title",
        tip: "Bold white Montserrat with a drop shadow",
        css: ".set { color: #ffffff; font-family: Montserrat; font-weight: 800; font-style: normal; letter-spacing: 0.5px; text-transform: none; text-shadow: 0px 2px 4px rgba(0, 0, 0, 0.5); -webkit-text-stroke: 0px #000000; }",
    },
    BuiltIn {
        kind: Kind::Text,
        name: "Body",
        tip: "Plain readable Inter",
        css: ".set { color: #e5e7eb; font-family: Inter; font-weight: 400; font-style: normal; letter-spacing: 0px; text-transform: none; text-shadow: none; -webkit-text-stroke: 0px #000000; }",
    },
    BuiltIn {
        kind: Kind::Text,
        name: "Subtle",
        tip: "Grey, for hints and captions",
        css: ".set { color: #9ca3af; font-family: Inter; font-weight: 400; font-style: italic; letter-spacing: 0px; text-transform: none; text-shadow: none; -webkit-text-stroke: 0px #000000; }",
    },
    BuiltIn {
        kind: Kind::Text,
        name: "Caps label",
        tip: "Small spaced-out capitals, for headings over groups",
        css: ".set { color: #93c5fd; font-family: Inter; font-weight: 700; font-style: normal; letter-spacing: 2px; text-transform: uppercase; text-shadow: none; -webkit-text-stroke: 0px #000000; }",
    },
    BuiltIn {
        kind: Kind::Text,
        name: "Score",
        tip: "Chunky white with a thick dark outline, readable over anything",
        css: ".set { color: #ffffff; font-family: Poppins; font-weight: 800; font-style: normal; letter-spacing: 1px; text-transform: none; text-shadow: 0px 3px 0px rgba(17, 24, 39, 0.8); -webkit-text-stroke: 3px #111827; }",
    },
    BuiltIn {
        kind: Kind::Text,
        name: "Neon",
        tip: "Cyan with a glow",
        css: ".set { color: #a5f3fc; font-family: Poppins; font-weight: 700; font-style: normal; letter-spacing: 1px; text-transform: none; text-shadow: 0px 0px 10px #22d3ee; -webkit-text-stroke: 0px #000000; }",
    },
    BuiltIn {
        kind: Kind::Text,
        name: "Terminal",
        tip: "Green JetBrains Mono, for consoles and hackers",
        css: ".set { color: #86efac; font-family: \"JetBrains Mono\"; font-weight: 400; font-style: normal; letter-spacing: 0px; text-transform: none; text-shadow: 0px 0px 6px rgba(34, 197, 94, 0.5); -webkit-text-stroke: 0px #000000; }",
    },
    BuiltIn {
        kind: Kind::Text,
        name: "Storybook",
        tip: "Warm Lora italic, for dialogue and lore",
        css: ".set { color: #fde68a; font-family: Lora; font-weight: 400; font-style: italic; letter-spacing: 0px; text-transform: none; text-shadow: 0px 1px 2px rgba(0, 0, 0, 0.6); -webkit-text-stroke: 0px #000000; }",
    },
    // Text boxes.
    BuiltIn {
        kind: Kind::TextBox,
        name: "Dark field",
        tip: "Dark, with a blue ring while typed in",
        css: ".set { background-color: #111827; color: #f9fafb; border: 1px solid #374151; border-radius: 8px; box-shadow: none; }
.set:hover { border-color: #4b5563; }
.set:focus { border-color: #3b82f6; box-shadow: 0px 0px 0px 3px rgba(59, 130, 246, 0.35); }",
    },
    BuiltIn {
        kind: Kind::TextBox,
        name: "Light field",
        tip: "White, with a blue ring while typed in",
        css: ".set { background-color: #ffffff; color: #111827; border: 1px solid #d1d5db; border-radius: 8px; box-shadow: none; }
.set:hover { border-color: #9ca3af; }
.set:focus { border-color: #3b82f6; box-shadow: 0px 0px 0px 3px rgba(59, 130, 246, 0.3); }",
    },
    BuiltIn {
        kind: Kind::TextBox,
        name: "Search",
        tip: "A round-ended field, for searching and chat",
        css: ".set { background-color: #1f2937; color: #f9fafb; border: 1px solid #4b5563; border-radius: 999px; box-shadow: inset 0px 1px 3px rgba(0, 0, 0, 0.4); }
.set:hover { border-color: #6b7280; }
.set:focus { border-color: #10b981; box-shadow: 0px 0px 0px 3px rgba(16, 185, 129, 0.3); }",
    },
    // Sliders.
    BuiltIn {
        kind: Kind::Slider,
        name: "Blue",
        tip: "A blue fill and a white knob",
        css: ".set::track { background-color: #374151; border-radius: 999px; }
.set::fill { background-color: #3b82f6; background-image: none; border-radius: 999px; }
.set::thumb { background-color: #ffffff; border: 2px solid #3b82f6; border-radius: 999px; box-shadow: 0px 1px 4px rgba(0, 0, 0, 0.4); }",
    },
    BuiltIn {
        kind: Kind::Slider,
        name: "Volume",
        tip: "A green to yellow fill",
        css: ".set::track { background-color: #1f2937; border-radius: 999px; }
.set::fill { background-color: #22c55e; background-image: linear-gradient(to bottom, #a3e635, #16a34a); border-radius: 999px; }
.set::thumb { background-color: #f9fafb; border: 2px solid #16a34a; border-radius: 999px; box-shadow: 0px 1px 4px rgba(0, 0, 0, 0.4); }",
    },
    BuiltIn {
        kind: Kind::Slider,
        name: "Arcade",
        tip: "Square, with a yellow knob",
        css: ".set::track { background-color: #1f2937; border-radius: 0px; }
.set::fill { background-color: #f97316; background-image: none; border-radius: 0px; }
.set::thumb { background-color: #facc15; border: 3px solid #1f2937; border-radius: 0px; box-shadow: 2px 2px 0px #1f2937; }",
    },
    // Toggles.
    BuiltIn {
        kind: Kind::Toggle,
        name: "Blue",
        tip: "Grey off, blue on",
        css: ".set::track { background-color: #4b5563; }
.set:checked::track { background-color: #3b82f6; }
.set::thumb { background-color: #ffffff; }
.set::check { background-color: #ffffff; }",
    },
    BuiltIn {
        kind: Kind::Toggle,
        name: "Green",
        tip: "Grey off, green on",
        css: ".set::track { background-color: #4b5563; }
.set:checked::track { background-color: #10b981; }
.set::thumb { background-color: #ffffff; }
.set::check { background-color: #ffffff; }",
    },
    BuiltIn {
        kind: Kind::Toggle,
        name: "Night",
        tip: "Dark off, purple on, with a pale knob",
        css: ".set::track { background-color: #1f2937; }
.set:checked::track { background-color: #8b5cf6; }
.set::thumb { background-color: #e9d5ff; }
.set::check { background-color: #e9d5ff; }",
    },
    // Progress bars.
    BuiltIn {
        kind: Kind::Progress,
        name: "Health",
        tip: "A green bar in a dark rounded frame",
        css: ".set { background-color: #1f2937; border: 2px solid #111827; border-radius: 999px; }
.set::fill { background-color: #22c55e; background-image: linear-gradient(to bottom, #4ade80, #16a34a); border-radius: 999px; }",
    },
    BuiltIn {
        kind: Kind::Progress,
        name: "Mana",
        tip: "A blue bar in a dark rounded frame",
        css: ".set { background-color: #1f2937; border: 2px solid #111827; border-radius: 999px; }
.set::fill { background-color: #3b82f6; background-image: linear-gradient(to bottom, #60a5fa, #1d4ed8); border-radius: 999px; }",
    },
    BuiltIn {
        kind: Kind::Progress,
        name: "Experience",
        tip: "A thin gold bar",
        css: ".set { background-color: #292524; border: 1px solid #78716c; border-radius: 2px; }
.set::fill { background-color: #f59e0b; background-image: linear-gradient(to bottom, #fcd34d, #d97706); border-radius: 2px; }",
    },
    BuiltIn {
        kind: Kind::Progress,
        name: "Loading",
        tip: "A flat purple bar",
        css: ".set { background-color: #e5e7eb; border: none; border-radius: 4px; }
.set::fill { background-color: #8b5cf6; background-image: none; border-radius: 4px; }",
    },
    // Dropdowns.
    BuiltIn {
        kind: Kind::Dropdown,
        name: "Dark",
        tip: "Dark, with a dark list",
        css: ".set { background-color: #1f2937; color: #f9fafb; border: 1px solid #4b5563; border-radius: 8px; }
.set::list { background-color: #111827; border: 1px solid #374151; border-radius: 8px; box-shadow: 0px 8px 24px rgba(0, 0, 0, 0.45); }",
    },
    BuiltIn {
        kind: Kind::Dropdown,
        name: "Light",
        tip: "White, with a white list",
        css: ".set { background-color: #ffffff; color: #111827; border: 1px solid #d1d5db; border-radius: 8px; }
.set::list { background-color: #ffffff; border: 1px solid #d1d5db; border-radius: 8px; box-shadow: 0px 8px 24px rgba(0, 0, 0, 0.18); }",
    },
];

/// The selector built-in sets are written for.
const BUILT_IN_SELECTOR: &str = ".set";

/// `built_in`'s rules, read from its CSS.
pub fn built_in_rules(built_in: &BuiltIn) -> RuleSet {
    let (sheet, _) = axcenity_ui::parse_stylesheet("built-in.css", built_in.css);
    axcenity_ui::rule_sets(&sheet)
        .into_iter()
        .find(|set| set.selector == BUILT_IN_SELECTOR)
        .unwrap_or_default()
}

/// The built-in sets for an element of `class`.
pub fn built_ins_for(class: ClassName) -> impl Iterator<Item = &'static BuiltIn> {
    let kind = Kind::of(class);
    BUILT_IN
        .iter()
        .filter(move |built_in| Some(built_in.kind) == kind)
}

/// Every rule set in the project's stylesheets, with the stylesheet's path.
pub fn project_sets(stylesheets: &[Stylesheet]) -> Vec<(String, RuleSet)> {
    stylesheets
        .iter()
        .flat_map(|sheet| {
            axcenity_ui::rule_sets(sheet)
                .into_iter()
                .map(|set| (sheet.path().to_owned(), set))
        })
        .collect()
}

/// Whether `id`'s own rules already have every declaration of `set`, so
/// it looks as the set makes it.
fn wears(world: &World, stylesheets: &[Stylesheet], id: InstanceId, set: &RuleSet) -> bool {
    let Some(element_id) = super::arrange::element_id(world, id) else {
        return false;
    };
    let own = axcenity_ui::own_rules(stylesheets, element_id);
    set.rules.iter().all(|(suffix, declarations)| {
        own.iter()
            .find(|(known, _)| known == suffix)
            .is_some_and(|(_, own)| {
                declarations
                    .iter()
                    .all(|declaration| own.contains(declaration))
            })
    })
}

/// "with :hover, :active": the states and parts besides the set's own rule.
fn states_of(set: &RuleSet) -> String {
    let others: Vec<&str> = set
        .rules
        .iter()
        .map(|(suffix, _)| suffix.as_str())
        .filter(|suffix| !suffix.is_empty())
        .collect();
    if others.is_empty() {
        String::new()
    } else {
        format!(" with {}", others.join(", "))
    }
}

/// The value of `property` in the set's rule for `suffix`.
fn value<'a>(set: &'a RuleSet, suffix: &str, property: &str) -> Option<&'a str> {
    set.rules
        .iter()
        .find(|(known, _)| known == suffix)?
        .1
        .iter()
        .rev()
        .find(|(name, _)| name == property)
        .map(|(_, value)| value.as_str())
}

/// A CSS colour as egui's, or `None` for a value that isn't one.
fn color(value: &str) -> Option<egui::Color32> {
    let (color, alpha) = axcenity_ui::parse_css_color(value.trim())?;
    let byte = |channel: f32| (channel.clamp(0.0, 1.0) * 255.0).round() as u8;
    Some(egui::Color32::from_rgba_unmultiplied(
        byte(color.r),
        byte(color.g),
        byte(color.b),
        byte(alpha),
    ))
}

/// The first colour in a value such as `2px solid #fff` or a gradient.
fn first_color(value: &str) -> Option<egui::Color32> {
    let inner = value
        .trim()
        .strip_prefix("linear-gradient(")
        .and_then(|inner| inner.strip_suffix(')'));
    match inner {
        Some(inner) => axcenity_ui::split_css_list(inner)
            .into_iter()
            .find_map(color),
        None => value
            .split_whitespace()
            .find_map(color)
            .or_else(|| color(value)),
    }
}

/// The colours a tile shows `set` in: its background (top and bottom),
/// its border, and its text or the colour of its fill, knob or on track.
struct Swatch {
    top: Option<egui::Color32>,
    bottom: Option<egui::Color32>,
    border: Option<egui::Color32>,
    ink: Option<egui::Color32>,
    radius: f32,
}

impl Swatch {
    fn of(set: &RuleSet) -> Swatch {
        // A widget whose own rule has no background shows its track's.
        let base = ["", "::track"]
            .into_iter()
            .find(|suffix| value(set, suffix, "background-color").is_some())
            .unwrap_or("");
        let gradient = value(set, base, "background-image")
            .filter(|image| image.trim_start().starts_with("linear-gradient("))
            .map(|image| {
                let inner = &image[image.find('(').map_or(0, |at| at + 1)..];
                let colors: Vec<egui::Color32> =
                    axcenity_ui::split_css_list(inner.strip_suffix(')').unwrap_or(inner))
                        .into_iter()
                        .filter_map(color)
                        .collect();
                (colors.first().copied(), colors.get(1).copied())
            });
        let plain = value(set, base, "background-color").and_then(color);
        let (top, bottom) = gradient.unwrap_or((plain, plain));
        let border = value(set, base, "border")
            .filter(|border| !border.contains("none"))
            .and_then(first_color);
        let ink = value(set, "", "color")
            .and_then(color)
            .or_else(|| value(set, ":checked::track", "background-color").and_then(color))
            .or_else(|| value(set, "::fill", "background-image").and_then(first_color))
            .or_else(|| value(set, "::fill", "background-color").and_then(color))
            .or_else(|| value(set, "::thumb", "background-color").and_then(color));
        let radius = value(set, base, "border-radius")
            .and_then(|radius| radius.trim().trim_end_matches("px").parse::<f32>().ok())
            .unwrap_or(0.0);
        Swatch {
            top,
            bottom,
            border,
            ink,
            radius,
        }
    }
}

/// A tile's size, in points.
const TILE: egui::Vec2 = egui::vec2(64.0, 30.0);

/// A small picture of `set` with its name under it: a click applies it.
/// Starts a new row of the wrapping row it's in when it doesn't fit (a
/// wrapping row doesn't wrap the groups in it by itself).
fn tile(ui: &mut egui::Ui, name: &str, set: &RuleSet, worn: bool, tip: &str) -> bool {
    if ui.available_size_before_wrap().x < TILE.x && ui.cursor().min.x > ui.max_rect().min.x + 1.0 {
        ui.end_row();
    }
    let palette = theme::palette(ui.ctx());
    let swatch = Swatch::of(set);
    ui.vertical(|ui| {
        ui.set_width(TILE.x);
        let (rect, response) = ui.allocate_exact_size(TILE, egui::Sense::click());
        let painter = ui.painter_at(rect.expand(3.0));
        painter.rect_filled(rect, 4, palette.inset);
        let box_rect = rect.shrink(4.0);
        let radius = swatch.radius.min(box_rect.height() / 2.0);
        match (swatch.top, swatch.bottom) {
            (Some(top), Some(bottom)) if top != bottom => {
                let mut mesh = egui::Mesh::default();
                mesh.colored_vertex(box_rect.left_top(), top);
                mesh.colored_vertex(box_rect.right_top(), top);
                mesh.colored_vertex(box_rect.right_bottom(), bottom);
                mesh.colored_vertex(box_rect.left_bottom(), bottom);
                mesh.add_triangle(0, 1, 2);
                mesh.add_triangle(0, 2, 3);
                painter.add(mesh);
            }
            (Some(top), _) => {
                painter.rect_filled(box_rect, radius, top);
            }
            _ => {}
        }
        if let Some(border) = swatch.border {
            painter.rect_stroke(
                box_rect,
                radius,
                egui::Stroke::new(1.5_f32, border),
                egui::StrokeKind::Inside,
            );
        }
        if let Some(ink) = swatch.ink {
            painter.text(
                box_rect.center(),
                egui::Align2::CENTER_CENTER,
                "Aa",
                egui::FontId::proportional(theme::type_scale::BODY),
                ink,
            );
        }
        if worn || response.hovered() {
            painter.rect_stroke(
                rect,
                4,
                egui::Stroke::new(if worn { 2.0_f32 } else { 1.0 }, palette.accent),
                egui::StrokeKind::Outside,
            );
        }
        ui.add(egui::Label::new(egui::RichText::new(name).small()).truncate());
        response
            .on_hover_text(if worn {
                format!("{tip}. It's what this element looks like now.")
            } else {
                format!(
                    "{tip}. Copies its CSS into this element's own rules, where it can be \
                     changed like any other style."
                )
            })
            .clicked()
    })
    .inner
}

/// The Styles section: the built-in sets for the element's kind, the sets
/// read from a `.css` file (`imported`), and the Import menu.
pub(super) fn styles_section(
    ui: &mut egui::Ui,
    scene: &DesignerScene,
    id: InstanceId,
    imported: &mut Option<(String, Vec<RuleSet>)>,
    actions: &mut Vec<DesignerAction>,
) {
    let world = scene.world;
    let Some(instance) = world.get(id) else {
        return;
    };
    // Several selected: each of them gets it.
    let ids = if scene.selection.len() > 1 && scene.selection.contains(&id) {
        scene.selection.to_vec()
    } else {
        vec![id]
    };
    let apply = |name: &str, set: &RuleSet| DesignerAction::ApplyStyleSet {
        ids: ids.clone(),
        name: name.to_owned(),
        rules: set.rules.clone(),
    };
    let built_ins: Vec<&BuiltIn> = built_ins_for(instance.class()).collect();
    if !built_ins.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
            for built_in in built_ins {
                let set = built_in_rules(built_in);
                let worn = wears(world, scene.stylesheets, id, &set);
                if tile(ui, built_in.name, &set, worn, built_in.tip) && !worn {
                    actions.push(apply(built_in.name, &set));
                }
            }
        });
    }
    let mut close = false;
    if let Some((file, sets)) = imported.as_ref() {
        ui.horizontal(|ui| {
            ui.weak(format!("From {file}"));
            if widgets::icon_button(ui, icons::X, "Put these away", false).clicked() {
                close = true;
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
            for set in sets {
                let worn = wears(world, scene.stylesheets, id, set);
                let tip = format!("{} in {file}{}", set.selector, states_of(set));
                if tile(ui, &set.selector, set, worn, &tip) && !worn {
                    actions.push(apply(&set.selector, set));
                }
            }
        });
    }
    if close {
        *imported = None;
    }
    axcenity_editor_kit::widgets::menu_button(ui, Some(icons::DOWNLOAD_SIMPLE), "Import", |ui| {
        let sets = project_sets(scene.stylesheets);
        if sets.is_empty() {
            ui.weak("The project's stylesheets have no rules to import yet.");
        }
        let mut shown_path = None;
        for (path, set) in &sets {
            if shown_path != Some(path) {
                shown_path = Some(path);
                widgets::section_label(ui, path);
            }
            let label = format!("{}{}", set.selector, states_of(set));
            if ui
                .button(label)
                .on_hover_text("Copy this rule (and its states) into this element's own rules")
                .clicked()
            {
                actions.push(apply(&set.selector, set));
                ui.close();
            }
        }
        ui.separator();
        if ui
            .button(theme::with_icon(icons::FILE_CODE, "From a .css file…"))
            .on_hover_text(
                "Read a stylesheet from anywhere on this computer and pick one of its rules",
            )
            .clicked()
        {
            actions.push(DesignerAction::ImportStyleFile(id));
            ui.close();
        }
    })
    .response
    .on_hover_text("Copy a look from a CSS rule: the project's stylesheets, or a .css file");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_built_in_set_is_supported_css_for_its_selector() {
        for built_in in BUILT_IN {
            let (sheet, warnings) = axcenity_ui::parse_stylesheet("built-in.css", built_in.css);
            assert!(warnings.is_empty(), "{}: {warnings:?}", built_in.name);
            let sets = axcenity_ui::rule_sets(&sheet);
            assert_eq!(sets.len(), 1, "{}", built_in.name);
            assert_eq!(sets[0].selector, BUILT_IN_SELECTOR);
            for (_, declarations) in &sets[0].rules {
                for (property, _) in declarations {
                    assert!(
                        axcenity_ui::SUPPORTED_PROPERTIES.contains(&property.as_str()),
                        "{}: {property}",
                        built_in.name
                    );
                }
            }
        }
    }

    /// Picking one set after another leaves nothing of the first behind.
    #[test]
    fn each_kinds_sets_name_the_same_properties_in_the_same_states() {
        let shape = |set: &RuleSet| {
            let mut shape: Vec<(String, String)> = set
                .rules
                .iter()
                .flat_map(|(suffix, declarations)| {
                    declarations
                        .iter()
                        .map(move |(property, _)| (suffix.clone(), property.clone()))
                })
                .collect();
            shape.sort();
            shape
        };
        for kind in BUILT_IN.iter().map(|built_in| built_in.kind) {
            let mut sets = BUILT_IN.iter().filter(|built_in| built_in.kind == kind);
            let first = sets.next().unwrap();
            let expected = shape(&built_in_rules(first));
            for built_in in sets {
                assert_eq!(
                    shape(&built_in_rules(built_in)),
                    expected,
                    "{} and {}",
                    built_in.name,
                    first.name
                );
            }
        }
    }

    #[test]
    fn every_element_with_a_look_has_built_in_sets() {
        for class in [
            ClassName::TextButton,
            ClassName::Frame,
            ClassName::TextLabel,
            ClassName::TextBox,
            ClassName::Slider,
            ClassName::Toggle,
            ClassName::ProgressBar,
            ClassName::Dropdown,
        ] {
            assert!(built_ins_for(class).count() >= 2, "{class:?}");
        }
    }
}
