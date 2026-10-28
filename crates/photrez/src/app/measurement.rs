use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use gpui_kit::{Bounds, Context, EventEmitter, Pixels};

use super::App;

pub mod driver;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RefreshOutcome {
    Succeeded,
    Failed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InputMeasurementReceipt {
    pub sample_id: u64,
    pub doc_id: String,
    pub layer_id: String,
    pub t_down: Instant,
    pub t_up: Instant,
    pub t_preview: Option<Instant>,
    pub t_commit: Instant,
    pub epoch: u64,
    pub version: u64,
    pub refresh: RefreshOutcome,
    pub commit_phases: photrez_core::pixel::CommitPhases,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InputMeasurementRender {
    pub sample_id: u64,
    pub doc_id: String,
    pub version: u64,
    pub rendered_at: Instant,
}

#[derive(Clone, Debug, Default)]
pub struct InputMeasurementSnapshot {
    pub receipt: Option<InputMeasurementReceipt>,
    pub render: Option<InputMeasurementRender>,
}

impl EventEmitter<InputMeasurementReceipt> for App {}
impl EventEmitter<InputMeasurementRender> for App {}

#[derive(Default)]
pub(super) struct InputMeasurement {
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    next_sample_id: u64,
    active: Option<ActiveSample>,
    awaiting_render: Option<u64>,
    snapshot: InputMeasurementSnapshot,
}

#[derive(Default)]
struct ActiveSample {
    id: u64,
    down: Option<Instant>,
    up: Option<Instant>,
    preview: Option<Instant>,
}

impl InputMeasurement {
    fn bounds(&self) -> Rc<Cell<Option<Bounds<Pixels>>>> {
        Rc::clone(&self.bounds)
    }

    /// Share the app's one canvas-bounds sink: the display seam publishes once
    /// and both the camera and the measurement driver read the same rectangle.
    pub(super) fn share_bounds(&mut self, bounds: Rc<Cell<Option<Bounds<Pixels>>>>) {
        self.bounds = bounds;
    }

    fn start(&mut self) -> Option<u64> {
        if self.active.is_some() || self.awaiting_render.is_some() {
            return None;
        }
        self.next_sample_id = self.next_sample_id.checked_add(1)?;
        self.snapshot = InputMeasurementSnapshot::default();
        let id = self.next_sample_id;
        self.active = Some(ActiveSample {
            id,
            ..Default::default()
        });
        Some(id)
    }

    fn cancel(&mut self, sample_id: u64) -> bool {
        if self.active.as_ref().map(|sample| sample.id) == Some(sample_id) {
            self.active = None;
            return true;
        }
        if self.awaiting_render == Some(sample_id) {
            self.awaiting_render = None;
            return true;
        }
        false
    }

    pub(super) fn accepted_down(&mut self, at: Instant) {
        if let Some(sample) = self.active.as_mut() {
            if sample.down.is_none() {
                sample.down = Some(at);
            }
        }
    }

    pub(super) fn accepted_up(&mut self, at: Instant) {
        if let Some(sample) = self.active.as_mut() {
            if sample.down.is_some() && sample.up.is_none() {
                sample.up = Some(at);
            }
        }
    }

    pub(super) fn accepted_preview(&mut self, at: Instant) {
        if let Some(sample) = self.active.as_mut() {
            if sample.down.is_some() && sample.preview.is_none() {
                sample.preview = Some(at);
            }
        }
    }

    fn preview(&self) -> Option<Instant> {
        self.active.as_ref().and_then(|sample| sample.preview)
    }

    #[allow(clippy::too_many_arguments)] // one receipt record; a struct would only rename it
    pub(super) fn finish_commit(
        &mut self,
        at: Instant,
        doc_id: &str,
        layer_id: &str,
        epoch: u64,
        version: u64,
        refresh: RefreshOutcome,
        commit_phases: photrez_core::pixel::CommitPhases,
    ) -> Option<InputMeasurementReceipt> {
        let sample = self.active.take()?;
        let receipt = InputMeasurementReceipt {
            sample_id: sample.id,
            doc_id: doc_id.to_owned(),
            layer_id: layer_id.to_owned(),
            t_down: sample.down?,
            t_up: sample.up?,
            t_preview: sample.preview,
            t_commit: at,
            epoch,
            version,
            refresh,
            commit_phases,
        };
        self.snapshot.receipt = Some(receipt.clone());
        self.awaiting_render = (refresh == RefreshOutcome::Succeeded).then_some(sample.id);
        Some(receipt)
    }

    pub(super) fn refused_commit(&mut self) {
        self.active = None;
    }

    pub(super) fn rendered(
        &mut self,
        at: Instant,
        active_identity: Option<(&str, u64)>,
    ) -> Option<InputMeasurementRender> {
        let sample_id = self.awaiting_render.take()?;
        let receipt = self.snapshot.receipt.as_ref()?;
        let (doc_id, version) = active_identity?;
        if receipt.sample_id != sample_id
            || receipt.refresh != RefreshOutcome::Succeeded
            || receipt.doc_id != doc_id
            || receipt.version != version
            || self.snapshot.render.is_some()
        {
            return None;
        }
        let render = InputMeasurementRender {
            sample_id: receipt.sample_id,
            doc_id: receipt.doc_id.clone(),
            version: receipt.version,
            rendered_at: at,
        };
        self.snapshot.render = Some(render.clone());
        Some(render)
    }
}

impl App {
    /// Arms one bounded input sample; a second arm is rejected until completion or cancellation.
    pub fn start_input_measurement(&mut self) -> Option<u64> {
        self.measurement.start()
    }

    /// Cancels the matching outstanding sample without changing document state.
    pub fn cancel_input_measurement(&mut self, sample_id: u64) -> bool {
        self.measurement.cancel(sample_id)
    }

    /// Returns the latest immutable receipt and real App-render marker.
    pub fn input_measurement_snapshot(&self) -> InputMeasurementSnapshot {
        self.measurement.snapshot.clone()
    }

    /// Shares the geometry published by the production canvas display element.
    pub fn input_measurement_bounds(&self) -> Rc<Cell<Option<Bounds<Pixels>>>> {
        self.measurement.bounds()
    }

    /// First preview timestamp of the currently armed measurement, if any.
    pub fn input_measurement_preview(&self) -> Option<Instant> {
        self.measurement.preview()
    }

    /// Sets the Brush size for measurement runs; measurement-only.
    pub fn set_input_measurement_brush_size(&mut self, size: f64, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| state.brush_size = size);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::GesturePhase;
    use crate::render::canvas_bridge::fit_placement;
    use crate::services::file_io::{FileIo, LocalFileIo};
    use crate::state::ToolId;
    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{
        point, px, size, AppContext, Bounds, Entity, MouseButton, MouseDownEvent, MouseUpEvent,
        Pixels, PlatformInput, Point, Subscription, WindowBounds, WindowHandle, WindowOptions,
    };
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    const FIXTURE: &[u8] = include_bytes!("../../tests/fixtures/bounded-raster.ptz");
    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let id = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "photrez-input-measurement-{}-{id}",
                std::process::id()
            ));
            std::fs::create_dir(&path).expect("create test directory");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn mount_app(
        cx: &mut gpui_kit::TestAppContext,
        directory: &TestDirectory,
    ) -> (WindowHandle<gpui_kit::base::Root>, Entity<App>) {
        let source = directory.path().join("bounded-raster.ptz");
        std::fs::write(&source, FIXTURE).expect("copy fixture");
        let file_io = Arc::new(
            LocalFileIo::new(directory.path(), directory.path().join("approvals.json"))
                .expect("create file boundary"),
        );
        file_io.approve_path(&source).expect("approve fixture");

        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::views::title_bar::init(&cx.svg_renderer());
            crate::app::load_fonts(cx);
        });

        cx.update(|cx| {
            let bounds = Bounds {
                origin: Point::default(),
                size: size(px(1366.), px(820.)),
            };
            let (window, content) = gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                cx,
                move |window, cx| {
                    cx.new(|cx| App::new_with_file_io(window, None, Some(source), file_io, cx))
                },
            )
            .expect("open test window");
            (
                window
                    .downcast::<gpui_kit::base::Root>()
                    .expect("base root"),
                content,
            )
        })
    }

    fn set_brush(content: &Entity<App>, cx: &mut gpui_kit::TestAppContext) {
        cx.update_entity(content, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Brush;
                state.brush_size = 2.0;
                state.fg_color = [0, 0, 0, 255];
            });
        });
    }

    fn pixels(content: &Entity<App>, cx: &gpui_kit::TestAppContext) -> (u32, u32, Vec<u8>) {
        cx.read_entity(content, |app, _| {
            app.active_core_pixels_for_test()
                .expect("fixture has a raster layer")
        })
    }

    fn history_version(content: &Entity<App>, cx: &gpui_kit::TestAppContext) -> u64 {
        cx.read_entity(content, |app, _| {
            app.active_core_history_version_for_test()
                .expect("fixture has a history version")
        })
    }

    fn brush_position(window: &mut gpui_kit::Window, doc: (u32, u32)) -> Point<Pixels> {
        let bounds = window.find("canvas-hitbox").bounds();
        let (page_x, page_y, page_w, page_h) = fit_placement(
            bounds.size.width.into(),
            bounds.size.height.into(),
            doc.0 as f32,
            doc.1 as f32,
        )
        .expect("the canvas has a page placement");
        bounds.origin
            + point(
                px(page_x + 0.5 / doc.0 as f32 * page_w),
                px(page_y + 0.5 / doc.1 as f32 * page_h),
            )
    }

    fn dispatch_down(window: &mut gpui_kit::Window, at: Point<Pixels>, cx: &mut gpui_kit::App) {
        let _ = window.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: at,
                ..Default::default()
            }),
            cx,
        );
    }

    fn dispatch_up(window: &mut gpui_kit::Window, at: Point<Pixels>, cx: &mut gpui_kit::App) {
        let _ = window.dispatch_event(
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: at,
                ..Default::default()
            }),
            cx,
        );
    }

    fn subscribe_receipts(
        content: &Entity<App>,
        cx: &mut gpui_kit::TestAppContext,
    ) -> (Rc<RefCell<Vec<InputMeasurementReceipt>>>, Subscription) {
        let events = Rc::new(RefCell::new(Vec::new()));
        let captured = Rc::clone(&events);
        let subscription = cx.update_entity(content, |_, cx| {
            cx.subscribe_self(move |_, receipt: &InputMeasurementReceipt, _| {
                captured.borrow_mut().push(receipt.clone());
            })
        });
        (events, subscription)
    }

    fn subscribe_renders(
        content: &Entity<App>,
        cx: &mut gpui_kit::TestAppContext,
    ) -> (Rc<RefCell<Vec<InputMeasurementRender>>>, Subscription) {
        let events = Rc::new(RefCell::new(Vec::new()));
        let captured = Rc::clone(&events);
        let subscription = cx.update_entity(content, |_, cx| {
            cx.subscribe_self(move |_, marker: &InputMeasurementRender, _| {
                captured.borrow_mut().push(marker.clone());
            })
        });
        (events, subscription)
    }

    #[gpui_kit::test]
    fn real_hitbox_stroke_changes_core_and_emits_receipt(cx: &mut gpui_kit::TestAppContext) {
        let directory = TestDirectory::new();
        let (window, content) = mount_app(cx, &directory);
        set_brush(&content, cx);
        let (width, height, before) = pixels(&content, cx);
        let history_before = history_version(&content, cx);
        let (receipts, _receipt_subscription) = subscribe_receipts(&content, cx);
        let (renders, _render_subscription) = subscribe_renders(&content, cx);
        let sample_id = cx
            .update_entity(&content, |app, _| app.start_input_measurement())
            .expect("measurement sample is armed");
        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            let position = brush_position(window, (width, height));

            dispatch_down(window, position, cx);
            dispatch_up(window, position, cx);
            window.render_frame(cx);
        })
        .expect("test window remains open");

        let (after_width, after_height, after) = pixels(&content, cx);
        assert_eq!((after_width, after_height), (width, height));
        assert_ne!(
            after, before,
            "the production hitbox must change core pixels"
        );
        assert!(history_version(&content, cx) > history_before);
        assert_eq!(
            cx.read_entity(&content, |app, _| app.input_phase_for_test()),
            GesturePhase::Idle
        );

        let snapshot = cx.read_entity(&content, |app, _| app.input_measurement_snapshot());
        let receipt = snapshot
            .receipt
            .expect("the committed stroke emits an observation receipt");
        assert_eq!(receipt.sample_id, sample_id);
        assert_eq!(receipt.doc_id, "parity-doc");
        assert_eq!(receipt.layer_id, "raster-1");
        assert!(receipt.t_down <= receipt.t_up && receipt.t_up <= receipt.t_commit);
        assert!(
            receipt.t_preview.is_some(),
            "the brush preview timestamp is recorded"
        );
        assert!(receipt.version > history_before);
        assert_eq!(receipt.refresh, RefreshOutcome::Succeeded);
        assert_eq!(receipts.borrow().as_slice(), &[receipt.clone()]);

        let render = snapshot
            .render
            .expect("the real App render marks the same sample");
        assert_eq!(render.sample_id, sample_id);
        assert_eq!(render.doc_id, receipt.doc_id);
        assert_eq!(render.version, receipt.version);
        assert!(render.rendered_at >= receipt.t_commit);
        assert_eq!(renders.borrow().as_slice(), &[render]);
    }

    #[gpui_kit::test]
    fn missed_hitbox_does_not_emit_commit_receipt(cx: &mut gpui_kit::TestAppContext) {
        let directory = TestDirectory::new();
        let (window, content) = mount_app(cx, &directory);
        set_brush(&content, cx);
        let (_, _, before) = pixels(&content, cx);
        let history_before = history_version(&content, cx);
        let (receipts, _subscription) = subscribe_receipts(&content, cx);
        cx.update_entity(&content, |app, _| app.start_input_measurement())
            .expect("measurement sample is armed");
        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            let outside = point(px(1.), px(1.));

            dispatch_down(window, outside, cx);
            dispatch_up(window, outside, cx);
        })
        .expect("test window remains open");

        assert_eq!(pixels(&content, cx).2, before);
        assert_eq!(history_version(&content, cx), history_before);
        assert!(cx
            .read_entity(&content, |app, _| app.input_measurement_snapshot())
            .receipt
            .is_none());
        assert!(receipts.borrow().is_empty());
    }

    #[gpui_kit::test]
    fn omitted_up_does_not_emit_commit_receipt(cx: &mut gpui_kit::TestAppContext) {
        let directory = TestDirectory::new();
        let (window, content) = mount_app(cx, &directory);
        set_brush(&content, cx);
        let (width, height, before) = pixels(&content, cx);
        let history_before = history_version(&content, cx);
        let (receipts, _subscription) = subscribe_receipts(&content, cx);
        cx.update_entity(&content, |app, _| app.start_input_measurement())
            .expect("measurement sample is armed");
        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            let position = brush_position(window, (width, height));
            dispatch_down(window, position, cx);
        })
        .expect("test window remains open");

        assert_eq!(pixels(&content, cx).2, before);
        assert_eq!(history_version(&content, cx), history_before);
        assert!(cx
            .read_entity(&content, |app, _| app.input_measurement_snapshot())
            .receipt
            .is_none());
        assert!(receipts.borrow().is_empty());
    }

    #[test]
    fn preview_timestamp_survives_commit() {
        let mut measurement = InputMeasurement::default();
        measurement.start().expect("sample is armed");
        let at = Instant::now();
        measurement.accepted_down(at);
        measurement.accepted_preview(at + std::time::Duration::from_millis(1));
        measurement.accepted_up(at + std::time::Duration::from_millis(2));
        let receipt = measurement
            .finish_commit(
                at + std::time::Duration::from_millis(3),
                "doc",
                "layer",
                1,
                1,
                RefreshOutcome::Succeeded,
                photrez_core::pixel::CommitPhases::default(),
            )
            .expect("stroke commit produces a receipt");
        assert_eq!(
            receipt.t_preview,
            Some(at + std::time::Duration::from_millis(1))
        );
        assert!(measurement.preview().is_none());
    }

    #[test]
    fn failed_refresh_has_no_render_marker() {
        let mut measurement = InputMeasurement::default();
        let sample_id = measurement.start().expect("sample is armed");
        let at = Instant::now();
        measurement.accepted_down(at);
        measurement.accepted_up(at);
        let receipt = measurement
            .finish_commit(
                at,
                "doc",
                "layer",
                1,
                1,
                RefreshOutcome::Failed,
                photrez_core::pixel::CommitPhases::default(),
            )
            .expect("stroke commit produces a receipt");

        assert_eq!(receipt.sample_id, sample_id);
        assert!(measurement.rendered(at, Some(("doc", 1))).is_none());
        assert!(measurement.snapshot.render.is_none());
    }

    #[test]
    fn render_marker_requires_matching_document_and_history() {
        for active_identity in [("other", 1), ("doc", 2)] {
            let mut measurement = InputMeasurement::default();
            measurement.start().expect("sample is armed");
            let at = Instant::now();
            measurement.accepted_down(at);
            measurement.accepted_up(at);
            measurement
                .finish_commit(
                    at,
                    "doc",
                    "layer",
                    1,
                    1,
                    RefreshOutcome::Succeeded,
                    photrez_core::pixel::CommitPhases::default(),
                )
                .expect("stroke commit produces a receipt");

            assert!(measurement.rendered(at, Some(active_identity)).is_none());
            assert!(measurement.snapshot.render.is_none());
        }
    }
}
