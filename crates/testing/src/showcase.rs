//! The showcase scene: the owner's Overgrown Carriage project (an overgrown
//! train carriage in a forest of the Abandoned Farmhouse's trees, with fog,
//! dust and moths in the air and a light probe volume) with what it doesn't
//! use yet added, so one scene of real assets exercises every newer
//! rendering feature. `axcenity-test --make-showcase <folder> --from
//! <Overgrown Carriage> --view <view>` writes it; `axcenity-bench-check`'s
//! `showcase:<view>` scenes make it in `target/bench-projects/Showcase`.
//!
//! What's added (all in the Workspace's `Benchmark` Model or in Lighting,
//! so the carriage itself is untouched):
//! - **Lamps** under the carriage's ceiling lamps: [`LAMPS`] PointLights
//!   with `Shadows` on (shadow maps, or ray traced shadows with LE1).
//! - **A pond**: a 128-stud Terrain beside the forest with `Water` on
//!   (LE4; ray traced reflections with LE5 at reflection quality High or
//!   Ultra), ringed by [`POND_TREES`] copies of the forest's own trees and
//!   bushes at their own sizes.
//! - **Sky**: Clouds and a SunRaysEffect in Lighting.
//! - **The camera**: `scripts/view.client.lua` is replaced by one that
//!   holds one of [`VIEWS`] (or tours them all), so a run always draws the
//!   same pictures. The project's own reference view is `interior`.
//!
//! The project's files are copied, never written: the source folder is
//! only read. Built the same way every time (no randomness), so runs
//! compare. The assets aren't in this repository (they're the owner's), so
//! the scene is made on the machine that has them.

use std::fs;
use std::io;
use std::path::Path;

use axcenity_core::{ClassName, Color3, InstanceId, TerrainData, Value, Vector3, World};

/// The scene's name, also its project's name.
pub const NAME: &str = "Showcase";
/// The project it's made from, in the projects folder.
pub const SOURCE_PROJECT: &str = "Overgrown Carriage";
/// The camera views a run can hold. `tour` moves through the others.
pub const VIEWS: [&str; 4] = ["interior", "exterior", "pond", "tour"];
pub const LAMPS: usize = 6;
pub const POND_TREES: usize = 24;

/// The pond's terrain: its middle (X, Z), its footprint in studs and its
/// samples a side (one a stud).
const POND_CENTER: (f32, f32) = (-184.0, -21.0);
const POND_SIZE: f32 = 128.0;
const POND_SAMPLES: usize = 129;
/// The terrain's bottom (world Y) and height range: heights 0 to 1 span it.
const POND_BOTTOM: f32 = -10.0;
const POND_RANGE: f32 = 16.0;
/// The Ground part's top in the carriage project, where the land around
/// the pond sits.
const GROUND_TOP: f32 = -3.45;
/// The pond's radius and deepest point (world Y), and the water's level.
const POND_RADIUS: f32 = 30.0;
const POND_DEEPEST: f32 = -8.0;
const WATER_LEVEL: f32 = -4.2;

/// The camera script. `VIEW_NAME` is filled in by [`camera_script`].
const CAMERA_SCRIPT: &str = r#"-- Benchmark camera (axcenity-test --make-showcase): holds one view, or
-- tours them, the same way every run. Written over the project's own
-- view script in the benchmark's copy only.

local RunService = game:GetService("RunService")
local camera = workspace.CurrentCamera
local VIEW_NAME = "@VIEW@"

local VIEWS = {
    -- The project's reference photo viewpoint, down the carriage.
    interior = { from = Vector3.new(0.15, 3.96, -3.6), to = Vector3.new(-0.3, 2.16, -42.0), fov = 68 },
    -- Outside, three-quarters on: the carriage, ivy and the forest behind.
    exterior = { from = Vector3.new(16, 7, 10), to = Vector3.new(0, 3, -24), fov = 60 },
    -- Across the pond's water at the trees on its far bank.
    pond = { from = Vector3.new(-136, 1.5, -21), to = Vector3.new(-200, -2, -21), fov = 60 },
    -- High over the forest, the carriage and the pond.
    overview = { from = Vector3.new(40, 70, 70), to = Vector3.new(-60, -4, -21), fov = 60 },
}
-- The tour: each stop in turn, `SEGMENT` seconds from one to the next.
local TOUR = { "interior", "exterior", "overview", "pond", "interior" }
local SEGMENT = 3

camera.CameraType = Enum.CameraType.Scriptable

local function show(from, to, fov)
    camera.FieldOfView = fov
    camera.CFrame = CFrame.lookAt(from, to)
end

local function smooth(t)
    return t * t * (3 - 2 * t)
end

if VIEW_NAME ~= "tour" then
    local view = VIEWS[VIEW_NAME]
    show(view.from, view.to, view.fov)
    -- Held every frame, in case anything else moves the camera.
    RunService.RenderStepped:Connect(function()
        show(view.from, view.to, view.fov)
    end)
else
    local elapsed = 0
    RunService.RenderStepped:Connect(function(dt)
        elapsed += dt
        local span = (#TOUR - 1) * SEGMENT
        local t = elapsed % span
        local index = math.floor(t / SEGMENT) + 1
        local a, b = VIEWS[TOUR[index]], VIEWS[TOUR[index + 1]]
        local k = smooth((t - (index - 1) * SEGMENT) / SEGMENT)
        show(a.from:Lerp(b.from, k), a.to:Lerp(b.to, k), a.fov + (b.fov - a.fov) * k)
    end)
end
"#;

/// Writes the showcase into `folder` from the Overgrown Carriage project in
/// `source`, holding `view` (one of [`VIEWS`]). Files already copied with
/// the same size are left as they are, so switching views is quick; the
/// world, the camera script and the pond's terrain are written each time.
pub fn write(folder: &Path, source: &Path, view: &str) -> io::Result<()> {
    if !VIEWS.contains(&view) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unknown view `{view}`: use one of {}", VIEWS.join(", ")),
        ));
    }
    let text = fs::read_to_string(source.join(axcenity_project::WORLD_FILE))?;
    let mut world = axcenity_project::parse_world(&text).map_err(io::Error::other)?;
    copy_new_files(source, folder)?;
    fs::write(
        folder.join(axcenity_project::MANIFEST_FILE),
        format!(
            "{{\n  \"name\": \"{NAME}\",\n  \"format_version\": {}\n}}\n",
            axcenity_project::FORMAT_VERSION
        ),
    )?;
    add_benchmark(&mut world)?;
    axcenity_project::save_world(folder, &world).map_err(io::Error::other)?;
    axcenity_project::save_terrain(folder, POND_FOLDER, &pond_terrain()?)
        .map_err(io::Error::other)?;
    fs::create_dir_all(folder.join("scripts"))?;
    fs::write(
        folder.join("scripts").join("view.client.lua"),
        camera_script(view),
    )?;
    Ok(())
}

/// The camera script holding `view`.
pub fn camera_script(view: &str) -> String {
    CAMERA_SCRIPT.replace("@VIEW@", view)
}

/// One line saying what the scene holds.
pub fn summary(view: &str) -> String {
    format!(
        "{NAME} ({view}): {SOURCE_PROJECT} with {LAMPS} shadow-casting lamps, a pond with \
         terrain water ringed by {POND_TREES} trees, clouds and sun rays"
    )
}

/// The pond's terrain files, inside the project.
const POND_FOLDER: &str = "assets/terrain/BenchmarkPond";

/// Adds the lamps, the pond and its trees, and the sky to the carriage's
/// world.
fn add_benchmark(world: &mut World) -> io::Result<()> {
    let workspace = world.workspace();
    let missing = |what: &str| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("the {SOURCE_PROJECT} world has no {what}"),
        )
    };
    let forest = world
        .find_first_child(workspace, "Forest")
        .ok_or_else(|| missing("Forest"))?;
    // One of each tree and bush the forest has, to copy at its own size.
    let mut kinds: Vec<InstanceId> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for &child in world
        .get(forest)
        .map(|forest| forest.children())
        .unwrap_or(&[])
    {
        let Some(Value::String(mesh)) = world.get(child).and_then(|tree| tree.property("MeshId"))
        else {
            continue;
        };
        if !seen.contains(mesh) {
            seen.push(mesh.clone());
            kinds.push(child);
        }
    }
    if kinds.is_empty() {
        return Err(missing("trees in its Forest"));
    }

    let bench = add(world, ClassName::Model, workspace, "Benchmark", &[]);

    // Lamps under the ceiling, down the carriage's length (z 0 to -42).
    for index in 0..LAMPS {
        let z = -4.5 - index as f32 * 6.5;
        add(
            world,
            ClassName::PointLight,
            bench,
            &format!("Lamp{index}"),
            &[
                ("Position", vector(0.0, 6.9, z)),
                ("Color", Value::Color3(Color3::new(1.0, 0.82, 0.6))),
                ("Brightness", Value::Number(1.5)),
                ("Range", Value::Number(14.0)),
                ("Shadows", Value::Bool(true)),
            ],
        );
    }

    // The pond's terrain, with water and two layers (grass, mud on slopes).
    let (cx, cz) = POND_CENTER;
    let terrain = add(
        world,
        ClassName::Terrain,
        bench,
        "Pond",
        &[
            ("Position", vector(cx, POND_BOTTOM, cz)),
            ("Size", vector(POND_SIZE, POND_RANGE, POND_SIZE)),
            ("DataFolder", Value::String(POND_FOLDER.into())),
            ("Water", Value::Bool(true)),
            ("WaterLevel", Value::Number(WATER_LEVEL as f64)),
        ],
    );
    add(
        world,
        ClassName::TerrainLayer,
        terrain,
        "Grass",
        &[("Color", Value::Color3(Color3::new(0.3, 0.42, 0.2)))],
    );
    add(
        world,
        ClassName::TerrainLayer,
        terrain,
        "Mud",
        &[
            ("Color", Value::Color3(Color3::new(0.3, 0.24, 0.17))),
            ("AutoEnabled", Value::Bool(true)),
            ("MinSlope", Value::Number(12.0)),
        ],
    );

    // Trees and bushes around the pond, each kind in turn, at the forest's
    // sizes and heights above the ground.
    for index in 0..POND_TREES {
        let kind = kinds[index % kinds.len()];
        let copy = world
            .duplicate(kind)
            .map_err(|err| io::Error::other(format!("{err:?}")))?;
        world
            .set_parent(copy, Some(bench))
            .map_err(|err| io::Error::other(format!("{err:?}")))?;
        let _ = world.set_name(copy, format!("PondTree{index}"));
        let Some(Value::Vector3(at)) = world.get(kind).and_then(|tree| tree.property("Position"))
        else {
            continue;
        };
        let lift = at.y - GROUND_TOP;
        let angle = index as f32 / POND_TREES as f32 * std::f32::consts::TAU;
        let radius = POND_RADIUS + 9.0 + (index % 3) as f32 * 6.0;
        let _ = world.set_property(
            copy,
            "Position",
            vector(
                cx + radius * angle.cos(),
                GROUND_TOP + lift,
                cz + radius * angle.sin(),
            ),
        );
        let _ = world.set_property(
            copy,
            "Orientation",
            vector(0.0, (index * 47 % 360) as f32, 0.0),
        );
    }

    // Sky: clouds overhead and the sun's rays on screen.
    let lighting = world.lighting();
    add(
        world,
        ClassName::Clouds,
        lighting,
        "BenchmarkClouds",
        &[("Cover", Value::Number(0.35))],
    );
    add(
        world,
        ClassName::SunRaysEffect,
        lighting,
        "BenchmarkSunRays",
        &[],
    );
    Ok(())
}

/// Level land at the Ground part's top with a round pond in the middle,
/// deepest at its centre and easing up to the bank.
fn pond_terrain() -> io::Result<TerrainData> {
    let height_of = |y: f32| (y - POND_BOTTOM) / POND_RANGE;
    let mut data = TerrainData::flat(POND_SAMPLES, POND_SAMPLES, height_of(GROUND_TOP + 0.05))
        .map_err(|err| io::Error::other(format!("{err:?}")))?;
    let bounds = data.bounds();
    let heights = data.heights_mut_in(bounds);
    let step = POND_SIZE / (POND_SAMPLES - 1) as f32;
    let middle = (POND_SAMPLES - 1) as f32 / 2.0;
    for z in 0..POND_SAMPLES {
        for x in 0..POND_SAMPLES {
            let distance = ((x as f32 - middle).hypot(z as f32 - middle)) * step;
            if distance < POND_RADIUS {
                // Cosine bowl: 1 in the middle, 0 at the bank.
                let depth = 0.5 + 0.5 * (std::f32::consts::PI * distance / POND_RADIUS).cos();
                let y = GROUND_TOP + 0.05 + (POND_DEEPEST - GROUND_TOP) * depth;
                heights[z * POND_SAMPLES + x] = height_of(y);
            }
        }
    }
    Ok(data)
}

/// Copies `from` into `to`, leaving out the `.axcenity` cache and files
/// already there with the same size.
fn copy_new_files(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        if entry.file_name() == ".axcenity" {
            continue;
        }
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_new_files(&entry.path(), &target)?;
        } else if fs::metadata(&target).map(|meta| meta.len()).ok() != Some(entry.metadata()?.len())
        {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
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

    /// A stand-in for the owner's project: the same Instances the showcase
    /// looks for (a Forest of trees), without the assets.
    fn stand_in(folder: &Path) {
        let mut world = World::new();
        let workspace = world.workspace();
        let forest = add(&mut world, ClassName::Model, workspace, "Forest", &[]);
        for (name, mesh, y) in [
            ("Oak", "assets/models/oak.glb", 8.0),
            ("Bush", "assets/models/bush.glb", -1.0),
            ("Oak2", "assets/models/oak.glb", 8.0),
        ] {
            add(
                &mut world,
                ClassName::MeshPart,
                forest,
                name,
                &[
                    ("MeshId", Value::String(mesh.into())),
                    ("Position", vector(10.0, y, 10.0)),
                    ("Anchored", Value::Bool(true)),
                ],
            );
        }
        fs::create_dir_all(folder.join("scripts")).unwrap();
        fs::write(
            folder.join("scripts/view.client.lua"),
            "-- the owner's view\n",
        )
        .unwrap();
        fs::write(folder.join(".axcenity-keep"), "x").unwrap();
        fs::create_dir_all(folder.join(".axcenity")).unwrap();
        fs::write(folder.join(".axcenity/cache"), "x").unwrap();
        axcenity_project::save_world(folder, &world).unwrap();
    }

    #[test]
    fn the_showcase_adds_lamps_a_pond_trees_and_a_held_camera() {
        let root = std::env::temp_dir().join(format!("axcenity-showcase-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let (source, folder) = (root.join("source"), root.join("showcase"));
        stand_in(&source);
        let before = fs::read(source.join("world.json")).unwrap();

        write(&folder, &source, "pond").unwrap();
        // The source is only read.
        assert_eq!(fs::read(source.join("world.json")).unwrap(), before);
        assert!(
            !folder.join(".axcenity/cache").exists(),
            "the cache isn't copied"
        );

        let project = axcenity_project::load(&folder).unwrap();
        assert!(project.problems.is_empty(), "{:?}", project.problems);
        let world = &project.world;
        let bench = world
            .find_first_child(world.workspace(), "Benchmark")
            .unwrap();
        let count = |class: ClassName| {
            world
                .get(bench)
                .unwrap()
                .children()
                .iter()
                .filter(|&&child| world.get(child).unwrap().class() == class)
                .count()
        };
        assert_eq!(count(ClassName::PointLight), LAMPS);
        assert_eq!(count(ClassName::MeshPart), POND_TREES);
        assert_eq!(count(ClassName::Terrain), 1);
        // The pond's land and its hollow, with water above the hollow.
        let pond = world.find_first_child(bench, "Pond").unwrap();
        assert_eq!(
            world.get(pond).unwrap().property("Water"),
            Some(&Value::Bool(true))
        );
        let data = pond_terrain().unwrap();
        let world_y = |h: f32| POND_BOTTOM + h * POND_RANGE;
        let middle = POND_SAMPLES / 2;
        assert!(world_y(data.height(middle as isize, middle as isize)) < WATER_LEVEL - 3.0);
        assert!((world_y(data.height(0, 0)) - GROUND_TOP).abs() < 0.1);
        assert!(folder.join(POND_FOLDER).join("terrain.json").exists());

        let script = fs::read_to_string(folder.join("scripts/view.client.lua")).unwrap();
        assert!(script.contains("local VIEW_NAME = \"pond\""));

        // Another view rewrites only the camera.
        write(&folder, &source, "tour").unwrap();
        let script = fs::read_to_string(folder.join("scripts/view.client.lua")).unwrap();
        assert!(script.contains("local VIEW_NAME = \"tour\""));
        assert!(write(&folder, &source, "sideways").is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn every_view_the_tour_visits_is_defined() {
        let script = camera_script("tour");
        for view in VIEWS.iter().filter(|&&view| view != "tour") {
            assert!(script.contains(&format!("    {view} = {{")), "{view}");
        }
        assert!(script.contains("overview = {"));
    }
}
