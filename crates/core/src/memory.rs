//! What the game holds in memory, by kind (roadmap G7), for the editor's
//! Profiler and the player's Developer Console. Each crate counts its own
//! part ([`World::memory`], `Assets::memory`, `Physics::memory`,
//! `GameAudio::memory`); the numbers are estimates of the heap each
//! holds, not what the allocator handed out, so they add up to less than
//! the process's resident memory.

use crate::{ClassName, Instance, Value, World};

/// How many of something there are and about how many bytes they hold.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MemoryUse {
    pub count: usize,
    pub bytes: usize,
}

impl MemoryUse {
    pub fn add(&mut self, bytes: usize) {
        self.count += 1;
        self.bytes += bytes;
    }
}

impl std::ops::AddAssign for MemoryUse {
    fn add_assign(&mut self, other: MemoryUse) {
        self.count += other.count;
        self.bytes += other.bytes;
    }
}

/// The Instances, and how many of each class, most first.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InstanceMemory {
    pub total: MemoryUse,
    /// Each class there's at least one of: its Instances and their bytes,
    /// the most bytes first (then by class name).
    pub classes: Vec<(ClassName, MemoryUse)>,
}

impl InstanceMemory {
    /// How many Instances of `class` there are.
    pub fn count_of(&self, class: ClassName) -> usize {
        self.classes
            .iter()
            .find(|(c, _)| *c == class)
            .map_or(0, |(_, used)| used.count)
    }
}

/// The game's memory by kind, on the CPU. GPU memory is the renderer's
/// (`RenderStats`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MemoryBreakdown {
    /// Both Lua heaps (server and client).
    pub lua: usize,
    pub instances: InstanceMemory,
    /// Models read from files: their triangles, levels of detail and hulls.
    pub models: MemoryUse,
    /// Texture images (decoded pixels and cooked mip levels), those inside
    /// models included, each once.
    pub textures: MemoryUse,
    /// Skies, reflection and light probe bakes, color lookups and palettes.
    pub lighting: MemoryUse,
    /// Decoded sounds, ready to play.
    pub sounds: MemoryUse,
    /// Physics bodies and their colliders' shapes.
    pub physics: MemoryUse,
}

impl MemoryBreakdown {
    /// How many Instances of `class` there are.
    pub fn classes_of(&self, class: ClassName) -> usize {
        self.instances.count_of(class)
    }

    /// Everything counted, in bytes.
    pub fn total(&self) -> usize {
        self.lua
            + self.instances.total.bytes
            + self.models.bytes
            + self.textures.bytes
            + self.lighting.bytes
            + self.sounds.bytes
            + self.physics.bytes
    }

    /// The rows the profilers show: a name, how many (`None` for Lua) and
    /// the bytes, in a fixed order.
    pub fn rows(&self) -> [(&'static str, Option<usize>, usize); 7] {
        [
            ("Lua", None, self.lua),
            (
                "Instances",
                Some(self.instances.total.count),
                self.instances.total.bytes,
            ),
            ("Models", Some(self.models.count), self.models.bytes),
            ("Textures", Some(self.textures.count), self.textures.bytes),
            (
                "Sky and probes",
                Some(self.lighting.count),
                self.lighting.bytes,
            ),
            ("Sounds", Some(self.sounds.count), self.sounds.bytes),
            ("Physics", Some(self.physics.count), self.physics.bytes),
        ]
    }
}

impl World {
    /// The Instances' memory, by class (services aren't counted).
    pub fn memory(&self) -> InstanceMemory {
        let mut classes: Vec<(ClassName, MemoryUse)> = ClassName::ALL
            .iter()
            .filter_map(|&class| {
                let mut used = MemoryUse::default();
                for id in self.all_of(class) {
                    if let Some(instance) = self.get(id) {
                        used.add(instance_bytes(instance));
                    }
                }
                (used.count > 0).then_some((class, used))
            })
            .collect();
        classes.sort_by(|a, b| {
            b.1.bytes
                .cmp(&a.1.bytes)
                .then(a.0.as_str().cmp(b.0.as_str()))
        });
        let mut total = MemoryUse::default();
        for (_, used) in &classes {
            total += *used;
        }
        InstanceMemory { total, classes }
    }
}

/// About what one Instance holds: itself, its name, children, properties,
/// attributes and tags.
fn instance_bytes(instance: &Instance) -> usize {
    // A B-tree entry: the key and value, and about as much again for the
    // node they sit in.
    let entry =
        |key: usize, value: &Value| 2 * (key + std::mem::size_of::<Value>()) + value_bytes(value);
    std::mem::size_of::<Instance>()
        + instance.name().len()
        + std::mem::size_of_val(instance.children())
        + instance
            .properties()
            .map(|(_, value)| entry(std::mem::size_of::<&str>(), value))
            .sum::<usize>()
        + instance
            .attributes()
            .iter()
            .map(|(name, value)| entry(std::mem::size_of::<String>() + name.len(), value))
            .sum::<usize>()
        + instance
            .tags()
            .iter()
            .map(|tag| std::mem::size_of::<String>() + tag.len())
            .sum::<usize>()
}

/// What a value holds on the heap.
fn value_bytes(value: &Value) -> usize {
    match value {
        Value::String(text) => text.len(),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instances_are_counted_by_class_the_biggest_first() {
        let mut world = World::new();
        let workspace = world.workspace();
        for _ in 0..3 {
            let part = world.create(ClassName::Part).unwrap();
            world.set_parent(part, Some(workspace)).unwrap();
        }
        let model = world.create(ClassName::Model).unwrap();
        world.set_parent(model, Some(workspace)).unwrap();
        let memory = world.memory();
        let (class, parts) = memory.classes[0];
        assert_eq!(class, ClassName::Part, "{:?}", memory.classes);
        assert_eq!(parts.count, 3);
        let models = memory
            .classes
            .iter()
            .find(|(class, _)| *class == ClassName::Model)
            .unwrap()
            .1;
        assert_eq!(models.count, 1);
        assert!(parts.bytes > 3 * models.bytes, "a part has many properties");
        let counted: usize = memory.classes.iter().map(|(_, used)| used.count).sum();
        assert_eq!(memory.total.count, counted);
        assert_eq!(counted, 4, "services aren't counted");

        // A long name and a long string attribute count.
        let big = world.create(ClassName::Folder).unwrap();
        world.set_parent(big, Some(workspace)).unwrap();
        let before = world.memory().total.bytes;
        world.set_name(big, "x".repeat(1000)).unwrap();
        world
            .set_attribute(big, "Notes", Some(Value::String("y".repeat(5000))))
            .unwrap();
        assert!(world.memory().total.bytes >= before + 6000);
    }

    #[test]
    fn the_breakdown_adds_up_its_rows() {
        let breakdown = MemoryBreakdown {
            lua: 100,
            instances: InstanceMemory {
                total: MemoryUse {
                    count: 2,
                    bytes: 20,
                },
                classes: Vec::new(),
            },
            models: MemoryUse { count: 1, bytes: 3 },
            textures: MemoryUse { count: 1, bytes: 4 },
            lighting: MemoryUse { count: 1, bytes: 5 },
            sounds: MemoryUse { count: 1, bytes: 6 },
            physics: MemoryUse { count: 1, bytes: 7 },
        };
        assert_eq!(breakdown.total(), 145);
        let summed: usize = breakdown.rows().iter().map(|row| row.2).sum();
        assert_eq!(summed, breakdown.total());
    }
}
