//! A clip file bound to the skeleton it poses.

use std::sync::Arc;

use axcenity_assets::{Pose, Rig, Take};

use crate::files::ClipFile;

/// A clip ready to pose one rig: its file, its take, and which bone of the
/// rig each of the take's tracks moves (matched by name, so a take from an
/// animation-only file moves a model loaded from another).
#[derive(Clone, Debug)]
pub struct BoundClip {
    pub file: ClipFile,
    takes: Arc<Vec<Take>>,
    take: usize,
    binding: Vec<Option<usize>>,
}

impl BoundClip {
    /// Binds `file`'s take, found by name in `takes` (its source file's), to
    /// `rig`. Fails, saying why, if the take isn't there or moves none of
    /// the rig's bones.
    pub fn bind(file: ClipFile, takes: Arc<Vec<Take>>, rig: &Rig) -> Result<Self, String> {
        let take = takes
            .iter()
            .position(|take| take.name == file.take)
            .ok_or_else(|| {
                let names: Vec<String> = takes.iter().map(|t| format!("`{}`", t.name)).collect();
                if names.is_empty() {
                    format!("`{}` has no animation in it", file.source)
                } else {
                    format!(
                        "`{}` has no take named `{}` (it has {})",
                        file.source,
                        file.take,
                        names.join(", ")
                    )
                }
            })?;
        let binding = takes[take].bind(rig);
        if !binding.iter().any(Option::is_some) && !takes[take].tracks.is_empty() {
            return Err(format!(
                "the take `{}` moves none of the model's bones (it's for another skeleton)",
                file.take
            ));
        }
        Ok(Self {
            file,
            takes,
            take,
            binding,
        })
    }

    pub fn take(&self) -> &Take {
        &self.takes[self.take]
    }

    /// Seconds one pass of the clip lasts at its speed. Never 0, so phases
    /// always move on.
    pub fn duration(&self) -> f64 {
        let speed = self.file.speed.abs().max(1e-3);
        (self.file.length() / speed).max(1.0 / 60.0)
    }

    /// Where in the take (seconds) the clip is at `phase`: 0 is its start,
    /// 1 its end, 1.5 halfway through the second pass. A looping clip goes
    /// round; one that doesn't holds its end. A negative speed plays it
    /// backwards.
    pub fn time_at(&self, phase: f64) -> f64 {
        let through = if self.file.looping {
            phase.rem_euclid(1.0)
        } else {
            phase.clamp(0.0, 1.0)
        };
        let through = if self.file.speed < 0.0 {
            1.0 - through
        } else {
            through
        };
        self.file.start + through * self.file.length()
    }

    /// Moves the bones the clip animates in `pose` to where they are at
    /// `phase` (see [`time_at`](Self::time_at)).
    pub fn sample(&self, phase: f64, rig: &Rig, pose: &mut Pose) {
        let time = self.time_at(phase) as f32;
        self.take().sample(time, &self.binding, rig, pose);
    }
}
