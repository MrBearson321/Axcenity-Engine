use super::*;

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
    #[error("`{value}` isn't {what}; choose from {choices}")]
    InvalidChoice {
        what: &'static str,
        value: String,
        choices: String,
    },
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
            kind if kind.tweens() => {
                services.insert("TweenService");
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

pub(super) fn note_ids(block: &Block, ids: &mut HashSet<u64>) -> Result<(), TranslateError> {
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

pub(super) fn visit_expressions(expression: &Expression, visit: &mut impl FnMut(&Expression)) {
    visit(expression);
    for child in expression.children() {
        visit_expressions(child, visit);
    }
}

#[derive(Default)]
pub(super) struct Writer {
    pub(super) lines: Vec<String>,
    pub(super) block_lines: Vec<(usize, u64)>,
    pub(super) indent: usize,
    /// True inside "when touched", where `otherPart` exists.
    pub(super) in_touched: bool,
}

impl Writer {
    pub(super) fn line(&mut self, text: impl Into<String>) {
        self.lines
            .push(format!("{}{}", "    ".repeat(self.indent), text.into()));
    }

    /// Notes that `id`'s code starts on the next line.
    pub(super) fn tag(&mut self, id: u64, ids: &mut HashSet<u64>) -> Result<(), TranslateError> {
        if !ids.insert(id) {
            return Err(TranslateError::DuplicateId(id));
        }
        self.block_lines.push((self.lines.len() + 1, id));
        Ok(())
    }

    pub(super) fn write_event(
        &mut self,
        block: &Block,
        ids: &mut HashSet<u64>,
    ) -> Result<(), TranslateError> {
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
            BlockKind::WhenMouseEnters { element, body } => (
                format!("{}.MouseEnter:Connect(function()", target(element)?),
                body,
            ),
            BlockKind::WhenMouseLeaves { element, body } => (
                format!("{}.MouseLeave:Connect(function()", target(element)?),
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
    pub(super) fn code(&mut self, code: &str) {
        for line in code.lines() {
            if line.trim().is_empty() {
                self.lines.push(String::new());
            } else {
                self.line(line);
            }
        }
    }

    pub(super) fn nested(
        &mut self,
        body: &[Block],
        ids: &mut HashSet<u64>,
    ) -> Result<(), TranslateError> {
        self.indent += 1;
        self.write_body(body, ids)?;
        self.indent -= 1;
        Ok(())
    }

    pub(super) fn write_body(
        &mut self,
        body: &[Block],
        ids: &mut HashSet<u64>,
    ) -> Result<(), TranslateError> {
        for block in body {
            if block.kind.shape() != BlockShape::Statement {
                return Err(TranslateError::InvalidEventPlacement);
            }
            self.tag(block.id, ids)?;
            self.write_statement(&block.kind, ids)?;
        }
        Ok(())
    }

    pub(super) fn write_statement(
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
            BlockKind::ScrollTo { target: t, x, y } => {
                let (x, y) = (self.expression(x)?, self.expression(y)?);
                self.line(format!(
                    "{}.CanvasPosition = Vector2.new({x}, {y})",
                    target(t)?
                ));
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
            BlockKind::Tween {
                target: t,
                property,
                value,
                seconds,
                style,
                direction,
            } => {
                if !valid_identifier(property) {
                    return Err(TranslateError::InvalidProperty(property.clone()));
                }
                let style = choice(style, &EASING_STYLES, "an easing style")?;
                let direction = choice(direction, &EASING_DIRECTIONS, "an easing direction")?;
                let (object, value) = (target(t)?, self.expression(value)?);
                let seconds = self.expression(seconds)?;
                self.line(format!(
                    "TweenService:Create({object}, TweenInfo.new({seconds}, \
                     Enum.EasingStyle.{style}, Enum.EasingDirection.{direction}), \
                     {{{property} = {value}}}):Play()"
                ));
            }
            // The info first, so `seconds` can't see the `goal` local.
            BlockKind::SlideIn {
                target: t,
                from,
                seconds,
            } => {
                let offset = match choice(from, &SLIDE_EDGES, "an edge")? {
                    "Left" => "goal - UDim2.fromScale(1, 0)",
                    "Right" => "goal + UDim2.fromScale(1, 0)",
                    "Top" => "goal - UDim2.fromScale(0, 1)",
                    _ => "goal + UDim2.fromScale(0, 1)",
                };
                let (object, seconds) = (target(t)?, self.expression(seconds)?);
                self.line("do");
                self.indent += 1;
                self.line(format!("local info = TweenInfo.new({seconds})"));
                self.line(format!("local goal = {object}.Position"));
                self.line(format!("{object}.Position = {offset}"));
                self.line(format!(
                    "TweenService:Create({object}, info, {{Position = goal}}):Play()"
                ));
                self.indent -= 1;
                self.line("end");
            }
            BlockKind::FadeIn { target: t, seconds }
            | BlockKind::FadeOut { target: t, seconds } => {
                let transparency = u8::from(matches!(kind, BlockKind::FadeOut { .. }));
                let (object, seconds) = (target(t)?, self.expression(seconds)?);
                self.line(format!(
                    "TweenService:Create({object}, TweenInfo.new({seconds}), \
                     {{BackgroundTransparency = {transparency}}}):Play()"
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
            | BlockKind::WhenMouseEnters { .. }
            | BlockKind::WhenMouseLeaves { .. }
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

    pub(super) fn expression(&self, expression: &Expression) -> Result<String, TranslateError> {
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
            Expression::Raycast { origin, direction } => {
                format!(
                    "workspace:Raycast({}, {})",
                    self.expression(origin)?,
                    self.expression(direction)?
                )
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
pub(super) fn number(value: f64) -> String {
    value.to_string()
}

/// `value` if it's one of `choices`, which are `what`.
fn choice<'a>(
    value: &str,
    choices: &[&'a str],
    what: &'static str,
) -> Result<&'a str, TranslateError> {
    choices
        .iter()
        .copied()
        .find(|choice| *choice == value)
        .ok_or_else(|| TranslateError::InvalidChoice {
            what,
            value: value.to_owned(),
            choices: choices.join(", "),
        })
}

/// `Enum.KeyCode.<name>`, for a key's name such as `Space`.
pub(super) fn key(name: &str) -> Result<String, TranslateError> {
    if !valid_identifier(name) {
        return Err(TranslateError::InvalidKey(name.to_owned()));
    }
    Ok(format!("Enum.KeyCode.{name}"))
}

/// The Lua for an object at a dotted path under the Workspace, such as
/// `workspace.Pad.Chime`. Names that aren't Lua names are bracketed:
/// `workspace["Red Ball"]`.
pub(super) fn target(path: &str) -> Result<String, TranslateError> {
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

pub(super) fn quote(text: &str) -> String {
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
