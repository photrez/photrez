use gpui_kit::base::h_flex;
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::{div, px, Div, FontWeight};

use crate::icons;
use crate::tokens::color;

use super::{
    caption, value_mono, value_sans, FIELD_H, FONT_UI, ICON_SM, INNER_GAP, LABEL_W, PAD_X, RADIUS,
    ROW_GAP, SELECT_PAD_X, SLOT_GAP, TEXT_SM,
};

/// The `NumField` / `EditableNumField` box: 24px tall, vertically centred, a
/// 4px inner gap, a 4px radius, a 1px field border over the field fill, and
/// 6px of horizontal padding.
///
/// The border is [`color::BORDER`] (`#333333`), not [`color::FIELD_BORDER`]
/// (`#383838`), for the reason `crate::tokens` documents at length: the field
/// border resolves to the darker grey, and only a background fill reaches the
/// lighter `#383838`.
///
/// The non-selectable-text affordance is dropped with the rest of the
/// pointer/keyboard states — see the module docs.
pub fn field_chrome(pad_x: f32) -> impl FnOnce(Div) -> Div {
    move |el| {
        el.h(px(FIELD_H))
            .items_center()
            .gap(px(INNER_GAP))
            .rounded(px(RADIUS))
            .border_1()
            .border_color(color::BORDER)
            .bg(color::FIELD)
            .px(px(pad_x))
            // Clip, not spill. An editable field clips its own text at the
            // padding edge; a plain box does not, so a field squeezed narrower
            // than "X 100 px" painted its glyphs straight through the border
            // and on top of the neighbouring field. `min_w(0)` is the other
            // half of the fix: flex items keep their content's intrinsic width
            // as a floor by default, so without it the box declines to narrow
            // at all and clipping alone would not help.
            .min_w(px(0.0))
            .overflow_hidden()
    }
}

// ── NumField ─────────────────────────────────────────────────────────────────

/// A read-only numeric field.
///
/// ```text
/// | label | 800 | px |
/// ```
///
/// Nothing is interactive: no hover style, no focus ring, no editing. It is
/// rendered as a plain flex row.
///
/// There are no per-call-site class or label overrides. Those would only let a
/// caller widen the box or recolour the caption, and callers that widen the
/// box pass their own width anyway — so `width` is a parameter and the caption
/// colour is not.
pub fn num_field(
    label: Option<&str>,
    width: Option<f32>,
    value: &str,
    suffix: Option<&str>,
) -> Div {
    // `None` means "fill the slot I was dropped into", not "width 0".
    //
    // The old code read `None` as `w(px(0.0))` and paired it with
    // `flex_shrink_0()` — a contradiction: `flex-basis: 0` with shrinking
    // switched off and no width to grow from. The box stayed zero-wide and its
    // children spilled out of it. That was the overlap in the Properties
    // panel: Position, Size, Rotation and the percentage rows pass `None`, so
    // all five fields painted their glyphs on top of one another.
    let row = match width {
        Some(w) => h_flex().w(px(w)).flex_none(),
        None => h_flex().flex_1().min_w(px(0.0)),
    };

    let mut inner = h_flex();
    inner = field_chrome(PAD_X)(inner);

    if let Some(l) = label {
        inner = inner.child(caption(l));
    }
    inner = inner.child(value_mono(value));
    if let Some(s) = suffix {
        inner = inner.child(caption(s));
    }

    row.child(inner)
}

// ── SelectField ──────────────────────────────────────────────────────────────

/// A value with a chevron, justified to both edges.
///
/// ```text
/// | Move                  v |
/// ```
///
/// The hover border (#4B515D) is a real difference from [`NumField`], which
/// has no hover at all, so it is recorded on [`color::HOVER_BORDER`] rather
/// than applied here: see the module docs on why hover colours are omitted
/// from a resting render.
///
/// The chevron is drawn at stroke width 1.75; [`icons::CHEVRON_DOWN`] already
/// carries 1.75 in its own SVG, because the icon's primary consumer pins that
/// stroke width at generation time. Nothing to override.
pub fn select_field(value: &str, width: Option<f32>) -> Div {
    // `None` fills the slot, for the same reason as `num_field`: a full-width
    // fill stretches, it does not collapse.
    let row = match width {
        Some(w) => h_flex().w(px(w)).flex_none(),
        None => h_flex().flex_1().min_w(px(0.0)),
    };

    let mut inner = h_flex().justify_between();
    inner = field_chrome(SELECT_PAD_X)(inner);
    inner = inner.child(value_sans(value)).child(
        Icon::default()
            .data(icons::CHEVRON_DOWN)
            .size(px(ICON_SM))
            .text_color(color::FIELD_CAPTION),
    );

    row.child(inner)
}

// ── PropRow ──────────────────────────────────────────────────────────────────

/// A 50px caption and a content slot beside it.
///
/// ```text
/// | Opacity | [====|======] |
/// ```
///
/// This is the only one of the five that does not itself draw a box: it is
/// the label that every row in a properties panel shares, so its width is
/// fixed and refuses to shrink — two properties that matter, because the slot
/// takes the remaining space and would otherwise squeeze the caption out of
/// shape.
///
/// Takes `impl IntoElement` rather than a concrete child type because the slot
/// is *any* element — whichever control the caller wants: a [`Slider`], a
/// [`num_field`], a row of checkboxes.
pub fn prop_row(label: &str, slot: impl IntoElement) -> Div {
    h_flex()
        .items_center()
        .gap(px(ROW_GAP))
        .child(
            div()
                .w(px(LABEL_W))
                .flex_none()
                // `flex_none()` holds the 50px; `overflow_hidden()` clips a
                // label that does not fit inside it. `flex_none()` alone gave
                // the right width and the wrong overflow behaviour: the box
                // stayed 50px while the glyphs kept going, so a label like
                // "Invert Selection" ran under the field beside it. Overflow is
                // visible by default, so the label column clips rather than
                // letting the text run on.
                .overflow_hidden()
                .text_size(px(TEXT_SM))
                .font_family(FONT_UI)
                .font_weight(FontWeight(500.))
                .text_color(color::FIELD_CAPTION)
                .child(label.to_string()),
        )
        .child(
            h_flex()
                .items_center()
                .gap(px(SLOT_GAP))
                .flex_1()
                // The slot takes the remaining space, so it inherits the same
                // intrinsic-min-width trap the fields had: without an explicit
                // floor of 0 it refuses to give back the space its children
                // need, and the row overflows the panel instead of squeezing.
                .min_w(px(0.0))
                .overflow_hidden()
                .child(slot),
        )
}

// ── EditableNumField ─────────────────────────────────────────────────────────

/// The editable form of [`NumField`].
///
/// ```text
/// | label | 800 | px |          <- at rest
/// ```
///
/// **At rest only.** A fully editable field needs focus, commit-on-blur,
/// Enter/Escape and arrow-key stepping; none of that can be expressed as a
/// function returning `impl IntoElement`, and none of it appears in any
/// baseline — a field that is not being edited renders exactly this row of
/// caption, mono value, caption.
///
/// So the interaction is the *whole* remaining difference, and it is deferred
/// rather than half-built: a text input that cannot take focus is worse than
/// an honest static row, because the first reader would assume the missing
/// behaviour was a bug rather than a decision.
///
/// `disabled` is also unrendered for the same reason — it only changes the
/// appearance *while an interaction is in progress* (a dimmed, inert state),
/// and the resting pixels of a disabled field and an enabled one are identical
/// until you reach for it.
pub fn editable_num_field(
    label: Option<&str>,
    value: &str,
    suffix: Option<&str>,
    width: Option<f32>,
) -> Div {
    // `None` means "fill the slot I was dropped into", not "width 0".
    //
    // The old code read `None` as `w(px(0.0))` and paired it with
    // `flex_shrink_0()` — a contradiction: `flex-basis: 0` with shrinking
    // switched off and no width to grow from. The box stayed zero-wide and its
    // children spilled out of it. That was the overlap in the Properties
    // panel: Position, Size, Rotation and the percentage rows pass `None`, so
    // all five fields painted their glyphs on top of one another.
    let row = match width {
        Some(w) => h_flex().w(px(w)).flex_none(),
        None => h_flex().flex_1().min_w(px(0.0)),
    };

    let mut inner = h_flex();
    inner = field_chrome(PAD_X)(inner);

    if let Some(l) = label {
        inner = inner.child(caption(l));
    }
    inner = inner.child(value_mono(value));
    if let Some(s) = suffix {
        inner = inner.child(caption(s));
    }

    row.child(inner)
}
