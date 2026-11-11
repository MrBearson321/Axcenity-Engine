//! Running the benchmark in tests: a GPU device when there is one, and a
//! quick whole run. The editor's tests use these too, through the
//! `test-support` feature.

use axcenity_render::wgpu;

use crate::{Plan, Progress, Report, Run};

/// A device to run on, or `None` (the test is skipped) without a GPU.
pub fn device() -> Option<(wgpu::Device, wgpu::Queue, wgpu::AdapterInfo)> {
    let instance = axcenity_render::adapters::instance();
    let adapter = pollster::block_on(axcenity_render::adapters::adapter(&instance, None)).ok()?;
    // Timestamps where the adapter has them, as the editor asks.
    let features = adapter.features() & wgpu::Features::TIMESTAMP_QUERY;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: features,
        ..Default::default()
    }))
    .ok()?;
    Some((device, queue, adapter.get_info()))
}

/// Runs a whole benchmark with a few frames each.
pub fn quick_report() -> Option<Report> {
    let Some((device, queue, info)) = device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return None;
    };
    let mut run = Run::start(&device, &queue, &info, (320, 180), Plan::QUICK);
    for _ in 0..100 {
        match run.step() {
            Progress::Running => continue,
            Progress::Finished(report) => return Some(*report),
            Progress::Failed(error) => panic!("{error}"),
        }
    }
    panic!("the run never finished");
}
