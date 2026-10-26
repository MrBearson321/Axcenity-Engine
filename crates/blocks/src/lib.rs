//! Scratch-style script data and its one-way translation to Lua.
//!
//! A [`BlockScript`] is a canvas of stacks. Each stack starts with an event
//! block (a "hat", such as "when game starts") whose body runs when the
//! event happens, or is an [`BlockKind::Unattached`] stack of loose blocks
//! that doesn't run, like blocks left lying around in Scratch. Statement
//! blocks hold values in slots: plain values typed in (numbers, text, ...)
//! or reporter blocks ([`Expression`]) that work a value out.
//!
//! Every block translates to the documented Lua API (see `docs/scripting/blocks.md`),
//! so a block script runs exactly like a typed one. [`from_lua`] goes the
//! other way, for switching a script between Code and Blocks (or Graph:
//! the same blocks as nodes and wires, laid out by [`Graph`]).

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

mod translate;

pub use translate::*;

static NEXT_BLOCK_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockShape {
    /// Starts a stack ("when ..."): only at the top level.
    Event,
    /// One step in a stack.
    Statement,
    /// A round reporter: a number, text or other value.
    Value,
    /// A pointed reporter: true or false.
    Boolean,
}

impl BlockShape {
    /// Whether a block of shape `child` fits where a block of shape `self`
    /// goes. As in Scratch, a true-or-false reporter also fits a value slot,
    /// but only true-or-false reporters fit a condition.
    pub fn accepts(self, child: BlockShape) -> bool {
        matches!(
            (self, child),
            (BlockShape::Event, BlockShape::Event)
                | (BlockShape::Statement, BlockShape::Statement)
                | (BlockShape::Value, BlockShape::Value | BlockShape::Boolean)
                | (BlockShape::Boolean, BlockShape::Boolean)
        )
    }
}

/// Which side of the game a block script runs on: blocks that read the
/// keyboard or UI clicks only work on the player's side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Server,
    Client,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockScript {
    pub blocks: Vec<Block>,
    /// How the script looks as a node graph. Written only once it's been
    /// shown as one, so other block scripts don't change.
    #[serde(default, skip_serializing_if = "Graph::is_empty")]
    pub graph: Graph,
}

/// The node-graph view of a block script (see `docs/scripting/graphs.md`).
/// The blocks are the script; this is only where the nodes are, which view
/// is shown, and value nodes wired to nothing, so it never changes the Lua.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Graph {
    /// The script is shown as a node graph rather than as blocks.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub shown: bool,
    /// Where nodes have been placed, in graph units, by node key: a block's
    /// id (`"12"`), or a value node's owner id and slots (`"12.0.1"`).
    /// Nodes without one are laid out after the node they're wired to.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub nodes: BTreeMap<String, [f32; 2]>,
    /// Value nodes wired to nothing. They don't run.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub loose: Vec<LooseValue>,
}

impl Graph {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// A value node wired to nothing, with its own id (from the blocks' ids).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LooseValue {
    pub id: u64,
    pub value: Expression,
}

impl LooseValue {
    pub fn new(value: Expression) -> Self {
        Self {
            id: NEXT_BLOCK_ID.fetch_add(1, Ordering::Relaxed),
            value,
        }
    }
}

impl BlockScript {
    /// The block with `id`, wherever it is.
    pub fn find_mut(&mut self, id: u64) -> Option<&mut Block> {
        fn find(blocks: &mut [Block], id: u64) -> Option<&mut Block> {
            for block in blocks {
                if block.id == id {
                    return Some(block);
                }
                for body in block.kind.bodies_mut() {
                    if let Some(found) = find(body, id) {
                        return Some(found);
                    }
                }
            }
            None
        }
        find(&mut self.blocks, id)
    }

    /// The stack holding the block with `id`, and where in it the block is.
    pub fn stack_of_mut(&mut self, id: u64) -> Option<(&mut Vec<Block>, usize)> {
        fn find(blocks: &mut Vec<Block>, id: u64) -> Option<(&mut Vec<Block>, usize)> {
            if let Some(index) = blocks.iter().position(|block| block.id == id) {
                return Some((blocks, index));
            }
            for block in blocks.iter_mut() {
                for body in block.kind.bodies_mut() {
                    if let Some(found) = find(body, id) {
                        return Some(found);
                    }
                }
            }
            None
        }
        find(&mut self.blocks, id)
    }

    /// Calls `visit` for every block in the script, stacks and bodies alike,
    /// parents before their bodies.
    pub fn for_each_block(&self, visit: &mut impl FnMut(&Block)) {
        fn walk(blocks: &[Block], visit: &mut impl FnMut(&Block)) {
            for block in blocks {
                visit(block);
                for body in block.kind.bodies() {
                    walk(body, visit);
                }
            }
        }
        walk(&self.blocks, visit);
    }
}

/// Makes sure blocks made from now on get IDs that `script` doesn't use.
pub fn reserve_block_ids(script: &BlockScript) {
    let mut maximum = 0;
    script.for_each_block(&mut |block| maximum = maximum.max(block.id));
    for loose in &script.graph.loose {
        maximum = maximum.max(loose.id);
    }
    NEXT_BLOCK_ID.fetch_max(maximum.saturating_add(1), Ordering::Relaxed);
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Block {
    pub id: u64,
    pub kind: BlockKind,
    /// Where a stack's top is on the editor's canvas, for top-level blocks.
    /// `None` lays it out below the stack before it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<[f32; 2]>,
}

impl Block {
    pub fn new(kind: BlockKind) -> Self {
        Self {
            id: NEXT_BLOCK_ID.fetch_add(1, Ordering::Relaxed),
            kind,
            position: None,
        }
    }

    /// A copy with new IDs, for duplicating blocks.
    pub fn duplicate(&self) -> Self {
        let mut copy = self.clone();
        copy.renumber();
        copy
    }

    fn renumber(&mut self) {
        self.id = NEXT_BLOCK_ID.fetch_add(1, Ordering::Relaxed);
        for body in self.kind.bodies_mut() {
            for block in body {
                block.renumber();
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum BlockKind {
    // Events: the top of a stack.
    WhenStarted {
        body: Vec<Block>,
    },
    WhenTouched {
        part: String,
        body: Vec<Block>,
    },
    /// Client only.
    WhenKeyPressed {
        key: String,
        body: Vec<Block>,
    },
    /// When a TextButton is clicked or tapped. Client only.
    WhenClicked {
        button: String,
        body: Vec<Block>,
    },
    /// Runs every frame (`RunService.Heartbeat`).
    EveryFrame {
        body: Vec<Block>,
    },
    WhenSoundEnded {
        sound: String,
        body: Vec<Block>,
    },
    /// When an Animator moves to another state (`StateChanged`).
    WhenStateChanged {
        animator: String,
        body: Vec<Block>,
    },
    /// When an Animator's clip passes the event named `event`
    /// (`AnimationEvent`).
    WhenAnimationEvent {
        animator: String,
        event: String,
        body: Vec<Block>,
    },
    /// Loose blocks on the canvas, under no event. They don't run.
    Unattached {
        body: Vec<Block>,
    },
    /// Lua at the top of the script that has no blocks, kept word for
    /// word from a script switched from Code to Blocks. It runs where it is.
    TopCode {
        code: String,
    },

    // Control.
    Wait {
        seconds: Expression,
    },
    WaitUntil {
        #[serde(deserialize_with = "condition")]
        condition: Expression,
    },
    Repeat {
        count: Expression,
        body: Vec<Block>,
    },
    Forever {
        body: Vec<Block>,
    },
    RepeatUntil {
        #[serde(deserialize_with = "condition")]
        condition: Expression,
        body: Vec<Block>,
    },
    If {
        #[serde(deserialize_with = "condition")]
        condition: Expression,
        body: Vec<Block>,
    },
    IfElse {
        #[serde(deserialize_with = "condition")]
        condition: Expression,
        then_body: Vec<Block>,
        else_body: Vec<Block>,
    },
    /// Ends the event's handler early.
    StopScript,

    // Motion.
    MoveTo {
        target: String,
        position: Expression,
    },
    MoveBy {
        target: String,
        offset: Expression,
    },
    TurnTo {
        target: String,
        orientation: Expression,
    },
    TurnBy {
        target: String,
        rotation: Expression,
    },
    SetAnchored {
        target: String,
        anchored: Expression,
    },

    // Looks.
    SetColor {
        target: String,
        color: Expression,
    },
    SetText {
        target: String,
        text: Expression,
    },
    Show {
        target: String,
    },
    Hide {
        target: String,
    },
    /// Sets a PageLayout's current page to a Frame's name.
    ShowPage {
        layout: String,
        page: Expression,
    },
    SetProperty {
        target: String,
        property: String,
        value: Expression,
    },

    // Sound.
    PlaySound {
        target: String,
    },
    StopSound {
        target: String,
    },
    PauseSound {
        target: String,
    },
    ResumeSound {
        target: String,
    },
    SetVolume {
        target: String,
        volume: Expression,
    },

    // Animation: an Animator's parameters and states.
    SetAnimatorBool {
        animator: String,
        parameter: String,
        value: Expression,
    },
    SetAnimatorNumber {
        animator: String,
        parameter: String,
        value: Expression,
    },
    SetAnimatorTrigger {
        animator: String,
        parameter: String,
    },
    ResetAnimatorTrigger {
        animator: String,
        parameter: String,
    },
    PlayAnimatorState {
        animator: String,
        state: String,
    },
    /// Loads an Animation on an Animator and plays the track
    /// (`animator:LoadAnimation(animation):Play()`).
    PlayAnimation {
        animator: String,
        animation: String,
    },

    // Tweens: a property moving smoothly (`TweenService`).
    /// Moves one property of an object to `value` over `seconds`, eased by
    /// `style` (one of [`EASING_STYLES`]) and `direction` (one of
    /// [`EASING_DIRECTIONS`]).
    Tween {
        target: String,
        property: String,
        value: Expression,
        seconds: Expression,
        style: String,
        direction: String,
    },
    /// Slides a UI element in to where it is from one parent's width or
    /// height past an edge, `from` being one of [`SLIDE_EDGES`].
    SlideIn {
        target: String,
        from: String,
        seconds: Expression,
    },
    /// Fades a UI element's background in (`BackgroundTransparency` to 0).
    FadeIn {
        target: String,
        seconds: Expression,
    },
    /// Fades a UI element's background out (`BackgroundTransparency` to 1).
    FadeOut {
        target: String,
        seconds: Expression,
    },

    // Variables.
    SetVariable {
        name: String,
        value: Expression,
    },
    ChangeVariable {
        name: String,
        by: Expression,
    },

    // Other.
    Print {
        value: Expression,
    },
    Destroy {
        target: String,
    },

    /// Lua that has no blocks, kept word for word from a script switched
    /// from Code to Blocks.
    Code {
        code: String,
    },
}

/// `Enum.EasingStyle`'s items, which the tween block offers.
pub const EASING_STYLES: [&str; 7] = [
    "Linear", "Sine", "Quad", "Cubic", "Back", "Bounce", "Elastic",
];
/// `Enum.EasingDirection`'s items.
pub const EASING_DIRECTIONS: [&str; 3] = ["In", "Out", "InOut"];
/// The edges "slide in" can come from.
pub const SLIDE_EDGES: [&str; 4] = ["Left", "Right", "Top", "Bottom"];

impl BlockKind {
    pub fn shape(&self) -> BlockShape {
        if self.is_event() {
            BlockShape::Event
        } else {
            BlockShape::Statement
        }
    }

    /// Event blocks, and unattached stacks: what can be at the top level.
    pub fn is_event(&self) -> bool {
        matches!(
            self,
            BlockKind::WhenStarted { .. }
                | BlockKind::WhenTouched { .. }
                | BlockKind::WhenKeyPressed { .. }
                | BlockKind::WhenClicked { .. }
                | BlockKind::EveryFrame { .. }
                | BlockKind::WhenSoundEnded { .. }
                | BlockKind::WhenStateChanged { .. }
                | BlockKind::WhenAnimationEvent { .. }
                | BlockKind::Unattached { .. }
                | BlockKind::TopCode { .. }
        )
    }

    /// Whether this block only works in client block scripts (LocalScripts).
    pub fn client_only(&self) -> bool {
        matches!(
            self,
            BlockKind::WhenKeyPressed { .. } | BlockKind::WhenClicked { .. }
        )
    }

    /// The stacks of blocks inside this one, in order (an if/else's two).
    pub fn bodies(&self) -> Vec<&Vec<Block>> {
        match self {
            BlockKind::WhenStarted { body }
            | BlockKind::WhenTouched { body, .. }
            | BlockKind::WhenKeyPressed { body, .. }
            | BlockKind::WhenClicked { body, .. }
            | BlockKind::EveryFrame { body }
            | BlockKind::WhenSoundEnded { body, .. }
            | BlockKind::WhenStateChanged { body, .. }
            | BlockKind::WhenAnimationEvent { body, .. }
            | BlockKind::Unattached { body }
            | BlockKind::Repeat { body, .. }
            | BlockKind::Forever { body }
            | BlockKind::RepeatUntil { body, .. }
            | BlockKind::If { body, .. } => vec![body],
            BlockKind::IfElse {
                then_body,
                else_body,
                ..
            } => vec![then_body, else_body],
            _ => Vec::new(),
        }
    }

    pub fn bodies_mut(&mut self) -> Vec<&mut Vec<Block>> {
        match self {
            BlockKind::WhenStarted { body }
            | BlockKind::WhenTouched { body, .. }
            | BlockKind::WhenKeyPressed { body, .. }
            | BlockKind::WhenClicked { body, .. }
            | BlockKind::EveryFrame { body }
            | BlockKind::WhenSoundEnded { body, .. }
            | BlockKind::WhenStateChanged { body, .. }
            | BlockKind::WhenAnimationEvent { body, .. }
            | BlockKind::Unattached { body }
            | BlockKind::Repeat { body, .. }
            | BlockKind::Forever { body }
            | BlockKind::RepeatUntil { body, .. }
            | BlockKind::If { body, .. } => vec![body],
            BlockKind::IfElse {
                then_body,
                else_body,
                ..
            } => vec![then_body, else_body],
            _ => Vec::new(),
        }
    }

    /// Like [`BlockKind::expressions`], for changing them.
    pub fn expressions_mut(&mut self) -> Vec<&mut Expression> {
        match self {
            BlockKind::Wait { seconds: e }
            | BlockKind::WaitUntil { condition: e }
            | BlockKind::Repeat { count: e, .. }
            | BlockKind::RepeatUntil { condition: e, .. }
            | BlockKind::If { condition: e, .. }
            | BlockKind::IfElse { condition: e, .. }
            | BlockKind::MoveTo { position: e, .. }
            | BlockKind::MoveBy { offset: e, .. }
            | BlockKind::TurnTo { orientation: e, .. }
            | BlockKind::TurnBy { rotation: e, .. }
            | BlockKind::SetAnchored { anchored: e, .. }
            | BlockKind::SetColor { color: e, .. }
            | BlockKind::SetText { text: e, .. }
            | BlockKind::ShowPage { page: e, .. }
            | BlockKind::SetProperty { value: e, .. }
            | BlockKind::SetVolume { volume: e, .. }
            | BlockKind::SetAnimatorBool { value: e, .. }
            | BlockKind::SetAnimatorNumber { value: e, .. }
            | BlockKind::SetVariable { value: e, .. }
            | BlockKind::ChangeVariable { by: e, .. }
            | BlockKind::SlideIn { seconds: e, .. }
            | BlockKind::FadeIn { seconds: e, .. }
            | BlockKind::FadeOut { seconds: e, .. }
            | BlockKind::Print { value: e } => vec![e],
            BlockKind::Tween { value, seconds, .. } => vec![value, seconds],
            _ => Vec::new(),
        }
    }

    /// Tween blocks, which need `TweenService`.
    pub fn tweens(&self) -> bool {
        matches!(
            self,
            BlockKind::Tween { .. }
                | BlockKind::SlideIn { .. }
                | BlockKind::FadeIn { .. }
                | BlockKind::FadeOut { .. }
        )
    }

    /// Blocks nothing can go after: they never finish ("forever") or end
    /// the script ("stop this script").
    pub fn is_cap(&self) -> bool {
        matches!(self, BlockKind::Forever { .. } | BlockKind::StopScript)
    }

    /// The value slots this block has, in the order they're shown.
    pub fn expressions(&self) -> Vec<&Expression> {
        match self {
            BlockKind::Wait { seconds: e }
            | BlockKind::WaitUntil { condition: e }
            | BlockKind::Repeat { count: e, .. }
            | BlockKind::RepeatUntil { condition: e, .. }
            | BlockKind::If { condition: e, .. }
            | BlockKind::IfElse { condition: e, .. }
            | BlockKind::MoveTo { position: e, .. }
            | BlockKind::MoveBy { offset: e, .. }
            | BlockKind::TurnTo { orientation: e, .. }
            | BlockKind::TurnBy { rotation: e, .. }
            | BlockKind::SetAnchored { anchored: e, .. }
            | BlockKind::SetColor { color: e, .. }
            | BlockKind::SetText { text: e, .. }
            | BlockKind::ShowPage { page: e, .. }
            | BlockKind::SetProperty { value: e, .. }
            | BlockKind::SetVolume { volume: e, .. }
            | BlockKind::SetAnimatorBool { value: e, .. }
            | BlockKind::SetAnimatorNumber { value: e, .. }
            | BlockKind::SetVariable { value: e, .. }
            | BlockKind::ChangeVariable { by: e, .. }
            | BlockKind::SlideIn { seconds: e, .. }
            | BlockKind::FadeIn { seconds: e, .. }
            | BlockKind::FadeOut { seconds: e, .. }
            | BlockKind::Print { value: e } => vec![e],
            BlockKind::Tween { value, seconds, .. } => vec![value, seconds],
            _ => Vec::new(),
        }
    }
}

/// Reads a condition slot: a reporter, or the `{ left, operator, right }`
/// comparison the first block scripts saved, which becomes a compare block.
fn condition<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Expression, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Slot {
        Expression(Expression),
        Legacy {
            left: Expression,
            operator: Comparison,
            right: Expression,
        },
    }
    Ok(match Slot::deserialize(deserializer)? {
        Slot::Expression(expression) => expression,
        Slot::Legacy {
            left,
            operator,
            right,
        } => Expression::Compare {
            op: operator,
            left: Box::new(left),
            right: Box::new(right),
        },
    })
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Expression {
    // Values typed into a slot.
    Number {
        value: f64,
    },
    Text {
        value: String,
    },
    Boolean {
        value: bool,
    },
    Vector3 {
        x: f64,
        y: f64,
        z: f64,
    },
    Color3 {
        r: f64,
        g: f64,
        b: f64,
    },

    // Reporters.
    Variable {
        name: String,
    },
    Math {
        op: MathOp,
        left: Box<Expression>,
        right: Box<Expression>,
    },
    /// `math.abs` and friends, and rounding.
    Function {
        function: MathFunction,
        value: Box<Expression>,
    },
    /// A whole number from `min` to `max`, both included.
    Random {
        min: Box<Expression>,
        max: Box<Expression>,
    },
    /// Two values as text, one after the other.
    Join {
        left: Box<Expression>,
        right: Box<Expression>,
    },
    Compare {
        op: Comparison,
        left: Box<Expression>,
        right: Box<Expression>,
    },
    Logic {
        op: LogicOp,
        left: Box<Expression>,
        right: Box<Expression>,
    },
    Not {
        value: Box<Expression>,
    },
    /// An object's property, such as a part's `Position`.
    Property {
        target: String,
        property: String,
    },
    /// `workspace:Raycast(origin, direction)`.
    Raycast {
        origin: Box<Expression>,
        direction: Box<Expression>,
    },
    /// The part that touched, inside "when touched".
    TouchingPart,
    /// Whether a key is held down. Client only.
    KeyDown {
        key: String,
    },
    /// An Animator's Bool parameter (`animator:GetBool`).
    AnimatorBool {
        animator: String,
        parameter: String,
    },
    /// An Animator's Number parameter (`animator:GetNumber`).
    AnimatorNumber {
        animator: String,
        parameter: String,
    },
    /// Lua that has no block, kept word for word from a script switched
    /// from Code to Blocks. Pointed, so it fits any slot.
    Code {
        code: String,
    },
}

impl Expression {
    pub fn shape(&self) -> BlockShape {
        match self {
            Expression::Boolean { .. }
            | Expression::Compare { .. }
            | Expression::Logic { .. }
            | Expression::Not { .. }
            | Expression::KeyDown { .. }
            | Expression::AnimatorBool { .. }
            | Expression::Code { .. } => BlockShape::Boolean,
            _ => BlockShape::Value,
        }
    }

    /// A value typed in, rather than a reporter block.
    pub fn is_literal(&self) -> bool {
        matches!(
            self,
            Expression::Number { .. }
                | Expression::Text { .. }
                | Expression::Boolean { .. }
                | Expression::Vector3 { .. }
                | Expression::Color3 { .. }
        )
    }

    /// Whether this reporter only works in client block scripts.
    pub fn client_only(&self) -> bool {
        matches!(self, Expression::KeyDown { .. })
    }

    /// Like [`Expression::children`], for changing them.
    pub fn children_mut(&mut self) -> Vec<&mut Expression> {
        match self {
            Expression::Math { left, right, .. }
            | Expression::Join { left, right }
            | Expression::Compare { left, right, .. }
            | Expression::Logic { left, right, .. } => vec![left, right],
            Expression::Random { min, max } => vec![min, max],
            Expression::Raycast { origin, direction } => vec![origin, direction],
            Expression::Function { value, .. } | Expression::Not { value } => vec![value],
            _ => Vec::new(),
        }
    }

    /// The reporters directly inside this one.
    pub fn children(&self) -> Vec<&Expression> {
        match self {
            Expression::Math { left, right, .. }
            | Expression::Join { left, right }
            | Expression::Compare { left, right, .. }
            | Expression::Logic { left, right, .. } => vec![left, right],
            Expression::Random { min, max } => vec![min, max],
            Expression::Raycast { origin, direction } => vec![origin, direction],
            Expression::Function { value, .. } | Expression::Not { value } => vec![value],
            _ => Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Comparison {
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
}

impl Comparison {
    pub const ALL: [Comparison; 6] = [
        Comparison::Equal,
        Comparison::NotEqual,
        Comparison::Less,
        Comparison::LessOrEqual,
        Comparison::Greater,
        Comparison::GreaterOrEqual,
    ];

    pub fn lua(self) -> &'static str {
        match self {
            Self::Equal => "==",
            Self::NotEqual => "~=",
            Self::Less => "<",
            Self::LessOrEqual => "<=",
            Self::Greater => ">",
            Self::GreaterOrEqual => ">=",
        }
    }

    /// The symbol a block shows.
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Equal => "=",
            Self::NotEqual => "≠",
            Self::Less => "<",
            Self::LessOrEqual => "≤",
            Self::Greater => ">",
            Self::GreaterOrEqual => "≥",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MathOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Power,
}

impl MathOp {
    pub const ALL: [MathOp; 6] = [
        MathOp::Add,
        MathOp::Subtract,
        MathOp::Multiply,
        MathOp::Divide,
        MathOp::Modulo,
        MathOp::Power,
    ];

    pub fn lua(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Subtract => "-",
            Self::Multiply => "*",
            Self::Divide => "/",
            Self::Modulo => "%",
            Self::Power => "^",
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Subtract => "−",
            Self::Multiply => "×",
            Self::Divide => "÷",
            Self::Modulo => "mod",
            Self::Power => "^",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogicOp {
    And,
    Or,
}

impl LogicOp {
    pub fn lua(self) -> &'static str {
        match self {
            Self::And => "and",
            Self::Or => "or",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MathFunction {
    Abs,
    Floor,
    Ceil,
    Round,
    Sqrt,
}

impl MathFunction {
    pub const ALL: [MathFunction; 5] = [
        MathFunction::Abs,
        MathFunction::Floor,
        MathFunction::Ceil,
        MathFunction::Round,
        MathFunction::Sqrt,
    ];

    /// The name a block shows.
    pub fn name(self) -> &'static str {
        match self {
            Self::Abs => "abs",
            Self::Floor => "floor",
            Self::Ceil => "ceiling",
            Self::Round => "round",
            Self::Sqrt => "sqrt",
        }
    }
}

mod from_lua;
mod lua;

pub use from_lua::{FromLuaError, from_lua};
pub use lua::SyntaxError;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod tween_tests;
