// SPDX-License-Identifier: AGPL-3.0-or-later
// Selection-arm state contract: every selection command bumps the document version
// (an accepted transition) but leaves the history entry count unchanged, and the
// canonical shadow selection tracks the engine selection.
use crate::document::content::{PtzDocumentContent, SelectionShape, SelectionState};
use crate::engine::DocumentCore;
use crate::wire::command::Command;

use super::commands::{env, layer};

fn sel_doc() -> PtzDocumentContent {
    PtzDocumentContent {
        id: "sel-doc".into(),
        name: "S".into(),
        width: 200.0,
        height: 150.0,
        layers: vec![],
        selection: None,
    }
}

#[test]
fn selection_arms_bump_version_without_history_entries_and_track_shadow() {
    let mut e = DocumentCore::new();
    e.seed_ptz(sel_doc());
    e.seed_layers(vec![layer()], 0);

    let entries0 = e.entries.len();
    let v0 = e.version();

    // SetSelection -> version +1, no new entry, engine + shadow selection set.
    e.apply(env(Command::SetSelection {
        selection: SelectionState {
            x: 10.0,
            y: 20.0,
            width: 30.0,
            height: 40.0,
            angle: 5.0,
            shape: Some(SelectionShape::Ellipse),
            inverted: Some(false),
        },
    }))
    .unwrap();
    assert_eq!(e.version(), v0 + 1, "setSelection bumps version");
    assert_eq!(
        e.entries.len(),
        entries0,
        "setSelection commits no history entry"
    );
    assert_eq!(e.selection().unwrap().x, 10.0);
    assert_eq!(
        e.ptz_content().unwrap().selection.as_ref().unwrap().x,
        10.0,
        "shadow selection tracks engine after set"
    );

    // ClearSelection -> version +1, no new entry, both cleared.
    e.apply(env(Command::ClearSelection)).unwrap();
    assert_eq!(e.version(), v0 + 2, "clearSelection bumps version");
    assert_eq!(
        e.entries.len(),
        entries0,
        "clearSelection commits no history entry"
    );
    assert!(e.selection().is_none());
    assert!(
        e.ptz_content().unwrap().selection.is_none(),
        "shadow cleared too"
    );

    // SelectAll -> full-canvas rect from seeded doc dims, no new entry.
    e.apply(env(Command::SelectAll)).unwrap();
    assert_eq!(e.version(), v0 + 3, "selectAll bumps version");
    assert_eq!(
        e.entries.len(),
        entries0,
        "selectAll commits no history entry"
    );
    let all = e.selection().unwrap();
    assert_eq!(all.x, 0.0);
    assert_eq!(all.y, 0.0);
    assert_eq!(all.width, 200.0);
    assert_eq!(all.height, 150.0);

    // InvertSelection (selection present) -> toggles inverted, no new entry.
    e.apply(env(Command::InvertSelection)).unwrap();
    assert_eq!(e.version(), v0 + 4, "invertSelection bumps version");
    assert_eq!(
        e.entries.len(),
        entries0,
        "invertSelection commits no history entry"
    );
    assert_eq!(e.selection().unwrap().inverted, Some(true));
}

#[test]
fn invert_without_selection_falls_back_to_full_canvas() {
    // No selection + seeded canonical -> InvertSelection falls back to the SAME
    // full-canvas rect the SelectAll arm builds, exactly (no-op fallback removed
    // to mirror the host op). Still no history entry, still version +1.
    let mut e = DocumentCore::new();
    e.seed_ptz(sel_doc());
    e.seed_layers(vec![layer()], 0);
    let entries0 = e.entries.len();
    let v0 = e.version();

    e.apply(env(Command::InvertSelection)).unwrap();
    let inv = e
        .selection()
        .expect("fallback produced a full-canvas selection");
    assert_eq!(
        e.entries.len(),
        entries0,
        "invert fallback commits no entry"
    );
    assert_eq!(e.version(), v0 + 1, "invert fallback bumps version");

    // Control: SelectAll on the same seeded dims.
    let mut ref_e = DocumentCore::new();
    ref_e.seed_ptz(sel_doc());
    ref_e.seed_layers(vec![layer()], 0);
    ref_e.apply(env(Command::SelectAll)).unwrap();
    let all = ref_e
        .selection()
        .expect("selectAll produced a full-canvas selection");

    // InvertSelection-without-selection must equal SelectAll EXACTLY.
    assert_eq!(inv.x, all.x);
    assert_eq!(inv.y, all.y);
    assert_eq!(inv.width, all.width);
    assert_eq!(inv.height, all.height);
    assert_eq!(inv.angle, all.angle);
    assert_eq!(inv.shape, all.shape);
    assert_eq!(inv.inverted, all.inverted);
    assert_eq!(
        inv.inverted, None,
        "fallback matches SelectAll (inverted: None)"
    );

    // Existing selection -> toggles inverted (unchanged behavior).
    let mut t = DocumentCore::new();
    t.seed_ptz(sel_doc());
    t.seed_layers(vec![layer()], 0);
    t.apply(env(Command::SetSelection {
        selection: SelectionState {
            x: 1.0,
            y: 2.0,
            width: 3.0,
            height: 4.0,
            angle: 0.0,
            shape: None,
            inverted: Some(false),
        },
    }))
    .unwrap();
    t.apply(env(Command::InvertSelection)).unwrap();
    assert_eq!(
        t.selection().unwrap().inverted,
        Some(true),
        "existing selection toggles inverted"
    );

    // No canonical shadow -> falls back to select-all which requires seeded
    // canonical, so it rejects with the same E_INVALID-shaped error as SelectAll.
    let mut n = DocumentCore::new();
    n.seed_layers(vec![layer()], 0);
    let r = n.apply(env(Command::InvertSelection));
    assert!(r.is_err());
    assert_eq!(r.unwrap_err().code, "E_INVALID");
    assert!(n.selection().is_none(), "no mutation on rejection");
}

#[test]
fn select_all_without_seeded_canonical_rejects_e_invalid() {
    let mut e = DocumentCore::new();
    e.seed_layers(vec![layer()], 0);
    // No canonical shadow seeded -> host must seed before select-all.
    let res = e.apply(env(Command::SelectAll));
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().code, "E_INVALID");
    assert!(e.selection().is_none(), "no mutation on rejection");
}

#[test]
fn set_selection_rejects_nonfinite_or_negative_dims_with_e_invalid() {
    let mut e = DocumentCore::new();
    e.seed_layers(vec![layer()], 0);
    let bad = |sel: SelectionState| {
        let mut e2 = DocumentCore::new();
        e2.seed_layers(vec![layer()], 0);
        let r = e2.apply(env(Command::SetSelection { selection: sel }));
        assert!(r.is_err(), "non-finite/negative geometry must be rejected");
        assert_eq!(r.unwrap_err().code, "E_INVALID");
    };
    bad(SelectionState {
        x: 0.0,
        y: 0.0,
        width: f64::INFINITY,
        height: 1.0,
        angle: 0.0,
        shape: None,
        inverted: None,
    });
    bad(SelectionState {
        x: 0.0,
        y: 0.0,
        width: -1.0,
        height: 1.0,
        angle: 0.0,
        shape: None,
        inverted: None,
    });
    bad(SelectionState {
        x: 0.0,
        y: 0.0,
        width: 1.0,
        height: f64::NAN,
        angle: 0.0,
        shape: None,
        inverted: None,
    });
    // The arm rejects NaN angle too - it is stricter than 0.1.0's selection
    // creation, which validated none of the five numeric fields.
    bad(SelectionState {
        x: 0.0,
        y: 0.0,
        width: 1.0,
        height: 1.0,
        angle: f64::NAN,
        shape: None,
        inverted: None,
    });
}
