use axcenity_core::{ClassName, Color3, Value, Vector3, World};
use axcenity_script::{OutputKind, ScriptRuntime};

use super::*;

fn number(value: f64) -> Expression {
    Expression::Number { value }
}

fn boxed(expression: Expression) -> Box<Expression> {
    Box::new(expression)
}

fn variable(name: &str) -> Expression {
    Expression::Variable {
        name: name.to_owned(),
    }
}

fn block(id: u64, kind: BlockKind) -> Block {
    Block {
        id,
        kind,
        position: None,
    }
}

fn script(blocks: Vec<Block>) -> BlockScript {
    BlockScript { blocks }
}

/// Which variant `kind` is. There's no `_ =>`, so a new block type stops
/// this compiling until it's added to [`every_statement`] and here.
fn statement_index(kind: &BlockKind) -> Option<usize> {
    Some(match kind {
        BlockKind::WhenStarted { .. }
        | BlockKind::WhenTouched { .. }
        | BlockKind::WhenKeyPressed { .. }
        | BlockKind::WhenClicked { .. }
        | BlockKind::EveryFrame { .. }
        | BlockKind::WhenSoundEnded { .. }
        | BlockKind::Unattached { .. } => return None,
        BlockKind::Wait { .. } => 0,
        BlockKind::WaitUntil { .. } => 1,
        BlockKind::Repeat { .. } => 2,
        BlockKind::Forever { .. } => 3,
        BlockKind::RepeatUntil { .. } => 4,
        BlockKind::If { .. } => 5,
        BlockKind::IfElse { .. } => 6,
        BlockKind::StopScript => 7,
        BlockKind::MoveTo { .. } => 8,
        BlockKind::MoveBy { .. } => 9,
        BlockKind::TurnTo { .. } => 10,
        BlockKind::TurnBy { .. } => 11,
        BlockKind::SetAnchored { .. } => 12,
        BlockKind::SetColor { .. } => 13,
        BlockKind::SetText { .. } => 14,
        BlockKind::Show { .. } => 15,
        BlockKind::Hide { .. } => 16,
        BlockKind::SetProperty { .. } => 17,
        BlockKind::PlaySound { .. } => 18,
        BlockKind::StopSound { .. } => 19,
        BlockKind::PauseSound { .. } => 20,
        BlockKind::ResumeSound { .. } => 21,
        BlockKind::SetVolume { .. } => 22,
        BlockKind::SetVariable { .. } => 23,
        BlockKind::ChangeVariable { .. } => 24,
        BlockKind::Print { .. } => 25,
        BlockKind::Destroy { .. } => 26,
    })
}
const STATEMENT_COUNT: usize = 27;

/// One of each statement block, with the Lua it must produce.
fn every_statement() -> Vec<(BlockKind, &'static str)> {
    let pad = || "Pad".to_owned();
    let truth = || Expression::Boolean { value: true };
    vec![
        (
            BlockKind::Wait {
                seconds: number(0.5),
            },
            "task.wait(0.5)",
        ),
        (
            BlockKind::WaitUntil { condition: truth() },
            "while not (true) do",
        ),
        (
            BlockKind::Repeat {
                count: number(3.0),
                body: Vec::new(),
            },
            "for _ = 1, 3 do",
        ),
        (BlockKind::Forever { body: Vec::new() }, "while true do"),
        (
            BlockKind::RepeatUntil {
                condition: truth(),
                body: Vec::new(),
            },
            "while not (true) do",
        ),
        (
            BlockKind::If {
                condition: truth(),
                body: Vec::new(),
            },
            "if true then",
        ),
        (
            BlockKind::IfElse {
                condition: truth(),
                then_body: Vec::new(),
                else_body: vec![block(900, BlockKind::StopScript)],
            },
            "else",
        ),
        (BlockKind::StopScript, "do return end"),
        (
            BlockKind::MoveTo {
                target: pad(),
                position: Expression::Vector3 {
                    x: 1.0,
                    y: 2.0,
                    z: 3.0,
                },
            },
            "workspace.Pad.Position = Vector3.new(1, 2, 3)",
        ),
        (
            BlockKind::MoveBy {
                target: pad(),
                offset: Expression::Vector3 {
                    x: 0.0,
                    y: 1.0,
                    z: 0.0,
                },
            },
            "workspace.Pad.Position = workspace.Pad.Position + Vector3.new(0, 1, 0)",
        ),
        (
            BlockKind::TurnTo {
                target: pad(),
                orientation: Expression::Vector3 {
                    x: 0.0,
                    y: 90.0,
                    z: 0.0,
                },
            },
            "workspace.Pad.Orientation = Vector3.new(0, 90, 0)",
        ),
        (
            BlockKind::TurnBy {
                target: pad(),
                rotation: Expression::Vector3 {
                    x: 0.0,
                    y: 5.0,
                    z: 0.0,
                },
            },
            "workspace.Pad.Orientation = workspace.Pad.Orientation + Vector3.new(0, 5, 0)",
        ),
        (
            BlockKind::SetAnchored {
                target: pad(),
                anchored: Expression::Boolean { value: false },
            },
            "workspace.Pad.Anchored = false",
        ),
        (
            BlockKind::SetColor {
                target: pad(),
                color: Expression::Color3 {
                    r: 1.0,
                    g: 0.0,
                    b: 0.0,
                },
            },
            "workspace.Pad.Color = Color3.new(1, 0, 0)",
        ),
        (
            BlockKind::SetText {
                target: "Hud.Score".to_owned(),
                text: variable("score"),
            },
            "workspace.Hud.Score.Text = tostring(score)",
        ),
        (
            BlockKind::Show { target: pad() },
            "workspace.Pad.Visible = true",
        ),
        (
            BlockKind::Hide { target: pad() },
            "workspace.Pad.Visible = false",
        ),
        (
            BlockKind::SetProperty {
                target: pad(),
                property: "Roughness".to_owned(),
                value: number(0.25),
            },
            "workspace.Pad.Roughness = 0.25",
        ),
        (
            BlockKind::PlaySound {
                target: "Pad.Chime".to_owned(),
            },
            "workspace.Pad.Chime:Play()",
        ),
        (
            BlockKind::StopSound {
                target: "Music".to_owned(),
            },
            "workspace.Music:Stop()",
        ),
        (
            BlockKind::PauseSound {
                target: "Music".to_owned(),
            },
            "workspace.Music:Pause()",
        ),
        (
            BlockKind::ResumeSound {
                target: "Music".to_owned(),
            },
            "workspace.Music:Resume()",
        ),
        (
            BlockKind::SetVolume {
                target: "Music".to_owned(),
                volume: number(0.5),
            },
            "workspace.Music.Volume = 0.5",
        ),
        (
            BlockKind::SetVariable {
                name: "score".to_owned(),
                value: number(10.0),
            },
            "score = 10",
        ),
        (
            BlockKind::ChangeVariable {
                name: "score".to_owned(),
                by: number(1.0),
            },
            "score = score + 1",
        ),
        (
            BlockKind::Print {
                value: Expression::Text {
                    value: "hi".to_owned(),
                },
            },
            "print(\"hi\")",
        ),
        (
            BlockKind::Destroy {
                target: "Red Ball".to_owned(),
            },
            "workspace[\"Red Ball\"]:Destroy()",
        ),
    ]
}

#[test]
fn translation_covers_every_statement_block() {
    let statements = every_statement();
    let mut covered = [false; STATEMENT_COUNT];
    let body = statements
        .iter()
        .enumerate()
        .map(|(index, (kind, _))| {
            covered[statement_index(kind).unwrap()] = true;
            block(index as u64 + 10, kind.clone())
        })
        .collect();
    assert!(covered.iter().all(|&c| c), "every_statement misses a block");

    let translated = translate(&script(vec![block(1, BlockKind::WhenStarted { body })])).unwrap();
    for (kind, lua) in &statements {
        assert!(
            translated.source.contains(lua),
            "{kind:?} should give `{lua}`:\n{}",
            translated.source
        );
    }
}

#[test]
fn translation_covers_every_event_block() {
    let say = |id| {
        vec![block(
            id,
            BlockKind::Print {
                value: Expression::TouchingPart,
            },
        )]
    };
    let quiet = |id| vec![block(id, BlockKind::StopScript)];
    let blocks = vec![
        block(1, BlockKind::WhenStarted { body: quiet(11) }),
        block(
            2,
            BlockKind::WhenTouched {
                part: "Pad".to_owned(),
                body: say(12),
            },
        ),
        block(
            3,
            BlockKind::WhenKeyPressed {
                key: "Space".to_owned(),
                body: quiet(13),
            },
        ),
        block(
            4,
            BlockKind::WhenClicked {
                button: "Menu.Start".to_owned(),
                body: quiet(14),
            },
        ),
        block(5, BlockKind::EveryFrame { body: quiet(15) }),
        block(
            6,
            BlockKind::WhenSoundEnded {
                sound: "Music".to_owned(),
                body: quiet(16),
            },
        ),
        block(7, BlockKind::Unattached { body: quiet(17) }),
    ];
    let source = translate(&script(blocks)).unwrap().source;
    for lua in [
        "local RunService = game:GetService(\"RunService\")",
        "local UserInputService = game:GetService(\"UserInputService\")",
        "task.spawn(function()",
        "workspace.Pad.Touched:Connect(function(otherPart)",
        "    print(otherPart)",
        "UserInputService.InputBegan:Connect(function(input)",
        "    if input.KeyCode == Enum.KeyCode.Space then",
        "workspace.Menu.Start.Activated:Connect(function()",
        "RunService.Heartbeat:Connect(function(deltaTime)",
        "workspace.Music.Ended:Connect(function()",
    ] {
        assert!(source.contains(lua), "missing `{lua}`:\n{source}");
    }
    // Unattached blocks don't run.
    assert_eq!(source.matches("do return end").count(), 5, "{source}");
}

#[test]
fn reporters_translate_with_brackets_where_the_blocks_nest() {
    let sum = Expression::Math {
        op: MathOp::Add,
        left: boxed(variable("a")),
        right: boxed(number(2.0)),
    };
    let cases = [
        (
            Expression::Math {
                op: MathOp::Multiply,
                left: boxed(sum.clone()),
                right: boxed(number(3.0)),
            },
            "(a + 2) * 3",
        ),
        (
            Expression::Function {
                function: MathFunction::Round,
                value: boxed(variable("a")),
            },
            "math.floor(a + 0.5)",
        ),
        (
            Expression::Random {
                min: boxed(number(1.0)),
                max: boxed(number(6.0)),
            },
            "math.random(math.floor(1), math.floor(6))",
        ),
        (
            Expression::Join {
                left: boxed(Expression::Text {
                    value: "Score: ".to_owned(),
                }),
                right: boxed(variable("a")),
            },
            "tostring(\"Score: \") .. tostring(a)",
        ),
        (
            Expression::Not {
                value: boxed(Expression::Logic {
                    op: LogicOp::And,
                    left: boxed(Expression::Compare {
                        op: Comparison::Greater,
                        left: boxed(variable("a")),
                        right: boxed(number(1.0)),
                    }),
                    right: boxed(Expression::KeyDown {
                        key: "W".to_owned(),
                    }),
                }),
            },
            "not ((a > 1) and UserInputService:IsKeyDown(Enum.KeyCode.W))",
        ),
        (
            Expression::Property {
                target: "Pad".to_owned(),
                property: "Position".to_owned(),
            },
            "workspace.Pad.Position",
        ),
    ];
    for (expression, lua) in cases {
        let blocks = vec![block(
            1,
            BlockKind::WhenStarted {
                body: vec![block(
                    2,
                    BlockKind::Print {
                        value: expression.clone(),
                    },
                )],
            },
        )];
        let source = translate(&script(blocks)).unwrap().source;
        assert!(
            source.contains(&format!("print({lua})")),
            "{expression:?} should give `{lua}`:\n{source}"
        );
    }
}

#[test]
fn variables_belong_to_the_whole_script_and_start_at_zero() {
    let blocks = vec![block(
        1,
        BlockKind::WhenStarted {
            body: vec![
                block(
                    2,
                    BlockKind::ChangeVariable {
                        name: "score".to_owned(),
                        by: number(1.0),
                    },
                ),
                block(
                    3,
                    BlockKind::Print {
                        value: variable("lives"),
                    },
                ),
            ],
        },
    )];
    let translated = translate(&script(blocks)).unwrap();
    assert_eq!(
        translated.source,
        "local lives = 0\nlocal score = 0\n\ntask.spawn(function()\n    score = score + 1\n    print(lives)\nend)\n"
    );
    // Lines count from 1: the event is on 4, its blocks on 5 and 6.
    assert_eq!(translated.block_lines, [(4, 1), (5, 2), (6, 3)]);
    assert_eq!(translated.block_at(6), Some(3));
    assert_eq!(translated.block_at(7), Some(3));
    assert_eq!(translated.block_at(2), None);
}

#[test]
fn translated_blocks_run_in_the_scheduler_like_typed_lua() {
    let mut world = World::new();
    let pad = world.create(ClassName::Part).unwrap();
    world.set_name(pad, "Pad").unwrap();
    world.set_parent(pad, Some(world.workspace())).unwrap();

    let blocks = vec![block(
        1,
        BlockKind::WhenStarted {
            body: vec![
                block(
                    2,
                    BlockKind::SetColor {
                        target: "Pad".to_owned(),
                        color: Expression::Color3 {
                            r: 1.0,
                            g: 0.0,
                            b: 0.0,
                        },
                    },
                ),
                block(
                    3,
                    BlockKind::Repeat {
                        count: number(3.0),
                        body: vec![
                            block(
                                4,
                                BlockKind::MoveBy {
                                    target: "Pad".to_owned(),
                                    offset: Expression::Vector3 {
                                        x: 0.0,
                                        y: 1.0,
                                        z: 0.0,
                                    },
                                },
                            ),
                            block(
                                5,
                                BlockKind::ChangeVariable {
                                    name: "moves".to_owned(),
                                    by: number(1.0),
                                },
                            ),
                        ],
                    },
                ),
                block(
                    6,
                    BlockKind::Wait {
                        seconds: number(1.0),
                    },
                ),
                block(
                    7,
                    BlockKind::Print {
                        value: Expression::Join {
                            left: boxed(Expression::Text {
                                value: "moves: ".to_owned(),
                            }),
                            right: boxed(variable("moves")),
                        },
                    },
                ),
            ],
        },
    )];
    let source = translate(&script(blocks)).unwrap().source;

    let lua = world.create(ClassName::Script).unwrap();
    world.set_name(lua, "Blocks").unwrap();
    world
        .set_parent(lua, Some(world.server_script_service()))
        .unwrap();
    let mut runtime = ScriptRuntime::new();
    runtime.start_script(
        &mut world,
        lua,
        "scripts/Blocks.server.blocks.json",
        &source,
    );
    runtime.step(&mut world, 0.5);
    let property = |name| world.get(pad).unwrap().property(name).unwrap().clone();
    assert_eq!(property("Color"), Value::Color3(Color3::new(1.0, 0.0, 0.0)));
    assert_eq!(
        property("Position"),
        Value::Vector3(Vector3::new(0.0, 3.0, 0.0))
    );
    assert!(runtime.take_output().is_empty(), "the wait isn't over yet");

    runtime.step(&mut world, 1.0);
    let output: Vec<_> = runtime
        .take_output()
        .into_iter()
        .map(|line| (line.kind, line.text))
        .collect();
    assert_eq!(output, [(OutputKind::Print, "moves: 3".to_owned())]);
}

#[test]
fn the_first_saved_format_still_loads() {
    let saved = r#"{ "blocks": [ { "id": 1, "kind": { "type": "when_started", "body": [
        { "id": 2, "kind": { "type": "if_else",
          "condition": { "left": { "type": "number", "value": 1 }, "operator": "less",
                         "right": { "type": "variable", "name": "x" } },
          "then_body": [], "else_body": [] } } ] } } ] }"#;
    let script: BlockScript = serde_json::from_str(saved).unwrap();
    let BlockKind::WhenStarted { body } = &script.blocks[0].kind else {
        panic!("{script:?}");
    };
    let BlockKind::IfElse { condition, .. } = &body[0].kind else {
        panic!("{body:?}");
    };
    assert_eq!(
        *condition,
        Expression::Compare {
            op: Comparison::Less,
            left: boxed(number(1.0)),
            right: boxed(variable("x")),
        }
    );
    assert!(translate(&script).unwrap().source.contains("if 1 < x then"));
}

#[test]
fn slots_only_take_blocks_of_their_shape() {
    assert!(!BlockShape::Statement.accepts(BlockShape::Event));
    assert!(!BlockShape::Boolean.accepts(BlockShape::Value));
    assert!(BlockShape::Value.accepts(BlockShape::Boolean));
    assert!(!BlockShape::Value.accepts(BlockShape::Statement));
    assert_eq!(
        Expression::Compare {
            op: Comparison::Equal,
            left: boxed(number(1.0)),
            right: boxed(number(1.0)),
        }
        .shape(),
        BlockShape::Boolean
    );
}

#[test]
fn mistakes_are_reported_instead_of_making_broken_lua() {
    let repeated = block(
        2,
        BlockKind::Wait {
            seconds: number(1.0),
        },
    );
    let doubled = script(vec![block(
        1,
        BlockKind::WhenStarted {
            body: vec![repeated.clone(), repeated],
        },
    )]);
    assert_eq!(translate(&doubled), Err(TranslateError::DuplicateId(2)));

    let outside = script(vec![block(
        1,
        BlockKind::WhenStarted {
            body: vec![block(
                2,
                BlockKind::Print {
                    value: Expression::TouchingPart,
                },
            )],
        },
    )]);
    assert_eq!(
        translate(&outside),
        Err(TranslateError::TouchingPartOutsideTouched)
    );

    let loose = script(vec![block(1, BlockKind::StopScript)]);
    assert_eq!(
        translate(&loose),
        Err(TranslateError::InvalidEventPlacement)
    );
    assert_eq!(quote("sound\"path"), "\"sound\\\"path\"");
}

#[test]
fn duplicates_get_new_ids_all_the_way_down() {
    let original = Block::new(BlockKind::Forever {
        body: vec![Block::new(BlockKind::StopScript)],
    });
    let copy = original.duplicate();
    let ids = |block: &Block| {
        let mut ids = vec![block.id];
        script(vec![block.clone()]).for_each_block(&mut |b| ids.push(b.id));
        ids
    };
    let (a, b) = (ids(&original), ids(&copy));
    assert!(a.iter().all(|id| !b.contains(id)), "{a:?} vs {b:?}");
}
