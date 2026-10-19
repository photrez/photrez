//! Raw RGB delta comparison - the sensitive-but-noisy number (`diff.ts`
//! semantics). Never a pass/fail on its own: it counts rasteriser
//! antialiasing as failure, which is why the class-map verdict exists.
//! Also writes the side-by-side panels (original | port | delta heatmap).

use std::path::Path;

use super::pngio::RgbaImage;
use super::{round3, Anchor, RawRegionReport, Rect, RegionSpec, WorstRow};

/// Per-channel delta above which a pixel counts as "different" (TS constant).
pub const THRESHOLD: u8 = 12;

/// Gap between columns in the side-by-side panel (TS constant).
pub(crate) const GAP: u32 = 4;

/// `diff.ts` semantics: per-region anchored bands over two full images.
///
/// Requires both images to have equal widths and enough rows for every region.
pub fn compare_anchored(
    baseline: &RgbaImage,
    port: &RgbaImage,
    regions: &[RegionSpec],
) -> Result<Vec<RawRegionReport>, String> {
    if baseline.width != port.width {
        return Err(format!(
            "compare_anchored: width mismatch: baseline {} vs port {}",
            baseline.width, port.width
        ));
    }
    let width = baseline.width;

    let mut reports = Vec::with_capacity(regions.len());
    for spec in regions {
        let count = spec.rows;
        if count > baseline.height || count > port.height {
            return Err(format!(
                "compare_anchored: region '{}' needs {count} rows, but baseline has {} and port has {}",
                spec.name, baseline.height, port.height
            ));
        }

        let ya = anchor_start(baseline.height, spec.anchor, count);
        let yb = anchor_start(port.height, spec.anchor, count);

        let mut diff_count: u64 = 0;
        let mut total: u64 = 0;
        let mut max_delta: u8 = 0;
        let mut max_at: [u32; 2] = [0, 0];
        let mut worst_row = WorstRow { row: 0, pct: 0.0 };

        for dy in 0..count {
            let y_a = ya + dy;
            let y_b = yb + dy;
            let mut row_diff: u64 = 0;

            for x in 0..width {
                let ia = ((y_a as usize) * (width as usize) + x as usize) * 4;
                let ib = ((y_b as usize) * (width as usize) + x as usize) * 4;

                let mut worst: u8 = 0;
                for c in 0..3 {
                    let d = baseline.data[ia + c].abs_diff(port.data[ib + c]);
                    if d > worst {
                        worst = d;
                    }
                }

                total += 1;
                if worst > max_delta {
                    max_delta = worst;
                    max_at = [x, y_a];
                }
                if worst > THRESHOLD {
                    diff_count += 1;
                    row_diff += 1;
                }
            }

            let row_pct = (row_diff as f64 / width as f64) * 100.0;
            if row_pct > worst_row.pct {
                worst_row = WorstRow {
                    row: dy,
                    pct: row_pct,
                };
            }
        }

        reports.push(RawRegionReport {
            name: spec.name.clone(),
            anchored_to: spec.anchor.as_str().to_string(),
            rows_compared: count,
            original_y_range: [ya, ya + count],
            port_y_range: [yb, yb + count],
            diff_pixels: diff_count,
            diff_pct: round3((diff_count as f64 / total as f64) * 100.0),
            worst_row,
            max_channel_delta: max_delta,
            max_at,
        });
    }
    Ok(reports)
}

/// `shell-diff.ts` semantics: baseline crop vs a rect in the full capture.
///
/// The rect's size must match the crop's dimensions and must lie within the
/// port image. Returns `Err` otherwise.
pub fn compare_crop_in(
    baseline_crop: &RgbaImage,
    port_full: &RgbaImage,
    rect: Rect,
) -> Result<RawRegionReport, String> {
    let w = baseline_crop.width;
    let h = baseline_crop.height;

    if rect.width != w || rect.height != h {
        return Err(format!(
            "compare_crop_in: crop is {w}x{h} but rect is {}x{}",
            rect.width, rect.height
        ));
    }

    if rect.x + w > port_full.width || rect.y + h > port_full.height {
        return Err(format!(
            "compare_crop_in: rect ({},{}) {}x{} exceeds port image {}x{}",
            rect.x, rect.y, w, h, port_full.width, port_full.height
        ));
    }

    let mut diff: u64 = 0;
    let mut max_delta: u8 = 0;
    let mut max_at: [u32; 2] = [0, 0];
    let mut worst_row = WorstRow { row: 0, pct: 0.0 };

    for dy in 0..h {
        let mut row_diff: u64 = 0;
        for dx in 0..w {
            let pa = ((dy as usize) * (w as usize) + dx as usize) * 4;
            let pb = (((rect.y + dy) as usize) * (port_full.width as usize)
                + (rect.x + dx) as usize)
                * 4;

            let mut worst: u8 = 0;
            for c in 0..3 {
                let d = baseline_crop.data[pa + c].abs_diff(port_full.data[pb + c]);
                if d > worst {
                    worst = d;
                }
            }

            if worst > max_delta {
                max_delta = worst;
                max_at = [rect.x + dx, rect.y + dy];
            }
            if worst > THRESHOLD {
                diff += 1;
                row_diff += 1;
            }
        }

        let row_pct = (row_diff as f64 / w as f64) * 100.0;
        if row_pct > worst_row.pct {
            worst_row = WorstRow {
                row: rect.y + dy,
                pct: row_pct,
            };
        }
    }

    let total = (w * h) as u64;

    Ok(RawRegionReport {
        name: "crop".to_string(),
        anchored_to: "absolute".to_string(),
        rows_compared: h,
        original_y_range: [0, h],
        port_y_range: [rect.y, rect.y + h],
        diff_pixels: diff,
        diff_pct: round3((diff as f64 / total as f64) * 100.0),
        worst_row,
        max_channel_delta: max_delta,
        max_at,
    })
}

/// Side-by-side panel: `original | port | heatmap`, rows aligned.
///
/// `start_a` / `start_b` select the first row compared in each image (the TS
/// anchors rows per region); the heatmap marks every pixel whose worst channel
/// delta exceeds [`THRESHOLD`] with `min(255, worst*3)` red on black.
pub fn write_sbs_panel(
    original: &RgbaImage,
    start_a: u32,
    port: &RgbaImage,
    start_b: u32,
    rows: u32,
    out: &Path,
) -> Result<(), String> {
    if original.width != port.width {
        return Err(format!(
            "write_sbs_panel: width mismatch: original {} vs port {}",
            original.width, port.width
        ));
    }
    if start_a + rows > original.height || start_b + rows > port.height {
        return Err(format!(
            "write_sbs_panel: {rows} rows starting at {start_a}/{start_b} exceed image heights {}/{}",
            original.height, port.height
        ));
    }

    let w = original.width;
    let out_w = w * 3 + GAP * 2;
    let mut data = vec![0u8; (out_w * rows) as usize * 4];

    for dy in 0..rows {
        for x in 0..w as usize {
            let pa = (((start_a + dy) as usize) * (w as usize) + x) * 4;
            let pb = (((start_b + dy) as usize) * (w as usize) + x) * 4;

            // Column 0: original pixel (RGBA copy)
            let dest_a = ((dy as usize) * (out_w as usize) + x) * 4;
            data[dest_a..dest_a + 4].copy_from_slice(&original.data[pa..pa + 4]);

            // Column 1: port pixel (RGBA copy)
            let dest_b = ((dy as usize) * (out_w as usize) + (w + GAP) as usize + x) * 4;
            data[dest_b..dest_b + 4].copy_from_slice(&port.data[pb..pb + 4]);

            // Column 2: heatmap - red = min(255, worst*3) if worst > THRESHOLD,
            // otherwise black. Alpha always 255.
            let mut worst: u8 = 0;
            for c in 0..3 {
                let d = original.data[pa + c].abs_diff(port.data[pb + c]);
                if d > worst {
                    worst = d;
                }
            }
            let dest_h = ((dy as usize) * (out_w as usize) + 2 * (w + GAP) as usize + x) * 4;
            let r = if worst > THRESHOLD {
                worst.saturating_mul(3)
            } else {
                0
            };
            data[dest_h] = r;
            data[dest_h + 1] = 0;
            data[dest_h + 2] = 0;
            data[dest_h + 3] = 255;
        }
    }

    let img = RgbaImage::from_rgba(out_w, rows, data)?;
    img.save(out)
}

/// Compute the start row for an edge-anchored region.
fn anchor_start(image_height: u32, anchor: Anchor, count: u32) -> u32 {
    match anchor {
        Anchor::Top => 0,
        Anchor::Bottom => image_height - count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// (c) rawdiff counts a +13 per-channel delta but not +12.
    #[test]
    fn threshold_boundary_13_vs_12() {
        // Two pixels: pixel 0 has R delta 13 (above threshold -> counted),
        // pixel 1 has R delta 12 (at threshold, `>` is strict -> not counted).
        let w = 2u32;
        let h = 1u32;
        let baseline = RgbaImage::from_rgba(w, h, vec![0, 0, 0, 255, 0, 0, 0, 255]).unwrap();
        let port = RgbaImage::from_rgba(w, h, vec![13, 0, 0, 255, 12, 0, 0, 255]).unwrap();

        let rect = Rect {
            x: 0,
            y: 0,
            width: w,
            height: h,
        };
        let report = compare_crop_in(&baseline, &port, rect).unwrap();
        assert_eq!(
            report.diff_pixels, 1,
            "only the +13 pixel should be counted"
        );
        assert_eq!(report.diff_pct, 50.0, "1 of 2 pixels = 50.000%");
        assert_eq!(report.max_channel_delta, 13, "max delta is 13");
        assert_eq!(report.max_at, [0, 0], "max delta at pixel (0,0)");
        assert_eq!(report.worst_row.row, 0);
        assert_eq!(report.worst_row.pct, 50.0, "row 0 has 1 diff of 2 px = 50%");
    }

    /// `write_sbs_panel` lays out 3 columns with a GAP and a red heatmap.
    #[test]
    fn sbs_panel_layout_three_columns() {
        // One pixel each, delta 20 in R (> threshold).
        let orig = RgbaImage::from_rgba(1, 1, vec![0, 0, 0, 255]).unwrap();
        let port = RgbaImage::from_rgba(1, 1, vec![20, 0, 0, 255]).unwrap();

        let dir = std::env::temp_dir();
        let path = dir.join("sbs_panel_test.png");
        write_sbs_panel(&orig, 0, &port, 0, 1, &path).unwrap();

        let out = RgbaImage::load(&path).unwrap();
        // width = 1*3 + 4*2 = 11, height = 1
        assert_eq!(out.width, 11);
        assert_eq!(out.height, 1);

        // Column 0 (original): black pixel
        let p0 = out.pixel(0, 0);
        assert_eq!(p0, [0, 0, 0, 255]);

        // Column 1 (port): dark red pixel (delta 20)
        let p1 = out.pixel(1 + GAP, 0);
        assert_eq!(p1, [20, 0, 0, 255]);

        // Column 2 (heatmap): red = min(255, 20*3) = 60
        let p2 = out.pixel(2 + 2 * GAP, 0);
        assert_eq!(p2, [60, 0, 0, 255]);
    }

    /// Threshold is strict `>`: a delta of exactly 12 does not count.
    #[test]
    fn threshold_is_strict_gt() {
        let w = 1u32;
        let h = 1u32;
        let baseline = RgbaImage::from_rgba(w, h, vec![0, 0, 0, 255]).unwrap();
        let port = RgbaImage::from_rgba(w, h, vec![12, 0, 0, 255]).unwrap();

        let rect = Rect {
            x: 0,
            y: 0,
            width: w,
            height: h,
        };
        let report = compare_crop_in(&baseline, &port, rect).unwrap();
        assert_eq!(
            report.diff_pixels, 0,
            "delta of 12 (at threshold) should not count"
        );
        assert_eq!(report.diff_pct, 0.0);
    }
}
