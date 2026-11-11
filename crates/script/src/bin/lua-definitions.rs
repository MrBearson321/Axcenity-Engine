//! Rewrites `definitions/axcenity.d.luau` and the generated API reference
//! pages (`documentation/api/` and the API section of
//! `documentation/SUMMARY.md`) from the current script API. Run it after
//! changing the API (a test fails until you do):
//!
//! ```text
//! cargo run -p axcenity-script --bin lua-definitions
//! ```

use std::path::Path;

fn main() -> std::io::Result<()> {
    // CARGO_MANIFEST_DIR is crates/script, so this works from any folder.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = root.join(axcenity_script::DEFINITIONS_PATH);
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)?;
    }
    std::fs::write(&path, axcenity_script::lua_definitions())?;
    println!("wrote {}", axcenity_script::DEFINITIONS_PATH);

    // The pages are all generated, so a page for something removed goes too.
    let api = root.join(axcenity_script::API_DOCS_PATH);
    if api.exists() {
        std::fs::remove_dir_all(&api)?;
    }
    let pages = axcenity_script::api_markdown();
    for (name, text) in &pages {
        let path = api.join(name);
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        std::fs::write(path, text)?;
    }
    println!(
        "wrote {} pages in {}",
        pages.len(),
        axcenity_script::API_DOCS_PATH
    );

    let summary = root.join(axcenity_script::SUMMARY_PATH);
    let text = std::fs::read_to_string(&summary).unwrap_or_default();
    std::fs::write(&summary, axcenity_script::with_api_summary(&text))?;
    println!("wrote the API section of {}", axcenity_script::SUMMARY_PATH);
    Ok(())
}
