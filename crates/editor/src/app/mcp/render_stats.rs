//! `render_stats`: what drawing the editor's viewport costs, from its last
//! frame: CPU and GPU time, each GPU pass, draws, triangles, culling,
//! levels of detail and GPU memory, and the Profiler's readings of the
//! game playing (or the last Play). Only the open editor draws frames;
//! without the window there's nothing to measure (`docs/editor/mcp.md`).

use std::time::Instant;

use axcenity_mcp::{Args, Class, Code, Outcome, Profile, ToolError, ToolSpec};

use super::Host;
use crate::profiler::RenderCost;

pub const SPEC: ToolSpec = ToolSpec {
    name: "render_stats",
    title: "Rendering stats",
    summary: "Frame, GPU pass and draw numbers and GPU memory from the open editor's viewport.",
    profile: Profile::Full,
    class: Class::Read,
    idempotent: true,
    params: &[],
};

/// The viewport's last frame, as the editor keeps it for this tool.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LastDrawn {
    pub cost: RenderCost,
    pub at: Instant,
    /// The viewport's size in pixels.
    pub size: (u32, u32),
    pub playing: bool,
}

pub fn call(host: &mut Host, _args: &Args) -> Outcome {
    let app = &host.app;
    let Some(drawn) = app.last_drawn else {
        return Err(ToolError::new(
            Code::Unsupported,
            "no frames are drawn: this server runs without the editor window",
        )
        .with_fix(
            "turn on Settings > AI connections in the editor and restart this server; \
             game_run reports script frame times",
        ));
    };
    let ms = |time: std::time::Duration| time.as_secs_f64() * 1000.0;
    let age = drawn.at.elapsed().as_secs_f64();
    let mut text = format!(
        "viewport {}×{} · {} · last frame {} ago · CPU {:.2} ms · GPU {} · {} lights",
        drawn.size.0,
        drawn.size.1,
        if drawn.playing { "playing" } else { "editing" },
        if age < 1.0 {
            format!("{:.0} ms", age * 1000.0)
        } else {
            format!("{age:.0} s")
        },
        ms(drawn.cost.cpu),
        drawn.cost.stats.gpu_time.map_or_else(
            || "not timed".to_owned(),
            |gpu| format!("{:.2} ms", ms(gpu))
        ),
        drawn.cost.lights
    );
    text.push_str(&format!(
        "\ngraphics {}{}",
        app.graphics.quality.label(),
        app.adapter
            .as_deref()
            .map_or_else(String::new, |gpu| format!(" · {gpu}"))
    ));
    for line in axcenity_ui::render_lines(&drawn.cost.stats) {
        text.push('\n');
        text.push_str(&line);
    }
    if let Some(readout) = app.profiler.readout() {
        text.push_str(&format!(
            "\n{}: 1% low {:.0} fps · CPU {:.2} ms · GPU {} · {}",
            if app.profiler.is_live() {
                "Play"
            } else {
                "last Play"
            },
            readout.low_fps,
            readout.cpu_ms,
            readout
                .gpu_ms
                .map_or_else(|| "not timed".to_owned(), |gpu| format!("{gpu:.2} ms")),
            readout.bound
        ));
    }
    if age > 2.0 {
        text.push_str(
            "\n(the window is idle, so this is an older frame; an idle editor doesn't redraw)",
        );
    }
    Ok(text)
}
