//! What Luau's built-in `vector` type does, for roadmap L3's check before
//! Vector3 could use it (`docs/research/native-vector-check.md`). Nothing
//! in the engine uses a native vector yet; these pin Luau's behavior so a
//! Luau upgrade that changes a fact the check relies on fails here.
//!
//! Each test runs on a bare Luau state with the `vector` library, plus a
//! type metatable like the one Vector3's methods would need.

use mlua::{Lua, Value, Vector};

/// A Luau state whose vectors have a `Magnitude` property and a `Dot`
/// method through the shared type metatable, as Vector3 would need.
fn lua_with_vector_metatable() -> Lua {
    let lua = Lua::new();
    let metatable = lua.create_table().unwrap();
    let index = lua
        .create_function(|lua, (v, key): (Vector, String)| -> mlua::Result<Value> {
            match key.as_str() {
                "Magnitude" => {
                    let m = (v.x() * v.x() + v.y() * v.y() + v.z() * v.z()).sqrt();
                    Ok(Value::Number(f64::from(m)))
                }
                "Dot" => Ok(Value::Function(lua.create_function(
                    |_, (a, b): (Vector, Vector)| Ok(a.x() * b.x() + a.y() * b.y() + a.z() * b.z()),
                )?)),
                _ => Err(mlua::Error::runtime(format!(
                    "{key} is not a valid member of Vector3"
                ))),
            }
        })
        .unwrap();
    metatable.set("__index", index).unwrap();
    metatable
        .set("__metatable", "The metatable is locked")
        .unwrap();
    lua.set_type_metatable::<Vector>(Some(metatable));
    lua
}

fn eval<T: mlua::FromLuaMulti>(lua: &Lua, code: &str) -> T {
    lua.load(code).eval().unwrap()
}

#[test]
fn type_and_typeof_both_say_vector() {
    let lua = Lua::new();
    let (ty, typeof_): (String, String) = eval(
        &lua,
        "local v = vector.create(1, 2, 3) return type(v), typeof(v)",
    );
    // Not "Vector3": only the sandbox's own `typeof` could answer that.
    assert_eq!((ty.as_str(), typeof_.as_str()), ("vector", "vector"));
}

#[test]
fn single_letter_fields_are_read_in_either_case_and_are_32_bit() {
    let lua = Lua::new();
    let (x, upper_x, z): (f64, f64, f64) = eval(
        &lua,
        "local v = vector.create(0.1, 2, 3) return v.x, v.X, v.Z",
    );
    assert_eq!(x, upper_x);
    assert_eq!(z, 3.0);
    // Components are f32, as the engine's Vector3 already is.
    assert_eq!(x, f64::from(0.1f32));
    assert_ne!(x, 0.1);
}

#[test]
fn other_members_go_through_the_shared_type_metatable() {
    let lua = lua_with_vector_metatable();
    let (magnitude, dot): (f64, f64) = eval(
        &lua,
        "local v = vector.create(3, 4, 0) return v.Magnitude, v:Dot(vector.create(1, 1, 0))",
    );
    assert_eq!((magnitude, dot), (5.0, 7.0));
    let error = lua
        .load("return vector.create(1, 2, 3).Nope")
        .exec()
        .unwrap_err()
        .to_string();
    assert!(error.contains("Nope is not a valid member"), "{error}");
}

#[test]
fn the_type_metatable_can_be_locked_against_scripts() {
    let lua = lua_with_vector_metatable();
    let locked: String = eval(&lua, "return getmetatable(vector.create(1, 2, 3))");
    assert_eq!(locked, "The metatable is locked");
}

#[test]
fn fields_cannot_be_assigned() {
    let lua = Lua::new();
    assert!(
        lua.load("local v = vector.create(1, 2, 3) v.X = 5")
            .exec()
            .is_err()
    );
}

#[test]
fn equality_and_table_keys_are_by_value() {
    let lua = Lua::new();
    let (equal, raw, found, count): (bool, bool, String, i32) = eval(
        &lua,
        r#"
        local a, b = vector.create(1, 2, 3), vector.create(1, 2, 3)
        local t = { [a] = "found" }
        t[b] = "found"
        local count = 0
        for _ in t do count += 1 end
        return a == b, rawequal(a, b), t[vector.create(1, 2, 3)], count
        "#,
    );
    // Vector3 userdata today: `==` by value through `__eq`, but `rawequal`
    // false and a new key per value.
    assert_eq!(
        (equal, raw, found.as_str(), count),
        (true, true, "found", 1)
    );
}

#[test]
fn arithmetic_is_built_in_and_needs_no_metatable() {
    let lua = Lua::new();
    let (x, y, z): (f64, f64, f64) = eval(
        &lua,
        "local v = vector.create(1, 2, 3) * 2 + vector.create(1, 1, 1) - vector.create(0, 0, 1) / 1 return v.x, v.y, v.z",
    );
    assert_eq!((x, y, z), (3.0, 5.0, 6.0));
}

#[test]
fn tostring_reads_like_vector3s() {
    let lua = Lua::new();
    let text: String = eval(&lua, "return tostring(vector.create(1, 2.5, -3))");
    assert_eq!(text, "1, 2.5, -3");
}
