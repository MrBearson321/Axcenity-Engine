//! The Animator Inspector's snapshot of the `hello-animation` noodle.

use std::path::Path;

use axcenity_assets::Assets;
use axcenity_core::{ClassName, PlayRequest, Value, Vector3, World};

use crate::Animators;

#[test]
fn inspecting_shows_the_state_its_fade_what_plays_over_it_and_what_fired() {
    let assets =
        Assets::new(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-animation"));
    let mut world = World::new();
    let workspace = world.workspace();
    let model = world.create(ClassName::Model).unwrap();
    world.set_parent(model, Some(workspace)).unwrap();
    let part = world.create(ClassName::MeshPart).unwrap();
    world
        .set_property(
            part,
            "MeshId",
            Value::String("assets/models/noodle.fbx".into()),
        )
        .unwrap();
    world
        .set_property(part, "Size", Value::Vector3(Vector3::new(1.0, 3.8, 1.0)))
        .unwrap();
    world.set_parent(part, Some(model)).unwrap();
    let animator = world.create(ClassName::Animator).unwrap();
    world
        .set_property(
            animator,
            "StateMachine",
            Value::String("assets/animations/noodle/noodle.statemachine.json".into()),
        )
        .unwrap();
    world.set_parent(animator, Some(part)).unwrap();

    let mut animators = Animators::new();
    assert_eq!(
        animators.inspect(&world, part),
        None,
        "nothing has played yet"
    );
    animators.update(&mut world, &assets, 0.0);
    animators.update(&mut world, &assets, 0.25);

    // The part, its Animator and the Model around it all show the part.
    let view = animators.inspect(&world, part).unwrap();
    assert_eq!(animators.inspect(&world, animator), Some(view.clone()));
    assert_eq!(animators.inspect(&world, model), Some(view.clone()));
    assert_eq!(animators.animated(), [part]);
    assert_eq!(view.owner, part);
    assert_eq!(view.animator, Some(animator));
    assert!(view.posed);
    let machine = view.machine.as_ref().unwrap();
    assert_eq!(machine.state, "Locomotion");
    assert!(machine.fade.is_none());
    assert!(machine.duration > 0.0);
    assert!((0.0..=1.0).contains(&machine.progress) && machine.progress > 0.0);
    assert!(
        view.parameters.contains_key("Speed"),
        "{:?}",
        view.parameters
    );
    assert_eq!(view.fired[0].what, "Started in Locomotion");
    assert!((view.fired[0].seconds_ago - 0.25).abs() < 1e-9);

    // A blend to Wave shows where it came from, and the change is newest.
    world.animator_mut(animator).unwrap().play = Some(PlayRequest {
        state: "Wave".into(),
        blend: Some(0.5),
    });
    animators.update(&mut world, &assets, 0.1);
    let view = animators.inspect(&world, part).unwrap();
    let machine = view.machine.unwrap();
    assert_eq!(machine.state, "Wave");
    let fade = machine.fade.unwrap();
    assert_eq!(fade.from, "Locomotion");
    assert!(fade.weight > 0.0 && fade.weight < 1.0, "{fade:?}");
    assert_eq!(fade.seconds, 0.5);
    assert_eq!(view.fired[0].what, "Entered Wave (from Locomotion)");
    assert_eq!(view.fired.len(), 2);

    // A track over it is listed with its clip and weight.
    let animation = world.create(ClassName::Animation).unwrap();
    world
        .set_property(
            animation,
            "Clip",
            Value::String("assets/animations/noodle/Wave.anim.json".into()),
        )
        .unwrap();
    world.set_parent(animation, Some(animator)).unwrap();
    let track = world.load_animation(animator, animation).unwrap();
    let state = world.animation_track_mut(track).unwrap();
    state.playing = true;
    state.weight = 1.0;
    state.target_weight = 1.0;
    state.order = 1;
    animators.update(&mut world, &assets, 0.1);
    let view = animators.inspect(&world, part).unwrap();
    assert_eq!(view.playing.len(), 1, "{:?}", view.playing);
    assert_eq!(view.playing[0].source, track);
    assert_eq!(
        view.playing[0].clip,
        "assets/animations/noodle/Wave.anim.json"
    );
    assert_eq!(view.playing[0].weight, 1.0);
}
