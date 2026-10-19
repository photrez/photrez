//! Screen-level composition of the native shell.
//!
//! Mirrors `EditorShell.tsx` from the web build (`apps/desktop/src/components/editor/shell/`),
//! adapted for the gpui-kit port. Every nested region is a function call so
//! each can be `cargo check`ed in isolation during the port.

#![allow(dead_code)]

use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, Window};

use crate::shot;
use crate::state::AppState;
use crate::tokens::color;
use crate::views::{
    context_menus, dialogs, document_tabs, empty_state, loading, menus, option_bar, right_dock,
    status_bar, title_bar, toasts, tool_rail, tooltips,
};

/// Title bar height: 46px.
const TITLEBAR_H: f32 = 46.0;
/// Status bar height: 24px.
const STATUSBAR_H: f32 = 24.0;
/// Tool rail width: 52px.
const RAIL_W: f32 = 52.0;
/// Right dock width: 533px.
const DOCK_W: f32 = 533.0;
/// Document tabs height: 38px.
const TABBAR_H: f32 = 38.0;
/// Option bar height: 44px.
const OPTIONBAR_H: f32 = 44.0;

/// Render the full shell for one frame.
///
/// Geometry mirrors `EditorShell.tsx`:
///
/// ```text
/// <photrez-app>           BG (#171717)
///   AppTitleBar          (46px)
///   <main> flex_1 min_h_0
///     LeftToolRail        52px
///     <center col> flex_1 min_w_0
///       DocumentTabs      (38px)
///       OptionBar         (44px)
///       Canvas            flex_1  <- 668px at 820px window
///     RightDock           533px (only when dock_open)
///   BottomStatusBar        (24px)
/// ```
pub fn render(state: &AppState, window: &mut Window) -> impl IntoElement {
    // Middle row height: 820 - 46 (title) - 24 (status).
    let middle_h = 820.0 - TITLEBAR_H - STATUSBAR_H;

    // Outer BG fill — full window, matches `bg-editor-bg` (`#171717`).
    v_flex()
        .size_full()
        .bg(color::BG)
        .child(
            // AppTitleBar — 46px (title bar)
            div().h(px(TITLEBAR_H)).child(title_bar::render(window)),
        )
        .child(
            // <main class="flex min-h-0 flex-1 overflow-hidden">
            h_flex()
                .flex_1()
                .min_h(px(0.))
                .items_stretch()
                .bg(color::BG)
                .when(state.chrome_visible, |el| {
                    el.child(
                        // LeftToolRail — 52px, flex_none
                        div()
                            .w(px(RAIL_W))
                            .h(px(middle_h))
                            .flex_none()
                            .bg(color::BG)
                            .child(tool_rail::render(&state.active_tool)),
                    )
                })
                .child(
                    // <div class="flex flex-1 min-w-0 overflow-hidden">
                    h_flex()
                        .flex_1()
                        .min_w(px(0.))
                        .items_stretch()
                        .overflow_hidden()
                        .child(
                            // <section class="flex min-w-0 flex-1 flex-col">
                            v_flex()
                                .flex_1()
                                .min_w(px(0.))
                                .items_stretch()
                                .overflow_hidden()
                                .when(state.has_document, |el| {
                                    el.child(
                                        // DocumentTabsBar — 38px
                                        div().h(px(TABBAR_H)).child(document_tabs::render()),
                                    )
                                })
                                .when(state.has_document && state.chrome_visible, |el| {
                                    el.child(
                                        // OptionBar — 44px
                                        div()
                                            .h(px(OPTIONBAR_H))
                                            .child(option_bar::render(&state.active_tool)),
                                    )
                                })
                                .child(
                                    // CanvasViewport - flex_1, min_h_0. Flat
                                    // stand-in: the document itself is
                                    // `photrez-core`'s territory, not chrome.
                                    div().flex_1().min_h(px(0.0)).w_full().bg(color::CANVAS),
                                ),
                        )
                        // RightDock — only when dock_open and the chrome shows
                        .when(state.dock_open && state.chrome_visible, |el| {
                            el.child(
                                div()
                                    .w(px(DOCK_W))
                                    .h(px(middle_h))
                                    .flex_none()
                                    .bg(color::BG)
                                    .child(right_dock::render(
                                        &state.inspector_tab,
                                        &state.layer_tab,
                                    )),
                            )
                        }),
                ),
        )
        // Status bar — 24px, at the very bottom
        .when(state.chrome_visible, |el| {
            el.child(div().h(px(STATUSBAR_H)).child(status_bar::render()))
        })
        // Overlays last on the root — GPUI has no z_index, paint order is tree
        // order. Each is gated on the shot id so a plain launch still renders
        // the default shell unchanged.
        .when(shot::starts_with("ctx-"), |el| {
            el.child(context_menus::render())
        })
        .when(shot::starts_with("empty-"), |el| {
            el.child(empty_state::render())
        })
        .when(shot::starts_with("toast-"), |el| el.child(toasts::render()))
        .when(shot::starts_with("tooltip-"), |el| {
            el.child(tooltips::render())
        })
        .when(shot::starts_with("menu-"), |el| el.child(menus::render()))
        .when(shot::starts_with("dialog-"), |el| {
            el.child(dialogs::render())
        })
        .when(shot::starts_with("loading-"), |el| {
            el.child(loading::render())
        })
}
