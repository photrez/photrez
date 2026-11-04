//! The Print dialog's Position and Size section: the unit select, the paper
//! picker, margins, centring, the scale controls, the printed size fields and
//! the resolution badge.

use super::*;
use crate::app::PRINT_PAPERS;
use crate::state::SliderTarget;
use crate::views::option_bar::slider_surface::BarSliderSurface;

// One parameter per row value this section lays out; the dialog reads them once.
#[allow(clippy::too_many_arguments)]
pub(super) fn position_size(
    mut inspector: Div,
    state: &gpui_kit::Entity<crate::state::AppState>,
    position_open: bool,
    paper: usize,
    paper_open: bool,
    unit: String,
    unit_open: bool,
    margins: f64,
    center: bool,
    scale_to_fit: bool,
    scale: u32,
    scale_bounds: std::rc::Rc<std::cell::Cell<Option<gpui_kit::Bounds<gpui_kit::Pixels>>>>,
    capture: std::rc::Rc<std::cell::Cell<Option<gpui_kit::HitboxId>>>,
    scale_focus: Option<gpui_kit::FocusHandle>,
    doc_w: u32,
    doc_h: u32,
    doc_scale: f32,
    sizes: PaperSizes,
    num_field: Option<crate::state::NumFieldSession>,
    cx: &Context<crate::app::App>,
) -> Div {
    // Position and Size: the header carries the unit select.
    let position_state = state.clone();
    let unit_state = state.clone();
    let position_header = h_flex()
        .id("print-position-header")
        .test_support()
        .h(px(HEADER_H))
        .w_full()
        .items_center()
        .justify_between()
        .cursor_pointer()
        .on_click(
            cx.listener(move |_app, _: &gpui_kit::ClickEvent, _window, cx| {
                cx.stop_propagation();
                position_state.update(cx, |state, _| {
                    state.print_position_open = !state.print_position_open
                });
                cx.notify();
            }),
        )
        .child(
            h_flex()
                .items_center()
                .gap(px(6.0))
                .child(
                    Icon::default()
                        .data(icons::CHEVRON_DOWN)
                        .size(px(10.0))
                        .text_color(color::TEXT_DIM),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(FontWeight(500.))
                        .text_color(color::TEXT)
                        .child(t("dialogs.print.positionAndSize")),
                ),
        )
        .child(
            div().w(px(120.0)).child(
                field(
                    "print-unit",
                    unit_label(&unit),
                    cx,
                    move |_app, _: &gpui_kit::ClickEvent, _window, cx| {
                        cx.stop_propagation();
                        unit_state.update(cx, |state, _| {
                            state.print_unit_open = !state.print_unit_open
                        });
                        cx.notify();
                    },
                )
                .into_any_element(),
            ),
        );
    inspector = inspector.child(position_header);

    if position_open {
        if unit_open {
            let mut list = v_flex().w_full().gap(px(2.0)).pb(px(4.0));
            for code in ["mm", "cm", "in", "px"] {
                let pick = state.clone();
                list = list.child(
                    div()
                        .id(format!("print-unit-{code}"))
                        .test_support()
                        .h(px(22.0))
                        .px(px(8.0))
                        .flex()
                        .items_center()
                        .rounded(px(3.0))
                        .text_size(px(11.0))
                        .text_color(if unit == code {
                            color::ACCENT
                        } else {
                            color::TEXT
                        })
                        .cursor_pointer()
                        .on_click(cx.listener(
                            move |_app, _: &gpui_kit::ClickEvent, _window, cx| {
                                cx.stop_propagation();
                                pick.update(cx, |state, _| {
                                    state.print_unit = code.to_string();
                                    state.print_unit_open = false;
                                });
                                cx.notify();
                            },
                        ))
                        .child(unit_label(code)),
                );
            }
            inspector = inspector.child(list);
        }

        let paper_state = state.clone();
        inspector = inspector.child(
            h_flex()
                .w_full()
                .items_center()
                .gap(px(8.0))
                .h(px(ROW_H))
                .child(label(&t("dialogs.print.paperSize")))
                .child(div().flex_1().child(field(
                    "print-paper",
                    paper_label(paper, &sizes),
                    cx,
                    move |_app, _: &gpui_kit::ClickEvent, _window, cx| {
                        cx.stop_propagation();
                        paper_state.update(cx, |state, _| {
                            state.print_paper_open = !state.print_paper_open
                        });
                        cx.notify();
                    },
                ))),
        );
        // What the driver itself reports. An empty answer needs no extra row:
        // the field's own tail already says the paper is not listed.
        let report = match &sizes {
            PaperSizes::Listed(entries) => Some(
                v_flex()
                    .id("print-paper-reported")
                    .test_support()
                    .w_full()
                    .gap(px(2.0))
                    .pb(px(4.0))
                    .text_size(px(10.5))
                    .text_color(color::TEXT_DIM)
                    .child(t("dialogs.print.printerOffers").to_string())
                    .children(entries.iter().map(|(name, width_mm, height_mm)| {
                        div().child(paper_name_label(name, *width_mm, *height_mm))
                    }))
                    .into_any_element(),
            ),
            PaperSizes::Empty => None,
            PaperSizes::Failed(reason) => Some(
                h_flex()
                    .id("print-paper-error")
                    .test_support()
                    .w_full()
                    .items_center()
                    .gap(px(6.0))
                    .text_size(px(10.5))
                    .text_color(color::ROSE_400)
                    .child(reason.clone())
                    .into_any_element(),
            ),
            PaperSizes::Unknown => Some(
                div()
                    .id("print-paper-unknown")
                    .test_support()
                    .w_full()
                    .text_size(px(10.5))
                    .text_color(color::TEXT_DIM)
                    .child(t("dialogs.print.paperListUnavailable").to_string())
                    .into_any_element(),
            ),
        };
        if paper_open {
            let mut list = v_flex().w_full().gap(px(2.0)).pb(px(4.0));
            for (index, (name, _, w, h)) in PRINT_PAPERS.iter().enumerate() {
                let pick = state.clone();
                list = list.child(
                    div()
                        .id(format!("print-paper-{index}"))
                        .test_support()
                        .h(px(22.0))
                        .px(px(8.0))
                        .flex()
                        .items_center()
                        .rounded(px(3.0))
                        .text_size(px(11.0))
                        .text_color(if index == paper {
                            color::ACCENT
                        } else {
                            color::TEXT
                        })
                        .cursor_pointer()
                        .on_click(cx.listener(
                            move |_app, _: &gpui_kit::ClickEvent, _window, cx| {
                                cx.stop_propagation();
                                pick.update(cx, |state, _| {
                                    state.print_paper = index;
                                    state.print_paper_open = false;
                                });
                                cx.notify();
                            },
                        ))
                        .child(format!("{name} ({w:.0} \u{00d7} {h:.0} mm)")),
                );
            }
            inspector = inspector.child(list);
            // The driver's answer goes under the presets, inside the open list.
            // These rows carry no click: the dialog prints the paper picked from
            // the preset list, so this is the printer's own report, not a second
            // control. Keeping it here also holds the inspector's fixed height,
            // because a driver can name many sizes.
            if let Some(report) = report {
                inspector = inspector.child(report);
            }
        }

        inspector = inspector.child(
            h_flex()
                .w_full()
                .items_center()
                .gap(px(8.0))
                .h(px(ROW_H))
                .child(label(&t("dialogs.print.margins")))
                .child(div().flex_1())
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(color::TEXT)
                        .child(format!(
                            "{:.1} {}",
                            to_unit(margins, &unit),
                            unit_suffix(&unit)
                        )),
                ),
        );

        let center_state = state.clone();
        inspector = inspector.child(checkbox(
            "print-center",
            &t("dialogs.print.centerOnPage"),
            center,
            cx,
            move |_app, _: &gpui_kit::ClickEvent, _window, cx| {
                cx.stop_propagation();
                center_state.update(cx, |state, _| {
                    state.print_center_on_page = !state.print_center_on_page
                });
                cx.notify();
            },
        ));

        inspector = inspector.child(
            h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .py(px(6.0))
                .child(
                    div()
                        .text_size(px(11.5))
                        .font_weight(FontWeight(600.))
                        .text_color(color::TEXT)
                        .child(t("dialogs.print.scaledPrintSize")),
                )
                .child({
                    let fit_state = state.clone();
                    div().w(px(120.0)).child(checkbox(
                        "print-scale-fit",
                        &t("dialogs.print.scaleToFit"),
                        scale_to_fit,
                        cx,
                        move |_app, _: &gpui_kit::ClickEvent, _window, cx| {
                            cx.stop_propagation();
                            fit_state.update(cx, |state, _| {
                                state.print_scale_to_fit = !state.print_scale_to_fit
                            });
                            cx.notify();
                        },
                    ))
                }),
        );

        let track = div().w_full().h(px(14.0)).flex().items_center().child(
            div()
                .relative()
                .w_full()
                .h(px(4.0))
                .rounded(px(2.0))
                .bg(color::BORDER)
                .child(
                    div()
                        .absolute()
                        .left(px(0.0))
                        .top(px(0.0))
                        .h(px(4.0))
                        .w(relative(((scale - 10) as f32) / 390.0))
                        .rounded(px(2.0))
                        .bg(color::ACCENT),
                )
                .child(
                    div()
                        .absolute()
                        .top(px(-4.0))
                        .left(relative(((scale - 10) as f32) / 390.0))
                        .ml(px(-6.0))
                        .w(px(12.0))
                        .h(px(12.0))
                        .rounded_full()
                        .bg(color::ACCENT),
                ),
        );
        // The surface focuses this wrapper on a click; arrows step one percent
        // across the same 10..=400 range and clear Scale to Fit.
        let mut scale_track = div()
            .id("print-scale-track")
            .test_support()
            .flex_1()
            .h(px(14.0))
            .aria_label(t("dialogs.print.scale"));
        scale_track = crate::views::option_bar::slider_surface::track_focus_and_keys(
            scale_track,
            &scale_focus,
            SliderTarget::PrintScale,
            cx,
        );
        scale_track = scale_track.child(BarSliderSurface::new(
            track,
            cx.entity().downgrade(),
            SliderTarget::PrintScale,
            scale_bounds,
            capture,
            scale_focus,
        ));
        inspector = inspector.child(
            h_flex()
                .w_full()
                .items_center()
                .gap(px(8.0))
                .h(px(ROW_H))
                .child(
                    div()
                        .w(px(42.0))
                        .text_size(px(11.0))
                        .text_color(color::TEXT_DIM)
                        .child(t("dialogs.print.scale")),
                )
                .child(scale_track)
                .child(
                    div()
                        .w(px(44.0))
                        .h(px(22.0))
                        .flex()
                        .items_center()
                        .justify_end()
                        .px(px(6.0))
                        .rounded(px(3.0))
                        .border_1()
                        .border_color(color::BORDER)
                        .bg(color::FIELD)
                        .text_size(px(11.0))
                        .text_color(color::TEXT)
                        .child(scale.to_string()),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(color::TEXT_DIM)
                        .child("%"), // i18n-exempt: unit glyph
                ),
        );

        let width_mm = doc_w as f64 * doc_scale as f64 * 25.4 / PRINT_PPI;
        let height_mm = doc_h as f64 * doc_scale as f64 * 25.4 / PRINT_PPI;
        // The printed size at 100 % scale, already in the shown unit. `to_unit`
        // is a pure scale, so a typed size divides by this basis to get the
        // percentage back and needs no second conversion.
        let fit = if scale_to_fit {
            doc_scale as f64
        } else {
            doc_scale as f64 * 100.0 / scale as f64
        };
        let basis = (
            to_unit(doc_w as f64 * fit * 25.4 / PRINT_PPI, &unit),
            to_unit(doc_h as f64 * fit * 25.4 / PRINT_PPI, &unit),
        );
        // The field seeds the one decimal it shows, so opening it does not type
        // a longer number than the row can display.
        let width_typed = (to_unit(width_mm, &unit) * 10.0).round() / 10.0;
        let height_typed = (to_unit(height_mm, &unit) * 10.0).round() / 10.0;
        let width_box = num_box(
            "print-width",
            "Width", // i18n-exempt: copy not in catalog
            68.0,
            format!("{width_typed:.1}"),
            super::widgets::NumFieldEdit {
                session: num_field.as_ref(),
                focus: state.read(cx).num_field_focus_for("print-width"),
            },
            move |app, window, cx| {
                app.begin_print_size_field("print-width", basis, width_typed, window, cx);
            },
            cx,
        );
        let height_box = num_box(
            "print-height",
            "Height", // i18n-exempt: copy not in catalog
            68.0,
            format!("{height_typed:.1}"),
            super::widgets::NumFieldEdit {
                session: num_field.as_ref(),
                focus: state.read(cx).num_field_focus_for("print-height"),
            },
            move |app, window, cx| {
                app.begin_print_size_field("print-height", basis, height_typed, window, cx);
            },
            cx,
        );
        inspector = inspector.child(
            h_flex()
                .w_full()
                .items_center()
                .gap(px(8.0))
                .h(px(ROW_H))
                .child(
                    div()
                        .w(px(42.0))
                        .text_size(px(11.0))
                        .text_color(color::TEXT_DIM)
                        .child("Width"), // i18n-exempt: copy not in catalog
                )
                .child(width_box)
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(color::TEXT_DIM)
                        .child(unit_suffix(&unit)),
                )
                .child(div().flex_1())
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(color::TEXT_DIM)
                        .child("Height"), // i18n-exempt: copy not in catalog
                )
                .child(height_box)
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(color::TEXT_DIM)
                        .child(unit_suffix(&unit)),
                ),
        );

        let print_ppi = effective_ppi((doc_w as f64, doc_h as f64), (width_mm, height_mm));
        let (badge_text, badge_tint, badge_fill) = ppi_quality(print_ppi);
        inspector = inspector.child(
            h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .px(px(8.0))
                .py(px(6.0))
                .rounded(px(4.0))
                .border_1()
                .border_color(color::BORDER)
                .bg(badge_fill.opacity(0.4))
                .text_size(px(11.0))
                .text_color(badge_tint)
                .child(t("dialogs.print.printResolution").to_string())
                // i18n-exempt: the reference keeps this copy out of its catalog
                .child(format!("{print_ppi} PPI \u{2014} {badge_text}")),
        );
    }
    inspector
}
