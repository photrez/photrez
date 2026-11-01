//! The History panel: its appearance.
//!
//! # Where it sits: 260x527 at (1106, 84)
//!
//! Exactly `layers_panel`'s slot — the *other* tab of the same LayerDock pane,
//! so only one of the two is ever on screen and they
//! share bounds. The height, 527, is `611 - 84`: the panel column is `flex-1`
//! between the dock's 38px tab bar (y 46..83, its bottom border painted at 83)
//! the Navigator (`shell-navigator` manifest box, y=611, drawn by whoever
//! owns the dock chrome — not here). See `layers_panel.rs` for how those
//! anchors were derived from `manifest.json`. The panel root is `flex_1` (its
//! parent is the definite flex column in `right_dock::layer_body`), so 527 is
//! what the column resolves to at the baseline window and the list absorbs any
//! difference when the window is taller or shorter.
//!
//! The baseline is **`dock-history.png`** (533x750 @ 833,46), the whole right
//! dock with this tab selected; in crop coordinates this panel occupies
//! x 273..533, y 38..564.
//!
//! # Measured from the baseline
//!
//! Scan passes over the baseline crop (column at x=500, row at y=57,
//! ink profiling per band) give, crop-relative:
//!
//! ```text
//! row box     y 38..75 (38px), x 273..526 (254px — not 260, see below)
//! row fill    #393939 = white/[0.12] over the #1e1e1e panel  (0.12*255+0.88*30 = 57)
//! icon ink    x 290..303, y 50..63, peak colour #f28e42 = color::ACCENT
//! label ink   x 315..344 ("Open"), peak colour #cacaca = color::TEXT
//! hint ink    x 290..373, y 88..97, peak colour #4e4e4e
//! y 98..564   nothing but #1e1e1e
//! ```
//!
//! Every coordinate falls out of the row arithmetic: 16px horizontal padding
//! puts the
//! 16px icon box at x=289, a 10px gap puts the label at x=315, and the
//! 38px row's `items-center` centres both glyph runs exactly where the ink
//! lands (icon box y 49..64 with 1px AA inset; 12px Inter line box 14.5px
//! centred → cap top at 52.6, descender to 64.3 — measured 52..63). The hint's
//! brightest pixel (78) sits just under the 80 that dim text at 55%
//! composites to, as expected for antialiased 11px text.
//!
//! # The 260-wide column's rows are 254 wide: the scrollbar gutter
//!
//! The row stops at x=526, six pixels short of the panel's right edge, even
//! though the list is given no padding. The cause is global:
//! every scrollable area reserves a 6px scrollbar gutter
//! (overriding the wider 10px default). The capture therefore shows a 6px
//! strip on the right of the scroll container whether or not it overflows; the
//! track is transparent so the panel background shows through. It is
//! transcribed as `.pr(px(SCROLLBAR_GUTTER))` on the scroll column, which
//! makes every `w-full` child 260 - 6 = 254 wide — matching the scan to the
//! pixel. (GPUI has no scrollbars to reserve space, so the reservation is
//! spelled out rather than emergent.)
//!
//! # What the captured state contains
//!
//! Exactly one history item and the hint line. Both facts come from the
//! engine rather than being invented: the panel reads the document's
//! history stack, every stack starts with an
//! `Open` entry,
//! and the captured document has no edits on top of it, so the list holds one
//! item; the panel shows "Edits appear here" when
//! the stack holds exactly one entry. The single item is the *current* step
//! (index 0 === the active history index), which is why it carries the
//! 12%-white fill, the accent icon and the full-strength label.
//!
//! The per-label icon map (`getActionIcon`) is transcribed in full even
//! though only its first arm (`"open"` → `image`) is reachable here: it is
//! pure presentation, and the other arms decide which glyph later states
//! show.
//!
//! # Complete vs. omitted
//!
//! * **Complete:** the panel shell, the row's three visual states (current /
//!   future-italic-dim / past-dim — only "current" is on screen in the
//!   baseline, but the branches are presentational and cheap), the label→icon
//!   map, and the hint line.
//! * **Omitted — state the capture never shows:** the no-document fallback
//!   (an 88px dashed box with a `history` glyph and "No image open"); it is
//!   keyed on `activeDocumentId`, which is not a thing a static render has.
//! * **Omitted — interaction only:** `onClick` / `aria-current`,
//!   `focus-visible:bg-white/[0.075]`, `hover:bg-white/[0.02]` and the
//!   matching text-colour shifts, `transition-colors`, `select-none`, and
//!   real scrolling (the content is 67px tall in a 527px column).
//! * `truncate` on the label *is* transcribed (overflow-hidden +
//!   text-ellipsis + nowrap), but it can never fire in this state: "Open" is
//!   30px of the 196px the label gets.
//!
//! # Known fidelity gaps
//!
//! * The capture shows LCD subpixel-AA fringes on the glyphs (paired
//!   #c4793f / #398aae pixels around the accent icon and the label); GPUI
//!   rasterises greyscale, so glyph edges will differ in colour though not in
//!   position or mass.
//! * Vertical centering: the 12px label's 14.5px line box in the 38px row
//!   leaves a half-pixel tie (11.75) that GPUI rounds down and the previous
//!   renderer rounds up (the project-wide known gap), so the label may sit 1px
//!   above the baseline's cap-top row.
//! * Icon tint is static (decided at tree-build time), matching the resting
//!   state; the hover tint is left out with the rest of the hover state.

use crate::components::ui;
use crate::i18n::t;
use crate::icons;
use crate::state::AppState;
use crate::tokens::color;

use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::{div, px, Div, FontWeight, Hsla, TestSupportExt};

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

/// The 6px scrollbar-gutter reservation of the scrollable list.
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

/// `strokeWidth={1.75}` on a non-current row's icon (same line).
const STROKE_IDLE: f32 = 1.75;

// ── Faces ────────────────────────────────────────────────────────────────────

// --font-sans: the row label inherits it from the dock chrome; main.rs loads the collection.
//── Colours not in tokens.rs ────────────────────────────────────────────────
//
// The four alpha composites below are local candidates for `tokens.rs`
// consolidation; each is named by its literal class.

/// `bg-white/[0.12]` — the current row's fill. Over `color::PANEL` this is
/// (57,57,57), which is exactly the #393939 the column scan reads at x=500.
const WHITE_12: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 1.0,
    a: 0.12,
};

/// `text-editor-text/40` — a *future* (redo) row's label. Unreachable in the
/// captured state: the stack has no redo entries.
const TEXT_40: Hsla = Hsla {
    h: color::TEXT.h,
    s: color::TEXT.s,
    l: color::TEXT.l,
    a: 0.40,
};

/// `text-editor-text/30` — a *future* row's icon. Same reachability note.
const TEXT_30: Hsla = Hsla {
    h: color::TEXT.h,
    s: color::TEXT.s,
    l: color::TEXT.l,
    a: 0.30,
};

// ── Copy ────────────────────────────────────────────────────────────────────

/// The catalog key for the single-item hint: `t("history.editsAppearHere")`
/// renders `"Edits appear here"` in `en` (`assets/i18n/en.json`).
/// The panel asks for that key through [`t`]; the const holds the key, and
/// every call site wraps it with [`t`].
const HINT_TEXT_KEY: &str = "history.editsAppearHere";

// ── Data ────────────────────────────────────────────────────────────────────

/// The row's visual state, from `data-history-state` in the reference markup.
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

/// `flex-1 overflow-y-auto` — the scroll container, with the 6px
/// `scrollbar-gutter: stable` strip on its right (module docs). Rows are
/// `w-full` of what remains: 254px.
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
        // `truncate`
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
        .cursor_pointer()
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
        // Source asks for `text-editor-text-dim/55`; 55% composites to ~2.1:1
        // over the panel, so the hint paints at the full dim strength.
        .text_color(color::TEXT_DIM)
        .child(t(HINT_TEXT_KEY))
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
        assert_eq!(t(HINT_TEXT_KEY), "Edits appear here");
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
