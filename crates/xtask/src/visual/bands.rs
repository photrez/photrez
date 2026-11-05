//! Per-band identity report: what the single percentage of `perceptual-diff` cannot say.
//!
//! The ratified comparator answers "how much differs", which is why a control painted the
//! wrong colour can score below the ceiling. This module splits the same image pair into
//! horizontal bands and asks per band whether the difference is a displacement, and which
//! exact colours each side is made of. A displaced band and a recoloured band look
//! identical in the total. The bands are measured after the pair is aligned sideways, so
//! one displaced column reads as one horizontal finding, not as content in every band.
//!
//! [`decide`] applies the two assertions of the adopted rule: the whole-surface percentage
//! floor and the band identity ceiling. The ceiling is applied to a band's residue, so a
//! displaced band costs what the shift leaves rather than its whole difference.
//!
//! Only [`super::perceptual::color_delta`] is shared with the ratified comparator, with the
//! same `y_only == false`. Bands never skip a pixel as anti-aliasing, because a border
//! drawn one pixel off is exactly the drift this instrument exists to catch.

use super::perceptual::color_delta;
use super::pngio::RgbaImage;
use serde::Serialize;

/// Band height the ratified band census measured with, in rows.
pub const DEFAULT_BAND_HEIGHT: u32 = 60;
/// Alignment search on both axes, in pixels, from the ratified rule.
pub const DEFAULT_MAX_SHIFT: i32 = 8;
/// Colour tiers kept per band, from the ratified census.
pub const DEFAULT_TIERS: usize = 3;
/// The band identity ceiling adopted 2026-11-04, as a percent of a band's own pixels.
/// A caller or a bundle states another number; this is only the fallback, so no
/// comparison carries a literal.
pub const RATIFIED_MAX_BAND_PCT: f64 = 20.0;

/// The rows a band covers, and what this tool concluded about them.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Band {
    /// First row of the band, inclusive.
    pub y0: u32,
    /// Last row of the band, exclusive.
    pub y1: u32,
    pub total_pixels: u64,
    /// Pixels whose YIQ distance exceeds the threshold, without anti-aliasing exclusion.
    pub changed_pixels: u64,
    pub changed_pct: f64,
    /// Rows to add to `b`'s row index to reach the matching row of `a`: positive means
    /// `b`'s content sits below `a`'s, and zero means the band is not displaced.
    pub best_dy: i32,
    /// Differing pixels at `best_dy`, which is what remains once the shift is absorbed.
    pub residual_at_best_dy: u64,
    /// The most common exact colours, as `#rrggbb` with counts, for each side of the
    /// aligned pair.
    pub tiers_a: Vec<(String, u64)>,
    pub tiers_b: Vec<(String, u64)>,
}

/// The whole-image horizontal alignment. Option-bar and list surfaces are displaced
/// sideways, not vertically, so the bands alone would report them as content differences.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Shift {
    /// Columns to add to `b`'s column index to reach the matching column of `a`: positive
    /// means `b`'s content sits to the right of `a`'s.
    pub best_dx: i32,
    /// Differing pixels with no shift applied, the starting point the alignment improves on.
    pub changed_pixels: u64,
    /// Differing pixels at `best_dx`, over the narrower region both sides still share.
    pub residual_at_best_dx: u64,
}

/// Every band of one image pair, plus the horizontal alignment they share.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BandReport {
    pub width: u32,
    pub height: u32,
    /// Columns left after the horizontal shift is applied; bands are measured on this
    /// narrower pair, so a sideways displacement is not counted as content in every band.
    pub aligned_width: u32,
    pub threshold: f64,
    pub band_height: u32,
    pub max_shift: i32,
    pub horizontal: Shift,
    pub bands: Vec<Band>,
}

/// The two ceilings one scored surface answers to. Both are stated numbers: the
/// percentage floor comes from the bundle's own allowance, the band ceiling from the
/// caller or the bundle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ceilings {
    pub max_changed_pct: f64,
    pub max_band_pct: f64,
}

/// The band that decides the verdict, read two ways.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorstBand {
    pub y0: u32,
    pub y1: u32,
    pub total_pixels: u64,
    /// Pixels over the threshold on the aligned pair, before this band's own shift.
    pub changed_pixels: u64,
    /// Pixels over the threshold once this band's own shift is absorbed. The ceiling is
    /// applied to this, so a displaced band costs its residue, not its whole difference.
    pub residual_pixels: u64,
    pub changed_pct: f64,
    pub residual_pct: f64,
}

/// One surface's verdict against both ceilings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Verdict {
    pub pass: bool,
    pub percent_pass: bool,
    pub band_pass: bool,
    pub changed_pct: f64,
    pub worst: Option<WorstBand>,
}

/// A share of a region, three decimals, with an empty region reading as zero.
pub(crate) fn share(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        return 0.0;
    }
    super::round3(part as f64 / whole as f64 * 100.0)
}

impl Band {
    fn worst(&self) -> WorstBand {
        WorstBand {
            y0: self.y0,
            y1: self.y1,
            total_pixels: self.total_pixels,
            changed_pixels: self.changed_pixels,
            residual_pixels: self.residual_at_best_dy,
            changed_pct: self.changed_pct,
            residual_pct: share(self.residual_at_best_dy, self.total_pixels),
        }
    }
}

impl BandReport {
    /// The band left with the largest residue after its own shift, as a share of its
    /// pixels. Ties go to the band higher on screen, so the named band is stable.
    pub fn worst_band(&self) -> Option<WorstBand> {
        let mut worst: Option<WorstBand> = None;
        for band in &self.bands {
            let candidate = band.worst();
            if worst
                .as_ref()
                .is_none_or(|current| candidate.residual_pct > current.residual_pct)
            {
                worst = Some(candidate);
            }
        }
        worst
    }

    /// The band with the largest changed share, before each band's own shift. Recorded
    /// beside the residue so a reader sees the displacement and the difference apart.
    pub fn worst_changed_band(&self) -> Option<WorstBand> {
        let mut worst: Option<WorstBand> = None;
        for band in &self.bands {
            let candidate = band.worst();
            if worst
                .as_ref()
                .is_none_or(|current| candidate.changed_pct > current.changed_pct)
            {
                worst = Some(candidate);
            }
        }
        worst
    }
}

/// The adopted verdict: the whole-image percentage floor and the band ceiling together.
/// A ceiling is met at equality, because the rule reads "exceeds". A surface with no
/// band has nothing that can exceed the ceiling.
pub fn decide(changed_pct: f64, report: &BandReport, ceilings: Ceilings) -> Verdict {
    let worst = report.worst_band();
    let band_pass = worst.is_none_or(|band| band.residual_pct <= ceilings.max_band_pct);
    let percent_pass = changed_pct <= ceilings.max_changed_pct;
    Verdict {
        pass: percent_pass && band_pass,
        percent_pass,
        band_pass,
        changed_pct,
        worst,
    }
}

fn pixel(img: &RgbaImage, x: u32, y: u32) -> [u8; 3] {
    let i = ((y as usize) * (img.width as usize) + x as usize) * 4;
    [img.data[i], img.data[i + 1], img.data[i + 2]]
}

fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

fn differs(a: &RgbaImage, b: &RgbaImage, x: u32, ya: u32, yb: u32, threshold: f64) -> bool {
    color_delta(pixel(a, x, ya), pixel(b, x, yb), false) > threshold
}

/// Differing pixels in rows `y0..y1` of `a` against the same rows of `b`.
fn band_changed(a: &RgbaImage, b: &RgbaImage, y0: u32, y1: u32, threshold: f64) -> u64 {
    let mut count = 0;
    for y in y0..y1 {
        for x in 0..a.width {
            if differs(a, b, x, y, y, threshold) {
                count += 1;
            }
        }
    }
    count
}

/// Differing pixels comparing `a[y]` against `b[y + dy]` over the band's rows. A shift
/// whose partner rows fall outside the image is refused, so an edge can never be reported
/// as an alignment by comparing fewer rows.
fn shifted_changed(
    a: &RgbaImage,
    b: &RgbaImage,
    y0: u32,
    y1: u32,
    dy: i32,
    threshold: f64,
) -> Option<u64> {
    let first = y0 as i64 + i64::from(dy);
    let last = y1 as i64 - 1 + i64::from(dy);
    if first < 0 || last >= i64::from(b.height) {
        return None;
    }
    let mut count = 0;
    for y in y0..y1 {
        let yb = (y as i64 + i64::from(dy)) as u32;
        for x in 0..a.width {
            if differs(a, b, x, y, yb, threshold) {
                count += 1;
            }
        }
    }
    Some(count)
}

/// The band's own vertical alignment: the shift within `±max_shift` that leaves the fewest
/// differing pixels, and what that residue is. Ties go to the smallest shift, because a
/// band that is not displaced must not be reported as one.
fn alignment(
    a: &RgbaImage,
    b: &RgbaImage,
    y0: u32,
    y1: u32,
    max_shift: i32,
    threshold: f64,
) -> (i32, u64) {
    let mut candidates: Vec<i32> = vec![0];
    for step in 1..=max_shift.abs() {
        candidates.push(step);
        candidates.push(-step);
    }
    let mut best = (0i32, u64::MAX);
    for dy in candidates {
        if let Some(count) = shifted_changed(a, b, y0, y1, dy, threshold) {
            if count < best.1 {
                best = (dy, count);
            }
        }
    }
    best
}

/// The `n` most common exact colours in the band, most frequent first.
fn tiers(img: &RgbaImage, y0: u32, y1: u32, n: usize) -> Vec<(String, u64)> {
    let mut counts: Vec<([u8; 3], u64)> = Vec::new();
    for y in y0..y1 {
        for x in 0..img.width {
            let c = pixel(img, x, y);
            match counts.iter_mut().find(|(existing, _)| *existing == c) {
                Some((_, seen)) => *seen += 1,
                None => counts.push((c, 1)),
            }
        }
    }
    counts.sort_by(|(_, left), (_, right)| right.cmp(left));
    counts
        .into_iter()
        .take(n)
        .map(|(c, seen)| (hex(c), seen))
        .collect()
}

/// Differing pixels comparing `a[x]` against `b[x + dx]`, and how many columns were
/// actually comparable. A shift that leaves nothing to compare is refused.
fn shifted_changed_x(a: &RgbaImage, b: &RgbaImage, dx: i32, threshold: f64) -> Option<(u64, u64)> {
    let width = i64::from(a.width);
    let shift = i64::from(dx);
    let (first, last) = if shift >= 0 {
        (0, width - shift)
    } else {
        (-shift, width)
    };
    if last <= first {
        return None;
    }
    let mut count = 0;
    for y in 0..a.height {
        for xi in first..last {
            let x = xi as u32;
            let xb = (xi + shift) as u32;
            if color_delta(pixel(a, x, y), pixel(b, xb, y), false) > threshold {
                count += 1;
            }
        }
    }
    Some((count, (last - first) as u64 * u64::from(a.height)))
}

/// The horizontal alignment of the whole pair. Ranked by the fraction of the comparable
/// region that differs, since a wider shift always has fewer columns; ties go to zero.
fn horizontal_alignment(a: &RgbaImage, b: &RgbaImage, max_shift: i32, threshold: f64) -> Shift {
    let unaligned = shifted_changed_x(a, b, 0, threshold)
        .map(|(c, _)| c)
        .unwrap_or(0);
    let mut candidates: Vec<i32> = vec![0];
    for step in 1..=max_shift.abs() {
        candidates.push(step);
        candidates.push(-step);
    }
    let mut best = (0i32, unaligned);
    let mut best_ratio = f64::INFINITY;
    for dx in candidates {
        if let Some((count, compared)) = shifted_changed_x(a, b, dx, threshold) {
            let ratio = count as f64 / compared as f64;
            if ratio < best_ratio {
                best_ratio = ratio;
                best = (dx, count);
            }
        }
    }
    Shift {
        best_dx: best.0,
        changed_pixels: unaligned,
        residual_at_best_dx: best.1,
    }
}

/// Both sides cropped to the columns they still share once the horizontal shift is applied.
/// Band measures run on this pair, so a displaced column of controls is one horizontal
/// finding rather than a content difference in every band.
fn aligned_pair(a: &RgbaImage, b: &RgbaImage, dx: i32) -> (RgbaImage, RgbaImage) {
    let magnitude = dx.unsigned_abs();
    let width = a.width - magnitude;
    let crop = |img: &RgbaImage, first: u32| -> RgbaImage {
        let mut data = Vec::with_capacity((width * img.height * 4) as usize);
        for y in 0..img.height {
            for x in 0..width {
                let i = ((y as usize) * (img.width as usize) + (first + x) as usize) * 4;
                data.extend_from_slice(&img.data[i..i + 4]);
            }
        }
        RgbaImage {
            width,
            height: img.height,
            data,
        }
    };
    let (a_first, b_first) = if dx >= 0 {
        (0, magnitude)
    } else {
        (magnitude, 0)
    };
    (crop(a, a_first), crop(b, b_first))
}

/// Splits both images into bands of `band_height` rows (the last band may be shorter) and
/// reports each one. Sizes must match, because a layout comparison is undefined otherwise.
pub fn report(
    a: &RgbaImage,
    b: &RgbaImage,
    threshold: f64,
    band_height: u32,
    max_shift: i32,
    tier_count: usize,
) -> Result<BandReport, String> {
    if a.width != b.width || a.height != b.height {
        return Err(format!(
            "band report: size mismatch: {}x{} vs {}x{}",
            a.width, a.height, b.width, b.height
        ));
    }
    if band_height == 0 {
        return Err("band report: --band-height must be at least 1".to_string());
    }
    let horizontal = horizontal_alignment(a, b, max_shift, threshold);
    let (shift_a, shift_b) = aligned_pair(a, b, horizontal.best_dx);
    let mut bands = Vec::new();
    let mut y0 = 0;
    while y0 < shift_a.height {
        let y1 = (y0 + band_height).min(shift_a.height);
        let total = u64::from(shift_a.width) * u64::from(y1 - y0);
        let changed = band_changed(&shift_a, &shift_b, y0, y1, threshold);
        let (best_dy, residual) = alignment(&shift_a, &shift_b, y0, y1, max_shift, threshold);
        bands.push(Band {
            y0,
            y1,
            total_pixels: total,
            changed_pixels: changed,
            changed_pct: super::round3(changed as f64 / total as f64 * 100.0),
            best_dy,
            residual_at_best_dy: residual,
            tiers_a: tiers(&shift_a, y0, y1, tier_count),
            tiers_b: tiers(&shift_b, y0, y1, tier_count),
        });
        y0 = y1;
    }
    Ok(BandReport {
        width: a.width,
        height: a.height,
        aligned_width: shift_a.width,
        threshold,
        band_height,
        max_shift,
        horizontal,
        bands,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(width: u32, height: u32, pixel_for: impl Fn(u32, u32) -> [u8; 3]) -> RgbaImage {
        let mut data = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                let [r, g, b] = pixel_for(x, y);
                data.extend_from_slice(&[r, g, b, 255]);
            }
        }
        RgbaImage {
            width,
            height,
            data,
        }
    }

    fn flat(width: u32, height: u32) -> RgbaImage {
        image(width, height, |_, _| [30, 30, 30])
    }

    /// A band report on identical images must find nothing: no changed pixels, no shift,
    /// and the same tiers on both sides.
    #[test]
    fn identical_images_report_nothing() {
        let a = flat(16, 40);
        let b = flat(16, 40);
        let r = report(&a, &b, 0.1, 20, 4, 3).expect("report");
        assert_eq!(r.bands.len(), 2);
        for band in &r.bands {
            assert_eq!(band.changed_pixels, 0, "band {}..{}", band.y0, band.y1);
            assert_eq!(band.best_dy, 0);
            assert_eq!(band.residual_at_best_dy, 0);
            assert_eq!(band.tiers_a, band.tiers_b);
            assert_eq!(band.tiers_a[0].0, "#1e1e1e");
            assert_eq!(band.tiers_a[0].1, 16 * 20);
        }
    }

    /// A band whose content sits three rows lower is a displacement: the shift has to be
    /// found, and the residue after it must be far below the unaligned difference.
    #[test]
    fn a_displaced_band_is_found_as_a_shift_not_a_difference() {
        let stripe = |_x: u32, y: u32| {
            if (40..44).contains(&y) {
                [242, 142, 66]
            } else {
                [30, 30, 30]
            }
        };
        let a = image(16, 80, stripe);
        let b = image(16, 80, |x, y| stripe(x, y.saturating_sub(3)));
        let r = report(&a, &b, 0.1, 20, 6, 2).expect("report");
        let band = &r.bands[2];
        assert_eq!(band.best_dy, 3, "b's stripe sits three rows lower than a's");
        assert_eq!(
            band.residual_at_best_dy, 0,
            "the shift explains the whole band"
        );
        assert!(band.changed_pixels > 0);
    }

    /// A list displaced sideways is found on the horizontal axis, and the bands measured
    /// afterwards are clean: the shift is a layout fact, not content in every band.
    #[test]
    fn a_sideways_displacement_is_found_horizontally() {
        let block = |x: u32, y: u32| {
            if (40..60).contains(&x) && (10..30).contains(&y) {
                [242, 142, 66]
            } else {
                [30, 30, 30]
            }
        };
        let a = image(100, 40, block);
        let b = image(100, 40, |x, y| {
            if x >= 7 {
                block(x - 7, y)
            } else {
                [30, 30, 30]
            }
        });
        let r = report(&a, &b, 0.1, 20, 10, 2).expect("report");
        assert_eq!(
            r.horizontal.best_dx, 7,
            "b's block sits seven columns right"
        );
        assert_eq!(
            r.horizontal.residual_at_best_dx, 0,
            "the shift explains the whole pair"
        );
        assert!(r.horizontal.changed_pixels > 0);
        assert_eq!(r.aligned_width, 93, "100 columns minus a 7-column shift");
        for band in &r.bands {
            assert_eq!(
                band.changed_pixels, 0,
                "band {}..{} is aligned, not different",
                band.y0, band.y1
            );
        }
    }

    /// The two halves of the report must describe the same pair: the band totals add up to
    /// the residue the horizontal alignment reported.
    #[test]
    fn band_totals_add_up_to_the_aligned_residue() {
        let a = image(60, 50, |x, y| {
            if x > 30 && y > 10 {
                [242, 142, 66]
            } else {
                [30, 30, 30]
            }
        });
        let b = image(60, 50, |x, y| {
            if x > 20 && y > 18 {
                [70, 130, 180]
            } else {
                [30, 30, 30]
            }
        });
        let r = report(&a, &b, 0.1, 10, 12, 2).expect("report");
        let total: u64 = r.bands.iter().map(|band| band.changed_pixels).sum();
        assert_eq!(total, r.horizontal.residual_at_best_dx);
        assert!(total > 0, "this pair really does differ");
    }

    /// The guard itself: a shift that would read rows outside the image is refused rather
    /// than quietly comparing fewer rows and looking better than it is.
    #[test]
    fn a_shift_needing_rows_outside_the_image_is_refused() {
        let a = flat(8, 80);
        let b = flat(8, 80);
        assert!(shifted_changed(&a, &b, 0, 10, 8, 0.1).is_some());
        assert!(shifted_changed(&a, &b, 0, 10, -1, 0.1).is_none());
        assert!(shifted_changed(&a, &b, 70, 80, -8, 0.1).is_some());
        assert!(shifted_changed(&a, &b, 70, 80, 1, 0.1).is_none());
    }

    /// The case the total percentage cannot see: identical geometry, different colour.
    /// No shift helps, so the residue stays at the full difference while the tiers change.
    #[test]
    fn a_recoloured_band_shows_no_shift_and_different_tiers() {
        let a = flat(16, 20);
        let b = image(16, 20, |_, _| [255, 0, 0]);
        let r = report(&a, &b, 0.1, 20, 6, 1).expect("report");
        let band = &r.bands[0];
        assert_eq!(band.changed_pixels, 16 * 20);
        assert_eq!(band.best_dy, 0, "a colour change is not a shift");
        assert_eq!(band.residual_at_best_dy, band.changed_pixels);
        assert_eq!(band.tiers_a[0].0, "#1e1e1e");
        assert_eq!(band.tiers_b[0].0, "#ff0000");
    }

    /// A band that spans the whole image has no partner rows outside itself, so every
    /// non-zero shift is refused and the difference stays a difference.
    #[test]
    fn a_full_height_band_has_no_room_to_shift() {
        let a = image(
            8,
            10,
            |_, y| {
                if y < 5 {
                    [30, 30, 30]
                } else {
                    [242, 142, 66]
                }
            },
        );
        let b = flat(8, 10);
        let r = report(&a, &b, 0.1, 10, 8, 2).expect("report");
        assert_eq!(r.bands[0].best_dy, 0, "no shift fits inside a 10-row image");
        assert_eq!(r.bands[0].residual_at_best_dy, r.bands[0].changed_pixels);
    }

    /// A layout comparison between different sizes is meaningless, and must say so.
    #[test]
    fn mismatched_sizes_are_refused() {
        let a = flat(16, 20);
        let b = flat(16, 24);
        let err = report(&a, &b, 0.1, 10, 4, 2).expect_err("sizes differ");
        assert!(err.contains("size mismatch"), "got `{err}`");
    }

    const DARK: [u8; 3] = [30, 30, 30];
    const MAGENTA: [u8; 3] = [255, 0, 255];
    const ORANGE: [u8; 3] = [242, 142, 66];

    /// The ceilings the ratified rule states, so no test invents its own line.
    fn ratified_ceilings() -> Ceilings {
        Ceilings {
            max_changed_pct: crate::visual::RATIFIED_MAX_CHANGED_PCT,
            max_band_pct: RATIFIED_MAX_BAND_PCT,
        }
    }

    /// A band that sits exactly on the ceiling is not over it: the rule reads "exceeds",
    /// so 20 % of a band's pixels passing is the ratified line, not a failure.
    #[test]
    fn a_band_exactly_at_the_ceiling_passes() {
        let a = image(100, 60, |_, _| DARK);
        let b = image(100, 60, |_, y| if y < 12 { MAGENTA } else { DARK });
        let report = report(&a, &b, 0.1, 60, 8, 3).expect("report");
        let worst = report.worst_band().expect("one band");
        assert_eq!((worst.changed_pixels, worst.total_pixels), (1200, 6000));
        assert_eq!(
            worst.residual_pct, RATIFIED_MAX_BAND_PCT,
            "12 of 60 rows is exactly {} %",
            RATIFIED_MAX_BAND_PCT
        );
        assert!(
            decide(0.0, &report, ratified_ceilings()).band_pass,
            "at the ceiling the band passes"
        );
    }

    /// One pixel more than the ceiling is a surface failure, and `pass` says so rather
    /// than leaving the reader to combine two fields.
    #[test]
    fn one_pixel_above_the_ceiling_fails_the_surface() {
        let a = image(100, 60, |_, _| DARK);
        let b = image(100, 60, |_, y| if y < 13 { MAGENTA } else { DARK });
        let report = report(&a, &b, 0.1, 60, 8, 3).expect("report");
        let verdict = decide(0.0, &report, ratified_ceilings());
        assert_eq!(verdict.worst.expect("one band").residual_pct, 21.667);
        assert!(!verdict.band_pass);
        assert!(!verdict.pass, "a band fail is a surface fail");
    }

    /// A displaced band is a layout fact, not a colour defect. The ceiling is applied to
    /// what the shift leaves, so the same pair that reads 26.667 % as measured reads 0 %.
    #[test]
    fn alignment_rescues_a_displaced_band() {
        let a = image(
            200,
            180,
            |_, y| {
                if (60..72).contains(&y) {
                    ORANGE
                } else {
                    DARK
                }
            },
        );
        let b = image(
            200,
            180,
            |_, y| {
                if (68..80).contains(&y) {
                    ORANGE
                } else {
                    DARK
                }
            },
        );
        let report = report(&a, &b, 0.1, 60, 8, 3).expect("report");
        let displaced = &report.bands[1];
        assert_eq!((displaced.y0, displaced.y1), (60, 120));
        assert_eq!(
            report.worst_changed_band().expect("a band").changed_pct,
            26.667,
            "as measured, the band is over the ceiling"
        );
        assert_eq!(displaced.best_dy, 8, "b's stripe sits eight rows lower");
        assert_eq!(
            displaced.residual_at_best_dy, 0,
            "the shift explains the whole band"
        );
        assert_eq!(report.worst_band().expect("a band").residual_pct, 0.0);
        assert!(
            decide(5.0, &report, ratified_ceilings()).pass,
            "so a displaced band is not reported as a colour defect"
        );
    }

    /// The case the whole-image percentage cannot see: one control painted the wrong colour
    /// is 7.5 % of the mask and 25 % of the band it sits in, so the floor clears it and the
    /// ceiling does not.
    #[test]
    fn a_band_can_fail_while_the_percentage_passes() {
        let a = image(400, 200, |_, _| DARK);
        let b = image(400, 200, |x, y| {
            if (100..300).contains(&x) && (80..110).contains(&y) {
                MAGENTA
            } else {
                DARK
            }
        });
        let measured = super::super::perceptual::compare(&a, &b, 0.1, true).expect("measure");
        assert_eq!(
            measured.changed_pixels, 6000,
            "the painted control, nothing else"
        );
        assert!(
            measured.changed_pct < ratified_ceilings().max_changed_pct,
            "the ratified floor would pass it: {} %",
            measured.changed_pct
        );
        let report = report(&a, &b, 0.1, 60, 8, 3).expect("report");
        assert_eq!(report.horizontal.best_dx, 0, "the pair is not displaced");
        let verdict = decide(measured.changed_pct, &report, ratified_ceilings());
        assert!(verdict.percent_pass, "the floor is met on its own");
        assert!(!verdict.band_pass, "the band the control sits in is not");
        let worst = verdict.worst.expect("a band trips the ceiling");
        assert_eq!((worst.y0, worst.y1), (60, 120), "and the report names it");
        assert_eq!(worst.residual_pct, 25.0);
    }

    /// A report with no band has nothing that can exceed the ceiling, so it is not a pass
    /// invented by an absent measurement.
    #[test]
    fn an_empty_report_has_no_worst_band() {
        let a = image(100, 0, |_, _| DARK);
        let b = image(100, 0, |_, _| DARK);
        let report = report(&a, &b, 0.1, 60, 8, 3).expect("report");
        assert!(report.bands.is_empty());
        assert_eq!(report.worst_band(), None);
        assert!(decide(0.0, &report, ratified_ceilings()).band_pass);
    }
}
