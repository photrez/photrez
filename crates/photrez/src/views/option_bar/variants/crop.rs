//! The Crop bar: its parameters and items, and the ratio, unit and size machinery
//! the ratio control opens.

use super::*;

/// Resting crop bar: `cropMode="free"` → ratio reads "Free"; both `Delete` and
/// `Fill BG` ship checked; the fill colour block is visible (`fill_enabled`).
pub fn crop_params() -> CropParams {
    CropParams {
        pill_label: PILL_LABEL_CROP_KEY,
        ratio_prefix: "tools.options.aspectRatio",
        ratio_value: "crop.free",
        delete_id: "opt-crop-delete",
        delete_label: "Delete", // i18n-exempt: not in catalog
        delete_checked: true,
        fill_id: "opt-crop-fill",
        fill_label: "Fill BG", // i18n-exempt: not in catalog
        fill_checked: true,
        use_bg_label: "Use BG", // i18n-exempt: not in catalog
        cancel_label: "common.cancel",
        apply_label: "common.apply",
    }
}

/// The `Crop` variant — the resting 781px capture.
///
/// The ratio mode rests on `"free"`, the ratio dropdown is closed, and
/// `fill_enabled` is true. The advanced group (straighten / rotate / guide /
/// classic toggle), which needs 880 px, stays hidden, so none of those paint.
/// Element order reads, line-for-line:
///
/// `pill` ⟵ `Divider` ⟵ `Aspect Ratio: Free` ⟵ `Divider` ⟵ `Delete` ⟵
/// `Fill BG` ⟵ `white swatch + Use BG` ⟵ `Divider` ⟵ `Cancel` ⟵ `Apply`.
///
/// No overflow trigger is painted between dividers: the crop bar
/// contributes no overflow group, and the guide and classic-mode selectors it
/// would open are not implemented, so a painted trigger would open nothing.
/// One divider still separates the swatch group from `Cancel`.
pub(super) fn crop_items(
    state: &gpui_kit::Entity<AppState>,
    row: Div,
    cx: &Context<crate::app::App>,
) -> Div {
    let (
        delete_checked,
        fill_checked,
        mode,
        aspect,
        recents,
        ratio_open,
        size_w,
        size_h,
        unit,
        unit_open,
        edit,
        edit_custom,
        field,
    ) = {
        let s = state.read(cx);
        (
            s.crop_delete_pixels,
            s.crop_fill_background,
            s.crop_mode,
            s.crop_aspect,
            s.recent_crop_ratios.clone(),
            s.crop_ratio_open,
            s.crop_size_w.clone(),
            s.crop_size_h.clone(),
            s.crop_size_unit,
            s.crop_unit_open,
            s.crop_edit.clone(),
            s.crop_edit_custom,
            s.crop_size_field,
        )
    };
    let custom_active = mode == crate::crop_aspect::CropMode::Ratio
        && aspect.is_some_and(|a| {
            !crate::crop_aspect::CROP_PRESETS
                .iter()
                .any(|(p, _)| *p == a)
        });
    let (ratio_bounds, unit_bounds) = {
        let s = state.read(cx);
        (s.crop_ratio_bounds.clone(), s.crop_unit_bounds.clone())
    };
    let p = crop_params();
    row.child(tool_pill(
        icons::with_stroke(icons::CROP, STROKE_PILL),
        t(p.pill_label),
    ))
    .child(divider())
    .child(crop_ratio_control(
        &t(p.ratio_prefix),
        mode,
        aspect,
        &recents,
        ratio_open,
        ratio_bounds,
        cx,
    ))
    .when(custom_active, |row| {
        row.child(divider()).child(crop_custom_fields(
            aspect,
            edit.as_deref(),
            edit_custom,
            field,
            cx,
        ))
    })
    .when(mode == crate::crop_aspect::CropMode::Size, |row| {
        row.child(divider()).child(crop_size_fields(
            &size_w,
            &size_h,
            unit,
            unit_open,
            edit.as_deref(),
            !edit_custom,
            field,
            unit_bounds,
            cx,
        ))
    })
    .child(divider())
    .child(option_checkbox_action(
        p.delete_id,
        p.delete_label,
        delete_checked,
        None,
        cx,
        |app, cx| app.toggle_crop_delete(cx),
    ))
    .child(option_checkbox_action(
        p.fill_id,
        p.fill_label,
        fill_checked,
        None,
        cx,
        |app, cx| app.toggle_crop_fill(cx),
    ))
    .child(crop_fill_swatch_group())
    .child(divider())
    .child(cancel_button(cx))
    .child(apply_button(cx))
}

/// The `Aspect Ratio` button plus its inline list: lock the current
/// shape, the three recents, Free / Custom / Size, then the preset list.
pub(super) fn crop_ratio_control(
    prefix: &str,
    mode: crate::crop_aspect::CropMode,
    aspect: Option<(u32, u32)>,
    recents: &[(u32, u32)],
    open: bool,
    _bounds: std::rc::Rc<std::cell::Cell<Option<gpui_kit::Bounds<gpui_kit::Pixels>>>>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    use crate::crop_aspect::ratio_label;

    let trigger = h_flex()
        .id("crop-ratio")
        .test_support()
        .h(px(CTRL_H))
        .flex_shrink_0()
        .items_center()
        .gap(px(GAP))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .px(px(PILL_PX))
        .text_size(px(TEXT_SM))
        .font_weight(FontWeight(500.))
        .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
            cx.stop_propagation();
            app.toggle_crop_ratio(cx);
        }))
        .child(
            h_flex()
                .child(div().text_color(ZINC).child(format!("{prefix}: ")))
                .child(
                    div()
                        .font_weight(FontWeight(600.))
                        .text_color(color::WHITE)
                        .child(ratio_label(mode, aspect)),
                )
                .child(
                    Icon::default()
                        .data(icons::with_stroke(icons::CHEVRON_DOWN, 1.75))
                        .size(px(ICON_SM))
                        .text_color(ZINC),
                ),
        );

    let panel = crop_ratio_list(mode, aspect, recents, cx);

    div()
        .relative()
        .flex_shrink_0()
        .child(trigger)
        .when(open, |el| {
            el.child(gpui_kit::deferred(
                div()
                    .absolute()
                    .top(px(CTRL_H + 6.0))
                    .left(px(0.0))
                    .child(panel),
            ))
        })
}

pub(super) fn crop_ratio_list(
    mode: crate::crop_aspect::CropMode,
    aspect: Option<(u32, u32)>,
    recents: &[(u32, u32)],
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    use crate::crop_aspect::{CropMode, CROP_PRESETS};
    let custom_active = mode == CropMode::Ratio
        && aspect.is_some_and(|a| !CROP_PRESETS.iter().any(|(p, _)| *p == a));
    let mut panel = gpui_kit::base::v_flex()
        .id("crop-ratio-list")
        .test_support()
        .w(px(160.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::MENU_LIST)
        .py(px(4.0))
        .max_h(px(300.0))
        .overflow_y_scroll()
        .child(crop_ratio_action_row(
            "crop-ratio-lock",
            Some(icons::LOCK),
            "Lock Current Shape",
            false,
            mode == CropMode::Free,
            cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
                cx.stop_propagation();
                app.crop_lock_shape(cx);
            }),
        ))
        .child(crop_ratio_divider());
    if !recents.is_empty() {
        panel = panel.child(crop_ratio_caption(&t("common.recents")));
        for (index, recent) in recents.iter().enumerate() {
            let value = *recent;
            panel = panel.child(crop_ratio_action_row(
                &format!("crop-ratio-recent-{index}"),
                None,
                &format!("{}:{}", value.0, value.1),
                false,
                true,
                cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                    cx.stop_propagation();
                    app.crop_choose_ratio(value, cx);
                }),
            ));
        }
        panel = panel.child(crop_ratio_divider());
    }
    panel = panel
        .child(crop_ratio_action_row(
            "crop-ratio-free",
            None,
            "Free",
            mode == CropMode::Free,
            true,
            cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
                cx.stop_propagation();
                app.crop_choose_free(cx);
            }),
        ))
        .child(crop_ratio_action_row(
            "crop-ratio-custom",
            None,
            "Custom...",
            custom_active,
            true,
            cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
                cx.stop_propagation();
                app.crop_choose_custom(cx);
            }),
        ))
        .child(crop_ratio_action_row(
            "crop-ratio-size",
            None,
            "Size",
            mode == CropMode::Size,
            true,
            cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
                cx.stop_propagation();
                app.crop_choose_size(cx);
            }),
        ))
        .child(crop_ratio_divider())
        .child(crop_ratio_caption("Presets")); // i18n-exempt: fixed section caption, not in catalog
    for (index, (preset, label)) in CROP_PRESETS.iter().enumerate() {
        let value = *preset;
        panel = panel.child(crop_ratio_preset_row(
            &format!("crop-ratio-preset-{index}"),
            label,
            mode == CropMode::Ratio && aspect == Some(value),
            cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                cx.stop_propagation();
                app.crop_choose_ratio(value, cx);
            }),
        ));
    }
    panel
}

pub(super) fn crop_ratio_divider() -> impl IntoElement {
    div().w_full().h(px(1.0)).my(px(4.0)).bg(color::DIVIDER)
}

pub(super) fn crop_ratio_caption(label: &str) -> impl IntoElement {
    div()
        .px(px(12.0))
        .py(px(2.0))
        .text_size(px(9.0))
        .font_weight(FontWeight(700.))
        .text_color(color::TEXT_DIM)
        .child(label.to_uppercase())
}

#[allow(clippy::too_many_arguments)] // id, icon, label, tone, gate and listener are one row record
pub(super) fn crop_ratio_action_row(
    id: &str,
    icon: Option<&'static [u8]>,
    label: &str,
    active: bool,
    enabled: bool,
    listener: impl Fn(&gpui_kit::ClickEvent, &mut gpui_kit::Window, &mut gpui_kit::App) + 'static,
) -> impl IntoElement {
    let mut row = h_flex()
        .id(id.to_string())
        .test_support()
        .w_full()
        .items_center()
        .gap(px(GAP))
        .px(px(12.0))
        .py(px(6.0))
        .text_size(px(TEXT_SM))
        .text_color(if active { color::WHITE } else { ZINC })
        .font_weight(if active {
            FontWeight(600.)
        } else {
            FontWeight(400.)
        })
        .when(active, |el| {
            el.border_l(px(2.0))
                .border_color(color::BORDER)
                .bg(color::ACCENT.opacity(0.2))
        });
    if let Some(icon) = icon {
        row = row.child(
            Icon::default()
                .data(icon)
                .size(px(ICON_SM))
                .text_color(if active { color::WHITE } else { ZINC }),
        );
    }
    row = row.child(div().child(label.to_string()));
    if enabled {
        row.hover(|style| style.bg(color::WHITE_10))
            .on_click(listener)
    } else {
        row.opacity(0.4)
    }
}

pub(super) fn crop_ratio_preset_row(
    id: &str,
    label: &str,
    active: bool,
    listener: impl Fn(&gpui_kit::ClickEvent, &mut gpui_kit::Window, &mut gpui_kit::App) + 'static,
) -> impl IntoElement {
    h_flex()
        .id(id.to_string())
        .test_support()
        .w_full()
        .items_center()
        .px(px(12.0))
        .py(px(6.0))
        .text_size(px(TEXT_SM))
        .text_color(if active { color::ACCENT } else { color::TEXT })
        .font_weight(if active {
            FontWeight(500.)
        } else {
            FontWeight(400.)
        })
        .hover(|style| style.bg(color::FIELD.opacity(0.6)))
        .on_click(listener)
        .child(label.to_string())
}

/// The crop bar's custom ratio fields: two integer W/H fields.
pub(super) fn crop_custom_fields(
    aspect: Option<(u32, u32)>,
    edit: Option<&str>,
    editing_custom: bool,
    field: usize,
    cx: &Context<crate::app::App>,
) -> Div {
    let (w, h) = aspect.unwrap_or((1, 1));
    let w_text = if editing_custom && edit.is_some() && field == 0 {
        edit.unwrap_or("").to_string()
    } else {
        w.to_string()
    };
    let h_text = if editing_custom && edit.is_some() && field == 1 {
        edit.unwrap_or("").to_string()
    } else {
        h.to_string()
    };
    h_flex()
        .flex_shrink_0()
        .gap(px(GAP))
        .child(crop_edit_field(
            "crop-custom-w",
            "W",
            &w_text,
            "",
            cx,
            0,
            true,
        ))
        .child(crop_edit_field(
            "crop-custom-h",
            "H",
            &h_text,
            "",
            cx,
            1,
            true,
        ))
}

/// The Size fields: W/H with the unit as suffix, a swap button and the unit
/// list (expanded inline, like the language list in Settings).
// Separate field and geometry inputs mirror the two rendered size fields.
#[allow(clippy::too_many_arguments)]
pub(super) fn crop_size_fields(
    w: &str,
    h: &str,
    unit: crate::crop_aspect::CropSizeUnit,
    unit_open: bool,
    edit: Option<&str>,
    editing_size: bool,
    field: usize,
    _bounds: std::rc::Rc<std::cell::Cell<Option<gpui_kit::Bounds<gpui_kit::Pixels>>>>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let w_text = if editing_size && edit.is_some() && field == 0 {
        edit.unwrap_or("").to_string()
    } else {
        w.to_string()
    };
    let h_text = if editing_size && edit.is_some() && field == 1 {
        edit.unwrap_or("").to_string()
    } else {
        h.to_string()
    };
    let suffix = unit.label();
    div()
        .relative()
        .flex_shrink_0()
        .child(
            h_flex()
                .gap(px(GAP))
                .child(crop_edit_field(
                    "crop-size-w",
                    "W",
                    &w_text,
                    suffix,
                    cx,
                    0,
                    false,
                ))
                .child(
                    div()
                        .id("crop-size-swap")
                        .test_support()
                        .h(px(CTRL_H))
                        .w(px(CTRL_H))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(RADIUS))
                        .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
                            cx.stop_propagation();
                            app.crop_swap_size(cx);
                        }))
                        .child(
                            Icon::default()
                                .data(icons::SWAP)
                                .size(px(12.0))
                                .text_color(color::ICON),
                        ),
                )
                .child(crop_edit_field(
                    "crop-size-h",
                    "H",
                    &h_text,
                    suffix,
                    cx,
                    1,
                    false,
                ))
                .child(
                    h_flex()
                        .id("crop-size-unit")
                        .test_support()
                        .h(px(CTRL_H))
                        .flex_shrink_0()
                        .items_center()
                        .gap(px(GAP))
                        .rounded(px(RADIUS))
                        .border_1()
                        .border_color(color::BORDER)
                        .bg(color::FIELD)
                        .px(px(PILL_PX))
                        .text_size(px(TEXT_SM))
                        .font_weight(FontWeight(600.))
                        .text_color(color::WHITE)
                        .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
                            cx.stop_propagation();
                            app.toggle_crop_unit(cx);
                        }))
                        .child(unit.label().to_string()),
                ),
        )
        .when(unit_open, |el| {
            el.child(gpui_kit::deferred(
                div()
                    .absolute()
                    .top(px(CTRL_H + 32.0))
                    .left(px(0.0))
                    .child(crop_unit_list(unit, cx)),
            ))
        })
}

pub(super) fn crop_edit_field(
    id: &str,
    label: &str,
    value: &str,
    suffix: &str,
    cx: &Context<crate::app::App>,
    field: usize,
    custom: bool,
) -> impl IntoElement {
    let mut el = h_flex()
        .id(id.to_string())
        .test_support()
        .h(px(CTRL_H))
        .flex_shrink_0()
        .items_center()
        .gap(px(GAP))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .px(px(PILL_PX))
        .text_size(px(TEXT_SM))
        .on_click(
            cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                cx.stop_propagation();
                app.crop_begin_edit(field, custom, cx);
            }),
        )
        .child(div().text_color(ZINC).child(label.to_string()))
        .child(
            div()
                .font_weight(FontWeight(600.))
                .text_color(color::WHITE)
                .child(value.to_string()),
        );
    if !suffix.is_empty() {
        el = el.child(div().text_color(ZINC).child(suffix.to_string()));
    }
    el
}

pub(super) fn crop_unit_list(
    unit: crate::crop_aspect::CropSizeUnit,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let mut list = gpui_kit::base::v_flex()
        .id("crop-size-unit-list")
        .test_support()
        .w(px(64.0))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::MENU_LIST)
        .py(px(4.0));
    for candidate in crate::crop_aspect::CropSizeUnit::ALL {
        list = list.child(crop_ratio_preset_row(
            &format!("crop-size-unit-{}", candidate.label()),
            candidate.label(),
            candidate == unit,
            cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                cx.stop_propagation();
                app.crop_set_size_unit(candidate, cx);
            }),
        ));
    }
    list
}
