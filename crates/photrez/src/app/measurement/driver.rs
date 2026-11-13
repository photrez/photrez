use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::profiler::journal::{
    ForegroundEvent, ForegroundJournalCollector, ForegroundJournalEntry,
};
use gpui_kit::profiler::{FrameEvent, FrameTimingCollector};
use gpui_kit::{
    px, size, AnyWindowHandle, App as GpuiApp, AppContext, Bounds, KeyDownEvent, Keystroke,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, PlatformInput, WindowBounds,
    WindowId, WindowOptions,
};
use serde_json::{json, Value};

use crate::app::App;
use crate::services::file_io::{FileIo, LocalFileIo};
use crate::services::settings::FileSettings;
use crate::state::ToolId;

use super::{InputMeasurementSnapshot, RefreshOutcome};

mod args;
mod canonical;
mod fixture;
mod metrics;
mod report;
mod sandbox;

#[cfg(test)]
mod tests;

use args::{parse_args, Case, Config, USAGE};
use canonical::{
    digest64, find_ink_target, inspect_canonical, nearby_pixel, pixel_diff, point_for_pixel,
    validate_committed_state, validate_snapshot,
};
use fixture::stage_fixture;
use metrics::{aggregate_events, aggregate_strokes, stroke_geometry};
use report::{
    correlate, emit, failure_json, first_frame_after, millis, round4, signed_millis, summary_json,
};
use sandbox::Sandbox;

const WAIT_LIMIT: Duration = Duration::from_secs(60);
/// A click or hover that paints nothing produces no frame; this is how long a
/// sample waits before it is recorded as `no_frame` instead of blocking a run.
const EVENT_WAIT_LIMIT: Duration = Duration::from_millis(2_000);
const POLL_INTERVAL: Duration = Duration::from_millis(2);

#[derive(Clone)]
struct CanonicalState {
    doc_id: String,
    layer_id: String,
    width: u32,
    height: u32,
    epoch: u64,
    version: u64,
    pixels: Vec<u8>,
    foreground: [u8; 4],
}

#[derive(Clone, Copy)]
enum FrameRecord<W> {
    Draw {
        window_id: W,
        start: Instant,
        end: Instant,
    },
    Present {
        window_id: W,
        end: Instant,
    },
}

struct Submission<W> {
    window_id: W,
    draw_start: Instant,
    draw_end: Instant,
    present_end: Instant,
}

enum Correlation<W> {
    Pending,
    Matched(Submission<W>),
    Rejected(&'static str),
}

#[derive(Clone, Copy)]
struct StrokeRequest {
    sample_index: usize,
    stroke_index: usize,
    brush_size: u32,
}

struct SampleReport<W> {
    request: StrokeRequest,
    dispatch_started: Instant,
    first_pixel_ms: Option<f64>,
    preview_push_ms: Option<f64>,
    before: CanonicalState,
    snapshot: InputMeasurementSnapshot,
    submission: Submission<W>,
    window_id_label: String,
    expected_window: W,
}

pub fn run() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let config = match parse_args(&args) {
        Ok(Some(config)) => config,
        Ok(None) => {
            println!("{USAGE}");
            return;
        }
        Err(error) => emit(failure_json(error), 2),
    };
    let sandbox = match Sandbox::create() {
        Ok(sandbox) => sandbox,
        Err(error) => emit(failure_json(error), 2),
    };
    let (fixture_path, fixture_label) = match stage_fixture(&sandbox.root, &config) {
        Ok(value) => value,
        Err(error) => {
            sandbox.cleanup();
            emit(failure_json(error), 2);
        }
    };
    // The sandbox is deleted when the run ends, so a receipt that has to hash
    // the measured fixture needs a copy that outlives the process.
    if let Some(path) = config.fixture_out.as_deref() {
        if let Err(error) = std::fs::copy(&fixture_path, path) {
            sandbox.cleanup();
            emit(
                failure_json(format!("cannot copy the fixture to {path}: {error}")),
                2,
            );
        }
    }
    let file_io: Arc<dyn FileIo> =
        match LocalFileIo::new(sandbox.root.clone(), sandbox.root.join("approvals.json")) {
            Ok(file_io) => Arc::new(file_io),
            Err(error) => {
                sandbox.cleanup();
                emit(
                    failure_json(format!("cannot create sandbox file boundary: {error:?}")),
                    2,
                );
            }
        };

    gpui_kit::application().run(move |cx: &mut GpuiApp| {
        gpui_kit::init(cx);
        crate::views::title_bar::init(&cx.svg_renderer());
        crate::app::load_fonts(cx);
        let _ = gpui_kit::profiler::set_trace_enabled(true);

        let startup_error = Rc::new(RefCell::new(None));
        let closure_error = Rc::clone(&startup_error);
        let closure_file_io = Arc::clone(&file_io);
        let closure_fixture = fixture_path.clone();
        let settings_path = sandbox.root.join("settings.json");
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(1366.), px(820.)), cx)),
            ..Default::default()
        };
        let (window_handle, app) = match gpui_kit::open_window(options, cx, move |window, cx| {
            cx.new(|cx| {
                let mut app = App::new(window, Some(FileSettings::at(settings_path)), None, cx);
                app.file_io = Some(Arc::clone(&closure_file_io));
                if let Err(error) =
                    app.open_startup_document(&closure_fixture, closure_file_io.as_ref(), cx)
                {
                    *closure_error.borrow_mut() = Some(error);
                }
                app
            })
        }) {
            Ok(result) => result,
            Err(error) => {
                sandbox.cleanup();
                emit(
                    failure_json(format!("cannot open native window: {error:?}")),
                    2,
                );
            }
        };
        if let Some(error) = startup_error.borrow_mut().take() {
            sandbox.cleanup();
            emit(failure_json(format!("fixture load failed: {error}")), 2);
        }
        let Some(window) = window_handle.downcast::<gpui_kit::base::Root>() else {
            sandbox.cleanup();
            emit(failure_json("cannot downcast native window root".into()), 2);
        };
        let runner = Runner {
            config,
            fixture_label,
            window,
            window_identity: window_handle,
            app: app.downgrade(),
            sandbox,
            drag_ordinal: Cell::new(0),
        };
        cx.spawn(async move |async_cx: &mut gpui_kit::AsyncApp| {
            runner.run(async_cx).await;
        })
        .detach();
    });
}

struct Runner {
    config: Config,
    fixture_label: String,
    window: gpui_kit::WindowHandle<gpui_kit::base::Root>,
    window_identity: AnyWindowHandle,
    app: gpui_kit::WeakEntity<App>,
    sandbox: Sandbox,
    /// Counts drag samples so a drag that moves state (a dialog offset, a
    /// camera, a layer transform) can alternate direction and return to its
    /// start instead of drifting.
    drag_ordinal: Cell<usize>,
}

impl Runner {
    async fn run(self, cx: &mut gpui_kit::AsyncApp) {
        if let Err(error) = self.wait_for_canvas_bounds(cx).await {
            self.finish(failure_json(error), 1);
        }
        if let Err(error) = self.select_tool(cx) {
            self.finish(failure_json(error), 1);
        }
        let baseline = match self.canonical(cx) {
            Ok(state) => state,
            Err(error) => self.finish(failure_json(error), 1),
        };

        if self.config.case != Case::Stroke {
            let runs = self.run_event_case(cx).await;
            match runs {
                Ok(runs) => {
                    let summary =
                        summary_json(&self.config, &self.fixture_label, runs, "ok", &baseline);
                    self.finish(summary, 0);
                }
                Err(error) => self.finish(failure_json(error), 1),
            }
        }

        let samples = self.config.samples;
        let strokes_per_run = self.config.strokes;
        let brush_sizes = self.config.brush_sizes.clone();
        let first_brush = brush_sizes.first().copied().unwrap_or(2);
        if let Err(error) = self.set_brush_size(first_brush, cx) {
            self.finish(failure_json(error), 1);
        }
        // A warm run does not charge first-use texture costs to the measured
        // samples; the warm-up strokes are undone like any run.
        if self.config.warmup > 0 {
            for stroke in 0..self.config.warmup {
                let request = StrokeRequest {
                    sample_index: 0,
                    stroke_index: stroke,
                    brush_size: first_brush,
                };
                if let Err(error) = self.run_sample(request, cx).await {
                    self.finish(failure_json(error), 1);
                }
            }
            if let Err(error) = self.reset_run(self.config.warmup, &baseline, cx) {
                self.finish(failure_json(error), 1);
            }
        }
        let mut runs = Vec::with_capacity(samples.saturating_mul(brush_sizes.len()));
        for sample in 1..=samples {
            for &brush_size in &brush_sizes {
                if let Err(error) = self.set_brush_size(brush_size, cx) {
                    self.finish(failure_json(error), 1);
                }
                let mut strokes = Vec::with_capacity(strokes_per_run);
                for stroke in 0..strokes_per_run {
                    let request = StrokeRequest {
                        sample_index: sample,
                        stroke_index: stroke,
                        brush_size,
                    };
                    match self.run_sample(request, cx).await {
                        Ok(value) => strokes.push(value),
                        Err(error) => self.finish(failure_json(error), 1),
                    }
                }
                if let Err(error) = self.reset_run(strokes_per_run, &baseline, cx) {
                    self.finish(failure_json(error), 1);
                }
                let aggregate = aggregate_strokes(&strokes);
                runs.push(json!({
                    "sample": sample,
                    "brush_size": brush_size,
                    "strokes": strokes,
                    "aggregate": aggregate,
                }));
            }
        }
        let summary = summary_json(&self.config, &self.fixture_label, runs, "ok", &baseline);
        self.finish(summary, 0);
    }

    fn canonical(&self, cx: &mut gpui_kit::AsyncApp) -> Result<CanonicalState, String> {
        self.app
            .update(cx, inspect_canonical)
            .map_err(|error| format!("App closed while reading canonical state: {error:?}"))?
    }

    fn reset_run(
        &self,
        strokes: usize,
        baseline: &CanonicalState,
        cx: &mut gpui_kit::AsyncApp,
    ) -> Result<(), String> {
        self.app
            .update(cx, |app, app_cx| {
                app.input_measurement_undo(strokes, app_cx)
            })
            .map_err(|error| format!("App closed while resetting the run: {error:?}"))?;
        let after = self.canonical(cx)?;
        if after.layer_id != baseline.layer_id || after.pixels != baseline.pixels {
            return Err("document pixels did not return to the run baseline after undo".to_owned());
        }
        Ok(())
    }

    fn set_brush_size(&self, brush_size: u32, cx: &mut gpui_kit::AsyncApp) -> Result<(), String> {
        self.app
            .update(cx, |app, app_cx| {
                app.set_input_measurement_brush_size(brush_size as f64, app_cx);
            })
            .map_err(|error| format!("App closed while setting brush size: {error:?}"))
    }

    async fn wait_for_canvas_bounds(&self, cx: &mut gpui_kit::AsyncApp) -> Result<(), String> {
        let deadline = Instant::now() + WAIT_LIMIT;
        loop {
            let bounds = self
                .app
                .update(cx, |app, _| app.input_measurement_bounds().get())
                .map_err(|error| {
                    format!("App closed while waiting for canvas render: {error:?}")
                })?;
            if bounds.is_some() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err("timed out waiting for the production canvas bounds".into());
            }
            cx.background_executor().timer(POLL_INTERVAL).await;
        }
    }

    /// Selects the tool the case drives through the production Canvas key
    /// binding: Move for a metadata click or a transform, Brush for a stroke
    /// or a hover. Slider, dialog and pan cases need no tool switch.
    fn select_tool(&self, cx: &mut gpui_kit::AsyncApp) -> Result<(), String> {
        let (key, expected, label) = match self.config.case {
            Case::Click | Case::Transform => ("v", ToolId::Move, "Move"),
            Case::Hover | Case::Stroke => ("b", ToolId::Brush, "Brush"),
            Case::SliderAdjust | Case::SliderOpacity | Case::DialogDrag | Case::Pan => {
                return Ok(());
            }
        };
        let focus_handle = self
            .app
            .update(cx, |app, _| app.canvas_focus_handle.clone())
            .map_err(|error| format!("App closed while preparing Canvas focus: {error:?}"))?;
        cx.update_window(self.window.into(), |_, window, cx| {
            focus_handle.focus(window, cx);
            if !focus_handle.is_focused(window) {
                return Err("production Canvas focus was not acquired".to_owned());
            }
            let keystroke = Keystroke::parse(key)
                .map_err(|error| format!("cannot build {label} keystroke: {error}"))?;
            let _ = window.dispatch_event(
                PlatformInput::KeyDown(KeyDownEvent {
                    keystroke,
                    is_held: false,
                    prefer_character_input: false,
                }),
                cx,
            );
            Ok(())
        })
        .map_err(|error| format!("window closed while selecting {label}: {error:?}"))??;
        let selected = self
            .app
            .update(cx, move |app, app_cx| {
                app.state.read(app_cx).active_tool == expected
            })
            .map_err(|error| format!("App closed during {label} key dispatch: {error:?}"))?;
        if selected {
            Ok(())
        } else {
            Err(format!(
                "focused Canvas key dispatch did not select {label}"
            ))
        }
    }

    /// Click, hover and drag cases: dispatch one production interaction,
    /// then time the first same-window frame that starts after the dispatch
    /// and its following Present. No document commit is involved, so the App
    /// render marker is not the endpoint; the profiler events are.
    async fn run_event_case(&self, cx: &mut gpui_kit::AsyncApp) -> Result<Vec<Value>, String> {
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
                    Err(error) => return Err(format!("sample {sample}: {error}")),
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
            .map_err(|error| format!("App closed while reading canvas bounds: {error:?}"))?
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

    fn journal_collector(
        &self,
        cx: &mut gpui_kit::AsyncApp,
    ) -> Result<ForegroundJournalCollector, String> {
        cx.update_window(self.window.into(), |_, _, cx| {
            cx.foreground_journal().collector()
        })
        .map_err(|error| format!("window closed while starting the view profiler: {error:?}"))
    }

    fn drain_view_timings(
        collector: &mut ForegroundJournalCollector,
        totals: &mut HashMap<(&'static str, &'static str), (f64, usize)>,
    ) {
        for entry in collector.collect_unseen().entries {
            if let ForegroundJournalEntry::Event(ForegroundEvent::View(timing)) = entry {
                let elapsed = millis(timing.end.saturating_duration_since(timing.start));
                let entry = totals
                    .entry((timing.name, timing.phase.as_str()))
                    .or_insert((0.0, 0));
                entry.0 += elapsed;
                entry.1 += 1;
            }
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
                        format!("App closed while opening the About dialog: {error:?}")
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

    fn set_inspector_tab(
        &self,
        tab: crate::state::InspectorTab,
        cx: &mut gpui_kit::AsyncApp,
    ) -> Result<(), String> {
        self.app
            .update(cx, |app, app_cx| {
                app.state.update(app_cx, |state, _| {
                    state.inspector_tab = tab;
                });
            })
            .map_err(|error| format!("App closed while selecting the inspector tab: {error:?}"))?;
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
                .map_err(|error| format!("App closed while reading slider bounds: {error:?}"))?;
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
                        format!("window closed while reading the viewport: {error:?}")
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
                    .map_err(|error| format!("App closed while reading canvas bounds: {error:?}"))?
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
                    .map_err(|error| format!("App closed while reading canvas bounds: {error:?}"))?
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

    fn frame_outcome_all(
        frames: &[FrameRecord<WindowId>],
        expected: WindowId,
        after: Instant,
    ) -> Option<(f64, f64, Vec<f64>)> {
        let draw_index = frames.iter().position(|event| {
            matches!(
                event,
                FrameRecord::Draw { window_id, start, .. }
                    if *window_id == expected && *start >= after
            )
        })?;
        let FrameRecord::Draw { start, .. } = frames[draw_index] else {
            return None;
        };
        let present_index = frames
            .iter()
            .skip(draw_index + 1)
            .position(|event| {
                matches!(
                    event,
                    FrameRecord::Present { window_id, .. } if *window_id == expected
                )
            })
            .map(|offset| draw_index + 1 + offset)?;
        let FrameRecord::Present {
            end: present_end, ..
        } = frames[present_index]
        else {
            return None;
        };
        let draw_durations = frames
            .iter()
            .filter_map(|event| match event {
                FrameRecord::Draw {
                    window_id,
                    start,
                    end,
                } if *window_id == expected && *start >= after && *start <= present_end => {
                    Some(millis(end.saturating_duration_since(*start)))
                }
                _ => None,
            })
            .collect();
        Some((
            millis(start.saturating_duration_since(after)),
            millis(present_end.saturating_duration_since(after)),
            draw_durations,
        ))
    }

    async fn run_sample(
        &self,
        request: StrokeRequest,
        cx: &mut gpui_kit::AsyncApp,
    ) -> Result<Value, String> {
        let StrokeRequest {
            sample_index,
            stroke_index,
            brush_size,
        } = request;
        let before = self.app.update(cx, inspect_canonical).map_err(|error| {
            format!("sample {sample_index}: cannot read pre-state: {error:?}")
        })??;
        let bounds = self
            .app
            .update(cx, |app, _| app.input_measurement_bounds().get())
            .map_err(|error| format!("sample {sample_index}: App closed: {error:?}"))?
            .ok_or_else(|| {
                format!("sample {sample_index}: production canvas bounds are missing")
            })?;
        let (start_pixel, end_pixel) = if self.config.canvas == 2 {
            let start = find_ink_target(&before)?;
            let end = nearby_pixel(start, before.width, before.height)
                .ok_or_else(|| "bounded fixture is too small for a move".to_owned())?;
            (start, end)
        } else {
            stroke_geometry(stroke_index, before.width, before.height, brush_size)
        };

        let moves = self.config.moves.max(1);
        let mut points = Vec::with_capacity(moves.saturating_add(1));
        points.push(point_for_pixel(bounds, &before, start_pixel)?);
        for step in 1..=moves {
            let fraction = step as f64 / moves as f64;
            let x = start_pixel.0 as f64 + (end_pixel.0 as f64 - start_pixel.0 as f64) * fraction;
            let y = start_pixel.1 as f64 + (end_pixel.1 as f64 - start_pixel.1 as f64) * fraction;
            points.push(point_for_pixel(
                bounds,
                &before,
                (x.round() as u32, y.round() as u32),
            )?);
        }

        let sample_id = self
            .app
            .update(cx, |app, _| app.start_input_measurement())
            .map_err(|error| format!("sample {sample_index}: App closed while arming: {error:?}"))?
            .ok_or_else(|| format!("sample {sample_index}: another measurement is outstanding"))?;
        let result = self
            .dispatch_and_observe(request, sample_id, before, &points, cx)
            .await;
        if result.is_err() {
            let _ = self
                .app
                .update(cx, |app, _| app.cancel_input_measurement(sample_id));
        }
        result
    }

    async fn dispatch_and_observe(
        &self,
        request: StrokeRequest,
        sample_id: u64,
        before: CanonicalState,
        points: &[gpui_kit::Point<Pixels>],
        cx: &mut gpui_kit::AsyncApp,
    ) -> Result<Value, String> {
        let sample_index = request.sample_index;
        let Some(start) = points.first().copied() else {
            return Err("stroke has no pointer points".to_owned());
        };
        let Some(up_position) = points.last().copied() else {
            return Err("stroke has no pointer-up position".to_owned());
        };
        let mut collector = FrameTimingCollector::new();
        let mut frames: Vec<FrameRecord<WindowId>> = Vec::new();
        let window_id = self.window_identity.window_id();
        let window_id_label = format!("{window_id:?}");
        let active_tool = self
            .app
            .update(cx, |app, app_cx| app.state.read(app_cx).active_tool)
            .map_err(|error| format!("App closed before sample dispatch: {error:?}"))?;
        if active_tool != ToolId::Brush {
            return Err("Brush was not active at sample dispatch".to_owned());
        }
        let dispatch_started = Instant::now();
        let down = MouseDownEvent {
            button: MouseButton::Left,
            position: start,
            ..Default::default()
        };
        self.dispatch_input(PlatformInput::MouseDown(down), cx, sample_index)?;

        let mut first_preview = self.app_preview(cx)?;
        let mut preview_push_ms =
            first_preview.map(|at| millis(at.saturating_duration_since(dispatch_started)));
        let mut first_pixel_ms = None;
        if let Some(preview_at) = first_preview {
            if let Some(present) = self
                .wait_for_preview_present(&mut collector, &mut frames, window_id, preview_at, cx)
                .await?
            {
                first_pixel_ms = Some(millis(present.saturating_duration_since(dispatch_started)));
            }
        }

        for position in points.iter().skip(1) {
            let moved = MouseMoveEvent {
                position: *position,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            };
            self.dispatch_input(PlatformInput::MouseMove(moved), cx, sample_index)?;
            if first_preview.is_none() {
                first_preview = self.app_preview(cx)?;
                if let Some(preview_at) = first_preview {
                    preview_push_ms = Some(millis(
                        preview_at.saturating_duration_since(dispatch_started),
                    ));
                    if let Some(present) = self
                        .wait_for_preview_present(
                            &mut collector,
                            &mut frames,
                            window_id,
                            preview_at,
                            cx,
                        )
                        .await?
                    {
                        first_pixel_ms =
                            Some(millis(present.saturating_duration_since(dispatch_started)));
                    }
                }
            }
        }
        let up = MouseUpEvent {
            button: MouseButton::Left,
            position: up_position,
            ..Default::default()
        };
        self.dispatch_input(PlatformInput::MouseUp(up), cx, sample_index)?;

        let deadline = Instant::now() + WAIT_LIMIT;
        loop {
            self.collect_frames(&mut collector, &mut frames);
            let snapshot = self
                .app
                .update(cx, |app, _| app.input_measurement_snapshot())
                .map_err(|error| {
                    format!("sample {sample_index}: App closed while observing: {error:?}")
                })?;
            if let Some(receipt) = snapshot.receipt.as_ref() {
                if receipt.sample_id != sample_id {
                    return Err(format!("sample {sample_index}: stale commit receipt"));
                }
                if receipt.refresh != RefreshOutcome::Succeeded {
                    return Err(format!("sample {sample_index}: document refresh failed"));
                }
            }
            if snapshot.render.is_some() {
                validate_snapshot(sample_id, &snapshot)
                    .map_err(|error| format!("sample {sample_index}: {error}"))?;
                let render = snapshot
                    .render
                    .as_ref()
                    .ok_or_else(|| format!("sample {sample_index}: render marker disappeared"))?;
                match correlate(&frames, window_id, render.rendered_at) {
                    Correlation::Matched(submission) => {
                        return self.finish_sample(
                            SampleReport {
                                request,
                                dispatch_started,
                                first_pixel_ms,
                                preview_push_ms,
                                before,
                                snapshot,
                                submission,
                                window_id_label,
                                expected_window: window_id,
                            },
                            cx,
                        );
                    }
                    Correlation::Pending => {}
                    Correlation::Rejected(reason) => {
                        return Err(format!("sample {sample_index}: {reason}"));
                    }
                }
            }
            if Instant::now() >= deadline {
                return Err(match (&snapshot.receipt, &snapshot.render) {
                    (None, _) => {
                        format!("sample {sample_index}: timed out waiting for commit receipt")
                    }
                    (Some(_), None) => {
                        format!("sample {sample_index}: timed out waiting for App render marker")
                    }
                    (Some(_), Some(_)) => format!(
                        "sample {sample_index}: timed out waiting for matching same-window Present"
                    ),
                });
            }
            cx.background_executor().timer(POLL_INTERVAL).await;
        }
    }

    fn dispatch_input(
        &self,
        input: PlatformInput,
        cx: &mut gpui_kit::AsyncApp,
        sample_index: usize,
    ) -> Result<(), String> {
        cx.update_window(self.window.into(), move |_, window, cx| {
            let _ = window.dispatch_event(input, cx);
        })
        .map_err(|error| format!("sample {sample_index}: window closed during dispatch: {error:?}"))
    }

    fn app_preview(&self, cx: &mut gpui_kit::AsyncApp) -> Result<Option<Instant>, String> {
        self.app
            .update(cx, |app, _| app.input_measurement_preview())
            .map_err(|error| format!("App closed while reading the preview timestamp: {error:?}"))
    }

    fn collect_frames(
        &self,
        collector: &mut FrameTimingCollector,
        frames: &mut Vec<FrameRecord<WindowId>>,
    ) {
        for event in collector.collect_unseen() {
            match event {
                FrameEvent::Draw(timing) => frames.push(FrameRecord::Draw {
                    window_id: timing.window_id,
                    start: timing.draw_start,
                    end: timing.draw_end,
                }),
                FrameEvent::Present(timing) => frames.push(FrameRecord::Present {
                    window_id: timing.window_id,
                    end: timing.present_end,
                }),
            }
        }
    }

    async fn wait_for_preview_present(
        &self,
        collector: &mut FrameTimingCollector,
        frames: &mut Vec<FrameRecord<WindowId>>,
        expected_window: WindowId,
        preview_at: Instant,
        cx: &mut gpui_kit::AsyncApp,
    ) -> Result<Option<Instant>, String> {
        let deadline = Instant::now() + WAIT_LIMIT;
        loop {
            self.collect_frames(collector, frames);
            let draw_index = frames.iter().position(|event| {
                matches!(
                    event,
                    FrameRecord::Draw { window_id, start, .. }
                        if *window_id == expected_window && *start >= preview_at
                )
            });
            if let Some(index) = draw_index {
                for event in frames.iter().skip(index + 1) {
                    if let FrameRecord::Present { window_id, end } = event {
                        if *window_id == expected_window {
                            return Ok(Some(*end));
                        }
                    }
                }
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
            cx.background_executor().timer(POLL_INTERVAL).await;
        }
    }

    fn finish_sample<W: Copy + PartialEq>(
        &self,
        report: SampleReport<W>,
        cx: &mut gpui_kit::AsyncApp,
    ) -> Result<Value, String> {
        let SampleReport {
            request,
            dispatch_started,
            first_pixel_ms,
            preview_push_ms,
            before,
            snapshot,
            submission,
            window_id_label,
            expected_window,
        } = report;
        let StrokeRequest {
            sample_index,
            stroke_index,
            brush_size,
        } = request;
        let receipt = snapshot
            .receipt
            .as_ref()
            .ok_or_else(|| format!("sample {sample_index}: missing commit receipt"))?;
        let render = snapshot
            .render
            .as_ref()
            .ok_or_else(|| format!("sample {sample_index}: missing App render marker"))?;
        let after = self.app.update(cx, inspect_canonical).map_err(|error| {
            format!("sample {sample_index}: cannot read post-state: {error:?}")
        })??;
        validate_committed_state(sample_index, &before, &after, receipt, render)?;
        if submission.window_id != expected_window {
            return Err(format!(
                "sample {sample_index}: matching Present belongs to another window"
            ));
        }
        if submission.present_end < receipt.t_commit {
            return Err(format!(
                "sample {sample_index}: matching Present predates commit"
            ));
        }
        let draw_duration = submission
            .draw_end
            .saturating_duration_since(submission.draw_start);
        let (changed_pixels, changed_tiles) = pixel_diff(&before, &after)?;
        let digest = digest64(&after.pixels);
        let commit_phases = json!({
            "crop_x": receipt.commit_phases.crop_x,
            "crop_y": receipt.commit_phases.crop_y,
            "crop_w": receipt.commit_phases.crop_w,
            "crop_h": receipt.commit_phases.crop_h,
            "premultiply_us": receipt.commit_phases.premultiply_us,
            "raster_us": receipt.commit_phases.raster_us,
            "patch_us": receipt.commit_phases.patch_us,
            "apply_us": receipt.commit_phases.apply_us,
        });
        Ok(json!({
            "sample": sample_index,
            "stroke": stroke_index,
            "brush_size": brush_size,
            "moves": self.config.moves,
            "sample_id": receipt.sample_id,
            "status": "ok",
            "doc_id": receipt.doc_id,
            "layer_id": receipt.layer_id,
            "epoch_before": before.epoch,
            "epoch": receipt.epoch,
            "version_before": before.version,
            "version": receipt.version,
            "canonical_bytes_changed": true,
            "refresh": "succeeded",
            "claim": "synthetic_input_to_commit_only",
            "first_pixel_ms": first_pixel_ms,
            "preview_push_ms": preview_push_ms,
            "pointer_to_brush_ms": first_pixel_ms,
            "dispatch_to_commit_ms": millis(receipt.t_commit.saturating_duration_since(dispatch_started)),
            "handler_entry_to_commit_ms": millis(receipt.t_commit.saturating_duration_since(receipt.t_down)),
            "up_to_commit_ms": millis(receipt.t_commit.saturating_duration_since(receipt.t_up)),
            "app_render_after_commit_ms": millis(render.rendered_at.saturating_duration_since(receipt.t_commit)),
            "window_id": window_id_label,
            "draw_start_offset_from_commit_ms": signed_millis(submission.draw_start, receipt.t_commit),
            "draw_end_offset_from_commit_ms": signed_millis(submission.draw_end, receipt.t_commit),
            "draw_duration_ms": millis(draw_duration),
            "commit_to_submission_completion_ms": millis(submission.present_end.saturating_duration_since(receipt.t_commit)),
            "changed_pixels": changed_pixels,
            "changed_tiles": changed_tiles,
            "commit_phases": commit_phases,
            "digest": format!("{digest:016x}"),
            "rss_bytes": null,
            "rss_status": "external_sampling_required",
            "physical_display_latency_ms": null,
            "physical_display_latency_status": "not_measured"
        }))
    }

    fn finish(&self, value: Value, code: i32) -> ! {
        self.sandbox.cleanup();
        emit(value, code)
    }
}
