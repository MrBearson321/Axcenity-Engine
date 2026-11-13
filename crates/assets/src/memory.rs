//! What the loaded assets hold (roadmap G7), for the profilers.

use std::collections::HashSet;
use std::sync::Arc;

use crate::{Assets, Image, LightProbes, Model, ProbeLight};

/// How many of a kind of asset are loaded and about how many bytes they
/// hold (`core::memory::MemoryUse` in plain data: `assets` depends on no
/// engine crate).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheUse {
    pub count: usize,
    pub bytes: usize,
}

impl CacheUse {
    fn add(&mut self, bytes: usize) {
        self.count += 1;
        self.bytes += bytes;
    }
}

/// The asset caches' memory, by kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AssetMemory {
    /// Models' triangles, levels of detail and hulls.
    pub models: CacheUse,
    /// Texture files and the images inside models, each once.
    pub textures: CacheUse,
    /// Skies, probe bakes, color lookups and palettes.
    pub lighting: CacheUse,
}

impl Assets {
    /// What every model, texture, sky and bake read so far holds. Walks the
    /// caches, so the profilers ask every few frames, not every one.
    pub fn memory(&self) -> AssetMemory {
        let shared = self.lock();
        let mut memory = AssetMemory::default();
        // An image shared by several models (or a model and a texture
        // file) is counted once.
        let mut images: HashSet<*const Image> = HashSet::new();
        let mut add_image = |memory: &mut AssetMemory, image: &Arc<Image>| {
            if images.insert(Arc::as_ptr(image)) {
                memory.textures.add(image.heap_bytes());
            }
        };
        for model in shared.models.values().flatten() {
            memory.models.add(model_bytes(model));
            for primitive in &model.primitives {
                for image in primitive.images() {
                    add_image(&mut memory, image);
                }
            }
        }
        for image in shared.textures.values().flatten() {
            add_image(&mut memory, image);
        }
        let floats = |count: usize| count * std::mem::size_of::<f32>();
        for sky in shared.sky_images.values().flatten() {
            memory.lighting.add(floats(sky.rgb.len()));
        }
        for probe in shared.probes.values().flatten() {
            memory.lighting.add(floats(probe.rgb.len()));
        }
        for probes in shared.light_probes.values().flatten() {
            memory.lighting.add(light_probe_bytes(probes));
        }
        for lookup in shared.lookups.values().flatten() {
            memory.lighting.add(floats(lookup.rgb.len()));
        }
        for palette in shared.palettes.values().flatten() {
            memory
                .lighting
                .add(std::mem::size_of_val(palette.colors.as_slice()));
        }
        memory
    }
}

/// A model's own bytes: its primitives, hull and collision pieces, not its
/// images.
pub fn model_bytes(model: &Model) -> usize {
    model
        .primitives
        .iter()
        .map(|primitive| primitive.heap_bytes())
        .sum::<usize>()
        + std::mem::size_of_val(model.hull_points.as_slice())
        + model
            .collision
            .iter()
            .map(|piece| std::mem::size_of_val(piece.as_slice()))
            .sum::<usize>()
}

fn light_probe_bytes(probes: &LightProbes) -> usize {
    probes.probes.len() * std::mem::size_of::<ProbeLight>()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn example(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples")
            .join(name)
    }

    #[test]
    fn a_models_triangles_and_its_images_are_counted_once_each() {
        let assets = Assets::new(example("hello-pbr"));
        assert_eq!(assets.memory(), AssetMemory::default(), "nothing loaded");
        let path = "assets/models/DamagedHelmet/DamagedHelmet.glb";
        let model = assets.model_blocking(path).unwrap();
        // Asking again loads nothing more.
        assets.model_blocking(path).unwrap();
        let memory = assets.memory();
        assert_eq!(memory.models.count, 1);
        assert_eq!(memory.models.bytes, model_bytes(&model));
        let triangles: usize = model.primitives.iter().map(|p| p.indices.len() / 3).sum();
        assert!(
            memory.models.bytes > triangles * 3 * 4,
            "at least its indices"
        );
        let mut images: Vec<*const Image> = model
            .primitives
            .iter()
            .flat_map(|primitive| primitive.images().map(Arc::as_ptr))
            .collect();
        images.sort();
        images.dedup();
        assert!(!images.is_empty(), "the helmet has textures");
        // Its metal/rough and occlusion maps are channel textures; they
        // count too.
        let material = &model.primitives[0].material;
        for texture in [&material.roughness_texture, &material.occlusion_texture] {
            let image = Arc::as_ptr(&texture.as_ref().unwrap().image);
            assert!(images.contains(&image), "channel textures are counted");
        }
        assert_eq!(memory.textures.count, images.len());
        let bytes: usize = model
            .primitives
            .iter()
            .flat_map(|primitive| primitive.images())
            .map(|image| image.heap_bytes())
            .sum::<usize>();
        assert!(memory.textures.bytes <= bytes, "each once");
        assert_eq!(memory.lighting, CacheUse::default());
    }

    #[test]
    fn a_texture_file_counts_its_pixels() {
        let assets = Assets::new(example("hello-decals"));
        let image = assets
            .texture_blocking("assets/textures/arrow.png")
            .unwrap();
        let pixels = image.rgba().len();
        assert_eq!(pixels, image.width as usize * image.height as usize * 4);
        let memory = assets.memory();
        assert_eq!(memory.textures.count, 1);
        assert!(memory.textures.bytes >= pixels, "{memory:?}");
        assert_eq!(memory.models, CacheUse::default());
    }
}
