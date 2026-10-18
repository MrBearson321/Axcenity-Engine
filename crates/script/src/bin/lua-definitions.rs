//! Rewrites `definitions/axcenity.lua` from the current script API. Run it
//! after changing the API (a test fails until you do):
//!
//! ```text
//! cargo run -p axcenity-script --bin lua-definitions
//! ```

use std::path::Path;

fn main() -> std::io::Result<()> {
    // CARGO_MANIFEST_DIR is crates/script, so this works from any folder.
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(axcenity_script::DEFINITIONS_PATH);
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)?;
    }
    std::fs::write(&path, axcenity_script::lua_definitions())?;
    println!("wrote {}", axcenity_script::DEFINITIONS_PATH);
    Ok(())
}
