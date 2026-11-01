#![recursion_limit = "256"]

#[cfg(any(windows, test))]
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

#[cfg(any(windows, test))]
const PATCH_COMMIT_MOVES: usize = 8;

#[cfg(any(windows, test))]
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

#[cfg(any(windows, test))]
#[path = "args.rs"]
mod args;

#[cfg(any(windows, test))]
#[path = "presentmon.rs"]
mod presentmon;

#[cfg(any(windows, test))]
#[path = "common.rs"]
mod common;

#[cfg(windows)]
#[path = "windows.rs"]
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
