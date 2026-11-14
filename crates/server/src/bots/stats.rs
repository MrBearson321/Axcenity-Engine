//! What the bots measure, and the summary of a run.

use std::collections::BTreeMap;

/// One bot's numbers at one moment.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BotSample {
    pub bot: usize,
    /// Seconds from starting to having the server's world.
    pub joined_after: Option<f64>,
    /// In the game now.
    pub connected: bool,
    /// Why its connection ended, if it has.
    pub ended: Option<String>,
    /// Seconds from starting until it ended (`--flood`).
    pub ended_after: Option<f64>,
    /// Times this bot left and a new one took its place (`--churn`).
    pub churned: u64,
    pub rtt_ms: f64,
    pub packet_loss: f64,
    pub sent_per_second: f64,
    pub received_per_second: f64,
    /// Times the server moved its character back.
    pub corrections: u64,
    /// Times it rejoined after losing its connection (MP2).
    pub rejoins: u64,
    /// Lost its connection and trying to rejoin now.
    pub rejoining: bool,
    pub script_errors: u64,
    /// Its own frames' average time since the last sample, in ms.
    pub frame_ms: f64,
    /// Instances in its world.
    pub instances: usize,
    /// How it reached the server (MP5): `"direct"`, `"punched"` or
    /// `"relayed"`; `None` before it has a token.
    pub route: Option<&'static str>,
    /// With server-moved characters (MP6a): moves predicted, the
    /// server's answers that differed (and the largest, in studs), and the
    /// time taking moves again took, in ms.
    pub predicted: u64,
    pub predict_corrections: u64,
    pub predict_largest: f64,
    pub replay_ms: f64,
}

/// The value at `share` (0 to 1) of `values`, nearest rank; 0 for none.
pub fn percentile(values: &[f64], share: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let rank = (share * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

/// Every bot's latest numbers, summed up.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Summary {
    pub bots: usize,
    pub joined: usize,
    pub connected: usize,
    pub join_p50: f64,
    pub join_max: f64,
    pub rtt_p50: f64,
    pub rtt_p95: f64,
    pub rtt_max: f64,
    pub loss_mean: f64,
    /// Bytes a second, per connected bot.
    pub sent_mean: f64,
    pub received_mean: f64,
    pub corrections: u64,
    pub rejoins: u64,
    /// Bots that left and were replaced (`--churn`).
    pub churned: u64,
    /// The longest a connection lasted before it ended (`--flood`).
    pub ended_after_max: f64,
    /// Bots trying to rejoin now.
    pub rejoining: usize,
    pub script_errors: u64,
    pub frame_ms_mean: f64,
    pub frame_ms_max: f64,
    /// Why connections ended, with how many each.
    pub ended: BTreeMap<String, usize>,
    /// How bots reached the server, with how many each (MP5).
    pub routes: BTreeMap<&'static str, usize>,
    /// Prediction (MP6a), summed over the bots; the largest correction.
    pub predicted: u64,
    pub predict_corrections: u64,
    pub predict_largest: f64,
    pub replay_ms: f64,
}

impl Summary {
    pub fn of(samples: &[BotSample]) -> Summary {
        let joins: Vec<f64> = samples.iter().filter_map(|s| s.joined_after).collect();
        let live: Vec<&BotSample> = samples.iter().filter(|s| s.connected).collect();
        let of_live =
            |f: fn(&BotSample) -> f64| -> Vec<f64> { live.iter().map(|s| f(s)).collect() };
        let mean = |values: &[f64]| {
            if values.is_empty() {
                0.0
            } else {
                values.iter().sum::<f64>() / values.len() as f64
            }
        };
        let rtts = of_live(|s| s.rtt_ms);
        let frames: Vec<f64> = samples.iter().map(|s| s.frame_ms).collect();
        let mut ended = BTreeMap::new();
        for reason in samples.iter().filter_map(|s| s.ended.clone()) {
            *ended.entry(reason).or_insert(0) += 1;
        }
        let mut routes = BTreeMap::new();
        for route in samples.iter().filter_map(|s| s.route) {
            *routes.entry(route).or_insert(0) += 1;
        }
        Summary {
            predicted: samples.iter().map(|s| s.predicted).sum(),
            predict_corrections: samples.iter().map(|s| s.predict_corrections).sum(),
            predict_largest: samples
                .iter()
                .map(|s| s.predict_largest)
                .fold(0.0, f64::max),
            replay_ms: samples.iter().map(|s| s.replay_ms).sum(),
            routes,
            bots: samples.len(),
            joined: joins.len(),
            connected: live.len(),
            join_p50: percentile(&joins, 0.5),
            join_max: percentile(&joins, 1.0),
            rtt_p50: percentile(&rtts, 0.5),
            rtt_p95: percentile(&rtts, 0.95),
            rtt_max: percentile(&rtts, 1.0),
            loss_mean: mean(&of_live(|s| s.packet_loss)),
            sent_mean: mean(&of_live(|s| s.sent_per_second)),
            received_mean: mean(&of_live(|s| s.received_per_second)),
            corrections: samples.iter().map(|s| s.corrections).sum(),
            rejoins: samples.iter().map(|s| s.rejoins).sum(),
            churned: samples.iter().map(|s| s.churned).sum(),
            ended_after_max: samples
                .iter()
                .filter_map(|s| s.ended_after)
                .fold(0.0, f64::max),
            rejoining: samples.iter().filter(|s| s.rejoining).count(),
            script_errors: samples.iter().map(|s| s.script_errors).sum(),
            frame_ms_mean: mean(&frames),
            frame_ms_max: percentile(&frames, 1.0),
            ended,
        }
    }

    /// One line for the console.
    pub fn line(&self) -> String {
        let mut line = format!(
            "{} of {} bots in ({} joined; join p50 {:.2} s, max {:.2} s); round trip p50 {:.0} ms, \
             p95 {:.0}, max {:.0}; loss {:.1}%; each bot sends {:.1} KB/s, receives {:.1} KB/s; \
             {} corrections; bots' frames {:.2} ms (max {:.2})",
            self.connected,
            self.bots,
            self.joined,
            self.join_p50,
            self.join_max,
            self.rtt_p50,
            self.rtt_p95,
            self.rtt_max,
            self.loss_mean * 100.0,
            self.sent_mean / 1000.0,
            self.received_mean / 1000.0,
            self.corrections,
            self.frame_ms_mean,
            self.frame_ms_max,
        );
        if self.rejoins > 0 || self.rejoining > 0 {
            line.push_str(&format!(
                "; {} rejoins, {} rejoining now",
                self.rejoins, self.rejoining
            ));
        }
        if self.churned > 0 {
            line.push_str(&format!("; {} replaced", self.churned));
        }
        if self.predicted > 0 {
            line.push_str(&format!(
                "; {} moves predicted, {} answers differed (largest {:.2} studs), {:.1} ms taking moves again",
                self.predicted, self.predict_corrections, self.predict_largest, self.replay_ms
            ));
        }
        if self.script_errors > 0 {
            line.push_str(&format!("; {} script errors", self.script_errors));
        }
        if self.routes.keys().any(|route| *route != "direct") {
            let routes: Vec<String> = self
                .routes
                .iter()
                .map(|(route, count)| format!("{count} {route}"))
                .collect();
            line.push_str(&format!("; {}", routes.join(", ")));
        }
        for (reason, count) in &self.ended {
            line.push_str(&format!("; {count} ended: {reason}"));
        }
        line
    }

    pub fn json(&self, seconds: f64) -> serde_json::Value {
        serde_json::json!({
            "t": seconds,
            "bots": self.bots,
            "joined": self.joined,
            "connected": self.connected,
            "join_p50_s": self.join_p50,
            "join_max_s": self.join_max,
            "rtt_p50_ms": self.rtt_p50,
            "rtt_p95_ms": self.rtt_p95,
            "rtt_max_ms": self.rtt_max,
            "loss_mean": self.loss_mean,
            "sent_bytes_per_s": self.sent_mean,
            "received_bytes_per_s": self.received_mean,
            "corrections": self.corrections,
            "rejoins": self.rejoins,
            "churned": self.churned,
            "ended_after_max_s": self.ended_after_max,
            "rejoining": self.rejoining,
            "script_errors": self.script_errors,
            "bot_frame_ms_mean": self.frame_ms_mean,
            "bot_frame_ms_max": self.frame_ms_max,
            "ended": self.ended,
            "routes": self.routes,
            "predicted": self.predicted,
            "predict_corrections": self.predict_corrections,
            "predict_largest": self.predict_largest,
            "replay_ms": self.replay_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_take_the_nearest_rank() {
        let values = [5.0, 1.0, 4.0, 2.0, 3.0];
        assert_eq!(percentile(&values, 0.5), 3.0);
        assert_eq!(percentile(&values, 0.95), 5.0);
        assert_eq!(percentile(&values, 1.0), 5.0);
        assert_eq!(percentile(&[], 0.5), 0.0);
    }

    #[test]
    fn a_summary_counts_only_connected_bots_for_the_link_numbers() {
        let mut samples = vec![
            BotSample {
                bot: 0,
                joined_after: Some(1.0),
                connected: true,
                rtt_ms: 10.0,
                sent_per_second: 2000.0,
                ..BotSample::default()
            },
            BotSample {
                bot: 1,
                joined_after: Some(3.0),
                connected: true,
                rtt_ms: 30.0,
                sent_per_second: 4000.0,
                corrections: 2,
                ..BotSample::default()
            },
        ];
        samples.push(BotSample {
            bot: 2,
            ended: Some("The game is full (2 of 2 players).".to_owned()),
            rtt_ms: 999.0,
            ..BotSample::default()
        });
        let summary = Summary::of(&samples);
        assert_eq!((summary.bots, summary.joined, summary.connected), (3, 2, 2));
        assert_eq!(summary.rtt_max, 30.0);
        assert_eq!(summary.sent_mean, 3000.0);
        assert_eq!(summary.corrections, 2);
        assert_eq!(summary.join_max, 3.0);
        assert_eq!(
            summary.ended.get("The game is full (2 of 2 players)."),
            Some(&1)
        );
        assert!(
            summary.line().contains("2 of 3 bots in"),
            "{}",
            summary.line()
        );
    }
}
