//! The document tab strip: its appearance.
//!
//! The 38px strip that caps the editor column, directly under the title bar:
//! at the default client area the active tab occupies `x52,y46,89.86x37`, and the
//! column runs from the tool rail's right edge (x=52) to the right dock
//! (x=833) — 781px wide. Left to right it holds the flexible tab list, the
//! new-document button, and the utility cluster (Export only)
//! pressed against the dock edge.
//!
//! # Scope: the resting state, with exactly one open (and active) document
//!
//! The strip measures: surface #141414, active tab #171717, every border
//! column #333333, underline #F28E42 across y=81..82, export button 725..816.
//!
//! A static screenshot cannot show behaviour, so what stays out of this module
//! is drag-to-reorder with drop indicators, cross-document drag dimming,
//! the hover-to-switch timer, the toasts, the export menu, and the
//! wheel/keyboard navigation. Clicks that have a native command are wired:
//! a tab activates,
//! its close button closes the document, `+` opens the New Document dialog and
//! Export opens the Export dialog. Hover *styling* is reproduced where it is a plain
//! colour change, as in
//! [`crate::views::tool_rail`] and [`crate::views::title_bar`].
//!
//! With **zero** documents this strip can still paint: the tab list renders no
//! tab, so the flexible list comes up empty while the `+` and the utility
//! cluster stay where they are. That state is *not* implemented here; this
//! module renders the captured one-document state, because that is what the
//! baseline measured.
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
//!   hovers paint nothing: both are no-ops here and are
//!   recorded as such below rather than invented.
//! * **`Export…` is the resolved translation**, not the `"Export"` fallback
//!   in the catalogue key: `assets/i18n/en.json:51` is `"Export…"` (one
//!   U+2026), which is what the baseline paints.
//! * **The dock-close button is hidden at this width**: the breakpoint is
//!   1024px and the baseline window is 1366px, so it is not shown and
//!   takes no space. The arithmetic agrees with leaving it out: the utility
//!   cluster measures 117px (1 border + 8 left padding + 92 export +
//!   16 right padding).
//! * **The layout toggle was captured, and is removed.** The baseline paints
//!   it: the cluster measured 153px (1 border + 8 left padding + 28 toggle +
//!   8 gap + 92 export + 16 right padding), which put its left border at
//!   exactly x=680, where `shell-full-page.png` has #333333. That control
//!   switched the right dock between side-by-side and stacked, and tinted the
//!   glyph with the accent only in the stacked layout. This app ships one
//!   dock layout, so the control had no handler: a painted control that answers
//!   no click is removed rather than kept, the same ruling that took the crop
//!   bar's dead control out. Export keeps its measured x=725..816 place; the
//!   cluster's left border moves from x=680 to x=716.
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
//!   tree is built, before hover refinement, so the `plus` and
//!   tab-close `x` glyphs do not brighten with their buttons — only the
//!   backgrounds and plain text do. Same gap as `tool_rail` and `title_bar`;
//!   no measurement covers a hover of those three controls, so it is unverified
//!   either way.
//! * The transition timings, the text-selection lock, the
//!   ARIA roles and tab order have no pixel consequence and are dropped. The
//!   pointer cursor is applied to the controls that work, and the icon-only `+`
//!   carries an accessible name.

// `gpui-kit` re-exports all of GPUI at its root, so every import below goes
// through it; `h_flex` is the free function from `gpui-base`, which builds the
// centred single row used here.
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
// Dimensions. The captured layout sizes in rem on a 16px root, so every value
// below is already resolved to pixels, and each const names the part it
// measures.
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

/// 4px corners — export button.
const RADIUS_4: f32 = 4.0;

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

/// The 38px strip: one centred row that is never squeezed, a 1px bottom
/// border, the topbar fill, and its children pushed to both ends.
///
/// Text selection has no GPUI equivalent and nothing here is selectable; the
/// drag-and-drop handling is behaviour, not pixels. Width is not set: `main.rs`
/// puts this in a
/// `v_flex().items_stretch()`, so it spans the editor column (781px) with no
/// width of its own.
pub fn render(
    state: &gpui_kit::Entity<crate::state::AppState>,
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
        // The tab strip is the flexible one (the recorded strip scrolls
        // sideways); the two controls at the far end are fixed and must never be
        // squeezed, so the strip yields and they do not.
        .child(ui::clip(tab_list(tabs, active_id, cx)))
        .child(new_tab_button(cx))
        .child(ui::pin(cluster(state, cx)))
}

/// The tab list: a row that takes the spare width and the strip's full height,
/// stretches each tab to that height, may shrink below its content, and scrolls
/// sideways; its accessibility role is `tablist`.
///
/// The reference styling styled only a scrollbar
/// that a single tab never produces, so the scroll
/// container is dropped rather than emulated: a GPUI scroll host would
/// reserve or overlay a scrollbar that is not present here.
fn tab_list(
    tabs: &[crate::state::TabRow],
    active_id: Option<&str>,
    cx: &Context<crate::app::App>,
) -> Div {
    h_flex()
        // Full height on a centred row: stretch instead of hug, so the list is
        // the strip's full 37px content height.
        .self_stretch()
        // Stretching the children is what gives
        // each tab that same height — `h_flex()` would otherwise centre them.
        .items_stretch()
        .flex_1()
        .min_w(px(0.))
        .children(tabs.iter().map(|tab_row| {
            let active = active_id == Some(tab_row.doc_id.as_str());
            tab(tab_row, active, cx)
        }))
}

/// A tab: the `tab` accessibility role on a positioned row that is never
/// squeezed and centres its children — 8px between them, a 1px right border,
/// 12px left and 10px right padding, the document background behind the active
/// one, a pointer cursor and a 75ms colour transition.
///
/// Only the **active** tab is rendered: the resting layout is the
/// one-document state, and with one open document the list yields exactly
/// one row. The hover-grouping marker is dropped because nothing inside the tab
/// consumes a descendant hover rule (the close button uses a plain hover,
/// which is independent of the ancestor).
///
/// The inactive branch would be the topbar fill — the strip colour, i.e.
/// invisible against it — plus the topbar hover fill, which names a token the
/// app never declared and therefore paints nothing at all.
///
/// Not reproduced, because the resting layout cannot show them:
/// the drag-over outline, the dimmed
/// (30% opacity), and reordering states (60% opacity, a 0.98 scale and a ring)
/// and the two drop-edge indicators.
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
        // Positioned as the containing block for the active underline, whose
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
        // The document background is the active branch; inactive tabs sit on
        // the strip colour, which is the topbar fill.
        .when(active, |el| el.bg(color::BG))
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

/// The tab label: 11.5px medium-weight text in the plain text colour, kept on
/// one line — the active, non-dirty branch of the class chain
/// The dirty branch (accent text plus a `•`) is not
/// rendered: a freshly captured blank document is clean.
fn tab_label(name: &str, dirty: bool) -> impl IntoElement {
    div()
        .whitespace_nowrap()
        .text_size(px(TAB_TEXT))
        // Medium weight.
        .font_weight(FontWeight(500.))
        .text_color(if dirty { color::ACCENT } else { color::TEXT })
        .child(if dirty {
            format!("\u{2022} {name}")
        } else {
            name.to_owned()
        })
}

/// The close button: a 16px square with 4px corners, centred on one row, in
/// the dim text colour and brightening to the plain text colour on hover,
/// holding the `x` glyph at 14px with a 1.75 stroke.
///
/// The close button's hover fill is omitted deliberately: no app-hover token
/// exists anywhere in the app, so nothing backs it: the hover above only
/// changes the glyph colour, never the fill.
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

/// A 2px accent bar pinned to the tab's bottom edge and stretched from its
/// left edge to its right.
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

/// The new-document button: 44px wide, never squeezed, full strip height,
/// centred on one row, a 1px left border, in the icon colour and brightening to
/// the plain text colour on hover; it holds the `plus` glyph at 18px with a
/// 1.75 stroke.
///
/// Clicking opens the New Document dialog, the same command as File > New.
/// Cancelling an active transform session, and refusing a new document when
/// the workspace is full, have no native counterpart yet.
fn new_tab_button(cx: &Context<crate::app::App>) -> impl IntoElement {
    h_flex()
        .id("new-document")
        .test_support()
        .w(px(NEW_TAB_W))
        // Full strip height.
        .self_stretch()
        // Pin the whole button rather than wrapping it in ui::pin(): it carries
        // an explicit width and an .id(), so its type is Stateful<Div> and the
        // generic helper cannot be applied from outside. flex_none() also
        // stops it picking up spare width in the strip.
        .flex_none()
        .items_center()
        .justify_center()
        .aria_label(t("menus.items.newDocument"))
        .border_l_1()
        .border_color(color::BORDER)
        .hover(|s| s.text_color(color::TEXT))
        .on_click(cx.listener(
            |app, _: &gpui_kit::ClickEvent, _window, cx: &mut Context<crate::app::App>| {
                app.open_new_document(cx);
            },
        ))
        .child(
            Icon::default()
                .data(icons::PLUS)
                .size(px(NEW_TAB_ICON))
                .text_color(color::ICON),
        )
}

/// The utility cluster: centred on one row, 8px left and 16px right padding,
/// never squeezed, a 1px left border and the full strip height. It holds the
/// export button.
///
/// The dock-close button is hidden from the
/// 1024px breakpoint up, so it paints nothing at the 1366px baseline; it is
/// absent rather than omitted.
/// See the module docs for the 117px arithmetic that confirms it, and for why
/// the layout toggle is not mounted here.
fn cluster(state: &gpui_kit::Entity<crate::state::AppState>, cx: &Context<crate::app::App>) -> Div {
    h_flex()
        // Full strip height.
        .self_stretch()
        .flex_shrink_0()
        .items_center()
        .gap(px(GAP_2))
        .pl(px(CLUSTER_PAD_L))
        .pr(px(CLUSTER_PAD_R))
        .border_l_1()
        .border_color(color::BORDER)
        .child(export_button(state, cx))
}

/// The export button: 28px tall, never squeezed, centred on one row with 8px
/// between its children, 4px corners, a 1px field-border, 12px horizontal
/// padding and 12.5px text in the plain text colour, with a colour transition
/// and a hover fill of white at 4.5%. It carries the `chevron-down` glyph at
/// 14px in the dim text colour with a 1.75 stroke.
///
/// The export button's border paints #333333 like every other border, and
/// its hover text colour is a no-op (the label is already
/// full-brightness text), so only the background changes on hover. The
/// chevron keeps the dim colour here too, because it sets
/// its own colour rather than inheriting one.
///
/// Its width is content-driven: 2*12 padding + 2 border + 8 gap + 14 icon +
/// a ~44px advance for "Export…" = 92px, i.e. x=725..816 where
/// `shell-full-page.png` has its two border columns.
///
/// Clicking opens the floating export menu: quick exports that preset the
/// dialog's format, and Export As... which opens it as-is. The save path
/// itself stays an approved, user-picked target.
fn export_button(
    state: &gpui_kit::Entity<crate::state::AppState>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let open = state.read(cx).open_select == Some("tab-export");
    let mut button = h_flex()
        .id("export-button")
        .test_support()
        .relative()
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
        .on_click(cx.listener(
            |app, _: &gpui_kit::ClickEvent, window, cx: &mut Context<crate::app::App>| {
                app.toggle_select("tab-export", 0, 4, window, cx);
            },
        ))
        .child(t(EXPORT_LABEL_KEY))
        .child(
            Icon::default()
                .data(icons::CHEVRON_DOWN)
                .size(px(EXPORT_ICON))
                .text_color(color::TEXT_DIM),
        );
    if open {
        let options = [
            t("menus.items.quickExportPng").to_string(),
            t("menus.items.quickExportJpg").to_string(),
            t("menus.items.quickExportWebp").to_string(),
            t("menus.items.exportAs").to_string(),
        ];
        button = button.child(gpui_kit::deferred(
            crate::views::option_bar::controls::floating_options_panel(
                "tab-export",
                &options,
                0,
                EXPORT_H + 4.0,
                0.0,
                210.0,
                cx,
            ),
        ));
    }
    button
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
            .border_color(color::BORDER)
            .shadow(vec![ui::outset_ring(color::ACCENT.opacity(0.6))])
            .text_size(px(TAB_TEXT))
            .text_color(color::TEXT)
            .child(self.name.clone())
    }
}
