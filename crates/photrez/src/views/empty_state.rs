//! The empty workspace: the no-document
//! state, i.e. the "Start a Photrez document" card.
//!
//! Everything that needs an event loop - the two handlers, the
//! drag-and-drop seeding, the workspace-full toast - is gone; what is here is
//! the resting pixels and nothing else.
//!
//! # Where it sits, and the arithmetic that is *not* to be re-derived
//!
//! ```text
//!   empty-card            520x352 @  182,245   (manifest; this file uses 183)
//!   empty-new-document    780x498 @  293,161   <- dialog-new-document's rect
//!   empty-full           1366x820 @    0, 0   <- the whole window, chrome on
//! ```
//!
//! **The card is centred in the content column, not in the window.** The
//! obvious reading of "fill the cell, centred on both axes, 24px padding" is "middle
//! of the client area", and that would put it at `(1366-520)/2 = 423`. The
//! baseline says `182.5`. The reason is that this component is not the whole
//! client area: it is one cell of the main row, the column between the 52px
//! tool
//! rail and the 272px inspector, which `manifest.json` pins as
//! `shell-option-bar` = 781 wide at x=52. With 24px padding on each side that
//! cell's content box is 733 wide at x=76, and `76 + (733-520)/2 = 182.5`.
//! Vertically it is the same story against the main row's 750 (46..796): 24px padding
//! leaves 702 at y=70, and `70 + (702-351.5)/2 = 245.25`.
//!
//! So the recorded box really is `182.5, 245.25, 520 x 351.5`, and the manifest
//! records exactly that. What this file uses is **where the border lands**,
//! which is one pixel further right and none further down:
//!
//! ```text
//!   manifest box          182.5 .. 702.5   x   245.25 .. 596.75
//!   border pixels         183 .. 702       x   245 .. 596
//! ```
//!
//! The recorded capture snaps a border edge outward to whole device pixels: in `empty-card.png` the left border column is crop
//! x=1 and the right is crop x=520, with crop (0,0) = viewport (182,245) - a
//! crop origin confirmed by cross-correlating the card PNG against
//! `empty-full.png` (mean luma delta 0.0000). The comparison crops the
//! view at 182.5 truncated to 182 and compares 521 columns, so a card
//! placed at 182 would sit one pixel left of the baseline for its whole
//! width; `CARD_X = 183` lines the borders up.
//!
//! **Why the number is stated rather than computed.** Nothing here can
//! re-derive 183 from the layout: this view has no 24px-padded viewport box
//! to centre inside, only a frame. Recomputing `(781-520)/2 + 52` here would
//! give 182.5 again and lose the snap, which is exactly the mistake the
//! constant exists to prevent.
//!
//! # `empty-new-document` is the dialog, not the card
//!
//! The shot id is in this file's vocabulary, but what it captures is the
//! **New Document dialog** raised by the card's second button - same
//! `[data-dialog-kind='new-document']` selector as `dialog-new-document`, and
//! the same 780x498 at (293,161).
//!
//! The two PNGs are **not** byte-identical, and the difference is worth
//! writing down before anyone reproduces the dialog twice. Measured over the
//! 780x498 crop:
//!
//! | where | how much |
//! |---|---|
//! | crop x 190..343, y 102..244 (a 154x144 block) | 20 949 px differ, deltas 21..60 |
//! | crop rows 0..2, 496..497 and columns 0..2, 779 | 52 px, deltas 1..9 (dither) |
//! | everywhere else | identical |
//!
//! Only 19 pixels anywhere exceed a delta of 60, all of them inside that one
//! block. So the dialog's chrome is the same picture in both captures and the
//! block is the dialog's **canvas-preview swatch**: `#272727` when the dialog
//! is opened from the empty state, `#191919` when it is opened with a document
//! already loaded (`manifest.json` marks `dialog-new-document` `needsDoc:
//! true`). The rest of the delta is sub-quantisation noise on the 1px
//! backdrop columns, which differ because the backdrop does.
//!
//! `dialogs.rs` owns that swatch and every other pixel of the dialog, so
//! `render()` **delegates** `empty-new-document` rather than drawing a second
//! copy. Only the card is reproduced here.
//!
//! # What the chrome around the card does
//!
//! `empty-full.png` is the same card in the same place with the shell around
//! it, and the shell is unchanged by the card being up: `#141414` title bar
//! over y 0..45, `#0f0f0f` canvas over y 46..795, `#1e1e1e` status bar from
//! y=797 (a 1px `#333333` rule at 796). The rail draws, the tab strip and
//! option bar do not - there is no document - and nothing here has to
//! reproduce any of it. `render()` returns a transparent 1366x820 frame with
//! the card pinned inside, the same shape `context_menus.rs` uses.
//!
//! # Internal geometry (crop coordinates; viewport = crop + 182, +245)
//!
//! Every block height below is the *measured* one, and the four of them sum to
//! the card's 350px of content exactly - 74 + 64 + 166 + 46 = 350 - which is the
//! check that the model is right rather than merely plausible:
//!
//! ```text
//! block    rows   height  how it adds up
//! header     1..74    74  16 top+bottom padding + 41 row + 16 + 1 bottom border
//! actions   75..138   64  14 top+bottom padding + 36 button + 14
//! presets 139..304  166  1 top border + 14 + 17 + 10 under-heading margin + 51 + 8 row gap + 51 + 14
//! footer   305..350   46  1 top border + 12 top+bottom padding + 21 + 12
//! ```
//!
//! The 41 and the 51 are the interesting ones, because both are content
//! heights that come out of line boxes:
//!
//! * header 41 = 21 (a 14px h2 line) + 2 (the title's 2px top margin) + 18 (a 12px p line).
//!   The icon box is 36 in a 41 row, i.e. centred at +2.5 - and the baseline
//!   puts it at **+3** (`#252525` first appears on crop row 20).
//! * tile 51 = 2 borders + 16 (8px padding top and bottom) + 18 (a 12px label) + 15
//!   (a 10px tag). Its 28px icon box is centred in the inner 33, i.e.
//!   +2.5 - and the baseline puts it at **+2** (`#1e1e1e` rows 192..219).
//!
//! The two are the same half-pixel and the recorded rows round them **differently**,
//! so both offsets are written as literals. A line-height of 1.5 is what makes
//! 21 / 18 / 15 integers at all; the one exception is the 11px heading's
//! 16.5, which the capture spends as **17** (the "Quick Presets" line box is crop
//! 154..170, and 17 + 10 of heading margin puts the first tile on crop row
//! 181 exactly).
//!
//! Columns: 20px of horizontal padding either side of the 518px content box, so 478 of
//! usable width. The actions grid has a 10px gap -> 234 + 234; the presets
//! grid an 8px gap -> 235 + 235. Both are stated rather than flexed,
//! because the measured numbers are exact and a `flex_1` split would only
//! re-derive them with a rounding step in the way.
//!
//! # Colours
//!
//! Every value is a [`crate::tokens::color`] constant; none is a literal. The
//! three the capture pins hardest:
//!
//! | what | measured | token |
//! |---|---|---|
//! | card / tiles' icon-box fill | `#1e1e1e` | [`color::PANEL`] |
//! | field, buttons, tiles, kbd | `#252525` | [`color::FIELD`] |
//! | **every** border in the card | `#333333` | [`color::BORDER`] |
//!
//! The third is not a translation of the divider token.
//! `tokens.rs` documents why: a global border colour outranks every per-element
//! border the card asks for, so all nine rules here -
//! the card, the header's bottom border, the presets' top border, the footer's
//! top border, the second action button, four preset tiles and the key hint -
//! paint `#333333`. The scan confirms it: not one `#2e2e2e` row anywhere in
//! the crop.
//!
//! The accent button is the only saturated fill in the card: `#f28e42` =
//! [`color::ACCENT`], edge to edge across 234x36 with a 4px radius.
//!
//! # Not expressible here
//!
//! * **The drop shadow is close but not the same curve.**
//!   The recorded shadow spec `0 12px 32px rgba(0,0,0,0.8)` is transcribed as a `BoxShadow`
//!   with the same offset and blur radius, but the recorded shadow coverage at
//!   the box edge is about 0.4 (measured: `#090909` against a
//!   `#0f0f0f` canvas, 3px out, on both sides), not the 0.8 the alpha says - a
//!   blurred shadow whose rect is offset and finite never reaches full
//!   coverage at its own edge. GPUI's falloff will differ again. This is
//!   visible in `empty-full.png` and **outside** `empty-card.png`, which is
//!   cropped tight to the card and so contains no shadow at all.
//! * **The drag-highlight state never arms.** The drag-over branch adds a 2px
//!   accent ring, a 2px ring offset and a 2px upward shift, and swaps the
//!   header icon's fill to the accent; the ring and the shift are gaps, and
//!   no baseline catches a drag. The resting branch is what is drawn.
//! * **Motion and interaction states** (transform/box-shadow/colour
//!   transitions, hover and focused variants) - per the project-wide list.
//!   Note this cuts the other way too: the tiles' 50%-accent hover border and
//!   accent hover label colour are *not* painted, rather
//!   than being fixed into the resting image.
//! * **Clipped-with-ellipsis preset labels** need clipping, an ellipsis and no
//!   wrapping at once. Nothing clips at these
//!   widths, so this view uses `whitespace_nowrap()` and lets GPUI size the run.
//! * **Text rasterisation.** ClearType subpixel fringes and the recorded
//!   font-smoothing hint are the rasteriser's own output; every
//!   measured ink profile here carries the same LCD colour splits (`7a7a7a`
//!   reads as `627a7a` / `6f5232` on adjacent channels) that GPUI cannot
//!   produce.
//! * **No 24px-padding wrapper.** The recorded outer element was the drop target and
//!   supplies the centring; this view pins the card instead, because the frame
//!   it would centre inside is the window rather than `<main>`'s content cell.
//!
//! # Interaction with other modules
//!
//! `empty-new-document` hands off to `crate::dialogs`. Nothing else here is
//! shared: the card has no analogue in [`crate::components::ui`] (those are the five
//! primitive controls), and `properties_panel.rs` / `layers_panel.rs`
//! own panel chrome rather than overlay chrome, which is `context_menus.rs`'s
//! territory. The one helper worth hoisting later is `text_line` - a text
//! run pinned to an explicit line-box height - which four other modules
//! re-implement inline; it is left local, as they are.

use gpui_kit::prelude::*;
use gpui_kit::{div, px, AnyElement};

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
