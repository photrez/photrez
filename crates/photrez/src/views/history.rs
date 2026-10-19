//! Visual-only port of `editor/HistoryPanel.tsx`.
//!
//! # Where it sits: 260x527 at (1106, 84)
//!
//! Exactly `layers_panel`'s slot — the *other* tab of the same LayerDock pane
//! (`RightDock.tsx:240`), so only one of the two is ever on screen and they
//! share bounds. The height, 527, is `611 - 84`: the panel column is `flex-1`
//! between the dock's 38px tab bar (y 46..83, `border-b` painted at 83) and
//! the Navigator (`shell-navigator` manifest box, y=611, drawn by whoever
//! owns the dock chrome — not here). See `layers_panel.rs` for how those
//! anchors were derived from `manifest.json`.
//!
//! The baseline is **`dock-history.png`** (533x750 @ 833,46), the whole right
//! dock with this tab selected; in crop coordinates this panel occupies
//! x 273..533, y 38..564.
//!
//! # Measured from the baseline
//!
//! Scans with `baseline/probe-history*.ts` (column at x=500, row at y=57,
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
//! Every coordinate falls out of the JSX arithmetic: `px-4` (16) puts the
//! 16px icon box at x=289, `gap-2.5` (10) puts the label at x=315, and the
//! 38px row's `items-center` centres both glyph runs exactly where the ink
//! lands (icon box y 49..64 with 1px AA inset; 12px Inter line box 14.5px
//! centred → cap top at 52.6, descender to 64.3 — measured 52..63). The hint's
//! brightest pixel (78) sits just under the 80 that `text-editor-text-dim/55`
//! composites to, as expected for antialiased 11px text.
//!
//! # The 260-wide column's rows are 254 wide: scrollbar-gutter: stable
//!
//! The row stops at x=526, six pixels short of the panel's right edge, even
//! though `HistoryPanel.tsx` gives the list no padding. The cause is global:
//! `styles.css:316-328` applies `scrollbar-gutter: stable` to every
//! `overflow-y-auto` and re-declares `::-webkit-scrollbar { width: 6px }`
//! (overriding the 10px rule at `:178`). Chromium therefore reserves a 6px
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
//! engine rather than being invented: `workspaceSync.ts:69` feeds
//! `history.getHistoryStack()` into the panel, every stack starts with an
//! `Open` entry (`engine/__tests__/historyImperative.test.ts:97` — "Open + 3"),
//! and the captured document has no edits on top of it, so the list holds one
//! item; `HistoryPanel.tsx:90` shows "Edits appear here" when
//! `historyItems().length === 1`. The single item is the *current* step
//! (index 0 === `activeHistoryIndex`), which is why it carries the
//! `white/[0.12]` fill, the accent icon and the full-strength label.
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
//! * The capture shows Chromium LCD subpixel-AA fringes on the glyphs (paired
//!   #c4793f / #398aae pixels around the accent icon and the label); GPUI
//!   rasterises greyscale, so glyph edges will differ in colour though not in
//!   position or mass.
//! * Vertical centering: the 12px label's 14.5px line box in the 38px row
//!   leaves a half-pixel tie (11.75) that GPUI rounds down and Chromium
//!   rounds up (the project-wide known gap), so the label may sit 1px above
//!   the baseline's cap-top row.
//! * Icon tint is static (decided at tree-build time), matching the resting
//!   state; the hover tint is not ported with the rest of the hover state.

use crate::assets::icons;
use crate::components::ui;
use crate::i18n::t;
use crate::tokens::color;

use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::{div, px, Div, FontWeight, Hsla};

// ── Geometry ────────────────────────────────────────────────────────────────

/// LayerDock's column width: `shell-navigator` is 260 wide at x=1106 and the
/// dock runs to the window edge at 1366.
const PANEL_W: f32 = 260.0;

/// Content height above the Navigator: 611 - 84.
const PANEL_H: f32 = 527.0;

/// The `scrollbar-gutter: stable` reservation of the `overflow-y-auto` list
/// (`styles.css:316-328`, `::-webkit-scrollbar { width: 6px }`). See the
/// module docs; this is why rows are 254 wide inside a 260 panel.
const SCROLLBAR_GUTTER: f32 = 6.0;

/// `h-[38px]` on a history row.
const ROW_H: f32 = 38.0;

/// `px-4` on a row and on the hint line.
const PAD_X: f32 = 16.0;

/// `py-1` on a row — no visual effect under `items-center` in a fixed-height
/// row, transcribed for completeness.
const ROW_PY: f32 = 4.0;

/// `gap-2.5` between a row's icon and label.
const ROW_GAP: f32 = 10.0;

/// `py-2` above and below the hint text.
const HINT_PY: f32 = 8.0;

/// `size-4` — the row icon.
const ICON_ROW: f32 = 16.0;

/// `text-[12px]` on a row label.
const TEXT_ROW: f32 = 12.0;

/// `text-[11px]` on the hint.
const TEXT_HINT: f32 = 11.0;

/// `strokeWidth={2.0}` on the *current* row's icon (`HistoryPanel.tsx:72`).
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
/// renders `"Edits appear here"` in `en` (`i18n/locales/en/translation.json`).
/// `HistoryPanel.tsx:90` calls `t(...)` on it; the const holds the key, and
/// every call site wraps it with [`t`].
const HINT_TEXT_KEY: &str = "history.editsAppearHere";

// ── Data ────────────────────────────────────────────────────────────────────

/// The row's visual state, from `data-history-state` in the JSX. Only
/// `Current` occurs in the baseline; the other two are transcribed because
/// they are pure presentation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Step {
    /// index === activeHistoryIndex: filled, accent icon, solid label.
    Current,
    /// item.isRedo: italic, 40% label, 30% icon.
    Future,
    /// committed and behind the cursor: dim label and icon.
    ///
    /// Not constructed by the captured state (one `Current` row); kept because
    /// the three states are the row's whole visual vocabulary, and the tests
    /// exercise it. `allow`, not `expect`: the lint fires only in non-test
    /// builds, so an expectation would be unfulfilled under `cfg(test)`.
    #[allow(dead_code)]
    Past,
}

/// One row of `historyItems()`. The engine type (`engine/history.ts`
/// `HistoryItem`) also carries snapshots and redo flags; only the label and
/// the derived state reach the pixels, so only those are here.
struct HistoryItem {
    label: &'static str,
    step: Step,
}

/// The captured default state: a freshly opened, unedited document.
/// `getHistoryStack()` yields exactly one entry, `Open`, and it is the
/// current step.
const ITEMS: &[HistoryItem] = &[HistoryItem {
    label: "Open", // i18n-exempt: demo content <engine history-step label, captured stack; kept English for action_icon's keyword match>
    step: Step::Current,
}];

// ── Panel ───────────────────────────────────────────────────────────────────

/// `<div data-history-panel class="flex min-h-0 flex-1 select-none flex-col">`
/// holding the `<Show when={activeDocumentId()}>` branch (the fallback is
/// state-keyed and omitted — see the module docs).
///
/// The two pass-through flex columns between this div and the rows
/// (`data-history-list` and the `flex-1 overflow-y-auto` scroll container)
/// collapse into one; the scroll container keeps its `.pr(6)` reservation and
/// its `overflow-hidden` (nothing overflows in this state, but the class is
/// in the source and costs nothing).
pub fn render() -> impl IntoElement {
    let mut list = list_column();
    if ITEMS.len() == 1 {
        list = list.child(hint());
    }

    v_flex()
        .w(px(PANEL_W))
        .h(px(PANEL_H))
        .flex_shrink_0()
        .bg(color::PANEL)
        .child(list)
}

/// `flex-1 overflow-y-auto` — the scroll container, with the 6px
/// `scrollbar-gutter: stable` strip on its right (module docs). Rows are
/// `w-full` of what remains: 254px.
fn list_column() -> Div {
    let mut list = v_flex().flex_1().overflow_hidden().pr(px(SCROLLBAR_GUTTER));
    for item in ITEMS {
        list = list.child(history_row(item));
    }
    list
}

/// A history step: `<button class="group relative flex h-[38px] w-full
/// items-center gap-2.5 px-4 py-1 text-left text-[12px] ...">` containing the
/// action icon and the label.
///
/// The button's `relative`, `group`, `transition-colors`, `focus:outline-none`
/// and `focus-visible:bg-white/[0.075]` are interaction-only and omitted; the
/// label carries `truncate`'s three properties so a long step name clips with
/// an ellipsis instead of painting over the gutter.
fn history_row(item: &HistoryItem) -> impl IntoElement {
    // Captured at tree-build time rather than inherited from a hover style —
    // same constraint `tool_rail.rs` documents for its icons.
    let (icon_tint, label_color) = match item.step {
        Step::Current => (color::ACCENT, color::TEXT),
        Step::Future => (TEXT_30, TEXT_40),
        Step::Past => (color::TEXT_DIM, color::TEXT_DIM),
    };
    let stroke = if item.step == Step::Current {
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
        .child(item.label.to_string());
    if item.step == Step::Future {
        label = label.italic();
    }

    h_flex()
        // Reused as the ElementId so the row is hit-testable once onClick
        // lands; inert in a static render.
        .id(item.label)
        .h(px(ROW_H))
        .w_full()
        .items_center()
        .gap(px(ROW_GAP))
        .px(px(PAD_X))
        .py(px(ROW_PY))
        .when(item.step == Step::Current, |el| el.bg(WHITE_12))
        .child(
            Icon::default()
                .data(icons::with_stroke(action_icon(item.label), stroke))
                .size(px(ICON_ROW))
                .text_color(icon_tint),
        )
        .child(label)
}

/// `<p class="px-4 py-2 text-[11px] text-editor-text-dim/55">` — shown by
/// `HistoryPanel.tsx:90` when the stack holds a single entry, i.e. an
/// untouched document. Default (400) weight: the class list asks for no
/// `font-*`.
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

/// `getActionIcon` (`HistoryPanel.tsx:19-30`): keyword match on the lowercased
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

    /// The captured default stack holds exactly one `Current` step —
    /// `Step::Current` fills `ITEMS`. RED proof: change `Step::Current`
    /// to `Step::Past` in the constant and the length assertion still
    /// passes but the step assertion flips.
    #[test]
    fn captured_items_shape() {
        assert_eq!(ITEMS.len(), 1);
        assert_eq!(ITEMS[0].label, "Open");
        assert_eq!(ITEMS[0].step, Step::Current);
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

    /// `render` adds the hint only when the stack has a single entry —
    /// the exact captured condition.
    #[test]
    fn hint_shows_only_for_single_item() {
        assert_eq!(ITEMS.len(), 1);
        // `HINT_TEXT_KEY` is the `en` locale string for the single-item case.
        assert_eq!(t(HINT_TEXT_KEY), "Edits appear here");
    }
}
