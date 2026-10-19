//! Visual-only port of `editor/Tooltip.tsx`, as captured on the tool rail.
//!
//! # One component, three labels — proven by the crops
//!
//! The three baselines are the *same* component with different strings; the
//! width is never set anywhere, it is whatever the label measures plus the
//! chrome around it. The three manifest boxes share a height to the pixel and
//! differ only in width, which is what a label-driven box looks like from the
//! outside:
//!
//! ```text
//! tooltip-rail-tool          94x26 @  50,59   "Move Tool"                 + V
//! tooltip-rail-variant-tool 243x26 @  50,97   "Rectangle Select (Right-  + M
//!                                                 click for options)"
//! tooltip-rail-swatch       104x26 @  42,743  "Foreground Color"          (none)
//! ```
//!
//! So there is one [`tooltip`] function and three `(label, shortcut)` pairs,
//! and every number below was read out of `baseline/shots/tooltip-*.png` with
//! the throwaway `baseline/probe-tooltips.ts` / `probe-tooltips2.ts` (crops —
//! crop 0,0 is the tooltip's own top-left corner, offset by the manifest
//! `box`, which is fractional: `y=58.75` etc.; the port renders the rounded
//! origins from the table above and GPUI cannot place a border on a half
//! pixel anyway).
//!
//! # The 26px height is *not* content-driven here
//!
//! The source has no height: `px-2 py-1` + a border + the 11px label's line
//! box give 26.5 in Chromium (that is the manifest's 26.5). GPUI computes its
//! own line box for Inter 11px, so leaving the height to the text would put
//! the border row wherever DirectWrite's metrics say — one engine-derivation
//! away from the baseline. The height is therefore pinned at 26 ([`TOOLTIP_H`])
//! with the content vertically centred, which lands the ink where the probe
//! found it: cap-height rows 10..17 of a box whose top border sits at crop
//! row ~1. The one thing that *was* measured rather than assumed inside that
//! box is the kbd chip: the probe reads it 15px tall (crop rows 7..21,
//! border to border) — `9px text at leading-none + py-0.5 (2+2) + border
//! (1+1)` — so [`KBD_H`] is pinned to that and centred like everything else.
//!
//! # Colours, all measured
//!
//! * border `border-white/10` → probes read **#333333** on all four sides,
//!   the unlayered `* { border-color }` override every `border-*` in Photrez
//!   loses to ([`color::BORDER`]; see `tokens.rs` for the cascade at length).
//! * fill `bg-editor-panel` → #1e1e1e interior, [`color::PANEL`].
//! * label `text-editor-text` → peak ink #cacaca, [`color::TEXT`].
//! * kbd `bg-white/10` → #353535 flat fill, which is [`color::WHITE_10`]
//!   composited over the panel; kbd text `text-white/50` → glyph plateaus at
//!   #9a9a9a = white at 50% over that fill (local [`WHITE_50`], no token
//!   exists); kbd `border-white/5` → #333333 again, same override.
//! * shadow `0 4px 12px rgba(0,0,0,0.5)` → one [`BoxShadow`], transcribed
//!   stop for stop, the same way `tool_rail.rs` spells `shadow-md` out.
//!
//! # No arrow — checked, not assumed
//!
//! Tooltip components often carry a `clip-path` notch pointing at the
//! trigger; the project-wide gap list calls that out as inexpressible in
//! GPUI. This one does not need it: the probe's corner ramp is a plain 4px
//! `rounded-[4px]` arc on all four corners of all three crops, with no
//! protrusion on the placement side (or any side). `Tooltip.tsx` renders a
//! bare div; the gap does not apply here.
//!
//! # Positioning: `fixed` + portal has no GPUI mapping
//!
//! The source computes viewport coordinates on hover and portals the node to
//! `document.body` with `position: fixed`. Neither concept exists in a GPUI
//! child element, so — following the convention `context_menus.rs` set for
//! the same problem — [`render`] returns a transparent full-client-area
//! absolute frame with the tooltip pinned at its manifest origin inside.
//! Consequences, stated rather than worked around:
//!
//! * the caller must mount this **late** in the element tree: GPUI has no
//!   `z_index`, and late-in-tree is the only ordering that paints on top;
//! * the coordinates are the captured ones, not anchor arithmetic —
//!   anchoring needs the trigger's rect at hover time, which is interaction;
//! * `transition-opacity duration-100` and the `opacity-0` pre-measure state
//!   are unrenderable and unneeded: the baseline settles at full opacity
//!   (`settle: 700` outlasts the 100ms fade), so this paints `opacity-100`
//!   unconditionally.
//!
//! # Not reused from `ui.rs`
//!
//! All four shared helpers were considered and none fits: [`ui::caption`] is
//! 10px/500 in the caption grey, [`ui::value_sans`] is 11px but **600 white**,
//! [`ui::value_mono`] adds `font-mono`, [`ui::field_chrome`] is the 24px
//! field box. The tooltip label is 11px Inter *regular* in `text-editor-text`
//! — no existing run matches it, so [`label_run`] and [`kbd`] are local. If a
//! second component turns up wanting an 11px regular `#cacaca` run, those two
//! are the consolidation candidates (into `ui.rs` here, `tokens.rs` for
//! [`WHITE_50`]).
//!
//! # i18n
//!
//! Labels are the English fallbacks the capture ran with:
//! `t("tools.move", "Move Tool")`, the selection variant resolved through
//! `VARIANT_I18N_KEYS` → `t("tools.rectSelect", "Rectangle Select")` plus
//! `t("tools.rightClickOptions", " (Right-click for options)")`, and
//! `t("tools.foregroundColor", "Foreground Color")`. The kbd `uppercase` is
//! a no-op on "V"/"M" and needs no GPUI equivalent.

use gpui_kit::base::h_flex;
use gpui_kit::prelude::*;
use gpui_kit::{div, hsla, px, BoxShadow, Div, FontWeight, Hsla};

use crate::components::ui;
use crate::shot;
use crate::tokens::color;

// ── Geometry ─────────────────────────────────────────────────────────────────

/// The client area, duplicated from `main.rs`'s private `CLIENT_W`/`CLIENT_H`
/// for the overlay frame — same call `context_menus.rs` makes, for the same
/// reason (the constants are private and this file needs them).
const FRAME_W: f32 = 1366.0;
const FRAME_H: f32 = 820.0;

/// The rendered tooltip height. The source's is content-driven (26.5 in
/// Chromium); pinned here because GPUI's Inter line box is not Chromium's —
/// see the module docs.
pub const TOOLTIP_H: f32 = 26.0;

/// `px-2` — horizontal padding; label ink starts at crop x=10, i.e. 1 border
/// + 8 padding + the "M" glyph's ~1px side bearing.
const PAD_X: f32 = 8.0;

/// `rounded-[4px]` — the probe's corner ramp spans ~3px of AA per corner on
/// every crop, consistent with a 4px arc and with nothing larger.
const RADIUS: f32 = 4.0;

/// `gap-1.5` between the label span and the kbd. The kbd *also* carries
/// `ml-1` (4px), so the two together are 10px of floor — transcribed as
/// [`KBD_ML`] on the chip rather than folded into the gap, matching the
/// source's own composition.
const GAP: f32 = 6.0;

/// `text-[11px]` — the label; the source gives it no `font-weight`, i.e.
/// regular 400, which is also GPUI's default (set explicitly below anyway so
/// the run is self-documenting).
const LABEL_TEXT: f32 = 11.0;

/// `ml-1` on the kbd.
const KBD_ML: f32 = 4.0;
/// `px-1` on the kbd.
const KBD_PX: f32 = 2.0;
/// Measured kbd height, border to border: crop rows 7..21 in
/// `tooltip-rail-tool.png` = 9px `leading-none` line + `py-0.5` (2+2) +
/// border (1+1).
const KBD_H: f32 = 15.0;
/// `text-[9px] font-semibold` — the shortcut glyph.
const KBD_TEXT: f32 = 9.0;
const KBD_WEIGHT: f32 = 600.0;

// `--font-sans` — Inter, the same face `main.rs` registers and every other
// port uses.

// ── Colours ─────────────────────────────────────────────────────────────────

/// `text-white/50` on the kbd — no token exists for it (the `WHITE_*` ramp in
/// `tokens.rs` covers 4.5/5/5.5/8/10/20/30/45/80, not 50). Measured: glyphs
/// plateau at #9a9a9a, which is white 50% over the #353535 chip fill — the
/// composite checks out to the integer. Marked for consolidation into
/// `tokens.rs` when a second consumer appears.
const WHITE_50: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 1.0,
    a: 0.5,
};

/// `shadow-[0_4px_12px_rgba(0,0,0,0.5)]` — one layer, transcribed stop for
/// stop.
fn tooltip_shadow() -> Vec<BoxShadow> {
    vec![BoxShadow::new(px(0.), px(4.), hsla(0., 0., 0., 0.5)).blur_radius(px(12.))]
}

// ── Captured state ───────────────────────────────────────────────────────────

/// What one baseline tooltip is: where it was captured, and the two strings
/// that make its width what it is. `shortcut: None` renders no kbd, which is
/// the whole difference between the swatch tooltip and the two rail ones.
#[derive(Debug, PartialEq)]
struct Spec {
    x: f32,
    y: f32,
    label: &'static str,
    shortcut: Option<&'static str>,
}

/// The requested shot's spec, read from [`shot::active`].
fn spec() -> Spec {
    spec_for(shot::active())
}

/// Maps a shot id to its tooltip spec. Extracted from [`spec`] so the
/// anchor table is unit-testable.
fn spec_for(id: Option<&str>) -> Spec {
    match id {
        // `t("tools.rectSelect", "Rectangle Select")` for the rect variant
        // (`selectionShape` defaults to "rect"), plus the variants suffix.
        Some("tooltip-rail-variant-tool") => Spec {
            x: 50.0,
            y: 97.0,
            label: "Rectangle Select (Right-click for options)",
            shortcut: Some("M"),
        },
        Some("tooltip-rail-swatch") => Spec {
            x: 42.0,
            y: 743.0,
            label: "Foreground Color",
            shortcut: None,
        },
        // "tooltip-rail-tool" and unset/unknown: the default state.
        _ => Spec {
            x: 50.0,
            y: 59.0,
            label: "Move Tool",
            shortcut: Some("V"),
        },
    }
}

// ── Render ───────────────────────────────────────────────────────────────────

/// A transparent client-area frame with the requested tooltip pinned inside
/// it — the `context_menus.rs` convention for a `position: fixed` + portal
/// source. Mount late in the tree: that is this port's only `z-[100]`.
pub fn render() -> impl IntoElement {
    let spec = spec();
    let tip: Div = tooltip(spec.label, spec.shortcut)
        .absolute()
        .left(px(spec.x))
        .top(px(spec.y));

    div()
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .w(px(FRAME_W))
        .h(px(FRAME_H))
        .child(tip)
}

/// The tooltip box: fixed height, label-driven width (no `w()` anywhere —
/// GPUI hugs the content row, exactly as the source's auto-width div does),
/// 4px corners, the #333333 edge every Photrez border actually paints, panel
/// fill, and the drop shadow.
///
/// `whitespace-nowrap` needs no equivalent: without a width constraint GPUI
/// does not wrap, which is the same reason the source needs the class in the
/// first place.
fn tooltip(label: &str, shortcut: Option<&str>) -> Div {
    let mut row = h_flex().items_center().gap(px(GAP)).child(label_run(label));
    if let Some(keys) = shortcut {
        row = row.child(kbd(keys));
    }

    div()
        .h(px(TOOLTIP_H))
        .items_center()
        .px(px(PAD_X))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::PANEL)
        .shadow(tooltip_shadow())
        .child(row)
}

/// `<span class="text-[11px] font-sans text-editor-text">` — the content
/// string. `.child()` copies the `&str` (see `ui.rs` on why the `to_string`
/// is load-bearing).
fn label_run(text: &str) -> Div {
    div()
        .font_family(ui::FONT_UI)
        .text_size(px(LABEL_TEXT))
        .font_weight(FontWeight(400.))
        .text_color(color::TEXT)
        .child(text.to_string())
}

/// `<kbd class="ml-1 px-1 py-0.5 rounded bg-white/10 text-white/50
/// text-[9px] font-sans font-semibold border border-white/5 uppercase
/// leading-none">`.
///
/// Height is pinned to the measured 15px rather than left to `leading-none`:
/// GPUI has no line-height control, so the 9px line box is whatever DirectWrite
/// says, and pinning keeps the chip's border rows on crop 7/21 where the probe
/// put them. `uppercase` is dropped as a no-op — see the module docs.
fn kbd(keys: &str) -> Div {
    div()
        .h(px(KBD_H))
        .flex_shrink_0()
        .items_center()
        .ml(px(KBD_ML))
        .px(px(KBD_PX))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::WHITE_10)
        .text_color(WHITE_50)
        .child(
            div()
                .font_family(ui::FONT_UI)
                .text_size(px(KBD_TEXT))
                .font_weight(FontWeight(KBD_WEIGHT))
                .child(keys.to_string()),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The anchor table maps each shot id to its manifest origin, label, and
    /// shortcut. `None` / unknown fall through to the default (`tooltip-rail-tool`).
    ///
    /// RED proof: change any Spec field (e.g. swatch `y` to 744.0, or variant
    /// `shortcut` to `Some("V")`) and the corresponding assertion fails.
    /// GREEN: every shot maps to its captured values.
    #[test]
    fn spec_table_matches_manifest_origins() {
        // tooltip-rail-tool: the default (no shortcut key in the swatch).
        let tool = spec_for(Some("tooltip-rail-tool"));
        assert_eq!(tool.x, 50.0, "rail-tool x");
        assert_eq!(tool.y, 59.0, "rail-tool y (manifest 58.75 rounded)");
        assert_eq!(tool.label, "Move Tool", "rail-tool label");
        assert_eq!(tool.shortcut, Some("V"), "rail-tool shortcut");

        // tooltip-rail-variant-tool: the selection variant with suffix.
        let variant = spec_for(Some("tooltip-rail-variant-tool"));
        assert_eq!(variant.x, 50.0, "variant x");
        assert_eq!(variant.y, 97.0, "variant y (manifest 96.75 rounded)");
        assert_eq!(
            variant.label, "Rectangle Select (Right-click for options)",
            "variant label with suffix"
        );
        assert_eq!(variant.shortcut, Some("M"), "variant shortcut");

        // tooltip-rail-swatch: no shortcut (placement=right on the swatch).
        let swatch = spec_for(Some("tooltip-rail-swatch"));
        assert_eq!(swatch.x, 42.0, "swatch x (manifest 41.6875 rounded)");
        assert_eq!(swatch.y, 743.0, "swatch y (manifest 742.75 rounded)");
        assert_eq!(swatch.label, "Foreground Color", "swatch label");
        assert_eq!(swatch.shortcut, None, "swatch has no shortcut kbd");

        // Unset shot → default (tooltip-rail-tool).
        let none = spec_for(None);
        assert_eq!(none.x, 50.0);
        assert_eq!(none.y, 59.0);
        assert_eq!(none.label, "Move Tool");
        assert_eq!(none.shortcut, Some("V"));

        // Unknown shot → default as well.
        let unknown = spec_for(Some("tooltip-bogus"));
        assert_eq!(unknown, none, "unknown shots fall through to default");
    }

    /// The tooltip's pinned height is 26px (the source's 26.5 in Chromium,
    /// rounded for GPUI's line-box handling).
    #[test]
    fn tooltip_height_is_pinned() {
        assert!((TOOLTIP_H - 26.0).abs() < 0.001, "TOOLTIP_H = {TOOLTIP_H}");
    }

    /// The kbd chip is 15px tall (border to border), measured from the probe
    /// crop at rows 7..21 in `tooltip-rail-tool.png`.
    #[test]
    fn kbd_height_is_measured() {
        assert!((KBD_H - 15.0).abs() < 0.001, "KBD_H = {KBD_H}");
    }

    /// The kbd text weight is semibold (600), matching `font-semibold`.
    #[test]
    fn kbd_weight_is_semibold() {
        assert!(
            (KBD_WEIGHT - 600.0).abs() < 0.001,
            "KBD_WEIGHT = {KBD_WEIGHT}"
        );
    }

    /// The label font size is 11px, matching `text-[11px]` in the source.
    #[test]
    fn label_size_is_eleven() {
        assert!(
            (LABEL_TEXT - 11.0).abs() < 0.001,
            "LABEL_TEXT = {LABEL_TEXT}"
        );
    }
}
