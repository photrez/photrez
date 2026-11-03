//! The live brush/eraser right-click menu.
//!
//! The static shot of this panel (`ctx-brush`) lives in `context_menus.rs`;
//! this module is the production overlay: a 200 px panel at the pointer,
//! clamped into the window, with three live 14 px slider tracks, the 2-column
//! preset grid and Reset. The panel reads and writes `AppState`'s paint
//! settings through the shared slider surface, so a drag keeps tracking
//! outside the track.

use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, relative, Div, Entity, FontWeight, TestSupportExt};

use crate::app::tool_settings::BRUSH_PRESETS;
use crate::components::ui::{self, SliderKind};
use crate::i18n::t;
use crate::state::{AppState, SliderTarget, ToolId};
use crate::tokens::color;
use crate::views::option_bar::slider_surface::BarSliderSurface;

const MENU_W: f32 = 200.0;
const MENU_H: f32 = 260.0;
const PAD: f32 = 12.0;
const GAP: f32 = 12.0;
const BLOCK_GAP: f32 = 4.0;
const SLIDER_H: f32 = 14.0;
const BTN_TEXT: f32 = 10.0;
const BTN_PX: f32 = 8.0;
const BTN_PY: f32 = 4.0;
const R3: f32 = 3.0;

const PRESET_NAMES: [&str; 6] = [
    "Hard Round",
    "Soft Round",
    "Detail",
    "Large Soft",
    "Hard Eraser",
    "Soft Eraser",
]; // i18n-exempt: brush preset names (no catalog key)

pub(crate) fn render(
    state: &Entity<AppState>,
    window: &gpui_kit::Window,
    cx: &gpui_kit::Context<crate::app::App>,
) -> impl IntoElement {
    let Some((x, y)) = state.read(cx).brush_menu else {
        return div();
    };
    let viewport = window.viewport_size();
    let left = x.min((f32::from(viewport.width) - (MENU_W + 20.0)).max(0.0));
    let top = y.min((f32::from(viewport.height) - MENU_H).max(0.0));

    let (eraser, size, hardness, opacity, preset) = {
        let s = state.read(cx);
        let eraser = s.active_tool == ToolId::Eraser;
        if eraser {
            (
                true,
                s.eraser_size,
                s.eraser_hardness,
                s.eraser_opacity,
                s.brush_preset_index,
            )
        } else {
            (
                false,
                s.brush_size,
                s.brush_hardness,
                s.brush_opacity,
                s.brush_preset_index,
            )
        }
    };

    let close = cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
        app.close_brush_menu(cx);
    });
    let stop = cx.listener(|_app, _: &gpui_kit::ClickEvent, _window, cx| {
        cx.stop_propagation();
    });

    let size_percent = crate::app::brush_menu::paint_size_to_slider(size) as f32;
    let hardness_percent = (hardness * 100.0).round() as f32;
    let strength_percent = (opacity * 100.0).round() as f32;
    let (size_bounds, hardness_bounds, strength_bounds, capture) = {
        let s = state.read(cx);
        (
            s.brush_menu_size_bounds.clone(),
            s.brush_menu_hardness_bounds.clone(),
            s.brush_menu_strength_bounds.clone(),
            s.brush_menu_capture.clone(),
        )
    };

    let slider_row =
        |id: &'static str,
         label: String,
         visual: Div,
         target: SliderTarget,
         bounds: std::rc::Rc<std::cell::Cell<Option<gpui_kit::Bounds<gpui_kit::Pixels>>>>|
         -> Div {
            // A click focuses the track and arrows step the same 0..=100
            // position a drag sets.
            let focus = state.read(cx).slider_focus_for(target);
            let mut track = div()
                .id(id)
                .test_support()
                .w_full()
                .h(px(SLIDER_H))
                .aria_label(label.clone());
            if let Some(handle) = focus.as_ref() {
                track = track.track_focus(handle).on_key_down(cx.listener(
                    move |app, event: &gpui_kit::KeyDownEvent, _window, cx| {
                        app.slider_key(target, event, cx);
                    },
                ));
            }
            let track = track.child(BarSliderSurface::new(
                visual,
                cx.entity().downgrade(),
                target,
                bounds,
                capture.clone(),
                focus,
            ));
            v_flex()
                .w_full()
                .gap(px(BLOCK_GAP))
                .child(
                    div()
                        .text_size(px(BTN_TEXT))
                        .font_family(ui::FONT_UI)
                        .font_weight(FontWeight(500.0))
                        .line_height(relative(1.5))
                        .text_color(color::TEXT_DIM)
                        .child(label),
                )
                .child(
                    h_flex()
                        .w_full()
                        .h(px(SLIDER_H))
                        .items_center()
                        .child(track),
                )
        };

    let mut presets = v_flex().w_full().gap(px(BLOCK_GAP));
    let visible: Vec<usize> = BRUSH_PRESETS
        .iter()
        .enumerate()
        .filter(|(_, preset)| eraser || !preset.5)
        .map(|(index, _)| index)
        .collect();
    for chunk in visible.chunks(2) {
        let mut row = h_flex().w_full().gap(px(BLOCK_GAP));
        for &index in chunk {
            let selected = preset == index;
            row = row.child(
                h_flex()
                    .id(format!("brush-menu-preset-{index}"))
                    .test_support()
                    .flex_1()
                    .items_center()
                    .rounded(px(R3))
                    .border_1()
                    .border_color(if selected {
                        gpui_kit::Hsla {
                            h: color::ACCENT.h,
                            s: color::ACCENT.s,
                            l: color::ACCENT.l,
                            a: 0.4,
                        }
                    } else {
                        color::FIELD_BORDER
                    })
                    .bg(color::FIELD)
                    .px(px(BTN_PX))
                    .py(px(BTN_PY))
                    .text_size(px(BTN_TEXT))
                    .font_family(ui::FONT_UI)
                    .line_height(relative(1.5))
                    .text_color(if selected { color::ACCENT } else { color::TEXT })
                    .cursor_pointer()
                    .on_click(
                        cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                            cx.stop_propagation();
                            app.brush_menu_apply_preset(index, cx);
                        }),
                    )
                    .child(
                        PRESET_NAMES
                            .get(index)
                            .copied()
                            .unwrap_or("Preset")
                            .to_string(),
                    ),
            );
        }
        presets = presets.child(row);
    }

    let reset = div()
        .id("brush-menu-reset")
        .test_support()
        .w_full()
        .rounded(px(R3))
        .border_1()
        .border_color(color::FIELD_BORDER)
        .bg(color::FIELD)
        .px(px(BTN_PX))
        .py(px(BTN_PY))
        .text_size(px(BTN_TEXT))
        .font_family(ui::FONT_UI)
        .line_height(relative(1.5))
        .text_color(color::TEXT)
        .cursor_pointer()
        .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
            cx.stop_propagation();
            app.brush_menu_reset(cx);
        }))
        .child(t("common.reset").to_string());

    let panel = v_flex()
        .id("brush-menu")
        .test_support()
        .absolute()
        .left(px(left))
        .top(px(top))
        .w(px(MENU_W))
        .flex()
        .flex_col()
        .gap(px(GAP))
        .rounded(px(6.0))
        .border_1()
        .border_color(color::FIELD_BORDER)
        .bg(color::PANEL)
        .p(px(PAD))
        // Large drop shadow: 0 20px 25px -5px / 0 8px 10px -6px at 10% black.
        // GPUI has no spread, so only the offsets and blurs carry over.
        .shadow(vec![
            gpui_kit::BoxShadow::new(px(0.0), px(20.0), gpui_kit::hsla(0.0, 0.0, 0.0, 0.10))
                .blur_radius(px(25.0)),
            gpui_kit::BoxShadow::new(px(0.0), px(8.0), gpui_kit::hsla(0.0, 0.0, 0.0, 0.10))
                .blur_radius(px(10.0)),
        ])
        .on_click(stop)
        .child(slider_row(
            "brush-menu-size",
            format!("{}: {:.0}px", t("tools.options.size"), size),
            ui::slider(size_percent, None, SliderKind::BrushSize, true),
            SliderTarget::BrushMenuSize,
            size_bounds,
        ))
        .child(slider_row(
            "brush-menu-hardness",
            format!(
                "{}: {}%",
                t("tools.options.hardness"),
                hardness_percent as i32
            ),
            ui::slider(hardness_percent, None, SliderKind::BrushHardness, true),
            SliderTarget::BrushMenuHardness,
            hardness_bounds,
        ))
        .child(slider_row(
            "brush-menu-strength",
            format!(
                "{}: {}%",
                t("tools.options.strength"),
                strength_percent as i32
            ),
            ui::slider(strength_percent, None, SliderKind::Alpha, true),
            SliderTarget::BrushMenuStrength,
            strength_bounds,
        ))
        .child(presets)
        .child(reset);

    v_flex()
        .absolute()
        .inset(px(0.0))
        .child(
            div()
                .id("brush-menu-away")
                .test_support()
                .absolute()
                .inset(px(0.0))
                .on_click(close),
        )
        .child(panel)
}
