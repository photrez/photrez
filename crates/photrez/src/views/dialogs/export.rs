//! The Export dialog: format, quality and the path prompt.

use super::*;

/// Export dialog: format, quality and the path prompt.
pub(super) fn export_body(
    state: &Entity<AppState>,
    document_size: Option<(u32, u32)>,
    cx: &gpui_kit::Context<crate::app::App>,
) -> Div {
    use crate::state::{ExportFormat, SliderTarget};
    use crate::views::option_bar::slider_surface::BarSliderSurface;

    let (format, quality, open, name) = {
        let s = state.read(cx);
        (
            s.export_format,
            s.export_quality,
            s.export_format_open,
            s.document_name.clone(),
        )
    };
    let size = document_size;

    let card =
        h_flex()
            .gap(px(12.0))
            .items_center()
            .p(px(10.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(color::BORDER)
            .bg(color::FIELD)
            .child(
                div()
                    .w(px(36.0))
                    .h(px(36.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(color::BORDER)
                    .child(
                        Icon::default()
                            .data(icons::IMAGE)
                            .size(px(20.0))
                            .text_color(color::ACCENT),
                    ),
            )
            .child(
                v_flex()
                    .gap(px(2.0))
                    .min_w(px(0.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(FontWeight(600.0))
                            .text_color(color::TEXT)
                            .child(name),
                    )
                    .child(div().text_size(px(11.0)).text_color(color::TEXT_DIM).child(
                        match size {
                            Some((w, h)) => format!("{} \u{00d7} {} px \u{00b7} RGB 8-bit", w, h),
                            None => "No document".to_string(),
                        },
                    )),
            );

    let toggle_state = state.clone();
    let trigger = div()
        .id("export-format-trigger")
        .test_support()
        .h(px(30.0))
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .px(px(10.0))
        .rounded(px(4.0))
        .border_1()
        .border_color(color::FIELD_BORDER)
        .bg(color::FIELD)
        .cursor_pointer()
        .on_click(
            cx.listener(move |_app, _: &gpui_kit::ClickEvent, _window, cx| {
                cx.stop_propagation();
                toggle_state.update(cx, |state, _| {
                    state.export_format_open = !state.export_format_open;
                });
                cx.notify();
            }),
        )
        .child(
            h_flex()
                .gap(px(8.0))
                .items_center()
                .child(
                    div()
                        .px(px(6.0))
                        .py(px(2.0))
                        .rounded(px(3.0))
                        .bg(color::CANVAS)
                        .border_1()
                        .border_color(color::BORDER)
                        .text_size(px(9.0))
                        .font_weight(FontWeight(700.0))
                        .text_color(color::TEXT_DIM)
                        .child(format.label().to_uppercase()),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(FontWeight(500.0))
                        .text_color(color::TEXT)
                        .child(format.label()),
                ),
        )
        .child(
            Icon::default()
                .data(icons::CHEVRON_DOWN)
                .size(px(12.0))
                .text_color(color::TEXT_DIM),
        );

    let mut format_section = v_flex()
        .w_full()
        .gap(px(6.0))
        .child(
            div()
                .text_size(px(11.0))
                .font_weight(FontWeight(700.0))
                .text_color(color::TEXT_DIM)
                .child(t("dialogs.export.format").to_uppercase()),
        )
        .child(trigger);
    if open {
        let mut list = v_flex().w_full().gap(px(2.0)).py(px(4.0));
        for candidate in [ExportFormat::Png, ExportFormat::Jpeg, ExportFormat::WebP] {
            let pick_state = state.clone();
            let current = candidate == format;
            list = list.child(
                div()
                    .id(format!("export-format-{}", candidate.extension()))
                    .test_support()
                    .h(px(28.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .rounded(px(3.0))
                    .when(current, |row| row.bg(hsla(0.0, 0.0, 1.0, 0.04)))
                    .text_size(px(12.0))
                    .text_color(if current { color::ACCENT } else { color::TEXT })
                    .cursor_pointer()
                    .on_click(
                        cx.listener(move |_app, _: &gpui_kit::ClickEvent, _window, cx| {
                            cx.stop_propagation();
                            pick_state.update(cx, |state, _| {
                                state.export_format = candidate;
                                state.export_format_open = false;
                            });
                            cx.notify();
                        }),
                    )
                    .child(candidate.label())
                    .when(current, |row| {
                        row.child(
                            Icon::default()
                                .data(icons::CHEVRON_RIGHT)
                                .size(px(12.0))
                                .text_color(color::ACCENT),
                        )
                    }),
            );
        }
        format_section = format_section.child(
            v_flex()
                .w_full()
                .rounded(px(6.0))
                .border_1()
                .border_color(color::BORDER)
                .bg(color::PANEL)
                .child(list),
        );
    }
    let description = match format {
        ExportFormat::Png => t("dialogs.export.formats.pngDesc"),
        ExportFormat::Jpeg => t("dialogs.export.formats.jpegDesc"),
        ExportFormat::WebP => t("dialogs.export.formats.webpDesc"),
    };
    format_section = format_section.child(
        div()
            .text_size(px(11.0))
            .text_color(color::TEXT_DIM)
            .child(description),
    );

    let mut body = v_flex().gap(px(16.0)).w_full().child(card);
    body = body.child(format_section);

    if format != ExportFormat::Png {
        let percent = f32::from(quality.clamp(1, 100)) / 100.0;
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
                        .w(relative(percent))
                        .rounded(px(2.0))
                        .bg(color::ACCENT),
                )
                .child(
                    div()
                        .absolute()
                        .top(px(-4.0))
                        .left(relative(percent))
                        .ml(px(-6.0))
                        .w(px(12.0))
                        .h(px(12.0))
                        .rounded_full()
                        .bg(color::ACCENT),
                ),
        );
        let (bounds, capture, focus) = {
            let s = state.read(cx);
            (
                s.export_quality_slider_bounds.clone(),
                s.slider_capture.clone(),
                s.slider_focus_for(SliderTarget::ExportQuality),
            )
        };
        let quality_text = match quality {
            90..=100 => t("dialogs.export.qualities.veryHigh"),
            70..=89 => t("dialogs.export.qualities.high"),
            _ => t("dialogs.export.qualities.medium"),
        };
        // The surface focuses this wrapper on a click; arrows step the quality
        // across the same 1..=100 range the pointer maps.
        let mut quality_track = div()
            .id("export-quality")
            .test_support()
            .w_full()
            .h(px(14.0))
            .aria_label(t("dialogs.export.quality"));
        quality_track = crate::views::option_bar::slider_surface::track_focus_and_keys(
            quality_track,
            &focus,
            SliderTarget::ExportQuality,
            cx,
        );
        quality_track = quality_track.child(BarSliderSurface::new(
            track,
            cx.entity().downgrade(),
            SliderTarget::ExportQuality,
            bounds,
            capture,
            focus,
        ));
        body = body.child(
            v_flex()
                .w_full()
                .gap(px(6.0))
                .pt(px(16.0))
                .border_t_1()
                .border_color(color::BORDER)
                .child(
                    h_flex()
                        .w_full()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_size(px(11.0))
                                .font_weight(FontWeight(700.0))
                                .text_color(color::TEXT_DIM)
                                .child(t("dialogs.export.quality").to_uppercase()),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .font_weight(FontWeight(700.0))
                                .text_color(color::TEXT)
                                .child(format!("{quality}%")),
                        ),
                )
                .child(quality_track)
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(color::TEXT_DIM)
                        .child(quality_text),
                ),
        );
    }

    body
}
