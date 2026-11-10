//! `intel_tex_2` links a C++ part of Intel's compressor (its ASTC encoder,
//! which the engine doesn't use) but not the C++ runtime that part needs on
//! Linux and macOS; MSVC links its own. Without this the editor fails to link
//! there ("undefined symbol: __gxx_personality_v0").

fn main() {
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    match (os.as_str(), env.as_str()) {
        ("macos" | "ios", _) => println!("cargo:rustc-link-lib=c++"),
        (_, "msvc") => {}
        _ => println!("cargo:rustc-link-lib=stdc++"),
    }
}
