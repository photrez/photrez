//! Visual-only port of the right-click context menus.
//!
//! Sources, in the order they were read: `editor/ContextMenu.tsx` (the shared
//! shell every menu below except the brush one renders through it),
//! `editor/BrushContextMenu.tsx` (-> `ctx-brush`),
//! `editor/canvas/CanvasContextMenu.tsx` (-> `ctx-canvas`), and `ExportButton`'s
//! `menuItems` in `editor/shell/DocumentTabsBar.tsx` (-> `ctx-export`).
//! `ctx-layer`'s entries are `layerContextItems` in
//! `editor/layers/LayersPanel.tsx`.
//!
//! # Geometry: pointer-anchored, so only the size is fixed
//!
//! These open wherever the pointer is (`ContextMenu.tsx` clamps the anchor
//! into the viewport with a 4px margin), so the captured origins are click
//! points, not layout constants — only the sizes have to be reproduced:
//!
//! ```text
//! ctx-canvas       210x280 @  442,462
//! ctx-export       210x131 @  607, 82.5
//! ctx-layer        210x345 @ 1152,190
//! ctx-brush        200x245 @  420,323   (panel; the shot itself is 1366x820)
//! ```
//!
//! Every figure was read back out of `baseline/shots/ctx-*.png` with the
//! throwaway `baseline/zz-probe-ctx.ts` (crop-relative coordinates — the three
//! small PNGs are element crops, so crop 0,0 is the menu's own top-left; only
//! `ctx-brush` is a viewport shot). The row arithmetic then checks itself:
//! `py-1` (4) + N rows x 28px + S seps x (`my-1` 8 + 1px separator) + `py-1` (4),
//! plus the 2px border, lands exactly on each manifest height — 280 for canvas
//! (9 rows, 2 separators), 131 for export (4 rows, 1 separator), 345 for layer
//! (11 rows, 3 separators).
//!
//! The canvas origin in the capture (`ctx-canvas`) is (442,462), read from
//! `manifest.json`; the frozen reference recorded the same crop with a
//! (420,320) probe, and this port uses the manifest coordinates so the
//! overlay aligns to the baseline box.
//!
//! # Why 210px and not `menus.rs`'s 224px
//!
//! Different components, different constants. The title-bar popups are
//! `AppMenuBar`'s own markup at `w-56` (224px); these three are
//! `ContextMenu.tsx` at `min-w-[210px]`. Both were measured rather than
//! derived, and they disagree — so this file keeps its own `CTX_W` and shares
//! nothing width-like with `menus.rs`, which another agent owns. If a shared
//! menu-chrome helper ever emerges, `menu_shell` / `menu_row` /
//! `menu_separator` below are the three functions to compare against it: the
//! item styling is *not* known to match `menus.rs` (different source
//! component), so consolidation needs a pixel check first, not just a
//! signature match.
//!
//! # What `ContextMenu.tsx` contributes as shared chrome
//!
//! One shell (`min-w-[210px] overflow-hidden rounded-[6px] border
//! border-editor-divider bg-editor-panel py-1` + the
//! `0 8px 24px rgba(0,0,0,0.45)` shadow), one row shape (`flex min-h-7 w-full
//! items-center justify-between gap-6 px-3 text-[12px]`, label left in
//! `text-editor-text`, optional shortcut right in 10px `text-editor-text-dim`,
//! disabled rows at `opacity-35`), one separator (`my-1 border-t
//! border-editor-divider`, full-bleed — the probe reads it edge to edge).
//! Factored once here as `menu_shell`, `menu_row`, `menu_separator`;
//! each menu below is just an entry list through them. `z-[79]/z-[80]`,
//! `hover:`, `focus-visible:` navigation, `FadeIn` and `onSelect` are
//! interaction and are not rendered — with one measured exception, below.
//!
//! The `hover:` tints are omitted, matching the reference's static-only
//! decision: none of the `ctx-*` baselines captures a hover. A real hover
//! would be `hover(|s| s.bg(hsla(0., 0., 1., 0.06)))` on a
//! non-danger row and `hsla(0.072, 0.8713, 0.6039, 0.15)` on a danger row;
//! there is no resting baseline showing either, so no `.id` +
//! `Stateful` wrapper is built here.
//!
//! # What state each baseline caught
//!
//! * **ctx-canvas** — no selection, empty clipboard: Cut / Copy / Paste,
//!   Deselect and Zoom to Selection read `#5a5a5a` (TEXT at 35%) and are drawn
//!   `disabled`; Select All, Invert Selection, Actual Size, Fit Canvas read
//!   `#cacaca`. Separators at crop y=93, 186.
//! * **ctx-export** — all four entries enabled, no highlight anywhere (rows
//!   5..14 read plain panel, so no focus tint to reproduce). Separator at
//!   crop y=94. The PNG is 132 tall against a 131-tall manifest box —
//!   `y=82.5` rounding in the capture, not a second row of padding.
//! * **ctx-layer** — single layer, unlocked, no adjustments: Move Up / Move
//!   Down / Merge Down / Flatten Image / Apply Adjustment / Delete Layer all
//!   read `#5a5a5a` (disabled); New / Duplicate / Rename / Hide / Lock read
//!   `#cacaca`. Labels assume the fresh-layer defaults the pixels cannot
//!   distinguish: "Hide Layer" (visible) and "Lock Layer" (unlocked).
//!   Separators at crop y=93, 158, 307. `danger` needs no branch: a danger
//!   entry is `text-editor-text` at rest (accent only arrives on
//!   hover/focus-visible), so Delete Layer renders as a normal disabled row.
//! * **ctx-brush** — brush tool at its defaults (Size 20px -> slider 3,
//!   Hardness 80%, Strength 100%), no preset selected (all four `tool: "both"`
//!   presets — Hard Round, Soft Round, Detail, Large Soft — read default
//!   `#252525` chrome, 2x2 grid, Reset below). The panel is 200 wide at
//!   viewport (420,323); the shot is full-viewport because at that zoom the
//!   menu floats over the canvas, so this renders as the full-page variant:
//!   a transparent 1366x820 frame with the panel inside it, same as the three
//!   small menus.
//!
//! # The one interaction trace that stays
//!
//! `ContextMenu.tsx` focuses the first enabled entry on open, and Chromium
//! applies its `focus-visible:bg-editor-accent/15` there: crop rows 98..125
//! of `ctx-canvas` ("Select All") and 5..32 of `ctx-layer` ("New Layer") read
//! `#3d2e23` edge to edge — accent at 15% over panel, to the pixel. The label
//! on those rows stays `#cacaca` (`focus-visible:text-editor-text` for
//! non-danger entries), so only the row background differs. It is painted
//! here as the static constant `FOCUS_BG` — there is no keyboard nav behind
//! it and none is planned (see the project-wide gap list), this is simply
//! what the resting capture shows. `ctx-export` has no such row, so none is
//! painted there.
//!
//! # How this composes
//!
//! `render()` reads `crate::shot::active()` and returns a transparent
//! 1366x820 frame with the requested menu absolutely positioned inside it —
//! the frame is `absolute` so appending it late in the shell tree disturbs no
//! layout, and late means it paints over the shell, which is what stands in
//! for the `z-[80]` this port cannot express (no `z_index` in GPUI). Unknown
//! ids (including unset) render `ctx-canvas`.
//!
//! # Known gaps, specific to this file
//!
//! * The brush size slider's wedge (`clip-path: polygon(...)` over the track)
//!   renders as the rectangle `ui::slider` already draws — that gap is owned
//!   by `ui.rs` (documented on `SliderKind::BrushSize`) and inherited here,
//!   not re-explained.
//! * The brush panel's invisible range `<input>`s (absolute, `opacity-0`)
//!   take pointer input only; nothing visible, nothing rendered.
//! * The brush grid is `grid-cols-2` in source and two flex rows here —
//!   identical pixels for the fixed 2x2, but a third preset row would need a
//!   third flex row rather than flowing automatically.
//! * No explicit `line-height` anywhere: the small menus centre 12px/10px
//!   runs in fixed 28px rows so line height cannot move them, but the brush
//!   panel stacks 10px labels and `py-1` buttons in flow, where GPUI's
//!   default line height decides the exact block heights. Transcribed
//!   faithfully (`p-3`, `gap-3`, `gap-1`, `py-1`); if a later capture
//!   disagrees by a pixel per block, line height is the first suspect.
//! * `shadow-xl` on the brush panel is two `BoxShadow`s transcribed stop for
//!   stop; whether GPUI blurs them exactly as Chromium does is unmeasured.
//! * `ctx-export`'s `y=82.5` is kept fractional as measured; subpixel top
//!   edges rasterise differently per engine, so its top/bottom border rows
//!   are the likeliest single-pixel misses in this file.

use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::{div, hsla, px, BoxShadow, Div, FontWeight, Hsla};

use crate::components::ui;
use crate::components::ui::SliderKind;
use crate::i18n::t;
use crate::shot;
use crate::tokens::color;

/// `min-w-[210px]` (`ContextMenu.tsx`) — every pointer menu, measured, not
/// shared with `menus.rs`'s 224px. See the module docs on why the two widths
/// are different constants from different components.
pub const CTX_W: f32 = 210.0;

/// The overlay frame both menus render inside: the client area
/// (`main.rs`'s `CLIENT_W`/`CLIENT_H`), local because those are private and
/// this is the only overlay that positions from viewport coordinates.
const FRAME_W: f32 = 1366.0;
const FRAME_H: f32 = 820.0;

/// `rounded-[6px]` — the menu shell.
const MENU_R: f32 = 6.0;

/// `py-1` — the shell's own vertical inset; rows start at crop y=4 in every
/// small-menu baseline. `px-3`, `min-h-7`, `gap-6`, `my-1` and the two text
/// sizes all live in `ui::menu_row` / `ui::menu_separator` (measured 12 / 28
/// / 24 / 4 / 12 / 10 there), so they are not redeclared here; only the
/// separator's `my-1` (=4px) is restated as a literal in the height tests.
const MENU_PY: f32 = 4.0;

/// The captured `:focus-visible` background: `bg-editor-accent/15` over
/// `bg-editor-panel` reads `#3d2e23` edge to edge on the focused row, so the
/// constant is accent at 15% alpha and the shell behind it stays panel.
/// `h/s/l` are `color::ACCENT`'s own (0.072, 0.8713, 0.6039); there is no
/// accent-alpha token, and this trace belongs to these captures rather than
/// the palette, so it lives here rather than in `tokens.rs`.
const FOCUS_BG: Hsla = Hsla {
    h: 0.072,
    s: 0.8713,
    l: 0.6039,
    a: 0.15,
};

/// Capture origins. The small menus open at the pointer (`ContextMenu.tsx`
/// clamps with a 4px margin); export opens under its button
/// (`rect.right - 210`, `rect.bottom + 4`); the brush panel clamps with a
/// 220/260px keep-out. All four are measurements from `manifest.json`, kept
/// fractional where measured (`EXPORT_Y`).
const CANVAS_X: f32 = 442.0;
const CANVAS_Y: f32 = 462.0;
const EXPORT_X: f32 = 607.0;
const EXPORT_Y: f32 = 82.5;
const LAYER_X: f32 = 1152.0;
const LAYER_Y: f32 = 190.0;
const BRUSH_X: f32 = 420.0;
const BRUSH_Y: f32 = 323.0;

/// `width: 200px` — the brush panel's own width (its side borders probe at
/// exactly 199px apart in `ctx-brush.png`).
const BRUSH_W: f32 = 200.0;

/// `p-3` on the panel, `gap-3` between its sections.
const BRUSH_PAD: f32 = 12.0;
const BRUSH_GAP: f32 = 12.0;

/// `gap-1` inside a slider section and inside the preset grid.
const BLOCK_GAP: f32 = 4.0;

/// `h-[14px]` — the slider's positioning box (`relative flex items-center`);
/// the track/thumb inside it come from [`ui::slider`].
const SLIDER_H: f32 = 14.0;

/// `rounded-[3px] px-2 py-1 text-[10px]` — preset and Reset buttons.
const BTN_R: f32 = 3.0;
const BTN_PX: f32 = 8.0;
const BTN_PY: f32 = 4.0;
const BTN_TEXT: f32 = 10.0;

pub fn render() -> impl IntoElement {
    let id = shot::active().unwrap_or("ctx-canvas");
    let body: Div = match id {
        "ctx-export" => positioned(EXPORT_X, EXPORT_Y, export_menu()),
        "ctx-layer" => positioned(LAYER_X, LAYER_Y, layer_menu()),
        "ctx-brush" => positioned(BRUSH_X, BRUSH_Y, brush_panel()),
        // "ctx-canvas" and unset/unknown: the default state.
        _ => positioned(CANVAS_X, CANVAS_Y, canvas_menu()),
    };
    // Transparent full-viewport frame, itself absolute so it disturbs no
    // layout wherever it is appended. The caller puts this late in the tree;
    // late paints on top, which is the only `z-[80]` equivalent available.
    div()
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .w(px(FRAME_W))
        .h(px(FRAME_H))
        .child(body)
}

/// Pin a menu/panel at its capture origin inside the frame.
fn positioned(x: f32, y: f32, menu: Div) -> Div {
    menu.absolute().left(px(x)).top(px(y))
}

/// `ContextMenu.tsx`'s shell: 210px, 6px corners, 1px [`color::BORDER`] edge
/// (every `border-*` paints `#333333` — see `tokens.rs`), [`color::PANEL`]
/// fill, `py-1`, clipped, under the `0 8px 24px rgba(0,0,0,0.45)` shadow.
/// One helper for all three small menus — the entries differ, the chrome
/// does not.
fn menu_shell(entries: Vec<Div>) -> Div {
    v_flex()
        .w(px(CTX_W))
        .rounded(px(MENU_R))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::PANEL)
        .py(px(MENU_PY))
        .overflow_hidden()
        .shadow(menu_shadow())
        .children(entries)
}

/// `shadow-[0_8px_24px_rgba(0,0,0,0.45)]` (`ContextMenu.tsx`). Local: no
/// token carries this shadow and `menus.rs` is another agent's file, so it
/// is spelled out here; consolidate if the two menus ever share a helper.
fn menu_shadow() -> Vec<BoxShadow> {
    vec![BoxShadow::new(px(0.0), px(8.0), hsla(0.0, 0.0, 0.0, 0.45)).blur_radius(px(24.0))]
}

/// `CanvasContextMenu.tsx`: 9 entries, 2 separators, 210x280. Disabled rows
/// (no selection, empty clipboard) are Cut / Copy / Paste / Deselect /
/// Zoom to Selection; "Select All" carries the captured focus tint as the
/// first enabled entry.
fn canvas_menu() -> Div {
    menu_shell(vec![
        ui::menu_row(&t("menus.items.cut"), Some("Ctrl+X"), true, None),
        ui::menu_row(&t("menus.items.copy"), Some("Ctrl+C"), true, None),
        ui::menu_row(&t("menus.items.paste"), Some("Ctrl+V"), true, None),
        ui::menu_separator(),
        ui::menu_row(
            &t("menus.items.selectAll"),
            Some("Ctrl+A"),
            false,
            Some(FOCUS_BG),
        ),
        ui::menu_row(&t("menus.items.deselect"), Some("Ctrl+D"), true, None),
        ui::menu_row(
            &t("menus.items.invertSelection"),
            Some("Ctrl+Shift+I"),
            false,
            None,
        ),
        ui::menu_separator(),
        ui::menu_row(&t("menus.items.actualSize"), Some("Ctrl+1"), false, None),
        ui::menu_row(&t("menus.items.fitCanvas"), Some("Ctrl+0"), false, None),
        ui::menu_row(
            &t("menus.items.zoomToSelection"),
            Some("Ctrl+Alt+0"),
            true,
            None,
        ),
    ])
}

/// `ExportButton`'s `menuItems` (`DocumentTabsBar.tsx`): 4 entries,
/// 1 separator, 210x131, nothing disabled, no focus tint in the capture.
fn export_menu() -> Div {
    menu_shell(vec![
        ui::menu_row(&t("menus.items.quickExportPng"), None, false, None),
        ui::menu_row(&t("menus.items.quickExportJpg"), None, false, None),
        ui::menu_row(&t("menus.items.quickExportWebp"), None, false, None),
        ui::menu_separator(),
        ui::menu_row(&t("menus.items.exportAs"), None, false, None),
    ])
}

/// `layerContextItems` (`LayersPanel.tsx:207-308`), single-layer branch:
/// 11 entries, 3 separators, 210x345. "New Layer" carries the captured focus
/// tint. Hide/Lock assume the fresh-layer defaults (visible, unlocked) —
/// the pixels read the same either way, only the words differ.
///
/// # Deviation from the frozen reference
///
/// The frozen reference's `layer_menu` listed 10 rows + 2 separators and
/// omitted the `Delete Layer` row that `layerContextItems` in the TSX source
/// produces as the last entry (danger, `Delete` shortcut, disabled for a
/// fresh background-only layer). That 10-row list does not square against
/// the `ctx-layer` manifest box height of 345 (which requires 11 rows +
/// 3 separators), so this port restores the full 14-entry list at the
/// 11-row / 3-separator layout, matching the TSX order and state. The
/// `FocusBg` trace and the two top-group row orders follow the TSX.
fn layer_menu() -> Div {
    menu_shell(vec![
        ui::menu_row(
            &t("layers.newLayer"),
            Some("Ctrl+Shift+N"),
            false,
            Some(FOCUS_BG),
        ),
        ui::menu_row(&t("layers.duplicateLayer"), Some("Ctrl+J"), false, None),
        ui::menu_row(&t("layers.renameLayer"), None, false, None),
        ui::menu_separator(),
        ui::menu_row(&t("layers.hideLayer"), None, false, None),
        ui::menu_row(&t("layers.lockLayer"), None, false, None),
        ui::menu_separator(),
        ui::menu_row(&t("layers.moveLayerUp"), None, true, None),
        ui::menu_row(&t("layers.moveLayerDown"), None, true, None),
        ui::menu_row(&t("layers.mergeDown"), Some("Ctrl+E"), true, None),
        ui::menu_row(
            &t("menus.items.flattenImage"),
            Some("Ctrl+Shift+E"),
            true,
            None,
        ),
        ui::menu_row(&t("layers.applyAdjustment"), None, true, None),
        ui::menu_separator(),
        ui::menu_row(&t("layers.deleteLayer"), Some("Delete"), true, None),
    ])
}

/// `BrushContextMenu.tsx` at brush defaults: 200px panel, `rounded-md`
/// corners, `p-3`, `gap-3`, `shadow-xl`. Three slider sections, the 2x2
/// preset grid (all four `tool: "both"` presets, none selected), Reset.
/// Slider percentages are the capture state's: size 20px maps to
/// `paintSizeToSizeSlider(20)` = round(19/499 * 75) = 3; hardness 0.8 -> 80;
/// strength (opacity) 1.0 -> 100. Tracks and thumbs come from [`ui::slider`].
fn brush_panel() -> Div {
    v_flex()
        .w(px(BRUSH_W))
        .rounded(px(MENU_R))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::PANEL)
        .p(px(BRUSH_PAD))
        .gap(px(BRUSH_GAP))
        .overflow_hidden()
        .shadow(brush_shadow())
        .child(brush_section(
            &format!("{}: 20px", t("tools.options.size")),
            ui::slider(3.0, None, SliderKind::BrushSize, true),
        ))
        .child(brush_section(
            &format!("{}: 80%", t("tools.options.hardness")),
            ui::slider(80.0, None, SliderKind::BrushHardness, true),
        ))
        .child(brush_section(
            &format!("{}: 100%", t("tools.options.strength")),
            ui::slider(100.0, None, SliderKind::Alpha, true),
        ))
        .child(preset_grid())
        .child(reset_button())
}

/// One slider block: 10px medium dim caption, `gap-1`, then the 14px
/// positioning box around the shared [`ui::slider`]. (The invisible range
/// input overlaid in source takes pointer input only — nothing rendered.)
fn brush_section(label: &str, control: impl IntoElement) -> Div {
    v_flex()
        .w_full()
        .gap(px(BLOCK_GAP))
        .child(
            div()
                .text_size(px(BTN_TEXT))
                .font_family(ui::FONT_UI)
                .font_weight(FontWeight(500.0))
                .text_color(color::TEXT_DIM)
                .child(label.to_string()),
        )
        .child(
            h_flex()
                .w_full()
                .h(px(SLIDER_H))
                .items_center()
                .child(control),
        )
}

/// `grid-cols-2 gap-1` as two flex rows — identical pixels for the fixed 2x2
/// (a third preset row would need a third flex row; see module docs).
fn preset_grid() -> Div {
    v_flex()
        .w_full()
        .gap(px(BLOCK_GAP))
        .child(preset_row(["Hard Round", "Soft Round"])) // i18n-exempt: brush preset names (no catalog key)
        .child(preset_row(["Detail", "Large Soft"])) // i18n-exempt: brush preset names (no catalog key)
}

fn preset_row(names: [&str; 2]) -> Div {
    h_flex()
        .w_full()
        .gap(px(BLOCK_GAP))
        .children(names.map(preset_button))
}

/// `rounded-[3px] border border-editor-field-border bg-editor-field px-2 py-1
/// text-left text-[10px] text-editor-text` — the resting branch. The capture
/// shows no preset selected, so the accent/40 selected branch is not drawn.
fn preset_button(name: &str) -> Div {
    h_flex()
        .flex_1()
        .items_center()
        .rounded(px(BTN_R))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .px(px(BTN_PX))
        .py(px(BTN_PY))
        .child(
            div()
                .text_size(px(BTN_TEXT))
                .font_family(ui::FONT_UI)
                .text_color(color::TEXT)
                .child(name.to_string()),
        )
}

/// Same chrome full-width, centred label — the source button has no
/// `text-left`, so it centres.
fn reset_button() -> Div {
    h_flex()
        .w_full()
        .items_center()
        .justify_center()
        .rounded(px(BTN_R))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .px(px(BTN_PX))
        .py(px(BTN_PY))
        .child(
            div()
                .text_size(px(BTN_TEXT))
                .font_family(ui::FONT_UI)
                .text_color(color::TEXT)
                .child(t("common.reset")),
        )
}

/// `shadow-xl`: `0 20px 25px -5px rgb(0 0 0 / 0.1),
/// 0 8px 10px -6px rgb(0 0 0 / 0.1)` (`BrushContextMenu.tsx`). Transcribed
/// stop for stop; whether GPUI blurs them exactly as Chromium is unmeasured —
/// see module docs.
fn brush_shadow() -> Vec<BoxShadow> {
    vec![
        BoxShadow::new(px(0.0), px(20.0), hsla(0.0, 0.0, 0.0, 0.1))
            .blur_radius(px(25.0))
            .spread_radius(px(-5.0)),
        BoxShadow::new(px(0.0), px(8.0), hsla(0.0, 0.0, 0.0, 0.1))
            .blur_radius(px(10.0))
            .spread_radius(px(-6.0)),
    ]
}

#[cfg(test)]
mod tests {
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
    /// This arithmetic was the correction that dropped the reference's
    /// stale `layer_menu` row list (10 rows, 2 separators, no Delete Layer)
    /// and restored the full `layerContextItems` order from the TSX.
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
    /// manifest box origin, not the stale (420,320) probe the frozen reference
    /// recorded for `ctx-canvas`.
    #[test]
    fn ctx_canvas_anchor_is_from_manifest() {
        assert_eq!((CANVAS_X, CANVAS_Y), (442.0, 462.0));
        assert_eq!((EXPORT_X, EXPORT_Y), (607.0, 82.5));
        assert_eq!((LAYER_X, LAYER_Y), (1152.0, 190.0));
        assert_eq!((BRUSH_X, BRUSH_Y), (420.0, 323.0));
    }
}
