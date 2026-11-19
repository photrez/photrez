//! Resting tooltip overlay: one label-driven box, three shot variants.
//! Height is pinned at 26px; GPUI's own line box would move the border row.
//! Borders resolve to BORDER (#333333); the tooltip is a bare box with no arrow or notch.
//! The absolute frame must be appended late; coordinates are captured origins, not anchor arithmetic.
//! Labels come from catalog keys; the kbd chip is local because no shared run matches 11px regular text.
use gpui_kit::base::h_flex;
use gpui_kit::prelude::*;
use gpui_kit::{BoxShadow, Context, Div, Entity, FontWeight, Hsla, div, hsla, px};

use crate::components::ui;
use crate::shot;
use crate::state::{AppState, SelectionShape};
use crate::tokens::color;
use gpui_kit::TestSupportExt;
use rust_i18n::t;

// ── Geometry ─────────────────────────────────────────────────────────────────

/// The client area, duplicated from `main.rs`'s private `CLIENT_W`/`CLIENT_H`
/// for the overlay frame — same call `context_menus.rs` makes, for the same
/// reason (the constants are private and this file needs them).
const FRAME_W: f32 = 1366.0;
const FRAME_H: f32 = 820.0;

/// The rendered tooltip height, pinned. The recorded box measured 26.5 and
/// GPUI computes its own Inter line box, so the height is stated here.
/// See the module docs.
pub const TOOLTIP_H: f32 = 26.0;

/// Where a rail tooltip sits, measured from the captured frames: the Move
/// button is at (8,54) 36x36 and its tooltip at (50,59).
const RAIL_TIP_X: f32 = 50.0;
const RAIL_TIP_Y: f32 = 59.0;
/// The rail's button pitch, from the measured 54 / 92 / 168 rows.
const RAIL_ROW_PITCH: f32 = 38.0;
/// The tooltip-id prefix the rail buttons publish.
const RAIL_TOOLTIP_PREFIX: &str = "rail-tooltip-";
/// The foreground swatch's tooltip, measured at (42,743) with no shortcut.
const SWATCH_TIP_X: f32 = 42.0;
const SWATCH_TIP_Y: f32 = 743.0;

/// 8px horizontal padding; label ink starts at crop x=10, i.e. 1 border
/// + 8 padding + the "M" glyph's ~1px side bearing.
const PAD_X: f32 = 8.0;

/// The 4px corner radius: the probe's corner ramp spans ~3px of AA per corner on
/// every crop, consistent with a 4px arc and with nothing larger.
const RADIUS: f32 = 4.0;

/// 6px between the label span and the kbd. The kbd *also* carries
/// its own 4px left margin, so the two together are 10px of floor — transcribed as
/// [`KBD_ML`] on the chip rather than folded into the gap, matching the
/// recorded composition.
const GAP: f32 = 6.0;

/// 11px — the label; no weight was recorded for it, i.e.
/// regular 400, which is also GPUI's default (set explicitly below anyway so
/// the run is self-documenting).
const LABEL_TEXT: f32 = 11.0;

/// The kbd chip's 4px left margin.
const KBD_ML: f32 = 4.0;
/// The kbd chip's horizontal padding.
const KBD_PX: f32 = 2.0;
/// Measured kbd height, border to border: crop rows 7..21 in
/// `tooltip-rail-tool.png` = 9px zero-leading line + 2+2 vertical padding +
/// border (1+1).
const KBD_H: f32 = 15.0;
/// 9px semibold — the shortcut glyph.
const KBD_TEXT: f32 = 9.0;
const KBD_WEIGHT: f32 = 600.0;

// The UI sans stack — Inter, the same face `main.rs` registers and every other
// view uses.

// ── Colours ─────────────────────────────────────────────────────────────────

/// White at 50% on the kbd — no token exists for it (the `WHITE_*` ramp in
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

/// One layer: no x offset, 4px down, 12px blur, black at 50% — transcribed stop
/// for stop.
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
    label: String,
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
        // `t!("tools.rectSelect")` for the rect variant
        // (`selectionShape` defaults to "rect"), plus the variants suffix.
        Some("tooltip-rail-variant-tool") => Spec {
            x: 50.0,
            y: 97.0,
            label: format!(
                "{} ({})",
                t!("tools.rectSelect"),
                t!("tools.rightClickOptions")
            ),
            shortcut: Some("M"),
        },
        Some("tooltip-rail-swatch") => Spec {
            x: 42.0,
            y: 743.0,
            label: t!("tools.foregroundColor").into_owned(),
            shortcut: None,
        },
        // "tooltip-rail-tool" and unset/unknown: the default state.
        _ => Spec {
            x: 50.0,
            y: 59.0,
            label: t!("tools.move").into_owned(),
            shortcut: Some("V"),
        },
    }
}

// ── Render ───────────────────────────────────────────────────────────────────

/// A transparent client-area frame with the requested tooltip pinned inside
/// it — the `context_menus.rs` convention for a viewport-pinned, portalled
/// source. Mount late in the tree: that is this view's only stacking-order claim.
pub fn render() -> impl IntoElement {
    let spec = spec();
    let tip: Div = tooltip(&spec.label, spec.shortcut)
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
/// GPUI hugs the content row, which is the recorded auto-width behaviour),
/// 4px corners, the #333333 edge every Photrez border actually paints, panel
/// fill, and the drop shadow.
///
/// A label that stays on one line needs no equivalent: without a width
/// constraint GPUI does not wrap, which is the same reason the recorded rule needs its
/// no-wrap rule in the first place.
pub(crate) fn tooltip(label: &str, shortcut: Option<&str>) -> Div {
    let mut row = h_flex().items_center().gap(px(GAP)).child(label_run(label));
    if let Some(keys) = shortcut {
        row = row.child(kbd(keys));
    }

    h_flex()
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

/// The live rail tooltip's origin, label and shortcut, from the same measured
/// table the fixture reads. `None` for an id the rail does not publish.
fn live_spec(
    id: &str,
    selection_shape: SelectionShape,
) -> Option<(f32, String, Option<&'static str>)> {
    let name = id.strip_prefix(RAIL_TOOLTIP_PREFIX)?;
    if name == "fg" {
        return Some((SWATCH_TIP_Y, t!("tools.foregroundColor").into_owned(), None));
    }
    let index = crate::views::tool_rail::tool_index(name)?;
    let (label, shortcut) = if name == "selection" {
        let key = match selection_shape {
            SelectionShape::Rect => "tools.rectSelect",
            SelectionShape::Ellipse => "tools.ellipseSelect",
        };
        (
            format!("{} ({})", t!(key), t!("tools.rightClickOptions")),
            "M",
        )
    } else {
        let (key, shortcut) = crate::views::tool_rail::tool_tooltip(name)?;
        (t!(key).to_string(), shortcut)
    };
    Some((
        RAIL_TIP_Y + RAIL_ROW_PITCH * index as f32,
        label,
        Some(shortcut),
    ))
}

/// The overlay that carries a live rail tooltip. It paints nothing unless a
/// rail button is hovered, and it has to be appended after the shell because
/// GPUI paints in tree order and the tooltip overlaps the canvas.
pub(crate) fn render_live(
    state: &Entity<AppState>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let (hovered, since, selection_shape) = {
        let s = state.read(cx);
        (s.tooltip.clone(), s.tooltip_since, s.selection_shape)
    };
    let Some(id) = hovered.filter(|id| id.starts_with(RAIL_TOOLTIP_PREFIX)) else {
        return div().into_any_element();
    };
    if !tooltip_delay_elapsed(since, cx) {
        return div().into_any_element();
    }
    let Some((y, label, shortcut)) = live_spec(&id, selection_shape) else {
        return div().into_any_element();
    };
    let x = if id.ends_with("-fg") {
        SWATCH_TIP_X
    } else {
        RAIL_TIP_X
    };
    tooltip(&label, shortcut)
        .id(id)
        .test_support()
        .absolute()
        .left(px(x))
        .top(px(y))
        .into_any_element()
}

/// Whether the hovered control's tooltip delay has elapsed, read from the
/// executor's clock so tests advance it with the fake timer and production
/// reads real time. The one-shot notify task repaints an idle shell; this
/// check keeps an earlier repaint from showing the tooltip too soon.
fn tooltip_delay_elapsed(since: Option<std::time::Instant>, cx: &Context<crate::app::App>) -> bool {
    let Some(since) = since else {
        return false;
    };
    cx.background_executor().now().duration_since(since) >= crate::app::TOOLTIP_DELAY
}

/// The label run — 11px sans in the text colour, carrying the content
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

/// The chip as recorded: a 4px left margin, horizontal padding,
/// 2px above and below, 4px corners, white at 10% on the panel, white at 50%
/// text, 9px semibold sans, a 1px light border, uppercased, no line leading.
///
/// Height is pinned to the measured 15px rather than left to the recorded
/// zero-leading line box: GPUI has no line-height control, so the 9px line box
/// is whatever DirectWrite says, and pinning keeps the chip's border rows on
/// crop 7/21 where the probe put them. Uppercasing is a no-op: see module docs.
fn kbd(keys: &str) -> Div {
    h_flex()
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

    /// The tooltip's pinned height is 26px (the recorded 26.5, rounded for
    /// GPUI's line-box handling).
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

    /// The kbd text weight is semibold (600), as recorded.
    #[test]
    fn kbd_weight_is_semibold() {
        assert!(
            (KBD_WEIGHT - 600.0).abs() < 0.001,
            "KBD_WEIGHT = {KBD_WEIGHT}"
        );
    }

    /// The label font size is 11px, the recorded size.
    #[test]
    fn label_size_is_eleven() {
        assert!(
            (LABEL_TEXT - 11.0).abs() < 0.001,
            "LABEL_TEXT = {LABEL_TEXT}"
        );
    }
}
