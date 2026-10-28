use std::cell::RefCell;
use std::io::Write;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use gpui_kit::profiler::{FrameEvent, FrameTimingCollector};
use gpui_kit::{
    point, px, size, AnyWindowHandle, App as GpuiApp, AppContext, Bounds, KeyDownEvent, Keystroke,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, PlatformInput, WindowBounds,
    WindowId, WindowOptions,
};
use photrez_core::ptz::content::LayerType;
use serde_json::{json, Value};

use crate::app::App;
use crate::render::canvas_bridge::fit_placement;
use crate::services::file_io::{FileIo, LocalFileIo};
use crate::services::settings::FileSettings;
use crate::state::ToolId;

use super::{
    InputMeasurementReceipt, InputMeasurementRender, InputMeasurementSnapshot, RefreshOutcome,
};

const USAGE: &str = "measure-input [options]\n  --samples N       independent runs (default 1; max 10)\n  --strokes N       strokes per run (default 1; max 100)\n  --moves N         pointer moves per stroke (default 1; max 1000)\n  --canvas N        square canvas (2 = bounded fixture; default 2; max 4096)\n  --brush-sizes A,B brush sizes in document px (default 2)\n  --base blank|opaque base layer for a generated canvas (default opaque)\n  --help            show this help\n\nG8 example: --samples 3 --strokes 12 --moves 40 --canvas 4096 --brush-sizes 800,40 --base blank";
const MAX_SAMPLES: usize = 10;
const MAX_STROKES: usize = 100;
const MAX_MOVES: usize = 1_000;
const MAX_CANVAS: u32 = 4_096;
const MAX_BRUSH: u32 = 4_096;
const WAIT_LIMIT: Duration = Duration::from_secs(60);
const POLL_INTERVAL: Duration = Duration::from_millis(2);
const BOUNDED_FIXTURE: &[u8] = include_bytes!("../../../tests/fixtures/bounded-raster.ptz");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Base {
    Blank,
    Opaque,
}

impl Base {
    fn label(self) -> &'static str {
        match self {
            Self::Blank => "blank",
            Self::Opaque => "opaque",
        }
    }

    fn pixel(self) -> [u8; 4] {
        match self {
            Self::Blank => [0, 0, 0, 0],
            Self::Opaque => [255, 255, 255, 255],
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Config {
    samples: usize,
    strokes: usize,
    moves: usize,
    canvas: u32,
    brush_sizes: Vec<u32>,
    base: Base,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            samples: 1,
            strokes: 1,
            moves: 1,
            canvas: 2,
            brush_sizes: vec![2],
            base: Base::Opaque,
        }
    }
}

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

fn parse_args(args: &[String]) -> Result<Option<Config>, String> {
    let mut config = Config::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--help" => return Ok(None),
            "--samples" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--samples requires a value".to_owned())?;
                config.samples = value
                    .parse::<usize>()
                    .map_err(|_| format!("invalid sample count: {value}"))?;
                if !(1..=MAX_SAMPLES).contains(&config.samples) {
                    return Err(format!("--samples must be in 1..={MAX_SAMPLES}"));
                }
            }
            "--strokes" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--strokes requires a value".to_owned())?;
                config.strokes = value
                    .parse::<usize>()
                    .map_err(|_| format!("invalid stroke count: {value}"))?;
                if !(1..=MAX_STROKES).contains(&config.strokes) {
                    return Err(format!("--strokes must be in 1..={MAX_STROKES}"));
                }
            }
            "--moves" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--moves requires a value".to_owned())?;
                config.moves = value
                    .parse::<usize>()
                    .map_err(|_| format!("invalid move count: {value}"))?;
                if !(1..=MAX_MOVES).contains(&config.moves) {
                    return Err(format!("--moves must be in 1..={MAX_MOVES}"));
                }
            }
            "--canvas" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--canvas requires a value".to_owned())?;
                config.canvas = value
                    .parse::<u32>()
                    .map_err(|_| format!("invalid canvas size: {value}"))?;
                if !(2..=MAX_CANVAS).contains(&config.canvas) {
                    return Err(format!("--canvas must be in 2..={MAX_CANVAS}"));
                }
            }
            "--brush-sizes" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--brush-sizes requires a value".to_owned())?;
                let mut sizes = Vec::new();
                for part in value.split(',') {
                    let size = part
                        .parse::<u32>()
                        .map_err(|_| format!("invalid brush size: {part}"))?;
                    if !(1..=MAX_BRUSH).contains(&size) {
                        return Err(format!("brush sizes must be in 1..={MAX_BRUSH}"));
                    }
                    sizes.push(size);
                }
                if sizes.is_empty() {
                    return Err("--brush-sizes requires at least one value".to_owned());
                }
                config.brush_sizes = sizes;
            }
            "--base" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--base requires a value".to_owned())?;
                config.base = match value.as_str() {
                    "blank" => Base::Blank,
                    "opaque" => Base::Opaque,
                    other => return Err(format!("unknown base: {other}")),
                };
            }
            other => return Err(format!("unknown option: {other}")),
        }
        index += 1;
    }
    Ok(Some(config))
}

fn stage_fixture(root: &std::path::Path, config: &Config) -> Result<(PathBuf, String), String> {
    if config.canvas == 2 {
        let path = root.join("bounded-raster.ptz");
        std::fs::write(&path, BOUNDED_FIXTURE)
            .map_err(|error| format!("cannot stage bounded fixture: {error}"))?;
        return Ok((path, "bounded-raster.ptz".into()));
    }
    let label = format!("generated-{}-{}.ptz", config.canvas, config.base.label());
    let path = root.join(&label);
    let bytes = build_ptz(config.canvas, config.base)?;
    std::fs::write(&path, &bytes)
        .map_err(|error| format!("cannot stage generated fixture: {error}"))?;
    Ok((path, label))
}

fn build_ptz(size: u32, base: Base) -> Result<Vec<u8>, String> {
    let document = serde_json::json!({
        "format": "photrez-ptz",
        "version": 3,
        "id": "parity-doc",
        "name": format!("Generated {size} {}", base.label()),
        "width": size,
        "height": size,
        "layers": [{
            "id": "raster-1",
            "name": "Raster",
            "type": "raster",
            "visible": true,
            "opacity": 1.0,
            "locked": false,
            "blendMode": "normal",
            "transform": {
                "x": 0.0, "y": 0.0, "scaleX": 1.0, "scaleY": 1.0,
                "rotation": 0.0, "flipH": false, "flipV": false
            },
            "width": size,
            "height": size
        }]
    });
    let document = serde_json::to_vec(&document)
        .map_err(|error| format!("cannot encode fixture document: {error}"))?;
    let png = encode_png(size, size, base.pixel())?;
    let cursor = std::io::Cursor::new(Vec::new());
    let mut writer = zip::write::ZipWriter::new(cursor);
    writer
        .start_file("document.json", fixture_file_options())
        .map_err(|error| format!("cannot start fixture document entry: {error}"))?;
    writer
        .write_all(&document)
        .map_err(|error| format!("cannot write fixture document: {error}"))?;
    writer
        .start_file("layers/raster-1.png", fixture_file_options())
        .map_err(|error| format!("cannot start fixture layer entry: {error}"))?;
    writer
        .write_all(&png)
        .map_err(|error| format!("cannot write fixture layer: {error}"))?;
    let cursor = writer
        .finish()
        .map_err(|error| format!("cannot finish fixture archive: {error}"))?;
    Ok(cursor.into_inner())
}

fn fixture_file_options() -> zip::write::SimpleFileOptions {
    zip::write::SimpleFileOptions::DEFAULT
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(6))
        .system(zip::System::Dos)
        .external_attributes(0o600 << 16)
}

fn encode_png(width: u32, height: u32, pixel: [u8; 4]) -> Result<Vec<u8>, String> {
    if width == 0 || height == 0 {
        return Err("PNG dimensions must be non-zero".to_owned());
    }
    let row = (width as usize)
        .checked_mul(4)
        .and_then(|value| value.checked_add(1))
        .ok_or_else(|| "PNG row size overflow".to_owned())?;
    let raw_len = (height as usize)
        .checked_mul(row)
        .ok_or_else(|| "PNG data size overflow".to_owned())?;
    let mut raw = Vec::with_capacity(raw_len);
    for _ in 0..height {
        raw.push(0);
        for _ in 0..width {
            raw.extend_from_slice(&pixel);
        }
    }
    let zlib = zlib_store(&raw);
    let mut png = Vec::with_capacity(zlib.len() + 64);
    png.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    push_png_chunk(&mut png, b"IHDR", &ihdr)?;
    push_png_chunk(&mut png, b"IDAT", &zlib)?;
    push_png_chunk(&mut png, b"IEND", &[])?;
    Ok(png)
}

fn push_png_chunk(out: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) -> Result<(), String> {
    let length = u32::try_from(data.len()).map_err(|_| "PNG chunk is too large".to_owned())?;
    out.extend_from_slice(&length.to_be_bytes());
    out.extend_from_slice(tag);
    out.extend_from_slice(data);
    out.extend_from_slice(&crc32_chunks(&[tag, data]).to_be_bytes());
    Ok(())
}

fn zlib_store(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + data.len() / 65_535 * 5 + 16);
    out.extend_from_slice(&[0x78, 0x01]);
    let mut offset = 0;
    loop {
        let remaining = data.len().saturating_sub(offset);
        let take = remaining.min(65_535);
        let end = offset + take;
        let final_block = end == data.len();
        out.push(u8::from(final_block));
        let length = take as u16;
        out.extend_from_slice(&length.to_le_bytes());
        out.extend_from_slice(&(!length).to_le_bytes());
        if let Some(chunk) = data.get(offset..end) {
            out.extend_from_slice(chunk);
        }
        offset = end;
        if final_block {
            break;
        }
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn adler32(data: &[u8]) -> u32 {
    const MOD_ADLER: u32 = 65_521;
    let mut first = 1u32;
    let mut second = 0u32;
    for &byte in data {
        first = (first + byte as u32) % MOD_ADLER;
        second = (second + first) % MOD_ADLER;
    }
    (second << 16) | first
}

fn crc32_chunks(chunks: &[&[u8]]) -> u32 {
    let mut table = [0u32; 256];
    for (index, entry) in table.iter_mut().enumerate() {
        let mut value = index as u32;
        for _ in 0..8 {
            value = if value & 1 != 0 {
                0xEDB8_8320 ^ (value >> 1)
            } else {
                value >> 1
            };
        }
        *entry = value;
    }
    let mut crc = 0xFFFF_FFFFu32;
    for chunk in chunks {
        for &byte in *chunk {
            crc = table[((crc ^ byte as u32) & 0xFF) as usize] ^ (crc >> 8);
        }
    }
    crc ^ 0xFFFF_FFFF
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
        };
        cx.spawn(async move |async_cx: &mut gpui_kit::AsyncApp| {
            runner.run(async_cx).await;
        })
        .detach();
    });
}

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn create() -> Result<Self, String> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("system clock precedes Unix epoch: {error}"))?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "photrez-measure-input-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir(&root)
            .map_err(|error| format!("cannot create sandbox {}: {error}", root.display()))?;
        Ok(Self { root })
    }

    fn cleanup(&self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

struct Runner {
    config: Config,
    fixture_label: String,
    window: gpui_kit::WindowHandle<gpui_kit::base::Root>,
    window_identity: AnyWindowHandle,
    app: gpui_kit::WeakEntity<App>,
    sandbox: Sandbox,
}

impl Runner {
    async fn run(self, cx: &mut gpui_kit::AsyncApp) {
        if let Err(error) = self.wait_for_canvas_bounds(cx).await {
            self.finish(failure_json(error), 1);
        }
        if let Err(error) = self.select_brush(cx) {
            self.finish(failure_json(error), 1);
        }

        let samples = self.config.samples;
        let strokes_per_run = self.config.strokes;
        let brush_sizes = self.config.brush_sizes.clone();
        let mut runs = Vec::with_capacity(samples.saturating_mul(brush_sizes.len()));
        for sample in 1..=samples {
            for &brush_size in &brush_sizes {
                if let Err(error) = self.set_brush_size(brush_size, cx) {
                    self.finish(failure_json(error), 1);
                }
                let mut strokes = Vec::with_capacity(strokes_per_run);
                for stroke in 0..strokes_per_run {
                    match self.run_sample(sample, stroke, brush_size, cx).await {
                        Ok(value) => strokes.push(value),
                        Err(error) => self.finish(failure_json(error), 1),
                    }
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
        let summary = summary_json(&self.config, &self.fixture_label, runs, "ok");
        self.finish(summary, 0);
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

    fn select_brush(&self, cx: &mut gpui_kit::AsyncApp) -> Result<(), String> {
        let focus_handle = self
            .app
            .update(cx, |app, _| app.canvas_focus_handle.clone())
            .map_err(|error| format!("App closed while preparing Canvas focus: {error:?}"))?;
        cx.update_window(self.window.into(), |_, window, cx| {
            focus_handle.focus(window, cx);
            if !focus_handle.is_focused(window) {
                return Err("production Canvas focus was not acquired".to_owned());
            }
            let keystroke = Keystroke::parse("b")
                .map_err(|error| format!("cannot build Brush keystroke: {error}"))?;
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
        .map_err(|error| format!("window closed while selecting Brush: {error:?}"))??;
        let selected = self
            .app
            .update(cx, |app, app_cx| {
                app.state.read(app_cx).active_tool == ToolId::Brush
            })
            .map_err(|error| format!("App closed during Brush key dispatch: {error:?}"))?;
        if selected {
            Ok(())
        } else {
            Err("focused Canvas key dispatch did not select Brush".into())
        }
    }

    async fn run_sample(
        &self,
        sample_index: usize,
        stroke_index: usize,
        brush_size: u32,
        cx: &mut gpui_kit::AsyncApp,
    ) -> Result<Value, String> {
        let before = self
            .app
            .update(cx, |app, cx| inspect_canonical(app, cx))
            .map_err(|error| {
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
            .dispatch_and_observe(
                sample_index,
                stroke_index,
                brush_size,
                sample_id,
                before,
                &points,
                cx,
            )
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
        sample_index: usize,
        stroke_index: usize,
        brush_size: u32,
        sample_id: u64,
        before: CanonicalState,
        points: &[gpui_kit::Point<Pixels>],
        cx: &mut gpui_kit::AsyncApp,
    ) -> Result<Value, String> {
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
                            sample_index,
                            stroke_index,
                            brush_size,
                            dispatch_started,
                            first_pixel_ms,
                            preview_push_ms,
                            before,
                            snapshot,
                            submission,
                            window_id_label,
                            window_id,
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
        sample_index: usize,
        stroke_index: usize,
        brush_size: u32,
        dispatch_started: Instant,
        first_pixel_ms: Option<f64>,
        preview_push_ms: Option<f64>,
        before: CanonicalState,
        snapshot: InputMeasurementSnapshot,
        submission: Submission<W>,
        window_id_label: String,
        expected_window: W,
        cx: &mut gpui_kit::AsyncApp,
    ) -> Result<Value, String> {
        let receipt = snapshot
            .receipt
            .as_ref()
            .ok_or_else(|| format!("sample {sample_index}: missing commit receipt"))?;
        let render = snapshot
            .render
            .as_ref()
            .ok_or_else(|| format!("sample {sample_index}: missing App render marker"))?;
        let after = self
            .app
            .update(cx, |app, cx| inspect_canonical(app, cx))
            .map_err(|error| {
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

fn pixel_diff(before: &CanonicalState, after: &CanonicalState) -> Result<(usize, usize), String> {
    if before.width != after.width || before.height != after.height {
        return Err("canonical dimensions changed during a stroke".to_owned());
    }
    let width = before.width as usize;
    let height = before.height as usize;
    if width == 0 || height == 0 {
        return Err("canonical dimensions are zero".to_owned());
    }
    if before.pixels.len() != after.pixels.len() {
        return Err("canonical buffer length changed during a stroke".to_owned());
    }
    const TILE: usize = 256;
    let tiles_x = (width + TILE - 1) / TILE;
    let tiles_y = (height + TILE - 1) / TILE;
    let mut changed_tiles = vec![false; tiles_x.saturating_mul(tiles_y)];
    let mut changed_pixels = 0usize;
    for (index, (left, right)) in before
        .pixels
        .chunks_exact(4)
        .zip(after.pixels.chunks_exact(4))
        .enumerate()
    {
        if left != right {
            changed_pixels = changed_pixels.saturating_add(1);
            let x = index % width;
            let y = index / width;
            let tile = (y / TILE) * tiles_x + (x / TILE);
            if let Some(flag) = changed_tiles.get_mut(tile) {
                *flag = true;
            }
        }
    }
    Ok((
        changed_pixels,
        changed_tiles.iter().filter(|&&flag| flag).count(),
    ))
}

fn digest64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for &byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn stroke_geometry(
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

fn aggregate_strokes(strokes: &[Value]) -> Value {
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

fn percentile(sorted: &[f64], fraction: f64) -> f64 {
    let Some(last) = sorted.len().checked_sub(1) else {
        return 0.0;
    };
    let index = ((last as f64) * fraction).round() as usize;
    sorted.get(index.min(last)).copied().unwrap_or(0.0)
}

fn inspect_canonical(
    app: &mut App,
    cx: &mut gpui_kit::Context<App>,
) -> Result<CanonicalState, String> {
    let doc_id = app
        .active_document
        .as_ref()
        .ok_or_else(|| "no fixture document is active".to_owned())?
        .doc_id
        .clone();
    let session = app
        .sessions
        .get(&doc_id)
        .ok_or_else(|| "active document session is missing".to_owned())?;
    let loaded = session.loaded_document();
    let content = loaded
        .content_snapshot()
        .ok_or_else(|| "loaded document content is missing".to_owned())?;
    let layer_id = content
        .layers
        .iter()
        .find(|layer| matches!(layer.layer_type, LayerType::Raster))
        .map(|layer| layer.id.clone())
        .ok_or_else(|| "fixture has no raster layer".to_owned())?;
    let layer = loaded
        .registry
        .get_layer(&doc_id, &layer_id)
        .ok_or_else(|| "canonical raster pixels are missing".to_owned())?;
    let epoch = loaded
        .registry
        .get_epoch(&doc_id, &layer_id)
        .map_err(|error| format!("cannot read canonical epoch: {error}"))?;
    let version = loaded
        .registry
        .get_history_version(&doc_id)
        .ok_or_else(|| "canonical history version is missing".to_owned())?;
    Ok(CanonicalState {
        doc_id,
        layer_id,
        width: layer.width,
        height: layer.height,
        epoch,
        version,
        pixels: layer.pixels.clone(),
        foreground: app.state.read(cx).fg_color,
    })
}

fn find_ink_target(state: &CanonicalState) -> Result<(u32, u32), String> {
    let expected = (state.width as usize)
        .checked_mul(state.height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "fixture pixel dimensions overflow".to_owned())?;
    if expected != state.pixels.len() {
        return Err("canonical pixel buffer has unexpected dimensions".into());
    }
    let index = state
        .pixels
        .chunks_exact(4)
        .position(|pixel| pixel != state.foreground)
        .ok_or_else(|| "fixture has no pixel that differs from the Brush foreground".to_owned())?;
    Ok((
        (index % state.width as usize) as u32,
        (index / state.width as usize) as u32,
    ))
}

fn nearby_pixel(start: (u32, u32), width: u32, height: u32) -> Option<(u32, u32)> {
    if start.0 + 1 < width {
        Some((start.0 + 1, start.1))
    } else if start.0 > 0 {
        Some((start.0 - 1, start.1))
    } else if start.1 + 1 < height {
        Some((start.0, start.1 + 1))
    } else if start.1 > 0 {
        Some((start.0, start.1 - 1))
    } else {
        None
    }
}

fn point_for_pixel(
    bounds: Bounds<Pixels>,
    state: &CanonicalState,
    pixel: (u32, u32),
) -> Result<gpui_kit::Point<Pixels>, String> {
    let (page_x, page_y, page_width, page_height) = fit_placement(
        bounds.size.width.into(),
        bounds.size.height.into(),
        state.width as f32,
        state.height as f32,
    )
    .ok_or_else(|| "cannot fit fixture to production canvas".to_owned())?;
    Ok(bounds.origin
        + point(
            px(page_x + (pixel.0 as f32 + 0.5) / state.width as f32 * page_width),
            px(page_y + (pixel.1 as f32 + 0.5) / state.height as f32 * page_height),
        ))
}

fn validate_snapshot(
    sample_id: u64,
    snapshot: &InputMeasurementSnapshot,
) -> Result<(), &'static str> {
    let receipt = snapshot.receipt.as_ref().ok_or("missing commit receipt")?;
    let render = snapshot
        .render
        .as_ref()
        .ok_or("missing App render marker")?;
    if receipt.sample_id != sample_id || render.sample_id != sample_id {
        return Err("stale receipt or render marker belongs to another sample");
    }
    if receipt.doc_id != render.doc_id
        || receipt.version != render.version
        || receipt.refresh != RefreshOutcome::Succeeded
        || receipt.t_down > receipt.t_up
        || receipt.t_up > receipt.t_commit
        || render.rendered_at < receipt.t_commit
    {
        return Err("receipt and render identities do not match a successful commit");
    }
    Ok(())
}

fn validate_committed_state(
    sample_index: usize,
    before: &CanonicalState,
    after: &CanonicalState,
    receipt: &InputMeasurementReceipt,
    render: &InputMeasurementRender,
) -> Result<(), String> {
    if receipt.sample_id == 0
        || receipt.doc_id != before.doc_id
        || receipt.layer_id != before.layer_id
        || after.doc_id != before.doc_id
        || after.layer_id != before.layer_id
    {
        return Err(format!(
            "sample {sample_index}: document/layer identity changed"
        ));
    }
    if receipt.refresh != RefreshOutcome::Succeeded {
        return Err(format!("sample {sample_index}: document refresh failed"));
    }
    if receipt.epoch != after.epoch
        || receipt.version != after.version
        || receipt.epoch != before.epoch.saturating_add(1)
        || receipt.version != before.version.saturating_add(1)
    {
        return Err(format!(
            "sample {sample_index}: epoch/version receipt did not match the canonical commit"
        ));
    }
    if before.pixels == after.pixels {
        return Err(format!(
            "sample {sample_index}: Brush stroke did not change canonical pixels"
        ));
    }
    if receipt.t_down > receipt.t_up || receipt.t_up > receipt.t_commit {
        return Err(format!(
            "sample {sample_index}: receipt timestamps are out of order"
        ));
    }
    if let Some(preview) = receipt.t_preview {
        if preview < receipt.t_down || preview > receipt.t_up {
            return Err(format!(
                "sample {sample_index}: preview timestamp is out of order"
            ));
        }
    }
    if render.sample_id != receipt.sample_id
        || render.doc_id != receipt.doc_id
        || render.version != receipt.version
        || render.rendered_at < receipt.t_commit
    {
        return Err(format!(
            "sample {sample_index}: App render marker is stale or mismatched"
        ));
    }
    Ok(())
}

fn correlate<W: Copy + PartialEq>(
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
            return Correlation::Rejected("a new Draw preceded the matching Present");
        }
        FrameRecord::Present { window_id, end } => {
            if window_id != expected_window {
                return Correlation::Rejected("following Present belongs to another window");
            }
            if end < draw_end {
                return Correlation::Rejected("matching Present predates Draw completion");
            }
            return Correlation::Matched(Submission {
                window_id,
                draw_start,
                draw_end,
                present_end: end,
            });
        }
    }
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn signed_millis(at: Instant, origin: Instant) -> f64 {
    if at >= origin {
        millis(at.duration_since(origin))
    } else {
        -millis(origin.duration_since(at))
    }
}

fn summary_json(config: &Config, fixture: &str, runs: Vec<Value>, status: &str) -> Value {
    json!({
        "benchmark": "measure-input",
        "status": status,
        "claim": "synthetic_input_to_commit_only",
        "input_path": "Window::dispatch_event -> production canvas hitbox",
        "fixture": fixture,
        "workload": {
            "samples": config.samples,
            "strokes": config.strokes,
            "moves": config.moves,
            "canvas": config.canvas,
            "brush_sizes": config.brush_sizes,
            "base": if config.canvas == 2 { "bounded" } else { config.base.label() },
        },
        "profile": if cfg!(debug_assertions) { "dev" } else { "release" },
        "runs": runs,
        "metric_mapping": {
            "pointer_to_brush_ms": "first preview present after pointerdown (Q6 overlay path); null when no preview",
            "dispatch_to_commit_ms": "down dispatch to successful core commit (record-only)",
            "brush_commit_ms": "handler_entry_to_commit_ms",
            "brush_mouseup_ms": "up_to_commit_ms"
        },
        "display_observation": "same-window GPUI draw followed by platform submission completion",
        "rss_bytes": null,
        "rss_status": "external_sampling_required",
        "physical_display_latency_ms": null,
        "physical_display_latency_status": "not_measured",
        "machine": format!("windows-{} x {} cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|count| count.get()).unwrap_or(0)),
    })
}

fn failure_json(error: String) -> Value {
    json!({
        "benchmark": "measure-input",
        "status": "failed",
        "claim": "synthetic_input_to_commit_only",
        "error": error,
        "physical_display_latency_ms": null,
        "physical_display_latency_status": "not_measured",
    })
}

fn emit(value: Value, code: i32) -> ! {
    match serde_json::to_string(&value) {
        Ok(line) => println!("{line}"),
        Err(error) => eprintln!("measure-input: cannot encode result: {error}"),
    }
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(sample_id: u64, at: Instant) -> InputMeasurementRender {
        InputMeasurementRender {
            sample_id,
            doc_id: "doc".into(),
            version: 1,
            rendered_at: at,
        }
    }

    #[test]
    fn parser_bounds_workload() {
        assert_eq!(parse_args(&[]), Ok(Some(Config::default())));
        assert!(parse_args(&["--samples".into(), "2".into()]).is_ok());
        assert!(parse_args(&["--samples".into(), "0".into()]).is_err());
        assert!(parse_args(&["--samples".into(), "11".into()]).is_err());
        assert!(parse_args(&["--strokes".into(), "0".into()]).is_err());
        assert!(parse_args(&["--moves".into(), "1001".into()]).is_err());
        assert!(parse_args(&["--canvas".into(), "1".into()]).is_err());
        assert!(parse_args(&["--canvas".into(), "4097".into()]).is_err());
        assert!(parse_args(&["--brush-sizes".into(), "0".into()]).is_err());
        assert!(parse_args(&["--base".into(), "both".into()]).is_err());
        assert_eq!(parse_args(&["--help".into()]), Ok(None));
    }

    #[test]
    fn parser_accepts_g8_workload() {
        let args: Vec<String> = [
            "--samples",
            "3",
            "--strokes",
            "12",
            "--moves",
            "40",
            "--canvas",
            "4096",
            "--brush-sizes",
            "800,40",
            "--base",
            "blank",
        ]
        .iter()
        .map(|value| (*value).to_owned())
        .collect();
        let config = parse_args(&args)
            .expect("G8 arguments parse")
            .expect("G8 arguments are not help");
        assert_eq!(config.samples, 3);
        assert_eq!(config.strokes, 12);
        assert_eq!(config.moves, 40);
        assert_eq!(config.canvas, 4096);
        assert_eq!(config.brush_sizes, vec![800, 40]);
        assert_eq!(config.base, Base::Blank);
    }

    #[test]
    fn crc_and_adler_match_known_vectors() {
        assert_eq!(crc32_chunks(&[b"123456789"]), 0xCBF4_3926);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn png_encoder_emits_signature_and_idat() {
        let png = encode_png(2, 1, [12, 34, 56, 255]).expect("small PNG encodes");
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(png.windows(4).any(|window| window == b"IDAT"));
    }

    #[test]
    fn generated_fixture_is_a_zip_archive() {
        let bytes = build_ptz(2, Base::Blank).expect("generated fixture builds");
        assert!(bytes.starts_with(b"PK\x03\x04"));
        assert!(bytes.len() > 100);
    }

    #[test]
    fn percentile_reports_expected_quantiles() {
        let values = [1.0, 2.0, 3.0, 4.0];
        assert_eq!(percentile(&values, 0.5), 3.0);
        assert_eq!(percentile(&values, 0.95), 4.0);
    }

    #[test]
    fn pixel_diff_counts_pixels_and_tiles() {
        let before = CanonicalState {
            doc_id: "doc".into(),
            layer_id: "layer".into(),
            width: 2,
            height: 1,
            epoch: 0,
            version: 0,
            pixels: vec![0, 0, 0, 0, 10, 10, 10, 10],
            foreground: [0, 0, 0, 255],
        };
        let after = CanonicalState {
            pixels: vec![1, 0, 0, 0, 10, 10, 10, 10],
            ..before.clone()
        };
        let (pixels, tiles) = pixel_diff(&before, &after).expect("diff succeeds");
        assert_eq!(pixels, 1);
        assert_eq!(tiles, 1);
    }

    #[test]
    fn stroke_geometry_stays_inside_the_canvas() {
        for brush_size in [8, 40, 800] {
            let ((x0, y0), (x1, y1)) = stroke_geometry(11, 4096, 4096, brush_size);
            assert!(x0 <= x1 && x1 < 4096);
            assert!(y0 < 4096 && y1 < 4096);
        }
    }

    #[test]
    fn wrong_window_draw_and_present_are_rejected() {
        let start = Instant::now();
        let rendered_at = start + Duration::from_millis(2);
        let events = [
            FrameRecord::Draw {
                window_id: 2_u8,
                start,
                end: start + Duration::from_millis(3),
            },
            FrameRecord::Present {
                window_id: 2_u8,
                end: start + Duration::from_millis(4),
            },
        ];
        assert!(matches!(
            correlate(&events, 1_u8, rendered_at),
            Correlation::Rejected(_)
        ));
    }

    #[test]
    fn ambiguous_draw_association_is_rejected() {
        let start = Instant::now();
        let rendered_at = start + Duration::from_millis(1);
        let events = [
            FrameRecord::Draw {
                window_id: 1_u8,
                start,
                end: start + Duration::from_millis(3),
            },
            FrameRecord::Present {
                window_id: 1_u8,
                end: start + Duration::from_millis(4),
            },
            FrameRecord::Draw {
                window_id: 1_u8,
                start: start + Duration::from_millis(1),
                end: start + Duration::from_millis(5),
            },
        ];
        assert!(matches!(
            correlate(&events, 1_u8, rendered_at),
            Correlation::Rejected(_)
        ));
    }

    #[test]
    fn matching_draw_and_following_present_correlate() {
        let start = Instant::now();
        let rendered_at = start + Duration::from_millis(1);
        let events = [
            FrameRecord::Draw {
                window_id: 1_u8,
                start,
                end: start + Duration::from_millis(2),
            },
            FrameRecord::Present {
                window_id: 1_u8,
                end: start + Duration::from_millis(3),
            },
        ];
        assert!(matches!(
            correlate(&events, 1_u8, rendered_at),
            Correlation::Matched(_)
        ));
        let early_present = [
            events[0],
            FrameRecord::Present {
                window_id: 1_u8,
                end: start + Duration::from_micros(1500),
            },
        ];
        assert!(matches!(
            correlate(&early_present, 1_u8, rendered_at),
            Correlation::Rejected(_)
        ));
    }

    #[test]
    fn stale_render_timestamp_is_rejected() {
        let start = Instant::now();
        let events = [
            FrameRecord::Draw {
                window_id: 1_u8,
                start,
                end: start + Duration::from_millis(1),
            },
            FrameRecord::Present {
                window_id: 1_u8,
                end: start + Duration::from_millis(2),
            },
        ];
        assert!(matches!(
            correlate(&events, 1_u8, start + Duration::from_millis(3)),
            Correlation::Rejected(_)
        ));
    }

    #[test]
    fn missing_present_before_next_draw_is_rejected() {
        let start = Instant::now();
        let events = [
            FrameRecord::Draw {
                window_id: 1_u8,
                start,
                end: start + Duration::from_millis(1),
            },
            FrameRecord::Draw {
                window_id: 1_u8,
                start: start + Duration::from_millis(2),
                end: start + Duration::from_millis(3),
            },
            FrameRecord::Present {
                window_id: 1_u8,
                end: start + Duration::from_millis(4),
            },
        ];
        assert!(matches!(
            correlate(&events, 1_u8, start),
            Correlation::Rejected(_)
        ));
    }

    #[test]
    fn missing_receipt_is_rejected() {
        let at = Instant::now();
        let snapshot = InputMeasurementSnapshot {
            receipt: None,
            render: Some(render(1, at)),
        };
        assert!(validate_snapshot(1, &snapshot).is_err());
    }
}
