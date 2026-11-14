//! `--stats <seconds>`: the server's numbers while it serves (multiplayer
//! readiness MP1): how long its frames took, how many players are in, and
//! their connections' round trips, loss and bandwidth, as renet measures
//! them. A line that often on stdout, and a JSON line in `--stats-file`.
//! The Instance count and Lua memory show leaks under churn (MP2).

use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use axcenity_net::Network;
use axcenity_sim::Sim;

/// The value at `share` (0 to 1) of sorted `values`, nearest rank.
fn percentile(sorted: &[f64], share: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = (share * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

pub struct ServerStats {
    every: Duration,
    started: Instant,
    since: Instant,
    /// This interval's frame times, in ms.
    frames: Vec<f64>,
    file: Option<File>,
}

impl ServerStats {
    pub fn new(seconds: f64, file: Option<&Path>) -> Result<ServerStats> {
        let file = match file {
            Some(path) => Some(
                File::create(path)
                    .with_context(|| format!("could not write `{}`", path.display()))?,
            ),
            None => None,
        };
        Ok(ServerStats {
            every: Duration::from_secs_f64(seconds),
            started: Instant::now(),
            since: Instant::now(),
            frames: Vec::new(),
            file,
        })
    }

    /// After each frame, which took `took` to run (not counting the wait).
    pub fn frame(&mut self, took: Duration, network: &Network, sim: &Sim) {
        self.frames.push(took.as_secs_f64() * 1000.0);
        if self.since.elapsed() < self.every {
            return;
        }
        let interval = self.since.elapsed().as_secs_f64();
        self.since = Instant::now();
        let mut frames = std::mem::take(&mut self.frames);
        frames.sort_by(f64::total_cmp);
        let links: Vec<_> = network.stats().into_iter().map(|peer| peer.link).collect();
        let mut rtts: Vec<f64> = links.iter().map(|link| link.rtt * 1000.0).collect();
        rtts.sort_by(f64::total_cmp);
        // `+ 0.0`: an empty sum is -0.0, which would print as "-0.0".
        let sum = |f: fn(&axcenity_net::LinkStats) -> f64| links.iter().map(f).sum::<f64>() + 0.0;
        let sent = sum(|link| link.bytes_sent_per_second);
        let received = sum(|link| link.bytes_received_per_second);
        let loss = if links.is_empty() {
            0.0
        } else {
            sum(|link| link.packet_loss) / links.len() as f64
        };
        let fps = frames.len() as f64 / interval;
        let t = self.started.elapsed().as_secs_f64();
        let instances = sim.world.len();
        let lua_bytes = sim.scripts.used_memory();
        println!(
            "[{t:6.1} s] {} players; {fps:.0} frames a second, frame p50 {:.2} ms, p95 {:.2}, \
             max {:.2}; round trip p50 {:.0} ms, max {:.0}; loss {:.1}%; sending {:.1} KB/s \
             ({:.1} per player), receiving {:.1} KB/s; {instances} Instances, Lua {:.1} MB",
            links.len(),
            percentile(&frames, 0.5),
            percentile(&frames, 0.95),
            percentile(&frames, 1.0),
            percentile(&rtts, 0.5),
            percentile(&rtts, 1.0),
            loss * 100.0,
            sent / 1000.0,
            sent / 1000.0 / links.len().max(1) as f64,
            received / 1000.0,
            lua_bytes as f64 / 1e6,
        );
        if let Some(file) = &mut self.file {
            let line = serde_json::json!({
                "t": t,
                "players": links.len(),
                "fps": fps,
                "frame_p50_ms": percentile(&frames, 0.5),
                "frame_p95_ms": percentile(&frames, 0.95),
                "frame_max_ms": percentile(&frames, 1.0),
                "rtt_p50_ms": percentile(&rtts, 0.5),
                "rtt_max_ms": percentile(&rtts, 1.0),
                "loss_mean": loss,
                "sent_bytes_per_s": sent,
                "received_bytes_per_s": received,
                "instances": instances,
                "lua_bytes": lua_bytes,
            });
            let _ = writeln!(file, "{line}");
        }
    }
}
