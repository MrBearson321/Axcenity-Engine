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
            BlockKind::EveryFrame { .. }
            | BlockKind::BeforePhysics { .. }
            | BlockKind::BeforeRender { .. } => {
                services.insert("RunService");
            }
            BlockKind::DestroyAfter { .. } => {
                services.insert("Debris");
            }
            kind if kind.tweens() => {
                services.insert("TweenService");
            }
            kind if kind.saves() => {
                services.insert("SaveService");
            }
            BlockKind::SetLanguage { .. } => {
                services.insert("LocalizationService");
            }
            BlockKind::SetToneMapping { .. } => {
                services.insert("Lighting");
            }
            kind if kind.sets_graphics() => {
                services.insert("UserGameSettings");
            }
            _ => {}
        }
        for expression in block.kind.expressions() {
            visit_expressions(expression, &mut |expression| match expression {
                Expression::KeyDown { .. } => {
                    services.insert("UserInputService");
                }
                Expression::DataRow { .. } => {
                    services.insert("DataService");
                }
                _ => {}
            });
        }
    });
    read_variables(&script.blocks, &mut Vec::new(), &mut variables);
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

/// Adds the variables `blocks` read, except a "for each child" loop's own
/// variable inside that loop (the loop declares it).
fn read_variables(blocks: &[Block], loops: &mut Vec<String>, variables: &mut BTreeSet<String>) {
    for block in blocks {
        for expression in block.kind.expressions() {
            visit_expressions(expression, &mut |expression| {
                if let Expression::Variable { name } = expression
                    && !loops.contains(name)
                {
                    variables.insert(name.clone());
                }
            });
        }
        let looping = match &block.kind {
            BlockKind::ForEachChild { variable, .. } => Some(variable.clone()),
            _ => None,
        };
        let pushed = looping.is_some();
        loops.extend(looping);
        for body in block.kind.bodies() {
            read_variables(body, loops, variables);
        }
        if pushed {
            loops.pop();
        }
    }
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
            BlockKind::WhenValueChanged { widget, body } => (
                format!("{}.ValueChanged:Connect(function()", target(widget)?),
                body,
            ),
            BlockKind::EveryFrame { body } => (
                "RunService.Heartbeat:Connect(function(deltaTime)".to_owned(),
                body,
            ),
            BlockKind::BeforePhysics { body } => (
                "RunService.Stepped:Connect(function(time, deltaTime)".to_owned(),
                body,
            ),
            BlockKind::BeforeRender { body } => (
                "RunService.RenderStepped:Connect(function(deltaTime)".to_owned(),
                body,
            ),
            BlockKind::WhenEventFired { event, body } => {
                (format!("{}.Event:Connect(function()", stored(event)?), body)
            }
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
            BlockKind::WhenAgentSenses { agent, sense, body } => {
                self.line(format!(
                    "{}.PlayerSensed:Connect(function(sense)",
                    target(agent)?
                ));
                self.indent += 1;
                self.line(format!("if sense == {} then", quote(sense)));
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
            BlockKind::ForEachChild {
                target: t,
                variable,
                body,
            } => {
                if !valid_identifier(variable) {
                    return Err(TranslateError::InvalidVariable(variable.clone()));
                }
                let object = container(t)?;
                self.line(format!("for _, {variable} in {object}:GetChildren() do"));
                self.nested(body, ids)?;
                self.line("end");
            }
            BlockKind::WaitForChild { target: t, name } => {
                self.line(format!("{}:WaitForChild({})", container(t)?, quote(name)));
            }
            BlockKind::DestroyAfter { target: t, seconds } => {
                let seconds = self.expression(seconds)?;
                self.line(format!("Debris:AddItem({}, {seconds})", target(t)?));
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
            BlockKind::LookAt {
                target: t,
                position,
            } => {
                let (object, value) = (target(t)?, self.expression(position)?);
                self.line(format!(
                    "{object}.CFrame = CFrame.lookAt({object}.Position, {value})"
                ));
            }
            BlockKind::MoveForward {
                target: t,
                distance,
            } => {
                let (object, value) = (target(t)?, self.operand(distance)?);
                self.line(format!(
                    "{object}.Position = {object}.Position + {object}.CFrame.LookVector * {value}"
                ));
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
            BlockKind::PushWithForce { target: t, force } => {
                let value = self.expression(force)?;
                self.line(format!("{}:ApplyImpulse({value})", target(t)?));
            }
            BlockKind::SetPartVelocity {
                target: t,
                velocity,
            } => {
                let value = self.expression(velocity)?;
                self.line(format!("{}.AssemblyLinearVelocity = {value}", target(t)?));
            }
            // The friction first, so it can't see the `physical` local.
            BlockKind::SetFriction {
                target: t,
                friction,
            } => {
                let (object, friction) = (target(t)?, self.expression(friction)?);
                self.line("do");
                self.indent += 1;
                self.line(format!("local friction = {friction}"));
                self.line(format!(
                    "local physical = {object}.CurrentPhysicalProperties"
                ));
                self.line(format!(
                    "{object}.CustomPhysicalProperties = PhysicalProperties.new(physical.Density, \
                     friction, physical.Elasticity, physical.FrictionWeight, \
                     physical.ElasticityWeight)"
                ));
                self.indent -= 1;
                self.line("end");
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
            BlockKind::SetValue { target: t, value } => {
                let value = self.expression(value)?;
                self.line(format!("{}.Value = {value}", target(t)?));
            }
            BlockKind::SetDestination { agent, position } => {
                let value = self.expression(position)?;
                self.line(format!("{}.Destination = {value}", target(agent)?));
            }
            BlockKind::SetVelocity { agent, velocity } => {
                let value = self.expression(velocity)?;
                self.line(format!("{}.Velocity = {value}", target(agent)?));
            }
            BlockKind::SaveSlot { slot } => {
                let slot = self.expression(slot)?;
                self.line(format!("SaveService:SaveSlot({slot})"));
            }
            BlockKind::LoadSlot { slot } => {
                let slot = self.expression(slot)?;
                self.line(format!("SaveService:LoadSlot({slot})"));
            }
            BlockKind::SetCheckpoint { checkpoint } => {
                self.line(format!(
                    "SaveService:SetCheckpoint({})",
                    target(checkpoint)?
                ));
            }
            BlockKind::RestoreCheckpoint => self.line("SaveService:RestoreCheckpoint()".to_owned()),
            BlockKind::SetLanguage { language } => {
                let language = self.expression(language)?;
                self.line(format!("LocalizationService.Language = {language}"));
            }
            BlockKind::SetCameraType { camera_type } => {
                let camera_type = choice(camera_type, &CAMERA_TYPES, "a camera type")?;
                self.line(format!(
                    "workspace.CurrentCamera.CameraType = Enum.CameraType.{camera_type}"
                ));
            }
            BlockKind::PointCameraAt { position } => {
                let value = self.expression(position)?;
                self.line(format!(
                    "workspace.CurrentCamera.CFrame = \
                     CFrame.lookAt(workspace.CurrentCamera.Position, {value})"
                ));
            }
            BlockKind::ShakeCamera { intensity, seconds } => {
                let (intensity, seconds) = (self.expression(intensity)?, self.expression(seconds)?);
                self.line(format!(
                    "workspace.CurrentCamera:Shake({intensity}, {seconds})"
                ));
            }
            BlockKind::SetToneMapping { curve } => {
                let curve = choice(curve, &TONE_MAPPINGS, "a tone mapping")?;
                self.line(format!("Lighting.ToneMapping = Enum.ToneMapping.{curve}"));
            }
            BlockKind::SetGraphicsQuality { quality } => {
                let quality = choice(quality, &GRAPHICS_QUALITIES, "a graphics quality")?;
                self.line(format!(
                    "UserGameSettings.GraphicsQuality = Enum.GraphicsQuality.{quality}"
                ));
            }
            BlockKind::SetGraphicsOption {
                setting,
                choice: chosen,
            } => {
                let setting = choice(setting, &GRAPHICS_OPTIONS, "a graphics option")?;
                let chosen = choice(chosen, graphics_choices(setting), "a choice for it")?;
                let value = match (setting, chosen) {
                    (_, PRESET) => "nil".to_owned(),
                    (setting, on) if GRAPHICS_SWITCHES.contains(&setting) => {
                        (on == "On").to_string()
                    }
                    (setting, item) => format!("Enum.{setting}.{item}"),
                };
                self.line(format!("UserGameSettings.{setting} = {value}"));
            }
            BlockKind::SetGraphicsNumber { setting, value } => {
                let setting = choice(setting, &GRAPHICS_NUMBERS, "a graphics number")?;
                let value = self.expression(value)?;
                self.line(format!("UserGameSettings.{setting} = {value}"));
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
            BlockKind::FireEvent { event } => self.line(format!("{}:Fire()", stored(event)?)),
            BlockKind::CloneInto { target: t, parent } => self.line(format!(
                "{}:Clone().Parent = {}",
                target(t)?,
                container(parent)?
            )),
            BlockKind::Code { code } => self.code(code),
            BlockKind::WhenStarted { .. }
            | BlockKind::WhenTouched { .. }
            | BlockKind::WhenKeyPressed { .. }
            | BlockKind::WhenClicked { .. }
            | BlockKind::WhenMouseEnters { .. }
            | BlockKind::WhenMouseLeaves { .. }
            | BlockKind::WhenValueChanged { .. }
            | BlockKind::EveryFrame { .. }
            | BlockKind::BeforePhysics { .. }
            | BlockKind::BeforeRender { .. }
            | BlockKind::WhenEventFired { .. }
            | BlockKind::WhenSoundEnded { .. }
            | BlockKind::WhenStateChanged { .. }
            | BlockKind::WhenAnimationEvent { .. }
            | BlockKind::WhenAgentSenses { .. }
            | BlockKind::Unattached { .. }
            | BlockKind::TopCode { .. } => {
                return Err(TranslateError::InvalidEventPlacement);
            }
        }
        Ok(())
    }

    /// An expression as one side of an operator. Reporters inside other
    /// reporters are bracketed, so `(a + b) * c` means what the blocks show.
    fn operand(&self, expression: &Expression) -> Result<String, TranslateError> {
        let text = self.expression(expression)?;
        Ok(match expression {
            Expression::Math { .. }
            | Expression::Join { .. }
            | Expression::Compare { .. }
            | Expression::Logic { .. }
            | Expression::Not { .. } => format!("({text})"),
            _ => text,
        })
    }

    /// An expression before a `:` method call, which needs a name or a call
    /// there, so anything else is bracketed: `(a or b):IsA("Part")`.
    fn receiver(&self, expression: &Expression) -> Result<String, TranslateError> {
        let text = self.expression(expression)?;
        Ok(match expression {
            Expression::Variable { .. }
            | Expression::TouchingPart
            | Expression::PartUnderMouse
            | Expression::Property { .. }
            | Expression::Raycast { .. }
            | Expression::Vector3 { .. }
            | Expression::Color3 { .. }
            | Expression::Lerp { .. }
            | Expression::DataRow { .. }
            | Expression::DataValue { .. } => text,
            _ => format!("({text})"),
        })
    }

    pub(super) fn expression(&self, expression: &Expression) -> Result<String, TranslateError> {
        let inner = |expression: &Expression| self.operand(expression);
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
            Expression::RandomNumber { min, max } => {
                let (min, max) = (self.expression(min)?, self.expression(max)?);
                format!("Random.new():NextNumber({min}, {max})")
            }
            Expression::Lerp { from, to, alpha } => format!(
                "{}:Lerp({}, {})",
                self.receiver(from)?,
                self.expression(to)?,
                self.expression(alpha)?
            ),
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
            Expression::PartUnderMouse => "game.Players.LocalPlayer:GetMouse().Target".to_owned(),
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
            Expression::IsA { value, class } => {
                format!("{}:IsA({})", self.receiver(value)?, quote(class))
            }
            Expression::DataRow { table, key } => format!(
                "DataService:GetTable({}):Get({})",
                quote(table),
                self.expression(key)?
            ),
            // A column whose name isn't a Lua name (`Fire Rate`) is indexed.
            Expression::DataValue { column, row } if valid_identifier(column) => {
                format!("{}.{column}", self.receiver(row)?)
            }
            Expression::DataValue { column, row } => {
                format!("{}[{}]", self.receiver(row)?, quote(column))
            }
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

/// An object in ReplicatedStorage, such as a BindableEvent, by its dotted
/// path there: `Events.Scored` is `game.ReplicatedStorage.Events.Scored`.
pub(super) fn stored(path: &str) -> Result<String, TranslateError> {
    Ok(target(path)?.replacen("workspace", "game.ReplicatedStorage", 1))
}

/// Like [`target`], with an empty path meaning the Workspace itself.
pub(super) fn container(path: &str) -> Result<String, TranslateError> {
    if path.is_empty() {
        Ok("workspace".to_owned())
    } else {
        target(path)
    }
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

#[cfg(test)]
mod tests {
    #[test]
    fn quote_escapes_quote_marks() {
        assert_eq!(super::quote("sound\"path"), "\"sound\\\"path\"");
    }
}
