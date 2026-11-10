# Native `vector` for Vector3: the check

Roadmap L3 and the [development policy](../development-policy.md#scripting-api-and-vector3) say not to make Vector3 a Luau native `vector` until its type identity and interop are checked. This is that check, against the Luau the engine builds (`luau0-src` 0.22.0, Luau 0.740, through mlua 0.12.2). Nothing in the engine uses a native vector. The facts Luau itself decides are pinned by `crates/script/src/native_vector_tests.rs`, so a Luau upgrade that changes one fails there.

**Verdict (2026-11-10): not yet.** The type itself fits the Roblox-style behavior well, better than today's userdata in places (equality, table keys, `type()`). Two things block it: scripts would see `0.10000000149011612` where they now see `0.1`, and every place the engine turns a Lua value into a `Vector3` (32 sites in 15 files) needs a second branch. Neither is verified to be acceptable, and the measured gain only matters for Vector3-heavy scripts.

## The checks

| Check (from the policy) | Native `vector` | Today (`LuaVector3` userdata) | Result |
|---|---|---|---|
| `typeof(v)` | `"vector"` from Luau's own; the sandbox's `typeof` (`types::typeof_name`) can answer `"Vector3"` | `"Vector3"` | OK with one line in `typeof_name` |
| `type(v)` | `"vector"` (as in Roblox) | `"userdata"` | Changes; closer to Roblox |
| `.X/.Y/.Z` casing | Single letters in either case (`v.x` and `v.X`), in the VM's fast path | `X`, `Y`, `Z` only | OK; lowercase also starts working |
| Component values | `f32` widened: `Vector3.new(0.1, 0, 0).X` is `0.10000000149011612` | `0.1`: `f32_to_lua` goes through the shortest decimal text | **Changes what scripts see**; `v.X == 0.1` turns false. Roblox behaves like native here |
| `Magnitude`, `Unit`, `:Dot`, `:Cross`, `:Lerp`, ... | Through one type metatable per Lua state (`Lua::set_type_metatable::<Vector>`), its `__index` a Rust function | Userdata fields and methods | OK; the VM calls a C `__index` directly. Only one type can be the native vector, so Vector2 stays userdata |
| Locking the metatable | `__metatable` on the type metatable; `getmetatable(v)` returns the lock string | Locked by mlua (`false`) | OK |
| Assigning `v.X = 5` | Error | Error | Same (message differs) |
| Equality | By value, `rawequal` too | By value through `__eq`; `rawequal` false | Changes; closer to Roblox |
| As a table key | By value: `t[Vector3.new(1, 2, 3)]` finds an earlier key with the same value | A new key for every value | Changes; closer to Roblox |
| `+ - * /`, unary `-` | Built into the VM, no metamethods; `v * v` and `v / v` are componentwise | Metamethods in Rust | OK if today's operand rules match (number on either side of `*`, componentwise `v * v`) |
| `tostring(v)` | `"1, 2.5, -3"` | `"1, 2.5, -3"` | Same format; `0.1` vs `0.10000000149011612` as above |
| RemoteEvents, attributes, saves, DataService, `PropertyValue` | mlua converts with `mlua::Vector` | `borrow::<LuaVector3>()` / `LuaVector3(...)`: 32 sites in 15 files (`remote.rs`, `collection.rs`, `types.rs`, `api.rs`, `query.rs`, `humanoid.rs`, ...) | **Work**: each site needs the vector case, and tests per site |
| luau-lsp definitions | `declare extern type Vector3` stays; scripts get values from `Vector3.new`, which the definitions type | Same | Not verified here (no luau-lsp in the container). luau-lsp's own `vector` type and `type(v)` narrowing need a look |
| Blocks translator | Emits `Vector3.new(...)`, unchanged | Same | OK |
| Bytecode constants | The compiler can fold `Vector3.new(1, 2, 3)` into a constant (`Compiler::set_vector_ctor`) | Not possible | Possible gain |

## What it would buy

From [benchmarks.md](../benchmarks.md#fast-builtins-and-native-code-l3): 200,000 `v = v + Vector3.new(1, 0, 0) * 0.5` take about 175 ms today, interpreted or native, because each step allocates userdata and calls Rust three times. A native vector does this arithmetic in the VM with no allocation, and native code (`--!native`) inlines it. A `part.Position` read still calls Rust (about 450 ns) but returns a value instead of allocating.

## What would settle it

1. Decide whether `v.X` may read `0.10000000149011612` (Roblox's behavior) instead of `0.1`. If not, native vector is out: the widening happens in the VM.
2. Run luau-lsp over the examples with a definitions file whose `Vector3.new` returns `vector`, and check autocomplete and `type(v) == "vector"` narrowing.
3. Measure a Vector3-heavy game script, not only the micro-benchmark, to see whether the gain justifies touching the 32 conversion sites.
