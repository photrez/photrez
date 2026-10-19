//! Class-map region comparison with the 3x3 neighbourhood tolerance - the
//! `HARD MISMATCH` gate.
//!
//! Two entry points because the frozen tools have two shapes:
//! `shape-diff.ts` compares two images region-by-region with each region
//! anchored to its own edge; `shell-diff.ts` compares a baseline crop against
//! a rectangle inside a full client capture - the rectangle takes its origin
//! from the manifest but its size from the baseline PNG itself (fractional
//! layout widths round differently on the way into a file).

use super::classify::{classify, Klass, CLASS_ORDER};
use super::pngio::RgbaImage;
use super::{round3, Anchor, ClassHistogramRow, HardSpot, Rect, RegionSpec, ShapeRegionReport};

/// Hard spots recorded per region before truncation (TS `hardSpots.length < 40`).
pub(crate) const MAX_HARD_SPOTS: usize = 40;

/// Maximum fringe radius: the 3x3 neighbourhood (dy,dx in [-1,1]) matches the
/// TS `matchesNeighbour` exactly.
const FRINGE_RADIUS: i32 = 1;

/// Classify a sub-rectangle of `img` into a row-major class map.
/// The rectangle must lie within the image bounds.
pub(crate) fn class_map(img: &RgbaImage, rect: Rect) -> Result<Vec<Vec<Klass>>, String> {
    if !fits(img.width, img.height, rect) {
        return Err(format!(
            "class_map: rect ({},{}) {}x{} exceeds image {}x{}",
            rect.x, rect.y, rect.width, rect.height, img.width, img.height
        ));
    }
    let mut map = Vec::with_capacity(rect.height as usize);
    for dy in 0..rect.height {
        let abs_y = rect.y + dy;
        let row_start = ((abs_y as usize) * (img.width as usize) + rect.x as usize) * 4;
        let mut row = Vec::with_capacity(rect.width as usize);
        for dx in 0..rect.width {
            let i = row_start + (dx as usize) * 4;
            row.push(classify(img.data[i], img.data[i + 1], img.data[i + 2]));
        }
        map.push(row);
    }
    Ok(map)
}

/// True when `rect` lies inside an image of `w`x`h` pixels.
fn fits(w: u32, h: u32, rect: Rect) -> bool {
    rect.x <= w && rect.y <= h && rect.x + rect.width <= w && rect.y + rect.height <= h
}

/// True if `target` class appears anywhere in the 3x3 neighbourhood of
/// (x, y) in `map`. Mirrors the TS `matchesNeighbour`: iterates dy,dx in
/// [-FRINGE_RADIUS,FRINGE_RADIUS] and skips out-of-bounds coordinates.
fn matches_neighbour(target: Klass, x: u32, y: u32, map: &[Vec<Klass>]) -> bool {
    let rows = map.len() as i32;
    if rows == 0 {
        return false;
    }
    let width = map[0].len() as i32;
    for dy in -FRINGE_RADIUS..=FRINGE_RADIUS {
        for dx in -FRINGE_RADIUS..=FRINGE_RADIUS {
            let yy = y as i32 + dy;
            let xx = x as i32 + dx;
            if yy < 0 || yy >= rows || xx < 0 || xx >= width {
                continue;
            }
            if map[yy as usize][xx as usize] == target {
                return true;
            }
        }
    }
    false
}

/// Compare two equally-shaped class maps under the 3x3 neighbourhood rule.
///
/// Returns `(fringe_count, hard_count, hard_spots)` where `hard_spots` carry
/// **relative** coordinates (the caller adds the appropriate offset to make
/// them absolute).
///
/// A class disagreement counts as `fringe` when the other map carries that
/// class anywhere in the 3x3 neighbourhood; otherwise it is `hard` and, past
/// [`MAX_HARD_SPOTS`], still counted but no longer recorded.
pub(crate) fn compare_maps(a: &[Vec<Klass>], b: &[Vec<Klass>]) -> (u64, u64, Vec<HardSpot>) {
    let count = a.len();
    if count == 0 {
        return (0, 0, Vec::new());
    }
    let width = a[0].len();

    let mut fringe: u64 = 0;
    let mut hard: u64 = 0;
    let mut hard_spots: Vec<HardSpot> = Vec::new();

    for y in 0..count {
        for x in 0..width {
            let ca = a[y][x];
            let cb = b[y][x];
            if ca == cb {
                continue;
            }
            if matches_neighbour(ca, x as u32, y as u32, b)
                || matches_neighbour(cb, x as u32, y as u32, a)
            {
                fringe += 1;
            } else {
                hard += 1;
                if hard_spots.len() < MAX_HARD_SPOTS {
                    hard_spots.push(HardSpot {
                        x: x as u32,
                        y: y as u32,
                        original: ca.as_str().to_string(),
                        port: cb.as_str().to_string(),
                    });
                }
            }
        }
    }

    (fringe, hard, hard_spots)
}

/// Histogram over both maps, in [`CLASS_ORDER`].
pub(crate) fn histogram(a: &[Vec<Klass>], b: &[Vec<Klass>]) -> Vec<ClassHistogramRow> {
    let mut orig = [0u64; 5];
    let mut port = [0u64; 5];
    for row in a.iter() {
        for &k in row {
            orig[klass_idx(k)] += 1;
        }
    }
    for row in b.iter() {
        for &k in row {
            port[klass_idx(k)] += 1;
        }
    }
    CLASS_ORDER
        .iter()
        .enumerate()
        .map(|(i, c)| ClassHistogramRow {
            class: c.as_str().to_string(),
            original: orig[i],
            port: port[i],
        })
        .collect()
}

/// Map each [`Klass`] variant to its position in [`CLASS_ORDER`].
fn klass_idx(k: Klass) -> usize {
    match k {
        Klass::Bg => 0,
        Klass::Active => 1,
        Klass::Stroke => 2,
        Klass::Orange => 3,
        Klass::White => 4,
    }
}

/// `shape-diff.ts` semantics: two images, per-region anchoring.
///
/// Both images must have equal widths and enough rows for every region;
/// violation panics with a descriptive message (the cross-check harness
/// wraps this in `catch_unwind`).
pub fn compare_anchored(
    baseline: &RgbaImage,
    port: &RgbaImage,
    regions: &[RegionSpec],
) -> Vec<ShapeRegionReport> {
    debug_assert_eq!(
        baseline.width, port.width,
        "compare_anchored: width mismatch: {} vs {}",
        baseline.width, port.width
    );
    let width = baseline.width;

    let mut reports = Vec::with_capacity(regions.len());
    for spec in regions {
        let count = spec.rows;
        debug_assert!(
            count <= baseline.height && count <= port.height,
            "compare_anchored: region '{}' needs {count} rows, but baseline has {} and port has {}",
            spec.name,
            baseline.height,
            port.height
        );

        let ya = anchor_start(baseline.height, spec.anchor, count);
        let yb = anchor_start(port.height, spec.anchor, count);

        let ca = class_map(
            baseline,
            Rect {
                x: 0,
                y: ya,
                width,
                height: count,
            },
        )
        .expect("rect pre-validated above");
        let cb = class_map(
            port,
            Rect {
                x: 0,
                y: yb,
                width,
                height: count,
            },
        )
        .expect("rect pre-validated above");

        let (fringe, hard, mut spots) = compare_maps(&ca, &cb);
        // shape-diff.ts: x is relative to region (always starts at 0), y is
        // anchored to the baseline image's start row.
        for spot in &mut spots {
            spot.y += ya;
        }

        let total = (count * width) as u64;
        let hist = histogram(&ca, &cb);

        reports.push(assemble(
            spec.name.clone(),
            spec.anchor.as_str().to_string(),
            count,
            total,
            fringe,
            hard,
            spots,
            hist,
        ));
    }
    reports
}

/// `shell-diff.ts` semantics: one baseline crop vs a rect in the full capture.
///
/// The rect's size must match the crop's dimensions; the rect must lie within
/// the port image. Returns `Err` otherwise.
pub fn compare_crop_in(
    baseline_crop: &RgbaImage,
    port_full: &RgbaImage,
    rect: Rect,
) -> Result<ShapeRegionReport, String> {
    let w = baseline_crop.width;
    let h = baseline_crop.height;

    if rect.width != w || rect.height != h {
        return Err(format!(
            "compare_crop_in: crop is {w}x{h} but rect is {}x{}",
            rect.width, rect.height
        ));
    }

    if !fits(port_full.width, port_full.height, rect) {
        return Err(format!(
            "compare_crop_in: rect ({},{}) {}x{} exceeds port image {}x{}",
            rect.x, rect.y, w, h, port_full.width, port_full.height
        ));
    }

    let ca = class_map(
        baseline_crop,
        Rect {
            x: 0,
            y: 0,
            width: w,
            height: h,
        },
    )?;
    let cb = class_map(port_full, rect)?;

    let (fringe, hard, mut spots) = compare_maps(&ca, &cb);
    // shell-diff.ts: hard spots are absolute in the port coordinate system.
    for spot in &mut spots {
        spot.x += rect.x;
        spot.y += rect.y;
    }

    let total = (w * h) as u64;
    let hist = histogram(&ca, &cb);

    Ok(assemble(
        "crop".to_string(),
        "absolute".to_string(),
        h,
        total,
        fringe,
        hard,
        spots,
        hist,
    ))
}

/// Compute the start row for an edge-anchored region.
fn anchor_start(image_height: u32, anchor: Anchor, count: u32) -> u32 {
    match anchor {
        Anchor::Top => 0,
        Anchor::Bottom => image_height - count,
    }
}

/// Rounding + assembly shared by both entry points; kept private.
#[expect(
    clippy::too_many_arguments,
    reason = "the parameters are the report's fields in order; bundling them in a struct would duplicate ShapeRegionReport"
)]
fn assemble(
    name: String,
    anchored_to: String,
    rows: u32,
    total: u64,
    fringe: u64,
    hard: u64,
    hard_spots: Vec<HardSpot>,
    histogram: Vec<ClassHistogramRow>,
) -> ShapeRegionReport {
    let total_f = total as f64;
    ShapeRegionReport {
        name,
        anchored_to,
        rows,
        agree_pct: round3(((total - hard - fringe) as f64 / total_f) * 100.0),
        fringe_only_pct: round3((fringe as f64 / total_f) * 100.0),
        hard_geometry_mismatch_pct: round3((hard as f64 / total_f) * 100.0),
        hard_spots,
        histogram,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // (a) A lone differing pixel whose class exists in the neighbour's 3x3
    // is fringe, not hard.
    #[test]
    fn lone_differing_pixel_is_fringe() {
        // A: all bg (10x10)
        let a: Vec<Vec<Klass>> = vec![vec![Klass::Bg; 10]; 10];
        // B: all bg except one stroke pixel at (5,5)
        let mut b: Vec<Vec<Klass>> = vec![vec![Klass::Bg; 10]; 10];
        b[5][5] = Klass::Stroke;

        let (fringe, hard, spots) = compare_maps(&a, &b);
        assert_eq!(
            fringe, 1,
            "lone pixel with matching class in 3x3 should be fringe"
        );
        assert_eq!(
            hard, 0,
            "lone pixel with matching class in 3x3 should not be hard"
        );
        assert!(spots.is_empty(), "no hard spots expected");
    }

    // (b) A shifted block is hard, with exact percentages.
    #[test]
    fn shifted_block_is_hard_with_exact_percentages() {
        // A: all bg (10x10)
        // B: all bg except a 4x4 stroke block at rows 3-6, cols 3-6.
        // The 4 interior pixels (4,4)(4,5)(5,4)(5,5) have no "bg" neighbour in
        // B's 3x3 -> hard. The 12 edge pixels have at least one "bg" neighbour
        // -> fringe.
        let a: Vec<Vec<Klass>> = vec![vec![Klass::Bg; 10]; 10];
        let mut b: Vec<Vec<Klass>> = vec![vec![Klass::Bg; 10]; 10];
        for y in 3..=6 {
            for x in 3..=6 {
                b[y as usize][x as usize] = Klass::Stroke;
            }
        }

        let (fringe, hard, spots) = compare_maps(&a, &b);
        assert_eq!(fringe, 12, "edge pixels of the block should be fringe");
        assert_eq!(hard, 4, "interior pixels of the block should be hard");
        assert_eq!(spots.len(), 4, "4 hard spots expected");

        // Verify exact percentages through assemble: total=100, hard=4, fringe=12.
        let report = assemble(
            "test".to_string(),
            "top".to_string(),
            10,
            100,
            12,
            4,
            spots.clone(),
            histogram(&a, &b),
        );
        assert_eq!(report.agree_pct, 84.0, "agree = 84/100 = 84.000");
        assert_eq!(report.fringe_only_pct, 12.0, "fringe = 12/100 = 12.000");
        assert_eq!(
            report.hard_geometry_mismatch_pct, 4.0,
            "hard = 4/100 = 4.000"
        );
    }

    /// (d) Histogram counts per class.
    #[test]
    fn histogram_counts_per_class() {
        // A: 2 bg + 1 stroke (1x3)
        // B: 1 bg + 2 active (1x3)
        let a = vec![vec![Klass::Bg, Klass::Bg, Klass::Stroke]];
        let b = vec![vec![Klass::Bg, Klass::Active, Klass::Active]];

        let hist = histogram(&a, &b);
        assert_eq!(
            hist.len(),
            5,
            "histogram has one row per class in CLASS_ORDER"
        );

        assert_eq!(hist[0].class, "bg");
        assert_eq!(hist[0].original, 2);
        assert_eq!(hist[0].port, 1);

        assert_eq!(hist[1].class, "active");
        assert_eq!(hist[1].original, 0);
        assert_eq!(hist[1].port, 2);

        assert_eq!(hist[2].class, "stroke");
        assert_eq!(hist[2].original, 1);
        assert_eq!(hist[2].port, 0);

        assert_eq!(hist[3].class, "orange");
        assert_eq!(hist[3].original, 0);
        assert_eq!(hist[3].port, 0);

        assert_eq!(hist[4].class, "white");
        assert_eq!(hist[4].original, 0);
        assert_eq!(hist[4].port, 0);
    }

    /// (a) + (d) end-to-end: `compare_anchored` on two full images whose
    /// bottom half differs (one class flip), verifying both the fringe
    /// percentage and the histogram counts in the same pass.
    #[test]
    fn anchored_comparison_produces_expected_hist_and_fringer() {
        use crate::parity::Anchor;
        use crate::parity::RegionSpec;

        // Two 2x2 images. A: all bg. B: one stroke pixel at (0,0).
        let a_img = RgbaImage::from_rgba(
            2,
            2,
            vec![0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255],
        )
        .unwrap();
        let b_img = RgbaImage::from_rgba(
            2,
            2,
            vec![
                0x7A, 0x7A, 0x7A, 255, // (0,0) -> stroke (luma 122)
                0, 0, 0, 255, // (1,0) -> bg
                0, 0, 0, 255, // (0,1) -> bg
                0, 0, 0, 255, // (1,1) -> bg
            ],
        )
        .unwrap();

        let regions = [RegionSpec {
            name: "test".into(),
            anchor: Anchor::Top,
            rows: 2,
        }];
        let reports = compare_anchored(&a_img, &b_img, &regions);
        let r = &reports[0];

        // Total = 4. One stroke-vs-bg disagreement at (0,0).
        // (0,0) in B has bg at (1,0) in 3x3 -> fringe, not hard.
        assert_eq!(
            r.hard_geometry_mismatch_pct, 0.0,
            "corner pixel should be fringe"
        );
        assert_eq!(r.fringe_only_pct, 25.0, "1 of 4 pixels = 25%");
        assert_eq!(r.agree_pct, 75.0, "3 of 4 pixels = 75%");

        // Histogram: A = 4 bg. B = 3 bg + 1 stroke.
        let hist = &r.histogram;
        assert_eq!(hist[0].original, 4, "A has 4 bg");
        assert_eq!(hist[0].port, 3, "B has 3 bg");
        assert_eq!(hist[2].original, 0, "A has 0 stroke");
        assert_eq!(hist[2].port, 1, "B has 1 stroke");
    }
}
