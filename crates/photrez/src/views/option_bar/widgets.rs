use super::*;
use crate::i18n::t;

// ── Move-variant parts ────────────────────────────────────────────────────────

/// `<div class="flex shrink-0 items-center gap-1">` around the X and Y
/// fields (`MoveOptionBar.tsx:271`).
///
/// Both are identity-transform zeros in the capture: X=0, Y=0.
pub(super) fn position_group() -> impl IntoElement {
    h_flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(ITEM_GAP))
        .child(editable_field("num-x", "X", FIELD_W_XY, "0", "px")) // i18n-exempt: technical token (field label glyph)
        .child(editable_field("num-y", "Y", FIELD_W_XY, "0", "px")) // i18n-exempt: technical token (field label glyph)
}

/// `<div class="flex shrink-0 items-center gap-1">` around the W and H
/// fields (`MoveOptionBar.tsx:292`).
///
/// `capture-baseline.ts` builds an 800x600 document, so the background
/// layer's scaled size reads 800 and 600 — the two values in the baseline.
pub(super) fn size_group() -> impl IntoElement {
    h_flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(ITEM_GAP))
        .child(num_field("W", FIELD_W_WH, "800", "px")) // i18n-exempt: technical token (field label glyph)
        .child(num_field("H", FIELD_W_WH, "600", "px")) // i18n-exempt: technical token (field label glyph)
}

/// The brush size slider (`BrushOptionBar.tsx`), gated behind `PAINT_SIZE_SLIDER`.
pub(super) fn brush_size_slider(percent: f32) -> Div {
    div()
        .relative()
        .flex_shrink_0()
        .w(px(BRUSH_SLIDER_W))
        .h(px(BRUSH_SLIDER_H))
        .items_center()
        .child(ui::slider(percent, None, ui::SliderKind::BrushSize, true))
}

// ── Eyedropper ─────────────────────────────────────────────────────────────────

/// The eyedropper's foreground colour square.
pub(super) fn fg_swatch_square() -> Div {
    let mut shadows = vec![ring_white_20()];
    shadows.extend(shadow_sm_layers());
    div()
        .size(px(ICON_LG))
        .flex_shrink_0()
        .rounded(px(RADIUS_SM))
        .border_1()
        .border_color(color::BORDER)
        .bg(FG)
        .shadow(shadows)
}

/// The "click to copy HEX" badge.
pub(super) fn hex_badge() -> Stateful<Div> {
    div()
        .id("opt-copy-hex")
        .h(px(CTRL_H))
        .flex_shrink_0()
        .h_flex()
        .items_center()
        .gap(px(GAP))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .px(px(PILL_PX))
        .text_size(px(TEXT_SM))
        .font_weight(FontWeight(600.))
        .text_color(color::WHITE)
        .child(value_text("#E15A17"))
        .child(
            Icon::default()
                .data(icons::with_stroke(icons::COPY, 2.0))
                .size(px(ICON_SM))
                .text_color(ZINC),
        )
}

/// One of the five tolerance preset chips.
pub(super) fn tolerance_preset(
    id: &'static str,
    value: &'static str,
    active: bool,
) -> Stateful<Div> {
    let chip = div()
        .id(id)
        .h(px(CTRL_H))
        .flex_shrink_0()
        .h_flex()
        .items_center()
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .px(px(FIELD_PX))
        .font_family(ui::FONT_MONO)
        .text_size(px(TEXT_XS))
        .bg(if active { ACCENT_20 } else { FIELD_40 })
        .text_color(if active { color::WHITE } else { ZINC })
        .font_weight(if active {
            FontWeight(700.)
        } else {
            FontWeight(500.)
        });

    if active {
        chip.shadow(vec![
            BoxShadow::new(px(0.), px(1.), SHADOW_XS).blur_radius(px(2.))
        ])
    } else {
        chip
    }
    .child(value)
}

/// The tolerance slider wrapper (`PaintBucketOptionBar.tsx`).
pub(super) fn tolerance_slider() -> Div {
    div()
        .relative()
        .flex_shrink_0()
        .w(px(TOLERANCE_SLIDER_W))
        .items_center()
        .child(ui::slider(TOLERANCE_PCT, None, ui::SliderKind::Alpha, true))
}

// ── Gradient ───────────────────────────────────────────────────────────────────

/// The gradient preview strip (`GradientOptionBar.tsx`).
pub(super) fn gradient_strip() -> Div {
    let mut shadows = vec![BoxShadow::new(px(0.), px(0.), color::WHITE_10).spread_radius(px(1.))];
    shadows.extend(shadow_sm_layers());
    div()
        .relative()
        .flex_shrink_0()
        .h(px(STRIP_H))
        .w(px(STRIP_W))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .overflow_hidden()
        .bg(STRIP_BG)
        .shadow(shadows)
        .child(
            div()
                .absolute()
                .inset(px(0.))
                .bg(checkerboard(STRIP_CHECK, 8.)),
        )
        .child(div().absolute().inset(px(0.)).bg(linear_gradient(
            90.0,
            linear_color_stop(FG, 0.0),
            linear_color_stop(color::WHITE, 1.0),
        )))
}

/// A round gradient-end swatch.
pub(super) fn round_swatch(fill: Hsla) -> Div {
    let mut shadows = vec![ring_white_20()];
    shadows.extend(shadow_sm_layers());
    div()
        .size(px(ICON_MD))
        .flex_shrink_0()
        .rounded_full()
        .border_1()
        .border_color(color::BORDER)
        .bg(fill)
        .shadow(shadows)
}

/// The gradient bar's Reverse button.
pub(super) fn reverse_button() -> Stateful<Div> {
    div()
        .id("opt-reverse")
        .h(px(CTRL_H))
        .flex_shrink_0()
        .h_flex()
        .items_center()
        .gap(px(GAP))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(FIELD_40)
        .px(px(PILL_PX))
        .text_size(px(TEXT_SM))
        .font_weight(FontWeight(600.))
        .text_color(ZINC)
        .child(div().text_size(px(TEXT_MD)).child("\u{21c4}")) // i18n-exempt: glyph or symbol
        .child(div().child(t("tools.options.reverse")))
}

// ── Shape ──────────────────────────────────────────────────────────────────────

/// The shape bar's Fill toggle, resting on.
pub(super) fn fill_toggle() -> Stateful<Div> {
    div()
        .id("opt-fill")
        .h(px(CTRL_H))
        .flex_shrink_0()
        .h_flex()
        .items_center()
        .gap(px(ITEM_GAP))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(ACCENT_20)
        .px(px(PILL_PX))
        .text_size(px(TEXT_SM))
        .font_weight(FontWeight(600.))
        .text_color(color::WHITE)
        .shadow(vec![
            BoxShadow::new(px(0.), px(1.), SHADOW_XS).blur_radius(px(2.))
        ])
        .child(t("tools.fill"))
}

/// The shape bar's 22px fill colour swatch.
pub(super) fn fill_swatch() -> Stateful<Div> {
    div()
        .id("opt-fill-color")
        .size(px(SWATCH_FILL))
        .flex_shrink_0()
        .rounded(px(RADIUS_SM))
        .border_1()
        .border_color(color::BORDER)
        .bg(FG)
        .shadow(vec![ring_white_20()])
}

/// The stroke pill's off-state dot.
pub(super) fn stroke_dot() -> Div {
    div()
        .size(px(DOT))
        .flex_shrink_0()
        .rounded_full()
        .border_1()
        .border_color(color::BORDER)
        .bg(STROKE_DOT)
}

/// The stroke pill in its resting off state.
pub(super) fn stroke_pill_off(id: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(CTRL_H))
        .w(px(STROKE_PILL_W))
        .flex_shrink_0()
        .h_flex()
        .items_center()
        .justify_between()
        .gap(px(GAP))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .px(px(PILL_PX))
        .text_size(px(TEXT_SM))
        .font_weight(FontWeight(500.))
        .child(ui::clip(
            h_flex()
                .items_center()
                .gap(px(GAP))
                .child(stroke_dot())
                .child(div().text_color(ZINC).child("Stroke:")) // i18n-exempt: not in catalog (catalog has "Stroke" without colon)
                .child(
                    div()
                        .w(px(STROKE_VAL_W))
                        .flex_none()
                        .font_family(ui::FONT_MONO)
                        .text_color(ZINC)
                        .child(t("common.off")),
                ),
        ))
        .child(ui::pin(div().child(chevron_175())))
}

/// The shape bar's corner-radius field, resting at 0.
pub(super) fn radius_field() -> Div {
    h_flex()
        .h(px(CTRL_H))
        .flex_shrink_0()
        .items_center()
        .gap(px(ITEM_GAP))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .px(px(FIELD_PX))
        .child(dim_span(&t("properties.cornerRadius")))
        .child(
            div()
                .w(px(RADIUS_INPUT_W))
                .flex_none()
                .text_center()
                .child(value_text("0")),
        )
        .child(dim_span("px")) // i18n-exempt: technical token (unit)
}

// ── Text ───────────────────────────────────────────────────────────────────────

/// The text bar's font-family trigger.
pub(super) fn font_family_button() -> Stateful<Div> {
    div()
        .id("opt-font-family")
        .h(px(CTRL_H))
        .flex_shrink_0()
        .h_flex()
        .items_center()
        .gap(px(GAP))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .px(px(PILL_PX))
        .text_size(px(TEXT_SM))
        .font_weight(FontWeight(500.))
        .child(
            div()
                .text_size(px(TEXT_13))
                .font_weight(FontWeight(700.))
                .text_color(color::ACCENT)
                .font_family("Arial") // i18n-exempt: technical token
                .child("Aa"), // i18n-exempt: glyph or symbol
        )
        .child(
            div()
                .font_family("Arial") // i18n-exempt: technical token
                .text_color(color::WHITE)
                .child("Arial"), // i18n-exempt: technical token
        )
        .child(ml_label(chevron_175()))
}

/// The text bar's font-size field, resting at 48.
pub(super) fn font_size_field() -> Div {
    h_flex()
        .h(px(CTRL_H))
        .flex_shrink_0()
        .items_center()
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .px(px(FIELD_PX))
        .child(
            div()
                .w(px(SIZE_INPUT_W))
                .flex_none()
                .text_center()
                .child(value_text("48")),
        )
        .child(dim_span("px")) // i18n-exempt: technical token (unit)
        .child(ml_label(chevron_175()))
}

/// The text bar's weight trigger, resting at "Regular".
pub(super) fn weight_button() -> Stateful<Div> {
    div()
        .id("opt-weight")
        .h(px(CTRL_H))
        .flex_shrink_0()
        .h_flex()
        .items_center()
        .gap(px(ITEM_GAP))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .px(px(PILL_PX))
        .text_size(px(TEXT_SM))
        .child(
            div()
                .w(px(WEIGHT_W))
                .flex_none()
                .font_weight(FontWeight(600.))
                .text_color(color::WHITE)
                .child(WEIGHT_LABEL),
        )
        .child(ml_label(chevron_175()))
}

/// A text bar toggle button, resting off.
pub(super) fn text_toggle_button(id: &'static str, glyph: Div) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(BTN))
        .w(px(BTN))
        .flex_shrink_0()
        .h_flex()
        .items_center()
        .justify_center()
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(FIELD_40)
        .text_color(ZINC)
        .child(glyph)
}

/// A text-align icon button, active or inactive.
pub(super) fn text_icon_button(
    id: &'static str,
    icon: &'static [u8],
    active: bool,
) -> Stateful<Div> {
    let btn = div()
        .id(id)
        .h(px(BTN))
        .w(px(BTN))
        .flex_shrink_0()
        .h_flex()
        .items_center()
        .justify_center()
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .text_size(px(TEXT_SM));

    let btn = if active {
        btn.bg(ACCENT_20)
            .font_weight(FontWeight(700.))
            .text_color(color::WHITE)
            .shadow(vec![
                BoxShadow::new(px(0.), px(1.), SHADOW_XS).blur_radius(px(2.))
            ])
    } else {
        btn.bg(FIELD_40).text_color(ZINC)
    };

    btn.child(
        Icon::default()
            .data(icons::with_stroke(icon, STROKE_DROPDOWN))
            .size(px(ICON_MD))
            .text_color(if active { color::ACCENT } else { ZINC }),
    )
}

/// The text bar's box-mode trigger, resting at "Auto Width".
pub(super) fn box_mode_button() -> Stateful<Div> {
    div()
        .id("opt-box-mode")
        .h(px(CTRL_H))
        .flex_shrink_0()
        .h_flex()
        .items_center()
        .justify_between()
        .gap(px(GAP))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .px(px(PILL_PX))
        .text_size(px(TEXT_SM))
        .font_weight(FontWeight(500.))
        .child(div().text_color(color::WHITE).child(BOX_MODE))
        .child(chevron_175())
}

/// The text bar's colour swatch.
pub(super) fn text_color_swatch() -> Stateful<Div> {
    div()
        .id("opt-text-color")
        .size(px(BTN))
        .flex_shrink_0()
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(FG)
        .shadow(vec![ring_white_20()])
}

// ── Shared hover colour ───────────────────────────────────────────────────────

/// `#4B515D` — the hover border for editable fields and the overflow button.
pub(super) const HOVER_BORDER_LOCAL: Hsla = Hsla {
    h: 0.611111,
    s: 0.107143,
    l: 0.329412,
    a: 1.0,
};

// ── Group labels ─�─────────────────────────────────────────────────────────────

/// The selection bar's caps captions ("Position", "Size", "Rotation").
pub(super) fn caps_label(text: &str) -> Div {
    div()
        .mr(px(MR_LABEL))
        .text_size(px(TEXT_XS))
        .font_weight(FontWeight(700.))
        .text_color(color::TEXT_DIM)
        .child(text.to_string())
}

// ── Crop ───────────────────────────────────────────────────────────────────────

/// `CropFillControls.tsx` (lines 528-552) — the white colour swatch + "Use BG"
/// button, shown when `fillEnabled()` is true. At the capture's rest state the
/// fill source resolves to the white background default (see
/// `resolvedCropFillColor`), so the swatch paints [`color::WHITE`].
///
/// The swatch is a 22x18 rounded rect (`h-[18px] w-[22px] rounded-[2px]`), not
/// the circular `round_swatch` - it is flat (no ring) to match the baseline.
pub(super) fn crop_fill_swatch_group() -> Stateful<Div> {
    div()
        .id("opt-crop-use-bg")
        .h(px(CTRL_H))
        .flex_shrink_0()
        .h_flex()
        .items_center()
        .gap(px(ITEM_GAP))
        .rounded(px(RADIUS_SM))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .px(px(FIELD_PX))
        .child(
            div()
                .h(px(18.0))
                .w(px(22.0))
                .flex_shrink_0()
                .rounded(px(RADIUS_SM))
                .border_1()
                .border_color(color::BORDER)
                .bg(color::WHITE),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(TEXT_XS))
                .text_color(ZINC)
                .font_weight(FontWeight(500.))
                .child("Use BG"), // i18n-exempt: not in catalog
        )
}

/// `t("common.apply", "Apply")` (`CropOptionBar.tsx:649-666`) — accent action:
/// `h-6 px-2.5 rounded-[4px] font-bold text-white shadow-xs`, with the fill at
/// 15%. Live via the Crop bar, so [`ACTION_PX`] is consumed (the dead-code
/// allow on it was dropped in mod.rs).
///
/// The fill: the resting `option-crop` baseline blends the button interior to
/// `#3a2b20` (58, 43, 32) over the `#1b1b1b` bar - [`color::ACCENT_15`], not
/// the solid `bg-editor-accent` the transform variant's class string spells
/// out. No baseline captures the transform bar's Apply, so the measured crop
/// value is what paints here.
pub(super) fn apply_button() -> Stateful<Div> {
    div()
        .id("opt-apply")
        .h(px(CTRL_H))
        .flex_shrink_0()
        .h_flex()
        .items_center()
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::ACCENT_15)
        .px(px(ACTION_PX))
        .text_size(px(TEXT_SM))
        .font_weight(FontWeight(700.))
        .text_color(color::WHITE)
        .shadow(vec![
            BoxShadow::new(px(0.), px(1.), SHADOW_XS).blur_radius(px(2.))
        ])
        .child(t("common.apply"))
}

/// `t("common.cancel", "Cancel")` (`CropOptionBar.tsx:642-657`) — secondary
/// action. The class string reads `bg-editor-field text-[#A1A1AA]`, but the
/// resting `option-crop` baseline paints the border-only ghost: the interior
/// stays the bar's `#1b1b1b` (27) where `bg-editor-field` would read 37. The
/// border and the label paint; the fill does not.
pub(super) fn cancel_button() -> Stateful<Div> {
    div()
        .id("opt-cancel")
        .h(px(CTRL_H))
        .flex_shrink_0()
        .h_flex()
        .items_center()
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .px(px(ACTION_PX))
        .text_size(px(TEXT_SM))
        .font_weight(FontWeight(600.))
        .text_color(ZINC)
        .child(t("common.cancel"))
}
