//! Roadmap item M14's measure: how long a host's UI layout takes for a
//! 500-element inventory GUI styled by a stylesheet, with no GPU. One
//! frame is one `UiInteraction::layout`, the call the player and editor
//! Play make every frame, with the pointer moving over the slots so hover
//! styles change. Ignored by default:
//!
//! ```text
//! cargo test -p axcenity-ui --release --test bench_inventory_ui -- --ignored --nocapture
//! ```

use std::time::{Duration, Instant};

use axcenity_core::{ClassName, InstanceId, UDim2, Value, World};
use axcenity_ui::{UiInput, UiInteraction, layout_ui, parse_stylesheet};

const WARM_UP_FRAMES: usize = 30;
/// Each number printed is the fastest batch's median.
const BATCHES: usize = 5;
const BATCH_FRAMES: usize = 60;
const SCREEN: (u32, u32) = (1920, 1080);

/// A game's inventory stylesheet: variables on the root, classes, ids,
/// descendant selectors and hover and pressed states.
const STYLESHEET: &str = r"
#inventory { --slot: #2a2f3a; --accent: #f2c14e; --text: #e8e8e8; --radius: 6px;
  background-color: #1b1e25; padding: 16px; display: flex; flex-direction: column; gap: 8px; border-radius: 10px; }
#inventory TextLabel.title { color: var(--accent); font-size: 28px; font-weight: bold; height: 40px; }
#bag { display: flex; flex-direction: column; gap: 6px; flex-grow: 1; background-color: #00000000; }
.row { display: flex; flex-direction: row; gap: 6px; height: 88px; background-color: #00000000; }
.slot { width: 150px; background-color: var(--slot); border-radius: var(--radius); border: 1px solid #3a4150; }
.slot:hover { background-color: #343b49; border-color: var(--accent); }
.slot.rare { border-color: #5b8def; }
.slot.epic { border-color: #a35bef; }
#inventory .slot ImageLabel.icon { background-color: #00000000; }
.slot TextLabel { color: var(--text); background-color: #00000000; font-size: 14px; }
.slot TextLabel.count { text-align: right; font-weight: bold; }
.slot TextLabel.name { text-align: left; white-space: nowrap; }
.slot TextButton.use { background-color: #3d6b45; color: white; border-radius: 4px; font-size: 12px; }
.slot TextButton.use:hover { background-color: #4f8a59; }
.slot TextButton.use:active { background-color: #2e5234; }
.slot.empty TextButton.use { display: none; }
";

fn child(
    world: &mut World,
    parent: InstanceId,
    class: ClassName,
    props: &[(&str, Value)],
) -> InstanceId {
    let id = world.create(class).unwrap();
    world.set_parent(id, Some(parent)).unwrap();
    for (name, value) in props {
        world.set_property(id, name, value.clone()).unwrap();
    }
    id
}

fn text(value: &str) -> Value {
    Value::String(value.to_owned())
}

fn udim2(xs: f32, xo: f32, ys: f32, yo: f32) -> Value {
    Value::UDim2(UDim2::new(xs, xo, ys, yo))
}

/// A ScreenGui with a titled inventory of 10 rows of 10 slots, each slot an
/// icon, a count, a name and a Use button: 513 UI Instances, 499 of them
/// shown (empty slots hide their button).
fn inventory() -> (World, Vec<InstanceId>) {
    let mut world = World::new();
    let workspace = world.workspace();
    let gui = child(&mut world, workspace, ClassName::ScreenGui, &[]);
    let panel = child(
        &mut world,
        gui,
        ClassName::Frame,
        &[
            ("Id", text("inventory")),
            ("Position", udim2(0.0, 40.0, 0.0, 40.0)),
            ("Size", udim2(0.0, 1600.0, 0.0, 1000.0)),
        ],
    );
    child(
        &mut world,
        panel,
        ClassName::TextLabel,
        &[("Class", text("title")), ("Text", text("Inventory"))],
    );
    let bag = child(
        &mut world,
        panel,
        ClassName::ScrollingFrame,
        &[
            ("Id", text("bag")),
            ("CanvasSize", udim2(0.0, 0.0, 0.0, 940.0)),
        ],
    );
    let mut slots = Vec::new();
    for row_index in 0..10 {
        let row = child(&mut world, bag, ClassName::Frame, &[("Class", text("row"))]);
        for column in 0..10 {
            let n = row_index * 10 + column;
            let rarity = match n % 7 {
                0 => "slot rare",
                3 => "slot epic",
                5 => "slot empty",
                _ => "slot",
            };
            let slot = child(
                &mut world,
                row,
                ClassName::Frame,
                &[("Class", text(rarity))],
            );
            child(
                &mut world,
                slot,
                ClassName::ImageLabel,
                &[
                    ("Class", text("icon")),
                    ("Position", udim2(0.0, 6.0, 0.0, 6.0)),
                    ("Size", udim2(0.0, 48.0, 0.0, 48.0)),
                ],
            );
            child(
                &mut world,
                slot,
                ClassName::TextLabel,
                &[
                    ("Class", text("count")),
                    ("Text", text(&format!("x{}", n * 3 % 64 + 1))),
                    ("Position", udim2(1.0, -46.0, 0.0, 6.0)),
                    ("Size", udim2(0.0, 40.0, 0.0, 18.0)),
                ],
            );
            child(
                &mut world,
                slot,
                ClassName::TextLabel,
                &[
                    ("Class", text("name")),
                    ("Text", text(&format!("Item {n}"))),
                    ("Position", udim2(0.0, 6.0, 1.0, -24.0)),
                    ("Size", udim2(0.6, 0.0, 0.0, 18.0)),
                ],
            );
            child(
                &mut world,
                slot,
                ClassName::TextButton,
                &[
                    ("Class", text("use")),
                    ("Text", text("Use")),
                    ("Position", udim2(1.0, -50.0, 1.0, -26.0)),
                    ("Size", udim2(0.0, 44.0, 0.0, 20.0)),
                ],
            );
            slots.push(slot);
        }
    }
    (world, slots)
}

fn median_ms(mut frame: impl FnMut(usize) -> Duration) -> f64 {
    for index in 0..WARM_UP_FRAMES {
        frame(index);
    }
    let median = (0..BATCHES)
        .map(|_| {
            let mut times: Vec<Duration> = (0..BATCH_FRAMES).map(&mut frame).collect();
            times.sort();
            times[BATCH_FRAMES / 2]
        })
        .min()
        .unwrap();
    median.as_secs_f64() * 1000.0
}

#[test]
#[ignore = "benchmark: run with --release -- --ignored --nocapture"]
fn bench_inventory_500_elements() {
    let (world, slots) = inventory();
    let (sheet, warnings) = parse_stylesheet("inventory.css", STYLESHEET);
    assert!(warnings.is_empty(), "{warnings:?}");
    let sheets = [sheet];

    let layout = layout_ui(&world, SCREEN.0, SCREEN.1, &sheets);
    assert!(
        layout.elements.len() >= 490,
        "{} elements",
        layout.elements.len()
    );
    // Where each slot is, to move the pointer over them.
    let centers: Vec<[f32; 2]> = slots
        .iter()
        .filter_map(|&id| layout.element(id))
        .map(|element| {
            let rect = element.rect;
            [rect.x + rect.width / 2.0, rect.y + rect.height / 2.0]
        })
        .collect();
    assert!(!centers.is_empty());

    let one_pass = median_ms(|_| {
        let start = Instant::now();
        let layout = layout_ui(&world, SCREEN.0, SCREEN.1, &sheets);
        let took = start.elapsed();
        drop(layout);
        took
    });

    let mut ui = UiInteraction::default();
    let interactive = median_ms(|frame| {
        // A new slot hovered every few frames, as a moving mouse does.
        let pointer = centers[(frame / 4) % centers.len()];
        let input = UiInput {
            pointer: Some(pointer),
            ..UiInput::default()
        };
        let start = Instant::now();
        let layout = ui.layout(&world, SCREEN, &sheets, input, None, 1.0 / 60.0);
        let took = start.elapsed();
        assert!(layout.hovered.is_some());
        drop(layout);
        took
    });

    // Editor Play's viewport: a hit test layout for clicks first, then the
    // frame's layout.
    let mut ui = UiInteraction::default();
    let editor = median_ms(|frame| {
        let pointer = centers[(frame / 4) % centers.len()];
        let input = UiInput {
            pointer: Some(pointer),
            ..UiInput::default()
        };
        let start = Instant::now();
        let hit = ui.hit_layout(&world, SCREEN, &sheets, input, None);
        assert!(hit.hit_button(pointer[0], pointer[1]).is_none());
        let layout = ui.layout(&world, SCREEN, &sheets, input, None, 1.0 / 60.0);
        let took = start.elapsed();
        drop(layout);
        took
    });

    println!(
        "bench_inventory_500_elements: {} elements, layout_ui {one_pass:.3} ms, \
         UiInteraction::layout (player frame) {interactive:.3} ms, \
         hit_layout + layout (editor Play frame) {editor:.3} ms",
        layout.elements.len()
    );
}
