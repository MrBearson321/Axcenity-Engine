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

/// A puzzle tab under a block, or the notch cut into the top of the first
/// block of a loose stack. Drawn after the blocks, so a tab covers the top
/// of the block it fits into, as in Scratch.
struct Tab {
    at: Pos2,
    fill: Color32,
    edge: Color32,
}

/// Draws blocks with one painter, and collects what was done to them.
struct BlockPainter<'a> {
    painter: egui::Painter,
    z: f32,
    /// False for the palette and the dragged blocks: nothing reacts.
    interactive: bool,
    objects: &'a [ObjectInfo],
    variables: &'a [String],
    highlighted: Option<u64>,
    /// The background, which a loose stack's notch shows.
    background: Color32,
    id: egui::Id,
    tabs: Vec<Tab>,
    notches: Vec<Tab>,
    stack_spots: StackSpots,
    slot_spots: SlotSpots,
    /// A drag that started this frame.
    started: Option<Drag>,
    actions: Vec<Action>,
}

/// How a block was drawn.
struct Drawn {
    height: f32,
    width: f32,
    /// The block was grabbed this frame, from this offset.
    grabbed: Option<Vec2>,
}

impl<'a> BlockPainter<'a> {
    fn new(
        painter: egui::Painter,
        z: f32,
        interactive: bool,
        background: Color32,
        id: egui::Id,
        context: &'a BlockContext<'a>,
        variables: &'a [String],
    ) -> Self {
        Self {
            painter,
            z,
            interactive,
            objects: context.objects,
            variables,
            highlighted: context.highlighted,
            background,
            id,
            tabs: Vec::new(),
            notches: Vec::new(),
            stack_spots: Vec::new(),
            slot_spots: Vec::new(),
            started: None,
            actions: Vec::new(),
        }
    }

    fn s(&self, value: f32) -> f32 {
        value * self.z
    }

    fn font(&self) -> FontId {
        FontId::proportional(FONT_SIZE * self.z)
    }

    fn code_galley(&self, ui: &egui::Ui, code: &str) -> std::sync::Arc<egui::Galley> {
        ui.fonts(|fonts| {
            fonts.layout_no_wrap(
                code.to_owned(),
                FontId::monospace(FONT_SIZE * self.z * 0.95),
                Color32::WHITE,
            )
        })
    }

    fn text_width(&self, ui: &egui::Ui, text: &str) -> f32 {
        ui.fonts(|fonts| {
            fonts
                .layout_no_wrap(text.to_owned(), self.font(), Color32::WHITE)
                .size()
                .x
        })
    }

    // ---- Sizes ------------------------------------------------------------

    fn dropdown_size(&self, ui: &egui::Ui, text: &str) -> Vec2 {
        vec2(
            self.text_width(ui, text) + self.s(28.0),
            self.s(INPUT_HEIGHT),
        )
    }

    fn literal_size(&self, ui: &egui::Ui, expression: &Expression, kind: SlotKind) -> Vec2 {
        let height = self.s(INPUT_HEIGHT);
        match expression {
            Expression::Number { value } => vec2(
                (self.text_width(ui, &format_number(*value)) + self.s(16.0)).max(self.s(30.0)),
                height,
            ),
            Expression::Text { value } => vec2(
                (self.text_width(ui, value) + self.s(14.0)).max(self.s(36.0)),
                height,
            ),
            Expression::Boolean { .. } if kind == SlotKind::Condition => vec2(self.s(46.0), height),
            Expression::Boolean { value } => self.dropdown_size(ui, truth_name(*value)),
            Expression::Vector3 { x, y, z } => {
                let width: f32 = [x, y, z]
                    .iter()
                    .map(|value| self.axis_width(ui, **value))
                    .sum();
                vec2(width + self.s(6.0), height)
            }
            Expression::Color3 { .. } => vec2(self.s(40.0), height),
            _ => vec2(self.s(30.0), height),
        }
    }

    fn axis_width(&self, ui: &egui::Ui, value: f64) -> f32 {
        (self.text_width(ui, &format_number(value)) + self.s(12.0)).max(self.s(28.0))
    }

    fn part_size(&self, ui: &egui::Ui, part: &mut Part) -> Vec2 {
        match part {
            Part::Label(text) => vec2(self.text_width(ui, text), self.s(16.0)),
            Part::Icon(_) => vec2(self.s(18.0), self.s(18.0)),
            Part::Slot(expression, kind) => {
                if expression.is_literal() {
                    self.literal_size(ui, expression, *kind)
                } else {
                    self.reporter_size(ui, expression)
                }
            }
            Part::Object(text, _) | Part::Property(text, _) | Part::Key(text) => {
                self.dropdown_size(ui, text)
            }
            Part::Variable(name) => self.dropdown_size(ui, name),
            Part::Name(name, _, _) => self.dropdown_size(ui, name),
            Part::Math(op) => self.dropdown_size(ui, op.symbol()),
            Part::Compare(op) => self.dropdown_size(ui, op.symbol()),
            Part::Logic(op) => self.dropdown_size(ui, op.lua()),
            Part::Function(function) => self.dropdown_size(ui, function.name()),
            Part::Code(code) => self.code_galley(ui, code).size() + vec2(self.s(12.0), self.s(8.0)),
        }
    }

    fn parts_size(&self, ui: &egui::Ui, parts: &mut [Part]) -> Vec2 {
        let mut size = Vec2::ZERO;
        for (index, part) in parts.iter_mut().enumerate() {
            let part = self.part_size(ui, part);
            size.x += part.x + if index > 0 { self.s(GAP) } else { 0.0 };
            size.y = size.y.max(part.y);
        }
        size
    }

    fn row_size(&self, ui: &egui::Ui, parts: &mut [Part]) -> Vec2 {
        let content = self.parts_size(ui, parts);
        vec2(
            content.x + 2.0 * self.s(PAD),
            (content.y + self.s(12.0)).max(self.s(ROW_HEIGHT)),
        )
    }

    fn reporter_size(&self, ui: &egui::Ui, expression: &mut Expression) -> Vec2 {
        let boolean = expression.shape() == BlockShape::Boolean;
        let mut parts = reporter_parts(expression, self.objects);
        let content = self.parts_size(ui, &mut parts);
        let height = (content.y + self.s(8.0)).max(self.s(REPORTER_MIN_HEIGHT));
        let inset = if boolean { height * 0.5 } else { self.s(10.0) };
        vec2(content.x + 2.0 * inset, height)
    }

    // ---- Stacks and blocks ---------------------------------------------------

    /// Draws a stack with its top-left corner at `at`, and returns its
    /// height. `loose` is true for a stack under no event, whose top block
    /// shows the notch a block above would fit into. A block grabbed this
    /// frame is taken out of the stack, with the blocks under it.
    fn stack(&mut self, ui: &mut egui::Ui, blocks: &mut Vec<Block>, at: Pos2, loose: bool) -> f32 {
        let mut y = at.y;
        let mut grabbed = None;
        let count = blocks.len();
        for (index, block) in blocks.iter_mut().enumerate() {
            let drawn = self.statement(ui, block, pos2(at.x, y), loose && index == 0);
            if let Some(grab) = drawn.grabbed {
                grabbed = Some((index, grab));
            }
            y += drawn.height;
            if self.interactive && !block.kind.is_cap() {
                self.stack_spots.push((
                    pos2(at.x, y),
                    StackSpot::After {
                        block: block.id,
                        last: index + 1 == count,
                    },
                ));
            }
        }
        if let Some((index, grab)) = grabbed {
            let tail = blocks.split_off(index);
            self.started = Some(Drag {
                item: Dragged::Stack(tail),
                grab,
                size: Vec2::ZERO,
            });
        }
        y - at.y
    }

    /// Draws an event block and the stack under it. Returns the height of
    /// both, and whether the event block was grabbed.
    fn script(&mut self, ui: &mut egui::Ui, block: &mut Block, at: Pos2) -> Drawn {
        if let BlockKind::Unattached { body } = &mut block.kind {
            let height = self.stack(ui, body, at, true);
            return Drawn {
                height,
                width: 0.0,
                grabbed: None,
            };
        }
        let drawn = self.hat(ui, block, at);
        let body_top = pos2(at.x, at.y + drawn.height);
        // Top-level Lua has nothing under it.
        let has_body = !block.kind.bodies().is_empty();
        if self.interactive && has_body {
            self.stack_spots.push((
                body_top,
                StackSpot::Mouth {
                    block: block.id,
                    mouth: 0,
                    empty: block
                        .kind
                        .bodies()
                        .first()
                        .is_none_or(|body| body.is_empty()),
                },
            ));
        }
        let mut height = drawn.height;
        if let Some(body) = block.kind.bodies_mut().into_iter().next() {
            height += self.stack(ui, body, body_top, false);
        }
        Drawn { height, ..drawn }
    }

    /// The row of an event block, with its rounded cap.
    fn hat(&mut self, ui: &mut egui::Ui, block: &mut Block, at: Pos2) -> Drawn {
        let category = block_category(&block.kind);
        let (fill, edge) = (category.fill(), category.edge());
        let id = self.id.with(block.id);
        let block_id = block.id;
        let highlighted = self.highlighted == Some(block_id);
        let has_body = !block.kind.bodies().is_empty();
        let mut sections = sections(&mut block.kind, self.objects);
        let Some(Section::Row(parts)) = sections.first_mut() else {
            return Drawn {
                height: 0.0,
                width: 0.0,
                grabbed: None,
            };
        };
        let size = self.row_size(ui, parts);
        let width = size.x.max(self.s(MIN_WIDTH + 40.0));
        let cap = self.s(CAP);
        let row = Rect::from_min_size(pos2(at.x, at.y + cap), vec2(width, size.y));
        let background = self.painter.add(Shape::Noop);
        let response = self.interact(
            ui,
            row.union(Rect::from_min_size(at, vec2(self.s(CAP_WIDTH), cap))),
            id.with("hat"),
        );
        self.draw_row(ui, parts, row, id, block_id);
        drop(sections);

        let cap_points = cap_arc(at, self.s(CAP_WIDTH), cap);
        let mut fill_shapes = vec![
            Shape::convex_polygon(cap_points.clone(), fill, Stroke::NONE),
            Shape::rect_filled(
                row,
                CornerRadius {
                    nw: 0,
                    ne: radius(self.s(CORNER)),
                    sw: radius(self.s(CORNER)),
                    se: radius(self.s(CORNER)),
                },
                fill,
            ),
        ];
        let corner = self.s(CORNER);
        let mut outline = vec![pos2(at.x, row.bottom() - corner)];
        outline.extend(cap_points.iter().copied());
        outline.extend(rounded(
            &[
                (pos2(row.right(), row.top()), corner),
                (pos2(row.right(), row.bottom()), corner),
                (pos2(row.left(), row.bottom()), corner),
            ],
            Some(*cap_points.last().unwrap_or(&row.left_top())),
            Some(pos2(at.x, row.bottom() - corner)),
        ));
        fill_shapes.push(Shape::closed_line(outline, self.outline(edge, highlighted)));
        self.painter.set(background, Shape::Vec(fill_shapes));
        if has_body {
            self.tabs.push(Tab {
                at: pos2(at.x, row.bottom()),
                fill,
                edge,
            });
        }

        let grabbed = self.grab(&response, at, block_id);
        Drawn {
            height: cap + size.y,
            width,
            grabbed,
        }
    }

    /// A statement block, with any blocks in its mouths.
    fn statement(&mut self, ui: &mut egui::Ui, block: &mut Block, at: Pos2, loose: bool) -> Drawn {
        let category = block_category(&block.kind);
        let (fill, edge) = (category.fill(), category.edge());
        let id = self.id.with(block.id);
        let block_id = block.id;
        let cap = block.kind.is_cap();
        let highlighted = self.highlighted == Some(block_id);
        let objects = self.objects;
        let mut sections = sections(&mut block.kind, objects);

        let mut width = self.s(MIN_WIDTH);
        let mut row_heights = Vec::new();
        for section in &mut sections {
            if let Section::Row(parts) = section {
                let size = self.row_size(ui, parts);
                width = width.max(size.x);
                row_heights.push(size.y);
            }
        }
        let has_mouth = sections
            .iter()
            .any(|section| matches!(section, Section::Mouth(_)));

        let background = self.painter.add(Shape::Noop);
        let mut responses = Vec::new();
        // Each piece: its rect, and whether it's a mouth.
        let mut pieces: Vec<(Rect, bool)> = Vec::new();
        let mut y = at.y;
        let (mut row, mut mouth) = (0, 0);
        for section in &mut sections {
            match section {
                Section::Row(parts) => {
                    let rect = Rect::from_min_size(pos2(at.x, y), vec2(width, row_heights[row]));
                    responses.push(self.interact(ui, rect, id.with(("row", row))));
                    self.draw_row(ui, parts, rect, id.with(row), block_id);
                    pieces.push((rect, false));
                    y += rect.height();
                    row += 1;
                }
                Section::Mouth(body) => {
                    let top = pos2(at.x + self.s(ARM), y);
                    if self.interactive {
                        self.stack_spots.push((
                            top,
                            StackSpot::Mouth {
                                block: block_id,
                                mouth,
                                empty: body.is_empty(),
                            },
                        ));
                    }
                    let inside = self.stack(ui, body, top, false).max(self.s(MOUTH_MIN));
                    let arm = Rect::from_min_size(pos2(at.x, y), vec2(self.s(ARM), inside));
                    responses.push(self.interact(ui, arm, id.with(("arm", mouth))));
                    pieces.push((arm, true));
                    // The tab hanging into the mouth, which the first block
                    // inside fits onto.
                    self.tabs.push(Tab {
                        at: top,
                        fill,
                        edge,
                    });
                    y += inside;
                    mouth += 1;
                }
            }
        }
        drop(sections);
        if has_mouth {
            let foot = Rect::from_min_size(pos2(at.x, y), vec2(width, self.s(FOOT)));
            responses.push(self.interact(ui, foot, id.with("foot")));
            pieces.push((foot, false));
            y += foot.height();
        }

        let shapes = self.block_shapes(&pieces, fill, edge, highlighted);
        self.painter.set(background, Shape::Vec(shapes));
        if !cap {
            self.tabs.push(Tab {
                at: pos2(at.x, y),
                fill,
                edge,
            });
        }
        if loose {
            self.notches.push(Tab { at, fill, edge });
        }

        let mut grabbed = None;
        for response in &responses {
            if let Some(grab) = self.grab(response, at, block_id) {
                grabbed = Some(grab);
            }
        }
        Drawn {
            height: y - at.y,
            width,
            grabbed,
        }
    }

    /// Senses dragging and right-clicks on part of a block.
    fn interact(&self, ui: &mut egui::Ui, rect: Rect, id: egui::Id) -> Option<egui::Response> {
        self.interactive.then(|| {
            ui.interact(rect, id, Sense::click_and_drag())
                .on_hover_cursor(CursorIcon::Grab)
        })
    }

    /// If `response` started a drag, the offset from the block's top-left
    /// corner `at` to the pointer. Also shows the block's right-click menu.
    fn grab(&mut self, response: &Option<egui::Response>, at: Pos2, block: u64) -> Option<Vec2> {
        let response = response.as_ref()?;
        response.context_menu(|ui| {
            if ui
                .button(theme::with_icon(icons::COPY, "Duplicate"))
                .clicked()
            {
                self.actions.push(Action::Duplicate(block));
                ui.close();
            }
            if ui
                .button(theme::with_icon(icons::TRASH, "Delete"))
                .clicked()
            {
                self.actions.push(Action::Delete(block));
                ui.close();
            }
        });
        if response.drag_started() {
            let origin = response.interact_pointer_pos()?;
            return Some(origin - at);
        }
        None
    }

    fn outline(&self, edge: Color32, highlighted: bool) -> Stroke {
        if highlighted {
            Stroke::new(self.s(3.0).max(2.0), ERROR_OUTLINE)
        } else {
            Stroke::new(1.0_f32, edge)
        }
    }

    /// Fills a block's pieces (rows, arms, foot) and outlines them as one shape.
    fn block_shapes(
        &self,
        pieces: &[(Rect, bool)],
        fill: Color32,
        edge: Color32,
        highlighted: bool,
    ) -> Vec<Shape> {
        let (corner, inner) = (self.s(CORNER), self.s(INNER_CORNER));
        let Some(&(first, _)) = pieces.first() else {
            return Vec::new();
        };
        let (left, right) = (first.left(), first.right());
        let last = pieces.len() - 1;
        let mut shapes = Vec::new();
        for (index, &(rect, arm)) in pieces.iter().enumerate() {
            if arm {
                // A little longer, so no seam shows where it meets the rows.
                shapes.push(Shape::rect_filled(
                    rect.expand2(vec2(0.0, 1.0)),
                    CornerRadius::ZERO,
                    fill,
                ));
                continue;
            }
            let after_mouth = index > 0;
            let before_mouth = index < last;
            let rounding = CornerRadius {
                nw: if after_mouth { 0 } else { radius(corner) },
                ne: radius(if after_mouth { inner } else { corner }),
                sw: if before_mouth { 0 } else { radius(corner) },
                se: radius(if before_mouth { inner } else { corner }),
            };
            shapes.push(Shape::rect_filled(rect, rounding, fill));
        }

        // The outline goes clockwise from the top-left corner: along the
        // top, down the right side, in and out of each mouth, then back
        // along the bottom and up the left side.
        let mut corners = vec![(first.left_top(), corner), (first.right_top(), corner)];
        for (index, &(rect, arm)) in pieces.iter().enumerate() {
            if arm {
                let inside = rect.right();
                corners.push((pos2(right, rect.top()), inner));
                corners.push((pos2(inside, rect.top()), inner));
                corners.push((pos2(inside, rect.bottom()), inner));
                corners.push((pos2(right, rect.bottom()), inner));
            } else if index == last {
                corners.push((pos2(right, rect.bottom()), corner));
                corners.push((pos2(left, rect.bottom()), corner));
            }
        }
        shapes.push(Shape::closed_line(
            rounded(&corners, None, None),
            self.outline(edge, highlighted),
        ));
        shapes
    }

    // ---- Rows and their parts -------------------------------------------------

    /// Lays `parts` out along `rect`, centred top to bottom.
    fn draw_row(
        &mut self,
        ui: &mut egui::Ui,
        parts: &mut [Part],
        rect: Rect,
        id: egui::Id,
        block: u64,
    ) {
        let address = SlotAddress {
            block,
            path: Vec::new(),
        };
        self.draw_parts(
            ui,
            parts,
            rect.left() + self.s(PAD),
            rect.center().y,
            id,
            &address,
        );
    }

    /// Draws `parts` left to right from `x`, centred on `center_y`. Slots
    /// are numbered from 0 under `address`.
    fn draw_parts(
        &mut self,
        ui: &mut egui::Ui,
        parts: &mut [Part],
        mut x: f32,
        center_y: f32,
        id: egui::Id,
        address: &SlotAddress,
    ) {
        let mut slot = 0;
        for (index, part) in parts.iter_mut().enumerate() {
            let size = self.part_size(ui, part);
            let rect = Rect::from_min_size(pos2(x, center_y - size.y * 0.5), size);
            let part_id = id.with(index);
            match part {
                Part::Label(text) => {
                    self.painter.text(
                        rect.left_center(),
                        egui::Align2::LEFT_CENTER,
                        text.as_str(),
                        self.font(),
                        Color32::WHITE,
                    );
                }
                Part::Icon(icon) => {
                    self.painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        *icon,
                        FontId::proportional(16.0 * self.z),
                        Color32::from_rgb(76, 191, 86),
                    );
                }
                Part::Slot(expression, kind) => {
                    self.slot(ui, expression, *kind, rect, part_id, address.child(slot));
                    slot += 1;
                }
                Part::Object(path, filter) => {
                    let filter = *filter;
                    if let Some(response) = self.dropdown(ui, rect, part_id, path) {
                        let objects = self.objects;
                        egui::Popup::menu(&response).show(|ui| {
                            object_menu(ui, path, filter, objects);
                        });
                    }
                }
                Part::Property(name, class) => {
                    let class = *class;
                    if let Some(response) = self.dropdown(ui, rect, part_id, name) {
                        egui::Popup::menu(&response).show(|ui| property_menu(ui, name, class));
                    }
                }
                Part::Variable(name) => {
                    let variables = self.variables;
                    if let Some(response) = self.dropdown(ui, rect, part_id, name) {
                        egui::Popup::menu(&response)
                            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                            .show(|ui| variable_menu(ui, name, variables));
                    }
                }
                Part::Key(key) => {
                    if let Some(response) = self.dropdown(ui, rect, part_id, key) {
                        egui::Popup::menu(&response).show(|ui| key_menu(ui, key));
                    }
                }
                Part::Name(name, kind, animator) => {
                    let (kind, objects) = (*kind, self.objects);
                    let suggestions: Vec<String> = objects
                        .iter()
                        .find(|object| object.path == *animator)
                        .map(|object| {
                            object
                                .names
                                .iter()
                                .filter(|(of, _)| *of == kind)
                                .map(|(_, name)| name.clone())
                                .collect()
                        })
                        .unwrap_or_default();
                    if let Some(response) = self.dropdown(ui, rect, part_id, name) {
                        egui::Popup::menu(&response)
                            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                            .show(|ui| name_menu(ui, name, kind, &suggestions));
                    }
                }
                Part::Math(op) => {
                    if let Some(response) = self.dropdown(ui, rect, part_id, op.symbol()) {
                        egui::Popup::menu(&response).show(|ui| {
                            for choice in MathOp::ALL {
                                if ui
                                    .selectable_label(**op == choice, choice.symbol())
                                    .clicked()
                                {
                                    **op = choice;
                                    ui.close();
                                }
                            }
                        });
                    }
                }
                Part::Compare(op) => {
                    if let Some(response) = self.dropdown(ui, rect, part_id, op.symbol()) {
                        egui::Popup::menu(&response).show(|ui| {
                            for choice in Comparison::ALL {
                                if ui
                                    .selectable_label(**op == choice, choice.symbol())
                                    .clicked()
                                {
                                    **op = choice;
                                    ui.close();
                                }
                            }
                        });
                    }
                }
                Part::Logic(op) => {
                    if let Some(response) = self.dropdown(ui, rect, part_id, op.lua()) {
                        egui::Popup::menu(&response).show(|ui| {
                            for choice in [LogicOp::And, LogicOp::Or] {
                                if ui.selectable_label(**op == choice, choice.lua()).clicked() {
                                    **op = choice;
                                    ui.close();
                                }
                            }
                        });
                    }
                }
                Part::Code(code) => {
                    let galley = self.code_galley(ui, code);
                    self.painter.rect_filled(
                        rect,
                        self.s(4.0),
                        Color32::from_rgba_unmultiplied(0, 0, 0, 60),
                    );
                    self.painter.galley(
                        rect.min + vec2(self.s(6.0), self.s(4.0)),
                        galley,
                        Color32::WHITE,
                    );
                }
                Part::Function(function) => {
                    if let Some(response) = self.dropdown(ui, rect, part_id, function.name()) {
                        egui::Popup::menu(&response).show(|ui| {
                            for choice in MathFunction::ALL {
                                if ui
                                    .selectable_label(**function == choice, choice.name())
                                    .clicked()
                                {
                                    **function = choice;
                                    ui.close();
                                }
                            }
                        });
                    }
                }
            }
            x += size.x + self.s(GAP);
        }
    }

    /// A menu's button on a block: darker than the block, with an arrow.
    fn dropdown(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        id: egui::Id,
        text: &str,
    ) -> Option<egui::Response> {
        let response = self
            .interactive
            .then(|| ui.interact(rect, id, Sense::click()));
        let hovered = response.as_ref().is_some_and(egui::Response::hovered);
        let fill = Color32::from_black_alpha(if hovered { 70 } else { 45 });
        self.painter
            .rect_filled(rect, radius(rect.height() * 0.5), fill);
        self.painter.text(
            pos2(rect.left() + self.s(9.0), rect.center().y),
            egui::Align2::LEFT_CENTER,
            text,
            self.font(),
            Color32::WHITE,
        );
        self.painter.text(
            pos2(rect.right() - self.s(9.0), rect.center().y),
            egui::Align2::CENTER_CENTER,
            icons::CARET_DOWN,
            FontId::proportional(10.0 * self.z),
            Color32::WHITE,
        );
        response
    }

    /// A value slot: a value typed in, or a reporter dropped in.
    fn slot(
        &mut self,
        ui: &mut egui::Ui,
        expression: &mut Expression,
        kind: SlotKind,
        rect: Rect,
        id: egui::Id,
        address: SlotAddress,
    ) {
        if expression.is_literal() {
            if self.interactive {
                self.slot_spots.push((rect, address, kind));
            }
            self.literal(ui, expression, kind, rect, id);
        } else {
            self.reporter(ui, expression, rect.min, id, Some((address, kind)));
        }
    }

    /// Draws a reporter with its top-left corner at `at`, and returns its
    /// size. In a slot (`slot`), grabbing it takes it out of the slot.
    fn reporter(
        &mut self,
        ui: &mut egui::Ui,
        expression: &mut Expression,
        at: Pos2,
        id: egui::Id,
        slot: Option<(SlotAddress, SlotKind)>,
    ) -> Vec2 {
        let size = self.reporter_size(ui, expression);
        let rect = Rect::from_min_size(at, size);
        if self.interactive
            && let Some((_, kind)) = &slot
        {
            let response = ui
                .interact(rect, id.with("reporter"), Sense::click_and_drag())
                .on_hover_cursor(CursorIcon::Grab);
            if response.drag_started()
                && let Some(origin) = response.interact_pointer_pos()
            {
                let taken = std::mem::replace(expression, kind.empty());
                self.started = Some(Drag {
                    item: Dragged::Reporter(taken),
                    grab: origin - at,
                    size,
                });
                return size;
            }
        }
        let category = reporter_category(expression);
        let (fill, edge) = (category.fill(), category.edge());
        let boolean = expression.shape() == BlockShape::Boolean;
        if boolean {
            self.painter.add(Shape::convex_polygon(
                hexagon(rect),
                fill,
                Stroke::new(1.0_f32, edge),
            ));
        } else {
            self.painter.add(egui::epaint::RectShape::new(
                rect,
                radius(rect.height() * 0.5),
                fill,
                Stroke::new(1.0_f32, edge),
                egui::StrokeKind::Inside,
            ));
        }
        let inset = if boolean { size.y * 0.5 } else { self.s(10.0) };
        let address = slot.map_or(
            SlotAddress {
                block: 0,
                path: Vec::new(),
            },
            |(address, _)| address,
        );
        let objects = self.objects;
        let mut parts = reporter_parts(expression, objects);
        // Slots inside a reporter that isn't in a block (the palette, or
        // one being dragged) aren't places to drop into.
        let interactive = self.interactive;
        if address.path.is_empty() {
            self.interactive = false;
        }
        self.draw_parts(ui, &mut parts, at.x + inset, rect.center().y, id, &address);
        self.interactive = interactive;
        size
    }

    /// A value typed into a slot.
    fn literal(
        &mut self,
        ui: &mut egui::Ui,
        expression: &mut Expression,
        kind: SlotKind,
        rect: Rect,
        id: egui::Id,
    ) {
        let font = self.font();
        let response = match expression {
            Expression::Number { value } => {
                self.painter
                    .rect_filled(rect, radius(rect.height() * 0.5), Color32::WHITE);
                self.number_input(ui, rect, id, value, font)
            }
            Expression::Text { value } => {
                self.painter
                    .rect_filled(rect, radius(self.s(4.0)), Color32::WHITE);
                if self.interactive {
                    Some(
                        ui.put(
                            rect,
                            egui::TextEdit::singleline(value)
                                .id(id)
                                .frame(false)
                                .font(font)
                                .text_color(INPUT_TEXT)
                                .horizontal_align(egui::Align::Center)
                                .vertical_align(egui::Align::Center)
                                .min_size(rect.size())
                                .margin(egui::Margin::ZERO),
                        ),
                    )
                } else {
                    self.painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        value.as_str(),
                        font,
                        INPUT_TEXT,
                    );
                    None
                }
            }
            Expression::Boolean { value } if kind == SlotKind::Condition => {
                // An empty pointed hole, as in Scratch, that still says
                // what it's set to.
                self.painter.add(Shape::convex_polygon(
                    hexagon(rect),
                    Color32::from_black_alpha(60),
                    Stroke::NONE,
                ));
                self.painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    truth_name(*value),
                    FontId::proportional(10.5 * self.z),
                    Color32::from_white_alpha(170),
                );
                let response = self
                    .interactive
                    .then(|| ui.interact(rect, id, Sense::click()));
                if let Some(response) = &response
                    && response.clicked()
                {
                    *value = !*value;
                }
                response.map(|response| {
                    response.on_hover_text(
                        "Drop a pointed block here, or click to switch true and false",
                    )
                })
            }
            Expression::Boolean { value } => {
                let text = truth_name(*value);
                let response = self.dropdown(ui, rect, id, text);
                if let Some(response) = &response {
                    egui::Popup::menu(response).show(|ui| {
                        for choice in [true, false] {
                            if ui
                                .selectable_label(*value == choice, truth_name(choice))
                                .clicked()
                            {
                                *value = choice;
                                ui.close();
                            }
                        }
                    });
                }
                response
            }
            Expression::Vector3 { x, y, z } => {
                let mut left = rect.left();
                let mut last = None;
                for (axis, value) in [x, y, z].into_iter().enumerate() {
                    let width = self.axis_width(ui, *value);
                    let oval =
                        Rect::from_min_size(pos2(left, rect.top()), vec2(width, rect.height()));
                    self.painter
                        .rect_filled(oval, radius(oval.height() * 0.5), Color32::WHITE);
                    last = self.number_input(ui, oval, id.with(axis), value, font.clone());
                    left += width + self.s(3.0);
                }
                last
            }
            Expression::Color3 { r, g, b } => {
                let mut srgb =
                    [*r, *g, *b].map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8);
                self.painter
                    .rect_filled(rect, radius(rect.height() * 0.5), Color32::WHITE);
                let swatch = rect.shrink(self.s(3.0));
                if self.interactive {
                    // `Color3` is sRGB from 0 to 1, as the colour button's bytes are.
                    let response = ui
                        .scope_builder(egui::UiBuilder::new().max_rect(swatch), |ui| {
                            ui.spacing_mut().interact_size = swatch.size();
                            ui.color_edit_button_srgb(&mut srgb)
                        })
                        .inner;
                    if response.changed() {
                        [*r, *g, *b] = srgb.map(|channel| f64::from(channel) / 255.0);
                    }
                    Some(response)
                } else {
                    let [red, green, blue] = srgb;
                    self.painter.rect_filled(
                        swatch,
                        radius(swatch.height() * 0.5),
                        Color32::from_rgb(red, green, blue),
                    );
                    None
                }
            }
            _ => None,
        };
        // Any value slot can hold any kind of value: right-click to choose.
        if kind == SlotKind::Any
            && let Some(response) = response
        {
            response.context_menu(|ui| {
                ui.weak("Use a…");
                for (name, value) in [
                    ("Number", SlotKind::Number.empty()),
                    ("Text", SlotKind::Text.empty()),
                    ("True or false", SlotKind::Truth.empty()),
                    ("Vector3", SlotKind::Vector.empty()),
                    ("Color", SlotKind::Color.empty()),
                ] {
                    if ui.button(name).clicked() {
                        *expression = value;
                        ui.close();
                    }
                }
            });
        }
    }

    /// A number typed into a block. What's typed is kept as text while the
    /// box has focus, so a half-typed `1.` isn't turned back into `1`.
    fn number_input(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        id: egui::Id,
        value: &mut f64,
        font: FontId,
    ) -> Option<egui::Response> {
        if !self.interactive {
            self.painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                format_number(*value),
                font,
                INPUT_TEXT,
            );
            return None;
        }
        let buffer = id.with("typed");
        let mut text = ui
            .data(|data| data.get_temp::<String>(buffer))
            .unwrap_or_else(|| format_number(*value));
        let response = ui.put(
            rect,
            egui::TextEdit::singleline(&mut text)
                .id(id)
                .frame(false)
                .font(font)
                .text_color(INPUT_TEXT)
                .horizontal_align(egui::Align::Center)
                .vertical_align(egui::Align::Center)
                .min_size(rect.size())
                .margin(egui::Margin::ZERO),
        );
        if response.changed()
            && let Ok(number) = text.trim().parse::<f64>()
            && number.is_finite()
        {
            *value = number;
        }
        if response.has_focus() {
            ui.data_mut(|data| data.insert_temp(buffer, text));
        } else {
            ui.data_mut(|data| data.remove::<String>(buffer));
        }
        Some(response)
    }

    /// Draws the tabs and notches collected while drawing, over the blocks.
    fn finish(&mut self) {
        for tab in std::mem::take(&mut self.tabs) {
            let points = tab_points(tab.at, self.z);
            let mut filled = points;
            // Covers the outline it hangs from, so the two blocks look joined.
            filled[0].y -= 1.5;
            filled[3].y -= 1.5;
            self.painter.add(Shape::convex_polygon(
                filled.to_vec(),
                tab.fill,
                Stroke::NONE,
            ));
            self.painter
                .add(Shape::line(points.to_vec(), Stroke::new(1.0_f32, tab.edge)));
        }
        for notch in std::mem::take(&mut self.notches) {
            let points = tab_points(notch.at, self.z);
            let mut filled = points;
            filled[0].y -= 1.5;
            filled[3].y -= 1.5;
            self.painter.add(Shape::convex_polygon(
                filled.to_vec(),
                self.background,
                Stroke::NONE,
            ));
            self.painter.add(Shape::line(
                points.to_vec(),
                Stroke::new(1.0_f32, notch.edge),
            ));
        }
    }
}

/// Corner radius as egui stores it.
fn radius(value: f32) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

fn truth_name(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

/// A number as blocks show it: no decimal point for whole numbers.
fn format_number(value: f64) -> String {
    let rounded = (value * 1000.0).round() / 1000.0;
    rounded.to_string()
}

/// The puzzle tab's corners, from the top-left, for a block whose left edge
/// and bottom are at `at`.
fn tab_points(at: Pos2, z: f32) -> [Pos2; 4] {
    let left = at.x + TAB_LEFT * z;
    let right = left + TAB_WIDTH * z;
    let slope = 6.0 * z;
    let depth = TAB_DEPTH * z;
    [
        pos2(left, at.y),
        pos2(left + slope, at.y + depth),
        pos2(right - slope, at.y + depth),
        pos2(right, at.y),
    ]
}

/// The pointed shape of a true-or-false reporter.
fn hexagon(rect: Rect) -> Vec<Pos2> {
    let point = rect.height() * 0.5;
    vec![
        pos2(rect.left(), rect.center().y),
        pos2(rect.left() + point, rect.top()),
        pos2(rect.right() - point, rect.top()),
        pos2(rect.right(), rect.center().y),
        pos2(rect.right() - point, rect.bottom()),
        pos2(rect.left() + point, rect.bottom()),
    ]
}

/// The rounded cap of an event block, from its left end to its right, for
/// a block whose top-left corner (above the cap) is `at`.
fn cap_arc(at: Pos2, width: f32, height: f32) -> Vec<Pos2> {
    let steps = 14;
    (0..=steps)
        .map(|step| {
            let t = step as f32 / steps as f32;
            let angle = std::f32::consts::PI * t;
            pos2(
                at.x + width * 0.5 * (1.0 - angle.cos()),
                at.y + height * (1.0 - angle.sin()),
            )
        })
        .collect()
}

/// The points of a polygon whose corners are rounded by their radii. With
/// `before` and `after`, the path comes from `before` and goes on to
/// `after` (an open run of corners); without, it's closed.
fn rounded(corners: &[(Pos2, f32)], before: Option<Pos2>, after: Option<Pos2>) -> Vec<Pos2> {
    let count = corners.len();
    let mut points = Vec::new();
    for index in 0..count {
        let (corner, radius) = corners[index];
        let previous = if index == 0 {
            before.unwrap_or(corners[count - 1].0)
        } else {
            corners[index - 1].0
        };
        let next = if index + 1 == count {
            after.unwrap_or(corners[0].0)
        } else {
            corners[index + 1].0
        };
        let into = (previous - corner).normalized();
        let out = (next - corner).normalized();
        let reach = radius
            .min((previous - corner).length() * 0.5)
            .min((next - corner).length() * 0.5);
        if reach < 0.5 || !into.is_finite() || !out.is_finite() {
            points.push(corner);
            continue;
        }
        let start = corner + into * reach;
        let end = corner + out * reach;
        // A quadratic curve through the corner looks like a rounded corner.
        let steps = 5;
        for step in 0..=steps {
            let t = step as f32 / steps as f32;
            let a = start.lerp(corner, t);
            let b = corner.lerp(end, t);
            points.push(a.lerp(b, t));
        }
    }
    points
}

fn object_menu(ui: &mut egui::Ui, path: &mut String, filter: Filter, objects: &[ObjectInfo]) {
    let mut any = false;
    egui::ScrollArea::vertical()
        .max_height(280.0)
        .show(ui, |ui| {
            for object in objects.iter().filter(|object| filter.matches(object.class)) {
                any = true;
                let text = theme::with_icon(theme::class_icon(object.class), &object.path);
                if ui.selectable_label(*path == object.path, text).clicked() {
                    *path = object.path.clone();
                    ui.close();
                }
            }
        });
    if !any {
        ui.weak("No objects of this kind in the Workspace yet.");
    }
    ui.separator();
    ui.weak("Or type its path (Model.Part):");
    ui.text_edit_singleline(path);
}

fn property_menu(ui: &mut egui::Ui, name: &mut String, class: Option<ClassName>) {
    let names: Vec<&'static str> = match class {
        Some(class) => class
            .default_properties()
            .into_iter()
            .map(|(name, _)| name)
            .collect(),
        None => vec![
            "Position",
            "Size",
            "Orientation",
            "Color",
            "Anchored",
            "Visible",
        ],
    };
    egui::ScrollArea::vertical()
        .max_height(280.0)
        .show(ui, |ui| {
            for property in names {
                if ui.selectable_label(name == property, property).clicked() {
                    *name = property.to_owned();
                    ui.close();
                }
            }
        });
    ui.separator();
    ui.text_edit_singleline(name);
}

fn variable_menu(ui: &mut egui::Ui, name: &mut String, variables: &[String]) {
    for variable in variables {
        if ui.selectable_label(name == variable, variable).clicked() {
            *name = variable.clone();
            ui.close();
        }
    }
    ui.separator();
    ui.weak("Rename:");
    ui.text_edit_singleline(name);
    if !valid_identifier(name) {
        ui.colored_label(
            ui.visuals().error_fg_color,
            "Letters, digits and _ only, not starting with a digit",
        );
    }
}

/// The names the Animator's controller has of this kind, and a box to type
/// another.
fn name_menu(ui: &mut egui::Ui, name: &mut String, kind: NameKind, suggestions: &[String]) {
    for suggestion in suggestions {
        if ui
            .selectable_label(name == suggestion, suggestion)
            .clicked()
        {
            *name = suggestion.clone();
            ui.close();
        }
    }
    if suggestions.is_empty() {
        ui.weak(format!(
            "Its state machine has no {} yet (or pick the Animator first).",
            kind.what()
        ));
    }
    ui.separator();
    ui.weak(format!("Or type the {}'s name:", kind.what()));
    ui.text_edit_singleline(name);
}

fn key_menu(ui: &mut egui::Ui, key: &mut String) {
    egui::ScrollArea::vertical()
        .max_height(280.0)
        .show(ui, |ui| {
            for code in KeyCode::ALL {
                let name = code.as_str();
                if ui.selectable_label(key == name, name).clicked() {
                    *key = name.to_owned();
                    ui.close();
                }
            }
        });
}

/// A block or reporter in the palette.
enum PaletteItem {
    Block(BlockKind),
    Reporter(Expression),
    /// The "Make a Variable" button.
    NewVariable,
}

/// The palette's blocks for `category`. Object menus start on the first
/// object of their kind, so a new block usually works as it is.
fn palette(
    category: Category,
    client: bool,
    objects: &[ObjectInfo],
    variables: &[String],
) -> Vec<PaletteItem> {
    let first = |filter: Filter| {
        objects
            .iter()
            .find(|object| filter.matches(object.class))
            .map_or_else(
                || filter.placeholder().to_owned(),
                |object| object.path.clone(),
            )
    };
    let number = |value| Expression::Number { value };
    let text = |value: &str| Expression::Text {
        value: value.to_owned(),
    };
    let vector = |x, y, z| Expression::Vector3 { x, y, z };
    let condition = || Expression::Boolean { value: false };
    let part = first(Filter::Parts);
    let sound = first(Filter::Sounds);
    let variable = variables
        .first()
        .cloned()
        .unwrap_or_else(|| "score".to_owned());
    use PaletteItem::{Block as B, Reporter as R};
    let mut items = match category {
        Category::Events => vec![
            B(BlockKind::WhenStarted { body: Vec::new() }),
            B(BlockKind::WhenTouched {
                part: part.clone(),
                body: Vec::new(),
            }),
            B(BlockKind::WhenKeyPressed {
                key: "Space".to_owned(),
                body: Vec::new(),
            }),
            B(BlockKind::WhenClicked {
                button: first(Filter::Buttons),
                body: Vec::new(),
            }),
            B(BlockKind::EveryFrame { body: Vec::new() }),
            B(BlockKind::WhenSoundEnded {
                sound: sound.clone(),
                body: Vec::new(),
            }),
        ],
        Category::Control => vec![
            B(BlockKind::Wait {
                seconds: number(1.0),
            }),
            B(BlockKind::Repeat {
                count: number(10.0),
                body: Vec::new(),
            }),
            B(BlockKind::Forever { body: Vec::new() }),
            B(BlockKind::If {
                condition: condition(),
                body: Vec::new(),
            }),
            B(BlockKind::IfElse {
                condition: condition(),
                then_body: Vec::new(),
                else_body: Vec::new(),
            }),
            B(BlockKind::WaitUntil {
                condition: condition(),
            }),
            B(BlockKind::RepeatUntil {
                condition: condition(),
                body: Vec::new(),
            }),
            B(BlockKind::StopScript),
        ],
        Category::Motion => vec![
            B(BlockKind::MoveTo {
                target: part.clone(),
                position: vector(0.0, 5.0, 0.0),
            }),
            B(BlockKind::MoveBy {
                target: part.clone(),
                offset: vector(0.0, 1.0, 0.0),
            }),
            B(BlockKind::TurnTo {
                target: part.clone(),
                orientation: vector(0.0, 0.0, 0.0),
            }),
            B(BlockKind::TurnBy {
                target: part.clone(),
                rotation: vector(0.0, 15.0, 0.0),
            }),
            B(BlockKind::SetAnchored {
                target: part.clone(),
                anchored: Expression::Boolean { value: true },
            }),
        ],
        Category::Looks => vec![
            B(BlockKind::SetColor {
                target: part.clone(),
                color: Expression::Color3 {
                    r: 1.0,
                    g: 0.3,
                    b: 0.3,
                },
            }),
            B(BlockKind::SetText {
                target: first(Filter::Texts),
                text: text("Hello!"),
            }),
            B(BlockKind::Show {
                target: first(Filter::Gui),
            }),
            B(BlockKind::Hide {
                target: first(Filter::Gui),
            }),
            B(BlockKind::ShowPage {
                layout: first(Filter::PageLayouts),
                page: text("Home"),
            }),
            B(BlockKind::SetProperty {
                target: part.clone(),
                property: "Size".to_owned(),
                value: vector(4.0, 1.0, 2.0),
            }),
        ],
        Category::Sound => vec![
            B(BlockKind::PlaySound {
                target: sound.clone(),
            }),
            B(BlockKind::StopSound {
                target: sound.clone(),
            }),
            B(BlockKind::PauseSound {
                target: sound.clone(),
            }),
            B(BlockKind::ResumeSound {
                target: sound.clone(),
            }),
            B(BlockKind::SetVolume {
                target: sound,
                volume: number(0.5),
            }),
        ],
        Category::Operators => {
            let math = |op| {
                R(Expression::Math {
                    op,
                    left: Box::new(number(0.0)),
                    right: Box::new(number(0.0)),
                })
            };
            let compare = |op, right| {
                R(Expression::Compare {
                    op,
                    left: Box::new(text("")),
                    right: Box::new(number(right)),
                })
            };
            let logic = |op| {
                R(Expression::Logic {
                    op,
                    left: Box::new(condition()),
                    right: Box::new(condition()),
                })
            };
            vec![
                math(MathOp::Add),
                math(MathOp::Subtract),
                math(MathOp::Multiply),
                math(MathOp::Divide),
                R(Expression::Random {
                    min: Box::new(number(1.0)),
                    max: Box::new(number(10.0)),
                }),
                compare(Comparison::Greater, 50.0),
                compare(Comparison::Less, 50.0),
                compare(Comparison::Equal, 50.0),
                logic(LogicOp::And),
                logic(LogicOp::Or),
                R(Expression::Not {
                    value: Box::new(condition()),
                }),
                R(Expression::Join {
                    left: Box::new(text("apple ")),
                    right: Box::new(text("banana")),
                }),
                math(MathOp::Modulo),
                R(Expression::Function {
                    function: MathFunction::Round,
                    value: Box::new(number(0.0)),
                }),
            ]
        }
        Category::Variables => {
            let mut items = vec![PaletteItem::NewVariable];
            items.extend(
                variables
                    .iter()
                    .map(|name| R(Expression::Variable { name: name.clone() })),
            );
            items.push(B(BlockKind::SetVariable {
                name: variable.clone(),
                value: number(0.0),
            }));
            items.push(B(BlockKind::ChangeVariable {
                name: variable,
                by: number(1.0),
            }));
            items
        }
        Category::Sensing => vec![
            R(Expression::TouchingPart),
            R(Expression::KeyDown {
                key: "Space".to_owned(),
            }),
            R(Expression::Property {
                target: part,
                property: "Position".to_owned(),
            }),
            R(Expression::Raycast {
                origin: Box::new(vector(0.0, 10.0, 0.0)),
                direction: Box::new(vector(0.0, -20.0, 0.0)),
            }),
        ],
        Category::Animation => {
            let animator = first(Filter::Animators);
            let name = |kind: NameKind| {
                objects
                    .iter()
                    .find(|object| object.path == animator)
                    .and_then(|object| object.names.iter().find(|(of, _)| *of == kind))
                    .map_or_else(|| kind.example().to_owned(), |(_, name)| name.clone())
            };
            vec![
                B(BlockKind::SetAnimatorTrigger {
                    animator: animator.clone(),
                    parameter: name(NameKind::Trigger),
                }),
                B(BlockKind::SetAnimatorBool {
                    animator: animator.clone(),
                    parameter: name(NameKind::Bool),
                    value: Expression::Boolean { value: true },
                }),
                B(BlockKind::SetAnimatorNumber {
                    animator: animator.clone(),
                    parameter: name(NameKind::Number),
                    value: number(1.0),
                }),
                B(BlockKind::ResetAnimatorTrigger {
                    animator: animator.clone(),
                    parameter: name(NameKind::Trigger),
                }),
                B(BlockKind::PlayAnimatorState {
                    animator: animator.clone(),
                    state: name(NameKind::State),
                }),
                B(BlockKind::PlayAnimation {
                    animator: animator.clone(),
                    animation: first(Filter::Animations),
                }),
                B(BlockKind::WhenStateChanged {
                    animator: animator.clone(),
                    body: Vec::new(),
                }),
                B(BlockKind::WhenAnimationEvent {
                    animator: animator.clone(),
                    event: name(NameKind::Event),
                    body: Vec::new(),
                }),
                R(Expression::AnimatorBool {
                    animator: animator.clone(),
                    parameter: name(NameKind::Bool),
                }),
                R(Expression::AnimatorNumber {
                    animator: animator.clone(),
                    parameter: name(NameKind::Number),
                }),
                R(Expression::Property {
                    target: animator,
                    property: "CurrentState".to_owned(),
                }),
            ]
        }
        Category::More => vec![
            B(BlockKind::Print {
                value: text("Hello!"),
            }),
            B(BlockKind::Destroy {
                target: first(Filter::Parts),
            }),
        ],
        // Grey blocks only come from code, so there are none to drag out.
        Category::Lua => Vec::new(),
    };
    // Keyboard and button blocks only work on the player's side.
    if !client {
        items.retain(|item| match item {
            PaletteItem::Block(kind) => !kind.client_only(),
            PaletteItem::Reporter(expression) => !expression.client_only(),
            PaletteItem::NewVariable => true,
        });
    }
    items
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

fn show_palette(
    ui: &mut egui::Ui,
    rect: Rect,
    state: &mut State,
    context: &BlockContext,
    variables: &[String],
) {
    let background = ui.visuals().faint_bg_color;
    ui.painter().rect_filled(rect, 0, background);
    ui.painter().vline(
        rect.right() - 0.5,
        rect.y_range(),
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    child.set_clip_rect(rect);
    let scroll_to = state.scroll_to.take();
    let mut new_variable = state.new_variable.take();
    let mut made = None;
    let mut started = None;
    egui::ScrollArea::vertical()
        .id_salt(("block palette", context.key))
        .auto_shrink(false)
        .show(&mut child, |ui| {
            ui.add_space(8.0);
            for category in Category::ALL {
                let items = palette(category, context.client, context.objects, variables);
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    let (dot, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
                    ui.painter()
                        .circle_filled(dot.center(), 6.0, category.fill());
                    let heading = ui.label(
                        egui::RichText::new(category.name())
                            .strong()
                            .family(theme::heading_family()),
                    );
                    if scroll_to == Some(category) {
                        heading.scroll_to_me(Some(egui::Align::TOP));
                    }
                });
                ui.add_space(6.0);
                for item in items {
                    ui.horizontal(|ui| {
                        ui.add_space(14.0);
                        match item {
                            PaletteItem::NewVariable => {
                                if let Some(name) = &mut new_variable {
                                    let edit = ui.add(
                                        egui::TextEdit::singleline(name)
                                            .hint_text("Variable name")
                                            .desired_width(150.0),
                                    );
                                    edit.request_focus();
                                    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                                    if enter || ui.button("OK").clicked() {
                                        made = Some(name.clone());
                                    } else if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                        made = Some(String::new());
                                    }
                                } else if ui.button("Make a Variable").clicked() {
                                    new_variable = Some(String::new());
                                }
                            }
                            PaletteItem::Block(kind) => {
                                if let Some(drag) =
                                    palette_block(ui, kind, context, variables, background)
                                {
                                    started = Some(drag);
                                }
                            }
                            PaletteItem::Reporter(expression) => {
                                if let Some(drag) =
                                    palette_reporter(ui, expression, context, variables)
                                {
                                    started = Some(drag);
                                }
                            }
                        }
                    });
                    ui.add_space(8.0);
                }
                ui.add_space(14.0);
            }
        });
    if let Some(name) = made {
        new_variable = None;
        let name = name.trim().to_owned();
        if valid_identifier(&name) && !variables.contains(&name) {
            state.variables.push(name);
        }
    }
    state.new_variable = new_variable;
    if started.is_some() {
        state.drag = started;
    }
}

/// A block in the palette, which can be dragged out. Returns the drag if
/// one started.
fn palette_block(
    ui: &mut egui::Ui,
    kind: BlockKind,
    context: &BlockContext,
    variables: &[String],
    background: Color32,
) -> Option<Drag> {
    let event = kind.is_event();
    let mut block = Block::new(kind.clone());
    // Measured with a painter that draws nothing, then drawn where it fits.
    let measure = ui.painter().with_clip_rect(Rect::NOTHING);
    let mut painter = BlockPainter::new(
        measure,
        1.0,
        false,
        background,
        egui::Id::NULL,
        context,
        variables,
    );
    let size = if event {
        let drawn = painter.hat(ui, &mut block, Pos2::ZERO);
        vec2(drawn.width, drawn.height + TAB_DEPTH)
    } else {
        let drawn = painter.statement(ui, &mut block, Pos2::ZERO, true);
        vec2(drawn.width, drawn.height + TAB_DEPTH)
    };
    let (rect, response) = ui.allocate_exact_size(size, Sense::drag());
    let response = response.on_hover_cursor(CursorIcon::Grab);
    let mut painter = BlockPainter::new(
        ui.painter().clone(),
        1.0,
        false,
        background,
        egui::Id::NULL,
        context,
        variables,
    );
    if event {
        painter.hat(ui, &mut block, rect.min);
    } else {
        painter.statement(ui, &mut block, rect.min, true);
    }
    painter.finish();
    if response.drag_started()
        && let Some(origin) = response.interact_pointer_pos()
    {
        return Some(Drag {
            item: Dragged::Stack(vec![Block::new(kind)]),
            grab: origin - rect.min,
            size,
        });
    }
    None
}

fn palette_reporter(
    ui: &mut egui::Ui,
    mut expression: Expression,
    context: &BlockContext,
    variables: &[String],
) -> Option<Drag> {
    let background = ui.visuals().faint_bg_color;
    let painter = BlockPainter::new(
        ui.painter().clone(),
        1.0,
        false,
        background,
        egui::Id::NULL,
        context,
        variables,
    );
    let size = painter.reporter_size(ui, &mut expression);
    let (rect, response) = ui.allocate_exact_size(size, Sense::drag());
    let response = response.on_hover_cursor(CursorIcon::Grab);
    let mut painter = painter;
    painter.reporter(ui, &mut expression.clone(), rect.min, egui::Id::NULL, None);
    if response.drag_started()
        && let Some(origin) = response.interact_pointer_pos()
    {
        return Some(Drag {
            item: Dragged::Reporter(expression),
            grab: origin - rect.min,
            size,
        });
    }
    None
}

/// The canvas with the script's stacks. Returns where dragged blocks can go.
fn show_canvas(
    ui: &mut egui::Ui,
    rect: Rect,
    script: &mut BlockScript,
    state: &mut State,
    context: &BlockContext,
    variables: &[String],
) -> (StackSpots, SlotSpots) {
    let dark = ui.visuals().dark_mode;
    let background = if dark {
        Color32::from_rgb(30, 33, 41)
    } else {
        Color32::from_rgb(249, 249, 251)
    };
    let mut canvas = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    canvas.set_clip_rect(rect);
    let painter = canvas.painter_at(rect);
    painter.rect_filled(rect, 0, background);

    // Dragging the background scrolls; so do the wheel and trackpad, and
    // Ctrl + wheel (or pinching) zooms around the pointer.
    let back = canvas.interact(
        rect,
        egui::Id::new(("block canvas", context.key)),
        Sense::click_and_drag(),
    );
    if back.dragged() {
        state.pan += back.drag_delta();
    }
    if canvas.rect_contains_pointer(rect) && state.drag.is_none() {
        let (scroll, zoom, pointer) =
            canvas.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.pointer.hover_pos()));
        state.pan += scroll;
        if zoom != 1.0
            && let Some(pointer) = pointer
        {
            let old = state.zoom;
            state.zoom = (old * zoom).clamp(0.5, 2.0);
            let from_origin = pointer - rect.min - state.pan;
            state.pan = pointer - rect.min - from_origin * (state.zoom / old);
        }
    }
    let z = state.zoom;

    // A dot grid, which moves with the blocks.
    let spacing = 24.0 * z;
    let dot = if dark {
        Color32::from_white_alpha(18)
    } else {
        Color32::from_black_alpha(22)
    };
    let start = rect.min
        + vec2(
            state.pan.x.rem_euclid(spacing),
            state.pan.y.rem_euclid(spacing),
        );
    let mut y = start.y;
    while y < rect.bottom() {
        let mut x = start.x;
        while x < rect.right() {
            painter.circle_filled(pos2(x, y), 1.2, dot);
            x += spacing;
        }
        y += spacing;
    }

    if script.blocks.is_empty() {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Drag a yellow Events block, such as \"when game starts\", here to begin.",
            FontId::proportional(14.0),
            ui.visuals().weak_text_color(),
        );
    }

    let origin = rect.min + state.pan;
    let mut blocks = BlockPainter::new(
        painter.clone(),
        z,
        true,
        background,
        egui::Id::new(("blocks", context.key)),
        context,
        variables,
    );
    let mut auto_y = 20.0;
    let mut grabbed_script = None;
    let count = script.blocks.len();
    for index in 0..count {
        let block = &mut script.blocks[index];
        let position = block.position.unwrap_or([20.0, auto_y]);
        let at = origin + vec2(position[0], position[1]) * z;
        if matches!(block.kind, BlockKind::Unattached { .. }) {
            blocks.stack_spots.push((at, StackSpot::Above(index)));
        }
        let drawn = blocks.script(&mut canvas, block, at);
        if let Some(grab) = drawn.grabbed {
            grabbed_script = Some((index, grab));
        }
        auto_y = position[1] + drawn.height / z + STACK_GAP;
    }
    blocks.finish();

    // What was done to blocks this frame.
    for action in std::mem::take(&mut blocks.actions) {
        apply_action(script, action);
    }
    if let Some((index, grab)) = grabbed_script {
        let block = script.blocks.remove(index);
        blocks.started = Some(Drag {
            item: Dragged::Stack(vec![block]),
            grab,
            size: Vec2::ZERO,
        });
    }
    if let Some(drag) = blocks.started.take() {
        state.drag = Some(drag);
    }
    // A loose stack that's been emptied is gone.
    script
        .blocks
        .retain(|block| !matches!(&block.kind, BlockKind::Unattached { body } if body.is_empty()));
    // Positions are only stored once a stack is moved, so stacks that
    // haven't been don't change the file.
    (blocks.stack_spots, blocks.slot_spots)
}

fn apply_action(script: &mut BlockScript, action: Action) {
    match action {
        Action::Delete(id) => {
            if let Some((stack, index)) = script.stack_of_mut(id) {
                stack.remove(index);
            }
        }
        Action::Duplicate(id) => {
            if let Some(index) = script.blocks.iter().position(|block| block.id == id) {
                let mut copy = script.blocks[index].duplicate();
                let [x, y] = copy.position.unwrap_or([20.0, 20.0]);
                copy.position = Some([x + 30.0, y + 30.0]);
                script.blocks.push(copy);
            } else if let Some((stack, index)) = script.stack_of_mut(id) {
                let copy = stack[index].duplicate();
                stack.insert(index + 1, copy);
            }
        }
    }
}

/// Draws what's being dragged under the pointer, shows where it would go,
/// and drops it when the mouse button is let go.
#[allow(clippy::too_many_arguments)]
fn finish_drag(
    ui: &mut egui::Ui,
    state: &mut State,
    script: &mut BlockScript,
    context: &BlockContext,
    variables: &[String],
    canvas: Rect,
    palette: Rect,
    stack_spots: &[(Pos2, StackSpot)],
    slot_spots: &[(Rect, SlotAddress, SlotKind)],
) {
    let Some(mut drag) = state.drag.take() else {
        return;
    };
    let (pointer, down) = ui.input(|i| (i.pointer.latest_pos(), i.pointer.primary_down()));
    let Some(pointer) = pointer else {
        state.drag = Some(drag);
        return;
    };
    ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
    let z = state.zoom;
    let top_left = pointer - drag.grab;
    let over_palette = palette.contains(pointer);

    // Where it would go.
    let mut stack_target = None;
    let mut slot_target = None;
    match &drag.item {
        Dragged::Stack(blocks) => {
            let event = blocks.first().is_some_and(|block| block.kind.is_event());
            let ends_in_cap = blocks.last().is_some_and(|block| block.kind.is_cap());
            if !event && !over_palette {
                let mut best = SNAP * z;
                for (at, spot) in stack_spots {
                    let fits = match spot {
                        StackSpot::After { last, .. } => !ends_in_cap || *last,
                        StackSpot::Mouth { empty, .. } => !ends_in_cap || *empty,
                        StackSpot::Above(_) => !ends_in_cap,
                    };
                    let place = match spot {
                        StackSpot::Above(_) => *at - vec2(0.0, drag.size.y - TAB_DEPTH * z),
                        _ => *at,
                    };
                    let distance = (top_left - place).length();
                    if fits && distance < best {
                        best = distance;
                        stack_target = Some((place, spot.clone()));
                    }
                }
            }
        }
        Dragged::Reporter(expression) => {
            let shape = expression.shape();
            slot_target = slot_spots
                .iter()
                .filter(|(rect, _, kind)| rect.expand(4.0).contains(pointer) && kind.accepts(shape))
                .min_by(|a, b| a.0.area().total_cmp(&b.0.area()))
                .cloned();
        }
    }

    let layer = ui.ctx().layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        egui::Id::new("dragged blocks"),
    ));
    // Where the blocks would snap: a grey shadow, as in Scratch.
    if let Some((place, _)) = &stack_target {
        let shadow = Rect::from_min_size(
            *place,
            vec2(
                drag.size.x.max(MIN_WIDTH * z),
                drag.size.y.max(ROW_HEIGHT * z),
            ),
        );
        layer.rect_filled(
            shadow,
            radius(CORNER * z),
            Color32::from_gray(128).gamma_multiply(0.45),
        );
    }
    if let Some((rect, _, _)) = &slot_target {
        layer.rect_stroke(
            rect.expand(2.0),
            radius(rect.height() * 0.5),
            Stroke::new(2.5_f32, Color32::WHITE),
            egui::StrokeKind::Outside,
        );
    }
    if over_palette {
        layer.rect_filled(palette, 0, Color32::from_black_alpha(60));
        layer.text(
            palette.center(),
            egui::Align2::CENTER_CENTER,
            theme::with_icon(icons::TRASH, "Drop to delete"),
            FontId::proportional(16.0),
            Color32::WHITE,
        );
    }

    // The dragged blocks, slightly see-through, under the pointer.
    let mut preview = BlockPainter::new(
        layer.clone(),
        z,
        false,
        Color32::TRANSPARENT,
        egui::Id::new("drag preview"),
        context,
        variables,
    );
    match &drag.item {
        Dragged::Stack(blocks) => {
            let mut blocks = blocks.clone();
            let mut height = 0.0;
            let mut width: f32 = 0.0;
            if blocks.first().is_some_and(|block| block.kind.is_event()) {
                for block in &mut blocks {
                    let drawn = preview.script(ui, block, top_left + vec2(0.0, height));
                    height += drawn.height;
                    width = width.max(drawn.width);
                }
            } else {
                height = preview.stack(ui, &mut blocks, top_left, true);
                width = MIN_WIDTH * z;
            }
            drag.size = vec2(width, height + TAB_DEPTH * z);
        }
        Dragged::Reporter(expression) => {
            drag.size = preview.reporter(
                ui,
                &mut expression.clone(),
                top_left,
                egui::Id::new("drag preview"),
                None,
            );
        }
    }
    preview.finish();
    ui.ctx().request_repaint();

    if down {
        state.drag = Some(drag);
        return;
    }
    // Let go: drop it.
    if over_palette {
        return;
    }
    match drag.item {
        Dragged::Stack(blocks) => {
            if let Some((_, spot)) = stack_target {
                drop_stack(script, spot, blocks, drag.size.y / z);
            } else if canvas.contains(pointer) || !blocks.is_empty() {
                let at = (top_left - canvas.min - state.pan) / z;
                let position = Some([at.x.max(0.0), at.y.max(0.0)]);
                let block = if blocks.len() == 1 && blocks[0].kind.is_event() {
                    let mut block = blocks.into_iter().next().expect("one block");
                    block.position = position;
                    block
                } else {
                    let mut block = Block::new(BlockKind::Unattached { body: blocks });
                    block.position = position;
                    block
                };
                script.blocks.push(block);
            }
        }
        Dragged::Reporter(expression) => {
            if let Some((_, address, _)) = slot_target
                && let Some(slot) = address.resolve(script)
            {
                *slot = expression;
            }
            // Dropped anywhere else, a reporter is deleted.
        }
    }
}

fn drop_stack(script: &mut BlockScript, spot: StackSpot, blocks: Vec<Block>, height: f32) {
    match spot {
        StackSpot::After { block, .. } => {
            if let Some((stack, index)) = script.stack_of_mut(block) {
                stack.splice(index + 1..index + 1, blocks);
            }
        }
        StackSpot::Mouth { block, mouth, .. } => {
            if let Some(found) = script.find_mut(block)
                && let Some(body) = found.kind.bodies_mut().into_iter().nth(mouth)
            {
                body.splice(0..0, blocks);
            }
        }
        StackSpot::Above(index) => {
            if let Some(top) = script.blocks.get_mut(index)
                && let BlockKind::Unattached { body } = &mut top.kind
            {
                body.splice(0..0, blocks);
                // The stack grows upwards, so what was there stays put.
                let [x, y] = top.position.unwrap_or([20.0, 20.0]);
                top.position = Some([x, (y - height + TAB_DEPTH).max(0.0)]);
            }
        }
    }
}

fn show_lua(ui: &mut egui::Ui, rect: Rect, script: &BlockScript) {
    ui.painter()
        .rect_filled(rect, 0, ui.visuals().extreme_bg_color);
    ui.painter().vline(
        rect.left() + 0.5,
        rect.y_range(),
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
    let mut panel = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(12.0)));
    panel.label(
        egui::RichText::new(theme::with_icon(icons::CODE, "Lua"))
            .strong()
            .family(theme::heading_family()),
    );
    panel.weak("What these blocks become. It runs exactly like a typed script.");
    panel.separator();
    match translate(script) {
        Ok(translation) => {
            egui::ScrollArea::both()
                .id_salt("block lua")
                .auto_shrink(false)
                .show(&mut panel, |ui| {
                    let source = if translation.source.is_empty() {
                        "-- Nothing runs yet: add an Events block.".to_owned()
                    } else {
                        translation.source
                    };
                    let job =
                        lua_syntax::layout_job(&source, FontId::monospace(12.0), ui.visuals());
                    ui.add(egui::Label::new(job).selectable(true).extend());
                });
        }
        Err(err) => {
            panel.colored_label(panel.visuals().error_fg_color, err.to_string());
        }
    }
}

#[cfg(test)]
mod tests;
