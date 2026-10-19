//! Visual-only port of `editor/AdjustmentsPanel.tsx` (549 lines).
//!
//! # Geometry: 272 wide, x 833..1104, y 84..795
//!
//! Same slot as `properties_panel` — the two are the InspectorDock's
//! contents, swapping on the tab bar — so this panel is the leftmost
//! 272px of `dock-adjustments.png` below its 38px tab bar: crop
//! x 0..271, crop y 38..749. Every number below was read back out of
//! that PNG rather than derived from the source's class names.
//!
//! ```text
//! tab bar bottom border  crop y=37        #333333, full width
//! section 1              crop y=37..137   "Selected Layer"
//!   SectionHeader        text crop y=51..63
//!   thumb box            crop y=75..125, x 13..253   FIELD fill, #333333 border
//!     thumb 34x34        crop x 22..55, y 83..117    checkerboard, #333333 border
//!     name               crop y 87..98                 11.5px medium, TEXT
//!     type + size        crop y 104..113               10.5px, TEXT_DIM
//! section 1 divider      crop y=137
//! section 2              crop y=137..384  "Basic"
//!   SectionHeader        text crop y=151..163 ("Basic" x38..65)
//!     reset icon         ink x243..247, y154..161      (disabled, 40% opacity)
//!   slider rows          labels crop y185..196 / 221..230 / 257..266
//!     tracks (4px)       crop y188..191 / 224..227 / 260..263
//!     thumbs (12px)      centred at crop x=142, y=189.5 / 225.5 / 261.5
//!     value boxes        crop x215..254, y179..200 / 216..235 / 252..271
//!       "0"              ink x241..246 in each box
//!   description          crop y291..300 + y306..313    (two wrapped lines)
//!   StatusHint box       crop y326..372; icon ink x26..40, y338..349;
//!                        text y338..347 + y353..362
//! section 2 divider      crop y=384
//! 5 collapsed sections   crop y=384..599, 42px + 1px border each:
//!   icon box             crop x16..32 (icon 15px, ink x19..31)
//!   label                12.5px TEXT from crop x~43
//!   chevron              ink x240..246
//! panel content ends     crop y=599 — the last 113px of the 712px
//!                        column is empty PANEL; `flex-1` on the scroll
//!                        column is all that is needed.
//! ```
//!
//! # What state the baseline caught
//!
//! `RightDock.tsx:46-49` returns the adjustments sub-view unless the
//! inspector tab is "presets", and the capture has the Adjustments tab
//! active (its accent underline sits at crop y35..36, x79..168), so
//! this panel — not the fallback — is what was captured.
//!
//! A layer *is* selected, but it has no `imageBitmap`: "Background", a
//! 100×100 raster. That decides the resting content in three places —
//! the `LayerThumb` shows the empty checkerboard (its canvas draws
//! nothing over it), `basicStatusText()` returns the "no pixels"
//! message so the `StatusHint` box renders inside Basic, and the three
//! sliders sit at 0 with the reset button disabled. The five
//! `CollapsibleSection`s start collapsed (`open` is `false`), so only
//! their 42px header rows appear; the descriptions in
//! `COMING_SOON_SECTIONS` are not rendered at all.
//!
//! # Known gaps, specific to this panel
//!
//! * `LayerThumb`'s canvas alternates `rgba(255,255,255,0.05)` and
//!   `rgba(255,255,255,0.1)` in 6px cells over FIELD (`#252525`),
//!   which composites to `#303030` / `#3b3b3b`. GPUI's `checkerboard()`
//!   takes one colour, so the port paints the 50/50 mean (`#353535`,
//!   [`CHECKER_MEAN`]). The layer's own pixels are moot here: the
//!   captured layer has no bitmap, so the empty checkerboard *is* the
//!   resting content.
//! * The reset button is *disabled* at rest (no pending adjustment),
//!   so `disabled:opacity-40` applies — that is a resting state and is
//!   painted, at 40% opacity. Its `hover:bg-white/[0.045]` /
//!   `hover:text-editor-text` and the collapsible rows'
//!   `hover:bg-white/[0.03]` are interaction states and are omitted,
//!   as are the slider thumbs' `hover:scale-110` and the
//!   `transition-colors` on the status box.
//! * `tabular-nums` on the slider value boxes: GPUI has no
//!   `font-variant-numeric`, so Inter's default figures stand in.
//! * `Tooltip` wrappers (the collapsible icons carry tooltips in the
//!   interactive app) are not reproducible in a static port.
//! * Carried from the standard gap list: no letter-spacing, no
//!   clip-path, two-stop gradients only (the five-stop saturation
//!   track is already split by `ui::gradient_segments`), no
//!   translate/z-index/scale, padding rounds to whole px, and ±1px on
//!   odd-height centring (the 22px value boxes centre in 28px rows).
//!
//! # Two cascade facts this panel depends on
//!
//! Both are verified for `tokens.rs`; they are restated because this
//! panel leans on them twice:
//!
//! * Every `border-*` utility loses to the unlayered
//!   `* { border-color: var(--color-border) }` rule in `styles.css`,
//!   so every border here paints `#333333` — [`color::BORDER`], not
//!   the `border-editor-divider` / `border-editor-field-border` /
//!   `border-editor-accent` the class names ask for. The PNG agrees:
//!   all seven divider rows and every box border scan as `#333333`.
//! * Every `overflow-y-auto` reserves 6px for `scrollbar-gutter:
//!   stable` whether or not it overflows (`styles.css:321`), so the
//!   scroll column is `.pr(px(6.))` and content ends at crop x=254,
//!   not x=259.

use crate::assets::icons;
use crate::components::ui;
use crate::i18n::t;
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
///
/// Unused in `render()` — the parent dock sets size via `.w_full().h_full()` —
/// so it documents the baseline geometry rather than driving layout.
#[expect(
    dead_code,
    reason = "documents baseline geometry; parent dock sets size via w_full()/h_full()"
)]
const PANEL_H: f32 = 712.0;

/// `px-3` — each section's horizontal inset, measured as the first
/// ink pixel at crop x=12.
const PAD_X: f32 = 12.0;

/// `py-2.5` — the inset around each section's content.
const PAD_Y: f32 = 10.0;

/// `mt-2` between a `SectionHeader` and the block under it.
const HEADER_GAP: f32 = 8.0;

/// `gap-2.5` between the thumbnail and its text, and between the
/// slider and the value box (`AdjustmentsPanel.tsx:44,506`).
const ROW_GAP: f32 = 10.0;

/// `p-2` inside the "Selected Layer" box.
const BOX_PAD: f32 = 8.0;

/// `LayerThumb`'s canvas is 34x34 (`LayerThumb.tsx:20`).
const THUMB: f32 = 34.0;

/// `gap-2` between the layer name and its type line — the source has
/// no gap and puts `mt-0.5` (2px) on the second `<p>` instead
/// (`AdjustmentsPanel.tsx:397`).
const TYPE_MT: f32 = 2.0;

/// `rounded-[4px]` / `rounded-[3px]`.
const R4: f32 = 4.0;
const R3: f32 = 3.0;

/// `size-4` / `size-[15px]` — `SectionHeader`'s icon box and icon
/// (`SectionHeader.tsx:15-20`).
const HDR_ICON_BOX: f32 = 16.0;
const HDR_ICON: f32 = 15.0;

/// `text-[12.5px] font-medium` — `SectionHeader`'s label.
const HDR_TEXT: f32 = 12.5;

/// `text-[11.5px]` / `text-[10.5px]` — the layer name and type
/// (`AdjustmentsPanel.tsx:394,397`).
const NAME_TEXT: f32 = 11.5;
const TYPE_TEXT: f32 = 10.5;

/// `w-[58px]` — an `AdjustmentSliderRow`'s label.
const SLIDER_LABEL_W: f32 = 58.0;

/// `text-[12px]` — the slider row's label.
const SLIDER_LABEL: f32 = 12.0;

/// `min-h-[28px]` — the slider row, and `h-[18px]` — the slider's
/// wrapper (`AdjustmentsPanel.tsx:502,507`).
const SLIDER_ROW_H: f32 = 28.0;
const SLIDER_WRAP_H: f32 = 18.0;

/// `h-[22px] w-[40px]` and `px-1.5` — the slider's value box
/// (`AdjustmentsPanel.tsx:527`).
const VALUE_W: f32 = 40.0;
const VALUE_H: f32 = 22.0;
const VALUE_PAD_X: f32 = 6.0;

/// `text-[11px]` — the value box's readout.
const VALUE_TEXT: f32 = 11.0;

/// `mt-1` above the "Drag to preview…" paragraph, and its
/// `text-[11px]`.
const DESC_MT: f32 = 4.0;
const DESC_TEXT: f32 = 11.0;

/// `gap-2` / `px-2.5` / `py-2` / `size-3.5` — the `StatusHint`
/// box, its icon and its inset (`AdjustmentsPanel.tsx:479-484`).
const HINT_GAP: f32 = 8.0;
const HINT_PAD_X: f32 = 10.0;
const HINT_PAD_Y: f32 = 8.0;
const HINT_ICON: f32 = 14.0;

/// `text-[11px]` — the `StatusHint`'s message.
const HINT_TEXT: f32 = 11.0;

/// `h-[42px]` / `px-4` / `gap-2.5` / `size-4` / `size-[15px]` /
/// `text-[12.5px]` / `size-4` — a `CollapsibleSection`'s header row
/// and its parts (`AdjustmentsPanel.tsx:548-564`).
const COLLAPSE_H: f32 = 42.0;
const COLLAPSE_PX: f32 = 16.0;
const COLLAPSE_GAP: f32 = 10.0;
const COLLAPSE_ICON_BOX: f32 = 16.0;
const COLLAPSE_ICON: f32 = 15.0;
const COLLAPSE_TEXT: f32 = 12.5;
const CHEVRON: f32 = 16.0;

/// `size-5` / `size-3.5` — the Basic header's reset button and its
/// icon (`AdjustmentsPanel.tsx:414,416`).
const RESET: f32 = 20.0;
const RESET_ICON: f32 = 14.0;

/// The mean of `LayerThumb`'s two checkerboard tones, as a solid
/// colour.
///
/// The source alternates `rgba(255,255,255,0.05)` and
/// `rgba(255,255,255,0.1)` in 6px cells over FIELD (`#252525`), which
/// composites to `#303030` and `#3b3b3b` — an exact 50/50 split of
/// the 6x6 cell grid, so the mean is `#353535`. GPUI's
/// `checkerboard()` takes one colour where the source needs two, so the
/// port paints the mean and the alternation is a documented gap rather
/// than an approximation someone could mistake for the real thing.
const CHECKER_MEAN: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.208,
    a: 1.0,
};

// ── Ad-hoc icon colours, not `--editor-*` tokens ─────────────────
//
// `COMING_SOON_SECTIONS` colours its icons with the raw Tailwind
// palette (`text-sky-400` etc.). The app's `@theme` block does not
// override those, so the values below are Tailwind v4's own
// `oklch(...)` definitions, run through the same OKLab -> sRGB -> HSL
// path `tokens.rs` documents. Each was cross-checked against the
// baseline: the icon's max-chroma pixel scans as the hex quoted.

/// `text-sky-400` — the "HSL / Color" icon.
///
/// Tailwind's `sky-400` is `oklch(0.746 0.16 232.661)` = `#00bcff`,
/// i.e. HSL(195.8°, 100%, 50%). Measured core: `#00bcff`.
const SKY_400: Hsla = Hsla {
    h: 0.543791,
    s: 1.0,
    l: 0.5,
    a: 1.0,
};

/// `text-sky-300` — the "Detail" icon.
///
/// Tailwind's `sky-300` is `oklch(0.828 0.111 230.318)` = `#74d4ff`,
/// i.e. HSL(198.6°, 100%, 72.7%). Measured core: `#74d4ff`.
const SKY_300: Hsla = Hsla {
    h: 0.551559,
    s: 1.0,
    l: 0.727451,
    a: 1.0,
};

/// `text-amber-400` — the "Color Grading" icon.
///
/// Tailwind's `amber-400` is `oklch(0.828 0.189 84.429)` = `#ffb900`,
/// i.e. HSL(43.5°, 100%, 50%). Measured core: `#fcb701`, within the
/// anti-aliasing tolerance of a 1.75px stroke.
const AMBER_400: Hsla = Hsla {
    h: 0.120915,
    s: 1.0,
    l: 0.5,
    a: 1.0,
};

/// `text-emerald-400` — the "Lens Corrections" icon.
///
/// Tailwind's `emerald-400` is `oklch(0.765 0.177 163.223)` =
/// `#00d492`, i.e. HSL(161.3°, 100%, 41.6%). Measured core: `#01d291`,
/// within anti-aliasing tolerance.
const EMERALD_400: Hsla = Hsla {
    h: 0.448113,
    s: 1.0,
    l: 0.415686,
    a: 1.0,
};

/// Format a slider's `displayValue` — `value > 0 ? "+${value}" : "${value}"`
/// (`AdjustmentsPanel.tsx:530`). Extracted so the one-branch formatting
/// logic is unit-testable without a GPUI build tree.
pub fn format_display_value(value: i32) -> String {
    if value > 0 {
        format!("+{}", value)
    } else {
        value.to_string()
    }
}

pub fn render() -> impl IntoElement {
    v_flex()
        .w_full()
        .h_full()
        .overflow_hidden()
        .bg(color::PANEL)
        .child(
            v_flex()
                .flex_1()
                .overflow_hidden()
                // `scrollbar-gutter: stable` reserves 6px on the trailing
                // edge of every `overflow-y-auto`, even with nothing to
                // scroll — `styles.css:321` and `:326`. GPUI has no
                // scrollbars, so the reservation has to be made
                // explicitly or the rows come out 6px too wide.
                .child(selected_layer_section())
                .child(basic_section())
                // `COMING_SOON_SECTIONS`, in order. All five are
                // collapsed in the capture, so only the header rows
                // render; the descriptions are not painted (see the
                // module doc's "What state the baseline caught").
                .child(coming_soon_section(
                    icons::with_stroke(icons::SPLINE, 1.75),
                    color::TEXT_DIM,
                    &t("adjustments.toneCurve"),
                ))
                .child(coming_soon_section(
                    icons::with_stroke(icons::PALETTE, 1.75),
                    SKY_400,
                    &t("adjustments.hslColor"),
                ))
                .child(coming_soon_section(
                    icons::with_stroke(icons::SWATCH, 1.75),
                    AMBER_400,
                    &t("adjustments.colorGrading"),
                ))
                .child(coming_soon_section(
                    icons::with_stroke(icons::SPARKLES, 1.75),
                    SKY_300,
                    &t("adjustments.detail"),
                ))
                .child(coming_soon_section(
                    icons::with_stroke(icons::APERTURE, 1.75),
                    EMERALD_400,
                    &t("adjustments.lensCorrections"),
                )),
        )
}

/// `SectionHeader` (`layers/SectionHeader.tsx`) — a 16px icon box
/// holding a 15px icon, an 8px gap, then a 12.5px medium label, with
/// an optional trailing element on the right of the same row
/// (`justify-between`).
///
/// `SectionHeader` passes `strokeWidth={1.75}` at every call site, so
/// icons whose constant is pinned to another width come through
/// [`icons::with_stroke`] — see the call sites in [`render`].
fn section_header(icon: &'static [u8], label: &str, trailing: Option<Div>) -> Div {
    let mut row = h_flex().items_center().justify_between().child(
        h_flex()
            .items_center()
            .gap(px(ROW_GAP))
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
    );
    if let Some(t) = trailing {
        row = row.child(t);
    }
    row
}

/// `LayerThumb`'s frame: 34x34, 1px border, `rounded-[3px]`, checker
/// inside. The canvas bitmap the source draws is a *layer preview*,
/// which a static port cannot have — and the captured layer has no
/// `imageBitmap` at all, so this empty chrome is the correct resting
/// content, not a stub.
///
/// The border asks for `border-editor-accent` (the thumb is active,
/// `AdjustmentsPanel.tsx:431`), but the unlayered `*` rule repaints
/// every border to [`color::BORDER`] — the PNG scans the thumb's
/// outline as `#333333`, not the accent orange.
fn layer_thumb() -> Div {
    div()
        .w(px(THUMB))
        .h(px(THUMB))
        .rounded(px(R3))
        .border_1()
        .border_color(color::BORDER)
        // One flat colour where the source draws two alternating tones.
        .bg(CHECKER_MEAN)
        .flex_shrink_0()
}

/// The "Selected Layer" section: header, then the bordered
/// thumb+text box (`AdjustmentsPanel.tsx:386-401`).
fn selected_layer_section() -> Div {
    v_flex()
        .w_full()
        .flex_shrink_0()
        .px(px(PAD_X))
        .py(px(PAD_Y))
        .border_b(px(1.0))
        .border_color(color::BORDER)
        .child(section_header(
            icons::with_stroke(icons::LAYERS, 1.75),
            &t("adjustments.selectedLayer"),
            None,
        ))
        .child(
            div()
                .mt(px(HEADER_GAP))
                .flex()
                .items_center()
                .gap(px(ROW_GAP))
                .rounded(px(R4))
                .border_1()
                .border_color(color::BORDER)
                .bg(color::FIELD)
                .p(px(BOX_PAD))
                .child(layer_thumb())
                .child(
                    v_flex()
                        .flex_1()
                        .min_w(px(0.0))
                        .child(
                            div()
                                .text_size(px(NAME_TEXT))
                                .font_family(ui::FONT_UI)
                                .font_weight(FontWeight(500.))
                                // `leading-tight`.
                                .line_height(relative(1.25))
                                .text_color(color::TEXT)
                                // Measured in the baseline, not the
                                // placeholder "Layer 1" the properties
                                // port uses: this capture's layer is
                                // named "Background".
                                .child("Background".to_string()), // i18n-exempt: demo content <layer name, captured frame>
                        )
                        // `mt-0.5` = 2px, and `leading-snug`.
                        .child(
                            div()
                                .mt(px(TYPE_MT))
                                .text_size(px(TYPE_TEXT))
                                .font_family(ui::FONT_UI)
                                .line_height(relative(1.375))
                                .text_color(color::TEXT_DIM)
                                // Raster layers read "Image layer"
                                // here, unlike the properties panel's
                                // "Raster layer" (`AdjustmentsPanel.tsx:399`).
                                .child("Image layer \u{00b7} 100 \u{00d7} 100 px".to_string()), // i18n-exempt: demo content <layer type+size, captured frame>
                        ),
                ),
        )
}

/// The "Basic" section: header with its reset button, three slider
/// rows, the hint paragraph and the "no pixels" `StatusHint`
/// (`AdjustmentsPanel.tsx:403-456`).
fn basic_section() -> Div {
    v_flex()
        .w_full()
        .flex_shrink_0()
        .px(px(PAD_X))
        .py(px(PAD_Y))
        .border_b(px(1.0))
        .border_color(color::BORDER)
        .child(section_header(
            icons::with_stroke(icons::SUN, 1.75),
            &t("adjustments.basic"),
            Some(reset_button()),
        ))
        .child(
            v_flex()
                .mt(px(HEADER_GAP))
                .gap(px(8.0))
                .w_full()
                .child(slider_row(
                    &t("adjustments.bright"),
                    ui::SliderKind::Brightness,
                ))
                .child(slider_row(
                    &t("adjustments.contrast"),
                    ui::SliderKind::Contrast,
                ))
                .child(slider_row(
                    &t("adjustments.saturate"),
                    ui::SliderKind::Saturation,
                ))
                .child(
                    div()
                        .mt(px(DESC_MT))
                        .text_size(px(DESC_TEXT))
                        .font_family(ui::FONT_UI)
                        // `leading-snug`.
                        .line_height(relative(1.375))
                        .text_color(color::TEXT_DIM)
                        .child(t("adjustments.dragToPreview")),
                )
                .child(status_hint(
                    "This layer has no pixels yet. Add image pixels before adjusting tone.", // i18n-exempt: not in catalog
                )),
        )
}

/// One `AdjustmentSliderRow` (`AdjustmentsPanel.tsx:490-533`): a
/// 58px label, the slider, and the 40px value box.
///
/// This is *not* a [`ui::prop_row`]: the source builds the row inline
/// with its own geometry — a 58px `text-[12px] font-medium`
/// `text-editor-text-dim` label where `PropRow` uses 50px of 11px
/// `text-[#A1A1AA]`.
fn slider_row(label: &str, kind: ui::SliderKind) -> Div {
    h_flex()
        .min_h(px(SLIDER_ROW_H))
        .items_center()
        .gap(px(ROW_GAP))
        .child(
            div()
                .w(px(SLIDER_LABEL_W))
                .flex_none()
                // Same reasoning as ui::prop_row's label: hold the measured
                // width, but clip a caption that does not fit rather than
                // letting it run under the track beside it.
                .overflow_hidden()
                .text_size(px(SLIDER_LABEL))
                .font_family(ui::FONT_UI)
                .font_weight(FontWeight(500.))
                .text_color(color::TEXT_DIM)
                .child(label.to_string()),
        )
        .child(
            h_flex()
                .flex_1()
                .items_center()
                .gap(px(ROW_GAP))
                // `relative flex h-[18px] flex-1 items-center` — the
                // wrapper exists to host the source's invisible range
                // input; here it just centres the 4px slider in 18px.
                .child(
                    h_flex()
                        .flex_1()
                        .h(px(SLIDER_WRAP_H))
                        .items_center()
                        // Centre-origin slider at rest: `percent =
                        // (0 + 100) / 2` = 50, `value = 0`, so the
                        // fill has zero width and only the centre tick
                        // and the centred thumb show. `accent` is
                        // unused by the centre-origin kinds.
                        .child(ui::slider(50.0, Some(0.0), kind, true)),
                )
                .child(value_box(&format_display_value(0))),
        )
}

/// The slider's readout — `flex h-[22px] w-[40px] items-center
/// justify-end rounded-[3px] border border-editor-field-border
/// bg-editor-field px-1.5 text-right text-[11px] tabular-nums
/// text-editor-text` (`AdjustmentsPanel.tsx:527`).
///
/// Deliberately *not* a [`ui::num_field`]: that box is 24px tall,
/// `rounded-[4px]`, and pairs a 10px caption with a mono semibold
/// value. This one is 22px, caption-free, Inter, `rounded-[3px]` and
/// right-aligned — the two share nothing but being a bordered FIELD
/// box. `displayValue` is `value > 0 ? "+${value}" : "${value}"`,
/// which is a bare `"0"` at rest.
fn value_box(value: &str) -> Div {
    h_flex()
        .w(px(VALUE_W))
        .h(px(VALUE_H))
        .flex_none()
        .items_center()
        .justify_end()
        .rounded(px(R3))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .px(px(VALUE_PAD_X))
        // 22px is sized for the bare "0" the panel rests at. Once a slider
        // moves, displayValue becomes "+8" or "-12", and without a clip those
        // glyphs render straight through the right-hand border.
        .overflow_hidden()
        .child(
            div()
                .text_size(px(VALUE_TEXT))
                .font_family(ui::FONT_UI)
                .text_right()
                .text_color(color::TEXT)
                .child(value.to_string()),
        )
}

/// The reset button in the Basic header — `flex size-5 items-center
/// justify-center rounded-[3px] text-editor-text-dim ...
/// disabled:pointer-events-none disabled:opacity-40`
/// (`AdjustmentsPanel.tsx:409-417`).
///
/// It is **disabled at rest**: `hasPendingAdjustment()` is false with
/// all three sliders at 0, so `disabled:opacity-40` is a resting
/// state and is painted — the icon renders at 40% of
/// [`color::TEXT_DIM`]. The hover styles are interaction states and
/// are omitted per the module-gap list.
fn reset_button() -> Div {
    div()
        .w(px(RESET))
        .h(px(RESET))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(R3))
        .flex_shrink_0()
        .opacity(0.4)
        .child(
            // `strokeWidth={1.75}` matches the width `icons::X` is
            // pinned to, so no `with_stroke` is needed.
            Icon::default()
                .data(icons::X)
                .size(px(RESET_ICON))
                .text_color(color::TEXT_DIM),
        )
}

/// `StatusHint` (`AdjustmentsPanel.tsx:477-488`) — the "no pixels"
/// notice. `items-start` because the message wraps to two lines while
/// the icon stays on the first (`mt-0.5` = 2px).
///
/// Rendered because the captured layer has no `imageBitmap`, so
/// `basicStatusText()` returns the message rather than `null`.
fn status_hint(message: &str) -> Div {
    h_flex()
        .items_start()
        .gap(px(HINT_GAP))
        .rounded(px(R4))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .px(px(HINT_PAD_X))
        .py(px(HINT_PAD_Y))
        .child(
            // `strokeWidth={1.75}` matches the width `icons::SLIDERS`
            // is pinned to, so no `with_stroke` is needed.
            Icon::default()
                .data(icons::SLIDERS)
                .size(px(HINT_ICON))
                .text_color(color::TEXT_DIM)
                .mt(px(TYPE_MT))
                .flex_shrink_0(),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .overflow_hidden()
                .text_size(px(HINT_TEXT))
                .font_family(ui::FONT_UI)
                // `leading-snug`.
                .line_height(relative(1.375))
                .text_color(color::TEXT_DIM)
                .child(message.to_string()),
        )
}

/// One collapsed `CollapsibleSection` (`AdjustmentsPanel.tsx:535-581`)
/// — a 42px header row: a 16px box holding a 15px coloured icon, an
/// 8px gap, a 12.5px label, and a 16px chevron-right on the right.
///
/// The label is plain `text-editor-text` — *no* `font-medium`, unlike
/// `SectionHeader`'s label, which is the one typographic difference
/// between the two row kinds.
///
/// `open` starts `false`, so the description box the source shows when
/// expanded never renders in the baseline and is not ported. The
/// chevron is `chevron-right` for the same reason (the expanded form
/// would swap in `chevron-up`).
fn coming_soon_section(icon: &'static [u8], icon_colour: Hsla, label: &str) -> Div {
    v_flex()
        .w_full()
        .flex_shrink_0()
        .border_b(px(1.0))
        .border_color(color::BORDER)
        .child(
            div()
                .h(px(COLLAPSE_H))
                .w_full()
                .flex()
                .items_center()
                .justify_between()
                .px(px(COLLAPSE_PX))
                .child(
                    h_flex()
                        .items_center()
                        .gap(px(COLLAPSE_GAP))
                        .child(
                            div()
                                .w(px(COLLAPSE_ICON_BOX))
                                .h(px(COLLAPSE_ICON_BOX))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    // `strokeWidth={1.75}` at the call
                                    // site; each constant is pinned to
                                    // its primary consumer's width, so
                                    // the ones that differ come through
                                    // `with_stroke` — see `render`.
                                    Icon::default()
                                        .data(icon)
                                        .size(px(COLLAPSE_ICON))
                                        .text_color(icon_colour),
                                ),
                        )
                        .child(
                            div()
                                .text_size(px(COLLAPSE_TEXT))
                                .font_family(ui::FONT_UI)
                                .text_color(color::TEXT)
                                .child(label.to_string()),
                        ),
                )
                // `strokeWidth={1.75}` matches the width
                // `icons::CHEVRON_RIGHT` is pinned to.
                .child(
                    Icon::default()
                        .data(icons::CHEVRON_RIGHT)
                        .size(px(CHEVRON))
                        .text_color(color::TEXT_DIM),
                ),
        )
}

#[cfg(test)]
mod tests {
    use super::format_display_value;

    /// `format_display_value` mirrors the source's `displayValue` ternary:
    /// positive values get a leading `+`, everything else (including 0)
    /// is bare. RED proof: flip the `>` to `<` and the positive assertion
    /// flips from `"+5"` to `"5"`.
    #[test]
    fn format_display_value_adds_plus_for_positive() {
        assert_eq!(format_display_value(0), "0");
        assert_eq!(format_display_value(5), "+5");
        assert_eq!(format_display_value(-5), "-5");
        assert_eq!(format_display_value(100), "+100");
    }
}
