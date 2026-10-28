#![recursion_limit = "256"]

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
//! # Why the crate-level unsafe allowance
//!
//! `probe.rs` calls Win32 GDI and EnumWindows for the paint-prove, as the
//! earlier standalone measurement did. No product module calls Win32 directly.

#![allow(unsafe_code)] // Win32 FFI in probe.rs: GDI capture + own-window lookup, the paint-prove

mod args;
mod presentation;
mod presentmon;
mod probe;
mod runner;
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

use crate::timing::Pacing;
use args::parse_args;

const USAGE: &str = "\
measure-frames - product-seam frame pacing for the native canvas

Usage: measure-frames [options]
  --size N        square document edge (default 4096; 1..=16384)
  --demo          use the real 800x600 demo document (ignores --size)
  --strokes N     stroke cycles (default 12)
  --moves N       document updates per stroke (default 40)
  --brush N       brush radius in document px (default 32; 1..=256)
  --warmup N      warm-up updates (default 3 warm, 0 cold)
  --mode MODE     cold or warm (default warm)
  --tile-size N   base tile edge (default 512; default-off experiment)
  --commit-moves N  dense moves per overlay commit (default 8; experiment)
  --patch-union   union dirty rects per commit (default off)
  --progressive   paint the first quarter tile set first (default off)
  --control       idle-repaint control (diagnostic; no canvas updates)
  --red upload    inject a 40 ms delay before the full upload (must fail)
  --red dirty     inject a 40 ms delay before the first dirty upload (must fail)
  --presentmon-csv PATH  summarize a PresentMon CSV as the displayed interval
  --presentmon-skip-ms N  skip CSV rows before N ms (opening/full upload)
  --out PATH      also write the JSON row to PATH
  --help          show this help

Exit 0 only when the applicable display budgets pass; 1 on a failed target; 2 on
a setup error.";

const STROKE: [u8; 4] = [220, 40, 40, 255];
/// Moves between patch-overlay commits. Bounds the overlay to eight images and
/// keeps each commit's retired-texture batch small.
const PATCH_COMMIT_MOVES: usize = 8;
const WINDOW_WIDTH: f32 = 1024.0;
const WINDOW_HEIGHT: f32 = 768.0;

struct Config {
    size: usize,
    strokes: usize,
    moves: usize,
    brush_px: f32,
    warmup: usize,
    mode: String,
    red: String,
    demo: bool,
    control: bool,
    tile_size: u32,
    commit_moves: usize,
    patch_union: bool,
    progressive: bool,
    presentmon_csv: Option<String>,
    presentmon_skip_ms: f64,
    out_path: Option<String>,
}

fn main() {
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

/// Clipped bounding box of one disc, or `None` when the disc misses the page.
/// The paint loop and the dirty upload use the same bounds.
fn disc_bounds(
    center_x: i32,
    center_y: i32,
    radius: i32,
    width: u32,
    height: u32,
) -> Option<(u32, u32, u32, u32)> {
    if radius < 0 || width == 0 || height == 0 {
        return None;
    }
    let x0 = (center_x - radius).max(0);
    let x1 = (center_x + radius).min(width as i32 - 1);
    let y0 = (center_y - radius).max(0);
    let y1 = (center_y + radius).min(height as i32 - 1);
    if x0 > x1 || y0 > y1 {
        return None;
    }
    Some((
        x0 as u32,
        y0 as u32,
        (x1 - x0 + 1) as u32,
        (y1 - y0 + 1) as u32,
    ))
}

/// Copies one row-major RGBA sub-rectangle out of a wider document buffer.
/// The caller guarantees the rectangle is inside the source.
fn copy_region(
    source: &[u8],
    width: u32,
    x: u32,
    y: u32,
    region_width: u32,
    region_height: u32,
) -> Vec<u8> {
    let row_len = region_width as usize * 4;
    let mut out = Vec::with_capacity(row_len * region_height as usize);
    for row in y..y + region_height {
        let start = ((row as usize * width as usize) + x as usize) * 4;
        out.extend_from_slice(&source[start..start + row_len]);
    }
    out
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disc_bounds_clips_to_the_document_and_rejects_empty() {
        assert_eq!(disc_bounds(5, 5, 2, 10, 10), Some((3, 3, 5, 5)));
        assert_eq!(disc_bounds(0, 0, 2, 10, 10), Some((0, 0, 3, 3)));
        assert_eq!(disc_bounds(9, 9, 2, 10, 10), Some((7, 7, 3, 3)));
        assert_eq!(disc_bounds(10, 5, 2, 10, 10), Some((8, 3, 2, 5)));
        assert_eq!(disc_bounds(-5, 5, 2, 10, 10), None);
        assert_eq!(disc_bounds(5, 5, 0, 10, 10), Some((5, 5, 1, 1)));
        assert_eq!(disc_bounds(5, 5, 2, 0, 10), None);
    }

    #[test]
    fn copy_region_copies_rows_with_the_requested_stride() {
        let source: Vec<u8> = (0..(3 * 2 * 4) as u8).collect();
        let copied = copy_region(&source, 3, 1, 0, 2, 2);
        assert_eq!(copied.len(), 2 * 2 * 4);
        assert_eq!(&copied[0..8], &source[4..12]);
        assert_eq!(&copied[8..16], &source[16..24]);
    }
}
