//! Empty workspace: the "Start a Photrez document" card.
//! Card x=183, not the computed 182.5: the capture snaps the border outward one pixel.
//! `empty-new-document` delegates to `dialogs.rs`; only the card is drawn here.
//! Geometry is pinned from the reference capture; this view has no box to re-derive it from.
//! Shadow, drag highlight, hover and text rasterisation are not reproduced.
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, div, px};

use crate::icons;
use crate::shot;
use crate::tokens::color;

use super::dialogs;

// ── Frame ────────────────────────────────────────────────────────────────────

/// The client area `empty-full.png` captures, and the box the card is placed
/// inside. Transparent: the shell behind it belongs to `main.rs`.
const FRAME_W: f32 = 1366.0;
const FRAME_H: f32 = 820.0;

// ── Card ─────────────────────────────────────────────────────────────────────

/// A 520px max width on the card section. The content column is 781 wide, so
/// the max width binds long before the column does.
const CARD_W: f32 = 520.0;

/// The section's own height. No height was recorded; 351.5 is what the
/// four blocks below add up to, and the bordered box that is painted is 352.
const CARD_H: f32 = 352.0;

/// **Painted** top edge. The recorded box is at `245.25`; snapping rounds down here,
/// so this one is the number the manifest already quotes.
const CARD_Y: f32 = 245.0;

/// 6px corners on the section. Measured from the corner profile: the top
/// border row runs 7..514 of a 520-wide box, i.e. inset 6 on each side.
const CARD_R: f32 = 6.0;

/// 20px horizontal padding on all four blocks - 20 off the 518px content box, leaving 478.
const PAD_X: f32 = 20.0;

// ── Block heights ────────────────────────────────────────────────────────────

/// 16 + 41 + 16 + a 1px bottom border.
const HEADER_H: f32 = 74.0;
/// 16px vertical padding.
const HEADER_PAD_Y: f32 = 16.0;
/// 12px between the header icon box and its text.
const HEADER_GAP: f32 = 12.0;
/// The header row's height: a 21px h2 line, a 2px title margin, an 18px p line.
const HEADER_ROW_H: f32 = 41.0;

/// The 36px header icon box, and the offset it takes inside the 41px row.
///
/// 36 in 41 centres at +2.5; the baseline's first `#252525` row is crop y=20,
/// which is +3. Written as a literal because GPUI would round it down.
const HEADER_ICON_BOX: f32 = 36.0;
const HEADER_ICON_OFFSET: f32 = 3.0;

/// 14 + 36 + 14.
const ACTIONS_H: f32 = 64.0;
/// 14px vertical padding.
const ACTIONS_PAD_Y: f32 = 14.0;
/// 10px gap on the actions grid.
const ACTIONS_GAP: f32 = 10.0;

/// 36px tall on both buttons.
const BUTTON_H: f32 = 36.0;
/// (478 - 10) / 2.
const BUTTON_W: f32 = 234.0;
/// 8px between a button's icon and its label.
const BUTTON_GAP: f32 = 8.0;

/// 16px on the buttons' icons.
const BUTTON_ICON: f32 = 16.0;

/// 20px - the `image-plus` glyph in the header's icon box.
const HEADER_ICON: f32 = 20.0;

/// 1 + 14 + 17 + 10 + 51 + 8 + 51 + 14.
const PRESETS_H: f32 = 166.0;
/// 14px vertical padding.
const PRESETS_PAD_Y: f32 = 14.0;
/// 10px margin under the "Quick Presets" line.
///
/// Its line box is **17**px, not the 16.5 that `11 x 1.5` suggests: 17 + 10
/// puts the first tile's top border on crop row 181, which is where the scan
/// finds it.
const PRESETS_HEADING_MB: f32 = 10.0;

/// (478 - 8) / 2.
const TILE_W: f32 = 235.0;
/// 2 borders + 16 of 8px padding + 18 + 15.
const TILE_H: f32 = 51.0;
/// 8px padding.
const TILE_PAD: f32 = 8.0;
/// 10px between a tile's icon box and its two text lines.
const TILE_GAP: f32 = 10.0;
/// 28px - the tile's icon box.
const TILE_ICON_BOX: f32 = 28.0;
/// 14px - the glyph inside it.
const TILE_ICON: f32 = 14.0;

/// 8px between the two rows of the presets grid.
const PRESET_ROW_GAP: f32 = 8.0;

/// 1 + 12 + 21 + 12.
const FOOTER_H: f32 = 46.0;
/// 12px vertical padding.
const FOOTER_PAD_Y: f32 = 12.0;

/// 6px horizontal padding on the `<kbd>`.
const KBD_PAD_X: f32 = 6.0;
/// 2px vertical padding on the `<kbd>`.
const KBD_PAD_Y: f32 = 2.0;

// ── Radii ────────────────────────────────────────────────────────────────────

/// 4px radius: the card's inner buttons and preset tiles.
const R4: f32 = 4.0;
/// 3px radius: a tile's icon box, and the `<kbd>`.
const R3: f32 = 3.0;

// ── Type ─────────────────────────────────────────────────────────────────────

/// The Inter sans font, on every run in the card. 14px semibold - the card title.
const TITLE_TEXT: f32 = 14.0;
const TITLE_LINE_H: f32 = 21.0;

/// 12px - the card's one-line description.
const DESC_TEXT: f32 = 12.0;
const DESC_LINE_H: f32 = 18.0;

/// 2px margin between the title and the description.
const DESC_MT: f32 = 2.0;

/// 13px medium - both action buttons.
const BUTTON_TEXT: f32 = 13.0;

/// 11px semibold - "Quick Presets".
const HEADING_TEXT: f32 = 11.0;
const HEADING_LINE_H: f32 = 17.0;

/// 12px medium - a tile's label.
const TILE_LABEL_TEXT: f32 = 12.0;
const TILE_LABEL_LINE_H: f32 = 18.0;

/// 10px - a tile's size tag, and the `<kbd>`.
const TAG_TEXT: f32 = 10.0;
const TAG_LINE_H: f32 = 15.0;

/// 11px - the drop prompt.
const PROMPT_TEXT: f32 = 11.0;

// ── Icons ────────────────────────────────────────────────────────────────────

/// `strokeWidth` on every icon in the card.
///
/// The recorded stroke widths are 1.5 for the header's `image-plus` and 1.75 to all four
/// others. [`icons::with_stroke`] re-strokes a baked constant on demand, so
/// the two 1.5 call sites use [`icons::IMAGE_PLUS`] directly and the four
/// 1.75 ones go through [`preset_icon`] / the baked `1.75` constants.
const STROKE_PRESET: f32 = 1.75;

/// The quick presets, in display order: label, the size tag
/// under it, and the document size the tile creates. The `x` is U+00D7 and is
/// present in the baseline - the first tile's tag scans as four ink groups at
/// crop x 69..89, 94..98, 103..123, 128..137, and the second is a 5px glyph.
const PRESETS: [(&str, &str, u32, u32); 4] = [
    ("Instagram Post", "1080 \u{00d7} 1080 px", 1080, 1080),
    ("Story / Reels", "1080 \u{00d7} 1920 px", 1080, 1920),
    ("Full HD Canvas", "1920 \u{00d7} 1080 px", 1920, 1080),
    ("A4 Print (300 DPI)", "2480 \u{00d7} 3508 px", 2480, 3508),
];

/// The document a Quick Preset creates: its label as the name and its pixel
/// size. `None` for an index the table does not have.
pub(crate) fn preset_document(index: usize) -> Option<(&'static str, u32, u32)> {
    PRESETS
        .get(index)
        .map(|&(label, _tag, width, height)| (label, width, height))
}

/// The same four entries' `icon` field, in the same order: `square`,
/// `smartphone`, `image`, `file-text`.
///
/// Only `SQUARE` is baked at [`STROKE_PRESET`]; the other three arrive at 1.5
/// or 2 because their primary consumer asked for that, so they are re-stroked
/// rather than edited.
const PRESET_ICONS: [&[u8]; 4] = [
    icons::SQUARE,
    icons::SMARTPHONE,
    icons::IMAGE,
    icons::FILE_TEXT,
];

/// The `<Icon />` for preset `index`, at the width this component asks for.
fn preset_icon(index: usize) -> &'static [u8] {
    icons::with_stroke(PRESET_ICONS[index], STROKE_PRESET)
}

// ── Render ───────────────────────────────────────────────────────────────────

/// Which arm of the rest-headed shell this shot renders. The variant selection
/// is factored out of `render()` as a pure function of the shot id so the
/// routing table is unit-testable without spinning up a GPUI tree - `render()`
/// keeps the exact two arms the baseline defines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EmptyVariant {
    /// `empty-card` / `empty-full` / any unset id - the welcome card itself.
    Card,
    /// `empty-new-document` - the New Document dialog, owned by `dialogs`.
    NewDocument,
}

/// `shot::active()` -> arm. `empty-new-document` hands off to the dialog;
/// every other id, including `None`, draws the card.
fn select(shot: Option<&str>) -> EmptyVariant {
    match shot {
        Some("empty-new-document") => EmptyVariant::NewDocument,
        _ => EmptyVariant::Card,
    }
}

/// A transparent full-viewport frame with the resting state pinned inside it.
///
/// The two arms are deliberately not symmetric:
///
/// * `empty-card` (and `empty-full`, and any unset id) draw the card.
/// * `empty-new-document` is **the New Document dialog**, not the card, and
///   `crate::dialogs` owns it - see the module docs for the measured
///   difference between the two states. Delegating means the dialog is drawn
///   once; drawing a second copy here would guarantee the two drift.
///
/// The frame is absolute so it disturbs nothing wherever appended, and the
/// caller must add it late: GPUI has no stacking index, so paint order is the
/// only stacking control. The
/// live `canvas_col_w` is the canvas column width: the default layout centres the card inside it, so the painted
/// left edge moves with the viewport and the wide-viewport dock widths (the
/// measured `CARD_X = 183`: the formula at the default 1366x820 client area).
/// `col_left` is the canvas column's left edge in window coordinates (the
/// rail width); the card's painted left edge is
/// `col_left + (col_w - CARD_W) / 2`.
pub fn render(col_left: f32, col_w: f32) -> impl IntoElement {
    // `AnyElement` rather than `Div`, because the two arms are different
    // types: the card is a concrete `Div` and `dialogs::render()` is an
    // opaque `impl IntoElement`.
    let card_x = col_left + ((col_w - CARD_W) / 2.0).max(0.0);
    let body: AnyElement = match select(shot::active()) {
        EmptyVariant::NewDocument => dialogs::render().into_any_element(),
        EmptyVariant::Card => card()
            .absolute()
            .left(px(card_x))
            .top(px(CARD_Y))
            .into_any_element(),
    };

    div()
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .w(px(FRAME_W))
        .h(px(FRAME_H))
        .child(body)
}

/// The production no-document state: the same card, centred in the canvas
/// slot and wired to the actions it advertises.
///
/// The card's slot is a centred, padded cell on the canvas fill, so the card
/// centres in the space the canvas would
/// occupy rather than at a captured coordinate. The captured [`render`] pins the
/// card because its frame is the whole 1366x820 window; here the slot is the
/// live layout and centring is the honest reproduction.
pub fn render_live(cx: &gpui_kit::Context<crate::app::App>) -> impl IntoElement {
    div()
        .id("empty-workspace")
        .flex_1()
        .min_h(px(0.0))
        .w_full()
        .flex()
        .items_center()
        .justify_center()
        .p(px(24.0))
        .bg(color::CANVAS)
        .on_drop::<crate::app::cross_doc::LayerDragPayload>(cx.listener(
            |app,
             payload: &crate::app::cross_doc::LayerDragPayload,
             window,
             cx: &mut gpui_kit::Context<crate::app::App>| {
                app.apply_layer_drag(
                    payload,
                    crate::app::cross_doc::LayerDropTarget::NewDocument,
                    window.modifiers().alt,
                    cx,
                );
            },
        ))
        .child(card::card_live(cx))
}

mod card;
#[cfg(test)]
mod tests;

use card::card;
