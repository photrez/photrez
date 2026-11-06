//! The Print dialog's row widgets and unit maths. The dialog's `render` reads
//! `AppState` once and lays out both columns; these are the pieces it lays with.

use super::*;

pub(super) fn label(text: &str) -> Div {
    div()
        .w(px(LABEL_W))
        .flex_none()
        .text_size(px(11.0))
        .text_color(color::TEXT_DIM)
        .child(text.to_string())
}

pub(super) fn header_row(title: &str, trailing: &str) -> Div {
    h_flex()
        .h(px(HEADER_H))
        .w_full()
        .items_center()
        .justify_between()
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
                        .child(title.to_string()),
                ),
        )
        .child(
            div()
                .text_size(px(11.0))
                .text_color(color::TEXT_DIM)
                .child(trailing.to_string()),
        )
}

pub(super) fn field<F>(
    id: &str,
    value: String,
    cx: &Context<crate::app::App>,
    handler: F,
) -> impl IntoElement
where
    F: Fn(
            &mut crate::app::App,
            &gpui_kit::ClickEvent,
            &mut gpui_kit::Window,
            &mut Context<crate::app::App>,
        ) + 'static,
{
    div()
        .id(id.to_string())
        .test_support()
        .h(px(ROW_H))
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .px(px(8.0))
        .rounded(px(4.0))
        .border_1()
        .border_color(color::FIELD_BORDER)
        .bg(color::FIELD)
        .text_size(px(11.5))
        .text_color(color::TEXT)
        .cursor_pointer()
        .on_click(cx.listener(handler))
        .child(div().child(value))
        .child(
            Icon::default()
                .data(icons::CHEVRON_DOWN)
                .size(px(11.0))
                .text_color(color::TEXT_DIM),
        )
}

pub(super) fn checkbox<F>(
    id: &str,
    text: &str,
    checked: bool,
    cx: &Context<crate::app::App>,
    handler: F,
) -> impl IntoElement
where
    F: Fn(
            &mut crate::app::App,
            &gpui_kit::ClickEvent,
            &mut gpui_kit::Window,
            &mut Context<crate::app::App>,
        ) + 'static,
{
    div()
        .id(id.to_string())
        .test_support()
        .h(px(20.0))
        .w_full()
        .flex()
        .items_center()
        .gap(px(8.0))
        .cursor_pointer()
        .child(
            div()
                .w(px(14.0))
                .h(px(14.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(3.0))
                .border_1()
                .border_color(if checked {
                    color::ACCENT
                } else {
                    color::FIELD_BORDER
                })
                .bg(if checked { color::ACCENT } else { color::FIELD })
                .when(checked, |box_| {
                    box_.child(
                        Icon::default()
                            .data(icons::CHECK)
                            .size(px(10.0))
                            .text_color(color::BG),
                    )
                }),
        )
        .child(
            div()
                .text_size(px(11.5))
                .text_color(color::TEXT)
                .child(text.to_string()),
        )
        .on_click(cx.listener(handler))
}

pub(super) fn toggle<F>(
    id: &str,
    text: &str,
    on: bool,
    glyph: Option<AnyElement>,
    cx: &Context<crate::app::App>,
    handler: F,
) -> impl IntoElement
where
    F: Fn(
            &mut crate::app::App,
            &gpui_kit::ClickEvent,
            &mut gpui_kit::Window,
            &mut Context<crate::app::App>,
        ) + 'static,
{
    let box_ = div()
        .id(id.to_string())
        .test_support()
        .h(px(28.0))
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .gap(px(6.0))
        .rounded(px(4.0))
        .border_1()
        .border_color(if on {
            color::ACCENT
        } else {
            color::FIELD_BORDER
        })
        .bg(if on { color::ACCENT } else { color::FIELD })
        .text_size(px(11.5))
        .text_color(if on { color::BG } else { color::TEXT })
        .cursor_pointer()
        .on_click(cx.listener(handler));
    // The page shape leads the label, as it does in the 0.1 build's markup.
    match glyph {
        Some(glyph) => box_.child(glyph).child(text.to_string()),
        None => box_.child(text.to_string()),
    }
}

/// The page shape the reference draws inside each orientation button: a bordered
/// rectangle in the page's own aspect, with two text lines across it.
/// `icons.rs` has no page outline that matches, so this composes the shape from
/// divs the way the reference composes it from an SVG `rect` and two `line`s.
/// The strokes follow the button's own text colour, so an active button keeps
/// its inverted look.
pub(super) fn page_glyph(id: &'static str, landscape: bool, on: bool) -> AnyElement {
    let (w, h) = if landscape {
        (14.0, 10.0)
    } else {
        (10.0, 14.0)
    };
    let stroke = if on { color::BG } else { color::TEXT };
    div()
        .id(id)
        .test_support()
        .w(px(w))
        .h(px(h))
        .flex_none()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(2.0))
        .rounded(px(1.5))
        .border_1()
        .border_color(stroke)
        .child(div().w(px(w * 0.5)).h(px(1.0)).flex_none().bg(stroke))
        .child(div().w(px(w * 0.5)).h(px(1.0)).flex_none().bg(stroke))
        .into_any_element()
}

/// The edit state a typed number field renders from: the open session, if any,
/// and the handle the dialog moves focus to while that session lasts.
pub(super) struct NumFieldEdit<'a> {
    pub session: Option<&'a crate::state::NumFieldSession>,
    pub focus: Option<gpui_kit::FocusHandle>,
}

/// One of the Print dialog's typed number fields. It runs on the app's shared
/// inline edit session, the same one the Move bar and Properties rows use: a
/// click opens the session, digits commit live, Escape restores the value the
/// edit started from, and Enter ends it.
pub(super) fn num_box<F>(
    id: &'static str,
    label: &str,
    width: f32,
    display: String,
    edit: NumFieldEdit<'_>,
    begin: F,
    cx: &Context<crate::app::App>,
) -> AnyElement
where
    F: Fn(&mut crate::app::App, &mut gpui_kit::Window, &mut Context<crate::app::App>)
        + Clone
        + 'static,
{
    let NumFieldEdit { session, focus } = edit;
    let key_begin = begin.clone();
    let editing = session.filter(|field| field.id == id).map(|field| {
        if field.buffer.is_empty() {
            "|".to_string()
        } else {
            format!("{}|", field.buffer)
        }
    });
    let active = editing.is_some();
    let text = editing.unwrap_or(display);
    let box_ = div()
        .id(id)
        .test_support()
        .h(px(ROW_H))
        .w(px(width))
        .flex()
        .items_center()
        .px(px(6.0))
        .rounded(px(4.0))
        .border_1()
        .border_color(if active {
            color::ACCENT
        } else {
            color::FIELD_BORDER
        })
        .bg(color::FIELD)
        .text_size(px(11.5))
        .text_color(color::TEXT)
        .cursor_pointer()
        .aria_label(label.to_string())
        .on_click(
            cx.listener(move |app, _: &gpui_kit::ClickEvent, window, cx| {
                cx.stop_propagation();
                begin(app, window, cx);
            }),
        )
        .child(div().flex_1().child(text));
    match focus {
        Some(handle) => ui::tabbable_focus(box_, &handle)
            .on_key_down(
                cx.listener(move |app, event: &gpui_kit::KeyDownEvent, window, cx| {
                    // `key_begin` carries this row's unit basis, so a keystroke
                    // converts through the click's own path and never a second time.
                    let begin = key_begin.clone();
                    app.num_field_key_in_row(
                        id,
                        event,
                        window,
                        move |app, window, cx| {
                            begin(app, window, cx);
                        },
                        cx,
                    );
                }),
            )
            .into_any_element(),
        None => box_.into_any_element(),
    }
}

/// Driver names such as `A4 (210 x 297 mm)` already carry their own dimensions,
/// so they show verbatim. A bare name gets the millimetre pair appended. This
/// is the 0.1 build's `formatPaperSizeLabel` rule, kept as a plain substring
/// test rather than the regular expression it uses.
pub(super) fn paper_name_label(name: &str, width_mm: f64, height_mm: f64) -> String {
    let lower = name.to_ascii_lowercase();
    let shows_dims = lower.chars().any(|c| c.is_ascii_digit())
        && (lower.contains("mm")
            || lower.contains('x')
            || lower.contains('\u{00d7}')
            || lower.contains('('));
    if shows_dims {
        return name.to_string();
    }
    format!("{name} ({width_mm:.0} \u{00d7} {height_mm:.0} mm)")
}

/// What the selected printer's driver answered about its paper sizes.
pub(super) enum PaperSizes {
    /// The driver listed at least one size.
    Listed(Vec<(String, f64, f64)>),
    /// The driver answered and listed none, which is the one case that lets the
    /// dialog say the chosen paper is not on this printer.
    Empty,
    /// The enumeration failed; the reason travels with it.
    Failed(String),
    /// Nobody asked, because the platform has no enumerator. The dialog makes no
    /// listing claim from this.
    Unknown,
}

/// Reads the two stored answers into one. A recorded reason wins over the list,
/// because a failure leaves the list unknown rather than empty.
pub(super) fn paper_sizes_reported(
    papers: Option<&Vec<(String, f64, f64)>>,
    error: Option<&str>,
) -> PaperSizes {
    match (papers, error) {
        (Some(papers), None) if papers.is_empty() => PaperSizes::Empty,
        (Some(papers), None) => PaperSizes::Listed(papers.clone()),
        (_, Some(reason)) => PaperSizes::Failed(reason.to_string()),
        (None, None) => PaperSizes::Unknown,
    }
}

/// The Paper Size field's own text. The old "not listed on this printer" tail
/// stays, but only where the driver reported an empty list.
pub(super) fn paper_label(index: usize, sizes: &PaperSizes) -> String {
    use crate::app::PRINT_PAPERS;
    let (name, _, w, h) = PRINT_PAPERS[index.min(PRINT_PAPERS.len() - 1)];
    let listed = match sizes {
        PaperSizes::Empty => " (not listed on this printer)", // i18n-exempt: copy not in catalog
        _ => "",
    };
    format!("{name} ({w:.0} \u{00d7} {h:.0} mm){listed}")
}

pub(super) fn unit_label(unit: &str) -> String {
    match unit {
        "cm" => t("dialogs.print.centimeters"),
        "in" => t("dialogs.print.inches"),
        "px" => t("dialogs.print.pixels"),
        _ => t("dialogs.print.millimeters"),
    }
}

pub(super) fn to_unit(mm: f64, unit: &str) -> f64 {
    match unit {
        "cm" => mm / 10.0,
        "in" => mm / 25.4,
        "px" => mm / 25.4 * PRINT_PPI,
        _ => mm,
    }
}

pub(super) fn unit_suffix(unit: &str) -> &'static str {
    match unit {
        "cm" => "cm",
        "in" => "in",
        "px" => "px",
        _ => "mm",
    }
}

#[cfg(test)]
mod tests {
    use super::{paper_label, paper_name_label, paper_sizes_reported, PaperSizes};

    /// The "not listed on this printer" tail belongs to the empty answer alone.
    /// Every other answer leaves the claim out, because the app did not check it.
    #[test]
    fn the_paper_label_claims_only_what_the_driver_answered() {
        let listed = paper_label(
            0,
            &PaperSizes::Listed(vec![("A4".to_string(), 210.0, 297.0)]),
        );
        assert!(
            !listed.contains("not listed"),
            "a printer that lists sizes earns no claim about this one: {listed}"
        );
        let unknown = paper_label(0, &PaperSizes::Unknown);
        assert!(
            !unknown.contains("not listed"),
            "an unanswered enumeration earns no claim: {unknown}"
        );
        let failed = paper_label(0, &PaperSizes::Failed("boom".to_string()));
        assert!(
            !failed.contains("not listed"),
            "a failed enumeration earns no claim: {failed}"
        );
        let empty = paper_label(0, &PaperSizes::Empty);
        assert!(
            empty.contains("not listed on this printer"),
            "an empty driver list is the one case that says it: {empty}"
        );
    }

    /// A failure outranks the list, so a refused enumeration is never read as
    /// "this printer has no paper".
    #[test]
    fn a_recorded_reason_outranks_the_stored_list() {
        let empty: Vec<(String, f64, f64)> = Vec::new();
        assert!(matches!(
            paper_sizes_reported(Some(&empty), Some("spooler busy")),
            PaperSizes::Failed(_)
        ));
        assert!(matches!(
            paper_sizes_reported(None, None),
            PaperSizes::Unknown
        ));
        assert!(matches!(
            paper_sizes_reported(Some(&empty), None),
            PaperSizes::Empty
        ));
    }

    /// A driver name that already shows its dimensions is not given them twice.
    #[test]
    fn a_driver_name_that_shows_its_dimensions_shows_verbatim() {
        assert_eq!(
            paper_name_label("A4 (210 x 297 mm)", 210.0, 297.0),
            "A4 (210 x 297 mm)"
        );
        assert_eq!(
            paper_name_label("Letter", 216.0, 279.0),
            "Letter (216 \u{00d7} 279 mm)"
        );
    }
}
