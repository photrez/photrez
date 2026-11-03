#![allow(dead_code)]
// shared control vocabulary for the panels that land after the shell; nothing imports this module yet

//! The five shared field controls the panels are built on.
//!
//! # Why these are a module
//!
//! This is the one place a control is *defined* rather than assembled:
//! `NumField`, `SelectField`, `PropRow`, `Slider` and `EditableNumField` are
//! each a single function that takes props and returns markup, and every panel
//! imports them instead of writing its own. The option bar and all four
//! inspector panels are built on them, so defining them once here is what keeps
//! those five usages from diverging — the same twenty-four pixel box, the same
//! caption grey, the same Consolas value, in each.
//!
//! These controls are **visual only**, matching the project's first pass: everything
//! that a resting screenshot can see is reproduced, and everything that needs
//! an event loop is not yet. Concretely, what is here:
//!
//! * the box geometry, borders, fills and radii;
//! * every colour, resolved through [`crate::tokens`] rather than restated;
//! * the value/caption typography, including the monospace face;
//! * `Slider`'s per-kind track — the gradients, the checkerboard, the
//!   centre-origin fills, the thumb.
//!
//! And what is not, with the reason at each site:
//!
//! * **interaction** — `EditableNumField` cannot be edited, `SelectField`
//!   cannot open, `Slider` cannot be dragged. These need focus, a keymap and a
//!   mutable value, which is `Render`/`Entity` territory rather than a
//!   function returning `impl IntoElement`, and the captured baseline shows
//!   none of it.
//! * **the focus ring** — same problem: there is no focus state to ring.
//!   A 1px-spread `BoxShadow` would stand in for the ring if a focused variant
//!   is ever needed.
//! * **colour transitions and the hover grow** — GPUI has no declarative
//!   per-element transition, so the *hover* colours are also absent rather
//!   than painted permanently. Painting them would be worse than omitting
//!   them: a hover state baked into the resting screenshot is a pixel the
//!   diff would then have to explain away.
//!
//! # Two conventions inherited from the option bar
//!
//! The monospace face is rendered as **Consolas**, and `#A1A1AA` as
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

pub use fields::{
    editable_num_field, field_chrome, live_num_field, num_field, prop_row, select_field,
};
pub use slider::{centre_fill, gradient_segments, slider};

// ── Geometry ─────────────────────────────────────────────────────────────────

/// 24px tall — every field control, without exception.
const FIELD_H: f32 = 24.0;
/// 4px corner radius on the field box.
const RADIUS: f32 = 4.0;
/// 4px between a field's label, value and suffix.
const INNER_GAP: f32 = 4.0;
/// 6px horizontal padding on `NumField` and `EditableNumField`.
const PAD_X: f32 = 6.0;
/// 8px horizontal padding on `SelectField` — wider, because it justifies
/// content to both edges instead of packing it.
const SELECT_PAD_X: f32 = 8.0;
/// 50px-wide [`PropRow`] label, never squeezed.
const LABEL_W: f32 = 50.0;
/// 8px between a [`PropRow`]'s label and its slot.
const ROW_GAP: f32 = 8.0;
/// 6px inside a [`PropRow`]'s content slot.
const SLOT_GAP: f32 = 6.0;
/// 12px square — the `SelectField` chevron.
const ICON_SM: f32 = 12.0;
/// 10px — captions.
const TEXT_XS: f32 = 10.0;
/// 11px — values and row labels.
const TEXT_SM: f32 = 11.0;

// ── Faces ────────────────────────────────────────────────────────────────────

/// The monospace face, i.e. Consolas — values in `NumField` and
/// `EditableNumField`.
///
/// The numeric field values use a monospace face. On this
/// Windows build the face that resolves is Consolas. It stands in for the
/// reference's monospace stack rather than being that stack literally, a gap
/// first recorded in the option bar's font notes and unchanged here.
///
/// Public for the same reason as [`FONT_UI`]: view modules that render mono
/// values read it directly instead of spelling "Consolas" per module.
pub const FONT_MONO: &str = "Consolas"; // i18n-exempt: technical token

/// The sans face — Inter, on everything that is *not* monospace.
/// Inter, the font every label uses.
///
/// Public because thirteen modules each declared their own copy before this
/// existed, and a font that is spelled out per-module is a font that can be
/// changed in one place and missed in twelve. The default `.SystemUIFont`
/// (Segoe UI) cannot stand in: every measurement was taken against Inter.
pub const FONT_UI: &str = "Inter"; // i18n-exempt: technical token

/// A menu row's measured height, and its insets — the same figures both menu
/// modules measured independently.
const MENU_ROW_H: f32 = 28.0;
const MENU_ROW_PX: f32 = 12.0;
const MENU_ROW_GAP: f32 = 24.0;
const MENU_ROW_TEXT: f32 = 12.0;
const MENU_SHORTCUT_TEXT: f32 = 10.0;

/// 4px vertical margin around a separator, plus the line itself.
const MENU_SEP_MY: f32 = 4.0;

// ── Shared runs ──────────────────────────────────────────────────────────────

/// A 10px, medium-weight, `#A1A1AA`, non-selectable caption run.
///
/// One helper because these are byte-identical everywhere: a field's `label`,
/// its `suffix`, `PropRow`'s label and `EditableNumField`'s suffix share one
/// styling run, and they read the same on screen too.
/// A run of text with its size, weight and colour — the shape that was
/// hand-rolled ten times across the views as
/// `div().text_size(..).font_family(FONT_UI).font_weight(..).text_color(..)`.
///
/// Four setters in a fixed order is a lot of places to get the order wrong in,
/// and the order is load-bearing: `text_color` after `.bg()` tints the glyphs,
/// while the same pair before a parent's colour can be inherited differently.
/// One helper means one place to reason about it.
///
/// `weight` takes a raw `f32` rather than `FontWeight` so a call site can pass
/// a literal `500.` without importing the wrapper type.
/// One row of a popup menu: label left, shortcut right, 28px tall.
///
/// Shared by the pointer-anchored context menus and the title-bar popups
/// because the row chrome is identical — only the *shell* differs, which is
/// why the two shells are not merged but this is. Both measured ROW_H=28,
/// ROW_GAP=24, ROW_PX=12 independently and arrived at the same numbers.
///
/// `focus_bg` is `Option` rather than a plain colour because the two callers
/// genuinely differ here: the pointer-anchored context menus capture a
/// focus-visible accent tint (accent at 15% opacity) on the first enabled row
/// (a state the keyboard produces, not the mouse), while the title-bar popups
/// have no captured focus tint at all. `None` paints no background, which is
/// what a static render of those should do.
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
/// GPUI does not clip a text box by default the way a dedicated truncation
/// widget would, so every row that must yield has to say so out loud with this
/// helper.
pub fn clip<T: gpui_kit::Styled>(el: T) -> T {
    el.min_w(px(0.0)).overflow_hidden()
}

/// The opposite of [clip]: hold your intrinsic width and refuse to give it up.
/// Right for a chevron, a shortcut, a counter -- anything that must stay whole
/// while the text beside it is the part that should truncate.
pub fn pin<T: gpui_kit::Styled>(el: T) -> T {
    el.flex_none()
}

/// An inset well: a shadow drawn *inside* the box, over its own background.
///
/// This is the recessed-box layer every 24px control in the design carries
/// (`shadow-inner`): the gradient strip, the stroke-position segmented track,
/// the unchecked option-bar checkbox and the About logo tile. It costs no
/// layout — a box with a well has the same content box as a box without one,
/// which is why it is a shadow layer and not a border.
pub fn inset_well() -> gpui_kit::BoxShadow {
    gpui_kit::BoxShadow::new(px(0.), px(2.), gpui_kit::hsla(0.0, 0.0, 0.0, 0.05))
        .blur_radius(px(4.))
        .inset()
}

/// The 28px- tall dialog field well, one step deeper than [`inset_well`]: a
/// 1px offset with a 2px blur at 35% black.
///
/// Applied to the field chrome that the dialogs share — new-document inputs and
/// selects, the resize dimension inputs and their unit pickers. Not applied to
/// the print or export dialogs' own rows, whose reference controls carry no
/// inset layer.
pub fn field_well() -> gpui_kit::BoxShadow {
    gpui_kit::BoxShadow::new(px(0.), px(1.), gpui_kit::hsla(0.0, 0.0, 0.0, 0.35))
        .blur_radius(px(2.))
        .inset()
}

/// [`field_well`] tinted with the accent instead of black, for the control that
/// changes colour when it engages: the locked `Keep proportions` pill.
pub fn accent_well() -> gpui_kit::BoxShadow {
    gpui_kit::BoxShadow::new(px(0.), px(1.), color::ACCENT_15)
        .blur_radius(px(2.))
        .inset()
}

/// A 1px ring painted *inside* the edge: the inset form of the spread-only ring
/// this palette already builds for option-bar swatches.
///
/// An inset ring and a 1px border put the same line on the same pixel edge, but
/// only the ring leaves the content box alone, so children stay exactly where
/// they would sit in the reference.
pub fn inset_ring(colour: gpui_kit::Hsla) -> gpui_kit::BoxShadow {
    gpui_kit::BoxShadow::new(px(0.), px(0.), colour)
        .spread_radius(px(1.))
        .inset()
}

/// The same spread with no `.inset()`: a halo that grows *outside* the box,
/// which is what a plain `ring-1` is. [`inset_ring`] and this are not
/// interchangeable — the reference uses the inset form for layer rows and the
/// outset form for field halos, and they land on different pixels.
pub fn outset_ring(colour: gpui_kit::Hsla) -> gpui_kit::BoxShadow {
    gpui_kit::BoxShadow::new(px(0.), px(0.), colour).spread_radius(px(1.))
}

/// An inset accent band approximating the focus outline declared in the pinned
/// reference source. Its compiled cascade and pointer-focus behavior are
/// unverified. GPUI has no outline property, so this uses an inset spread.
pub fn focus_band(colour: gpui_kit::Hsla) -> gpui_kit::BoxShadow {
    gpui_kit::BoxShadow::new(px(0.), px(0.), colour)
        .spread_radius(px(1.5))
        .inset()
}

/// The layers a bare numeric field carries while it is being edited.
///
/// The pinned reference source declares a focus outline for number inputs, but
/// the compiled cascade and pointer-focus behavior are unverified. The edge
/// stays [`color::BORDER`] at the call site; the accent is a shadow.
pub fn outline_cue(active: bool) -> Vec<gpui_kit::BoxShadow> {
    if active {
        vec![focus_band(color::ACCENT.opacity(0.7))]
    } else {
        Vec::new()
    }
}

/// The layers a labelled number field carries while it is being edited.
///
/// The reference wraps the input in a box that grows a 1px accent/70 halo
/// outside its border, so unlike [`outline_cue`] the cue follows the wrapper
/// rather than the text control inside it.
pub fn ring_cue(active: bool) -> Vec<gpui_kit::BoxShadow> {
    if active {
        vec![outset_ring(color::ACCENT.opacity(0.7))]
    } else {
        Vec::new()
    }
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
    // Whole-row, not label-only: the 0.35 disabled opacity is applied to the
    // whole row, so the shortcut dims with the label. Getting this wrong shows
    // up as a shortcut that reads brighter than the item it belongs to.
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

/// A 4px vertical margin around a 1px full-bleed divider — a probe reads
/// separator rows edge to edge, so there is no horizontal inset.
///
/// Shared for the same reason [`menu_row`] is. Note the colour is `DIVIDER`
/// (#2e2e2e), *not* `BORDER` (#333333): both read as "a border", but the
/// separator resolves to the divider grey, and a scan looking only for #333333
/// reports the separators as missing.
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
        // clips "100" down to "10" the way an edit field would.
        .flex_none()
        .text_size(px(TEXT_XS))
        .font_weight(FontWeight(500.))
        .text_color(color::FIELD_CAPTION)
        // `.child()` builds a tree that outlives this call, so the borrowed
        // `&str` is copied: `String` and `SharedString` both implement
        // `IntoElement`, `&str` does not unless it is `'static`.
        .child(text.to_string())
}

/// A numeric field's value run — 11px, semibold, monospace, white.
pub fn value_mono(text: &str) -> Div {
    div()
        .font_family(FONT_MONO)
        .text_size(px(TEXT_SM))
        .font_weight(FontWeight(600.))
        .text_color(color::WHITE)
        .child(text.to_string())
}

/// `SelectField`'s value run — 11px, semibold, set in Inter (not mono),
/// white.
///
/// Deliberately *not* [`value_mono`]: this one is not set in mono, so it
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

/// [`SliderKind`] — which track treatment to paint.
///
/// The variant is not decoration: it selects the background, whether the fill
/// grows from the centre or the left, and how tall the whole control is. All
/// three are driven off the single kind in one place, which is why they are
/// one enum here rather than five functions.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SliderKind {
    /// `default` — a plain trough and an accent fill from the left.
    Default,
    /// the layer-Opacity and brush-Strength sliders — a checkerboard under a
    /// fade to accent, because the slider fades to no colour and needs
    /// something to show through it.
    Alpha,
    /// `brightness` — black → `color::SLIDER_MID_BRIGHT` → white.
    Brightness,
    /// `contrast` — black → `color::SLIDER_MID_CONTRAST` → white.
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
    /// field control restates, captured here as `FIELD_H`.
    /// RED proof: change `FIELD_H` to anything else and this fails.
    #[test]
    fn field_height_is_24px() {
        assert_eq!(super::FIELD_H, 24.0);
    }

    /// The three inset wells are the reference's shadow layers: inset, at the
    /// reference offset, blur and alpha, with no spread.
    ///
    /// The harness cannot read a painted shadow (gpui-kit's `TestWindowExt`
    /// snapshot exposes bounds and visibility only), so this pins the values the
    /// call sites hand to `.shadow(...)`; the per-site wiring is cited in the
    /// G25 register rows these wells close.
    ///
    /// RED proof: drop `.inset()` from any helper and its `inset` assertion
    /// fails; edit an offset, blur or alpha and that row fails.
    #[test]
    fn inset_wells_are_the_reference_shadow_layers() {
        use gpui_kit::{hsla, px};

        let well = super::inset_well();
        assert!(well.inset, "the 24px well is drawn inside the box");
        assert_eq!((well.offset.x, well.offset.y), (px(0.), px(2.)));
        assert_eq!(well.blur_radius, px(4.));
        assert_eq!(well.spread_radius, px(0.));
        assert_eq!(well.color, hsla(0.0, 0.0, 0.0, 0.05));

        let field = super::field_well();
        assert!(field.inset, "the dialog field well is drawn inside the box");
        assert_eq!((field.offset.x, field.offset.y), (px(0.), px(1.)));
        assert_eq!(field.blur_radius, px(2.));
        assert_eq!(field.color, hsla(0.0, 0.0, 0.0, 0.35));

        let accent = super::accent_well();
        assert!(accent.inset, "the locked pill's glow is drawn inside it");
        assert_eq!((accent.offset.x, accent.offset.y), (px(0.), px(1.)));
        assert_eq!(accent.blur_radius, px(2.));
        assert_eq!(accent.color, crate::tokens::color::ACCENT_15);
    }

    /// An inset ring is a 1px spread shadow with no offset or blur, drawn
    /// inside the edge, in the colour it was given.
    /// RED proof: drop `.inset()` or change the spread and the ring assertions
    /// fail; pass a colour through and compare it to prove the argument is used.
    #[test]
    fn inset_ring_is_a_one_pixel_inset_spread() {
        use gpui_kit::{hsla, px};

        let tint = hsla(0.08, 0.9, 0.6, 0.4);
        let ring = super::inset_ring(tint);
        assert!(ring.inset);
        assert_eq!((ring.offset.x, ring.offset.y), (px(0.), px(0.)));
        assert_eq!(ring.blur_radius, px(0.));
        assert_eq!(ring.spread_radius, px(1.));
        assert_eq!(ring.color, tint);
    }

    /// The halo is the inset ring's mirror: same 1px spread, no offset or blur,
    /// but drawn outside the box.
    /// RED proof: add `.inset()` or change the spread and the assertions fail.
    #[test]
    fn outset_ring_is_a_one_pixel_halo_outside_the_box() {
        use gpui_kit::{hsla, px};

        let ring = super::outset_ring(hsla(0.08, 0.9, 0.6, 0.7));
        assert!(!ring.inset, "a plain ring grows outside the box");
        assert_eq!((ring.offset.x, ring.offset.y), (px(0.), px(0.)));
        assert_eq!(ring.blur_radius, px(0.));
        assert_eq!(ring.spread_radius, px(1.));
    }

    /// The focus band keeps the reference's 1.5px width and its inset side.
    /// RED proof: round the spread to 1px or drop `.inset()` and it fails.
    #[test]
    fn focus_band_is_a_one_and_a_half_pixel_inset_band() {
        use gpui_kit::px;

        let band = super::focus_band(crate::tokens::color::ACCENT);
        assert!(band.inset, "the outline sits inside the edge");
        assert_eq!(band.spread_radius, px(1.5));
        assert_eq!(band.blur_radius, px(0.));
    }

    /// Both field cues are the accent at 70% and only exist while the field is
    /// being edited; the edge they replace is a colour the site keeps itself.
    /// RED proof: make a cue return the full accent, or give an inactive field a
    /// layer, and these fail.
    #[test]
    fn field_cues_are_accent_seventy_and_only_while_active() {
        use crate::tokens::color;

        for cue in [super::ring_cue(false), super::outline_cue(false)] {
            assert!(cue.is_empty(), "an idle field carries no accent layer");
        }

        let ring = &super::ring_cue(true)[0];
        assert_eq!(ring.color, color::ACCENT.opacity(0.7));
        assert!(!ring.inset, "the wrapper's halo grows outside its edge");

        let band = &super::outline_cue(true)[0];
        assert_eq!(band.color, color::ACCENT.opacity(0.7));
        assert!(band.inset, "the input's outline sits inside its edge");
    }
}
