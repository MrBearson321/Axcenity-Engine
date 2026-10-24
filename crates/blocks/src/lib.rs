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
//! other way, for switching a script between Code and Blocks.

use std::collections::{BTreeSet, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

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
            | BlockKind::Print { value: e } => vec![e],
            _ => Vec::new(),
        }
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
            | BlockKind::Print { value: e } => vec![e],
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

#[derive(Clone, Debug, PartialEq)]
pub struct Translation {
    pub source: String,
    /// The generated Lua line each block's code starts on (numbered from 1),
    /// with the block's ID, so a runtime error can point at its block.
    pub block_lines: Vec<(usize, u64)>,
}

impl Translation {
    /// The block whose code is on `line`: the last one starting at or
    /// before it, which is the innermost block around it.
    pub fn block_at(&self, line: usize) -> Option<u64> {
        self.block_lines
            .iter()
            .filter(|(start, _)| *start <= line)
            .max_by_key(|(start, _)| *start)
            .map(|(_, id)| *id)
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum TranslateError {
    #[error("block ID {0} appears more than once")]
    DuplicateId(u64),
    #[error(
        "variable name `{0}` must start with a letter or underscore and contain only letters, digits or underscores"
    )]
    InvalidVariable(String),
    #[error("number blocks must contain finite numbers")]
    InvalidNumber,
    #[error("Instance target `{0}` must be a non-empty dotted path of child names")]
    InvalidTarget(String),
    #[error("`{0}` isn't a key name, such as Space, W or Up")]
    InvalidKey(String),
    #[error("property name `{0}` must contain only letters, digits or underscores")]
    InvalidProperty(String),
    #[error("only event blocks can be at the top level, and events cannot be nested")]
    InvalidEventPlacement,
    #[error("\"touching part\" only works inside a \"when touched\" block")]
    TouchingPartOutsideTouched,
}

/// Translates `script` to Lua. Unattached stacks are left out, as they
/// don't run.
pub fn translate(script: &BlockScript) -> Result<Translation, TranslateError> {
    let mut ids = HashSet::new();
    let mut writer = Writer::default();
    for block in &script.blocks {
        if !block.kind.is_event() {
            return Err(TranslateError::InvalidEventPlacement);
        }
    }

    // Variables are the whole script's, as in Scratch, so they're declared
    // once at the top, starting at 0.
    let mut variables = BTreeSet::new();
    let mut services = BTreeSet::new();
    script.for_each_block(&mut |block| {
        if let BlockKind::SetVariable { name, .. } | BlockKind::ChangeVariable { name, .. } =
            &block.kind
        {
            variables.insert(name.clone());
        }
        match &block.kind {
            BlockKind::WhenKeyPressed { .. } => {
                services.insert("UserInputService");
            }
            BlockKind::EveryFrame { .. } => {
                services.insert("RunService");
            }
            _ => {}
        }
        for expression in block.kind.expressions() {
            visit_expressions(expression, &mut |expression| match expression {
                Expression::Variable { name } => {
                    variables.insert(name.clone());
                }
                Expression::KeyDown { .. } => {
                    services.insert("UserInputService");
                }
                _ => {}
            });
        }
    });
    for service in &services {
        writer.line(format!("local {service} = game:GetService(\"{service}\")"));
    }
    for name in &variables {
        if !valid_identifier(name) {
            return Err(TranslateError::InvalidVariable(name.clone()));
        }
        writer.line(format!("local {name} = 0"));
    }

    for block in &script.blocks {
        if matches!(block.kind, BlockKind::Unattached { .. }) {
            // Still checked, so IDs stay unique across the canvas.
            note_ids(block, &mut ids)?;
            continue;
        }
        if !writer.lines.is_empty() {
            writer.lines.push(String::new());
        }
        writer.write_event(block, &mut ids)?;
    }
    let source = writer.lines.join("\n") + if writer.lines.is_empty() { "" } else { "\n" };
    Ok(Translation {
        source,
        block_lines: writer.block_lines,
    })
}

fn note_ids(block: &Block, ids: &mut HashSet<u64>) -> Result<(), TranslateError> {
    if !ids.insert(block.id) {
        return Err(TranslateError::DuplicateId(block.id));
    }
    for body in block.kind.bodies() {
        for child in body {
            note_ids(child, ids)?;
        }
    }
    Ok(())
}

fn visit_expressions(expression: &Expression, visit: &mut impl FnMut(&Expression)) {
    visit(expression);
    for child in expression.children() {
        visit_expressions(child, visit);
    }
}

#[derive(Default)]
struct Writer {
    lines: Vec<String>,
    block_lines: Vec<(usize, u64)>,
    indent: usize,
    /// True inside "when touched", where `otherPart` exists.
    in_touched: bool,
}

impl Writer {
    fn line(&mut self, text: impl Into<String>) {
        self.lines
            .push(format!("{}{}", "    ".repeat(self.indent), text.into()));
    }

    /// Notes that `id`'s code starts on the next line.
    fn tag(&mut self, id: u64, ids: &mut HashSet<u64>) -> Result<(), TranslateError> {
        if !ids.insert(id) {
            return Err(TranslateError::DuplicateId(id));
        }
        self.block_lines.push((self.lines.len() + 1, id));
        Ok(())
    }

    fn write_event(&mut self, block: &Block, ids: &mut HashSet<u64>) -> Result<(), TranslateError> {
        self.tag(block.id, ids)?;
        let (opening, body) = match &block.kind {
            BlockKind::WhenStarted { body } => ("task.spawn(function()".to_owned(), body),
            BlockKind::WhenTouched { part, body } => {
                self.in_touched = true;
                (
                    format!("{}.Touched:Connect(function(otherPart)", target(part)?),
                    body,
                )
            }
            BlockKind::WhenKeyPressed { key: name, body } => {
                let key = key(name)?;
                self.line("UserInputService.InputBegan:Connect(function(input)");
                self.indent += 1;
                self.line(format!("if input.KeyCode == {key} then"));
                self.nested(body, ids)?;
                self.line("end");
                self.indent -= 1;
                self.line("end)");
                return Ok(());
            }
            BlockKind::WhenClicked { button, body } => (
                format!("{}.Activated:Connect(function()", target(button)?),
                body,
            ),
            BlockKind::EveryFrame { body } => (
                "RunService.Heartbeat:Connect(function(deltaTime)".to_owned(),
                body,
            ),
            BlockKind::WhenSoundEnded { sound, body } => {
                (format!("{}.Ended:Connect(function()", target(sound)?), body)
            }
            BlockKind::WhenStateChanged { animator, body } => (
                format!(
                    "{}.StateChanged:Connect(function(state, previous)",
                    target(animator)?
                ),
                body,
            ),
            BlockKind::WhenAnimationEvent {
                animator,
                event,
                body,
            } => {
                self.line(format!(
                    "{}.AnimationEvent:Connect(function(name)",
                    target(animator)?
                ));
                self.indent += 1;
                self.line(format!("if name == {} then", quote(event)));
                self.nested(body, ids)?;
                self.line("end");
                self.indent -= 1;
                self.line("end)");
                return Ok(());
            }
            BlockKind::TopCode { code } => {
                self.code(code);
                return Ok(());
            }
            _ => return Err(TranslateError::InvalidEventPlacement),
        };
        self.line(opening);
        self.indent += 1;
        self.write_body(body, ids)?;
        self.indent -= 1;
        self.line("end)");
        self.in_touched = false;
        Ok(())
    }

    /// Lua kept from a typed script, one line at a time at this indent.
    fn code(&mut self, code: &str) {
        for line in code.lines() {
            if line.trim().is_empty() {
                self.lines.push(String::new());
            } else {
                self.line(line);
            }
        }
    }

    fn nested(&mut self, body: &[Block], ids: &mut HashSet<u64>) -> Result<(), TranslateError> {
        self.indent += 1;
        self.write_body(body, ids)?;
        self.indent -= 1;
        Ok(())
    }

    fn write_body(&mut self, body: &[Block], ids: &mut HashSet<u64>) -> Result<(), TranslateError> {
        for block in body {
            if block.kind.shape() != BlockShape::Statement {
                return Err(TranslateError::InvalidEventPlacement);
            }
            self.tag(block.id, ids)?;
            self.write_statement(&block.kind, ids)?;
        }
        Ok(())
    }

    fn write_statement(
        &mut self,
        kind: &BlockKind,
        ids: &mut HashSet<u64>,
    ) -> Result<(), TranslateError> {
        match kind {
            BlockKind::Wait { seconds } => {
                let seconds = self.expression(seconds)?;
                self.line(format!("task.wait({seconds})"));
            }
            BlockKind::WaitUntil { condition } => {
                let condition = self.expression(condition)?;
                self.line(format!("while not ({condition}) do"));
                self.indent += 1;
                self.line("task.wait()");
                self.indent -= 1;
                self.line("end");
            }
            // `_` is Lua's name for a counter nothing reads.
            BlockKind::Repeat { count, body } => {
                let count = self.expression(count)?;
                self.line(format!("for _ = 1, {count} do"));
                self.nested(body, ids)?;
                self.line("end");
            }
            // Loops that may run for a long time wait a frame each time
            // round, as Scratch's do, so the game keeps running.
            BlockKind::Forever { body } => {
                self.line("while true do");
                self.nested(body, ids)?;
                self.indent += 1;
                self.line("task.wait()");
                self.indent -= 1;
                self.line("end");
            }
            BlockKind::RepeatUntil { condition, body } => {
                let condition = self.expression(condition)?;
                self.line(format!("while not ({condition}) do"));
                self.nested(body, ids)?;
                self.indent += 1;
                self.line("task.wait()");
                self.indent -= 1;
                self.line("end");
            }
            BlockKind::If { condition, body } => {
                let condition = self.expression(condition)?;
                self.line(format!("if {condition} then"));
                self.nested(body, ids)?;
                self.line("end");
            }
            BlockKind::IfElse {
                condition,
                then_body,
                else_body,
            } => {
                let condition = self.expression(condition)?;
                self.line(format!("if {condition} then"));
                self.nested(then_body, ids)?;
                if !else_body.is_empty() {
                    self.line("else");
                    self.nested(else_body, ids)?;
                }
                self.line("end");
            }
            // `do return end` can go anywhere; a bare `return` must be last.
            BlockKind::StopScript => self.line("do return end"),
            BlockKind::MoveTo {
                target: t,
                position,
            } => {
                let value = self.expression(position)?;
                self.line(format!("{}.Position = {value}", target(t)?));
            }
            BlockKind::MoveBy { target: t, offset } => {
                let (object, value) = (target(t)?, self.expression(offset)?);
                self.line(format!("{object}.Position = {object}.Position + {value}"));
            }
            BlockKind::TurnTo {
                target: t,
                orientation,
            } => {
                let value = self.expression(orientation)?;
                self.line(format!("{}.Orientation = {value}", target(t)?));
            }
            BlockKind::TurnBy {
                target: t,
                rotation,
            } => {
                let (object, value) = (target(t)?, self.expression(rotation)?);
                self.line(format!(
                    "{object}.Orientation = {object}.Orientation + {value}"
                ));
            }
            BlockKind::SetAnchored {
                target: t,
                anchored,
            } => {
                let value = self.expression(anchored)?;
                self.line(format!("{}.Anchored = {value}", target(t)?));
            }
            BlockKind::SetColor { target: t, color } => {
                let value = self.expression(color)?;
                self.line(format!("{}.Color = {value}", target(t)?));
            }
            BlockKind::SetText { target: t, text } => {
                let value = match text {
                    Expression::Text { .. } => self.expression(text)?,
                    other => format!("tostring({})", self.expression(other)?),
                };
                self.line(format!("{}.Text = {value}", target(t)?));
            }
            BlockKind::Show { target: t } => {
                self.line(format!("{}.Visible = true", target(t)?));
            }
            BlockKind::Hide { target: t } => {
                self.line(format!("{}.Visible = false", target(t)?));
            }
            BlockKind::ShowPage { layout, page } => {
                let value = match page {
                    Expression::Text { .. } => self.expression(page)?,
                    other => format!("tostring({})", self.expression(other)?),
                };
                self.line(format!("{}.CurrentPage = {value}", target(layout)?));
            }
            BlockKind::SetProperty {
                target: t,
                property,
                value,
            } => {
                if !valid_identifier(property) {
                    return Err(TranslateError::InvalidProperty(property.clone()));
                }
                let value = self.expression(value)?;
                self.line(format!("{}.{property} = {value}", target(t)?));
            }
            BlockKind::PlaySound { target: t } => self.line(format!("{}:Play()", target(t)?)),
            BlockKind::StopSound { target: t } => self.line(format!("{}:Stop()", target(t)?)),
            BlockKind::PauseSound { target: t } => self.line(format!("{}:Pause()", target(t)?)),
            BlockKind::ResumeSound { target: t } => {
                self.line(format!("{}:Resume()", target(t)?));
            }
            BlockKind::SetVolume { target: t, volume } => {
                let value = self.expression(volume)?;
                self.line(format!("{}.Volume = {value}", target(t)?));
            }
            BlockKind::SetAnimatorBool {
                animator,
                parameter,
                value,
            } => {
                let value = self.expression(value)?;
                self.line(format!(
                    "{}:SetBool({}, {value})",
                    target(animator)?,
                    quote(parameter)
                ));
            }
            BlockKind::SetAnimatorNumber {
                animator,
                parameter,
                value,
            } => {
                let value = self.expression(value)?;
                self.line(format!(
                    "{}:SetNumber({}, {value})",
                    target(animator)?,
                    quote(parameter)
                ));
            }
            BlockKind::SetAnimatorTrigger {
                animator,
                parameter,
            } => self.line(format!(
                "{}:SetTrigger({})",
                target(animator)?,
                quote(parameter)
            )),
            BlockKind::ResetAnimatorTrigger {
                animator,
                parameter,
            } => self.line(format!(
                "{}:ResetTrigger({})",
                target(animator)?,
                quote(parameter)
            )),
            BlockKind::PlayAnimatorState { animator, state } => {
                self.line(format!("{}:Play({})", target(animator)?, quote(state)));
            }
            BlockKind::PlayAnimation {
                animator,
                animation,
            } => {
                self.line(format!(
                    "{}:LoadAnimation({}):Play()",
                    target(animator)?,
                    target(animation)?
                ));
            }
            BlockKind::SetVariable { name, value } => {
                let value = self.expression(value)?;
                self.line(format!("{name} = {value}"));
            }
            BlockKind::ChangeVariable { name, by } => {
                let value = self.expression(by)?;
                self.line(format!("{name} = {name} + {value}"));
            }
            BlockKind::Print { value } => {
                let value = self.expression(value)?;
                self.line(format!("print({value})"));
            }
            BlockKind::Destroy { target: t } => self.line(format!("{}:Destroy()", target(t)?)),
            BlockKind::Code { code } => self.code(code),
            BlockKind::WhenStarted { .. }
            | BlockKind::WhenTouched { .. }
            | BlockKind::WhenKeyPressed { .. }
            | BlockKind::WhenClicked { .. }
            | BlockKind::EveryFrame { .. }
            | BlockKind::WhenSoundEnded { .. }
            | BlockKind::WhenStateChanged { .. }
            | BlockKind::WhenAnimationEvent { .. }
            | BlockKind::Unattached { .. }
            | BlockKind::TopCode { .. } => {
                return Err(TranslateError::InvalidEventPlacement);
            }
        }
        Ok(())
    }

    fn expression(&self, expression: &Expression) -> Result<String, TranslateError> {
        // Reporters inside other reporters are bracketed, so `(a + b) * c`
        // means what the blocks show.
        let inner = |expression: &Expression| -> Result<String, TranslateError> {
            let text = self.expression(expression)?;
            Ok(match expression {
                Expression::Math { .. }
                | Expression::Join { .. }
                | Expression::Compare { .. }
                | Expression::Logic { .. }
                | Expression::Not { .. } => format!("({text})"),
                _ => text,
            })
        };
        let finite = |values: &[f64]| values.iter().all(|value| value.is_finite());
        Ok(match expression {
            Expression::Number { value } if value.is_finite() => number(*value),
            Expression::Text { value } => quote(value),
            Expression::Boolean { value } => value.to_string(),
            Expression::Vector3 { x, y, z } if finite(&[*x, *y, *z]) => {
                format!(
                    "Vector3.new({}, {}, {})",
                    number(*x),
                    number(*y),
                    number(*z)
                )
            }
            Expression::Color3 { r, g, b } if finite(&[*r, *g, *b]) => {
                format!("Color3.new({}, {}, {})", number(*r), number(*g), number(*b))
            }
            Expression::Number { .. } | Expression::Vector3 { .. } | Expression::Color3 { .. } => {
                return Err(TranslateError::InvalidNumber);
            }
            Expression::Variable { name } if valid_identifier(name) => name.clone(),
            Expression::Variable { name } => {
                return Err(TranslateError::InvalidVariable(name.clone()));
            }
            Expression::Math { op, left, right } => {
                format!("{} {} {}", inner(left)?, op.lua(), inner(right)?)
            }
            Expression::Function { function, value } => {
                let value = self.expression(value)?;
                match function {
                    MathFunction::Abs => format!("math.abs({value})"),
                    MathFunction::Floor => format!("math.floor({value})"),
                    MathFunction::Ceil => format!("math.ceil({value})"),
                    MathFunction::Round => format!("math.floor({value} + 0.5)"),
                    MathFunction::Sqrt => format!("math.sqrt({value})"),
                }
            }
            Expression::Random { min, max } => {
                let (min, max) = (self.expression(min)?, self.expression(max)?);
                format!("math.random(math.floor({min}), math.floor({max}))")
            }
            Expression::Join { left, right } => {
                let (left, right) = (self.expression(left)?, self.expression(right)?);
                format!("tostring({left}) .. tostring({right})")
            }
            Expression::Compare { op, left, right } => {
                format!("{} {} {}", inner(left)?, op.lua(), inner(right)?)
            }
            Expression::Logic { op, left, right } => {
                format!("{} {} {}", inner(left)?, op.lua(), inner(right)?)
            }
            Expression::Not { value } => format!("not {}", inner(value)?),
            Expression::Property {
                target: t,
                property,
            } => {
                if !valid_identifier(property) {
                    return Err(TranslateError::InvalidProperty(property.clone()));
                }
                format!("{}.{property}", target(t)?)
            }
            Expression::TouchingPart if self.in_touched => "otherPart".to_owned(),
            Expression::TouchingPart => return Err(TranslateError::TouchingPartOutsideTouched),
            Expression::KeyDown { key: name } => {
                format!("UserInputService:IsKeyDown({})", key(name)?)
            }
            Expression::AnimatorBool {
                animator,
                parameter,
            } => format!("{}:GetBool({})", target(animator)?, quote(parameter)),
            Expression::AnimatorNumber {
                animator,
                parameter,
            } => format!("{}:GetNumber({})", target(animator)?, quote(parameter)),
            // Bracketed already where it needs to be (see `from_lua`).
            Expression::Code { code } => code.clone(),
        })
    }
}

/// A number as Lua writes it: whole numbers without a decimal point.
fn number(value: f64) -> String {
    value.to_string()
}

/// `Enum.KeyCode.<name>`, for a key's name such as `Space`.
fn key(name: &str) -> Result<String, TranslateError> {
    if !valid_identifier(name) {
        return Err(TranslateError::InvalidKey(name.to_owned()));
    }
    Ok(format!("Enum.KeyCode.{name}"))
}

/// The Lua for an object at a dotted path under the Workspace, such as
/// `workspace.Pad.Chime`. Names that aren't Lua names are bracketed:
/// `workspace["Red Ball"]`.
fn target(path: &str) -> Result<String, TranslateError> {
    let parts = path.split('.').collect::<Vec<_>>();
    if parts.iter().any(|part| part.is_empty()) {
        return Err(TranslateError::InvalidTarget(path.to_owned()));
    }
    Ok(parts
        .into_iter()
        .fold("workspace".to_owned(), |prefix, part| {
            if valid_identifier(part) {
                format!("{prefix}.{part}")
            } else {
                format!("{prefix}[{}]", quote(part))
            }
        }))
}

fn quote(text: &str) -> String {
    let mut escaped = String::from("\"");
    for character in text.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\0' => escaped.push_str("\\000"),
            character if character.is_control() => {
                escaped.push_str(&format!("\\{:03}", character as u32));
            }
            character => escaped.push(character),
        }
    }
    escaped.push('"');
    escaped
}

/// Whether `name` can be a Lua variable: letters, digits and underscores,
/// not starting with a digit, and not a Lua keyword.
pub fn valid_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
        && chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
        && !matches!(
            name,
            "and"
                | "break"
                | "do"
                | "else"
                | "elseif"
                | "end"
                | "false"
                | "for"
                | "function"
                | "goto"
                | "if"
                | "in"
                | "local"
                | "nil"
                | "not"
                | "or"
                | "repeat"
                | "return"
                | "then"
                | "true"
                | "until"
                | "while"
        )
}

mod from_lua;
mod lua;

pub use from_lua::{FromLuaError, from_lua};
pub use lua::SyntaxError;

#[cfg(test)]
mod tests;
