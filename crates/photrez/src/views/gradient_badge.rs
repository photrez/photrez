//! `GradientOverlay.tsx`'s floating DIST/ANGLE card while a gradient drag is
//! live. Geometry: `left = max(12, endX + 16)`, `top = max(50, endY - 14)`
//! translated up by the card height, border `neutral-700/80`, fill `#18181B`,
//! radius 6, padding 10x6, a 2 px row gap and 11px text.

use crate::state::GradientBadge;

use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, BoxShadow, FontWeight, Hsla, TestSupportExt};

const CARD_BG: Hsla = Hsla {
    h: 0.6667,
    s: 0.0588,
    l: 0.1,
    a: 1.0,
};
const CARD_BORDER: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.2510,
    a: 0.8,
};
const CARD_TEXT: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.9608,
    a: 1.0,
};
const CARD_DIM: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.6392,
    a: 1.0,
};

const ROW_H: f32 = 17.0;
const ROW_GAP: f32 = 2.0;
const PAD_Y: f32 = 6.0;
const CARD_H: f32 = ROW_H * 2.0 + ROW_GAP + PAD_Y * 2.0;
const LABEL_W: f32 = 28.0;
const SHADOW: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.0,
    a: 0.4,
};

pub(crate) fn render(badge: GradientBadge, origin: (f32, f32)) -> impl IntoElement {
    let left = (badge.x - origin.0 + 16.0).max(12.0);
    let top = (badge.y - origin.1 - 14.0 - CARD_H).max(50.0);
    let row = |label: &'static str, value: String, unit: &'static str| {
        h_flex()
            .items_center()
            .gap(px(6.0))
            .h(px(ROW_H))
            .child(
                div()
                    .w(px(LABEL_W))
                    .text_size(px(10.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(CARD_DIM)
                    .child(label),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(CARD_TEXT)
                    .child(value),
            )
            .child(div().text_size(px(10.0)).text_color(CARD_DIM).child(unit))
    };
    v_flex()
        .id("gradient-badge")
        .test_support()
        .absolute()
        .left(px(left))
        .top(px(top))
        .flex_none()
        .gap(px(ROW_GAP))
        .rounded(px(6.0))
        .border(px(1.0))
        .border_color(CARD_BORDER)
        .bg(CARD_BG)
        .px(px(10.0))
        .py(px(PAD_Y))
        .shadow(vec![BoxShadow::new(px(0.0), px(4.0), SHADOW)])
        .child(row("DIST", format!("{}", badge.distance), "px"))
        .child(row("ANGLE", format!("{:.1}", badge.angle), "\u{b0}"))
}
