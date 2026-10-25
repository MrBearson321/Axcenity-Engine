//! The block editor: Scratch-style blocks on a canvas.
//!
//! On the left are the categories (Events, Control, Motion, ...) and a
//! palette of their blocks; on the right, the script's canvas. Drag a block
//! from the palette onto the canvas, or next to a stack, where it snaps
//! into place. Dragging a block takes the blocks under it along, as in
//! Scratch, and dropping blocks on the palette deletes them. Reporters (the
//! round and pointed blocks) drop into slots: a pointed one (true or false)
//! fits any slot, a round one only round slots. Right-click a block to
//! duplicate or delete it, and a value slot to change what kind of value
//! it holds. "View as Lua" shows the Lua the blocks become.
//!
//! Blocks are painted here, piece by piece. egui only fills convex shapes,
//! so a block is filled as convex pieces (rectangles, the hat's cap, the
//! puzzle tab under it) and then outlined as one path.
//!
//! Where to change behavior:
//! - `Category`, `sections` and `reporter_parts` — which pieces a block has
//! - `BlockPainter` — convex fills and the outline path
//! - `palette`, `show`, `show_canvas` and `finish_drag` — dragging, snapping and the toolbar

use std::collections::BTreeSet;

use axcenity_blocks::{
    Block, BlockKind, BlockScript, BlockShape, Comparison, Expression, LogicOp, MathFunction,
    MathOp, translate, valid_identifier,
};
use axcenity_core::{ClassName, KeyCode};
use eframe::egui::{
    self, Color32, CornerRadius, CursorIcon, FontId, Pos2, Rect, Sense, Shape, Stroke, Vec2, pos2,
    vec2,
};

use crate::lua_syntax;
use crate::theme::{self, icons};

mod canvas;
mod graph;
mod menus;
mod painter;
mod palette;

use canvas::*;
use menus::*;
use painter::*;
use palette::*;

pub use graph::show_graph;

/// An object a block can name: its path below the Workspace, such as
/// `Pad.Chime`, and its class.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectInfo {
    pub path: String,
    pub class: ClassName,
    /// For an Animator, the names its controller has, to offer in its
    /// blocks' menus. Empty for anything else.
    pub names: Vec<(NameKind, String)>,
}

/// What a name typed into an animation block names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameKind {
    Bool,
    Number,
    Trigger,
    State,
    Event,
}

impl NameKind {
    /// What the menu offers for a block with nothing to suggest.
    fn example(self) -> &'static str {
        match self {
            NameKind::Bool => "IsGrounded",
            NameKind::Number => "Speed",
            NameKind::Trigger => "Jump",
            NameKind::State => "Idle",
            NameKind::Event => "Footstep",
        }
    }

    fn what(self) -> &'static str {
        match self {
            NameKind::Bool => "Bool parameter",
            NameKind::Number => "Number parameter",
            NameKind::Trigger => "Trigger",
            NameKind::State => "state",
            NameKind::Event => "animation event",
        }
    }
}

/// What the block editor needs besides the script.
pub struct BlockContext<'a> {
    /// Tells this script's canvas (its scroll and zoom) apart from others'.
    pub key: &'a str,
    /// True for a client block script (a LocalScript).
    pub client: bool,
    /// The block a runtime error came from, outlined in red.
    pub highlighted: Option<u64>,
    /// What the blocks' object menus offer.
    pub objects: &'a [ObjectInfo],
}

// Sizes at zoom 1, in points.
const FONT_SIZE: f32 = 12.5;
const ROW_HEIGHT: f32 = 40.0;
const PAD: f32 = 10.0;
const GAP: f32 = 6.0;
const MIN_WIDTH: f32 = 90.0;
const ARM: f32 = 16.0;
const MOUTH_MIN: f32 = 24.0;
const FOOT: f32 = 24.0;
const CORNER: f32 = 6.0;
const INNER_CORNER: f32 = 4.0;
const CAP: f32 = 16.0;
const CAP_WIDTH: f32 = 84.0;
const TAB_LEFT: f32 = 14.0;
const TAB_WIDTH: f32 = 32.0;
const TAB_DEPTH: f32 = 8.0;
const INPUT_HEIGHT: f32 = 24.0;
const REPORTER_MIN_HEIGHT: f32 = 32.0;
/// How near a dragged stack has to be to a place to snap into it.
const SNAP: f32 = 34.0;
const STACK_GAP: f32 = 32.0;
const CATEGORY_WIDTH: f32 = 74.0;
const PALETTE_WIDTH: f32 = 270.0;
const TOOLBAR_HEIGHT: f32 = 40.0;

/// The dark grey of text typed into blocks, as in Scratch.
const INPUT_TEXT: Color32 = Color32::from_rgb(87, 94, 117);
const ERROR_OUTLINE: Color32 = Color32::from_rgb(255, 72, 72);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Category {
    Events,
    Control,
    Motion,
    Looks,
    Sound,
    Operators,
    Variables,
    Sensing,
    /// An Animator's parameters and states.
    Animation,
    More,
    /// Grey blocks of Lua that have no blocks, from a script switched from
    /// Code. Not in the palette.
    Lua,
}

impl Category {
    const ALL: [Category; 10] = [
        Category::Events,
        Category::Control,
        Category::Motion,
        Category::Looks,
        Category::Sound,
        Category::Operators,
        Category::Variables,
        Category::Sensing,
        Category::Animation,
        Category::More,
    ];

    fn name(self) -> &'static str {
        match self {
            Category::Events => "Events",
            Category::Control => "Control",
            Category::Motion => "Motion",
            Category::Looks => "Looks",
            Category::Sound => "Sound",
            Category::Operators => "Operators",
            Category::Variables => "Variables",
            Category::Sensing => "Sensing",
            Category::Animation => "Animation",
            Category::More => "More",
            Category::Lua => "Lua",
        }
    }

    /// Scratch's colours for its categories.
    fn fill(self) -> Color32 {
        match self {
            Category::Events => Color32::from_rgb(255, 191, 0),
            Category::Control => Color32::from_rgb(255, 171, 25),
            Category::Motion => Color32::from_rgb(76, 151, 255),
            Category::Looks => Color32::from_rgb(153, 102, 255),
            Category::Sound => Color32::from_rgb(207, 99, 207),
            Category::Operators => Color32::from_rgb(89, 192, 89),
            Category::Variables => Color32::from_rgb(255, 140, 26),
            Category::Sensing => Color32::from_rgb(92, 177, 214),
            Category::Animation => Color32::from_rgb(64, 186, 140),
            Category::More => Color32::from_rgb(255, 102, 128),
            Category::Lua => Color32::from_rgb(108, 117, 138),
        }
    }

    /// The darker outline, and the background of menus on the block.
    fn edge(self) -> Color32 {
        match self {
            Category::Events => Color32::from_rgb(204, 153, 0),
            Category::Control => Color32::from_rgb(207, 139, 23),
            Category::Motion => Color32::from_rgb(51, 115, 204),
            Category::Looks => Color32::from_rgb(119, 77, 203),
            Category::Sound => Color32::from_rgb(189, 66, 189),
            Category::Operators => Color32::from_rgb(56, 148, 56),
            Category::Variables => Color32::from_rgb(219, 110, 0),
            Category::Sensing => Color32::from_rgb(46, 142, 184),
            Category::Animation => Color32::from_rgb(36, 148, 106),
            Category::More => Color32::from_rgb(255, 51, 85),
            Category::Lua => Color32::from_rgb(78, 86, 105),
        }
    }
}

fn block_category(kind: &BlockKind) -> Category {
    match kind {
        BlockKind::WhenStarted { .. }
        | BlockKind::WhenTouched { .. }
        | BlockKind::WhenKeyPressed { .. }
        | BlockKind::WhenClicked { .. }
        | BlockKind::EveryFrame { .. }
        | BlockKind::WhenSoundEnded { .. }
        | BlockKind::Unattached { .. } => Category::Events,
        BlockKind::Wait { .. }
        | BlockKind::WaitUntil { .. }
        | BlockKind::Repeat { .. }
        | BlockKind::Forever { .. }
        | BlockKind::RepeatUntil { .. }
        | BlockKind::If { .. }
        | BlockKind::IfElse { .. }
        | BlockKind::StopScript => Category::Control,
        BlockKind::MoveTo { .. }
        | BlockKind::MoveBy { .. }
        | BlockKind::TurnTo { .. }
        | BlockKind::TurnBy { .. }
        | BlockKind::SetAnchored { .. } => Category::Motion,
        BlockKind::SetColor { .. }
        | BlockKind::SetText { .. }
        | BlockKind::Show { .. }
        | BlockKind::Hide { .. }
        | BlockKind::ShowPage { .. }
        | BlockKind::SetProperty { .. } => Category::Looks,
        BlockKind::PlaySound { .. }
        | BlockKind::StopSound { .. }
        | BlockKind::PauseSound { .. }
        | BlockKind::ResumeSound { .. }
        | BlockKind::SetVolume { .. } => Category::Sound,
        BlockKind::WhenStateChanged { .. }
        | BlockKind::WhenAnimationEvent { .. }
        | BlockKind::SetAnimatorBool { .. }
        | BlockKind::SetAnimatorNumber { .. }
        | BlockKind::SetAnimatorTrigger { .. }
        | BlockKind::ResetAnimatorTrigger { .. }
        | BlockKind::PlayAnimatorState { .. }
        | BlockKind::PlayAnimation { .. } => Category::Animation,
        BlockKind::SetVariable { .. } | BlockKind::ChangeVariable { .. } => Category::Variables,
        BlockKind::Print { .. } | BlockKind::Destroy { .. } => Category::More,
        BlockKind::Code { .. } | BlockKind::TopCode { .. } => Category::Lua,
    }
}

fn reporter_category(expression: &Expression) -> Category {
    match expression {
        Expression::Variable { .. } => Category::Variables,
        Expression::Property { property, .. } if property == "CurrentState" => Category::Animation,
        Expression::Property { .. }
        | Expression::TouchingPart
        | Expression::KeyDown { .. }
        | Expression::Raycast { .. } => Category::Sensing,
        Expression::AnimatorBool { .. } | Expression::AnimatorNumber { .. } => Category::Animation,
        Expression::Code { .. } => Category::Lua,
        _ => Category::Operators,
    }
}

/// Which objects an object menu offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Filter {
    Parts,
    Sounds,
    Buttons,
    Texts,
    PageLayouts,
    Gui,
    Animators,
    Animations,
    Any,
}

impl Filter {
    fn matches(self, class: ClassName) -> bool {
        match self {
            Filter::Parts => matches!(class, ClassName::Part | ClassName::MeshPart),
            Filter::Sounds => class == ClassName::Sound,
            Filter::Buttons => class == ClassName::TextButton,
            Filter::Texts => {
                matches!(
                    class,
                    ClassName::TextLabel | ClassName::TextButton | ClassName::TextBox
                )
            }
            Filter::PageLayouts => class == ClassName::PageLayout,
            Filter::Animators => class == ClassName::Animator,
            Filter::Animations => class == ClassName::Animation,
            Filter::Gui => matches!(
                class,
                ClassName::ScreenGui
                    | ClassName::Frame
                    | ClassName::TextLabel
                    | ClassName::TextButton
                    | ClassName::ImageLabel
                    | ClassName::TextBox
                    | ClassName::PageLayout
                    | ClassName::Table
            ),
            Filter::Any => true,
        }
    }

    /// A name to use when the Workspace has no object of this kind yet.
    fn placeholder(self) -> &'static str {
        match self {
            Filter::Parts | Filter::Any => "Part",
            Filter::Sounds => "Sound",
            Filter::Buttons => "TextButton",
            Filter::Texts => "TextLabel",
            Filter::PageLayouts => "PageLayout",
            Filter::Gui => "Frame",
            Filter::Animators => "Model.Animator",
            Filter::Animations => "Model.Animation",
        }
    }
}

/// What a value slot expects, which decides its look, the value it's
/// reset to, and which reporters fit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SlotKind {
    Number,
    Text,
    /// Any value: right-click to pick which kind.
    Any,
    /// A pointed slot (if, until): only true-or-false reporters fit.
    Condition,
    /// True or false, picked from a menu.
    Truth,
    Vector,
    Color,
}

impl SlotKind {
    fn accepts(self, shape: BlockShape) -> bool {
        let slot = if self == SlotKind::Condition {
            BlockShape::Boolean
        } else {
            BlockShape::Value
        };
        slot.accepts(shape)
    }

    /// What's left in the slot when its reporter is dragged out.
    fn empty(self) -> Expression {
        match self {
            SlotKind::Number => Expression::Number { value: 0.0 },
            SlotKind::Text | SlotKind::Any => Expression::Text {
                value: String::new(),
            },
            SlotKind::Condition => Expression::Boolean { value: false },
            SlotKind::Truth => Expression::Boolean { value: true },
            SlotKind::Vector => Expression::Vector3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            SlotKind::Color => Expression::Color3 {
                r: 1.0,
                g: 1.0,
                b: 1.0,
            },
        }
    }
}

/// One thing in a block's row.
enum Part<'a> {
    Label(String),
    Icon(&'static str),
    Slot(&'a mut Expression, SlotKind),
    Object(&'a mut String, Filter),
    Property(&'a mut String, Option<ClassName>),
    Variable(&'a mut String),
    Key(&'a mut String),
    /// A name in an Animator's controller (a parameter, state or event),
    /// with the Animator's path, for the menu's suggestions.
    Name(&'a mut String, NameKind, String),
    Math(&'a mut MathOp),
    Compare(&'a mut Comparison),
    Logic(&'a mut LogicOp),
    Function(&'a mut MathFunction),
    /// Lua kept word for word, in a grey block. Switch to Code to change it.
    Code(String),
}

/// A row of a block, or the mouth of a C-shaped block (if, repeat, ...)
/// with the blocks inside it.
enum Section<'a> {
    Row(Vec<Part<'a>>),
    Mouth(&'a mut Vec<Block>),
}

/// The most lines of Lua a grey block shows.
const CODE_LINES: usize = 14;

/// The Lua a grey block shows: long code is cut short.
fn shown_code(code: &str) -> String {
    let lines: Vec<&str> = code.lines().collect();
    if lines.len() <= CODE_LINES {
        return code.to_owned();
    }
    let mut shown = lines[..CODE_LINES - 1].join(
        "
",
    );
    shown.push_str(&format!(
        "
-- … {} more lines (switch to Code to see them)",
        lines.len() - (CODE_LINES - 1)
    ));
    shown
}

fn label(text: &str) -> Part<'static> {
    Part::Label(text.to_owned())
}

fn class_of(objects: &[ObjectInfo], path: &str) -> Option<ClassName> {
    objects
        .iter()
        .find(|object| object.path == path)
        .map(|object| object.class)
}

/// What a block shows, top to bottom. An event block's own body isn't
/// here: it's the stack under it.
fn sections<'a>(kind: &'a mut BlockKind, objects: &[ObjectInfo]) -> Vec<Section<'a>> {
    use Part::{Icon, Key, Object, Property, Slot, Variable};
    use Section::{Mouth, Row};
    match kind {
        BlockKind::WhenStarted { .. } => {
            vec![Row(vec![label("when game starts"), Icon(icons::FLAG)])]
        }
        BlockKind::WhenTouched { part, .. } => vec![Row(vec![
            label("when"),
            Object(part, Filter::Parts),
            label("is touched"),
        ])],
        BlockKind::WhenKeyPressed { key, .. } => {
            vec![Row(vec![label("when"), Key(key), label("key pressed")])]
        }
        BlockKind::WhenClicked { button, .. } => vec![Row(vec![
            label("when"),
            Object(button, Filter::Buttons),
            label("is clicked"),
        ])],
        BlockKind::EveryFrame { .. } => vec![Row(vec![label("every frame")])],
        BlockKind::WhenSoundEnded { sound, .. } => vec![Row(vec![
            label("when"),
            Object(sound, Filter::Sounds),
            label("ends"),
        ])],
        BlockKind::WhenStateChanged { animator, .. } => vec![Row(vec![
            label("when"),
            Object(animator, Filter::Animators),
            label("changes state"),
        ])],
        BlockKind::WhenAnimationEvent {
            animator, event, ..
        } => {
            let path = animator.clone();
            vec![Row(vec![
                label("when"),
                Object(animator, Filter::Animators),
                label("reaches event"),
                Part::Name(event, NameKind::Event, path),
            ])]
        }
        BlockKind::SetAnimatorBool {
            animator,
            parameter,
            value,
        } => {
            let path = animator.clone();
            vec![Row(vec![
                label("set"),
                Part::Name(parameter, NameKind::Bool, path),
                label("of"),
                Object(animator, Filter::Animators),
                label("to"),
                Slot(value, SlotKind::Truth),
            ])]
        }
        BlockKind::SetAnimatorNumber {
            animator,
            parameter,
            value,
        } => {
            let path = animator.clone();
            vec![Row(vec![
                label("set"),
                Part::Name(parameter, NameKind::Number, path),
                label("of"),
                Object(animator, Filter::Animators),
                label("to"),
                Slot(value, SlotKind::Number),
            ])]
        }
        BlockKind::SetAnimatorTrigger {
            animator,
            parameter,
        } => {
            let path = animator.clone();
            vec![Row(vec![
                label("set trigger"),
                Part::Name(parameter, NameKind::Trigger, path),
                label("of"),
                Object(animator, Filter::Animators),
            ])]
        }
        BlockKind::ResetAnimatorTrigger {
            animator,
            parameter,
        } => {
            let path = animator.clone();
            vec![Row(vec![
                label("reset trigger"),
                Part::Name(parameter, NameKind::Trigger, path),
                label("of"),
                Object(animator, Filter::Animators),
            ])]
        }
        BlockKind::PlayAnimatorState { animator, state } => {
            let path = animator.clone();
            vec![Row(vec![
                label("play state"),
                Part::Name(state, NameKind::State, path),
                label("of"),
                Object(animator, Filter::Animators),
            ])]
        }
        BlockKind::PlayAnimation {
            animator,
            animation,
        } => vec![Row(vec![
            label("play animation"),
            Object(animation, Filter::Animations),
            label("on"),
            Object(animator, Filter::Animators),
        ])],
        BlockKind::Unattached { .. } => Vec::new(),
        BlockKind::Wait { seconds } => vec![Row(vec![
            label("wait"),
            Slot(seconds, SlotKind::Number),
            label("seconds"),
        ])],
        BlockKind::WaitUntil { condition } => vec![Row(vec![
            label("wait until"),
            Slot(condition, SlotKind::Condition),
        ])],
        BlockKind::Repeat { count, body } => vec![
            Row(vec![
                label("repeat"),
                Slot(count, SlotKind::Number),
                label("times"),
            ]),
            Mouth(body),
        ],
        BlockKind::Forever { body } => vec![Row(vec![label("forever")]), Mouth(body)],
        BlockKind::RepeatUntil { condition, body } => vec![
            Row(vec![
                label("repeat until"),
                Slot(condition, SlotKind::Condition),
            ]),
            Mouth(body),
        ],
        BlockKind::If { condition, body } => vec![
            Row(vec![
                label("if"),
                Slot(condition, SlotKind::Condition),
                label("then"),
            ]),
            Mouth(body),
        ],
        BlockKind::IfElse {
            condition,
            then_body,
            else_body,
        } => vec![
            Row(vec![
                label("if"),
                Slot(condition, SlotKind::Condition),
                label("then"),
            ]),
            Mouth(then_body),
            Row(vec![label("else")]),
            Mouth(else_body),
        ],
        BlockKind::StopScript => vec![Row(vec![label("stop this script")])],
        BlockKind::MoveTo { target, position } => vec![Row(vec![
            label("move"),
            Object(target, Filter::Parts),
            label("to"),
            Slot(position, SlotKind::Vector),
        ])],
        BlockKind::MoveBy { target, offset } => vec![Row(vec![
            label("move"),
            Object(target, Filter::Parts),
            label("by"),
            Slot(offset, SlotKind::Vector),
        ])],
        BlockKind::TurnTo {
            target,
            orientation,
        } => vec![Row(vec![
            label("turn"),
            Object(target, Filter::Parts),
            label("to"),
            Slot(orientation, SlotKind::Vector),
            label("degrees"),
        ])],
        BlockKind::TurnBy { target, rotation } => vec![Row(vec![
            label("turn"),
            Object(target, Filter::Parts),
            label("by"),
            Slot(rotation, SlotKind::Vector),
            label("degrees"),
        ])],
        BlockKind::SetAnchored { target, anchored } => vec![Row(vec![
            label("set"),
            Object(target, Filter::Parts),
            label("anchored to"),
            Slot(anchored, SlotKind::Truth),
        ])],
        BlockKind::SetColor { target, color } => vec![Row(vec![
            label("set color of"),
            Object(target, Filter::Parts),
            label("to"),
            Slot(color, SlotKind::Color),
        ])],
        BlockKind::SetText { target, text } => vec![Row(vec![
            label("set text of"),
            Object(target, Filter::Texts),
            label("to"),
            Slot(text, SlotKind::Text),
        ])],
        BlockKind::Show { target } => vec![Row(vec![label("show"), Object(target, Filter::Gui)])],
        BlockKind::Hide { target } => vec![Row(vec![label("hide"), Object(target, Filter::Gui)])],
        BlockKind::ShowPage { layout, page } => vec![Row(vec![
            label("show page"),
            Slot(page, SlotKind::Text),
            label("of"),
            Object(layout, Filter::PageLayouts),
        ])],
        BlockKind::SetProperty {
            target,
            property,
            value,
        } => {
            let class = class_of(objects, target);
            vec![Row(vec![
                label("set"),
                Property(property, class),
                label("of"),
                Object(target, Filter::Any),
                label("to"),
                Slot(value, SlotKind::Any),
            ])]
        }
        BlockKind::PlaySound { target } => {
            vec![Row(vec![
                label("play sound"),
                Object(target, Filter::Sounds),
            ])]
        }
        BlockKind::StopSound { target } => {
            vec![Row(vec![
                label("stop sound"),
                Object(target, Filter::Sounds),
            ])]
        }
        BlockKind::PauseSound { target } => {
            vec![Row(vec![
                label("pause sound"),
                Object(target, Filter::Sounds),
            ])]
        }
        BlockKind::ResumeSound { target } => {
            vec![Row(vec![
                label("resume sound"),
                Object(target, Filter::Sounds),
            ])]
        }
        BlockKind::SetVolume { target, volume } => vec![Row(vec![
            label("set volume of"),
            Object(target, Filter::Sounds),
            label("to"),
            Slot(volume, SlotKind::Number),
        ])],
        BlockKind::SetVariable { name, value } => vec![Row(vec![
            label("set"),
            Variable(name),
            label("to"),
            Slot(value, SlotKind::Any),
        ])],
        BlockKind::ChangeVariable { name, by } => vec![Row(vec![
            label("change"),
            Variable(name),
            label("by"),
            Slot(by, SlotKind::Number),
        ])],
        BlockKind::Print { value } => vec![Row(vec![label("print"), Slot(value, SlotKind::Any)])],
        BlockKind::Destroy { target } => {
            vec![Row(vec![label("destroy"), Object(target, Filter::Any)])]
        }
        BlockKind::Code { code } | BlockKind::TopCode { code } => {
            vec![Row(vec![label("Lua"), Part::Code(shown_code(code))])]
        }
    }
}

/// What a reporter shows. Its slots are in the order of
/// [`Expression::children_mut`].
fn reporter_parts<'a>(expression: &'a mut Expression, objects: &[ObjectInfo]) -> Vec<Part<'a>> {
    use Part::{Key, Object, Property, Slot};
    match expression {
        Expression::Variable { name } => vec![Part::Label(name.clone())],
        Expression::Math { op, left, right } => vec![
            Slot(left, SlotKind::Number),
            Part::Math(op),
            Slot(right, SlotKind::Number),
        ],
        Expression::Function { function, value } => vec![
            Part::Function(function),
            label("of"),
            Slot(value, SlotKind::Number),
        ],
        Expression::Random { min, max } => vec![
            label("pick random"),
            Slot(min, SlotKind::Number),
            label("to"),
            Slot(max, SlotKind::Number),
        ],
        Expression::Join { left, right } => vec![
            label("join"),
            Slot(left, SlotKind::Text),
            Slot(right, SlotKind::Text),
        ],
        Expression::Compare { op, left, right } => vec![
            Slot(left, SlotKind::Any),
            Part::Compare(op),
            Slot(right, SlotKind::Any),
        ],
        Expression::Logic { op, left, right } => vec![
            Slot(left, SlotKind::Condition),
            Part::Logic(op),
            Slot(right, SlotKind::Condition),
        ],
        Expression::Not { value } => vec![label("not"), Slot(value, SlotKind::Condition)],
        Expression::Property { target, property } => {
            let class = class_of(objects, target);
            vec![
                Property(property, class),
                label("of"),
                Object(target, Filter::Any),
            ]
        }
        Expression::Raycast { origin, direction } => vec![
            label("raycast from"),
            Slot(origin, SlotKind::Vector),
            label("along"),
            Slot(direction, SlotKind::Vector),
        ],
        Expression::TouchingPart => vec![label("touching part")],
        Expression::KeyDown { key } => vec![label("key"), Key(key), label("pressed?")],
        Expression::AnimatorBool {
            animator,
            parameter,
        } => {
            let path = animator.clone();
            vec![
                Part::Name(parameter, NameKind::Bool, path),
                label("of"),
                Object(animator, Filter::Animators),
            ]
        }
        Expression::AnimatorNumber {
            animator,
            parameter,
        } => {
            let path = animator.clone();
            vec![
                Part::Name(parameter, NameKind::Number, path),
                label("of"),
                Object(animator, Filter::Animators),
            ]
        }
        Expression::Code { code } => vec![Part::Code(shown_code(code))],
        // Values typed in aren't reporters, and are drawn as inputs.
        Expression::Number { .. }
        | Expression::Text { .. }
        | Expression::Boolean { .. }
        | Expression::Vector3 { .. }
        | Expression::Color3 { .. } => Vec::new(),
    }
}

/// Where a value slot is: the block it's in, then which slot at each level
/// down through the reporters in it.
#[derive(Clone, Debug, PartialEq)]
struct SlotAddress {
    block: u64,
    path: Vec<usize>,
}

impl SlotAddress {
    fn child(&self, index: usize) -> SlotAddress {
        let mut path = self.path.clone();
        path.push(index);
        SlotAddress {
            block: self.block,
            path,
        }
    }

    fn resolve<'a>(&self, script: &'a mut BlockScript) -> Option<&'a mut Expression> {
        let block = script.find_mut(self.block)?;
        let (first, rest) = self.path.split_first()?;
        let mut slot = block.kind.expressions_mut().into_iter().nth(*first)?;
        for &index in rest {
            slot = slot.children_mut().into_iter().nth(index)?;
        }
        Some(slot)
    }
}

/// Where dragged stacks can snap, at the point their top-left corner goes.
type StackSpots = Vec<(Pos2, StackSpot)>;
/// Value slots a dragged reporter can drop into.
type SlotSpots = Vec<(Rect, SlotAddress, SlotKind)>;

/// Somewhere a dragged stack can snap to.
#[derive(Clone, Debug, PartialEq)]
enum StackSpot {
    /// Under this block.
    After { block: u64, last: bool },
    /// At the top of this mouth of a C-shaped block, or under an event.
    Mouth {
        block: u64,
        mouth: usize,
        empty: bool,
    },
    /// Above the loose stack at this index of the script's top level.
    Above(usize),
}

/// What's being dragged.
#[derive(Clone, Debug)]
enum Dragged {
    Stack(Vec<Block>),
    Reporter(Expression),
}

#[derive(Clone, Debug)]
struct Drag {
    item: Dragged,
    /// From the dragged item's top-left corner to the pointer.
    grab: Vec2,
    /// Its size as last drawn, for snapping it above a stack.
    size: Vec2,
}

/// What the editor remembers between frames for one script.
#[derive(Clone, Debug)]
struct State {
    pan: Vec2,
    zoom: f32,
    category: Category,
    /// A category clicked, for the palette to scroll to.
    scroll_to: Option<Category>,
    show_lua: bool,
    drag: Option<Drag>,
    /// The name being typed for a new variable, while the box is open.
    new_variable: Option<String>,
    /// Variables made with "Make a Variable" that no block uses yet.
    variables: Vec<String>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            pan: Vec2::ZERO,
            zoom: 1.0,
            category: Category::Events,
            scroll_to: None,
            show_lua: false,
            drag: None,
            new_variable: None,
            variables: Vec::new(),
        }
    }
}

/// Something done to a block from its right-click menu.
enum Action {
    Duplicate(u64),
    Delete(u64),
}

/// Every variable the script uses, and those just made, sorted.
fn known_variables(script: &BlockScript, made: &[String]) -> Vec<String> {
    let mut names: BTreeSet<String> = made.iter().cloned().collect();
    fn visit(expression: &Expression, names: &mut BTreeSet<String>) {
        if let Expression::Variable { name } = expression {
            names.insert(name.clone());
        }
        for child in expression.children() {
            visit(child, names);
        }
    }
    script.for_each_block(&mut |block| {
        if let BlockKind::SetVariable { name, .. } | BlockKind::ChangeVariable { name, .. } =
            &block.kind
        {
            names.insert(name.clone());
        }
        for expression in block.kind.expressions() {
            visit(expression, &mut names);
        }
    });
    names.into_iter().collect()
}

/// Draws the block editor for `script`. Returns true if the script changed.
pub fn show(ui: &mut egui::Ui, script: &mut BlockScript, context: BlockContext) -> bool {
    let before = script.clone();
    let state_id = egui::Id::new(("block editor", context.key));
    let mut state: State = ui
        .data_mut(|data| data.get_temp(state_id))
        .unwrap_or_default();
    let variables = known_variables(script, &state.variables);

    let full = ui.available_rect_before_wrap();
    let toolbar = Rect::from_min_size(full.min, vec2(full.width(), TOOLBAR_HEIGHT));
    let body = Rect::from_min_max(pos2(full.left(), toolbar.bottom()), full.max);
    let categories = Rect::from_min_size(body.min, vec2(CATEGORY_WIDTH, body.height()));
    let palette_rect = Rect::from_min_max(
        pos2(categories.right(), body.top()),
        pos2(categories.right() + PALETTE_WIDTH, body.bottom()),
    );
    let lua_width = if state.show_lua {
        (body.width() * 0.3).clamp(260.0, 460.0)
    } else {
        0.0
    };
    let canvas = Rect::from_min_max(
        pos2(palette_rect.right(), body.top()),
        pos2(body.right() - lua_width, body.bottom()),
    );
    let lua_rect = Rect::from_min_max(pos2(canvas.right(), body.top()), body.max);

    show_toolbar(ui, toolbar, &mut state, &context, script);
    show_categories(ui, categories, &mut state);
    show_palette(ui, palette_rect, &mut state, &context, &variables);
    let (stack_spots, slot_spots) =
        show_canvas(ui, canvas, script, &mut state, &context, &variables);
    if state.show_lua {
        show_lua(ui, lua_rect, script);
    }
    finish_drag(
        ui,
        &mut state,
        script,
        &context,
        &variables,
        canvas,
        palette_rect.union(categories),
        &stack_spots,
        &slot_spots,
    );
    ui.advance_cursor_after_rect(full);
    ui.data_mut(|data| data.insert_temp(state_id, state));
    *script != before
}

fn show_toolbar(
    ui: &mut egui::Ui,
    rect: Rect,
    state: &mut State,
    context: &BlockContext,
    script: &mut BlockScript,
) {
    ui.painter().rect_filled(rect, 0, ui.visuals().panel_fill);
    ui.painter().hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
    let mut bar = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(vec2(10.0, 6.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let palette = theme::palette(ui.ctx());
    let (icon, side, color, tip) = if context.client {
        (
            icons::USER,
            "Client blocks",
            palette.success,
            "Runs on each player's side, like a LocalScript: it can use the keyboard and UI clicks.",
        )
    } else {
        (
            icons::CLOUD,
            "Server blocks",
            palette.accent,
            "Runs on the server, like a Script. Keyboard and UI click blocks need a client block script.",
        )
    };
    egui::Frame::new()
        .fill(color.gamma_multiply(0.18))
        .stroke(Stroke::new(1.0_f32, color))
        .corner_radius(10)
        .inner_margin(egui::Margin::symmetric(8, 2))
        .show(&mut bar, |ui| {
            ui.label(
                egui::RichText::new(theme::with_icon(icon, side))
                    .color(color)
                    .strong(),
            );
        })
        .response
        .on_hover_text(tip);
    bar.weak("Drag blocks onto the canvas. Drop them back on the palette to delete.");
    bar.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let lua = egui::Button::new(theme::with_icon(icons::CODE, "View as Lua"))
            .selected(state.show_lua);
        if ui
            .add(lua)
            .on_hover_text(
                "Shows the Lua these blocks become. It runs exactly like a typed script.",
            )
            .clicked()
        {
            state.show_lua = !state.show_lua;
        }
        ui.separator();
        if ui
            .button(icons::MAGNIFYING_GLASS_PLUS)
            .on_hover_text("Zoom in")
            .clicked()
        {
            state.zoom = (state.zoom * 1.2).min(2.0);
        }
        if ui
            .button(format!("{:.0}%", state.zoom * 100.0))
            .on_hover_text("Back to 100%")
            .clicked()
        {
            state.zoom = 1.0;
        }
        if ui
            .button(icons::MAGNIFYING_GLASS_MINUS)
            .on_hover_text("Zoom out")
            .clicked()
        {
            state.zoom = (state.zoom / 1.2).max(0.5);
        }
        if ui
            .button(theme::with_icon(icons::BROOM, "Clean up"))
            .on_hover_text("Lines the scripts up down the left side")
            .clicked()
        {
            for block in &mut script.blocks {
                block.position = None;
            }
            state.pan = Vec2::ZERO;
        }
    });
}

fn show_categories(ui: &mut egui::Ui, rect: Rect, state: &mut State) {
    let visuals = ui.visuals().clone();
    ui.painter().rect_filled(rect, 0, visuals.panel_fill);
    ui.painter().vline(
        rect.right() - 0.5,
        rect.y_range(),
        visuals.widgets.noninteractive.bg_stroke,
    );
    let mut y = rect.top() + 8.0;
    for category in Category::ALL {
        let item = Rect::from_min_size(pos2(rect.left(), y), vec2(rect.width(), 54.0));
        let response = ui
            .interact(
                item,
                egui::Id::new(("block category", category.name())),
                Sense::click(),
            )
            .on_hover_cursor(CursorIcon::PointingHand);
        if category == state.category {
            ui.painter().rect_filled(
                item.shrink(3.0),
                6,
                visuals.selection.bg_fill.gamma_multiply(0.4),
            );
        } else if response.hovered() {
            ui.painter()
                .rect_filled(item.shrink(3.0), 6, visuals.widgets.hovered.weak_bg_fill);
        }
        let center = pos2(item.center().x, item.top() + 18.0);
        ui.painter().circle(
            center,
            10.0,
            category.fill(),
            Stroke::new(1.0_f32, category.edge()),
        );
        ui.painter().text(
            pos2(item.center().x, item.top() + 40.0),
            egui::Align2::CENTER_CENTER,
            category.name(),
            FontId::proportional(10.5),
            visuals.text_color(),
        );
        if response.clicked() {
            state.category = category;
            state.scroll_to = Some(category);
        }
        y += 56.0;
    }
}

#[cfg(test)]
mod tests;
