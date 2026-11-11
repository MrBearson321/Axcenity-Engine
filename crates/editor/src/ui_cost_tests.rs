//! How long the Explorer and the Console take to draw with many rows
//! (design system V7 and V8), and Properties with that many Parts selected
//! (V5). Ignored: run it in release for numbers,
//! `cargo test -p axcenity-editor --release --bins ui_cost -- --ignored --nocapture`.
//! Each panel is drawn alone in a window of its usual shape, after three
//! frames to warm up, and the time per frame is averaged over 20 frames.

use std::time::Instant;

use axcenity_core::{ClassName, World};
use eframe::egui;

use crate::explorer::Explorer;
use crate::output::Output;
use crate::properties;
use crate::theme;

const COUNTS: [usize; 3] = [100, 1_000, 10_000];

/// Milliseconds per frame drawing `draw` in a window `size` big.
fn time(size: egui::Vec2, mut draw: impl FnMut(&mut egui::Ui)) -> f64 {
    let ctx = theme::test_context();
    let mut frame = || {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            ..Default::default()
        };
        let _ = crate::theme::run(&ctx, input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| draw(ui));
        });
    };
    for _ in 0..3 {
        frame();
    }
    let frames = 20;
    let start = Instant::now();
    for _ in 0..frames {
        frame();
    }
    start.elapsed().as_secs_f64() * 1000.0 / frames as f64
}

/// A Folder of `count` Parts in the Workspace, open.
fn folder_of_parts(count: usize) -> (World, Explorer) {
    let mut world = World::new();
    let folder = world.create(ClassName::Folder).unwrap();
    world.set_parent(folder, Some(world.workspace())).unwrap();
    for _ in 0..count {
        let part = world.create(ClassName::Part).unwrap();
        world.set_parent(part, Some(folder)).unwrap();
    }
    let mut explorer = Explorer::new(&world);
    explorer.toggle(folder);
    (world, explorer)
}

/// The per-frame times at each count. The plan's targets: 10,000 rows cost
/// at most twice 100, and Properties with 1,000 selected stays well under
/// a frame.
#[test]
#[ignore = "timing; run in release with --ignored --nocapture"]
fn explorer_and_console_cost_at_100_1000_and_10000_rows() {
    let mut explorer_ms = Vec::new();
    let mut console_ms = Vec::new();
    let mut properties_ms = Vec::new();
    for count in COUNTS {
        let (world, mut explorer) = folder_of_parts(count);
        explorer_ms.push(time(egui::vec2(300.0, 800.0), |ui| {
            explorer.show(ui, &world, &[], true);
        }));
        let parts = world.descendants(world.workspace());
        properties_ms.push(time(egui::vec2(320.0, 900.0), |ui| {
            let _ = properties::show_many(ui, &world, &Default::default(), &parts, false);
        }));
        let mut output = Output::default();
        for index in 0..count {
            output.info(format!("Line {index}: something a script printed"));
        }
        console_ms.push(time(egui::vec2(900.0, 260.0), |ui| {
            output.show(ui);
        }));
    }
    println!("| Rows | Explorer | Console | Properties, all selected |");
    println!("|---:|---:|---:|---:|");
    for (index, count) in COUNTS.iter().enumerate() {
        println!(
            "| {count} | {:.2} ms | {:.2} ms | {:.2} ms |",
            explorer_ms[index], console_ms[index], properties_ms[index]
        );
    }
}
