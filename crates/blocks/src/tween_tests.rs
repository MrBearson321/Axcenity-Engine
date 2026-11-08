use axcenity_core::{ClassName, InstanceId, UDim2, Value, World};
use axcenity_script::{OutputKind, ScriptRuntime};

use super::*;

fn block(id: u64, kind: BlockKind) -> Block {
    Block {
        id,
        kind,
        ..Block::new(BlockKind::StopScript)
    }
}

fn started(body: Vec<Block>) -> BlockScript {
    BlockScript {
        blocks: vec![block(1, BlockKind::WhenStarted { body })],
        ..Default::default()
    }
}

fn seconds(value: f64) -> Expression {
    Expression::Number { value }
}

/// A Part named `Pad` and a Frame `Hud.Menu` at (0, 60, 0, 40), and a
/// runtime running `source` as a Script.
fn run(source: &str) -> (World, ScriptRuntime, InstanceId, InstanceId) {
    let mut world = World::new();
    let pad = world.create(ClassName::Part).unwrap();
    world.set_name(pad, "Pad").unwrap();
    world.set_parent(pad, Some(world.workspace())).unwrap();
    let hud = world.create(ClassName::ScreenGui).unwrap();
    world.set_name(hud, "Hud").unwrap();
    world.set_parent(hud, Some(world.workspace())).unwrap();
    let menu = world.create(ClassName::Frame).unwrap();
    world.set_name(menu, "Menu").unwrap();
    world.set_parent(menu, Some(hud)).unwrap();
    world
        .set_property(
            menu,
            "Position",
            Value::UDim2(UDim2::new(0.0, 60.0, 0.0, 40.0)),
        )
        .unwrap();
    let script = world.create(ClassName::Script).unwrap();
    world
        .set_parent(script, Some(world.server_script_service()))
        .unwrap();
    let mut runtime = ScriptRuntime::new();
    runtime.start_script(&mut world, script, "scripts/Tweens.server.lua", source);
    (world, runtime, pad, menu)
}

/// The Pad's position and the Menu's position and background
/// transparency, each frame for two seconds.
fn motion(source: &str) -> Vec<(Value, Value, Value)> {
    let (mut world, mut runtime, pad, menu) = run(source);
    let mut frames = Vec::new();
    for _ in 0..120 {
        runtime.step(&mut world, 1.0 / 60.0);
        let get = |id: InstanceId, name| world.get(id).unwrap().property(name).unwrap().clone();
        frames.push((
            get(pad, "Position"),
            get(menu, "Position"),
            get(menu, "BackgroundTransparency"),
        ));
    }
    let errors: Vec<String> = runtime
        .take_output()
        .into_iter()
        .filter(|line| line.kind == OutputKind::Error)
        .map(|line| line.text)
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
    frames
}

fn tween_blocks() -> BlockScript {
    started(vec![
        block(
            2,
            BlockKind::Tween {
                target: "Pad".to_owned(),
                property: "Position".to_owned(),
                value: Expression::Vector3 {
                    x: 10.0,
                    y: 0.0,
                    z: 0.0,
                },
                seconds: seconds(1.0),
                style: "Back".to_owned(),
                direction: "InOut".to_owned(),
            },
        ),
        block(
            3,
            BlockKind::SlideIn {
                target: "Hud.Menu".to_owned(),
                from: "Left".to_owned(),
                seconds: seconds(0.5),
            },
        ),
        block(
            4,
            BlockKind::Wait {
                seconds: seconds(1.0),
            },
        ),
        block(
            5,
            BlockKind::FadeOut {
                target: "Hud.Menu".to_owned(),
                seconds: seconds(0.5),
            },
        ),
    ])
}

/// The same tweens, as someone would type them.
const TYPED: &str = r#"
local TweenService = game:GetService("TweenService")
local menu = workspace.Hud.Menu

task.spawn(function()
    local info = TweenInfo.new(1, Enum.EasingStyle.Back, Enum.EasingDirection.InOut)
    TweenService:Create(workspace.Pad, info, {Position = Vector3.new(10, 0, 0)}):Play()
    local onScreen = menu.Position
    menu.Position = UDim2.new(onScreen.X.Scale - 1, onScreen.X.Offset, onScreen.Y.Scale, onScreen.Y.Offset)
    TweenService:Create(menu, TweenInfo.new(0.5), {Position = onScreen}):Play()
    task.wait(1)
    TweenService:Create(menu, TweenInfo.new(0.5), {BackgroundTransparency = 1}):Play()
end)
"#;

#[test]
fn tween_blocks_make_the_same_motion_as_typed_tween_service_lua() {
    let lua = translate(&tween_blocks()).unwrap().source;
    assert!(
        lua.starts_with("local TweenService = game:GetService(\"TweenService\")\n"),
        "{lua}"
    );
    let from_blocks = motion(&lua);
    assert_eq!(from_blocks, motion(TYPED));

    // The menu starts a screen to the left, and is back by 0.5 s.
    let x = |frame: usize| match &from_blocks[frame].1 {
        Value::UDim2(position) => position.x,
        other => panic!("{other:?}"),
    };
    assert!(x(0).scale < -0.5, "{:?}", x(0));
    assert_eq!(x(40), axcenity_core::UDim::new(0.0, 60.0));
    // Faded out by 2 s, and the pad went all the way.
    assert_eq!(from_blocks[119].2, Value::Number(1.0));
    assert_eq!(
        from_blocks[119].0,
        Value::Vector3(axcenity_core::Vector3::new(10.0, 0.0, 0.0))
    );
}

#[test]
fn every_slide_edge_and_fade_switches_to_code_and_back() {
    let mut body = Vec::new();
    for (index, edge) in SLIDE_EDGES.iter().enumerate() {
        body.push(block(
            10 + index as u64,
            BlockKind::SlideIn {
                target: "Hud.Menu".to_owned(),
                from: (*edge).to_owned(),
                seconds: Expression::Variable {
                    name: "goal".to_owned(),
                },
            },
        ));
    }
    body.push(block(
        20,
        BlockKind::FadeIn {
            target: "Hud.Menu".to_owned(),
            seconds: seconds(0.25),
        },
    ));
    for (index, style) in EASING_STYLES.iter().enumerate() {
        body.push(block(
            30 + index as u64,
            BlockKind::Tween {
                target: "Pad".to_owned(),
                property: "Transparency".to_owned(),
                value: seconds(0.5),
                seconds: seconds(2.0),
                style: (*style).to_owned(),
                direction: EASING_DIRECTIONS[index % 3].to_owned(),
            },
        ));
    }
    let original = started(body);
    let lua = translate(&original).unwrap().source;
    let back = from_lua(&lua).unwrap();
    assert_eq!(translate(&back).unwrap().source, lua);
    let statements = |script: &BlockScript| {
        let mut kinds = Vec::new();
        script.for_each_block(&mut |block| {
            if !block.kind.is_event() {
                kinds.push(block.kind.clone());
            }
        });
        kinds
    };
    assert_eq!(statements(&back), statements(&original));

    // A block variable named `goal` still sets the time, not the local.
    let (mut world, mut runtime, _, menu) = run(&lua.replace("local goal = 0", "local goal = 2"));
    runtime.step(&mut world, 1.0);
    assert_ne!(
        world.get(menu).unwrap().property("Position"),
        Some(&Value::UDim2(UDim2::new(0.0, 60.0, 0.0, 40.0))),
        "two seconds long, so not there yet"
    );
}

#[test]
fn typed_tweens_become_tween_blocks_and_other_tweens_stay_lua() {
    let typed = r#"local TweenService = game:GetService("TweenService")

task.spawn(function()
    TweenService:Create(workspace.Pad, TweenInfo.new(2, Enum.EasingStyle.Quad, Enum.EasingDirection.Out), {Transparency = 1}):Play()
    TweenService:Create(workspace.Pad, TweenInfo.new(2), {Transparency = 1, Color = Color3.new(1, 0, 0)}):Play()
    TweenService:Create(workspace.Pad, TweenInfo.new(1, Enum.EasingStyle.Quad), {Transparency = 1}):Play()
end)
"#;
    let blocks = from_lua(typed).unwrap();
    // The service's declaration is the blocks' own, so it isn't kept.
    let [stack] = blocks.blocks.as_slice() else {
        panic!("{blocks:#?}");
    };
    let BlockKind::WhenStarted { body } = &stack.kind else {
        panic!("{blocks:#?}");
    };
    // Written out in full, as the block writes it, it's a tween block.
    assert_eq!(
        body[0].kind,
        BlockKind::Tween {
            target: "Pad".to_owned(),
            property: "Transparency".to_owned(),
            value: seconds(1.0),
            seconds: seconds(2.0),
            style: "Quad".to_owned(),
            direction: "Out".to_owned(),
        }
    );
    // Two goals, or a style without a direction, are plain method calls.
    assert!(matches!(&body[1].kind, BlockKind::CallMethod { method, .. } if method == "Play"));
    assert!(matches!(&body[2].kind, BlockKind::CallMethod { method, .. } if method == "Play"));
    assert_eq!(translate(&blocks).unwrap().source.lines().count(), 7);
}

#[test]
fn a_misspelled_easing_or_edge_is_reported() {
    let script = |kind| started(vec![block(2, kind)]);
    let tween = BlockKind::Tween {
        target: "Pad".to_owned(),
        property: "Transparency".to_owned(),
        value: seconds(1.0),
        seconds: seconds(1.0),
        style: "Quadratic".to_owned(),
        direction: "Out".to_owned(),
    };
    assert_eq!(
        translate(&script(tween)).unwrap_err().to_string(),
        "`Quadratic` isn't an easing style; choose from Linear, Sine, Quad, Cubic, Back, Bounce, Elastic"
    );
    let slide = BlockKind::SlideIn {
        target: "Hud.Menu".to_owned(),
        from: "Up".to_owned(),
        seconds: seconds(1.0),
    };
    assert_eq!(
        translate(&script(slide)).unwrap_err().to_string(),
        "`Up` isn't an edge; choose from Left, Right, Top, Bottom"
    );
}

#[test]
fn the_easing_menus_offer_engine_enum_items() {
    let lines: Vec<String> = EASING_STYLES
        .iter()
        .map(|name| format!("assert(Enum.EasingStyle.{name})"))
        .chain(
            EASING_DIRECTIONS
                .iter()
                .map(|name| format!("assert(Enum.EasingDirection.{name})")),
        )
        .collect();
    let (_, mut runtime, _, _) = run(&lines.join(
        "
",
    ));
    assert!(runtime.take_output().is_empty());
}
