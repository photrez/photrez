//! One interaction and the frames it produced: the event and drag cases, the slider bounds and the drag sample.

use super::*;

impl Runner {
    /// Click, hover and drag cases: dispatch one production interaction,
    /// then time the first same-window frame that starts after the dispatch
    /// and its following Present. No document commit is involved, so the App
    /// render marker is not the endpoint; the profiler events are.
    pub(super) async fn run_event_case(
        &self,
        cx: &mut gpui_kit::AsyncApp,
    ) -> Result<Vec<Value>, String> {
        if self.config.case.is_drag() {
            self.prepare_drag_case(cx).await?;
        }
        let events_per_run = self.config.strokes.max(1);
        let mut runs = Vec::with_capacity(self.config.samples);
        for sample in 1..=self.config.samples {
            if self.config.warmup > 0 {
                for index in 0..self.config.warmup {
                    self.event_or_drag_sample(index, cx).await?;
                }
            }
            let mut events = Vec::with_capacity(events_per_run);
            for index in 0..events_per_run {
                match self.event_or_drag_sample(index, cx).await {
                    Ok(value) => events.push(value),
                    Err(error) => return Err(format("sample {sample}: {error}")),
                }
            }
            let matched = events
                .iter()
                .filter(|event| event.get("present").and_then(Value::as_f64).is_some())
                .count();
            let aggregate = aggregate_events(&events);
            runs.push(json!({
                "sample": sample,
                "brush_size": Value::Null,
                "strokes": events,
                "matched_present_count": matched,
                "aggregate": aggregate,
            }));
        }
        Ok(runs)
    }

    async fn event_sample(
        &self,
        index: usize,
        cx: &mut gpui_kit::AsyncApp,
    ) -> Result<Value, String> {
        let bounds = self
            .app
            .update(cx, |app, _| app.input_measurement_bounds().get())
            .map_err(|error| format("App closed while reading canvas bounds: {error:?}"))?
            .ok_or_else(|| "production canvas bounds are missing".to_owned())?;
        let state = self.canonical(cx)?;
        // Two page points that land on different layers of the canonical
        // fixture, so each click changes the selection and forces a repaint.
        let pixel = match index % 2 {
            0 => (state.width / 3, state.height / 2),
            _ => (state.width * 2 / 3, state.height / 2),
        };
        let position = point_for_pixel(bounds, &state, pixel)?;
        let input = match self.config.case {
            Case::Hover => PlatformInput::MouseMove(MouseMoveEvent {
                position,
                pressed_button: None,
                ..Default::default()
            }),
            Case::Click | Case::Stroke => PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position,
                ..Default::default()
            }),
            _ => {
                return Err("drag cases must use a drag sample, not an event sample".to_owned());
            }
        };
        let mut collector = FrameTimingCollector::new();
        let mut frames: Vec<FrameRecord<WindowId>> = Vec::new();
        let window_id = self.window_identity.window_id();
        let start = Instant::now();
        self.dispatch_input(input, cx, index)?;
        if self.config.case == Case::Click {
            self.dispatch_input(
                PlatformInput::MouseUp(MouseUpEvent {
                    button: MouseButton::Left,
                    position,
                    ..Default::default()
                }),
                cx,
                index,
            )?;
        }
        let handler_ms = millis(start.elapsed());

        let deadline = Instant::now() + EVENT_WAIT_LIMIT;
        loop {
            self.collect_frames(&mut collector, &mut frames);
            if let Some(outcome) = first_frame_after(&frames, window_id, start) {
                return Ok(json!({
                    "sample": index,
                    "case": self.config.case.label(),
                    "status": "present",
                    "handler_ms": round4(handler_ms),
                    "draw_start_offset_ms": round4(outcome.draw_start_offset_ms),
                    "draw_duration_ms": round4(outcome.draw_duration_ms),
                    "present": round4(outcome.present_ms),
                    "point": [pixel.0, pixel.1],
                }));
            }
            if Instant::now() >= deadline {
                return Ok(json!({
                    "sample": index,
                    "case": self.config.case.label(),
                    "status": "no_frame",
                    "handler_ms": round4(handler_ms),
                    "present": Value::Null,
                    "point": [pixel.0, pixel.1],
                }));
            }
            cx.background_executor().timer(POLL_INTERVAL).await;
        }
    }

    async fn prepare_drag_case(&self, cx: &mut gpui_kit::AsyncApp) -> Result<(), String> {
        match self.config.case {
            Case::SliderAdjust => {
                self.set_inspector_tab(crate::state::InspectorTab::Adjustments, cx)?;
            }
            Case::SliderOpacity => {
                self.set_inspector_tab(crate::state::InspectorTab::Properties, cx)?;
            }
            Case::DialogDrag => {
                self.app
                    .update(cx, |app, app_cx| {
                        app.state.update(app_cx, |state, _| {
                            state.active_dialog = Some(crate::state::DialogId::About);
                        });
                    })
                    .map_err(|error| {
                        format("App closed while opening the About dialog: {error:?}")
                    })?;
                // Let the dialog frame paint before the first drag sample.
                cx.background_executor()
                    .timer(Duration::from_millis(150))
                    .await;
            }
            _ => {}
        }
        Ok(())
    }
    async fn event_or_drag_sample(
        &self,
        index: usize,
        cx: &mut gpui_kit::AsyncApp,
    ) -> Result<Value, String> {
        if self.config.case.is_drag() {
            self.drag_sample(index, cx).await
        } else {
            self.event_sample(index, cx).await
        }
    }

    async fn slider_bounds(&self, cx: &mut gpui_kit::AsyncApp) -> Result<Bounds<Pixels>, String> {
        let deadline = Instant::now() + WAIT_LIMIT;
        loop {
            let bounds = self
                .app
                .update(cx, |app, app_cx| {
                    let state = app.state.read(app_cx);
                    match self.config.case {
                        Case::SliderOpacity => state.opacity_slider_bounds.get(),
                        _ => state.adjustment_slider_bounds[0].get(),
                    }
                })
                .map_err(|error| format("App closed while reading slider bounds: {error:?}"))?;
            if let Some(bounds) = bounds {
                return Ok(bounds);
            }
            if Instant::now() >= deadline {
                return Err("timed out waiting for the production slider bounds".to_owned());
            }
            cx.background_executor().timer(POLL_INTERVAL).await;
        }
    }

    async fn drag_points(
        &self,
        cx: &mut gpui_kit::AsyncApp,
    ) -> Result<(MouseButton, Vec<gpui_kit::Point<Pixels>>), String> {
        let ordinal = self.drag_ordinal.get();
        self.drag_ordinal.set(ordinal + 1);
        let direction: f32 = if ordinal.is_multiple_of(2) { 1.0 } else { -1.0 };
        match self.config.case {
            Case::SliderAdjust | Case::SliderOpacity => {
                let bounds = self.slider_bounds(cx).await?;
                let centre = 0.55f32;
                let start_fraction = centre - direction * 0.20;
                let end_fraction = centre + direction * 0.20;
                let y = bounds.origin.y + bounds.size.height / 2.0;
                let point_at = |fraction: f32| {
                    gpui_kit::point(
                        bounds.origin.x + px(f32::from(bounds.size.width) * fraction),
                        y,
                    )
                };
                Ok((
                    MouseButton::Left,
                    vec![point_at(start_fraction), point_at(end_fraction)],
                ))
            }
            Case::DialogDrag => {
                let viewport = cx
                    .update_window(self.window.into(), |_, window, _| window.viewport_size())
                    .map_err(|error| {
                        format("window closed while reading the viewport: {error:?}")
                    })?;
                // About dialog shell is 400x296; titlebar is 36px tall.
                let base = gpui_kit::point(
                    px(f32::from(viewport.width) / 2.0),
                    px((f32::from(viewport.height) - 296.0) / 2.0 + 18.0),
                );
                let current: (f32, f32) = if ordinal.is_multiple_of(2) {
                    (0.0, 0.0)
                } else {
                    (40.0, 20.0)
                };
                let start = gpui_kit::point(
                    px(f32::from(base.x) + current.0 - direction * 20.0),
                    px(f32::from(base.y) + current.1 - direction * 10.0),
                );
                let end = gpui_kit::point(
                    px(f32::from(base.x) + current.0 + direction * 20.0),
                    px(f32::from(base.y) + current.1 + direction * 10.0),
                );
                Ok((MouseButton::Left, vec![start, end]))
            }
            Case::Pan => {
                let bounds = self
                    .app
                    .update(cx, |app, _| app.input_measurement_bounds().get())
                    .map_err(|error| format("App closed while reading canvas bounds: {error:?}"))?
                    .ok_or_else(|| "production canvas bounds are missing".to_owned())?;
                let centre =
                    bounds.origin + size(bounds.size.width / 2.0, bounds.size.height / 2.0).into();
                let start = centre + gpui_kit::point(px(-direction * 20.0), px(-direction * 10.0));
                let end = centre + gpui_kit::point(px(direction * 20.0), px(direction * 10.0));
                Ok((MouseButton::Middle, vec![start, end]))
            }
            Case::Transform => {
                let bounds = self
                    .app
                    .update(cx, |app, _| app.input_measurement_bounds().get())
                    .map_err(|error| format("App closed while reading canvas bounds: {error:?}"))?
                    .ok_or_else(|| "production canvas bounds are missing".to_owned())?;
                let state = self.canonical(cx)?;
                let width = state.width.max(1);
                let height = state.height.max(1);
                let start_pixel = ((width as f64 * 0.35) as u32, (height as f64 * 0.35) as u32);
                let end_pixel = ((width as f64 * 0.45) as u32, (height as f64 * 0.45) as u32);
                let (start_pixel, end_pixel) = if direction > 0.0 {
                    (start_pixel, end_pixel)
                } else {
                    (end_pixel, start_pixel)
                };
                let start = point_for_pixel(bounds, &state, start_pixel)?;
                let end = point_for_pixel(bounds, &state, end_pixel)?;
                Ok((MouseButton::Left, vec![start, end]))
            }
            _ => Err("drag points were requested for a non-drag case".to_owned()),
        }
    }

    async fn drag_sample(
        &self,
        index: usize,
        cx: &mut gpui_kit::AsyncApp,
    ) -> Result<Value, String> {
        let (button, points) = self.drag_points(cx).await?;
        let Some(start_point) = points.first().copied() else {
            return Err("drag has no start point".to_owned());
        };
        let Some(end_point) = points.get(1).copied() else {
            return Err("drag has no end point".to_owned());
        };
        let moves = self.config.moves.max(1);
        let mut collector = FrameTimingCollector::new();
        let mut journal_collector = self.journal_collector(cx)?;
        let mut view_totals: HashMap<(&'static str, &'static str), (f64, usize)> = HashMap::new();
        let mut frames: Vec<FrameRecord<WindowId>> = Vec::new();
        let window_id = self.window_identity.window_id();
        let total_start = Instant::now();
        let mut handler_total_ms = 0.0;
        self.dispatch_input(
            PlatformInput::MouseDown(MouseDownEvent {
                button,
                position: start_point,
                ..Default::default()
            }),
            cx,
            index,
        )?;
        let mut move_present_ms = Vec::new();
        let mut move_draw_durations = Vec::new();
        for step in 1..=moves {
            let fraction = step as f64 / moves as f64;
            let position = gpui_kit::point(
                px(f32::from(start_point.x)
                    + (f32::from(end_point.x) - f32::from(start_point.x)) * fraction as f32),
                px(f32::from(start_point.y)
                    + (f32::from(end_point.y) - f32::from(start_point.y)) * fraction as f32),
            );
            let step_start = Instant::now();
            self.dispatch_input(
                PlatformInput::MouseMove(MouseMoveEvent {
                    position,
                    pressed_button: Some(button),
                    ..Default::default()
                }),
                cx,
                index,
            )?;
            handler_total_ms += millis(step_start.elapsed());
            let Some(outcome) = self
                .wait_drag_frame(
                    &mut collector,
                    &mut frames,
                    &mut journal_collector,
                    &mut view_totals,
                    window_id,
                    step_start,
                    Duration::from_millis(500),
                    cx,
                )
                .await
            else {
                return Ok(json!({
                    "sample": index,
                    "case": self.config.case.label(),
                    "status": "no_frame",
                    "handler_ms": round4(millis(total_start.elapsed())),
                    "present": Value::Null,
                    "start": [f32::from(start_point.x), f32::from(start_point.y)],
                    "end": [f32::from(end_point.x), f32::from(end_point.y)],
                }));
            };
            move_present_ms.push(round4(outcome.1));
            move_draw_durations.extend(outcome.2);
        }
        let up_start = Instant::now();
        self.dispatch_input(
            PlatformInput::MouseUp(MouseUpEvent {
                button,
                position: end_point,
                ..Default::default()
            }),
            cx,
            index,
        )?;
        handler_total_ms += millis(up_start.elapsed());
        let final_outcome = self
            .wait_drag_frame(
                &mut collector,
                &mut frames,
                &mut journal_collector,
                &mut view_totals,
                window_id,
                up_start,
                EVENT_WAIT_LIMIT,
                cx,
            )
            .await;
        let Some((draw_offset, present_offset, final_draws)) = final_outcome else {
            return Ok(json!({
                "sample": index,
                "case": self.config.case.label(),
                "status": "no_frame",
                "handler_ms": round4(millis(total_start.elapsed())),
                "present": Value::Null,
                "start": [f32::from(start_point.x), f32::from(start_point.y)],
                "end": [f32::from(end_point.x), f32::from(end_point.y)],
            }));
        };
        let mut draw_durations = move_draw_durations.clone();
        draw_durations.extend(final_draws);
        let max_draw = draw_durations.iter().copied().fold(0.0f64, f64::max);
        let mut views: Vec<(f64, &'static str, &'static str, usize)> = view_totals
            .into_iter()
            .map(|((name, phase), (total_ms, count))| (total_ms, name, phase, count))
            .collect();
        views.sort_by(|left, right| right.0.total_cmp(&left.0));
        let view_timings: Vec<Value> = views
            .into_iter()
            .take(10)
            .map(|(total_ms, name, phase, count)| {
                json!({
                    "view": name,
                    "phase": phase,
                    "total_ms": round4(total_ms),
                    "count": count,
                })
            })
            .collect();
        Ok(json!({
            "sample": index,
            "case": self.config.case.label(),
            "status": "present",
            "handler_ms": round4(handler_total_ms),
            "total_handler_ms": round4(millis(total_start.elapsed())),
            "draw_start_offset_ms": round4(draw_offset),
            "draw_duration_ms": round4(draw_durations.first().copied().unwrap_or(0.0)),
            "draw_duration_max_ms": round4(max_draw),
            "draw_durations_ms": draw_durations
                .iter()
                .map(|value| round4(*value))
                .collect::<Vec<_>>(),
            "draw_frame_count": draw_durations.len(),
            "move_present_ms": move_present_ms,
            "view_timings": view_timings,
            "present": round4(present_offset),
            "start": [f32::from(start_point.x), f32::from(start_point.y)],
            "end": [f32::from(end_point.x), f32::from(end_point.y)],
        }))
    }

    #[allow(clippy::too_many_arguments)] // one drag receipt; a struct would only rename it
    async fn wait_drag_frame(
        &self,
        collector: &mut FrameTimingCollector,
        frames: &mut Vec<FrameRecord<WindowId>>,
        journal_collector: &mut ForegroundJournalCollector,
        view_totals: &mut HashMap<(&'static str, &'static str), (f64, usize)>,
        expected: WindowId,
        after: Instant,
        timeout: Duration,
        cx: &mut gpui_kit::AsyncApp,
    ) -> Option<(f64, f64, Vec<f64>)> {
        let deadline = Instant::now() + timeout;
        loop {
            self.collect_frames(collector, frames);
            Self::drain_view_timings(journal_collector, view_totals);
            if let Some(outcome) = Self::frame_outcome_all(frames, expected, after) {
                return Some(outcome);
            }
            if Instant::now() >= deadline {
                return None;
            }
            cx.background_executor().timer(POLL_INTERVAL).await;
        }
    }
}
