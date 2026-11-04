//! The reporting side of a run: frame correlation, the timing helpers and the
//! JSON the driver prints before it exits.

use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::args::{Case, Config};
use super::canonical::digest64;
use super::{CanonicalState, Correlation, FrameRecord, Submission};

pub(super) fn correlate<W: Copy + PartialEq>(
    events: &[FrameRecord<W>],
    expected_window: W,
    rendered_at: Instant,
) -> Correlation<W> {
    if events
        .iter()
        .any(|event| matches!(event, FrameRecord::Draw { start, end, .. } if start > end))
    {
        return Correlation::Rejected("profiler reported an invalid Draw interval");
    }
    let mut matching_draw = None;
    for (index, event) in events.iter().enumerate() {
        if let FrameRecord::Draw {
            window_id,
            start,
            end,
        } = *event
        {
            if start <= rendered_at && rendered_at <= end {
                if matching_draw.is_some() {
                    return Correlation::Rejected("App render marker matches multiple Draw events");
                }
                matching_draw = Some((index, window_id, start, end));
            }
        }
    }
    let Some((draw_index, draw_window, draw_start, draw_end)) = matching_draw else {
        return if events.is_empty() {
            Correlation::Pending
        } else {
            Correlation::Rejected("App render marker is stale or missing from Draw events")
        };
    };
    if draw_window != expected_window {
        return Correlation::Rejected("App render Draw belongs to another window");
    }
    let Some(event) = events.get(draw_index + 1) else {
        return Correlation::Pending;
    };
    match *event {
        FrameRecord::Draw { .. } => {
            Correlation::Rejected("a new Draw preceded the matching Present")
        }
        FrameRecord::Present { window_id, end } => {
            if window_id != expected_window {
                return Correlation::Rejected("following Present belongs to another window");
            }
            if end < draw_end {
                return Correlation::Rejected("matching Present predates Draw completion");
            }
            Correlation::Matched(Submission {
                window_id,
                draw_start,
                draw_end,
                present_end: end,
            })
        }
    }
}

pub(super) fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

pub(super) fn round4(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

/// The first frame that starts after an event, and the Present that follows it.
pub(super) struct FrameOutcome {
    pub(super) draw_start_offset_ms: f64,
    pub(super) draw_duration_ms: f64,
    pub(super) present_ms: f64,
}

pub(super) fn first_frame_after<W: Copy + PartialEq>(
    events: &[FrameRecord<W>],
    expected_window: W,
    after: Instant,
) -> Option<FrameOutcome> {
    let draw_index = events.iter().position(|event| {
        matches!(
            event,
            FrameRecord::Draw { window_id, start, .. }
                if *window_id == expected_window && *start >= after
        )
    })?;
    let FrameRecord::Draw { start, end, .. } = events[draw_index] else {
        return None;
    };
    let present = events
        .iter()
        .skip(draw_index + 1)
        .find_map(|event| match event {
            FrameRecord::Present { window_id, end } if *window_id == expected_window => Some(*end),
            _ => None,
        })?;
    Some(FrameOutcome {
        draw_start_offset_ms: millis(start.saturating_duration_since(after)),
        draw_duration_ms: millis(end.saturating_duration_since(start)),
        present_ms: millis(present.saturating_duration_since(after)),
    })
}

pub(super) fn signed_millis(at: Instant, origin: Instant) -> f64 {
    if at >= origin {
        millis(at.duration_since(origin))
    } else {
        -millis(origin.duration_since(at))
    }
}

pub(super) fn summary_json(
    config: &Config,
    fixture: &str,
    runs: Vec<Value>,
    status: &str,
    baseline: &CanonicalState,
) -> Value {
    json!({
        "benchmark": "measure-input",
        "status": status,
        "claim": match config.case {
            Case::Stroke => "synthetic_input_to_commit_only",
            _ => "event_dispatch_to_first_frame_present",
        },
        "input_path": "Window::dispatch_event -> production canvas hitbox",
        "fixture": fixture,
        "fixture_identity": {
            "width": baseline.width,
            "height": baseline.height,
            "active_layer_id": baseline.layer_id,
            "baseline_digest_fnv64": format!("{:016x}", digest64(&baseline.pixels)),
        },
        "workload": {
            "case": config.case.label(),
            "mode": config.mode.as_str(),
            "warmup": config.warmup,
            "samples": config.samples,
            "strokes": config.strokes,
            "moves": config.moves,
            "canvas": config.canvas,
            "canvas_height": config.height(),
            "layers": if config.canvas == 2 { 1 } else { config.layers },
            "brush_sizes": config.brush_sizes,
            "base": if config.canvas == 2 { "bounded" } else { config.base.label() },
            "content": if config.canvas == 2 { "bounded" } else { config.content.label() },
            "reset_between_runs": "undo to the run baseline, pixels verified",
        },
        "clock": "monotonic Instant; start = the dispatch call, endpoint = profiler present_end",
        "present_tier": "platform submission completion, not measured display latency",
        "profile": if cfg!(debug_assertions) { "dev" } else { "release" },
        "runs": runs,
        "metric_mapping": {
            "pointer_to_brush_ms": "first preview present after pointerdown (Q6 overlay path); null when no preview",
            "dispatch_to_commit_ms": "down dispatch to successful core commit (record-only)",
            "brush_commit_ms": "handler_entry_to_commit_ms",
            "brush_mouseup_ms": "up_to_commit_ms",
            "dispatch_to_present_ms": "event dispatch to the first Present of the frame it caused",
            "handler_dispatch_ms": "synchronous cost of dispatching the event pair"
        },
        "display_observation": "same-window GPUI draw followed by platform submission completion",
        "rss_bytes": null,
        "rss_status": "external_sampling_required",
        "physical_display_latency_ms": null,
        "physical_display_latency_status": "not_measured",
        "machine": format!("windows-{} x {} cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|count| count.get()).unwrap_or(0)),
    })
}

pub(super) fn failure_json(error: String) -> Value {
    json!({
        "benchmark": "measure-input",
        "status": "failed",
        "claim": "synthetic_input_to_commit_only",
        "error": error,
        "physical_display_latency_ms": null,
        "physical_display_latency_status": "not_measured",
    })
}

pub(super) fn emit(value: Value, code: i32) -> ! {
    match serde_json::to_string(&value) {
        Ok(line) => println!("{line}"),
        Err(error) => eprintln!("measure-input: cannot encode result: {error}"),
    }
    std::process::exit(code);
}
