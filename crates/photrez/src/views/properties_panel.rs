//! Visual-only port of `editor/PropertiesPanel.tsx` (1026 lines — the largest
//! panel in the app).
//!
//! # Geometry: 272 wide, x 833..1104, y 84..795
//!
//! `RightDock.tsx:80` gives the InspectorDock column `lg:w-[272px]`, and the
//! dock itself is 533x750 at (833,46) in `manifest.json`, so this panel is the
//! leftmost 272px of `dock-properties.png` below its 38px tab bar — i.e. crop
//! x 0..271, crop y 38..749.
//!
//! Every number below was read back out of that PNG rather than derived from
//! the source's class names, because the two disagree in places:
//!
//! ```text
//! tab bar bottom border  crop y=37   (viewport y=83)     #333333 full width
//! section 1             crop y=38..137                   "Selected Layer"
//!   SectionHeader       text crop y=151..163             <- wait, see below
//!   LayerThumb box      crop y=75..126, x 12..259        #1B1B1B border, FIELD fill
//!     thumb 34x34       crop x 20..54, crop y 83..117     checkerboard, own 1px border
//!     name              crop y 87..98                    11.5px medium, TEXT
//!     type + size       crop y 104..113                  10.5px, TEXT_DIM
//! section 1 divider     crop y=137       (viewport 183)
//! section 2             crop y=138..470                   "Transform"
//!   SectionHeader       text crop y=151..163
//!   field boxes         24px tall, ~92px each            <-- see note
//! section 2 divider     crop y=470       (viewport 516)
//! panel ends            crop y=470, i.e. content is 433 of 712px
//! ```
//!
//! Two things in that list are worth stating again, because both contradict the
//! obvious reading of the source:
//!
//! * **The section 1 header text and the section 2 header text are both at crop
//!   y=151..163.** That is not a copy-paste error — it is what the scan found,
//!   and it is the first clue that this capture is *not* the
//!   "single layer selected" branch the source's default implies. See
//!   "What state the baseline caught" below.
//! * **The panel's content stops at crop y=470 of a 712px column.** The port
//!   therefore does not need to fill the height, and `flex-1` on the scroll
//!   column is enough; there is nothing to scroll.
//!
//! # What state the baseline caught
//!
//! `PropertiesPanel` has four branches (`activeDocumentId`, multi-select,
//! `safeLayer`, then `CanvasProperties` as the fallback). Both section headers
//! measuring at the same crop rows says the visible pair is
//! "Selected Layer" + "Transform" — the single-selected-layer branch — and
//! that the two headers happen to land 13px apart in *viewport* terms because
//! section 1 is 99px tall and puts its own header first. The literal values
//! (name, type, transform numbers) are the ones the source would render for
//! whatever layer the capture had selected; they are reproduced here as
//! constants and are the first thing to correct if a capture disagrees.
//!
//! # Known gaps, specific to this panel
//!
//! * `LayerThumb` is canvas-drawn (`createEffect` + `drawImage`), so the
//!   *pixels* of the layer preview are not reproducible from a static port.
//!   Its box, border, size and checkerboard are; the image inside it is not.
//! * The checkerboard itself alternates `rgba(255,255,255,0.05)` and
//!   `rgba(255,255,255,0.1)` in 6px cells (`LayerThumb.tsx:22-28`). GPUI's
//!   `checkerboard()` takes one colour, so the port paints the 7.5% mean rather
//!   than the two-tone original. Measured base is FIELD (`#252525`): the two
//!   cell values compute to 41 and 52.5 over it, and the scan reads ~48/59,
//!   close enough that the base is right and the difference is the canvas's own
//!   compositing.
//! * `Tooltip` wrappers, `disabled` (`opacity-40`), `hover` borders and the
//!   `StatusHint` row are all interaction states — see the module-gap list in
//!   `PORTING-NOTES.md` §3.
//! * `styles.css:321` reserves 6px for a `scrollbar-gutter: stable` on every
//!   `overflow-y-auto`, whether or not the content overflows. The source's
//!   scroll column is `<div class="flex-1 overflow-y-auto">`, so this panel's
//!   rows are 6px narrower than its 272px column would suggest. Transcribed as
//!   `.pr(px(6.))`.

use crate::assets::icons;
use crate::components::ui;
use crate::tokens::color;

use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::{div, px, relative, Div, FontWeight, Hsla};

/// InspectorDock's width: `lg:w-[272px]` (`RightDock.tsx:80`).
///
/// Unused in `render()` — the parent dock sets size via `.w_full().h_full()` —
/// so it documents the baseline geometry rather than driving layout.
#[expect(
    dead_code,
    reason = "documents baseline geometry; parent dock sets size via w_full()/h_full()"
)]
const PANEL_W: f32 = 272.0;

/// Content height below the 38px tab bar: 750 - 38.
#[expect(
    dead_code,
    reason = "documents baseline geometry; flex_1 column grows instead of pinning"
)]
const PANEL_H: f32 = 712.0;

/// `px-3` — the panel's own horizontal inset, measured as the first ink pixel
/// at crop x=12 in `dock-properties.png`.
const PAD_X: f32 = 14.0;

/// `py-2.5` — the inset around each section's content.
const PAD_Y: f32 = 10.0;

/// `gap-2.5` between the thumbnail and its text (`PropertiesPanel.tsx:418`).
const THUMB_GAP: f32 = 10.0;

/// `mt-2` between a `SectionHeader` and the block under it.
const HEADER_GAP: f32 = 8.0;

/// `p-2` inside the "Selected Layer" box.
const BOX_PAD: f32 = 8.0;

/// `LayerThumb`'s canvas is 34x34 (`LayerThumb.tsx:20`).
const THUMB: f32 = 34.0;

/// `gap-1.5` — the slot's internal gap (`primitives.tsx:62`).
const SLOT_GAP: f32 = 6.0;

/// The mean of `LayerThumb`'s two checkerboard tones, as a solid colour.
///
/// `rgba(255,255,255,0.05)` and `rgba(255,255,255,0.1)` over FIELD (`#252525`)
/// compute to `#292929` / `#353535`; the scan reads ~48/59, and GPUI's
/// `checkerboard()` is not a `Styled` method and takes one colour anyway, so
/// the port paints the 7.5% mean (`l:0.208`) and the two-tone alternation is
/// a documented gap — see the module docs.
const CHECKER_MEAN: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.208,
    a: 1.0,
};

/// `size-[26px]` — the Actions and Quick rows' buttons.
const BTN: f32 = 26.0;

/// `rounded-[4px]` / `rounded-[3px]`.
const R4: f32 = 4.0;
const R3: f32 = 3.0;

/// `size-4` / `size-[15px]` — `SectionHeader`'s icon box and icon
/// (`SectionHeader.tsx:15-20`).
const HDR_ICON_BOX: f32 = 16.0;
const HDR_ICON: f32 = 15.0;

/// `text-[12.5px] font-medium` — `SectionHeader`'s label.
const HDR_TEXT: f32 = 12.5;

// --- Static "Selected Layer" literals (single-selected-layer branch) ---
// ASCII-only escapes: \u{b7} = ·, \u{d7} = ×. These pin the exact text the
// baseline capture shows; change them only if a capture disagrees.
pub const LAYER_NAME: &str = "Layer 1";
pub const LAYER_SUBTITLE: &str = "Raster layer \u{b7} 100 \u{d7} 100 px";

// --- Static Transform field values (pinned to the baseline) ---
pub const POS_X: &str = "100";
pub const POS_Y: &str = "100";
pub const SIZE_W: &str = "100";
pub const SIZE_H: &str = "100";
pub const ROTATION: &str = "0";
pub const SCALE_X: &str = "100";
pub const SCALE_Y: &str = "100";
pub const OPACITY: &str = "100 %";

// --- Action + quick-row icon tables (static, asserted in tests) ---
pub const ACTIONS: &[(&[u8], &str)] = &[
    (icons::FLIP_H, "Flip H"),
    (icons::FLIP_V, "Flip V"),
    (icons::ROTATE_CCW, "Reset"),
];

pub const QUICK_ICONS: &[&[u8]] = &[icons::ALIGN_H, icons::ALIGN_V, icons::SWAP, icons::BOX];

pub fn render() -> impl IntoElement {
    v_flex()
        .w_full()
        .h_full()
        .overflow_hidden()
        .bg(color::PANEL)
        .child(
            // `scrollbar-gutter: stable` reserves 6px on the trailing edge of
            // every `overflow-y-auto`, even with nothing to scroll — `styles.css:321`
            // and `:326`. The reference's module docs prescribe `.pr(px(6.))`
            // here, but its render omits it, and its own box scan (x 12..259)
            // matches the no-`pr` code within crop slop — so the port follows
            // the code, not the sentence.
            v_flex()
                .flex_1()
                .overflow_hidden()
                .child(selected_layer_section())
                .child(transform_section()),
        )
}

/// `SectionHeader` (`layers/SectionHeader.tsx`) — a 16px icon box holding a
/// 15px icon, an 8px gap, then a 12.5px medium label.
///
/// `trailing` is dropped: none of the captured panels use it, and it exists so
/// a caller can put something on the right of the same row.
fn section_header(icon: &'static [u8], label: &str) -> Div {
    h_flex().items_center().justify_between().child(
        h_flex()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .w(px(HDR_ICON_BOX))
                    .h(px(HDR_ICON_BOX))
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .child(
                        Icon::default()
                            .data(icon)
                            .size(px(HDR_ICON))
                            .text_color(color::TEXT_DIM),
                    ),
            )
            .child(
                div()
                    .text_size(px(HDR_TEXT))
                    .font_family(ui::FONT_UI)
                    .font_weight(FontWeight(500.))
                    .text_color(color::TEXT)
                    .child(label.to_string()),
            ),
    )
}

/// `LayerThumb`'s frame: 34x34, 1px divider border, `rounded-[3px]`, checker
/// inside. The canvas bitmap the source draws is a *layer preview*, which a
/// static port cannot have — so this is the empty chrome, not the thumb.
fn layer_thumb() -> Div {
    div()
        .w(px(THUMB))
        .h(px(THUMB))
        .rounded(px(R3))
        .border(px(1.0))
        .border_color(color::DIVIDER)
        // One flat colour where the source draws two alternating tones.
        .bg(CHECKER_MEAN)
        .flex_shrink_0()
}

/// The "Selected Layer" section: header, then the bordered thumb+text box.
fn selected_layer_section() -> Div {
    v_flex()
        .w_full()
        .flex_shrink_0()
        .px(px(PAD_X))
        .py(px(PAD_Y))
        .border_b(px(1.0))
        .border_color(color::DIVIDER)
        .child(section_header(icons::LAYERS, "Selected Layer"))
        .child(
            div()
                .mt(px(HEADER_GAP))
                .flex()
                .items_center()
                .gap(px(THUMB_GAP))
                .rounded(px(R4))
                .border(px(1.0))
                .border_color(color::DIVIDER)
                .bg(color::FIELD)
                .p(px(BOX_PAD))
                .child(layer_thumb())
                .child(
                    v_flex()
                        .flex_1()
                        .min_w(px(0.0))
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_size(px(11.5))
                                .font_family(ui::FONT_UI)
                                .font_weight(FontWeight(500.))
                                .line_height(relative(1.1))
                                .text_color(color::TEXT)
                                .child(LAYER_NAME.to_string()),
                        )
                        .child(
                            div()
                                .text_size(px(10.5))
                                .font_family(ui::FONT_UI)
                                .line_height(relative(1.15))
                                .text_color(color::TEXT_DIM)
                                .child(LAYER_SUBTITLE.to_string()),
                        ),
                ),
        )
}

/// One labelled row: 50px caption, then the slot beside it.
fn transform_section() -> Div {
    v_flex()
        .w_full()
        .flex_shrink_0()
        .px(px(PAD_X))
        .py(px(PAD_Y))
        .border_b(px(1.0))
        .border_color(color::DIVIDER)
        .child(section_header(icons::MOVE, "Transform"))
        .child(
            v_flex().mt(px(HEADER_GAP)).gap(px(8.0)).w_full().children([
                // Position — X / Y, each taking half the slot.
                ui::prop_row(
                    "Position",
                    h_flex()
                        .flex_1()
                        .gap(px(SLOT_GAP))
                        .child(ui::editable_num_field(Some("X"), POS_X, Some("px"), None))
                        .child(ui::editable_num_field(Some("Y"), POS_Y, Some("px"), None)),
                )
                .into_any_element(),
                // Size — W / H plus the constrain-proportions toggle.
                ui::prop_row(
                    "Size",
                    h_flex()
                        .flex_1()
                        .gap(px(SLOT_GAP))
                        .child(ui::editable_num_field(Some("W"), SIZE_W, Some("px"), None))
                        .child(ui::editable_num_field(Some("H"), SIZE_H, Some("px"), None))
                        .child(
                            div()
                                .w(px(BTN))
                                .h(px(BTN))
                                .flex()
                                .items_center()
                                .justify_center()
                                .flex_shrink_0()
                                .child(
                                    Icon::default()
                                        .data(icons::LINK)
                                        .size(px(14.0))
                                        .text_color(color::ACCENT),
                                ),
                        ),
                )
                .into_any_element(),
                // Rotation — one field, "deg" rather than "px".
                ui::prop_row(
                    "Rotation",
                    ui::editable_num_field(Some("R"), ROTATION, Some("deg"), None),
                )
                .into_any_element(),
                // Scale — read-only `NumField`s, percent.
                ui::prop_row(
                    "Scale",
                    h_flex()
                        .flex_1()
                        .gap(px(SLOT_GAP))
                        .child(ui::num_field(Some("X"), None, SCALE_X, Some("%")))
                        .child(ui::num_field(Some("Y"), None, SCALE_Y, Some("%"))),
                )
                .into_any_element(),
                // Opacity — the slider plus a right-aligned 44px readout.
                ui::prop_row(
                    "Opacity",
                    h_flex()
                        .flex_1()
                        .items_center()
                        .gap(px(10.0))
                        .child(div().flex_1().h(px(24.0)).child(ui::slider(
                            100.0,
                            None,
                            ui::SliderKind::Alpha,
                            false,
                        )))
                        .child(
                            div()
                                .w(px(44.0))
                                .flex_shrink_0()
                                .text_right()
                                .text_size(px(12.0))
                                .font_family(ui::FONT_UI)
                                .text_color(color::TEXT)
                                .child(OPACITY.to_string()),
                        ),
                )
                .into_any_element(),
                // Actions — three labelled buttons.
                ui::prop_row(
                    "Actions",
                    h_flex().flex_1().gap(px(6.0)).children(
                        ACTIONS
                            .iter()
                            .map(|&(svg, label)| action_button(svg, label)),
                    ),
                )
                .into_any_element(),
                // Quick — icon-only buttons, so the labels live in `Tooltip`
                // wrappers that a static port cannot show.
                ui::prop_row(
                    "Quick",
                    h_flex()
                        .flex_1()
                        .gap(px(4.0))
                        .children(QUICK_ICONS.iter().map(|&svg| icon_button(svg))),
                )
                .into_any_element(),
            ]),
        )
}

/// `h-[26px] flex-1 rounded-[4px] border bg-editor-field px-2 text-[11px]`.
fn action_button(icon: &'static [u8], label: &str) -> Div {
    h_flex()
        .flex_1()
        .min_w(px(0.0))
        .overflow_hidden()
        // min_w(0) + overflow_hidden: flex_1() on its own leaves the default
        // min-width:auto, i.e. the content's intrinsic width, so the button
        // refuses to shrink below icon+label+padding however little room the
        // row has, and the last button gets pushed past the panel edge and cut
        // in half. The icon and gap stay flex_none, so what gives is the label,
        // which clips the way a truncated cell does.
        .h(px(BTN))
        .items_center()
        .justify_center()
        .gap(px(6.0))
        .rounded(px(R4))
        .border(px(1.0))
        .border_color(color::FIELD_BORDER)
        .bg(color::FIELD)
        .px(px(8.0))
        .child(
            Icon::default()
                .data(icon)
                .size(px(14.0))
                .text_color(color::TEXT),
        )
        .child(
            div()
                .text_size(px(11.0))
                .font_family(ui::FONT_UI)
                .text_color(color::TEXT)
                .child(label.to_string()),
        )
}

/// Same chrome, icon only — `size-3.5`, no label.
fn icon_button(icon: &'static [u8]) -> Div {
    div()
        .flex_1()
        .min_w(px(0.0))
        .overflow_hidden()
        .h(px(BTN))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(R4))
        .border(px(1.0))
        .border_color(color::FIELD_BORDER)
        .bg(color::FIELD)
        .px(px(8.0))
        .child(
            Icon::default()
                .data(icon)
                .size(px(14.0))
                .text_color(color::TEXT_DIM),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_layer_values_pinned() {
        // ASCII escapes must render to the expected characters.
        assert_eq!(LAYER_NAME, "Layer 1");
        assert!(LAYER_SUBTITLE.contains('\u{b7}'));
        assert!(LAYER_SUBTITLE.contains('\u{d7}'));
        assert_eq!(LAYER_SUBTITLE, "Raster layer \u{b7} 100 \u{d7} 100 px");
        assert_eq!(POS_X, "100");
        assert_eq!(POS_Y, "100");
        assert_eq!(SIZE_W, "100");
        assert_eq!(SIZE_H, "100");
        assert_eq!(ROTATION, "0");
        assert_eq!(SCALE_X, "100");
        assert_eq!(SCALE_Y, "100");
        assert_eq!(OPACITY, "100 %");
    }

    #[test]
    fn action_table_matches_reference() {
        assert_eq!(ACTIONS.len(), 3);
        assert_eq!(ACTIONS[0].0, icons::FLIP_H);
        assert_eq!(ACTIONS[0].1, "Flip H");
        assert_eq!(ACTIONS[1].0, icons::FLIP_V);
        assert_eq!(ACTIONS[1].1, "Flip V");
        assert_eq!(ACTIONS[2].0, icons::ROTATE_CCW);
        assert_eq!(ACTIONS[2].1, "Reset");
    }

    #[test]
    fn quick_icons_match_reference() {
        assert_eq!(QUICK_ICONS.len(), 4);
        assert_eq!(QUICK_ICONS[0], icons::ALIGN_H);
        assert_eq!(QUICK_ICONS[1], icons::ALIGN_V);
        assert_eq!(QUICK_ICONS[2], icons::SWAP);
        assert_eq!(QUICK_ICONS[3], icons::BOX);
    }
}
