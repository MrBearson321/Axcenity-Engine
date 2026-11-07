//! What the ability bar shows for one Ability (Phase 76): the `script`
//! crate works it out (the server's cooldown, or the one a player's device
//! started at once) and `ui` draws it.

use crate::InstanceId;

/// One slot of the ability bar.
#[derive(Clone, Debug, PartialEq)]
pub struct AbilitySlot {
    pub ability: InstanceId,
    /// The key or button its `ActivationKey` action is bound to, such as
    /// `Q`. Empty when the action has none.
    pub key: String,
    /// Seconds of cooldown left, and the whole cooldown, for the sweep.
    pub cooldown_left: f32,
    pub cooldown: f32,
}

impl AbilitySlot {
    /// How much of the cooldown is left, from 1 (just started) to 0.
    pub fn sweep(&self) -> f32 {
        if self.cooldown > 0.0 && self.cooldown_left > 0.0 {
            (self.cooldown_left / self.cooldown).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}
