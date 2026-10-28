//! The character Insert > Character makes, which is also each player's
//! character when `StarterPlayer.StarterCharacter` names no prefab (Phase
//! 47), so the editor and the player build the same one.

use axcenity_core::{ClassName, Color3, InstanceId, Value, Vector3, World};

/// The Animator states Insert > Character's state machine has, each with an
/// empty clip to replace with the character's own.
pub const CHARACTER_STATES: [&str; 6] = ["Idle", "Walk", "Run", "Jump", "Fall", "Sit"];

/// The character Insert > Character makes, with no parent yet: a Model
/// named Character whose feet are at `position`, facing -Z, of six parts
/// (Torso, Head, Left Arm, Right Arm, Left Leg and Right Leg, 5 studs tall)
/// welded to the Torso, with a Humanoid, a ThirdPersonController, a Health,
/// an Inventory and an Animator playing `state_machine` (empty for none).
pub fn character(world: &mut World, position: Vector3, state_machine: &str) -> InstanceId {
    let model = world.create(ClassName::Model).expect("not a service");
    world.set_name(model, "Character").expect("just made");
    let mut make = |class, name: &str, parent: InstanceId, properties: Vec<(&str, Value)>| {
        let id = world.create(class).expect("not a service");
        world.set_name(id, name).expect("just made");
        for (property, value) in properties {
            world
                .set_property(id, property, value)
                .expect("the class has it");
        }
        world.set_parent(id, Some(parent)).expect("a new Model");
        id
    };
    let skin = Color3::new(0.96, 0.8, 0.41);
    let shirt = Color3::new(0.29, 0.45, 0.78);
    let trousers = Color3::new(0.2, 0.25, 0.35);
    let part = |size: (f32, f32, f32), at: (f32, f32, f32), color: Color3| {
        vec![
            ("Size", Value::Vector3(Vector3::new(size.0, size.1, size.2))),
            (
                "Position",
                Value::Vector3(Vector3::new(
                    position.x + at.0,
                    position.y + at.1,
                    position.z + at.2,
                )),
            ),
            ("Color", Value::Color3(color)),
        ]
    };
    let torso = make(
        ClassName::Part,
        "Torso",
        model,
        part((2.0, 2.0, 1.0), (0.0, 3.0, 0.0), shirt),
    );
    let limbs = [
        ("Head", (2.0, 1.0, 1.0), (0.0, 4.5, 0.0), skin),
        ("Left Arm", (1.0, 2.0, 1.0), (-1.5, 3.0, 0.0), skin),
        ("Right Arm", (1.0, 2.0, 1.0), (1.5, 3.0, 0.0), skin),
        ("Left Leg", (1.0, 2.0, 1.0), (-0.5, 1.0, 0.0), trousers),
        ("Right Leg", (1.0, 2.0, 1.0), (0.5, 1.0, 0.0), trousers),
    ];
    for (name, size, at, color) in limbs {
        let limb = make(ClassName::Part, name, model, part(size, at, color));
        make(
            ClassName::WeldConstraint,
            &format!("{name}Weld"),
            torso,
            vec![
                ("Part0", Value::Instance(Some(torso))),
                ("Part1", Value::Instance(Some(limb))),
            ],
        );
    }
    make(ClassName::Humanoid, "Humanoid", model, Vec::new());
    make(
        ClassName::ThirdPersonController,
        "ThirdPersonController",
        model,
        Vec::new(),
    );
    make(ClassName::Health, "Health", model, Vec::new());
    make(ClassName::Inventory, "Inventory", model, Vec::new());
    make(
        ClassName::Animator,
        "Animator",
        model,
        vec![("StateMachine", Value::String(state_machine.to_owned()))],
    );
    model
}
