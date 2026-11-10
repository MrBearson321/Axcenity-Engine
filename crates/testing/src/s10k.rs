//! S10k, the standard measurement scene (roadmap item I1,
//! `docs/development/benchmarks.md`). Every host benchmarks the same
//! project, written to a folder by [`write`]:
//!
//! - 10,000 parts: 9,000 anchored (a floor, 8,899 grid blocks and 100
//!   spinners) and 1,000 unanchored drops falling onto them. A script
//!   lifts 100 settled drops back up every second, so about the same
//!   number are always moving.
//! - 200 Scripts: 100 turn a spinner every `Heartbeat`, 97 loop on
//!   `task.wait` reading and recoloring a grid block, and three drive the
//!   drops, the Sounds and the agents.
//! - 50 looping Sounds, 20 animated characters (hello-animation's Noodle,
//!   each with an Animator) and 10 navigation agents walking back and
//!   forth across the grid.
//!
//! The scene is built the same way every time (no randomness), so runs
//! can be compared.

use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::Path;

use axcenity_core::{ClassName, Color3, InstanceId, PartType, Value, Vector3, World};

/// The scene's name, also its project's name.
pub const NAME: &str = "S10k";
/// How many anchored parts, unanchored parts and Scripts it has.
pub const ANCHORED_PARTS: usize = 9_000;
pub const UNANCHORED_PARTS: usize = 1_000;
pub const SCRIPTS: usize = 200;
pub const SOUNDS: usize = 50;
pub const CHARACTERS: usize = 20;
pub const AGENTS: usize = 10;

const SPINNERS: usize = 100;
const GRID: usize = ANCHORED_PARTS - SPINNERS - 1;
const WAIT_SCRIPTS: usize = SCRIPTS - SPINNERS - 3;
/// Grid blocks are this far apart, in a square this many to a side.
const SPACING: f32 = 4.0;
const SIDE: usize = 95;

/// hello-animation's character and its clips, so the scene needs no
/// other folder.
const ASSETS: &[(&str, &[u8])] = &[
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
    (
        "assets/audio/chime.wav",
        include_bytes!("../../../examples/hello-audio/assets/audio/chime.wav"),
    ),
];

/// Writes the S10k project into `folder`, replacing the scene files of
/// any S10k written there before.
pub fn write(folder: &Path) -> io::Result<()> {
    let scripts = folder.join("scripts");
    // Only the files this writes: an old run's scripts would run too.
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
    for (name, source) in script_files() {
        fs::write(scripts.join(name), source)?;
    }
    Ok(())
}

/// The scene's world.
pub fn world() -> World {
    let mut world = World::new();
    let workspace = world.workspace();
    add(
        &mut world,
        ClassName::Part,
        workspace,
        "Floor",
        &[
            ("Anchored", Value::Bool(true)),
            ("Position", vector(0.0, -0.5, 0.0)),
            ("Size", vector(440.0, 1.0, 440.0)),
            ("Color", Value::Color3(Color3::new(0.35, 0.37, 0.4))),
        ],
    );

    // The grid: blocks of a few heights and shapes, and the spinners in
    // its middle rows.
    let grid = add(&mut world, ClassName::Folder, workspace, "Grid", &[]);
    let spinners = add(&mut world, ClassName::Folder, workspace, "Spinners", &[]);
    let shapes = [PartType::Block, PartType::Cylinder, PartType::Wedge];
    for index in 0..GRID + SPINNERS {
        let (column, row) = (index % SIDE, index / SIDE);
        let x = column as f32 * SPACING - (SIDE as f32 - 1.0) * SPACING / 2.0;
        let z = row as f32 * SPACING - (SIDE as f32 - 1.0) * SPACING / 2.0;
        let height = 1.0 + (index * 7 % 5) as f32;
        let hue = (index % 8) as f32 / 8.0;
        let spinner = index >= GRID;
        let (parent, name) = if spinner {
            (spinners, format!("S{}", index - GRID))
        } else {
            (grid, format!("G{index}"))
        };
        add(
            &mut world,
            ClassName::Part,
            parent,
            &name,
            &[
                ("Anchored", Value::Bool(true)),
                ("Position", vector(x, height / 2.0, z)),
                ("Size", vector(2.0, height, 2.0)),
                (
                    "Shape",
                    Value::PartType(if spinner {
                        PartType::Block
                    } else {
                        shapes[index % shapes.len()]
                    }),
                ),
                (
                    "Color",
                    Value::Color3(Color3::new(hue, 1.0 - hue, 0.5 + hue / 2.0)),
                ),
            ],
        );
    }

    // The drops, in a 10 × 10 × 10 block over the middle of the grid.
    let drops = add(&mut world, ClassName::Folder, workspace, "Drops", &[]);
    for index in 0..UNANCHORED_PARTS {
        let (x, y, z) = (index % 10, index / 100, index / 10 % 10);
        add(
            &mut world,
            ClassName::Part,
            drops,
            &format!("D{index}"),
            &[
                (
                    "Position",
                    vector(
                        x as f32 * 3.0 - 13.5,
                        12.0 + y as f32 * 3.0,
                        z as f32 * 3.0 - 13.5,
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
                ("Color", Value::Color3(Color3::new(0.95, 0.6, 0.2))),
            ],
        );
    }

    // Where the test runner's and player's character starts: a corner
    // clear of the grid.
    add(
        &mut world,
        ClassName::SpawnLocation,
        workspace,
        "SpawnLocation",
        &[
            ("Anchored", Value::Bool(true)),
            ("Position", vector(205.0, 0.5, 205.0)),
            ("Size", vector(6.0, 1.0, 6.0)),
        ],
    );

    // Looping Sounds, scattered over the grid; `sounds.server.lua` plays them.
    let sounds = add(&mut world, ClassName::Folder, workspace, "Sounds", &[]);
    for index in 0..SOUNDS {
        let holder = add(
            &mut world,
            ClassName::Part,
            sounds,
            &format!("Speaker{index}"),
            &[
                ("Anchored", Value::Bool(true)),
                ("CanCollide", Value::Bool(false)),
                ("Transparency", Value::Number(1.0)),
                (
                    "Position",
                    vector(
                        (index % 10) as f32 * 36.0 - 162.0,
                        8.0,
                        (index / 10) as f32 * 72.0 - 144.0,
                    ),
                ),
                ("Size", vector(1.0, 1.0, 1.0)),
            ],
        );
        add(
            &mut world,
            ClassName::Sound,
            holder,
            "Sound",
            &[
                ("SoundId", Value::String("assets/audio/chime.wav".into())),
                ("Looped", Value::Bool(true)),
                ("Volume", Value::Number(0.02)),
            ],
        );
    }

    // Animated characters, in a row along one edge, clear of the agents'
    // paths.
    let characters = add(&mut world, ClassName::Folder, workspace, "Characters", &[]);
    for index in 0..CHARACTERS {
        let noodle = add(
            &mut world,
            ClassName::MeshPart,
            characters,
            &format!("Noodle{index}"),
            &[
                ("Anchored", Value::Bool(true)),
                ("MeshId", Value::String("assets/models/noodle.fbx".into())),
                ("Position", vector(205.0, 3.8, index as f32 * 8.0 - 76.0)),
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

    // Navigation agents along the other edge; `agents.server.lua` sends
    // them across and back.
    let agents = add(&mut world, ClassName::Folder, workspace, "Agents", &[]);
    for index in 0..AGENTS {
        let model = add(
            &mut world,
            ClassName::Model,
            agents,
            &format!("Agent{index}"),
            &[],
        );
        let x = index as f32 * 16.0 - 72.0;
        add(
            &mut world,
            ClassName::Part,
            model,
            "Body",
            &[
                ("Position", vector(x, 2.5, -205.0)),
                ("Size", vector(2.0, 5.0, 1.0)),
                ("Color", Value::Color3(Color3::new(0.78, 0.3, 0.29))),
            ],
        );
        add(
            &mut world,
            ClassName::NavigationAgent,
            model,
            "NavigationAgent",
            &[
                ("Destination", vector(x, 0.0, 205.0)),
                ("WalkSpeed", Value::Number(12.0)),
            ],
        );
    }
    world
}

/// Every script file's name and source: [`SCRIPTS`] of them.
pub fn script_files() -> Vec<(String, String)> {
    let mut files = Vec::with_capacity(SCRIPTS);
    for index in 0..SPINNERS {
        let speed = 30 + index % 7 * 20;
        files.push((
            format!("spin{index:03}.server.lua"),
            format!(
                "-- S10k: turns a spinner every Heartbeat.\n\
                 local RunService = game:GetService(\"RunService\")\n\
                 local part = workspace.Spinners:FindFirstChild(\"S{index}\")\n\
                 local angle = 0\n\
                 RunService.Heartbeat:Connect(function(deltaTime)\n\
                 \x20   angle = (angle + {speed} * deltaTime) % 360\n\
                 \x20   part.Orientation = Vector3.new(0, angle, 0)\n\
                 end)\n"
            ),
        ));
    }
    for index in 0..WAIT_SCRIPTS {
        let block = index * (GRID / WAIT_SCRIPTS);
        let wait = 0.1 + (index % 5) as f32 * 0.1;
        files.push((
            format!("blink{index:03}.server.lua"),
            format!(
                "-- S10k: wakes every {wait:.1} s, reads a grid block and recolors it.\n\
                 local part = workspace.Grid:FindFirstChild(\"G{block}\")\n\
                 local base = part.Color\n\
                 local count = 0\n\
                 while true do\n\
                 \x20   task.wait({wait:.1})\n\
                 \x20   count += 1\n\
                 \x20   local height = part.Position.Y\n\
                 \x20   if count % 2 == 0 and height > 0 then\n\
                 \x20       part.Color = base\n\
                 \x20   else\n\
                 \x20       part.Color = Color3.new(1, 0.5, 0)\n\
                 \x20   end\n\
                 end\n"
            ),
        ));
    }
    files.push((
        "drops.server.lua".to_owned(),
        "-- S10k: lifts 100 drops back up every second, so some always fall.\n\
         local drops = workspace.Drops:GetChildren()\n\
         local next = 1\n\
         while true do\n\
         \x20   task.wait(1)\n\
         \x20   for _ = 1, 100 do\n\
         \x20       local drop = drops[next]\n\
         \x20       local x = (next % 10) * 3 - 13.5\n\
         \x20       local z = (math.floor(next / 10) % 10) * 3 - 13.5\n\
         \x20       drop.Position = Vector3.new(x, 40, z)\n\
         \x20       next = next % #drops + 1\n\
         \x20   end\n\
         end\n"
            .to_owned(),
    ));
    files.push((
        "sounds.server.lua".to_owned(),
        "-- S10k: starts every looping Sound.\n\
         for _, speaker in workspace.Sounds:GetChildren() do\n\
         \x20   speaker.Sound:Play()\n\
         end\n"
            .to_owned(),
    ));
    files.push((
        "agents.server.lua".to_owned(),
        "-- S10k: sends every agent across the grid and back, every 8 s.\n\
         local agents = workspace.Agents:GetChildren()\n\
         local across = true\n\
         while true do\n\
         \x20   task.wait(8)\n\
         \x20   across = not across\n\
         \x20   for _, agent in agents do\n\
         \x20       local x = agent.Body.Position.X\n\
         \x20       agent.NavigationAgent.Destination = Vector3.new(x, 0, if across then 205 else -205)\n\
         \x20   end\n\
         end\n"
            .to_owned(),
    ));
    debug_assert_eq!(files.len(), SCRIPTS);
    files
}

/// A one-line description of the scene, for reports.
pub fn summary() -> String {
    let mut text = String::new();
    let _ = write!(
        text,
        "{NAME}: {ANCHORED_PARTS} anchored + {UNANCHORED_PARTS} unanchored parts, \
         {SCRIPTS} scripts, {SOUNDS} Sounds, {CHARACTERS} animated characters, {AGENTS} agents"
    );
    text
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
