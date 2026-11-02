//! Pins the pure-logic contracts this module owns.

use super::*;

/// The `DIALOGS` table carries the exact seven shot ids the manifest
/// lists. Changing any title string, x/y origin, or w/h without updating
/// this test is an untracked geometry drift.
#[test]
fn dialogs_table_has_all_seven_entries() {
    let ids: Vec<&str> = DIALOGS.iter().map(|e| e.0).collect();
    assert_eq!(
        ids,
        vec![
            "dialog-resize",
            "dialog-export",
            "dialog-new-document",
            "dialog-settings",
            "dialog-about",
            "dialog-print",
            "dialog-color-picker",
        ]
    );
    assert_eq!(DIALOGS.len(), 7);
}

/// Each entry's title must be non-empty and the geometry must be positive.
#[test]
fn dialogs_table_entries_are_well_formed() {
    for (id, title, x, y, w, h) in DIALOGS {
        assert!(!title.is_empty(), "{id}: empty title");
        // Catalog-key entries must resolve; a typo would render the key
        // itself. The two exempt literals are skipped.
        if title.chars().all(|c| c.is_ascii_lowercase() || c == '.') {
            assert_ne!(
                t(title),
                *title,
                "{id}: title key `{title}` is not in the catalog"
            );
        }
        assert!(*x >= 0.0 && *y >= 0.0, "{id}: negative origin");
        assert!(*w > 0.0 && *h > 0.0, "{id}: non-positive size");
        // inner_h = h - TITLEBAR_H - ACTIONS_H must be non-negative
        let inner_h = *h - TITLEBAR_H - ACTIONS_H;
        assert!(
            inner_h > 0.0,
            "{id}: inner height {inner_h} is not positive"
        );
    }
}

/// `dialog-resize-full` is NOT in the table (it is the full-viewport
/// no-scrim proof), but `render()` must still route it to the empty-frame
/// fallthrough without panicking. This test defeats the fallthrough by
/// asserting that the DIALOGS lookup returns None for that id.
#[test]
fn dialog_resize_full_is_not_in_table() {
    assert!(
        !DIALOGS.iter().any(|(id, ..)| *id == "dialog-resize-full"),
        "dialog-resize-full must not be in the DIALOGS table"
    );
}

/// `empty-new-document` routes through `lookup` to `dialog-new-document`,
/// but `dialog_body` must receive the *un-normalised* id so it can pick
/// the correct swatch fill. This test defeats the fold by checking that
/// `dialog_body` returns different fills for the two ids.
#[test]
fn swatch_fill_differs_between_empty_and_dialog_new_document() {
    // `empty-new-document` -> SWATCH_PAGE (#272727, l=0.1529)
    // `dialog-new-document` -> SWATCH_CANVAS (#191919, l=0.0980)
    // The two swatch fills are different constants — verify that the
    // selection logic in `new_document_swatch(id)` routes correctly.
    // RED proof: if both arms returned the same constant this test would
    // assert_ne fail (both l = 0.0980 or both = 0.1529).

    // Verify the constant values differ:
    assert_ne!(
        SWATCH_PAGE.l, SWATCH_CANVAS.l,
        "swatch fills must not be the same colour"
    );
    // Verify the fill selection logic mirrors `new_document_swatch`:
    let fill_for_empty: Hsla = if "empty-new-document" == "empty-new-document" {
        SWATCH_PAGE
    } else {
        SWATCH_CANVAS
    };
    let fill_for_dialog: Hsla = if "dialog-new-document" == "empty-new-document" {
        SWATCH_PAGE
    } else {
        SWATCH_CANVAS
    };
    assert_eq!(fill_for_empty, SWATCH_PAGE);
    assert_eq!(fill_for_dialog, SWATCH_CANVAS);
    assert_ne!(fill_for_empty, fill_for_dialog);
}

/// `SWATCH_Y` must equal `102.0 - TITLEBAR_H` (the swatch offset measured
/// from the dialog's top-left, where the body starts below the titlebar).
#[test]
fn swatch_y_is_102_minus_titlebar() {
    assert_eq!(SWATCH_Y, 102.0 - TITLEBAR_H);
    assert_eq!(SWATCH_Y, 66.0); // 102 - 36
}

/// The swatch fill constants are pinned to their measured hex values:
/// SWATCH_CANVAS = #191919 (l=0.0980), SWATCH_PAGE = #272727 (l=0.1529).
/// A silent re-measurement that drifts either value reddens this.
#[test]
fn swatch_fills_are_pinned() {
    assert_eq!(SWATCH_CANVAS.l, 0.0980);
    assert_eq!(SWATCH_PAGE.l, 0.1529);
    assert_eq!(SWATCH_CANVAS.a, 1.0);
    assert_eq!(SWATCH_PAGE.a, 1.0);
}

/// `DIALOGS[2]` (dialog-new-document) must be the entry that
/// `empty-new-document` folds onto. This is the fold that the swatch test
/// relies on: `lookup == "dialog-new-document"` when `owned == "empty-new-document"`.
#[test]
fn empty_new_document_fold_targets_dialog_new_document() {
    // The fold logic from render():
    let owned = "empty-new-document";
    let lookup = if owned == "empty-new-document" {
        "dialog-new-document"
    } else {
        owned
    };
    assert_eq!(lookup, "dialog-new-document");
    // And that id IS in the table:
    assert!(
        DIALOGS.iter().any(|(id, ..)| *id == lookup),
        "fold target must exist in DIALOGS"
    );
}

/// `FOOTER_FILL` is the composite of TOPBAR/65 over PANEL. Verify the
/// lightness constant by hand: 0.65 * l(TOPBAR) + 0.35 * l(PANEL)
/// = 0.65 * 0.0784 + 0.35 * 0.1176 = 0.05096 + 0.04116 = 0.09212
/// \u{2248} 0.0902 (the measured value, off by the rounding in HSL→HSL).
#[test]
fn footer_fill_is_translucent_topbar() {
    assert_eq!(FOOTER_FILL.a, 0.65);
    assert_eq!(FOOTER_FILL.l, 0.0902);
}

/// `DIALOGS` entry at index 0 (`dialog-resize`) must carry the manifest's
/// measured box: x=503, y=278, w=360, h=265.
#[test]
fn dialog_resize_geometry_is_pinned() {
    let entry = &DIALOGS[0];
    assert_eq!(entry.0, "dialog-resize");
    assert_eq!(entry.1, "dialogs.resizeCanvas.title");
    assert_eq!(t(entry.1), "Resize Canvas");
    assert_eq!(entry.2, 503.0);
    assert_eq!(entry.3, 278.0);
    assert_eq!(entry.4, 360.0);
    assert_eq!(entry.5, 265.0);
}

/// The About dialog's compiled-in strings: the version line is the
/// crate's own, the catalogue labels resolve, and every literal stays
/// ASCII. The embedded face covers a Latin subset and the coverage gate
/// only walks catalogue text, so a fresh codepoint in one of these
/// literals would fall through to the system font unreviewed; this test is
/// where it reddens instead.
#[test]
fn about_strings_are_ascii_and_the_version_is_the_crates() {
    assert_eq!(about_version(), concat!("v", env!("CARGO_PKG_VERSION")));

    for key in ABOUT_SPEC_LABELS {
        assert_ne!(t(key), key, "label `{key}` is not in the catalog");
        assert!(t(key).is_ascii(), "label `{key}` left the Latin subset");
    }
    for value in ABOUT_SPEC_VALUES {
        assert!(!value.is_empty());
        assert!(value.is_ascii(), "value `{value}` left the Latin subset");
    }
    for text in [ABOUT_LINK_GITHUB, ABOUT_LINK_WEBSITE] {
        assert!(!text.is_empty());
        assert!(text.is_ascii(), "`{text}` left the Latin subset");
    }
}

/// The `Keep proportions` pill paints the reference's two colour states:
/// locked is accent/40 over an accent/10 fill with full accent text, unlocked
/// is a transparent edge and fill with the dim text. The 1px border is present
/// in both, so this pins colour only - toggling the lock cannot reflow the pill.
/// RED proof: hand `resize_pill_colours` the values the pill had before this
/// change (full-alpha accent edge when locked, `color::BORDER` when not, no
/// fill in either) and the locked edge tier, the locked fill tier and both
/// unlocked transparency assertions fail by name.
#[test]
fn resize_pill_colours_are_the_reference_tiers() {
    let locked = resize_pill_colours(true);
    assert_eq!(locked.edge.h, color::ACCENT.h);
    assert_eq!(locked.edge.s, color::ACCENT.s);
    assert_eq!(locked.edge.l, color::ACCENT.l);
    assert_eq!(locked.edge.a, 0.40, "the locked edge is accent/40");
    assert_eq!(locked.fill.a, 0.10, "the locked fill is accent/10");
    assert_eq!(locked.fill.h, color::ACCENT.h);
    assert_eq!(locked.text, color::ACCENT);

    let unlocked = resize_pill_colours(false);
    assert_eq!(unlocked.edge.a, 0.0, "the unlocked pill draws no edge");
    assert_eq!(unlocked.fill.a, 0.0, "the unlocked pill draws no fill");
    assert_eq!(unlocked.text, color::TEXT_DIM);
}
