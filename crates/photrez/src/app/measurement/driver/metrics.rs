//! Metric roll-ups for the input-latency report: the stroke geometry and the
//! percentile summaries each aggregate publishes.

use serde_json::{json, Value};

pub(super) fn stroke_geometry(
    stroke: usize,
    width: u32,
    height: u32,
    brush_size: u32,
) -> ((u32, u32), (u32, u32)) {
    let margin_x = (brush_size / 2)
        .saturating_add(1)
        .min(width.saturating_sub(1) / 2);
    let margin_y = (brush_size / 2)
        .saturating_add(1)
        .min(height.saturating_sub(1) / 2);
    let usable_width = width.saturating_sub(margin_x.saturating_mul(2)).max(1);
    let usable_height = height.saturating_sub(margin_y.saturating_mul(2)).max(1);
    let offset = (stroke as u32).saturating_mul(usable_height) / 12;
    let y = margin_y + offset.min(usable_height.saturating_sub(1));
    let x0 = margin_x;
    let x1 = margin_x + usable_width.saturating_sub(1);
    ((x0, y), (x1, y))
}

pub(super) fn aggregate_strokes(strokes: &[Value]) -> Value {
    let first_pixel = collect_metric(strokes, "first_pixel_ms");
    let preview_push = collect_metric(strokes, "preview_push_ms");
    let dispatch = collect_metric(strokes, "dispatch_to_commit_ms");
    let handler = collect_metric(strokes, "handler_entry_to_commit_ms");
    let up = collect_metric(strokes, "up_to_commit_ms");
    let render = collect_metric(strokes, "app_render_after_commit_ms");
    let draw = collect_metric(strokes, "draw_duration_ms");
    let submission = collect_metric(strokes, "commit_to_submission_completion_ms");
    let pointer = if first_pixel.is_empty() {
        Value::Null
    } else {
        metric_summary(&first_pixel)
    };
    let changed_pixels: u64 = strokes
        .iter()
        .filter_map(|value| value.get("changed_pixels").and_then(Value::as_u64))
        .sum();
    let changed_tiles: u64 = strokes
        .iter()
        .filter_map(|value| value.get("changed_tiles").and_then(Value::as_u64))
        .sum();
    let digest = strokes
        .last()
        .and_then(|value| value.get("digest"))
        .cloned()
        .unwrap_or(Value::Null);
    let commit = metric_summary(&handler);
    let mouseup = metric_summary(&up);
    let premultiply = collect_phase_metric(strokes, "premultiply_us");
    let raster = collect_phase_metric(strokes, "raster_us");
    let patch = collect_phase_metric(strokes, "patch_us");
    let apply = collect_phase_metric(strokes, "apply_us");
    let last = strokes.last();
    let crop = json!({
        "x": last.and_then(|value| value.get("commit_phases")).and_then(|phases| phases.get("crop_x")).cloned().unwrap_or(Value::Null),
        "y": last.and_then(|value| value.get("commit_phases")).and_then(|phases| phases.get("crop_y")).cloned().unwrap_or(Value::Null),
        "w": last.and_then(|value| value.get("commit_phases")).and_then(|phases| phases.get("crop_w")).cloned().unwrap_or(Value::Null),
        "h": last.and_then(|value| value.get("commit_phases")).and_then(|phases| phases.get("crop_h")).cloned().unwrap_or(Value::Null),
    });
    let commit_phases = json!({
        "premultiply_us": metric_summary(&premultiply),
        "raster_us": metric_summary(&raster),
        "patch_us": metric_summary(&patch),
        "apply_us": metric_summary(&apply),
        "last_crop": crop,
    });
    json!({
        "stroke_count": strokes.len(),
        "pointer_to_brush_ms": pointer,
        "first_pixel_ms": if first_pixel.is_empty() {
            Value::Null
        } else {
            metric_summary(&first_pixel)
        },
        "preview_push_ms": metric_summary(&preview_push),
        "dispatch_to_commit_ms": metric_summary(&dispatch),
        "brush_commit_ms": commit,
        "brush_mouseup_ms": mouseup,
        "brush_commit_p95_ms": commit.get("p95").and_then(Value::as_f64).unwrap_or(0.0),
        "brush_commit_max_ms": commit.get("max").and_then(Value::as_f64).unwrap_or(0.0),
        "brush_mouseup_p95_ms": mouseup.get("p95").and_then(Value::as_f64).unwrap_or(0.0),
        "app_render_after_commit_ms": metric_summary(&render),
        "draw_duration_ms": metric_summary(&draw),
        "commit_to_submission_completion_ms": metric_summary(&submission),
        "changed_pixels": changed_pixels,
        "changed_tiles": changed_tiles,
        "commit_phases": commit_phases,
        "final_digest": digest,
        "rss_bytes": null,
        "rss_status": "external_sampling_required"
    })
}

pub(super) fn aggregate_events(events: &[Value]) -> Value {
    let present = collect_metric(events, "present");
    let handler = collect_metric(events, "handler_ms");
    let draw = collect_metric(events, "draw_duration_ms");
    let raw: Vec<Value> = events
        .iter()
        .map(|event| event.get("present").cloned().unwrap_or(Value::Null))
        .collect();
    json!({
        "event_count": events.len(),
        "present_count": present.len(),
        "dispatch_to_present_ms": if present.is_empty() {
            Value::Null
        } else {
            metric_summary(&present)
        },
        "handler_dispatch_ms": metric_summary(&handler),
        "draw_duration_ms": metric_summary(&draw),
        "dispatch_to_present_samples_ms": raw,
    })
}

fn collect_metric(strokes: &[Value], key: &str) -> Vec<f64> {
    strokes
        .iter()
        .filter_map(|value| value.get(key).and_then(Value::as_f64))
        .collect()
}

fn collect_phase_metric(strokes: &[Value], key: &str) -> Vec<f64> {
    strokes
        .iter()
        .filter_map(|value| {
            value
                .get("commit_phases")
                .and_then(|phases| phases.get(key))
                .and_then(Value::as_f64)
        })
        .collect()
}

fn metric_summary(values: &[f64]) -> Value {
    let mut sorted = values.to_vec();
    sorted.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    json!({
        "count": sorted.len(),
        "p50": percentile(&sorted, 0.5),
        "p95": percentile(&sorted, 0.95),
        "max": sorted.last().copied().unwrap_or(0.0),
    })
}

pub(super) fn percentile(sorted: &[f64], fraction: f64) -> f64 {
    let Some(last) = sorted.len().checked_sub(1) else {
        return 0.0;
    };
    let index = ((last as f64) * fraction).round() as usize;
    sorted.get(index.min(last)).copied().unwrap_or(0.0)
}
