//! The right dock: its appearance and its two tab strips.
//!
//! # Geometry: 533x750 at (833,46)
//!
//! `manifest.json`: `shell-right-dock` / `dock-properties` / `dock-adjustments` /
//! `dock-history` / `dock-presets` are 533x750 at (833,46), the `<main>` row
//! beside the canvas. 750 = 820 - 46 (title bar) - 24 (status bar); five
//! captures share the rect and differ only in selected tabs.
//!
//! Two columns measured in `dock-properties.png`:
//! ```text
//!   InspectorDock   x 833..1104   (272)   272px wide
//!   +---------------------------+------------------+  y=46
//!   | tab bar  38px  Properties | tab bar  38px    |  1px bottom border #333333
//!   |                           | Layers | History  |
//!   +---------------------------+------------------+  y=84
//!   | panel content   712 tall   | panel content    |
//!   |  272 - 2*1 border          |  527 tall        |
//!   +----------------------------+------------------+  y=611
//!   | Navigator (260x185 @1106,611)                 |
//!   +-----------------------------------------------+  y=795
//! ```
//! 833 + 272 = 1105; InspectorDock's 1px right border takes 1px, placing
//! `shell-navigator` at 1106, not 1105. LayerDock is 260px
//! wide to the window edge; 272 + 1 + 260 = 533.
//! Tab bars are 38px in both columns. The active
//! **2px** ACCENT underline is 78px under "Properties" and 57px under "Layers",
//! the labels' advance widths at 11.5px medium; buttons are **content-width,
//! not equal-width**.
//! Bottom Navigator: 185px; 38 (tab bar) + 527 (panel) + 185 = 750.
//! `shell-navigator` is 260x185 at (1106,611): 611 = 46 + 38 + 527. Its 185
//! includes a 1px top border; canvas below the 38px header is 185 - 38 - 1 = 146.
//!
//! The dock reads its tab choice from [`AppState`]
//! (`state::is_seeded` / `state::active` / `shot::is`
//! are the same question asked of the shot id):
//! each frame this view copies the two `Copy` fields `inspector_tab` and
//! `layer_tab` from [`AppState`], matching the shell's selection.
//! A tab click writes the clicked tab to [`AppState`] and notifies the root,
//! lighting it and swapping the panel next frame.
//! Inactive tabs brighten their text and gain a 2%-white background on hover,
//! reproduced through `.hover(...)` with `hsla(0.0, 0.0, 1.0, 0.02)`.
//! No [`crate::tokens::color`] token represents 2% white.
//! Every border here resolves to #333333
//! ([`color::BORDER`]), not the #2e2e2e divider token ([`color::DIVIDER`]).
//! `tokens.rs` sampled `shell-right-dock`'s left border: 750/750 pixels
//! #333333; the divider value is corrected here.
//! Four dock panels: `inspector_body` / `layer_body` call `properties_panel`,
//! `adjustments_panel`, `layers`, and `history`; each owns its body and pixels,
//! while this module frames and switches them.
//! The Navigator's preview is canvas-drawn,
//! not reproducible statically; frame, header,
//! and label are drawn.
//!
//! # Gaps
//!
//! * No `z_index`; everything renders last in `workspace.rs`'s tree.
//! * `Tooltip`, `FadeIn`, the colour transition, and per-row hover on active tabs
//!   are interaction-only. Active-tab hover is omitted; inactive-tab hover is
//!   reproduced.
//! * Known deviation: the Presets tab's 50% opacity is omitted; re-add
//!   `.opacity(0.5)` if the baseline flags it as too bright.
//! * Navigator header's zoom controls, collapsible chevron button, and
//!   `Fit Screen` button are omitted; the captured header is a
//!   label and a canvas placeholder.

use crate::components::ui;
use crate::display::canvas_bridge::{self, Camera, CanvasImage};
use crate::i18n::t;
use crate::icons;
use crate::state::{AppState, InspectorTab, LayerTab};
use crate::tokens::color;
use crate::views::{adjustments_panel, history, layers, properties_panel};

use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::{
    div, hsla, px, relative, Bounds, Context, Div, Entity, FontWeight, MouseButton, Pixels,
    TestSupportExt,
};
use std::cell::Cell;
use std::rc::Rc;

/// The dock's measured width: 533px.
const DOCK_W: f32 = 533.0;

/// The dock at the wide breakpoint (viewport 1536 px or wider): the layout
/// widens both columns to 300px and 298px.
const DOCK_W_WIDE: f32 = 598.0;

/// The whole `<main>` row: 820 - 46 - 24.
///
/// Unused in `render()` — the parent `workspace.rs` sets the dock's height
/// externally, so this documents the baseline geometry rather than driving
/// layout. It is exercised by the geometry-pin test.
#[allow(dead_code)]
const DOCK_H: f32 = 750.0;

/// InspectorDock's width: 272px.
const INSPECTOR_W: f32 = 272.0;

/// InspectorDock at the wide breakpoint: 300px.
const INSPECTOR_W_WIDE: f32 = 300.0;

/// LayerDock's width: 260px.
const LAYER_W: f32 = 260.0;

/// LayerDock at the wide breakpoint: 298px.
const LAYER_W_WIDE: f32 = 298.0;

/// Both tab bars: 38px.
const TAB_BAR_H: f32 = 38.0;

/// The border between the two columns.
const SPLIT_BORDER: f32 = 1.0;

/// The Navigator block: 185px tall. The 185 counts the block's own 1px top
/// border, so the canvas below the 38px header is `185 - 38 - 1` = 146.
const NAV_H: f32 = 185.0;
const NAV_HEADER_H: f32 = 38.0;

/// The active tab's underline: 2px tall.
const UNDERLINE_H: f32 = 2.0;

/// A tab button's horizontal inset: 12px.
const TAB_PAD_X: f32 = 12.0;

/// The tab labels: 11.5px, medium weight.
const TAB_TEXT: f32 = 11.5;

/// The live app-state entity the dock writes its tab selection back into, plus
/// the owning-root context each button's click handler uses to run on the app.
#[allow(clippy::too_many_arguments)] // the dock reads one state record spread across panel inputs
pub fn render(
    state: &Entity<AppState>,
    canvas: Option<&CanvasImage>,
    camera: Option<Camera>,
    zoom_percent: Option<u32>,
    wide_layout: bool,
    canvas_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    navigator_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    layer_thumbs: &[Option<std::sync::Arc<gpui_kit::RenderImage>>],
    transform_readout: Option<crate::state::LayerTransformReadout>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let (inspector_tab, layer_tab) = {
        let s = state.read(cx);
        (s.inspector_tab, s.layer_tab)
    };
    let (dock_w, inspector_w, layer_w) = if wide_layout {
        (DOCK_W_WIDE, INSPECTOR_W_WIDE, LAYER_W_WIDE)
    } else {
        (DOCK_W, INSPECTOR_W, LAYER_W)
    };
    let document_size = canvas.map(|image| image.dimensions());
    h_flex()
        .id("right-dock")
        .test_support()
        .flex_none()
        .items_stretch()
        .w(px(dock_w))
        .h_full()
        .bg(color::PANEL)
        .child(
            inspector_dock(
                inspector_tab,
                state,
                document_size,
                zoom_percent,
                inspector_w,
                transform_readout,
                cx,
            )
            .id("inspector-dock")
            .test_support(),
        )
        .child(
            layer_dock(
                layer_tab,
                state,
                canvas,
                camera,
                layer_w,
                canvas_bounds,
                navigator_bounds,
                layer_thumbs,
                cx,
            )
            .id("layer-dock")
            .test_support(),
        )
}

fn inspector_dock(
    inspector_tab: InspectorTab,
    state: &Entity<AppState>,
    document_size: Option<(u32, u32)>,
    zoom_percent: Option<u32>,
    width: f32,
    transform_readout: Option<crate::state::LayerTransformReadout>,
    cx: &Context<crate::app::App>,
) -> Div {
    let active = inspector_tab_id(&inspector_tab);
    v_flex()
        .w(px(width))
        .h_full()
        .flex_none()
        .overflow_hidden()
        .bg(color::PANEL)
        .border_r(px(SPLIT_BORDER))
        .border_color(color::BORDER)
        .child(tab_bar(
            active,
            &["properties", "adjustments", "presets"],
            write_inspector_tab,
            state,
            cx,
        ))
        .child(
            v_flex()
                .id(match inspector_tab {
                    InspectorTab::Properties => "inspector-body-properties",
                    InspectorTab::Adjustments => "inspector-body-adjustments",
                    InspectorTab::Presets => "inspector-body-presets",
                })
                .test_support()
                .flex_1()
                .min_h(px(0.0))
                .overflow_hidden()
                .child(inspector_body(
                    &inspector_tab,
                    state,
                    document_size,
                    zoom_percent,
                    transform_readout,
                    cx,
                )),
        )
}

/// The Adjustments and Properties branches wrap their panels because the panel
/// `render()` functions return opaque `impl IntoElement`, not `Div`, so they
/// need a uniform type from this single `match`. The wrapper must be a flex
/// column, not a plain `div()`: the toolkit's default `display: block` would
/// size the panel by its content instead of letting its `h_full`/`flex_1`
/// reach the column. The Presets stub returns `Div` and sets its own `h_full`,
/// so it needs no wrapper.
fn inspector_body(
    inspector_tab: &InspectorTab,
    state: &Entity<AppState>,
    document_size: Option<(u32, u32)>,
    zoom_percent: Option<u32>,
    transform_readout: Option<crate::state::LayerTransformReadout>,
    cx: &Context<crate::app::App>,
) -> Div {
    match inspector_tab {
        InspectorTab::Adjustments => v_flex()
            .flex_1()
            .min_h(px(0.0))
            .overflow_hidden()
            .child(adjustments_panel::render(state, cx)),
        InspectorTab::Presets => presets_stub(),
        InspectorTab::Properties => {
            v_flex()
                .flex_1()
                .min_h(px(0.0))
                .overflow_hidden()
                .child(properties_panel::render(
                    state,
                    document_size,
                    zoom_percent,
                    transform_readout,
                    cx,
                ))
        }
    }
}

/// The right column: tabs, a panel body, then the Navigator pinned to the
/// bottom.
#[allow(clippy::too_many_arguments)] // the dock reads one state record spread across panel inputs
fn layer_dock(
    layer_tab: LayerTab,
    state: &Entity<AppState>,
    canvas: Option<&CanvasImage>,
    camera: Option<Camera>,
    width: f32,
    canvas_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    navigator_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    layer_thumbs: &[Option<std::sync::Arc<gpui_kit::RenderImage>>],
    cx: &Context<crate::app::App>,
) -> Div {
    let active = layer_tab_id(&layer_tab);
    v_flex()
        .w(px(width))
        .h_full()
        .flex_none()
        .overflow_hidden()
        .bg(color::PANEL)
        .child(tab_bar(
            active,
            &["layers", "history"],
            write_layer_tab,
            state,
            cx,
        ))
        .child(
            v_flex()
                .id(match layer_tab {
                    LayerTab::Layers => "layer-body-layers",
                    LayerTab::History => "layer-body-history",
                })
                .test_support()
                .flex_1()
                .min_h(px(0.0))
                .overflow_hidden()
                .child(layer_body(&layer_tab, state, layer_thumbs, cx)),
        )
        .child(
            navigator(canvas, camera, canvas_bounds, navigator_bounds, cx)
                .id("navigator")
                .test_support(),
        )
}

/// Both branches are wrapped in a flex column because the panel `render()`
/// functions return opaque `impl IntoElement`, not `Div`, and a plain `div()`
/// is `display: block` in the toolkit - its child would take its content
/// height, so the panel's `flex_1`/`h_full` would never reach the column and
/// the Layers action footer would sit right under the rows instead of at the
/// panel's foot. History's root stretches the same way (it was a fixed
/// baseline height); at the baseline window both resolve to the measured 527.
fn layer_body(
    layer_tab: &LayerTab,
    state: &Entity<AppState>,
    layer_thumbs: &[Option<std::sync::Arc<gpui_kit::RenderImage>>],
    cx: &Context<crate::app::App>,
) -> Div {
    match layer_tab {
        LayerTab::History => v_flex()
            .flex_1()
            .min_h(px(0.0))
            .overflow_hidden()
            .child(history::render(state, cx)),
        LayerTab::Layers => v_flex()
            .flex_1()
            .min_h(px(0.0))
            .overflow_hidden()
            .child(layers::render(state, layer_thumbs, cx)),
    }
}

/// The Presets tab's placeholder, transcribed from the capture.
///
/// The only one of the three inspector bodies with no separate component: a
/// centred icon and two lines, and its body is the "Coming soon" stub.
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

/// A top-bordered, non-growing section with a 38px header and the
/// canvas below it.
///
/// The preview itself is canvas-drawn,
/// so this is the frame only. Its height is what is left of [`NAV_H`] after the
/// 1px top border and the header: `NAV_H - NAV_HEADER_H - SPLIT_BORDER` = 146.
fn navigator(
    canvas: Option<&CanvasImage>,
    camera: Option<Camera>,
    canvas_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    navigator_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    cx: &Context<crate::app::App>,
) -> Div {
    let state = canvas_bridge::navigator_element(
        canvas,
        camera,
        canvas_bounds,
        Rc::clone(&navigator_bounds),
    );
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
                .relative()
                .w_full()
                .h(px(NAV_H - NAV_HEADER_H - SPLIT_BORDER))
                .overflow_hidden()
                // With no canvas the navigator shows its empty branch:
                // the localized "No image open" line centred in the body.
                .when(canvas.is_none(), |el| {
                    el.child(
                        div()
                            .absolute()
                            .inset(px(0.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(11.5))
                            .font_family(ui::FONT_UI)
                            .text_color(color::TEXT_DIM)
                            .child(t("history.noImageOpen")),
                    )
                })
                .child(state)
                .child(
                    div()
                        .id("navigator-hitbox")
                        .test_support()
                        .absolute()
                        .inset(px(0.0))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(
                                |app: &mut crate::app::App,
                                 event: &gpui_kit::MouseDownEvent,
                                 _window,
                                 cx| {
                                    app.navigator_pan_to(event.position, cx);
                                },
                            ),
                        ),
                ),
        )
}

mod tabs;
#[cfg(test)]
mod tests;

#[cfg(test)]
use tabs::{inspector_tab_from_id, layer_tab_from_id};
use tabs::{inspector_tab_id, layer_tab_id, tab_bar, write_inspector_tab, write_layer_tab};
