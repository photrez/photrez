use super::*;

/// All three severities map to `color::BORDER` — the unlayered
/// `* { border-color }` rule overrides every `border-*` utility.
///
/// RED proof: change any arm of `Severity::border()` to return a different
/// colour and this fails. GREEN: all arms return `BORDER`.
#[test]
fn severity_border_map_is_border() {
    assert_eq!(Severity::Info.border(), color::BORDER);
    assert_eq!(Severity::Warn.border(), color::BORDER);
    assert_eq!(Severity::Error.border(), color::BORDER);
}

/// The six captured labels are the capture tooling's own literals, verbatim.
///
/// RED proof: change any `MSG_*` constant to a different string and this
/// fails. GREEN: every label matches the baseline spec.
#[test]
fn captured_labels_match_baseline() {
    assert_eq!(MSG_INFO, "Layer merged");
    assert_eq!(MSG_WARN, "Could not write the file");
    assert_eq!(MSG_ERROR, "Export failed");
    assert_eq!(MSG_STACK_INFO, "Opened");
    assert_eq!(MSG_STACK_WARN, "Copied");
    assert_eq!(MSG_STACK_ERROR, "Disk full");
}

/// `toast-stacked` height = 3 × TOAST_H + 2 × TOAST_GAP = 126.25.
///
/// Manifest pins the stack at 126.25px. This verifies the constants
/// reproduce that arithmetic exactly.
///
/// RED proof: change `TOAST_H` or `TOAST_GAP` to any other value and the
/// product diverges from 126.25. GREEN: the sum closes on the manifest.
#[test]
fn stack_height_is_three_toasts_two_gaps() {
    let stacked = 3.0 * TOAST_H + 2.0 * TOAST_GAP;
    assert!(
        (stacked - 126.25).abs() < 0.001,
        "stacked height {} != 126.25",
        stacked
    );
    // And the single height matches the manifest's 36.75.
    assert!((TOAST_H - 36.75).abs() < 0.001);
}

/// `toast-info` x + width = 1342.0 = 1366 - RIGHT_INSET.
#[test]
fn right_inset_pins_every_shot() {
    // All four shots share the same right edge (1342.0 = 1366 - 24).
    // The code positions from the right edge via RIGHT_INSET + flex,
    // so this verifies the constant the layout depends on.
    assert!((RIGHT_INSET - 24.0).abs() < 0.001);
}

/// The dispatch table maps shot ids to the correct severity + message.
///
/// RED proof: swap any arm (e.g. `"toast-warn"` → return `Severity::Info`)
/// and the corresponding assertion fails. GREEN: each shot selects its
/// documented stack.
#[test]
fn messages_for_matches_shot() {
    // Default / unset → single info toast.
    let def = messages_for(None);
    assert_eq!(def.len(), 1);
    assert_eq!(def[0].0, Severity::Info);
    assert_eq!(def[0].1, MSG_INFO);

    // toast-warn → single warn.
    let warn = messages_for(Some("toast-warn"));
    assert_eq!(warn.len(), 1);
    assert_eq!(warn[0].0, Severity::Warn);
    assert_eq!(warn[0].1, MSG_WARN);

    // toast-error → single error.
    let err = messages_for(Some("toast-error"));
    assert_eq!(err.len(), 1);
    assert_eq!(err[0].0, Severity::Error);
    assert_eq!(err[0].1, MSG_ERROR);

    // toast-stacked → three, in capture order (info/warn/error).
    let stack = messages_for(Some("toast-stacked"));
    assert_eq!(stack.len(), 3);
    assert_eq!(stack[0].0, Severity::Info);
    assert_eq!(stack[0].1, MSG_STACK_INFO);
    assert_eq!(stack[1].0, Severity::Warn);
    assert_eq!(stack[1].1, MSG_STACK_WARN);
    assert_eq!(stack[2].0, Severity::Error);
    assert_eq!(stack[2].1, MSG_STACK_ERROR);

    // Unknown shot → default (same as None).
    let unknown = messages_for(Some("toast-bogus"));
    assert_eq!(unknown.len(), 1);
    assert_eq!(unknown[0].0, Severity::Info);
}
