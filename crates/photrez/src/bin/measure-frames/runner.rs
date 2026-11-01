use crate::common::{copy_region, disc_bounds};
use crate::presentmon;
use std::path::Path;
use std::time::{Duration, Instant};

use super::timing::Pacing;
use super::{probe, timing};
use photrez::render::canvas_bridge::union_rect;

use super::{
    clone_rgba, dot_position, emit, fail, machine_line, paint_disc, round2, round6, since, Runner,
    WaitMode, STROKE,
};

impl Runner {
    fn radius(&self) -> i32 {
        self.cfg.brush_px.round() as i32
    }

    pub(super) async fn run(mut self, cx: &mut gpui_kit::AsyncApp) {
        // Let the window come up and paint its first frame.
        cx.background_executor()
            .timer(Duration::from_millis(600))
            .await;

        if self.cfg.control {
            self.run_control(cx).await;
            return;
        }

        let radius = self.radius();

        // Warm-up on a throwaway Pacing: these presents must not pollute the
        // measured intervals. warm=3, cold=0 by default, overridable.
        if self.cfg.warmup > 0 {
            let mut warm = Pacing::start();
            for index in 0..self.cfg.warmup {
                let mut rgba = match clone_rgba(&self.base) {
                    Ok(value) => value,
                    Err(error) => fail(&self.cfg, &error),
                };
                let (x, y) = dot_position(index, self.cfg.warmup, self.width, self.height, radius);
                paint_disc(&mut rgba, self.width, self.height, x, y, radius, STROKE);
                self.set_image(cx, rgba, Instant::now(), &mut warm).await;
            }
        }

        // Full-document upload. The snapshot copy is inside the measured block,
        // matching the earlier standalone measurement's buffer-creation window.
        let mut pace = Pacing::start();
        let mut opening_start = None;
        let mut opening_first_ms = None;
        let mut opening_total_ms = None;
        if self.cfg.progressive {
            let start = Instant::now();
            opening_start = Some(start);
            let first_rgba = match clone_rgba(&self.base) {
                Ok(value) => value,
                Err(error) => fail(&self.cfg, &error),
            };
            let cols = self.width.div_ceil(self.cfg.tile_size) as usize;
            let rows = self.height.div_ceil(self.cfg.tile_size) as usize;
            let first_limit = cols.saturating_mul(rows).div_ceil(4).max(1);
            self.set_image_with_limit(cx, first_rgba, start, &mut pace, Some(first_limit))
                .await;
            opening_first_ms = Some(since(&pace, start).as_secs_f64() * 1000.0);
        }
        let full_start = Instant::now();
        if self.cfg.red == "upload" {
            cx.background_executor()
                .timer(Duration::from_millis(40))
                .await;
        }
        let full_rgba = match clone_rgba(&self.base) {
            Ok(value) => value,
            Err(error) => fail(&self.cfg, &error),
        };
        self.set_image(cx, full_rgba, full_start, &mut pace).await;
        let upload_full_ms = since(&pace, full_start).as_secs_f64() * 1000.0;
        if let Some(start) = opening_start {
            opening_total_ms = Some(since(&pace, start).as_secs_f64() * 1000.0);
        }

        // Paint-prove baseline: the page before the stroke.
        let hwnd = probe::self_hwnd();
        let before = hwnd.and_then(probe::capture);

        // Dirty warm-up: exercise tile replacement/eviction on a throwaway
        // Pacing, so the measured phase does not pay first-use atlas costs.
        let mut trail = std::mem::take(&mut self.base);
        if self.cfg.warmup > 0 {
            let mut warm = Pacing::start();
            for index in 0..self.cfg.warmup {
                let (x, y) = dot_position(index, self.cfg.warmup, self.width, self.height, radius);
                paint_disc(&mut trail, self.width, self.height, x, y, radius, STROKE);
                let Some((dirty_x, dirty_y, dirty_w, dirty_h)) =
                    disc_bounds(x, y, radius, self.width, self.height)
                else {
                    continue;
                };
                let dirty = copy_region(&trail, self.width, dirty_x, dirty_y, dirty_w, dirty_h);
                self.push_dirty(
                    cx,
                    dirty_x,
                    dirty_y,
                    dirty_w,
                    dirty_h,
                    dirty,
                    Instant::now(),
                    &mut warm,
                    WaitMode::Sample,
                )
                .await;
            }
            self.commit_patches(cx, Instant::now(), &mut warm).await;
        }

        // Isolated dirty samples: one upload per stroke, the isolated-sample
        // shape. This is the upload_dirty_ms population; the dense loop after it
        // is the frame-pacing population.
        let mut dirty_pace = Pacing::start();
        let mut dirty_samples: Vec<Duration> = Vec::with_capacity(self.cfg.strokes);
        for index in 0..self.cfg.strokes {
            let (x, y) = dot_position(index, self.cfg.strokes, self.width, self.height, radius);
            paint_disc(&mut trail, self.width, self.height, x, y, radius, STROKE);
            let Some((dirty_x, dirty_y, dirty_w, dirty_h)) =
                disc_bounds(x, y, radius, self.width, self.height)
            else {
                continue;
            };
            let frame_start = Instant::now();
            let dirty = copy_region(&trail, self.width, dirty_x, dirty_y, dirty_w, dirty_h);
            if self.cfg.red == "dirty" && index == 0 {
                cx.background_executor()
                    .timer(Duration::from_millis(40))
                    .await;
            }
            self.push_dirty(
                cx,
                dirty_x,
                dirty_y,
                dirty_w,
                dirty_h,
                dirty,
                frame_start,
                &mut dirty_pace,
                WaitMode::Sample,
            )
            .await;
            dirty_samples.push(since(&dirty_pace, frame_start));
            // Commit each isolated sample so its patch is the only overlay in
            // the scene. This is setup between upload_dirty_ms samples, not a
            // measured sample itself.
            self.commit_patches(cx, Instant::now(), &mut dirty_pace)
                .await;
        }
        let upload_dirty_ms = dirty_samples
            .iter()
            .copied()
            .max()
            .unwrap_or(Duration::ZERO)
            .as_secs_f64()
            * 1000.0;
        let dirty_max_index = dirty_samples
            .iter()
            .enumerate()
            .max_by_key(|(_, sample)| **sample)
            .map(|(index, _)| index);
        let dirty_samples_ms: Vec<f64> = dirty_samples
            .iter()
            .map(|sample| round2(sample.as_secs_f64() * 1000.0))
            .collect();

        // Dense stroke loop: one small patch per move. The trail is the
        // retained full RGBA buffer, so no 64 MiB clone happens per frame; the
        // seam uploads only the disc bounds. Every eight moves folds the
        // overlay into the base tiles, bounding both the overlay and the number
        // of textures retired by one commit.
        let total = self.cfg.strokes * self.cfg.moves;
        let mut stroke_pace = Pacing::start();
        let mut latencies: Vec<Duration> = Vec::with_capacity(total);
        let mut commits: Vec<Duration> = Vec::with_capacity(total / self.cfg.commit_moves + 1);
        let mut pending_union: Option<(u32, u32, u32, u32)> = None;
        for index in 0..total {
            let (x, y) = dot_position(index, total, self.width, self.height, radius);
            paint_disc(&mut trail, self.width, self.height, x, y, radius, STROKE);
            let Some((dirty_x, dirty_y, dirty_w, dirty_h)) =
                disc_bounds(x, y, radius, self.width, self.height)
            else {
                continue;
            };

            if self.cfg.patch_union {
                pending_union = Some(match pending_union {
                    Some(existing) => union_rect(existing, (dirty_x, dirty_y, dirty_w, dirty_h)),
                    None => (dirty_x, dirty_y, dirty_w, dirty_h),
                });
                if (index + 1) % self.cfg.commit_moves != 0 {
                    continue;
                }
                let Some((u_x, u_y, u_w, u_h)) = pending_union.take() else {
                    continue;
                };
                let frame_start = Instant::now();
                let dirty = copy_region(&trail, self.width, u_x, u_y, u_w, u_h);
                self.push_dirty(
                    cx,
                    u_x,
                    u_y,
                    u_w,
                    u_h,
                    dirty,
                    frame_start,
                    &mut stroke_pace,
                    WaitMode::Frame,
                )
                .await;
                latencies.push(since(&stroke_pace, frame_start));
                let commit_start = Instant::now();
                self.commit_patches(cx, commit_start, &mut stroke_pace)
                    .await;
                commits.push(since(&stroke_pace, commit_start));
                continue;
            }

            let frame_start = Instant::now();
            let dirty = copy_region(&trail, self.width, dirty_x, dirty_y, dirty_w, dirty_h);
            self.push_dirty(
                cx,
                dirty_x,
                dirty_y,
                dirty_w,
                dirty_h,
                dirty,
                frame_start,
                &mut stroke_pace,
                WaitMode::Frame,
            )
            .await;
            latencies.push(since(&stroke_pace, frame_start));
            if (index + 1) % self.cfg.commit_moves == 0 {
                let commit_start = Instant::now();
                self.commit_patches(cx, commit_start, &mut stroke_pace)
                    .await;
                commits.push(since(&stroke_pace, commit_start));
            }
        }

        if self.cfg.patch_union {
            if let Some((u_x, u_y, u_w, u_h)) = pending_union.take() {
                let frame_start = Instant::now();
                let dirty = copy_region(&trail, self.width, u_x, u_y, u_w, u_h);
                self.push_dirty(
                    cx,
                    u_x,
                    u_y,
                    u_w,
                    u_h,
                    dirty,
                    frame_start,
                    &mut stroke_pace,
                    WaitMode::Frame,
                )
                .await;
                latencies.push(since(&stroke_pace, frame_start));
                let commit_start = Instant::now();
                self.commit_patches(cx, commit_start, &mut stroke_pace)
                    .await;
                commits.push(since(&stroke_pace, commit_start));
            }
        }

        // Paint-prove: the page after the stroke must differ from the baseline.
        let after = hwnd.and_then(probe::capture);
        let (paint_proven, changed_fraction) = match (before, after) {
            (Some((w0, h0, b0)), Some((w1, h1, b1)))
                if w0 == w1 && h0 == h1 && b0.len() == b1.len() =>
            {
                let (_, fraction) = probe::changed_pixels(&b0, &b1);
                (fraction > 0.0005, fraction)
            }
            _ => (false, 0.0),
        };

        let intervals = stroke_pace.intervals();
        let frame_p50_ms = timing::p50(&intervals).as_secs_f64() * 1000.0;
        let frame_p95_ms = timing::p95(&intervals).as_secs_f64() * 1000.0;
        let frames_ge_33 = timing::count_at_least(&intervals, Duration::from_millis(33));
        let update_p50_ms = timing::p50(&latencies).as_secs_f64() * 1000.0;
        let update_p95_ms = timing::p95(&latencies).as_secs_f64() * 1000.0;
        let update_max_ms = latencies
            .iter()
            .copied()
            .max()
            .unwrap_or(Duration::ZERO)
            .as_secs_f64()
            * 1000.0;
        let commit_p50_ms = timing::p50(&commits).as_secs_f64() * 1000.0;
        let commit_p95_ms = timing::p95(&commits).as_secs_f64() * 1000.0;
        let commit_max_ms = commits
            .iter()
            .copied()
            .max()
            .unwrap_or(Duration::ZERO)
            .as_secs_f64()
            * 1000.0;

        let dense_samples_ms: Vec<f64> = latencies
            .iter()
            .map(|sample| round2(sample.as_secs_f64() * 1000.0))
            .collect();
        let commit_samples_ms: Vec<f64> = commits
            .iter()
            .map(|sample| round2(sample.as_secs_f64() * 1000.0))
            .collect();

        let display = match self.cfg.presentmon_csv.as_deref() {
            Some(path) => {
                match presentmon::summarize_csv(Path::new(path), self.cfg.presentmon_skip_ms) {
                    Ok(summary) => Some(summary),
                    Err(error) => fail(
                        &self.cfg,
                        &format!("cannot read PresentMon CSV {path}: {error}"),
                    ),
                }
            }
            None => None,
        };

        let mut verdicts: Vec<String> = Vec::new();
        if self.cfg.red == "upload" && upload_full_ms > 16.7 {
            verdicts.push(format!("upload_full_ms {upload_full_ms:.2} > 16.7"));
        }
        if let Some(first) = opening_first_ms {
            if first > 82.0 {
                verdicts.push(format!("opening_first_ms {first:.2} > 82.0"));
            }
        }
        if upload_dirty_ms > 16.7 {
            verdicts.push(format!("upload_dirty_ms {upload_dirty_ms:.2} > 16.7"));
        }
        if let Some(display) = &display {
            if self.cfg.mode == "warm" && display.p95_ms > 16.9 {
                verdicts.push(format!("display_p95_ms {:.2} > 16.9", display.p95_ms));
            }
            if display.frames_ge_33 > 0 {
                verdicts.push(format!(
                    "display_frames_ge_33ms {} > 0",
                    display.frames_ge_33
                ));
            }
        } else {
            if self.cfg.mode == "warm" && frame_p95_ms > 16.9 {
                verdicts.push(format!("frame_p95_ms {frame_p95_ms:.2} > 16.9"));
            }
            if frames_ge_33 > 0 {
                verdicts.push(format!("frames_ge_33ms {frames_ge_33} > 0"));
            }
        }
        if !paint_proven {
            verdicts.push(format!(
                "paint not proven (changed fraction {changed_fraction:.6})"
            ));
        }
        let verdict_ok = verdicts.is_empty();
        let verdict_note = if display.is_some() {
            "displayed intervals from PresentMon; application intervals are diagnostic"
        } else {
            "application present interval; displayed gate not supplied"
        };

        let row = serde_json::json!({
            "shot": "measure-frames",
            "harness": "canvas_bridge",
            "mode": self.cfg.mode.as_str(),
            "size": self.width,
            "requested_size": if self.cfg.demo { None } else { Some(self.cfg.size) },
            "width": self.width,
            "height": self.height,
            "layers": if self.cfg.demo { 1 } else { self.cfg.layers },
            "content": if self.cfg.demo { "demo" } else { self.cfg.content.as_str() },
            "fixture_digest_fnv64": format!("{:016x}", self.fixture_digest),
            "fixture_png": self.cfg.fixture_png.clone(),
            "demo": self.cfg.demo,
            "control": false,
            "red": if self.cfg.red.is_empty() { None } else { Some(self.cfg.red.as_str()) },
            "strokes": self.cfg.strokes,
            "moves": self.cfg.moves,
            "warmup_strokes": self.cfg.warmup,
            "brush_px": self.cfg.brush_px,
            "endpoint_clock": "monotonic Instant; start = document update issued, end = profiler present_end",
            "upload_full_ms": round2(upload_full_ms),
            "upload_full_record_only": true,
            "opening_first_ms": opening_first_ms.map(round2),
            "opening_total_ms": opening_total_ms.map(round2),
            "upload_dirty_ms": round2(upload_dirty_ms),
            "dirty_upload": true,
            "dirty_strategy": "base_tiles_plus_patch_overlay",
            "tile_size": self.cfg.tile_size,
            "commit_moves": self.cfg.commit_moves,
            "patch_union": self.cfg.patch_union,
            "progressive": self.cfg.progressive,
            "dirty_sample_count": dirty_samples.len(),
            "dirty_samples_ms": dirty_samples_ms,
            "dirty_max_index": dirty_max_index,
            "frame_p50_ms": round2(frame_p50_ms),
            "frame_p95_ms": round2(frame_p95_ms),
            "frame_p95_scope": "application",
            "frames_ge_33ms": frames_ge_33,
            "frame_interval_count": intervals.len(),
            "display_p50_ms": display.as_ref().map(|summary| round2(summary.p50_ms)),
            "display_p95_ms": display.as_ref().map(|summary| round2(summary.p95_ms)),
            "display_frames_ge_33ms": display.as_ref().map(|summary| summary.frames_ge_33),
            "display_sample_count": display.as_ref().map(|summary| summary.sample_count),
            "display_skipped": display.as_ref().map(|summary| summary.skipped),
            "display_skip_ms": round2(self.cfg.presentmon_skip_ms),
            "display_source": display.as_ref().map(|_| "presentmon"),
            "update_p50_ms": round2(update_p50_ms),
            "update_p95_ms": round2(update_p95_ms),
            "update_max_ms": round2(update_max_ms),
            "update_count": latencies.len(),
            "update_samples_ms": dense_samples_ms,
            "commit_count": commits.len(),
            "commit_p50_ms": round2(commit_p50_ms),
            "commit_p95_ms": round2(commit_p95_ms),
            "commit_max_ms": round2(commit_max_ms),
            "commit_samples_ms": commit_samples_ms,
            "paint_proven": paint_proven,
            "paint_changed_frac": round6(changed_fraction),
            "adapter": probe::adapter_string(),
            "machine": machine_line(),
            "verdict_ok": verdict_ok,
            "verdict": verdicts,
            "verdict_note": verdict_note,
        });
        emit(&self.cfg, row, if verdict_ok { 0 } else { 1 });
    }
}
