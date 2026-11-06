//! Invalid-input contracts: zero dimensions, non-finite transforms, oversize
//! canvases - each one a defined refusal, none a panic.

use super::canvas_bridge::{fit_placement, CanvasImage, FrameError, MAX_CANVAS_DIM};

#[test]
fn zero_dimension_is_refused() {
    assert_eq!(
        CanvasImage::from_rgba(0, 1, Vec::new()).err(),
        Some(FrameError::ZeroDimension)
    );
    assert_eq!(
        CanvasImage::from_rgba(1, 0, Vec::new()).err(),
        Some(FrameError::ZeroDimension)
    );
}

#[test]
fn oversize_canvas_is_refused() {
    let too_large = MAX_CANVAS_DIM + 1;
    assert_eq!(
        CanvasImage::from_rgba(too_large, 1, Vec::new()).err(),
        Some(FrameError::TooLarge {
            width: too_large,
            height: 1
        })
    );
    assert_eq!(
        CanvasImage::from_rgba(1, too_large, Vec::new()).err(),
        Some(FrameError::TooLarge {
            width: 1,
            height: too_large
        })
    );
}

#[test]
fn length_mismatch_is_refused() {
    assert_eq!(
        CanvasImage::from_rgba(2, 2, vec![0; 15]).err(),
        Some(FrameError::LengthMismatch {
            expected: 16,
            got: 15
        })
    );
}

#[test]
fn nan_or_negative_transform_places_nothing() {
    let base = (781., 668., 800., 600.);
    for broken in [
        (f32::NAN, base.1, base.2, base.3),
        (base.0, f32::INFINITY, base.2, base.3),
        (base.0, base.1, f32::NAN, base.3),
        (base.0, base.1, base.2, f32::NEG_INFINITY),
        (0., base.1, base.2, base.3),
        (-10., base.1, base.2, base.3),
        (base.0, base.1, 0., base.3),
        (base.0, base.1, base.2, -1.),
    ] {
        assert_eq!(fit_placement(broken.0, broken.1, broken.2, broken.3), None);
    }
}

#[test]
fn tiny_viewport_clamps_zoom_instead_of_overflowing() {
    // Below the 80px padding the raw ratio is negative; the 0.1 viewport
    // clamps it at 0.05, so an 800x600 page draws as 40x30, centered.
    let (x, y, w, h) = fit_placement(50., 50., 800., 600.).expect("fits");
    assert!((x - 5.).abs() < 0.01, "x: {x}");
    assert!((y - 10.).abs() < 0.01, "y: {y}");
    assert!((w - 40.).abs() < 0.01, "w: {w}");
    assert!((h - 30.).abs() < 0.01, "h: {h}");
}
