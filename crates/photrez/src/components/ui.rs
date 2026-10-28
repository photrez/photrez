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
use gpui_kit::prelude::*;
use gpui_kit::{div, px, Div, FontWeight, TestSupportExt};

use crate::tokens::color;

mod fields;
mod slider;

pub use fields::{editable_num_field, field_chrome, num_field, prop_row, select_field};
pub use slider::{centre_fill, gradient_segments, slider};

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

/// The same row with an element id and a click handler, for production menus.
pub fn menu_row_action(
    label: &str,
    shortcut: Option<&str>,
    id: impl Into<gpui_kit::ElementId>,
    listener: impl Fn(&gpui_kit::ClickEvent, &mut gpui_kit::Window, &mut gpui_kit::App) + 'static,
) -> impl IntoElement {
    menu_row(label, shortcut, false, None)
        .id(id)
        .test_support()
        .on_click(listener)
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
