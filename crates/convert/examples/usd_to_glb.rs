//! Converts USD files to `.glb` beside them and prints what came across:
//! `cargo run -p axcenity-convert --example usd_to_glb -- scene.usdz …`.

fn main() {
    for path in std::env::args().skip(1) {
        let path = std::path::Path::new(&path);
        let started = std::time::Instant::now();
        match axcenity_convert::convert(path) {
            Ok(converted) => {
                let out = path.with_extension("glb");
                if let Err(err) = std::fs::write(&out, &converted.glb) {
                    eprintln!(
                        "{}: couldn't write {}: {err}",
                        path.display(),
                        out.display()
                    );
                    continue;
                }
                println!(
                    "{} -> {} ({} KB, {} ms)",
                    path.display(),
                    out.display(),
                    converted.glb.len() / 1024,
                    started.elapsed().as_millis()
                );
                for warning in converted.warnings {
                    println!("  warning: {warning}");
                }
            }
            Err(err) => println!("{}: {err}", path.display()),
        }
    }
}
