//! The bottom-right toast stack — its host, placement, and the three
//! severities a toast carries.
//!
//! # Geometry: the anchor is the bottom-right corner, not the left edge
//!
//! The recorded toast host sits out of the flow, pinned to the window with
//! *both* right and bottom at 1.5rem = 24px, its toasts stacked in a column with
//! 8px between them, painted above the shell and letting the mouse pass through
//! to whatever is underneath. Every number below was read back out of
//! `manifest.json` and then out of the PNGs rather than derived from the
//! layout code:
//!
//! ```text
//! toast-info     109.609x36.75  @ 1232.390,759.25
//! toast-warn     160.766x36.75  @ 1181.234,759.25
//! toast-error    103.062x36.75  @ 1238.937,759.25
//! toast-stacked   78.281x126.25  @ 1263.718,669.75
//! ```
//!
//! * **Right inset = 24.** 1232.390+109.609, 1181.234+160.766,
//!   1238.937+103.062 and 1263.718+78.281 all land on **1342.0** = 1366 - 24.
//!   Four different widths, one right edge: the right side is what is fixed,
//!   and the left edge is wherever the label's advance puts it. Placing the
//!   toast from a left margin would match exactly one of the four shots.
//! * **Bottom = 796.** 759.25 + 36.75 = 796, and 669.75 + 126.25 = 796 — both
//!   stacks end on the same row, which is `shell-status-bar`'s top edge
//!   (`status_bar.rs`: 1366x24 at 0,796). It is also 820 - 24, i.e. the
//!   bottom inset again: the toast bottom sitting *on* the status bar is two different
//!   24s agreeing (the bar is 24 tall, the inset is 24), not a layout
//!   relationship anyone designed. This view therefore anchors with
//!   [`RIGHT_INSET`] and [`BOTTOM_INSET`] from the window's edges — see [`render`].
//!
//! # Width: hugged from the label, never pinned
//!
//! `width = 1 (border) + 16 (padding) + shaped text + 16 + 1`, so the three
//! severities differ only because their captured labels do: "Layer merged" =
//! 75.609px of text, "Could not write the file" = 126.766, "Export failed" =
//! 69.062. This view hugs rather than pinning those four numbers, because the
//! same Inter file at 12.5px is what the baseline was shaped with —
//! `title_bar` measures this pair's advances to 0.005% — and a pinned width
//! would hide a shaping difference behind a correct-looking border.
//!
//! The padding is measured too, not assumed. Taking the first ink column of
//! each single-shot crop (the crops start at `floor(box.x)`, so crop x + that
//! origin is viewport x):
//!
//! ```text
//! info   ink from 1250      - 1232.390 = 17.6   \
//! warn   ink from 1198      - 1181.234 = 16.8    >  ~17 = 1px border + 16px
//! error  ink from 1256      - 1238.937 = 17.1   /
//! ```
//!
//! i.e. the text box starts 17px in, the residual being each glyph's side
//! bearing — 16px of padding behind a 1px border, on both sides (the warn
//! toast's last ink column ends 17.0px short of its right edge).
//!
//! # Height: 36.75, which the manifest displays as 37
//!
//! `1 (border) + 8 (vertical padding) + 18.75 + 8 (vertical padding) + 1 =
//! 36.75`. The line box is 12.5 x 1.5. `status_bar.rs` pins its own `LINE_H`
//! for the same reason: GPUI's default line box is neither 15 nor 15.75 there
//! and is neither 18.75 here, so this number is spelled out.
//! Glyph ink corroborates the box: cap tops at crop row 12 and
//! the baseline at ~22 in all three single shots, i.e. the line box occupies
//! element rows 9..27.75 exactly as border + padding + line box says.
//!
//! # The stack gap is 8px, not 4px
//!
//! A first reading of the four baselines gave "126 = 3x37 + 2x4" — but
//! 3x37 + 2x4 is 119,
//! not 126, and the fourth baseline does not fit a 4px gap under any rounding.
//! Measured instead, from `toast-stacked.png`'s border rows (the rows whose
//! pixels are the `#333333` rule, crop coordinates):
//!
//! ```text
//! toast 1   top border y=1    bottom border y=37     -> viewport box [670,707)
//!   8 rows of backdrop: y=38..45
//! toast 2   top border y=46   bottom border y=81     -> viewport box [715,751)
//!   8 rows of backdrop: y=82..89
//! toast 3   top border y=90   bottom border y=126    -> viewport box [759,796)
//! ```
//!
//! and arithmetic that closes exactly: 3 x 36.75 + 2 x 8 = **126.25** = the
//! manifest's stack height, with the last toast's bottom on 796. Independently,
//! the host's own gap is 0.5rem = 8px. Two sources, one answer:
//! [`TOAST_GAP`] is 8. A 4px gap would put the stack at 118.25 tall and every
//! border row in the baseline off by 8px per toast.
//!
//! The same shot also pins how the stack cross-aligns its children. All three
//! stacked toasts have their left border at crop x=1 and their right border at
//! crop x=78 — every child spans the *container's* width, which is the default
//! for a stacked column: each child stretches across it. A `gpui_kit`
//! `v_flex()` defaults to `items_center()`, so
//! [`render`] asks for `.items_stretch()` explicitly, exactly as `main.rs` does
//! for the shell's columns. The container in turn hugs its widest child, which
//! is why the stack is 78.281 wide while its widest member ("Opened") has only
//! 44.28px of text.
//!
//! # Severity: three border colours, one pixel
//!
//! Each severity asks for its own border:
//!
//! | severity | border it asks for | named colour | **measured** |
//! |---|---|---|---|
//! | info  | the editor divider | #2e2e2e | #333333 |
//! | warn  | the yellow ramp's 500 step     | yellow  | #333333 |
//! | error | the editor accent  | #F28E42 | #333333 |
//!
//! All three lose to the global border colour that every bordered surface
//! resolves to, which [`color::BORDER`] documents at length. The captures agree
//! rather than merely being predicted: every edge pixel of all four toast
//! shots reads `#333333`; the scan covered `edges: top/bottom/left/right`.
//! Severity is therefore **invisible in the chrome** in this build: fill,
//! border, radius and text colour are identical across the three, and the
//! label is the only difference. `Severity::border` still spells out the
//! colour each variant asks for, so if that rule is ever scoped the three
//! arms are where the real colours go.
//!
//! Also measured rather than guessed:
//!
//! * fill `#1E1E1E` = [`color::PANEL`], the panel fill, and text peak
//!   `#CACACA` = [`color::TEXT`], the default text colour — the toast sits
//!   over the right
//!   dock, whose panel is the *same* #1E1E1E, so the border is the only thing
//!   separating toast from backdrop in three of the four shots.
//! * radius 4 — the top border row starts at crop x=4 and ramps `#202020`
//!   `#272727` `#2F2F2F` before it, i.e. an arc of radius 4.
//! * **no arrow/notch.** Reading the corners straight off the PNGs shows
//!   none: four symmetric 4px corners, nothing pointing
//!   at anything. The measured toast box agrees — a rounded border, padding,
//!   12.5px text and a shadow, with no extra painted decoration and no caret.
//!
//! # Shadow: the large one, and it *is* visible
//!
//! A single toast's crop is its own box, so the shadow falls outside the
//! baseline entirely. In `toast-stacked` it does not: the 8px gaps are inside
//! the captured element, and there the backdrop under a toast measurably
//! darkens — `#1E1E1E -> #1A1A1A` in the first row of gap 1, directly under
//! toast 1, and a `#333333` rule of the dock showing through gap 2 at
//! `#2E2E2E`, which is exactly 10% black over it. So both layers of the web
//! baseline's large shadow are painted (`shadow_lg`), matching
//! `tool_rail.rs`'s treatment of the medium one.
//!
//! # State: `shot::active()`, nothing else
//!
//! ```text
//! PHOTREZ_SHOT=toast-info    "Layer merged"
//! PHOTREZ_SHOT=toast-warn    "Could not write the file"
//! PHOTREZ_SHOT=toast-error   "Export failed"
//! PHOTREZ_SHOT=toast-stacked "Opened" / "Copied" / "Disk full"
//! unset / anything else      toast-info
//! ```
//!
//! The four messages are the capture tooling's own literals, in the
//! order it fires them (info, warn, error) for the stack — so the stack reads
//! top-to-bottom info/warn/error, as the shot does.
//!
//! # Deliberately omitted — behaviour, not pixels
//!
//! The shot path ([`render`]) paints a fixed stack for a capture and owns no
//! timers, so these belong to the live path instead:
//!
//! * Timed dismissal (3500ms info/warn, 5000ms error) and dismissing a toast
//!   on click: [`render_live`] renders the queue that
//!   `app::toasts` owns, and that queue carries the two deadlines.
//! * the FIFO cap of three toasts — `toast-stacked` *is* the
//!   post-cap state; `app::toasts` applies the same cap to the live queue.
//! * enter/exit transition and the host letting the mouse through — interaction.
//! * the live-region announcement and the status/alert role — accessibility,
//!   not rendering.
//!
//! # Known gaps, specific to this component
//!
//! * **No stacking order.** The host paints above the shell (its layer was
//!   numbered 50); GPUI has
//!   no stacking index, only tree order. This file therefore cannot place itself —
//!   its caller must append [`render`]'s result **late** in the tree (after
//!   `status_bar::render()`, as the last child of the window-sized root), and
//!   [`render`] helps by returning an `absolute().inset(0.)` layer rather than
//!   an in-flow box, so it covers the window without disturbing layout. Appended
//!   early, the dock paints over it and three of the four baselines break.
//! * The bottom-right anchoring assumes that layer's containing block is the
//!   1366x820 window — true when it hangs off `main.rs`'s `size_full()` root,
//!   which is the only window-sized box in the tree.
//! * `whitespace_nowrap()` guards the one wrap hazard: in the stack the widest
//!   child is laid out at exactly its own max-content width, so a fractional
//!   rounding could otherwise push it to two lines. A toast is a single line,
//!   so the guard names the intended behaviour, not a workaround.
//! * No 6px scrollbar gutter is reserved: this host never scrolls, and adding
//!   one would move the left edge of every toast by 6px for nothing.
//! * In the stacked crop the corner notches and the area left of the stack are
//!   warm `#291E19`/`#723719` pixels — that is the right dock's own content
//!   showing through the transparent host, verified against `shell-right-dock`.
//!   Nothing in this file paints it, and nothing here should.
//! * Icon tint, clipping paths, positional transforms, scaling, multi-stop
//!   gradients and
//!   tooltips are listed as project-wide gaps; this component uses none of
//!   them, so each is a non-event here rather than an approximation.
//! * Padding is already whole (8/16), so the "padding rounds to whole px"
//!   gap is inert. The 18.75px line box is *not* whole: GPUI snaps a half
//!   pixel down where whole-pixel rounding would go up, so the glyphs may land 1px off
//!   vertically while the box (1 + 8 + 18.75 + 8 + 1) stays put. First
//!   thing to check if a capture shows the text a row high.

use gpui_kit::base::v_flex;
use gpui_kit::prelude::*;
use gpui_kit::{div, px, AnyElement, BoxShadow, Div, Hsla, TestSupportExt};

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

// ── The captured labels ──
//
// The capture tooling decides what each shot says; these are its
// literals, not invented copy. In the live component they are whatever the
// operation hands to the toast queue, and this view has no engine to ask.

/// The capture fires "Layer merged" at severity `info`.
const MSG_INFO: &str = "Layer merged"; // i18n-exempt: not in catalog

/// The capture fires "Could not write the file" at severity `warn`.
const MSG_WARN: &str = "Could not write the file"; // i18n-exempt: not in catalog

/// The capture fires "Export failed" at severity `error`.
const MSG_ERROR: &str = "Export failed"; // i18n-exempt: not in catalog

/// The stack's three, in the order the capture fires them.
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
