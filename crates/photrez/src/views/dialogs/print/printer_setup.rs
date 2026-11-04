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
    inspector
}
