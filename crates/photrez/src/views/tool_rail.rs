//! Native port of `apps/desktop/src/components/editor/shell/LeftToolRail.tsx`.
//!
//! Ported from the frozen appearance reference
//! `reference/gpui-appearance-reference/src/tool_rail.rs` with the
//! hardcoded resting state replaced by a per-tool `active_tool` input, so the
//! per-tool `option-*` shots highlight the correct button.
//!
//! # Omitted (interaction only - cannot affect a static screenshot)
//!
//! * Tooltips (`placement="right"`, `shortcut`)
//! * the selection-variant fly-out popover (right-click)
//! * `disabled` mode (`opacity-50 pointer-events-none grayscale`)
//! * `transition-all duration-100` / `duration-150` - GPUI has no declarative
//!   per-element transition
//!
//! # Transcription notes
//!
//! * `z-10` / `z-20` need no equivalent: GPUI paints siblings in tree order,
//!   and the JSX already lists bg-swatch -> fg-swatch -> swap button, so the
//!   stacking comes out identical.
//! * `mt-auto` on the divider pins the swatch stack to the bottom.
//!
//! # `ToolId -> icon/active` mapping
//!
//! The reference keyed every button by a string `tool.id` and compared it
//! against a module-level active tool; the port instead carries the typed
//! `ToolId` on the `Tool` struct and compares it against the `active_tool`
//! argument so the correct button is lit per shot.

use gpui_kit::base::v_flex;
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::{div, hsla, px, BoxShadow, Hsla};

use crate::assets::icons;
use crate::state::ToolId;
use crate::tokens::{color, dim};

/// Mirrors `ToolItem` in `editorData.ts`. `has_variants` drives the
/// bottom-right micro-indicator; only `selection` has variants there.
struct Tool {
    /// `ToolId` - the typed tool the button represents. Compared against
    /// `active_tool` in `tool_button`.
    tool_id: ToolId,
    /// `id` - the web `ToolId` string, reused as the GPUI `ElementId` so
    /// buttons are hit-testable (gpui only hit-tests an element that has an
    /// id).
    id: &'static str,
    icon: &'static [u8],
    has_variants: bool,
}

/// `TOOL_ITEMS` from `editorData.ts`, in rail order.
const TOOLS: &[Tool] = &[
    Tool {
        tool_id: ToolId::Move,
        id: "move",
        icon: icons::CURSOR,
        has_variants: false,
    },
    Tool {
        tool_id: ToolId::Selection,
        id: "selection",
        icon: icons::SQUARE_DASHED,
        has_variants: true,
    },
    Tool {
        tool_id: ToolId::Crop,
        id: "crop",
        icon: icons::CROP,
        has_variants: false,
    },
    Tool {
        tool_id: ToolId::Eyedropper,
        id: "eyedropper",
        icon: icons::PIPETTE,
        has_variants: false,
    },
    Tool {
        tool_id: ToolId::Brush,
        id: "brush",
        icon: icons::BRUSH,
        has_variants: false,
    },
    Tool {
        tool_id: ToolId::Eraser,
        id: "eraser",
        icon: icons::ERASER,
        has_variants: false,
    },
    Tool {
        tool_id: ToolId::PaintBucket,
        id: "paintBucket",
        icon: icons::PAINT_BUCKET,
        has_variants: false,
    },
    Tool {
        tool_id: ToolId::Gradient,
        id: "gradient",
        icon: icons::SWATCH,
        has_variants: false,
    },
    Tool {
        tool_id: ToolId::Shape,
        id: "shape",
        icon: icons::RECTANGLE,
        has_variants: false,
    },
    Tool {
        tool_id: ToolId::Text,
        id: "text",
        icon: icons::TYPE,
        has_variants: false,
    },
];

/// `editorState.ts:37` - `createSignal("#E15A17")`.
/// sRGB (225, 90, 23) -> HSL (19.9 deg, 81.45%, 48.63%).
const FG_COLOR: Hsla = Hsla {
    h: 0.0553,
    s: 0.8145,
    l: 0.4863,
    a: 1.0,
};

/// `editorState.ts:38` - `createSignal("#FFFFFF")`.
const BG_COLOR: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 1.0,
    a: 1.0,
};

/// The sub-variant triangle, from the inline `<svg>` in the JSX. It is not a
/// lucide icon - it's a raw `<polygon points="6,6 0,6 6,0">` on a 6x6 viewBox.
/// `currentColor` becomes `#ffffff` because GPUI tints by alpha (see icons.rs).
const VARIANT_INDICATOR: &[u8] =
    b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"5\" height=\"5\" viewBox=\"0 0 6 6\" fill=\"#ffffff\"><polygon points=\"6,6 0,6 6,0\" /></svg>";

/// The two `shadow-md` layers GPUI generates, spelled out so they can be
/// composed with the outline ring below. These are byte-for-byte Tailwind v4's
/// `--shadow-md`, which is why gpui's `.shadow_md()` is safe to use elsewhere.
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

/// `<aside class="flex w-[52px] shrink-0 flex-col items-center gap-0.5
///  bg-editor-toolbar py-2">`
///
/// `active_tool` is the tool the shell currently has selected; the matching
/// button is highlighted. No arguments otherwise: the reference's hardcoded
/// resting state has been replaced by this parameter.
pub fn render(active_tool: &ToolId) -> impl IntoElement {
    // `flex_none()` so the rail keeps its 52px at any window width; the port's
    // shells were all captured at 1366 and a shrunken rail misplaces every
    // button by a different amount.
    v_flex()
        .w(px(dim::RAIL_W))
        .h_full()
        .flex_none()
        .items_center()
        .gap(px(dim::RAIL_GAP))
        .bg(color::TOOLBAR)
        .py(px(dim::RAIL_PY))
        .children(TOOLS.iter().map(|t| tool_button(t, active_tool)))
        .child(divider())
        .child(swatch_stack())
}

/// A tool row: a 36px rounded square centring an 18px icon, plus the optional
/// sub-variant triangle.
///
/// The original wraps the button in `<div class="relative">` purely to anchor
/// the fly-out popover, which this port omits - so the `relative` moves onto
/// the button itself, where it is needed for the absolute triangle.
fn tool_button(tool: &Tool, active_tool: &ToolId) -> impl IntoElement {
    // `Icon` captures its tint when the element tree is built, *before* hover
    // styles are applied at paint time, so the tint has to be decided here
    // rather than inherited from the button's `.hover(|s| s.text_color(..))`.
    // Consequence: the icon does not brighten on hover, only the button
    // background does. See KNOWN-FIDELITY-GAPS.
    let active = tool.tool_id == *active_tool;
    let icon_tint = if active { color::TEXT } else { color::ICON };

    div()
        .id(tool.id)
        .relative()
        .size(px(dim::TOOL_BTN))
        .flex_shrink_0()
        .cursor_pointer()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(dim::TOOL_BTN_RADIUS))
        .when(active, |el| el.bg(color::WHITE_5).text_color(color::TEXT))
        .when(!active, |el| {
            el.text_color(color::ICON)
                .hover(|s| s.bg(color::WHITE_5).text_color(color::TEXT))
        })
        .child(
            Icon::default()
                .data(tool.icon)
                .size(px(dim::ICON))
                .text_color(icon_tint),
        )
        .when(tool.has_variants, |el| el.child(variant_indicator()))
}

/// `absolute bottom-1 right-1 size-[5px] text-editor-text-dim/80`
fn variant_indicator() -> impl IntoElement {
    Icon::default()
        .data(VARIANT_INDICATOR)
        .absolute()
        .right(px(dim::TRIANGLE_INSET))
        .bottom(px(dim::TRIANGLE_INSET))
        .size(px(dim::TRIANGLE))
        .text_color(color::TEXT_DIM_80)
}

/// `<div class="mb-1 mt-auto h-px w-6 shrink-0 bg-editor-divider" />`
///
/// `mt-auto` is load-bearing: it is the only thing pushing the divider and the
/// swatch stack to the bottom of the rail.
fn divider() -> impl IntoElement {
    div()
        .mt_auto()
        .mb(px(dim::DIVIDER_MB))
        .h(px(dim::DIVIDER_H))
        .w(px(dim::DIVIDER_W))
        .flex_shrink_0()
        .bg(color::DIVIDER)
}

/// `<div class="relative size-[38px] shrink-0 group my-2 select-none">`
///
/// `.group("swatch")` is the direct equivalent of Tailwind's `group`, and the
/// swap button below consumes it with `.group_hover("swatch", ..)` - i.e. the
/// same parent-hover relationship, with no state on this view.
fn swatch_stack() -> impl IntoElement {
    div()
        .id("swatch-stack")
        .group("swatch")
        .relative()
        .size(px(dim::SWATCH_BOX))
        .flex_shrink_0()
        .my(px(dim::SWATCH_MY))
        .child(background_swatch())
        .child(foreground_swatch())
        .child(swap_button())
}

/// `absolute bottom-0 right-0 size-[28px] rounded-full border border-white/20
///  shadow-md hover:scale-105`
///
/// Rendered first so the foreground swatch overlaps it - which is what the
/// original achieves with `z-10` on the foreground swatch.
fn background_swatch() -> impl IntoElement {
    div()
        .absolute()
        .bottom(px(0.))
        .right(px(0.))
        .size(px(dim::SWATCH))
        .rounded_full()
        .border_1()
        // `border-white/20` does not survive: the unlayered `*` sets
        // `border-color` for every element, so this paints #333333 like every
        // other rule. See `color::BORDER` - and note this is why the rail's
        // raw diff is 2.377% while its hard count is near zero: `classify()`
        // buckets #333333 and #484848 as the same `active` class.
        .border_color(color::BORDER)
        .bg(BG_COLOR)
        .shadow_md()
}

/// `absolute top-0 left-0 size-[28px] rounded-full border border-white/30
///  outline outline-1 outline-black/40 shadow-md z-10 hover:scale-105`
///
/// ## The one real fidelity gap in this component
///
/// GPUI's `Styled` has no `outline-*`. CSS `outline` is drawn *outside* the
/// border box and does not consume layout, whereas a second `.border_1()`
/// would shrink the 28px content box. A 1px-spread, 0-blur `BoxShadow`
/// reproduces an outline's geometry exactly (outside the border, no layout
/// effect), so the `outline` is composited into the same `.shadow()` call as
/// `shadow-md` rather than using the `.shadow_md()` shorthand.
fn foreground_swatch() -> impl IntoElement {
    let mut shadows = vec![
        // `outline outline-1 outline-black/40`
        BoxShadow::new(px(0.), px(0.), color::BLACK_40).spread_radius(px(1.)),
    ];
    shadows.extend(shadow_md_layers());

    div()
        .absolute()
        .top(px(0.))
        .left(px(0.))
        .size(px(dim::SWATCH))
        .rounded_full()
        .border_1()
        .border_color(color::BORDER)
        .bg(FG_COLOR)
        .shadow(shadows)
}

/// `absolute -top-1.5 -right-1.5 z-20 size-4 bg-editor-toolbar
///  border border-editor-divider rounded-full ... scale-0 group-hover:scale-100
///  shadow hover:text-editor-text`
///
/// `scale-0` / `group-hover:scale-100` is expressed as `opacity(0.)` /
/// `group_hover(.., opacity(1.))`: `Styled` exposes no 2-D transform for `Div`
/// (only `Svg` has `with_transformation`). For a screenshot the two are
/// indistinguishable - invisible at rest, fully visible on hover. The missing
/// piece is only the 150ms scale-*animation*.
///
/// Note this icon keeps `stroke-width` 2: line 282 of the JSX passes no
/// `strokeWidth`, unlike the tool buttons.
fn swap_button() -> impl IntoElement {
    div()
        .id("swap-colors")
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
        .hover(|s| s.text_color(color::TEXT))
        .child(
            // LeftToolRail.tsx:282 renders `<Icon name="rotate" />` with no
            // `strokeWidth`, i.e. lucide's default 2. `rotate` and `rotate-cw`
            // both map to RotateCw - the same glyph - but the `rotate-cw` key
            // is drawn at 1.5 by the option bars, so it is a different
            // constant and not the one this micro-button uses.
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

    /// Pin the `ToolId -> (id, icon, has_variants)` table.
    ///
    /// RED proof: swap any `tool_id` arm (e.g. `Crop` -> `Brush`) and this
    /// fails. GREEN: every variant maps to its web id + its rail icon, in
    /// rail order, and only `Selection` carries the sub-variant indicator.
    #[test]
    fn tool_table_maps_each_toolid_its_web_id_and_icon() {
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
