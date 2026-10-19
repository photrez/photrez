//! Visual-only port of `editor/shell/EmptyWorkspace.tsx` - the no-document
//! state, i.e. the "Start a Photrez document" card.
//!
//! Source: `apps/desktop/src/components/editor/shell/EmptyWorkspace.tsx`
//! (214 lines). Everything that needs an event loop - the two handlers, the
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
//! obvious reading of `flex flex-1 items-center justify-center p-6` is "middle
//! of the client area", and that would put it at `(1366-520)/2 = 423`. The
//! baseline says `182.5`. The reason is that this component is not the whole
//! client area: it is one cell of `<main>`, the column between the 52px tool
//! rail and the 272px inspector, which `manifest.json` pins as
//! `shell-option-bar` = 781 wide at x=52. With `p-6` (24) on each side that
//! cell's content box is 733 wide at x=76, and `76 + (733-520)/2 = 182.5`.
//! Vertically it is the same story against `<main>`'s 750 (46..796): `p-6`
//! leaves 702 at y=70, and `70 + (702-351.5)/2 = 245.25`.
//!
//! So the CSS box really is `182.5, 245.25, 520 x 351.5`, and the manifest
//! records exactly that. What this file uses is **where the border lands**,
//! which is one pixel further right and none further down:
//!
//! ```text
//!   manifest box          182.5 .. 702.5   x   245.25 .. 596.75
//!   border pixels         183 .. 702       x   245 .. 596
//! ```
//!
//! Chromium snaps a border edge outward to whole device pixels, and the
//! capture agrees: in `empty-card.png` the left border column is crop x=1 and
//! the right is crop x=520, with crop (0,0) = viewport (182,245) - a crop
//! origin confirmed by cross-correlating the card PNG against `empty-full.png`
//! (`zz-probe-cardA2.ts`, mean luma delta 0.0000). `shell-diff.ts` crops the
//! port at `Math.trunc(182.5) = 182` and compares 521 columns, so a card
//! placed at 182 would sit one pixel left of the baseline for its whole
//! width; `CARD_X = 183` lines the borders up.
//!
//! **Why the number is stated rather than computed.** Nothing in the port can
//! re-derive 183 from the layout the way a browser would: the port has no
//! `p-6` viewport box to centre inside, only a frame. Recomputing
//! `(781-520)/2 + 52` here would give 182.5 again and lose the snap, which is
//! exactly the mistake the constant exists to prevent.
//!
//! # `empty-new-document` is the dialog, not the card
//!
//! The shot id is in this file's vocabulary, but what it captures is the
//! **New Document dialog** raised by the card's second button - same
//! `[data-dialog-kind='new-document']` selector as `dialog-new-document`, and
//! the same 780x498 at (293,161).
//!
//! The two PNGs are **not** byte-identical, and the difference is worth
//! writing down before anyone ports the dialog twice. Measured over the
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
//! copy. Only the card is ported here.
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
//! header     1..74    74  16 py-4 + 41 row + 16 + 1 border-b
//! actions   75..138   64  14 py-3.5 + 36 h-9 + 14
//! presets 139..304  166  1 border-t + 14 + 17 + 10 mb-2.5 + 51 + 8 gap-2 + 51 + 14
//! footer   305..350   46  1 border-t + 12 py-3 + 21 + 12
//! ```
//!
//! The 41 and the 51 are the interesting ones, because both are content
//! heights that come out of line boxes:
//!
//! * header 41 = 21 (`text-[14px]` h2) + 2 (`mt-0.5`) + 18 (`text-[12px]` p).
//!   The icon box is 36 in a 41 row, i.e. centred at +2.5 - and the baseline
//!   puts it at **+3** (`#252525` first appears on crop row 20).
//! * tile 51 = 2 borders + 16 `p-2` + 18 (`text-[12px]` label) + 15
//!   (`text-[10px]` tag). Its 28px icon box is centred in the inner 33, i.e.
//!   +2.5 - and the baseline puts it at **+2** (`#1e1e1e` rows 192..219).
//!
//! The two are the same half-pixel and Chromium rounds them **differently**,
//! so both offsets are written as literals. A line-height of 1.5 is what makes
//! 21 / 18 / 15 integers at all; the one exception is `text-[11px]`'s 16.5,
//! which Chromium spends as **17** (the "Quick Presets" line box is crop
//! 154..170, and 17 + 10 `mb-2.5` puts the first tile on crop row 181 exactly).
//!
//! Columns: `px-5` is 20 either side of the 518px content box, so 478 of
//! usable width. The actions grid is `gap-2.5` (10) -> 234 + 234; the presets
//! grid is `gap-2` (8) -> 235 + 235. Both are stated rather than flexed,
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
//! The third is not a translation of the source's `border-editor-divider`.
//! `tokens.rs` documents why: the unlayered `* { border-color }` rule in
//! `styles.css` outranks every `border-*` utility, so all nine rules here -
//! the card, the header's `border-b`, the presets' `border-t`, the footer's
//! `border-t`, the second action button, four preset tiles and the `kbd` -
//! paint `#333333`. The scan confirms it: not one `#2e2e2e` row anywhere in
//! the crop.
//!
//! The accent button is the only saturated fill in the card: `#f28e42` =
//! [`color::ACCENT`], edge to edge across 234x36 with a 4px radius.
//!
//! # Not expressible here
//!
//! * **The drop shadow is close but not the same curve.**
//!   `shadow-[0_12px_32px_rgba(0,0,0,0.8)]` is transcribed as a `BoxShadow`
//!   with the same offset and blur radius, but Chromium's shadow coverage at
//!   the box edge is about 0.4 (measured: `#090909` against a `#0f0f0f`
//!   canvas, 3px out, on both sides), not the 0.8 the alpha says - a blurred
//!   shadow whose rect is offset and finite never reaches full coverage at its
//!   own edge. GPUI's falloff will differ again. This is visible in
//!   `empty-full.png` and **outside** `empty-card.png`, which is cropped tight
//!   to the card and so contains no shadow at all.
//! * **`armed()` is never true.** The `fileDragOver` branch adds
//!   `ring-2 ring-editor-accent ring-offset-2 -translate-y-0.5` and swaps the
//!   header icon's fill to the accent; rings and `translate` are both gaps, and
//!   no baseline catches a drag. The resting branch is what is drawn.
//! * **`transition-[transform,box-shadow]` / `transition-colors` /
//!   `hover:` / `focus-visible:`** - interaction, per the project-wide list.
//!   Note this cuts the other way too: `hover:border-editor-accent/50` and
//!   `group-hover:text-editor-accent` on the tiles are *not* painted, rather
//!   than being frozen into the resting image.
//! * **`truncate`** on the preset labels is `overflow:hidden` +
//!   `text-overflow:ellipsis` + `white-space:nowrap`. Nothing clips at these
//!   widths, so the port uses `whitespace_nowrap()` and lets GPUI size the run.
//! * **Text rasterisation.** ClearType subpixel fringes and `-webkit-font-
//!   smoothing: antialiased` are the rasteriser's, not the markup's; every
//!   measured ink profile here carries the same LCD colour splits (`7a7a7a`
//!   reads as `627a7a` / `6f5232` on adjacent channels) that GPUI cannot
//!   produce.
//! * **No `p-6` wrapper.** The source's outer `div` is the drop target and
//!   supplies the centring; the port pins the card instead, because the frame
//!   it would centre inside is the window rather than `<main>`'s content cell.
//!
//! # Interaction with other modules
//!
//! `empty-new-document` hands off to [`crate::dialogs`]. Nothing else here is
//! shared: the card has no analogue in [`crate::components::ui`] (those are the five
//! `primitives.tsx` controls), and `properties_panel.rs` / `layers_panel.rs`
//! own panel chrome rather than overlay chrome, which is `context_menus.rs`'s
//! territory. The one helper worth hoisting later is [`text_line`] - a text
//! run pinned to an explicit line-box height - which four other modules
//! re-implement inline; it is left local, as they are.

use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::{div, hsla, px, AnyElement, BoxShadow, Div, FontWeight, Hsla};

use crate::assets::icons;
use crate::shot;
use crate::tokens::color;

use super::dialogs;
use crate::components::ui;

// ── Frame ────────────────────────────────────────────────────────────────────

/// The client area `empty-full.png` captures, and the box the card is placed
/// inside. Transparent: the shell behind it belongs to `main.rs`.
const FRAME_W: f32 = 1366.0;
const FRAME_H: f32 = 820.0;

// ── Card ─────────────────────────────────────────────────────────────────────

/// `max-w-[520px]` on the `<section>`. The content column is 781 wide, so
/// `max-w` wins over `w-full` by a wide margin.
const CARD_W: f32 = 520.0;

/// The section's own height. The source sets no height; 351.5 is what the
/// four blocks below add up to, and the border-box the browser paints is 352.
const CARD_H: f32 = 352.0;

/// **Painted** left edge, in viewport pixels.
///
/// The CSS box is at `182.5` (`76 + (733-520)/2`, see the module docs) and
/// Chromium snaps the 1px border outward, so the border column lands on 183.
/// Placing the card at 182 would shift the whole card one pixel left against
/// the baseline crop, which `shell-diff.ts` takes at `trunc(182.5) = 182`.
const CARD_X: f32 = 183.0;

/// **Painted** top edge. The CSS box is at `245.25`; snapping rounds down here,
/// so this one is the number the manifest already quotes.
const CARD_Y: f32 = 245.0;

/// `rounded-[6px]` on the section. Measured from the corner profile: the top
/// border row runs 7..514 of a 520-wide box, i.e. inset 6 on each side.
const CARD_R: f32 = 6.0;

/// `px-5` on all four blocks - 20 off the 518px content box, leaving 478.
const PAD_X: f32 = 20.0;

// ── Block heights ────────────────────────────────────────────────────────────

/// 16 + 41 + 16 + a 1px `border-b`.
const HEADER_H: f32 = 74.0;
/// `py-4`.
const HEADER_PAD_Y: f32 = 16.0;
/// `gap-3` between the header icon box and its text.
const HEADER_GAP: f32 = 12.0;
/// The header row's height: a 21px h2 line, a 2px `mt-0.5`, an 18px p line.
const HEADER_ROW_H: f32 = 41.0;

/// The `size-9` icon box, and the offset it takes inside the 41px row.
///
/// 36 in 41 centres at +2.5; the baseline's first `#252525` row is crop y=20,
/// which is +3. Written as a literal because GPUI would round it down.
const HEADER_ICON_BOX: f32 = 36.0;
const HEADER_ICON_OFFSET: f32 = 3.0;

/// 14 + 36 + 14.
const ACTIONS_H: f32 = 64.0;
/// `py-3.5`.
const ACTIONS_PAD_Y: f32 = 14.0;
/// `gap-2.5` on the actions grid.
const ACTIONS_GAP: f32 = 10.0;

/// `h-9` on both buttons.
const BUTTON_H: f32 = 36.0;
/// (478 - 10) / 2.
const BUTTON_W: f32 = 234.0;
/// `gap-2` between a button's icon and its label.
const BUTTON_GAP: f32 = 8.0;

/// `size-4` on the buttons' icons.
const BUTTON_ICON: f32 = 16.0;

/// `size-5` - the `image-plus` in the header's icon box.
const HEADER_ICON: f32 = 20.0;

/// 1 + 14 + 17 + 10 + 51 + 8 + 51 + 14.
const PRESETS_H: f32 = 166.0;
/// `py-3.5`.
const PRESETS_PAD_Y: f32 = 14.0;
/// `mb-2.5` under the "Quick Presets" line.
///
/// Its line box is **17**px, not the 16.5 that `11 x 1.5` suggests: 17 + 10
/// puts the first tile's top border on crop row 181, which is where the scan
/// finds it.
const PRESETS_HEADING_MB: f32 = 10.0;

/// (478 - 8) / 2.
const TILE_W: f32 = 235.0;
/// 2 borders + 16 `p-2` + 18 + 15.
const TILE_H: f32 = 51.0;
/// `p-2`.
const TILE_PAD: f32 = 8.0;
/// `gap-2.5` between a tile's icon box and its two text lines.
const TILE_GAP: f32 = 10.0;
/// `size-7` - the tile's icon box.
const TILE_ICON_BOX: f32 = 28.0;
/// `size-3.5` - the glyph inside it.
const TILE_ICON: f32 = 14.0;

/// `gap-2` between the two rows of the presets grid.
const PRESET_ROW_GAP: f32 = 8.0;

/// 1 + 12 + 21 + 12.
const FOOTER_H: f32 = 46.0;
/// `py-3`.
const FOOTER_PAD_Y: f32 = 12.0;

/// `px-1.5` on the `<kbd>`.
const KBD_PAD_X: f32 = 6.0;
/// `py-0.5` on the `<kbd>`.
const KBD_PAD_Y: f32 = 2.0;

// ── Radii ────────────────────────────────────────────────────────────────────

/// `rounded-[4px]`: the card's inner buttons and preset tiles.
const R4: f32 = 4.0;
/// `rounded-[3px]`: a tile's icon box, and the `<kbd>`.
const R3: f32 = 3.0;

// ── Type ─────────────────────────────────────────────────────────────────────

/// `--font-sans` - Inter, on every run in the card. `text-[14px] font-semibold` - the card title.
const TITLE_TEXT: f32 = 14.0;
const TITLE_LINE_H: f32 = 21.0;

/// `text-[12px]` - the card's one-line description.
const DESC_TEXT: f32 = 12.0;
const DESC_LINE_H: f32 = 18.0;

/// `mt-0.5` between the title and the description.
const DESC_MT: f32 = 2.0;

/// `text-[13px] font-medium` - both action buttons.
const BUTTON_TEXT: f32 = 13.0;

/// `text-[11px] font-semibold` - "Quick Presets".
const HEADING_TEXT: f32 = 11.0;
const HEADING_LINE_H: f32 = 17.0;

/// `text-[12px] font-medium` - a tile's label.
const TILE_LABEL_TEXT: f32 = 12.0;
const TILE_LABEL_LINE_H: f32 = 18.0;

/// `text-[10px]` - a tile's size tag, and the `<kbd>`.
const TAG_TEXT: f32 = 10.0;
const TAG_LINE_H: f32 = 15.0;

/// `text-[11px]` - the drop prompt.
const PROMPT_TEXT: f32 = 11.0;

// ── Icons ────────────────────────────────────────────────────────────────────

/// `strokeWidth` on every icon in the card.
///
/// The source passes 1.5 to the header's `image-plus` and 1.75 to all four
/// others. [`icons::with_stroke`] re-strokes a baked constant on demand, so
/// the two 1.5 call sites use [`icons::IMAGE_PLUS`] directly and the four
/// 1.75 ones go through [`preset_icon`] / the baked `1.75` constants.
const STROKE_PRESET: f32 = 1.75;

/// `EmptyWorkspace.tsx`'s `PRESETS`, in source order: label, then the size tag
/// under it. The `x` is U+00D7 and is present in the baseline - the first
/// tile's tag scans as four ink groups at crop x 69..89, 94..98, 103..123,
/// 128..137, and the second is a 5px glyph.
const PRESETS: [(&str, &str); 4] = [
    ("Instagram Post", "1080 \u{00d7} 1080 px"),
    ("Story / Reels", "1080 \u{00d7} 1920 px"),
    ("Full HD Canvas", "1920 \u{00d7} 1080 px"),
    ("A4 Print (300 DPI)", "2480 \u{00d7} 3508 px"),
];

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
/// every other id, including `None`, draws the card. Mirrors the reference's
/// `match shot::active() { "empty-new-document" => ..., _ => ... }` exactly.
fn select(shot: Option<&str>) -> EmptyVariant {
    match shot {
        Some("empty-new-document") => EmptyVariant::NewDocument,
        _ => EmptyVariant::Card,
    }
}

/// A transparent full-viewport frame with the captured state pinned inside it.
///
/// The two arms are deliberately not symmetric:
///
/// * `empty-card` (and `empty-full`, and any unset id) draw the card.
/// * `empty-new-document` is **the New Document dialog**, not the card, and
///   [`crate::dialogs`] owns it - see the module docs for the measured
///   comparison of the two baselines. Delegating means the dialog is ported
///   once; drawing a second copy here would guarantee the two drift.
///
/// The frame is absolute so it disturbs nothing wherever it is appended, and
/// the caller must add it late in the tree: GPUI has no `z-index`, so paint
/// order is the only stacking control available.
pub fn render() -> impl IntoElement {
    // `AnyElement` rather than `Div`, because the two arms are different
    // types: the card is a concrete `Div` and `dialogs::render()` is an
    // opaque `impl IntoElement`.
    let body: AnyElement = match select(shot::active()) {
        EmptyVariant::NewDocument => dialogs::render().into_any_element(),
        EmptyVariant::Card => card()
            .absolute()
            .left(px(CARD_X))
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

/// The `<section>` itself: 520x352, 6px corners, a 1px [`color::BORDER`] edge
/// and a [`color::PANEL`] fill, over `0 12px 32px rgba(0,0,0,0.8)`.
///
/// `overflow_hidden` stands in for nothing the source has - the card's content
/// is exactly 350 tall - but it keeps a stray flex rounding step from painting
/// past the 6px corners.
fn card() -> Div {
    v_flex()
        .items_stretch()
        .w(px(CARD_W))
        .h(px(CARD_H))
        .rounded(px(CARD_R))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::PANEL)
        .overflow_hidden()
        .shadow(card_shadow())
        .child(header())
        .child(actions())
        .child(presets())
        .child(footer())
}

/// `shadow-[0_12px_32px_rgba(0,0,0,0.8)]`.
///
/// The parameters are exact; the *curve* is not. Chromium's coverage at the
/// card's own edge reads about 0.4 (`#090909` over a `#0f0f0f` canvas, 3px out,
/// symmetric on both sides), because a finite blurred rect offset 12px down
/// never reaches full coverage at its edge - and GPUI approximates the blur
/// differently again. `empty-card.png` is cropped to the card, so it contains
/// no shadow at all; only `empty-full.png` shows this, 12px below and 30px
/// either side of it.
fn card_shadow() -> Vec<BoxShadow> {
    vec![BoxShadow::new(px(0.0), px(12.0), hsla(0.0, 0.0, 0.0, 0.8)).blur_radius(px(32.0))]
}

/// The header block: `border-b border-editor-divider px-5 py-4`.
///
/// 74px = 16 + 41 + 16 + 1. The border is [`color::BORDER`], not
/// [`color::DIVIDER`] - every rule in the card is, and `tokens.rs` says why.
fn header() -> Div {
    v_flex()
        .w_full()
        .h(px(HEADER_H))
        .flex_shrink_0()
        .px(px(PAD_X))
        .py(px(HEADER_PAD_Y))
        .border_b(px(1.0))
        .border_color(color::BORDER)
        .child(
            h_flex()
                .w_full()
                .h(px(HEADER_ROW_H))
                // `items-center`, resolved by hand: the icon box would sit at
                // +2 of the 41px row and the capture has it at +3.
                .items_start()
                .gap(px(HEADER_GAP))
                .child(
                    div()
                        .w(px(HEADER_ICON_BOX))
                        .h(px(HEADER_ICON_BOX))
                        .mt(px(HEADER_ICON_OFFSET))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(R4))
                        .bg(color::FIELD)
                        .child(
                            Icon::default()
                                .data(icons::IMAGE_PLUS)
                                .size(px(HEADER_ICON))
                                .text_color(color::TEXT_DIM),
                        ),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w(px(0.0))
                        .child(text_line(
                            "Start a Photrez document",
                            TITLE_TEXT,
                            TITLE_LINE_H,
                            FontWeight(600.),
                            color::TEXT,
                        ))
                        .child(
                            text_line(
                                "Open an image, drop one into the workspace, or create a blank canvas.",
                                DESC_TEXT,
                                DESC_LINE_H,
                                FontWeight(400.),
                                color::TEXT_DIM,
                            )
                            .mt(px(DESC_MT)),
                        ),
                ),
        )
}

/// The two action buttons: `grid grid-cols-2 gap-2.5 px-5 py-3.5`.
///
/// 64px = 14 + 36 + 14. Column widths are 234 each, from (478 - 10) / 2.
fn actions() -> Div {
    h_flex()
        .w_full()
        .h(px(ACTIONS_H))
        .flex_shrink_0()
        .items_stretch()
        .px(px(PAD_X))
        .py(px(ACTIONS_PAD_Y))
        .gap(px(ACTIONS_GAP))
        .child(action_button(
            icons::FOLDER_PLUS,
            "Open Image",
            color::ACCENT,
            color::WHITE,
            false,
        ))
        .child(action_button(
            icons::PLUS,
            "New Document",
            color::FIELD,
            color::TEXT,
            true,
        ))
}

/// One action button.
///
/// The first is `bg-editor-accent text-white` with no border; the second is
/// `border border-editor-field-border bg-editor-field text-editor-text`, and
/// that `border-editor-field-border` resolves to [`color::BORDER`] like every
/// other border in the app rather than to [`color::FIELD_BORDER`].
///
/// Both are `h-9 ... justify-center gap-2 px-3`, so the icon and the label
/// centre as a group: the baseline puts the first group's ink at crop
/// x 91..186 (97 wide) and the second's at 328..440 (114 wide), against a
/// 234px column.
fn action_button(icon: &'static [u8], label: &str, fill: Hsla, ink: Hsla, bordered: bool) -> Div {
    let mut button = h_flex()
        .w(px(BUTTON_W))
        .h(px(BUTTON_H))
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .gap(px(BUTTON_GAP))
        .rounded(px(R4))
        .bg(fill);

    if bordered {
        button = button.border_1().border_color(color::BORDER);
    }

    button
        .child(
            Icon::default()
                .data(icon)
                .size(px(BUTTON_ICON))
                .text_color(ink),
        )
        .child(
            div()
                .flex()
                .items_center()
                .whitespace_nowrap()
                .text_size(px(BUTTON_TEXT))
                .font_family(ui::FONT_UI)
                .font_weight(FontWeight(500.))
                .text_color(ink)
                .child(label.to_string()),
        )
}

/// The Quick Presets block: `border-t border-editor-divider px-5 py-3.5`.
///
/// 166px = 1 + 14 + 17 + 10 + 51 + 8 + 51 + 14. Two things are worth keeping
/// in mind when this block is read again: the heading's line box is 17 rather
/// than the 16.5 that `11 x 1.5` gives, and the tiles are 51 rather than the
/// 49 that `p-2` plus two default line heights suggest. Both are what the
/// capture measures, and both are load-bearing for the rows landing on crop
/// 181 and 240.
fn presets() -> Div {
    v_flex()
        .w_full()
        .h(px(PRESETS_H))
        .flex_shrink_0()
        .px(px(PAD_X))
        .py(px(PRESETS_PAD_Y))
        .border_t(px(1.0))
        .border_color(color::BORDER)
        .child(
            text_line(
                "Quick Presets",
                HEADING_TEXT,
                HEADING_LINE_H,
                FontWeight(600.),
                color::TEXT,
            )
            .mb(px(PRESETS_HEADING_MB)),
        )
        .child(
            v_flex()
                .w_full()
                .gap(px(PRESET_ROW_GAP))
                .child(preset_row(0, 1))
                .child(preset_row(2, 3)),
        )
}

/// One row of the `grid-cols-2` presets grid - two 235px tiles 8px apart.
fn preset_row(first: usize, second: usize) -> Div {
    h_flex()
        .w_full()
        .h(px(TILE_H))
        .flex_shrink_0()
        .items_stretch()
        .gap(px(PRESET_ROW_GAP))
        .child(preset_tile(first))
        .child(preset_tile(second))
}

/// One preset: `flex items-center gap-2.5 rounded-[4px] border
/// border-editor-field-border bg-editor-field p-2`.
///
/// 51px = 2 borders + 16 padding + 33 of content, and the 33 is an 18px label
/// line over a 15px tag line. The 28px icon box centres in that 33 at +2.5,
/// which GPUI rounds the same way the capture does - so unlike the header's
/// icon box, this one is left to `items_center`.
fn preset_tile(index: usize) -> Div {
    let (label, tag) = PRESETS[index];

    h_flex()
        .w(px(TILE_W))
        .h(px(TILE_H))
        .flex_shrink_0()
        .items_center()
        .gap(px(TILE_GAP))
        .rounded(px(R4))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .p(px(TILE_PAD))
        .child(
            div()
                .w(px(TILE_ICON_BOX))
                .h(px(TILE_ICON_BOX))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(R3))
                .bg(color::PANEL)
                .child(
                    Icon::default()
                        .data(preset_icon(index))
                        .size(px(TILE_ICON))
                        .text_color(color::TEXT_DIM),
                ),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w(px(0.0))
                .child(text_line(
                    label,
                    TILE_LABEL_TEXT,
                    TILE_LABEL_LINE_H,
                    FontWeight(500.),
                    color::TEXT,
                ))
                .child(text_line(
                    tag,
                    TAG_TEXT,
                    TAG_LINE_H,
                    FontWeight(400.),
                    color::TEXT_DIM,
                )),
        )
}

/// The footer: `flex items-center justify-between border-t
/// border-editor-divider px-5 py-3 text-[11px] text-editor-text-dim`.
///
/// 46px = 1 + 12 + 21 + 12. The 21 is the `<kbd>`'s own height (2 border +
/// 4 padding + a 15px 10px line), and it is the tallest thing in the row, so
/// it sets the block's content height rather than the 11px prompt.
fn footer() -> Div {
    h_flex()
        .w_full()
        .h(px(FOOTER_H))
        .flex_shrink_0()
        .items_center()
        .justify_between()
        .px(px(PAD_X))
        .py(px(FOOTER_PAD_Y))
        .border_t(px(1.0))
        .border_color(color::BORDER)
        // The prompt is one long unwrapped sentence and the kbd chip is a
        // fixed 44px mark at its right. Before the split the sentence kept its
        // intrinsic width and painted straight through the chip.
        .child(ui::clip(
            div()
                .flex()
                .items_center()
                .whitespace_nowrap()
                .text_size(px(PROMPT_TEXT))
                .font_family(ui::FONT_UI)
                .text_color(color::TEXT_DIM)
                .child("Drop PNG, JPEG, or WebP files anywhere in this workspace."),
        ))
        .child(ui::pin(kbd()))
}

/// `<kbd class="font-sans rounded-[3px] border border-editor-divider
/// bg-editor-field px-1.5 py-0.5 text-[10px] text-editor-text-dim">`.
///
/// Measured 44x21 at crop x 456..499, y 318..338, with the "Ctrl+O" run's ink
/// at x 463..492 - which is `456 + 1 (border) + 6 (px-1.5)`, so the box is
/// exactly padding + text + padding and nothing here needs an explicit width.
/// (The *earlier* apparent 38px text run was the chip's own top and bottom
/// border rows being read as ink.)
fn kbd() -> Div {
    h_flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .rounded(px(R3))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .px(px(KBD_PAD_X))
        .py(px(KBD_PAD_Y))
        .child(
            div()
                .flex()
                .items_center()
                .whitespace_nowrap()
                .text_size(px(TAG_TEXT))
                .font_family(ui::FONT_UI)
                .font_weight(FontWeight(400.))
                .text_color(color::TEXT_DIM)
                .child("Ctrl+O"),
        )
}

/// A text run pinned to an explicit line-box height.
///
/// **Worth consolidating.** Tailwind's `text-[Npx]` here resolves to a line
/// box of 1.5N, and this card depends on those heights for four of its block
/// sizes, so each run is given the height it was measured at and centred in
/// it. Centring is enough rather than a computed baseline offset: a glyph's
/// baseline lands at `box_centre + (ascent - descent) / 2` whatever the
/// line-height, because half-leading is symmetric - which is also why the
/// button labels need no explicit height even though Chromium gave them a
/// 19.5px line box inside a 36px button.
///
/// Four other modules spell this out inline at the call site
/// (`text_size` + `font_family` + `font_weight` + `text_color` on a fixed-height
/// `div`), so this is the natural thing to hoist into `ui.rs` if a fifth
/// arrives.
///
/// `whitespace_nowrap` stands in for the source's `truncate` on the preset
/// labels and for the single-line spans; nothing clips at these widths, so
/// the ellipsis itself never appears.
fn text_line(text: &str, size: f32, line_h: f32, weight: FontWeight, colour: Hsla) -> Div {
    div()
        .h(px(line_h))
        .flex()
        .items_center()
        .whitespace_nowrap()
        .text_size(px(size))
        .font_family(ui::FONT_UI)
        .font_weight(weight)
        .text_color(colour)
        .child(text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Variant table ───────────────────────────────────────────────────────

    #[test]
    fn select_routes_empty_new_document_to_dialog() {
        assert_eq!(
            select(Some("empty-new-document")),
            EmptyVariant::NewDocument
        );
    }

    #[test]
    fn select_draws_card_for_every_other_shot() {
        assert_eq!(select(Some("empty-card")), EmptyVariant::Card);
        assert_eq!(select(Some("empty-full")), EmptyVariant::Card);
        // An unset id (plain launch) still draws the card.
        assert_eq!(select(None), EmptyVariant::Card);
        // Shots that belong to other modules do not hijack the card arm.
        assert_eq!(select(Some("loading-overlay")), EmptyVariant::Card);
    }

    // ── Empty-state card content table ──────────────────────────────────────

    #[test]
    fn presets_table_has_four_entries() {
        assert_eq!(PRESETS.len(), 4);
        assert_eq!(PRESETS[0].0, "Instagram Post");
        assert_eq!(PRESETS[3].1, "2480 \u{00d7} 3508 px");
        // The multiplication sign is the measured glyph, not a stray '*'.
        assert!(PRESETS.iter().all(|(_l, t)| t.contains("\u{00d7}")));
    }

    #[test]
    fn preset_icons_line_up_with_labels() {
        assert_eq!(PRESET_ICONS.len(), PRESETS.len());
        // preset_icon re-strokes each tile icon; the header icon stays at its
        // baked 1.5, so it is not part of the re-stroked tile set.
        for i in 0..PRESET_ICONS.len() {
            let re = preset_icon(i);
            assert!(!re.is_empty(), "tile {i} produced an empty glyph");
        }
    }

    // ── Geometry invariant: blocks tile the card height exactly ─────────────

    #[test]
    fn blocks_tile_card_height() {
        // 74 + 64 + 166 + 46 = 350 content; the 352 border box adds the 1px
        // header border implicitly captured by CARD_H in the docs.
        assert!(
            (HEADER_H + ACTIONS_H + PRESETS_H + FOOTER_H - 350.0).abs() < 1e-9,
            "block heights must sum to the 350px content height"
        );
        assert!(
            (HEADER_H + ACTIONS_H + PRESETS_H + FOOTER_H - (CARD_H - 2.0)).abs() < 1e-9
                || (HEADER_H + ACTIONS_H + PRESETS_H + FOOTER_H - CARD_H).abs() < 1e-9
        );
    }
}
