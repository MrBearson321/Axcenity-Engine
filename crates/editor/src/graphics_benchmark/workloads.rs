//! The worlds the graphics benchmark draws. They're built in memory and
//! never touch the open project: each is a plain `World` drawn through the
//! same `build_draw_list` and renderer as the viewport, so what's measured
//! is what building in the editor costs.

use axcenity_core::{ClassName, Color3, InstanceId, Material, PartType, Value, Vector3, World};
use axcenity_render::Camera;
use axcenity_render::glam::Vec3;
use serde::{Deserialize, Serialize};

/// What a measurement draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Workload {
    /// A small town on a baseplate, as a creator's world looks while they
    /// build: every part shape, mixed materials (glass, neon, metal), a
    /// sun, lamps (a few casting shadows), an atmosphere and clouds. Each
    /// preset is measured on this one.
    Town,
    /// Many parts of every shape: geometry throughput, and the CPU's work
    /// turning the world into a draw list.
    Geometry,
    /// Every material, solid, see-through and shadowless, in many colors:
    /// many batches and the material shaders.
    Materials,
    /// Many lamps, a lot of them casting shadows: lighting and shadow maps.
    Lights,
}

impl Workload {
    pub const STRESS: [Workload; 3] = [Workload::Geometry, Workload::Materials, Workload::Lights];

    pub fn label(self) -> &'static str {
        match self {
            Workload::Town => "Test town",
            Workload::Geometry => "Geometry",
            Workload::Materials => "Materials",
            Workload::Lights => "Lights and shadows",
        }
    }

    /// What it draws, for the results.
    pub fn about(self) -> &'static str {
        match self {
            Workload::Town => {
                "About 2,000 parts: every shape, glass, neon and metal, a sun, 16 lamps \
                 (6 with shadows), an atmosphere and clouds."
            }
            Workload::Geometry => "20,000 parts of every shape: triangles and the CPU's draw list.",
            Workload::Materials => {
                "Every material in 8 variants (solid, see-through, casting no shadow), in \
                 many colors: many batches and material shaders."
            }
            Workload::Lights => "80 lamps, 24 of them casting shadows, over 600 parts.",
        }
    }

    /// The world it draws.
    pub fn build(self) -> World {
        match self {
            Workload::Town => town(),
            Workload::Geometry => geometry(),
            Workload::Materials => materials(),
            Workload::Lights => lights(),
        }
    }

    /// Where the camera is on `frame`: circling the world once every
    /// [`ORBIT_FRAMES`] frames, so culling, shadow cascades and the sky all
    /// change as they do while flying around in the editor. Depends only
    /// on the frame, so every preset sees the same views.
    pub fn camera(self, frame: u32) -> Camera {
        let (radius, height, target) = match self {
            Workload::Town => (70.0, 28.0, Vec3::new(0.0, 4.0, 0.0)),
            Workload::Geometry => (110.0, 45.0, Vec3::ZERO),
            Workload::Materials => (55.0, 22.0, Vec3::new(0.0, 4.0, 0.0)),
            Workload::Lights => (60.0, 25.0, Vec3::ZERO),
        };
        let turn = frame as f32 / ORBIT_FRAMES as f32 * std::f32::consts::TAU;
        Camera {
            position: Vec3::new(turn.cos() * radius, height, turn.sin() * radius),
            look_at: target,
            fov_y_degrees: 70.0,
        }
    }
}

/// Frames for the camera to circle a world once.
pub const ORBIT_FRAMES: u32 = 240;

/// Adds an anchored part to the Workspace.
fn part(
    world: &mut World,
    shape: PartType,
    position: Vector3,
    size: Vector3,
    material: Material,
    color: Color3,
) -> InstanceId {
    let id = world.create(ClassName::Part).expect("Part is creatable");
    let set = |world: &mut World, name: &str, value: Value| {
        world
            .set_property(id, name, value)
            .unwrap_or_else(|error| panic!("Part.{name}: {error:?}"));
    };
    set(world, "Shape", Value::PartType(shape));
    set(world, "Position", Value::Vector3(position));
    set(world, "Size", Value::Vector3(size));
    set(world, "Material", Value::Material(material));
    set(world, "Color", Value::Color3(color));
    set(world, "Anchored", Value::Bool(true));
    world
        .set_parent(id, Some(world.workspace()))
        .expect("parts go in the Workspace");
    id
}

fn set(world: &mut World, id: InstanceId, name: &str, value: Value) {
    world
        .set_property(id, name, value)
        .unwrap_or_else(|error| panic!("{name}: {error:?}"));
}

/// A lamp in `parent`: a PointLight, or a SpotLight shining down.
fn lamp(world: &mut World, parent: InstanceId, spot: bool, shadows: bool, color: Color3) {
    let class = if spot {
        ClassName::SpotLight
    } else {
        ClassName::PointLight
    };
    let light = world.create(class).expect("lights are creatable");
    set(world, light, "Color", Value::Color3(color));
    set(world, light, "Brightness", Value::Number(2.0));
    set(
        world,
        light,
        "Range",
        Value::Number(if spot { 30.0 } else { 20.0 }),
    );
    set(world, light, "Shadows", Value::Bool(shadows));
    if spot {
        set(
            world,
            light,
            "Direction",
            Value::Vector3(Vector3::new(0.0, -1.0, 0.0)),
        );
    }
    world
        .set_parent(light, Some(parent))
        .expect("a light goes in its part");
}

/// The Lighting every world shares: shadows reaching far enough that each
/// preset's shadow distance shows.
fn lighting(world: &mut World) {
    let lighting = world.lighting();
    set(world, lighting, "ShadowDistance", Value::Number(300.0));
}

/// A pseudo-random sequence with a fixed seed, so every run draws the same
/// world (no `rand`: the same numbers on every machine).
struct Seq(u32);

impl Seq {
    fn next(&mut self) -> f32 {
        // xorshift32
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 >> 8) as f32 / (1u32 << 24) as f32
    }

    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[((self.next() * items.len() as f32) as usize).min(items.len() - 1)]
    }

    fn color(&mut self) -> Color3 {
        Color3::new(
            0.3 + self.next() * 0.7,
            0.3 + self.next() * 0.7,
            0.3 + self.next() * 0.7,
        )
    }
}

const SOLID_MATERIALS: [Material; 8] = [
    Material::Plastic,
    Material::SmoothPlastic,
    Material::Wood,
    Material::Brick,
    Material::Concrete,
    Material::Slate,
    Material::Granite,
    Material::WoodPlanks,
];

fn town() -> World {
    let mut world = World::new();
    lighting(&mut world);
    let mut seq = Seq(0x5eed_0001);
    let v = Vector3::new;
    // The baseplate.
    part(
        &mut world,
        PartType::Block,
        v(0.0, -0.5, 0.0),
        v(256.0, 1.0, 256.0),
        Material::Plastic,
        Color3::new(0.39, 0.37, 0.35),
    );
    let mut lamps = 0;
    // 6 × 6 houses: walls, floors, a wedge roof, windows, and props.
    for row in 0..6 {
        for column in 0..6 {
            let x = (column as f32 - 2.5) * 34.0;
            let z = (row as f32 - 2.5) * 34.0;
            let wall_material = seq.pick(&SOLID_MATERIALS);
            let wall_color = seq.color();
            let floors = 1 + (seq.next() * 3.0) as usize;
            for floor in 0..floors {
                let y = floor as f32 * 8.0;
                for (dx, dz, sx, sz) in [
                    (0.0f32, -8.0f32, 16.0f32, 1.0f32),
                    (0.0, 8.0, 16.0, 1.0),
                    (-8.0, 0.0, 1.0, 16.0),
                    (8.0, 0.0, 1.0, 16.0),
                ] {
                    // A wall in pieces around a window, as people build.
                    for piece in 0..4 {
                        let along = piece as f32 * 4.0 - 6.0;
                        let (px, pz) = if sx > sz {
                            (x + dx + along, z + dz)
                        } else {
                            (x + dx, z + dz + along)
                        };
                        let (wx, wz) = if sx > sz { (4.0, 1.0) } else { (1.0, 4.0) };
                        part(
                            &mut world,
                            PartType::Block,
                            v(px, y + 4.0, pz),
                            v(wx, 8.0, wz),
                            wall_material,
                            wall_color,
                        );
                    }
                    let window = part(
                        &mut world,
                        PartType::Block,
                        v(x + dx * 1.01, y + 5.0, z + dz * 1.01),
                        v(sx.max(0.4) * 0.25, 3.0, sz.max(0.4) * 0.25),
                        Material::Glass,
                        Color3::new(0.6, 0.8, 0.95),
                    );
                    set(&mut world, window, "Transparency", Value::Number(0.5));
                }
                part(
                    &mut world,
                    PartType::Block,
                    v(x, y + 8.25, z),
                    v(17.0, 0.5, 17.0),
                    Material::WoodPlanks,
                    Color3::new(0.55, 0.4, 0.28),
                );
            }
            let top = floors as f32 * 8.0 + 0.5;
            for (side, angle) in [(-1.0f32, 0.0f32), (1.0, 180.0)] {
                let roof = part(
                    &mut world,
                    PartType::Wedge,
                    v(x, top + 2.5, z + side * 4.25),
                    v(17.0, 5.0, 8.5),
                    Material::Slate,
                    Color3::new(0.35, 0.2, 0.18),
                );
                set(
                    &mut world,
                    roof,
                    "Orientation",
                    Value::Vector3(v(0.0, angle, 0.0)),
                );
            }
            // Props in the yard: every shape.
            for prop in 0..12 {
                let shape = seq.pick(&PartType::ALL);
                let size = 1.0 + seq.next() * 2.0;
                part(
                    &mut world,
                    shape,
                    v(
                        x - 12.0 + seq.next() * 24.0,
                        size / 2.0,
                        z + 10.0 + (prop % 2) as f32 * 2.5,
                    ),
                    v(size, size, size),
                    seq.pick(&[Material::Metal, Material::Plastic, Material::Wood]),
                    seq.color(),
                );
            }
            // A lamp post with a neon head in every other yard.
            if (row + column) % 2 == 0 && lamps < 16 {
                part(
                    &mut world,
                    PartType::Cylinder,
                    v(x + 11.0, 3.0, z + 11.0),
                    v(6.0, 0.6, 0.6),
                    Material::Metal,
                    Color3::new(0.2, 0.2, 0.22),
                );
                let head = part(
                    &mut world,
                    PartType::Ball,
                    v(x + 11.0, 6.5, z + 11.0),
                    v(1.2, 1.2, 1.2),
                    Material::Neon,
                    Color3::new(1.0, 0.85, 0.6),
                );
                // Every third lamp is a spotlight; six cast shadows.
                lamp(
                    &mut world,
                    head,
                    lamps % 3 == 2,
                    lamps % 3 == 0,
                    Color3::new(1.0, 0.85, 0.6),
                );
                lamps += 1;
            }
        }
    }
    let atmosphere = world
        .create(ClassName::Atmosphere)
        .expect("Atmosphere is creatable");
    set(&mut world, atmosphere, "Haze", Value::Number(1.0));
    world
        .set_parent(atmosphere, Some(world.lighting()))
        .expect("an Atmosphere goes in Lighting");
    let clouds = world
        .create(ClassName::Clouds)
        .expect("Clouds is creatable");
    world
        .set_parent(clouds, Some(world.lighting()))
        .expect("Clouds go in Lighting");
    world
}

fn geometry() -> World {
    let mut world = World::new();
    lighting(&mut world);
    let mut seq = Seq(0x5eed_0002);
    let v = Vector3::new;
    part(
        &mut world,
        PartType::Block,
        v(0.0, -0.5, 0.0),
        v(300.0, 1.0, 300.0),
        Material::Plastic,
        Color3::new(0.39, 0.37, 0.35),
    );
    // 20,000 parts on a 200 × 100 grid, two layers.
    for index in 0..20_000u32 {
        let column = index % 200;
        let row = (index / 200) % 50;
        let layer = index / 10_000;
        let shape = PartType::ALL[(index % PartType::ALL.len() as u32) as usize];
        part(
            &mut world,
            shape,
            v(
                column as f32 * 1.4 - 140.0,
                0.6 + layer as f32 * 1.4,
                row as f32 * 5.5 - 135.0,
            ),
            v(1.2, 1.2, 1.2),
            Material::Plastic,
            seq.color(),
        );
    }
    world
}

fn materials() -> World {
    let mut world = World::new();
    lighting(&mut world);
    let mut seq = Seq(0x5eed_0003);
    let v = Vector3::new;
    part(
        &mut world,
        PartType::Block,
        v(0.0, -0.5, 0.0),
        v(200.0, 1.0, 200.0),
        Material::Concrete,
        Color3::new(0.5, 0.5, 0.5),
    );
    // Each material in 8 variants, as a column of 16 parts each.
    for (m, material) in Material::ALL.into_iter().enumerate() {
        for variant in 0..8 {
            for step in 0..16 {
                let id = part(
                    &mut world,
                    PartType::ALL[step % PartType::ALL.len()],
                    v(
                        m as f32 * 5.0 - 40.0,
                        1.0 + step as f32 * 2.1,
                        variant as f32 * 5.0 - 20.0,
                    ),
                    v(2.0, 2.0, 2.0),
                    material,
                    seq.color(),
                );
                match variant {
                    1 => set(&mut world, id, "Transparency", Value::Number(0.4)),
                    2 => set(&mut world, id, "CastShadow", Value::Bool(false)),
                    3 => set(&mut world, id, "Transparency", Value::Number(0.8)),
                    _ => {}
                }
            }
        }
    }
    world
}

fn lights() -> World {
    let mut world = World::new();
    lighting(&mut world);
    let mut seq = Seq(0x5eed_0004);
    let v = Vector3::new;
    part(
        &mut world,
        PartType::Block,
        v(0.0, -0.5, 0.0),
        v(200.0, 1.0, 200.0),
        Material::Plastic,
        Color3::new(0.39, 0.37, 0.35),
    );
    // 600 pillars and boxes for the lamps to light and shadow.
    for index in 0..600 {
        let x = (index % 30) as f32 * 5.0 - 72.5;
        let z = (index / 30) as f32 * 7.0 - 66.5;
        let height = 1.0 + seq.next() * 6.0;
        part(
            &mut world,
            seq.pick(&[PartType::Block, PartType::Cylinder, PartType::Ball]),
            v(x, height / 2.0, z),
            v(2.0, height, 2.0),
            seq.pick(&SOLID_MATERIALS),
            seq.color(),
        );
    }
    // 80 lamps hanging over them: 64 point lights, 16 spotlights, every
    // third one (24 of them, minus the last two's) casting shadows.
    for index in 0..80 {
        let x = (index % 10) as f32 * 15.0 - 67.5;
        let z = (index / 10) as f32 * 17.0 - 59.5;
        let holder = part(
            &mut world,
            PartType::Ball,
            v(x, 10.0, z),
            v(0.8, 0.8, 0.8),
            Material::Neon,
            seq.color(),
        );
        let color = seq.color();
        lamp(
            &mut world,
            holder,
            index >= 64,
            index % 3 == 0 && index < 72,
            color,
        );
    }
    world
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(world: &World, class: ClassName) -> usize {
        world
            .descendants(world.workspace())
            .into_iter()
            .filter(|&id| {
                world
                    .get(id)
                    .is_some_and(|instance| instance.class() == class)
            })
            .count()
    }

    #[test]
    fn each_workload_has_what_its_description_says() {
        let town = Workload::Town.build();
        let parts = count(&town, ClassName::Part);
        assert!((1_500..=2_500).contains(&parts), "town has {parts} parts");
        let lights = count(&town, ClassName::PointLight) + count(&town, ClassName::SpotLight);
        assert_eq!(lights, 16);

        assert_eq!(count(&Workload::Geometry.build(), ClassName::Part), 20_001);
        assert_eq!(
            count(&Workload::Materials.build(), ClassName::Part),
            1 + Material::ALL.len() * 8 * 16
        );
        let lit = Workload::Lights.build();
        assert_eq!(
            count(&lit, ClassName::PointLight) + count(&lit, ClassName::SpotLight),
            80
        );
        let shadowed = lit
            .descendants(lit.workspace())
            .into_iter()
            .filter(|&id| {
                lit.get(id).is_some_and(|instance| {
                    matches!(
                        instance.class(),
                        ClassName::PointLight | ClassName::SpotLight
                    ) && instance.property("Shadows") == Some(&Value::Bool(true))
                })
            })
            .count();
        assert_eq!(shadowed, 24);
    }

    #[test]
    fn every_build_is_the_same_world() {
        let draw = |world: &World| {
            let list = axcenity_scene::build_draw_list(
                world,
                &axcenity_assets::Assets::default(),
                Workload::Town.camera(0),
            );
            (list.items.len(), list.lights.len())
        };
        assert_eq!(draw(&Workload::Town.build()), draw(&Workload::Town.build()));
    }

    #[test]
    fn the_camera_circles_and_comes_back() {
        let start = Workload::Town.camera(0);
        let half = Workload::Town.camera(ORBIT_FRAMES / 2);
        assert!(start.position.distance(half.position) > 100.0);
        let again = Workload::Town.camera(ORBIT_FRAMES);
        assert!(start.position.distance(again.position) < 1e-3);
    }
}
