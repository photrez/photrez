//! The New Document dialog: its measured box, the preset grid, the custom-size
//! panel and the footer. `dialogs.rs` keeps the shell plumbing and calls
//! `new_document_shell`.

use super::*;

/// The New Document dialog's measured box: 780 CSS wide and 36 (titlebar) +
/// 460 (body incl. footer) tall, matching 780px / 460px.
const NEW_DOC_W: f32 = 780.0;
const NEW_DOC_H: f32 = 496.0;
const NEW_DOC_BODY_H: f32 = 460.0;
const NEW_DOC_LEFT_W: f32 = 540.0;
const NEW_DOC_RIGHT_W: f32 = 240.0;
const NEW_DOC_TABS_H: f32 = 44.0;
const NEW_DOC_FOOTER_H: f32 = 56.0;
const NEW_DOC_CARD_H: f32 = 144.0;
const NEW_DOC_CARD_GAP: f32 = 12.0;
const NEW_DOC_GRID_PAD: f32 = 20.0;
const NEW_DOC_CARD_W: f32 =
    (NEW_DOC_LEFT_W - 2.0 * NEW_DOC_GRID_PAD - 2.0 * NEW_DOC_CARD_GAP) / 3.0;

fn new_doc_field_box(active: bool) -> Div {
    div()
        .h(px(28.0))
        .w_full()
        .flex()
        .items_center()
        .rounded(px(R4))
        .border_1()
        .border_color(if active {
            color::ACCENT
        } else {
            color::FIELD_BORDER
        })
        .bg(color::FIELD)
        .shadow(vec![ui::field_well()])
        .px(px(8.0))
        .text_size(px(12.0))
        .text_color(color::TEXT)
}

fn new_doc_label(key: &str) -> Div {
    div()
        .text_size(px(12.0))
        .font_weight(FontWeight(500.))
        .text_color(color::TEXT_DIM)
        .child(t(key).to_string())
}

/// `getOrientationBadge`: L/P/SQ, the card's only icon-sized mark.
fn new_doc_orientation(width: u32, height: u32) -> &'static str {
    match width.cmp(&height) {
        std::cmp::Ordering::Greater => "L",
        std::cmp::Ordering::Less => "P",
        std::cmp::Ordering::Equal => "SQ",
    }
}

/// `getCanvasStyle`: the preview keeps the preset's aspect ratio inside an
/// 80 px box with a 20 px floor.
fn new_doc_preview_size(width: u32, height: u32) -> (f32, f32) {
    let max_dim = 80.0;
    let scale = max_dim / width.max(height) as f32;
    (
        (width as f32 * scale).round().max(20.0),
        (height as f32 * scale).round().max(20.0),
    )
}

/// The preset card: a white page preview over the name, an
/// orientation badge and the pixel dimensions.
fn new_doc_preset_card(
    index: usize,
    preset: crate::state::NewDocPreset,
    selected: bool,
    cx: &gpui_kit::Context<crate::app::App>,
) -> impl IntoElement {
    let (preview_w, preview_h) = new_doc_preview_size(preset.width, preset.height);
    div()
        .id(format!("new-doc-preset-{index}"))
        .test_support()
        .w(px(NEW_DOC_CARD_W))
        .h(px(NEW_DOC_CARD_H))
        .flex_shrink_0()
        .flex()
        .flex_col()
        .items_center()
        .p(px(12.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(if selected {
            color::ACCENT
        } else {
            color::DIVIDER
        })
        .bg(if selected {
            Hsla {
                h: 0.0,
                s: 0.0,
                l: 1.0,
                a: 0.08,
            }
        } else {
            Hsla {
                h: 0.0,
                s: 0.0,
                l: 0.0,
                a: 0.15,
            }
        })
        .cursor_pointer()
        .on_click(
            cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                cx.stop_propagation();
                app.click_new_document_preset(index, cx);
            }),
        )
        .child(
            div()
                .flex_1()
                .w_full()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .w(px(preview_w))
                        .h(px(preview_h))
                        .rounded(px(2.0))
                        .border_1()
                        .border_color(Hsla {
                            h: 0.0,
                            s: 0.0,
                            l: 0.0,
                            a: if selected { 0.4 } else { 0.2 },
                        })
                        .bg(Hsla {
                            h: 0.0,
                            s: 0.0,
                            l: 1.0,
                            a: if selected { 1.0 } else { 0.9 },
                        }),
                ),
        )
        .child(
            div()
                .flex_shrink_0()
                .mt(px(8.0))
                .w_full()
                .flex()
                .items_center()
                .justify_center()
                .gap(px(6.0))
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(FontWeight(500.))
                        .text_color(color::TEXT)
                        .child(preset.name.to_string()),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .rounded(px(2.0))
                        .bg(Hsla {
                            h: 0.0,
                            s: 0.0,
                            l: 1.0,
                            a: 0.10,
                        })
                        .px(px(4.0))
                        .py(px(1.0))
                        .text_size(px(9.0))
                        .font_weight(FontWeight(500.))
                        .text_color(color::TEXT_DIM)
                        .child(new_doc_orientation(preset.width, preset.height).to_string()),
                ),
        )
        .child(
            div()
                .flex_shrink_0()
                .mt(px(2.0))
                .text_size(px(11.0))
                .text_color(color::TEXT_DIM)
                .child(format!("{} × {}", preset.width, preset.height)),
        )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NewDocSelectKind {
    Unit,
    Background,
}

/// A select-style control: the field chrome plus the current label and a
/// chevron, opening a small option list below it.
// Explicit state, display, and control-kind inputs keep the select call site clear.
#[allow(clippy::too_many_arguments)]
fn new_doc_select(
    state: &Entity<AppState>,
    id: &'static str,
    label: String,
    open: bool,
    options: Vec<(&'static str, String, bool)>,
    center: bool,
    kind: NewDocSelectKind,
    cx: &gpui_kit::Context<crate::app::App>,
) -> Div {
    let toggle_state = state.clone();
    let mut field = new_doc_field_box(open)
        .id(id)
        .test_support()
        .cursor_pointer();
    if center {
        field = field.justify_center();
    }
    field = field.on_click(
        cx.listener(move |_app, _: &gpui_kit::ClickEvent, _window, cx| {
            cx.stop_propagation();
            toggle_state.update(cx, |state, _| {
                if let Some(session) = state.new_document.as_mut() {
                    match kind {
                        NewDocSelectKind::Unit => {
                            session.unit_open = !session.unit_open;
                            session.background_open = false;
                        }
                        NewDocSelectKind::Background => {
                            session.background_open = !session.background_open;
                            session.unit_open = false;
                        }
                    }
                }
            });
            cx.notify();
        }),
    );
    field = field.child(
        div()
            .flex_1()
            .text_size(px(12.0))
            .text_color(color::TEXT)
            .child(label.to_string()),
    );
    field = field.child(
        Icon::default()
            .data(icons::CHEVRON_DOWN)
            .size(px(10.0))
            .text_color(color::ICON),
    );
    let mut wrapper = div().relative().w_full().child(field);
    if open {
        let mut panel = v_flex()
            .id(format!("{id}-panel"))
            .absolute()
            .top(px(30.0))
            .left(px(0.0))
            .w_full()
            .p(px(4.0))
            .gap(px(2.0))
            .rounded(px(4.0))
            .border_1()
            .border_color(color::FIELD_BORDER)
            .bg(color::PANEL)
            .shadow(vec![BoxShadow::new(
                px(0.0),
                px(8.0),
                hsla(0.0, 0.0, 0.0, 0.5),
            )
            .blur_radius(px(20.0))]);
        for (value, text, selected) in options {
            let pick_state = state.clone();
            panel = panel.child(
                div()
                    .id(format!("{id}-{value}"))
                    .test_support()
                    .h(px(24.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .rounded(px(3.0))
                    .bg(if selected {
                        Hsla {
                            h: 0.0,
                            s: 0.0,
                            l: 1.0,
                            a: 0.08,
                        }
                    } else {
                        color::PANEL
                    })
                    .text_size(px(11.5))
                    .text_color(if selected { color::ACCENT } else { color::TEXT })
                    .cursor_pointer()
                    .on_click(
                        cx.listener(move |_app, _: &gpui_kit::ClickEvent, _window, cx| {
                            cx.stop_propagation();
                            pick_state.update(cx, |state, _| {
                                if let Some(session) = state.new_document.as_mut() {
                                    match kind {
                                        NewDocSelectKind::Unit => {
                                            let unit = crate::units::DimensionUnit::ALL
                                                .iter()
                                                .copied()
                                                .find(|unit| unit.label() == value)
                                                .unwrap_or_default();
                                            session.set_unit(unit);
                                        }
                                        NewDocSelectKind::Background => {
                                            session.background = if value == "transparent" {
                                                crate::state::NewDocBackground::Transparent
                                            } else {
                                                crate::state::NewDocBackground::White
                                            };
                                            session.background_open = false;
                                        }
                                    }
                                }
                            });
                            cx.notify();
                        }),
                    )
                    .child(text),
            );
        }
        wrapper = wrapper.child(panel);
    }
    wrapper
}

/// The live New Document dialog: a tabbed preset
/// grid on the left and the name/size/unit/background form on the right,
/// with the dialog's own 56 px footer.
fn new_document_body(state: &Entity<AppState>, cx: &gpui_kit::Context<crate::app::App>) -> Div {
    use crate::state::{NewDocCategory, NewDocField};

    let session = state.read(cx).new_document.clone().unwrap_or_default();

    // Category tabs: 44px, equal cells, active gets a 2 px underline.
    let mut tabs = h_flex()
        .w_full()
        .h(px(NEW_DOC_TABS_H))
        .flex_shrink_0()
        .border_b_1()
        .border_color(color::DIVIDER);
    for (index, category) in NewDocCategory::ALL.iter().enumerate() {
        let active = session.category == *category;
        let category = *category;
        tabs = tabs.child(
            div()
                .id(format!("new-doc-tab-{index}"))
                .test_support()
                .flex_1()
                .h_full()
                .relative()
                .flex()
                .items_center()
                .justify_center()
                .px(px(12.0))
                .text_size(px(12.0))
                .font_weight(FontWeight(500.))
                .text_color(if active { color::TEXT } else { color::TEXT_DIM })
                .cursor_pointer()
                .on_click(
                    cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                        cx.stop_propagation();
                        app.select_new_doc_category(category, cx);
                    }),
                )
                .when(active, |tab| {
                    tab.child(
                        div()
                            .absolute()
                            .bottom(px(0.0))
                            .left(px(0.0))
                            .right(px(0.0))
                            .h(px(2.0))
                            .bg(color::ACCENT),
                    )
                })
                .child(t(category.label_key()).to_string()),
        );
    }

    // The 3-column grid is laid out in rows so every card keeps its measured
    // size; a wrapping flex would re-flow the fixed 3 columns.
    let presets = session.category.presets();
    let mut grid = v_flex().w_full().gap(px(NEW_DOC_CARD_GAP));
    let mut card_index = 0;
    for row in presets.chunks(3) {
        let mut row_el = h_flex().w_full().gap(px(NEW_DOC_CARD_GAP));
        for preset in row {
            row_el = row_el.child(new_doc_preset_card(
                card_index,
                *preset,
                session.preset_selected(preset),
                cx,
            ));
            card_index += 1;
        }
        grid = grid.child(row_el);
    }

    let left = v_flex()
        .w(px(NEW_DOC_LEFT_W))
        .flex_shrink_0()
        .h_full()
        .border_r_1()
        .border_color(color::DIVIDER)
        .bg(color::PANEL)
        .child(tabs)
        .child(
            div()
                .flex_1()
                .min_h(px(0.0))
                .overflow_hidden()
                .p(px(NEW_DOC_GRID_PAD))
                .child(grid),
        );

    // Right panel: Name, Width/Height/Unit, Background.
    // Each text field carries its own focus handle, so Tab lands on it and the
    // dialog's keystrokes follow real focus.
    let (name_focus, width_focus, height_focus) = {
        let read = state.read(cx);
        (
            read.new_document_field_focus(NewDocField::Name),
            read.new_document_field_focus(NewDocField::Width),
            read.new_document_field_focus(NewDocField::Height),
        )
    };

    let name_field = chrome::tabbable_field(
        new_doc_field_box(session.field == Some(NewDocField::Name))
            .id("new-doc-name")
            .test_support()
            .cursor_pointer()
            .aria_label(t("dialogs.newDocument.name").to_string())
            .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, window, cx| {
                cx.stop_propagation();
                app.focus_new_doc_field(NewDocField::Name, window, cx);
            }))
            .child(format!("{}|", session.name)),
        &name_focus,
    );

    let width_field = chrome::tabbable_field(
        new_doc_field_box(session.field == Some(NewDocField::Width))
            .id("new-doc-width")
            .test_support()
            .cursor_pointer()
            .aria_label(t("dialogs.newDocument.width").to_string())
            .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, window, cx| {
                cx.stop_propagation();
                app.focus_new_doc_field(NewDocField::Width, window, cx);
            }))
            .child(format!("{}|", session.width_text)),
        &width_focus,
    );

    let height_field = chrome::tabbable_field(
        new_doc_field_box(session.field == Some(NewDocField::Height))
            .id("new-doc-height")
            .test_support()
            .cursor_pointer()
            .aria_label(t("dialogs.newDocument.height").to_string())
            .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, window, cx| {
                cx.stop_propagation();
                app.focus_new_doc_field(NewDocField::Height, window, cx);
            }))
            .child(format!("{}|", session.height_text)),
        &height_focus,
    );

    let unit_select = new_doc_select(
        state,
        "new-doc-unit",
        session.unit.label().to_string(),
        session.unit_open,
        DimensionUnit::ALL
            .iter()
            .map(|unit| {
                (
                    unit.label(),
                    unit.label().to_string(),
                    *unit == session.unit,
                )
            })
            .collect(),
        true,
        NewDocSelectKind::Unit,
        cx,
    );

    let unit_label = new_doc_label("dialogs.newDocument.unit");
    let background_select = new_doc_select(
        state,
        "new-doc-background",
        t(session.background.label_key()).to_string(),
        session.background_open,
        vec![
            (
                "transparent",
                t("dialogs.newDocument.transparent").to_string(),
                session.background == crate::state::NewDocBackground::Transparent,
            ),
            (
                "white",
                t("dialogs.newDocument.white").to_string(),
                session.background == crate::state::NewDocBackground::White,
            ),
        ],
        false,
        NewDocSelectKind::Background,
        cx,
    );

    let right = v_flex()
        .w(px(NEW_DOC_RIGHT_W))
        .flex_shrink_0()
        .h_full()
        .p(px(16.0))
        .gap(px(16.0))
        .bg(Hsla {
            h: color::TOPBAR.h,
            s: color::TOPBAR.s,
            l: color::TOPBAR.l,
            a: 0.30,
        })
        .child(
            v_flex()
                .gap(px(6.0))
                .child(new_doc_label("dialogs.newDocument.name"))
                .child(name_field),
        )
        .child(
            h_flex()
                .gap(px(8.0))
                .items_start()
                .child(
                    v_flex()
                        .flex_1()
                        .min_w(px(0.0))
                        .gap(px(6.0))
                        .child(new_doc_label("dialogs.newDocument.width"))
                        .child(width_field),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w(px(0.0))
                        .gap(px(6.0))
                        .child(new_doc_label("dialogs.newDocument.height"))
                        .child(height_field),
                )
                .child(
                    v_flex()
                        .w(px(60.0))
                        .flex_shrink_0()
                        .gap(px(6.0))
                        .child(unit_label)
                        .child(unit_select),
                ),
        )
        .child(
            v_flex()
                .gap(px(6.0))
                .child(new_doc_label("dialogs.newDocument.background"))
                .child(background_select),
        );

    let footer = h_flex()
        .w_full()
        .h(px(NEW_DOC_FOOTER_H))
        .flex_shrink_0()
        .items_center()
        .justify_end()
        .gap(px(12.0))
        .px(px(20.0))
        .border_t_1()
        .border_color(color::DIVIDER)
        .bg(color::PANEL)
        .child(
            new_doc_button(&t("common.cancel"), false)
                .id("new-doc-cancel")
                .test_support()
                .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
                    cx.stop_propagation();
                    app.close_new_document(cx);
                })),
        )
        .child(
            new_doc_button(&t("dialogs.newDocument.create"), true)
                .id("new-doc-create")
                .test_support()
                .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
                    cx.stop_propagation();
                    app.create_new_document(cx);
                })),
        );

    v_flex()
        .w_full()
        .h(px(NEW_DOC_BODY_H))
        .flex_shrink_0()
        .child(
            h_flex()
                .w_full()
                .flex_1()
                .min_h(px(0.0))
                .child(left)
                .child(right),
        )
        .child(footer)
}

/// The source's dialog button, 96x32: 6 px radius, 11 px medium.
fn new_doc_button(label: &str, primary: bool) -> Div {
    div()
        .w(px(96.0))
        .h(px(32.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(BTN_R))
        .border_1()
        .border_color(if primary {
            color::ACCENT
        } else {
            color::FIELD_BORDER
        })
        .bg(if primary { color::ACCENT } else { color::FIELD })
        .text_size(px(BTN_TEXT))
        .font_weight(if primary {
            FontWeight(600.)
        } else {
            FontWeight(500.)
        })
        .text_color(if primary {
            Hsla {
                h: 0.0,
                s: 0.0,
                l: 0.0,
                a: 0.9,
            }
        } else {
            color::TEXT
        })
        .cursor_pointer()
        .child(label.to_string())
}

/// The measured shell for the live New Document dialog: the shared titlebar
/// plus the dialog's own 460 px body (its footer is part of that body).
pub(super) fn new_document_shell(
    state: &Entity<AppState>,
    cx: &gpui_kit::Context<crate::app::App>,
) -> impl IntoElement {
    let stop = cx.listener(|_app, _: &gpui_kit::ClickEvent, _window, cx| {
        cx.stop_propagation();
    });
    v_flex()
        .id("new-doc-shell")
        .test_support()
        .w(px(NEW_DOC_W))
        .h(px(NEW_DOC_H))
        .rounded(px(SHELL_R))
        .border_1()
        .border_color(color::FIELD_BORDER)
        .bg(color::PANEL)
        .overflow_hidden()
        .shadow(vec![BoxShadow::new(
            px(0.0),
            px(18.0),
            hsla(0.0, 0.0, 0.0, 0.55),
        )
        .blur_radius(px(50.0))])
        .on_click(stop)
        .child(chrome::titlebar(&t("dialogs.newDocument.title")))
        .child(new_document_body(state, cx))
}
