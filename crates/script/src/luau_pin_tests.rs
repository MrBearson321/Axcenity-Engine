//! Guards the `luau0-src` pin (roadmap I6). `mlua-sys` reads Luau bytecode
//! only up to a fixed version, but declares a `luau0-src` range wide enough
//! to take a Luau that writes newer bytecode. Then `mlua` takes every
//! compiled chunk for source text, and no script loads at all. The workspace
//! `Cargo.toml` pins `luau0-src` exactly; these tests fail if a raise of the
//! pin (or of `mlua`) brings that mismatch back.

use mlua::Lua;
use mlua::chunk::{ChunkMode, Compiler};

/// The newest bytecode version `mlua-sys` 0.13 reads (`LBC_VERSION_MAX` in
/// its `luau/compat.rs`). Raise this only with `mlua`, after checking it.
const MLUA_READS_UP_TO: u8 = 11;

#[test]
fn compiled_luau_chunks_load_and_run() {
    let bytecode = Compiler::new()
        .compile("local t = {} t[1] = 20 return t[1] + 22")
        .expect("the Luau compiler builds a chunk");
    // A chunk's first byte is its bytecode version (0 is a compile error).
    let version = bytecode[0];
    assert!(
        (1..=MLUA_READS_UP_TO).contains(&version),
        "Luau writes bytecode version {version}, but mlua reads only up to \
         {MLUA_READS_UP_TO}: keep `luau0-src` pinned to a release that matches `mlua`"
    );

    let lua = Lua::new();
    // mlua must recognise the bytes as bytecode by itself, as it does for
    // every script it compiles from source.
    let chunk = lua.load(&bytecode[..]);
    assert_eq!(chunk.mode(), ChunkMode::Binary);
    let answer: i64 = chunk.eval().expect("a compiled chunk runs");
    assert_eq!(answer, 42);
}

#[test]
fn luau_source_loads_and_runs() {
    let lua = Lua::new();
    let answer: i64 = lua
        .load("local function add(a: number, b: number): number return a + b end return add(40, 2)")
        .eval()
        .expect("Luau source compiles and runs");
    assert_eq!(answer, 42);
}

#[test]
fn luau_sources_stay_pinned() {
    // `luau0-src` must stay on the exact release the workspace pins, which
    // `mlua` 0.12.2 reads. CI builds with `--locked`, so this checks the
    // lockfile it builds from.
    let lock = include_str!("../../../Cargo.lock");
    let pinned = lock
        .split("[[package]]")
        .find(|package| package.contains("\nname = \"luau0-src\"\n"))
        .expect("Cargo.lock lists luau0-src");
    assert!(
        pinned.contains("\nversion = \"0.22.0+"),
        "luau0-src moved off 0.22.0; raise it only together with mlua \
         (see the workspace Cargo.toml):\n{pinned}"
    );
}
