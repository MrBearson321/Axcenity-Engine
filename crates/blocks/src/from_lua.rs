//! Lua to blocks, for switching a script from Code to Blocks or Graph.
//!
//! Each statement becomes the friendliest block that writes the same code:
//! a block such as "move to" if it fits, or else the block for what it is in
//! Luau (a local, a call, a loop, a function: see `BlockKind::RunScript`).
//! Every choice is checked by writing the block back and comparing it with
//! what was typed ([`lua::same_code`]). What no block can write stays word
//! for word in a grey Lua block ([`BlockKind::Code`] and
//! [`Expression::Code`]), and if the blocks as a whole still don't write the
//! same code, the script stays one grey block ([`BlockKind::TopCode`]). So
//! nothing is lost either way, and the blocks run the same code.

use std::collections::BTreeSet;

use crate::lua::{self, Expr, ExprKind, FunctionBody, Stat, StatKind, SyntaxError};
use crate::{
    ATMOSPHERE_NUMBERS, Block, BlockKind, BlockScript, BlockShape, CAMERA_TYPES, Comparison,
    EASING_DIRECTIONS, EASING_STYLES, Expression, GRAPHICS_NUMBERS, GRAPHICS_OPTIONS,
    GRAPHICS_QUALITIES, GRAPHICS_SWITCHES, LogicOp, MathFunction, MathOp, PRESET, SIGNAL_METHODS,
    TONE_MAPPINGS, TableField, binary_priority, block_lua, expression_lua, graphics_choices,
    needs_brackets, translate,
};

/// The services blocks get with `game:GetService`, which the translation
/// declares at the top itself when blocks need them.
const SERVICES: [&str; 13] = [
    "UserInputService",
    "RunService",
    "Debris",
    "TweenService",
    "SaveService",
    "LevelService",
    "NetworkService",
    "TextChatService",
    "LocalizationService",
    "Lighting",
    "UserGameSettings",
    "DataService",
    "ConsoleService",
];

/// Why some Lua couldn't become blocks.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum FromLuaError {
    /// The code has a syntax error: fix it first.
    #[error("line {}: {}", .0.line, .0.message)]
    Syntax(SyntaxError),
    #[error("the blocks it became don't translate back: {0}")]
    Translate(String),
}

/// Turns Lua into a block script that runs the same code.
pub fn from_lua(source: &str) -> Result<BlockScript, FromLuaError> {
    let chunk = lua::parse(source).map_err(FromLuaError::Syntax)?;
    let tabs = source.lines().any(|line| line.starts_with('\t'));

    // `local name = 0` and `local RunService = game:GetService("RunService")`
    // at the very top are the blocks' variables and services: the translation
    // writes those itself, but only for the ones blocks use, so a second pass
    // keeps the declarations of the rest as code.
    let mut variables = BTreeSet::new();
    let mut services = BTreeSet::new();
    for stat in &chunk {
        if let Some(name) = variable_declaration(stat) {
            variables.insert(name);
        } else if let Some(name) = service_declaration(stat) {
            services.insert(name);
        } else {
            break;
        }
    }
    let first = Converter::new(source, variables, services).chunk(&chunk);
    let (used_variables, used_services) = used_by(&first);
    let attempts = [
        Converter::new(source, used_variables, used_services),
        Converter::new(source, BTreeSet::new(), BTreeSet::new()),
    ];
    for converter in attempts {
        let mut script = converter.chunk(&chunk);
        script.tabs = tabs;
        let Ok(translation) = translate(&script) else {
            continue;
        };
        if lua::same_code(source, &translation.source)
            && lua::keeps_comments(source, &translation.source)
        {
            return Ok(script);
        }
    }

    // Nothing fits, so the whole script is kept as it is.
    let script = BlockScript {
        blocks: vec![Block::new(BlockKind::TopCode {
            code: source.trim_end().to_owned(),
        })],
        tabs,
        ..Default::default()
    };
    translate(&script).map_err(|err| FromLuaError::Translate(err.to_string()))?;
    Ok(script)
}

/// The item name of `Enum.<enum_name>.<item>`, if it's one of `items`.
fn enum_item(expr: &Expr, enum_name: &str, items: &[&str]) -> Option<String> {
    let ExprKind::Field(of, item) = &expr.kind else {
        return None;
    };
    let ExprKind::Field(root, name) = &of.kind else {
        return None;
    };
    (name == enum_name
        && matches!(&root.kind, ExprKind::Name(root) if root == "Enum")
        && items.contains(&item.as_str()))
    .then(|| item.clone())
}

/// Whether `expr` is `Enum.StreamingState.StreamedIn`.
fn is_streamed_in_item(expr: &Expr) -> bool {
    enum_item(expr, "StreamingState", &["StreamedIn"]).is_some()
}

fn variable_declaration(stat: &Stat) -> Option<String> {
    match &stat.kind {
        StatKind::Local { names, values }
            if names.len() == 1
                && crate::valid_identifier(&names[0])
                && values.len() == 1
                && matches!(values[0].kind, ExprKind::Number(Some(value)) if value == 0.0) =>
        {
            Some(names[0].clone())
        }
        _ => None,
    }
}

fn service_declaration(stat: &Stat) -> Option<String> {
    let StatKind::Local { names, values } = &stat.kind else {
        return None;
    };
    let ([name], [value]) = (names.as_slice(), values.as_slice()) else {
        return None;
    };
    let ExprKind::Method(object, method, args) = &value.kind else {
        return None;
    };
    let is_game = matches!(&object.kind, ExprKind::Name(game) if game == "game");
    let service = match args.as_slice() {
        [
            Expr {
                kind: ExprKind::Str(service),
                ..
            },
        ] => service,
        _ => return None,
    };
    (is_game && method == "GetService" && service == name && SERVICES.contains(&name.as_str()))
        .then(|| name.clone())
}

/// The variables and services the blocks of `script` need declared.
fn used_by(script: &BlockScript) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut variables = BTreeSet::new();
    let mut services = BTreeSet::new();
    fn visit(
        expression: &Expression,
        variables: &mut BTreeSet<String>,
        services: &mut BTreeSet<String>,
    ) {
        match expression {
            Expression::Variable { name } => {
                variables.insert(name.clone());
            }
            Expression::KeyDown { .. } => {
                services.insert("UserInputService".to_owned());
            }
            Expression::DataRow { .. } => {
                services.insert("DataService".to_owned());
            }
            _ => {}
        }
        for child in expression.children() {
            visit(child, variables, services);
        }
    }
    script.for_each_block(&mut |block| {
        match &block.kind {
            BlockKind::SetVariable { name, .. } | BlockKind::ChangeVariable { name, .. } => {
                variables.insert(name.clone());
            }
            BlockKind::WhenKeyPressed { .. } => {
                services.insert("UserInputService".to_owned());
            }
            BlockKind::EveryFrame { .. }
            | BlockKind::BeforePhysics { .. }
            | BlockKind::BeforeRender { .. } => {
                services.insert("RunService".to_owned());
            }
            BlockKind::DestroyAfter { .. } => {
                services.insert("Debris".to_owned());
            }
            kind if kind.tweens() => {
                services.insert("TweenService".to_owned());
            }
            kind if kind.saves() => {
                services.insert("SaveService".to_owned());
            }
            kind if kind.levels() => {
                services.insert("LevelService".to_owned());
            }
            kind if kind.networks() => {
                services.insert("NetworkService".to_owned());
            }
            BlockKind::SendChatMessage { .. } | BlockKind::ShowSystemMessage { .. } => {
                services.insert("TextChatService".to_owned());
            }
            BlockKind::WhenCommandRun { .. } => {
                services.insert("ConsoleService".to_owned());
            }
            BlockKind::SetLanguage { .. } => {
                services.insert("LocalizationService".to_owned());
            }
            kind if kind.sets_lighting() => {
                services.insert("Lighting".to_owned());
            }
            kind if kind.sets_graphics() => {
                services.insert("UserGameSettings".to_owned());
            }
            _ => {}
        }
        for expression in block.kind.expressions() {
            visit(expression, &mut variables, &mut services);
        }
    });
    (variables, services)
}

struct Converter<'a> {
    source: &'a str,
    variables: BTreeSet<String>,
    services: BTreeSet<String>,
}

/// What's in scope while converting a handler's body.
#[derive(Clone, Copy, Default)]
struct Scope<'a> {
    /// The name "when touched" gives the part that touched.
    touched: Option<&'a str>,
    /// The innermost "for each child" loop's variable.
    child: Option<&'a str>,
}

impl<'a> Converter<'a> {
    fn new(source: &'a str, variables: BTreeSet<String>, services: BTreeSet<String>) -> Self {
        Self {
            source,
            variables,
            services,
        }
    }

    /// Whether a string's source has an escape for a raw byte, which text
    /// blocks can't hold: `\xFF` or `\200`.
    fn has_byte_escape(&self, expr: &Expr) -> bool {
        let text = self.source[expr.span.clone()].as_bytes();
        let mut at = 0;
        while at + 1 < text.len() {
            if text[at] == b'\\' {
                if text[at + 1] == b'x' || text[at + 1].is_ascii_digit() {
                    return true;
                }
                at += 1;
            }
            at += 1;
        }
        false
    }

    /// The Lua of `span`, with the lines after the first moved left as far
    /// as the first line's column, so it can be indented again.
    fn text(&self, span: std::ops::Range<usize>) -> String {
        let line_start = self.source[..span.start].rfind('\n').map_or(0, |at| at + 1);
        let column = self.source[line_start..span.start].chars().count();
        let text = &self.source[span];
        let mut lines = text.split('\n');
        let mut result = lines
            .next()
            .unwrap_or_default()
            .trim_end_matches('\r')
            .to_owned();
        for line in lines {
            let line = line.trim_end_matches('\r');
            let spaces = line
                .chars()
                .take(column)
                .take_while(|c| *c == ' ' || *c == '\t')
                .count();
            result.push('\n');
            result.push_str(&line[spaces..]);
        }
        result
    }

    // ---- The top level ------------------------------------------------------

    /// A declaration the translation writes itself.
    fn declared(&self, stat: &Stat) -> bool {
        variable_declaration(stat).is_some_and(|name| self.variables.contains(&name))
            || service_declaration(stat).is_some_and(|name| self.services.contains(&name))
    }

    fn chunk(&self, chunk: &[Stat]) -> BlockScript {
        let start = chunk.iter().take_while(|stat| self.declared(stat)).count();
        let mut blocks = Vec::new();
        let mut previous_end = start.checked_sub(1).map(|last| chunk[last].span.end);
        let mut run_start = start;
        for (index, stat) in chunk.iter().enumerate().skip(start) {
            let Some(mut event) = self.top_event(stat) else {
                continue;
            };
            if run_start < index {
                let run = self.run(&chunk[run_start..index], previous_end);
                previous_end = Some(chunk[index - 1].span.end);
                blocks.push(run);
            }
            event.spaced = self.gap_before(previous_end, stat.span.start);
            previous_end = Some(stat.span.end);
            blocks.push(event);
            run_start = index + 1;
        }
        if run_start < chunk.len() {
            blocks.push(self.run(&chunk[run_start..], previous_end));
        }
        BlockScript {
            blocks,
            ..Default::default()
        }
    }

    /// Statements in a row at the top level, as a "script runs" stack.
    fn run(&self, stats: &[Stat], previous_end: Option<usize>) -> Block {
        let mut block = Block::new(BlockKind::RunScript {
            body: self.body(stats, Scope::default()),
        });
        block.spaced = stats
            .first()
            .is_some_and(|stat| self.gap_before(previous_end, stat.span.start));
        block
    }

    /// Whether a blank line is between what ended at `previous_end` and `at`.
    fn gap_before(&self, previous_end: Option<usize>, at: usize) -> bool {
        previous_end
            .and_then(|end| self.source.get(end..at))
            .is_some_and(|between| between.matches('\n').count() >= 2)
    }

    /// What `stat` is at the top level if it's a stack of its own: an event
    /// block that writes the same code, or else a signal connection.
    fn top_event(&self, stat: &Stat) -> Option<Block> {
        if let Some(event) = self.event(stat)
            && self.writes_statement(&event, stat, false)
        {
            return Some(event);
        }
        let (signal, method, params, body) = self.connection(stat, Scope::default())?;
        let event = Block::new(BlockKind::OnSignal {
            signal,
            method,
            params,
            body,
        });
        self.writes_statement(&event, stat, false).then_some(event)
    }

    /// `signal:Connect(function(params) ... end)`, as its pieces.
    fn connection(
        &self,
        stat: &Stat,
        scope: Scope,
    ) -> Option<(Expression, String, String, Vec<Block>)> {
        let StatKind::Call(call) = &stat.kind else {
            return None;
        };
        let ExprKind::Method(signal, method, args) = &call.kind else {
            return None;
        };
        let [
            Expr {
                kind: ExprKind::Function(function),
                ..
            },
        ] = args.as_slice()
        else {
            return None;
        };
        (SIGNAL_METHODS.contains(&method.as_str())
            && function.generics.is_empty()
            && function.returns.is_empty()
            && function.attributes.is_empty())
        .then(|| {
            (
                self.expression(signal, scope),
                method.clone(),
                function.written_params.clone(),
                self.body(&function.body, scope),
            )
        })
    }

    /// Whether `block` writes the same code as `stat`.
    fn writes_statement(&self, block: &Block, stat: &Stat, touched: bool) -> bool {
        block_lua(block, touched)
            .is_ok_and(|written| lua::same_code(&self.source[stat.span.clone()], &written))
    }

    /// Whether `value` writes the same code as `expr`.
    fn writes_expression(&self, value: &Expression, expr: &Expr, touched: bool) -> bool {
        expression_lua(value, touched)
            .is_ok_and(|written| lua::same_code(&self.source[expr.span.clone()], &written))
    }

    /// An event handler, as its event block.
    fn event(&self, stat: &Stat) -> Option<Block> {
        let StatKind::Call(call) = &stat.kind else {
            return None;
        };
        // task.spawn(function() ... end)
        if let ExprKind::Call(function, args) = &call.kind
            && self.is_field(function, "task", "spawn")
            && let [handler] = args.as_slice()
            && let Some(body) = self.handler(handler, 0)
        {
            let body = self.body(&body.body, Scope::default());
            return Some(Block::new(BlockKind::WhenStarted { body }));
        }
        // object.Event:Connect(function(...) ... end)
        let ExprKind::Method(signal, method, args) = &call.kind else {
            return None;
        };
        // ConsoleService:RegisterCommand(name, help, function() ... end)
        if method == "RegisterCommand"
            && self.is_service(signal, "ConsoleService")
            && let [command, help, handler] = args.as_slice()
            && let Some(function) = self.handler(handler, 0)
        {
            let scope = Scope::default();
            return Some(Block::new(BlockKind::WhenCommandRun {
                command: self.expression(command, scope),
                help: self.expression(help, scope),
                body: self.body(&function.body, scope),
            }));
        }
        if method != "Connect" {
            return None;
        }
        let [handler] = args.as_slice() else {
            return None;
        };
        if let ExprKind::Method(object, method, args) = &signal.kind
            && method == "GetPropertyChangedSignal"
            && matches!(args.as_slice(), [Expr { kind: ExprKind::Str(name), .. }] if name == "Occupant")
        {
            return self.seat_occupied(object, handler);
        }
        let ExprKind::Field(object, event) = &signal.kind else {
            return None;
        };
        match event.as_str() {
            "Touched" => {
                let part = self.target(object)?;
                let function = self.handler(handler, 1)?;
                let param = function.params.first().map(String::as_str);
                let scope = Scope {
                    touched: param.or(Some("otherPart")),
                    ..Scope::default()
                };
                let body = self.body(&function.body, scope);
                if let Some(param) = param
                    && param != "otherPart"
                    && code_mentions(&body, param)
                {
                    return None;
                }
                Some(Block::new(BlockKind::WhenTouched { part, body }))
            }
            "StreamedIn" => {
                let target = self.target(object)?;
                let function = self.handler(handler, 0)?;
                let body = self.body(&function.body, Scope::default());
                Some(Block::new(BlockKind::WhenStreamedIn { target, body }))
            }
            "Triggered" | "MouseClick" => {
                let target = self.target(object)?;
                let function = self.handler(handler, 0)?;
                let body = self.body(&function.body, Scope::default());
                Some(Block::new(if event == "Triggered" {
                    BlockKind::WhenPromptTriggered {
                        prompt: target,
                        body,
                    }
                } else {
                    BlockKind::WhenPartClicked {
                        detector: target,
                        body,
                    }
                }))
            }
            "Activated" => {
                let button = self.target(object)?;
                let function = self.handler(handler, 0)?;
                let body = self.body(&function.body, Scope::default());
                Some(Block::new(BlockKind::WhenClicked { button, body }))
            }
            "MouseEnter" | "MouseLeave" => {
                let element = self.target(object)?;
                let function = self.handler(handler, 0)?;
                let body = self.body(&function.body, Scope::default());
                Some(Block::new(if event == "MouseEnter" {
                    BlockKind::WhenMouseEnters { element, body }
                } else {
                    BlockKind::WhenMouseLeaves { element, body }
                }))
            }
            "ValueChanged" => {
                let widget = self.target(object)?;
                let function = self.handler(handler, 1)?;
                let body = self.body(&function.body, Scope::default());
                // The blocks read the new value as the widget's Value, not
                // the handler's parameter.
                if function
                    .params
                    .iter()
                    .any(|param| code_mentions(&body, param))
                {
                    return None;
                }
                Some(Block::new(BlockKind::WhenValueChanged { widget, body }))
            }
            "Ended" => {
                let sound = self.target(object)?;
                let function = self.handler(handler, 0)?;
                let body = self.body(&function.body, Scope::default());
                Some(Block::new(BlockKind::WhenSoundEnded { sound, body }))
            }
            "ConnectionFailed" if self.is_service(object, "NetworkService") => {
                let function = self.handler(handler, 1)?;
                let body = self.body(&function.body, Scope::default());
                // The blocks can't use the reason.
                if function
                    .params
                    .iter()
                    .any(|param| code_mentions(&body, param))
                {
                    return None;
                }
                Some(Block::new(BlockKind::WhenConnectionFails { body }))
            }
            "Completed" => {
                let sequence = self.target(object)?;
                let function = self.handler(handler, 0)?;
                let body = self.body(&function.body, Scope::default());
                Some(Block::new(BlockKind::WhenCutsceneFinished {
                    sequence,
                    body,
                }))
            }
            "StateChanged" => {
                let animator = self.target(object)?;
                let function = self.handler(handler, 2)?;
                let body = self.body(&function.body, Scope::default());
                // The blocks can't use the handler's parameters.
                if function
                    .params
                    .iter()
                    .any(|param| code_mentions(&body, param))
                {
                    return None;
                }
                Some(Block::new(BlockKind::WhenStateChanged { animator, body }))
            }
            "AnimationEvent" => {
                let animator = self.target(object)?;
                let (event, body) = self.one_name_branch(handler)?;
                Some(Block::new(BlockKind::WhenAnimationEvent {
                    animator,
                    event,
                    body,
                }))
            }
            "PlayerSensed" => {
                let agent = self.target(object)?;
                let (sense, body) = self.one_name_branch(handler)?;
                if !crate::SENSES.contains(&sense.as_str()) {
                    return None;
                }
                Some(Block::new(BlockKind::WhenAgentSenses {
                    agent,
                    sense,
                    body,
                }))
            }
            "Reached" => {
                let follower = self.target(object)?;
                let (which, body) = self.one_name_branch(handler)?;
                if !crate::SPLINE_ENDS.contains(&which.as_str()) {
                    return None;
                }
                Some(Block::new(BlockKind::WhenSplineReached {
                    follower,
                    which,
                    body,
                }))
            }
            "Heartbeat" if self.is_service(object, "RunService") => {
                let function = self.handler(handler, 1)?;
                let body = self.body(&function.body, Scope::default());
                if let Some(param) = function.params.first()
                    && param != "deltaTime"
                    && code_mentions(&body, param)
                {
                    return None;
                }
                Some(Block::new(BlockKind::EveryFrame { body }))
            }
            "Stepped" if self.is_service(object, "RunService") => {
                let function = self.handler(handler, 2)?;
                let body = self.body(&function.body, Scope::default());
                let named = ["time", "deltaTime"];
                if function
                    .params
                    .iter()
                    .zip(named)
                    .any(|(param, name)| param != name && code_mentions(&body, param))
                {
                    return None;
                }
                Some(Block::new(BlockKind::BeforePhysics { body }))
            }
            "RenderStepped" if self.is_service(object, "RunService") => {
                let function = self.handler(handler, 1)?;
                let body = self.body(&function.body, Scope::default());
                if let Some(param) = function.params.first()
                    && param != "deltaTime"
                    && code_mentions(&body, param)
                {
                    return None;
                }
                Some(Block::new(BlockKind::BeforeRender { body }))
            }
            "Event" => {
                let event = self.stored(object)?;
                let function = self.handler(handler, 0)?;
                let body = self.body(&function.body, Scope::default());
                Some(Block::new(BlockKind::WhenEventFired { event, body }))
            }
            "InputBegan" if self.is_service(object, "UserInputService") => {
                let function = self.handler(handler, 1)?;
                let param = function.params.first()?;
                let [
                    Stat {
                        kind: StatKind::If { arms, otherwise },
                        ..
                    },
                ] = function.body.as_slice()
                else {
                    return None;
                };
                let ([(condition, inner)], None) = (arms.as_slice(), otherwise) else {
                    return None;
                };
                let key = self.key_check(condition, param)?;
                let body = self.body(inner, Scope::default());
                if param != "input" && code_mentions(&body, param) {
                    return None;
                }
                Some(Block::new(BlockKind::WhenKeyPressed { key, body }))
            }
            _ => None,
        }
    }

    /// `function(...) ... end` with at most `params` parameters. One with
    /// type annotations stays code, since an event block would lose them.
    /// A handler of one parameter whose whole body is `if param == "name"
    /// then ... end`, as the blocks for events that name something write
    /// it: the name, and the branch as blocks, which mustn't use the parameter.
    /// `seat:GetPropertyChangedSignal("Occupant"):Connect(function() if
    /// seat.Occupant then ... end end)`: when someone sits on the seat.
    fn seat_occupied(&self, object: &Expr, handler: &Expr) -> Option<Block> {
        let seat = self.target(object)?;
        let function = self.handler(handler, 0)?;
        let [
            Stat {
                kind: StatKind::If { arms, otherwise },
                ..
            },
        ] = function.body.as_slice()
        else {
            return None;
        };
        let ([(condition, inner)], None) = (arms.as_slice(), otherwise) else {
            return None;
        };
        let ExprKind::Field(checked, field) = &condition.kind else {
            return None;
        };
        if field != "Occupant" || self.target(checked)? != seat {
            return None;
        }
        let body = self.body(inner, Scope::default());
        Some(Block::new(BlockKind::WhenSeatOccupied { seat, body }))
    }

    fn one_name_branch(&self, handler: &Expr) -> Option<(String, Vec<Block>)> {
        let function = self.handler(handler, 1)?;
        let param = function.params.first()?;
        let [
            Stat {
                kind: StatKind::If { arms, otherwise },
                ..
            },
        ] = function.body.as_slice()
        else {
            return None;
        };
        let ([(condition, inner)], None) = (arms.as_slice(), otherwise) else {
            return None;
        };
        let ExprKind::Binary("==", left, right) = &condition.kind else {
            return None;
        };
        let (ExprKind::Name(name), ExprKind::Str(value)) = (&left.kind, &right.kind) else {
            return None;
        };
        if name != param {
            return None;
        }
        let body = self.body(inner, Scope::default());
        if code_mentions(&body, param) {
            return None;
        }
        Some((value.clone(), body))
    }

    fn handler<'e>(&self, expr: &'e Expr, params: usize) -> Option<&'e FunctionBody> {
        match &expr.kind {
            ExprKind::Function(body)
                if !body.vararg && !body.annotated && body.params.len() <= params =>
            {
                Some(body)
            }
            _ => None,
        }
    }

    /// `param.KeyCode == Enum.KeyCode.Name`, as the key's name.
    fn key_check(&self, condition: &Expr, param: &str) -> Option<String> {
        let ExprKind::Binary("==", left, right) = &condition.kind else {
            return None;
        };
        let ExprKind::Field(input, field) = &left.kind else {
            return None;
        };
        if field != "KeyCode" || !matches!(&input.kind, ExprKind::Name(name) if name == param) {
            return None;
        }
        self.key(right)
    }

    /// `Enum.KeyCode.Name`, as `Name`.
    fn key(&self, expr: &Expr) -> Option<String> {
        let ExprKind::Field(key_code, name) = &expr.kind else {
            return None;
        };
        let ExprKind::Field(enum_name, key_code_name) = &key_code.kind else {
            return None;
        };
        (key_code_name == "KeyCode"
            && matches!(&enum_name.kind, ExprKind::Name(name) if name == "Enum"))
        .then(|| name.clone())
    }

    fn is_field(&self, expr: &Expr, object: &str, field: &str) -> bool {
        matches!(&expr.kind, ExprKind::Field(inner, name)
            if name == field && matches!(&inner.kind, ExprKind::Name(o) if o == object))
    }

    /// The table's name in `DataService:GetTable("weapons")`.
    fn data_table(&self, expr: &Expr) -> Option<String> {
        match &expr.kind {
            ExprKind::Method(service, method, args)
                if method == "GetTable" && self.is_service(service, "DataService") =>
            {
                match args.as_slice() {
                    [
                        Expr {
                            kind: ExprKind::Str(table),
                            ..
                        },
                    ] => Some(table.clone()),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Whether `expr` is a "row of table" block's Lua.
    fn is_data_row(&self, expr: &Expr) -> bool {
        matches!(&expr.kind, ExprKind::Method(table, method, args)
            if method == "Get" && args.len() == 1 && self.data_table(table).is_some())
    }

    fn is_service(&self, expr: &Expr, service: &str) -> bool {
        self.services.contains(service)
            && matches!(&expr.kind, ExprKind::Name(name) if name == service)
    }

    /// An object in ReplicatedStorage, such as
    /// `game.ReplicatedStorage.Events.Scored`, as its path there `Events.Scored`.
    fn stored(&self, expr: &Expr) -> Option<String> {
        fn parts(expr: &Expr, out: &mut Vec<String>) -> Option<()> {
            match &expr.kind {
                ExprKind::Field(game, name)
                    if name == "ReplicatedStorage"
                        && matches!(&game.kind, ExprKind::Name(g) if g == "game") =>
                {
                    Some(())
                }
                ExprKind::Field(inner, name) => {
                    parts(inner, out)?;
                    out.push(name.clone());
                    Some(())
                }
                ExprKind::Index(inner, key) => {
                    let ExprKind::Str(name) = &key.kind else {
                        return None;
                    };
                    if name.is_empty() || name.contains('.') {
                        return None;
                    }
                    parts(inner, out)?;
                    out.push(name.clone());
                    Some(())
                }
                _ => None,
            }
        }
        let mut path = Vec::new();
        parts(expr, &mut path)?;
        (!path.is_empty()).then(|| path.join("."))
    }

    /// The one object `RaycastParams.new({FilterDescendantsInstances = {x}})`
    /// passes by, as its path: what "raycast … ignoring" writes.
    fn ignored(&self, params: &Expr) -> Option<String> {
        let ExprKind::Call(function, args) = &params.kind else {
            return None;
        };
        let ExprKind::Field(library, new) = &function.kind else {
            return None;
        };
        if new != "new" || !matches!(&library.kind, ExprKind::Name(name) if name == "RaycastParams")
        {
            return None;
        }
        let [
            Expr {
                kind: ExprKind::Table(fields),
                ..
            },
        ] = args.as_slice()
        else {
            return None;
        };
        let [lua::TableField::Named(field, list)] = fields.as_slice() else {
            return None;
        };
        let ExprKind::Table(items) = &list.kind else {
            return None;
        };
        match (field.as_str(), items.as_slice()) {
            ("FilterDescendantsInstances", [lua::TableField::Item(item)]) => self.target(item),
            _ => None,
        }
    }

    /// Like [`Self::target`], with `workspace` itself as the empty path.
    fn container(&self, expr: &Expr) -> Option<String> {
        match &expr.kind {
            ExprKind::Name(name) if name == "workspace" => Some(String::new()),
            _ => self.target(expr),
        }
    }

    /// An object under the Workspace, such as `workspace.Pad.Chime`, as its
    /// path `Pad.Chime`.
    fn target(&self, expr: &Expr) -> Option<String> {
        fn parts(expr: &Expr, out: &mut Vec<String>) -> Option<()> {
            match &expr.kind {
                ExprKind::Name(name) if name == "workspace" => Some(()),
                ExprKind::Field(inner, name) => {
                    parts(inner, out)?;
                    out.push(name.clone());
                    Some(())
                }
                ExprKind::Index(inner, key) => {
                    let ExprKind::Str(name) = &key.kind else {
                        return None;
                    };
                    if name.is_empty() || name.contains('.') {
                        return None;
                    }
                    parts(inner, out)?;
                    out.push(name.clone());
                    Some(())
                }
                _ => None,
            }
        }
        let mut path = Vec::new();
        parts(expr, &mut path)?;
        (!path.is_empty()).then(|| path.join("."))
    }

    // ---- Statements ---------------------------------------------------------

    /// The blocks for a body, with its comments and blank lines kept.
    fn body(&self, stats: &[Stat], scope: Scope) -> Vec<Block> {
        let mut blocks: Vec<Block> = Vec::new();
        let mut previous_end: Option<usize> = None;
        // Grey blocks already have the comments inside them.
        let mut covered_to = 0;
        let mut index = 0;
        while index < stats.len() {
            let stat = &stats[index];
            index += 1;
            match stat.kind {
                StatKind::Empty => continue,
                StatKind::Comment if stat.span.end <= covered_to => continue,
                StatKind::Comment => {
                    let mut text = self.text(stat.span.clone());
                    let mut end = stat.span.end;
                    // A comment over several lines is one block.
                    while let Some(next) = stats.get(index)
                        && matches!(next.kind, StatKind::Comment)
                        && next.span.start >= covered_to
                        && self
                            .source
                            .get(end..next.span.start)
                            .is_some_and(|between| between.matches('\n').count() == 1)
                    {
                        text.push('\n');
                        text.push_str(&self.text(next.span.clone()));
                        end = next.span.end;
                        index += 1;
                    }
                    let same_line = previous_end
                        .and_then(|before| self.source.get(before..stat.span.start))
                        .is_some_and(|between| !between.contains('\n'));
                    let mut block = Block::new(BlockKind::Comment { text });
                    block.spaced = self.gap_before(previous_end, stat.span.start);
                    block.chained = same_line && !blocks.is_empty();
                    previous_end = Some(end);
                    blocks.push(block);
                    continue;
                }
                _ => {}
            }
            let mut block = self.statement(stat, scope);
            block.spaced = self.gap_before(previous_end, stat.span.start);
            if matches!(block.kind, BlockKind::Code { .. }) {
                covered_to = stat.span.end;
            }
            previous_end = Some(stat.span.end);
            blocks.push(block);
        }
        blocks
    }

    fn code_block(&self, span: std::ops::Range<usize>) -> Block {
        Block::new(BlockKind::Code {
            code: self.text(span),
        })
    }

    /// The block for a statement: a block made for it, else the one for
    /// what it is in Luau, else grey Lua. Each has to write the same code.
    fn statement(&self, stat: &Stat, scope: Scope) -> Block {
        let touched = scope.touched.is_some();
        for kind in [
            self.try_statement(stat, scope),
            self.luau_statement(stat, scope),
        ]
        .into_iter()
        .flatten()
        {
            let block = Block::new(kind);
            if self.writes_statement(&block, stat, touched) {
                return block;
            }
        }
        self.code_block(stat.span.clone())
    }

    /// The block for what a statement is in Luau.
    fn luau_statement(&self, stat: &Stat, scope: Scope) -> Option<BlockKind> {
        let define = |name: &str, local: bool, function: &FunctionBody| BlockKind::DefineFunction {
            name: name.to_owned(),
            local,
            params: function.written_params.clone(),
            generics: function.generics.clone(),
            returns: function.returns.clone(),
            attributes: function.attributes.clone(),
            body: self.body(&function.body, scope),
        };
        Some(match &stat.kind {
            StatKind::Local { names, values } => BlockKind::Local {
                names: names.join(", "),
                values: self.expressions(values, scope),
            },
            StatKind::Assign { targets, values } => BlockKind::Assign {
                targets: self.expressions(targets, scope),
                values: self.expressions(values, scope),
            },
            StatKind::CompoundAssign { target, op, value } => BlockKind::CompoundAssign {
                target: self.expression(target, scope),
                op: (*op).to_owned(),
                value: self.expression(value, scope),
            },
            StatKind::Call(call) => {
                if let Some((signal, method, params, body)) = self.connection(stat, scope) {
                    return Some(BlockKind::Connect {
                        signal,
                        method,
                        params,
                        body,
                    });
                }
                match &call.kind {
                    ExprKind::Call(function, args) => BlockKind::CallFunction {
                        function: self.expression(function, scope),
                        args: self.expressions(args, scope),
                    },
                    ExprKind::Method(object, method, args) => BlockKind::CallMethod {
                        object: self.expression(object, scope),
                        method: method.clone(),
                        args: self.expressions(args, scope),
                    },
                    _ => return None,
                }
            }
            StatKind::While { condition, body } => BlockKind::While {
                condition: self.expression(condition, scope),
                body: self.body(body, scope),
            },
            StatKind::Repeat { body, condition } => BlockKind::RepeatLoop {
                body: self.body(body, scope),
                condition: self.expression(condition, scope),
            },
            StatKind::NumericFor {
                variable,
                start,
                stop,
                step,
                body,
            } => BlockKind::NumericFor {
                variable: variable.clone(),
                start: self.expression(start, scope),
                stop: self.expression(stop, scope),
                step: step.as_ref().map(|step| self.expression(step, scope)),
                body: self.body(body, scope),
            },
            StatKind::GenericFor {
                names,
                values,
                body,
            } => BlockKind::GenericFor {
                names: names.join(", "),
                values: self.expressions(values, scope),
                body: self.body(body, scope),
            },
            StatKind::Do(body) => BlockKind::DoBlock {
                body: self.body(body, scope),
            },
            StatKind::Function { name, function } => define(name, false, function),
            StatKind::LocalFunction { name, function } => define(name, true, function),
            StatKind::Return(values) => BlockKind::Return {
                values: self.expressions(values, scope),
            },
            StatKind::Break => BlockKind::Break,
            StatKind::Continue => BlockKind::Continue,
            _ => return None,
        })
    }

    fn expressions(&self, exprs: &[Expr], scope: Scope) -> Vec<Expression> {
        exprs
            .iter()
            .map(|expr| self.expression(expr, scope))
            .collect()
    }

    fn try_statement(&self, stat: &Stat, scope: Scope) -> Option<BlockKind> {
        match &stat.kind {
            StatKind::Call(call) => self.call_statement(call, scope),
            StatKind::Assign { targets, values } => {
                let ([target], [value]) = (targets.as_slice(), values.as_slice()) else {
                    return None;
                };
                self.assignment(target, value, scope)
            }
            StatKind::CompoundAssign {
                target,
                op: "+",
                value,
            } => self.add_to(target, value, scope),
            StatKind::While { condition, body } => self.while_loop(condition, body, scope),
            StatKind::NumericFor {
                variable,
                start,
                stop,
                step: None,
                body,
            } if variable == "_"
                && matches!(start.kind, ExprKind::Number(Some(value)) if value == 1.0) =>
            {
                Some(BlockKind::Repeat {
                    count: self.expression(stop, scope),
                    body: self.body(body, scope),
                })
            }
            // for _, child in workspace.Folder:GetChildren() do
            StatKind::GenericFor {
                names,
                values,
                body,
            } => {
                let (
                    [skip, variable],
                    [
                        Expr {
                            kind: ExprKind::Method(object, method, args),
                            ..
                        },
                    ],
                ) = (names.as_slice(), values.as_slice())
                else {
                    return None;
                };
                if skip != "_" {
                    return None;
                }
                let inner = Scope {
                    child: Some(variable.as_str()),
                    ..scope
                };
                let in_workspace =
                    matches!(&object.kind, ExprKind::Name(name) if name == "workspace");
                match (method.as_str(), args.as_slice()) {
                    ("GetChildren", []) => Some(BlockKind::ForEachChild {
                        target: self.container(object)?,
                        variable: variable.clone(),
                        body: self.body(body, inner),
                    }),
                    ("GetPartBoundsInRadius", [center, radius]) if in_workspace => {
                        Some(BlockKind::ForEachPartInRadius {
                            center: self.expression(center, scope),
                            radius: self.expression(radius, scope),
                            variable: variable.clone(),
                            body: self.body(body, inner),
                        })
                    }
                    ("GetPartsInPart", [part]) if in_workspace => {
                        Some(BlockKind::ForEachPartTouching {
                            target: self.target(part)?,
                            variable: variable.clone(),
                            body: self.body(body, inner),
                        })
                    }
                    _ => None,
                }
            }
            StatKind::If { arms, otherwise } => self.if_chain(arms, otherwise.as_deref(), scope),
            StatKind::Do(body) if matches!(body.as_slice(), [Stat { kind: StatKind::Return(values), .. }] if values.is_empty()) => {
                Some(BlockKind::StopScript)
            }
            StatKind::Do(body) => self
                .slide_in(body, scope)
                .or_else(|| self.set_friction(body, scope)),
            StatKind::Return(values) if values.is_empty() => Some(BlockKind::StopScript),
            _ => None,
        }
    }

    fn call_statement(&self, call: &Expr, scope: Scope) -> Option<BlockKind> {
        if let Some(tween) = self.tween(call, scope) {
            return Some(tween);
        }
        if let Some(save) = self.save_call(call, scope) {
            return Some(save);
        }
        if let Some(level) = self.level_call(call, scope) {
            return Some(level);
        }
        if let Some(network) = self.network_call(call, scope) {
            return Some(network);
        }
        if let Some(chat) = self.chat_call(call, scope) {
            return Some(chat);
        }
        match &call.kind {
            ExprKind::Call(function, args) => {
                if self.is_field(function, "task", "wait")
                    && let [seconds] = args.as_slice()
                {
                    return Some(BlockKind::Wait {
                        seconds: self.expression(seconds, scope),
                    });
                }
                if matches!(&function.kind, ExprKind::Name(name) if name == "print")
                    && let [value] = args.as_slice()
                {
                    return Some(BlockKind::Print {
                        value: self.expression(value, scope),
                    });
                }
                None
            }
            // workspace:SpawnDecal(texture,
            //     CFrame.lookAt(Vector3.new(), facing) + position, size, seconds)
            ExprKind::Method(object, method, args)
                if method == "SpawnDecal"
                    && matches!(&object.kind, ExprKind::Name(name) if name == "workspace")
                    && let [texture, placed, size, seconds] = args.as_slice()
                    && let ExprKind::Binary("+", turn, position) = &placed.kind
                    && let ExprKind::Call(look_at, look_args) = &turn.kind
                    && self.is_field(look_at, "CFrame", "lookAt")
                    && let [origin, facing] = look_args.as_slice()
                    && let ExprKind::Call(new, no_args) = &origin.kind
                    && self.is_field(new, "Vector3", "new")
                    && no_args.is_empty() =>
            {
                Some(BlockKind::SpawnDecal {
                    texture: self.expression(texture, scope),
                    position: self.expression(position, scope),
                    facing: self.expression(facing, scope),
                    size: self.expression(size, scope),
                    seconds: self.expression(seconds, scope),
                })
            }
            // workspace.CurrentCamera:Shake(intensity, seconds)
            ExprKind::Method(object, method, args)
                if method == "Shake"
                    && self.target(object).as_deref() == Some("CurrentCamera")
                    && args.len() == 2 =>
            {
                Some(BlockKind::ShakeCamera {
                    intensity: self.expression(&args[0], scope),
                    seconds: self.expression(&args[1], scope),
                })
            }
            ExprKind::Method(object, method, args)
                if method == "WaitForChild"
                    && let [
                        Expr {
                            kind: ExprKind::Str(name),
                            ..
                        },
                    ] = args.as_slice() =>
            {
                Some(BlockKind::WaitForChild {
                    target: self.container(object)?,
                    name: name.clone(),
                })
            }
            // An Animator's parameters and states, named by a string.
            ExprKind::Method(object, method, args)
                if method != "Kick"
                    && method != "RequestStreamAroundAsync"
                    && matches!(args.first().map(|arg| &arg.kind), Some(ExprKind::Str(_))) =>
            {
                let animator = self.target(object)?;
                let ExprKind::Str(name) = &args[0].kind else {
                    return None;
                };
                let name = name.clone();
                Some(match (method.as_str(), &args[1..]) {
                    ("SetBool", [value]) => BlockKind::SetAnimatorBool {
                        animator,
                        parameter: name,
                        value: self.condition(value, scope),
                    },
                    ("SetNumber", [value]) => BlockKind::SetAnimatorNumber {
                        animator,
                        parameter: name,
                        value: self.expression(value, scope),
                    },
                    ("SetLayerWeight", [weight, seconds]) => BlockKind::SetAnimatorLayerWeight {
                        animator,
                        layer: name,
                        weight: self.expression(weight, scope),
                        seconds: self.expression(seconds, scope),
                    },
                    ("SetTrigger", []) => BlockKind::SetAnimatorTrigger {
                        animator,
                        parameter: name,
                    },
                    ("ResetTrigger", []) => BlockKind::ResetAnimatorTrigger {
                        animator,
                        parameter: name,
                    },
                    ("Play", []) => BlockKind::PlayAnimatorState {
                        animator,
                        state: name,
                    },
                    _ => return None,
                })
            }
            // `animator:LoadAnimation(animation):Play()`.
            ExprKind::Method(object, method, args)
                if method == "Play"
                    && args.is_empty()
                    && matches!(&object.kind, ExprKind::Method(_, load, loaded)
                        if load == "LoadAnimation" && loaded.len() == 1) =>
            {
                let ExprKind::Method(animator, _, loaded) = &object.kind else {
                    return None;
                };
                Some(BlockKind::PlayAnimation {
                    animator: self.target(animator)?,
                    animation: self.target(&loaded[0])?,
                })
            }
            // game.ReplicatedStorage.Scored:Fire()
            ExprKind::Method(object, method, args)
                if method == "Fire" && args.is_empty() && self.stored(object).is_some() =>
            {
                Some(BlockKind::FireEvent {
                    event: self.stored(object)?,
                })
            }
            // Debris:AddItem(workspace.Spark, 2)
            ExprKind::Method(object, method, args)
                if method == "AddItem" && self.is_service(object, "Debris") =>
            {
                let [item, seconds] = args.as_slice() else {
                    return None;
                };
                Some(BlockKind::DestroyAfter {
                    target: self.target(item)?,
                    seconds: self.expression(seconds, scope),
                })
            }
            // workspace.Cart:PivotTo(workspace.Track:GetCFrameAt(12))
            ExprKind::Method(object, method, args)
                if method == "PivotTo"
                    && let [
                        Expr {
                            kind: ExprKind::Method(spline, at, distance),
                            ..
                        },
                    ] = args.as_slice()
                    && at == "GetCFrameAt"
                    && distance.len() == 1 =>
            {
                Some(BlockKind::PlaceOnSpline {
                    target: self.target(object)?,
                    spline: self.target(spline)?,
                    distance: self.expression(&distance[0], scope),
                })
            }
            // workspace.Guard.NavigationAgent:Patrol(workspace.Route)
            ExprKind::Method(object, method, args) if method == "Patrol" && args.len() == 1 => {
                Some(BlockKind::Patrol {
                    agent: self.target(object)?,
                    spline: self.target(&args[0])?,
                })
            }
            // workspace.Ball:ApplyImpulse(Vector3.new(0, 50, 0))
            ExprKind::Method(object, method, args)
                if method == "ApplyImpulse" && args.len() == 1 =>
            {
                Some(BlockKind::PushWithForce {
                    target: self.target(object)?,
                    force: self.expression(&args[0], scope),
                })
            }
            // workspace.Car.DriverSeat:SetNetworkOwner(player)
            ExprKind::Method(object, method, args)
                if method == "SetNetworkOwner" && args.len() == 1 =>
            {
                Some(BlockKind::SetNetworkOwner {
                    target: self.target(object)?,
                    owner: self.expression(&args[0], scope),
                })
            }
            // player:RequestStreamAroundAsync(Vector3.new(0, 0, 500))
            ExprKind::Method(object, method, args)
                if method == "RequestStreamAroundAsync" && args.len() == 1 =>
            {
                Some(BlockKind::RequestStreamAround {
                    player: self.expression(object, scope),
                    position: self.expression(&args[0], scope),
                })
            }
            // player:Kick("Cheating")
            ExprKind::Method(object, method, args) if method == "Kick" && args.len() == 1 => {
                Some(BlockKind::KickPlayer {
                    player: self.expression(object, scope),
                    reason: self.expression(&args[0], scope),
                })
            }
            ExprKind::Method(object, method, args) if args.is_empty() => {
                let target = self.target(object)?;
                Some(match method.as_str() {
                    "Play" => BlockKind::PlaySound { target },
                    "Stop" => BlockKind::StopSound { target },
                    "Pause" => BlockKind::PauseSound { target },
                    "Resume" => BlockKind::ResumeSound { target },
                    "Destroy" => BlockKind::Destroy { target },
                    "SetNetworkOwnershipAuto" => BlockKind::SetNetworkOwnershipAuto { target },
                    _ => return None,
                })
            }
            _ => None,
        }
    }

    /// `SaveService:SaveSlot(1)`, `LoadSlot`, `SetCheckpoint(workspace.Cave)`
    /// and `RestoreCheckpoint()`.
    fn save_call(&self, call: &Expr, scope: Scope) -> Option<BlockKind> {
        let ExprKind::Method(object, method, args) = &call.kind else {
            return None;
        };
        if !self.is_service(object, "SaveService") {
            return None;
        }
        Some(match (method.as_str(), args.as_slice()) {
            ("SaveSlot", [slot]) => BlockKind::SaveSlot {
                slot: self.expression(slot, scope),
            },
            ("LoadSlot", [slot]) => BlockKind::LoadSlot {
                slot: self.expression(slot, scope),
            },
            ("SetCheckpoint", [checkpoint]) => BlockKind::SetCheckpoint {
                checkpoint: self.target(checkpoint)?,
            },
            ("RestoreCheckpoint", []) => BlockKind::RestoreCheckpoint,
            _ => return None,
        })
    }

    /// `LevelService:LoadLevel("Castle", "Gate")`, `AddLevel("Dungeon")` and
    /// `RemoveLevel("Dungeon")`.
    fn level_call(&self, call: &Expr, scope: Scope) -> Option<BlockKind> {
        let ExprKind::Method(object, method, args) = &call.kind else {
            return None;
        };
        if !self.is_service(object, "LevelService") {
            return None;
        }
        Some(match (method.as_str(), args.as_slice()) {
            ("LoadLevel", [level]) => BlockKind::LoadLevel {
                level: self.expression(level, scope),
                spawn: Expression::Text {
                    value: String::new(),
                },
            },
            ("LoadLevel", [level, spawn]) => BlockKind::LoadLevel {
                level: self.expression(level, scope),
                spawn: self.expression(spawn, scope),
            },
            ("AddLevel", [level]) => BlockKind::AddLevel {
                level: self.expression(level, scope),
            },
            ("RemoveLevel", [level]) => BlockKind::RemoveLevel {
                level: self.expression(level, scope),
            },
            _ => return None,
        })
    }

    /// `NetworkService:Host(47800)`, `Join("192.168.1.20:47800")` and
    /// `Disconnect()`. With a password they stay Lua.
    fn network_call(&self, call: &Expr, scope: Scope) -> Option<BlockKind> {
        let ExprKind::Method(object, method, args) = &call.kind else {
            return None;
        };
        if !self.is_service(object, "NetworkService") {
            return None;
        }
        Some(match (method.as_str(), args.as_slice()) {
            ("Host", [port]) => BlockKind::HostGame {
                port: self.expression(port, scope),
            },
            ("Join", [address]) => BlockKind::JoinGame {
                address: self.expression(address, scope),
            },
            ("Disconnect", []) => BlockKind::LeaveGame,
            _ => return None,
        })
    }

    /// `TextChatService:SendMessage(text)` and `DisplaySystemMessage(text)`.
    fn chat_call(&self, call: &Expr, scope: Scope) -> Option<BlockKind> {
        let ExprKind::Method(object, method, args) = &call.kind else {
            return None;
        };
        if !self.is_service(object, "TextChatService") {
            return None;
        }
        Some(match (method.as_str(), args.as_slice()) {
            ("SendMessage", [text]) => BlockKind::SendChatMessage {
                text: self.expression(text, scope),
            },
            ("DisplaySystemMessage", [text]) => BlockKind::ShowSystemMessage {
                text: self.expression(text, scope),
            },
            _ => return None,
        })
    }

    /// `TweenService:Create(object, TweenInfo.new(...), {Property = value}):Play()`:
    /// a fade when it's `BackgroundTransparency` to 0 or 1 with the
    /// default easing, else a tween.
    fn tween(&self, call: &Expr, scope: Scope) -> Option<BlockKind> {
        let (target, info, property, value) = self.tween_parts(call)?;
        let ExprKind::Call(new, info) = &info.kind else {
            return None;
        };
        if !self.is_field(new, "TweenInfo", "new") {
            return None;
        }
        let (seconds, style, direction) = match info.as_slice() {
            [seconds] => {
                if property == "BackgroundTransparency" {
                    let seconds = self.expression(seconds, scope);
                    match value.kind {
                        ExprKind::Number(Some(0.0)) => {
                            return Some(BlockKind::FadeIn { target, seconds });
                        }
                        ExprKind::Number(Some(1.0)) => {
                            return Some(BlockKind::FadeOut { target, seconds });
                        }
                        _ => {}
                    }
                }
                // What `TweenInfo.new` leaves out.
                (seconds, "Quad".to_owned(), "Out".to_owned())
            }
            [seconds, style, direction] => (
                seconds,
                enum_item(style, "EasingStyle", &EASING_STYLES)?,
                enum_item(direction, "EasingDirection", &EASING_DIRECTIONS)?,
            ),
            _ => return None,
        };
        Some(BlockKind::Tween {
            target,
            property,
            value: self.expression(value, scope),
            seconds: self.expression(seconds, scope),
            style,
            direction,
        })
    }

    /// The object, TweenInfo, property and value of a tween played as
    /// it's made, with one goal.
    fn tween_parts<'e>(&self, call: &'e Expr) -> Option<(String, &'e Expr, String, &'e Expr)> {
        let ExprKind::Method(created, play, played) = &call.kind else {
            return None;
        };
        let ExprKind::Method(service, create, args) = &created.kind else {
            return None;
        };
        if play != "Play"
            || !played.is_empty()
            || create != "Create"
            || !self.is_service(service, "TweenService")
        {
            return None;
        }
        let [object, info, goals] = args.as_slice() else {
            return None;
        };
        let ExprKind::Table(fields) = &goals.kind else {
            return None;
        };
        let [lua::TableField::Named(property, value)] = fields.as_slice() else {
            return None;
        };
        Some((self.target(object)?, info, property.clone(), value))
    }

    /// The `do … end` that "slide in" writes.
    fn slide_in(&self, body: &[Stat], scope: Scope) -> Option<BlockKind> {
        let [info, goal, moved, play] = body else {
            return None;
        };
        let is_name = |expr: &Expr, wanted: &str| matches!(&expr.kind, ExprKind::Name(name) if name == wanted);
        let ExprKind::Call(new, info) = &local(info, "info")?.kind else {
            return None;
        };
        let ([seconds], true) = (info.as_slice(), self.is_field(new, "TweenInfo", "new")) else {
            return None;
        };
        let ExprKind::Field(object, position) = &local(goal, "goal")?.kind else {
            return None;
        };
        let target = self.target(object).filter(|_| position == "Position")?;
        let StatKind::Assign { targets, values } = &moved.kind else {
            return None;
        };
        let ([moved_object], [offset]) = (targets.as_slice(), values.as_slice()) else {
            return None;
        };
        let same_position = |expr: &Expr| {
            matches!(&expr.kind, ExprKind::Field(inner, name)
                if name == "Position" && self.target(inner).as_deref() == Some(target.as_str()))
        };
        if !same_position(moved_object) {
            return None;
        }
        let ExprKind::Binary(op, from, by) = &offset.kind else {
            return None;
        };
        let ExprKind::Call(from_scale, scale) = &by.kind else {
            return None;
        };
        if !is_name(from, "goal") || !self.is_field(from_scale, "UDim2", "fromScale") {
            return None;
        }
        let scale: Vec<Option<f64>> = scale
            .iter()
            .map(|arg| match arg.kind {
                ExprKind::Number(value) => value,
                _ => None,
            })
            .collect();
        let from = match (*op, scale.as_slice()) {
            ("-", [Some(1.0), Some(0.0)]) => "Left",
            ("+", [Some(1.0), Some(0.0)]) => "Right",
            ("-", [Some(0.0), Some(1.0)]) => "Top",
            ("+", [Some(0.0), Some(1.0)]) => "Bottom",
            _ => return None,
        };
        let StatKind::Call(play) = &play.kind else {
            return None;
        };
        let (played, info, property, value) = self.tween_parts(play)?;
        if played != target
            || !is_name(info, "info")
            || property != "Position"
            || !is_name(value, "goal")
        {
            return None;
        }
        Some(BlockKind::SlideIn {
            target,
            from: from.to_owned(),
            seconds: self.expression(seconds, scope),
        })
    }

    /// The `do` block "set friction" translates to: the friction in a
    /// local, the part's current properties in another, then new custom
    /// ones with that friction.
    fn set_friction(&self, body: &[Stat], scope: Scope) -> Option<BlockKind> {
        let [friction, physical, assign] = body else {
            return None;
        };
        let friction = local(friction, "friction")?;
        let ExprKind::Field(object, current) = &local(physical, "physical")?.kind else {
            return None;
        };
        let target = self
            .target(object)
            .filter(|_| current == "CurrentPhysicalProperties")?;
        let StatKind::Assign { targets, values } = &assign.kind else {
            return None;
        };
        let ([custom], [made]) = (targets.as_slice(), values.as_slice()) else {
            return None;
        };
        let same_custom = matches!(&custom.kind, ExprKind::Field(inner, name)
            if name == "CustomPhysicalProperties"
                && self.target(inner).as_deref() == Some(target.as_str()));
        let ExprKind::Call(new, args) = &made.kind else {
            return None;
        };
        let is_name = |expr: &Expr, wanted: &str| matches!(&expr.kind, ExprKind::Name(name) if name == wanted);
        let is_physical = |expr: &Expr, field: &str| {
            matches!(&expr.kind, ExprKind::Field(inner, name)
            if name == field && is_name(inner, "physical"))
        };
        let [
            density,
            given,
            elasticity,
            friction_weight,
            elasticity_weight,
        ] = args.as_slice()
        else {
            return None;
        };
        let matches = same_custom
            && self.is_field(new, "PhysicalProperties", "new")
            && is_physical(density, "Density")
            && is_name(given, "friction")
            && is_physical(elasticity, "Elasticity")
            && is_physical(friction_weight, "FrictionWeight")
            && is_physical(elasticity_weight, "ElasticityWeight");
        matches.then(|| BlockKind::SetFriction {
            target,
            friction: self.expression(friction, scope),
        })
    }

    /// `UserGameSettings.<property> = value`, as a graphics block.
    fn graphics_setting(&self, property: &str, value: &Expr, scope: Scope) -> Option<BlockKind> {
        if property == "GraphicsQuality" {
            return Some(BlockKind::SetGraphicsQuality {
                quality: enum_item(value, "GraphicsQuality", &GRAPHICS_QUALITIES)?,
            });
        }
        if GRAPHICS_NUMBERS.contains(&property) {
            return Some(BlockKind::SetGraphicsNumber {
                setting: property.to_owned(),
                value: self.expression(value, scope),
            });
        }
        if !GRAPHICS_OPTIONS.contains(&property) {
            return None;
        }
        let choice = match (&value.kind, property) {
            (ExprKind::Nil, _) => PRESET.to_owned(),
            (ExprKind::True, setting) if GRAPHICS_SWITCHES.contains(&setting) => "On".to_owned(),
            (ExprKind::False, setting) if GRAPHICS_SWITCHES.contains(&setting) => "Off".to_owned(),
            (_, setting) if GRAPHICS_SWITCHES.contains(&setting) => return None,
            (_, setting) => {
                let items = graphics_choices(setting);
                // Every item but the last, which is `PRESET`.
                enum_item(value, setting, &items[..items.len() - 1])?
            }
        };
        Some(BlockKind::SetGraphicsOption {
            setting: property.to_owned(),
            choice,
        })
    }

    fn assignment(&self, target: &Expr, value: &Expr, scope: Scope) -> Option<BlockKind> {
        if let ExprKind::Name(name) = &target.kind {
            if !self.variables.contains(name) {
                return None;
            }
            // name = name + n
            if let ExprKind::Binary("+", left, right) = &value.kind
                && matches!(&left.kind, ExprKind::Name(same) if same == name)
            {
                return Some(BlockKind::ChangeVariable {
                    name: name.clone(),
                    by: self.expression(right, scope),
                });
            }
            return Some(BlockKind::SetVariable {
                name: name.clone(),
                value: self.expression(value, scope),
            });
        }
        let ExprKind::Field(object, property) = &target.kind else {
            return None;
        };
        // workspace.Coin:Clone().Parent = workspace
        if let ExprKind::Method(original, method, args) = &object.kind
            && property == "Parent"
            && method == "Clone"
            && args.is_empty()
        {
            return Some(BlockKind::CloneInto {
                target: self.target(original)?,
                parent: self.container(value)?,
            });
        }
        // Lighting.ToneMapping = Enum.ToneMapping.Neutral
        if property == "ToneMapping" && self.is_service(object, "Lighting") {
            return Some(BlockKind::SetToneMapping {
                curve: enum_item(value, "ToneMapping", &TONE_MAPPINGS)?,
            });
        }
        // Lighting.ClockTime = 18
        if property == "ClockTime" && self.is_service(object, "Lighting") {
            return Some(BlockKind::SetClockTime {
                time: self.expression(value, scope),
            });
        }
        // Lighting.Atmosphere.Haze = 2
        if let ExprKind::Field(parent, child) = &object.kind
            && child == "Atmosphere"
            && self.is_service(parent, "Lighting")
            && ATMOSPHERE_NUMBERS.contains(&property.as_str())
        {
            return Some(BlockKind::SetAtmosphere {
                property: property.clone(),
                value: self.expression(value, scope),
            });
        }
        if self.is_service(object, "UserGameSettings") {
            return self.graphics_setting(property, value, scope);
        }
        // LocalizationService.Language = "Français"
        if property == "Language" && self.is_service(object, "LocalizationService") {
            return Some(BlockKind::SetLanguage {
                language: self.expression(value, scope),
            });
        }
        // workspace.CurrentCamera.CameraType = Enum.CameraType.Scriptable
        if property == "CameraType" && self.target(object).as_deref() == Some("CurrentCamera") {
            return Some(BlockKind::SetCameraType {
                camera_type: enum_item(value, "CameraType", &CAMERA_TYPES)?,
            });
        }
        let path = self.target(object)?;
        let same_property = |expr: &Expr| {
            matches!(&expr.kind, ExprKind::Field(inner, name)
                if name == property && self.target(inner).as_deref() == Some(path.as_str()))
        };
        // object.CFrame = CFrame.lookAt(object.Position, v)
        if property == "CFrame"
            && let ExprKind::Call(function, args) = &value.kind
            && self.is_field(function, "CFrame", "lookAt")
            && let [from, position] = args.as_slice()
            && matches!(&from.kind, ExprKind::Field(inner, name)
                if name == "Position" && self.target(inner).as_deref() == Some(path.as_str()))
        {
            let position = self.expression(position, scope);
            if path == "CurrentCamera" {
                return Some(BlockKind::PointCameraAt { position });
            }
            return Some(BlockKind::LookAt {
                target: path,
                position,
            });
        }
        // object.Position = object.Position + v
        if let ExprKind::Binary("+", left, right) = &value.kind
            && same_property(left)
        {
            if property == "Position"
                && let Some(distance) = self.forward_distance(&path, right)
            {
                return Some(BlockKind::MoveForward {
                    target: path,
                    distance: self.expression(distance, scope),
                });
            }
            let by = self.expression(right, scope);
            match property.as_str() {
                "Position" => {
                    return Some(BlockKind::MoveBy {
                        target: path,
                        offset: by,
                    });
                }
                "Orientation" => {
                    return Some(BlockKind::TurnBy {
                        target: path,
                        rotation: by,
                    });
                }
                _ => {}
            }
        }
        let converted = || self.expression(value, scope);
        Some(match property.as_str() {
            "Position" => BlockKind::MoveTo {
                target: path,
                position: converted(),
            },
            "Orientation" => BlockKind::TurnTo {
                target: path,
                orientation: converted(),
            },
            "Anchored" => BlockKind::SetAnchored {
                target: path,
                anchored: converted(),
            },
            "Color" => BlockKind::SetColor {
                target: path,
                color: converted(),
            },
            "Text" => {
                // tostring(v), which the translation adds back.
                let text = match &value.kind {
                    ExprKind::Call(function, args)
                        if matches!(&function.kind, ExprKind::Name(name) if name == "tostring")
                            && args.len() == 1 =>
                    {
                        self.expression(&args[0], scope)
                    }
                    _ => converted(),
                };
                BlockKind::SetText { target: path, text }
            }
            "CurrentPage" => BlockKind::ShowPage {
                layout: path,
                page: converted(),
            },
            // Vector2.new(x, y), which the translation writes.
            "CanvasPosition"
                if matches!(&value.kind, ExprKind::Call(function, args)
                    if args.len() == 2
                        && matches!(&function.kind, ExprKind::Field(library, new)
                            if new == "new"
                                && matches!(&library.kind, ExprKind::Name(name) if name == "Vector2"))) =>
            {
                let ExprKind::Call(_, args) = &value.kind else {
                    unreachable!("checked just above");
                };
                BlockKind::ScrollTo {
                    target: path,
                    x: self.expression(&args[0], scope),
                    y: self.expression(&args[1], scope),
                }
            }
            "Visible" if matches!(value.kind, ExprKind::True) => BlockKind::Show { target: path },
            "Visible" if matches!(value.kind, ExprKind::False) => BlockKind::Hide { target: path },
            "Volume" => BlockKind::SetVolume {
                target: path,
                volume: converted(),
            },
            "Value" => BlockKind::SetValue {
                target: path,
                value: converted(),
            },
            "Destination" => BlockKind::SetDestination {
                agent: path,
                position: converted(),
            },
            "Velocity" => BlockKind::SetVelocity {
                agent: path,
                velocity: converted(),
            },
            "Sit" if matches!(value.kind, ExprKind::False) => {
                BlockKind::LeaveSeat { controller: path }
            }
            "AssemblyLinearVelocity" => BlockKind::SetPartVelocity {
                target: path,
                velocity: converted(),
            },
            _ => BlockKind::SetProperty {
                target: path,
                property: property.clone(),
                value: converted(),
            },
        })
    }

    /// The distance in `object.CFrame.LookVector * n`, where `object` is at `path`.
    fn forward_distance<'e>(&self, path: &str, expr: &'e Expr) -> Option<&'e Expr> {
        let ExprKind::Binary("*", look, distance) = &expr.kind else {
            return None;
        };
        let ExprKind::Field(cframe, name) = &look.kind else {
            return None;
        };
        let ExprKind::Field(object, cframe_name) = &cframe.kind else {
            return None;
        };
        (name == "LookVector"
            && cframe_name == "CFrame"
            && self.target(object).as_deref() == Some(path))
        .then_some(distance.as_ref())
    }

    /// `name += n`, `object.Position += v` and `object.Orientation += v`:
    /// the blocks that the translation writes as `name = name + n` and so on.
    fn add_to(&self, target: &Expr, value: &Expr, scope: Scope) -> Option<BlockKind> {
        match &target.kind {
            ExprKind::Name(name) if self.variables.contains(name) => {
                Some(BlockKind::ChangeVariable {
                    name: name.clone(),
                    by: self.expression(value, scope),
                })
            }
            ExprKind::Field(object, property) => {
                let target = self.target(object)?;
                if property == "Position"
                    && let Some(distance) = self.forward_distance(&target, value)
                {
                    return Some(BlockKind::MoveForward {
                        target,
                        distance: self.expression(distance, scope),
                    });
                }
                let by = self.expression(value, scope);
                match property.as_str() {
                    "Position" => Some(BlockKind::MoveBy { target, offset: by }),
                    "Orientation" => Some(BlockKind::TurnBy {
                        target,
                        rotation: by,
                    }),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn while_loop(&self, condition: &Expr, body: &[Stat], scope: Scope) -> Option<BlockKind> {
        let waits_a_frame = |stat: &Stat| {
            matches!(&stat.kind, StatKind::Call(call)
                if matches!(&call.kind, ExprKind::Call(function, args)
                    if args.is_empty() && self.is_field(function, "task", "wait")))
        };
        let (last, rest) = body.split_last()?;
        if !waits_a_frame(last) {
            return None;
        }
        if matches!(condition.kind, ExprKind::True) {
            return Some(BlockKind::Forever {
                body: self.body(rest, scope),
            });
        }
        let ExprKind::Unary("not", inner) = &condition.kind else {
            return None;
        };
        let inner = match &inner.kind {
            ExprKind::Paren(inner) => inner,
            _ => inner,
        };
        let condition = self.condition(inner, scope);
        Some(if rest.is_empty() {
            BlockKind::WaitUntil { condition }
        } else {
            BlockKind::RepeatUntil {
                condition,
                body: self.body(rest, scope),
            }
        })
    }

    fn if_chain(
        &self,
        arms: &[(Expr, Vec<Stat>)],
        otherwise: Option<&[Stat]>,
        scope: Scope,
    ) -> Option<BlockKind> {
        let ((condition, then_body), rest) = arms.split_first()?;
        let condition = self.condition(condition, scope);
        let then_body = self.body(then_body, scope);
        if rest.is_empty() {
            return Some(match otherwise {
                Some(else_body) if !else_body.is_empty() => BlockKind::IfElse {
                    condition,
                    then_body,
                    else_body: self.body(else_body, scope),
                },
                _ => BlockKind::If {
                    condition,
                    body: then_body,
                },
            });
        }
        // elseif: an if inside the else, marked to be written as one.
        let mut inner = Block::new(self.if_chain(rest, otherwise, scope)?);
        inner.chained = true;
        Some(BlockKind::IfElse {
            condition,
            then_body,
            else_body: vec![inner],
        })
    }

    // ---- Values -------------------------------------------------------------

    /// A value for a pointed (true or false) slot.
    fn condition(&self, expr: &Expr, scope: Scope) -> Expression {
        let expression = self.expression(expr, scope);
        if expression.shape() == BlockShape::Boolean {
            expression
        } else {
            self.code_expression(expr)
        }
    }

    fn code_expression(&self, expr: &Expr) -> Expression {
        let text = self.text(expr.span.clone());
        // Operators are bracketed, so the code means the same wherever it
        // goes. So are `if` expressions, whose `else` part would take in
        // what follows, and casts.
        let code = match expr.kind {
            ExprKind::Binary(..)
            | ExprKind::Unary(..)
            | ExprKind::IfElse(..)
            | ExprKind::Cast(..) => format!("({text})"),
            _ => text,
        };
        Expression::Code { code }
    }

    /// The value for an expression: a reporter made for it, else what it is
    /// in Luau, else grey Lua. Each has to write the same code.
    fn expression(&self, expr: &Expr, scope: Scope) -> Expression {
        let touched = scope.touched.is_some();
        for value in [
            self.try_expression(expr, scope),
            self.luau_expression(expr, scope),
        ]
        .into_iter()
        .flatten()
        {
            if self.writes_expression(&value, expr, touched) {
                return value;
            }
        }
        self.code_expression(expr)
    }

    /// One side of an operator. Brackets the writer puts back itself
    /// aren't kept.
    fn operand(&self, expr: &Expr, op: &str, right: bool, scope: Scope) -> Expression {
        let parent = binary_priority(op).unwrap_or((UNARY, UNARY));
        if let ExprKind::Paren(inner) = &expr.kind {
            let child = match &inner.kind {
                ExprKind::Binary(child, ..) => binary_priority(child).map(|p| (p, false)),
                ExprKind::Unary(..) => Some(((UNARY, UNARY), true)),
                _ => None,
            };
            if let Some((child, unary)) = child
                && needs_brackets(child, unary, parent, right)
            {
                return self.expression(inner, scope);
            }
        }
        self.expression(expr, scope)
    }

    /// The value for what an expression is in Luau.
    fn luau_expression(&self, expr: &Expr, scope: Scope) -> Option<Expression> {
        let boxed = |expr: &Expr| Box::new(self.expression(expr, scope));
        if let Some(name) = path(expr) {
            return Some(Expression::Name { name });
        }
        Some(match &expr.kind {
            ExprKind::Nil => Expression::Nil,
            ExprKind::Vararg => Expression::Vararg,
            ExprKind::Field(object, name) => Expression::Field {
                object: boxed(object),
                name: name.clone(),
            },
            ExprKind::Index(object, key) => Expression::Index {
                object: boxed(object),
                key: boxed(key),
            },
            ExprKind::Call(function, args) => Expression::Call {
                function: boxed(function),
                args: self.expressions(args, scope),
            },
            ExprKind::Method(object, method, args) => Expression::MethodCall {
                object: boxed(object),
                method: method.clone(),
                args: self.expressions(args, scope),
            },
            ExprKind::Function(function) if function.attributes.is_empty() => Expression::Closure {
                params: function.written_params.clone(),
                generics: function.generics.clone(),
                returns: function.returns.clone(),
                body: self.body(&function.body, scope),
            },
            ExprKind::Table(fields) => Expression::Table {
                fields: fields
                    .iter()
                    .map(|field| match field {
                        lua::TableField::Named(name, value) => TableField::Named {
                            name: name.clone(),
                            value: self.expression(value, scope),
                        },
                        lua::TableField::Keyed(key, value) => TableField::Keyed {
                            key: self.expression(key, scope),
                            value: self.expression(value, scope),
                        },
                        lua::TableField::Item(value) => TableField::Item {
                            value: self.expression(value, scope),
                        },
                    })
                    .collect(),
                multiline: self.source[expr.span.clone()].contains('\n'),
            },
            ExprKind::Binary(op, left, right) => Expression::Binary {
                op: (*op).to_owned(),
                left: Box::new(self.operand(left, op, false, scope)),
                right: Box::new(self.operand(right, op, true, scope)),
            },
            ExprKind::Unary(op, value) => Expression::Unary {
                op: (*op).to_owned(),
                value: Box::new(self.operand(value, op, true, scope)),
            },
            ExprKind::Paren(value) => Expression::Paren {
                value: boxed(value),
            },
            ExprKind::Interpolated(parts, values) => Expression::Interpolated {
                parts: parts.clone(),
                values: self.expressions(values, scope),
            },
            ExprKind::IfElse(arms, otherwise) => {
                let mut parts = Vec::with_capacity(arms.len() * 2 + 1);
                for (condition, value) in arms {
                    parts.push(self.expression(condition, scope));
                    parts.push(self.expression(value, scope));
                }
                parts.push(self.expression(otherwise, scope));
                Expression::IfValue { parts }
            }
            ExprKind::Cast(value, annotation) => Expression::Cast {
                value: boxed(value),
                annotation: annotation.clone(),
            },
            _ => return None,
        })
    }

    fn number(&self, expr: &Expr) -> Option<f64> {
        match &expr.kind {
            ExprKind::Number(value) => *value,
            ExprKind::Unary("-", inner) => match inner.kind {
                ExprKind::Number(Some(value)) => Some(-value),
                _ => None,
            },
            _ => None,
        }
    }

    fn try_expression(&self, expr: &Expr, scope: Scope) -> Option<Expression> {
        let boxed = |expr: &Expr| Box::new(self.expression(expr, scope));
        if let Some(value) = self.number(expr) {
            // A number written another way (`0xFF`, `1e3`) stays as written.
            return (self.source[expr.span.clone()] == crate::number_text(value))
                .then_some(Expression::Number { value });
        }
        Some(match &expr.kind {
            // Escapes for raw bytes can't be held as text.
            ExprKind::Str(_) if self.has_byte_escape(expr) => return None,
            ExprKind::Str(value) => Expression::Text {
                value: value.clone(),
            },
            ExprKind::True => Expression::Boolean { value: true },
            ExprKind::False => Expression::Boolean { value: false },
            ExprKind::Name(name) if scope.touched == Some(name.as_str()) => {
                Expression::TouchingPart
            }
            ExprKind::Name(name)
                if self.variables.contains(name) || scope.child == Some(name.as_str()) =>
            {
                Expression::Variable { name: name.clone() }
            }
            // thing.StreamingState == Enum.StreamingState.StreamedIn
            ExprKind::Binary("==", left, right)
                if matches!(&left.kind, ExprKind::Field(_, field) if field == "StreamingState")
                    && is_streamed_in_item(right) =>
            {
                let ExprKind::Field(object, _) = &left.kind else {
                    unreachable!("checked above");
                };
                Expression::IsStreamedIn {
                    value: boxed(object),
                }
            }
            ExprKind::Binary(op, left, right) => {
                let math = match *op {
                    "+" => Some(MathOp::Add),
                    "-" => Some(MathOp::Subtract),
                    "*" => Some(MathOp::Multiply),
                    "/" => Some(MathOp::Divide),
                    "%" => Some(MathOp::Modulo),
                    "^" => Some(MathOp::Power),
                    _ => None,
                };
                let compare = match *op {
                    "==" => Some(Comparison::Equal),
                    "~=" => Some(Comparison::NotEqual),
                    "<" => Some(Comparison::Less),
                    "<=" => Some(Comparison::LessOrEqual),
                    ">" => Some(Comparison::Greater),
                    ">=" => Some(Comparison::GreaterOrEqual),
                    _ => None,
                };
                let side =
                    |expr: &Expr, right: bool| Box::new(self.operand(expr, op, right, scope));
                if let Some(op) = math {
                    Expression::Math {
                        op,
                        left: side(left, false),
                        right: side(right, true),
                    }
                } else if let Some(op) = compare {
                    Expression::Compare {
                        op,
                        left: side(left, false),
                        right: side(right, true),
                    }
                } else if *op == "and" || *op == "or" {
                    let (left, right) = (side(left, false), side(right, true));
                    // `a or 5` isn't true or false, so it's not this block.
                    if left.shape() != BlockShape::Boolean || right.shape() != BlockShape::Boolean {
                        return None;
                    }
                    let op = if *op == "and" {
                        LogicOp::And
                    } else {
                        LogicOp::Or
                    };
                    Expression::Logic { op, left, right }
                } else if *op == ".." {
                    // tostring(a) .. tostring(b), which the translation
                    // writes. Anything else isn't this block.
                    let text = |expr: &Expr| match &expr.kind {
                        ExprKind::Call(function, args)
                            if matches!(&function.kind, ExprKind::Name(n) if n == "tostring")
                                && args.len() == 1 =>
                        {
                            Some(boxed(&args[0]))
                        }
                        _ => None,
                    };
                    let (left, right) = (text(left), text(right));
                    let (Some(left), Some(right)) = (left, right) else {
                        return None;
                    };
                    Expression::Join { left, right }
                } else {
                    return None;
                }
            }
            ExprKind::Unary("not", inner) => {
                let value = self.operand(inner, "not", true, scope);
                if value.shape() != BlockShape::Boolean {
                    return None;
                }
                Expression::Not {
                    value: Box::new(value),
                }
            }
            ExprKind::Call(function, args) => return self.call_expression(function, args, scope),
            ExprKind::Method(object, method, args) => {
                if method == "Raycast"
                    && matches!(&object.kind, ExprKind::Name(name) if name == "workspace")
                {
                    match args.as_slice() {
                        [origin, direction] => {
                            return Some(Expression::Raycast {
                                origin: boxed(origin),
                                direction: boxed(direction),
                            });
                        }
                        [origin, direction, params] => {
                            return Some(Expression::RaycastIgnoring {
                                origin: boxed(origin),
                                direction: boxed(direction),
                                ignore: self.ignored(params)?,
                            });
                        }
                        _ => {}
                    }
                }
                if method == "IsA"
                    && let [
                        Expr {
                            kind: ExprKind::Str(class),
                            ..
                        },
                    ] = args.as_slice()
                {
                    return Some(Expression::IsA {
                        value: boxed(object),
                        class: class.clone(),
                    });
                }
                // workspace.Track:GetPositionAt(t) and GetClosestDistance(p)
                if let [arg] = args.as_slice()
                    && matches!(method.as_str(), "GetPositionAt" | "GetClosestDistance")
                    && let Some(spline) = self.target(object)
                {
                    return Some(if method == "GetPositionAt" {
                        Expression::SplinePosition {
                            spline,
                            t: boxed(arg),
                        }
                    } else {
                        Expression::SplineClosestDistance {
                            spline,
                            position: boxed(arg),
                        }
                    });
                }
                // v:Lerp(goal, alpha)
                if method == "Lerp"
                    && let [to, alpha] = args.as_slice()
                {
                    return Some(Expression::Lerp {
                        from: boxed(object),
                        to: boxed(to),
                        alpha: boxed(alpha),
                    });
                }
                // Random.new():NextNumber(min, max)
                if method == "NextNumber"
                    && let ExprKind::Call(function, none) = &object.kind
                    && none.is_empty()
                    && self.is_field(function, "Random", "new")
                    && let [min, max] = args.as_slice()
                {
                    return Some(Expression::RandomNumber {
                        min: boxed(min),
                        max: boxed(max),
                    });
                }
                // DataService:GetTable("weapons"):Get(key)
                if method == "Get"
                    && let [key] = args.as_slice()
                    && let Some(table) = self.data_table(object)
                {
                    return Some(Expression::DataRow {
                        table,
                        key: boxed(key),
                    });
                }
                if method == "IsKeyDown"
                    && self.is_service(object, "UserInputService")
                    && let [key] = args.as_slice()
                {
                    return Some(Expression::KeyDown {
                        key: self.key(key)?,
                    });
                }
                if let [arg] = args.as_slice()
                    && let ExprKind::Str(parameter) = &arg.kind
                {
                    let animator = self.target(object)?;
                    let parameter = parameter.clone();
                    return match method.as_str() {
                        "GetBool" => Some(Expression::AnimatorBool {
                            animator,
                            parameter,
                        }),
                        "GetNumber" => Some(Expression::AnimatorNumber {
                            animator,
                            parameter,
                        }),
                        "GetLayerWeight" => Some(Expression::AnimatorLayerWeight {
                            animator,
                            layer: parameter,
                        }),
                        _ => None,
                    };
                }
                return None;
            }
            // game.Players.LocalPlayer:GetMouse().Target
            ExprKind::Field(object, property)
                if property == "Target"
                    && matches!(&object.kind, ExprKind::Method(player, method, none)
                        if method == "GetMouse" && none.is_empty() && is_local_player(player)) =>
            {
                Expression::PartUnderMouse
            }
            // A data row's column: `DataService:GetTable("t"):Get(k).Damage`.
            ExprKind::Field(row, column) if self.is_data_row(row) => Expression::DataValue {
                column: column.clone(),
                row: boxed(row),
            },
            ExprKind::Index(row, column)
                if self.is_data_row(row)
                    && let ExprKind::Str(column) = &column.kind =>
            {
                Expression::DataValue {
                    column: column.clone(),
                    row: boxed(row),
                }
            }
            ExprKind::Field(object, property) => Expression::Property {
                target: self.target(object)?,
                property: property.clone(),
            },
            _ => return None,
        })
    }

    fn call_expression(&self, function: &Expr, args: &[Expr], scope: Scope) -> Option<Expression> {
        let numbers = || -> Option<[f64; 3]> {
            let [a, b, c] = args else {
                return None;
            };
            Some([self.number(a)?, self.number(b)?, self.number(c)?])
        };
        if self.is_field(function, "Vector3", "new") {
            let [x, y, z] = numbers()?;
            return Some(Expression::Vector3 { x, y, z });
        }
        if self.is_field(function, "Color3", "new") {
            let [r, g, b] = numbers()?;
            return Some(Expression::Color3 { r, g, b });
        }
        let ExprKind::Field(object, name) = &function.kind else {
            return None;
        };
        if !matches!(&object.kind, ExprKind::Name(math) if math == "math") {
            return None;
        }
        let one = |function: MathFunction| -> Option<Expression> {
            let [value] = args else {
                return None;
            };
            Some(Expression::Function {
                function,
                value: Box::new(self.expression(value, scope)),
            })
        };
        match name.as_str() {
            "abs" => one(MathFunction::Abs),
            "ceil" => one(MathFunction::Ceil),
            "sqrt" => one(MathFunction::Sqrt),
            "floor" => {
                // math.floor(x + 0.5) is rounding.
                if let [value] = args
                    && let ExprKind::Binary("+", inner, half) = &value.kind
                    && self.number(half) == Some(0.5)
                {
                    return Some(Expression::Function {
                        function: MathFunction::Round,
                        value: Box::new(self.expression(inner, scope)),
                    });
                }
                one(MathFunction::Floor)
            }
            "random" => {
                let [min, max] = args else {
                    return None;
                };
                // math.floor(a), which the translation adds back.
                let unfloored = |expr: &Expr| match &expr.kind {
                    ExprKind::Call(function, inner)
                        if self.is_field(function, "math", "floor") && inner.len() == 1 =>
                    {
                        Box::new(self.expression(&inner[0], scope))
                    }
                    _ => Box::new(self.expression(expr, scope)),
                };
                Some(Expression::Random {
                    min: unfloored(min),
                    max: unfloored(max),
                })
            }
            _ => None,
        }
    }
}

/// The value of `local <wanted> = value`.
fn local<'s>(stat: &'s Stat, wanted: &str) -> Option<&'s Expr> {
    match &stat.kind {
        StatKind::Local { names, values } => match (names.as_slice(), values.as_slice()) {
            ([name], [value]) if name == wanted => Some(value),
            _ => None,
        },
        _ => None,
    }
}

const UNARY: u8 = 12;

/// `a`, or `a.b.c`: a name and any fields of it, as one dotted path.
fn path(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Name(name) => Some(name.clone()),
        ExprKind::Field(object, name) => Some(format!("{}.{name}", path(object)?)),
        _ => None,
    }
}

/// Whether any block in `blocks` uses the name `name`.
fn code_mentions(blocks: &[Block], name: &str) -> bool {
    fn expression_mentions(expression: &Expression, name: &str) -> bool {
        match expression {
            Expression::Code { code } => lua::mentions(code, name),
            Expression::Name { name: path } if path.split('.').next() == Some(name) => true,
            _ => expression
                .children()
                .into_iter()
                .any(|child| expression_mentions(child, name)),
        }
    }
    blocks.iter().any(|block| {
        let here = match &block.kind {
            BlockKind::Code { code } => lua::mentions(code, name),
            kind => kind
                .expressions()
                .into_iter()
                .any(|expression| expression_mentions(expression, name)),
        };
        here || block
            .kind
            .bodies()
            .into_iter()
            .any(|body| code_mentions(body, name))
    })
}

/// `game.Players.LocalPlayer`.
fn is_local_player(expr: &Expr) -> bool {
    matches!(&expr.kind, ExprKind::Field(players, local)
        if local == "LocalPlayer"
            && matches!(&players.kind, ExprKind::Field(game, name)
                if name == "Players" && matches!(&game.kind, ExprKind::Name(g) if g == "game")))
}
