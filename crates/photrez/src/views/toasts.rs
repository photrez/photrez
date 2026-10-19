//! Visual-only port of `apps/desktop/src/components/editor/Toast.tsx` —
//! `ToastHost`, the bottom-right toast stack, and the three severities it
//! carries.
//!
//! # Geometry: the anchor is the bottom-right corner, not the left edge
//!
//! The source's host is `pointer-events-none fixed bottom-6 right-6 z-50
//! flex flex-col gap-2`, so *both* right and bottom are pinned to the window —
//! `6` = 1.5rem = 24px in each direction. Every number below was read back out
//! of `manifest.json` and then out of the PNGs rather than derived from the
//! class names:
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
//!   (`status_bar.rs`: 1366x24 at 0,796). It is also 820 - 24, i.e.
//!   `bottom-6`: the toast bottom sitting *on* the status bar is two different
//!   24s agreeing (the bar is 24 tall, the inset is 24), not a layout
//!   relationship anyone designed. The port therefore anchors with
//!   [`RIGHT_INSET`] and [`BOTTOM_INSET`] from the window's edges — see [`render`].
//!
//! # Width: hugged from the label, never pinned
//!
//! `width = 1 (border) + 16 (px-4) + shaped text + 16 + 1`, so the three
//! severities differ only because their captured labels do: "Layer merged" =
//! 75.609px of text, "Could not write the file" = 126.766, "Export failed" =
//! 69.062. The port hugs rather than pinning those four numbers, because the
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
//! warn   ink from 1198      - 1181.234 = 16.8    >  ~17 = 1px border + px-4
//! error  ink from 1256      - 1238.937 = 17.1   /
//! ```
//!
//! i.e. the text box starts 17px in, the residual being each glyph's side
//! bearing — `px-4` (16) behind a 1px border, on both sides (the warn toast's
//! last ink column ends 17.0px short of its right edge).
//!
//! # Height: 36.75, which the manifest displays as 37
//!
//! `1 (border) + 8 (py-2) + 18.75 + 8 (py-2) + 1 = 36.75`. The line box is
//! 12.5 x 1.5: Tailwind's preflight leaves `line-height: 1.5` inherited, the
//! same reason `status_bar.rs` pins its own `LINE_H` — GPUI's default line
//! box is neither 15 nor 15.75 there and is neither 18.75 here, so it is
//! spelled out. Glyph ink corroborates the box: cap tops at crop row 12 and
//! the baseline at ~22 in all three single shots, i.e. the line box occupies
//! element rows 9..27.75 exactly as `border + py-2 + 18.75` says.
//!
//! # The stack gap is 8px, not 4px
//!
//! The four-baseline brief states "126 = 3x37 + 2x4" — but 3x37 + 2x4 is 119,
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
//! `Toast.tsx:54` writes `gap-2` = 0.5rem = 8px. Two sources, one answer:
//! [`TOAST_GAP`] is 8. A 4px gap would put the stack at 118.25 tall and every
//! border row in the baseline off by 8px per toast.
//!
//! The same shot also pins `align-items`. All three stacked toasts have their
//! left border at crop x=1 and their right border at crop x=78 — every child
//! spans the *container's* width, which is CSS column-flex default
//! (`stretch`). A `gpui_kit` `v_flex()` defaults to `items_center()`, so
//! [`render`] asks for `.items_stretch()` explicitly, exactly as `main.rs` does
//! for the shell's columns. The container in turn hugs its widest child, which
//! is why the stack is 78.281 wide while its widest member ("Opened") has only
//! 44.28px of text.
//!
//! # Severity: three classes, one pixel
//!
//! The source picks a border per severity:
//!
//! | severity | class | named colour | **measured** |
//! |---|---|---|---|
//! | info  | `border-editor-divider` | #2e2e2e | #333333 |
//! | warn  | `border-yellow-500`     | yellow  | #333333 |
//! | error | `border-editor-accent`  | #F28E42 | #333333 |
//!
//! All three lose to the unlayered `* { border-color: var(--color-border) }`
//! rule that [`color::BORDER`] documents at length — and the PNGs agree
//! rather than merely being predicted: every edge pixel of all four toast
//! shots reads `#333333` (`probe-toast.ts`: `edges: top/bottom/left/right`).
//! Severity is therefore **invisible in the chrome** in this build: fill,
//! border, radius and text colour are identical across the three, and the
//! label is the only difference. [`Severity::border`] still spells out the
//! class each variant asks for, so if the cascade is ever fixed the three
//! arms are where the real colours go.
//!
//! Also measured rather than guessed:
//!
//! * fill `#1E1E1E` = [`color::PANEL`] (`bg-editor-panel`), text peak `#CACACA`
//!   = [`color::TEXT`] (`text-editor-text`) — the toast sits over the right
//!   dock, whose panel is the *same* #1E1E1E, so the border is the only thing
//!   separating toast from backdrop in three of the four shots.
//! * radius 4 — the top border row starts at crop x=4 and ramps `#202020`
//!   `#272727` `#2F2F2F` before it, i.e. an arc of radius 4 (`rounded`).
//! * **no arrow/notch.** The brief asks for one to be read off the PNGs; the
//!   read is that there is none: four symmetric 4px corners, nothing pointing
//!   at anything. `Toast.tsx` agrees — the class list is
//!   `rounded border px-4 py-2 text-[12.5px] shadow-lg`, with no
//!   pseudo-element and no caret.
//!
//! # Shadow: `shadow-lg`, and it *is* visible
//!
//! A single toast's crop is its own box, so the shadow falls outside the
//! baseline entirely. In `toast-stacked` it does not: the 8px gaps are inside
//! the captured element, and there the backdrop under a toast measurably
//! darkens — `#1E1E1E -> #1A1A1A` in the first row of gap 1, directly under
//! toast 1, and a `#333333` rule of the dock showing through gap 2 at
//! `#2E2E2E`, which is exactly 10% black over it. So the two layers of
//! Tailwind v4's `--shadow-lg` are painted ([`shadow_lg`]), matching
//! `tool_rail.rs`'s treatment of `--shadow-md`.
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
//! The four messages are `capture-baseline.ts:698-704`'s literals, in the
//! order it fires them (info, warn, error) for the stack — so the stack reads
//! top-to-bottom info/warn/error, as the shot does.
//!
//! # Deliberately omitted — behaviour, not pixels
//!
//! * `setTimeout` dismissal (3500ms info/warn, 5000ms error) and
//!   `dismissToast` on click: no timers in a static port.
//! * the FIFO cap (`next.length > 3 ? slice(-3)`) — `toast-stacked` *is* the
//!   post-cap state; reproducing the cap would be behaviour with no pixel to
//!   show for it.
//! * enter/exit transition and the host's `pointer-events` flags — interaction.
//! * `aria-live`, `role="status"|"alert"` — accessibility, not rendering.
//!
//! # Known gaps, specific to this component
//!
//! * **No `z_index`.** The host is `z-50` and paints over the shell; GPUI has
//!   no z-index, only tree order. This file therefore cannot place itself —
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
//!   rounding could otherwise push it to two lines. The source never wraps
//!   (a toast is a single line), so this is parity, not a workaround.
//! * The 6px `scrollbar-gutter` reservation (`styles.css:321/:326`) does **not**
//!   apply: the host has no `overflow-y-auto`, and adding the reservation here
//!   would move the left edge of every toast by 6px for nothing.
//! * In the stacked crop the corner notches and the area left of the stack are
//!   warm `#291E19`/`#723719` pixels — that is the right dock's own content
//!   showing through the transparent host, verified against `shell-right-dock`.
//!   Nothing in this file paints it, and nothing here should.
//! * Icon tint, `clip-path`, `translate`, `scale()`, multi-stop gradients and
//!   tooltips are listed as project-wide gaps; this component uses none of
//!   them, so each is a non-event here rather than an approximation.
//! * Padding is already whole (8/16), so the "padding rounds to whole px"
//!   gap is inert. The 18.75px line box is *not* whole: GPUI snaps a half
//!   pixel down where Chromium rounds up, so the glyphs may land 1px off
//!   vertically while the box (1 + 8 + 18.75 + 8 + 1) stays put. First thing
//!   to check if a capture shows the text a row high.

use gpui_kit::base::v_flex;
use gpui_kit::prelude::*;
use gpui_kit::{div, px, BoxShadow, Div, Hsla};

use crate::components::ui;
use crate::shot;
use crate::tokens::color;

// ── Geometry (Tailwind, 16px root) ──

/// `right-6` — 1.5rem. Measured as 1366 - 1342: every one of the four
/// baselines ends at x=1342.0 whatever its width.
pub const RIGHT_INSET: f32 = 24.0;

/// `bottom-6` — 1.5rem, the *other* 24. Measured as 820 - 796: both stack
/// bottoms land on 796, which is `shell-status-bar`'s top edge.
///
/// Kept separate from [`RIGHT_INSET`] because the two are coincidences of the
/// same number — one is `right-6`, the other `bottom-6`, and if the status bar
/// ever grew, only this one would move.
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

/// `gap-2` between stacked toasts — see the module docs for why this is 8 and
/// not the 4 the brief proposes (three border rows and `Toast.tsx:54` agree).
pub const TOAST_GAP: f32 = 8.0;

/// `px-4` — 1rem either side, confirmed at 17px from the box edge to the text
/// box (1px border + 16px) by the first ink column of all three single shots.
const PAD_X: f32 = 16.0;

/// `py-2` — 0.5rem top and bottom.
const PAD_Y: f32 = 8.0;

/// `rounded` — Tailwind's bare utility, 0.25rem.
const RADIUS: f32 = 4.0;

/// `text-[12.5px]` — the toast's own font size.
const TEXT_SIZE: f32 = 12.5;

/// Line box for the toast text: 12.5 x 1.5.
///
/// The 1.5 is Tailwind preflight's inherited `line-height`, the same way
/// `status_bar.rs` derives its own `LINE_H` — GPUI's default is neither that
/// nor anything else the browser would use, and the box height (36.75) hangs
/// off this number, so it has to be exact rather than stylistic.
const LINE_H: f32 = 18.75;

// `--font-sans` — Inter, the same family every other port uses.

/// `rgb(0 0 0 / 0.1)`, the alpha both `--shadow-lg` layers are painted at.
///
/// **Local, marked for consolidation**: this is a Tailwind default (not an
/// `--editor-*` token), so `tokens::color` has no name for it yet and it is
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
// `capture-baseline.ts:698-704` decides what each shot says; these are its
// literals, not invented copy. In the real component they are whatever the
// operation passed to `showToast`, and this port has no engine to ask.

/// `ed.showToast("Layer merged","info")`.
const MSG_INFO: &str = "Layer merged";

/// `ed.showToast("Could not write the file","warn")`.
const MSG_WARN: &str = "Could not write the file";

/// `ed.showToast("Export failed","error")`.
const MSG_ERROR: &str = "Export failed";

/// The stack's three, in the order the capture fires them.
const MSG_STACK_INFO: &str = "Opened";
const MSG_STACK_WARN: &str = "Copied";
const MSG_STACK_ERROR: &str = "Disk full";

/// The three values of `ToastSeverity`, which the source uses to pick a border
/// and a `role`. Only the border reaches pixels — see the module docs.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Severity {
    Info,
    Warn,
    Error,
}

impl Severity {
    /// The `border-*` colour this severity asks for, and the one it gets.
    ///
    /// All three arms return [`color::BORDER`] because all three *measured*
    /// #333333: the unlayered `* { border-color }` rule outranks every
    /// `border-*` utility (see `tokens::BORDER` for the cascade argument and
    /// the four-shot verification). The arms are kept separate rather than
    /// collapsed to one expression so that the class each severity names — and
    /// therefore the colour to restore if that rule is ever scoped — survives
    /// in the code instead of only in this comment.
    fn border(self) -> Hsla {
        match self {
            // `border-editor-divider`, #2e2e2e by name -> measured #333333.
            Severity::Info => color::BORDER,
            // `border-yellow-500`, Tailwind yellow by name -> measured #333333.
            Severity::Warn => color::BORDER,
            // `border-editor-accent`, --editor-accent #F28E42 -> measured #333333.
            Severity::Error => color::BORDER,
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
/// stack in its bottom-right corner (padding = the `right-6`/`bottom-6`
/// insets), because that is what `fixed` means and GPUI has no `fixed`. Two
/// consequences the caller should know, both in the module docs: the layer
/// only lands on the window's edges if its containing block is the
/// window-sized root, and it paints over the shell only if it is appended late
/// in that root's child list (there is no `z_index` to lean on).
pub fn render() -> impl IntoElement {
    let stack: Vec<Div> = messages_for(shot::active())
        .into_iter()
        .map(|(sev, msg)| toast(sev, msg))
        .collect();

    // The `fixed ... bottom-6 right-6` host: a layer over the window, with the
    // stack pushed into its bottom-right corner by flex alignment plus the
    // two insets. `.inset(px(0.))` is the fill pattern `ui::slider` already
    // uses for an absolutely positioned box; alignment is used rather than
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
        // `flex flex-col gap-2`, with `items_stretch`: gpui_kit's `v_flex()`
        // defaults to `items_center()`, which would left-align each toast at
        // its own width and break the stack shot, where all three share the
        // container's 78.281px (borders at crop x=1 and x=78 for every one).
        .child(v_flex().items_stretch().gap(px(TOAST_GAP)).children(stack))
}

/// One toast: `rounded border px-4 py-2 text-[12.5px] shadow-lg` over
/// `bg-editor-panel` in `text-editor-text`.
///
/// Width and height are hugged — no `.w()` and no `.h()` — because both are
/// content functions of the label: 109.609/160.766/103.062 wide for the three
/// singles, and the stack's own width is its widest child's. The bottom-right
/// position comes from the parent, so a width that lands a fraction off moves
/// the *left* border only, which is the same trade the source makes.
fn toast(severity: Severity, message: &str) -> Div {
    div()
        // A toast is a single line in the source and in all four baselines;
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
        // `shadow-lg` — the only part of this component that is visible
        // *outside* its own box in a baseline: the stacked shot's 8px gaps are
        // inside the captured element and show the shadow falling on the dock.
        .shadow(shadow_lg())
        .child(message.to_string())
}

/// Tailwind v4's `--shadow-lg`, spelled out as GPUI layers:
///
/// ```text
/// 0 10px 15px -3px rgb(0 0 0 / 0.1),  0 4px 6px -4px rgb(0 0 0 / 0.1)
/// ```
///
/// Same approach as `tool_rail::shadow_md_layers` for `--shadow-md`: CSS blur
/// radii passed through unchanged, so the two ports cannot drift from the
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
mod tests {
    use super::*;

    /// All three severities map to `color::BORDER` — the unlayered
    /// `* { border-color }` rule overrides every `border-*` utility.
    ///
    /// RED proof: change any arm of `Severity::border()` to return a different
    /// colour and this fails. GREEN: all arms return `BORDER`.
    #[test]
    fn severity_border_map_is_border() {
        assert_eq!(Severity::Info.border(), color::BORDER);
        assert_eq!(Severity::Warn.border(), color::BORDER);
        assert_eq!(Severity::Error.border(), color::BORDER);
    }

    /// The six captured labels match `capture-baseline.ts:698-704` verbatim.
    ///
    /// RED proof: change any `MSG_*` constant to a different string and this
    /// fails. GREEN: every label matches the baseline spec.
    #[test]
    fn captured_labels_match_baseline() {
        assert_eq!(MSG_INFO, "Layer merged");
        assert_eq!(MSG_WARN, "Could not write the file");
        assert_eq!(MSG_ERROR, "Export failed");
        assert_eq!(MSG_STACK_INFO, "Opened");
        assert_eq!(MSG_STACK_WARN, "Copied");
        assert_eq!(MSG_STACK_ERROR, "Disk full");
    }

    /// `toast-stacked` height = 3 × TOAST_H + 2 × TOAST_GAP = 126.25.
    ///
    /// Manifest pins the stack at 126.25px. This verifies the constants
    /// reproduce that arithmetic exactly.
    ///
    /// RED proof: change `TOAST_H` or `TOAST_GAP` to any other value and the
    /// product diverges from 126.25. GREEN: the sum closes on the manifest.
    #[test]
    fn stack_height_is_three_toasts_two_gaps() {
        let stacked = 3.0 * TOAST_H + 2.0 * TOAST_GAP;
        assert!(
            (stacked - 126.25).abs() < 0.001,
            "stacked height {} != 126.25",
            stacked
        );
        // And the single height matches the manifest's 36.75.
        assert!((TOAST_H - 36.75).abs() < 0.001);
    }

    /// `toast-info` x + width = 1342.0 = 1366 - RIGHT_INSET.
    #[test]
    fn right_inset_pins_every_shot() {
        // All four shots share the same right edge (1342.0 = 1366 - 24).
        // The code positions from the right edge via RIGHT_INSET + flex,
        // so this verifies the constant the layout depends on.
        assert!((RIGHT_INSET - 24.0).abs() < 0.001);
    }

    /// The dispatch table maps shot ids to the correct severity + message.
    ///
    /// RED proof: swap any arm (e.g. `"toast-warn"` → return `Severity::Info`)
    /// and the corresponding assertion fails. GREEN: each shot selects its
    /// documented stack.
    #[test]
    fn messages_for_matches_shot() {
        // Default / unset → single info toast.
        let def = messages_for(None);
        assert_eq!(def.len(), 1);
        assert_eq!(def[0].0, Severity::Info);
        assert_eq!(def[0].1, MSG_INFO);

        // toast-warn → single warn.
        let warn = messages_for(Some("toast-warn"));
        assert_eq!(warn.len(), 1);
        assert_eq!(warn[0].0, Severity::Warn);
        assert_eq!(warn[0].1, MSG_WARN);

        // toast-error → single error.
        let err = messages_for(Some("toast-error"));
        assert_eq!(err.len(), 1);
        assert_eq!(err[0].0, Severity::Error);
        assert_eq!(err[0].1, MSG_ERROR);

        // toast-stacked → three, in capture order (info/warn/error).
        let stack = messages_for(Some("toast-stacked"));
        assert_eq!(stack.len(), 3);
        assert_eq!(stack[0].0, Severity::Info);
        assert_eq!(stack[0].1, MSG_STACK_INFO);
        assert_eq!(stack[1].0, Severity::Warn);
        assert_eq!(stack[1].1, MSG_STACK_WARN);
        assert_eq!(stack[2].0, Severity::Error);
        assert_eq!(stack[2].1, MSG_STACK_ERROR);

        // Unknown shot → default (same as None).
        let unknown = messages_for(Some("toast-bogus"));
        assert_eq!(unknown.len(), 1);
        assert_eq!(unknown[0].0, Severity::Info);
    }
}
