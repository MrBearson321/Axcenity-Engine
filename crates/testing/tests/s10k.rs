//! The S10k measurement scene (roadmap I1): it has the counts it promises,
//! and the project it writes opens and runs headless without script
//! errors. (Opening 10,000 parts takes about 30 s in a debug build, so the
//! project is opened once.)

use axcenity_core::{ClassName, Value};
use axcenity_testing::{Runner, s10k};

#[test]
fn s10k_has_its_counts_and_runs_frames_without_errors() {
    let world = s10k::world();
    let parts: Vec<_> = world
        .descendants(world.workspace())
        .into_iter()
        .filter(|&id| world.get(id).is_some_and(|i| i.class() == ClassName::Part))
        .collect();
    let anchored = parts
        .iter()
        .filter(|&&id| world.get(id).unwrap().property("Anchored") == Some(&Value::Bool(true)))
        .count();
    // Grid, spinners and floor are anchored; drops aren't. The Sounds'
    // holders and the agents' bodies are extra.
    assert_eq!(anchored, s10k::ANCHORED_PARTS + s10k::SOUNDS);
    assert_eq!(
        parts.len() - anchored,
        s10k::UNANCHORED_PARTS + s10k::AGENTS
    );
    assert_eq!(s10k::script_files().len(), s10k::SCRIPTS);

    let folder = std::env::temp_dir().join(format!("axcenity-s10k-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    s10k::write(&folder).unwrap();
    let mut runner = Runner::open(&folder).unwrap();
    let (run, problems) = runner.bench(0, 3).unwrap();
    assert!(problems.is_empty(), "{problems:#?}");
    assert_eq!(run.samples.frames(), 3);
    let stages: Vec<_> = run.samples.summaries().iter().map(|s| s.name).collect();
    assert_eq!(
        stages,
        [
            "Characters",
            "Agents",
            "Input",
            "Scripts",
            "Physics",
            "Heartbeat",
            "Animation",
            "RenderStep"
        ]
    );
    let _ = std::fs::remove_dir_all(&folder);
}
