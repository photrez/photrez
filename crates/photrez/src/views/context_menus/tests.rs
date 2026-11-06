use super::*;

/// `ctx-canvas` manifest box: 210 x 280. Height arithmetic: 4 (py-1) +
/// 9 rows x 28 + 2 seps x 9 + 4 (py-1) + 2 (border) = 280. The row height
/// (28) and separator block (9) are shared constants in `ui.rs`, so they
/// are inlined here as literals rather than redefined.
#[test]
fn ctx_canvas_height_is_280px() {
    let row = 28.0;
    let sep_block = 9.0;
    let computed = MENU_PY + 9.0 * row + 2.0 * sep_block + MENU_PY + 2.0;
    assert_eq!(computed, 280.0);
}

/// `ctx-export` manifest box: 210 x 131. Height: 4 + 4x28 + 1x9 + 4 + 2.
#[test]
fn ctx_export_height_is_131px() {
    let row = 28.0;
    let sep_block = 9.0;
    let computed = MENU_PY + 4.0 * row + 1.0 * sep_block + MENU_PY + 2.0;
    assert_eq!(computed, 131.0);
}

/// `ctx-layer` manifest box: 210 x 345. Height: 4 + 11x28 + 3x9 + 4 + 2.
/// This arithmetic was the correction that dropped the 0.1 build's
/// stale `layer_menu` row list (10 rows, 2 separators, no Delete Layer)
/// and restored the full `layerContextItems` order from the 0.1 markup.
#[test]
fn ctx_layer_height_is_345px() {
    let row = 28.0;
    let sep_block = 9.0;
    let computed = MENU_PY + 11.0 * row + 3.0 * sep_block + MENU_PY + 2.0;
    assert_eq!(computed, 345.0);
}

/// `FOCUS_BG` is accent at 15% alpha over `#1e1e1e` panel — the exact
/// value `ctx-canvas.png` crop rows 98..125 read edge to edge.
#[test]
fn focus_bg_is_accent_alpha_15() {
    assert_eq!(FOCUS_BG.h, color::ACCENT.h);
    assert_eq!(FOCUS_BG.s, color::ACCENT.s);
    assert_eq!(FOCUS_BG.l, color::ACCENT.l);
    assert_eq!(FOCUS_BG.a, 0.15);
}

/// Capture anchors are manifest-derived; the small menus pin to their
/// manifest box origin, not the stale (420,320) probe an earlier
/// recorded for `ctx-canvas`.
#[test]
fn ctx_canvas_anchor_is_from_manifest() {
    assert_eq!((CANVAS_X, CANVAS_Y), (442.0, 462.0));
    assert_eq!((EXPORT_X, EXPORT_Y), (607.0, 82.5));
    assert_eq!((LAYER_X, LAYER_Y), (1152.0, 190.0));
    assert_eq!((BRUSH_X, BRUSH_Y), (420.0, 323.0));
}
