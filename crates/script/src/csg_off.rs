//! Solid modeling left out of this build (the `csg` feature is off, roadmap
//! M8): `UnionAsync`, `SubtractAsync` and `IntersectAsync` still exist, so
//! scripts see the same API, but each raises an error saying why.

use axcenity_core::{ClassName, InstanceId};
use mlua::{Function, Lua};

/// Nothing is ever started, so there's nothing to forget.
#[derive(Default)]
pub struct CsgJobs;

impl CsgJobs {
    pub fn forget_script(&mut self, _script: InstanceId) {}
}

/// The three methods, for every BasePart class, each raising an error.
pub fn methods(lua: &Lua) -> mlua::Result<Vec<(ClassName, &'static str, Function)>> {
    let mut methods = Vec::new();
    for name in ["UnionAsync", "SubtractAsync", "IntersectAsync"] {
        let method = lua.create_function(move |_, _: mlua::MultiValue| -> mlua::Result<()> {
            Err(mlua::Error::runtime(format!(
                "{name} isn't available: this build was made without solid modeling (the `csg` feature)"
            )))
        })?;
        for class in ClassName::ALL.into_iter().filter(|c| c.is_base_part()) {
            methods.push((class, name, method.clone()));
        }
    }
    Ok(methods)
}

/// No jobs to hand back.
pub fn step(_lua: &Lua) {}
