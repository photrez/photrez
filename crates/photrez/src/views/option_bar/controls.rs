use super::*;

// ── Shared parts ─�─────────────────────────────────────────────────────────────

/// `ToolPill` (`OptionBarShared.tsx:56`).
pub(super) fn tool_pill(icon: &'static [u8], label: String) -> impl IntoElement {
    h_flex()
        .h(px(CTRL_H))
        .flex_shrink_0()
        .items_center()
        .gap(px(GAP))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .px(px(PILL_PX))
        .text_size(px(TEXT_SM))
        .font_weight(FontWeight(600.))
        .text_color(ZINC)
        .child(
            Icon::default()
                .data(icon)
                .size(px(ICON_SM))
                .text_color(ZINC),
        )
        .child(div().text_color(color::WHITE).child(label))
}

/// `Divider` (`OptionBarShared.tsx:53`).
pub(super) fn divider() -> impl IntoElement {
    div()
        .h(px(DIV_H))
        .w(px(DIV_W))
        .flex_shrink_0()
        .bg(DIVIDER_LINE)
}

/// `OptionCheckbox` in the checked state — the Move bar's three are all checked.
pub(super) fn option_checkbox(id: &'static str, label: &str) -> impl IntoElement {
    option_checkbox_state(id, label, true)
}

/// `OptionCheckbox` (`OptionBarShared.tsx:27`) with an explicit `checked`.
pub(super) fn option_checkbox_state(
    id: &'static str,
    label: &str,
    checked: bool,
) -> impl IntoElement {
    div()
        .id(id)
        .h_flex()
        .h(px(CTRL_H))
        .items_center()
        .gap(px(GAP))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .px(px(FIELD_PX))
        .text_size(px(TEXT_SM))
        .font_weight(FontWeight(500.))
        .hover(|s| s.bg(FIELD_60))
        .child(if checked {
            checked_box()
        } else {
            unchecked_box()
        })
        .child(if checked {
            div()
                .font_weight(FontWeight(600.))
                .text_color(color::WHITE)
                .child(label.to_string())
        } else {
            div().text_color(ZINC).child(label.to_string())
        })
}

/// The checked square: accent fill, 1px shadow, tick at stroke 3.5.
pub(super) fn checked_box() -> Div {
    div()
        .h_flex()
        .size(px(ICON_MD))
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .rounded(px(RADIUS_SM))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::ACCENT)
        .shadow(vec![
            BoxShadow::new(px(0.), px(1.), SHADOW_XS).blur_radius(px(2.))
        ])
        .child(
            Icon::default()
                .data(icons::with_stroke(icons::CHECK, STROKE_CHECK))
                .size(px(ICON_SM))
                .text_color(color::WHITE),
        )
}

/// The unchecked square: field fill, no tick.
pub(super) fn unchecked_box() -> Div {
    div()
        .h_flex()
        .size(px(ICON_MD))
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .rounded(px(RADIUS_SM))
        .border_1()
        .border_color(color::BORDER)
        .bg(FIELD_80)
}

/// `EditableNumField` (`primitives.tsx:208`) — an editable field drawn at rest.
pub(super) fn editable_field(
    id: &'static str,
    label: &str,
    width: f32,
    value: &'static str,
    suffix: &'static str,
) -> impl IntoElement {
    div()
        .id(id)
        .h_flex()
        .h(px(CTRL_H))
        .w(px(width))
        .items_center()
        .gap(px(ITEM_GAP))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .px(px(FIELD_PX))
        .hover(|s| s.border_color(HOVER_BORDER_LOCAL))
        .when(FIELD_LABELS, |el| el.child(dim_span(label)))
        .child(div().flex_1().min_w(px(0.)).child(value_text(value)))
        .child(suffix_span(suffix))
}

/// `NumField` (`primitives.tsx:6`) — read-only W/H, value + suffix packed left.
pub(super) fn num_field(
    label: &str,
    width: f32,
    value: &'static str,
    suffix: &'static str,
) -> impl IntoElement {
    h_flex()
        .h(px(CTRL_H))
        .w(px(width))
        .items_center()
        .gap(px(ITEM_GAP))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .px(px(FIELD_PX))
        .when(FIELD_LABELS, |el| el.child(dim_span(label)))
        .child(value_text(value))
        .child(suffix_span(suffix))
}

/// Value run: `font-mono text-[11px] font-semibold text-white`.
pub(super) fn value_text(value: &'static str) -> impl IntoElement {
    div()
        .font_family(ui::FONT_MONO)
        .text_size(px(TEXT_SM))
        .font_weight(FontWeight(600.))
        .text_color(color::WHITE)
        .child(value)
}

/// Caption run: `text-[10px] font-medium text-[#A1A1AA]`.
pub(super) fn dim_span(text: &str) -> impl IntoElement {
    div()
        .text_size(px(TEXT_XS))
        .font_weight(FontWeight(500.))
        .text_color(ZINC)
        .child(text.to_string())
}

pub(super) fn suffix_span(s: &'static str) -> impl IntoElement {
    dim_span(s)
}

/// The `@min-[880px]:flex` group: divider, align, divider, flip, divider,
/// reset (`MoveOptionBar.tsx:312-377`).
pub(super) fn main_controls() -> impl IntoElement {
    h_flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(GAP))
        .child(divider())
        .child(align_group())
        .child(divider())
        .child(flip_group())
        .child(divider())
        .child(reset_button())
}

pub(super) fn align_group() -> impl IntoElement {
    h_flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(ICON_GAP))
        .when(FIELD_LABELS, |el| {
            el.child(group_label(&t("properties.align")))
        })
        .child(square_icon_button("Align left", icons::ALIGN_LEFT))
        .child(square_icon_button(
            "Align center horizontal",
            icons::ALIGN_H,
        ))
        .child(square_icon_button("Align right", icons::ALIGN_RIGHT))
        .child(square_icon_button("Align top", icons::ALIGN_TOP))
        .child(square_icon_button("Align center vertical", icons::ALIGN_V))
        .child(square_icon_button("Align bottom", icons::ALIGN_BOTTOM))
}

pub(super) fn flip_group() -> impl IntoElement {
    h_flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(ICON_GAP))
        .when(FIELD_LABELS, |el| {
            el.child(group_label(&t("properties.flip")))
        })
        .child(square_icon_button("Flip horizontal", icons::FLIP_H))
        .child(square_icon_button("Flip vertical", icons::FLIP_V))
}

pub(super) fn square_icon_button(id: &'static str, icon: &'static [u8]) -> impl IntoElement {
    div()
        .id(id)
        .h_flex()
        .size(px(BTN))
        .items_center()
        .justify_center()
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(FIELD_40)
        .hover(|s| s.bg(color::FIELD).text_color(color::WHITE))
        .child(
            Icon::default()
                .data(icon)
                .size(px(ICON_MD))
                .text_color(ZINC),
        )
}

pub(super) fn group_label(text: &str) -> Div {
    div().mr(px(MR_LABEL)).child(dim_span(text))
}

pub(super) fn reset_button() -> impl IntoElement {
    div()
        .id("opt-reset")
        .h_flex()
        .h(px(CTRL_H))
        .flex_shrink_0()
        .items_center()
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(FIELD_40)
        .px(px(PILL_PX))
        .text_size(px(TEXT_SM))
        .font_weight(FontWeight(600.))
        .hover(|s| s.bg(color::FIELD).text_color(color::WHITE))
        .text_color(ZINC)
        .child(t("common.reset"))
}

/// The overflow button (`MoreDropdown`) — `size-[24px] rounded-[3px]`.
pub(super) fn more_dropdown_button() -> impl IntoElement {
    div()
        .id("opt-more")
        .h_flex()
        .size(px(BTN))
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .rounded(px(RADIUS_SM))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .hover(|s| s.border_color(HOVER_BORDER_LOCAL).text_color(color::WHITE))
        .text_color(ZINC)
        .child(
            Icon::default()
                .data(icons::MORE)
                .size(px(ICON_LG))
                .text_color(ZINC),
        )
}

/// `SelectDropdown` (`OptionBarShared.tsx:104-146`) — closed state.
pub(super) fn select_dropdown(
    icon: Option<&'static [u8]>,
    prefix: Option<&str>,
    value: String,
) -> Div {
    let mut btn = div()
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
        .font_weight(FontWeight(500.));

    if let Some(data) = icon {
        btn = btn.child(
            Icon::default()
                .data(icons::with_stroke(data, STROKE_DROPDOWN))
                .size(px(ICON_MD))
                .text_color(color::ACCENT),
        );
    }

    let label = match prefix {
        Some(p) => h_flex()
            .child(div().text_color(ZINC).child(p.to_string()))
            .child(
                div()
                    .font_weight(FontWeight(600.))
                    .text_color(color::WHITE)
                    .child(value),
            ),
        None => div()
            .font_weight(FontWeight(600.))
            .text_color(color::WHITE)
            .child(value),
    };

    btn.child(label).child(
        Icon::default()
            .data(icons::with_stroke(icons::CHEVRON_DOWN, STROKE_DROPDOWN))
            .size(px(ICON_SM))
            .text_color(ZINC),
    )
}

/// A 1.75-stroke chevron used by the text bar's custom buttons.
pub(super) fn chevron_175() -> impl IntoElement {
    Icon::default()
        .data(icons::with_stroke(icons::CHEVRON_DOWN, STROKE_PILL))
        .size(px(ICON_SM))
        .text_color(ZINC)
}

/// `ml-1` spacing for the text bar's chevrons.
pub(super) fn ml_label(el: impl IntoElement) -> Div {
    div().ml(px(MR_LABEL)).child(el)
}

/// `shadow-sm` as two layered shadows.
pub(super) fn shadow_sm_layers() -> Vec<BoxShadow> {
    vec![
        BoxShadow::new(px(0.), px(1.), hsla(0.0, 0.0, 0.0, 0.10)).blur_radius(px(3.)),
        BoxShadow::new(px(0.), px(1.), hsla(0.0, 0.0, 0.0, 0.10))
            .blur_radius(px(2.))
            .spread_radius(px(-1.)),
    ]
}

/// `ring-1 ring-white/20` as a 1px spread shadow.
pub(super) fn ring_white_20() -> BoxShadow {
    BoxShadow::new(px(0.), px(0.), color::WHITE_20).spread_radius(px(1.))
}

// ── Brush bar ────────────────────────────────────────────────────────────────

/// The brush bar's Size field (`BrushOptionBar.tsx:143-156`).
pub(super) fn paint_field(
    id: &'static str,
    label: &str,
    input_w: f32,
    value: &'static str,
    suffix: &'static str,
) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(CTRL_H))
        .flex_shrink_0()
        .h_flex()
        .items_center()
        .gap(px(ITEM_GAP))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .px(px(FIELD_PX))
        .hover(|s| s.border_color(HOVER_BORDER_LOCAL))
        .child(dim_span(label))
        .child(div().w(px(input_w)).flex_none().child(value_text(value)))
        .child(dim_span(suffix))
}

/// `opacity-40` wrapper for the selection bar's disabled fields.
pub(super) fn dimmed(el: impl IntoElement) -> Div {
    div().opacity(0.4).child(el)
}

/// `<div class="flex shrink-0 items-center gap-1">` around paired fields.
pub(super) fn field_pair(first: impl IntoElement, second: impl IntoElement) -> Div {
    h_flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(ITEM_GAP))
        .child(first)
        .child(second)
}

// ── Paint Bucket ───────────────────────────────────────────────────────────────

/// The tolerance `<input type="number">` (`PaintBucketOptionBar.tsx`).
pub(super) fn right_num_field(id: &'static str, value: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(CTRL_H))
        .w(px(TOLERANCE_FIELD_W))
        .flex_shrink_0()
        .h_flex()
        .items_center()
        .justify_end()
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .px(px(FIELD_PX))
        .hover(|s| s.border_color(HOVER_BORDER_LOCAL))
        .child(value_text(value))
}
