//! Lending the World to Lua while scripts run.
//!
//! Lua callbacks can't borrow the caller's `&mut World`, because Lua may keep
//! those callbacks around for longer than the call. So each Lua state keeps a
//! [`LentWorld`] slot in its "app data", and [`with_world`] swaps the caller's
//! World into that slot for the length of one call, then swaps it back.
//!
//! The slot always holds *some* World: a small spare one while nothing is
//! lent. Swapping two Worlds only moves their fields; nothing is allocated,
//! built or dropped per call. (The first version moved the World in with
//! `std::mem::take`, which built a whole default World, game and 13 services,
//! on every call into Lua and threw it away afterwards.)

use std::ops::{Deref, DerefMut};

use axcenity_core::World;
use mlua::{AppDataRef, AppDataRefMut, Lua};

/// The app data slot the World is lent through.
struct LentWorld {
    /// The lent World while `lent`, else a spare that's never read.
    world: World,
    lent: bool,
}

/// Gives `lua` its slot. [`crate::context::Context::new`] calls this, so the
/// slot exists before any script runs; [`with_world`] also adds it if not.
pub(crate) fn install(lua: &Lua) {
    if lua.app_data_ref::<LentWorld>().is_none() {
        lua.set_app_data(LentWorld {
            world: World::new(),
            lent: false,
        });
    }
}

/// Lends `world` to Lua while `f` runs.
///
/// `impl FnOnce() -> R` means "any closure that can be called once and
/// returns an R", so callers can pass `|| ...` with whatever result they need.
///
/// The World comes back even if `f` panics (the editor's Play catches panics
/// per frame and keeps going), and a nested call, which lends a different
/// World for a while, restores the outer one when it ends.
pub(crate) fn with_world<R>(lua: &Lua, world: &mut World, f: impl FnOnce() -> R) -> R {
    /// Swaps the World back when dropped, which also happens while
    /// unwinding from a panic.
    struct Lend<'a> {
        lua: &'a Lua,
        world: &'a mut World,
        was_lent: bool,
    }

    impl Drop for Lend<'_> {
        fn drop(&mut self) {
            // `try_` because a second panic while unwinding would abort.
            // Every guard handed out by `world`/`world_mut` lives inside `f`,
            // so the slot is free by now unless that guard leaked.
            if let Ok(Some(mut slot)) = self.lua.try_app_data_mut::<LentWorld>() {
                std::mem::swap(&mut slot.world, self.world);
                slot.lent = self.was_lent;
            }
        }
    }

    install(lua);
    // A script may connect to anything from here on.
    world.watch_destroyed();
    let was_lent = {
        let mut slot = lua
            .app_data_mut::<LentWorld>()
            .expect("installed just above");
        std::mem::swap(&mut slot.world, world);
        std::mem::replace(&mut slot.lent, true)
    };
    let _lend = Lend {
        lua,
        world,
        was_lent,
    };
    f()
}

/// The lent World, for reading (see [`world`]).
pub struct WorldRef<'a>(AppDataRef<'a, LentWorld>);

impl Deref for WorldRef<'_> {
    type Target = World;

    fn deref(&self) -> &World {
        &self.0.world
    }
}

/// The lent World, for changing (see [`world_mut`]).
pub struct WorldMut<'a>(AppDataRefMut<'a, LentWorld>);

impl Deref for WorldMut<'_> {
    type Target = World;

    fn deref(&self) -> &World {
        &self.0.world
    }
}

impl DerefMut for WorldMut<'_> {
    fn deref_mut(&mut self) -> &mut World {
        &mut self.0.world
    }
}

/// The World, for reading. Only fails if called while no script is running.
pub fn world(lua: &Lua) -> mlua::Result<WorldRef<'_>> {
    lua.app_data_ref::<LentWorld>()
        .filter(|slot| slot.lent)
        .map(WorldRef)
        .ok_or_else(no_world)
}

/// The World, for changing. Drop the returned guard before calling back into
/// Lua, because the World can't be borrowed twice at once.
pub fn world_mut(lua: &Lua) -> mlua::Result<WorldMut<'_>> {
    lua.app_data_mut::<LentWorld>()
        .filter(|slot| slot.lent)
        .map(WorldMut)
        .ok_or_else(no_world)
}

fn no_world() -> mlua::Error {
    mlua::Error::runtime("the game world can only be used while scripts are running")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_core::ClassName;

    fn part_count(world: &World) -> usize {
        world.get(world.workspace()).unwrap().children().len()
    }

    fn add_part(world: &mut World) {
        let part = world.create(ClassName::Part).unwrap();
        world.set_parent(part, Some(world.workspace())).unwrap();
    }

    #[test]
    fn lends_the_world_and_gives_it_back() {
        let lua = Lua::new();
        let mut world = World::new();
        add_part(&mut world);
        assert!(super::world(&lua).is_err(), "nothing is lent yet");

        let seen = with_world(&lua, &mut world, || {
            let mut lent = world_mut(&lua).unwrap();
            add_part(&mut lent);
            part_count(&lent)
        });

        assert_eq!(seen, 2);
        assert_eq!(part_count(&world), 2, "the change made while lent stays");
        assert!(super::world(&lua).is_err(), "nothing is lent afterwards");
    }

    #[test]
    fn nested_lends_restore_the_outer_world() {
        let lua = Lua::new();
        let mut outer = World::new();
        add_part(&mut outer);
        let mut inner = World::new();

        with_world(&lua, &mut outer, || {
            assert_eq!(part_count(&super::world(&lua).unwrap()), 1);
            with_world(&lua, &mut inner, || {
                assert_eq!(part_count(&super::world(&lua).unwrap()), 0);
            });
            assert_eq!(part_count(&super::world(&lua).unwrap()), 1);
        });

        assert_eq!(part_count(&outer), 1);
        assert_eq!(part_count(&inner), 0);
        assert!(super::world(&lua).is_err());
    }

    #[test]
    fn a_panic_still_gives_the_world_back() {
        let lua = Lua::new();
        let mut world = World::new();
        add_part(&mut world);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            with_world(&lua, &mut world, || panic!("a bug in an engine callback"));
        }));

        assert!(result.is_err());
        assert_eq!(part_count(&world), 1);
        assert!(super::world(&lua).is_err());
    }

    /// How long one call through `with_world` takes. Not a pass/fail test:
    /// `cargo test -p axcenity-script --release -- --ignored bench_with_world --nocapture`
    #[test]
    #[ignore]
    fn bench_with_world() {
        let lua = Lua::new();
        let mut world = World::new();
        let calls = 1_000_000;
        let start = std::time::Instant::now();
        for _ in 0..calls {
            with_world(&lua, &mut world, || {
                std::hint::black_box(super::world(&lua).unwrap().workspace());
            });
        }
        let elapsed = start.elapsed();
        println!(
            "with_world: {:.1} ns per call ({calls} calls in {elapsed:?})",
            elapsed.as_nanos() as f64 / calls as f64
        );
    }
}
