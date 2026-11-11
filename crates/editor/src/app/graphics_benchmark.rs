//! Debug › Graphics Benchmark in the app: starting a run on the editor's
//! GPU device, driving it a batch a frame, and applying (or undoing) its
//! recommendation to the editor's own graphics settings. The window and
//! the measuring are `graphics_benchmark.rs`.

use eframe::egui_wgpu::RenderState;

use crate::graphics_benchmark::{Action, Plan, Progress, Run};

use super::*;

impl EditorApp {
    /// Opens the benchmark's window.
    pub(super) fn open_graphics_benchmark(&mut self) {
        self.benchmark.show();
    }

    /// Each frame: the window, and a batch of a run in progress.
    pub(super) fn graphics_benchmark_frame(
        &mut self,
        ctx: &egui::Context,
        render_state: Option<&RenderState>,
    ) {
        if let Some(run) = &mut self.benchmark.run {
            match run.step() {
                Progress::Running => ctx.request_repaint(),
                Progress::Finished(report) => {
                    self.benchmark.run = None;
                    self.benchmark.previous = None;
                    self.benchmark.report = Some(*report);
                    self.benchmark.scroll_to_top = true;
                    ctx.request_repaint();
                }
                Progress::Failed(error) => {
                    self.benchmark.run = None;
                    self.benchmark.error = Some(error);
                }
            }
        }
        if !self.benchmark.open && self.benchmark.run.is_none() {
            return;
        }
        let playing = self.playing();
        let action = self
            .benchmark
            .window(ctx, &self.graphics, self.adapter.as_deref(), playing);
        match action {
            Some(Action::Start) => self.start_graphics_benchmark(render_state, Plan::FULL),
            Some(Action::Cancel) => self.benchmark.run = None,
            Some(Action::Apply) => self.apply_graphics_benchmark(),
            Some(Action::Keep) => self.benchmark.open = false,
            Some(Action::Undo) => {
                if let Some(previous) = self.benchmark.previous.take() {
                    self.graphics = previous;
                }
            }
            None => {}
        }
    }

    /// Starts a run at the viewport's size (or 1280 × 720 before it has
    /// drawn). Not during Play: the game would share the GPU.
    pub(super) fn start_graphics_benchmark(
        &mut self,
        render_state: Option<&RenderState>,
        plan: Plan,
    ) {
        if self.playing() {
            return;
        }
        let Some(render_state) = render_state else {
            self.benchmark.error =
                Some("No graphics device to measure: the editor isn't drawing with wgpu.".into());
            return;
        };
        let size = match self.viewport.pixel_size() {
            (0, _) | (_, 0) => (1280, 720),
            size => size,
        };
        self.benchmark.error = None;
        self.benchmark.run = Some(Run::start(
            &render_state.device,
            &render_state.queue,
            &render_state.adapter.get_info(),
            size,
            plan,
        ));
    }

    /// Uses the recommendation as the editor's graphics settings (keeping
    /// what it doesn't weigh, `recommend::apply_to`), and the old ones for
    /// Undo. The viewport draws with them from the next
    /// frame, and they're saved as Settings › Graphics changes always are.
    pub(super) fn apply_graphics_benchmark(&mut self) {
        let Some(applied) = self.benchmark.applied(&self.graphics) else {
            return;
        };
        if applied == self.graphics {
            return;
        }
        self.benchmark.previous = Some(std::mem::replace(&mut self.graphics, applied));
    }
}
