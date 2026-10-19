//! Bounding-box and cross-section measurements (`measure.ts` semantics) -
//! "is the icon the right SIZE and the stroke the right WIDTH".
//!
//! The recorded probe: the crop icon is the rail's 3rd button (index 2, so
//! `y = 8 + 2*38 = 84`), its button window is `x 0..52, y 84..120` (52px rail,
//! 36px tall button). Floors 25 (solid ink) and 6 (faint halo). The recorded
//! solid bbox on the icon stroke is `x 18..33, y 94..109` - the ink of the
//! crop icon's vertical stroke clearing `#1B1B1B + 25`.

use serde::Serialize;

use super::pngio::RgbaImage;
use super::zoom::DEFAULT_BG_LUMA;
use super::Rect;

/// Rec.601 luma, matching `measure.ts` / `zoom.ts` / `classify` exactly.
fn luma(r: u8, g: u8, b: u8) -> f64 {
    0.2126 * f64::from(r) + 0.7152 * f64::from(g) + 0.0722 * f64::from(b)
}

/// Round to one decimal place, mirroring `measure.ts`'s `.toFixed(1)` on the
/// ink-above-background value reported by the cross-sections.
fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

/// Bounding box of pixels standing out from the region background.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bbox {
    pub min_x: u32,
    pub min_y: u32,
    pub max_x: u32,
    pub max_y: u32,
    pub w: u32,
    pub h: u32,
    pub count: u64,
}

/// Bounding box of pixels whose luma exceeds `bg_luma + floor`, clipped to the
/// window rect. `None` when no pixel clears the floor (TS returns `null`).
pub fn bbox(img: &RgbaImage, win: Rect, floor: f64, bg_luma: f64) -> Option<Bbox> {
    let x1 = win.x.saturating_add(win.width).min(img.width);
    let y1 = win.y.saturating_add(win.height).min(img.height);
    let mut min_x = u32::MAX;
    let mut min_y = u32::MAX;
    let mut max_x = 0u32;
    let mut max_y = 0u32;
    let mut count: u64 = 0;
    let mut found = false;
    for y in win.y..y1 {
        for x in win.x..x1 {
            let [r, g, b, _] = img.pixel(x, y);
            if luma(r, g, b) - bg_luma < floor {
                continue;
            }
            found = true;
            if x < min_x {
                min_x = x;
            }
            if x > max_x {
                max_x = x;
            }
            if y < min_y {
                min_y = y;
            }
            if y > max_y {
                max_y = y;
            }
            count += 1;
        }
    }
    if !found {
        return None;
    }
    Some(Bbox {
        min_x,
        min_y,
        max_x,
        max_y,
        w: max_x - min_x + 1,
        h: max_y - min_y + 1,
        count,
    })
}

/// Ink-above-background value per column across a horizontal band (inclusive
/// `x0..=x1`), one-decimal rounded like `measure.ts`.
pub fn column_profile(img: &RgbaImage, y: u32, x0: u32, x1: u32, bg_luma: f64) -> Vec<f64> {
    let mut out = Vec::with_capacity((x1 as usize).saturating_add(1).saturating_sub(x0 as usize));
    if y >= img.height {
        return out;
    }
    for x in x0..=x1 {
        if x >= img.width {
            break;
        }
        let [r, g, b, _] = img.pixel(x, y);
        out.push(round1(luma(r, g, b) - bg_luma));
    }
    out
}

/// Ink-above-background value per row down a vertical line (inclusive
/// `y0..=y1`), one-decimal rounded like `measure.ts`.
pub fn row_profile(img: &RgbaImage, x: u32, y0: u32, y1: u32, bg_luma: f64) -> Vec<f64> {
    let mut out = Vec::with_capacity((y1 as usize).saturating_add(1).saturating_sub(y0 as usize));
    if x >= img.width {
        return out;
    }
    for y in y0..=y1 {
        if y >= img.height {
            break;
        }
        let [r, g, b, _] = img.pixel(x, y);
        out.push(round1(luma(r, g, b) - bg_luma));
    }
    out
}

/// The crop icon button window from `measure.ts`: the 3rd button (index 2).
/// Buttons sit at `y = 8 + i*38`; for i=2 that is y=84, 36px tall, 52px wide.
fn crop_window() -> Rect {
    Rect {
        x: 0,
        y: 84,
        width: 52,
        height: 36,
    }
}

/// The full recorded probe: both images, both floors, both cross-sections.
/// Geometry frozen from `measure.ts`.
#[derive(Clone, Debug)]
pub struct RailCropIconProbe {
    pub baseline_solid: Option<Bbox>,
    pub baseline_faint: Option<Bbox>,
    pub port_solid: Option<Bbox>,
    pub port_faint: Option<Bbox>,
    pub baseline_horizontal: Vec<f64>,
    pub port_horizontal: Vec<f64>,
    pub baseline_vertical: Vec<f64>,
    pub port_vertical: Vec<f64>,
}

/// Run the recorded probe on the accepted fixture pair.
pub fn rail_crop_icon(baseline: &RgbaImage, port: &RgbaImage) -> RailCropIconProbe {
    let win = crop_window();
    let bg = DEFAULT_BG_LUMA;

    // Cross-section through the crop icon's vertical stroke: mid-button row
    // (btnY + 18 = 102), spanning the icon's box x 14..30 (measure.ts probeX0/1).
    let probe_y = 84 + 18;
    // Cross-section down the centre of the icon's horizontal stroke: the path
    // runs x 17..34 device px, so the centre column is floor((17+34)/2) = 25,
    // across rows iconTop+16 +/- 3 (iconTop = btnY + 9 = 93 -> row 109).
    let probe_x = (17 + 34) / 2;
    let row_y = (84 + 9) + 16;

    RailCropIconProbe {
        baseline_solid: bbox(baseline, win, 25.0, bg),
        baseline_faint: bbox(baseline, win, 6.0, bg),
        port_solid: bbox(port, win, 25.0, bg),
        port_faint: bbox(port, win, 6.0, bg),
        baseline_horizontal: column_profile(baseline, probe_y, 14, 30, bg),
        port_horizontal: column_profile(port, probe_y, 14, 30, bg),
        baseline_vertical: row_profile(baseline, probe_x, row_y - 3, row_y + 3, bg),
        port_vertical: row_profile(port, probe_x, row_y - 3, row_y + 3, bg),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, px: [u8; 4]) -> RgbaImage {
        let mut data = vec![0u8; w as usize * h as usize * 4];
        for p in data.chunks_exact_mut(4) {
            p.copy_from_slice(&px);
        }
        RgbaImage::from_rgba(w, h, data).expect("test image")
    }

    #[test]
    fn bbox_empty_when_below_floor() {
        // luma 27 == bg, ink 0 < 25 -> no pixels clear the floor.
        let a = solid(4, 4, [27, 27, 27, 255]);
        let win = Rect {
            x: 0,
            y: 0,
            width: 4,
            height: 4,
        };
        assert_eq!(bbox(&a, win, 25.0, 27.0), None);
    }

    #[test]
    fn bbox_synthetic_2x2_block() {
        // 4x4, bg luma 27 everywhere, 2x2 block of luma 127 at (1,1)..(2,2).
        // floor 25 -> only the 4 bright pixels (127-27=100 >= 25) count.
        let mut data = vec![27u8; 4 * 4 * 4];
        for y in 1..3 {
            for x in 1..3 {
                let i = ((y * 4 + x) as usize) * 4;
                data[i..i + 4].copy_from_slice(&[127, 127, 127, 255]);
            }
        }
        let a = RgbaImage::from_rgba(4, 4, data).expect("test image");
        let win = Rect {
            x: 0,
            y: 0,
            width: 4,
            height: 4,
        };
        let b = bbox(&a, win, 25.0, 27.0).expect("block present");
        assert_eq!(b.min_x, 1);
        assert_eq!(b.min_y, 1);
        assert_eq!(b.max_x, 2);
        assert_eq!(b.max_y, 2);
        assert_eq!(b.w, 2);
        assert_eq!(b.h, 2);
        assert_eq!(b.count, 4);
    }

    #[test]
    fn column_profile_one_decimal() {
        // luma 127, bg 27 -> 100.0 per pixel.
        let a = solid(4, 1, [127, 127, 127, 255]);
        let p = column_profile(&a, 0, 0, 3, 27.0);
        assert_eq!(p, vec![100.0, 100.0, 100.0, 100.0]);
    }

    #[test]
    fn row_profile_one_decimal() {
        // luma 100, bg 27 -> 73.0 per pixel.
        let a = solid(1, 4, [100, 100, 100, 255]);
        let p = row_profile(&a, 0, 0, 3, 27.0);
        assert_eq!(p, vec![73.0, 73.0, 73.0, 73.0]);
    }

    #[test]
    fn probe_geometry_is_frozen_from_measure_ts() {
        // The window/y and probe coords the cross-check asserts against are
        // derived from measure.ts; pin them so a "faithful refactor" cannot
        // drift the recorded bbox.
        let win = crop_window();
        assert_eq!(win.x, 0);
        assert_eq!(win.y, 84);
        assert_eq!(win.width, 52);
        assert_eq!(win.height, 36);
    }
}
