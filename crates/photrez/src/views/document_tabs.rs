//! The document tab strip: its appearance.
//!
//! The 38px strip that caps the editor column, directly under the title bar:
//! `manifest.json` records the active tab at `x52,y46,89.86x37`, and the
//! column runs from the tool rail's right edge (x=52) to the right dock
//! (x=833) — 781px wide. Left to right it holds the tab list (`flex-1`), the
//! new-document button, and the utility cluster (layout toggle + Export)
//! pressed against the dock edge.
//!
//! # Scope: the resting state, with exactly one open (and active) document
//!
//! Captured baselines: `shell-doc-tab.png` (the tab alone) and
//! `shell-full-page.png` (the whole strip — sampled pixel by pixel while
//! writing this: strip #141414, tab #171717, every border column #333333,
//! underline #F28E42 across y=81..82, export button 725..816).
//!
//! The tab strip's behaviour cannot appear in a static
//! screenshot and is not reproduced here: drag-to-reorder with drop
//! indicators, cross-document drag dimming, the hover-to-switch timer, the
//! dirty marker, the close- and new-document dialogs, the toasts, the export
//! menu, the tooltip on the layout toggle, and the wheel/keyboard
//! navigation. Hover *styling* is reproduced where it is a plain colour
//! change, as in
//! [`crate::views::tool_rail`] and [`crate::views::title_bar`].
//!
//! With **zero** documents the original still paints this strip — `<For>`
//! simply renders no tab, so the `flex-1` list comes up empty while the `+`
//! and the utility cluster stay where they are. That state is *not*
//! implemented here; this module renders the captured one-document state,
//! because that is what the baseline measured.
//!
//! # Transcription notes
//!
//! * **Every border paints `#333333`**, whatever colour the element asks
//!   for: a global border colour outranks the per-element
//!   divider and field-border values, so the root, tab,
//!   new-document button and utility cluster, and the
//!   export button all resolve to [`color::BORDER`]. Confirmed against
//!   `shell-full-page.png`: (141,60), (400,83), (636,60), (680,60), (725,64)
//!   and (816,64) are unanimous. `color::DIVIDER` (#2e2e2e) appears nowhere in
//!   this component — there is no divider fill on it either.
//! * **Two hover names refer to colours that do not exist.** No shipped
//!   token declares a topbar-hover or app-hover value, so those
//!   hovers paint nothing: both hovers are no-ops in the original and are
//!   recorded as such below rather than invented.
//! * **`Export…` is the resolved translation**, not the `"Export"` fallback
//!   in the catalogue key: `assets/i18n/en.json:51` is `"Export…"` (one
//!   U+2026), which is what the baseline paints.
//! * **The dock-close button is hidden at this width**: the breakpoint is
//!   1024px and the baseline window is 1366px, so it is not shown and
//!   takes no space. The arithmetic agrees with leaving it out: the utility
//!   cluster measures 153px (1 border + 8 left padding + 28 toggle + 8 gap +
//!   92 export +
//!   16 right padding), which puts its left border at exactly x=680, where
//!   `shell-full-page.png` has #333333.
//! * **The layout toggle is not accent-coloured**: the accent only
//!   applies when the dock layout is `"stacked"`, and the
//!   default is `"side-by-side"` — which is
//!   what the baseline shows (the dock panels sit side by side).
//! * **Box model**: GPUI/taffy defaults to border-box sizing, so the
//!   38px strip spends its 1px bottom border inside the 38 and leaves the 37px
//!   content height `manifest.json` measures for the tab. The tab's width is
//!   12 + 10 padding + 1 right border + 8 gap + 16 close button + the advance
//!   width of "Baseline" at 11.5px/500 ≈ 42.86 = 89.86.
//! * **No letter-spacing compensation** (unlike `title_bar::pad_x`): this
//!   strip asks for no tracking, so GPUI's lack of tracking
//!   costs nothing here.
//!
//! # Known fidelity gaps
//!
//! * **Icon hover tint.** `Icon` captures its `text_color` while the element
//!   tree is built, before hover refinement, so the `plus`, `columns` and
//!   tab-close `x` glyphs do not brighten with their buttons — only the
//!   backgrounds and plain text do. Same gap as `tool_rail` and `title_bar`;
//!   no baseline captures a hover of those three controls, so it is unverified
//!   either way.
//! * `cursor: pointer`, `transition-all duration-75`, `select-none`, the ARIA
//!   roles/`tabindex`, and every pointer/drag/wheel handler have no pixel
//!   consequence and are dropped.

// `gpui-kit` re-exports all of GPUI at its root, so every import below goes
// through it; `h_flex` is the free function from `gpui-base`, which builds the
// `flex-row + items-center` row that the JSX's `class="flex items-center"`
// expresses.
use gpui_kit::base::h_flex;
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::{div, px, FontWeight, Point, TestSupportExt, Window};

use crate::components::ui;
use crate::i18n::t;
use crate::icons;
use crate::tokens::color;

use gpui_kit::Div;

// ---------------------------------------------------------------------------
// Dimensions. Tailwind's 16px root: every value below is already resolved to
// pixels, and each const names the class (with its JSX line) it comes from.
// ---------------------------------------------------------------------------

/// 38px — strip height. `manifest.json`
/// gives the active tab as 37px at y=46 and the option bar starts at y=84, so
/// the 38px is border-box: 37 content + a 1px bottom border.
const BAR_H: f32 = 38.0;

/// 8px. Three separate uses in this strip: between a tab's
/// label and its close button, between the utility cluster's
/// children, and inside the export button.
const GAP_2: f32 = 8.0;

/// 12px — active tab's left padding.
const TAB_PAD_L: f32 = 12.0;

/// 10px — active tab's right padding.
const TAB_PAD_R: f32 = 10.0;

/// 11.5px — tab label.
const TAB_TEXT: f32 = 11.5;

/// 16px — the tab's close button.
const CLOSE_BTN: f32 = 16.0;

/// 14px — the `x` glyph inside it.
const CLOSE_ICON: f32 = 14.0;

/// 4px corners on the close button. Irrelevant to any pixel: the button's
/// only background is
/// the hover fill that paints nothing at all (see the module
/// docs), so no corner is ever painted.
const CLOSE_RADIUS: f32 = 4.0;

/// 2px — active-tab underline.
const UNDERLINE_H: f32 = 2.0;

/// 44px — new-document button width.
const NEW_TAB_W: f32 = 44.0;

/// 18px — its `plus` glyph.
const NEW_TAB_ICON: f32 = 18.0;

/// 8px — utility cluster's left padding.
const CLUSTER_PAD_L: f32 = 8.0;

/// 16px — utility cluster's right padding.
const CLUSTER_PAD_R: f32 = 16.0;

/// 28px — layout toggle button.
const TOGGLE_BTN: f32 = 28.0;

/// 4px corners — layout toggle and export button.
const RADIUS_4: f32 = 4.0;

/// 16px — the layout-toggle glyph.
const TOGGLE_ICON: f32 = 16.0;

/// 28px — export button height.
const EXPORT_H: f32 = 28.0;

/// 12px — export button horizontal padding.
const EXPORT_PX: f32 = 12.0;

/// 12.5px — export button label.
const EXPORT_TEXT: f32 = 12.5;

/// 14px — the export button's chevron glyph.
const EXPORT_ICON: f32 = 14.0;

/// The export label's *resolved* string, not the key's
/// fallback: `assets/i18n/en.json:51` is `"Export…"`.
const EXPORT_LABEL_KEY: &str = "menus.items.export";

/// `<div class="flex h-[38px] shrink-0 items-center border-b
///  border-editor-divider bg-editor-topbar justify-between select-none">`
///
/// `select-none` has no GPUI equivalent and nothing here is selectable; the
/// `data-tab-bar-empty` marker and the drop handlers are behaviour, not
/// pixels. Width is not set: `main.rs` puts this in a `v_flex().items_stretch()`,
/// so it spans the editor column (781px) exactly as the block-level div does
/// in the browser.
pub fn render(
    tabs: &[crate::state::TabRow],
    active_id: Option<&str>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    h_flex()
        .h(px(BAR_H))
        .flex_shrink_0()
        // `h_flex()` already sets items_center; the explicit call documents
        // that every child below is centred on the cross axis unless it opts
        // into `.self_stretch()`.
        .items_center()
        .justify_between()
        .border_b_1()
        // The divider token names #2e2e2e, but the global border colour
        // outranks it and paints #333333 — see `color::BORDER`.
        .border_color(color::BORDER)
        .bg(color::TOPBAR)
        // The whole shell is Inter. GPUI's theme default resolves to Segoe UI
        // here, whose advances differ and would move the measured 89.86px tab
        // width.
        .font_family(ui::FONT_UI)
        // The tab strip is the flexible one (it carries overflow-x-auto in the
        // source); the two controls at the far end are fixed and must never be
        // squeezed, so the strip yields and they do not.
        .child(ui::clip(tab_list(tabs, active_id, cx)))
        .child(new_tab_button())
        .child(ui::pin(cluster()))
}

/// `<div role="tablist" class="flex items-stretch overflow-x-auto
///  tab-scrollbar flex-1 h-full min-w-0">`
///
/// `overflow-x: overlay` + the `tab-scrollbar` rules only style a
/// `::-webkit-scrollbar` that a single tab never produces, so the scroll
/// container is dropped rather than emulated: a GPUI scroll host would
/// reserve or overlay a scrollbar the web render does not paint here.
fn tab_list(
    tabs: &[crate::state::TabRow],
    active_id: Option<&str>,
    cx: &Context<crate::app::App>,
) -> Div {
    h_flex()
        // `h-full` on a centred row: stretch instead of hug, so the list is
        // the strip's full 37px content height.
        .self_stretch()
        // `items-stretch` is explicit in the class list and is what gives
        // each tab that same height — `h_flex()` would otherwise centre them.
        .items_stretch()
        .flex_1()
        .min_w(px(0.))
        .children(tabs.iter().map(|tab_row| {
            let active = active_id == Some(tab_row.doc_id.as_str());
            tab(tab_row, active, cx)
        }))
}

/// `<div role="tab" ... class="group relative flex shrink-0 items-center
///  gap-2 border-r border-editor-divider pl-3 pr-2.5 cursor-pointer select-none
///  transition-all duration-75" ... bg-editor-bg">`
///
/// Only the **active** tab is rendered: the baseline captured the
/// one-document state, and with one open document the `<For>` yields exactly
/// one row. The `group` marker is dropped because nothing inside the tab
/// consumes a group-hover rule (the close button uses a plain hover,
/// which is independent of the ancestor).
///
/// The inactive branch would be `bg-editor-topbar` — the strip colour, i.e.
/// invisible against it — plus the topbar hover fill, which is a
/// class with no backing token and therefore paints nothing at all.
///
/// Not reproduced, because a resting screenshot cannot show them:
/// the drag-over outline, the dimmed
/// (30% opacity), and reordering states (60% opacity, a 0.98 scale and a ring)
/// and the two `before:`/`after:` drop indicators.
fn tab(
    tab_row: &crate::state::TabRow,
    active: bool,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let doc_id = tab_row.doc_id.clone();
    let click_id = tab_row.doc_id.clone();
    let close_id = tab_row.doc_id.clone();
    let drop_id = tab_row.doc_id.clone();
    let ghost_name = tab_row.name.clone();
    h_flex()
        // `relative` — the containing block for the active underline, whose
        // The underline anchors to this tab's *padding* box.
        .relative()
        .id(format!("document-tab-{}", tab_row.doc_id))
        .test_support()
        .aria_selected(active)
        .aria_label(tab_row.name.clone())
        .flex_shrink_0()
        .items_center()
        .gap(px(GAP_2))
        .pl(px(TAB_PAD_L))
        .pr(px(TAB_PAD_R))
        .border_r_1()
        .border_color(color::BORDER)
        // `bg-editor-bg` is the active branch; inactive tabs sit on the strip
        // colour, which is what `bg-editor-topbar` resolves to.
        .when(active, |el| el.bg(color::BG))
        .cursor_pointer()
        .on_click(
            cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                let _ = app.activate_document(&click_id, cx);
            }),
        )
        .on_drag(
            doc_id.clone(),
            move |_: &String, _: Point<gpui_kit::Pixels>, _: &mut Window, cx| {
                cx.new(|_| TabDragGhost {
                    name: ghost_name.clone(),
                })
            },
        )
        .on_drop::<String>(cx.listener(
            move |app, from: &String, _window, cx: &mut Context<crate::app::App>| {
                app.move_document_tab(from, &drop_id, cx);
            },
        ))
        .on_drop::<crate::app::cross_doc::LayerDragPayload>({
            let target_doc = tab_row.doc_id.clone();
            cx.listener(
                move |app,
                      payload: &crate::app::cross_doc::LayerDragPayload,
                      window,
                      cx: &mut Context<crate::app::App>| {
                    app.apply_layer_drag(
                        payload,
                        crate::app::cross_doc::LayerDropTarget::Tab {
                            doc_id: target_doc.clone(),
                        },
                        window.modifiers().alt,
                        cx,
                    );
                },
            )
        })
        .child(tab_label(&tab_row.name, tab_row.dirty))
        .child(close_tab_button(close_id, cx))
        .when(active, |el| el.child(active_underline()))
}

/// `<span class="whitespace-nowrap text-[11.5px] ... text-editor-text
///  font-medium">` — the active, non-dirty branch of the clsx chain
/// The dirty branch (accent text plus a `•`) is not
/// rendered: a freshly captured blank document is clean.
fn tab_label(name: &str, dirty: bool) -> impl IntoElement {
    div()
        .whitespace_nowrap()
        .text_size(px(TAB_TEXT))
        // `font-medium`
        .font_weight(FontWeight(500.))
        .text_color(if dirty { color::ACCENT } else { color::TEXT })
        .child(if dirty {
            format!("\u{2022} {name}")
        } else {
            name.to_owned()
        })
}

/// `<button class="flex size-4 items-center justify-center rounded
///  text-editor-text-dim hover:text-editor-text hover:bg-editor-app-hover">`
/// with `<Icon name="x" class="size-3.5" strokeWidth={1.75} />`.
///
/// The close button's hover fill is omitted deliberately: there is no
/// `--editor-app-hover` token anywhere in the app, so Tailwind generates no
/// rule for it and the original's hover background is transparent too.
fn close_tab_button(doc_id: String, cx: &Context<crate::app::App>) -> impl IntoElement {
    h_flex()
        // The id is what makes the button hit-testable, and therefore what
        // makes `.hover()` fire.
        .id(format!("close-tab-{doc_id}"))
        .test_support()
        .size(px(CLOSE_BTN))
        .items_center()
        .justify_center()
        .rounded(px(CLOSE_RADIUS))
        .cursor_pointer()
        .hover(|s| s.text_color(color::TEXT))
        .on_click(
            cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                cx.stop_propagation();
                app.close_document_tab(&doc_id, cx);
            }),
        )
        .child(
            Icon::default()
                .data(icons::X)
                .size(px(CLOSE_ICON))
                // Baked at a 1.75 stroke width, matching icons.rs.
                // The tint is fixed here rather than inherited, so the glyph
                // does not follow the button's hover colour — see the module
                // docs.
                .text_color(color::TEXT_DIM),
        )
}

/// `<span class="absolute inset-x-0 bottom-0 h-[2px] bg-editor-accent" />`
///
/// The underline spans the padding box, so the orange stops at x=141 and
/// the tab's 1px right border column stays #333333 — which is what
/// `shell-full-page.png` shows: (52..140, 81) is #F28E42, (141, 81) is
/// #333333. Taffy offsets absolute children by the containing block's border
/// (`flexbox.rs:2409`), so left/right:0 land on the padding edge here too.
fn active_underline() -> impl IntoElement {
    div()
        .absolute()
        .left(px(0.))
        .right(px(0.))
        .bottom(px(0.))
        .h(px(UNDERLINE_H))
        .bg(color::ACCENT)
}

/// `<button class="flex w-11 shrink-0 items-center justify-center
///  text-editor-icon hover:text-editor-text border-l border-editor-divider
///  h-full">` with `<Icon name="plus" class="size-[18px]"
///  strokeWidth={1.75} />`.
fn new_tab_button() -> impl IntoElement {
    h_flex()
        .id("new-document")
        .w(px(NEW_TAB_W))
        // `h-full`
        .self_stretch()
        // Pin the whole button rather than wrapping it in ui::pin(): it carries
        // an explicit width and an .id(), so its type is Stateful<Div> and the
        // generic helper cannot be applied from outside. flex_none() also
        // stops it picking up spare width in the strip.
        .flex_none()
        .items_center()
        .justify_center()
        .border_l_1()
        .border_color(color::BORDER)
        .hover(|s| s.text_color(color::TEXT))
        .child(
            Icon::default()
                .data(icons::PLUS)
                .size(px(NEW_TAB_ICON))
                .text_color(color::ICON),
        )
}

/// `<div class="flex items-center gap-2 pr-4 pl-2 shrink-0 border-l
///  border-editor-divider h-full">` — layout toggle plus export.
///
/// The third child of this row, `CloseDockButton`, is `lg:hidden` and so is
/// `display:none` at the 1366px baseline; it is absent rather than omitted.
/// See the module docs for the 153px arithmetic that confirms it.
fn cluster() -> Div {
    h_flex()
        // `h-full`
        .self_stretch()
        .flex_shrink_0()
        .items_center()
        .gap(px(GAP_2))
        .pl(px(CLUSTER_PAD_L))
        .pr(px(CLUSTER_PAD_R))
        .border_l_1()
        .border_color(color::BORDER)
        .child(layout_toggle())
        .child(export_button())
}

/// `<button class="flex size-7 items-center justify-center rounded-[4px]
///  text-editor-icon hover:bg-white/[0.045] hover:text-editor-text">` wrapping
/// `<Icon name="columns" class="size-4" strokeWidth={1.75} />`.
///
/// Not accent-coloured because the dock is in its default `side-by-side`
/// layout — see the module docs. The `Tooltip` around it is hover-only and
/// not reproduced.
fn layout_toggle() -> impl IntoElement {
    h_flex()
        .id("layout-toggle")
        .size(px(TOGGLE_BTN))
        .items_center()
        .justify_center()
        .rounded(px(RADIUS_4))
        .hover(|s| s.bg(color::WHITE_45).text_color(color::TEXT))
        .child(
            Icon::default()
                .data(icons::COLUMNS)
                .size(px(TOGGLE_ICON))
                .text_color(color::ICON),
        )
}

/// `<button class="flex h-[28px] shrink-0 items-center gap-2 rounded-[4px]
///  border border-editor-field-border px-3 text-[12.5px] text-editor-text
///  transition-colors hover:bg-white/[0.045] hover:text-editor-text">` with
/// `<Icon name="chevron-down" class="size-3.5 text-editor-text-dim"
///  strokeWidth={1.75} />`.
///
/// The export button's border paints #333333 like every other border, and
/// its hover text colour is a no-op (the label is already
/// full-brightness text), so only the background changes on hover — and the
/// chevron keeps the dim colour in the original too, because it has
/// its own colour class rather than inheriting one.
///
/// Its width is content-driven: 2*12 padding + 2 border + 8 gap + 14 icon +
/// a ~44px advance for "Export…" = 92px, i.e. x=725..816 where
/// `shell-full-page.png` has its two border columns. The `ContextMenu` it
/// opens is not reproduced.
fn export_button() -> impl IntoElement {
    h_flex()
        .id("export-button")
        .h(px(EXPORT_H))
        .flex_shrink_0()
        .items_center()
        .gap(px(GAP_2))
        .rounded(px(RADIUS_4))
        .border_1()
        .border_color(color::BORDER)
        .px(px(EXPORT_PX))
        .text_size(px(EXPORT_TEXT))
        .text_color(color::TEXT)
        .hover(|s| s.bg(color::WHITE_45))
        .child(t(EXPORT_LABEL_KEY))
        .child(
            Icon::default()
                .data(icons::CHEVRON_DOWN)
                .size(px(EXPORT_ICON))
                .text_color(color::TEXT_DIM),
        )
}

/// The chip GPUI paints under the pointer while a tab is dragged.
struct TabDragGhost {
    name: String,
}

impl Render for TabDragGhost {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .h(px(26.0))
            .px(px(10.0))
            .flex()
            .items_center()
            .rounded(px(4.0))
            .bg(color::BG)
            .border_1()
            .border_color(color::ACCENT)
            .text_size(px(TAB_TEXT))
            .text_color(color::TEXT)
            .child(self.name.clone())
    }
}
