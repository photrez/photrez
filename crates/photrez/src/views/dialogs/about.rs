use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::{div, hsla, px, relative, Div, FontWeight, Hsla, Stateful};

use crate::components::ui;
use crate::i18n::t;
use crate::icons;
use crate::tokens::color;

use super::title_bar;
use super::{GAP_2, R4, R6};
/// The About dialog's own measurements: a 32px mark inside the 48px tile
/// with 8px padding, and 16px between the
/// three blocks with 4px body padding.
const ABOUT_MARK: f32 = 32.0;

/// 48px tile, 8px radius, 1px DIVIDER border, `#151516` fill, 8px padding.
const ABOUT_TILE: f32 = 48.0;
const ABOUT_TILE_RADIUS: f32 = 8.0;
const ABOUT_TILE_PAD: f32 = 8.0;

/// The body column's rhythm between hero, spec panel and link row, plus the
/// source's 4px vertical padding.
const ABOUT_GAP: f32 = 16.0;
const ABOUT_BODY_PY: f32 = 4.0;

/// 14px \u{2014} mark to identity block.
const ABOUT_HERO_GAP: f32 = 14.0;

/// 10px padding and 6px row gaps on the spec panel, whose 6px radius is the
/// shared R6.
const ABOUT_PANEL_PAD: f32 = 10.0;
const ABOUT_PANEL_ROW_GAP: f32 = 6.0;

/// 28px link buttons with a 14px icon at a 6px gap.
const ABOUT_LINK_H: f32 = 28.0;
const ABOUT_ICON: f32 = 14.0;

/// 6px / 2px padding and a 4px radius on the version pill.
const ABOUT_PILL_PX: f32 = 6.0;
const ABOUT_PILL_PY: f32 = 2.0;

/// `#151516` \u{2014} the spec panel's fill, a
/// component literal two steps darker than PANEL, so it is pinned here rather
/// than joining `tokens.rs`.
const ABOUT_PANEL_BG: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.0824,
    a: 1.0,
};

/// The version pill's fill: accent at 10%,
/// alongside the crop bar's `ACCENT_15` in `tokens.rs`.
const ABOUT_PILL_FILL: Hsla = Hsla {
    h: color::ACCENT.h,
    s: color::ACCENT.s,
    l: color::ACCENT.l,
    a: 0.10,
};

/// The version pill's edge: accent at 40%,
/// and the link buttons' hover border.
const ABOUT_ACCENT_40: Hsla = Hsla {
    h: color::ACCENT.h,
    s: color::ACCENT.s,
    l: color::ACCENT.l,
    a: 0.40,
};

/// The four spec rows' labels, catalogue-routed like the rest of the dialog.
pub(super) const ABOUT_SPEC_LABELS: [&str; 4] = [
    "dialogs.about.coreEngine",
    "dialogs.about.rasterizer",
    "dialogs.about.runtimeShell",
    "dialogs.about.license",
];

/// The spec values stay literals: the embedded catalogues are byte-locked to
/// the shipped translations (see the i18n module), so a native-only string
/// cannot join them. The rasterizer and shell rows name what actually runs
/// here, not what an earlier shell reported.
pub(super) const ABOUT_SPEC_VALUES: [&str; 4] = [
    "Rust (photrez-core)", // i18n-exempt: tech value, not in catalog
    "CPU tile renderer",   // i18n-exempt: tech value, not in catalog
    "Rust + GPUI",         // i18n-exempt: tech value, not in catalog
    "AGPLv3 Open-Source",  // i18n-exempt: tech value, not in catalog
];

// The two shipped catalogues' names for themselves, so the line reads the
// same under either locale. Comma-separated rather than a middle dot: the
// embedded face has no U+00B7, and the coverage gate reviews catalogue text,
// not native literals.

/// The link row's labels \u{2014} literals in the dialog,
/// not catalogue keys.
pub(super) const ABOUT_LINK_GITHUB: &str = "GitHub Repository"; // i18n-exempt: not in catalog
pub(super) const ABOUT_LINK_WEBSITE: &str = "Official Website"; // i18n-exempt: not in catalog

/// Where the two link buttons lead.
const ABOUT_LINK_GITHUB_URL: &str = "https://github.com/photrez/photrez"; // i18n-exempt: URL, not text
const ABOUT_LINK_WEBSITE_URL: &str = "https://photrez.github.io"; // i18n-exempt: URL, not text

/// `v{version}` \u{2014} the crate's own version is the only
/// source, so there is no fallback string to go stale.
pub(super) fn about_version() -> String {
    format!("v{}", env!("CARGO_PKG_VERSION"))
}

/// The About dialog, built for the native shell rather than transcribed from
/// `dialog-about.png`: the hero, spec grid and link row survive
/// as the structure and geometry, and three things change with the shell. The
/// mark is the real brand raster rather than a placeholder tile; the
/// version comes from `CARGO_PKG_VERSION` instead of a runtime round-trip;
/// and the locale line plus two stack rows state what ships here rather than
/// what the replaced shell ran. The chrome above and below is shared and
/// untouched, and the capture difference is a recorded decision, not drift.
pub(super) fn about_body() -> Div {
    v_flex()
        .w_full()
        .h_full()
        .justify_center()
        .gap(px(ABOUT_GAP))
        .py(px(ABOUT_BODY_PY))
        .child(about_hero())
        .child(about_specs())
        .child(about_links())
}

/// Centred, never-squeezed 48px tile with an 8px radius, 1px DIVIDER border,
/// `#151516` fill, the standard inset well and 8px padding, around the 32px
/// mark.
fn about_brand_tile() -> Div {
    div()
        .w(px(ABOUT_TILE))
        .h(px(ABOUT_TILE))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(ABOUT_TILE_RADIUS))
        .border_1()
        .border_color(color::DIVIDER)
        .bg(ABOUT_PANEL_BG)
        .shadow(vec![crate::components::ui::inset_well()])
        .p(px(ABOUT_TILE_PAD))
        .child(title_bar::logo(ABOUT_MARK))
}

/// Centred on one row with a 14px gap \u{2014} brand tile beside the identity
/// block.
fn about_hero() -> Div {
    h_flex()
        .w_full()
        .items_center()
        .gap(px(ABOUT_HERO_GAP))
        .child(about_brand_tile())
        .child(
            v_flex()
                .flex_1()
                .min_w(px(0.0))
                .child(
                    h_flex()
                        .items_center()
                        .gap(px(GAP_2))
                        .child(
                            div()
                                .text_size(px(15.0))
                                .font_family(ui::FONT_UI)
                                .font_weight(FontWeight(700.))
                                .line_height(relative(1.35))
                                .text_color(color::WHITE)
                                .child("Photrez".to_string()), // i18n-exempt: product name
                        )
                        .child(about_version_pill()),
                )
                .child(ui::clip(
                    div()
                        .mt(px(2.0))
                        .text_size(px(11.0))
                        .font_family(ui::FONT_UI)
                        .line_height(relative(1.35))
                        .text_color(color::TEXT_DIM)
                        .child(t("dialogs.about.tagline")),
                )),
        )
}

/// The version pill: 4px corners, a 40% accent edge, a 10% accent fill,
/// 6px horizontal and 2px vertical padding, 10px semibold accent text.
fn about_version_pill() -> Div {
    div()
        .rounded(px(R4))
        .border_1()
        .border_color(ABOUT_ACCENT_40)
        .bg(ABOUT_PILL_FILL)
        .px(px(ABOUT_PILL_PX))
        .py(px(ABOUT_PILL_PY))
        .child(
            div()
                .text_size(px(10.0))
                .font_family(ui::FONT_UI)
                .font_weight(FontWeight(600.))
                .line_height(relative(1.3))
                .text_color(color::ACCENT)
                .child(about_version()),
        )
}

/// The spec panel: 6px corners, a field-border edge, the `#151516` fill and
/// 10px padding, with a two-column grid inside at a 6px row gap.
fn about_specs() -> Div {
    v_flex()
        .w_full()
        .rounded(px(R6))
        .border_1()
        .border_color(color::FIELD_BORDER)
        .bg(ABOUT_PANEL_BG)
        .p(px(ABOUT_PANEL_PAD))
        .gap(px(ABOUT_PANEL_ROW_GAP))
        .child(about_spec_row(0, 1))
        .child(about_spec_row(2, 3))
}

/// One two-column row: two equal cells with no column gap.
fn about_spec_row(left: usize, right: usize) -> Div {
    h_flex()
        .w_full()
        .child(about_spec_cell(left))
        .child(about_spec_cell(right))
}

/// `Label: value` on one line: the label in the caption colour, the value
/// in medium weight and the full text colour. The label is pinned and the cell
/// clips, so the first cell's longer value cannot paint over the second.
fn about_spec_cell(i: usize) -> Div {
    ui::clip(h_flex().flex_1().gap(px(4.0)))
        .child(ui::pin(
            div()
                .text_size(px(11.0))
                .font_family(ui::FONT_UI)
                .line_height(px(18.0))
                .text_color(color::TEXT_DIM)
                .child(format!("{}:", t(ABOUT_SPEC_LABELS[i]))),
        ))
        .child(
            div()
                .text_size(px(11.0))
                .font_family(ui::FONT_UI)
                .font_weight(FontWeight(500.))
                .line_height(px(18.0))
                .text_color(color::TEXT)
                .child(ABOUT_SPEC_VALUES[i].to_string()),
        )
}

/// The link row: two 28px-tall buttons sharing the width equally with an
/// 8px gap.
fn about_links() -> Div {
    h_flex()
        .w_full()
        .gap(px(GAP_2))
        .child(about_link_button(
            icons::GITHUB,
            ABOUT_LINK_GITHUB,
            ABOUT_LINK_GITHUB_URL,
        ))
        .child(about_link_button(
            icons::GLOBE,
            ABOUT_LINK_WEBSITE,
            ABOUT_LINK_WEBSITE_URL,
        ))
}

/// A 28px-tall link button: equal share of the row, 4px corners, a field-border
/// edge, the field fill, 11px medium text, and a 14px icon at a 6px gap.
///
/// The dialog's one wired control: a pointer cursor, an 8%-white hover fill, a
/// 40% accent hover border, and a click that opens the URL in the user's
/// browser. `url` is both the hit-test id and the target,
/// so a button cannot drift from where it leads. Returns `Stateful<Div>` for
/// `.id(..)`, like the dock's tab button.
fn about_link_button(icon: &'static [u8], label: &str, url: &'static str) -> Stateful<Div> {
    h_flex()
        .id(url)
        .flex_1()
        .min_w(px(0.0))
        .h(px(ABOUT_LINK_H))
        .items_center()
        .justify_center()
        .gap(px(6.0))
        .rounded(px(R4))
        .border_1()
        .border_color(color::FIELD_BORDER)
        .bg(color::FIELD)
        .cursor_pointer()
        // No token holds 8% white (the scale carries 5 and 10); inline like the
        // dock tab's 2%. The alpha blends over FIELD exactly as the CSS does.
        .hover(|s| {
            s.bg(hsla(0.0, 0.0, 1.0, 0.08))
                .border_color(ABOUT_ACCENT_40)
        })
        .on_click(move |_, _, cx| cx.open_url(url))
        .child(
            Icon::default()
                .data(icon)
                .size(px(ABOUT_ICON))
                .text_color(color::ICON),
        )
        .child(ui::pin(
            div()
                .text_size(px(11.0))
                .font_family(ui::FONT_UI)
                .font_weight(FontWeight(500.))
                .line_height(relative(1.35))
                .text_color(color::TEXT)
                .child(label.to_string()),
        ))
}
