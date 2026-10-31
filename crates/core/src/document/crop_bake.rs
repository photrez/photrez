// SPDX-License-Identifier: AGPL-3.0-or-later
//! Pixel half of a destructive crop (`delete_cropped_pixels`).
//!
//! The crop arm's metadata math lives in `canvas.rs` and stays there. This
//! module answers the pixel question that math cannot: given a layer's
//! post-crop transform, which of its own pixels still fall inside the new
//! document rectangle? Everything else is discarded, exactly like the oracle's
//! pixel-baking crop, and the discarded bytes are captured as a pixel delta so
//! one undo step restores them.
//!
//! The engine owns no pixel buffer: the plan produced here is drained by
//! `DocumentPixelStore` right after the command is applied.

/// Layer placement after the crop, in the compositor's own convention
/// (`render::compositor::composite_transformed`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct CropLayerPlacement {
    pub x: f64,
    pub y: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub rotation: f64,
    pub flip_h: bool,
    pub flip_v: bool,
}

/// What the store needs to bake one layer.
#[derive(Clone, Debug)]
pub(crate) struct CropBakeLayer {
    pub layer_id: String,
    pub placement: CropLayerPlacement,
}

/// The pending pixel half of one crop command.
#[derive(Clone, Debug)]
pub(crate) struct CropBakePlan {
    /// Index of the metadata entry this bake belongs to.
    pub entry_index: usize,
    /// New document size.
    pub doc_width: f64,
    pub doc_height: f64,
    pub layers: Vec<CropBakeLayer>,
    /// `fillBackgroundColor`: paint this colour under the Background layer.
    pub fill: Option<FillBakePlan>,
}

/// Zero every pixel of `pixels` (`width` x `height`, row-major RGBA) whose
/// transformed position falls outside `[0, doc_width) x [0, doc_height)`.
/// Returns the number of pixels that were made transparent.
///
/// The forward map is the compositor's: a layer pixel centre `(i + 0.5,
/// j + 0.5)` is scaled about the bitmap centre by `(scale * flip)`, rotated by
/// `rotation` degrees, then translated so the bitmap centre lands at the
/// layer's centre. Keeping the test identical to the painter is what makes
/// "visible after bake" mean the same thing as "visible when composited".
pub(crate) fn discard_outside_document(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    placement: &CropLayerPlacement,
    doc_width: f64,
    doc_height: f64,
) -> u64 {
    let w = width as f64;
    let h = height as f64;
    let sx = placement.scale_x * if placement.flip_h { -1.0 } else { 1.0 };
    let sy = placement.scale_y * if placement.flip_v { -1.0 } else { 1.0 };
    if sx == 0.0 || sy == 0.0 {
        // Degenerate placement paints nothing; every pixel is cropped away.
        pixels.fill(0);
        return u64::from(width) * u64::from(height);
    }
    let cx = placement.x + (w * placement.scale_x.abs()) / 2.0;
    let cy = placement.y + (h * placement.scale_y.abs()) / 2.0;
    let theta = placement.rotation.to_radians();
    let cos = theta.cos();
    let sin = theta.sin();
    let mut discarded = 0u64;
    for j in 0..height {
        let local_y = (f64::from(j) + 0.5 - h / 2.0) * sy;
        for i in 0..width {
            let local_x = (f64::from(i) + 0.5 - w / 2.0) * sx;
            let doc_x = local_x * cos - local_y * sin + cx;
            let doc_y = local_x * sin + local_y * cos + cy;
            let inside = doc_x >= 0.0 && doc_x < doc_width && doc_y >= 0.0 && doc_y < doc_height;
            if !inside {
                let at = ((j as usize * width as usize) + i as usize) * 4;
                if pixels[at..at + 4].iter().any(|b| *b != 0) {
                    discarded += 1;
                }
                pixels[at..at + 4].fill(0);
            }
        }
    }
    discarded
}

/// The fill half of a destructive crop: a solid colour baked UNDER the
/// Background layer, producing a new document-sized Background bitmap.
#[derive(Clone, Debug)]
pub(crate) struct FillBakePlan {
    pub layer_id: String,
    /// Placement of the Background BEFORE the crop reset it; the bake samples
    /// the original content through this transform.
    pub placement: CropLayerPlacement,
    pub width: u32,
    pub height: u32,
    pub color: [u8; 4],
}

/// Sample one layer bilinearly with the compositor's own convention.
fn sample_bilinear(source: &[u8], width: usize, height: usize, u: f64, v: f64) -> [u8; 4] {
    let x = (u - 0.5).clamp(0.0, (width - 1) as f64);
    let y = (v - 0.5).clamp(0.0, (height - 1) as f64);
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);
    let fx = x - x0 as f64;
    let fy = y - y0 as f64;
    let at = |px: usize, py: usize| {
        let i = (py * width + px) * 4;
        [
            f64::from(source[i]),
            f64::from(source[i + 1]),
            f64::from(source[i + 2]),
            f64::from(source[i + 3]),
        ]
    };
    let (p00, p10, p01, p11) = (at(x0, y0), at(x1, y0), at(x0, y1), at(x1, y1));
    let mut out = [0u8; 4];
    for c in 0..4 {
        let top = p00[c] + (p10[c] - p00[c]) * fx;
        let bottom = p01[c] + (p11[c] - p01[c]) * fx;
        out[c] = (top + (bottom - top) * fy).round().clamp(0.0, 255.0) as u8;
    }
    out
}

/// Composite `fill` UNDER one layer's content for the whole cropped document,
/// returning a `doc_width x doc_height` RGBA buffer.
///
/// Mirrors the oracle's "fill canvas, then draw the cropped Background over it"
/// with straight-alpha `src over fill`, using the compositor's inverse map so
/// the sampled content lines up with what the painter shows.
pub(crate) fn composite_layer_over_fill(
    source: &[u8],
    width: u32,
    height: u32,
    placement: &CropLayerPlacement,
    doc_width: u32,
    doc_height: u32,
    fill: [u8; 4],
) -> Vec<u8> {
    let w = width as f64;
    let h = height as f64;
    let sw = width as usize;
    let sh = height as usize;
    let sx = placement.scale_x * if placement.flip_h { -1.0 } else { 1.0 };
    let sy = placement.scale_y * if placement.flip_v { -1.0 } else { 1.0 };
    let cx = placement.x + (w * placement.scale_x.abs()) / 2.0;
    let cy = placement.y + (h * placement.scale_y.abs()) / 2.0;
    let theta = placement.rotation.to_radians();
    let cos = theta.cos();
    let sin = theta.sin();
    let degenerate = sx == 0.0 || sy == 0.0;
    let fill_a = f64::from(fill[3]) / 255.0;
    let fill_p = [
        f64::from(fill[0]) * fill_a,
        f64::from(fill[1]) * fill_a,
        f64::from(fill[2]) * fill_a,
    ];
    let mut out = vec![0u8; (doc_width as usize * doc_height as usize) * 4];
    for y in 0..doc_height {
        for x in 0..doc_width {
            let mut dst = fill;
            if !degenerate && sw > 0 && sh > 0 {
                let rx = f64::from(x) + 0.5 - cx;
                let ry = f64::from(y) + 0.5 - cy;
                let local_x = rx * cos + ry * sin;
                let local_y = -rx * sin + ry * cos;
                let u = local_x / sx + w / 2.0;
                let v = local_y / sy + h / 2.0;
                let src = sample_bilinear(source, sw, sh, u, v);
                if src[3] > 0 {
                    let sa = f64::from(src[3]) / 255.0;
                    let inv = 1.0 - sa;
                    let out_a = sa + fill_a * inv;
                    if out_a > 0.0 {
                        for c in 0..3 {
                            let src_p = f64::from(src[c]) * sa;
                            let dst_p = fill_p[c] * inv;
                            dst[c] = ((src_p + dst_p) / out_a).round().clamp(0.0, 255.0) as u8;
                        }
                        dst[3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
                    }
                }
            }
            let at = ((y as usize * doc_width as usize) + x as usize) * 4;
            out[at..at + 4].copy_from_slice(&dst);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: u32, height: u32) -> Vec<u8> {
        vec![255u8; (width * height * 4) as usize]
    }

    fn placement(x: f64, y: f64, sx: f64, sy: f64) -> CropLayerPlacement {
        CropLayerPlacement {
            x,
            y,
            scale_x: sx,
            scale_y: sy,
            rotation: 0.0,
            flip_h: false,
            flip_v: false,
        }
    }

    #[test]
    fn identity_layer_keeps_the_document_rect_and_discards_the_rest() {
        // 100x100 layer at the origin, document 60x60: the right/bottom 40 px
        // of every row are outside and must become transparent.
        let mut pixels = solid(100, 100);
        let discarded = discard_outside_document(
            &mut pixels,
            100,
            100,
            &placement(0.0, 0.0, 1.0, 1.0),
            60.0,
            60.0,
        );
        assert_eq!(discarded, 100 * 100 - 60 * 60);
        let at = |x: u32, y: u32| pixels[((y * 100 + x) * 4) as usize + 3];
        assert_eq!(at(59, 59), 255, "inside the new document survives");
        assert_eq!(at(60, 0), 0, "first column outside is cleared");
        assert_eq!(at(0, 60), 0, "first row outside is cleared");
    }

    #[test]
    fn a_layer_shifted_out_of_the_document_is_fully_discarded() {
        let mut pixels = solid(10, 10);
        let discarded = discard_outside_document(
            &mut pixels,
            10,
            10,
            &placement(100.0, 100.0, 1.0, 1.0),
            60.0,
            60.0,
        );
        assert_eq!(discarded, 100);
        assert!(pixels.iter().all(|b| *b == 0));
    }

    #[test]
    fn a_scaled_layer_keeps_only_the_visible_part() {
        // 10x10 at scale 2 covers document x/y 0..20. Pixel centre `i` lands at
        // doc 2i+1, so a 5x5 document keeps i,j in {0,1} - four pixels - and
        // discards the other 96.
        let mut pixels = solid(10, 10);
        let discarded = discard_outside_document(
            &mut pixels,
            10,
            10,
            &placement(0.0, 0.0, 2.0, 2.0),
            5.0,
            5.0,
        );
        assert_eq!(discarded, 100 - 4);
        let at = |x: u32, y: u32| pixels[((y * 10 + x) * 4) as usize + 3];
        assert_eq!(at(0, 0), 255);
        assert_eq!(at(1, 1), 255);
        assert_eq!(at(2, 0), 0, "third layer column lands at doc x=5");
    }
}
