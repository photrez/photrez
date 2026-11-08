//! The left tool rail.
//!
//! The rail draws the per-tool active state from a per-tool `active_tool`
//! input rather than a hardcoded resting state, so the per-tool `option-*`
//! shots highlight the correct button.
//!
//! # Omitted (interaction only - cannot affect a static screenshot)
//!
//! * Tooltips (`placement="right"`, `shortcut`)
//! * the selection-variant fly-out popover (right-click)
//! * the disabled mode - half opacity, no pointer response, desaturated
//! * the 100ms and 150ms transitions - GPUI has no declarative
//!   per-element transition
//!
//! # Transcription notes
//!
//! * The recorded stack levels 10 and 20 need no equivalent: GPUI paints
//!   siblings in tree order, and the element order already lists bg-swatch ->
//!   fg-swatch -> swap button, so the stacking comes out identical.
//! * The divider's auto top margin pins the swatch stack to the bottom.
//!
//! # `ToolId -> icon/active` mapping
//!
//! The button-to-icon/active mapping keys every button by the typed `ToolId`
//! on the `Tool` struct and compares it against the `active_tool` argument,
//! so the correct button is lit per shot.

use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::{div, hsla, px, BoxShadow, Context, Entity, Hsla, MouseButton, TestSupportExt};

use crate::icons;
use crate::state::{AppState, SelectionShape, ToolId};
use crate::tokens::{color, dim};

/// One rail entry: a button plus its optional variant indicator. `has_variants`
/// drives the bottom-right micro-indicator; only `selection` has variants.
struct Tool {
    /// `ToolId` - the typed tool the button represents. Compared against
    /// `active_tool` in `tool_button`.
    tool_id: ToolId,
    /// `id` - the tool's id string, reused as the GPUI `ElementId` so
    /// buttons are hit-testable (gpui only hit-tests an element that has an
    /// id).
    id: &'static str,
    /// The tooltip label key, already in the catalog.
    label_key: &'static str,
    /// The shortcut letter the tooltip shows, matching the rail's own key bindings.
    shortcut: &'static str,
    icon: &'static [u8],
    has_variants: bool,
}

/// The rail entry's position in the column, for the tooltip overlay's origin.
pub(crate) fn tool_index(id: &str) -> Option<usize> {
    TOOLS.iter().position(|tool| tool.id == id)
}

/// The tooltip label key and shortcut letter a rail entry publishes.
pub(crate) fn tool_tooltip(id: &str) -> Option<(&'static str, &'static str)> {
    TOOLS
        .iter()
        .find(|tool| tool.id == id)
        .map(|tool| (tool.label_key, tool.shortcut))
}

/// The tool list, in the order the rail paints it, top to bottom.
const TOOLS: &[Tool] = &[
    Tool {
        tool_id: ToolId::Move,
        id: "move",
        label_key: "tools.move",
        shortcut: "V",
        icon: icons::CURSOR,
        has_variants: false,
    },
    Tool {
        tool_id: ToolId::Selection,
        id: "selection",
        label_key: "tools.select",
        shortcut: "M",
        icon: icons::SQUARE_DASHED,
        has_variants: true,
    },
    Tool {
        tool_id: ToolId::Crop,
        id: "crop",
        label_key: "tools.crop",
        shortcut: "C",
        icon: icons::CROP,
        has_variants: false,
    },
    Tool {
        tool_id: ToolId::Eyedropper,
        id: "eyedropper",
        label_key: "tools.eyedropper",
        shortcut: "I",
        icon: icons::PIPETTE,
        has_variants: false,
    },
    Tool {
        tool_id: ToolId::Brush,
        id: "brush",
        label_key: "tools.brush",
        shortcut: "B",
        icon: icons::BRUSH,
        has_variants: false,
    },
    Tool {
        tool_id: ToolId::Eraser,
        id: "eraser",
        label_key: "tools.eraser",
        shortcut: "E",
        icon: icons::ERASER,
        has_variants: false,
    },
    Tool {
        tool_id: ToolId::PaintBucket,
        id: "paintBucket",
        label_key: "tools.paintBucket",
        shortcut: "G",
        icon: icons::PAINT_BUCKET,
        has_variants: false,
    },
    Tool {
        tool_id: ToolId::Gradient,
        id: "gradient",
        label_key: "tools.gradient",
        shortcut: "Shift+G",
        icon: icons::SWATCH,
        has_variants: false,
    },
    Tool {
        tool_id: ToolId::Shape,
        id: "shape",
        label_key: "tools.shape",
        shortcut: "U",
        icon: icons::RECTANGLE,
        has_variants: false,
    },
    Tool {
        tool_id: ToolId::Text,
        id: "text",
        label_key: "tools.text",
        shortcut: "T",
        icon: icons::TYPE,
        has_variants: false,
    },
];

// The starting defaults (`#E15A17` foreground, `#FFFFFF`
// background). `AppState::default` carries the same pair and is the live
// source the swatches paint from, so no constant is needed here.

/// The sub-variant triangle, drawn as inline `<svg>` -
/// trace-exempt: that markup is the asset itself, copied verbatim into the const
/// below. It is not a lucide icon but a raw `<polygon points="6,6 0,6 6,0">` on
/// a 6x6 viewBox. `currentColor` becomes `#ffffff` - GPUI tints by alpha, see
/// icons.rs.
const VARIANT_INDICATOR: &[u8] =
    b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"5\" height=\"5\" viewBox=\"0 0 6 6\" fill=\"#ffffff\"><polygon points=\"6,6 0,6 6,0\" /></svg>";

/// The two medium-shadow layers GPUI generates, spelled out so they can be
/// composed with the outline ring below. They are byte-for-byte the upstream
/// theme's medium-shadow preset, which is why gpui's `.shadow_md()` is safe to
/// use elsewhere.
fn shadow_md_layers() -> Vec<BoxShadow> {
    vec![
        BoxShadow::new(px(0.), px(4.), hsla(0., 0., 0., 0.1))
            .blur_radius(px(6.))
            .spread_radius(px(-1.)),
        BoxShadow::new(px(0.), px(2.), hsla(0., 0., 0., 0.1))
            .blur_radius(px(4.))
            .spread_radius(px(-2.)),
    ]
}

/// The rail is a 52px-wide flex column with centred items, a 2px gap between
/// rows and 8px of vertical padding.
///
/// `state` is the live app-state entity the rail writes its selection back
/// into; the active tool is read once here (it is `Copy`) and passed down so
/// each button can light itself. No arguments otherwise: the fixed resting
/// state this first shipped with is now read through this parameter.
pub fn render(state: &Entity<AppState>, cx: &Context<crate::app::App>) -> impl IntoElement {
    // `Copy` the fields we branch on; the borrow of the state ends here.
    let (active_tool, fg_color, bg_color, has_document) = {
        let s = state.read(cx);
        (s.active_tool, s.fg_color, s.bg_color, s.has_document)
    };
    // With no document open the rail paints at 50% and its buttons and
    // swatches lose their clicks, the treatment the Layers panel uses for its
    // own rows. Keyboard tool shortcuts stay live; only the pointer is gated.
    // `flex_none()` keeps the rail at its 52px at any window width; the shell
    // baselines were all captured at 1366 and a shrunken rail misplaces every
    // button by a different amount.
    let mut rail = v_flex()
        .w(px(dim::RAIL_W))
        .h_full()
        .flex_none()
        .items_center()
        .gap(px(dim::RAIL_GAP))
        .bg(color::TOOLBAR)
        .py(px(dim::RAIL_PY))
        .children(
            TOOLS
                .iter()
                .map(|t| tool_button(t, active_tool, has_document, state, cx)),
        )
        .child(divider())
        .child(swatch_stack(
            crate::palette::hsla(fg_color),
            crate::palette::hsla(bg_color),
            has_document,
            cx,
        ));
    if !has_document {
        rail = rail.opacity(0.5);
    }
    rail
}

/// The rail button's icon tint for its two visual states.
///
/// The active tool and a hovered inactive tool both use [`color::TEXT`], the
/// colour an icon takes on hover. An inactive resting button keeps
/// [`color::ICON`].
fn tool_icon_tint(active: bool, hovered: bool) -> Hsla {
    if active || hovered {
        color::TEXT
    } else {
        color::ICON
    }
}

/// A tool row: a 36px rounded square centring an 18px icon, plus the optional
/// sub-variant triangle.
///
/// No fly-out popover is mounted here, so the positioning that a wrapper div
/// would have carried moves onto the button itself, where it is needed for the
/// absolute triangle.
///
/// The click handler writes this button's `ToolId` back into the live
/// `AppState` and notifies the root so the shell repaints with the new tool.
fn tool_button(
    tool: &Tool,
    active_tool: ToolId,
    enabled: bool,
    state: &Entity<AppState>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    // `Icon` resolves its tint from its own `text_color` field or the theme
    // foreground, never from an inherited hover style. So the tint is decided
    // here, from the button's active and hover state held in `AppState`.
    let active = tool.tool_id == active_tool;
    let (selection_shape, hovered) = {
        let s = state.read(cx);
        (s.selection_shape, s.hovered_tool == Some(tool.tool_id))
    };
    let icon_tint = tool_icon_tint(active, hovered);
    let icon = if tool.tool_id == ToolId::Selection && selection_shape == SelectionShape::Ellipse {
        icons::CIRCLE_DASHED
    } else {
        tool.icon
    };

    let tool_id = tool.tool_id;
    let on_click = cx.listener(move |app, _, _, cx: &mut Context<crate::app::App>| {
        app.select_canvas_tool(tool_id, cx);
    });
    let state_for_variants = state.clone();
    let on_right_click = cx.listener(move |_, _, _, cx: &mut Context<crate::app::App>| {
        state_for_variants.update(cx, |s, _| s.selection_flyout_open = true);
        cx.notify();
    });
    let state_for_hover = state.clone();
    let hover_tool_id = tool.tool_id;
    let tooltip_id = format!("rail-tooltip-{}", tool.id);
    let on_hover = cx.listener(move |app, hovered: &bool, _window, cx| {
        let next = hovered.then_some(hover_tool_id);
        let changed = state_for_hover.update(cx, |s, _| {
            if s.hovered_tool == next {
                false
            } else {
                s.hovered_tool = next;
                true
            }
        });
        // The tooltip itself is painted by the shell's overlay, because GPUI
        // paints in tree order and a panel inside the rail is covered by the canvas
        // it overlaps.
        app.set_tooltip(hovered.then(|| tooltip_id.clone()), cx);
        if changed {
            cx.notify();
        }
    });

    let mut button = div()
        .id(tool.id)
        .test_support()
        .relative()
        .size(px(dim::TOOL_BTN))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(dim::TOOL_BTN_RADIUS))
        .on_hover(on_hover);
    if enabled {
        button = button
            .cursor_pointer()
            .on_click(on_click)
            .when(tool.has_variants, |el| {
                el.on_mouse_down(MouseButton::Right, on_right_click)
            });
    }
    button
        .when(active, |el| el.bg(color::WHITE_5).text_color(color::TEXT))
        .when(!active, |el| {
            el.text_color(color::ICON)
                .hover(|s| s.bg(color::WHITE_5).text_color(color::TEXT))
        })
        .child(
            Icon::default()
                .data(icon)
                .size(px(dim::ICON))
                .text_color(icon_tint),
        )
        .when(tool.has_variants, |el| el.child(variant_indicator()))
}

/// The selection tool's right-click sub-variant popover.
///
/// It is an overlay appended after the shell (GPUI has no stacking index): the
/// measured origin is the selection row's right edge, 50px from the rail's
/// left, 92px below the window top (46 title + 8 rail inset + one 36px row +
/// 2 gap). Its measured layout is a 140px minimum width, 12px horizontal and
/// 6px vertical padding, and 11px text.
pub fn render_flyout(
    state: &Entity<AppState>,
    active_shape: SelectionShape,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let state_for_close = state.clone();
    let close = cx.listener(move |_, _, _, cx: &mut Context<crate::app::App>| {
        state_for_close.update(cx, |s, _| s.selection_flyout_open = false);
        cx.notify();
    });

    let variants = [
        (
            SelectionShape::Rect,
            "variant-rect",
            icons::SQUARE_DASHED,
            crate::i18n::t("tools.rectSelect"),
        ),
        (
            SelectionShape::Ellipse,
            "variant-ellipse",
            icons::CIRCLE_DASHED,
            crate::i18n::t("tools.ellipseSelect"),
        ),
    ];
    let mut popover = v_flex()
        .id("selection-variant-flyout")
        .test_support()
        .absolute()
        .left(px(50.0))
        .top(px(92.0))
        .w(px(140.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(color::FIELD_BORDER)
        .bg(color::PANEL)
        .py(px(4.0))
        .overflow_hidden();
    for (shape, id, icon, label) in variants {
        let state = state.clone();
        let active = shape == active_shape;
        popover = popover.child(
            h_flex()
                .id(id)
                .test_support()
                .h(px(28.0))
                .items_center()
                .gap(px(8.0))
                .px(px(12.0))
                .text_size(px(11.0))
                .font_family(crate::components::ui::FONT_UI)
                .text_color(if active { color::TEXT } else { color::TEXT_DIM })
                .when(active, |el| el.bg(color::WHITE_10))
                .hover(|style| style.bg(color::WHITE_5).text_color(color::TEXT))
                .on_click(
                    cx.listener(move |_, _, _, cx: &mut Context<crate::app::App>| {
                        state.update(cx, |s, _| {
                            s.selection_shape = shape;
                            s.active_tool = ToolId::Selection;
                            s.selection_flyout_open = false;
                        });
                        cx.notify();
                    }),
                )
                .child(
                    Icon::default()
                        .data(icon)
                        .size(px(16.0))
                        .text_color(if active { color::TEXT } else { color::ICON }),
                )
                .child(label),
        );
    }

    div()
        .id("selection-flyout-frame")
        .test_support()
        .absolute()
        .inset(px(0.0))
        .on_click(close)
        .child(popover)
}

/// Pinned 4px from the bottom and right edges: a 5px triangle in the dim text
/// colour at 80% opacity.
fn variant_indicator() -> impl IntoElement {
    Icon::default()
        .data(VARIANT_INDICATOR)
        .absolute()
        .right(px(dim::TRIANGLE_INSET))
        .bottom(px(dim::TRIANGLE_INSET))
        .size(px(dim::TRIANGLE))
        .text_color(color::TEXT_DIM_80)
}

/// The divider: 1px tall, 24px wide, never squeezed, 4px of bottom margin, in
/// the divider colour.
///
/// Its auto top margin is load-bearing: it is the only thing pushing the divider
/// and the swatch stack to the bottom of the rail.
fn divider() -> impl IntoElement {
    div()
        .mt_auto()
        .mb(px(dim::DIVIDER_MB))
        .h(px(dim::DIVIDER_H))
        .w(px(dim::DIVIDER_W))
        .flex_shrink_0()
        .bg(color::DIVIDER)
}

/// The swatch box: a 38px positioned square, never squeezed, with 8px of
/// vertical margin; nothing in it is text-selectable.
///
/// `.group("swatch")` is the direct equivalent of the upstream hover group, and
/// the swap button below consumes it with `.group_hover("swatch", ..)` - i.e.
/// the same parent-hover relationship, with no state on this view.
fn swatch_stack(
    fg: Hsla,
    bg: Hsla,
    enabled: bool,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    div()
        .id("swatch-stack")
        .group("swatch")
        .relative()
        .size(px(dim::SWATCH_BOX))
        .flex_shrink_0()
        .my(px(dim::SWATCH_MY))
        .child(background_swatch(bg, enabled, cx))
        .child(foreground_swatch(fg, enabled, cx))
        .child(swap_button(enabled, cx))
}

/// The background swatch: a 28px circle flush to the box's bottom-right, stacked
/// below the foreground one, with a 1px border (white at 20% intended), a medium
/// shadow, and 5% larger on hover.
///
/// Rendered first so the foreground swatch overlaps it, by paint order.
fn background_swatch(fill: Hsla, enabled: bool, cx: &Context<crate::app::App>) -> impl IntoElement {
    let mut swatch = div()
        .id("bg-swatch")
        .test_support()
        .absolute()
        .bottom(px(0.))
        .right(px(0.))
        .size(px(dim::SWATCH))
        .rounded_full()
        .border_1()
        // The intended white-at-20% border does not survive: the global
        // border colour covers every element, so this paints
        // #333333 like every other border. See `color::BORDER` - and note this is
        // why the rail's raw diff is 2.377% while its hard count is near zero:
        // `classify()` buckets #333333 and #484848 as the same `active` class.
        .border_color(color::BORDER)
        .bg(fill)
        .shadow_md();
    if enabled {
        swatch = swatch.cursor_pointer().on_click(cx.listener(
            |app, _: &gpui_kit::ClickEvent, window, cx| {
                cx.stop_propagation();
                app.open_color_picker(crate::state::ColorTarget::Background, window, cx);
            },
        ));
    }
    swatch
}

/// The foreground swatch: a 28px circle flush to the box's top-left, stacked
/// above the rest, 1px border (white at 30%), 1px black-at-40% outer ring,
/// medium shadow, 5% larger on hover.
///
/// ## The one real fidelity gap in this component
///
/// GPUI's `Styled` has no outline property. A browser outline is drawn
/// *outside* the border box and does not consume layout, whereas a second
/// `.border_1()` would shrink the 28px content box. A 1px-spread, 0-blur
/// `BoxShadow` reproduces an outline's geometry exactly (outside the border,
/// no layout effect), so the ring is composited into the same `.shadow()` call
/// as the medium shadow layers rather than using the `.shadow_md()` shorthand.
fn foreground_swatch(fill: Hsla, enabled: bool, cx: &Context<crate::app::App>) -> impl IntoElement {
    let mut shadows = vec![
        // The 1px black-at-40% outer ring, the outline substitute
        BoxShadow::new(px(0.), px(0.), color::BLACK_40).spread_radius(px(1.)),
    ];
    shadows.extend(shadow_md_layers());

    let mut swatch = div()
        .id("fg-swatch")
        .test_support()
        .absolute()
        .top(px(0.))
        .left(px(0.))
        .size(px(dim::SWATCH))
        .rounded_full()
        .border_1()
        .border_color(color::BORDER)
        .bg(fill)
        .shadow(shadows)
        .on_hover(cx.listener(|app, hovered: &bool, _, cx| {
            app.set_tooltip(hovered.then(|| "rail-tooltip-fg".to_string()), cx);
        }));
    if enabled {
        swatch = swatch.cursor_pointer().on_click(cx.listener(
            |app, _: &gpui_kit::ClickEvent, window, cx| {
                cx.stop_propagation();
                app.open_color_picker(crate::state::ColorTarget::Foreground, window, cx);
            },
        ));
    }
    swatch
}

/// The swap button: a 16px circle 6px past the box's top-right, stacked on top,
/// in the toolbar colour with a 1px divider border and a small shadow, hidden at
/// rest and shown - in the plain text colour - on the swatch box's hover.
///
/// Growing from nothing on hover is expressed as `opacity(0.)` /
/// `group_hover(.., opacity(1.))`: `Styled` exposes no 2-D transform for `Div`
/// (only `Svg` has `with_transformation`). For a screenshot the two are
/// indistinguishable - invisible at rest, fully visible on hover. The missing
/// piece is only the 150ms scale-*animation*.
///
/// Note this icon keeps `stroke-width` 2, unlike the tool button glyphs.
fn swap_button(enabled: bool, cx: &Context<crate::app::App>) -> impl IntoElement {
    let mut button = div()
        .id("swap-colors")
        .test_support()
        .absolute()
        .top(px(dim::SWAP_OFFSET))
        .right(px(dim::SWAP_OFFSET))
        .size(px(dim::SWAP_BTN))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .border_1()
        .border_color(color::BORDER)
        .bg(color::TOOLBAR)
        .text_color(color::ICON)
        .shadow_sm()
        .opacity(0.)
        .group_hover("swatch", |s| s.opacity(1.))
        .hover(|s| s.text_color(color::TEXT));
    if enabled {
        button = button.cursor_pointer().on_click(cx.listener(
            |app, _: &gpui_kit::ClickEvent, _, cx| {
                cx.stop_propagation();
                app.swap_colors(cx);
            },
        ));
    }
    button.child(
        // This micro-button's rotate icon uses the default stroke width
        // (2). `icons::ROTATE` and `icons::ROTATE_CW` are the same glyph, but
        // `icons::ROTATE_CW` is drawn at 1.5 by the option bars, so it is a
        // different constant and not the one used here.
        Icon::default()
            .data(icons::ROTATE)
            .size(px(dim::SWAP_ICON))
            .text_color(color::ICON),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ToolId::*;

    /// Pin the tint decision for the three button states.
    ///
    /// RED proof: drop `|| hovered` from `tool_icon_tint` and the
    /// hovered-inactive assertion fails naming the lifted colour.
    #[test]
    fn tool_icon_tint_lifts_only_for_hovered_or_active() {
        assert_eq!(
            tool_icon_tint(false, false),
            color::ICON,
            "resting inactive keeps the dim icon colour"
        );
        assert_eq!(
            tool_icon_tint(false, true),
            color::TEXT,
            "hovered inactive lifts to the text colour"
        );
        assert_eq!(tool_icon_tint(true, false), color::TEXT, "active lifts");
        assert_eq!(
            tool_icon_tint(true, true),
            color::TEXT,
            "hover changes nothing for the active tool"
        );
    }

    /// Pin the `ToolId -> (id, icon, has_variants)` table.
    ///
    /// RED proof: swap any `tool_id` arm (e.g. `Crop` -> `Brush`) and this
    /// fails. GREEN: every variant maps to its id + its rail icon, in
    /// rail order, and only `Selection` carries the sub-variant indicator.
    #[test]
    fn tool_table_maps_each_toolid_its_id_and_icon() {
        assert_eq!(TOOLS.len(), 10);

        let expected: &[(&str, ToolId)] = &[
            ("move", Move),
            ("selection", Selection),
            ("crop", Crop),
            ("eyedropper", Eyedropper),
            ("brush", Brush),
            ("eraser", Eraser),
            ("paintBucket", PaintBucket),
            ("gradient", Gradient),
            ("shape", Shape),
            ("text", Text),
        ];
        for (tool, &exp) in TOOLS.iter().zip(expected.iter()) {
            assert_eq!(tool.id, exp.0, "id column must be in rail order");
            assert_eq!(
                tool.tool_id, exp.1,
                "`{}` must map to its own ToolId",
                exp.0
            );
        }

        // Only `selection` has variants - every other button must not render
        // the sub-variant triangle.
        assert_eq!(
            TOOLS.iter().filter(|t| t.has_variants).count(),
            1,
            "exactly one tool (selection) carries the sub-variant indicator"
        );
        assert!(TOOLS[1].has_variants, "selection is the variant tool");
        for (i, tool) in TOOLS.iter().enumerate() {
            if i != 1 {
                assert!(!tool.has_variants, "tool[{i}] must not have variants");
            }
        }
    }

    /// `active_tool` selects the highlighted button: the matching `ToolId` is
    /// lit, the rest are not.
    ///
    /// RED proof: make two rows share a `tool_id` and this fails - the render
    /// path consumes the same `ToolId` equality, so this table is the single
    /// source of truth for which button lights.
    #[test]
    fn active_tool_highlights_only_its_button() {
        let expect: [ToolId; 10] = [
            Move,
            Selection,
            Crop,
            Eyedropper,
            Brush,
            Eraser,
            PaintBucket,
            Gradient,
            Shape,
            Text,
        ];
        for active in &expect {
            let lit: Vec<&Tool> = TOOLS.iter().filter(|t| t.tool_id == *active).collect();
            assert_eq!(
                lit.len(),
                1,
                "exactly one button lights for active {active:?}"
            );
            assert_eq!(lit[0].tool_id, *active);
        }
    }
}
