//! Facial animation F7b: what a server script sets on a face reaches
//! players' devices, by the face's choice and no more often than its rate.

use axcenity_core::face::{FaceBlend, FaceLayer, LipSyncPlay, face_control};
use axcenity_core::{ClassName, InstanceId, Value, World};

use crate::replicate::{Replicator, apply};
use crate::replication_tests::{Device, frames, three_players};

/// A MeshPart "Hero" in the Workspace with a FaceControls "Face", made by
/// a server script that keeps it as `_G.face`.
fn hero_face(server: &mut Device) -> InstanceId {
    server
        .scripts
        .run_command(
            &mut server.world,
            r#"
local hero = Instance.new("MeshPart")
hero.Name = "Hero"
local face = Instance.new("FaceControls")
face.Name = "Face"
face.Parent = hero
hero.Parent = workspace
_G.face = face
"#,
        )
        .unwrap();
    let world = &server.world;
    let hero = world.find_first_child(world.workspace(), "Hero").unwrap();
    world.find_first_child(hero, "Face").unwrap()
}

fn run(server: &mut Device, code: &str) {
    server.scripts.run_command(&mut server.world, code).unwrap();
}

#[test]
fn expressions_overrides_layers_and_lip_sync_a_server_script_sets_reach_every_device() {
    let (mut server, mut players) = three_players();
    let face = hero_face(&mut server);
    run(
        &mut server,
        r#"
_G.face:SetExpression("Happy", 0.8, 0.3, { Priority = 2, Asymmetry = -0.5 })
_G.face:SetOverride("Jaw_Open", 0.5)
_G.face:SetLayer("Procedural", 0.5, "Additive", "Eyes")
"#,
    );
    // PlayLipSync checks the file in the project; there's none here.
    server.world.face_mut(face).unwrap().lip_sync = Some(LipSyncPlay {
        path: "assets/lines/hello.lipsync.json".to_owned(),
        time: 0.0,
        speed: 1.5,
        sound: None,
        heard: false,
        weight: 0.0,
        target_weight: 1.0,
        fade: 0.1,
    });
    frames(&mut server, &mut players, 10);
    let jaw = face_control("Jaw_Open").unwrap();
    for player in &players {
        let state = player.world.face(face).expect("the face's live state");
        let [happy] = state.expressions.as_slice() else {
            panic!("one expression: {:?}", state.expressions);
        };
        assert_eq!(happy.name, "Happy");
        assert!((happy.target_intensity - 0.8).abs() < 1e-6);
        assert_eq!(happy.target_presence, 1.0);
        assert_eq!(happy.presence, 0.0, "it blends in on the device");
        assert_eq!(happy.priority, 2);
        assert!((happy.transition.unwrap() - 0.3).abs() < 1e-6);
        assert_eq!(state.overrides.get(&jaw), Some(&0.5));
        let layer = state.layer(FaceLayer::Procedural);
        assert_eq!(layer.blend, FaceBlend::Additive);
        assert_eq!(layer.weight, 0.5);
        assert!(layer.mask.as_ref().is_some_and(|mask| !mask.is_empty()));
        let lip = state
            .lip_sync
            .as_ref()
            .unwrap_or_else(|| panic!("the lip sync: {:?}", server.output));
        assert_eq!(lip.path, "assets/lines/hello.lipsync.json");
        assert_eq!(lip.speed, 1.5);
        assert_eq!(lip.target_weight, 1.0);
    }

    // Cleared and stopped on the server: they fade out on the devices.
    run(
        &mut server,
        r#"
_G.face:ClearExpression("Happy")
_G.face:ClearOverrides()
_G.face:StopLipSync()
"#,
    );
    frames(&mut server, &mut players, 10);
    for player in &players {
        let state = player.world.face(face).unwrap();
        assert_eq!(state.expressions[0].target_presence, 0.0);
        assert!(state.overrides.is_empty());
        assert_eq!(state.lip_sync.as_ref().unwrap().target_weight, 0.0);
    }
}

#[test]
fn a_face_that_doesnt_replicate_a_part_leaves_it_to_each_device() {
    let (mut server, mut players) = three_players();
    let face = hero_face(&mut server);
    run(
        &mut server,
        r#"
_G.face.ReplicateExpressions = false
_G.face:SetExpression("Angry")
_G.face:SetOverride("Jaw_Open", 0.25)
"#,
    );
    // A device's own expression stays its own.
    frames(&mut server, &mut players, 2);
    let first = &mut players[0];
    first
        .scripts
        .run_command(
            &mut first.world,
            r#"workspace.Hero.Face:SetExpression("Happy")"#,
        )
        .unwrap();
    frames(&mut server, &mut players, 10);
    let state = players[0].world.face(face).unwrap();
    let names: Vec<&str> = state.expressions.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Happy"]);
    assert_eq!(state.overrides.len(), 1, "overrides still go");
    assert!(players[1].world.face(face).unwrap().expressions.is_empty());
}

#[test]
fn a_faces_changes_wait_for_its_rate_and_its_lip_sync_keeps_time() {
    let mut world = World::new();
    let face = world.create(ClassName::FaceControls).unwrap();
    world.set_parent(face, Some(world.workspace())).unwrap();
    world
        .set_property(face, "ReplicationRate", Value::Number(4.0))
        .unwrap();
    let mut replicator = Replicator::default();
    let faces = |update: &crate::Update| update.faces.len();
    // Nothing set on it: nothing to send.
    assert_eq!(faces(&replicator.frame_at(&world, Some(0.0))), 0);

    let set_override = |world: &mut World, value: f64| {
        world.face_mut(face).unwrap().overrides.insert(0, value);
    };
    set_override(&mut world, 0.5);
    assert_eq!(faces(&replicator.frame_at(&world, Some(0.0))), 1);
    // A quarter of a second between sends: this waits, then goes.
    set_override(&mut world, 0.75);
    assert_eq!(faces(&replicator.frame_at(&world, Some(0.1))), 0);
    assert_eq!(faces(&replicator.frame_at(&world, Some(0.2))), 0);
    let update = replicator.frame_at(&world, Some(0.25));
    assert_eq!(update.faces[0].overrides, Some(vec![(0, 0.75)]));
    assert_eq!(update.faces[0].lip_sync, None, "no lip sync: said already");
    assert_eq!(faces(&replicator.frame_at(&world, Some(1.0))), 0);

    // A lip sync goes when it starts and once a second as it plays.
    world.face_mut(face).unwrap().lip_sync = Some(LipSyncPlay {
        path: "a.lipsync.json".to_owned(),
        time: 0.0,
        speed: 1.0,
        sound: None,
        heard: false,
        weight: 0.0,
        target_weight: 1.0,
        fade: 0.1,
    });
    let mut sent = Vec::new();
    for frame in 0..24 {
        let clock = 2.0 + f64::from(frame) / 8.0;
        world
            .face_mut(face)
            .unwrap()
            .lip_sync
            .as_mut()
            .unwrap()
            .time = clock - 2.0;
        for wire in replicator.frame_at(&world, Some(clock)).faces {
            sent.push(wire.lip_sync.flatten().unwrap().time);
        }
    }
    assert_eq!(sent, [0.0, 1.0, 2.0], "at its start and each second");

    // A device jumps only when it has drifted.
    world
        .face_mut(face)
        .unwrap()
        .lip_sync
        .as_mut()
        .unwrap()
        .time = 3.0;
    let mut device = world.empty_replica();
    let mut snapshot = Replicator::default();
    snapshot.frame(&world);
    apply(&mut device, &snapshot.snapshot(&world));
    let on_device = |device: &World| device.face(face).unwrap().lip_sync.clone().unwrap();
    assert_eq!(on_device(&device).time, 3.0);
    let mut nudge = |device: &mut World, device_time: f64, server_time: f64| {
        device
            .face_mut(face)
            .unwrap()
            .lip_sync
            .as_mut()
            .unwrap()
            .time = device_time;
        world
            .face_mut(face)
            .unwrap()
            .lip_sync
            .as_mut()
            .unwrap()
            .time = server_time;
        let faces = snapshot.snapshot(&world).faces;
        apply(
            device,
            &crate::Update {
                faces,
                ..Default::default()
            },
        );
        on_device(device).time
    };
    assert_eq!(nudge(&mut device, 4.05, 4.0), 4.05, "close enough");
    assert_eq!(nudge(&mut device, 4.5, 4.0), 4.0, "too far: it jumps");
}
