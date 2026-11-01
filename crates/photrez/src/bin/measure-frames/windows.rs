//! measure-frames: the product-seam frame-pacing bench for the native canvas.
//!
//! One process = one run. Prints one JSON row to stdout and exits 0 only when
//! the display budgets that apply to this seam pass. The protocol is carried
//! from the earlier standalone toolkit measurement: cold/warm, warm-up cycles,
//! presented-frame intervals, GDI paint-prove, fault-injection check, JSON row.
//!
//! The document is produced by `photrez-core` and painted only through
//! `render::canvas_bridge`. The display is a 512 px base-tile grid plus a live
//! patch overlay: each dense move pushes only the dirty rectangle as a patch,
//! and every eight moves the patches fold into the base tiles. `upload_dirty_ms`
//! is the maximum update-to-present latency over 12 isolated patch uploads, one
//! per stroke (matching the earlier standalone toolkit protocol); `update_max_ms`
//! covers every
//! dense move and `commit_max_ms` covers the overlay commits. `upload_full_ms`
//! is recorded but is not a normal-run verdict: the earlier design review
//! ruled a per-frame full upload a design constraint. There is no pointer injection
//! either: the native shell has no production pointer path yet, so this
//! measures document-update-to-present, not pointer-to-brush.
//!
//! Wait split: dense/control/compose frames use the frame callback; the isolated
//! full/dirty samples use a bounded 2 ms poll, so the sample does not pay the
//! one-frame callback cost it is measuring. This is a measurement cost, not a
//! canvas-path difference.
//!
//! `--control` is a diagnostic: it repaints the same window without changing the
//! canvas, so the present intervals show the compositor/machine floor. It is not
//! a product gate; the row is marked diagnostic-only.
//!
//! # Why the module-level unsafe allowance
//!
//! `probe.rs` calls Win32 GDI and EnumWindows for the paint-prove, as the
//! earlier standalone measurement did. The product print adapter has its own
//! `cfg(windows)` boundary.

#![allow(unsafe_code)] // Win32 FFI in probe.rs: GDI capture + own-window lookup, the paint-prove

#[path = "presentation.rs"]
mod presentation;
#[path = "probe.rs"]
mod probe;
#[path = "runner.rs"]
mod runner;
#[path = "timing.rs"]
mod timing;

use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::base::v_flex;
use gpui_kit::profiler::set_trace_enabled;
use gpui_kit::{
    px, size, AnyWindowHandle, App, AppContext, Context, IntoElement, ParentElement, Render,
    RenderImage, Styled, TitlebarOptions, WeakEntity, Window, WindowBounds, WindowOptions,
};
use photrez::render::canvas_bridge::{canvas_element, CanvasImage};
use photrez_core::pixel::PixelLayer;

use crate::args::parse_args;
use crate::{Config, USAGE};

use self::timing::Pacing;
const STROKE: [u8; 4] = [220, 40, 40, 255];
/// Moves between patch-overlay commits. Bounds the overlay to eight images and
/// keeps each commit's retired-texture batch small.
const WINDOW_WIDTH: f32 = 1024.0;
const WINDOW_HEIGHT: f32 = 768.0;

pub(super) fn run() {
    let cfg = match parse_args() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("measure-frames: {error}");
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    };
    let (width, height, base) = match build_base(&cfg) {
        Ok(document) => document,
        Err(error) => fail(&cfg, &error),
    };

    gpui_kit::application().run(move |cx: &mut App| {
        gpui_kit::init(cx);
        let _ = set_trace_enabled(true);

        let initial = match clone_rgba(&base).and_then(|rgba| {
            CanvasImage::from_rgba_with_tile_size(width, height, rgba, cfg.tile_size)
                .map_err(|error| format!("{error:?}"))
        }) {
            Ok(image) => image,
            Err(error) => fail(
                &cfg,
                &format!("cannot build the initial canvas image: {error}"),
            ),
        };

        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("Photrez".into()),
                appears_transparent: true,
                ..Default::default()
            }),
            window_bounds: Some(WindowBounds::centered(
                size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT)),
                cx,
            )),
            is_resizable: false,
            ..Default::default()
        };

        let view = BenchView {
            image: initial,
            retired: Vec::new(),
        };
        let (handle, entity) =
            match gpui_kit::open_window(options, cx, |_window, cx| cx.new(|_| view)) {
                Ok(value) => value,
                Err(error) => fail(
                    &cfg,
                    &format!("cannot open the measurement window: {error:?}"),
                ),
            };
        let weak = entity.downgrade();
        let runner = Runner {
            cfg,
            handle,
            weak,
            base,
            width,
            height,
        };
        cx.spawn(async move |async_cx: &mut gpui_kit::AsyncApp| runner.run(async_cx).await)
            .detach();
    });
}

/// Build the document through `photrez-core`: the real demo document with
/// `--demo`, otherwise a square core layer at the requested edge.
fn build_base(cfg: &Config) -> Result<(u32, u32, Vec<u8>), String> {
    if cfg.demo {
        return Ok(photrez::services::document::blank_white_document());
    }
    let edge =
        u32::try_from(cfg.size).map_err(|_| format!("--size does not fit a u32: {}", cfg.size))?;
    let bytes = (edge as usize)
        .checked_mul(edge as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| format!("--size {edge} overflows the buffer length"))?;
    let mut pixels = Vec::new();
    pixels.try_reserve_exact(bytes).map_err(|error| {
        format!("cannot allocate {bytes} bytes for a {edge}x{edge} document: {error:?}")
    })?;
    pixels.resize(bytes, 255);
    let layer = PixelLayer::new(edge, edge, pixels);
    // The display seam consumes the layer's buffer; `snapshot_region` would
    // duplicate 64 MiB at 4096 without exercising the display path.
    Ok((edge, edge, layer.pixels))
}

fn clone_rgba(source: &[u8]) -> Result<Vec<u8>, String> {
    let mut copy = Vec::new();
    copy.try_reserve_exact(source.len())
        .map_err(|error| format!("cannot allocate {} bytes: {error:?}", source.len()))?;
    copy.extend_from_slice(source);
    Ok(copy)
}

/// Paint one filled disc into an RGBA buffer. Coordinates outside the buffer
/// are clipped; this never panics.
fn paint_disc(
    rgba: &mut [u8],
    width: u32,
    height: u32,
    center_x: i32,
    center_y: i32,
    radius: i32,
    color: [u8; 4],
) {
    let radius_squared = radius * radius;
    let x0 = (center_x - radius).max(0);
    let x1 = (center_x + radius).min(width as i32 - 1);
    let y0 = (center_y - radius).max(0);
    let y1 = (center_y + radius).min(height as i32 - 1);
    for y in y0..=y1 {
        for x in x0..=x1 {
            let dx = x - center_x;
            let dy = y - center_y;
            if dx * dx + dy * dy <= radius_squared {
                let offset = ((y as usize * width as usize) + x as usize) * 4;
                rgba[offset..offset + 4].copy_from_slice(&color);
            }
        }
    }
}

/// A diagonal stroke across the page, inset by the brush radius.
fn dot_position(index: usize, total: usize, width: u32, height: u32, radius: i32) -> (i32, i32) {
    let margin = (radius + 2).max(8);
    let usable_width = (width as i32 - margin * 2).max(1);
    let usable_height = (height as i32 - margin * 2).max(1);
    let denominator = total.max(1) as i64;
    let x = margin as i64 + (index as i64 * usable_width as i64 / denominator);
    let y = margin as i64 + (index as i64 * usable_height as i64 / denominator);
    (x as i32, y as i32)
}

struct BenchView {
    image: CanvasImage,
    /// Textures retired by an update, evicted from the sprite atlas at the
    /// start of the next render. The atlas keys every `RenderImage` by id, so
    /// keeping one texture per update would grow without bound.
    retired: Vec<Arc<RenderImage>>,
}

impl Render for BenchView {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        for retired in self.retired.drain(..) {
            let _ = window.drop_image(retired);
        }
        v_flex().size_full().child(canvas_element(&self.image))
    }
}

/// Which clock the present wait uses. Dense/control frames follow the frame
/// callback; isolated dirty/full-upload samples poll so the sample does not
/// pay the wait's one-frame cost.
#[derive(Clone, Copy)]
enum WaitMode {
    Frame,
    Sample,
}

struct Runner {
    cfg: Config,
    handle: AnyWindowHandle,
    weak: WeakEntity<BenchView>,
    base: Vec<u8>,
    width: u32,
    height: u32,
}

fn since(pace: &Pacing, after: Instant) -> Duration {
    pace.presents
        .iter()
        .filter(|&&at| at > after)
        .map(|&at| at.saturating_duration_since(after))
        .max()
        .unwrap_or(Duration::ZERO)
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn round6(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

fn machine_line() -> String {
    let cpus = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(0);
    format!("windows-{} x {} cpus", std::env::consts::ARCH, cpus)
}

fn emit(cfg: &Config, row: serde_json::Value, code: i32) -> ! {
    match serde_json::to_string(&row) {
        Ok(json) => {
            println!("{json}");
            if let Some(path) = &cfg.out_path {
                match std::fs::write(path.as_str(), format!("{json}\n")) {
                    Ok(()) => {}
                    Err(error) => eprintln!("measure-frames: cannot write {path}: {error}"),
                }
            }
        }
        Err(error) => eprintln!("measure-frames: cannot encode the result: {error}"),
    }
    std::process::exit(code);
}

fn fail(cfg: &Config, message: &str) -> ! {
    let row = serde_json::json!({
        "shot": "measure-frames",
        "harness": "canvas_bridge",
        "mode": cfg.mode.as_str(),
        "size": cfg.size,
        "error": message,
        "verdict_ok": false,
        "verdict": [message],
    });
    emit(cfg, row, 2);
}
