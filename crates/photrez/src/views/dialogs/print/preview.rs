//! The Print dialog's paper preview column: the document card, the paper with
//! its dashed edge, and the dashed document rectangle placed by the offsets.

use super::*;

pub(super) fn preview_column(
    preview: PagePreview,
    doc_w: u32,
    doc_h: u32,
    paper_name: &str,
    paper_w_mm: f64,
    paper_h_mm: f64,
    show_paper_white: bool,
) -> Div {
    let PagePreview {
        page_w,
        page_h,
        doc_scale: _,
        doc_px_w,
        doc_px_h,
        doc_x,
        doc_y,
    } = preview;

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

    v_flex()
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
        )
}
