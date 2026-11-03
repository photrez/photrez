//! The paper-preview maths: how big the page is on screen and where the
//! document lands on it.

/// A 440 px tall page keeps the paper's ratio. The document is fitted to the
/// margins, or to the fitted size scaled by the chosen percentage, and is never
/// drawn smaller than 8 px.
pub(super) struct PagePreview {
    pub(super) page_w: f32,
    pub(super) page_h: f32,
    pub(super) doc_scale: f32,
    pub(super) doc_px_w: f32,
    pub(super) doc_px_h: f32,
    pub(super) doc_x: f32,
    pub(super) doc_y: f32,
}

pub(super) fn page_preview(
    page_mm: (f64, f64),
    doc_px: (u32, u32),
    margins: f64,
    center: bool,
    scale_to_fit: bool,
    scale: u32,
) -> PagePreview {
    let (doc_w, doc_h) = doc_px;
    let page_h = 440.0_f32;
    let page_w = page_h * (page_mm.0 as f32) / (page_mm.1 as f32);
    let fit = ((page_w - 2.0 * margins as f32) / doc_w as f32)
        .min((page_h - 2.0 * margins as f32) / doc_h as f32);
    let doc_scale = if scale_to_fit {
        fit
    } else {
        fit * (scale as f32 / 100.0)
    };
    let doc_px_w = (doc_w as f32 * doc_scale).max(8.0);
    let doc_px_h = (doc_h as f32 * doc_scale).max(8.0);
    let (doc_x, doc_y) = if center {
        ((page_w - doc_px_w) / 2.0, (page_h - doc_px_h) / 2.0)
    } else {
        (margins as f32, margins as f32)
    };
    PagePreview {
        page_w,
        page_h,
        doc_scale,
        doc_px_w,
        doc_px_h,
        doc_x,
        doc_y,
    }
}

#[cfg(test)]
mod tests {
    use super::page_preview;

    /// A4 portrait is narrow for an 800x600 page, so the width is the axis that
    /// runs out first: the fitted document spans the page minus both margins.
    #[test]
    fn the_fit_uses_the_binding_axis_and_keeps_the_ratio() {
        let p = page_preview((210.0, 297.0), (800, 600), 5.0, false, true, 100);
        assert_eq!(p.page_h, 440.0);
        assert!((p.doc_px_w - (p.page_w - 10.0)).abs() < 1e-3);
        assert!((p.doc_px_h / p.doc_px_w - 600.0 / 800.0).abs() < 1e-4);
        assert_eq!(p.doc_x, 5.0);
        assert_eq!(p.doc_y, 5.0);
    }

    /// A tall document on the same paper is limited by the page height instead,
    /// and centring then splits the leftover width.
    #[test]
    fn a_tall_document_is_limited_by_height_and_centering_splits_the_rest() {
        let p = page_preview((210.0, 297.0), (200, 800), 5.0, true, true, 100);
        assert!((p.doc_px_h - (p.page_h - 10.0)).abs() < 1e-3);
        assert!((p.doc_x - (p.page_w - p.doc_px_w) / 2.0).abs() < 1e-4);
        assert!(p.doc_x > 5.0);
    }

    /// Landscape exchanges the paper's axes, and a percentage scales the fitted
    /// size rather than replacing it.
    #[test]
    fn landscape_exchanges_the_axes_and_a_percentage_scales_the_fit() {
        let portrait = page_preview((210.0, 297.0), (800, 600), 5.0, true, true, 100);
        let landscape = page_preview((297.0, 210.0), (800, 600), 5.0, true, true, 100);
        assert!(landscape.page_w > portrait.page_w);
        let half = page_preview((210.0, 297.0), (800, 600), 5.0, true, false, 50);
        assert!((half.doc_scale - portrait.doc_scale * 0.5).abs() < 1e-5);
        assert!((half.doc_px_w - portrait.doc_px_w * 0.5).abs() < 1e-3);
    }
}
