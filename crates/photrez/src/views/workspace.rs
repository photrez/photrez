//! Screen-level composition of the native shell.
//!
//! The editor shell: title bar, menu bar, tool rail, option bar, canvas,
//! right dock and status bar. Every nested region is a function call so
//! each can be `cargo check`ed in isolation.

use crate::display::canvas_bridge::{self, Camera, CanvasImage, CheckerCache};
use crate::shot;
use crate::state::AppState;
use crate::tokens::color;
use crate::views::{
    brush_menu, color_picker, context_menus, dialogs, document_tabs, empty_state, loading, menus,
    option_bar, right_dock, status_bar, title_bar, toasts, tool_rail, tooltips,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, Bounds, Context, Entity, Pixels, TestSupportExt, Window};

/// Title bar height: 46px.
const TITLEBAR_H: f32 = 46.0;
/// Status bar height: 24px.
const STATUSBAR_H: f32 = 24.0;
/// Tool rail width: 52px.
const RAIL_W: f32 = 52.0;
/// Right dock width: 533px.
const DOCK_W: f32 = 533.0;
/// Right dock width at the `2xl` breakpoint: 272+1+260 widens to 300+298.
const DOCK_W_2XL: f32 = 598.0;
/// Document tabs height: 38px.
const TABBAR_H: f32 = 38.0;
/// Option bar height: 44px.
const OPTIONBAR_H: f32 = 44.0;
/// Text overlay hint strip width (the captured badge is 270 physical px).
const HINT_W: f32 = 216.0;
/// What the edit box's ring adds on each side: a 1px and a 2px band. The
/// overlay origin and the hint strip's offsets give it back.
const RING: f32 = 3.0;

/// Render the full shell for one frame.
///
/// Geometry of the shell regions:
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
///
/// `state` is the live state entity, not a snapshot: the tool rail and the
/// dock's tab bars write their selection back into it, and every field the
/// shell reads here is `Copy`, so one read per frame is enough.
#[allow(clippy::too_many_arguments)] // the shell composes one frame from the whole app state
pub(crate) fn render(
    state: &Entity<AppState>,
    window: &mut Window,
    canvas: Option<&CanvasImage>,
    camera: Option<Camera>,
    overlay: canvas_bridge::CanvasOverlay,
    canvas_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    checker: Rc<RefCell<CheckerCache>>,
    navigator_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    layer_thumbs: &[Option<std::sync::Arc<gpui_kit::RenderImage>>],
    focus_handle: &gpui_kit::FocusHandle,
    rename_focus: &gpui_kit::FocusHandle,
    resize_focus: &gpui_kit::FocusHandle,
    new_document_focus: &gpui_kit::FocusHandle,
    dialog_focus: &gpui_kit::FocusHandle,
    picker_focus: &gpui_kit::FocusHandle,
    capture: Rc<Cell<Option<gpui_kit::HitboxId>>>,
    cursor: crate::input_policy::CursorPolicy,
    has_pixel_selection: bool,
    has_copyable_selection: bool,
    has_selection_clipboard: bool,
    can_undo: bool,
    can_redo: bool,
    committed_selection: Option<photrez_core::document::content::SelectionState>,
    transform_readout: Option<crate::state::LayerTransformReadout>,
    cx: &mut Context<crate::app::App>,
) -> impl IntoElement {
    // One read of the live state; every field the shell branches on is `Copy`.
    let (
        dock_open,
        has_document,
        chrome_visible,
        status_error,
        tabs,
        active_tab,
        selection_flyout_open,
        selection_shape,
        open_menu,
        context_menu,
        brush_menu,
        active_dialog,
        color_picker_open,
        autosave_status,
        text_edit,
        text_editing,
        hud,
        gradient_badge,
    ) = {
        let s = state.read(cx);
        (
            s.dock_open,
            s.has_document,
            s.chrome_visible,
            s.status_error.clone(),
            s.tabs.clone(),
            s.active_tab.clone(),
            s.selection_flyout_open,
            s.selection_shape,
            s.open_menu,
            s.context_menu,
            s.brush_menu,
            s.active_dialog,
            s.color_picker.is_some(),
            s.autosave_status,
            s.text_edit.clone(),
            s.text_edit.is_some(),
            s.hud,
            s.gradient_badge,
        )
    };
    // The shell is responsive at the same breakpoints as the captured layout:
    // `2xl` (viewport >= 1536 CSS px) widens both dock columns.
    let two_xl = window.viewport_size().width >= px(1536.0);
    let dock_w = if two_xl { DOCK_W_2XL } else { DOCK_W };
    let (active_tool, selected_layer_name) = {
        let s = state.read(cx);
        (
            s.active_tool,
            s.layers
                .iter()
                .find(|row| row.id == s.selected_layer)
                .map(|row| row.name.clone()),
        )
    };
    let document_size = canvas.map(|image| image.dimensions());
    if !has_document || canvas.is_none() {
        canvas_bounds.set(None);
    }
    let zoom_percent = canvas.and_then(|image| {
        let (doc_w, doc_h) = image.dimensions();
        let concrete = camera.or_else(|| {
            let bounds = canvas_bounds.get()?;
            Camera::fit(
                bounds.size.width.into(),
                bounds.size.height.into(),
                doc_w as f32,
                doc_h as f32,
            )
        })?;
        Some((concrete.zoom * 100.).round() as u32)
    });

    // Outer BG fill — full window, the editor background (`#171717`).
    v_flex()
        .size_full()
        .bg(color::BG)
        .child(
            // AppTitleBar — 46px (title bar)
            div()
                .h(px(TITLEBAR_H))
                .child(title_bar::render(window, state, cx)),
        )
        .child(
            // The main row: takes the spare width, shrinks below its content.
            h_flex()
                .flex_1()
                .min_h(px(0.))
                .items_stretch()
                .bg(color::BG)
                .when(chrome_visible, |el| {
                    el.child(
                        // LeftToolRail — 52px, flex_none
                        div()
                            .w(px(RAIL_W))
                            .h_full()
                            .flex_none()
                            .bg(color::BG)
                            .child(tool_rail::render(state, cx)),
                    )
                })
                .child(
                    // The editor/dock row: stretches, shrinks, clipped to its box.
                    h_flex()
                        .flex_1()
                        .min_w(px(0.))
                        .items_stretch()
                        .overflow_hidden()
                        .child(
                            // The canvas column: stacked, stretches, may shrink.
                            v_flex()
                                .flex_1()
                                .min_w(px(0.))
                                .items_stretch()
                                .overflow_hidden()
                                .when(has_document, |el| {
                                    el.child(
                                        // DocumentTabsBar — 38px
                                        div().h(px(TABBAR_H)).child(document_tabs::render(
                                            &tabs,
                                            active_tab.as_deref(),
                                            cx,
                                        )),
                                    )
                                })
                                .when(has_document && chrome_visible, |el| {
                                    el.child(
                                        // OptionBar — 44px
                                        div().h(px(OPTIONBAR_H)).child(option_bar::render(
                                            state,
                                            f32::from(window.viewport_size().width)
                                                - RAIL_W
                                                - if dock_open { dock_w } else { 0.0 },
                                            committed_selection,
                                            transform_readout,
                                            cx,
                                        )),
                                    )
                                })
                                .child(
                                    // CanvasViewport - flex_1, min_h_0. With a
                                    // document open, the display seam paints
                                    // it (`render/canvas_bridge`); without
                                    // one, the flat canvas colour stays. The
                                    // document itself is `photrez-core`'s
                                    // territory, not chrome.
                                    match (has_document, canvas) {
                                        (true, Some(canvas_image)) => {
                                            let doc_size = canvas_image.dimensions();
                                            v_flex()
                                                .relative()
                                                .flex_1()
                                                .min_h(px(0.0))
                                                .w_full()
                                                .child(canvas_bridge::canvas_element_with_bounds(
                                                    canvas_image,
                                                    Rc::clone(&canvas_bounds),
                                                    camera,
                                                    Some(Rc::clone(&checker)),
                                                    overlay,
                                                ))
                                                .child(canvas_hitbox(
                                                    Rc::clone(&canvas_bounds),
                                                    doc_size,
                                                    cursor,
                                                    focus_handle,
                                                    Rc::clone(&capture),
                                                    text_editing,
                                                    cx,
                                                ))
                                                .when_some(text_edit.clone(), |el, session| {
                                                    el.child(text_editor_overlay(
                                                        &session,
                                                        canvas_image,
                                                        camera,
                                                        &canvas_bounds,
                                                    ))
                                                })
                                                .when_some(hud, |el, hud| {
                                                    let origin = canvas_bounds
                                                        .get()
                                                        .map(|bounds| {
                                                            (
                                                                f32::from(bounds.origin.x),
                                                                f32::from(bounds.origin.y),
                                                            )
                                                        })
                                                        .unwrap_or((0.0, 0.0));
                                                    el.child(super::transform_hud::render(
                                                        hud, origin,
                                                    ))
                                                })
                                                .when_some(gradient_badge, |el, badge| {
                                                    let origin = canvas_bounds
                                                        .get()
                                                        .map(|bounds| {
                                                            (
                                                                f32::from(bounds.origin.x),
                                                                f32::from(bounds.origin.y),
                                                            )
                                                        })
                                                        .unwrap_or((0.0, 0.0));
                                                    el.child(super::gradient_badge::render(
                                                        badge, origin,
                                                    ))
                                                })
                                                .into_any_element()
                                        }
                                        _ => super::file_drop_surface::FileDropSurface::new(
                                            empty_state::render_live(cx),
                                            cx.weak_entity(),
                                        )
                                        .into_any_element(),
                                    },
                                ),
                        )
                        // RightDock — only when dock_open and the chrome shows
                        .when(dock_open && chrome_visible, |el| {
                            el.child(
                                div()
                                    .w(px(dock_w))
                                    .h_full()
                                    .flex_none()
                                    .bg(color::BG)
                                    .child(right_dock::render(
                                        state,
                                        canvas,
                                        camera,
                                        zoom_percent,
                                        two_xl,
                                        Rc::clone(&canvas_bounds),
                                        Rc::clone(&navigator_bounds),
                                        layer_thumbs,
                                        transform_readout,
                                        cx,
                                    )),
                            )
                        }),
                ),
        )
        // Status bar — 24px, at the very bottom
        .when(chrome_visible, |el| {
            el.child(div().h(px(STATUSBAR_H)).child(status_bar::render(
                status_error.as_deref(),
                zoom_percent,
                document_size,
                active_tool,
                selected_layer_name.as_deref(),
                autosave_status,
                state,
                cx,
            )))
        })
        // Overlays last on the root — GPUI has no z_index, paint order is tree
        // order. Each is gated on the shot id so a plain launch still renders
        // the default shell unchanged.
        .when(selection_flyout_open, |el| {
            el.child(tool_rail::render_flyout(state, selection_shape, cx))
        })
        .when_some(open_menu, |el, menu| {
            el.child(menus::render_open(
                state,
                menu,
                has_pixel_selection,
                has_copyable_selection,
                has_selection_clipboard,
                can_undo,
                can_redo,
                cx,
            ))
        })
        .when_some(context_menu, |el, position| {
            el.child(menus::render_context(
                state,
                position,
                has_pixel_selection,
                has_copyable_selection,
                has_selection_clipboard,
                cx,
            ))
        })
        .when_some(brush_menu, |el, _| {
            el.child(brush_menu::render(state, window, cx))
        })
        .when_some(active_dialog, |el, dialog| {
            el.child(dialogs::render_dialog(
                state,
                dialog,
                document_size,
                rename_focus,
                resize_focus,
                new_document_focus,
                dialog_focus,
                cx,
            ))
        })
        .when(color_picker_open, |el| {
            el.child(color_picker::render(state, picker_focus, cx))
        })
        .when(shot::starts_with("ctx-"), |el| {
            el.child(context_menus::render())
        })
        .when(shot::starts_with("empty-"), |el| {
            // The empty card is centred in the canvas column, which
            // moves with the viewport (and with the `2xl` dock widths). The
            // card is positioned in window coordinates, so the column's left
            // edge (the rail width) is part of the answer.
            let col_left = RAIL_W;
            let col_w = f32::from(window.viewport_size().width) - RAIL_W - dock_w;
            el.child(empty_state::render(col_left, col_w))
        })
        .when(shot::starts_with("tooltip-"), |el| {
            el.child(tooltips::render())
        })
        .when(shot::starts_with("menu-"), |el| {
            el.child(menus::render(state, cx))
        })
        .when(shot::starts_with("dialog-"), |el| {
            el.child(dialogs::render())
        })
        .when(shot::starts_with("loading-"), |el| {
            el.child(loading::render())
        })
        // A hovered rail button's own tooltip, painted here because the panel
        // overlaps the canvas and GPUI has no z-index. The `tooltip-*` fixture
        // above is the captured equivalent.
        .child(tooltips::render_live(state, cx))
        // Last child in the window-sized root: the host stacks at level 50
        // there, and GPUI paints in tree order, so anything appended earlier
        // sits under the dock (`views::toasts` module docs).
        .child(if shot::starts_with("toast-") {
            toasts::render().into_any_element()
        } else {
            toasts::render_live(state, cx).into_any_element()
        })
}

/// Transparent pointer surface over the canvas slot.
///
/// Its bounds are the same element bounds the display seam publishes, so the
/// handler maps through the exact fit placement that was painted.
/// The in-place text editor: a panel anchored at the layer's document origin.
/// The overlay draws the live buffer on the layer's own metrics so typing is
/// WYSIWYG, and the hint strip is the text editor's key legend.
fn text_editor_overlay(
    session: &crate::state::TextEditSession,
    canvas: &CanvasImage,
    camera: Option<Camera>,
    bounds: &Rc<Cell<Option<Bounds<Pixels>>>>,
) -> impl IntoElement {
    let mut left = 0.0f32;
    let mut top = 0.0f32;
    let mut zoom = 1.0f32;
    if let Some(bounds) = bounds.get() {
        let (doc_w, doc_h) = canvas.dimensions();
        if let Some((page_x, page_y, page_w, _)) = canvas_bridge::camera_placement(
            bounds.size.width.into(),
            bounds.size.height.into(),
            doc_w as f32,
            doc_h as f32,
            camera,
        ) {
            zoom = page_w / doc_w as f32;
            left = page_x + session.doc_x as f32 * zoom;
            top = page_y + session.doc_y as f32 * zoom;
        }
    }
    let font_size = (session.data.font_size as f32) * zoom;
    let box_w = (session.preview.0 as f32) * zoom;
    let box_h = (session.preview.1 as f32) * zoom;
    let _ = font_size;
    div()
        .id("text-editor")
        .test_support()
        .absolute()
        .left(px(left - RING))
        .top(px(top - RING))
        .flex()
        .flex_col()
        .items_start()
        .child(
            // The edit box's double ring, from the 0.1 build's two box-shadows.
            // A GPUI drop shadow fills its rect instead of cutting a ring, and
            // this box has no fill to hide that, so the bands are borders on two
            // wrappers around the box. They add RING on every side, so the
            // overlay's origin and the hint strip's offsets shift back by it.
            div()
                .border_2()
                .border_color(gpui_kit::hsla(0., 0., 1., 0.4))
                .rounded(px(6.0))
                .child(
                    div()
                        .border_1()
                        .border_color(gpui_kit::hsla(0., 0., 0., 0.8))
                        .rounded(px(4.0))
                        .child(
                            div()
                                .id("text-editor-box")
                                .test_support()
                                .w(px(box_w.max(24.0)))
                                .h(px(box_h.max(font_size + 12.0)))
                                .rounded(px(3.0))
                                .border_1()
                                .border_color(color::ACCENT),
                        ),
                ),
        )
        .child(
            h_flex()
                .id("text-editor-hint")
                .test_support()
                .mt(px(12.0 - RING))
                .ml(px((font_size
                    * 0.5
                    * session.content.chars().count() as f32
                    + 12.0
                    - HINT_W)
                    / 2.0
                    + RING))
                .w(px(HINT_W))
                .justify_center()
                .items_center()
                .gap(px(6.0))
                .rounded(px(3.0))
                .border_1()
                .border_color(color::FIELD_BORDER)
                .bg(color::PANEL)
                .px(px(8.0))
                .py(px(2.0))
                .font_family(crate::components::ui::FONT_UI)
                .text_size(px(10.0))
                .text_color(color::TEXT_DIM)
                .child(
                    h_flex()
                        .gap(px(4.0))
                        .child(
                            div()
                                .font_weight(gpui_kit::FontWeight(600.))
                                .text_color(color::TEXT)
                                .child("\u{21b5}"), // i18n-exempt: keyboard glyph in the overlay legend
                        )
                        .child("Newline"), // i18n-exempt: overlay legend
                )
                .child(div().text_color(color::TEXT_DIM).child("\u{00b7}")) // i18n-exempt: legend separator glyph
                .child(
                    h_flex()
                        .gap(px(4.0))
                        .child(
                            div()
                                .font_weight(gpui_kit::FontWeight(600.))
                                .text_color(color::ACCENT)
                                .child("Ctrl+Enter"), // i18n-exempt: keyboard token in the overlay legend
                        )
                        .child("Commit"), // i18n-exempt: overlay legend
                )
                // i18n-exempt: legend separator glyph
                .child(div().text_color(color::TEXT_DIM).child("\u{00b7}")) // i18n-exempt: legend separator glyph
                .child(
                    h_flex()
                        .gap(px(4.0))
                        .child(
                            div()
                                .font_weight(gpui_kit::FontWeight(600.))
                                .text_color(color::TEXT)
                                .child("Esc"), // i18n-exempt: keyboard token in the overlay legend
                        )
                        .child("Cancel"), // i18n-exempt: overlay legend
                ),
        )
}

fn canvas_hitbox(
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    doc_size: (u32, u32),
    cursor: crate::input_policy::CursorPolicy,
    focus_handle: &gpui_kit::FocusHandle,
    capture: Rc<Cell<Option<gpui_kit::HitboxId>>>,
    text_editing: bool,
    cx: &mut Context<crate::app::App>,
) -> impl IntoElement {
    let hitbox = div()
        .id("canvas-hitbox")
        .test_support()
        .track_focus(focus_handle)
        .key_context(if text_editing { "TextEdit" } else { "Canvas" })
        .on_key_down(
            cx.listener(|app: &mut crate::app::App, event, _window, cx| {
                app.canvas_space_down(event, cx)
            }),
        )
        .on_key_up(
            cx.listener(|app: &mut crate::app::App, event, _window, cx| {
                app.canvas_space_up(event, cx)
            }),
        )
        .on_scroll_wheel(
            cx.listener(|app: &mut crate::app::App, event, _window, cx| {
                app.canvas_scroll(event, cx)
            }),
        )
        .on_drop::<crate::app::cross_doc::LayerDragPayload>({
            let drop_bounds = Rc::clone(&bounds);
            cx.listener(
                move |app: &mut crate::app::App,
                      payload: &crate::app::cross_doc::LayerDragPayload,
                      window,
                      cx| {
                    let at = crate::app::canvas_doc_coords_free(
                        window.mouse_position(),
                        &drop_bounds,
                        doc_size,
                        app.active_camera(),
                    )
                    .unwrap_or((doc_size.0 as f64 / 2.0, doc_size.1 as f64 / 2.0));
                    if let Ok(doc_id) = app.active_doc_id() {
                        app.apply_layer_drag(
                            payload,
                            crate::app::cross_doc::LayerDropTarget::Canvas { doc_id, at },
                            window.modifiers().alt,
                            cx,
                        );
                    }
                },
            )
        })
        .absolute()
        .inset(px(0.0))
        // The focus ring is the 1px border only. A zero-blur spread shadow is
        // painted across the element's interior as a tint, which covered the
        // whole page in 55% black after any keyboard shortcut.
        .focus_visible(|style| style.border_1().border_color(color::ACCENT));
    let hitbox = match cursor {
        crate::input_policy::CursorPolicy::SystemDefault => hitbox,
        crate::input_policy::CursorPolicy::Crosshair => {
            hitbox.cursor(gpui_kit::CursorStyle::Crosshair)
        }
        crate::input_policy::CursorPolicy::Text => hitbox.cursor(gpui_kit::CursorStyle::IBeam),
        crate::input_policy::CursorPolicy::Grabbing => {
            hitbox.cursor(gpui_kit::CursorStyle::ClosedHand)
        }
        crate::input_policy::CursorPolicy::Cloning => {
            hitbox.cursor(gpui_kit::CursorStyle::DragCopy)
        }
        crate::input_policy::CursorPolicy::OpenHand => {
            hitbox.cursor(gpui_kit::CursorStyle::OpenHand)
        }
        crate::input_policy::CursorPolicy::ResizeNwse => {
            hitbox.cursor(gpui_kit::CursorStyle::ResizeUpLeftDownRight)
        }
        crate::input_policy::CursorPolicy::ResizeNesw => {
            hitbox.cursor(gpui_kit::CursorStyle::ResizeUpRightDownLeft)
        }
        crate::input_policy::CursorPolicy::ResizeEw => {
            hitbox.cursor(gpui_kit::CursorStyle::ResizeLeftRight)
        }
        crate::input_policy::CursorPolicy::ResizeNs => {
            hitbox.cursor(gpui_kit::CursorStyle::ResizeUpDown)
        }
        crate::input_policy::CursorPolicy::NotAllowed => {
            hitbox.cursor(gpui_kit::CursorStyle::OperationNotAllowed)
        }
    };
    crate::views::canvas_surface::CanvasPointerSurface::new(
        hitbox,
        cx.weak_entity(),
        bounds,
        capture,
        doc_size,
    )
}
