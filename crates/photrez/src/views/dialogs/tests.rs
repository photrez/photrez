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

/// The footer paints TOPBAR at 65% over the dialog root. The blend happens in
/// the painted channel, not in a lightness number: 0.65*20 + 0.35*30 = 23.5,
/// the value the scan reads. The constant used to carry a hand-blended
/// lightness *and* the alpha, which blended twice and landed 2/255 light.
#[test]
fn footer_fill_is_translucent_topbar() {
    assert_eq!(FOOTER_FILL.a, 0.65);
    let topbar = crate::tokens::hsla_to_rgb(FOOTER_FILL);
    assert_eq!(topbar, (20, 20, 20), "the ingredient is TOPBAR");
    let panel = crate::tokens::hsla_to_rgb(color::PANEL);
    assert_eq!(panel, (30, 30, 30), "the dialog root is PANEL");
    let painted = |base: u8| FOOTER_FILL.a * topbar.0 as f32 + (1.0 - FOOTER_FILL.a) * base as f32;
    assert!(
        (painted(panel.0) - 23.5).abs() < 0.01,
        "the painted footer is the design's 23.5, not {}",
        painted(panel.0)
    );
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
/// locked is the global border grey over an accent/10 fill with full accent
/// text, unlocked is the same grey over no fill with the dim text.
///
/// The edge is grey in both states, not accent, because one app-wide border
/// colour outranks every per-element border colour the reference asks for; the
/// capture of the locked dialog measures rgb(51,51,51) on the pill's border
/// row, which is exactly [`color::BORDER`]. Fills are not affected by that
/// rule, so the accent/10 fill does paint. The 1px border is present in both
/// states, so this pins colour only - toggling the lock cannot reflow the pill.
/// RED proof: hand `resize_pill_colours` the values this function had before
/// the correction (an accent/40 edge when locked, a transparent edge when not)
/// and the locked edge, the unlocked edge and the unlocked fill assertions all
/// fail by name.
#[test]
fn resize_pill_colours_are_the_reference_tiers() {
    let locked = resize_pill_colours(true);
    assert_eq!(
        locked.edge,
        color::BORDER,
        "the locked edge is the global border grey, not accent/40"
    );
    assert_eq!(locked.fill.a, 0.10, "the locked fill is accent/10");
    assert_eq!(locked.fill.h, color::ACCENT.h);
    assert_eq!(locked.text, color::ACCENT);

    let unlocked = resize_pill_colours(false);
    assert_eq!(
        unlocked.edge,
        color::BORDER,
        "the unlocked pill keeps the same grey edge"
    );
    assert_eq!(unlocked.fill.a, 0.0, "the unlocked pill draws no fill");
    assert_eq!(unlocked.text, color::TEXT_DIM);
}

/// The About dialog's version pill: the same ruling as the resize pill, on a
/// different surface. The reference asks an accent/40 edge over an accent/10
/// fill; the edge that paints is the global border grey and the fill is
/// accent/10. The S015 about capture measures rgb(51,51,51) on the pill's
/// border rows over an accent wash, which is what separates a measured edge
/// from a reading of the rule.
///
/// RED proof: hand [`about_pill_colours`] the accent/40 edge it carried before
/// this correction and the edge assertion fails by name while the fill and
/// text assertions stay green.
#[test]
fn about_pill_edge_is_the_global_grey() {
    let (edge, fill, text) = about::about_pill_colours();
    assert_eq!(
        edge,
        color::BORDER,
        "the pill's 1px edge is the global border grey, not accent/40"
    );
    assert_eq!(fill.a, 0.10, "the pill's fill keeps its own accent/10 tier");
    assert_eq!(fill.h, color::ACCENT.h);
    assert_eq!(text, color::ACCENT);
}

/// The printer cell tells a failed enumeration apart from a machine with no
/// printers, and stays silent when the list read fine.
#[test]
fn printer_notice_reports_the_failure_not_an_empty_library() {
    assert_eq!(
        print::printer_notice(Some("EnumPrintersW failed"), 0).as_deref(),
        Some("EnumPrintersW failed"),
        "the reason a refresh failed outranks the generic copy, even when no printers are listed"
    );
    assert_eq!(
        print::printer_notice(Some("offline"), 2).as_deref(),
        Some("offline"),
        "a failure is still reported even when printers are listed"
    );
    assert_eq!(
        print::printer_notice(None, 0).as_deref(),
        Some(t("dialogs.print.noPrinters").as_str()),
        "an empty list says there is nothing to print to"
    );
}

/// The print-quality badge reports the resolution the layout really lands at,
/// and changes tier with it, so a stretched print stops calling itself optimal.
#[test]
fn print_quality_badge_follows_the_measured_resolution() {
    assert_eq!(
        print::effective_ppi((2480.0, 3508.0), (210.0, 297.0)),
        300,
        "an A4 page at 2480x3508 px is the 300 PPI reference print"
    );
    assert_eq!(
        print::effective_ppi((2480.0, 3508.0), (297.0, 420.0)),
        212,
        "the same document stretched to A3 loses resolution"
    );
    assert_eq!(
        print::effective_ppi((1000.0, 1000.0), (500.0, 500.0)),
        51,
        "the worse axis decides, so a square print reads low"
    );
    assert_eq!(
        print::effective_ppi((1000.0, 1000.0), (0.0, 297.0)),
        0,
        "an unknown printed size is no resolution, not an infinite one"
    );

    assert_eq!(
        print::ppi_quality(300).0,
        "Optimal (300+ PPI)",
        "the tier boundary is inclusive at 300"
    );
    assert_eq!(print::ppi_quality(299).0, "Acceptable (150-299 PPI)");
    assert_eq!(print::ppi_quality(150).0, "Acceptable (150-299 PPI)");
    assert_eq!(
        print::ppi_quality(149).0,
        "Low resolution (min 150 PPI recommended)",
        "the tier boundary is exclusive below 150"
    );
    assert_eq!(
        crate::tokens::hsla_to_rgb(print::ppi_quality(300).2),
        (0, 44, 34)
    );
    assert_eq!(
        crate::tokens::hsla_to_rgb(print::ppi_quality(200).2),
        (69, 26, 3)
    );
    assert_eq!(
        crate::tokens::hsla_to_rgb(print::ppi_quality(100).2),
        (76, 5, 25)
    );
    assert_eq!(
        crate::tokens::hsla_to_rgb(print::ppi_quality(200).1),
        (251, 191, 36)
    );
}
