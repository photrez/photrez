//! The Print Document dialog: a paper preview on the left and the print
//! inspector sections on the right, in the 1040x660 shell the capture shows.
//!
//! The layout is a paper viewport (document card, the white paper with
//! a dashed edge, a dashed document rectangle placed by the offsets) beside
//! the inspector (Printer Setup, Color Management, Position and Size) and a
//! footer with Show Paper White plus Cancel/Print. This view keeps that
//! structure and binds the controls to `AppState`; printing itself still goes
//! through the platform service.

use super::*;
use gpui_kit::AnyElement;

mod geometry;
mod quality;
mod widgets;
use geometry::*;
pub(super) use quality::{effective_ppi, ppi_quality, printer_notice};
use widgets::*;

const DIALOG_W: f32 = 1040.0;
const DIALOG_H: f32 = 660.0;
const TITLE_H: f32 = 36.0;
const FOOTER_H: f32 = 44.0;
const LEFT_W: f32 = 657.0;
const PAD: f32 = 14.0;
const LABEL_W: f32 = 72.0;
const ROW_H: f32 = 26.0;
const HEADER_H: f32 = 38.0;
/// The print resolution the dialog treats as optimal: 300 PPI.
const PRINT_PPI: f64 = 300.0;

pub(super) fn render(
    state: &gpui_kit::Entity<crate::state::AppState>,
    document_size: Option<(u32, u32)>,
    cx: &Context<crate::app::App>,
) -> AnyElement {
    use crate::app::PRINT_PAPERS;
    use crate::state::SliderTarget;
    use crate::views::option_bar::slider_surface::BarSliderSurface;

    let s = state.read(cx);
    let paper = s.print_paper.min(PRINT_PAPERS.len() - 1);
    let copies = s.print_copies;
    let landscape = s.print_landscape;
    let printers = s.printers.clone();
    let print_error = s.print_error.clone();
    let printer = s.print_printer.min(printers.len().saturating_sub(1));
    let margins = s.print_margins_mm;
    let center = s.print_center_on_page;
    let scale_to_fit = s.print_scale_to_fit;
    let scale = s.print_scale_percent.clamp(10, 400);
    let unit = s.print_unit.clone();
    let unit_open = s.print_unit_open;
    let paper_open = s.print_paper_open;
    let printer_open = s.print_printer_open;
    let show_paper_white = s.print_show_paper_white;
    let position_open = s.print_position_open;
    let color_open = s.print_color_open;
    let scale_bounds = s.print_scale_slider_bounds.clone();
    let capture = s.slider_capture.clone();
    let scale_focus = s.slider_focus_for(SliderTarget::PrintScale);

    let (doc_w, doc_h) = document_size.unwrap_or((800, 600));
    let paper_name = PRINT_PAPERS[paper].0;
    let (paper_w_mm, paper_h_mm) = (PRINT_PAPERS[paper].2, PRINT_PAPERS[paper].3);
    let (page_w_mm, page_h_mm) = if landscape {
        (paper_h_mm, paper_w_mm)
    } else {
        (paper_w_mm, paper_h_mm)
    };

    let PagePreview {
        page_w,
        page_h,
        doc_scale,
        doc_px_w,
        doc_px_h,
        doc_x,
        doc_y,
    } = page_preview(
        (page_w_mm, page_h_mm),
        (doc_w, doc_h),
        margins,
        center,
        scale_to_fit,
        scale,
    );

    let card = h_flex()
        .w_full()
        .items_center()
        .justify_center()
        .rounded(px(6.0))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .p(px(10.0))
        .child(
            v_flex()
                .items_center()
                .child(
                    div()
                        .text_size(px(11.5))
                        .font_weight(FontWeight(600.))
                        .text_color(color::TEXT)
                        .child(format!("Baseline ({doc_w} \u{00d7} {doc_h} px)")),
                )
                .child(
                    div()
                        .mt(px(2.0))
                        .text_size(px(10.5))
                        .text_color(color::TEXT_DIM)
                        .child(format!(
                            "Paper: {paper_name} ({paper_w_mm:.0} mm \u{00d7} {paper_h_mm:.0} mm)"
                        )),
                ),
        );

    let page = div()
        .w(px(page_w))
        .h(px(page_h))
        .border_1()
        .border_dashed()
        .border_color(color::BORDER)
        .bg(if show_paper_white {
            hsla(0.0, 0.0, 1.0, 1.0)
        } else {
            color::CANVAS
        })
        .relative()
        .child(
            div()
                .absolute()
                .left(px(doc_x))
                .top(px(doc_y))
                .w(px(doc_px_w))
                .h(px(doc_px_h))
                .border_1()
                .border_dashed()
                .border_color(color::TOOL_MARK),
        );

    let left = v_flex()
        .w(px(LEFT_W))
        .flex_none()
        .h_full()
        .items_center()
        .p(px(PAD))
        .gap(px(10.0))
        .child(card)
        .child(
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .child(page),
        )
        .child(
            div()
                .text_size(px(11.0))
                .text_color(color::TEXT_DIM)
                .child("Click and drag image on paper to reposition"), // i18n-exempt: copy not in catalog
        );

    // ── Inspector ───────────────────────────────────────────────────────────
    let mut inspector = v_flex()
        .flex_1()
        .h_full()
        .overflow_hidden()
        .border_l(px(1.0))
        .border_color(color::BORDER)
        .px(px(PAD))
        .gap(px(12.0));

    inspector = inspector.child(header_row(
        &t("dialogs.print.printerSetup"),
        &t("dialogs.print.systemSpooler"),
    ));

    let printer_name = printers.get(printer).cloned().unwrap_or_default();
    let toggle_state = state.clone();
    let trigger = field(
        "print-printer-trigger",
        printer_name,
        cx,
        move |_app, _: &gpui_kit::ClickEvent, _window, cx| {
            cx.stop_propagation();
            toggle_state.update(cx, |state, _| {
                state.print_printer_open = !state.print_printer_open
            });
            cx.notify();
        },
    );
    inspector = inspector.child(
        h_flex()
            .w_full()
            .items_center()
            .gap(px(8.0))
            .py(px(6.0))
            .child(label(&t("dialogs.print.printer")))
            .child(div().flex_1().child(trigger))
            .child(
                div()
                    .id("print-refresh")
                    .test_support()
                    .h(px(ROW_H))
                    .w(px(24.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(color::FIELD_BORDER)
                    .cursor_pointer()
                    .on_click(
                        cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                            cx.stop_propagation();
                            app.refresh_printers(cx);
                            cx.notify();
                        }),
                    )
                    .child(
                        Icon::default()
                            .data(icons::ROTATE_CW)
                            .size(px(12.0))
                            .text_color(color::TEXT_DIM),
                    ),
            ),
    );
    if printer_open {
        let mut list = v_flex().w_full().gap(px(2.0)).pb(px(4.0));
        for (index, name) in printers.iter().enumerate() {
            let pick = state.clone();
            list = list.child(
                div()
                    .id(format!("print-printer-{index}"))
                    .test_support()
                    .h(px(22.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .rounded(px(3.0))
                    .text_size(px(11.0))
                    .text_color(if index == printer {
                        color::ACCENT
                    } else {
                        color::TEXT
                    })
                    .cursor_pointer()
                    .on_click(
                        cx.listener(move |_app, _: &gpui_kit::ClickEvent, _window, cx| {
                            cx.stop_propagation();
                            pick.update(cx, |state, _| {
                                state.print_printer = index;
                                state.print_printer_open = false;
                            });
                            cx.notify();
                        }),
                    )
                    .child(name.clone()),
            );
        }
        inspector = inspector.child(list);
    }
    if let Some(message) = printer_notice(print_error.as_deref(), printers.len()) {
        inspector = inspector.child(
            h_flex()
                .id("print-error")
                .test_support()
                .w_full()
                .items_center()
                .gap(px(6.0))
                .px(px(10.0))
                .py(px(6.0))
                .rounded(px(4.0))
                .border_1()
                .border_color(color::BORDER)
                .bg(color::ROSE_950.opacity(0.2))
                .child(
                    Icon::default()
                        .data(icons::CIRCLE)
                        .size(px(12.0))
                        .text_color(color::ROSE_400),
                )
                .child(
                    div()
                        .text_size(px(10.5))
                        .text_color(color::ROSE_400)
                        .child(message),
                ),
        );
    }

    let copies_dec = state.clone();
    let copies_inc = state.clone();
    inspector = inspector.child(
        h_flex()
            .w_full()
            .items_center()
            .gap(px(8.0))
            .py(px(6.0))
            .child(label(&t("dialogs.print.copies")))
            .child(
                div()
                    .h(px(ROW_H))
                    .w(px(72.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(6.0))
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(color::FIELD_BORDER)
                    .bg(color::FIELD)
                    .child(
                        div()
                            .id("print-copies-dec")
                            .test_support()
                            .w(px(16.0))
                            .flex()
                            .justify_center()
                            .cursor_pointer()
                            .on_click(cx.listener(
                                move |_app, _: &gpui_kit::ClickEvent, _window, cx| {
                                    cx.stop_propagation();
                                    copies_dec.update(cx, |state, _| {
                                        state.print_copies =
                                            state.print_copies.saturating_sub(1).max(1);
                                    });
                                    cx.notify();
                                },
                            ))
                            .child("-"), // i18n-exempt: glyph or symbol
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(color::TEXT)
                            .child(copies.to_string()),
                    )
                    .child(
                        div()
                            .id("print-copies-inc")
                            .test_support()
                            .w(px(16.0))
                            .flex()
                            .justify_center()
                            .cursor_pointer()
                            .on_click(cx.listener(
                                move |_app, _: &gpui_kit::ClickEvent, _window, cx| {
                                    cx.stop_propagation();
                                    copies_inc.update(cx, |state, _| {
                                        state.print_copies =
                                            state.print_copies.saturating_add(1).min(99);
                                    });
                                    cx.notify();
                                },
                            ))
                            .child("+"), // i18n-exempt: glyph or symbol
                    ),
            )
            .child(div().flex_1())
            .child(
                div()
                    .id("print-settings")
                    .test_support()
                    .h(px(ROW_H))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(color::FIELD_BORDER)
                    .bg(color::FIELD)
                    .text_size(px(11.0))
                    .text_color(color::TEXT)
                    .cursor_pointer()
                    .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
                        cx.stop_propagation();
                        app.set_status_error(
                            Some("Printer properties are managed by the system".into()),
                            cx,
                        );
                    }))
                    .child(format!("{}...", t("dialogs.print.printerProps"))), // i18n-exempt: ellipsis suffix
            ),
    );

    let portrait_state = state.clone();
    let landscape_state = state.clone();
    inspector = inspector.child(
        h_flex()
            .w_full()
            .items_center()
            .gap(px(8.0))
            .py(px(6.0))
            .child(label(&t("dialogs.print.layout")))
            .child(toggle(
                "print-orientation",
                &t("dialogs.print.portrait"),
                !landscape,
                cx,
                move |_app, _: &gpui_kit::ClickEvent, _window, cx| {
                    cx.stop_propagation();
                    portrait_state.update(cx, |state, _| state.print_landscape = false);
                    cx.notify();
                },
            ))
            .child(toggle(
                "print-orientation-landscape",
                &t("dialogs.print.landscape"),
                landscape,
                cx,
                move |_app, _: &gpui_kit::ClickEvent, _window, cx| {
                    cx.stop_propagation();
                    landscape_state.update(cx, |state, _| state.print_landscape = true);
                    cx.notify();
                },
            )),
    );

    let color_state = state.clone();
    let color_header = header_row(
        &t("dialogs.print.colorManagement"),
        &t("dialogs.print.systemManaged"),
    )
    .id("print-color-header")
    .test_support()
    .cursor_pointer()
    .on_click(
        cx.listener(move |_app, _: &gpui_kit::ClickEvent, _window, cx| {
            cx.stop_propagation();
            color_state.update(cx, |state, _| {
                state.print_color_open = !state.print_color_open
            });
            cx.notify();
        }),
    );
    inspector = inspector.child(color_header);
    if color_open {
        for (name, value) in [
            (
                t("dialogs.print.docProfile"),
                "sRGB IEC61966-2.1".to_string(),
            ),
            (
                t("dialogs.print.colorHandling"),
                t("dialogs.print.printerManagesColors"),
            ),
            (
                t("dialogs.print.renderingIntent"),
                t("dialogs.print.perceptual"),
            ),
        ] {
            inspector = inspector.child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .py(px(3.0))
                    .text_size(px(11.0))
                    .text_color(color::TEXT_DIM)
                    .child(name)
                    .child(value),
            );
        }
    }

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
                    paper_label(paper),
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
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(color::TEXT)
                        .child(format!(
                            "{:.1} {}",
                            to_unit(width_mm, &unit),
                            unit_suffix(&unit)
                        )),
                )
                .child(div().flex_1())
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(color::TEXT_DIM)
                        .child("Height"), // i18n-exempt: copy not in catalog
                )
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(color::TEXT)
                        .child(format!(
                            "{:.1} {}",
                            to_unit(height_mm, &unit),
                            unit_suffix(&unit)
                        )),
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

    let cancel = dialog_action_button(
        &t("common.cancel"),
        false,
        "print-cancel",
        cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
            cx.stop_propagation();
            app.close_active_dialog(cx);
            cx.notify();
        }),
    )
    .into_any_element();
    let confirm = dialog_action_button(
        &t("dialogs.print.printBtn"),
        true,
        "print-confirm",
        cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
            cx.stop_propagation();
            app.print_now(cx);
        }),
    )
    .into_any_element();
    let paper_white_state = state.clone();
    let footer = h_flex()
        .h(px(FOOTER_H))
        .w_full()
        .flex_none()
        .items_center()
        .justify_between()
        .border_t(px(1.0))
        .border_color(color::BORDER)
        .bg(color::TOPBAR)
        .px(px(PAD))
        .child(div().w(px(240.0)).child(checkbox(
            "print-paper-white",
            "Show Paper White", // i18n-exempt: copy not in catalog
            show_paper_white,
            cx,
            move |_app, _: &gpui_kit::ClickEvent, _window, cx| {
                cx.stop_propagation();
                paper_white_state.update(cx, |state, _| {
                    state.print_show_paper_white = !state.print_show_paper_white
                });
                cx.notify();
            },
        )))
        .child(
            h_flex()
                .items_center()
                .gap(px(8.0))
                .child(cancel)
                .child(confirm),
        );

    let title = t("dialogs.print.title");
    let close_state = state.clone();
    let close = chrome::dialog_close_button(cx.listener(
        move |_app, _: &gpui_kit::ClickEvent, _window, cx| {
            cx.stop_propagation();
            close_state.update(cx, |state, _| state.active_dialog = None);
            cx.notify();
        },
    ));

    v_flex()
        .id("dialog-print")
        .test_support()
        .w(px(DIALOG_W))
        .h(px(DIALOG_H))
        .rounded(px(8.0))
        .border_1()
        .border_color(color::FIELD_BORDER)
        .bg(color::PANEL)
        .overflow_hidden()
        .child(
            h_flex()
                .h(px(TITLE_H))
                .w_full()
                .flex_none()
                .items_center()
                .justify_between()
                .border_b(px(1.0))
                .border_color(color::BORDER)
                .bg(color::TOPBAR)
                .px(px(12.0))
                .child(
                    h_flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .w(px(18.0))
                                .h(px(18.0))
                                .rounded(px(4.0))
                                .bg(color::ACCENT),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .font_weight(FontWeight(500.))
                                .text_color(color::TEXT)
                                .child(title),
                        ),
                )
                .child(close),
        )
        .child(
            h_flex()
                .flex_1()
                .w_full()
                .overflow_hidden()
                .child(left)
                .child(inspector),
        )
        .child(footer)
        .into_any_element()
}
