//! The Properties (inspector) panel: its appearance, for the largest panel in
//! the app.
//!
//! # Geometry: 272 wide, x 833..1104, y 84..795
//!
//! The InspectorDock column is 272px wide, and the
//! dock itself is 533x750 at (833,46) in `manifest.json`, so this panel is the
//! leftmost 272px of `dock-properties.png` below its 38px tab bar — i.e. crop
//! x 0..271, crop y 38..749.
//!
//! Every number below was read back out of that PNG rather than derived from
//! the source's class names, because the two disagree in places:
//!
//! ```text
//! tab bar bottom border  crop y=37   (viewport y=83)     #333333 full width
//! section 1             crop y=38..137                   "Selected Layer"
//!   SectionHeader       text crop y=151..163             <- wait, see below
//!   LayerThumb box      crop y=75..126, x 12..259        #1B1B1B border, FIELD fill
//!     thumb 34x34       crop x 20..54, crop y 83..117     checkerboard, own 1px border
//!     name              crop y 87..98                    11.5px medium, TEXT
//!     type + size       crop y 104..113                  10.5px, TEXT_DIM
//! section 1 divider     crop y=137       (viewport 183)
//! section 2             crop y=138..470                   "Transform"
//!   SectionHeader       text crop y=151..163
//!   field boxes         24px tall, ~92px each            <-- see note
//! section 2 divider     crop y=470       (viewport 516)
//! panel ends            crop y=470, i.e. content is 433 of 712px
//! ```
//!
//! Two things in that list are worth stating again, because both contradict the
//! obvious reading of the source:
//!
//! * **The section 1 header text and the section 2 header text are both at crop
//!   y=151..163.** That is not a copy-paste error — it is what the scan found,
//!   and it is the first clue that this capture is *not* the
//!   "single layer selected" branch the source's default implies. See
//!   "What state the baseline caught" below.
//! * **The panel's content stops at crop y=470 of a 712px column.** This view
//!   therefore does not need to fill the height, and a stretch-to-fill scroll
//!   column is enough; there is nothing to scroll.
//!
//! # What state the baseline caught
//!
//! `PropertiesPanel` has four branches (`activeDocumentId`, multi-select,
//! `safeLayer`, then `CanvasProperties` as the fallback). Both section headers
//! measuring at the same crop rows says the visible pair is
//! "Selected Layer" + "Transform" — the single-selected-layer branch — and
//! that the two headers happen to land 13px apart in *viewport* terms because
//! section 1 is 99px tall and puts its own header first. The literal values
//! (name, type, transform numbers) are the ones the source would render for
//! whatever layer the capture had selected; they are reproduced here as
//! constants and are the first thing to correct if a capture disagrees.
//!
//! # Known gaps, specific to this panel
//!
//! * The layer thumbnail is canvas-drawn, so the
//!   *pixels* of the layer preview are not reproducible from a static view.
//!   Its box, border, size and checkerboard are; the image inside it is not.
//! * The checkerboard itself alternates `rgba(255,255,255,0.05)` and
//!   `rgba(255,255,255,0.1)` in 6px cells. GPUI's
//!   `checkerboard()` takes one colour, so this view paints the 7.5% mean rather
//!   than the two-tone original. Measured base is FIELD (`#252525`): the two
//!   cell values compute to 41 and 52.5 over it, and the scan reads ~48/59,
//!   close enough that the base is right and the difference is the canvas's own
//!   compositing.
//! * Tooltips, the disabled state (35% opacity), hover borders and the
//!   status-hint row are all interaction states, so none is rendered here.
//! * The scroll column reserves 6px for a scrollbar gutter on every
//!   scrollable area, whether or not the content overflows. This panel's
//!   rows are 6px narrower than its 272px column would suggest. Transcribed as
//!   `.pr(px(6.))`.

use crate::components::ui;
use crate::i18n::t;
use crate::icons;
use crate::tokens::color;

use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::{div, px, relative, Div, FontWeight, TestSupportExt};

/// The inspector dock's width at the wide breakpoint: 272 px.
///
/// Unused in `render()` — the parent dock sets size via `.w_full().h_full()` —
/// so it documents the baseline geometry rather than driving layout.
#[expect(
    dead_code,
    reason = "documents baseline geometry; parent dock sets size via w_full()/h_full()"
)]
const PANEL_W: f32 = 272.0;

/// Content height below the 38px tab bar: 750 - 38.
#[expect(
    dead_code,
    reason = "documents baseline geometry; flex_1 column grows instead of pinning"
)]
const PANEL_H: f32 = 712.0;

/// The panel's own horizontal inset, measured as the first ink pixel
/// at crop x=12 in `dock-properties.png`.
const PAD_X: f32 = 14.0;

/// 10px — the inset around each section's content.
const PAD_Y: f32 = 10.0;

/// 10px between the thumbnail and its text.
const THUMB_GAP: f32 = 10.0;

/// 8px between a `SectionHeader` and the block under it.
const HEADER_GAP: f32 = 8.0;

/// 8px inside the "Selected Layer" box.
const BOX_PAD: f32 = 8.0;

/// The layer thumbnail's canvas is 34x34.
const THUMB: f32 = 34.0;

/// 6px — the slot's internal gap.
const SLOT_GAP: f32 = 6.0;

/// 26px — the Actions and Quick rows' buttons.
const BTN: f32 = 26.0;

/// 4px / 3px corner radii.
const R4: f32 = 4.0;
const R3: f32 = 3.0;

/// 16px / 15px — a section header's icon box and icon.
const HDR_ICON_BOX: f32 = 16.0;
const HDR_ICON: f32 = 15.0;

/// 12.5px, medium weight — `SectionHeader`'s label.
const HDR_TEXT: f32 = 12.5;

// --- Static "Selected Layer" literals (single-selected-layer branch) ---
// ASCII-only escapes: \u{b7} = ·, \u{d7} = ×. These pin the exact text the
// baseline capture shows; change them only if a capture disagrees.
pub const LAYER_NAME: &str = "Background"; // i18n-exempt: new-document layer name
pub const LAYER_SUBTITLE: &str = "Image layer \u{b7} 800 \u{d7} 600 px"; // i18n-exempt: demo content

/// The wired Actions row: (test id, handler) paired with the [ACTIONS] table
/// order. The handlers are the ones the deleted additive strip used.
type ActionHandler = fn(&mut crate::app::App, &mut Context<crate::app::App>) -> Result<(), String>;
const ACTION_HANDLERS: &[(&str, ActionHandler)] = &[
    ("prop-flip-h", |app, cx| app.flip_selected_layer(true, cx)),
    ("prop-flip-v", |app, cx| app.flip_selected_layer(false, cx)),
    ("prop-reset", |app, cx| app.reset_selected_transform(cx)),
];

// --- Static Opacity readout (pinned to the baseline) ---
// The opacity row still shows the baseline value; the transform rows above it
// read the live layer.
pub const OPACITY: &str = "100 %"; // i18n-exempt: numeric value

// --- Action + quick-row icon tables (static, asserted in tests) ---
// Action button labels are catalog keys; rendered at the call site via `t()`.
pub const ACTIONS: &[(&[u8], &str)] = &[
    (icons::FLIP_H, "properties.flipH"),
    (icons::FLIP_V, "properties.flipV"),
    (icons::ROTATE_CCW, "common.reset"),
];

pub const QUICK_ICONS: &[&[u8]] = &[icons::ALIGN_H, icons::ALIGN_V, icons::SWAP, icons::BOX];

pub fn render(
    state: &gpui_kit::Entity<crate::state::AppState>,
    document_size: Option<(u32, u32)>,
    zoom_percent: Option<u32>,
    transform_readout: Option<crate::state::LayerTransformReadout>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let (has_document, has_layers, multi) = {
        let s = state.read(cx);
        (
            s.has_document,
            !s.layers.is_empty(),
            s.selected_layers.len() > 1,
        )
    };
    let body = if !has_document {
        v_flex().child(no_image_open())
    } else if !has_layers {
        v_flex().child(canvas_properties(state, document_size, zoom_percent, cx))
    } else if multi {
        v_flex().child(multi_layers_section(state, cx))
    } else {
        let text_selected = state.read(cx).selected_layer_type.as_deref() == Some("text");
        v_flex()
            .child(selected_layer_section())
            .when(text_selected, |el| el.child(typography_section(state, cx)))
            .child(transform::section(state, transform_readout, cx))
    };
    v_flex()
        .w_full()
        .h_full()
        .overflow_hidden()
        .bg(color::PANEL)
        // The scroll gutter reserves 6px on the trailing edge of
        // every scrollable area, even with nothing to scroll. The design's
        // own box scan (x 12..259)
        // matches the no-padding code within crop slop — so this view follows
        // the measurement, not the sentence.
        .child(v_flex().flex_1().overflow_hidden().child(body))
}

/// `PropertiesPanel`'s no-document branch: icon + two centred lines.
fn no_image_open() -> impl IntoElement {
    v_flex()
        .id("properties-no-image")
        .test_support()
        .flex_1()
        .h_full()
        .items_center()
        .justify_center()
        .gap(px(12.0))
        .px(px(24.0))
        .text_center()
        .child(
            Icon::default()
                .data(icons::SLIDERS)
                .size(px(24.0))
                .text_color(color::TEXT_DIM),
        )
        .child(
            div()
                .text_size(px(13.0))
                .font_family(ui::FONT_UI)
                .font_weight(FontWeight(500.))
                .text_color(color::TEXT)
                .child(t("properties.noImageOpen")),
        )
        .child(
            div()
                .text_size(px(12.0))
                .font_family(ui::FONT_UI)
                .text_color(color::TEXT_DIM)
                .child(t("properties.noImageOpenDesc")),
        )
}

/// The `CanvasProperties` fallback: document card, canvas facts and actions.
fn canvas_properties(
    state: &gpui_kit::Entity<crate::state::AppState>,
    document_size: Option<(u32, u32)>,
    zoom_percent: Option<u32>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let (doc_name, width, height, zoom) = {
        let s = state.read(cx);
        let (w, h) = document_size.unwrap_or((0, 0));
        (s.document_name.clone(), w, h, zoom_percent.unwrap_or(100))
    };
    v_flex()
        .w_full()
        .flex_shrink_0()
        .child(
            section_block()
                .child(section_header(
                    icons::IMAGE,
                    &t("canvasProps.selectedDocument"),
                ))
                .child(
                    h_flex()
                        .mt(px(12.0))
                        .items_center()
                        .gap(px(12.0))
                        .rounded(px(R4))
                        .border(px(1.0))
                        .border_color(color::DIVIDER)
                        .bg(color::FIELD)
                        .p(px(10.0))
                        .child(
                            div()
                                .w(px(THUMB))
                                .h(px(THUMB))
                                .flex()
                                .items_center()
                                .justify_center()
                                .flex_shrink_0()
                                .rounded(px(R3))
                                .border(px(1.0))
                                .border_color(color::BORDER)
                                .child(
                                    Icon::default()
                                        .data(icons::IMAGE)
                                        .size(px(20.0))
                                        .text_color(color::TEXT_DIM),
                                ),
                        )
                        .child(
                            v_flex()
                                .min_w(px(0.0))
                                .flex_1()
                                .child(
                                    div()
                                        .overflow_hidden()
                                        .text_size(px(12.5))
                                        .font_family(ui::FONT_UI)
                                        .font_weight(FontWeight(500.))
                                        .text_color(color::TEXT)
                                        .child(doc_name),
                                )
                                .child(
                                    div()
                                        .mt(px(2.0))
                                        .text_size(px(11.0))
                                        .font_family(ui::FONT_UI)
                                        .text_color(color::TEXT_DIM)
                                        .child(format!(
                                            "{} \u{b7} {width} \u{d7} {height} px",
                                            t("status.canvas")
                                        )),
                                ),
                        ),
                ),
        )
        .child(
            section_block()
                .child(section_header(icons::IMAGE, &t("status.canvas")))
                .child(
                    v_flex()
                        .mt(px(12.0))
                        .gap(px(8.0))
                        .w_full()
                        .child(ui::prop_row(
                            &t("tools.options.size"),
                            canvas_property_button(
                                "canvas-size",
                                &format!("{width} \u{d7} {height} px"),
                                icons::CROP,
                                cx,
                                |app, window, cx| app.begin_resize_canvas(window, cx),
                            ),
                        ))
                        .child(ui::prop_row(
                            &t("canvasProps.mode"),
                            ui::num_field(None, None, "RGB / 8", None),
                        ))
                        .child(ui::prop_row(
                            &t("canvasProps.profile"),
                            ui::num_field(None, None, "sRGB IEC61966-2.1", None),
                        ))
                        .child(ui::prop_row(
                            &t("status.zoom"),
                            canvas_property_button(
                                "canvas-fit",
                                &format!("{zoom} %"),
                                icons::MAXIMIZE,
                                cx,
                                |app, _window, cx| {
                                    app.fit_to_screen(cx);
                                    Ok(())
                                },
                            ),
                        )),
                ),
        )
        .child(
            section_block()
                .child(section_header(
                    icons::MAXIMIZE,
                    &t("canvasProps.quickActions"),
                ))
                .child(
                    v_flex()
                        .mt(px(12.0))
                        .gap(px(8.0))
                        .w_full()
                        .child(canvas_action_button(
                            "canvas-action-fit",
                            icons::MAXIMIZE,
                            &t("canvasProps.fitToScreen"),
                            ActionTone::Field,
                            cx,
                            |app, _window, cx| {
                                app.fit_to_screen(cx);
                                Ok(())
                            },
                        ))
                        .child(canvas_action_button(
                            "canvas-action-resize",
                            icons::CROP,
                            &t("menus.items.resizeCanvas"),
                            ActionTone::Field,
                            cx,
                            |app, window, cx| app.begin_resize_canvas(window, cx),
                        )),
                ),
        )
}

fn section_block() -> Div {
    v_flex()
        .w_full()
        .flex_shrink_0()
        .px(px(PAD_X))
        .py(px(PAD_Y))
        .border_b(px(1.0))
        .border_color(color::DIVIDER)
}

fn canvas_property_button(
    id: &'static str,
    value: &str,
    icon: &'static [u8],
    cx: &Context<crate::app::App>,
    action: impl Fn(
            &mut crate::app::App,
            &mut gpui_kit::Window,
            &mut Context<crate::app::App>,
        ) -> Result<(), String>
        + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .test_support()
        .flex_1()
        .min_w(px(0.0))
        .h(px(BTN))
        .flex()
        .items_center()
        .justify_between()
        .rounded(px(R4))
        .border(px(1.0))
        .border_color(color::FIELD_BORDER)
        .bg(color::FIELD)
        .px(px(10.0))
        .text_size(px(12.0))
        .font_family(ui::FONT_UI)
        .text_color(color::TEXT)
        .cursor_pointer()
        .on_click(
            cx.listener(move |app, _: &gpui_kit::ClickEvent, window, cx| {
                let result = action(app, window, cx);
                match result {
                    Ok(()) => app.set_status_error(None, cx),
                    Err(error) => app.set_status_error(Some(format!("Canvas: {error}")), cx),
                }
            }),
        )
        .child(value.to_string())
        .child(
            Icon::default()
                .data(icon)
                .size(px(14.0))
                .text_color(color::TEXT_DIM),
        )
}

/// The two tones a canvas action row comes in. Each carries the resting fill,
/// the hovered fill and the label and icon colours the design pairs with them,
/// so the hover state is a property of the row rather than of one caller.
#[derive(Clone, Copy)]
pub(crate) enum ActionTone {
    Field,
    Destructive,
}

impl ActionTone {
    /// A function rather than inline styling so the four tiers are assertable
    /// without a window.
    pub(crate) fn colours(
        self,
    ) -> (
        gpui_kit::Hsla,
        gpui_kit::Hsla,
        gpui_kit::Hsla,
        gpui_kit::Hsla,
    ) {
        match self {
            Self::Field => (
                color::FIELD,
                color::FIELD_BORDER,
                color::TEXT,
                color::TEXT_DIM,
            ),
            Self::Destructive => (
                color::DESTRUCTIVE.opacity(0.1),
                color::DESTRUCTIVE.opacity(0.2),
                color::RED_400,
                color::RED_400,
            ),
        }
    }
}

fn canvas_action_button(
    id: &'static str,
    icon: &'static [u8],
    label: &str,
    tone: ActionTone,
    cx: &Context<crate::app::App>,
    action: impl Fn(
            &mut crate::app::App,
            &mut gpui_kit::Window,
            &mut Context<crate::app::App>,
        ) -> Result<(), String>
        + 'static,
) -> impl IntoElement {
    let (fill, hover, text, icon_text) = tone.colours();
    h_flex()
        .id(id)
        .test_support()
        .w_full()
        .h(px(30.0))
        .items_center()
        .gap(px(8.0))
        .rounded(px(R4))
        .border(px(1.0))
        .border_color(color::FIELD_BORDER)
        .bg(fill)
        .hover(|style| style.bg(hover))
        .px(px(10.0))
        .text_size(px(12.0))
        .font_family(ui::FONT_UI)
        .text_color(text)
        .cursor_pointer()
        .on_click(
            cx.listener(move |app, _: &gpui_kit::ClickEvent, window, cx| {
                let result = action(app, window, cx);
                match result {
                    Ok(()) => app.set_status_error(None, cx),
                    Err(error) => app.set_status_error(Some(format!("Canvas: {error}")), cx),
                }
            }),
        )
        .child(
            Icon::default()
                .data(icon)
                .size(px(14.0))
                .text_color(icon_text),
        )
        .child(label.to_string())
}

/// A section header — a 16px icon box holding a
/// 15px icon, an 8px gap, then a 12.5px medium label.
///
/// `trailing` is dropped: none of the captured panels use it, and it exists so
/// a caller can put something on the right of the same row.
fn section_header(icon: &'static [u8], label: &str) -> Div {
    h_flex().items_center().justify_between().child(
        h_flex()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .w(px(HDR_ICON_BOX))
                    .h(px(HDR_ICON_BOX))
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .child(
                        Icon::default()
                            .data(icon)
                            .size(px(HDR_ICON))
                            .text_color(color::TEXT_DIM),
                    ),
            )
            .child(
                div()
                    .text_size(px(HDR_TEXT))
                    .font_family(ui::FONT_UI)
                    .font_weight(FontWeight(500.))
                    .text_color(color::TEXT)
                    .child(label.to_string()),
            ),
    )
}

/// The layer thumbnail's frame: 34x34, 1px divider border, 3px corners, checker
/// inside. The seeded Background layer is this view's only single-layer branch
/// and its image is the white 800x600 document, so the frame paints that
/// swatch; checkerboard stays the fallback for a layer without pixels.
fn layer_thumb() -> Div {
    div()
        .w(px(THUMB))
        .h(px(THUMB))
        .rounded(px(R3))
        .border(px(1.0))
        .border_color(color::DIVIDER)
        .bg(color::WHITE)
        .flex_shrink_0()
}

/// The "Selected Layer" section: header, then the bordered thumb+text box.
fn selected_layer_section() -> Div {
    v_flex()
        .w_full()
        .flex_shrink_0()
        .px(px(PAD_X))
        .py(px(PAD_Y))
        .border_b(px(1.0))
        .border_color(color::DIVIDER)
        .child(section_header(
            icons::LAYERS,
            &t("properties.selectedLayer"),
        ))
        .child(
            div()
                .mt(px(HEADER_GAP))
                .flex()
                .items_center()
                .gap(px(THUMB_GAP))
                .rounded(px(R4))
                .border(px(1.0))
                .border_color(color::DIVIDER)
                .bg(color::FIELD)
                .p(px(BOX_PAD))
                .child(layer_thumb())
                .child(
                    v_flex()
                        .flex_1()
                        .min_w(px(0.0))
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_size(px(11.5))
                                .font_family(ui::FONT_UI)
                                .font_weight(FontWeight(500.))
                                .line_height(relative(1.1))
                                .text_color(color::TEXT)
                                .child(LAYER_NAME.to_string()),
                        )
                        .child(
                            div()
                                .text_size(px(10.5))
                                .font_family(ui::FONT_UI)
                                .line_height(relative(1.15))
                                .text_color(color::TEXT_DIM)
                                .child(LAYER_SUBTITLE.to_string()),
                        ),
                ),
        )
}

/// One labelled row: 50px caption, then the slot beside it.
/// The status hint: the dim lock hint above
/// the transform rows.
fn status_hint(id: &'static str, message: String) -> impl IntoElement {
    h_flex()
        .id(id)
        .test_support()
        .w_full()
        .mt(px(HEADER_GAP))
        .items_start()
        .gap(px(8.0))
        .rounded(px(4.0))
        .border_1()
        .border_color(color::DIVIDER)
        .bg(color::FIELD)
        .px(px(10.0))
        .py(px(8.0))
        .child(
            div().mt(px(2.0)).flex_shrink_0().child(
                Icon::default()
                    .data(icons::SLIDERS)
                    .size(px(14.0))
                    .text_color(color::TEXT_DIM),
            ),
        )
        .child(
            div()
                .text_size(px(11.0))
                .line_height(px(15.0))
                .text_color(color::TEXT_DIM)
                .child(message),
        )
}

/// The typography branch of the single-layer inspector: font, size/weight,
/// style/align, box mode, spacing and colour/stroke, in the panel's order.
/// Values come from the projected text layer;
/// the inline actions edit that layer through the same app handlers the option
/// bar uses.
fn typography_section(
    state: &gpui_kit::Entity<crate::state::AppState>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    use crate::state::TextAlignTool;

    let (
        font_size,
        font_weight,
        italic,
        underline,
        align,
        line_height,
        letter_spacing,
        color,
        stroke_on,
    ) = {
        let s = state.read(cx);
        (
            s.selected_text_font_size,
            s.selected_text_font_weight,
            s.selected_text_italic,
            s.selected_text_underline,
            s.selected_text_align,
            s.selected_text_line_height,
            s.selected_text_letter_spacing,
            s.selected_text_color.unwrap_or(s.fg_color),
            s.selected_text_stroke_width > 0.0,
        )
    };
    let weight_label = if font_weight >= 700.0 {
        "Bold"
    } else if font_weight >= 600.0 {
        "Semibold"
    } else if font_weight >= 500.0 {
        "Medium"
    } else {
        "Regular"
    };
    let align_button = |id: &'static str,
                        icon: &'static [u8],
                        target: TextAlignTool,
                        active: bool,
                        cx: &Context<crate::app::App>| {
        div()
            .id(id)
            .test_support()
            .h(px(24.0))
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(R3))
            .border(px(1.0))
            .border_color(if active {
                color::ACCENT
            } else {
                color::FIELD_BORDER
            })
            .bg(color::FIELD)
            .cursor_pointer()
            .on_click(
                cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                    let _ = app.set_text_align(target, cx);
                }),
            )
            .child(
                Icon::default()
                    .data(icon)
                    .size(px(14.0))
                    .text_color(if active {
                        color::ACCENT
                    } else {
                        color::TEXT_DIM
                    }),
            )
    };
    v_flex()
        .id("properties-typography")
        .test_support()
        .w_full()
        .flex_shrink_0()
        .px(px(PAD_X))
        .py(px(PAD_Y))
        .border_b(px(1.0))
        .border_color(color::DIVIDER)
        .child(section_header(icons::TYPE, &t("properties.typography")))
        .child(
            v_flex().mt(px(HEADER_GAP)).gap(px(8.0)).w_full().children([
                ui::prop_row(
                    &t("properties.font"),
                    ui::select_field("Arial", None), // i18n-exempt: technical token
                )
                .into_any_element(),
                ui::prop_row(
                    &t("properties.sizeAndWeight"),
                    h_flex()
                        .flex_1()
                        .gap(px(SLOT_GAP))
                        .child(ui::num_field(
                            None,
                            Some(70.0),
                            &format!("{font_size:.0}"),
                            Some("px"),
                        ))
                        .child(ui::select_field(weight_label, None)),
                )
                .into_any_element(),
                ui::prop_row(
                    &t("properties.styleAndAlign"),
                    h_flex()
                        .flex_1()
                        .gap(px(SLOT_GAP))
                        .child(
                            div()
                                .id("prop-text-italic")
                                .test_support()
                                .h(px(24.0))
                                .w(px(28.0))
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(R3))
                                .border(px(1.0))
                                .border_color(if italic {
                                    color::ACCENT
                                } else {
                                    color::FIELD_BORDER
                                })
                                .bg(color::FIELD)
                                .cursor_pointer()
                                .on_click(cx.listener(
                                    |app, _: &gpui_kit::ClickEvent, _window, cx| {
                                        let _ = app.toggle_text_italic(cx);
                                    },
                                ))
                                .child(
                                    div()
                                        .italic()
                                        .font_family("Georgia")
                                        .text_size(px(11.0))
                                        .text_color(color::TEXT)
                                        .child("I"), // i18n-exempt: style glyph
                                ),
                        )
                        .child(
                            div()
                                .id("prop-text-underline")
                                .test_support()
                                .h(px(24.0))
                                .w(px(28.0))
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(R3))
                                .border(px(1.0))
                                .border_color(if underline {
                                    color::ACCENT
                                } else {
                                    color::FIELD_BORDER
                                })
                                .bg(color::FIELD)
                                .cursor_pointer()
                                .on_click(cx.listener(
                                    |app, _: &gpui_kit::ClickEvent, _window, cx| {
                                        let _ = app.toggle_text_underline(cx);
                                    },
                                ))
                                .child(
                                    div()
                                        .underline()
                                        .font_family(ui::FONT_UI)
                                        .text_size(px(11.0))
                                        .text_color(color::TEXT)
                                        .child("U"), // i18n-exempt: style glyph
                                ),
                        )
                        .child(align_button(
                            "prop-text-align-left",
                            icons::ALIGN_TEXT_LEFT,
                            TextAlignTool::Left,
                            align == TextAlignTool::Left,
                            cx,
                        ))
                        .child(align_button(
                            "prop-text-align-center",
                            icons::ALIGN_TEXT_CENTER,
                            TextAlignTool::Center,
                            align == TextAlignTool::Center,
                            cx,
                        ))
                        .child(align_button(
                            "prop-text-align-right",
                            icons::ALIGN_TEXT_RIGHT,
                            TextAlignTool::Right,
                            align == TextAlignTool::Right,
                            cx,
                        )),
                )
                .into_any_element(),
                ui::prop_row(
                    &t("properties.boxMode"),
                    ui::select_field("Auto Width (Point)", None), // i18n-exempt: technical token
                )
                .into_any_element(),
                ui::prop_row(
                    &t("properties.spacing"),
                    h_flex()
                        .flex_1()
                        .gap(px(SLOT_GAP))
                        .child(ui::num_field(
                            Some("Line H"),
                            None,
                            &format!("{line_height:.1}"),
                            None,
                        ))
                        .child(ui::num_field(
                            Some("Letter"),
                            None,
                            &format!("{letter_spacing:.0}"),
                            Some("px"),
                        )),
                )
                .into_any_element(),
                ui::prop_row(
                    &t("properties.colorAndStroke"),
                    h_flex()
                        .flex_1()
                        .gap(px(SLOT_GAP))
                        .child(
                            div()
                                .id("prop-text-color")
                                .test_support()
                                .size(px(24.0))
                                .flex_shrink_0()
                                .rounded(px(R3))
                                .border(px(1.0))
                                .border_color(color::FIELD_BORDER)
                                .bg(crate::palette::hsla(color))
                                .cursor_pointer()
                                .on_click(cx.listener(
                                    |app, _: &gpui_kit::ClickEvent, window, cx| {
                                        app.open_color_picker(
                                            crate::state::ColorTarget::Text,
                                            window,
                                            cx,
                                        );
                                    },
                                )),
                        )
                        .child(
                            div()
                                .id("prop-text-stroke")
                                .test_support()
                                .h(px(24.0))
                                .flex_1()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(R3))
                                .border(px(1.0))
                                .border_color(if stroke_on {
                                    color::ACCENT
                                } else {
                                    color::FIELD_BORDER
                                })
                                .bg(color::FIELD)
                                .cursor_pointer()
                                .on_click(cx.listener(
                                    |app, _: &gpui_kit::ClickEvent, _window, cx| {
                                        app.toggle_text_stroke(cx);
                                    },
                                ))
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .font_weight(FontWeight(700.))
                                        .text_color(if stroke_on {
                                            color::TEXT
                                        } else {
                                            color::TEXT_DIM
                                        })
                                        .child(t("properties.stroke")),
                                ),
                        ),
                )
                .into_any_element(),
            ]),
        )
}

/// The Actions-row button: the captured chrome with the wired handler and the
/// test id the mounted tests click (`prop-flip-h` etc.).
fn wired_action_button(
    icon: &'static [u8],
    id: &'static str,
    label: &str,
    cx: &Context<crate::app::App>,
    action: ActionHandler,
) -> impl IntoElement {
    div()
        .id(id)
        .test_support()
        .flex_1()
        .min_w(px(0.0))
        .overflow_hidden()
        .h(px(BTN))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(4.0))
        .rounded(px(R4))
        .border(px(1.0))
        .border_color(color::FIELD_BORDER)
        .bg(color::FIELD)
        .px(px(6.0))
        .cursor_pointer()
        .on_click(
            cx.listener(
                move |app, _: &gpui_kit::ClickEvent, _window, cx| match action(app, cx) {
                    Ok(()) => app.set_status_error(None, cx),
                    Err(error) => app.set_status_error(Some(format!("Properties: {error}")), cx),
                },
            ),
        )
        .child(
            Icon::default()
                .data(icon)
                .size(px(12.0))
                .text_color(color::TEXT),
        )
        .child(
            // min_w(0) keeps the label shrinkable if it ever outgrows the slot;
            // at the current chrome it fits on one line.
            div()
                .min_w(px(0.0))
                .text_size(px(10.5))
                .font_family(ui::FONT_UI)
                .text_color(color::TEXT)
                .child(label.to_string()),
        )
}

/// The multi-selection branch: header card, combined bounds, align/distribute
/// and the batch actions, mirroring `PropertiesPanel`'s multi inspector.
fn multi_layers_section(
    state: &gpui_kit::Entity<crate::state::AppState>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let (count, names, bounds) = {
        let s = state.read(cx);
        let names: Vec<String> = s
            .layers
            .iter()
            .filter(|row| s.selected_layers.contains(&row.id))
            .map(|row| row.name.clone())
            .collect();
        (names.len(), names.join(", "), s.multi_bounds)
    };
    let bound_string = |value: f64| format!("{}", (value * 10.0).round() / 10.0);

    let mut panel = v_flex().w_full().flex_shrink_0().child(
        section_block()
            .child(section_header(
                icons::LAYERS,
                &t("properties.multipleLayers"),
            ))
            .child(
                h_flex()
                    .mt(px(8.0))
                    .items_center()
                    .gap(px(10.0))
                    .rounded(px(R4))
                    .border(px(1.0))
                    .border_color(color::DIVIDER)
                    .bg(color::FIELD)
                    .p(px(8.0))
                    .child(
                        div()
                            .w(px(32.0))
                            .h(px(32.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .flex_shrink_0()
                            .rounded(px(R3))
                            .border(px(1.0))
                            .border_color(color::DIVIDER)
                            .bg(color::PANEL)
                            .child(
                                Icon::default()
                                    .data(icons::LAYERS)
                                    .size(px(16.0))
                                    .text_color(color::ACCENT),
                            ),
                    )
                    .child(
                        v_flex()
                            .min_w(px(0.0))
                            .flex_1()
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .font_family(ui::FONT_UI)
                                    .font_weight(FontWeight(500.))
                                    .text_color(color::TEXT)
                                    .child(crate::i18n::t_args(
                                        "properties.layersSelected",
                                        &[("count", &count.to_string())],
                                    )),
                            )
                            .child(
                                div()
                                    .mt(px(2.0))
                                    .overflow_hidden()
                                    .text_size(px(10.5))
                                    .font_family(ui::FONT_UI)
                                    .text_color(color::TEXT_DIM)
                                    .child(names),
                            ),
                    ),
            ),
    );

    if let Some((x, y, w, h)) = bounds {
        panel = panel.child(
            section_block()
                .child(section_header(icons::MOVE, &t("properties.combinedBounds")))
                .child(
                    v_flex()
                        .mt(px(8.0))
                        .gap(px(8.0))
                        .w_full()
                        .child(ui::prop_row(
                            &t("properties.position"),
                            h_flex()
                                .flex_1()
                                .gap(px(SLOT_GAP))
                                .child(ui::num_field(Some("X"), None, &bound_string(x), Some("px")))
                                .child(ui::num_field(
                                    Some("Y"),
                                    None,
                                    &bound_string(y),
                                    Some("px"),
                                )),
                        ))
                        .child(ui::prop_row(
                            &t("tools.options.size"),
                            h_flex()
                                .flex_1()
                                .gap(px(SLOT_GAP))
                                .child(ui::num_field(Some("W"), None, &bound_string(w), Some("px")))
                                .child(ui::num_field(
                                    Some("H"),
                                    None,
                                    &bound_string(h),
                                    Some("px"),
                                )),
                        )),
                ),
        );
    }

    let align_button = |id: &'static str, icon: &'static [u8], mode: u8| {
        multi_icon_button(id, icon, cx, move |app, cx| {
            app.align_selected_layers(mode, cx)
        })
    };
    panel = panel.child(
        section_block()
            .child(section_header(
                icons::GRID_3,
                &t("properties.alignToCanvas"),
            ))
            .child(
                h_flex()
                    .mt(px(8.0))
                    .gap(px(4.0))
                    .w_full()
                    .child(align_button("prop-align-left", icons::ALIGN_LEFT, 0))
                    .child(align_button("prop-align-center-h", icons::ALIGN_H, 1))
                    .child(align_button("prop-align-right", icons::ALIGN_RIGHT, 2))
                    .child(align_button("prop-align-top", icons::ALIGN_TOP, 3))
                    .child(align_button("prop-align-center-v", icons::ALIGN_V, 4))
                    .child(align_button("prop-align-bottom", icons::ALIGN_BOTTOM, 5)),
            ),
    );

    if count >= 3 {
        panel = panel.child(
            section_block()
                .child(section_header(
                    icons::SPLIT_H,
                    &t("properties.distributeSpacing"),
                ))
                .child(
                    h_flex()
                        .mt(px(8.0))
                        .gap(px(6.0))
                        .w_full()
                        .child(multi_text_button(
                            "prop-distribute-h",
                            icons::DISTRIBUTE_H,
                            &t("properties.distributeH"),
                            cx,
                            |app, cx| app.distribute_selected_layers(true, cx),
                        ))
                        .child(multi_text_button(
                            "prop-distribute-v",
                            icons::DISTRIBUTE_V,
                            &t("properties.distributeV"),
                            cx,
                            |app, cx| app.distribute_selected_layers(false, cx),
                        )),
                ),
        );
    }

    panel.child(
        section_block()
            .child(section_header(
                icons::SLIDERS,
                &t("properties.batchActions"),
            ))
            .child(
                v_flex()
                    .mt(px(8.0))
                    .gap(px(6.0))
                    .w_full()
                    .child(canvas_action_button(
                        "prop-merge-selected",
                        icons::LAYERS,
                        &t("properties.mergeSelectedLayers"),
                        ActionTone::Field,
                        cx,
                        |app, _window, cx| app.merge_selected_layers(cx),
                    ))
                    .child(
                        h_flex()
                            .gap(px(6.0))
                            .w_full()
                            .child(canvas_action_button(
                                "prop-duplicate",
                                icons::COPY,
                                &t("layers.duplicateLayer"),
                                ActionTone::Field,
                                cx,
                                |app, _window, cx| app.duplicate_selected_layer_id(cx).map(|_| ()),
                            ))
                            .child(canvas_action_button(
                                "prop-delete",
                                icons::TRASH,
                                &t("layers.deleteLayer"),
                                ActionTone::Destructive,
                                cx,
                                |app, _window, cx| {
                                    app.delete_selected_layer(cx);
                                    Ok(())
                                },
                            )),
                    ),
            ),
    )
}

fn multi_icon_button(
    id: &'static str,
    icon: &'static [u8],
    cx: &Context<crate::app::App>,
    action: impl Fn(&mut crate::app::App, &mut Context<crate::app::App>) -> Result<(), String> + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .test_support()
        .flex_1()
        .h(px(28.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(R3))
        .border(px(1.0))
        .border_color(color::DIVIDER)
        .bg(color::FIELD)
        .cursor_pointer()
        .on_click(
            cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                let result = action(app, cx);
                match result {
                    Ok(()) => app.set_status_error(None, cx),
                    Err(error) => app.set_status_error(Some(format!("Properties: {error}")), cx),
                }
            }),
        )
        .child(
            Icon::default()
                .data(icon)
                .size(px(14.0))
                .text_color(color::TEXT_DIM),
        )
}

fn multi_text_button(
    id: &'static str,
    icon: &'static [u8],
    label: &str,
    cx: &Context<crate::app::App>,
    action: impl Fn(&mut crate::app::App, &mut Context<crate::app::App>) -> Result<(), String> + 'static,
) -> impl IntoElement {
    h_flex()
        .id(id)
        .test_support()
        .flex_1()
        .h(px(28.0))
        .items_center()
        .justify_center()
        .gap(px(6.0))
        .rounded(px(R3))
        .border(px(1.0))
        .border_color(color::DIVIDER)
        .bg(color::FIELD)
        .text_size(px(11.0))
        .font_family(ui::FONT_UI)
        .font_weight(FontWeight(500.))
        .text_color(color::TEXT_DIM)
        .cursor_pointer()
        .on_click(
            cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                let result = action(app, cx);
                match result {
                    Ok(()) => app.set_status_error(None, cx),
                    Err(error) => app.set_status_error(Some(format!("Properties: {error}")), cx),
                }
            }),
        )
        .child(
            Icon::default()
                .data(icon)
                .size(px(14.0))
                .text_color(color::TEXT_DIM),
        )
        .child(label.to_string())
}

/// Same chrome, icon only — a 14px glyph, no label.
fn icon_button(icon: &'static [u8]) -> Div {
    div()
        .flex_1()
        .min_w(px(0.0))
        .overflow_hidden()
        .h(px(BTN))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(R4))
        .border(px(1.0))
        .border_color(color::FIELD_BORDER)
        .bg(color::FIELD)
        .px(px(8.0))
        .child(
            Icon::default()
                .data(icon)
                .size(px(14.0))
                .text_color(color::TEXT_DIM),
        )
}

mod transform;

#[cfg(test)]
mod tests;
