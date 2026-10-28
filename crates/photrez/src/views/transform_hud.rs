//! The frozen `TransformHud`: a small dark readout floating up-right of the
//! pointer during a Move/resize/rotate drag, or during the Alt+Right brush
//! size/hardness adjustment.
//!
//! Geometry: `left = max(12, x + 16)`, `top = max(50, y - 4)` with the box
//! translated up by its own height, border `neutral-700/80`, fill `#18181B`,
//! radius 6, padding 10x6, 11px text.

use crate::components::ui;
use crate::state::{HudInfo, HudMode};
use crate::tokens::color;

use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, Div, FontWeight, Hsla, TestSupportExt};

/// `#18181B` — the frozen HUD fill (a raw neutral, not an editor token).
const HUD_BG: Hsla = Hsla {
    h: 0.6667,
    s: 0.0588,
    l: 0.1,
    a: 1.0,
};

/// One-row height for the rotate HUD; two rows for every other mode.
const ROW_H: f32 = 17.0;
const PAD_Y: f32 = 6.0;

pub(crate) fn render(hud: HudInfo, origin: (f32, f32)) -> impl IntoElement {
    let height = match hud.mode {
        HudMode::Rotate => ROW_H + PAD_Y * 2.0,
        _ => ROW_H * 2.0 + PAD_Y * 2.0,
    };
    let left = (hud.x - origin.0 + 16.0).max(12.0);
    let top = (hud.y - origin.1 - 4.0 - height).max(50.0);
    let box_ = v_flex()
        .id("transform-hud")
        .test_support()
        .absolute()
        .left(px(left))
        .top(px(top))
        .flex_none()
        .rounded(px(6.0))
        .border(px(1.0))
        .border_color(color::BORDER)
        .bg(HUD_BG)
        .px(px(10.0))
        .py(px(PAD_Y))
        .text_size(px(11.0))
        .font_family(ui::FONT_UI)
        .text_color(color::TEXT);

    match hud.mode {
        HudMode::Move => box_
            .child(row("X", signed(hud.a), "px", hud.snap))
            .child(row("Y", signed(hud.b), "px", hud.snap)),
        HudMode::Resize => box_
            .child(row("W", format!("{}", hud.a.round()), "px", hud.snap))
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(row("H", format!("{}", hud.b.round()), "px", hud.snap))
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(color::TEXT_DIM)
                            .child(format!("{}%", hud.scale_percent.round())),
                    ),
            ),
        HudMode::Rotate => box_.child(row("R", format!("{:.1}\u{b0}", hud.a), "", hud.snap)),
        HudMode::Brush => box_
            .child(row("Size", format!("{}", hud.a.round()), "px", false))
            .child(row("Hard", format!("{}", hud.b.round()), "%", false)),
    }
}

fn row(label: &'static str, value: String, suffix: &'static str, snap: bool) -> Div {
    let mut row = h_flex()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .child(
            h_flex()
                .items_center()
                .gap(px(6.0))
                .child(
                    div()
                        .w(px(12.0))
                        .text_size(px(10.0))
                        .font_weight(FontWeight(600.))
                        .text_color(color::TEXT_DIM)
                        .child(label),
                )
                .child(
                    div()
                        .font_weight(FontWeight(500.))
                        .text_color(color::TEXT)
                        .child(value),
                )
                .when(!suffix.is_empty(), |el| {
                    el.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(color::TEXT_DIM)
                            .child(suffix),
                    )
                }),
        );
    if snap {
        row = row.child(
            div()
                .rounded(px(3.0))
                .border(px(1.0))
                .border_color(color::ACCENT)
                .px(px(4.0))
                .text_size(px(9.5))
                .font_weight(FontWeight(500.))
                .text_color(color::ACCENT)
                .child("SNAP".to_string()), // i18n-exempt: technical HUD badge
        );
    }
    row
}

fn signed(value: f64) -> String {
    let rounded = value.round();
    if rounded >= 0.0 {
        format!("+{rounded}")
    } else {
        format!("{rounded}")
    }
}
