//! The crowd scene, for the multithreaded frame (unified Phase 93): the
//! graphics reference scene (`examples/hello-graphics`) with
//! [`CHARACTERS`] animated characters and [`BODIES`] physics bodies added.
//! `axcenity-test --make-crowd <folder>` writes it; every host's `--bench`
//! measures it like S10k.
//!
//! - The characters are hello-animation's Noodle, each with an Animator
//!   playing its state machine, in a 20 × 10 grid behind the reference
//!   objects. `crowd.server.lua` gives each a different Speed, so their
//!   poses differ.
//! - The bodies are a 20 × 10 × 10 block of unanchored parts over the
//!   floor's far half; the same script lifts 100 settled ones back up
//!   every second, so about the same number are always moving.
//!
//! Built the same way every time (no randomness), so runs compare.

use std::fs;
use std::io;
use std::path::Path;

use axcenity_core::{ClassName, Color3, InstanceId, PartType, Value, Vector3, World};

/// The scene's name, also its project's name.
pub const NAME: &str = "Crowd";
pub const CHARACTERS: usize = 200;
pub const BODIES: usize = 2_000;

/// The reference scene's world and files, and the Noodle's.
const WORLD: &str = include_str!("../../../examples/hello-graphics/world.json");
const ASSETS: &[(&str, &[u8])] = &[
    (
        "scripts/quality.client.lua",
        include_bytes!("../../../examples/hello-graphics/scripts/quality.client.lua"),
    ),
    (
        "assets/models/pbr_panel.fbx",
        include_bytes!("../../../examples/hello-graphics/assets/models/pbr_panel.fbx"),
    ),
    (
        "assets/models/maps/ao.png",
        include_bytes!("../../../examples/hello-graphics/assets/models/maps/ao.png"),
    ),
    (
        "assets/models/maps/color.png",
        include_bytes!("../../../examples/hello-graphics/assets/models/maps/color.png"),
    ),
    (
        "assets/models/maps/emissive.png",
        include_bytes!("../../../examples/hello-graphics/assets/models/maps/emissive.png"),
    ),
    (
        "assets/models/maps/gloss.png",
        include_bytes!("../../../examples/hello-graphics/assets/models/maps/gloss.png"),
    ),
    (
        "assets/models/maps/metalness.png",
        include_bytes!("../../../examples/hello-graphics/assets/models/maps/metalness.png"),
    ),
    (
        "assets/models/maps/normal.png",
        include_bytes!("../../../examples/hello-graphics/assets/models/maps/normal.png"),
    ),
    (
        "assets/models/maps/opacity.png",
        include_bytes!("../../../examples/hello-graphics/assets/models/maps/opacity.png"),
    ),
    (
        "assets/models/maps/roughness.png",
        include_bytes!("../../../examples/hello-graphics/assets/models/maps/roughness.png"),
    ),
    (
        "assets/models/maps/specular.png",
        include_bytes!("../../../examples/hello-graphics/assets/models/maps/specular.png"),
    ),
    (
        "assets/models/noodle.fbx",
        include_bytes!("../../../examples/hello-animation/assets/models/noodle.fbx"),
    ),
    (
        "assets/animations/noodle/noodle.statemachine.json",
        include_bytes!(
            "../../../examples/hello-animation/assets/animations/noodle/noodle.statemachine.json"
        ),
    ),
    (
        "assets/animations/noodle/Idle.anim.json",
        include_bytes!("../../../examples/hello-animation/assets/animations/noodle/Idle.anim.json"),
    ),
    (
        "assets/animations/noodle/Walk.anim.json",
        include_bytes!("../../../examples/hello-animation/assets/animations/noodle/Walk.anim.json"),
    ),
    (
        "assets/animations/noodle/Run.anim.json",
        include_bytes!("../../../examples/hello-animation/assets/animations/noodle/Run.anim.json"),
    ),
    (
        "assets/animations/noodle/Hop.anim.json",
        include_bytes!("../../../examples/hello-animation/assets/animations/noodle/Hop.anim.json"),
    ),
    (
        "assets/animations/noodle/Wave.anim.json",
        include_bytes!("../../../examples/hello-animation/assets/animations/noodle/Wave.anim.json"),
    ),
];

/// Gives each character its Speed and keeps the bodies falling.
const SCRIPT: &str = "-- Crowd: a Speed per character, so their poses differ, and 100 bodies\n\
-- lifted back up every second, so some always fall.\n\
for index, noodle in workspace.Crowd:GetChildren() do\n\
\x20   noodle.Animator:SetNumber(\"Speed\", (index % 9) / 2)\n\
end\n\
local bodies = workspace.Bodies:GetChildren()\n\
local next = 1\n\
while true do\n\
\x20   task.wait(1)\n\
\x20   for _ = 1, 100 do\n\
\x20       local body = bodies[next]\n\
\x20       local x = (next % 20) * 3 - 28.5\n\
\x20       local z = (math.floor(next / 20) % 10) * 3 + 40\n\
\x20       body.Position = Vector3.new(x, 50, z)\n\
\x20       next = next % #bodies + 1\n\
\x20   end\n\
end\n";

/// Writes the crowd project into `folder`, replacing the files of any
/// written there before.
pub fn write(folder: &Path) -> io::Result<()> {
    let scripts = folder.join("scripts");
    let _ = fs::remove_dir_all(&scripts);
    fs::create_dir_all(&scripts)?;
    fs::write(
        folder.join(axcenity_project::MANIFEST_FILE),
        format!(
            "{{\n  \"name\": \"{NAME}\",\n  \"format_version\": {}\n}}\n",
            axcenity_project::FORMAT_VERSION
        ),
    )?;
    for (path, bytes) in ASSETS {
        let path = folder.join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, bytes)?;
    }
    axcenity_project::save_world(folder, &world()).map_err(io::Error::other)?;
    fs::write(scripts.join("crowd.server.lua"), SCRIPT)?;
    Ok(())
}

/// The scene's world: the reference scene's, with the crowd and bodies.
pub fn world() -> World {
    let mut world = axcenity_project::parse_world(WORLD).expect("hello-graphics' world reads");
    let workspace = world.workspace();

    let crowd = add(&mut world, ClassName::Folder, workspace, "Crowd", &[]);
    for index in 0..CHARACTERS {
        let (column, row) = (index % 20, index / 20);
        let noodle = add(
            &mut world,
            ClassName::MeshPart,
            crowd,
            &format!("Noodle{index}"),
            &[
                ("Anchored", Value::Bool(true)),
                ("MeshId", Value::String("assets/models/noodle.fbx".into())),
                (
                    "Position",
                    vector(column as f32 * 4.0 - 38.0, 3.8, -30.0 - row as f32 * 5.0),
                ),
                ("Size", vector(2.0, 7.6, 2.0)),
            ],
        );
        add(
            &mut world,
            ClassName::Animator,
            noodle,
            "Animator",
            &[(
                "StateMachine",
                Value::String("assets/animations/noodle/noodle.statemachine.json".into()),
            )],
        );
    }

    let bodies = add(&mut world, ClassName::Folder, workspace, "Bodies", &[]);
    for index in 0..BODIES {
        let (x, z, y) = (index % 20, index / 20 % 10, index / 200);
        add(
            &mut world,
            ClassName::Part,
            bodies,
            &format!("B{index}"),
            &[
                (
                    "Position",
                    vector(
                        x as f32 * 3.0 - 28.5,
                        10.0 + y as f32 * 3.0,
                        z as f32 * 3.0 + 40.0,
                    ),
                ),
                ("Size", vector(1.5, 1.5, 1.5)),
                (
                    "Shape",
                    Value::PartType(if index % 2 == 0 {
                        PartType::Ball
                    } else {
                        PartType::Block
                    }),
                ),
                (
                    "Color",
                    Value::Color3(Color3::new(0.3 + (index % 5) as f32 * 0.1, 0.55, 0.85)),
                ),
            ],
        );
    }
    world
}

/// One line saying what the scene holds.
pub fn summary() -> String {
    format!(
        "{NAME}: the graphics reference scene with {CHARACTERS} animated characters and {BODIES} physics bodies"
    )
}

fn vector(x: f32, y: f32, z: f32) -> Value {
    Value::Vector3(Vector3::new(x, y, z))
}

fn add(
    world: &mut World,
    class: ClassName,
    parent: InstanceId,
    name: &str,
    properties: &[(&str, Value)],
) -> InstanceId {
    let id = world.create(class).expect("not a service");
    world.set_name(id, name).expect("a fresh Instance");
    world
        .set_parent(id, Some(parent))
        .expect("a fresh Instance");
    for (property, value) in properties {
        world
            .set_property(id, property, value.clone())
            .unwrap_or_else(|err| panic!("{class:?}.{property}: {err}"));
    }
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_crowd_scene_opens_and_runs_its_characters_and_bodies() {
        let folder = std::env::temp_dir().join("axcenity-crowd-scene");
        let _ = fs::remove_dir_all(&folder);
        write(&folder).unwrap();
        let project = axcenity_project::load(&folder).unwrap();
        assert!(project.problems.is_empty(), "{:?}", project.problems);
        let world = &project.world;
        let count = |name: &str| {
            let folder = world.find_first_child(world.workspace(), name).unwrap();
            world.get(folder).unwrap().children().len()
        };
        assert_eq!(count("Crowd"), CHARACTERS);
        assert_eq!(count("Bodies"), BODIES);
        // The reference scene is all there.
        for name in ["Floor", "Metal", "PbrPanel", "Lamp"] {
            assert!(
                world.find_first_child(world.workspace(), name).is_some(),
                "{name}"
            );
        }
        let _ = fs::remove_dir_all(&folder);
    }
}
