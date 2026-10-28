//! Frame-pacing measurement over gpui's profiler trace.
//!
//! `FrameTimingCollector` snapshots a cursor into the global frame-timing ring
//! buffer; `collect_unseen` returns only events pushed after the previous poll,
//! so draining it in a loop yields every present exactly once. This is the
//! same instrument the earlier standalone measurement used, which is what makes
//! this display row comparable with the baseline table.

use std::time::{Duration, Instant};

use gpui_kit::profiler::{FrameEvent, FrameTimingCollector, PresentTiming};

/// Presents observed since this collector was created, in observation order.
pub struct Pacing {
    collector: FrameTimingCollector,
    pub presents: Vec<Instant>,
}

impl Pacing {
    /// Start collecting from the next present onward.
    pub fn start() -> Self {
        Self {
            collector: FrameTimingCollector::new(),
            presents: Vec::new(),
        }
    }

    /// Drain everything newly recorded. Returns how many new presents arrived.
    pub fn drain(&mut self) -> usize {
        let before = self.presents.len();
        for event in self.collector.collect_unseen() {
            if let FrameEvent::Present(PresentTiming { present_end, .. }) = event {
                self.presents.push(present_end);
            }
        }
        self.presents.len() - before
    }

    /// Whether a present with `present_end` strictly after `after` was seen.
    pub fn seen_after(&self, after: Instant) -> bool {
        self.presents.iter().any(|&at| at > after)
    }

    /// Interval between consecutive presents. Requires at least two presents.
    pub fn intervals(&self) -> Vec<Duration> {
        self.presents
            .windows(2)
            .map(|pair| pair[1].saturating_duration_since(pair[0]))
            .collect()
    }
}

/// 95th percentile (nearest-rank) of a set of durations. `ZERO` when empty.
pub fn p95(durations: &[Duration]) -> Duration {
    if durations.is_empty() {
        return Duration::ZERO;
    }
    let mut sorted = durations.to_vec();
    sorted.sort_unstable();
    let index = ((sorted.len() as f64) * 0.95).ceil() as usize;
    let index = index.clamp(1, sorted.len()) - 1;
    sorted[index]
}

/// Count of intervals at or above `threshold`.
pub fn count_at_least(durations: &[Duration], threshold: Duration) -> usize {
    durations
        .iter()
        .filter(|&&duration| duration >= threshold)
        .count()
}
