use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::{div, hsla, px, BoxShadow, Div, FontWeight, Hsla, TestSupportExt};

use crate::components::ui;
use crate::i18n::t;
use crate::icons;
use crate::tokens::color;

use super::preset_icon;
use super::{
    ACTIONS_GAP, ACTIONS_H, ACTIONS_PAD_Y, BUTTON_GAP, BUTTON_H, BUTTON_ICON, BUTTON_TEXT,
    BUTTON_W, CARD_H, CARD_R, CARD_W, DESC_LINE_H, DESC_MT, DESC_TEXT, FOOTER_H, FOOTER_PAD_Y,
    HEADER_GAP, HEADER_H, HEADER_ICON, HEADER_ICON_BOX, HEADER_ICON_OFFSET, HEADER_PAD_Y,
    HEADER_ROW_H, HEADING_LINE_H, HEADING_TEXT, KBD_PAD_X, KBD_PAD_Y, PAD_X, PRESETS, PRESETS_H,
    PRESETS_HEADING_MB, PRESETS_PAD_Y, PRESET_ROW_GAP, PROMPT_TEXT, R3, R4, TAG_LINE_H, TAG_TEXT,
    TILE_GAP, TILE_H, TILE_ICON, TILE_ICON_BOX, TILE_LABEL_LINE_H, TILE_LABEL_TEXT, TILE_PAD,
    TILE_W, TITLE_LINE_H, TITLE_TEXT,
};
/// The `<section>` itself: 520x352, 6px corners, a 1px [`color::BORDER`] edge
/// and a [`color::PANEL`] fill, over `0 12px 32px rgba(0,0,0,0.8)`.
///
/// `overflow_hidden` stands in for nothing the source has - the card's content
/// is exactly 350 tall - but it keeps a stray flex rounding step from painting
/// past the 6px corners.
pub(super) fn card() -> Div {
    card_shell(actions(), presets())
}

/// The production card: same shell, with each control wired to the action the
/// the card attaches to it.
pub(super) fn card_live(cx: &gpui_kit::Context<crate::app::App>) -> Div {
    card_shell(actions_live(cx), presets_live(cx))
}

fn card_shell(actions: impl IntoElement, presets: impl IntoElement) -> Div {
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
        .child(actions)
        .child(presets)
        .child(footer())
}

/// The CSS shadow `0 12px 32px rgba(0,0,0,0.8)`.
///
/// The parameters are exact; the *curve* is not. The reference capture's
/// coverage at the card's own edge reads about 0.4 (`#090909` over a `#0f0f0f`
/// canvas, 3px out, symmetric on both sides), because a finite blurred rect
/// offset 12px down never reaches full coverage at its edge - and GPUI
/// approximates the blur differently again. `empty-card.png` is cropped to the
/// card, so it contains no shadow at all; only `empty-full.png` shows this, 12px
/// below and 30px either side of it.
fn card_shadow() -> Vec<BoxShadow> {
    vec![BoxShadow::new(px(0.0), px(12.0), hsla(0.0, 0.0, 0.0, 0.8)).blur_radius(px(32.0))]
}

/// The header block: a bottom border, 20px horizontal and 16px vertical
/// padding.
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
                // Centred on one row, resolved by hand: the icon box would sit at
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
                            &t("welcome.title"),
                            TITLE_TEXT,
                            TITLE_LINE_H,
                            FontWeight(600.),
                            color::TEXT,
                        ))
                        .child(
                            text_line(
                                &t("welcome.description"),
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

/// The two action buttons: a two-column grid, 10px gap, 20px horizontal and 14px vertical padding.
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
            &t("welcome.openImage"),
            color::ACCENT,
            color::WHITE,
            false,
        ))
        .child(action_button(
            icons::PLUS,
            &t("welcome.newDocument"),
            color::FIELD,
            color::TEXT,
            true,
        ))
}

/// The two action buttons, wired: Open Image runs the production open action
/// (the same one `Ctrl+O` and the File menu run) and New Document raises the
/// dialog. The card binds them to opening an image and
/// `handleNewCanvas`.
fn actions_live(cx: &gpui_kit::Context<crate::app::App>) -> Div {
    h_flex()
        .w_full()
        .h(px(ACTIONS_H))
        .flex_shrink_0()
        .items_stretch()
        .px(px(PAD_X))
        .py(px(ACTIONS_PAD_Y))
        .gap(px(ACTIONS_GAP))
        .child(
            action_button(
                icons::FOLDER_PLUS,
                &t("welcome.openImage"),
                color::ACCENT,
                color::WHITE,
                false,
            )
            .id("empty-open-image")
            .test_support()
            .on_click(cx.listener(
                |app,
                 _: &gpui_kit::ClickEvent,
                 _window,
                 cx: &mut gpui_kit::Context<crate::app::App>| {
                    app.open_document_action(&crate::app::OpenDocument, cx);
                },
            )),
        )
        .child(
            action_button(
                icons::PLUS,
                &t("welcome.newDocument"),
                color::FIELD,
                color::TEXT,
                true,
            )
            .id("empty-new-document")
            .test_support()
            .on_click(cx.listener(
                |app,
                 _: &gpui_kit::ClickEvent,
                 _window,
                 cx: &mut gpui_kit::Context<crate::app::App>| {
                    app.open_new_document(cx);
                },
            )),
        )
}

/// One action button.
///
/// The first uses the accent fill with white text and no border; the second is
/// a field fill and text colour with a border, and
/// that the field-border token resolves to [`color::BORDER`] like every
/// other border in the app rather than to [`color::FIELD_BORDER`].
///
/// Both are 36px tall, contents centred with an 8px gap and 12px side padding, so the icon and the label
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

/// The Quick Presets block: a top border, 20px horizontal and 14px vertical
/// padding.
///
/// 166px = 1 + 14 + 17 + 10 + 51 + 8 + 51 + 14. Two things are worth keeping
/// in mind when this block is read again: the heading's line box is 17 rather
/// than the 16.5 that `11 x 1.5` gives, and the tiles are 51 rather than the
/// 49 that 8px padding plus two default line heights suggest. Both are what the
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
                &t("welcome.quickPresets"),
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

/// The Quick Presets block, wired: each tile creates its blank white document
/// the tile advertises.
fn presets_live(cx: &gpui_kit::Context<crate::app::App>) -> Div {
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
                &t("welcome.quickPresets"),
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
                .child(preset_row_live(0, 1, cx))
                .child(preset_row_live(2, 3, cx)),
        )
}

/// One row of the two-column presets grid - two 235px tiles 8px apart.
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

fn preset_row_live(first: usize, second: usize, cx: &gpui_kit::Context<crate::app::App>) -> Div {
    h_flex()
        .w_full()
        .h(px(TILE_H))
        .flex_shrink_0()
        .items_stretch()
        .gap(px(PRESET_ROW_GAP))
        .child(preset_tile_live(first, cx))
        .child(preset_tile_live(second, cx))
}

/// One preset tile: a 10px-gap row, 4px corners, a border, 8px padding,
/// border-editor-field-border bg-editor-field p-2`.
///
/// 51px = 2 borders + 16 padding + 33 of content, and the 33 is an 18px label
/// line over a 15px tag line. The 28px icon box centres in that 33 at +2.5,
/// which GPUI rounds the same way the capture does - so unlike the header's
/// icon box, this one is left to `items_center`.
fn preset_tile(index: usize) -> Div {
    preset_tile_shell(index)
}

fn preset_tile_live(index: usize, cx: &gpui_kit::Context<crate::app::App>) -> impl IntoElement {
    preset_tile_shell(index)
        .id(format!("empty-preset-{index}"))
        .test_support()
        .on_click(cx.listener(
            move |app,
                  _: &gpui_kit::ClickEvent,
                  _window,
                  cx: &mut gpui_kit::Context<crate::app::App>| {
                app.create_preset_document(index, cx);
            },
        ))
}

fn preset_tile_shell(index: usize) -> Div {
    let (label, tag, _width, _height) = PRESETS[index];

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

/// The footer: a space-between row above a top border,
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
                .child(t("welcome.dropPromptDefault")),
        ))
        .child(ui::pin(kbd()))
}

/// `<kbd class="font-sans rounded-[3px] border border-editor-divider bg-editor-field` - trace-exempt:
/// `px-1.5 py-0.5 text-[10px] text-editor-text-dim">` - trace-exempt: verbatim source kbd markup.
///
/// Measured 44x21 at crop x 456..499, y 318..338, with the "Ctrl+O" run's ink
/// at x 463..492 - which is `456 + 1 (border) + 6 (left padding)`, so the box is
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
                .child("Ctrl+O"), // i18n-exempt: shortcut chord
        )
}

/// A text run pinned to an explicit line-box height.
///
/// **Worth consolidating.** An explicit `Npx` font size here resolves to a line
/// box of 1.5N, and this card depends on those heights for four of its block
/// sizes, so each run is given the height it was measured at and centred in
/// it. Centring is enough rather than a computed baseline offset: a glyph's
/// baseline lands at `box_centre + (ascent - descent) / 2` whatever the
/// line-height, because half-leading is symmetric - which is also why the
/// button labels need no explicit height even though the previous renderer gave
/// them a 19.5px line box inside a 36px button.
///
/// Four other modules spell this out inline at the call site
/// (`text_size` + `font_family` + `font_weight` + `text_color` on a fixed-height
/// `div`), so this is the natural thing to hoist into `ui.rs` if a fifth
/// arrives.
///
/// `whitespace_nowrap` stands in for the preset labels' clip-with-ellipsis and
/// for the single-line spans; nothing clips at these widths, so
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
