// SPDX-License-Identifier: AGPL-3.0-or-later
// Duplicate-name planner pins for the engine's next-free-name contract.

use super::*;

#[test]
fn base_name_strips_trailing_number() {
    assert_eq!(base_name("Layer 1"), "Layer");
    assert_eq!(base_name("Background 2"), "Background");
    assert_eq!(base_name("Layer"), "Layer");
    assert_eq!(base_name("123"), "");
}

#[test]
fn base_name_strips_nbsp_after_number() {
    // NBSP (U+00A0) counts as whitespace for this strip, so a trailing one is
    // removed before the digits.
    assert_eq!(base_name("Layer 2\u{00A0}"), "Layer");
}

#[test]
fn next_duplicate_name_sequence() {
    assert_eq!(
        next_duplicate_name(&["Layer 1", "Layer 2"], "Layer 1"),
        "Layer 3"
    );
    assert_eq!(
        next_duplicate_name(&["Layer 1", "Layer 2"], "Layer 2"),
        "Layer 3"
    );
}

#[test]
fn next_duplicate_name_trailing_garbage_counts_as_leading_digits() {
    // The pinned contract parses the suffix as leading digits: digits count and
    // trailing garbage is ignored ("2x" -> 2).
    assert_eq!(next_duplicate_name(&["Layer 2x"], "Layer"), "Layer 3");
}

#[test]
fn next_duplicate_name_accepts_a_leading_sign() {
    // A leading '+' or '-' sign is accepted.
    assert_eq!(next_duplicate_name(&["Layer +2"], "Layer"), "Layer 3");
}

#[test]
fn next_duplicate_name_huge_number_does_not_collapse() {
    // i64::parse overflows on 20-digit numbers; Rust must not silently
    // fall back to "Layer 2" like the old parse::<i64> path did. Mirrors
    // Leading-digit parse, exact here through i128 saturating.
    let dup = next_duplicate_name(&["Layer 99999999999999999999"], "Layer");
    assert_ne!(dup, "Layer 2");
    let suffix = dup.strip_prefix("Layer ").unwrap();
    assert_eq!(suffix, "100000000000000000000");
}
