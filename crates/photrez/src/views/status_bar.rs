//! Native status bar (the window footer).
//!
//! Geometry: `manifest.json` gives `shell-status-bar` as 1366x24 at (0,796) —
//! the full client width, below the content area, and the last 24px that take the
//! window from 796 to the baseline's 820.
//!
//! # Scope: the resting state the baseline captures
//!
//! The real footer is a live readout of engine state — document size, zoom,
//! active tool, the tool hint, the selected layer — plus five conditional
//! indicators that only mount while something is happening:
//!
//! * the tiling-progress hairline, shown while a loading message is set,
//! * the heavy-op spinner and its cancel button, on the same condition,
//! * the dev-mode frame timings, which mount whenever `frames > 0`,
//! * the autosave dot, shown whenever the autosave status is not idle,
//! * the manual-save progress row, shown whenever the save phase is not idle.
//!
//! Of those five, the dev-mode Frame cell IS captured in the resting state:
//! it is gated only on dev frame stats being on with `frames > 0`, and the
//! dev-mode capture has a live scheduler
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
//! `const` below names the state it stands in for, because this view has no
//! engine to read from.
//!
//! Labels are the `en` locale strings (`tests/fixtures/i18n/en/translation.json`,
//! `status.*`), the locale the baseline was captured in.
//!
//! # Transcription notes
//!
//! * Every border in the bar paints [`color::BORDER`] (#333333) — the top
//!   rule and the four cell separators included, even where the divider token
//!   names #2e2e2e. Sampling this very shot's top rule gives
//!   #333333 x 1366/1366 pixels. [`color::DIVIDER`] stays out of it; it is a
//!   fill token.
//! * **Line height.** The captured line box is 1.5 x the footer's 10.5px,
//!   i.e. 15.75px. Measured, each cell separator
//!   paints exactly 15 rows (y=5..19 of the 24px bar) — a 15px box centred in
//!   the border-box content area `y=1..24` lands on precisely those rows, with
//!   no dependence on how either renderer snaps a fractional edge. GPUI's own
//!   default line box (phi x font-size) is neither number, so `LINE_H` pins
//!   it to 15px. The glyph baseline does not move as a result: centring a line
//!   box inside a centred flex item cancels the box's height out of the
//!   baseline calculation, so 15 and 15.75 put every glyph in the same place.
//! * **Mixed weight on one line.** `Canvas: ` (dim, 400) and `800 x 600 px`
//!   (text, 700 — the source's emphasis run, which renders bolder) are inline
//!   siblings in the markup. GPUI stacks plain text children, so each cell is one
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
use gpui_kit::{div, font, px, Hsla, StyledText, TestSupportExt, TextRun};

use crate::components::ui;
use crate::i18n::t;
use crate::icons;
use crate::tokens::color;

use gpui_kit::Div;

// ── Dimensions (root font size 16px) ──

/// 24 px bar height. Same number as the manifest box
/// height (24), which is where this constant was first pinned.
const BAR_H: f32 = 24.0;

/// 0.75rem (12px) footer inset. The PNG's first
/// content pixel sits at x=12 and its last at x=1354, i.e. 12px both ends.
const BAR_PX: f32 = 12.0;

/// 10.5px footer font size.
const TEXT_SIZE: f32 = 10.5;

/// Line box height for every run in the bar; see the module docs for why it is
/// 15 rather than the previous renderer's 15.75 or GPUI's default.
const LINE_H: f32 = 15.0;

/// 0.75rem (12px) between the cells of the left group.
const GROUP_GAP: f32 = 12.0;

/// 0.75rem (12px) inside each bordered cell — the cells that carry a
/// separator and a left inset.
const CELL_PAD: f32 = 12.0;

/// 1.25rem (20px) in the right group. The bar only
/// ever renders the one History button there, so nothing spaces out today; the
/// gap is kept because it is part of the component's contract.
const RIGHT_GAP: f32 = 20.0;

/// 0.25rem (4px) between the History icon and its label.
const BTN_GAP: f32 = 4.0;

/// 0.875rem (14px) — the history icon size.
const HISTORY_ICON: f32 = 14.0;

/// 1.75 icon stroke width. Deliberately *not*
/// `tokens::dim::ICON_STROKE` (1.6, the tool rail's) and not
/// `tokens::title::ICON_STROKE` (1.75, but title-bar scoped): `icons::HISTORY`
/// bakes 1.5, so this call site asks [`crate::icons::with_stroke`] for
/// its own width.
const ICON_STROKE: f32 = 1.75;

/// The History button's vertical padding. Its content box is 15px tall (the
/// bar's line height), which left a 15px click target; 2x4.5 lifts the hit box
/// to the 24px minimum target size. Symmetric, so nothing moves visually.
const HIT_PY: f32 = 4.5;

/// Cap for the save-error message. The cell is the only variable-width member
/// of the pinned right group, so a filesystem error sentence must clip rather
/// than walk the History button off the bar.
const SAVE_ERROR_MAX_W: f32 = 420.0;

// ── State stand-ins ──
//
// The captured values. In the live app these are the document width x height,
// the rounded zoom percent, the active tool's display name, that tool's status
// sentence, the selected layer's name, and the dev-mode frame metrics read from
// the frame scheduler. This view renders what the baseline PNG shows because
// there is no engine behind it.

/// Document size as `{width} x {height} px`. The document is
/// the blank 800x600 "Baseline" capture recipe. The baseline renders the
/// multiplication sign as
/// the Unicode x-cross (`\u{d7}`); this string matches that glyph.
const DOC_SIZE: &str = "800 \u{d7} 600 px"; // i18n-exempt: numeric value

/// The rounded zoom percent, as captured (88%).
const ZOOM: &str = "88%"; // i18n-exempt: numeric value

// The active tool is Move, whose display name is `tools.move` in
// `tests/fixtures/i18n/en/translation.json:110`.
// The status sentence is the `move` tool's tooltip, `status.toolTips.move`
// at `tests/fixtures/i18n/en/translation.json:396`. This is the
// tooltip sentence, not a status: the footer shows the same string
// whenever nothing else (gradient drag, transform session, paint block) is
// in flight.

/// The selected layer's name. The status bar shows the same
/// selected layer the Layers and Properties panels draw, so it must not keep a
/// stale seed name: the seeded document's layer is the new-document
/// "Background" layer.
const LAYER: &str = "Background"; // i18n-exempt: new-document layer name

/// The tool-tip sentence colour: the plain text colour (#CACACA) at 60% alpha.
///
/// Not `color::TEXT_DIM_60`, which is text-**dim** at 60%. Over the
/// #1E1E1E panel it lands on ~#858585, and the PNG's status sentence peaks at
/// 132 where a full #CACACA peaks at 202 — consistent with 0.6 alpha.
const STATUS_COLOR: Hsla = Hsla {
    h: color::TEXT.h,
    s: color::TEXT.s,
    l: color::TEXT.l,
    a: 0.6,
};

/// The ` max {max}ms` tail span inside the Frame cell. Same base as
/// [`STATUS_COLOR`] at 40%: the golden
/// master measures one flat ~94 peak across that whole span, where the 60%
/// labels peak at ~122 and the full-alpha value at 202.
const FRAME_TAIL_COLOR: Hsla = Hsla {
    h: color::TEXT.h,
    s: color::TEXT.s,
    l: color::TEXT.l,
    a: 0.4,
};

/// The dev-mode frame metrics the Frame cell reads from the frame scheduler.
///
/// The golden master captured `avg_ms = 1.1`, `max_ms = 1.9`;
/// both are rendered to one decimal place.
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

/// The bar itself: a positioned row 24px tall, never squeezed, centred on one
/// row with children pushed to both ends; 1px top border, panel fill, 12px
/// side padding, 10.5px dim text, nothing selectable.
///
/// Being positioned only exists in the source to anchor the tiling-progress
/// hairline (omitted above); it is kept so the tree mirrors the layout.
///
/// `status_error` is production state, not a baseline value: the resting captures
/// pass `None`, so the default bar is pixel-identical to the golden master. A
/// failed save paints the typed service error in red until a save succeeds.
///
/// `state` and `cx` reach the History trigger's click handler and the accent
/// tint of its panel; every other cell here is a value the caller passes in.
#[allow(clippy::too_many_arguments)] // six live cells plus the state the trigger writes
pub fn render(
    status_error: Option<&str>,
    zoom_percent: Option<u32>,
    doc_size: Option<(u32, u32)>,
    tool: crate::state::ToolId,
    layer_name: Option<&str>,
    autosave: crate::state::AutosaveStatus,
    state: &gpui_kit::Entity<crate::state::AppState>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let history_open = state.read(cx).layer_tab == crate::state::LayerTab::History;
    h_flex()
        .relative()
        .h(px(BAR_H))
        .flex_shrink_0()
        .justify_between()
        .border_t_1()
        // The divider token names #2e2e2e, but the captured bar paints
        // #333333 on every border — see `color::BORDER`.
        .border_color(color::BORDER)
        .bg(color::PANEL)
        .px(px(BAR_PX))
        // GPUI's default family is `.SystemUIFont` (Segoe UI here); every run
        // in this bar is Inter, same as `title_bar`.
        .font_family(ui::FONT_UI)
        .text_size(px(TEXT_SIZE))
        .line_height(px(LINE_H))
        .text_color(color::TEXT_DIM)
        // With no document open the bar shows no readout cells at all -
        // the Canvas/Zoom/Active/Selected segment describes a document that
        // does not exist. The left child must still exist: `justify_between`
        // with a single child would park the History hint on the left, and
        // the baseline keeps it on the right.
        .child(if doc_size.is_some() {
            ui::clip(left_group(zoom_percent, doc_size, tool, layer_name)).into_any_element()
        } else {
            div().into_any_element()
        })
        .child(ui::pin(right_group(
            status_error,
            autosave,
            history_open,
            cx,
        )))
}

/// The readout cells, in order: centred on one row with 12px between them.
///
/// Six cells: Canvas, Zoom, Active, tool-hint, Selected Layer, and the dev-mode
/// Frame timing. The Frame cell keeps the previous build's gate - it shows
/// only while frame stats report at least one frame; the dev-mode capture
/// always has frames > 0, so the cell is always present in the golden master.
fn left_group(
    zoom_percent: Option<u32>,
    doc_size: Option<(u32, u32)>,
    tool: crate::state::ToolId,
    layer_name: Option<&str>,
) -> Div {
    let zoom = zoom_percent
        .map(|percent| format!("{percent}%"))
        .unwrap_or_else(|| ZOOM.to_string());
    let doc = doc_size
        .map(|(width, height)| format!("{width} \u{d7} {height} px"))
        .unwrap_or_else(|| DOC_SIZE.to_string());
    let tool_label = t(tool_key(tool));
    let layer = layer_name.unwrap_or(LAYER).to_string();
    h_flex()
        .gap(px(GROUP_GAP))
        // The first cell is the only one without the left rule and its padding.
        .child(field(&format!("{}: ", t("status.canvas")), &doc))
        .child(divided_field(&format!("{}: ", t("status.zoom")), &zoom))
        .child(divided_field(
            &format!("{}: ", t("status.active")),
            &tool_label,
        ))
        .child(divided_tip(tool))
        .child(divided_field(
            &format!("{}: ", t("status.selectedLayer")),
            &layer,
        ))
        .child(divided_frame())
}

fn tool_key(tool: crate::state::ToolId) -> &'static str {
    use crate::state::ToolId;
    match tool {
        ToolId::Move => "tools.move",
        ToolId::Selection => "tools.rectSelect",
        ToolId::Crop => "tools.crop",
        ToolId::Eyedropper => "tools.eyedropper",
        ToolId::Brush => "tools.brush",
        ToolId::Eraser => "tools.eraser",
        ToolId::PaintBucket => "tools.paintBucket",
        ToolId::Gradient => "tools.gradient",
        ToolId::Shape => "tools.shape",
        ToolId::Text => "tools.text",
    }
}

/// A dim label then a bold value, with no rule — the `Canvas` cell.
fn field(label: &str, value: &str) -> impl IntoElement {
    div().child(labeled(label, value))
}

/// The same cell behind a left-separated, 12px-inset field.
fn divided_field(label: &str, value: &str) -> impl IntoElement {
    ruled(labeled(label, value))
}

/// The dev-mode Frame timing cell, in the same left-separated, 12px-inset
/// field as the other readouts.
///
/// Its content is one line with three runs:
///
/// ```text
/// Frame: 1.1ms avg max 1.9ms
/// ```
///
/// Per the golden master's measured pixel levels: `Frame:` at [`STATUS_COLOR`]
/// (60%, peaks ~122), `{avg}ms avg` bold in `color::TEXT` (peaks 202), and the
/// ` max {max}ms` tail at [`FRAME_TAIL_COLOR`] (one flat ~94 level - the
/// number is not bold). The captured values come from the golden master:
/// `avg_ms = 1.1`, `max_ms = 1.9`.
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
            text: "Frame: ".to_string(), // i18n-exempt: not in catalog
            color: status_color,
            bold: false,
        },
        FrameRun {
            text: format!("{avg}ms avg"),
            color: color::TEXT,
            bold: true,
        },
        FrameRun {
            // One run, not two: the markup wraps ` max {max}ms` in a single
            // plain-text-at-40% span, and the golden master measures one
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

/// The tool hint: a 1px left rule with 12px left padding on the outer layer,
/// the sentence at 60% of the plain text colour on the inner one.
///
/// The inner layer carries the colour and the outer the rule; nesting them
/// rather than colouring the ruled div keeps the two layers apart, which
/// matters if either ever grows a second child.
fn divided_tip(tool: crate::state::ToolId) -> impl IntoElement {
    ruled(div().text_color(STATUS_COLOR).child(t(tip_key(tool))))
}

fn tip_key(tool: crate::state::ToolId) -> &'static str {
    use crate::state::ToolId;
    match tool {
        ToolId::Move => "status.toolTips.move",
        ToolId::Selection => "status.toolTips.selection",
        ToolId::Crop => "status.toolTips.crop",
        ToolId::Eyedropper => "status.toolTips.eyedropper",
        ToolId::Brush => "status.toolTips.brush",
        ToolId::Eraser => "status.toolTips.eraser",
        ToolId::PaintBucket => "status.toolTips.paintBucket",
        ToolId::Gradient => "status.toolTips.gradient",
        ToolId::Shape => "status.toolTips.shape",
        ToolId::Text => "status.toolTips.text",
    }
}

/// The left-rule wrapper, applied to every cell
/// after the first. 1px rule + 12px padding, exactly as CSS adds them to a
/// bordered flex item.
fn ruled(content: impl IntoElement) -> impl IntoElement {
    div()
        .border_l_1()
        .border_color(color::BORDER)
        .pl(px(CELL_PAD))
        .child(content)
}

/// One inline flow: the dim label, then the value bold — 700 over the 400 the
/// bar itself sets.
///
/// Two runs rather than two sibling elements because GPUI stacks text children
/// instead of flowing them, and because the space between `:` and the value
/// belongs to the label run in the source, where it is
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

/// The save cell: the design pairs every autosave state with an 8px dot in that
/// state's own tier, so the status reads at a glance and not only through the
/// label. The busy dot's `animate-pulse` is the 0.1 build's only motion here and
/// stays a static fill.
fn autosave_cell(autosave: crate::state::AutosaveStatus) -> Option<gpui_kit::AnyElement> {
    use crate::state::AutosaveStatus;
    let (label, dot, text) = match autosave {
        AutosaveStatus::Saving => (t("common.saving"), color::YELLOW_400, color::TEXT_DIM),
        AutosaveStatus::Saved => (t("common.saved"), color::GREEN_400, color::TEXT_DIM),
        AutosaveStatus::Error => (t("common.saveFailed"), color::RED_400, color::RED_400),
        AutosaveStatus::Idle => return None,
    };
    Some(
        h_flex()
            .id("autosave-status")
            .test_support()
            .items_center()
            .gap(px(4.0))
            .child(
                div()
                    .id("autosave-dot")
                    .test_support()
                    .size(px(8.0))
                    .rounded_full()
                    .bg(dot),
            )
            .child(div().text_color(text).child(label))
            .into_any_element(),
    )
}

/// The right-hand cells: centred on one row, never squeezed, 20px between them.
///
/// The source also dims the group to 50% and ignores input when no document
/// is open; the baseline has one, so that branch never renders here.
fn right_group(
    status_error: Option<&str>,
    autosave: crate::state::AutosaveStatus,
    history_open: bool,
    cx: &Context<crate::app::App>,
) -> Div {
    h_flex()
        .flex_shrink_0()
        .gap(px(RIGHT_GAP))
        .when_some(autosave_cell(autosave), |row, cell| row.child(cell))
        .when_some(status_error, |row, message| {
            row.child(
                div()
                    .id("status-error")
                    .test_support()
                    .max_w(px(SAVE_ERROR_MAX_W))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_color(color::RED_500)
                    .child(message.to_string()),
            )
        })
        .child(history_button(history_open, cx))
}

/// The History trigger: one row, centred, 4px between icon and label, colour
/// transitioned, plain text colour on hover.
///
/// It is accent-coloured while the History dock panel is the open one and dim
/// otherwise; the resting captures have the Layers panel open, so the golden
/// master shows dim. Clicking shows the History panel and opens the dock.
///
/// The `data-status-history-trigger` hook and `aria-pressed` have no GPUI
/// surface, so they stay with the reference; the wiring is real.
///
/// [`HIT_PY`] pads the button's 15px content box out to the 24px minimum
/// target size; symmetric, so the icon and label do not move.
fn history_button(open: bool, cx: &Context<crate::app::App>) -> impl IntoElement {
    let ink = if open { color::ACCENT } else { color::TEXT_DIM };
    let hover_ink = if open { color::ACCENT } else { color::TEXT };
    h_flex()
        .id("status-history")
        .test_support()
        .gap(px(BTN_GAP))
        .py(px(HIT_PY))
        .cursor_pointer()
        .text_color(ink)
        .hover(move |s| s.text_color(hover_ink))
        .on_click(cx.listener(
            |app, _: &gpui_kit::ClickEvent, _window, cx: &mut Context<crate::app::App>| {
                app.show_history_panel(cx);
            },
        ))
        .child(
            // `icons::HISTORY` bakes stroke-width 1.5; this call site asks for
            // lucide's `strokeWidth={1.75}` — see `crate::icons::with_stroke`.
            Icon::default()
                .data(icons::with_stroke(icons::HISTORY, ICON_STROKE))
                .size(px(HISTORY_ICON))
                .text_color(ink),
        )
        .child(t("panels.history"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Asserts on `frame_run_specs` — the pure data source `frame_text` builds
    /// from — because `StyledText` exposes its runs as a private field, not a
    /// method. `frame_text` is a thin consumer of these specs, so exercising them
    /// IS exercising the production path.
    ///
    /// RED proof: alter any text, bold, or alpha value in
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
    /// `avg_ms = 1.1`, `max_ms = 1.9`.
    #[test]
    fn frame_metrics_source_values() {
        assert_eq!(FRAME.avg_ms_str(), "1.1");
        assert_eq!(FRAME.max_ms_str(), "1.9");
    }

    /// The Active cell asks for a key chosen at runtime, so the literal-key
    /// sweep cannot see it: every tool must land on a catalog entry, or the
    /// cell prints the dotted key itself.
    #[test]
    fn every_tool_label_resolves() {
        use crate::state::ToolId;
        let tools = [
            ToolId::Move,
            ToolId::Selection,
            ToolId::Crop,
            ToolId::Eyedropper,
            ToolId::Brush,
            ToolId::Eraser,
            ToolId::PaintBucket,
            ToolId::Gradient,
            ToolId::Shape,
            ToolId::Text,
        ];
        for tool in tools {
            let key = tool_key(tool);
            assert_ne!(
                crate::i18n::t(key),
                key,
                "the Active cell renders {key} verbatim: no catalog entry answers it"
            );
        }
    }
}
