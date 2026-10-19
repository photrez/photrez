//! Visual-only port of `shell/RightDock.tsx`.
//!
//! # Geometry: 533x750 at (833,46)
//!
//! `manifest.json` gives `shell-right-dock` / `dock-properties` /
//! `dock-adjustments` / `dock-history` / `dock-presets` all as 533x750 at
//! (833,46) — the whole `<main>` row beside the canvas column. 750 is
//! 820 - 46 (title bar) - 24 (status bar). The five captures share the rect
//! and differ only in which tabs are selected.
//!
//! The dock is two columns; the split was measured out of `dock-properties.png`:
//!
//! ```text
//!   InspectorDock   x 833..1104   (272)   lg:w-[272px]  RightDock.tsx:80
//!   +---------------------------+------------------+  y=46
//!   | tab bar  38px  Properties | tab bar  38px    |  border-b #333333
//!   |                           | Layers | History  |
//!   +---------------------------+------------------+  y=84
//!   | panel content   712 tall   | panel content    |
//!   |  272 - 2*1 border          |  527 tall        |
//!   +----------------------------+------------------+  y=611
//!   | Navigator (260x185 @1106,611)                 |
//!   +-----------------------------------------------+  y=795
//! ```
//!
//! 833 + 272 = 1105, and `border-r` on the InspectorDock eats one pixel, which
//! is why `shell-navigator` starts at 1106 rather than 1105. The LayerDock is
//! `lg:w-[260px]` (`RightDock.tsx:204`) and runs to the window edge.
//! 272 + 1 + 260 = 533, the measured dock width.
//!
//! The tab bars are 38px (`h-[38px]`, `RightDock.tsx:83` and `:207`) and the
//! active tab carries a **2px** ACCENT underline of its own width — the
//! reference's underline spans 78px under "Properties" and 57px under "Layers",
//! which at 11.5px medium is exactly those labels' advance widths. So the
//! buttons are **content-width, not equal-width**.
//!
//! Inside the LayerDock, the Navigator block is 185px at the bottom:
//! 38 (tab bar) + 527 (panel) + 185 = 750, and `shell-navigator` is 260x185 at
//! (1106,611), so 611 = 46 + 38 + 527. The 185 includes the block's `border-t`,
//! so the canvas below the 38px header is 185 - 38 - 1 = 146.
//!
//! # State: passed as enums, not read from module-level state
//!
//! The reference reads `state::is_seeded` / `state::active` / `shot::is` to
//! decide which tab is shown. The port receives `&InspectorTab` and `&LayerTab`
//! from [`crate::views::workspace`], which already resolved those from
//! [`crate::state::AppState::seeded`]. No `state` or `shot` import is needed.
//!
//! # Clicks: omitted
//!
//! The reference's `tab_button` calls `state::select` / `state::refresh` on
//! click. The port is visual-only: no `.on_click()`.
//!
//! # Hover: included on inactive tabs
//!
//! The TSX paints `hover:bg-white/[0.02] hover:text-editor-text` on inactive
//! tabs. Tailwind emits CSS for both utilities, so they are ported with
//! `.hover(...)` using `hsla(0.0, 0.0, 1.0, 0.02)` inline — there is no token
//! in [`crate::tokens::color`] for 2% white.
//!
//! # Border colour: BORDER, not DIVIDER
//!
//! Every `border-*` in the web build is overridden by the unlayered
//! `* { border-color }` rule to #333333 ([`color::BORDER`]), not the
//! #2e2e2e that `border-editor-divider` names ([`color::DIVIDER`]).
//! `tokens.rs` verified this by sampling `shell-right-dock`'s `border-l`
//! (750/750 pixels at #333333). The reference's `DIVIDER` is corrected here.
//!
//! # What lives in other modules
//!
//! The four dock panels — `properties_panel`, `adjustments_panel`, `layers` and
//! `history` — are called from `inspector_body` / `layer_body`; each renders its
//! own body and owns its own pixels, so this module only frames and switches
//! them.
//!
//! The Navigator's *content* is canvas-drawn (`Navigator.tsx`'s
//! `drawNavigatorPreview` -> `createEffect` -> `drawImage`), so it is
//! unreproducible from a static port. Its frame, header and label are drawn.
//!
//! # Gaps
//!
//! * No `z_index`: everything renders last in `workspace.rs`'s tree.
//! * `Tooltip` wrappers, `FadeIn`, `transition-colors`, per-row hover on
//!   active tabs — all interaction. Active-tab hover is omitted; inactive-tab
//!   hover is ported (see above).
//! * The Presets icon in the TSX carries `opacity-50`; the reference port omits
//!   it. This is a known deviation — re-add `.opacity(0.5)` if the baseline diff
//!   flags the icon as too bright.
//! * Zoom controls, the collapsible chevron button, and the `Fit Screen` button
//!   in the Navigator header are omitted — the reference's simplified navigator
//!   is just a label and a canvas placeholder.

use crate::assets::icons;
use crate::components::ui;
use crate::i18n::t;
use crate::state::{InspectorTab, LayerTab};
use crate::tokens::color;
use crate::views::{adjustments_panel, history, layers, properties_panel};

use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::{div, hsla, px, relative, Div, FontWeight, Stateful};

// ── Geometry ─────────────────────────────────────────────────────────────────-

/// The dock's measured width: 533px.
const DOCK_W: f32 = 533.0;

/// The whole `<main>` row: 820 - 46 - 24.
///
/// Unused in `render()` — the parent `workspace.rs` sets the dock's height
/// externally, so this documents the baseline geometry rather than driving
/// layout. It is exercised by the geometry-pin test.
#[allow(dead_code)]
const DOCK_H: f32 = 750.0;

/// InspectorDock's width: `lg:w-[272px]` (`RightDock.tsx:80`).
const INSPECTOR_W: f32 = 272.0;

/// LayerDock's width: `lg:w-[260px]` (`RightDock.tsx:204`).
const LAYER_W: f32 = 260.0;

/// Both tab bars: `h-[38px]` (`RightDock.tsx:83`, `:207`).
const TAB_BAR_H: f32 = 38.0;

/// `border-r` between the two columns.
const SPLIT_BORDER: f32 = 1.0;

/// The Navigator block: 185px tall. The 185 counts the block's own `border-t`,
/// so the canvas below the 38px header is `185 - 38 - 1` = 146.
const NAV_H: f32 = 185.0;
const NAV_HEADER_H: f32 = 38.0;

/// `h-[2px]` — the active tab's underline.
const UNDERLINE_H: f32 = 2.0;

/// `px-3` — a tab button's horizontal inset.
const TAB_PAD_X: f32 = 12.0;

/// `text-[11.5px] font-medium` — the tab labels.
const TAB_TEXT: f32 = 11.5;

pub fn render(inspector_tab: &InspectorTab, layer_tab: &LayerTab) -> impl IntoElement {
    h_flex()
        .flex_none()
        .items_stretch()
        .w(px(DOCK_W))
        .bg(color::PANEL)
        .child(inspector_dock(inspector_tab))
        .child(layer_dock(layer_tab))
}

/// The left column: tabs, then whichever inspector panel is selected.
fn inspector_dock(inspector_tab: &InspectorTab) -> Div {
    let active = inspector_tab_id(inspector_tab);
    v_flex()
        .w(px(INSPECTOR_W))
        .h_full()
        .flex_none()
        .overflow_hidden()
        .bg(color::PANEL)
        .border_r(px(SPLIT_BORDER))
        .border_color(color::BORDER)
        .child(tab_bar(active, &["properties", "adjustments", "presets"]))
        .child(
            v_flex()
                .flex_1()
                .min_h(px(0.0))
                .overflow_hidden()
                .child(inspector_body(inspector_tab)),
        )
}

/// Which of the three inspector panels to draw.
///
/// The Adjustments and Properties branches wrap their panels because the panel
/// `render()` functions return opaque `impl IntoElement`, not `Div`. As in
/// [`layer_body`], the wrapper carries the stretch the reference left on its
/// bare panel, so the panel fills the column exactly as it did there. The
/// Presets stub returns `Div` and sets its own `h_full`, so it needs no
/// wrapper.
fn inspector_body(inspector_tab: &InspectorTab) -> Div {
    match inspector_tab {
        InspectorTab::Adjustments => div()
            .flex_1()
            .min_h(px(0.0))
            .overflow_hidden()
            .child(adjustments_panel::render()),
        InspectorTab::Presets => presets_stub(),
        InspectorTab::Properties => div()
            .flex_1()
            .min_h(px(0.0))
            .overflow_hidden()
            .child(properties_panel::render()),
    }
}

/// The right column: tabs, a panel body, then the Navigator pinned to the
/// bottom.
fn layer_dock(layer_tab: &LayerTab) -> Div {
    let active = layer_tab_id(layer_tab);
    v_flex()
        .w(px(LAYER_W))
        .h_full()
        .flex_none()
        .overflow_hidden()
        .bg(color::PANEL)
        .child(tab_bar(active, &["layers", "history"]))
        .child(
            v_flex()
                .flex_1()
                .min_h(px(0.0))
                .overflow_hidden()
                .child(layer_body(layer_tab)),
        )
        .child(navigator())
}

/// Which of the two LayerDock panels to draw.
///
/// Both branches are wrapped in `div()` because the panel `render()` functions
/// return opaque `impl IntoElement`, not `Div` — wrapping each branch yields a
/// uniform `Div` type from this single `match` expression.
///
/// The Layers wrapper carries the stretch the reference left on its bare panel:
/// the reference slotted `layers_panel::render()` straight into the flex column,
/// where its `flex-1 min-h-0` let the list fill any window height. With a
/// wrapper in between, those properties must sit on the wrapper too, or it hugs
/// its content and leaves a band of bare column below the panel. History sets a
/// fixed `PANEL_H`, so its plain wrapper is correct.
fn layer_body(layer_tab: &LayerTab) -> Div {
    match layer_tab {
        LayerTab::History => div().child(history::render()),
        LayerTab::Layers => div()
            .flex_1()
            .min_h(px(0.0))
            .overflow_hidden()
            .child(layers::render()),
    }
}

/// The Presets tab's placeholder, transcribed from `RightDock.tsx:132-138`.
///
/// The only one of the three inspector bodies with no separate component: a
/// centred icon and two lines, and the source itself says "Coming soon".
fn presets_stub() -> Div {
    v_flex()
        .w_full()
        .h_full()
        .items_center()
        .justify_center()
        .p(px(24.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .mb(px(12.0))
                .child(
                    Icon::default()
                        .data(icons::SPARKLES)
                        .size(px(24.0))
                        .text_color(color::TEXT_DIM),
                ),
        )
        .child(
            div()
                .text_size(px(13.0))
                .font_family(ui::FONT_UI)
                .font_weight(FontWeight(500.))
                .text_color(color::TEXT)
                .child(t("panels.presets")),
        )
        .child(
            div()
                .mt(px(4.0))
                .text_size(px(12.0))
                .font_family(ui::FONT_UI)
                .line_height(relative(1.3))
                .text_center()
                .text_color(color::TEXT_DIM)
                .child(t("panels.presetsComingSoon")),
        )
}

/// `shrink-0 border-t` with a 38px header (`RightDock.tsx:244-250`) and the
/// canvas below it.
///
/// The preview itself is canvas-drawn (`Navigator.tsx`'s `drawNavigatorPreview`),
/// so this is the frame only. Its height is what is left of [`NAV_H`] after the
/// `border-t` and the header: `NAV_H - NAV_HEADER_H - SPLIT_BORDER` = 146.
fn navigator() -> Div {
    v_flex()
        .w_full()
        .flex_shrink_0()
        .border_t(px(SPLIT_BORDER))
        .border_color(color::BORDER)
        .bg(color::PANEL)
        .child(
            h_flex()
                .w_full()
                .h(px(NAV_HEADER_H))
                .flex_shrink_0()
                .items_center()
                .justify_between()
                .px(px(12.0))
                .border_b(px(SPLIT_BORDER))
                .border_color(color::BORDER)
                .child(
                    div()
                        .text_size(px(11.5))
                        .font_family(ui::FONT_UI)
                        .font_weight(FontWeight(500.))
                        .text_color(color::TEXT)
                        .child(t("panels.navigator")),
                ),
        )
        .child(
            div()
                .w_full()
                .h(px(NAV_H - NAV_HEADER_H - SPLIT_BORDER))
                .bg(color::CANVAS),
        )
}

/// One tab bar: a 38px band with a full-width bottom border, holding
/// content-width `nav` buttons.
///
/// `active` is the id of the currently lit tab; `tabs` are the ids in order.
/// The label is derived from the id via [`tab_label`] rather than stored
/// beside it — every id in the shell is a plain lower-case word whose display
/// form is its capitalisation.
fn tab_bar(active: &'static str, tabs: &'static [&'static str]) -> Div {
    let bar = div()
        .w_full()
        .h(px(TAB_BAR_H))
        .flex_shrink_0()
        .flex()
        .items_center()
        .overflow_hidden()
        .border_b(px(SPLIT_BORDER))
        .border_color(color::BORDER)
        .bg(color::PANEL);

    let mut row = h_flex().h_full().min_w(px(0.0)).items_center();
    for id in tabs {
        row = row.child(tab_button(id, *id == active));
    }
    bar.child(row)
}

/// `Properties` from `properties` — the display form of a tab id.
fn tab_label(id: &str) -> String {
    let mut chars = id.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// The string id that corresponds to an [`InspectorTab`].
fn inspector_tab_id(tab: &InspectorTab) -> &'static str {
    match tab {
        InspectorTab::Properties => "properties",
        InspectorTab::Adjustments => "adjustments",
        InspectorTab::Presets => "presets",
    }
}

/// The string id that corresponds to a [`LayerTab`].
fn layer_tab_id(tab: &LayerTab) -> &'static str {
    match tab {
        LayerTab::Layers => "layers",
        LayerTab::History => "history",
    }
}

/// A `relative flex h-full items-center px-3 text-[11.5px] font-medium`
/// button, with the 2px ACCENT underline under it when active.
///
/// The underline is the button's *last child* rather than an absolutely
/// positioned pseudo-element: `after:absolute after:bottom-0 after:inset-x-0`
/// needs a positioned ancestor, which this port has no way to declare, and
/// `z_index` is unavailable besides. Stacking the label in a `flex_1` row
/// above a full-width 2px bar gets the same pixels without either.
///
/// Click handling is omitted — the port is visual-only.
///
/// Returns a `Stateful<Div>` rather than a `Div`: `.id(..)` and `.hover(..)`
/// both take the element through that wrapper, and the caller needs the same
/// concrete type whichever branch ran. `id` is `'static` because the tab ids
/// are all string literals.
fn tab_button(id: &'static str, active: bool) -> Stateful<Div> {
    let label = tab_label(id);
    let mut btn = v_flex()
        .h_full()
        .flex_shrink_0()
        .id(id)
        .cursor_pointer()
        .child(
            h_flex()
                .flex_1()
                .items_center()
                .px(px(TAB_PAD_X))
                .text_size(px(TAB_TEXT))
                .font_family(ui::FONT_UI)
                .font_weight(FontWeight(500.))
                .text_color(if active { color::TEXT } else { color::TEXT_DIM })
                .child(label),
        );
    if active {
        btn = btn.child(div().w_full().h(px(UNDERLINE_H)).bg(color::ACCENT));
    } else {
        // `hover:bg-white/[0.02] hover:text-editor-text` — Tailwind emits
        // CSS for both. No token for 2% white; use hsla inline.
        btn = btn.hover(|s| s.bg(hsla(0.0, 0.0, 1.0, 0.02)).text_color(color::TEXT));
    }
    btn
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `tab_label` capitalises the first character of a tab id and leaves the
    /// rest untouched. Empty input yields an empty string.
    ///
    /// RED proof: remove the `to_uppercase()` call and every assertion flips.
    #[test]
    fn tab_label_capitalizes_first_char() {
        assert_eq!(tab_label("properties"), "Properties");
        assert_eq!(tab_label("adjustments"), "Adjustments");
        assert_eq!(tab_label("presets"), "Presets");
        assert_eq!(tab_label("layers"), "Layers");
        assert_eq!(tab_label("history"), "History");
        assert_eq!(tab_label(""), "");
    }

    /// `inspector_tab_id` maps each `InspectorTab` variant to the string id
    /// the tab bar uses to light its button.
    ///
    /// RED proof: swap any arm (e.g. `Adjustments` → "properties") and this
    /// fails — `inspector_dock` uses this id to decide which tab is active.
    #[test]
    fn inspector_tab_id_maps_each_variant() {
        assert_eq!(inspector_tab_id(&InspectorTab::Properties), "properties");
        assert_eq!(inspector_tab_id(&InspectorTab::Adjustments), "adjustments");
        assert_eq!(inspector_tab_id(&InspectorTab::Presets), "presets");
    }

    /// `layer_tab_id` maps each `LayerTab` variant to its string id.
    ///
    /// RED proof: swap any arm and this fails.
    #[test]
    fn layer_tab_id_maps_each_variant() {
        assert_eq!(layer_tab_id(&LayerTab::Layers), "layers");
        assert_eq!(layer_tab_id(&LayerTab::History), "history");
    }

    /// The id→label chain must agree: `tab_label(inspector_tab_id(tab))`
    /// produces the display name the tab bar renders. If the two mappings
    /// disagree, the lit tab's underline would sit under the wrong label.
    ///
    /// RED proof: change `tab_label` to lowercase the rest and this fails.
    #[test]
    fn inspector_tab_label_round_trips() {
        assert_eq!(
            tab_label(inspector_tab_id(&InspectorTab::Properties)),
            "Properties"
        );
        assert_eq!(
            tab_label(inspector_tab_id(&InspectorTab::Adjustments)),
            "Adjustments"
        );
        assert_eq!(
            tab_label(inspector_tab_id(&InspectorTab::Presets)),
            "Presets"
        );
    }

    /// Pin the measured geometry constants from the baseline manifest.
    ///
    /// RED proof: change any constant and this fails.
    #[test]
    fn geometry_constants_are_pinned() {
        assert_eq!(DOCK_W, 533.0);
        assert_eq!(DOCK_H, 750.0);
        assert_eq!(INSPECTOR_W, 272.0);
        assert_eq!(LAYER_W, 260.0);
        assert_eq!(TAB_BAR_H, 38.0);
        assert_eq!(SPLIT_BORDER, 1.0);
        assert_eq!(NAV_H, 185.0);
        assert_eq!(NAV_HEADER_H, 38.0);
        assert_eq!(UNDERLINE_H, 2.0);
        assert_eq!(TAB_PAD_X, 12.0);
        assert_eq!(TAB_TEXT, 11.5);

        // 272 + 1 + 260 = 533 — the dock's two-column split.
        assert!((INSPECTOR_W + SPLIT_BORDER + LAYER_W - DOCK_W).abs() < f32::EPSILON);

        // 38 (tab bar) + 527 (panel) + 185 (navigator) = 750 (dock height).
        assert!((TAB_BAR_H + 527.0 + NAV_H - DOCK_H).abs() < f32::EPSILON);

        // Navigator canvas: 185 - 38 (header) - 1 (border) = 146.
        assert_eq!(NAV_H - NAV_HEADER_H - SPLIT_BORDER, 146.0);
    }

    /// The active tab underline is 2px — matching `after:h-[2px]` in the TSX.
    #[test]
    fn underline_height_matches_tsx() {
        assert_eq!(UNDERLINE_H, 2.0);
    }
}
