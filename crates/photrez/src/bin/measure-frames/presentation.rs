use std::time::{Duration, Instant};

use super::timing;
use super::timing::Pacing;
use photrez::display::canvas_bridge::CanvasImage;

use super::{emit, fail, machine_line, probe, round2, round6, Runner, WaitMode};

impl Runner {
    /// Diagnostic floor: repaint the same window without ever mutating the
    /// canvas. The before/after capture must be unchanged, which is the inverse
    /// of the normal paint-prove and proves this really is an idle control.
    pub(super) async fn run_control(&mut self, cx: &mut gpui_kit::AsyncApp) {
        let total = self.cfg.strokes * self.cfg.moves;
        if self.cfg.warmup > 0 {
            let mut warm = Pacing::start();
            for _ in 0..self.cfg.warmup {
                self.repaint(cx, &mut warm).await;
            }
        }

        let hwnd = probe::self_hwnd();
        let before = hwnd.and_then(probe::capture);
        let mut pace = Pacing::start();
        for _ in 0..total {
            self.repaint(cx, &mut pace).await;
        }
        let after = hwnd.and_then(probe::capture);

        let (control_unchanged, changed_fraction) = match (before, after) {
            (Some((w0, h0, b0)), Some((w1, h1, b1)))
                if w0 == w1 && h0 == h1 && b0.len() == b1.len() =>
            {
                let (_, fraction) = probe::changed_pixels(&b0, &b1);
                (fraction <= 0.0005, fraction)
            }
            _ => (false, 0.0),
        };

        let intervals = pace.intervals();
        let frame_p50_ms = timing::p50(&intervals).as_secs_f64() * 1000.0;
        let frame_p95_ms = timing::p95(&intervals).as_secs_f64() * 1000.0;
        let frames_ge_33 = timing::count_at_least(&intervals, Duration::from_millis(33));
        let control_samples_ms: Vec<f64> = intervals
            .iter()
            .map(|sample| round2(sample.as_secs_f64() * 1000.0))
            .collect();
        let mut verdicts: Vec<String> = Vec::new();
        if !control_unchanged {
            verdicts.push(format!(
                "idle control canvas changed (fraction {changed_fraction:.6})"
            ));
        }
        let verdict_ok = verdicts.is_empty();

        let row = serde_json::json!({
            "shot": "measure-frames-control",
            "harness": "canvas_bridge",
            "profile": crate::support::build_profile(),
            "control": true,
            "diagnostic_only": true,
            "mode": self.cfg.mode.as_str(),
            "size": self.width,
            "requested_size": if self.cfg.demo { None } else { Some(self.cfg.size) },
            "width": self.width,
            "height": self.height,
            "layers": if self.cfg.demo { 1 } else { self.cfg.layers },
            "content": if self.cfg.demo { "demo" } else { self.cfg.content.as_str() },
            "fixture_digest_fnv64": format!("{:016x}", self.fixture_digest),
            "demo": self.cfg.demo,
            "strokes": self.cfg.strokes,
            "moves": self.cfg.moves,
            "warmup_repaints": self.cfg.warmup,
            "control_repaints": total,
            "present_count": pace.presents.len(),
            "endpoint_clock": "monotonic Instant; present intervals between consecutive profiler present_end",
            "frame_p50_ms": round2(frame_p50_ms),
            "frame_p95_ms": round2(frame_p95_ms),
            "frames_ge_33ms": frames_ge_33,
            "frame_interval_samples_ms": control_samples_ms,
            "paint_proven": false,
            "control_unchanged": control_unchanged,
            "paint_changed_frac": round6(changed_fraction),
            "paint_note": "idle repaint control: no canvas mutation; unchanged pixels are the expected result",
            "adapter": probe::adapter_string(),
            "machine": machine_line(),
            "verdict_ok": verdict_ok,
            "verdict": verdicts,
            "verdict_note": "diagnostic only; product budgets are not applied",
        });
        emit(&self.cfg, row, if verdict_ok { 0 } else { 1 });
    }

    async fn repaint(&self, cx: &mut gpui_kit::AsyncApp, pace: &mut Pacing) {
        let after = Instant::now();
        let updated = self.weak.update(cx, |_view, view_cx| {
            view_cx.notify();
            Ok::<(), String>(())
        });
        match updated {
            Ok(Ok(())) => {}
            Ok(Err(message)) => fail(&self.cfg, &message),
            Err(error) => fail(&self.cfg, &format!("window update failed: {error:?}")),
        }
        self.wait_present(cx, pace, after, WaitMode::Frame).await;
    }

    pub(super) async fn set_image(
        &mut self,
        cx: &mut gpui_kit::AsyncApp,
        rgba: Vec<u8>,
        after: Instant,
        pace: &mut Pacing,
    ) {
        self.set_image_with_limit(cx, rgba, after, pace, None).await;
    }

    pub(super) async fn set_image_with_limit(
        &mut self,
        cx: &mut gpui_kit::AsyncApp,
        rgba: Vec<u8>,
        after: Instant,
        pace: &mut Pacing,
        tile_limit: Option<usize>,
    ) {
        let (width, height) = (self.width, self.height);
        let tile_size = self.cfg.tile_size;
        let updated =
            self.weak.update(
                cx,
                |view, view_cx| match CanvasImage::from_rgba_with_tile_size_limit(
                    width, height, rgba, tile_size, tile_limit,
                ) {
                    Ok(image) => {
                        let previous = std::mem::replace(&mut view.image, image);
                        view.retired.extend(previous.into_textures());
                        view_cx.notify();
                        Ok(())
                    }
                    Err(error) => Err(format!("canvas conversion failed: {error:?}")),
                },
            );
        match updated {
            Ok(Ok(())) => {}
            Ok(Err(message)) => fail(&self.cfg, &message),
            Err(error) => fail(&self.cfg, &format!("window update failed: {error:?}")),
        }
        self.wait_present(cx, pace, after, WaitMode::Sample).await;
    }

    #[allow(clippy::too_many_arguments)] // the dirty rect + pacing is the protocol; a struct would obscure it
    pub(super) async fn push_dirty(
        &mut self,
        cx: &mut gpui_kit::AsyncApp,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
        after: Instant,
        pace: &mut Pacing,
        mode: WaitMode,
    ) {
        let updated = self.weak.update(cx, |view, view_cx| {
            match view.image.push_patch(x, y, width, height, &rgba) {
                Ok(()) => {
                    view_cx.notify();
                    Ok(())
                }
                Err(error) => Err(format!("dirty canvas patch failed: {error:?}")),
            }
        });
        match updated {
            Ok(Ok(())) => {}
            Ok(Err(message)) => fail(&self.cfg, &message),
            Err(error) => fail(&self.cfg, &format!("window update failed: {error:?}")),
        }
        self.wait_present(cx, pace, after, mode).await;
    }

    pub(super) async fn commit_patches(
        &mut self,
        cx: &mut gpui_kit::AsyncApp,
        after: Instant,
        pace: &mut Pacing,
    ) {
        let updated = self
            .weak
            .update(cx, |view, view_cx| match view.image.commit_patches() {
                Ok(retired) => {
                    view.retired.extend(retired);
                    view_cx.notify();
                    Ok(())
                }
                Err(error) => Err(format!("canvas patch commit failed: {error:?}")),
            });
        match updated {
            Ok(Ok(())) => {}
            Ok(Err(message)) => fail(&self.cfg, &message),
            Err(error) => fail(&self.cfg, &format!("window update failed: {error:?}")),
        }
        self.wait_present(cx, pace, after, WaitMode::Frame).await;
    }

    async fn wait_present(
        &self,
        cx: &mut gpui_kit::AsyncApp,
        pace: &mut Pacing,
        after: Instant,
        mode: WaitMode,
    ) {
        for _ in 0..96 {
            pace.drain();
            if pace.seen_after(after) {
                return;
            }

            match mode {
                // Wake on the next frame callback. The callback runs before
                // that frame's draw/present, so the previous present is already
                // in `Pacing`; if it predates `after`, register again and
                // observe the next frame.
                WaitMode::Frame => {
                    let (tx, rx) = futures::channel::oneshot::channel::<()>();
                    let scheduled = self.handle.update(cx, |_view, window, _app| {
                        window.on_next_frame(move |_, _| {
                            let _ = tx.send(());
                        });
                    });
                    if let Err(error) = scheduled {
                        fail(
                            &self.cfg,
                            &format!("window closed while waiting for a frame: {error:?}"),
                        );
                    }
                    if rx.await.is_err() {
                        fail(
                            &self.cfg,
                            "frame callback dropped while waiting for a frame",
                        );
                    }
                }
                // Isolated samples poll so the sample does not pay the
                // one-frame callback cost; the poll is bounded by the loop.
                WaitMode::Sample => {
                    cx.background_executor()
                        .timer(Duration::from_millis(2))
                        .await;
                }
            }
        }
        fail(&self.cfg, "timed out waiting for a presented frame");
    }
}
