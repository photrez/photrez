use gpui_kit::Hsla;

use super::{TEXT, TEXT_DIM};

// --- Alpha-modified tokens (a base colour plus a percentage) ---

/// White at 5% on tool buttons + hover state on inactive buttons.
pub const WHITE_5: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 1.0,
    a: 0.05,
};

/// White at 10% on the active row in the variant fly-out.
pub const WHITE_10: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 1.0,
    a: 0.10,
};

/// A white border at 20% on the background swatch ring.
pub const WHITE_20: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 1.0,
    a: 0.20,
};

/// A white border at 30% on the foreground swatch ring.
pub const WHITE_30: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 1.0,
    a: 0.30,
};

/// A black outline at 40% on the foreground swatch.
pub const BLACK_40: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.0,
    a: 0.40,
};

/// Dim text at 80% - the sub-variant triangle indicator.
pub const TEXT_DIM_80: Hsla = Hsla {
    h: TEXT_DIM.h,
    s: TEXT_DIM.s,
    l: TEXT_DIM.l,
    a: 0.8,
};

// --- AppTitleBar / AppMenuBar ---

/// Text at 90% - a menu trigger at rest.
pub const TEXT_90: Hsla = Hsla {
    h: TEXT.h,
    s: TEXT.s,
    l: TEXT.l,
    a: 0.9,
};

/// Dim text at 60% - the `.` between the document name and the
/// "photrez" wordmark.
pub const TEXT_DIM_60: Hsla = Hsla {
    h: TEXT_DIM.h,
    s: TEXT_DIM.s,
    l: TEXT_DIM.l,
    a: 0.6,
};

/// White at 4.5% on hover - the shared faint hover: menu
/// triggers, undo/redo, the document tab's close button and the two
/// header icon buttons.
///
/// Four and a half percent, not forty-five. See [`WHITE_45_PCT`] for the
/// other value and why their names are so easy to swap.
pub const WHITE_45: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 1.0,
    a: 0.045,
};

/// White at 5.5% on hover - the minimize and maximize buttons.
pub const WHITE_55: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 1.0,
    a: 0.055,
};

/// White at 8% - the menu trigger whose popup is open.
pub const WHITE_80: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 1.0,
    a: 0.08,
};

/// Red at 85% on hover - the close-window button.
///
/// The original's red stop is `oklch(63.7% 0.237 25.331)` = `#FB2C36` =
/// rgb(251, 44, 54). Converted to HSL: max 251, min 44, delta 207,
/// hue 60 * ((44 - 54) / 207) = -2.90deg = 357.10deg, l = 147.5 / 255,
/// s = delta / (2 - max - min) = 0.8118 / 0.8431.
///
/// Only reachable on hover, which the title-bar baseline does not capture;
/// it is specified here for completeness rather than for the diff.
pub const RED_500_85: Hsla = Hsla {
    h: 0.991948,
    s: 0.962799,
    l: 0.578431,
    a: 0.85,
};

/// Accent at 15% - the crop bar's `Apply` interior in its resting
/// capture: `#3a2b20` (58, 43, 32) over the `#1b1b1b` bar, which is
/// `ACCENT` at 15% (the blend lands on 59, 44, 33). The transform bar's
/// full-strength `Apply` variant is not in any baseline, so this is pinned
/// from `option-crop.png`.
pub const ACCENT_15: Hsla = Hsla {
    h: 0.072,
    s: 0.8713,
    l: 0.6039,
    a: 0.15,
};

// -- Ad-hoc literals, not themed tokens ---------------------------------
//
// Everything above this line is a theme colour: one value, one meaning,
// reused across the app. The colours below are the
// other kind - hex literals written straight into a component's style
// declarations, so they have no variable to look up and are named here by
// the *part* they colour instead.
//
// All eleven come from the shared primitive controls,
// which is the module that introduces them and
// the reason they are worth sharing: `PropertiesPanel`,
// `AdjustmentsPanel` and the option bars all build on those controls, so a
// value pinned here is pinned once rather than transcribed per panel.
//
// Provenance is the literal hex, quoted; the themed values above
// were derived by the OKLCH -> sRGB -> HSL path, but these bypass it -
// there is no `oklch()` to expand, only a hex to turn into HSL.

/// `#A1A1AA` - the caption colour on a field: its `label` and its
/// `suffix`.
///
/// Not `color::TEXT_DIM` (#8c8c8c) and not `color::ICON` (#868686): this
/// one is a cool grey with a blue cast (hue 240deg, chroma 0.050), which is
/// what separates a field caption from the surrounding chrome.
pub const FIELD_CAPTION: Hsla = Hsla {
    h: 0.666667,
    s: 0.050296,
    l: 0.649020,
    a: 1.0,
};

/// `#4B515D` - the border a field moves to on hover
///
/// Only `SelectField` and `EditableNumField` ask for it; `NumField` has no
/// hover state at all, which is a real difference between the two and not
/// an omission.
pub const HOVER_BORDER: Hsla = Hsla {
    h: 0.611111,
    s: 0.107071,
    l: 0.329412,
    a: 1.0,
};

/// `#d8dce2` - the `Slider` thumb.
pub const SLIDER_THUMB: Hsla = Hsla {
    h: 0.600000,
    s: 0.147059,
    l: 0.866667,
    a: 1.0,
};

/// `#09090b` - the dark end of the `brightness` and `contrast` track
/// gradients.
pub const SLIDER_DARK: Hsla = Hsla {
    h: 0.666667,
    s: 0.100000,
    l: 0.039216,
    a: 1.0,
};

/// `#52525b` - `brightness`'s midpoint, and `saturation`'s start.
pub const SLIDER_MID_BRIGHT: Hsla = Hsla {
    h: 0.666667,
    s: 0.052019,
    l: 0.339216,
    a: 1.0,
};

/// `#71717a` - `contrast`'s midpoint, and `saturation`'s 35% stop.
pub const SLIDER_MID_CONTRAST: Hsla = Hsla {
    h: 0.666667,
    s: 0.038331,
    l: 0.460784,
    a: 1.0,
};

/// `#2d3139` - the light square of the transparency checkerboard that sits
/// under the layer-Opacity and brush-Strength slider tracks.
///
/// Paired with [`CHECK_DARK`]. Both are painted as an alternating checker
/// pattern rather than a flat fill, so a solid colour cannot substitute: the
/// alternation is the point, since a slider that fades to full transparency
/// has to show *something* through it.
pub const CHECK_LIGHT: Hsla = Hsla {
    h: 0.611111,
    s: 0.117647,
    l: 0.200000,
    a: 1.0,
};

/// `#1a1d24` - the dark square of that checkerboard.
pub const CHECK_DARK: Hsla = Hsla {
    h: 0.616667,
    s: 0.161290,
    l: 0.121569,
    a: 1.0,
};

/// `#ef4444` - `saturation`'s 60% stop; the palette's
/// mid red.
pub const RED_500: Hsla = Hsla {
    h: 0.0,
    s: 0.842424,
    l: 0.601961,
    a: 1.0,
};

/// `#eab308` - `saturation`'s 80% stop; the palette's
/// mid yellow.
pub const YELLOW_500: Hsla = Hsla {
    h: 0.126064,
    s: 0.933941,
    l: 0.474510,
    a: 1.0,
};

/// `#3b82f6` - `saturation`'s 100% stop; the palette's
/// mid blue.
pub const BLUE_500: Hsla = Hsla {
    h: 0.603354,
    s: 0.912216,
    l: 0.598039,
    a: 1.0,
};

/// Pale grey - the light end of the `brightness` fill.
pub const ZINC_100: Hsla = Hsla {
    h: 0.666667,
    s: 0.047619,
    l: 0.958824,
    a: 1.0,
};

/// Light grey - the light end of the `contrast` fill.
pub const ZINC_300: Hsla = Hsla {
    h: 0.666667,
    s: 0.048780,
    l: 0.839216,
    a: 1.0,
};

/// Pink - the start of the `saturation` fill, paired with
/// [`ROSE_500`].
pub const PINK_500: Hsla = Hsla {
    h: 0.917683,
    s: 0.811881,
    l: 0.603922,
    a: 1.0,
};

/// Rose - the end of the `saturation` fill.
pub const ROSE_500: Hsla = Hsla {
    h: 0.971455,
    s: 0.891626,
    l: 0.601961,
    a: 1.0,
};

/// Text at 55% - the `Slider` fill when `accent={false}`:
/// [`TEXT`] at 55% alpha.
pub const TEXT_55: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.792157,
    a: 0.55,
};

/// White at 45% - the `Slider`'s centre tick.
///
/// **Not** [`WHITE_45`], despite the identical-looking name. That one is
/// the fractional form, white at 4.5%, whose value and this
/// one's both read as "45" - so `WHITE_45` is four and a half
/// percent and this is forty-five. The suffix tracks the source's
/// spelling rather than a unit, which is what makes the collision
/// possible; the `_PCT` is here so the next reader does not assume the
/// two are the same number written twice.
pub const WHITE_45_PCT: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 1.0,
    a: 0.45,
};

/// A black border at 55% - the `Slider` thumb's own 1px border.
///
/// Distinct from [`BLACK_40`], which is the token this palette uses for
/// recessed shadows; the thumb is the one place asking for 55.
pub const BLACK_55: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.0,
    a: 0.55,
};

/// Unmodified white, `#ffffff` - the plain foreground colour.
///
/// The one value here that is not really a colour choice: it is the
/// absence of one, which is why the palette never gave it a variable.
/// It is named because it is everywhere
/// and each of its uses is a *foreground* that must not drift with the
/// chrome behind it - a field's value, a selected option, a label that
/// only appears once hovered. Spelling it as `l: 1.0` at every call site
/// would read as an arbitrary grey to the next reader.
pub const WHITE: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 1.0,
    a: 1.0,
};
