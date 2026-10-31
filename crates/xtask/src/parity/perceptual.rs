//! Perceptual pixel comparison with anti-aliasing detection.
//!
//! The class-map diff (`shapediff`) buckets colours, so every glyph edge a
//! rasteriser draws differently counts as a failure. Text-dense surfaces then
//! sit at 13-16 % even when their geometry is exact. This module implements the
//! industry-standard rule instead: pixelmatch's YIQ colour distance and its
//! anti-aliasing detector (the comparator behind Playwright's screenshot
//! assertion). A pixel whose colour is produced by anti-aliasing is not a
//! difference a user can see, so it is excluded when requested.
//!
//! The threshold is explicit and never defaults to a ratifying value: callers
//! pass it, and `changed_pct` is reported next to `ignored_aa_pixels` so the
//! reader can judge the measurement.

use super::pngio::RgbaImage;
use serde::Serialize;

/// pixelmatch's default perceptual threshold.
pub const DEFAULT_THRESHOLD: f64 = 0.1;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Report {
    pub changed_pixels: u64,
    pub total_pixels: u64,
    pub changed_pct: f64,
    pub ignored_aa_pixels: u64,
    pub threshold: f64,
    pub ignore_aa: bool,
}

fn rgb(img: &RgbaImage, x: u32, y: u32) -> [u8; 3] {
    let i = ((y as usize) * (img.width as usize) + x as usize) * 4;
    [img.data[i], img.data[i + 1], img.data[i + 2]]
}

/// pixelmatch's YIQ colour distance, with alpha folded into luminance.
pub fn color_delta(a: [u8; 3], b: [u8; 3], y_only: bool) -> f64 {
    let r1 = f64::from(a[0]) / 255.0;
    let g1 = f64::from(a[1]) / 255.0;
    let b1 = f64::from(a[2]) / 255.0;
    let r2 = f64::from(b[0]) / 255.0;
    let g2 = f64::from(b[1]) / 255.0;
    let b2 = f64::from(b[2]) / 255.0;
    let y = (r1 * 0.298_895 + g1 * 0.586_622 + b1 * 0.114_483)
        - (r2 * 0.298_895 + g2 * 0.586_622 + b2 * 0.114_483);
    if y_only {
        return y.abs();
    }
    let i = (r1 * 0.595_977 + g1 * -0.274_176 + b1 * -0.321_801)
        - (r2 * 0.595_977 + g2 * -0.274_176 + b2 * -0.321_801);
    let q = (r1 * 0.211_456 + g1 * -0.522_591 + b1 * 0.311_135)
        - (r2 * 0.211_456 + g2 * -0.522_591 + b2 * 0.311_135);
    y.abs().max(i.abs()).max(q.abs())
}

fn siblings(img: &RgbaImage, x: u32, y: u32) -> bool {
    let x0 = x.saturating_sub(1);
    let y0 = y.saturating_sub(1);
    let x2 = (x + 1).min(img.width - 1);
    let y2 = (y + 1).min(img.height - 1);
    let center = rgb(img, x, y);
    let mut count = 0;
    for ny in y0..=y2 {
        for nx in x0..=x2 {
            if nx == x && ny == y {
                continue;
            }
            if color_delta(center, rgb(img, nx, ny), true) == 0.0 {
                count += 1;
                if count > 2 {
                    return true;
                }
            }
        }
    }
    false
}

/// pixelmatch's `antialiased`: the pixel is on the ramp of an edge that has a
/// flat sibling area on at least one side, in both images.
pub fn antialiased(a: &RgbaImage, b: &RgbaImage, x: u32, y: u32) -> bool {
    let x0 = x.saturating_sub(1);
    let y0 = y.saturating_sub(1);
    let x2 = (x + 1).min(a.width - 1);
    let y2 = (y + 1).min(a.height - 1);
    let center = rgb(a, x, y);
    let mut zeroes = u32::from(x == x0 || x == x2 || y == y0 || y == y2);
    let mut min = f64::INFINITY;
    let mut max = 0.0f64;
    let mut min_at = (x, y);
    let mut max_at = (x, y);
    for ny in y0..=y2 {
        for nx in x0..=x2 {
            if nx == x && ny == y {
                continue;
            }
            let delta = color_delta(center, rgb(a, nx, ny), true);
            if delta == 0.0 {
                zeroes += 1;
                if zeroes > 2 {
                    return false;
                }
            } else if delta < min {
                min = delta;
                min_at = (nx, ny);
            } else if delta > max {
                max = delta;
                max_at = (nx, ny);
            }
        }
    }
    if min == f64::INFINITY || max == 0.0 {
        return false;
    }
    (siblings(a, min_at.0, min_at.1) && siblings(b, min_at.0, min_at.1))
        || (siblings(a, max_at.0, max_at.1) && siblings(b, max_at.0, max_at.1))
}

/// Compare two equally sized images, optionally ignoring anti-aliased pixels.
pub fn compare(
    a: &RgbaImage,
    b: &RgbaImage,
    threshold: f64,
    ignore_aa: bool,
) -> Result<Report, String> {
    if a.width != b.width || a.height != b.height {
        return Err(format!(
            "perceptual compare: size mismatch: {}x{} vs {}x{}",
            a.width, a.height, b.width, b.height
        ));
    }
    let mut changed = 0u64;
    let mut ignored = 0u64;
    for y in 0..a.height {
        for x in 0..a.width {
            let delta = color_delta(rgb(a, x, y), rgb(b, x, y), false);
            if delta <= threshold {
                continue;
            }
            if ignore_aa && (antialiased(a, b, x, y) || antialiased(b, a, x, y)) {
                ignored += 1;
                continue;
            }
            changed += 1;
        }
    }
    let total = u64::from(a.width) * u64::from(a.height);
    Ok(Report {
        changed_pixels: changed,
        total_pixels: total,
        changed_pct: super::round3((changed as f64 / total as f64) * 100.0),
        ignored_aa_pixels: ignored,
        threshold,
        ignore_aa,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(width: u32, height: u32, pixel: impl Fn(u32, u32) -> [u8; 3]) -> RgbaImage {
        let mut data = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                let [r, g, b] = pixel(x, y);
                data.extend_from_slice(&[r, g, b, 255]);
            }
        }
        RgbaImage {
            width,
            height,
            data,
        }
    }

    #[test]
    fn identical_images_are_zero() {
        let a = img(8, 8, |_, _| [40, 40, 40]);
        let report = compare(&a, &a, DEFAULT_THRESHOLD, true).unwrap();
        assert_eq!(report.changed_pixels, 0);
        assert_eq!(report.ignored_aa_pixels, 0);
    }

    #[test]
    fn a_real_colour_change_is_counted_even_with_aa_off() {
        let a = img(8, 8, |_, _| [0, 0, 0]);
        let b = img(8, 8, |_, _| [255, 255, 255]);
        let report = compare(&a, &b, DEFAULT_THRESHOLD, true).unwrap();
        assert_eq!(report.changed_pixels, 64);
    }

    #[test]
    fn an_antialiased_seam_is_ignored_when_requested() {
        // A hard dark/light edge versus the same edge with a one-pixel grey
        // ramp: the seam is rasteriser output, not a layout difference.
        let hard = img(
            16,
            8,
            |x, _| if x < 8 { [0, 0, 0] } else { [255, 255, 255] },
        );
        let soft = img(16, 8, |x, _| match x {
            0..=6 => [0, 0, 0],
            7 => [128, 128, 128],
            _ => [255, 255, 255],
        });
        let with_aa = compare(&hard, &soft, DEFAULT_THRESHOLD, false).unwrap();
        let without_aa = compare(&hard, &soft, DEFAULT_THRESHOLD, true).unwrap();
        assert!(with_aa.changed_pixels > 0);
        assert!(
            without_aa.ignored_aa_pixels > 0,
            "the seam must be classified as anti-aliasing"
        );
        assert!(without_aa.changed_pixels < with_aa.changed_pixels);
    }
}
