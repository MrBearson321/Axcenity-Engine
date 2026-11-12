//! Step 7's measurements, printed as the tables in `docs/mcp/benchmarks.md`:
//! definition tokens per profile, each tool's result size and time, calls
//! and tokens per reference workflow, the open editor's round trip and
//! idle cost, and the large scene (S10k). Ignored by default: run with
//! `cargo test -p axcenity-editor --release --bin axcenity-editor -- mcp::benchmarks --ignored --nocapture`.

use std::time::{Duration, Instant};

use axcenity_mcp::{Mode, Profile, text::estimate_tokens, tools_list_json};
use serde_json::{Value, json};

use super::super::EditorApp;
use super::super::tests::copy_of_example;
use super::tests::{LiveClient, call, open};
use super::{Headless, INSTRUCTIONS, tools_for};

/// Milliseconds, to two decimals.
fn ms(time: Duration) -> String {
    format!("{:.2}", time.as_secs_f64() * 1000.0)
}

/// The median of `runs` timings of `work`.
fn median(runs: usize, mut work: impl FnMut()) -> Duration {
    let mut times: Vec<Duration> = (0..runs)
        .map(|_| {
            let started = Instant::now();
            work();
            started.elapsed()
        })
        .collect();
    times.sort();
    times[times.len() / 2]
}

/// A call's text whether it worked or not (an error is a result too).
fn text(host: &mut Headless, tool: &str, args: &Value) -> String {
    match call(host, tool, args.clone()) {
        Ok(text) => text,
        Err(error) => error.to_string(),
    }
}

#[test]
#[ignore = "measurements: run by hand, see the module comment"]
fn benchmarks() {
    definitions();
    tools();
    workflows();
    live();
    large_scene();
}

fn definitions() {
    println!("\n## Definitions\n\n| Profile | Tools | Tokens (tools/list) |\n|---|---|---|");
    for profile in [Profile::Lite, Profile::Standard, Profile::Full] {
        let tools = tools_for(profile);
        println!(
            "| {} | {} | {} |",
            profile.name(),
            tools.len(),
            estimate_tokens(&tools_list_json(&tools))
        );
    }
    println!("| instructions | | {} |", estimate_tokens(INSTRUCTIONS));
}

/// Each tool once on hello-part, as a model would call it first.
fn tools() {
    println!(
        "\n## Tools on hello-part\n\n| Tool | Call | Result tokens | Time (ms) |\n|---|---|---|---|"
    );
    let calls: &[(&str, Value)] = &[
        ("project_info", json!({})),
        ("scene_inspect", json!({})),
        ("scene_inspect", json!({"root": "game", "depth": 3})),
        ("scene_inspect", json!({"class": "BasePart"})),
        ("object_get", json!({"id": "Workspace/Block"})),
        (
            "object_get",
            json!({"id": "Workspace/Block", "detail": "full"}),
        ),
        ("docs", json!({})),
        ("docs", json!({"class": "Part"})),
        ("script_read", json!({})),
        ("project_check", json!({"scope": "scripts"})),
        ("project_check", json!({"scope": "assets"})),
        ("project_check", json!({})),
        ("history", json!({})),
        ("history", json!({"action": "changes"})),
        ("asset_find", json!({})),
        ("level", json!({})),
        ("style_edit", json!({})),
        ("project_settings", json!({"setting": "input"})),
        (
            "object_create",
            json!({"kind": "Block", "name": "Crate", "color": [1, 0, 0]}),
        ),
        (
            "object_set",
            json!({"id": "Workspace/Crate", "props": {"Anchored": true, "Transparency": 0.5}}),
        ),
        (
            "object_duplicate",
            json!({"id": "Workspace/Crate", "count": 3, "offset": [0, 0, 6]}),
        ),
        ("object_delete", json!({"id": "Workspace/Crate"})),
        (
            "script_write",
            json!({"name": "Spin", "side": "server", "text": "local part = workspace.Block\nprint(part.Name)\n"}),
        ),
        (
            "material_edit",
            json!({"action": "create", "name": "Rust", "values": {"color": [0.5, 0.2, 0.1]}, "id": "Workspace/Block"}),
        ),
        ("env_set", json!({"preset": "sunset"})),
        (
            "terrain_edit",
            json!({"action": "generate", "preset": "mountain_range", "seed": 7}),
        ),
        ("game_run", json!({"seconds": 1})),
        ("history", json!({"action": "undo"})),
    ];
    let mut opened = open("hello-part", "mcp-bench-tools");
    let host = &mut opened.app;
    for (tool, args) in calls {
        let started = Instant::now();
        let result = text(host, tool, args);
        let time = started.elapsed();
        let shown = args.to_string();
        let shown = if shown.len() > 48 {
            format!(
                "{}…",
                &shown[..shown.char_indices().nth(47).map_or(shown.len(), |(i, _)| i)]
            )
        } else {
            shown
        };
        println!(
            "| {tool} | `{shown}` | {} | {} |",
            estimate_tokens(&result),
            ms(time)
        );
    }
}

/// Tokens a route costs a model: what it sends (the arguments) plus what
/// comes back, over all its calls.
fn route(host: &mut Headless, calls: &[(&str, Value)]) -> (usize, usize, Duration) {
    let started = Instant::now();
    let mut tokens = 0;
    for (tool, args) in calls {
        tokens += estimate_tokens(&args.to_string()) + estimate_tokens(&text(host, tool, args));
    }
    (calls.len(), tokens, started.elapsed())
}

fn workflows() {
    println!(
        "\n## Reference workflows\n\n| Workflow | Route | Calls | Tokens sent + returned | Time (ms) |\n|---|---|---|---|---|"
    );
    let hut_low: Vec<(&str, Value)> = vec![
        ("object_create", json!({"kind": "Model", "name": "Hut"})),
        (
            "object_create",
            json!({"kind": "Block", "name": "Floor", "parent": "Workspace/Hut", "size": [10, 1, 10]}),
        ),
        (
            "object_create",
            json!({"kind": "Block", "name": "Wall", "parent": "Workspace/Hut", "position": [0, 3, -5], "size": [10, 5, 1]}),
        ),
        (
            "object_create",
            json!({"kind": "Block", "name": "Back", "parent": "Workspace/Hut", "position": [0, 3, 5], "size": [10, 5, 1]}),
        ),
        (
            "object_create",
            json!({"kind": "Block", "name": "Left", "parent": "Workspace/Hut", "position": [-5, 3, 0], "size": [1, 5, 10]}),
        ),
        (
            "object_create",
            json!({"kind": "Block", "name": "Right", "parent": "Workspace/Hut", "position": [5, 3, 0], "size": [1, 5, 10]}),
        ),
        (
            "object_create",
            json!({"kind": "Block", "name": "Roof", "parent": "Workspace/Hut", "position": [0, 6, 0], "size": [12, 1, 12]}),
        ),
        (
            "object_create",
            json!({"kind": "PointLight", "parent": "Workspace/Hut/Roof"}),
        ),
    ];
    let hut_batch: Vec<(&str, Value)> = vec![(
        "batch",
        json!({"label": "Build a hut", "ops": [
            {"op": "create", "kind": "Model", "name": "Hut", "as": "hut"},
            {"op": "create", "kind": "Block", "name": "Floor", "parent": "$hut", "size": [10, 1, 10]},
            {"op": "create", "kind": "Block", "name": "Wall", "parent": "$hut", "position": [0, 3, -5], "size": [10, 5, 1]},
            {"op": "create", "kind": "Block", "name": "Back", "parent": "$hut", "position": [0, 3, 5], "size": [10, 5, 1]},
            {"op": "create", "kind": "Block", "name": "Left", "parent": "$hut", "position": [-5, 3, 0], "size": [1, 5, 10]},
            {"op": "create", "kind": "Block", "name": "Right", "parent": "$hut", "position": [5, 3, 0], "size": [1, 5, 10]},
            {"op": "create", "kind": "Block", "name": "Roof", "parent": "$hut", "position": [0, 6, 0], "size": [12, 1, 12], "as": "roof"},
            {"op": "create", "kind": "PointLight", "parent": "$roof"}
        ]}),
    )];
    let sunset_low: Vec<(&str, Value)> = vec![
        ("docs", json!({"search": "atmosphere"})),
        (
            "object_set",
            json!({"id": "Lighting", "props": {"ClockTime": 18.2, "FogEnd": 600}}),
        ),
        (
            "object_create",
            json!({"kind": "Atmosphere", "parent": "Lighting", "props": {"Haze": 2}}),
        ),
        (
            "object_create",
            json!({"kind": "Clouds", "parent": "Workspace/Terrain"}),
        ),
        (
            "object_create",
            json!({"kind": "BloomEffect", "parent": "Lighting", "props": {"Intensity": 0.6}}),
        ),
        (
            "object_create",
            json!({"kind": "ColorCorrectionEffect", "parent": "Lighting", "props": {"TintColor": [1, 0.85, 0.7]}}),
        ),
    ];
    let sunset_high: Vec<(&str, Value)> = vec![("env_set", json!({"preset": "sunset"}))];
    let script_flow: Vec<(&str, Value)> = vec![
        (
            "script_write",
            json!({"name": "Door", "side": "server", "text": "local door = workspace.Block\ndoor.Touched:Connect(function()\n\tdoor.Transparency = 0.5\nend)\n"}),
        ),
        ("game_run", json!({"seconds": 1})),
        ("script_read", json!({"path": "scripts/Door.server.lua"})),
    ];
    for (name, routes) in [
        (
            "Small hut (model, floor, 4 walls, roof, light)",
            vec![("low-level", hut_low), ("batch", hut_batch)],
        ),
        (
            "Sunset look",
            vec![("low-level", sunset_low), ("env_set", sunset_high)],
        ),
        ("Door script and try it", vec![("as is", script_flow)]),
    ] {
        for (label, calls) in routes {
            // Each route on a fresh copy, so they don't see each other's work.
            let mut opened = open("hello-part", "mcp-bench-flow");
            let (count, tokens, time) = route(&mut opened.app, &calls);
            println!("| {name} | {label} | {count} | {tokens} | {} |", ms(time));
        }
    }
}

/// The open editor: a client's round trip with the editor drawing at 60
/// frames a second (calls run at the start of a frame), and what an idle
/// frame costs with a client connected.
fn live() {
    use super::live::Live;
    println!("\n## In the open editor\n\n| Measure | Value |\n|---|---|");
    let folder = copy_of_example("hello-part", "mcp-bench-live");
    let mut app = EditorApp::new(Some(&folder));
    let mut live = Live::start(&folder, axcenity_mcp::Waker(std::sync::Arc::new(|| {}))).unwrap();
    for (label, frame) in [
        (
            "round trip, `project_info`, frames every 16.7 ms",
            Duration::from_micros(16_667),
        ),
        (
            "round trip, `project_info`, frames back to back (woken)",
            Duration::from_micros(200),
        ),
    ] {
        let client_folder = folder.clone();
        let client = std::thread::spawn(move || {
            let mut client = LiveClient::connect(&client_folder, Mode::Edit);
            let mut times: Vec<Duration> = (0..30)
                .map(|_| {
                    let started = Instant::now();
                    let (_, failed) = client.call("project_info", json!({}));
                    assert!(!failed);
                    started.elapsed()
                })
                .collect();
            times.sort();
            times[times.len() / 2]
        });
        while !client.is_finished() {
            live.drain(&mut app);
            std::thread::sleep(frame);
        }
        // The connection closes; one more frame lets go of it.
        let round_trip = client.join().unwrap();
        live.drain(&mut app);
        println!("| {label} (median of 30) | {} ms |", ms(round_trip));
    }
    // A connected client that sends nothing: the frame's cost.
    let client_folder = folder.clone();
    let (done, wait) = std::sync::mpsc::channel::<()>();
    let client = std::thread::spawn(move || {
        let _client = LiveClient::connect(&client_folder, Mode::Edit);
        wait.recv().unwrap();
    });
    // Until the client is in.
    let started = Instant::now();
    while live.clients().is_empty() && started.elapsed() < Duration::from_secs(10) {
        live.drain(&mut app);
        std::thread::sleep(Duration::from_millis(1));
    }
    let idle = median(9, || {
        for _ in 0..1000 {
            live.drain(&mut app);
        }
    }) / 1000;
    println!(
        "| idle frame with a client connected | {:.2} µs |",
        idle.as_secs_f64() * 1e6
    );
    done.send(()).unwrap();
    client.join().unwrap();
    drop(live);
    let _ = std::fs::remove_dir_all(&folder);
}

/// S10k, the standard measurement scene (roadmap I1).
fn large_scene() {
    println!("\n## Large scene (S10k)\n\n| Call | Result tokens | Time (ms) |\n|---|---|---|");
    let folder =
        std::env::temp_dir().join(format!("axcenity-mcp-bench-s10k-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    axcenity_testing::s10k::write(&folder).unwrap();
    let started = Instant::now();
    let app = EditorApp::new(Some(&folder));
    assert!(app.project.is_some());
    println!(
        "| open the project (what `--mcp` does before its first answer) | | {} |",
        ms(started.elapsed())
    );
    let mut host = Headless::new(app, Mode::Edit);
    let calls: &[(&str, Value)] = &[
        ("project_info", json!({})),
        ("scene_inspect", json!({})),
        ("scene_inspect", json!({"class": "BasePart"})),
        (
            "scene_inspect",
            json!({"class": "BasePart", "cursor": "5000"}),
        ),
        ("scene_inspect", json!({"name": "Spinner"})),
        ("scene_inspect", json!({"near": [0, 0, 0], "radius": 20})),
        ("project_check", json!({})),
        ("history", json!({"action": "changes"})),
        ("object_create", json!({"kind": "Block", "name": "Marker"})),
        ("history", json!({"action": "changes"})),
        ("object_delete", json!({"id": "Workspace/Marker"})),
        ("history", json!({"action": "undo"})),
    ];
    for (tool, args) in calls {
        let started = Instant::now();
        let result = text(&mut host, tool, args);
        println!(
            "| {tool} `{args}` | {} | {} |",
            estimate_tokens(&result),
            ms(started.elapsed())
        );
    }
    drop(host);
    let _ = std::fs::remove_dir_all(&folder);
}
