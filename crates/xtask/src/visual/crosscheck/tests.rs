use super::*;
use crate::visual::pngio::RgbaImage;

fn luma(px: [u8; 4]) -> f64 {
    0.2126 * f64::from(px[0]) + 0.7152 * f64::from(px[1]) + 0.0722 * f64::from(px[2])
}

/// Pin the rail-box framing so a future fixture recapture reddens this
/// test instead of silently drifting the oracle.
/// (a) strip height is 820 and the crop is exactly 750 rows (same as baseline);
/// (b) content-registration probe: first button ink at baseline rows 19..33
///     col 26 must also be ink in the cropped port at rows 19..33 col 26
///     (port row 65 = cropped row 19 = baseline row 19 = first button ink);
/// (c) evidence: titlebar logo fragment at port rows ~12..32 (above the
///     rail top at 46), rail icons at +46 offset.
#[test]
fn frozen_rail_box_reddens_on_framing_drift() {
    let base = env!("CARGO_MANIFEST_DIR");
    let dir = format!("{base}/tests/fixtures");
    let port = RgbaImage::load(std::path::Path::new(&(dir.clone() + "/gpui-strip.png")))
        .expect("frozen gpui-strip.png must be present");
    let baseline = RgbaImage::load(std::path::Path::new(&(dir + "/left-tool-rail.png")))
        .expect("frozen left-tool-rail.png must be present");

    // (a) strip height 820, baseline height 750, crop 750
    assert_eq!(port.height, 820, "port strip must be 52x820");
    assert_eq!(baseline.height, 750, "baseline rail must be 52x750");
    let cropped = crop_rail(&port).expect("crop_rail must succeed");
    assert_eq!(
        cropped.height, FROZEN_RAIL_H,
        "cropped port must be 750 rows (rail-to-rail)"
    );

    // (b) content-registration probe: first button ink at row 19..33 col 26
    // in both images. Ink threshold: luma > 33 (rail bg luma 27 + floor 6).
    let bg_luma = 27.0;
    let ink_floor = 6.0;
    let ink_at =
        |img: &RgbaImage, y: u32, x: u32| -> bool { luma(img.pixel(x, y)) - bg_luma > ink_floor };
    // Baseline first button: rows 19..33 col 26
    assert!(
        ink_at(&baseline, 19, 26),
        "baseline row 19 col 26 must be ink"
    );
    assert!(
        ink_at(&baseline, 33, 26),
        "baseline row 33 col 26 must be ink"
    );
    // Cropped port row 19 = original row 65 (= 19 + FROZEN_RAIL_TOP)
    assert_eq!(FROZEN_RAIL_TOP, 46, "rail top must remain 46");
    assert!(
        ink_at(&cropped, 19, 26),
        "cropped port row 19 col 26 must be ink (registration probe)"
    );
    assert!(
        ink_at(&cropped, 33, 26),
        "cropped port row 33 col 26 must be ink (registration probe)"
    );
}

/// Falsifiability proof: flipping one pixel in the port strip changes the
/// shape numbers enough to trip the assertions. Without this the test
/// suite could pass vacuously against a hardcoded oracle.
#[test]
fn shape_assertions_red_when_port_pixel_flipped() {
    let base = env!("CARGO_MANIFEST_DIR");
    let dir = format!("{base}/tests/fixtures");
    let port = RgbaImage::load(std::path::Path::new(&(dir.clone() + "/gpui-strip.png")))
        .expect("frozen gpui-strip.png must be present");

    let mut data = port.data.clone();
    // Flip a pixel in the rail content zone (row 100, col 26 - inside
    // the first button area, far from the titlebar fragment).
    let idx = ((100 * port.width + 26) as usize) * 4;
    data[idx] = data[idx].wrapping_add(64); // shift R channel
    let flipped = super::super::pngio::RgbaImage {
        width: port.width,
        height: port.height,
        data,
    };
    let temp = std::env::temp_dir().join("gpui-strip-flipped.png");
    flipped.save(&temp).expect("save flipped fixture");
    let baseline =
        super::super::pngio::RgbaImage::load(std::path::Path::new(&(dir + "/left-tool-rail.png")))
            .unwrap();
    let cropped = crop_rail(&flipped).expect("crop_rail must succeed");
    let reports = super::super::shapediff::compare_anchored(&baseline, &cropped, &shape_regions());
    // The flipped pixel must appear in the tools region histogram.
    let tools = reports.first().expect("tools region");
    eprintln!(
        "falsifiability: tools histogram after flip = {hist:?}",
        hist = &tools.histogram
    );
    // The original oracle numbers for tools/port (recorded):
    //   bg 18486, active 1557, stroke 757, orange 0, white 0
    // After flipping one pixel, at least one class count must change.
    let port_bg = tools
        .histogram
        .iter()
        .find(|h| h.class == "bg")
        .map(|h| h.port)
        .unwrap_or(0);
    let port_active = tools
        .histogram
        .iter()
        .find(|h| h.class == "active")
        .map(|h| h.port)
        .unwrap_or(0);
    let port_stroke = tools
        .histogram
        .iter()
        .find(|h| h.class == "stroke")
        .map(|h| h.port)
        .unwrap_or(0);
    let port_orange = tools
        .histogram
        .iter()
        .find(|h| h.class == "orange")
        .map(|h| h.port)
        .unwrap_or(0);
    let port_white = tools
        .histogram
        .iter()
        .find(|h| h.class == "white")
        .map(|h| h.port)
        .unwrap_or(0);
    let any_changed = port_bg != 18486
        || port_active != 1557
        || port_stroke != 757
        || port_orange != 0
        || port_white != 0;
    assert!(
        any_changed,
        "flipping one port pixel did not change any tools histogram count \
         (assertions may be vacuous)"
    );
    let _ = std::fs::remove_file(&temp);
}
