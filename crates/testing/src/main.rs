//! `axcenity-test`: runs a project's game tests without a window and exits
//! with 1 when any fail, for CI. `--junit <file>` also writes the results
//! as JUnit XML.
//!
//! Benchmarks (roadmap I1, `docs/development/benchmarks.md`):
//! `--make-s10k <folder>` writes the S10k measurement scene there, and
//! `--bench <frames>` runs a project's start level headless, as a test
//! does, and prints each frame step's times instead of running tests.

use std::path::PathBuf;
use std::process::ExitCode;

use axcenity_core::bench::BenchRun;
use axcenity_testing::{RunEvent, Runner, Selection, junit};

const USAGE: &str = "usage: axcenity-test <project-folder> [--junit <results.xml>] [--bench <frames>]\n\
                     \x20      axcenity-test --make-s10k <folder>\n\
                     example: cargo run -p axcenity-testing -- examples/hello-character";

fn main() -> ExitCode {
    let mut project = None;
    let mut junit_path = None;
    let mut bench = None;
    // `args_os` instead of `args` so folder names that aren't valid Unicode still work.
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--junit" {
            match args.next() {
                Some(path) => junit_path = Some(PathBuf::from(path)),
                None => return usage("--junit needs a file to write"),
            }
        } else if arg == "--bench" {
            match args
                .next()
                .and_then(|frames| frames.to_str()?.parse::<usize>().ok())
                .filter(|&frames| frames > 0)
            {
                Some(frames) => bench = Some(frames),
                None => return usage("--bench needs how many frames to measure, such as 600"),
            }
        } else if arg == "--make-s10k" {
            let Some(folder) = args.next() else {
                return usage("--make-s10k needs a folder to write the scene in");
            };
            let folder = PathBuf::from(folder);
            if let Err(err) = axcenity_testing::s10k::write(&folder) {
                eprintln!("error: could not write `{}`: {err}", folder.display());
                return ExitCode::from(2);
            }
            println!(
                "{} written to `{}`",
                axcenity_testing::s10k::summary(),
                folder.display()
            );
            return ExitCode::SUCCESS;
        } else if arg == "--help" || arg == "-h" {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        } else if project.is_none() {
            project = Some(PathBuf::from(arg));
        } else {
            return usage("give one project folder");
        }
    }
    let Some(project) = project else {
        return usage("give a project folder");
    };

    let mut runner = match Runner::open(&project) {
        Ok(runner) => runner,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::from(2);
        }
    };
    if let Some(frames) = bench {
        return match runner.bench(BenchRun::DEFAULT_WARM_UP, frames) {
            Ok((run, problems)) => {
                for problem in problems {
                    eprintln!("{problem}");
                }
                let scene = axcenity_core::bench::scene_name(&project);
                print!("{}", run.finish_text("testing", &scene));
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("error: {err}");
                ExitCode::from(2)
            }
        };
    }
    let files = runner.spec_files();
    println!(
        "Testing \"{}\": {} spec file{}",
        runner.name(),
        files.len(),
        if files.len() == 1 { "" } else { "s" }
    );
    let results = runner.run(&Selection::All, &mut |event| {
        if let RunEvent::Finished(result) = event {
            match &result.failure {
                None => println!("PASS {} > {}", result.file, result.title()),
                Some(failure) => {
                    println!("FAIL {} > {}", result.file, result.title());
                    for line in failure.message.lines() {
                        println!("     {line}");
                    }
                    for line in &result.output {
                        println!("     | {line}");
                    }
                }
            }
        }
    });
    let failed = results.iter().filter(|result| !result.passed()).count();
    println!("{} passed, {failed} failed", results.len() - failed);
    if let Some(path) = junit_path
        && let Err(err) = std::fs::write(&path, junit::junit_xml(runner.name(), &results))
    {
        eprintln!("error: could not write `{}`: {err}", path.display());
        return ExitCode::from(2);
    }
    if failed > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn usage(problem: &str) -> ExitCode {
    eprintln!("error: {problem}\n\n{USAGE}");
    ExitCode::from(2)
}
