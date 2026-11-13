//! Tests for R17.1: `NumberSequence`, `ColorSequence` and their keypoints
//! from Lua, on a ParticleEmitter's OverLife properties.

use axcenity_core::{
    ClassName, Color3, ColorSequence, NumberKeypoint, NumberSequence, Value, World,
};

use super::*;

fn run(world: &mut World, source: &str) -> Vec<String> {
    let mut runtime = ScriptRuntime::new();
    let script = world.create(ClassName::Script).unwrap();
    world
        .set_parent(script, Some(world.server_script_service()))
        .unwrap();
    runtime.start_script(world, script, "scripts/test.server.lua", source);
    runtime
        .take_output()
        .into_iter()
        .map(|line| match line.kind {
            OutputKind::Error => format!("error: {}", line.text),
            _ => line.text.replace('\t', " "),
        })
        .collect()
}

fn world_with_emitter() -> (World, axcenity_core::InstanceId) {
    let mut world = World::new();
    let emitter = world.create(ClassName::ParticleEmitter).unwrap();
    world.set_name(emitter, "Smoke").unwrap();
    world.set_parent(emitter, Some(world.workspace())).unwrap();
    (world, emitter)
}

#[test]
fn scripts_make_sequences_and_set_them_on_an_emitter() {
    let (mut world, emitter) = world_with_emitter();
    let lines = run(
        &mut world,
        r#"
        local smoke = workspace.Smoke
        print(smoke.SizeOverLife == nil, smoke.ColorOverLife == nil)
        smoke.SizeOverLife = NumberSequence.new({
            NumberSequenceKeypoint.new(0, 1),
            NumberSequenceKeypoint.new(0.5, 4, 0.5),
            NumberSequenceKeypoint.new(1, 0),
        })
        smoke.TransparencyOverLife = NumberSequence.new(0.2, 1)
        smoke.ColorOverLife = ColorSequence.new(Color3.new(1, 0, 0), Color3.new(0, 0, 1))
        local points = smoke.SizeOverLife.Keypoints
        print(#points, points[2].Time, points[2].Value, points[2].Envelope)
        print(smoke.ColorOverLife.Keypoints[2].Value == Color3.new(0, 0, 1))
        print(typeof(smoke.SizeOverLife), typeof(points[1]), typeof(smoke.ColorOverLife))
        print(NumberSequence.new(3) == NumberSequence.new(3, 3))
        "#,
    );
    assert_eq!(
        lines,
        [
            "true true",
            "3 0.5 4 0.5",
            "true",
            "NumberSequence NumberSequenceKeypoint ColorSequence",
            "true"
        ]
    );
    let instance = world.get(emitter).unwrap();
    assert_eq!(
        instance.property("SizeOverLife"),
        Some(&Value::NumberSequence(
            NumberSequence::new(vec![
                NumberKeypoint::new(0.0, 1.0, 0.0),
                NumberKeypoint::new(0.5, 4.0, 0.5),
                NumberKeypoint::new(1.0, 0.0, 0.0),
            ])
            .unwrap()
        ))
    );
    assert_eq!(
        instance.property("TransparencyOverLife"),
        Some(&Value::NumberSequence(NumberSequence::ramp(0.2, 1.0)))
    );
    assert_eq!(
        instance.property("ColorOverLife"),
        Some(&Value::ColorSequence(ColorSequence::ramp(
            Color3::new(1.0, 0.0, 0.0),
            Color3::new(0.0, 0.0, 1.0)
        )))
    );
    // nil unsets them again.
    let lines = run(
        &mut world,
        "workspace.Smoke.SizeOverLife = nil\nprint(workspace.Smoke.SizeOverLife)",
    );
    assert_eq!(lines, ["nil"]);
}

#[test]
fn a_sequence_out_of_order_or_not_from_0_to_1_is_an_error() {
    let (mut world, _) = world_with_emitter();
    let lines = run(
        &mut world,
        r#"
        local ok, message = pcall(NumberSequence.new, {
            NumberSequenceKeypoint.new(0, 1),
            NumberSequenceKeypoint.new(0.5, 2),
        })
        print(ok, message)
        ok, message = pcall(ColorSequence.new, {
            ColorSequenceKeypoint.new(0, Color3.new()),
            ColorSequenceKeypoint.new(0.7, Color3.new()),
            ColorSequenceKeypoint.new(0.3, Color3.new()),
            ColorSequenceKeypoint.new(1, Color3.new()),
        })
        print(ok, message)
        ok, message = pcall(function()
            workspace.Smoke.SizeOverLife = 3
        end)
        print(ok)
        "#,
    );
    assert_eq!(lines.len(), 3, "{lines:?}");
    assert!(
        lines[0].starts_with("false") && lines[0].contains("start at time 0 and end at time 1"),
        "{lines:?}"
    );
    assert!(
        lines[1].starts_with("false") && lines[1].contains("in time order"),
        "{lines:?}"
    );
    assert_eq!(lines[2], "false");
}
