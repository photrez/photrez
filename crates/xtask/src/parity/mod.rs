//! Rust port of the frozen TS parity tooling.
//!
//! The originals (read-only, never executed) are the five scripts behind the
//! appearance comparison: `shape-diff.ts` (class map + 3x3 tolerance ->
//! `HARD MISMATCH`), `diff.ts` (raw RGB delta + side-by-side panel),
//! `zoom.ts` (ink integral + peak luma), `measure.ts` (bbox / cross-sections),
//! `shell-diff.ts` (region selection + report). Their recorded outputs are the
//! cross-check oracle; once this port is verified the Rust code is the only
//! living implementation of the tolerance policy.
//!
//! Reporting conventions preserved from the TS originals: three-decimal
//! percentages, camelCase JSON keys, per-region `anchoredTo`, hard spots
//! capped at 40 per region.

pub mod classify;
pub mod crosscheck;
pub mod manifest;
pub mod measure;
pub mod pngio;
pub mod rawdiff;
pub mod run;
pub mod shapediff;
pub mod zoom;

use serde::Serialize;

/// Three-decimal rounding, matching the TS `toFixed(3)` report values.
pub(crate) fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

/// A pixel rectangle. Only ever produced from non-negative layout coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Which edge a region is measured from. The two shells differ in height at
/// the bottom (`mt-auto` absorbs it), so rail-style comparisons must anchor
/// each block to the edge it actually sits on (`diff.ts` header).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    Top,
    Bottom,
}

impl Anchor {
    pub fn as_str(self) -> &'static str {
        match self {
            Anchor::Top => "top",
            Anchor::Bottom => "bottom",
        }
    }
}

/// A band of rows on an image, named and edge-anchored.
#[derive(Clone, Debug)]
pub struct RegionSpec {
    pub name: String,
    pub anchor: Anchor,
    pub rows: u32,
}

/// One pixel where the two class maps disagree beyond the fringe.
#[derive(Clone, Debug, Serialize)]
pub struct HardSpot {
    pub x: u32,
    pub y: u32,
    pub original: String,
    pub port: String,
}

/// Per-class pixel counts, original vs port.
#[derive(Clone, Debug, Serialize)]
pub struct ClassHistogramRow {
    pub class: String,
    pub original: u64,
    pub port: u64,
}

/// One region's class-map verdict (port of `shape-diff.ts` region output).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShapeRegionReport {
    pub name: String,
    pub anchored_to: String,
    pub rows: u32,
    pub agree_pct: f64,
    pub fringe_only_pct: f64,
    pub hard_geometry_mismatch_pct: f64,
    pub hard_spots: Vec<HardSpot>,
    pub histogram: Vec<ClassHistogramRow>,
}

/// The worst row in a raw-diff region (layout-shift detector).
#[derive(Clone, Debug, Serialize)]
pub struct WorstRow {
    pub row: u32,
    pub pct: f64,
}

/// One region's raw-RGB verdict (port of `diff.ts` region output).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RawRegionReport {
    pub name: String,
    pub anchored_to: String,
    pub rows_compared: u32,
    pub original_y_range: [u32; 2],
    pub port_y_range: [u32; 2],
    pub diff_pixels: u64,
    pub diff_pct: f64,
    pub worst_row: WorstRow,
    pub max_channel_delta: u8,
    pub max_at: [u32; 2],
}

/// One region's ink verdict (port of `zoom.ts` numbers; the zoomed panels are
/// a separate, optional artifact).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InkRegionReport {
    pub name: String,
    pub anchor: String,
    pub ink_original: f64,
    pub ink_port: f64,
    pub ratio: f64,
    pub peak_original: f64,
    pub peak_port: f64,
}
