#![recursion_limit = "256"]
// The harness parses its arguments, formats a summary and keeps a result
// envelope in shared modules that are compiled for the unit tests on every
// platform; only the Windows driver calls them, so off Windows those call sites
// are absent by design rather than by neglect.
#![cfg_attr(not(windows), allow(dead_code))]

#[cfg(any(windows, test))]
const USAGE: &str = "\
measure-frames - product-seam frame pacing for the native canvas

Usage: measure-frames [options]
  --size N        square document edge (default 4096; 1..=16384)
  --width N       document width, overriding one --size edge (1..=16384)
  --height N      document height, overriding one --size edge (1..=16384)
  --layers N      raster layers in the document (default 1; 1..=4)
  --content MODE  plain (default) or canonical: distinct patterned layers
                  composited by photrez-core. canonical is implied by --layers > 1
  --fixture-png PATH  write the composited fixture PNG to PATH before measuring
  --demo          use the real 800x600 demo document (ignores size and layers)
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

#[cfg(any(windows, test))]
const PATCH_COMMIT_MOVES: usize = 8;

#[cfg(any(windows, test))]
struct Config {
    size: usize,
    width: usize,
    height: usize,
    layers: usize,
    content: String,
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
    fixture_png: Option<String>,
    out_path: Option<String>,
}

#[cfg(any(windows, test))]
mod args;
#[cfg(any(windows, test))]
mod presentmon;

#[cfg(any(windows, test))]
mod support;

#[cfg(windows)]
mod windows;

#[cfg(windows)]
fn main() {
    windows::run();
}

#[cfg(not(windows))]
fn main() {
    eprintln!("measure-frames is only supported on Windows");
    std::process::exit(2);
}
