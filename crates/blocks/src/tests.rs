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
    BlockScript {
        blocks,
        ..Default::default()
    }
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
        | BlockKind::WhenStateChanged { .. }
        | BlockKind::WhenAnimationEvent { .. }
        | BlockKind::Unattached { .. }
        | BlockKind::TopCode { .. } => return None,
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
        BlockKind::ShowPage { .. } => 27,
        BlockKind::Code { .. } => 28,
        BlockKind::SetAnimatorBool { .. } => 29,
        BlockKind::SetAnimatorNumber { .. } => 30,
        BlockKind::SetAnimatorTrigger { .. } => 31,
        BlockKind::ResetAnimatorTrigger { .. } => 32,
        BlockKind::PlayAnimatorState { .. } => 33,
        BlockKind::PlayAnimation { .. } => 34,
    })
}
const STATEMENT_COUNT: usize = 35;

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
        (
            BlockKind::ShowPage {
                layout: "Menu".to_owned(),
                page: Expression::Text {
                    value: "Home".to_owned(),
                },
            },
            "workspace.Menu.CurrentPage = \"Home\"",
        ),
        (
            BlockKind::Code {
                code: "for i = 1, 2 do\n    print(i)\nend".to_owned(),
            },
            "    for i = 1, 2 do\n        print(i)\n    end",
        ),
        (
            BlockKind::SetAnimatorBool {
                animator: animator(),
                parameter: "IsGrounded".to_owned(),
                value: truth(),
            },
            "workspace.Hero.Animator:SetBool(\"IsGrounded\", true)",
        ),
        (
            BlockKind::SetAnimatorNumber {
                animator: animator(),
                parameter: "Speed".to_owned(),
                value: number(4.0),
            },
            "workspace.Hero.Animator:SetNumber(\"Speed\", 4)",
        ),
        (
            BlockKind::SetAnimatorTrigger {
                animator: animator(),
                parameter: "Jump".to_owned(),
            },
            "workspace.Hero.Animator:SetTrigger(\"Jump\")",
        ),
        (
            BlockKind::ResetAnimatorTrigger {
                animator: animator(),
                parameter: "Jump".to_owned(),
            },
            "workspace.Hero.Animator:ResetTrigger(\"Jump\")",
        ),
        (
            BlockKind::PlayAnimatorState {
                animator: animator(),
                state: "Jump".to_owned(),
            },
            "workspace.Hero.Animator:Play(\"Jump\")",
        ),
        (
            BlockKind::PlayAnimation {
                animator: animator(),
                animation: "Hero.Wave".to_owned(),
            },
            "workspace.Hero.Animator:LoadAnimation(workspace.Hero.Wave):Play()",
        ),
    ]
}

fn animator() -> String {
    "Hero.Animator".to_owned()
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
        block(
            8,
            BlockKind::WhenStateChanged {
                animator: animator(),
                body: quiet(18),
            },
        ),
        block(
            9,
            BlockKind::WhenAnimationEvent {
                animator: animator(),
                event: "Footstep".to_owned(),
                body: quiet(19),
            },
        ),
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
        "workspace.Hero.Animator.StateChanged:Connect(function(state, previous)",
        "workspace.Hero.Animator.AnimationEvent:Connect(function(name)",
        "    if name == \"Footstep\" then",
    ] {
        assert!(source.contains(lua), "missing `{lua}`:\n{source}");
    }
    // Unattached blocks don't run.
    assert_eq!(source.matches("do return end").count(), 7, "{source}");
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
fn the_animation_examples_blocks_drive_its_animator() {
    use axcenity_core::{KeyCode, Parameter};
    use axcenity_script::KeyEvent;
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/hello-animation/scripts/Keys.client.blocks.json");
    let text = std::fs::read_to_string(path).unwrap();
    let blocks: BlockScript = serde_json::from_str(&text).unwrap();
    let source = translate(&blocks).unwrap().source;
    for lua in [
        "workspace.Noodle.Animator:SetNumber(\"Speed\", 2)",
        "workspace.Noodle.Animator:SetTrigger(\"Hop\")",
        "workspace.Noodle.Animator:Play(\"Wave\")",
        "workspace.Noodle.Animator.StateChanged:Connect(function(state, previous)",
    ] {
        assert!(source.contains(lua), "missing `{lua}`:\n{source}");
    }

    // A world with the noodle's Animator, and the blocks as a LocalScript.
    let mut world = World::new();
    let noodle = world.create(ClassName::MeshPart).unwrap();
    world.set_name(noodle, "Noodle").unwrap();
    world.set_parent(noodle, Some(world.workspace())).unwrap();
    let animator = world.create(ClassName::Animator).unwrap();
    world.set_parent(animator, Some(noodle)).unwrap();
    let keys = world.create(ClassName::LocalScript).unwrap();
    world
        .set_parent(keys, Some(world.starter_player_scripts()))
        .unwrap();
    let mut runtime = ScriptRuntime::new();
    runtime.start_script(&mut world, keys, "scripts/Keys.client.blocks.json", &source);
    let press = |key, pressed| KeyEvent { key, pressed };
    runtime.input(
        &mut world,
        &[press(KeyCode::W, true), press(KeyCode::Space, true)],
    );
    runtime.step(&mut world, 1.0 / 60.0);
    runtime.heartbeat(&mut world, 1.0 / 60.0);
    let live = world.animator(animator).unwrap();
    assert_eq!(live.parameters["Speed"], Parameter::Number(2.0));
    assert_eq!(live.parameters["Hop"], Parameter::Trigger(true));
    runtime.input(&mut world, &[press(KeyCode::Q, true)]);
    let request = world.animator(animator).unwrap().play.clone().unwrap();
    assert_eq!(request.state, "Wave");
    let errors: Vec<String> = runtime
        .take_output()
        .into_iter()
        .filter(|line| line.kind == OutputKind::Error)
        .map(|line| line.text)
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
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

/// Every block, as a script: one of each event, each holding every statement.
fn every_block_script() -> BlockScript {
    let mut id = 100;
    let mut body = || {
        every_statement()
            .into_iter()
            .filter(|(kind, _)| !matches!(kind, BlockKind::Code { .. }))
            .map(|(kind, _)| {
                id += 1;
                block(id, kind)
            })
            .collect::<Vec<_>>()
    };
    let mut touched_body = body();
    touched_body.push(block(
        90,
        BlockKind::Print {
            value: Expression::TouchingPart,
        },
    ));
    let stacks = [
        block(1, BlockKind::WhenStarted { body: body() }),
        block(
            2,
            BlockKind::WhenTouched {
                part: "Pad".to_owned(),
                body: touched_body,
            },
        ),
        block(
            3,
            BlockKind::WhenKeyPressed {
                key: "Space".to_owned(),
                body: body(),
            },
        ),
        block(
            4,
            BlockKind::WhenClicked {
                button: "Menu.Start".to_owned(),
                body: body(),
            },
        ),
        block(5, BlockKind::EveryFrame { body: body() }),
        block(
            6,
            BlockKind::WhenSoundEnded {
                sound: "Music".to_owned(),
                body: body(),
            },
        ),
        block(
            7,
            BlockKind::WhenStateChanged {
                animator: animator(),
                body: body(),
            },
        ),
        block(
            8,
            BlockKind::WhenAnimationEvent {
                animator: animator(),
                event: "Footstep".to_owned(),
                body: body(),
            },
        ),
    ];
    // New IDs all the way down, as the statements are copies.
    script(stacks.iter().map(Block::duplicate).collect())
}

/// Whether any of `script` came back as grey Lua blocks.
fn has_code(script: &BlockScript) -> bool {
    let mut found = false;
    script.for_each_block(&mut |block| {
        found |= matches!(
            block.kind,
            BlockKind::Code { .. } | BlockKind::TopCode { .. }
        ) || block
            .kind
            .expressions()
            .iter()
            .any(|e| matches!(e, Expression::Code { .. }));
    });
    found
}

#[test]
fn blocks_switched_to_code_and_back_are_the_same_blocks() {
    let original = every_block_script();
    let lua = translate(&original).unwrap().source;
    let back = from_lua(&lua).unwrap();
    assert!(!has_code(&back), "every block should come back:\n{back:#?}");
    assert_eq!(translate(&back).unwrap().source, lua);
}

#[test]
fn reporters_switched_to_code_and_back_are_the_same_reporters() {
    let every_reporter = vec![
        Expression::Math {
            op: MathOp::Power,
            left: boxed(variable("a")),
            right: boxed(number(-2.0)),
        },
        Expression::Function {
            function: MathFunction::Round,
            value: boxed(number(2.5)),
        },
        Expression::Function {
            function: MathFunction::Sqrt,
            value: boxed(number(9.0)),
        },
        Expression::Random {
            min: boxed(number(1.0)),
            max: boxed(number(6.0)),
        },
        Expression::Join {
            left: boxed(Expression::Text {
                value: "a \"quoted\"\n".to_owned(),
            }),
            right: boxed(variable("a")),
        },
        Expression::Logic {
            op: LogicOp::Or,
            left: boxed(Expression::Not {
                value: boxed(Expression::Boolean { value: false }),
            }),
            right: boxed(Expression::Compare {
                op: Comparison::GreaterOrEqual,
                left: boxed(Expression::Property {
                    target: "Pad".to_owned(),
                    property: "Transparency".to_owned(),
                }),
                right: boxed(number(0.5)),
            }),
        },
        Expression::KeyDown {
            key: "W".to_owned(),
        },
        Expression::Color3 {
            r: 0.0,
            g: 0.5,
            b: 1.0,
        },
        Expression::AnimatorBool {
            animator: animator(),
            parameter: "IsGrounded".to_owned(),
        },
        Expression::AnimatorNumber {
            animator: animator(),
            parameter: "Speed".to_owned(),
        },
        Expression::Property {
            target: animator(),
            property: "CurrentState".to_owned(),
        },
        Expression::Raycast {
            origin: boxed(Expression::Vector3 {
                x: 0.0,
                y: 10.0,
                z: 0.0,
            }),
            direction: boxed(Expression::Vector3 {
                x: 0.0,
                y: -20.0,
                z: 0.0,
            }),
        },
    ];
    let body = every_reporter
        .into_iter()
        .enumerate()
        .map(|(index, value)| block(10 + index as u64, BlockKind::Print { value }))
        .collect();
    let original = script(vec![block(1, BlockKind::EveryFrame { body })]);
    let lua = translate(&original).unwrap().source;
    let back = from_lua(&lua).unwrap();
    assert!(
        !has_code(&back),
        "every reporter should come back:\n{back:#?}"
    );
    assert_eq!(translate(&back).unwrap().source, lua);
}

#[test]
fn typed_lua_becomes_blocks_and_keeps_what_has_none_as_lua() {
    let typed = "\
local count = 0
-- Doubles a number.
local function double(x)
    return x * 2
end

workspace.Pad.Touched:Connect(function(hit)
    count = count + 1
    for i = 1, 2 do
        print(i)
    end
    print(double(count))
end)
print(\"Hello world!\")
";
    let blocks = from_lua(typed).unwrap();
    let kinds: Vec<_> = blocks.blocks.iter().map(|block| &block.kind).collect();
    let [
        BlockKind::TopCode { code },
        BlockKind::WhenTouched { part, body },
        BlockKind::WhenStarted { body: started },
    ] = kinds.as_slice()
    else {
        panic!("unexpected blocks: {kinds:#?}");
    };
    assert_eq!(
        code,
        "-- Doubles a number.\nlocal function double(x)\n    return x * 2\nend"
    );
    assert_eq!(part, "Pad");
    assert!(matches!(&body[0].kind, BlockKind::ChangeVariable { name, .. } if name == "count"));
    assert_eq!(
        body[1].kind,
        BlockKind::Code {
            code: "for i = 1, 2 do\n    print(i)\nend".to_owned()
        }
    );
    assert_eq!(
        body[2].kind,
        BlockKind::Print {
            value: Expression::Code {
                code: "double(count)".to_owned()
            }
        }
    );
    assert!(matches!(&started[0].kind, BlockKind::Print { .. }));

    // And it still runs.
    let mut world = World::new();
    let pad = world.create(ClassName::Part).unwrap();
    world.set_name(pad, "Pad").unwrap();
    world.set_parent(pad, Some(world.workspace())).unwrap();
    let lua = world.create(ClassName::Script).unwrap();
    world
        .set_parent(lua, Some(world.server_script_service()))
        .unwrap();
    let source = translate(&blocks).unwrap().source;
    let mut runtime = ScriptRuntime::new();
    runtime.start_script(&mut world, lua, "scripts/Typed.server.lua", &source);
    runtime.step(&mut world, 0.1);
    let output: Vec<_> = runtime
        .take_output()
        .into_iter()
        .map(|line| line.text)
        .collect();
    assert_eq!(output, ["Hello world!"], "{source}");
}

#[test]
fn luau_syntax_becomes_blocks_or_stays_as_lua() {
    let typed = "\
local count = 0
type Point = { x: number, y: number }
local function double(x: number): number
    return x * 2
end

count += 2
for i = 1, 3 do
    if i == 2 then continue end
    print(`step {i} of {count}`)
end
local label: string = if count > 1 then \"many\" else \"one\"
print(label, double(count :: number), 0b101 + 1_000)
";
    let blocks = from_lua(typed).unwrap();
    let kinds: Vec<_> = blocks.blocks.iter().map(|block| &block.kind).collect();
    let [
        BlockKind::TopCode { code: first },
        BlockKind::WhenStarted { body },
        BlockKind::TopCode { code: last },
    ] = kinds.as_slice()
    else {
        panic!("unexpected blocks: {kinds:#?}");
    };
    assert!(first.starts_with("type Point = "), "{first}");
    assert!(
        matches!(&body[..], [Block { kind: BlockKind::ChangeVariable { name, .. }, .. }] if name == "count")
    );
    assert!(
        last.contains("continue") && last.ends_with("1_000)"),
        "{last}"
    );

    // Switching again changes nothing more.
    let source = translate(&blocks).unwrap().source;
    assert_eq!(
        translate(&from_lua(&source).unwrap()).unwrap().source,
        source
    );

    // And it still runs.
    let mut world = World::new();
    let lua = world.create(ClassName::Script).unwrap();
    world
        .set_parent(lua, Some(world.server_script_service()))
        .unwrap();
    let mut runtime = ScriptRuntime::new();
    runtime.start_script(&mut world, lua, "scripts/Luau.server.lua", &source);
    runtime.step(&mut world, 0.1);
    let output: Vec<_> = runtime
        .take_output()
        .into_iter()
        .map(|line| line.text)
        .collect();
    assert_eq!(
        output,
        ["step 1 of 2", "step 3 of 2", "many\t4\t1005"],
        "{source}"
    );

    // A handler with type annotations stays Lua, so they aren't lost.
    let typed = "workspace.Pad.Touched:Connect(function(hit: BasePart)\n    print(hit)\nend)\n";
    let blocks = from_lua(typed).unwrap();
    assert!(
        matches!(&blocks.blocks[..], [Block { kind: BlockKind::TopCode { code }, .. }] if code == typed.trim_end()),
        "{blocks:#?}"
    );
    // `+=` on a part's Position is a "move by" block.
    let blocks = from_lua(
        "workspace.Pad.Touched:Connect(function()\n    workspace.Pad.Position += Vector3.new(0, 1, 0)\nend)\n",
    )
    .unwrap();
    let [
        Block {
            kind: BlockKind::WhenTouched { body, .. },
            ..
        },
    ] = &blocks.blocks[..]
    else {
        panic!("{blocks:#?}");
    };
    assert!(matches!(&body[0].kind, BlockKind::MoveBy { target, .. } if target == "Pad"));
}

#[test]
fn lua_5_4_syntax_that_luau_lacks_is_a_syntax_error() {
    for code in [
        "goto done",
        "local x <const> = 1",
        "print(1 << 2)",
        "::done::",
    ] {
        assert!(from_lua(code).is_err(), "{code}");
    }
    // `goto`, `continue` and `type` are names in Luau.
    assert!(from_lua("local goto = 1\nlocal continue = 2\nprint(type(goto), continue)").is_ok());
}

#[test]
fn top_level_code_that_waits_stays_in_order() {
    // Moving the wait into "when game starts" would connect the event
    // before the wait is over.
    let typed = "task.wait(2)\nworkspace.Pad.Touched:Connect(function()\nend)\n";
    let blocks = from_lua(typed).unwrap();
    assert!(matches!(
        &blocks.blocks[0].kind,
        BlockKind::TopCode { code } if code == "task.wait(2)"
    ));
    // A new script's code becomes a "when game starts" block.
    let blocks = from_lua("print(\"Hello world!\")\n").unwrap();
    assert!(matches!(
        &blocks.blocks[..],
        [Block {
            kind: BlockKind::WhenStarted { .. },
            ..
        }]
    ));
}

#[test]
fn code_with_a_syntax_error_says_where() {
    let err = from_lua("print(\"hi\")\nif x then\n").unwrap_err();
    let FromLuaError::Syntax(err) = err else {
        panic!("{err:?}");
    };
    assert_eq!(err.line, 3);
    assert!(from_lua("x = = 1").is_err());
}

#[test]
fn every_example_script_can_switch_to_blocks_and_back() {
    let examples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut checked = 0;
    for project in std::fs::read_dir(examples).unwrap() {
        let scripts = project.unwrap().path().join("scripts");
        let Ok(files) = std::fs::read_dir(scripts) else {
            continue;
        };
        for file in files {
            let path = file.unwrap().path();
            let name = path.to_string_lossy();
            if !(name.ends_with(".server.lua") || name.ends_with(".client.lua")) {
                continue;
            }
            let source = std::fs::read_to_string(&path).unwrap();
            let blocks = from_lua(&source).unwrap_or_else(|err| panic!("{name}: {err}"));
            // Switching again changes nothing more.
            let lua = translate(&blocks).unwrap().source;
            let again = translate(&from_lua(&lua).unwrap()).unwrap().source;
            assert_eq!(lua, again, "{name}");
            checked += 1;
        }
    }
    assert!(checked > 5, "only {checked} example scripts found");
}

#[test]
fn the_graph_view_is_saved_only_once_used_and_never_changes_the_lua() {
    let plain = script(vec![block(
        1,
        BlockKind::WhenStarted {
            body: vec![block(2, BlockKind::Print { value: number(1.0) })],
        },
    )]);
    let text = serde_json::to_string(&plain).unwrap();
    assert!(!text.contains("graph"), "{text}");
    assert_eq!(serde_json::from_str::<BlockScript>(&text).unwrap(), plain);

    let mut shown = plain.clone();
    shown.graph.shown = true;
    shown.graph.nodes.insert("2".to_owned(), [100.0, 40.0]);
    shown.graph.loose.push(LooseValue {
        id: 7,
        value: variable("score"),
    });
    let text = serde_json::to_string(&shown).unwrap();
    assert_eq!(serde_json::from_str::<BlockScript>(&text).unwrap(), shown);
    assert_eq!(
        translate(&shown).unwrap().source,
        translate(&plain).unwrap().source
    );
    // New blocks never take a loose value node's id.
    reserve_block_ids(&shown);
    assert!(Block::new(BlockKind::StopScript).id > 7);
}
