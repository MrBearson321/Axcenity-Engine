//! `axcenity-updater apply <instruction.json>`: installs a staged update or
//! rolls back, after the editor that wrote the instruction has exited. The
//! editor starts it; people don't need to. See `apply.rs`.

use std::path::Path;
use std::process::ExitCode;

use axcenity_updater::apply::{self, Instruction};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(text) = axcenity_release::version_flag("axcenity-updater", &args) {
        println!("{text}");
        return ExitCode::SUCCESS;
    }
    let [command, path] = args.as_slice() else {
        eprintln!("usage: axcenity-updater apply <instruction.json>");
        return ExitCode::from(2);
    };
    if command != "apply" {
        eprintln!("usage: axcenity-updater apply <instruction.json>");
        return ExitCode::from(2);
    }
    let instruction: Instruction = match std::fs::read_to_string(Path::new(path))
        .map_err(|e| e.to_string())
        .and_then(|text| serde_json::from_str(&text).map_err(|e| e.to_string()))
    {
        Ok(instruction) => instruction,
        Err(error) => {
            eprintln!("couldn't read {path}: {error}");
            return ExitCode::FAILURE;
        }
    };
    if apply::run(&instruction, &mut apply::System).ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
