//! The right-click context menus: appearance only.
//!
//! Five surfaces live here: the shared shell every menu except the brush one
//! renders through, the brush panel (`ctx-brush`), the canvas menu
//! (`ctx-canvas`), the export button's item list (`ctx-export`) and the
//! layer row's item list (`ctx-layer`).
//!
//! # Geometry: pointer-anchored, so only the size is fixed
//!
//! These open wherever the pointer is (the shell clamps the anchor
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
//! Every figure was read back out of `tests/baseline/ctx-*.png` with a
//! throwaway crop-analysis script (crop-relative coordinates — the three
//! small PNGs are element crops, so crop 0,0 is the menu's own top-left; only
//! `ctx-brush` is a viewport shot). The row arithmetic then checks itself:
//! 4px vertical padding + N rows x 28px + S separators x (8px margin + 1px
//! rule) + 4px vertical padding,
//! plus the 2px border, lands exactly on each manifest height — 280 for canvas
//! (9 rows, 2 separators), 131 for export (4 rows, 1 separator), 345 for layer
//! (11 rows, 3 separators).
//!
//! The canvas origin in the capture (`ctx-canvas`) is (442,462), read from
//! `manifest.json`; an earlier probe recorded the same crop at
//! (420,320), and this file uses the manifest coordinates so the
//! overlay aligns to the baseline box.
//!
//! # Why 210px and not `menus.rs`'s 224px
//!
//! Different surfaces, different constants. The title-bar popups are 224px
//! wide; these three are 210px.
//! Both were measured rather than
//! derived, and they disagree — so this file keeps its own `CTX_W` and shares
//! nothing width-like with `menus.rs`, which another agent owns. If a shared
//! menu-chrome helper ever emerges, `menu_shell` / `menu_row` /
//! `menu_separator` below are the three functions to compare against it: the
//! item styling is *not* known to match `menus.rs` (a different surface), so
//! consolidation needs a pixel check first, not just a
//! signature match.
//!
//! # What the shared shell contributes
//!
//! One shell (210px minimum width, clipped, 6px corners, a 1px divider edge,
//! panel fill, 4px vertical padding, plus the
//! `0 8px 24px rgba(0,0,0,0.45)` shadow), one row shape (at least 28px tall,
//! full width, label left, optional shortcut right at 10px in the dim text
//! colour, 24px horizontal padding, 12px labels, 24px gap between label and
//! shortcut, disabled rows at 35% opacity), one separator (1px rule with 4px
//! margins above and below, full-bleed — it reads edge to edge).
//! Factored once here as `menu_shell`, `menu_row`, `menu_separator`;
//! each menu below is just an entry list through them. Stacking order,
//! hover and keyboard-focus styling, open animation and the selection callback
//! are
//! interaction and are not rendered — with one measured exception, below.
//!
//! Hover tints are omitted, matching the capture's static-only
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
//!   Separators at crop y=93, 158, 307. The danger style needs no branch: a
//!   danger entry is full-brightness text at rest (accent only arrives on
//!   hover or keyboard focus), so Delete Layer renders as a normal disabled row.
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
//! The shell focuses the first enabled entry on open, and the capture's
//! applies its 15% accent focus wash there: crop rows 98..125
//! of `ctx-canvas` ("Select All") and 5..32 of `ctx-layer` ("New Layer") read
//! `#3d2e23` edge to edge — accent at 15% over panel, to the pixel. The label
//! on those rows stays `#cacaca` (the keyboard-focus text rule for
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
//! for the stacking order this view cannot express (no `z_index` in GPUI).
//! Unknown
//! ids (including unset) render `ctx-canvas`.
//!
//! # Known gaps, specific to this file
//!
//! * The brush size slider's wedge (`clip-path: polygon(...)` over the track)
//!   renders as the rectangle `ui::slider` already draws — that gap is owned
//!   by `ui.rs` (documented on `SliderKind::BrushSize`) and inherited here,
//!   not re-explained.
//! * The brush panel's invisible range fields (overlaid, fully transparent)
//!   take pointer input only; nothing visible, nothing rendered.
//! * The brush grid is two columns in source and two flex rows here —
//!   identical pixels for the fixed 2x2, but a third preset row would need a
//!   third flex row rather than flowing automatically.
//! * No explicit line height anywhere: the small menus centre 12px/10px
//!   runs in fixed 28px rows so line height cannot move them, but the brush
//!   panel stacks 10px labels and 4px-padded buttons in flow, where GPUI's
//!   default line height decides the exact block heights. Transcribed
//!   faithfully (12px panel inset, 12px and 4px gaps, 4px button padding);
//!   if a later capture
//!   disagrees by a pixel per block, line height is the first suspect.
//! * The brush panel's large drop shadow is two `BoxShadow`s, transcribed
//!   stop; whether the blur matches the capture's rasteriser is unmeasured.
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

/// The minimum width of every pointer-anchored menu, measured, not
/// shared with `menus.rs`'s 224px. See the module docs on why the two widths
/// are different constants from different components.
pub const CTX_W: f32 = 210.0;

/// The overlay frame both menus render inside: the client area
/// (`main.rs`'s `CLIENT_W`/`CLIENT_H`), local because those are private and
/// this is the only overlay that positions from viewport coordinates.
const FRAME_W: f32 = 1366.0;
const FRAME_H: f32 = 820.0;

/// 6px — the menu shell's corner radius.
const MENU_R: f32 = 6.0;

/// 4px — the shell's own vertical inset; rows start at crop y=4 in every
/// small-menu baseline. The 12px row inset, 28px row min-height, 24px gap,
/// 4px separator margin and the two text sizes all live in `ui::menu_row` /
/// `ui::menu_separator` (measured 12 / 28 / 24 / 4 / 12 / 10 there), so they
/// are not redeclared here; only the separator's 4px margin is restated as a
/// literal in the height tests.
const MENU_PY: f32 = 4.0;

/// The captured keyboard-focus background: accent at 15% over the panel
/// colour reads `#3d2e23` edge to edge on the focused row, so the
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

/// Capture origins. The small menus open at the pointer (the shell
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

/// 200px — the brush panel's own width (its side borders probe at
/// exactly 199px apart in `ctx-brush.png`).
const BRUSH_W: f32 = 200.0;

/// 12px on the panel, 12px between its sections.
const BRUSH_PAD: f32 = 12.0;
const BRUSH_GAP: f32 = 12.0;

/// 4px inside a slider section and inside the preset grid.
const BLOCK_GAP: f32 = 4.0;

/// 14px — the slider's centred positioning box;
/// the track/thumb inside it come from [`ui::slider`].
const SLIDER_H: f32 = 14.0;

/// 3px radius, 8px / 4px padding, 10px text — preset and Reset buttons.
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
    // late paints on top, which is the only stacking-order equivalent available.
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

/// The menu shell: 210px, 6px corners, 1px [`color::BORDER`] edge
/// (every border paints `#333333` — see `tokens.rs`), [`color::PANEL`]
/// fill, 4px vertical padding, clipped, under the `0 8px 24px rgba(0,0,0,0.45)`
/// shadow.
/// One helper for all three small menus — the entries differ, the chrome
/// does not.
fn menu_shell(entries: Vec<Div>) -> Div {
    shell(
        entries
            .into_iter()
            .map(IntoElement::into_any_element)
            .collect(),
    )
}

/// The live context menus share this shell: 210 px, 6 px corners, 1 px border,
/// 4px vertical padding, clipped and shadowed. One geometry source for the
/// captured specimen
/// and the production menu.
pub(crate) fn shell(entries: Vec<gpui_kit::AnyElement>) -> Div {
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

/// The shell's `0 8px 24px rgba(0,0,0,0.45)` drop shadow. Local: no
/// token carries this shadow and `menus.rs` is another agent's file, so it
/// is spelled out here; consolidate if the two menus ever share a helper.
fn menu_shadow() -> Vec<BoxShadow> {
    vec![BoxShadow::new(px(0.0), px(8.0), hsla(0.0, 0.0, 0.0, 0.45)).blur_radius(px(24.0))]
}

/// The canvas menu: 9 entries, 2 separators, 210x280. Disabled rows
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

/// The export button's items: 4 entries,
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

/// The layer row's items, single-layer branch:
/// 11 entries, 3 separators, 210x345. "New Layer" carries the captured focus
/// tint. Hide/Lock assume the fresh-layer defaults (visible, unlocked) —
/// the pixels read the same either way, only the words differ.
///
/// # Deviation from the capture
///
/// The captured `ctx-layer` specimen listed 10 rows + 2 separators and
/// omitted the `Delete Layer` row the layer menu produces as its last entry
/// (danger, `Delete` shortcut, disabled for a
/// fresh background-only layer). That 10-row list does not square against
/// the `ctx-layer` manifest box height of 345 (which requires 11 rows +
/// 3 separators), so this view restores the full 14-entry list at the
/// 11-row / 3-separator layout, matching the reference markup's order and
/// state. The focus-background trace and the two top-group row orders follow
/// that markup.
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

/// The brush panel at brush defaults: 200px panel, 6px corners, 12px
/// padding, 12px gaps, a large drop shadow. Three slider sections, the 2x2
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

/// One slider block: 10px medium dim caption, 4px gap, then the 14px
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

/// Two columns with a 4px gap as two flex rows — identical pixels for the fixed 2x2
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

/// 3px radius, 1px border, field fill, 8px / 4px padding, 10px left-aligned
/// full-brightness text — the resting branch. The capture
/// shows no preset selected, so the accent-at-40% selected branch is not drawn.
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
/// left-alignment rule, so it centres.
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

/// The large drop shadow: `0 20px 25px -5px rgb(0 0 0 / 0.1),
/// 0 8px 10px -6px rgb(0 0 0 / 0.1)`. Transcribed
/// stop for stop; whether the blur matches the capture's rasteriser is
/// unmeasured —
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
mod tests;
