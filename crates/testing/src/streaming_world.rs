//! The streaming world, for Phase 80's frame budget: a 4 × 4 km map with
//! [`TOWN_PARTS`] anchored parts and world streaming on.
//! `axcenity-test --make-streaming <folder>` writes it; every host's
//! `--bench` measures it like S10k (the server's `--bench-clients` counts
//! what streaming each joined player costs).
//!
//! - The ground is [`GROUND_TILES`] tiles of 500 studs, each streaming on
//!   its own.
//! - The towns are Models of 8 anchored parts, each Model a little group in
//!   its own place, spread over the whole map, as in the player's
//!   `bench_50000_parts_streaming`.
//! - [`CRATES`] unanchored crates stand on the ground across the map, so
//!   far physics has something to put to sleep.
//! - `tour.server.lua` moves every player's character to the next stop of
//!   a tour of the map every [`TOUR_SECONDS`], asking for the region first
//!   with `RequestStreamAroundAsync`, so what each device has keeps
//!   changing.
//!
//! Built the same way every time (no randomness), so runs compare.

use std::fs;
use std::io;
use std::path::Path;

use axcenity_core::{ClassName, Color3, InstanceId, Value, Vector3, World};

/// The scene's name, also its project's name.
pub const NAME: &str = "Streaming world";
/// The map is this many studs to a side, centered on the origin.
pub const SIDE: f32 = 4_000.0;
pub const TOWN_PARTS: usize = 50_000;
pub const PARTS_PER_TOWN: usize = 8;
pub const CRATES: usize = 1_000;
/// How far around each player things arrive (`StreamingTargetRadius`).
pub const TARGET_RADIUS: f64 = 512.0;
/// Seconds between the tour's stops.
pub const TOUR_SECONDS: f64 = 5.0;

const TILE: f32 = 500.0;
const GROUND_TILES: usize = 64;
/// A town's parts are scattered over a square this many studs wide.
const TOWN_WIDTH: f32 = 40.0;

/// The tour, a lap of the map's corners and middle.
const TOUR: &str = r#"-- Streaming world: every player is taken around the map, a stop every
-- few seconds. Asking for the region first means the character lands on
-- loaded ground.
local Players = game:GetService("Players")
local STOPS = {
	Vector3.new(0, 4, 0),
	Vector3.new(1500, 4, 1500),
	Vector3.new(-1500, 4, 1500),
	Vector3.new(-1500, 4, -1500),
	Vector3.new(1500, 4, -1500),
}

local function tour(player)
	local stop = 1
	while player.Parent do
		task.wait(TOUR_SECONDS)
		stop = stop % #STOPS + 1
		local there = STOPS[stop]
		player:RequestStreamAroundAsync(there)
		local character = player.Character
		if character then
			character:PivotTo(CFrame.new(there))
		end
	end
end

Players.PlayerAdded:Connect(function(player)
	task.spawn(tour, player)
end)
for _, player in Players:GetPlayers() do
	task.spawn(tour, player)
end
"#;

/// Writes the streaming world's project into `folder`, replacing the scene
/// files of one written there before.
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
    axcenity_project::save_world(folder, &world()).map_err(io::Error::other)?;
    fs::write(
        scripts.join("tour.server.lua"),
        TOUR.replace("TOUR_SECONDS", &TOUR_SECONDS.to_string()),
    )?;
    Ok(())
}

/// The scene's world.
pub fn world() -> World {
    let mut world = World::new();
    let workspace = world.workspace();
    for (name, value) in [
        ("StreamingEnabled", Value::Bool(true)),
        ("StreamingTargetRadius", Value::Number(TARGET_RADIUS)),
    ] {
        world.set_property(workspace, name, value).unwrap();
    }

    // The ground, in tiles that stream on their own.
    let ground = add(&mut world, ClassName::Folder, workspace, "Ground", &[]);
    let per_side = (SIDE / TILE) as usize;
    debug_assert_eq!(per_side * per_side, GROUND_TILES);
    for index in 0..GROUND_TILES {
        let (column, row) = (index % per_side, index / per_side);
        let shade = if (column + row) % 2 == 0 { 0.42 } else { 0.38 };
        add(
            &mut world,
            ClassName::Part,
            ground,
            &format!("Tile{index}"),
            &[
                ("Anchored", Value::Bool(true)),
                (
                    "Position",
                    vector(
                        -SIDE / 2.0 + TILE / 2.0 + column as f32 * TILE,
                        -0.5,
                        -SIDE / 2.0 + TILE / 2.0 + row as f32 * TILE,
                    ),
                ),
                ("Size", vector(TILE, 1.0, TILE)),
                ("Color", Value::Color3(Color3::new(shade, 0.5, shade))),
            ],
        );
    }

    // The towns: Models of 8 parts, each Model somewhere on the map.
    let mut random = Random(7);
    let mut town = None;
    let mut corner = (0.0, 0.0);
    for index in 0..TOWN_PARTS {
        if index % PARTS_PER_TOWN == 0 {
            let number = index / PARTS_PER_TOWN;
            town = Some(add(
                &mut world,
                ClassName::Model,
                workspace,
                &format!("Town{number}"),
                &[],
            ));
            // The town's near corner, so all of it is on the map.
            let reach = SIDE - TOWN_WIDTH;
            corner = (
                random.next() * reach - SIDE / 2.0,
                random.next() * reach - SIDE / 2.0,
            );
        }
        let height = 4.0 + (index * 7 % 9) as f32;
        let hue = (index % 24) as f32 / 24.0;
        add(
            &mut world,
            ClassName::Part,
            town.expect("made on the first part"),
            &format!("P{}", index % PARTS_PER_TOWN),
            &[
                ("Anchored", Value::Bool(true)),
                (
                    "Position",
                    vector(
                        corner.0 + random.next() * TOWN_WIDTH,
                        height / 2.0,
                        corner.1 + random.next() * TOWN_WIDTH,
                    ),
                ),
                ("Size", vector(6.0, height, 6.0)),
                (
                    "Color",
                    Value::Color3(Color3::new(0.5 + hue / 2.0, 0.6, 1.0 - hue / 2.0)),
                ),
            ],
        );
    }

    // Loose crates on the ground, each streaming on its own.
    let crates = add(&mut world, ClassName::Folder, workspace, "Crates", &[]);
    for index in 0..CRATES {
        let reach = SIDE - 20.0;
        add(
            &mut world,
            ClassName::Part,
            crates,
            &format!("Crate{index}"),
            &[
                (
                    "Position",
                    vector(
                        random.next() * reach - reach / 2.0,
                        1.0,
                        random.next() * reach - reach / 2.0,
                    ),
                ),
                ("Size", vector(2.0, 2.0, 2.0)),
                ("Color", Value::Color3(Color3::new(0.7, 0.5, 0.3))),
            ],
        );
    }

    // Where characters start: the middle of the map.
    add(
        &mut world,
        ClassName::SpawnLocation,
        workspace,
        "SpawnLocation",
        &[
            ("Anchored", Value::Bool(true)),
            ("Position", vector(0.0, 0.5, 0.0)),
            ("Size", vector(6.0, 1.0, 6.0)),
        ],
    );
    world
}

/// One line saying what the scene holds.
pub fn summary() -> String {
    format!(
        "{NAME}: {TOWN_PARTS} anchored parts in Models of {PARTS_PER_TOWN} and {CRATES} crates \
         over {} × {} km, streaming on (target radius {TARGET_RADIUS} studs)",
        SIDE / 1000.0,
        SIDE / 1000.0
    )
}

/// A small fixed-seed generator (a 64-bit LCG), so the map is the same
/// every time without a dependency.
struct Random(u64);

impl Random {
    /// The next number in `0.0..1.0`.
    fn next(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
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
    fn the_streaming_world_opens_with_streaming_on_and_every_part_on_the_map() {
        let folder = std::env::temp_dir().join("axcenity-streaming-world");
        let _ = fs::remove_dir_all(&folder);
        write(&folder).unwrap();
        let project = axcenity_project::load(&folder).unwrap();
        assert!(project.problems.is_empty(), "{:?}", project.problems);
        let world = &project.world;
        let workspace = world.workspace();
        let settings = axcenity_core::streaming::Settings::of(world).expect("streaming on");
        assert_eq!(settings.target_radius, TARGET_RADIUS as f32);

        let mut town_parts = 0;
        let mut towns = 0;
        for &child in world.get(workspace).unwrap().children() {
            let child = world.get(child).unwrap();
            if child.class() != ClassName::Model {
                continue;
            }
            towns += 1;
            for &part in child.children() {
                let position = world
                    .get(part)
                    .and_then(|part| part.property("Position"))
                    .and_then(Value::as_vector3)
                    .unwrap();
                assert!(position.x.abs() < SIDE / 2.0 && position.z.abs() < SIDE / 2.0);
                town_parts += 1;
            }
        }
        assert_eq!(town_parts, TOWN_PARTS);
        assert_eq!(towns, TOWN_PARTS / PARTS_PER_TOWN);
        let count = |name: &str| {
            let folder = world.find_first_child(workspace, name).unwrap();
            world.get(folder).unwrap().children().len()
        };
        assert_eq!(count("Ground"), GROUND_TILES);
        assert_eq!(count("Crates"), CRATES);
        let _ = fs::remove_dir_all(&folder);
    }
}
