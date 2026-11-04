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
mod position_size;
mod preview;
mod printer_setup;
mod quality;
mod widgets;
use geometry::*;
use position_size::*;
use preview::*;
use printer_setup::*;
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

    let preview = page_preview(
        (page_w_mm, page_h_mm),
        (doc_w, doc_h),
        margins,
        center,
        scale_to_fit,
        scale,
    );

    let doc_scale = preview.doc_scale;
    let left = preview_column(
        preview,
        doc_w,
        doc_h,
        paper_name,
        paper_w_mm,
        paper_h_mm,
        show_paper_white,
    );

    // ── Inspector ───────────────────────────────────────────────────────────
    let inspector = v_flex()
        .flex_1()
        .h_full()
        .overflow_hidden()
        .border_l(px(1.0))
        .border_color(color::BORDER)
        .px(px(PAD))
        .gap(px(12.0));
    let mut inspector = printer_setup(
        inspector,
        state,
        printers,
        print_error,
        printer,
        printer_open,
        copies,
        landscape,
        cx,
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

    let inspector = position_size(
        inspector,
        state,
        position_open,
        paper,
        paper_open,
        unit,
        unit_open,
        margins,
        center,
        scale_to_fit,
        scale,
        scale_bounds,
        capture,
        scale_focus,
        doc_w,
        doc_h,
        doc_scale,
        cx,
    );

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
