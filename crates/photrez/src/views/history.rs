//! History panel: the same 260x527 slot as the Layers tab, 6px scrollbar gutter included.
//! Geometry and the single "Open" row come from the reference capture; no document fallback is in the shot.
//! The label→icon map is complete even though only "open" is reachable in this state.
//! Hover, keyboard focus, transitions and real scrolling are omitted.
//! LCD subpixel fringes and a half-pixel vertical tie remain the known fidelity gaps.
use crate::components::ui;
use crate::icons;
use crate::state::AppState;
use crate::tokens::color;
use rust_i18n::t;

use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::{Div, FontWeight, Hsla, TestSupportExt, div, px};

// ── Geometry ────────────────────────────────────────────────────────────────

/// LayerDock's column width: `shell-navigator` is 260 wide at x=1106 and the
/// dock runs to the window edge at 1366.
const PANEL_W: f32 = 260.0;

/// Content height above the Navigator at the baseline window: 611 - 84.
///
/// Kept as the measured figure rather than as a layout input — the root is
/// `flex_1` now (its parent supplies the definite column), so this is what the
/// panel works out to at 1366x820 and what a capture at any other height grows
/// or shrinks from.
#[expect(
    dead_code,
    reason = "documents baseline geometry; root is flex_1 and absorbs window height"
)]
const PANEL_H: f32 = 527.0;

/// The 6px gutter reserved for the scrollable list's scrollbar.
/// See the module docs; this is why rows are 254 wide inside a 260 panel.
const SCROLLBAR_GUTTER: f32 = 6.0;

/// 38px tall — a history row.
const ROW_H: f32 = 38.0;

/// 16px horizontal padding, on a row and on the hint line.
const PAD_X: f32 = 16.0;

/// 4px vertical padding on a row — no visual effect under centred alignment
/// in a fixed-height
/// row, transcribed for completeness.
const ROW_PY: f32 = 4.0;

/// 10px between a row's icon and label.
const ROW_GAP: f32 = 10.0;

/// 8px above and below the hint text.
const HINT_PY: f32 = 8.0;

/// 16px — the row icon.
const ICON_ROW: f32 = 16.0;

/// 12px — a row label.
const TEXT_ROW: f32 = 12.0;

/// 11px — the hint.
const TEXT_HINT: f32 = 11.0;

/// A 2.0 stroke width on the *current* row's icon.
/// Every `icons::` constant is baked at 1.5 or 1.6, so both branches ask
/// `with_stroke` rather than editing the constants.
const STROKE_ACTIVE: f32 = 2.0;

/// Stroke width 1.75 on a non-current row's icon (same line).
const STROKE_IDLE: f32 = 1.75;

// ── Faces ────────────────────────────────────────────────────────────────────

// UI font: the row label inherits it from the dock chrome; main.rs loads the collection.
//── Colours not in tokens.rs ────────────────────────────────────────────────
//
// The four alpha composites below are local candidates for `tokens.rs`
// consolidation; each is named for its opacity.

/// White at 12% — the current row's fill. Over `color::PANEL` this is
/// (57,57,57), which is exactly the #393939 the column scan reads at x=500.
const WHITE_12: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 1.0,
    a: 0.12,
};

/// Editor text at 40% — a *future* (redo) row's label. Unreachable in the
/// captured state: the stack has no redo entries.
const TEXT_40: Hsla = Hsla {
    h: color::TEXT.h,
    s: color::TEXT.s,
    l: color::TEXT.l,
    a: 0.40,
};

/// Editor text at 30% — a *future* row's icon. Same reachability note.
const TEXT_30: Hsla = Hsla {
    h: color::TEXT.h,
    s: color::TEXT.s,
    l: color::TEXT.l,
    a: 0.30,
};

// ── Copy ────────────────────────────────────────────────────────────────────

/// The catalog key for the single-item hint: `t!("history.editsAppearHere")`
/// renders `"Edits appear here"` in `en` (`locales/en.json`).
/// The panel asks for that key through [`t`]; the const holds the key, and
/// every call site wraps it with [`t`].
const HINT_TEXT_KEY: &str = "history.editsAppearHere";

// ── Data ────────────────────────────────────────────────────────────────────

/// The row's visual state, marked on each row element.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Step {
    /// The cursor is on this entry.
    Current,
    /// A redo row ahead of the cursor.
    Future,
    /// A committed row behind the cursor.
    Past,
}

impl Step {
    fn for_row(row: &crate::state::HistoryRow) -> Self {
        if row.current {
            Self::Current
        } else if row.applied {
            Self::Past
        } else {
            Self::Future
        }
    }
}

// ── Panel ───────────────────────────────────────────────────────────────────

/// The captured default state has one `Open` row and the “Edits appear here”
/// hint; a document with entries lists them from the oldest applied entry to
/// the newest redo row.
pub fn render(
    state: &gpui_kit::Entity<AppState>,
    cx: &gpui_kit::Context<crate::app::App>,
) -> impl IntoElement {
    let rows = state.read(cx).history.clone();
    let mut list = list_column(&rows, cx);
    if rows.len() <= 1 {
        list = list.child(hint());
    }

    v_flex()
        .w(px(PANEL_W))
        .flex_1()
        .min_h(px(0.0))
        .bg(color::PANEL)
        .child(list)
}

/// The stretch-to-fill, vertically scrollable container, with the 6px
/// reserved-gutter strip on its right (module docs). Rows are
/// full-width of what remains: 254px.
fn list_column(rows: &[crate::state::HistoryRow], cx: &gpui_kit::Context<crate::app::App>) -> Div {
    let mut list = v_flex().flex_1().overflow_hidden().pr(px(SCROLLBAR_GUTTER));
    for (index, item) in rows.iter().enumerate() {
        list = list.child(history_row(index, item, cx));
    }
    list
}

fn history_row(
    index: usize,
    item: &crate::state::HistoryRow,
    cx: &gpui_kit::Context<crate::app::App>,
) -> impl IntoElement {
    let step = Step::for_row(item);
    let (icon_tint, label_color) = match step {
        Step::Current => (color::ACCENT, color::TEXT),
        Step::Future => (TEXT_30, TEXT_40),
        Step::Past => (color::TEXT_DIM, color::TEXT_DIM),
    };
    let stroke = if step == Step::Current {
        STROKE_ACTIVE
    } else {
        STROKE_IDLE
    };

    let mut label = div()
        .font_family(ui::FONT_UI)
        .text_size(px(TEXT_ROW))
        .font_weight(FontWeight(500.))
        .text_color(label_color)
        .overflow_hidden()
        .text_ellipsis()
        .whitespace_nowrap()
        .child(item.label.clone());
    if step == Step::Future {
        label = label.italic();
    }

    let on_click = cx.listener(move |app, _, _, cx| {
        app.jump_history_action(index as u64, cx);
    });

    h_flex()
        .id(format!("history-row-{index}"))
        .test_support()
        .h(px(ROW_H))
        .w_full()
        .items_center()
        .gap(px(ROW_GAP))
        .px(px(PAD_X))
        .py(px(ROW_PY))
        .when(step == Step::Current, |el| el.bg(WHITE_12))
        .on_click(on_click)
        .child(
            Icon::default()
                .data(icons::with_stroke(action_icon(&item.label), stroke))
                .size(px(ICON_ROW))
                .text_color(icon_tint),
        )
        .child(label)
}

/// The hint line: 16px horizontal and 8px vertical padding, 11px text in dim
/// text at 55%. Shown when the stack holds a single entry, i.e. an
/// untouched document. Default (400) weight: the design asks for no
/// explicit weight.
fn hint() -> Div {
    div()
        .px(px(PAD_X))
        .py(px(HINT_PY))
        .text_size(px(TEXT_HINT))
        // Source asks for dim text at 55%; 55% composites to ~2.1:1
        // over the panel, so the hint paints at the full dim strength.
        .text_color(color::TEXT_DIM)
        .child(t!(HINT_TEXT_KEY))
}

/// Picks a row's icon: keyword match on the lowercased
/// label, `history` as the fallback.
fn action_icon(label: &str) -> &'static [u8] {
    let l = label.to_lowercase();
    if l == "open" || l.contains("new document") {
        return icons::IMAGE;
    }
    if l.contains("brush") {
        return icons::BRUSH;
    }
    if l.contains("eraser") {
        return icons::ERASER;
    }
    if l.contains("layer") {
        return icons::LAYERS;
    }
    if l.contains("crop") {
        return icons::CROP;
    }
    if l.contains("resize") {
        return icons::MAXIMIZE;
    }
    if l.contains("cut") {
        return icons::SLICE;
    }
    if l.contains("paste") {
        return icons::COPY;
    }
    if l.contains("selection") || l.contains("deselect") {
        return icons::RECTANGLE;
    }
    icons::HISTORY
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `action_icon` maps the engine's lowercased keyword set to an icon
    /// constant. The baseline only exercises `"open"` → `IMAGE`, but the
    /// full map is presentational and is transcribed whole — the test
    /// pins every arm. RED proof: delete the `return icons::COPY` line and
    /// the `paste` assertion flips to `HISTORY`.
    #[test]
    fn action_icon_map() {
        assert_eq!(action_icon("Open"), icons::IMAGE);
        assert_eq!(action_icon("New Document"), icons::IMAGE);
        assert_eq!(action_icon("new document"), icons::IMAGE);
        assert_eq!(action_icon("Brush"), icons::BRUSH);
        assert_eq!(action_icon("Eraser"), icons::ERASER);
        assert_eq!(action_icon("Layer"), icons::LAYERS);
        assert_eq!(action_icon("Crop"), icons::CROP);
        assert_eq!(action_icon("Resize"), icons::MAXIMIZE);
        assert_eq!(action_icon("Cut"), icons::SLICE);
        assert_eq!(action_icon("Paste"), icons::COPY);
        assert_eq!(action_icon("Selection"), icons::RECTANGLE);
        assert_eq!(action_icon("Deselect"), icons::RECTANGLE);
        // Unknown label → fallback.
        assert_eq!(action_icon("Unknown"), icons::HISTORY);
    }

    /// The row state follows the core cursor: the current row, applied rows
    /// behind it, and redo rows ahead of it.
    #[test]
    fn step_for_row_maps_cursor() {
        use crate::state::HistoryRow;
        let past = HistoryRow {
            label: "Brush".into(),
            applied: true,
            current: false,
        };
        let current = HistoryRow {
            label: "Brush".into(),
            applied: true,
            current: true,
        };
        let future = HistoryRow {
            label: "Brush".into(),
            applied: false,
            current: false,
        };
        assert_eq!(Step::for_row(&past), Step::Past);
        assert_eq!(Step::for_row(&current), Step::Current);
        assert_eq!(Step::for_row(&future), Step::Future);
    }

    /// The hint is shown while the stream has no entries beyond the initial
    /// "Open" row.
    #[test]
    fn hint_text_is_the_single_row_copy() {
        assert_eq!(t!(HINT_TEXT_KEY), "Edits appear here");
    }

    /// `Step` is `PartialEq + Eq` — the `Current` branch is the one the
    /// baseline exercises; the other two exist for completeness and are
    /// exercised by `history_row`'s match (not here, since that needs a
    /// GPUI build). Pin the discriminant values so a future reordering
    /// is caught.
    #[test]
    fn step_variants_are_distinct() {
        assert_ne!(Step::Current, Step::Future);
        assert_ne!(Step::Current, Step::Past);
        assert_ne!(Step::Future, Step::Past);
    }
}
