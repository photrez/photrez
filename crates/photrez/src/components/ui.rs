#![allow(dead_code)]
// shared control vocabulary for panel ports that land after the shell; nothing imports this module yet

//! The five shared controls of the web build
//! (`apps/desktop/src/components/editor/primitives.tsx`).
//!
//! # Why these are a module
//!
//! `primitives.tsx` is the one place in the web build where a control is *defined*
//! rather than assembled: `NumField`, `SelectField`, `PropRow`, `Slider` and
//! `EditableNumField` are each a single function that takes props and returns
//! markup, and every panel imports them instead of writing its own. The option
//! bar and all four inspector panels are built on them, so porting them once
//! here is what keeps those five ports from diverging — the same twenty-four
//! pixel box, the same caption grey, the same Consolas value, in each.
//!
//! The port is **visual only**, matching the project's first pass: everything
//! that a resting screenshot can see is reproduced, and everything that needs
//! an event loop is not yet. Concretely, what is here:
//!
//! * the box geometry, borders, fills and radii;
//! * every colour, resolved through [`crate::tokens`] rather than restated;
//! * the value/caption typography, including the `font-mono` face;
//! * [`Slider`]'s per-kind track — the gradients, the checkerboard, the
//!   centre-origin fills, the thumb.
//!
//! And what is not, with the reason at each site:
//!
//! * **interaction** — `EditableNumField` cannot be edited, `SelectField`
//!   cannot open, `Slider` cannot be dragged. These need focus, a keymap and a
//!   mutable value, which is `Render`/`Entity` territory rather than a
//!   function returning `impl IntoElement`, and the captured baseline shows
//!   none of it.
//! * **`focus-within:ring`** — same problem: there is no focus state to ring.
//!   A 1px-spread `BoxShadow` would stand in for the ring if a focused variant
//!   is ever needed.
//! * **`transition-colors` / `hover:scale-110`** — GPUI has no declarative
//!   per-element transition, so the *hover* colours are also absent rather
//!   than painted permanently. Painting them would be worse than omitting
//!   them: a hover state frozen into the resting screenshot is a pixel the
//!   diff would then have to explain away.
//!
//! # Two conventions inherited from the option bar
//!
//! `font-mono` is rendered as **Consolas**, and `#A1A1AA` as
//! [`color::FIELD_CAPTION`]. Both were measured rather than assumed — the
//! mono face from its advance width against the baseline (18px for `"800"` at
//! 11px, where Inter would give 21px), the grey from being neither
//! [`color::TEXT_DIM`] nor [`color::ICON`]. The option bar established both
//! first; this module agrees with it rather than rediscovering them.

use gpui_kit::base::h_flex;
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::{
    checkerboard, div, hsla, linear_color_stop, linear_gradient, px, relative, Background,
    BoxShadow, Div, FontWeight, Hsla,
};

use crate::assets::icons;
use crate::tokens::color;

// ── Geometry ─────────────────────────────────────────────────────────────────

/// `h-[24px]` — every field control, without exception.
const FIELD_H: f32 = 24.0;
/// `rounded-[4px]` on the field box.
const RADIUS: f32 = 4.0;
/// `gap-1` between a field's label, value and suffix.
const INNER_GAP: f32 = 4.0;
/// `px-1.5` on [`NumField`] and [`EditableNumField`].
const PAD_X: f32 = 6.0;
/// `px-2` on [`SelectField`] — wider, because it justifies content to both
/// edges instead of packing it.
const SELECT_PAD_X: f32 = 8.0;
/// `w-[50px]` on a [`PropRow`] label, `shrink-0`.
const LABEL_W: f32 = 50.0;
/// `gap-2` between a [`PropRow`]'s label and its slot.
const ROW_GAP: f32 = 8.0;
/// `gap-1.5` inside a [`PropRow`]'s content slot.
const SLOT_GAP: f32 = 6.0;
/// `size-3` — the [`SelectField`] chevron.
const ICON_SM: f32 = 12.0;
/// `text-[10px]` — captions.
const TEXT_XS: f32 = 10.0;
/// `text-[11px]` — values and row labels.
const TEXT_SM: f32 = 11.0;

// ── Faces ────────────────────────────────────────────────────────────────────

/// `font-mono`, i.e. Consolas — values in [`NumField`] and
/// [`EditableNumField`].
///
/// `primitives.tsx:30,318` ask for Tailwind's `font-mono`, whose stack is
/// `ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, ...`; on this
/// Windows capture the first name that resolves is Consolas. It is a system
/// font, not the stack literally, which is recorded as a gap in the option
/// bar's font notes and applies unchanged here.
///
/// Public for the same reason as [`FONT_UI`]: view modules that render mono
/// values read it directly instead of spelling "Consolas" per module.
pub const FONT_MONO: &str = "Consolas"; // i18n-exempt: technical token

/// `--font-sans` — Inter, on everything that is *not* `font-mono`.
/// Inter, the font every label in the port uses.
///
/// Public because thirteen modules each declared their own copy before this
/// existed, and a font that is spelled out per-module is a font that can be
/// changed in one place and missed in twelve. The default `.SystemUIFont`
/// (Segoe UI) cannot stand in: every measurement in the web build was taken
/// against Inter.
pub const FONT_UI: &str = "Inter"; // i18n-exempt: technical token

/// A menu row's measured height, and its insets — the same figures both menu
/// modules measured independently.
const MENU_ROW_H: f32 = 28.0;
const MENU_ROW_PX: f32 = 12.0;
const MENU_ROW_GAP: f32 = 24.0;
const MENU_ROW_TEXT: f32 = 12.0;
const MENU_SHORTCUT_TEXT: f32 = 10.0;

/// `my-1` around a separator, plus the line itself.
const MENU_SEP_MY: f32 = 4.0;

// ── Shared runs ──────────────────────────────────────────────────────────────

/// `<span class="text-[10px] font-medium text-[#A1A1AA] select-none">`.
///
/// One helper because the source has one string: a field's `label`
/// (`primitives.tsx:23`), its `suffix` (`:32`), `PropRow`'s label (`:59`) and
/// `EditableNumField`'s suffix (`:321`) are byte-identical class lists, and
/// they read the same on screen too.
/// A run of text with its size, weight and colour — the shape that was
/// hand-rolled ten times across the port as
/// `div().text_size(..).font_family(FONT_UI).font_weight(..).text_color(..)`.
///
/// Four setters in a fixed order is a lot of places to get the order wrong in,
/// and the order is load-bearing: `text_color` after `bg` tints the glyphs,
/// while the same pair before a parent's colour can be inherited differently.
/// One helper means one place to reason about it.
///
/// `weight` takes a raw `f32` rather than `FontWeight` so a call site can pass
/// a literal `500.` without importing the wrapper type.
/// One row of a popup menu: label left, shortcut right, 28px tall.
///
/// Shared by the web build's pointer-anchored context menus and its title-bar
/// popups because the row chrome is identical — only the *shell* differs, which
/// is why the two shells are not merged but this is. Both measured ROW_H=28,
/// ROW_GAP=24, ROW_PX=12 independently and arrived at the same numbers.
///
/// `focus_bg` is `Option` rather than a plain colour because the two callers
/// genuinely differ here: the pointer-anchored context menus capture Chromium's
/// `focus-visible:bg-editor-accent/15` on the first enabled row (a state the
/// keyboard produces, not the mouse), while the title-bar popups have no
/// captured focus tint at all. `None` paints no background, which is what a
/// static render of those should do.
/// Make a box yield instead of overflowing: the flex min-width:auto floor drops
/// to 0 and the painted content is clipped at the edge.
///
/// This is the fix for one bug that showed up in nine places. A flex child's
/// default min-width is its content's intrinsic width, so a box holding text
/// will not narrow below that text no matter how little room the row has left.
/// The box keeps its size and the glyphs keep going, painting over whatever
/// sits next to them. An overflow_hidden() alone does not help, because the box
/// never narrowed in the first place; the min_w(0) is what lets it narrow, and
/// the clip is what makes the narrowing look deliberate.
///
/// On the web the source gets this for free -- a div clips nothing, but text
/// inside an input or a truncated cell does -- so every ported row has to say
/// it out loud.
pub fn clip<T: gpui_kit::Styled>(el: T) -> T {
    el.min_w(px(0.0)).overflow_hidden()
}

/// The opposite of [clip]: hold your intrinsic width and refuse to give it up.
/// Right for a chevron, a shortcut, a counter -- anything that must stay whole
/// while the text beside it is the part that should truncate.
pub fn pin<T: gpui_kit::Styled>(el: T) -> T {
    el.flex_none()
}

pub fn menu_row(
    label: &str,
    shortcut: Option<&str>,
    disabled: bool,
    focus_bg: Option<gpui_kit::Hsla>,
) -> Div {
    let mut row = h_flex()
        .w_full()
        .h(px(MENU_ROW_H))
        .items_center()
        .justify_between()
        .gap(px(MENU_ROW_GAP))
        .px(px(MENU_ROW_PX));
    if let Some(bg) = focus_bg {
        row = row.bg(bg);
    }
    // Whole-row, not label-only: the source's `disabled:opacity-35` sits on the
    // row, so the shortcut dims with the label. Getting this wrong shows up as
    // a shortcut that reads brighter than the item it belongs to.
    if disabled {
        row = row.opacity(0.35);
    }
    // The label yields and the shortcut does not. With `justify_between` and
    // nothing shrinking, a long label ("Zoom to Selection") pushed its
    // shortcut off the right edge instead of wrapping or clipping — the same
    // class of bug as the zero-width fields, one layer up.
    row = row.child(
        text_run(label, MENU_ROW_TEXT, 400., color::TEXT)
            .flex_1()
            .min_w(px(0.0))
            .overflow_hidden(),
    );
    if let Some(keys) = shortcut {
        row = row.child(
            div()
                .flex_none()
                .text_size(px(MENU_SHORTCUT_TEXT))
                .font_family(FONT_UI)
                .text_color(color::TEXT_DIM)
                .child(keys.to_string()),
        );
    }
    row
}

/// `my-1 border-t border-editor-divider`, full-bleed — a probe reads separator
/// rows edge to edge, so there is no horizontal inset.
///
/// Shared for the same reason [`menu_row`] is. Note the colour is `DIVIDER`
/// (#2e2e2e), *not* `BORDER` (#333333): both are "a border" in the source, but
/// `border-editor-divider` is a real class that resolves, and a scan looking
/// only for #333333 reports the separators as missing.
pub fn menu_separator() -> Div {
    div()
        .w_full()
        .my(px(MENU_SEP_MY))
        .border_t(px(1.0))
        .border_color(color::DIVIDER)
}
pub fn text_run(text: &str, size: f32, weight: f32, colour: gpui_kit::Hsla) -> Div {
    div()
        .text_size(px(size))
        .font_family(FONT_UI)
        .font_weight(FontWeight(weight))
        .text_color(colour)
        .child(text.to_string())
}
pub fn caption(text: &str) -> Div {
    div()
        // A field caption is a fixed prefix or suffix ("X", "px", "%"): it must
        // not be the thing that gives way when the field narrows. Without
        // this, flex shrink is proportional to basis and a squeezed field
        // chops all three pieces at once — "X 10", "0 p", "x" — which reads on
        // screen as the same garbled overlap, one step removed. Pinning the
        // captions hands the whole squeeze to the value, so a narrow field
        // clips "100" down to "10" the way an `<input>` would.
        .flex_none()
        .text_size(px(TEXT_XS))
        .font_weight(FontWeight(500.))
        .text_color(color::FIELD_CAPTION)
        // `.child()` builds a tree that outlives this call, so the borrowed
        // `&str` is copied: `String` and `SharedString` both implement
        // `IntoElement`, `&str` does not unless it is `'static`.
        .child(text.to_string())
}

/// `<span class="text-[11px] font-semibold font-mono text-white">` — a field's
/// value (`primitives.tsx:30`).
pub fn value_mono(text: &str) -> Div {
    div()
        .font_family(FONT_MONO)
        .text_size(px(TEXT_SM))
        .font_weight(FontWeight(600.))
        .text_color(color::WHITE)
        .child(text.to_string())
}

/// `<span class="text-[11px] font-semibold text-white">` — [`SelectField`]'s
/// value (`primitives.tsx:46`).
///
/// Deliberately *not* [`value_mono`]: this one carries no `font-mono`, so it
/// is set in Inter even though the two look almost alike at 11px.
pub fn value_sans(text: &str) -> Div {
    div()
        .font_family(FONT_UI)
        .text_size(px(TEXT_SM))
        .font_weight(FontWeight(600.))
        .text_color(color::WHITE)
        .child(text.to_string())
}

/// The `NumField` / `EditableNumField` box: `h-[24px] items-center gap-1
/// rounded-[4px] border border-editor-field-border bg-editor-field px-1.5`.
///
/// The border is [`color::BORDER`], not [`color::FIELD_BORDER`], for the
/// reason `crate::tokens` documents at length: `border-editor-field-border`
/// loses to the unlayered `* { border-color }` rule, so every `border-*` in the
/// web build paints `#333333`. Only a `background-*` would reach `#383838`.
///
/// `select-none` is dropped with the rest of the pointer/keyboard affordances
/// — see the module docs.
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
            // Clip, not spill. The source's `<input>` clips its own text at
            // the padding edge; a plain `div` does not, so a field squeezed
            // narrower than "X 100 px" painted its glyphs straight through the
            // border and on top of the neighbouring field. `min_w(0)` is the
            // other half of the fix: flex items default to `min-width: auto`,
            // which refuses to shrink below the content's intrinsic width, so
            // without it the box declines to narrow at all and clipping alone
            // would not help.
            .min_w(px(0.0))
            .overflow_hidden()
    }
}

// ── NumField ─────────────────────────────────────────────────────────────────

/// `primitives.tsx:6-36` — a read-only numeric field.
///
/// ```text
/// | label | 800 | px |
/// ```
///
/// Nothing is interactive: no hover style, no focus ring, no editing. It is
/// rendered as a plain flex row exactly as the source lays it out.
///
/// `class` and `labelClass` props are not carried over. They only exist in
/// the source to let a call site widen the box or recolour the caption, and
/// the call sites that do either pass their own width anyway — so `width` is
/// a parameter and the caption colour is not.
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

/// `primitives.tsx:38-54` — a value with a chevron, `justify-between`.
///
/// ```text
/// | Move                  v |
/// ```
///
/// The hover border (`hover:border-[#4B515D]`) is a real difference from
/// [`NumField`], which has no hover at all, so it is recorded on
/// [`color::HOVER_BORDER`] rather than applied here: see the module docs on
/// why hover colours are omitted from a resting render.
///
/// The source asks for `strokeWidth={1.75}` at the call site; [`icons::CHEVRON_DOWN`]
/// already carries 1.75 in its own SVG, because the icon's primary consumer
/// pins that stroke width at generation time. Nothing to override.
pub fn select_field(value: &str, width: Option<f32>) -> Div {
    // `None` fills the slot, for the same reason as `num_field`: the source's
    // `w-full` stretches, it does not collapse.
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

/// `primitives.tsx:56-65` — a 50px caption and a content slot beside it.
///
/// ```text
/// | Opacity | [====|======] |
/// ```
///
/// This is the only one of the five that does not itself draw a box: it is
/// the label that every row in a properties panel shares, so its width is
/// fixed and `shrink-0` — two properties that matter, because the slot is
/// `flex-1` and would otherwise squeeze the caption out of shape.
///
/// Takes `impl IntoElement` rather than a `JSX.Element` because in the source
/// the slot is *any* child, and here that is whichever control the caller
/// wants: a [`Slider`], a [`num_field`], a row of checkboxes.
pub fn prop_row(label: &str, slot: impl IntoElement) -> Div {
    h_flex()
        .items_center()
        .gap(px(ROW_GAP))
        .child(
            div()
                .w(px(LABEL_W))
                .flex_none()
                // `flex_none()` holds the 50px; `overflow_hidden()` clips a
                // label that does not fit inside it. `shrink_0` alone gave the
                // right width and the wrong overflow behaviour: the box stayed
                // 50px while the glyphs kept going, so a label like
                // "Invert Selection" ran under the field beside it. The source
                // clips because the label is a `<span>` in a `w-[50px]`
                // column, and overflow on a block box is visible by default —
                // `truncate` there, plain clip here.
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
                // The slot is `flex-1`, so it inherits the same
                // `min-width: auto` trap the fields had: without an explicit
                // floor of 0 it refuses to give back the space its children
                // need, and the row overflows the panel instead of squeezing.
                .min_w(px(0.0))
                .overflow_hidden()
                .child(slot),
        )
}

// ── EditableNumField ─────────────────────────────────────────────────────────

/// `primitives.tsx:208-325` — the editable form of [`NumField`].
///
/// ```text
/// | label | 800 | px |          <- at rest
/// ```
///
/// **At rest only.** The source is built around an `<input>` with focus,
/// commit-on-blur, Enter/Escape and arrow-key stepping; none of that can be
/// expressed as a function returning `impl IntoElement`, and none of it
/// appears in any baseline — a field that is not being edited renders exactly
/// this row of caption, mono value, caption.
///
/// So the interaction is the *whole* remaining difference, and it is deferred
/// rather than half-built: a text input that cannot take focus is worse than
/// an honest static row, because the first reader would assume the missing
/// behaviour was a bug rather than a decision.
///
/// `disabled` is also unrendered for the same reason — it only changes the
/// appearance *while an interaction is in progress* (`opacity-40
/// pointer-events-none`), and the resting pixels of a disabled field and an
/// enabled one are identical until you reach for it.
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

// ── Slider ───────────────────────────────────────────────────────────────────

/// `SliderType` (`primitives.tsx:67-77`) — which track treatment to paint.
///
/// The variant is not decoration: it selects the background, whether the fill
/// grows from the centre or the left, and how tall the whole control is. The
/// source drives all three off `type()` inside one component, which is why
/// they are one enum here rather than five functions.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SliderKind {
    /// `default` — a plain trough and an accent fill from the left.
    Default,
    /// `opacity` / `brush-strength` — a checkerboard under a fade to accent,
    /// because the slider fades to `transparent` and needs something to show
    /// through it.
    Alpha,
    /// `brightness` — black → zinc-600 → white.
    Brightness,
    /// `contrast` — black → zinc-500 → white.
    Contrast,
    /// `saturation` — grey → red → yellow → blue.
    Saturation,
    /// `straighten` — centre-origin in accent, like the three above but
    /// without a gradient track.
    Straighten,
    /// `brush-size` — the one slider that is 8px tall, unrounded, and clipped
    /// into a wedge rather than a pill.
    BrushSize,
    /// `brush-hardness` — a fade from accent-at-15% to full accent.
    BrushHardness,
}

/// `primitives.tsx:79-206`.
///
/// `percent` is the 0..100 fill; `value` is the raw value used by the
/// centre-origin kinds, which sit at 50% when it is 0 and reach the ends only
/// at ±100. Both are passed because the source does the same — `percent`
/// positions the thumb for every kind, while `value` decides *where* the fill
/// starts for the four centre-origin ones.
///
/// `accent: false` swaps the left-aligned fill from the accent to
/// [`color::TEXT_55`], the `bg-editor-text/55` branch (`primitives.tsx:189`).
pub fn slider(percent: f32, value: Option<f32>, kind: SliderKind, accent: bool) -> Div {
    let is_wedge = kind == SliderKind::BrushSize;
    let track_h = if is_wedge { 8.0 } else { 4.0 };

    let mut track = div().absolute().inset(px(0.)).overflow_hidden();

    if !is_wedge {
        track = track.rounded_full();
    }

    // Track background: a gradient/checkerboard for the kinds that specify
    // one, and `bg-editor-field-border` otherwise (`primitives.tsx:158`).
    match kind {
        SliderKind::Alpha => {
            // `linear-gradient(to right, transparent, accent)` over a
            // conic checkerboard, `background-size: auto, 8px 8px`. GPUI has
            // no compositing of two fills on one element, so the pattern goes
            // on a child underneath and the accent fade on another above it —
            // same two layers, same order.
            track = track
                .bg(color::FIELD_BORDER)
                .child(
                    div()
                        .absolute()
                        .inset(px(0.))
                        .bg(checkerboard(color::CHECK_DARK, 8.0)),
                )
                .child(div().absolute().inset(px(0.)).bg(linear_gradient(
                    90.0,
                    linear_color_stop(color::ACCENT.opacity(0.0), 0.0),
                    linear_color_stop(color::ACCENT, 1.0),
                )));
        }
        SliderKind::Brightness | SliderKind::Contrast | SliderKind::Saturation => {
            track = track.children(gradient_segments(&kind.track_stops()));
        }
        SliderKind::BrushHardness => {
            track = track.bg(linear_gradient(
                90.0,
                linear_color_stop(color::ACCENT.opacity(0.15), 0.0),
                linear_color_stop(color::ACCENT, 1.0),
            ));
        }
        SliderKind::BrushSize => {
            // `clip-path: polygon(0% 65%, 100% 15%, 100% 85%, 0% 65%)` over
            // `var(--color-editor-field-border)`, and `rounded` is absent so
            // the track is a sharp-ended bar rather than a pill.
            //
            // GPUI has no `clip-path`, and this is the one gap in the Slider
            // that is *visible* at rest rather than only under interaction: a
            // `brush-size` slider in the original is a wedge that grows to
            // the right, and here it is a rectangle. Worth knowing before the
            // brush panel is ported — the shape would have to be a filled
            // path, not a clipped box, and `gpui_kit` exposes no such helper
            // today.
            track = track.bg(color::FIELD_BORDER);
        }
        _ => {
            track = track.bg(color::FIELD_BORDER);
        }
    }

    // The fill, centred or left-aligned.
    if kind.is_centre_origin() {
        if let Some(raw) = value {
            let (left, width) = centre_fill(raw);
            let fill: Background = match kind {
                SliderKind::Brightness => color::ZINC_100.into(),
                SliderKind::Contrast => color::ZINC_300.into(),
                SliderKind::Straighten => color::ACCENT.into(),
                // `bg-gradient-to-r from-pink-500 to-rose-500` — a two-stop
                // gradient, which `linear_gradient` happens to support
                // natively, so it is not split like the wider track fills are.
                _ => linear_gradient(
                    90.0,
                    linear_color_stop(color::PINK_500, 0.0),
                    linear_color_stop(color::ROSE_500, 1.0),
                ),
            };
            let glow = match kind {
                SliderKind::Brightness => hsla(0.0, 0.0, 1.0, 0.45),
                SliderKind::Contrast => hsla(0.666667, 0.048780, 0.839216, 0.35),
                SliderKind::Straighten => hsla(0.0553, 0.8145, 0.4863, 0.45),
                _ => hsla(0.971455, 0.891626, 0.601961, 0.45),
            };

            track = track.child(
                div()
                    .absolute()
                    .left(relative(left / 100.0))
                    .w(relative(width / 100.0))
                    .top(px(0.))
                    .bottom(px(0.))
                    .rounded_full()
                    .bg(fill)
                    .shadow(vec![
                        BoxShadow::new(px(0.), px(0.), glow).blur_radius(px(8.))
                    ]),
            );
            // The neutral centre tick — `w-px bg-white/45`, centred. The
            // `_PCT` matters: `WHITE_45` is 4.5% and this is 45%.
            track = track.child(
                div()
                    .absolute()
                    .left(relative(0.5))
                    .w(px(1.0))
                    .top(px(0.))
                    .bottom(px(0.0))
                    .bg(color::WHITE_45_PCT),
            );
        }
    } else if kind != SliderKind::Alpha
        && kind != SliderKind::BrushHardness
        && kind != SliderKind::BrushSize
    {
        track = track.child(
            div()
                .absolute()
                .left(px(0.))
                .top(px(0.))
                .bottom(px(0.))
                .w(relative(percent / 100.0))
                .rounded_full()
                .bg(if accent {
                    color::ACCENT
                } else {
                    color::TEXT_55
                }),
        );
    }

    // `top-1/2` + `translate(-50%, -50%)` on a 12px dot: GPUI has no
    // transform, so the half-box is subtracted as a negative margin instead,
    // which lands the same pixels — `left` positions the margin edge for an
    // absolutely positioned child, so a `-6px` margin shifts it back half its
    // own width.
    let thumb = div()
        .absolute()
        .left(relative(percent / 100.0))
        .top(relative(0.5))
        .ml(px(-(ICON_SM / 2.0)))
        .mt(px(-(ICON_SM / 2.0)))
        .size(px(ICON_SM))
        .rounded_full()
        .border_1()
        .border_color(color::BLACK_55)
        .bg(color::SLIDER_THUMB)
        .shadow(vec![
            BoxShadow::new(px(0.0), px(1.0), hsla(0.0, 0.0, 0.0, 0.65)).blur_radius(px(2.0)),
            BoxShadow::new(px(0.0), px(0.0), hsla(0.0, 0.0, 1.0, 0.18)).spread_radius(px(1.0)),
        ]);

    div()
        .relative()
        .flex_1()
        .h(px(track_h))
        .child(track)
        .child(thumb)
}

impl SliderKind {
    /// `brightness`, `contrast`, `saturation` and `straighten` grow from the
    /// centre rather than the left (`primitives.tsx:142-144`).
    ///
    /// For those, `left: 50%` at rest and `width: |value| / 2 %` — so ±100
    /// reaches an end and 0 sits at the tick.
    fn is_centre_origin(self) -> bool {
        matches!(
            self,
            SliderKind::Brightness
                | SliderKind::Contrast
                | SliderKind::Saturation
                | SliderKind::Straighten
        )
    }

    /// The gradient stops of the track, as `(colour, 0..1 position)` pairs.
    ///
    /// Read straight off the `background-image` the source returns for this
    /// kind (`primitives.tsx:104-140`), in order.
    fn track_stops(self) -> Vec<(Hsla, f32)> {
        match self {
            SliderKind::Brightness => vec![
                (color::SLIDER_DARK, 0.0),
                (color::SLIDER_MID_BRIGHT, 0.5),
                (color::WHITE, 1.0),
            ],
            SliderKind::Contrast => vec![
                (color::SLIDER_DARK, 0.0),
                (color::SLIDER_MID_CONTRAST, 0.5),
                (color::WHITE, 1.0),
            ],
            SliderKind::Saturation => vec![
                (color::SLIDER_MID_BRIGHT, 0.0),
                (color::SLIDER_MID_CONTRAST, 0.35),
                (color::RED_500, 0.6),
                (color::YELLOW_500, 0.8),
                (color::BLUE_500, 1.0),
            ],
            _ => vec![],
        }
    }
}

/// `left` / `width` of the centre-origin fill, in percent, for a raw value.
///
/// Lifted verbatim from `getCenterFillStyle` (`primitives.tsx:88-102`), which
/// is piecewise: at or above zero the fill runs from the centre to the right
/// by `value / 2`; below zero it starts at `(value + 100) / 2` and stops at
/// the centre, so the bar is always `|value| / 2` wide and its free end moves
/// with the sign.
pub fn centre_fill(raw: f32) -> (f32, f32) {
    if raw >= 0.0 {
        (50.0, raw / 2.0)
    } else {
        let start = (raw + 100.0) / 2.0;
        (start, 50.0 - start)
    }
}

/// Lay a multi-stop gradient out as one `linear_gradient` per gap between
/// stops.
///
/// GPUI's `linear_gradient` takes exactly **two** stops
/// (`gpui::linear_gradient(angle, from, to)`), where CSS's takes a list. The
/// straightforward fix is one element per segment rather than a cleverer
/// encoding: a segment spanning `p0..p1` is an absolutely positioned box at
/// `left: p0%`, `width: (p1 - p0)%` carrying its own two-stop gradient across
/// its own box, which is the same interpolation CSS would have performed.
///
/// Splitting also sidesteps whether a two-stop gradient clamps outside its
/// stops — each segment's stops sit at its own 0 and 1, so there is nothing
/// to clamp.
pub fn gradient_segments(stops: &[(Hsla, f32)]) -> Vec<Div> {
    stops
        .windows(2)
        .map(|pair| {
            let (c0, p0) = pair[0];
            let (c1, p1) = pair[1];
            div()
                .absolute()
                .left(relative(p0))
                .w(relative(p1 - p0))
                .top(px(0.0))
                .bottom(px(0.0))
                .bg(linear_gradient(
                    90.0, // `to right`: 0deg is top, clockwise from there
                    linear_color_stop(c0, 0.0),
                    linear_color_stop(c1, 1.0),
                ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    //! Pins behaviour the module itself defines (not just its constants): a
    //! silent edit to `centre_fill`'s piecewise math or to the 24px field
    //! height below reddens these.

    /// `centre_fill` positions a centre-origin fill: 0% sits at the centre
    /// tick with zero width; positive values fill rightward from the centre;
    /// negative values fill leftward from the centre; and the width is always
    /// `|value| / 2`. RED proof: invert the sign of `width` and the negative
    /// assertions flip.
    #[test]
    fn centre_fill_is_signed_and_centred() {
        use super::centre_fill;

        assert_eq!(centre_fill(0.0), (50.0, 0.0));
        assert_eq!(centre_fill(100.0), (50.0, 50.0));
        assert_eq!(centre_fill(-100.0), (0.0, 50.0));
        assert_eq!(centre_fill(50.0), (50.0, 25.0));
        assert_eq!(centre_fill(-50.0), (25.0, 25.0));
    }

    /// The shared field box is exactly 24px tall — the one dimension every
    /// field control in the web build restates, captured here as `FIELD_H`.
    /// RED proof: change `FIELD_H` to anything else and this fails.
    #[test]
    fn field_height_is_24px() {
        assert_eq!(super::FIELD_H, 24.0);
    }
}
