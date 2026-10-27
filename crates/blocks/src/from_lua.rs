//! Lua back to blocks, for switching a script from Code to Blocks.
//!
//! Code shaped like what [`crate::translate`] writes becomes the matching
//! blocks, so switching to Code and back gives the same blocks. Anything
//! else is kept, word for word, in grey Lua blocks ([`BlockKind::Code`],
//! [`BlockKind::TopCode`] and [`Expression::Code`]), as MakeCode does with
//! JavaScript it has no blocks for. So nothing is lost either way, and
//! the blocks still run the same code.

use std::collections::BTreeSet;

use crate::lua::{self, Expr, ExprKind, FunctionBody, Stat, StatKind, SyntaxError};
use crate::{
    Block, BlockKind, BlockScript, BlockShape, CAMERA_TYPES, Comparison, EASING_DIRECTIONS,
    EASING_STYLES, Expression, GRAPHICS_NUMBERS, GRAPHICS_OPTIONS, GRAPHICS_QUALITIES,
    GRAPHICS_SWITCHES, LogicOp, MathFunction, MathOp, PRESET, TONE_MAPPINGS, graphics_choices,
    translate,
};

/// The services blocks get with `game:GetService`, which the translation
/// declares at the top itself when blocks need them.
const SERVICES: [&str; 9] = [
    "UserInputService",
    "RunService",
    "Debris",
    "TweenService",
    "SaveService",
    "LocalizationService",
    "Lighting",
    "UserGameSettings",
    "DataService",
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

    // Top-level `local name = 0` lines are the blocks' variables, and
    // `local RunService = game:GetService("RunService")` their services:
    // the translation writes those itself, but only for the ones blocks
    // use, so a second pass keeps the declarations of the rest as code.
    let mut variables = BTreeSet::new();
    let mut services = BTreeSet::new();
    for stat in &chunk {
        if let Some(name) = variable_declaration(stat) {
            variables.insert(name);
        }
        if let Some(name) = service_declaration(stat) {
            services.insert(name);
        }
    }
    let first = Converter::new(source, variables, services).chunk(&chunk);
    let (used_variables, used_services) = used_by(&first);
    let converter = Converter::new(source, used_variables, used_services);
    let script = converter.chunk(&chunk);
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

fn variable_declaration(stat: &Stat) -> Option<String> {
    match &stat.kind {
        StatKind::Local { names, values }
            if names.len() == 1
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
            BlockKind::SetLanguage { .. } => {
                services.insert("LocalizationService".to_owned());
            }
            BlockKind::SetToneMapping { .. } => {
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

    fn chunk(&self, chunk: &[Stat]) -> BlockScript {
        /// A top-level statement, sorted.
        enum Item<'s> {
            Event(Block),
            /// A plain statement that is a block, for "when game starts".
            Loose(Block, &'s Stat),
            /// Lua with no block: it goes in a grey block, word for word.
            Code(&'s Stat),
            /// A declaration the translation writes itself. Grey blocks
            /// don't reach over it, or it would be declared twice.
            Declared,
        }
        let mut items = Vec::new();
        for stat in chunk {
            let declared = variable_declaration(stat)
                .is_some_and(|name| self.variables.contains(&name))
                || service_declaration(stat).is_some_and(|name| self.services.contains(&name));
            items.push(if declared {
                Item::Declared
            } else if matches!(stat.kind, StatKind::Comment) {
                Item::Code(stat)
            } else if let Some(event) = self.event(stat) {
                Item::Event(event)
            } else if let Some(block) = self.top_statement(stat) {
                Item::Loose(block, stat)
            } else {
                Item::Code(stat)
            });
        }

        // Plain statements run when the script starts, before what comes
        // after them. Blocks in "when game starts" run alongside the rest,
        // so they only go in there if they can't wait or stop the script,
        // or nothing comes after them. Otherwise they stay as Lua.
        let mut index = 0;
        while index < items.len() {
            if !matches!(items[index], Item::Loose(..)) {
                index += 1;
                continue;
            }
            let end = (index..items.len())
                .find(|&at| !matches!(items[at], Item::Loose(..)))
                .unwrap_or(items.len());
            let at_end = items[end..].iter().all(|item| match item {
                Item::Code(stat) => matches!(stat.kind, StatKind::Comment),
                Item::Declared => true,
                _ => false,
            });
            let waits = items[index..end]
                .iter()
                .any(|item| matches!(item, Item::Loose(block, _) if may_wait(block)));
            if waits && !at_end {
                for item in &mut items[index..end] {
                    if let Item::Loose(_, stat) = item {
                        *item = Item::Code(stat);
                    }
                }
            }
            index = end;
        }

        let mut blocks = Vec::new();
        let mut code: Option<std::ops::Range<usize>> = None;
        let mut started: Vec<Block> = Vec::new();
        let flush = |code: &mut Option<std::ops::Range<usize>>,
                     started: &mut Vec<Block>,
                     blocks: &mut Vec<Block>| {
            if let Some(span) = code.take() {
                blocks.push(Block::new(BlockKind::TopCode {
                    code: self.text(span),
                }));
            }
            if !started.is_empty() {
                let body = std::mem::take(started);
                blocks.push(Block::new(BlockKind::WhenStarted { body }));
            }
        };
        for item in items {
            match item {
                Item::Code(stat) => {
                    if !started.is_empty() {
                        flush(&mut code, &mut started, &mut blocks);
                    }
                    code = Some(match code {
                        Some(span) => span.start..stat.span.end,
                        None => stat.span.clone(),
                    });
                }
                Item::Loose(block, _) => {
                    if code.is_some() {
                        flush(&mut code, &mut started, &mut blocks);
                    }
                    started.push(block);
                }
                Item::Event(block) => {
                    flush(&mut code, &mut started, &mut blocks);
                    blocks.push(block);
                }
                Item::Declared => flush(&mut code, &mut started, &mut blocks),
            }
        }
        flush(&mut code, &mut started, &mut blocks);
        BlockScript {
            blocks,
            ..Default::default()
        }
    }

    /// A plain statement at the top level, if it becomes a real block (not
    /// a grey one), and doesn't declare anything the rest of the script
    /// could need.
    fn top_statement(&self, stat: &Stat) -> Option<Block> {
        if matches!(stat.kind, StatKind::Comment | StatKind::Local { .. }) {
            return None;
        }
        let block = self.statement(stat, Scope::default());
        (!matches!(block.kind, BlockKind::Code { .. })).then_some(block)
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
        if method != "Connect" {
            return None;
        }
        let [handler] = args.as_slice() else {
            return None;
        };
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

    /// The blocks for a body. Lua with no blocks for it, side by side,
    /// goes into one grey block.
    fn body(&self, stats: &[Stat], scope: Scope) -> Vec<Block> {
        let mut blocks: Vec<Block> = Vec::new();
        let mut code: Option<std::ops::Range<usize>> = None;
        for stat in stats {
            let block = self.statement(stat, scope);
            if matches!(block.kind, BlockKind::Code { .. }) {
                code = Some(match code {
                    Some(span) => span.start..stat.span.end,
                    None => stat.span.clone(),
                });
                continue;
            }
            if let Some(span) = code.take() {
                blocks.push(self.code_block(span));
            }
            blocks.push(block);
        }
        if let Some(span) = code {
            blocks.push(self.code_block(span));
        }
        blocks
    }

    fn code_block(&self, span: std::ops::Range<usize>) -> Block {
        Block::new(BlockKind::Code {
            code: self.text(span),
        })
    }

    fn statement(&self, stat: &Stat, scope: Scope) -> Block {
        self.try_statement(stat, scope)
            .map(Block::new)
            .unwrap_or_else(|| self.code_block(stat.span.clone()))
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
                if skip != "_" || method != "GetChildren" || !args.is_empty() {
                    return None;
                }
                let inner = Scope {
                    child: Some(variable.as_str()),
                    ..scope
                };
                Some(BlockKind::ForEachChild {
                    target: self.container(object)?,
                    variable: variable.clone(),
                    body: self.body(body, inner),
                })
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
                if matches!(args.first().map(|arg| &arg.kind), Some(ExprKind::Str(_))) =>
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
            // workspace.Ball:ApplyImpulse(Vector3.new(0, 50, 0))
            ExprKind::Method(object, method, args)
                if method == "ApplyImpulse" && args.len() == 1 =>
            {
                Some(BlockKind::PushWithForce {
                    target: self.target(object)?,
                    force: self.expression(&args[0], scope),
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
        let ExprKind::Table {
            named,
            other: false,
        } = &goals.kind
        else {
            return None;
        };
        let [(property, value)] = named.as_slice() else {
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
        // elseif: an if inside the else.
        let inner = self.if_chain(rest, otherwise, scope)?;
        Some(BlockKind::IfElse {
            condition,
            then_body,
            else_body: vec![Block::new(inner)],
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
            ExprKind::Binary(..) | ExprKind::Unary(..) | ExprKind::IfElse | ExprKind::Cast => {
                format!("({text})")
            }
            _ => text,
        };
        Expression::Code { code }
    }

    fn expression(&self, expr: &Expr, scope: Scope) -> Expression {
        self.try_expression(expr, scope)
            .unwrap_or_else(|| self.code_expression(expr))
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
            return Some(Expression::Number { value });
        }
        Some(match &expr.kind {
            ExprKind::Str(value) => Expression::Text {
                value: value.clone(),
            },
            ExprKind::True => Expression::Boolean { value: true },
            ExprKind::False => Expression::Boolean { value: false },
            ExprKind::Paren(inner) => return self.try_expression(inner, scope),
            ExprKind::Name(name) if scope.touched == Some(name.as_str()) => {
                Expression::TouchingPart
            }
            ExprKind::Name(name)
                if self.variables.contains(name) || scope.child == Some(name.as_str()) =>
            {
                Expression::Variable { name: name.clone() }
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
                if let Some(op) = math {
                    Expression::Math {
                        op,
                        left: boxed(left),
                        right: boxed(right),
                    }
                } else if let Some(op) = compare {
                    Expression::Compare {
                        op,
                        left: boxed(left),
                        right: boxed(right),
                    }
                } else if *op == "and" || *op == "or" {
                    let op = if *op == "and" {
                        LogicOp::And
                    } else {
                        LogicOp::Or
                    };
                    Expression::Logic {
                        op,
                        left: Box::new(self.condition(left, scope)),
                        right: Box::new(self.condition(right, scope)),
                    }
                } else if *op == ".." {
                    // tostring(a) .. tostring(b), which the translation writes.
                    let text = |expr: &Expr| match &expr.kind {
                        ExprKind::Call(function, args)
                            if matches!(&function.kind, ExprKind::Name(n) if n == "tostring")
                                && args.len() == 1 =>
                        {
                            boxed(&args[0])
                        }
                        _ => boxed(expr),
                    };
                    Expression::Join {
                        left: text(left),
                        right: text(right),
                    }
                } else {
                    return None;
                }
            }
            ExprKind::Unary("not", inner) => Expression::Not {
                value: Box::new(self.condition(inner, scope)),
            },
            ExprKind::Call(function, args) => return self.call_expression(function, args, scope),
            ExprKind::Method(object, method, args) => {
                if method == "Raycast"
                    && matches!(&object.kind, ExprKind::Name(name) if name == "workspace")
                    && let [origin, direction] = args.as_slice()
                {
                    return Some(Expression::Raycast {
                        origin: boxed(origin),
                        direction: boxed(direction),
                    });
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

/// Whether a block here can wait, or stop the script: then it can't be
/// moved into "when game starts" without changing when things happen.
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

fn may_wait(block: &Block) -> bool {
    let waits = matches!(
        block.kind,
        BlockKind::Wait { .. }
            | BlockKind::WaitUntil { .. }
            | BlockKind::WaitForChild { .. }
            | BlockKind::Forever { .. }
            | BlockKind::RepeatUntil { .. }
            | BlockKind::StopScript
            | BlockKind::Code { .. }
    ) || block
        .kind
        .expressions()
        .into_iter()
        .any(has_code_expression);
    waits
        || block
            .kind
            .bodies()
            .into_iter()
            .any(|body| body.iter().any(may_wait))
}

fn has_code_expression(expression: &Expression) -> bool {
    matches!(expression, Expression::Code { .. })
        || expression.children().into_iter().any(has_code_expression)
}

/// Whether any grey block in `blocks` uses `name`.
fn code_mentions(blocks: &[Block], name: &str) -> bool {
    fn expression_mentions(expression: &Expression, name: &str) -> bool {
        match expression {
            Expression::Code { code } => lua::mentions(code, name),
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
