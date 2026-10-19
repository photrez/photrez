//! Native port of `apps/desktop/src/components/editor/shell/BottomStatusBar.tsx`.
//!
//! Geometry: `manifest.json` gives `shell-status-bar` as 1366x24 at (0,796) —
//! the full client width, below `<main>`, and the last 24px that take the
//! window from 796 to the baseline's 820.
//!
//! # Scope: the resting state the baseline captures
//!
//! The real footer is a live readout of engine state — document size, zoom,
//! active tool, the tool hint, the selected layer — plus five conditional
//! indicators that only mount while something is happening:
//!
//! * the tiling-progress hairline (`statusLoadingMessage`),
//! * the heavy-op spinner and its cancel button (`statusLoadingMessage`),
//! * the dev-mode frame timings, which mount whenever `frames > 0`,
//! * the autosave dot (`autosaveStatus() !== "idle"`),
//! * the manual-save progress row (`saveProgress().phase !== "idle"`).
//!
//! Of those five, the dev-mode Frame cell IS captured in the resting state:
//! `BottomStatusBar.tsx:170` gates it only on `devFrameStats() &&
//! devFrameStats()!.frames > 0`, and the dev-mode capture has a live scheduler
//! ticking, so `frames` is always > 0 and the cell is always present in the
//! golden master. `baseline/README.md` records run-to-run variance inside the
//! metrics segment (x 764-912, y 801-815 in viewport coords, affecting both
//! `shell-status-bar` and `dock-closed`) — the comparison harness masks that
//! segment rather than treating its numeric drift as a geometry failure.
//!
//! The other four indicators are genuinely absent from the PNG: their backing
//! signals are idle in the captured state, so a cell for them would be a pixel
//! that does not exist in the golden master.
//!
//! The six cells visible in the PNG are rendered as the literals it shows; each
//! `const` below names the state it stands in for, because this port has no
//! engine to read from.
//!
//! Labels are the `en` locale strings (`locales/en/translation.json`,
//! `status.*`), the locale the baseline was captured in.
//!
//! # Transcription notes
//!
//! * Every `border-*` paints [`color::BORDER`] (#333333) — the `border-t` and
//!   the four `border-l` rules included, `border-editor-divider` naming #2e2e2e
//!   notwithstanding. `probe-borders.ts` sampled this very shot's `border-t`:
//!   #333333 x 1366/1366. [`color::DIVIDER`] stays out of it; it is a `bg-`
//!   token.
//! * **Line height.** Tailwind's preflight leaves `line-height: 1.5` inherited,
//!   i.e. 15.75px at the footer's `text-[10.5px]`. Measured, each `border-l`
//!   paints exactly 15 rows (y=5..19 of the 24px bar) — a 15px box centred in
//!   the border-box content area `y=1..24` lands on precisely those rows, with
//!   no dependence on how either renderer snaps a fractional edge. GPUI's own
//!   default line box (phi x font-size) is neither number, so [`LINE_H`] pins
//!   it to 15px. The glyph baseline does not move as a result: centring a line
//!   box inside a centred flex item cancels the box's height out of the
//!   baseline calculation, so 15 and 15.75 put every glyph in the same place.
//! * **Mixed weight on one line.** `Canvas: ` (dim, 400) and `800 x 600 px`
//!   (text, 700 — Tailwind's preflight renders `<strong>` `bolder`) are inline
//!   siblings in JSX. GPUI stacks plain text children, so each cell is one
//!   [`StyledText`] carrying two [`TextRun`]s: one shaped line, one line box,
//!   and no invented inter-word gap. Runs spell out `Inter` themselves because
//!   an ambient `.font_family` reaches the element, not the run.
//! * The five cell rules land at x=125, 204, 314, 608 and ~760 in the PNG.
//!   Those are pure functions of the cell widths (padding 12 + gap 12 + 1px
//!   rule), so they are reproduced exactly as long as Inter shapes the strings
//!   the same way it does in `title_bar`, which measures to 0.005%.

use gpui_kit::base::h_flex;
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::{div, font, px, Hsla, StyledText, TextRun};

use crate::assets::icons;
use crate::components::ui;
use crate::tokens::color;

use gpui_kit::Div;

// ── Dimensions (Tailwind, 16px root) ──

/// `h-[24px]` — BottomStatusBar.tsx:122. Same number as the manifest box
/// height (24), which is where this constant was first pinned.
const BAR_H: f32 = 24.0;

/// `px-3` = 0.75rem — footer inset, BottomStatusBar.tsx:122. The PNG's first
/// content pixel sits at x=12 and its last at x=1354, i.e. 12px both ends.
const BAR_PX: f32 = 12.0;

/// `text-[10.5px]` — footer font size, BottomStatusBar.tsx:122.
const TEXT_SIZE: f32 = 10.5;

/// Line box height for every run in the bar; see the module docs for why it is
/// 15 rather than the browser's 15.75 or GPUI's default.
const LINE_H: f32 = 15.0;

/// `gap-3` = 0.75rem between the cells of the left group,
/// BottomStatusBar.tsx:126.
const GROUP_GAP: f32 = 12.0;

/// `pl-3` = 0.75rem inside each bordered cell — BottomStatusBar.tsx:131, 134,
/// 137, 161 and 170 (the cells carrying `border-l border-editor-divider pl-3`).
const CELL_PAD: f32 = 12.0;

/// `gap-5` = 1.25rem in the right group, BottomStatusBar.tsx:224. The bar only
/// ever renders the one History button there, so nothing spaces out today; the
/// gap is kept because it is part of the component's contract.
const RIGHT_GAP: f32 = 20.0;

/// `gap-1` = 0.25rem between the History icon and its label,
/// BottomStatusBar.tsx:235.
const BTN_GAP: f32 = 4.0;

/// `size-3.5` = 0.875rem — the history icon, BottomStatusBar.tsx:239.
const HISTORY_ICON: f32 = 14.0;

/// `strokeWidth={1.75}` — BottomStatusBar.tsx:239. Deliberately *not*
/// `tokens::dim::ICON_STROKE` (1.6, the tool rail's) and not
/// `tokens::title::ICON_STROKE` (1.75, but title-bar scoped): `icons::HISTORY`
/// bakes 1.5, so this call site asks [`crate::assets::icons::with_stroke`] for
/// its own width.
const ICON_STROKE: f32 = 1.75;

/// The History button's vertical padding. Its content box is 15px tall (the
/// bar's line height), which left a 15px click target; 2x4.5 lifts the hit box
/// to the 24px minimum target size. Symmetric, so nothing moves visually.
const HIT_PY: f32 = 4.5;

// ── State stand-ins ──
//
// The captured values. In the real component these are `docWidth()` x
// `docHeight()`, `Math.round(zoom() * 100)`, `getToolDisplayName()`,
// `statusText()` for the active tool, `activeLayerName()`, and the dev-mode
// frame metrics from `scheduler.getFrameMetrics()`. The port renders what the
// baseline PNG shows because there is no engine behind it.

/// `{docWidth()} x {docHeight()} px` — BottomStatusBar.tsx:129. The document is
/// `createBlankDocument("capture-baseline", "Baseline", 800, 600)`,
/// `capture-baseline.ts:183`. The baseline renders the multiplication sign as
/// the Unicode x-cross (`\u{d7}`); the port matches that glyph.
const DOC_SIZE: &str = "800 \u{d7} 600 px";

/// `Math.round(zoom() * 100)}%` — BottomStatusBar.tsx:132, as captured (88%).
const ZOOM: &str = "88%";

/// `getToolDisplayName()` for `activeTool() === "move"` — BottomStatusBar.tsx:135,
/// via `tools.move` in `locales/en/translation.json:110`.
const TOOL: &str = "Move Tool";

/// `statusText()` for the `move` tool — BottomStatusBar.tsx:107 resolving
/// `status.toolTips.move`, `locales/en/translation.json:396`. This is the
/// tooltip sentence, not a status: the real component shows the same string
/// whenever nothing else (gradient drag, transform session, paint block) is
/// in flight.
const STATUS_TIP: &str = "Drag to move layer. Hold Shift for constrained movement.";

/// `activeLayerName()` — BottomStatusBar.tsx:164. `activeTool()` is `move`, so
/// the name comes from the selected layer, which for a blank document is the
/// seed layer "Background" (`crates/core/src/canonical_model.rs:428`).
const LAYER: &str = "Background";

/// `text-editor-text/60` — the tool-tip sentence, BottomStatusBar.tsx:138.
///
/// Not `color::TEXT_DIM_60`, which is text-**dim** at 60%; this is
/// `--editor-text` (#CACACA) at 60%, per the class on that span. Over the
/// #1E1E1E panel it lands on ~#858585, and the PNG's status sentence peaks at
/// 132 where a full #CACACA peaks at 202 — consistent with 0.6 alpha.
const STATUS_COLOR: Hsla = Hsla {
    h: color::TEXT.h,
    s: color::TEXT.s,
    l: color::TEXT.l,
    a: 0.6,
};

/// `text-editor-text/40` — the ` max {max}ms` tail span inside the Frame cell
/// (BottomStatusBar.tsx:172). Same base as [`STATUS_COLOR`] at 40%: the golden
/// master measures one flat ~94 peak across that whole span, where the 60%
/// labels peak at ~122 and the full-alpha value at 202.
const FRAME_TAIL_COLOR: Hsla = Hsla {
    h: color::TEXT.h,
    s: color::TEXT.s,
    l: color::TEXT.l,
    a: 0.4,
};

/// The dev-mode frame metrics the cell reads, from the web build's
/// `FrameMetrics` (`apps/desktop/src/renderer/scheduler`).
///
/// The golden master captured with `avgMs = 1.1`, `maxMs = 1.9` — the values
/// `BottomStatusBar.tsx:172-173` format as `{avgMs.toFixed(1)}` and
/// `{maxMs.toFixed(1)}`. Both are rendered to one decimal place.
struct FrameMetrics {
    avg_ms: f64,
    max_ms: f64,
}

/// The captured frame metrics in the golden master; `frame_text` formats these
/// through `FrameMetrics::avg_ms_str` / `max_ms_str` so the literal the test
/// asserts against and the literal the cell paints come from the same source.
const FRAME: FrameMetrics = FrameMetrics {
    avg_ms: 1.1,
    max_ms: 1.9,
};

impl FrameMetrics {
    fn avg_ms_str(&self) -> String {
        format!("{:.1}", self.avg_ms)
    }
    fn max_ms_str(&self) -> String {
        format!("{:.1}", self.max_ms)
    }
}

/// `<footer class="relative flex h-[24px] shrink-0 items-center justify-between
///  border-t border-editor-divider bg-editor-panel-bg px-3 text-[10.5px]
///  text-editor-text-dim select-none">`
///
/// `relative` only exists in the source to anchor the tiling-progress hairline
/// (omitted above); it is kept so the element tree still mirrors the JSX.
pub fn render() -> impl IntoElement {
    h_flex()
        .relative()
        .h(px(BAR_H))
        .flex_shrink_0()
        .justify_between()
        .border_t_1()
        // `border-editor-divider` names #2e2e2e, but the unlayered `*` in
        // styles.css outranks it and paints #333333 — see `color::BORDER`.
        .border_color(color::BORDER)
        .bg(color::PANEL)
        .px(px(BAR_PX))
        // GPUI's default family is `.SystemUIFont` (Segoe UI here); every run
        // in this bar is Inter, same as `title_bar`.
        .font_family(ui::FONT_UI)
        .text_size(px(TEXT_SIZE))
        .line_height(px(LINE_H))
        .text_color(color::TEXT_DIM)
        // The left group yields and the right group does not: the readout cells
        // are variable-width, so with nothing shrinking a long layer name used
        // to walk the zoom percentage and the whole right cluster off the bar.
        .child(ui::clip(left_group()))
        .child(ui::pin(right_group()))
}

/// `<div class="flex items-center gap-3">` — the readout cells, in order.
///
/// Six cells: Canvas, Zoom, Active, tool-hint, Selected Layer, and the dev-mode
/// Frame timing. The Frame cell is gated in the web build on
/// `devFrameStats() && devFrameStats()!.frames > 0`; the dev-mode capture
/// always has frames > 0, so the cell is always present in the golden master.
fn left_group() -> Div {
    h_flex()
        .gap(px(GROUP_GAP))
        // The first cell is the only one without the `border-l ... pl-3` rule.
        .child(field("Canvas: ", DOC_SIZE))
        .child(divided_field("Zoom: ", ZOOM))
        .child(divided_field("Active: ", TOOL))
        .child(divided_tip())
        .child(divided_field("Selected Layer: ", LAYER))
        .child(divided_frame())
}

/// `label: <strong>value</strong>` without a rule — the `Canvas` cell.
fn field(label: &str, value: &str) -> impl IntoElement {
    div().child(labeled(label, value))
}

/// The same cell behind `<span class="border-l border-editor-divider pl-3">`.
fn divided_field(label: &str, value: &str) -> impl IntoElement {
    ruled(labeled(label, value))
}

/// The dev-mode Frame timing cell behind `<span class="border-l border-editor-divider
///  pl-3">`.
///
/// `BottomStatusBar.tsx:170`:
///
/// ```jsx
/// <span class="border-l border-editor-divider pl-3">
///   Frame: <strong class="text-editor-text">{avgMs.toFixed(1)}ms avg</strong>
///   <span class="text-editor-text/40"> max {maxMs.toFixed(1)}ms</span>
/// </span>
/// ```
///
/// Per the golden master's measured pixel levels: `Frame:` at [`STATUS_COLOR`]
/// (60%, peaks ~122), `{avg}ms avg` bold in `color::TEXT` (peaks 202), and the
/// ` max {max}ms` tail at [`FRAME_TAIL_COLOR`] (one flat ~94 level - the
/// number is not bold). The captured values come from the golden master:
/// avgMs=1.1, maxMs=1.9.
fn divided_frame() -> impl IntoElement {
    ruled(frame_text(&FRAME, STATUS_COLOR))
}

/// One fragment of the Frame cell: its text, its colour, and whether it is
/// bold.
struct FrameRun {
    text: String,
    color: Hsla,
    bold: bool,
}

/// The three fragments `frame_text` lays out, in order, with their formatting.
///
/// Extracted as a standalone function so the run structure is testable without
/// inspecting the opaque `StyledText` (its fields are private in GPUI).
fn frame_run_specs(metrics: &FrameMetrics, status_color: Hsla) -> Vec<FrameRun> {
    let avg = metrics.avg_ms_str();
    let max = metrics.max_ms_str();
    vec![
        FrameRun {
            text: "Frame: ".to_string(),
            color: status_color,
            bold: false,
        },
        FrameRun {
            text: format!("{avg}ms avg"),
            color: color::TEXT,
            bold: true,
        },
        FrameRun {
            // One run, not two: the JSX wraps ` max {max}ms` in a single
            // `text-editor-text/40` span, and the golden master measures one
            // flat ~94 level across the whole tail - `max 1.9ms` is NOT bold.
            text: format!(" max {max}ms"),
            color: FRAME_TAIL_COLOR,
            bold: false,
        },
    ]
}

/// Build the `Frame: <avg>ms avg max <max>ms` run with mixed weights.
///
/// `Frame:` uses `status_color` (60%); `{avg}ms avg` is bold `color::TEXT`;
/// the ` max {max}ms` tail is [`FRAME_TAIL_COLOR`] (40%, not bold). Three runs
/// on one [`StyledText`] so they share a single line box — GPUI stacks text
/// children rather than flowing them.
fn frame_text(metrics: &FrameMetrics, status_color: Hsla) -> StyledText {
    let specs = frame_run_specs(metrics, status_color);
    let joined: String = specs.iter().map(|r| r.text.as_str()).collect();

    StyledText::new(joined).with_runs(
        specs
            .iter()
            .map(|r| TextRun {
                len: r.text.len(),
                font: if r.bold {
                    font(ui::FONT_UI).bold()
                } else {
                    font(ui::FONT_UI)
                },
                color: r.color,
                ..Default::default()
            })
            .collect(),
    )
}

/// `<span class="border-l border-editor-divider pl-3">
///  <span class="text-editor-text/60">...</span></span>` — the tool hint.
///
/// The inner span carries the colour and the outer the rule; nesting them
/// rather than colouring the ruled div keeps the two JSX layers apart, which
/// matters if either ever grows a second child.
fn divided_tip() -> impl IntoElement {
    ruled(div().text_color(STATUS_COLOR).child(STATUS_TIP))
}

/// The `border-l border-editor-divider pl-3` wrapper, applied to every cell
/// after the first. 1px rule + 12px padding, exactly as CSS adds them to a
/// bordered flex item.
fn ruled(content: impl IntoElement) -> impl IntoElement {
    div()
        .border_l_1()
        .border_color(color::BORDER)
        .pl(px(CELL_PAD))
        .child(content)
}

/// One inline flow: the dim label, then the value in `<strong>` — `bolder` in
/// Tailwind's preflight, i.e. 700 over the 400 the footer sets.
///
/// Two runs rather than two sibling elements because GPUI stacks text children
/// instead of flowing them, and because the space between `:` and the value
/// belongs to the label run in the source (`{t(..)}: <strong>`), where it is
/// shaped as part of the same line.
fn labeled(label: &str, value: &str) -> StyledText {
    StyledText::new(format!("{label}{value}")).with_runs(vec![
        TextRun {
            len: label.len(),
            font: font(ui::FONT_UI),
            color: color::TEXT_DIM,
            ..Default::default()
        },
        TextRun {
            len: value.len(),
            font: font(ui::FONT_UI).bold(),
            color: color::TEXT,
            ..Default::default()
        },
    ])
}

/// `<div class="flex shrink-0 items-center gap-5">` — the right-hand cell.
///
/// The source also toggles `opacity-50 pointer-events-none` when no document
/// is open; the baseline has one, so that branch never renders here.
fn right_group() -> Div {
    h_flex()
        .flex_shrink_0()
        .gap(px(RIGHT_GAP))
        .child(history_button())
}

/// `<button type="button" data-status-history-trigger
///  class="flex items-center gap-1 hover:text-editor-text transition-colors">`
///
/// Resting colour is the footer's `text-editor-text-dim` — the button sets no
/// colour of its own unless the History dock panel is the open one, and the
/// PNG shows dim, not `text-editor-accent`, so it is not.
///
/// The `data-status-history-trigger` hook, `aria-pressed` and the dock wiring
/// are state, not chrome; only the appearance is ported.
///
/// [`HIT_PY`] pads the button's 15px content box out to the 24px minimum
/// target size; symmetric, so the icon and label do not move.
fn history_button() -> impl IntoElement {
    h_flex()
        .id("status-history")
        .gap(px(BTN_GAP))
        .py(px(HIT_PY))
        .hover(|s| s.text_color(color::TEXT))
        .child(
            // `icons::HISTORY` bakes stroke-width 1.5; this call site asks for
            // lucide's `strokeWidth={1.75}` — see `crate::assets::icons::with_stroke`.
            Icon::default()
                .data(icons::with_stroke(icons::HISTORY, ICON_STROKE))
                .size(px(HISTORY_ICON))
                .text_color(color::TEXT_DIM),
        )
        .child("History")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Asserts on `frame_run_specs` — the pure data source `frame_text` builds
    /// from — because `StyledText` exposes its runs as a private field, not a
    /// method. `frame_text` is a thin consumer of these specs, so exercising them
    /// IS exercising the production path.
    ///
    /// RED proof: alter any `text`, `bold`, or `alpha` value in
    /// `frame_run_specs` and the corresponding assertion here flips.
    #[test]
    fn frame_cell_runs_match_baseline() {
        let runs = frame_run_specs(&FRAME, STATUS_COLOR);

        assert_eq!(runs.len(), 3, "frame cell must have exactly three runs");

        let full: String = runs.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(
            full, "Frame: 1.1ms avg max 1.9ms",
            "frame cell text must match the baseline literal"
        );

        // `Frame: ` - dim label, 60% (golden peaks ~122 where a full-alpha
        // peak measures 202)
        assert_eq!(runs[0].text, "Frame: ");
        assert!(!runs[0].bold);
        assert_eq!(runs[0].color.a, STATUS_COLOR.a);

        // `{avg}ms avg` - the only bold fragment (golden peaks 202)
        assert_eq!(runs[1].text, "1.1ms avg");
        assert!(runs[1].bold);
        assert_eq!(runs[1].color.a, color::TEXT.a);

        // ` max {max}ms` - one 40% span, number included, NOT bold (golden
        // measures one flat ~94 level across the whole tail)
        assert_eq!(runs[2].text, " max 1.9ms");
        assert!(!runs[2].bold);
        assert_eq!(runs[2].color.a, FRAME_TAIL_COLOR.a);
    }

    /// The source values `FrameMetrics` formats must match the golden master:
    /// `avgMs = 1.1`, `maxMs = 1.9`.
    #[test]
    fn frame_metrics_source_values() {
        assert_eq!(FRAME.avg_ms_str(), "1.1");
        assert_eq!(FRAME.max_ms_str(), "1.9");
    }
}
