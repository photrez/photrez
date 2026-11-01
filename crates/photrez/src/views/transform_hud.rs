//! The transform HUD: a small dark readout floating up-right of the
//! pointer during a Move/resize/rotate drag, or during the Alt+Right brush
//! size/hardness adjustment.
//!
//! Geometry: `left = max(12, x + 16)`, `top = max(50, y - 4)` with the box
//! translated up by its own height, border `neutral-700/80`, fill `#18181B`,
//! radius 6, padding 10x6, a 2 px row gap inside a `gap-0.5` column, 11px text.
//!
//! Row layout is per mode:
//!
//! ```text
//! move    X +12 px        (no badge)
//!         Y -34 px  SNAP
//! resize  W 640 px   100%       <- the scale reads at the W row's right
//!         H 480 px   SNAP
//! brush   Size 24 px      (28 px label column)
//!         Hard 100 %
//! rotate  R 45.0deg  SNAP       (single row, badge inline)
//! ```

use crate::components::ui;
use crate::state::{HudInfo, HudMode};

use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::{div, hsla, px, BoxShadow, Div, FontWeight, Hsla, TestSupportExt};

/// `#18181B` — the HUD fill (a raw neutral, not an editor token).
const HUD_BG: Hsla = Hsla {
    h: 0.6667,
    s: 0.0588,
    l: 0.1,
    a: 1.0,
};

/// `border-neutral-700/80`, `text-neutral-100`, `text-neutral-400`: the HUD
/// uses raw neutrals, not the editor tokens.
const HUD_BORDER: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.2510,
    a: 0.8,
};
const HUD_TEXT: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.9608,
    a: 1.0,
};
const HUD_DIM: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.6392,
    a: 1.0,
};

/// `border-emerald-700/60`, `bg-emerald-950/80`, `text-emerald-300`.
const SNAP_BORDER: Hsla = Hsla {
    h: 0.3956,
    s: 0.7183,
    l: 0.2922,
    a: 0.6,
};
const SNAP_FILL: Hsla = Hsla {
    h: 0.4603,
    s: 0.2009,
    l: 0.0902,
    a: 0.8,
};
const SNAP_TEXT: Hsla = Hsla {
    h: 0.4339,
    s: 0.7159,
    l: 0.6686,
    a: 1.0,
};

/// One-row height for the readout; the rotate mode is one row, every other
/// mode two.
const ROW_H: f32 = 17.0;
/// `gap-0.5` between the rows.
const ROW_GAP: f32 = 2.0;
const PAD_Y: f32 = 6.0;
/// `w-3` on the X/Y/W/H/R labels; `w-7` on the brush Size/Hard labels.
const LABEL_W: f32 = 12.0;
const LABEL_W_BRUSH: f32 = 28.0;

pub(crate) fn render(hud: HudInfo, origin: (f32, f32)) -> impl IntoElement {
    let rows = row_specs(hud);
    let row_count = rows.len() as f32;
    let height = row_count * ROW_H + (row_count - 1.0).max(0.0) * ROW_GAP + PAD_Y * 2.0;
    let left = (hud.x - origin.0 + 16.0).max(12.0);
    let top = (hud.y - origin.1 - 4.0 - height).max(50.0);
    v_flex()
        .id("transform-hud")
        .test_support()
        .absolute()
        .left(px(left))
        .top(px(top))
        .flex_none()
        .gap(px(ROW_GAP))
        .rounded(px(6.0))
        .border(px(1.0))
        .border_color(HUD_BORDER)
        .bg(HUD_BG)
        .shadow(vec![BoxShadow::new(
            px(0.0),
            px(4.0),
            hsla(0.0, 0.0, 0.0, 0.4),
        )
        .blur_radius(px(12.0))])
        .px(px(10.0))
        .py(px(PAD_Y))
        .text_size(px(11.0))
        .font_family(ui::FONT_UI)
        .text_color(HUD_TEXT)
        .children(rows.into_iter().map(row))
}

/// One row of the readout, resolved from the HUD state before the element
/// tree is built so the per-mode table is testable without a window.
#[derive(Debug, Clone, PartialEq)]
struct RowSpec {
    label: &'static str,
    value: String,
    suffix: &'static str,
    snap: bool,
    /// The dimmed right-hand readout (`scalePercent` on the resize W row).
    trailing: Option<String>,
    label_w: f32,
}

fn row_specs(hud: HudInfo) -> Vec<RowSpec> {
    // The HUD's axis/size labels are fixed, non-translated literals (X, Y,
    // W, H, R, px, %), verified against the baseline capture.
    match hud.mode {
        HudMode::Move => vec![
            RowSpec {
                label: "X", // i18n-exempt: source HUD token
                value: signed(hud.a),
                suffix: "px",
                snap: false,
                trailing: None,
                label_w: LABEL_W,
            },
            RowSpec {
                label: "Y", // i18n-exempt: source HUD token
                value: signed(hud.b),
                suffix: "px",
                snap: hud.snap,
                trailing: None,
                label_w: LABEL_W,
            },
        ],
        HudMode::Resize => vec![
            RowSpec {
                label: "W", // i18n-exempt: source HUD token
                value: format!("{}", hud.a.round()),
                suffix: "px",
                snap: false,
                trailing: Some(format!("{}%", hud.scale_percent.round())),
                label_w: LABEL_W,
            },
            RowSpec {
                label: "H", // i18n-exempt: source HUD token
                value: format!("{}", hud.b.round()),
                suffix: "px",
                snap: hud.snap,
                trailing: None,
                label_w: LABEL_W,
            },
        ],
        HudMode::Rotate => vec![RowSpec {
            label: "R", // i18n-exempt: source HUD token
            value: format!("{:.1}\u{b0}", hud.a),
            suffix: "",
            snap: hud.snap,
            trailing: None,
            label_w: LABEL_W,
        }],
        HudMode::Brush => vec![
            RowSpec {
                label: "SIZE", // i18n-exempt: source HUD token
                value: format!("{}", hud.a.round()),
                suffix: "px",
                snap: false,
                trailing: None,
                label_w: LABEL_W_BRUSH,
            },
            RowSpec {
                label: "HARD", // i18n-exempt: source HUD token
                value: format!("{}", hud.b.round()),
                suffix: "%",
                snap: false,
                trailing: None,
                label_w: LABEL_W_BRUSH,
            },
        ],
    }
}

fn row(spec: RowSpec) -> Div {
    let mut row = h_flex()
        .h(px(ROW_H))
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .child(
            h_flex()
                .items_center()
                .gap(px(6.0))
                .child(
                    div()
                        .w(px(spec.label_w))
                        .flex_shrink_0()
                        .whitespace_nowrap()
                        .text_size(px(10.0))
                        .font_weight(FontWeight(600.))
                        .text_color(HUD_DIM)
                        .child(spec.label),
                )
                .child(
                    div()
                        .whitespace_nowrap()
                        .font_weight(FontWeight(500.))
                        .text_color(HUD_TEXT)
                        .child(spec.value),
                )
                .when(!spec.suffix.is_empty(), |el| {
                    el.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(HUD_DIM)
                            .child(spec.suffix),
                    )
                }),
        );
    if let Some(trailing) = spec.trailing {
        row = row.child(
            div()
                .text_size(px(10.0))
                .text_color(HUD_DIM)
                .child(trailing),
        );
    }
    if spec.snap {
        row = row.child(
            div()
                .rounded(px(3.0))
                .border(px(1.0))
                .border_color(SNAP_BORDER)
                .bg(SNAP_FILL)
                .px(px(4.0))
                .text_size(px(9.5))
                .font_weight(FontWeight(500.))
                .text_color(SNAP_TEXT)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn hud(mode: HudMode, a: f64, b: f64, snap: bool) -> HudInfo {
        HudInfo {
            mode,
            x: 0.0,
            y: 0.0,
            a,
            b,
            scale_percent: 100.0,
            snap,
        }
    }

    /// The badge sits on the second row only (move/resize), the scale percent
    /// reads on the resize W row, and the brush labels get their wider column.
    #[test]
    fn hud_rows_follow_the_oracle_per_mode() {
        let move_rows = row_specs(hud(HudMode::Move, 12.4, -34.2, true));
        assert_eq!(move_rows.len(), 2);
        assert_eq!(
            (move_rows[0].label, move_rows[0].value.as_str()),
            ("X", "+12")
        );
        assert_eq!(
            (move_rows[1].label, move_rows[1].value.as_str()),
            ("Y", "-34")
        );
        assert!(!move_rows[0].snap, "the X row carries no badge");
        assert!(move_rows[1].snap, "the Y row carries the badge");

        let resize_rows = row_specs(hud(HudMode::Resize, 640.2, 480.4, true));
        assert_eq!(resize_rows[0].trailing.as_deref(), Some("100%"));
        assert!(
            !resize_rows[0].snap,
            "the scale percent sits on W, not SNAP"
        );
        assert!(resize_rows[1].snap, "the H row carries SNAP");

        let rotate_rows = row_specs(hud(HudMode::Rotate, 45.04, 0.0, true));
        assert_eq!(rotate_rows.len(), 1);
        assert_eq!(rotate_rows[0].label, "R");
        assert_eq!(rotate_rows[0].value, "45.0\u{b0}");
        assert!(rotate_rows[0].snap);

        let brush_rows = row_specs(hud(HudMode::Brush, 24.0, 100.0, false));
        assert_eq!(brush_rows.len(), 2);
        assert_eq!(brush_rows[0].label, "SIZE");
        assert_eq!(brush_rows[1].label, "HARD");
        assert!(brush_rows.iter().all(|row| row.label_w == LABEL_W_BRUSH));
        assert!(brush_rows.iter().all(|row| !row.snap));
    }
}
