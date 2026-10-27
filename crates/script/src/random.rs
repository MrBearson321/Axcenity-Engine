//! `Random.new(seed?)`: a random number generator a script owns.
//!
//! It's PCG32 (pcg-random.org), done entirely in whole-number arithmetic,
//! so the same seed gives the same numbers on every run and every platform.
//! Numbers are made from its bits without `sin`, `cos` or `log`, whose last
//! digit can differ between platforms.

use std::hash::{BuildHasher, Hasher};

use axcenity_core::Vector3;
use mlua::{Lua, Table, UserData, UserDataMethods};

use crate::types::LuaVector3;

/// PCG32's multiplier and the stream every generator uses.
const MULTIPLIER: u64 = 6364136223846793005;
const INCREMENT: u64 = 1442695040888963407;

#[derive(Clone, Debug)]
pub struct LuaRandom {
    state: u64,
}

impl LuaRandom {
    pub fn new(seed: u64) -> Self {
        // The seeding steps PCG's reference code takes.
        let mut random = Self { state: 0 };
        random.next_u32();
        random.state = random.state.wrapping_add(seed);
        random.next_u32();
        random
    }

    /// A seed that differs each time, for `Random.new()`. Rust seeds each
    /// `RandomState` from the operating system's randomness.
    pub(crate) fn fresh_seed() -> u64 {
        std::collections::hash_map::RandomState::new()
            .build_hasher()
            .finish()
    }

    fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(MULTIPLIER).wrapping_add(INCREMENT);
        let shifted = (((old >> 18) ^ old) >> 27) as u32;
        shifted.rotate_right((old >> 59) as u32)
    }

    pub(crate) fn next_u64(&mut self) -> u64 {
        (u64::from(self.next_u32()) << 32) | u64::from(self.next_u32())
    }

    /// A number from 0 up to (not including) 1, with 53 random bits: all
    /// an `f64` holds.
    pub fn next_unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// A whole number from 0 to `below - 1`, every one equally likely.
    fn next_below(&mut self, below: u64) -> u64 {
        // Numbers past the last whole multiple of `below` would make the
        // low results likelier, so draw again for those.
        let zone = u64::MAX - u64::MAX % below;
        loop {
            let n = self.next_u64();
            if n < zone {
                return n % below;
            }
        }
    }

    /// A whole number from `min` to `max`, both included.
    pub fn next_integer(&mut self, min: i64, max: i64) -> i64 {
        let span = max.abs_diff(min);
        if span == u64::MAX {
            return self.next_u64() as i64;
        }
        min.wrapping_add(self.next_below(span + 1) as i64)
    }

    /// A direction one stud long, every direction equally likely: a point
    /// in the cube that's inside the ball, stretched to its edge.
    pub fn next_unit_vector(&mut self) -> [f64; 3] {
        loop {
            let [x, y, z] = [(); 3].map(|_| self.next_unit() * 2.0 - 1.0);
            let length_squared = x * x + y * y + z * z;
            if length_squared > 1e-9 && length_squared <= 1.0 {
                let length = length_squared.sqrt();
                return [x / length, y / length, z / length];
            }
        }
    }
}

impl UserData for LuaRandom {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method_mut(
            "NextNumber",
            |_, random, (min, max): (Option<f64>, Option<f64>)| {
                let (min, max) = (min.unwrap_or(0.0), max.unwrap_or(1.0));
                Ok(min + (max - min) * random.next_unit())
            },
        );
        methods.add_method_mut("NextInteger", |_, random, (min, max): (f64, f64)| {
            let (min, max) = (min.floor() as i64, max.floor() as i64);
            if min > max {
                return Err(mlua::Error::runtime(format!(
                    "NextInteger needs the smaller number first, such as random:NextInteger({max}, {min})"
                )));
            }
            Ok(random.next_integer(min, max))
        });
        methods.add_method_mut("NextUnitVector", |_, random, ()| {
            let [x, y, z] = random.next_unit_vector();
            Ok(LuaVector3(Vector3::new(x as f32, y as f32, z as f32)))
        });
        // Fisher-Yates: each item swaps with one at or before it, so every
        // order is equally likely.
        methods.add_method_mut("Shuffle", |_, random, list: Table| {
            for i in (2..=list.raw_len() as i64).rev() {
                let j = random.next_integer(1, i);
                let (a, b): (mlua::Value, mlua::Value) = (list.raw_get(i)?, list.raw_get(j)?);
                list.raw_set(i, b)?;
                list.raw_set(j, a)?;
            }
            Ok(())
        });
        methods.add_method("Clone", |_, random, ()| Ok(random.clone()));
    }
}

/// The `Random` global.
pub fn library(lua: &Lua) -> mlua::Result<Table> {
    let library = lua.create_table()?;
    library.raw_set(
        "new",
        lua.create_function(|_, seed: Option<f64>| {
            // Seeds are whole numbers; a fraction is dropped, and a negative
            // seed keeps its bits.
            let seed = seed.map_or_else(LuaRandom::fresh_seed, |seed| seed as i64 as u64);
            Ok(LuaRandom::new(seed))
        })?,
    )?;
    Ok(library)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integers_stay_in_range_and_reach_both_ends() {
        let mut random = LuaRandom::new(7);
        let rolls: Vec<i64> = (0..1000).map(|_| random.next_integer(1, 6)).collect();
        assert!(rolls.iter().all(|n| (1..=6).contains(n)));
        assert!(rolls.contains(&1) && rolls.contains(&6));
        assert_eq!(random.next_integer(5, 5), 5);
        assert!((-3..=3).contains(&random.next_integer(-3, 3)));
    }

    #[test]
    fn unit_vectors_are_one_long() {
        let mut random = LuaRandom::new(1);
        for _ in 0..100 {
            let [x, y, z] = random.next_unit_vector();
            assert!(((x * x + y * y + z * z).sqrt() - 1.0).abs() < 1e-12);
        }
    }
}
