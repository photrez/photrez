//! Bottom-right toast stack: pinned 24px from the right and bottom, 8px gaps.
//! Width hugs the label; height is 36.75 (1 border + 8 padding + 18.75 line box + 8 padding + 1 border).
//! The host must be appended last in the tree; GPUI has no stacking index.
//! Severity is invisible in the chrome: every border resolves to BORDER (#333333).
//! No arrow or notch; the large shadow is painted. Fixed shot copy comes from `shot::active()`.
//! Timed dismissal and the live queue belong to `app::toasts`, not this view.
use gpui_kit::base::v_flex;
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, BoxShadow, Div, Hsla, TestSupportExt, div, px};

use crate::components::ui;
use crate::shot;
use crate::state::{AppState, ToastSeverity};
use crate::tokens::color;

// ── Geometry (rem at a 16px root) ──

/// 1.5rem in from the right. Measured as 1366 - 1342: every one of the four
/// baselines ends at x=1342.0 whatever its width.
pub const RIGHT_INSET: f32 = 24.0;

/// 1.5rem up from the bottom, the *other* 24. Measured as 820 - 796: both
/// stack bottoms land on 796, which is `shell-status-bar`'s top edge.
///
/// Kept separate from [`RIGHT_INSET`] because the two are coincidences of the
/// same number — one insets from the right, the other from the bottom, and if
/// the status bar ever grew, only this one would move.
pub const BOTTOM_INSET: f32 = 24.0;

/// One toast's border-box height: 1 + 8 + 18.75 + 8 + 1.
///
/// `manifest.json` reports 36.75 and its table displays 37; the fractional
/// value is the real one and comes from the 18.75px line box, not from
/// rounding anywhere.
// reason: pub geometry constant - referenced in module docs and unit tests, not
// yet consumed by another module. `allow` rather than `expect` because the test
// build does consume it, which would leave the expectation itself unfulfilled.
#[allow(dead_code)]
pub const TOAST_H: f32 = 36.75;

/// Gap between stacked toasts — see the module docs for why this is 8 and
/// not 4 (three border rows agree on 8).
pub const TOAST_GAP: f32 = 8.0;

/// 1rem of horizontal padding either side, confirmed at 17px from the box edge
/// to the text box (1px border + 16px) by the first ink column of all three
/// single shots.
const PAD_X: f32 = 16.0;

/// 0.5rem of vertical padding top and bottom.
const PAD_Y: f32 = 8.0;

/// The bare 0.25rem corner radius.
const RADIUS: f32 = 4.0;

/// 12.5px — the toast's own font size.
const TEXT_SIZE: f32 = 12.5;

/// Line box for the toast text: 12.5 x 1.5.
///
/// The 1.5 is the line height this text uses, the same way
/// `status_bar.rs` derives its own `LINE_H`. GPUI's default is not that
/// number, and the box height (36.75) hangs off this
/// number, so it has to be exact rather than stylistic.
const LINE_H: f32 = 18.75;

// The baseline's default sans family — Inter, the same family every other
// view uses.

/// Black at 10% — the alpha both large-shadow layers are painted at.
///
/// **Local, marked for consolidation**: this alpha comes from the recorded
/// pixels rather than an editor token, so `tokens::color` has no name for it yet and it is
/// used by exactly one component so far. If a second one asks for a shadow,
/// this belongs next to `BLACK_40`.
const SHADOW_10: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.0,
    a: 0.1,
};

// ── The shot labels ──
//
// Fixed copy the shot path paints, not copy invented for this file. In the
// live component they are whatever the operation hands to the toast queue, and
// this view has no engine to ask.

const MSG_INFO: &str = "Layer merged"; // i18n-exempt: not in catalog
const MSG_WARN: &str = "Could not write the file"; // i18n-exempt: not in catalog
const MSG_ERROR: &str = "Export failed"; // i18n-exempt: not in catalog

/// The stacked state's three, held in severity order.
const MSG_STACK_INFO: &str = "Opened"; // i18n-exempt: not in catalog
const MSG_STACK_WARN: &str = "Copied"; // i18n-exempt: not in catalog
const MSG_STACK_ERROR: &str = "Disk full"; // i18n-exempt: not in catalog

/// The three values of `ToastSeverity`, which decides the border
/// and an accessibility role. Only the border reaches pixels — see the module
/// docs.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Severity {
    Info,
    Warn,
    Error,
}

impl Severity {
    /// The border colour this severity asks for, and the one it gets.
    ///
    /// All three arms return [`color::BORDER`] because all three *measured*
    /// #333333: the global border colour outranks every per-severity border
    /// (see `tokens::BORDER` for that argument and
    /// the four-shot verification). The arms are kept separate rather than
    /// collapsed to one expression so that the colour each severity names, and
    /// therefore the colour to restore if that global is ever scoped, survives
    /// in the code instead of only in this comment.
    fn border(self) -> Hsla {
        match self {
            // The editor divider border, #2e2e2e by name -> measured #333333.
            Severity::Info => color::BORDER,
            // The yellow ramp's 500 step by name -> measured #333333.
            Severity::Warn => color::BORDER,
            // The editor accent border, #F28E42 by name -> measured #333333.
            Severity::Error => color::BORDER,
        }
    }
}

impl From<ToastSeverity> for Severity {
    fn from(severity: ToastSeverity) -> Self {
        match severity {
            ToastSeverity::Info => Severity::Info,
            ToastSeverity::Warn => Severity::Warn,
            ToastSeverity::Error => Severity::Error,
        }
    }
}

// ── Shot dispatch ───────────────────────────────────────────────────────────

/// Maps a shot id to the toast stack it should render.
///
/// Extracted from [`render`] so the dispatch table is unit-testable. Returns
/// `(severity, message)` pairs that [`render`] maps into [`toast`] elements.
fn messages_for(id: Option<&str>) -> Vec<(Severity, &'static str)> {
    match id {
        Some("toast-warn") => vec![(Severity::Warn, MSG_WARN)],
        Some("toast-error") => vec![(Severity::Error, MSG_ERROR)],
        Some("toast-stacked") => vec![
            (Severity::Info, MSG_STACK_INFO),
            (Severity::Warn, MSG_STACK_WARN),
            (Severity::Error, MSG_STACK_ERROR),
        ],
        // `toast-info` and any unset id — the default state.
        _ => vec![(Severity::Info, MSG_INFO)],
    }
}

// ── Render ───────────────────────────────────────────────────────────────────

/// The whole toast stack, in whichever state [`shot::active`] asks for.
///
/// Returned as a transparent `absolute` layer covering the window with the
/// stack in its bottom-right corner (padding = the 24px right and bottom
/// insets), because the recorded host sat out of the flow and pinned itself to
/// the window and GPUI has no equivalent of that. Two
/// consequences the caller should know, both in the module docs: the layer
/// only lands on the window's edges if its containing block is the
/// window-sized root, and it paints over the shell only if it is appended late
/// in that root's child list (there is no stacking order to lean on).
pub fn render() -> Div {
    let stack: Vec<AnyElement> = messages_for(shot::active())
        .into_iter()
        .map(|(sev, msg)| render_toast(sev, msg).into_any_element())
        .collect();
    host(stack)
}

/// The production host: the live queue from [`AppState::toasts`], rendered in
/// the same bottom-right stack as the shot above.
///
/// Rows carry their queue id as the element id (and `test_support`) so mounted
/// tests can address one toast; the recorded status/alert roles have no
/// GPUI surface yet and are not faked.
pub fn render_live(state: &gpui_kit::Entity<AppState>, cx: &Context<crate::app::App>) -> Div {
    let toasts = state.read(cx).toasts.clone();
    let stack: Vec<AnyElement> = toasts
        .into_iter()
        .map(|toast| {
            render_toast(toast.severity.into(), &toast.message)
                .id(format!("toast-{}", toast.id))
                .test_support()
                .into_any_element()
        })
        .collect();
    host(stack)
}

/// The shot host and the live host differ only in their rows.
fn host(stack: Vec<AnyElement>) -> Div {
    // The host pinned to the window's bottom-right 24px insets: a layer over
    // the window, with the stack pushed into its bottom-right corner by flex
    // alignment plus the two insets. `.inset(px(0.))` is the fill pattern
    // `ui::slider` already uses for an absolutely positioned box; alignment is
    // used rather than
    // `.right()/.bottom()` offsets so the stack hugs its own width instead of
    // relying on shrink-to-fit resolving against the right edge.
    div()
        .absolute()
        .inset(px(0.))
        .flex()
        .justify_end()
        .items_end()
        .pr(px(RIGHT_INSET))
        .pb(px(BOTTOM_INSET))
        // The column stack, 8px between rows, stretched across its container:
        // gpui_kit's `v_flex()`
        // defaults to `items_center()`, which would left-align each toast at
        // its own width and break the stack shot, where all three share the
        // container's 78.281px (borders at crop x=1 and x=78 for every one).
        .child(v_flex().items_stretch().gap(px(TOAST_GAP)).children(stack))
}

/// One toast: a 4px corner radius, a 1px border, 16px of horizontal and 8px of
/// vertical padding, 12.5px text and the large shadow, over the panel fill in
/// the default text colour.
///
/// Width and height are hugged — no `.w()` and no `.h()` — because both are
/// content functions of the label: 109.609/160.766/103.062 wide for the three
/// singles, and the stack's own width is its widest child's. The bottom-right
/// position comes from the parent, so a width that lands a fraction off moves
/// the *left* border only.
fn render_toast(severity: Severity, message: &str) -> Div {
    div()
        // A toast is a single line in all four baselines;
        // in the stack the widest child is laid out at exactly its own
        // max-content width, where a fractional rounding could tip it into a
        // second line. See the module docs.
        .whitespace_nowrap()
        .rounded(px(RADIUS))
        .border_1()
        .border_color(severity.border())
        .bg(color::PANEL)
        .px(px(PAD_X))
        .py(px(PAD_Y))
        .font_family(ui::FONT_UI)
        .text_size(px(TEXT_SIZE))
        .line_height(px(LINE_H))
        .text_color(color::TEXT)
        // The large shadow — the only part of this component that is visible
        // *outside* its own box in a baseline: the stacked shot's 8px gaps are
        // inside the captured element and show the shadow falling on the dock.
        .shadow(shadow_lg())
        .child(message.to_string())
}

/// The theme's large-shadow preset, spelled out as GPUI layers:
///
/// ```text
/// 0 10px 15px -3px rgb(0 0 0 / 0.1),  0 4px 6px -4px rgb(0 0 0 / 0.1)
/// ```
///
/// Same approach as `tool_rail::shadow_md_layers` for the medium shadow: the
/// recorded blur radii passed through unchanged, so the two spellings cannot drift from the
/// theme or from each other. Measured against the baseline at gap 2, where a
/// `#333333` rule of the dock under a toast reads `#2E2E2E` = exactly 10%
/// black over it.
fn shadow_lg() -> Vec<BoxShadow> {
    vec![
        BoxShadow::new(px(0.), px(10.), SHADOW_10)
            .blur_radius(px(15.))
            .spread_radius(px(-3.)),
        BoxShadow::new(px(0.), px(4.), SHADOW_10)
            .blur_radius(px(6.))
            .spread_radius(px(-4.)),
    ]
}

#[cfg(test)]
mod tests;
