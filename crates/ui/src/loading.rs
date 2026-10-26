//! A game's own loading screen: a ScreenGui the project names, shown over
//! all the other UI while the game starts. It goes (its `Enabled` is
//! turned off) once it has been up for its minimum time and, when the
//! project asks, a LocalScript has called
//! `ReplicatedFirst:RemoveDefaultLoadingScreen()`. Hosts (the player, the
//! editor's Play) keep one, pass [`LoadingGui::on_top`] in
//! [`crate::UiInput::on_top`] and call [`LoadingGui::advance`] each frame.

use axcenity_core::{ClassName, InstanceId, Value, World};

#[derive(Debug)]
pub struct LoadingGui {
    gui: InstanceId,
    /// Seconds it has been up.
    shown: f32,
    min_seconds: f32,
    /// True until a LocalScript says the game is ready, when the project
    /// waits for one.
    waiting: bool,
    done: bool,
}

impl LoadingGui {
    /// Shows the first ScreenGui in the Workspace called `name` (turning its
    /// `Enabled` on), or returns `None` when there isn't one.
    pub fn start(
        world: &mut World,
        name: &str,
        min_seconds: f32,
        wait_for_script: bool,
    ) -> Option<Self> {
        let gui = find_screen_gui(world, name)?;
        world.set_property(gui, "Enabled", Value::Bool(true)).ok()?;
        Some(Self {
            gui,
            shown: 0.0,
            min_seconds: if min_seconds.is_finite() {
                min_seconds.max(0.0)
            } else {
                0.0
            },
            waiting: wait_for_script,
            done: false,
        })
    }

    /// The ScreenGui while it's covering the game.
    pub fn on_top(&self) -> Option<InstanceId> {
        (!self.done).then_some(self.gui)
    }

    /// Moves on by one frame of `seconds`. `ready` is whether a LocalScript
    /// called `RemoveDefaultLoadingScreen` since the last frame. Hides the
    /// ScreenGui once it's done; scripts see `Enabled` change.
    pub fn advance(&mut self, world: &mut World, seconds: f32, ready: bool) {
        if self.done {
            return;
        }
        if ready {
            self.waiting = false;
        }
        if seconds.is_finite() {
            self.shown += seconds.max(0.0);
        }
        if self.shown >= self.min_seconds && !self.waiting {
            self.done = true;
            // A script may have removed it already.
            let _ = world.set_property(self.gui, "Enabled", Value::Bool(false));
        }
    }
}

fn find_screen_gui(world: &World, name: &str) -> Option<InstanceId> {
    if name.is_empty() {
        return None;
    }
    world
        .descendants(world.workspace())
        .into_iter()
        .find(|&id| {
            world.get(id).is_some_and(|instance| {
                instance.class() == ClassName::ScreenGui && instance.name() == name
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world_with_gui(name: &str, enabled: bool) -> (World, InstanceId) {
        let mut world = World::new();
        let gui = world.create(ClassName::ScreenGui).unwrap();
        world.set_name(gui, name).unwrap();
        world
            .set_property(gui, "Enabled", Value::Bool(enabled))
            .unwrap();
        world.set_parent(gui, Some(world.workspace())).unwrap();
        (world, gui)
    }

    fn enabled(world: &World, gui: InstanceId) -> bool {
        world.get(gui).unwrap().property("Enabled") == Some(&Value::Bool(true))
    }

    #[test]
    fn it_shows_the_named_screen_gui_until_its_minimum_time() {
        let (mut world, gui) = world_with_gui("Loading", false);
        assert!(LoadingGui::start(&mut world, "Elsewhere", 1.0, false).is_none());
        let mut loading = LoadingGui::start(&mut world, "Loading", 1.0, false).unwrap();
        assert!(enabled(&world, gui), "shown even when saved disabled");
        loading.advance(&mut world, 0.6, false);
        assert_eq!(loading.on_top(), Some(gui));
        loading.advance(&mut world, 0.6, false);
        assert_eq!(loading.on_top(), None);
        assert!(!enabled(&world, gui));
    }

    #[test]
    fn it_can_wait_for_a_script_but_not_less_than_its_minimum() {
        let (mut world, gui) = world_with_gui("Loading", true);
        let mut loading = LoadingGui::start(&mut world, "Loading", 1.0, true).unwrap();
        loading.advance(&mut world, 5.0, false);
        assert_eq!(loading.on_top(), Some(gui), "still waiting for the script");
        let (mut world, gui) = world_with_gui("Loading", true);
        let mut loading = LoadingGui::start(&mut world, "Loading", 1.0, true).unwrap();
        loading.advance(&mut world, 0.1, true);
        assert_eq!(loading.on_top(), Some(gui), "ready, but up for too short");
        loading.advance(&mut world, 1.0, false);
        assert_eq!(loading.on_top(), None);
        assert!(!enabled(&world, gui));
    }
}
