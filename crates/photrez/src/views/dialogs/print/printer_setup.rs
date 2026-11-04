//! The Print dialog's Printer Setup section: the printer picker, its refresh
//! and notice rows, the copies row, and the orientation row.

use super::*;

// One parameter per row value this section lays out; the dialog reads them once.
#[allow(clippy::too_many_arguments)]
pub(super) fn printer_setup(
    mut inspector: Div,
    state: &gpui_kit::Entity<crate::state::AppState>,
    printers: Vec<String>,
    print_error: Option<String>,
    printer: usize,
    printer_open: bool,
    copies: u32,
    landscape: bool,
    num_field: Option<crate::state::NumFieldSession>,
    num_focus: Option<gpui_kit::FocusHandle>,
    cx: &Context<crate::app::App>,
) -> Div {
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
                        cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                            cx.stop_propagation();
                            pick.update(cx, |state, _| {
                                state.print_printer = index;
                                state.print_printer_open = false;
                            });
                            // A different printer has a different driver, so the
                            // paper list the dialog quotes has to be asked again.
                            app.refresh_print_papers(cx);
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

    let copies_label = t("dialogs.print.copies");
    inspector = inspector.child(
        h_flex()
            .w_full()
            .items_center()
            .gap(px(8.0))
            .py(px(6.0))
            .child(label(&copies_label))
            .child(num_box(
                "print-copies",
                &copies_label,
                72.0,
                copies.to_string(),
                super::widgets::NumFieldEdit {
                    session: num_field.as_ref(),
                    focus: num_focus.clone(),
                },
                move |app, window, cx| {
                    app.begin_num_field("print-copies", copies as f64, window, cx);
                },
                cx,
            ))
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
                    .child(t("dialogs.print.printerProps")),
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
                Some(page_glyph("print-portrait-glyph", false, !landscape)),
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
                Some(page_glyph("print-landscape-glyph", true, landscape)),
                cx,
                move |_app, _: &gpui_kit::ClickEvent, _window, cx| {
                    cx.stop_propagation();
                    landscape_state.update(cx, |state, _| state.print_landscape = true);
                    cx.notify();
                },
            )),
    );
    inspector
}
