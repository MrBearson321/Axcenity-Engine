//! The legend in the viewport's corner for the debug views whose colors
//! stand for numbers: LOD Levels (which color is which level, how many
//! items in view draw each, and the thresholds) and Overdraw (how many
//! layers each shade is, and on request the frame's numbers,
//! `OverdrawStats`), and Ray Hits (what the ray traced scene holds). What they measure: `docs/rendering/performance.md`.

use axcenity_render::{
    DebugView, LOD_LEVELS, LOD_SCREEN_SIZES, OVERDRAW_STEP, OverdrawStats, RenderStats,
};
use eframe::egui::{self, Color32, RichText};

use super::ViewMode;

/// The LOD Levels view's colors in linear light, as `lod_color` in
/// debug_views.wgsl draws them: each level, then a mesh with none.
const LOD_COLORS: [[f32; 3]; 4] = [
    [0.2, 0.8, 0.25],
    [0.95, 0.85, 0.2],
    [0.95, 0.5, 0.15],
    [0.9, 0.2, 0.2],
];
const NO_LEVELS: [f32; 3] = [0.5, 0.5, 0.5];

/// The layer counts the Overdraw legend shows a swatch for.
const OVERDRAW_SWATCHES: [u32; 6] = [1, 2, 4, 8, 16, 32];

/// The screen color of `linear` light, as an sRGB target shows it.
pub fn shown(linear: [f32; 3]) -> Color32 {
    let channel = |value: f32| {
        let value = value.clamp(0.0, 1.0);
        let encoded = if value <= 0.003_130_8 {
            value * 12.92
        } else {
            1.055 * value.powf(1.0 / 2.4) - 0.055
        };
        (encoded * 255.0).round() as u8
    };
    Color32::from_rgb(channel(linear[0]), channel(linear[1]), channel(linear[2]))
}

/// The Overdraw view's color for a pixel `layers` deep.
pub fn overdraw_color(layers: u32) -> Color32 {
    shown(OVERDRAW_STEP.map(|step| step * layers as f32))
}

/// The legend's lines for the LOD Levels view: each level with how many
/// items in view draw it, then the thresholds.
pub fn lod_lines(stats: &RenderStats) -> Vec<(Option<Color32>, String)> {
    let mut lines = Vec::new();
    for (level, color) in LOD_COLORS.iter().take(LOD_LEVELS).enumerate() {
        let name = if level == 0 {
            "LOD0, its own mesh".to_owned()
        } else {
            format!("LOD{level}, a simpler copy")
        };
        lines.push((
            Some(shown(*color)),
            format!("{name}: {} in view", stats.lod_items[level]),
        ));
    }
    lines.push((Some(shown(NO_LEVELS)), "No simpler copies".to_owned()));
    let thresholds: Vec<String> = LOD_SCREEN_SIZES
        .iter()
        .enumerate()
        .map(|(index, size)| format!("LOD{} below {:.0}%", index + 1, size * 100.0))
        .collect();
    lines.push((
        None,
        format!(
            "By size on screen (share of the view's height): {}. Switches back 15% past it.",
            thresholds.join(", ")
        ),
    ));
    lines
}

/// The legend's lines for the Overdraw view, with the frame's numbers once
/// measured.
pub fn overdraw_lines(stats: Option<&OverdrawStats>) -> Vec<(Option<Color32>, String)> {
    let mut lines: Vec<(Option<Color32>, String)> = OVERDRAW_SWATCHES
        .iter()
        .map(|&layers| {
            let label = match layers {
                1 => "1 surface".to_owned(),
                32 => "32 or more".to_owned(),
                n => format!("{n} surfaces"),
            };
            (Some(overdraw_color(layers)), label)
        })
        .collect();
    lines.push((
        None,
        "Surfaces each pixel looks through (depth complexity), not how often it's lit: \
         cut-outs are lit once per pixel."
            .to_owned(),
    ));
    if let Some(stats) = stats {
        lines.extend(
            stats
                .report()
                .lines()
                .map(|line| (None, line.trim().to_owned())),
        );
    }
    lines
}

/// The legend's lines for the Ray Hits view: what the ray traced scene
/// held this frame (`docs/rendering/ray-tracing.md`).
pub fn ray_hits_lines(stats: &RenderStats) -> Vec<(Option<Color32>, String)> {
    let rays = stats.ray_tracing;
    if rays.meshes == 0 && rays.meshes_waiting == 0 {
        return vec![(
            None,
            "Nothing traced: Hardware ray tracing is off or this GPU can't ray trace \
             (Settings > Graphics says which), or nothing is within the ray tracing distance."
                .to_owned(),
        )];
    }
    let mut lines = vec![
        (None, format!("{} items traced", rays.instances)),
        (
            None,
            format!(
                "{} meshes, {} triangles",
                rays.meshes,
                thousands(rays.triangles)
            ),
        ),
    ];
    if rays.meshes_waiting > 0 {
        lines.push((
            None,
            format!(
                "{} meshes still being built (a few each frame)",
                rays.meshes_waiting
            ),
        ));
    }
    lines.push((
        None,
        "Each item its own color, dimmer farther away. Items past the ray tracing distance \
         and animated characters aren't traced yet."
            .to_owned(),
    ));
    lines
}

/// `value` with thin separators every three digits: 1,234,567.
pub(super) fn thousands(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// The Path Traced view's legend: how many paths each pixel has averaged
/// so far, or why nothing is traced.
pub fn path_traced_lines(stats: &RenderStats) -> Vec<(Option<Color32>, String)> {
    let rays = stats.ray_tracing;
    if rays.path_traced_paths == 0 {
        return vec![(
            None,
            "Nothing traced: Hardware ray tracing is off or this GPU can't ray trace \
             (Settings > Graphics says which)."
                .to_owned(),
        )];
    }
    let most = axcenity_render::PATH_TRACED_MOST_PATHS;
    let paths = if rays.path_traced_paths >= most {
        format!("{} paths a pixel: done", thousands(u64::from(most)))
    } else {
        format!(
            "{} paths a pixel so far",
            thousands(u64::from(rays.path_traced_paths))
        )
    };
    vec![
        (None, paths),
        (None, format!("{} items traced", rays.instances)),
        (
            None,
            "Keep the camera still for a clean picture: moving it, or anything in the \
             scene, starts again."
                .to_owned(),
        ),
    ]
}

/// Draws the legend for `view_mode` at the top left of `rect`, if it
/// has one. True when Measure was clicked (Overdraw only).
pub fn paint(
    ui: &egui::Ui,
    rect: egui::Rect,
    view_mode: ViewMode,
    stats: &RenderStats,
    overdraw: Option<&OverdrawStats>,
) -> bool {
    let (title, lines) = match view_mode {
        ViewMode::Debug(DebugView::LodLevels) => ("LOD Levels", lod_lines(stats)),
        ViewMode::Debug(DebugView::Overdraw) => ("Overdraw", overdraw_lines(overdraw)),
        ViewMode::Debug(DebugView::RayHits) => ("Ray Hits", ray_hits_lines(stats)),
        ViewMode::Debug(DebugView::PathTraced) => ("Path Traced", path_traced_lines(stats)),
        _ => return false,
    };
    let mut measure = false;
    egui::Area::new(egui::Id::new("view mode legend"))
        .order(egui::Order::Foreground)
        .fixed_pos(rect.left_top() + egui::vec2(10.0, 10.0))
        .constrain_to(rect)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_max_width(320.0);
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.label(RichText::new(title).strong());
                for (swatch, text) in &lines {
                    ui.horizontal(|ui| {
                        if let Some(color) = swatch {
                            let (square, _) = ui
                                .allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                            ui.painter().rect_filled(square, 2.0, *color);
                        }
                        ui.add(egui::Label::new(RichText::new(text).small()).wrap());
                    });
                }
                if matches!(view_mode, ViewMode::Debug(DebugView::Overdraw)) {
                    measure = ui
                        .button("Measure")
                        .on_hover_text("Reads this frame's overdraw back as numbers")
                        .clicked();
                }
            });
        });
    measure
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lod_legend_names_each_level_its_count_and_the_thresholds() {
        let stats = RenderStats {
            lod_items: [10, 4, 2],
            ..RenderStats::default()
        };
        let lines = lod_lines(&stats);
        assert_eq!(lines[0].1, "LOD0, its own mesh: 10 in view");
        assert_eq!(lines[2].1, "LOD2, a simpler copy: 2 in view");
        assert_eq!(lines[3].1, "No simpler copies");
        assert!(
            lines[4].1.contains("LOD1 below 30%, LOD2 below 12%"),
            "{}",
            lines[4].1
        );
        // The swatches are the shader's colors as the screen shows them.
        assert_eq!(lines[0].0, Some(shown([0.2, 0.8, 0.25])));
    }

    #[test]
    fn the_overdraw_legend_shades_brighter_with_more_layers_and_adds_the_numbers() {
        let red = |layers| overdraw_color(layers).r();
        assert!(red(1) < red(2) && red(2) < red(4) && red(4) < red(8));
        assert_eq!(red(16), 255, "red is full from about 8 layers");
        let without = overdraw_lines(None);
        let stats = OverdrawStats::from_pixels(&[0, 0, 255, 255], 1, 1);
        let with = overdraw_lines(Some(&stats));
        assert!(with.len() > without.len());
        assert!(
            with.iter().any(|(_, text)| text.contains("max 50+")),
            "{with:?}"
        );
    }

    #[test]
    fn the_ray_hits_legend_counts_the_scene_or_says_why_its_empty() {
        let empty = ray_hits_lines(&RenderStats::default());
        assert_eq!(empty.len(), 1);
        assert!(
            empty[0].1.contains("Hardware ray tracing is off"),
            "{empty:?}"
        );
        let stats = RenderStats {
            ray_tracing: axcenity_render::RayTracingStats {
                instances: 40,
                meshes: 3,
                triangles: 1_234_567,
                meshes_waiting: 2,
                rebuilt: false,
                path_traced_paths: 0,
                bent: 0,
                shadow_reach: 0,
            },
            ..RenderStats::default()
        };
        let lines = ray_hits_lines(&stats);
        assert_eq!(lines[0].1, "40 items traced");
        assert_eq!(lines[1].1, "3 meshes, 1,234,567 triangles");
        assert!(lines[2].1.starts_with("2 meshes still being built"));
    }

    #[test]
    fn the_legend_draws_in_the_viewports_corner_for_its_views_only() {
        let stats = RenderStats {
            lod_items: [12, 5, 3],
            ..RenderStats::default()
        };
        let draw = |mode: ViewMode| {
            let ctx = crate::theme::test_context();
            let run = || {
                crate::theme::run(&ctx, Default::default(), |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        paint(ui, ui.max_rect(), mode, &stats, None);
                    });
                })
            };
            // An area measures itself on its first frame, unseen.
            run();
            let output = run();
            output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::epaint::Shape::Text(text) => Some(text.galley.text().to_owned()),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let lod = draw(ViewMode::Debug(DebugView::LodLevels));
        assert!(
            lod.iter().any(|t| t == "LOD0, its own mesh: 12 in view"),
            "{lod:?}"
        );
        let overdraw = draw(ViewMode::Debug(DebugView::Overdraw));
        assert!(overdraw.iter().any(|t| t == "8 surfaces"), "{overdraw:?}");
        assert!(overdraw.iter().any(|t| t == "Measure"), "{overdraw:?}");
        assert!(draw(ViewMode::Lit).is_empty());
        if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
            for (mode, name) in [
                (ViewMode::Debug(DebugView::LodLevels), "legend-lod.png"),
                (ViewMode::Debug(DebugView::Overdraw), "legend-overdraw.png"),
            ] {
                crate::snapshot::snapshot(
                    &std::path::Path::new(&folder).join(name),
                    egui::vec2(480.0, 320.0),
                    vec![Vec::new(); 2],
                    |ui| {
                        egui::CentralPanel::default().show(ui, |ui| {
                            paint(ui, ui.max_rect(), mode, &stats, None);
                        });
                    },
                );
            }
        }
    }
}
