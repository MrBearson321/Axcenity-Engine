//! The texture memory budget (`EffectiveGraphics::texture_budget`). When
//! textures take more GPU memory than it allows, the least recently drawn
//! surface textures lose their largest mip levels until everything fits,
//! and get them back once there's room. UI images, terrain and the white
//! texture count toward the budget but keep their size.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::Arc;

use crate::draw_list::{DrawList, TextureSource};

use super::gpu::{shrink_texture, texture_bytes, upload_texture};
use super::lighting::material_textures;
use super::{ScenePipeline, Texels};

/// A surface texture as the budget sees it.
#[derive(Clone, Copy, Debug)]
pub(super) struct Budgeted {
    pub(super) last_drawn: u64,
    /// Its size before the budget left any levels out (`GpuTexture::base`).
    pub(super) base: (u32, u32, u32),
    pub(super) texels: Texels,
}

/// How many of each texture's largest levels to leave out so that `fixed`
/// bytes (textures the budget doesn't shrink) and theirs fit in `budget`.
/// The least recently drawn lose levels first and, among those drawn as
/// recently, the largest. Each keeps its smallest level, so everything may
/// still not fit.
pub(super) fn levels_to_drop(textures: &[Budgeted], fixed: u64, budget: u64) -> Vec<u32> {
    let mut dropped = vec![0; textures.len()];
    let mut total = fixed
        + textures
            .iter()
            .map(|texture| texture_bytes(texture.texels, texture.base, 0))
            .sum::<u64>();
    // A max-heap: the oldest first (so `Reverse`), then the most bytes.
    let mut next: BinaryHeap<(Reverse<u64>, u64, usize)> = textures
        .iter()
        .enumerate()
        .filter(|(_, texture)| texture.texels.max_dropped(texture.base) > 0)
        .map(|(index, texture)| {
            let bytes = texture_bytes(texture.texels, texture.base, 0);
            (Reverse(texture.last_drawn), bytes, index)
        })
        .collect();
    while total > budget
        && let Some((age, bytes, index)) = next.pop()
    {
        dropped[index] += 1;
        let Budgeted { base, texels, .. } = textures[index];
        let smaller = texture_bytes(texels, base, dropped[index]);
        total -= bytes - smaller;
        if dropped[index] < texels.max_dropped(base) {
            next.push((age, smaller, index));
        }
    }
    dropped
}

impl ScenePipeline {
    /// Shrinks or restores the surface textures to fit the texture budget,
    /// and returns the keys of those it replaced (so the materials and
    /// sprite bindings holding the old ones are remade). Call after this
    /// frame's textures are noted: shrinking copies the smaller levels on
    /// the GPU, but restoring uploads the texture again, so only those
    /// drawn in `list` are restored.
    pub(super) fn fit_texture_budget(&mut self, list: &DrawList) -> HashSet<u64> {
        let budget = self.graphics.texture_budget;
        let shrunk = |cached: &super::Cached<super::GpuTexture>| {
            cached.gpu.as_ref().is_some_and(|gpu| gpu.dropped > 0)
        };
        if self.texture_bytes.total() <= budget && !self.textures.values().any(shrunk) {
            return HashSet::new();
        }
        let keys: Vec<u64> = self
            .textures
            .iter()
            .filter(|(_, cached)| cached.gpu.is_some())
            .map(|(key, _)| *key)
            .collect();
        let gpu = |key: &u64| self.textures[key].gpu.as_ref().expect("filtered");
        let textures: Vec<Budgeted> = keys
            .iter()
            .map(|key| Budgeted {
                last_drawn: self.textures[key].last_drawn,
                base: gpu(key).base,
                texels: gpu(key).texels,
            })
            .collect();
        let held: u64 = keys.iter().map(|key| gpu(key).bytes.bytes()).sum();
        let fixed = self.texture_bytes.total().saturating_sub(held);
        let targets = levels_to_drop(&textures, fixed, budget);

        let mut changed = HashSet::new();
        let mut restore = HashMap::new();
        let mut encoder = None;
        for (key, target) in keys.into_iter().zip(targets) {
            let cached = self.textures.get_mut(&key).expect("listed above");
            let gpu = cached.gpu.as_mut().expect("filtered");
            if target > gpu.dropped {
                let encoder = encoder.get_or_insert_with(|| {
                    self.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("texture budget"),
                        })
                });
                let smaller = shrink_texture(
                    &self.device,
                    encoder,
                    &self.texture_bytes,
                    gpu,
                    target - gpu.dropped,
                );
                *gpu = smaller;
                changed.insert(key);
            } else if target < gpu.dropped && cached.last_drawn == self.frame {
                restore.insert(key, target);
            }
        }
        if let Some(encoder) = encoder {
            self.queue.submit([encoder.finish()]);
        }
        for source in surface_textures(list) {
            let key = source.key();
            let Some(target) = restore.remove(&key) else {
                continue;
            };
            // Freed first, so the old and new copies are never both held.
            let cached = self.textures.get_mut(&key).expect("listed above");
            cached.gpu = None;
            cached.gpu = upload_texture(
                &self.device,
                &self.queue,
                &self.texture_bytes,
                source.as_ref(),
                (
                    self.graphics.texture_quality.skipped_levels(),
                    self.capabilities.max_texture_size,
                ),
                target,
                true,
            );
            changed.insert(key);
        }
        changed
    }
}

/// The textures `list` draws on surfaces and sprites: those the texture
/// quality sizes and the budget can shrink.
fn surface_textures(list: &DrawList) -> impl Iterator<Item = &Arc<dyn TextureSource>> {
    let items = list
        .items
        .iter()
        .flat_map(|item| material_textures(item).into_iter().flatten());
    let sprites = list
        .sprites
        .iter()
        .filter_map(|sprite| sprite.texture.as_ref());
    items.chain(sprites)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIB: u64 = 1024 * 1024;

    /// A 1024² texture with all 11 levels: about 5.3 MiB.
    fn big(last_drawn: u64) -> Budgeted {
        Budgeted {
            last_drawn,
            base: (1024, 1024, 11),
            texels: Texels::Rgba8,
        }
    }

    fn total(textures: &[Budgeted], dropped: &[u32]) -> u64 {
        textures
            .iter()
            .zip(dropped)
            .map(|(texture, &dropped)| texture_bytes(texture.texels, texture.base, dropped))
            .sum()
    }

    #[test]
    fn nothing_is_dropped_within_the_budget() {
        let textures = [big(5), big(5)];
        assert_eq!(levels_to_drop(&textures, 0, 11 * MIB), [0, 0]);
    }

    #[test]
    fn the_least_recently_drawn_lose_levels_first() {
        // Two drawn this frame, one 10 frames ago: only the old one shrinks
        // while that's enough.
        let textures = [big(10), big(0), big(10)];
        let budget = 2 * texture_bytes(Texels::Rgba8, big(0).base, 0) + 2 * MIB;
        let dropped = levels_to_drop(&textures, 0, budget);
        assert_eq!(dropped, [0, 1, 0]);
        assert!(total(&textures, &dropped) <= budget);
    }

    #[test]
    fn textures_drawn_as_recently_shrink_the_largest_first_and_fit() {
        let small = Budgeted {
            last_drawn: 3,
            base: (256, 256, 9),
            texels: Texels::Rgba8,
        };
        let textures = [big(3), small, big(3), big(3)];
        let budget = 4 * MIB;
        let dropped = levels_to_drop(&textures, 0, budget);
        assert!(total(&textures, &dropped) <= budget);
        assert_eq!(dropped[1], 0, "the small one is left alone: {dropped:?}");
        // Bytes the budget can't shrink leave less room for the rest.
        let dropped = levels_to_drop(&textures, 3 * MIB, budget);
        assert!(total(&textures, &dropped) + 3 * MIB <= budget);
    }

    #[test]
    fn each_texture_keeps_its_smallest_level() {
        let textures = [big(0), big(1)];
        assert_eq!(levels_to_drop(&textures, 10 * MIB, MIB), [10, 10]);
        assert_eq!(
            texture_bytes(Texels::Rgba8, big(0).base, 10),
            4,
            "one pixel each"
        );
    }

    #[test]
    fn a_bc7_texture_keeps_whole_blocks_and_takes_a_quarter() {
        let cooked = Budgeted {
            texels: Texels::Bc7,
            ..big(0)
        };
        // 1024 down to 4 wide: 8 levels can go, leaving 4×4, 2×2 and 1×1.
        assert_eq!(levels_to_drop(&[cooked], 0, 0), [8]);
        // A byte a pixel, but the 2×2 and 1×1 levels take a whole block.
        assert_eq!(
            texture_bytes(Texels::Bc7, cooked.base, 0) * 4,
            texture_bytes(Texels::Rgba8, cooked.base, 0) - (16 + 4) + 4 * (16 + 16),
        );
        assert_eq!(texture_bytes(Texels::Bc7, cooked.base, 8), 3 * 16);
    }
}
