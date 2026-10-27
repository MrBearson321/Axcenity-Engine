use super::*;

/// A block or reporter in the palette.
#[derive(Clone, Debug)]
pub(super) enum PaletteItem {
    Block(BlockKind),
    Reporter(Expression),
    /// The "Make a Variable" button.
    NewVariable,
}

/// The palette's blocks for `category`. Object menus start on the first
/// object of their kind, so a new block usually works as it is.
pub(super) fn palette(
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
            B(BlockKind::WhenMouseEnters {
                element: first(Filter::Gui),
                body: Vec::new(),
            }),
            B(BlockKind::WhenMouseLeaves {
                element: first(Filter::Gui),
                body: Vec::new(),
            }),
            B(BlockKind::WhenValueChanged {
                widget: first(Filter::Widgets),
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
            B(BlockKind::SetDestination {
                agent: first(Filter::Agents),
                position: vector(10.0, 0.0, 0.0),
            }),
            B(BlockKind::SetVelocity {
                agent: first(Filter::Agents),
                velocity: vector(0.0, 0.0, 0.0),
            }),
            B(BlockKind::WhenAgentSenses {
                agent: first(Filter::Agents),
                sense: axcenity_blocks::SENSES[0].to_owned(),
                body: Vec::new(),
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
            B(BlockKind::ScrollTo {
                target: first(Filter::ScrollingFrames),
                x: number(0.0),
                y: number(0.0),
            }),
            B(BlockKind::SetValue {
                target: first(Filter::Widgets),
                value: number(50.0),
            }),
            B(BlockKind::SetProperty {
                target: part.clone(),
                property: "Size".to_owned(),
                value: vector(4.0, 1.0, 2.0),
            }),
            B(BlockKind::SetToneMapping {
                curve: "Neutral".to_owned(),
            }),
            B(BlockKind::SetGraphicsQuality {
                quality: "High".to_owned(),
            }),
            B(BlockKind::SetGraphicsOption {
                setting: "ShadowQuality".to_owned(),
                choice: "High".to_owned(),
            }),
            B(BlockKind::SetGraphicsNumber {
                setting: "RenderScale".to_owned(),
                value: number(1.0),
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
        Category::Tweens => {
            let gui = first(Filter::Gui);
            vec![
                B(BlockKind::Tween {
                    target: part.clone(),
                    property: "Position".to_owned(),
                    value: vector(0.0, 10.0, 0.0),
                    seconds: number(1.0),
                    style: "Quad".to_owned(),
                    direction: "Out".to_owned(),
                }),
                B(BlockKind::SlideIn {
                    target: gui.clone(),
                    from: "Left".to_owned(),
                    seconds: number(0.5),
                }),
                B(BlockKind::FadeIn {
                    target: gui.clone(),
                    seconds: number(0.5),
                }),
                B(BlockKind::FadeOut {
                    target: gui,
                    seconds: number(0.5),
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
            B(BlockKind::SaveSlot { slot: number(1.0) }),
            B(BlockKind::LoadSlot { slot: number(1.0) }),
            B(BlockKind::SetCheckpoint {
                checkpoint: first(Filter::Checkpoints),
            }),
            B(BlockKind::RestoreCheckpoint),
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

pub(super) fn show_palette(
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
pub(super) fn palette_block(
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

pub(super) fn palette_reporter(
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
