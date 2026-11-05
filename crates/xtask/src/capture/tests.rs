#[cfg(windows)]
use super::assert_capture_environment;
use super::attached_catalog;
use super::bgra_to_rgba_top_down;
use super::locale_request_ok;
use super::LocaleSource;
use super::{child_args, CaptureSpec};
use std::path::PathBuf;

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

/// The attach guard follows the app's own order: override, then stored preference, then OS
/// language, then the English fallback. A blank value is unset, not a choice.
#[test]
fn attached_catalog_follows_the_apps_resolution_order() {
    assert_eq!(
        attached_catalog(Some("id"), Some("en"), "fr_FR"),
        ("id".to_string(), LocaleSource::Environment)
    );
    assert_eq!(
        attached_catalog(None, Some("id"), "fr_FR"),
        ("id".to_string(), LocaleSource::StoredSetting)
    );
    assert_eq!(
        attached_catalog(Some("   "), Some("id"), ""),
        ("id".to_string(), LocaleSource::StoredSetting)
    );
    assert_eq!(
        attached_catalog(None, None, "id_ID.UTF-8"),
        ("id".to_string(), LocaleSource::OsLanguage)
    );
    assert_eq!(
        attached_catalog(None, None, ""),
        ("en".to_string(), LocaleSource::EnglishFallback)
    );
}

/// A language the app has no catalog for renders English, so it must not be refused: the guard
/// asks what the app shows, not what the machine claims.
#[test]
fn a_language_without_a_catalog_captures_english() {
    assert_eq!(attached_catalog(None, None, "de_DE@euro").0, "en");
    assert_eq!(attached_catalog(Some("EN"), None, "id").0, "en");
    assert_eq!(attached_catalog(None, Some("en"), "id").0, "en");
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

/// The shot selects a UI state and an open path names a document; both reach
/// the child, and neither is invented when the caller leaves it out.
#[test]
fn child_args_forward_shot_and_open_only_when_given() {
    let base = CaptureSpec {
        shot: None,
        exe: None,
        out: PathBuf::from("out.png"),
        wait_ms: 30_000,
        expect_client: None,
        open: None,
    };
    assert_eq!(child_args(&base), Vec::<String>::new());
    let both = CaptureSpec {
        shot: Some("move".to_string()),
        open: Some("baseline.ptz".to_string()),
        ..base
    };
    assert_eq!(
        child_args(&both),
        vec![
            "--shot".to_string(),
            "move".to_string(),
            "--open".to_string(),
            "baseline.ptz".to_string()
        ]
    );
}

/// The reference environment: 100% scale, exact 1366x820 client.
#[cfg(windows)]
#[test]
fn capture_environment_accepts_reference_conditions() {
    assert!(assert_capture_environment(1366, 820, 96, (1366, 820)).is_ok());
}

/// An explicit expected size is what lets a run match the geometry a recorded
/// evidence set was taken at; a client that is not that size is still refused.
#[cfg(windows)]
#[test]
fn capture_environment_honours_an_explicit_client_size() {
    assert!(assert_capture_environment(1920, 1020, 96, (1920, 1020)).is_ok());
    let err = assert_capture_environment(1920, 1032, 96, (1920, 1020))
        .expect_err("a taller client than asked for must be rejected");
    assert!(err.contains("clamped"), "message names the cause: {err}");
    let scaled =
        assert_capture_environment(1920, 1020, 120, (1920, 1020)).expect_err("scaling still wins");
    assert!(
        scaled.contains("1920x1020"),
        "the asked size is named: {scaled}"
    );
}

/// A scaled display (120 dpi = 125%) must be rejected with the scale named.
#[cfg(windows)]
#[test]
fn capture_environment_rejects_scaled_display() {
    let err =
        assert_capture_environment(1366, 820, 120, (1366, 820)).expect_err("125% must be rejected");
    assert!(err.contains("125%"), "message names the scale: {err}");
}

/// A client clamped by the work area (e.g. 816 high) must be rejected with
/// the clamp named, not silently captured as a cropped page.
#[cfg(windows)]
#[test]
fn capture_environment_rejects_clamped_client() {
    let err = assert_capture_environment(1366, 816, 96, (1366, 820))
        .expect_err("clamped client must be rejected");
    assert!(err.contains("clamped"), "message names the cause: {err}");
}
