#[cfg(windows)]
use super::assert_capture_environment;
use super::bgra_to_rgba_top_down;
use super::locale_request_ok;

/// 2x2 bottom-up BGRA buffer:
///
///   screen top-left  = Red   BGRA [  0,   0, 255, 255]
///   screen top-right = Green BGRA [  0, 255,   0, 255]
///   screen bot-left  = Blue  BGRA [255,   0,   0, 255]
///   screen bot-right = White BGRA [255, 255, 255, 255]
///
/// Memory layout (bottom-up, stride 8):
///   row 0 (bottom on screen): Blue, White
///   row 1 (top on screen):    Red, Green
#[test]
fn bgra_2x2_flip_and_swap() {
    let bottom: [u8; 8] = [255, 0, 0, 255, 255, 255, 255, 255];
    let top: [u8; 8] = [0, 0, 255, 255, 0, 255, 0, 255];
    let src: Vec<u8> = bottom.iter().chain(top.iter()).copied().collect();

    let out = bgra_to_rgba_top_down(&src, 2, 2, 8).expect("2x2 conversion");

    // Expected top-down RGBA:
    //   row 0 (top):    Red -> [255, 0, 0, 255]
    //                   Green -> [  0, 255, 0, 255]
    //   row 1 (bottom): Blue -> [  0,   0, 255, 255]
    //                   White -> [255, 255, 255, 255]
    let expected: [u8; 16] = [
        255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
    ];
    assert_eq!(out.as_slice(), expected.as_slice());
}

#[test]
fn zero_dims_rejected() {
    let r = bgra_to_rgba_top_down(&[], 0, 0, 0);
    assert!(r.is_err());
}

/// The capture locale guard: absent/blank means unset, English (any case)
/// passes, anything else is refused and named.
#[test]
fn locale_request_accepts_only_english() {
    assert!(locale_request_ok(None).is_ok());
    assert!(locale_request_ok(Some("")).is_ok());
    assert!(locale_request_ok(Some("  ")).is_ok());
    assert!(locale_request_ok(Some("en")).is_ok());
    assert!(locale_request_ok(Some(" EN ")).is_ok());
    let err = locale_request_ok(Some("id")).expect_err("id must be refused");
    assert!(err.contains("PHOTREZ_LOCALE"), "names the variable: {err}");
    assert!(err.contains("id"), "names the value: {err}");
}

#[test]
fn stride_too_small_rejected() {
    // 2 px wide but stride of only 4 bytes (need 8).
    let r = bgra_to_rgba_top_down(&[0; 8], 2, 1, 4);
    assert!(r.is_err());
}

#[test]
fn buffer_too_short_rejected() {
    // stride 8, 2 rows need 16 bytes, give only 8.
    let r = bgra_to_rgba_top_down(&[0; 8], 2, 2, 8);
    assert!(r.is_err());
}

#[test]
fn single_row_identity() {
    // 1x1, bottom-up == top-down for a single row; BGRA -> RGBA.
    let src: [u8; 4] = [10, 20, 30, 40]; // B=10 G=20 R=30 A=40
    let out = bgra_to_rgba_top_down(&src, 1, 1, 4).expect("1x1");
    assert_eq!(out, vec![30, 20, 10, 40]); // R=30 G=20 B=10 A=40
}

/// The reference environment: 100% scale, exact 1366x820 client.
#[cfg(windows)]
#[test]
fn capture_environment_accepts_reference_conditions() {
    assert!(assert_capture_environment(1366, 820, 96).is_ok());
}

/// A scaled display (120 dpi = 125%) must be rejected with the scale named.
#[cfg(windows)]
#[test]
fn capture_environment_rejects_scaled_display() {
    let err = assert_capture_environment(1366, 820, 120).expect_err("125% must be rejected");
    assert!(err.contains("125%"), "message names the scale: {err}");
}

/// A client clamped by the work area (e.g. 816 high) must be rejected with
/// the clamp named, not silently captured as a cropped page.
#[cfg(windows)]
#[test]
fn capture_environment_rejects_clamped_client() {
    let err =
        assert_capture_environment(1366, 816, 96).expect_err("clamped client must be rejected");
    assert!(err.contains("clamped"), "message names the cause: {err}");
}
