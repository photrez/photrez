// SPDX-License-Identifier: AGPL-3.0-or-later
// Duplicate-name planner pins for the engine's copy-suffix contract.

use super::*;

#[test]
fn copy_stem_strips_one_copy_suffix() {
    assert_eq!(copy_stem("Layer 1"), "Layer 1");
    assert_eq!(copy_stem("Layer 1 copy"), "Layer 1");
    assert_eq!(copy_stem("Layer 1 copy 2"), "Layer 1");
    assert_eq!(copy_stem("Layer 1 copyx"), "Layer 1 copyx");
    assert_eq!(copy_stem("Layer 1 copy 2x"), "Layer 1 copy 2x");
}

#[test]
fn next_duplicate_name_uses_copy_suffix() {
    assert_eq!(next_duplicate_name(&["Layer 1"], "Layer 1"), "Layer 1 copy");
    assert_eq!(
        next_duplicate_name(&["Layer 1", "Layer 1 copy"], "Layer 1"),
        "Layer 1 copy 2"
    );
    assert_eq!(
        next_duplicate_name(
            &["Layer 1", "Layer 1 copy", "Layer 1 copy 2"],
            "Layer 1 copy 2"
        ),
        "Layer 1 copy 3"
    );
}
